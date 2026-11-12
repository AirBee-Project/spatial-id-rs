use crate::{
    Error, FlexId, SpatialIdError, ZoomLevel,
    id::range_id::convert::{split_f, split_xy},
};
use alloc::vec::Vec;

impl FlexId {
    /// このFlexIdのF方向の占有を、ズーム `z` の絶対座標範囲 `[start_f, end_f]` に置き換える。
    pub(crate) fn extrude_f(
        &self,
        z: u8,
        start_f: i32,
        end_f: i32,
    ) -> Result<impl Iterator<Item = FlexId>, Error> {
        if z > ZoomLevel::MAX.get() {
            return Err(SpatialIdError::ZOutOfRange { z }.into());
        }

        let (left, right) = (start_f.min(end_f), start_f.max(end_f));
        if left < ZoomLevel::new(z).unwrap().f_min() {
            return Err(SpatialIdError::FOutOfRange { z, f: left }.into());
        }
        if right > ZoomLevel::new(z).unwrap().f_max() {
            return Err(SpatialIdError::FOutOfRange { z, f: right }.into());
        }

        let x_zoomlevel = self.x_zoomlevel();
        let x_index = self.x_index();
        let y_zoomlevel = self.y_zoomlevel();
        let y_index = self.y_index();
        let (t_zoomlevel, t_index) = (self.t_zoomlevel(), self.t_index());

        Ok(split_f(z, [left, right]).map(move |(seg_z, seg_index)| {
            // SAFETY: 空間成分は範囲内検証済み、時間成分は元の有効な FlexId を継承
            unsafe {
                FlexId::new_unchecked(seg_z, seg_index, x_zoomlevel, x_index, y_zoomlevel, y_index)
                    .time_with_segment_unchecked(t_zoomlevel, t_index)
            }
        }))
    }

    /// このFlexIdのX方向の占有を、ズーム `z` の絶対座標範囲（`start_x` から東向きに `end_x` まで）へ置き換える。
    pub(crate) fn extrude_x(
        &self,
        z: u8,
        start_x: u32,
        end_x: u32,
    ) -> Result<impl Iterator<Item = FlexId>, Error> {
        if z > ZoomLevel::MAX.get() {
            return Err(SpatialIdError::ZOutOfRange { z }.into());
        }

        let xy_max = ZoomLevel::new(z).unwrap().xy_max();
        if start_x > xy_max {
            return Err(SpatialIdError::XOutOfRange { z, x: start_x }.into());
        }
        if end_x > xy_max {
            return Err(SpatialIdError::XOutOfRange { z, x: end_x }.into());
        }

        // start_x から東向きに end_x まで。境界跨ぎ（start_x > end_x）は2区間へ分ける。
        let ranges: Vec<[u32; 2]> = if start_x <= end_x {
            vec![[start_x, end_x]]
        } else {
            vec![[start_x, xy_max], [0, end_x]]
        };
        let f_zoomlevel = self.f_zoomlevel();
        let f_index = self.f_index();
        let y_zoomlevel = self.y_zoomlevel();
        let y_index = self.y_index();
        let (t_zoomlevel, t_index) = (self.t_zoomlevel(), self.t_index());

        Ok(ranges
            .into_iter()
            .flat_map(move |range| split_xy(z, range))
            .map(move |(seg_z, seg_index)| {
                // SAFETY: 空間成分は範囲内検証済み、時間成分は元の有効な FlexId を継承
                unsafe {
                    FlexId::new_unchecked(
                        f_zoomlevel,
                        f_index,
                        seg_z,
                        seg_index,
                        y_zoomlevel,
                        y_index,
                    )
                    .time_with_segment_unchecked(t_zoomlevel, t_index)
                }
            }))
    }

    /// このFlexIdのY方向の占有を、ズーム `z` の絶対座標範囲 `[start_y, end_y]` に置き換える。
    pub(crate) fn extrude_y(
        &self,
        z: u8,
        start_y: u32,
        end_y: u32,
    ) -> Result<impl Iterator<Item = FlexId>, Error> {
        if z > ZoomLevel::MAX.get() {
            return Err(SpatialIdError::ZOutOfRange { z }.into());
        }

        let (left, right) = (start_y.min(end_y), start_y.max(end_y));
        if right > ZoomLevel::new(z).unwrap().xy_max() {
            return Err(SpatialIdError::YOutOfRange { z, y: right }.into());
        }

        let f_zoomlevel = self.f_zoomlevel();
        let f_index = self.f_index();
        let x_zoomlevel = self.x_zoomlevel();
        let x_index = self.x_index();
        let (t_zoomlevel, t_index) = (self.t_zoomlevel(), self.t_index());

        Ok(split_xy(z, [left, right]).map(move |(seg_z, seg_index)| {
            // SAFETY: 空間成分は範囲内検証済み、時間成分は元の有効な FlexId を継承
            unsafe {
                FlexId::new_unchecked(f_zoomlevel, f_index, x_zoomlevel, x_index, seg_z, seg_index)
                    .time_with_segment_unchecked(t_zoomlevel, t_index)
            }
        }))
    }
}
