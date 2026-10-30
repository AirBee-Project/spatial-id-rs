pub(crate) mod primitive;
pub(crate) mod query;

#[cfg(test)]
mod test;

use crate::collection::core::{FlexTreeCore, NoSummary};
use crate::id::dimension::Dimension;
use crate::id::flex_id::ZOrderRel;
use crate::id::helpers::Side;
use crate::query::{
    SafeValue, merge_policy::MergePolicy, query_iter::QueryIter, traits::UnaryOperator,
};
use crate::{Error, FlexId, QueryError, RangeId, ZoomLevel};
use alloc::vec;
use alloc::vec::Vec;
use core::convert::TryFrom;
use core::marker::PhantomData;
use core::mem;
use core::ops::{Add, Div, Mul, Rem, Sub};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FalloffAxis {
    F,
    X,
    Y,
}

impl FalloffAxis {
    pub(crate) const fn dimension(self) -> Dimension {
        match self {
            Self::F => Dimension::F,
            Self::X => Dimension::X,
            Self::Y => Dimension::Y,
        }
    }

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

struct BoundsInfo {
    out_common_ancestor: [Option<FlexId>; 2],
    x_wraps_pos: [bool; 2],
    f_lower_crosses_to_upper: bool,
}

impl<P> Falloff<P> {
    fn shift_range_deltas(&self) -> (i32, i32) {
        let rad = self.radius as i32;
        let min_d = if self.direction == Some(Side::Upper) {
            0
        } else {
            -rad
        };
        let max_d = if self.direction == Some(Side::Lower) {
            0
        } else {
            rad
        };
        (min_d, max_d)
    }

    /// 入力のルート別共通祖先 `in_common_ancestor` から、シフト後のルート別共通祖先と境界跨ぎフラグを求める。
    fn compute_bounds_info(&self, in_common_ancestor: [Option<FlexId>; 2]) -> BoundsInfo {
        let z = self.z.get();
        let (min_d, max_d) = self.shift_range_deltas();
        let mut out_common_ancestor = [None, None];
        let mut x_wraps_pos = [false, false];
        let mut f_lower_crosses_to_upper = false;

        for (r, env_opt) in in_common_ancestor.into_iter().enumerate() {
            let Some(env) = env_opt else {
                continue;
            };
            match self.axis {
                FalloffAxis::Y => {
                    let yz = env.y_zoomlevel();
                    let max_z = yz.max(z);
                    let scale = 1i64 << (max_z - yz);
                    let d_scale = 1i64 << (max_z - z);
                    let left = (env.y_index() as i64) * scale + (min_d as i64) * d_scale;
                    let right = (env.y_index() as i64 + 1) * scale - 1 + (max_d as i64) * d_scale;
                    let limit = (1i64 << max_z) - 1;
                    if right >= 0 && left <= limit {
                        let (cz, cidx) = common_segment(max_z, left.max(0), right.min(limit));
                        out_common_ancestor[r] = Some(with_axis(env, FalloffAxis::Y, cz, cidx));
                    }
                }
                FalloffAxis::X => {
                    let xz = env.x_zoomlevel();
                    let max_z = xz.max(z);
                    let scale = 1i64 << (max_z - xz);
                    let d_scale = 1i64 << (max_z - z);
                    let left = (env.x_index() as i64) * scale + (min_d as i64) * d_scale;
                    let right = (env.x_index() as i64 + 1) * scale - 1 + (max_d as i64) * d_scale;
                    let limit = (1i64 << max_z) - 1;
                    if left >= 0 && right <= limit {
                        let (cz, cidx) = common_segment(max_z, left, right);
                        out_common_ancestor[r] = Some(with_axis(env, FalloffAxis::X, cz, cidx));
                    } else {
                        x_wraps_pos[r] = right > limit;
                        out_common_ancestor[r] = Some(with_axis(env, FalloffAxis::X, 0, 0));
                    }
                }
                FalloffAxis::F => {
                    let fz = env.f_zoomlevel();
                    let max_z = fz.max(z);
                    let scale = 1i64 << (max_z - fz);
                    let d_scale = 1i64 << (max_z - z);
                    let left = (env.f_index() as i64) * scale + (min_d as i64) * d_scale;
                    let right = (env.f_index() as i64 + 1) * scale - 1 + (max_d as i64) * d_scale;
                    let min_f = -(1i64 << max_z);
                    let max_f = (1i64 << max_z) - 1;
                    let c_left = left.max(min_f);
                    let c_right = right.min(max_f);
                    if c_left > c_right {
                        continue;
                    }
                    if c_right >= 0 {
                        if r == 1 {
                            f_lower_crosses_to_upper = true;
                        }
                        let (cz, cidx) = common_segment(max_z, c_left.max(0), c_right);
                        let cand = with_axis(env, FalloffAxis::F, cz, cidx);
                        out_common_ancestor[0] = Some(match out_common_ancestor[0] {
                            Some(prev) => lca_same_root(prev, cand),
                            None => cand,
                        });
                    }
                    if c_left < 0 {
                        let (cz, cidx) = common_segment(max_z, c_left, c_right.min(-1));
                        let cand = with_axis(env, FalloffAxis::F, cz, cidx);
                        out_common_ancestor[1] = Some(match out_common_ancestor[1] {
                            Some(prev) => lca_same_root(prev, cand),
                            None => cand,
                        });
                    }
                }
            }
        }

        BoundsInfo {
            out_common_ancestor,
            x_wraps_pos,
            f_lower_crosses_to_upper,
        }
    }

