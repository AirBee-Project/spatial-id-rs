use crate::{
    Error, FlexId, SpatialIdError, ZoomLevel,
    id::range_id::convert::{split_f, split_xy},
};
use alloc::vec::Vec;

impl FlexId {
    /// このFlexIdを高さ（F）方向へ、ズームレベル `z` のインデックス値 `index` 個分だけ平行移動した結果を返す。
    ///
    /// 移動量はズーム `z` を単位とするため、`z` がこのFlexIdのFズームレベルより
    /// 細かい場合は1インデックス値に満たない移動となり、結果が複数インデックス値へ分割されることがある。
    /// そのため複数の [`FlexId`] を生成するイテレーターを返す。XY方向の値は変更しない。
    ///
    /// # バリデーション
    /// - `z` が [`ZoomLevel::MAX`] を超える場合は [`SpatialIdError::ZOutOfRange`] を返す。
    /// - `index` がズーム `z` のF範囲（`ZoomLevel::new(z as u8).unwrap().f_min()..=ZoomLevel::new(z as u8).unwrap().f_max()`）外の場合は
    ///   [`SpatialIdError::FOutOfRange`] を返す。
    /// - 移動後の位置が、両者を合わせたズーム `max(f_zoomlevel, z)` のF範囲を超える場合は
    ///   [`SpatialIdError::FOutOfRange`] を返す。
    pub(crate) fn shift_f<Z: Into<u8>>(
        &self,
        z: Z,
        index: i32,
    ) -> Result<impl Iterator<Item = FlexId> + use<Z>, Error> {
        let z = z.into();
        // ズームレベルのチェック
        let zoom = ZoomLevel::new(z)?;
        zoom.check_f(index)?;

        let f_zoomlevel = self.f_zoomlevel();
        let max_z = f_zoomlevel.max(z);

        let segment_scale = 1_i32 << (max_z - f_zoomlevel);
        let delta_index = index * (1_i32 << (max_z - z));

        let left = self.f_index() * segment_scale + delta_index;
        let right = left + segment_scale - 1;

        // 移動後が max_z のF範囲を超える場合はエラー。
        if left < ZoomLevel::new(max_z)?.f_min() {
            return Err(SpatialIdError::FOutOfRange { z: max_z, f: left }.into());
        }
        if right > ZoomLevel::new(max_z)?.f_max() {
            return Err(SpatialIdError::FOutOfRange { z: max_z, f: right }.into());
        }

        // F以外の成分は値で捕捉し、返すイテレーターが self を借用しないようにする。
        let x_zoomlevel = self.x_zoomlevel();
        let x_index = self.x_index();
        let y_zoomlevel = self.y_zoomlevel();
        let y_index = self.y_index();
        let (t_zoomlevel, t_index) = (self.t_zoomlevel(), self.t_index());

        // 占有区間を整列したインデックス値群へ分解し、F以外の成分を保ったままFlexIdを構築する。
        Ok(
            split_f(max_z, [left, right]).map(move |(seg_z, seg_index)| {
                // SAFETY: 空間成分は範囲内検証済み、時間成分は元の有効な FlexId を継承
                unsafe {
                    FlexId::new_unchecked(
                        seg_z,
                        seg_index,
                        x_zoomlevel,
                        x_index,
                        y_zoomlevel,
                        y_index,
                    )
                    .time_with_segment_unchecked(t_zoomlevel, t_index)
                }
            }),
        )
    }

