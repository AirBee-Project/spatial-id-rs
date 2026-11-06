pub(crate) mod primitive;
pub(crate) mod query;

#[cfg(test)]
mod test;

use crate::query::{
    SafeValue, merge_policy::MergePolicy, query_iter::QueryIter, traits::UnaryOperator,
};
use crate::{Error, FlexId, QueryError, RangeId, SpatialIdMap, ZoomLevel};
use alloc::vec;
use alloc::vec::Vec;
use core::cmp::Ordering;
use core::convert::TryFrom;
use core::marker::PhantomData;
use core::ops::{Add, Div, Mul, RangeInclusive, Rem, Sub};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FalloffAxis {
    F,
    X,
    Y,
}

impl FalloffAxis {
    /// `radius` がズーム `z` でのこの軸の範囲に収まるか検証する。
    pub(crate) fn check_radius(self, z: ZoomLevel, radius: u32) -> Result<(), Error> {
        let max = match self {
            Self::F => z.f_max() as u32,
            Self::X | Self::Y => z.xy_max(),
        };
        if radius > max {
            return Err(
                QueryError::InvalidParameter("falloff の半径が軸の範囲を超えている").into(),
            );
        }
        Ok(())
    }
}

/// [`Query::falloff_f`](crate::query::Query::falloff_f) などで、距離 `d` に応じて値を減らす割合。半径を `r` とする。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FalloffPattern {
    /// `(r - d) / r`。距離に比例して減る。
    Linear,
    /// `1 - (d / r)²`。近くではゆっくり、遠くで急に減る。
    QuadraticIn,
    /// `((r - d) / r)²`。近くで急に、遠くではゆっくり減る。
    QuadraticOut,
}

pub(crate) enum Attenuator<V> {
    Linear { radius: V },
    QuadraticIn { r_sq: V },
    QuadraticOut { radius: V, r_sq: V },
}

impl<V> Attenuator<V>
where
    V: Add<Output = V>
        + Sub<Output = V>
        + Mul<Output = V>
        + Div<Output = V>
        + Rem<Output = V>
        + TryFrom<u64>
        + Clone,
{
    pub fn new(radius: u32, pattern: FalloffPattern) -> Result<Self, Error> {
        let too_large = || {
            Error::from(QueryError::InvalidParameter(
                "falloff の半径が値の型に収まらない",
            ))
        };
        let to_v = |n: u64| V::try_from(n).map_err(|_| too_large());
        // `V` の乗算で確かめるとあふれたときに panic するので、u64 で2乗してから変換して確かめる
        let sq = |n: u64| n.checked_mul(n).ok_or_else(too_large);
        let radius = u64::from(radius);
        // `attenuate` の途中の積は分母の2乗を超えないので、それが `V` に収まれば途中であふれない
        Ok(match pattern {
            FalloffPattern::Linear => {
                to_v(sq(radius)?)?;
                Self::Linear {
                    radius: to_v(radius)?,
                }
            }
            FalloffPattern::QuadraticIn => {
                to_v(sq(sq(radius)?)?)?;
                Self::QuadraticIn {
                    r_sq: to_v(sq(radius)?)?,
                }
            }
            FalloffPattern::QuadraticOut => {
                to_v(sq(sq(radius)?)?)?;
                Self::QuadraticOut {
                    radius: to_v(radius)?,
                    r_sq: to_v(sq(radius)?)?,
                }
            }
        })
    }

    /// `value * num / den` を、途中の積が `value` と `den` の2乗を超えないように求める。
    #[inline(always)]
    pub fn attenuate(&self, value: &V, distance: u32) -> V {
        let d = V::try_from(u64::from(distance)).unwrap_or_else(|_| unreachable!("距離は半径以下"));
        let (num, den) = match self {
            Self::Linear { radius } => (radius.clone() - d, radius),
            Self::QuadraticIn { r_sq } => (r_sq.clone() - d.clone() * d, r_sq),
            Self::QuadraticOut { radius, r_sq } => {
                let r_minus_d = radius.clone() - d;
                (r_minus_d.clone() * r_minus_d, r_sq)
            }
        };
        // value = q * den + rem と分けると、num <= den なので q * num は value を、rem * num は den の2乗を超えない。
        // 整数の切り捨ても value * num / den と一致する
        let rem = value.clone() % den.clone();
        (value.clone() - rem.clone()) / den.clone() * num.clone() + rem * num / den.clone()
    }
}

/// 値を軸方向へ広げながら減衰させる演算子。
///
/// 負の側は `-range[0]`、正の側は `range[1]` を半径として減衰させる。
/// 半径は、線形の減衰では半径の2乗、2次の減衰では半径の4乗が `V` と u64 に収まる範囲で指定する。
pub struct Falloff<P> {
    pub axis: FalloffAxis,
    pub z: ZoomLevel,
    /// 広げる距離の範囲 `[start, end]`。`start <= 0 <= end` を満たす。
    pub range: [i32; 2],
    pub pattern: FalloffPattern,
    _marker: PhantomData<fn() -> P>,
}

impl<P> Falloff<P> {
    pub fn new<T: Into<u8>>(
        axis: FalloffAxis,
        z: T,
        range: RangeInclusive<i32>,
        pattern: FalloffPattern,
    ) -> Result<Self, Error> {
        let z = ZoomLevel::new(z.into())?;
        let range = [*range.start(), *range.end()];
        if !(range[0] <= 0 && 0 <= range[1]) {
            return Err(QueryError::InvalidParameter("falloff の範囲が0を含んでいない").into());
        }
        Ok(Self {
            axis,
            z,
            range,
            pattern,
            _marker: PhantomData,
        })
    }

    /// 負の側と正の側の半径。
    fn radii(&self) -> [u32; 2] {
        self.range.map(i32::unsigned_abs)
    }