    /// 次の未処理入力 `next_id` 以降のすべての入力から生成されうるシフト領域の Z-order 下限を返す。
    fn horizon_for_next(
        &self,
        next_id: FlexId,
        in_common_ancestor: [Option<FlexId>; 2],
        x_wraps_pos: [bool; 2],
        f_lower_crosses_to_upper: bool,
    ) -> Option<FlexId> {
        let r = next_id.root_index();
        if x_wraps_pos[r] || (r == 0 && f_lower_crosses_to_upper) {
            return None;
        }
        let z = self.z.get();
        let (min_d, _) = self.shift_range_deltas();
        if min_d >= 0 {
            return Some(next_id);
        }

        let axis_dim = self.axis.dimension();
        let env = in_common_ancestor[r].unwrap_or(FlexId::ROOTS[r]);

        let make_cand = |raw_coords: [i64; 4]| -> FlexId {
            let bz = next_id.zoomlevel_on(axis_dim);
            let z_eff = bz.max(z);
            let c_min = raw_coords[axis_dim as usize] << (z_eff - bz);
            let delta = (min_d as i64) * (1i64 << (z_eff - z));
            let shifted = c_min + delta;
            let limit = (1i64 << z_eff) - 1;
            let valid_min = match self.axis {
                FalloffAxis::Y => shifted.clamp(0, limit),
                FalloffAxis::X => {
                    if shifted < 0 {
                        0
                    } else {
                        shifted.min(limit)
                    }
                }
                FalloffAxis::F => {
                    if r == 0 {
                        shifted.clamp(0, limit)
                    } else {
                        shifted.clamp(-(1i64 << z_eff), -1)
                    }
                }
            };
            let mut coords = raw_coords;
            coords[axis_dim as usize] = valid_min;

            let fz = if axis_dim == Dimension::F {
                z_eff
            } else {
                next_id.f_zoomlevel()
            };
            let xz = if axis_dim == Dimension::X {
                z_eff
            } else {
                next_id.x_zoomlevel()
            };
            let yz = if axis_dim == Dimension::Y {
                z_eff
            } else {
                next_id.y_zoomlevel()
            };
            #[cfg(feature = "temporal_id")]
            let tz = if axis_dim == Dimension::T {
                z_eff
            } else {
                next_id.t_zoomlevel()
            };
            #[cfg(not(feature = "temporal_id"))]
            let tz = next_id.t_zoomlevel();

            unsafe {
                FlexId::new_unchecked(
                    fz,
                    coords[Dimension::F as usize] as i32,
                    xz,
                    coords[Dimension::X as usize] as u32,
                    yz,
                    coords[Dimension::Y as usize] as u32,
                )
                .with_time_segment_unchecked(tz, coords[Dimension::T as usize] as u64)
            }
        };

        let mut next_coords = [0i64; 4];
        for d in Dimension::ALL {
            next_coords[d as usize] = next_id.index_on(d);
        }
        let mut horizon = make_cand(next_coords);

        for d in Dimension::ALL {
            let z_env = env.zoomlevel_on(d);
            let z_next = next_id.zoomlevel_on(d);
            if z_env >= z_next {
                continue;
            }
            for l in (z_env + 1)..=z_next {
                if (next_id.index_on(d) >> (z_next - l)) & 1 != 0 {
                    continue;
                }
                let mut branch_coords = [0i64; 4];
                for dim in Dimension::ALL {
                    let z_dim = next_id.zoomlevel_on(dim);
                    let idx = next_id.index_on(dim);
                    if dim == d {
                        let prefix = (idx >> (z_dim - l)) | 1;
                        branch_coords[dim as usize] = prefix << (z_dim - l);
                    } else {
                        let k = env
                            .zoomlevel_on(dim)
                            .max(if dim < d { l } else { l - 1 })
                            .min(z_dim);
                        let prefix = idx >> (z_dim - k);
                        let coord = prefix << (z_dim - k);
                        branch_coords[dim as usize] = coord;
                    }
                }
                let cand = make_cand(branch_coords);
                horizon = zorder_infimum(horizon, cand);
            }
        }
        Some(horizon)
    }

