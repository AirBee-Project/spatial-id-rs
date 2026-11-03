pub(crate) mod primitive;
pub(crate) mod query;

#[cfg(test)]
mod test;

use crate::SpatialIdMap;
use crate::query::SafeValue;
use crate::query::ops::unary::sort_by_flex_id;
use crate::{
    Error, FlexId, RangeId, ZoomLevel,
    query::{merge_policy::MergePolicy, query_iter::QueryIter, traits::UnaryOperator},
};
use alloc::vec::Vec;

#[derive(Debug, Clone, Copy)]
pub enum ExtrudeAxis {
    F(i32, i32),
    X(u32, u32),
    Y(u32, u32),
}

/// 各 FlexId の対象軸の座標を無視し、指定範囲に引き延ばす演算子。
pub struct Extrude<P> {
    pub target_z: ZoomLevel,
    pub axis: ExtrudeAxis,
    _marker: core::marker::PhantomData<fn() -> P>,
}

impl<P> Extrude<P> {
    pub fn new(target_z: ZoomLevel, axis: ExtrudeAxis) -> Self {
        Self {
            target_z,
            axis,
            _marker: core::marker::PhantomData,
        }
    }
}

impl<V: SafeValue + 'static, P> UnaryOperator<V> for Extrude<P>
where
    P: MergePolicy<V>,
{
    fn validate(&self) -> Result<(), Error> {
        match self.axis {
            ExtrudeAxis::F(start, end) => {
                self.target_z.check_f(start)?;
                self.target_z.check_f(end)?;
            }
            ExtrudeAxis::X(start, end) => {
                self.target_z.check_x(start)?;
                self.target_z.check_x(end)?;
            }
            ExtrudeAxis::Y(start, end) => {
                self.target_z.check_y(start)?;
                self.target_z.check_y(end)?;
            }
        }
        Ok(())
    }

    fn run<'a>(&'a self, input: QueryIter<'a, V>) -> Result<QueryIter<'a, V>, Error> {
        // 引き延ばす軸を (0, 0) に潰して先に集約することで、分割セグメント数倍のソートとマージを省く
        let mut projected: Vec<(FlexId, V)> = Vec::new();
        for item in input {
            let (id, v) = item?;
            let (fz, fi, xz, xi, yz, yi) = match self.axis {
                ExtrudeAxis::F(..) => (
                    0,
                    0,
                    id.x_zoomlevel(),
                    id.x_index(),
                    id.y_zoomlevel(),
                    id.y_index(),
                ),
                ExtrudeAxis::X(..) => (
                    id.f_zoomlevel(),
                    id.f_index(),
                    0,
                    0,
                    id.y_zoomlevel(),
                    id.y_index(),
                ),
                ExtrudeAxis::Y(..) => (
                    id.f_zoomlevel(),
                    id.f_index(),
                    id.x_zoomlevel(),
                    id.x_index(),
                    0,
                    0,
                ),
            };
            // SAFETY: 射影した軸は 0/0、その他の軸および時間成分は元の有効な FlexId を保持
            let proj = unsafe {
                FlexId::new_unchecked(fz, fi, xz, xi, yz, yi)
                    .with_time_segment_unchecked(id.t_zoomlevel(), id.t_index())
            };
            projected.push((proj, v));
        }

        sort_by_flex_id(&mut projected);

        let z = self.target_z.get();
        // 射影が一致しなくても、粗い列と細かい列は引き延ばした先で重なるので、重なりも MergePolicy で合成する
        let resolve = |a: &V, b: &V| P::resolve(a.clone(), b.clone());
        let mut table = SpatialIdMap::<V>::default();
        for chunk in projected.chunk_by(|a, b| a.0 == b.0) {
            let id = chunk[0].0;
            if let Some(merged) = P::resolve_many(chunk.iter().map(|(_, v)| v.clone())) {
                match self.axis {
                    ExtrudeAxis::F(s, e) => {
                        table.insert_with(id.extrude_f(z, s, e)?, merged, resolve)
                    }
                    ExtrudeAxis::X(s, e) => {
                        table.insert_with(id.extrude_x(z, s, e)?, merged, resolve)
                    }
                    ExtrudeAxis::Y(s, e) => {
                        table.insert_with(id.extrude_y(z, s, e)?, merged, resolve)
                    }
                }
            }
        }

        Ok(table.into())
    }

    fn reverse_target(&self, mut bounds: RangeId, out: &mut Vec<RangeId>) {
        let target_z = self.target_z.get();
        let bounds_z = bounds.z();
        let max_z = target_z.max(bounds_z);

        let scale_t = 1i64 << (max_z - target_z);
        let scale_b = 1i64 << (max_z - bounds_z);
        let bounds_zl = ZoomLevel::new(bounds_z).unwrap();

        match self.axis {
            ExtrudeAxis::F(start_f, end_f) => {
                let (s, e) = (start_f.min(end_f) as i64, start_f.max(end_f) as i64);
                let target_min = s * scale_t;
                let target_max = (e + 1) * scale_t - 1;
                let bounds_min = (bounds.f()[0] as i64) * scale_b;
                let bounds_max = (bounds.f()[1] as i64 + 1) * scale_b - 1;
                if target_max < bounds_min || bounds_max < target_min {
                    return;
                }
                bounds
                    .set_f([bounds_zl.f_min(), bounds_zl.f_max()])
                    .unwrap();
            }
            ExtrudeAxis::X(start_x, end_x) => {
                let len = 1i64 << max_z;
                let t_min = (start_x as i64) * scale_t;
                let t_max = (end_x as i64 + 1) * scale_t - 1;
                let t_span = (t_max - t_min).rem_euclid(len);

                let b_min = (bounds.x()[0] as i64) * scale_b;
                let b_max = (bounds.x()[1] as i64 + 1) * scale_b - 1;
                let b_span = (b_max - b_min).rem_euclid(len);

                // 周期境界上の2つの東向き弧が交差するか判定する
                let has_overlap = (b_min - t_min).rem_euclid(len) <= t_span
                    || (t_min - b_min).rem_euclid(len) <= b_span;
                if !has_overlap {
                    return;
                }
                bounds.set_x([0, bounds_zl.xy_max()]).unwrap();
            }
            ExtrudeAxis::Y(start_y, end_y) => {
                let (s, e) = (start_y.min(end_y) as i64, start_y.max(end_y) as i64);
                let target_min = s * scale_t;
                let target_max = (e + 1) * scale_t - 1;
                let bounds_min = (bounds.y()[0] as i64) * scale_b;
                let bounds_max = (bounds.y()[1] as i64 + 1) * scale_b - 1;
                if target_max < bounds_min || bounds_max < target_min {
                    return;
                }
                bounds.set_y([0, bounds_zl.xy_max()]).unwrap();
            }
        }
        out.push(bounds);
    }
}
