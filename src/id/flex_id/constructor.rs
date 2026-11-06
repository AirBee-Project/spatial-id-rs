#[cfg(feature = "temporal_id")]
use crate::id::zoom_level::TZoomLevel;
use crate::{Error, FlexId, id::zoom_level::ZoomLevel};

impl FlexId {
    /// 空間3軸から [`FlexId`] を構築する。時間軸は全時間になる。
    pub fn new(
        f_zoomlevel: impl Into<u8>,
        f_index: i32,
        x_zoomlevel: impl Into<u8>,
        x_index: u32,
        y_zoomlevel: impl Into<u8>,
        y_index: u32,
    ) -> Result<FlexId, Error> {
        let fz = ZoomLevel::new(f_zoomlevel.into())?;
        let xz = ZoomLevel::new(x_zoomlevel.into())?;
        let yz = ZoomLevel::new(y_zoomlevel.into())?;

        fz.check_f(f_index)?;
        xz.check_x(x_index)?;
        yz.check_y(y_index)?;

        Ok(FlexId {
            f_zoomlevel: fz,
            f_index,
            x_zoomlevel: xz,
            x_index,
            y_zoomlevel: yz,
            y_index,
            #[cfg(feature = "temporal_id")]
            t_zoomlevel: TZoomLevel::MIN,
            #[cfg(feature = "temporal_id")]
            t_index: 0,
        })
    }

    /// 検証を行わずに空間3軸から構築する。時間軸は全時間になる。
    ///
    /// # Safety
    /// 呼び出し側は、各次元のズームレベルとインデックスが対応する有効範囲内であることを保証しなければならない。
    #[inline]
    pub unsafe fn new_unchecked(
        f_zoomlevel: u8,
        f_index: i32,
        x_zoomlevel: u8,
        x_index: u32,
        y_zoomlevel: u8,
        y_index: u32,
    ) -> FlexId {
        debug_assert!(
            FlexId::new(
                f_zoomlevel,
                f_index,
                x_zoomlevel,
                x_index,
                y_zoomlevel,
                y_index
            )
            .is_ok(),
            "new_unchecked: 構成的に有効なはずの FlexId が検証に失敗した \
             (zf={f_zoomlevel} f={f_index} zx={x_zoomlevel} x={x_index} zy={y_zoomlevel} y={y_index})"
        );
        FlexId {
            f_zoomlevel: unsafe { ZoomLevel::new_unchecked(f_zoomlevel) },
            f_index,
            x_zoomlevel: unsafe { ZoomLevel::new_unchecked(x_zoomlevel) },
            x_index,
            y_zoomlevel: unsafe { ZoomLevel::new_unchecked(y_zoomlevel) },
            y_index,
            #[cfg(feature = "temporal_id")]
            t_zoomlevel: TZoomLevel::MIN,
            #[cfg(feature = "temporal_id")]
            t_index: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::FlexId;

    /// 全時間のSegmentはどちらの feature 構成でも作れる。
    #[test]
    fn whole_time_segment_is_always_constructible() {
        assert!(
            FlexId::new(3, 1, 3, 1, 3, 1)
                .unwrap()
                .time_with_segment(0u8, 0)
                .is_ok()
        );
    }

    /// `temporal_id` 無効時は、時間を持つSegmentを構築段階で弾く。
    ///
    /// 無効時の木はT軸を分割しないため、受け付けると挿入した時間区間が黙って
    /// 全時間へ広がってしまう（読み出すと4倍などに化ける）。
    #[cfg(not(feature = "temporal_id"))]
    #[test]
    fn temporal_segment_is_rejected_without_the_feature() {
        assert!(
            FlexId::new(3, 1, 3, 1, 3, 1)
                .unwrap()
                .time_with_segment(2u8, 0)
                .is_err()
        );
        assert!(
            FlexId::new(3, 1, 3, 1, 3, 1)
                .unwrap()
                .time_with_segment(2u8, 1)
                .is_err()
        );
        // FromStr も同じ経路を通るので一緒に塞がる。
        assert!("3/1|3/1|3/1|2/1".parse::<FlexId>().is_err());
    }

    /// `temporal_id` 有効時は従来どおり構築できる。
    #[cfg(feature = "temporal_id")]
    #[test]
    fn temporal_segment_is_accepted_with_the_feature() {
        let id = FlexId::new(3, 1, 3, 1, 3, 1)
            .unwrap()
            .time_with_segment(2u8, 1)
            .unwrap();
        assert_eq!(id.t_zoomlevel(), 2);
        assert_eq!(id.t_index(), 1);
        assert!("3/1|3/1|3/1|2/1".parse::<FlexId>().is_ok());
    }
}