    /// 入力ストリームからチャンク単位で近傍展開を行い、確定した領域から順次返すイテレータを構築する。
    fn stream_spread<'a, V, A, FWrap, FResolve, FDrain>(
        &'a self,
        input: QueryIter<'a, V>,
        attenuator: Attenuator<V>,
        wrap: FWrap,
        resolve: FResolve,
        mut on_drained: FDrain,
    ) -> QueryIter<'a, V>
    where
        V: SafeValue
            + Add<Output = V>
            + Sub<Output = V>
            + Mul<Output = V>
            + Div<Output = V>
            + Rem<Output = V>
            + TryFrom<u64>
            + 'static,
        A: SafeValue + 'static,
        FWrap: Fn(V) -> A + Send + Sync + 'a,
        FResolve: Fn(&A, &A) -> A + Send + Sync + 'a,
        FDrain: FnMut(&mut Vec<(FlexId, A)>, Option<(&FlexId, usize)>, &mut Vec<(FlexId, V)>)
            + Send
            + 'a,
    {
        let in_common_ancestor = input.common_ancestor();
        let bounds_info = self.compute_bounds_info(in_common_ancestor);
        let out_common_ancestor = bounds_info.out_common_ancestor;
        let x_wraps_pos = bounds_info.x_wraps_pos;
        let f_lower_crosses_to_upper = bounds_info.f_lower_crosses_to_upper;
        let axis = self.axis;
        let z = self.z;
        let radius = self.radius;
        let direction = self.direction;

        let mut input = input.fuse().peekable();
        let mut tree = FlexTreeCore::<A, NoSummary>::default();
        let mut drained_raw: Vec<(FlexId, A)> = Vec::new();
        // pop() で Z-order 昇順に返すため、補充時に逆順にして積む
        let mut ready: Vec<(FlexId, V)> = Vec::new();
        let mut moved: Vec<FlexId> = Vec::new();
        let mut finished = false;
        let mut limit = 16usize;

        let iter = core::iter::from_fn(move || {
            loop {
                if let Some(item) = ready.pop() {
                    return Some(Ok(item));
                }
                if finished {
                    return None;
                }

                #[cfg(feature = "rayon")]
                const MAX_CHUNK: usize = 16384;
                #[cfg(not(feature = "rayon"))]
                const MAX_CHUNK: usize = 1024;

                let mut chunk = Vec::with_capacity(limit);
                let mut pending_err: Option<Error> = None;
                while chunk.len() < limit {
                    match input.next() {
                        Some(Ok(item)) => chunk.push(item),
                        Some(Err(e)) => {
                            pending_err = Some(e);
                            break;
                        }
                        None => break,
                    }
                }

                if !chunk.is_empty() {
                    let spread_into =
                        |acc: &mut FlexTreeCore<A, NoSummary>,
                         moved_buf: &mut Vec<FlexId>,
                         (id, value): &(FlexId, V)| {
                            id.falloff_each(
                                axis,
                                z,
                                radius,
                                direction,
                                &attenuator,
                                value,
                                moved_buf,
                                |ids, v| acc.insert_with(ids.iter().copied(), wrap(v), &resolve),
                            );
                        };

                    #[cfg(feature = "rayon")]
                    {
                        use rayon::prelude::*;
                        if chunk.len() <= 64 {
                            for item in &chunk {
                                spread_into(&mut tree, &mut moved, item);
                            }
                        } else {
                            let merge =
                                |mut a: FlexTreeCore<A, NoSummary>, b: FlexTreeCore<A, NoSummary>| {
                                    a.merge_with(&b, &resolve);
                                    a
                                };
                            let batch = chunk
                                .par_iter()
                                .fold(
                                    || (FlexTreeCore::default(), Vec::new()),
                                    |(mut acc, mut m), item| {
                                        spread_into(&mut acc, &mut m, item);
                                        (acc, m)
                                     },
                                )
                                .map(|(acc, _)| acc)
                                .reduce(FlexTreeCore::default, merge);
                            tree.merge_with(&batch, &resolve);
                        }
                    }

                    #[cfg(not(feature = "rayon"))]
                    {
                        for item in &chunk {
                            spread_into(&mut tree, &mut moved, item);
                        }
                    }

                    limit = (limit * 4).min(MAX_CHUNK);
                }

                if let Some(err) = pending_err {
                    finished = true;
                    return Some(Err(err));
                }

                match input.peek() {
                    Some(Ok((next_id, _))) => {
                        let next_id = *next_id;
                        let r = next_id.root_index();
                        // ルート 1 へ移り、かつルート 1 からルート 0 への跨ぎがないならルート 0 はすべて確定
                        if r == 1 && !f_lower_crosses_to_upper {
                            tree.drain_confirmed(0, &FlexId::LOWER_MAX, &mut drained_raw);
                            on_drained(&mut drained_raw, Some((&FlexId::LOWER_MAX, 0)), &mut ready);
                        }
                        if (r == 0 || tree.common_ancestor_roots()[0].is_none())
                            && let Some(horizon) = self.horizon_for_next(
                                next_id,
                                in_common_ancestor,
                                x_wraps_pos,
                                f_lower_crosses_to_upper,
                            )
                        {
                            tree.drain_confirmed(r, &horizon, &mut drained_raw);
                            on_drained(&mut drained_raw, Some((&horizon, r)), &mut ready);
                        }
                    }
                    Some(Err(_)) => {
                        let Some(Err(e)) = input.next() else {
                            unreachable!()
                        };
                        finished = true;
                        return Some(Err(e));
                    }
                    None => {
                        finished = true;
                        drained_raw.extend(mem::take(&mut tree));
                        on_drained(&mut drained_raw, None, &mut ready);
                    }
                }

                ready.reverse();
            }
        });

        QueryIter::from_ordered(iter).with_common_ancestor(out_common_ancestor)
    }
}

