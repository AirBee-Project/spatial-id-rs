pub(crate) mod primitive;
pub(crate) mod query;

#[cfg(test)]
mod test;

use crate::id::helpers::Side;
use crate::query::{
    SafeValue, merge_policy::MergePolicy, query_iter::QueryIter, traits::UnaryOperator,
};
use crate::{Error, FlexId, QueryError, RangeId, SpatialIdMap, ZoomLevel};
use alloc::vec;
use alloc::vec::Vec;
use core::convert::TryFrom;
use core::marker::PhantomData;
use core::ops::{Add, Div, Mul, Rem, Sub};

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
/// 半径は、線形の減衰では半径の2乗、2次の減衰では半径の4乗が `V` と u64 に収まる範囲で指定する。
pub struct Falloff<P> {
    pub axis: FalloffAxis,
    pub z: ZoomLevel,
    pub radius: u32,
    pub direction: Option<Side>,
    pub pattern: FalloffPattern,
    _marker: PhantomData<fn() -> P>,
}

impl<P> Falloff<P> {
    pub fn new<T: Into<u8>>(
        axis: FalloffAxis,
        z: T,
        radius: u32,
        direction: Option<Side>,
        pattern: FalloffPattern,
    ) -> Result<Self, Error> {
        let z = ZoomLevel::new(z.into())?;
        Ok(Self {
            axis,
            z,
            radius,
            direction,
            pattern,
            _marker: PhantomData,
        })
    }
}

impl<P> Falloff<P> {
    /// 入力の各要素を近傍へ広げ、`wrap` した値の重なりを `resolve` で合成した木を返す。
    /// 塊ごとに作った木を入力の順に合成するので、`resolve` は結合的でなければならない。
    fn spread<V, A>(
        &self,
        input: QueryIter<'_, V>,
        attenuator: &Attenuator<V>,
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
                id.falloff_each(
                    self.axis,
                    self.z,
                    self.radius,
                    self.direction,
                    attenuator,
                    value,
                    moved,
                    |ids, value| tree.insert_with(ids.iter().copied(), wrap(value), &resolve),
                );
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
        if self.radius == 0 {
            return Ok(input);
        }
        let attenuator = Attenuator::new(self.radius, self.pattern)?;

        if P::IS_COMMUTATIVE {
            let resolve = |a: &V, b: &V| P::resolve(a.clone(), b.clone());
            return Ok(self
                .spread(input, &attenuator, |value| value, resolve)?
                .into());
        }

        // 順序を問う規則や平均のように resolve_many を持つ規則は、2件ずつ畳み込むと値が変わるので、
        // 重なった値を入力の順に集めてから resolve_many で1回だけ合成する
        let gathered = self.spread(
            input,
            &attenuator,
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
        let r = self.radius as i64;
        // 順方向が Upper (+方向) へ広がるなら、出力へ届きうる入力は Lower (-方向) にある
        let d = match self.direction {
            Some(Side::Upper) => [-r, 0],
            Some(Side::Lower) => [0, r],
            None => [-r, r],
        };
        let (f, x, y) = match self.axis {
            FalloffAxis::F => (d, [0, 0], [0, 0]),
            FalloffAxis::X => ([0, 0], d, [0, 0]),
            FalloffAxis::Y => ([0, 0], [0, 0], d),
        };
        bounds.shift_range(self.z.get(), f, x, y, out);
    }

    fn validate(&self) -> Result<(), Error> {
        self.axis.check_radius(self.z, self.radius)?;
        Attenuator::<V>::new(self.radius, self.pattern).map(drop)
    }
}
