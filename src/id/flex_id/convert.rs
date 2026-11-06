use crate::SpatialId;
use crate::time::span::TimeSpan;
use crate::{FlexId, RangeId, SingleId};

impl From<FlexId> for RangeId {
    fn from(flex_id: FlexId) -> Self {
        RangeId::from(&flex_id)
    }
}

impl From<&FlexId> for RangeId {
    fn from(flex_id: &FlexId) -> Self {
        let max_z = flex_id
            .f_zoomlevel
            .get()
            .max(flex_id.x_zoomlevel.get())
            .max(flex_id.y_zoomlevel.get());

        let scale_to_range = |val: i64, current_z: u8| -> [i64; 2] {
            let diff = max_z - current_z;
            let start = val << diff;
            let end = start + (1_i64 << diff) - 1;
            [start, end]
        };

        let f_range = scale_to_range(flex_id.f_index as i64, flex_id.f_zoomlevel.get());
        let x_range = scale_to_range(flex_id.x_index as i64, flex_id.x_zoomlevel.get());
        let y_range = scale_to_range(flex_id.y_index as i64, flex_id.y_zoomlevel.get());

        let (start, end) = flex_id.seconds_range();

        let spatial = RangeId::new(
            max_z,
            [f_range[0] as i32, f_range[1] as i32],
            [x_range[0] as u32, x_range[1] as u32],
            [y_range[0] as u32, y_range[1] as u32],
        )
        .unwrap();
        TimeSpan::new(start, end)
            .expect("Segmentの秒区間は空でない")
            .apply_to(spatial)
            .expect("Segmentの秒区間は常に有効")
    }
}

// `From<SingleId> for FlexId` は実装しない。[`SingleId`]は仕様通り任意の秒数の時間間隔を
// 持てる一方、[`FlexId`]はFlexTreeのノードアドレスとして2の冪秒のSegment1個しか持てないため、
// 両者は1対1に対応しないからである（例: 30分間隔は最大10個の[`FlexId`]へ分解される）。
// 変換には[`IntoIterator`]を使う（`SingleId::into_iter`が必要な数のSegmentへ分解する）。

impl IntoIterator for FlexId {
    type Item = FlexId;
    type IntoIter = core::iter::Once<FlexId>;
    fn into_iter(self) -> Self::IntoIter {
        core::iter::once(self)
    }
}

impl FlexId {
    pub fn single_ids(self) -> impl Iterator<Item = SingleId> {
        RangeId::from(self).single_ids()
    }
}