/// 同じ符号の区間 `[a, b]`（ズーム `max_z`）を包む最小共通 Segment `(zoom, index)` を返す。
fn common_segment(max_z: u8, a: i64, b: i64) -> (u8, i64) {
    let diff = (a ^ b) as u64;
    let drop = (u64::BITS - diff.leading_zeros()) as u8;
    let cz = max_z.saturating_sub(drop);
    (cz, a >> (max_z - cz))
}

/// 同じルートに属する 2 つの [`FlexId`] を包む最小共通祖先を返す。
fn lca_same_root(a: FlexId, b: FlexId) -> FlexId {
    let zoom = Dimension::ALL.map(|d| {
        let (za, zb) = (a.zoomlevel_on(d), b.zoomlevel_on(d));
        let deepest = za.min(zb);
        let diff = ((a.index_on(d) >> (za - deepest)) ^ (b.index_on(d) >> (zb - deepest))) as u64;
        deepest - (u64::BITS - diff.leading_zeros()) as u8
    });
    a.ancestor_at(zoom)
}

/// `id` の `axis` 軸だけを `(cz, cidx)` に置き換えた [`FlexId`] を返す。
fn with_axis(id: FlexId, axis: FalloffAxis, cz: u8, cidx: i64) -> FlexId {
    let (fz, fi) = if axis == FalloffAxis::F {
        (cz, cidx as i32)
    } else {
        (id.f_zoomlevel(), id.f_index())
    };
    let (xz, xi) = if axis == FalloffAxis::X {
        (cz, cidx as u32)
    } else {
        (id.x_zoomlevel(), id.x_index())
    };
    let (yz, yi) = if axis == FalloffAxis::Y {
        (cz, cidx as u32)
    } else {
        (id.y_zoomlevel(), id.y_index())
    };
    // SAFETY: 呼び出し側で各軸のズームとインデックスが有効範囲内であることを保証している
    unsafe {
        FlexId::new_unchecked(fz, fi, xz, xi, yz, yi)
            .with_time_segment_unchecked(id.t_zoomlevel(), id.t_index())
    }
}