    fn attenuators<V>(&self) -> Result<[Attenuator<V>; 2], Error>
    where
        V: Add<Output = V>
            + Sub<Output = V>
            + Mul<Output = V>
            + Div<Output = V>
            + Rem<Output = V>
            + TryFrom<u64>
            + Clone,
    {
        let [lower, upper] = self.radii();
        Ok([
            Attenuator::new(lower, self.pattern)?,
            Attenuator::new(upper, self.pattern)?,
        ])
    }
}

impl<P> Falloff<P> {
    /// 入力の各要素を近傍へ広げ、`wrap` した値の重なりを `resolve` で合成した木を返す。
    /// 塊ごとに作った木を入力の順に合成するので、`resolve` は結合的でなければならない。
    fn spread<V, A>(
        &self,
        input: QueryIter<'_, V>,
        [lower, upper]: &[Attenuator<V>; 2],
        wrap: impl Fn(V) -> A + Send + Sync,
        resolve: impl Fn(&A, &A) -> A + Send + Sync,
    ) -> Result<SpatialIdMap<A>, Error>
    where
        V: SafeValue
            + Add<Output = V>
            + Sub<Output = V>
            + Mul<Output = V>
            + Div<Output = V>
            + Rem<Output = V>
            + TryFrom<u64>,
        A: SafeValue,
    {
        let spread_into =
            |mut tree: SpatialIdMap<A>, moved: &mut Vec<FlexId>, (id, value): &(FlexId, V)| {
                id.falloff_each(self.axis, self.z, self.range, moved, |ids, d| {
                    let attenuated = match d.cmp(&0) {
                        Ordering::Less => lower.attenuate(value, d.unsigned_abs()),
                        Ordering::Equal => value.clone(),
                        Ordering::Greater => upper.attenuate(value, d.unsigned_abs()),
                    };
                    tree.insert_with_flex_ids(ids.iter().copied(), wrap(attenuated), &resolve)
                });
                tree
            };

        #[cfg(not(feature = "rayon"))]
        {
            let mut tree = SpatialIdMap::default();
            let mut moved = Vec::new();
            for item in input {
                tree = spread_into(tree, &mut moved, &item?);
            }
            Ok(tree)
        }

        #[cfg(feature = "rayon")]
        {
            use rayon::prelude::*;
            // 入力を区切って読むので、一度に持つ入力はこの数までに収まる
            const CHUNK: usize = 1 << 16;
            let merge = |mut a: SpatialIdMap<A>, b: SpatialIdMap<A>| {
                a.merge_with(&b, &resolve);
                a
            };
            let mut input = input.peekable();
            // 塊ごとに蓄積した木へ合成すると、木全体の複製が塊の数だけ走るので、最後に二分木状にまとめる
            let mut parts = Vec::new();
            while input.peek().is_some() {
                let chunk: Vec<(FlexId, V)> =
                    input.by_ref().take(CHUNK).collect::<Result<_, _>>()?;
                parts.push(
                    chunk
                        .par_iter()
                        .fold(
                            || (SpatialIdMap::default(), Vec::new()),
                            |(tree, mut moved), item| (spread_into(tree, &mut moved, item), moved),
                        )
                        .map(|(tree, _)| tree)
                        .reduce(SpatialIdMap::default, merge),
                );
            }
            Ok(parts.into_par_iter().reduce(SpatialIdMap::default, merge))
        }
    }
}

impl<V: SafeValue + 'static, P> UnaryOperator<V> for Falloff<P>
where
    V: Add<Output = V>
        + Sub<Output = V>
        + Mul<Output = V>
        + Div<Output = V>
        + Rem<Output = V>
        + TryFrom<u64>
        + Clone,
    P: MergePolicy<V>,
{
    fn run<'a>(&'a self, input: QueryIter<'a, V>) -> Result<QueryIter<'a, V>, Error> {
        if self.range == [0, 0] {
            return Ok(input);
        }
        let attenuators = self.attenuators()?;

        if P::IS_COMMUTATIVE {
            let resolve = |a: &V, b: &V| P::resolve(a.clone(), b.clone());
            return Ok(self
                .spread(input, &attenuators, |value| value, resolve)?
                .into());
        }

        // 順序を問う規則や平均のように resolve_many を持つ規則は、2件ずつ畳み込むと値が変わるので、
        // 重なった値を入力の順に集めてから resolve_many で1回だけ合成する
        let gathered = self.spread(
            input,
            &attenuators,
            |value| vec![value],
            |a, b| [a.as_slice(), b].concat(),
        )?;
        // SpatialIdMap から出てくる列はカノニカル列なので、collect は木を下から一括で組める
        let resolved: SpatialIdMap<V> = gathered
            .iter()
            .filter_map(|(id, values)| Some((id, P::resolve_many(values.iter().cloned())?)))
            .collect();
        Ok(resolved.into())
    }

    fn reverse_target(&self, bounds: RangeId, out: &mut Vec<RangeId>) {
        // 入力は range の距離だけ移って出力へ届くので、出力へ届きうる入力は range を反転した範囲にある
        let d = [-i64::from(self.range[1]), -i64::from(self.range[0])];
        let (f, x, y) = match self.axis {
            FalloffAxis::F => (d, [0, 0], [0, 0]),
            FalloffAxis::X => ([0, 0], d, [0, 0]),
            FalloffAxis::Y => ([0, 0], [0, 0], d),
        };
        bounds.shift_range(self.z.get(), f, x, y, out);
    }

    fn validate(&self) -> Result<(), Error> {
        let [lower, upper] = self.radii();
        self.axis.check_radius(self.z, lower.max(upper))?;
        self.attenuators::<V>().map(drop)
    }
}