    /// このFlexIdを東西（X）方向へ、ズームレベル `z` のインデックス値 `index` 個分だけ平行移動した結果を返す。
    ///
    /// X方向はWebメルカトル図法において東西に巡回するため、移動量がどれだけ大きくても
    /// エラーにはならず、`max(x_zoomlevel, z)` の周長を法として循環する。境界（経度±180度）を
    /// またぐ場合は、[`RangeId`](crate::RangeId) の巡回表現と同様に分割される。
    /// F・Y方向の値は変更しない。
    ///
    /// # バリデーション
    /// - `z` が [`ZoomLevel::MAX`] を超える場合は [`SpatialIdError::ZOutOfRange`] を返す。
    pub(crate) fn shift_x<Z: Into<u8>>(
        &self,
        z: Z,
        index: i32,
    ) -> Result<impl Iterator<Item = FlexId> + use<Z>, Error> {
        let z = z.into();
        let _zoom = ZoomLevel::new(z)?;

        let x_zoomlevel = self.x_zoomlevel();
        let max_z = x_zoomlevel.max(z);

        // max_z における周長（Xインデックス値数）。
        let circumference = 1_i64 << max_z;
        let segment_scale = 1_i64 << (max_z - x_zoomlevel);
        let delta_index = index as i64 * (1_i64 << (max_z - z));

        let left = self.x_index() as i64 * segment_scale + delta_index;
        let right = left + segment_scale - 1;

        let left_wrapped = left.rem_euclid(circumference);
        let right_wrapped = right.rem_euclid(circumference);
        let ranges: Vec<[u32; 2]> = if left_wrapped <= right_wrapped {
            vec![[left_wrapped as u32, right_wrapped as u32]]
        } else {
            vec![
                [left_wrapped as u32, (circumference - 1) as u32],
                [0, right_wrapped as u32],
            ]
        };

        let f_zoomlevel = self.f_zoomlevel();
        let f_index = self.f_index();
        let y_zoomlevel = self.y_zoomlevel();
        let y_index = self.y_index();
        let (t_zoomlevel, t_index) = (self.t_zoomlevel(), self.t_index());

        Ok(ranges
            .into_iter()
            .flat_map(move |range| split_xy(max_z, range))
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

    /// このFlexIdを南北（Y）方向へ、ズームレベル `z` のインデックス値 `index` 個分だけ平行移動した結果を返す。
    ///
    /// Y方向は巡回せず `[0[z]]` に制限される。`z` が このFlexIdのYズームレベルより細かい場合は結果が分割されることがある。F・X方向の値は変更しない。
    ///
    /// # バリデーション
    /// - `z` が [`ZoomLevel::MAX`] を超える場合は [`SpatialIdError::ZOutOfRange`] を返す。
    /// - 移動後の位置が、両者を合わせたズーム `max(y_zoomlevel, z)` のY範囲を超える場合は
    ///   [`SpatialIdError::YOutOfRange`] を返す。
    pub(crate) fn shift_y<Z: Into<u8>>(
        &self,
        z: Z,
        index: i32,
    ) -> Result<impl Iterator<Item = FlexId> + use<Z>, Error> {
        let z = z.into();
        let _zoom = ZoomLevel::new(z)?;

        let y_zoomlevel = self.y_zoomlevel();
        let max_z = y_zoomlevel.max(z);

        let segment_scale = 1_i64 << (max_z - y_zoomlevel);
        let delta_index = index as i64 * (1_i64 << (max_z - z));

        let left = self.y_index() as i64 * segment_scale + delta_index;
        let right = left + segment_scale - 1;

        // 移動後が max_z のY範囲 [0[max_z]] を超える場合はエラー。
        let y_max = ZoomLevel::new(max_z)?.xy_max() as i64;
        if left < 0 || right > y_max {
            let offending = if left < 0 { left } else { right };
            return Err(SpatialIdError::YOutOfRange {
                z: max_z,
                y: offending.clamp(0, u32::MAX as i64) as u32,
            }
            .into());
        }

        // F・X以外の成分は値で捕捉する。
        let f_zoomlevel = self.f_zoomlevel();
        let f_index = self.f_index();
        let x_zoomlevel = self.x_zoomlevel();
        let x_index = self.x_index();
        let (t_zoomlevel, t_index) = (self.t_zoomlevel(), self.t_index());

        Ok(
            split_xy(max_z, [left as u32, right as u32]).map(move |(seg_z, seg_index)| {
                // SAFETY: 空間成分は範囲内検証済み、時間成分は元の有効な FlexId を継承
                unsafe {
                    FlexId::new_unchecked(
                        f_zoomlevel,
                        f_index,
                        x_zoomlevel,
                        x_index,
                        seg_z,
                        seg_index,
                    )
                    .time_with_segment_unchecked(t_zoomlevel, t_index)
                }
            }),
        )
    }
}
