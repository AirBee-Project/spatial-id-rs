use crate::{FlexId, SingleId, time::span::TimeSegments};

/// [`SingleId`] を FlexTree のノードアドレス（[`FlexId`]）へ展開するイテレータ。
///
/// 空間部分は常に単一なので、`TimeSegment` ごとに「同じ空間部の `FlexId`＋その `TimeSegment`」を
/// 返すだけで済む。**ヒープ確保をしない**（`Box<dyn Iterator>` も中間 [`Vec`](alloc::vec::Vec) も使わない）
/// のが要点で、挿入はこの経路を1件につき1回通るため固定費がそのまま効く。
///
/// 時間を使わない場合（および時間がちょうど1つの `TimeSegment` に収まる場合）は要素が1つだけになり、
/// 時間軸の無かった頃の [`core::iter::once`] と同じコストになる。
pub struct SingleIdFlexIds {
    /// 空間部分（3軸とも [`SingleId`] のズーム）。時間だけを差し替えて使い回す。
    base: FlexId,
    time_segments: TimeSegments,
}

impl Iterator for SingleIdFlexIds {
    type Item = FlexId;

    fn next(&mut self) -> Option<FlexId> {
        let time_segment = self.time_segments.next()?;
        Some(unsafe {
            self.base
                .time_with_segment_unchecked(time_segment.zoom().get(), time_segment.index())
        })
    }
}

/// [`SingleId`] を FlexTree のノードアドレス（[`FlexId`]）へ展開する。
///
/// 空間部分は常に単一だが、時間部分は仕様通り任意の秒数の間隔を取れるため、2の冪乗の秒数を持つ
/// `TimeSegment` へ分解すると複数個になりうる（例: 30分間隔なら最大10個）。そのため戻り値はイテレータで、
/// 全要素を合わせて元の [`SingleId`] の時空間領域をちょうど覆う。
impl SingleId {
    pub fn flex_ids(self) -> SingleIdFlexIds {
        SingleIdFlexIds {
            // `SingleId` は3軸とも同じズームで、各インデックスは構築時に検証済み。
            base: unsafe {
                FlexId::new_unchecked(self.z(), self.f(), self.z(), self.x(), self.z(), self.y())
            },
            time_segments: self.time_segments(),
        }
    }
}

impl SingleId {
    pub fn single_ids(self) -> impl Iterator<Item = SingleId> {
        core::iter::once(self)
    }
}