/// `R < a` かつ `R < b` を満たすための `a` と `b` の Z-order 下限を返す。
fn zorder_infimum(a: FlexId, b: FlexId) -> FlexId {
    match a.zorder_rel(&b) {
        ZOrderRel::Less | ZOrderRel::Equal => a,
        ZOrderRel::Greater => b,
        ZOrderRel::Overlap { enclosing, .. } => enclosing,
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
            return Ok(self.stream_spread(
                input,
                attenuator,
                |value| value,
                resolve,
                |drained, _, ready| {
                    ready.append(drained);
                },
            ));
        }

        // 順序を問う規則や平均のように resolve_many を持つ規則は、2件ずつ畳み込むと値が変わるので、
        // 確定した領域ごとに重なった値を resolve_many で1回だけ合成し、出力用の木で同値結合してから流す
        let mut resolved_tree = FlexTreeCore::<V, NoSummary>::default();
        Ok(self.stream_spread(
            input,
            attenuator,
            |value| vec![value],
            |a, b| [a.as_slice(), b].concat(),
            move |drained, horizon_info, ready| {
                for (id, values) in drained.drain(..) {
                    if let Some(v) = P::resolve_many(values.into_iter()) {
                        resolved_tree.insert([id], v);
                    }
                }
                match horizon_info {
                    Some((horizon, r)) => {
                        if r == 1 && resolved_tree.common_ancestor_roots()[0].is_some() {
                            resolved_tree.drain_confirmed(0, &FlexId::LOWER_MAX, ready);
                        }
                        resolved_tree.drain_confirmed(r, horizon, ready);
                    }
                    None => {
                        ready.extend(mem::take(&mut resolved_tree));
                    }
                }
            },
        ))
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
