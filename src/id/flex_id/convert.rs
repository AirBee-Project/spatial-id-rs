use crate::time::Interval;
use crate::{FlexId, RangeId, SingleId};

impl From<FlexId> for RangeId {
    #[inline]
    fn from(flex_id: FlexId) -> Self {
        RangeId::from(&flex_id)
    }
}

impl From<&FlexId> for RangeId {
    #[inline]
    fn from(flex_id: &FlexId) -> Self {
        let f_z = flex_id.f_zoomlevel.get();
        let x_z = flex_id.x_zoomlevel.get();
        let y_z = flex_id.y_zoomlevel.get();
        let max_z = f_z.max(x_z).max(y_z);

        let df = max_z - f_z;
        let dx = max_z - x_z;
        let dy = max_z - y_z;

        let f0 = flex_id.f_index << df;
        let f1 = f0 | ((1_i32 << df) - 1);
        let x0 = flex_id.x_index << dx;
        let x1 = x0 | ((1_u32 << dx) - 1);
        let y0 = flex_id.y_index << dy;
        let y1 = y0 | ((1_u32 << dy) - 1);

        // FlexId の時間Segment `(tz, ti)` は常に幅 `unit = 2^(35 - tz)`、開始秒 `ti * unit` の
        // 単一Segmentであるため、`gcd(ti * unit, unit) = unit` となり、`RangeId` の時間表現は
        // 単位 `unit`・インデックス `[ti, ti]` に一意に定まる。
        let tz = flex_id.t_zoomlevel();
        let ti = flex_id.t_index();
        let interval = unsafe { Interval::new_unchecked(1u64 << (Interval::MAX_POW - tz)) };

        unsafe {
            RangeId::new_unchecked(max_z, [f0, f1], [x0, x1], [y0, y1])
                .time_with_unchecked(interval, [ti, ti])
        }
    }
}

// `From<SingleId> for FlexId` は実装しない。[`SingleId`]は仕様通り任意の秒数の時間間隔を
// 持てる一方、[`FlexId`]はFlexTreeのノードアドレスとして2の冪秒のSegment1個しか持てないため、
// 両者は1対1に対応しないからである（例: 30分間隔は最大10個の[`FlexId`]へ分解される）。
// 変換には`flex_ids`を使う（`SingleId::flex_ids`が必要な数のSegmentへ分解する）。

/// 木の内部 API が [`FlexId`] の列を受け取れるよう、自身だけを返す。
impl IntoIterator for FlexId {
    type Item = FlexId;
    type IntoIter = core::iter::Once<FlexId>;
    fn into_iter(self) -> Self::IntoIter {
        core::iter::once(self)
    }
}

impl FlexId {
    /// 自身だけを返す。[`SpatialId::flex_ids`](crate::SpatialId::flex_ids) の [`FlexId`] 版。
    pub fn flex_ids(self) -> core::iter::Once<FlexId> {
        core::iter::once(self)
    }

    pub fn single_ids(self) -> impl Iterator<Item = SingleId> {
        RangeId::from(self).single_ids()
    }
}
