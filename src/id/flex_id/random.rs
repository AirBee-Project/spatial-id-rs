#[cfg(all(test, feature = "temporal_id"))]
use crate::time::TZoomLevel;
use crate::{FlexId, ZoomLevel};
use core::ops::RangeInclusive;
#[cfg(test)]
use proptest::prelude::*;
use rand::{Rng, RngExt};

impl FlexId {
    /// ランダムな [`FlexId`] を作成する。時間軸は全時間になる。
    pub fn random() -> Self {
        let mut rng = rand::rng();
        Self::random_using(&mut rng)
    }

    /// 各空間軸が同じズームレベル `z` のランダムな [`FlexId`] を作成する。時間軸は全時間になる。
    pub fn random_at(z: u8) -> Self {
        let mut rng = rand::rng();
        Self::random_at_using(&mut rng, z)
    }

    /// 各空間軸が `z_range` 内のランダムなズームを持つ [`FlexId`] を作成する。時間軸は全時間になる。
    pub fn random_within(z_range: RangeInclusive<u8>) -> Self {
        let mut rng = rand::rng();
        Self::random_within_using(&mut rng, z_range)
    }

    /// 外部から渡された乱数生成器を使って、全ズーム範囲からランダムな [`FlexId`] を生成する。
    pub fn random_using<R: Rng>(rng: &mut R) -> Self {
        Self::random_within_using(rng, 0..=ZoomLevel::MAX.get())
    }

    /// 外部から渡された乱数生成器を使って、指定ズーム `z` のランダムな [`FlexId`] を生成する。
    pub fn random_at_using<R: Rng>(rng: &mut R, z: u8) -> Self {
        Self::random_within_using(rng, z..=z)
    }

    /// 外部から渡された乱数生成器を使って、各空間軸を `z_range` 内で選んだランダムな [`FlexId`] を生成する。
    ///
    /// `z_range` の終端は [`ZoomLevel::MAX`] でクリップされ、`start > end` の場合は `end` を採用する。
    pub fn random_within_using<R: Rng>(rng: &mut R, z_range: RangeInclusive<u8>) -> Self {
        let fz = ZoomLevel::random_in(rng, z_range.clone());
        let xz = ZoomLevel::random_in(rng, z_range.clone());
        let yz = ZoomLevel::random_in(rng, z_range);

        let f = rng.random_range(fz.f_min()..=fz.f_max());
        let x = rng.random_range(0..=xz.xy_max());
        let y = rng.random_range(0..=yz.xy_max());

        FlexId::new(fz, f, xz, x, yz, y).expect("Failed to generate random FlexId")
    }

    /// 外部から渡された乱数生成器を使って、`tz_range` 内の時間セグメントを設定した新しい [`FlexId`] を返す。
    ///
    /// `temporal_id` feature 無効時は時間軸を持たないため、元の [`FlexId`] をそのまま返す。
    pub fn time_random_within_using<R: Rng>(
        self,
        rng: &mut R,
        tz_range: RangeInclusive<u8>,
    ) -> Self {
        #[cfg(feature = "temporal_id")]
        {
            let tz = crate::time::TZoomLevel::random_in(rng, tz_range);
            let t = rng.random_range(0..=tz.max_index());
            self.time_with_segment(tz, t)
                .expect("Failed to generate random time segment for FlexId")
        }

        #[cfg(not(feature = "temporal_id"))]
        {
            let _ = (rng, tz_range);
            self
        }
    }

    /// 全空間ズーム範囲から [`FlexId`] を生成する `proptest` 用の [`Strategy`] を返す。
    #[cfg(test)]
    pub fn arb() -> impl Strategy<Value = Self> {
        Self::arb_within(0..=ZoomLevel::MAX.get())
    }

    /// 指定空間ズーム `z` の [`FlexId`] を生成する `proptest` 用の [`Strategy`] を返す。
    #[cfg(test)]
    pub fn arb_at(z: u8) -> impl Strategy<Value = Self> {
        Self::arb_within(z..=z)
    }

    /// 各空間軸を `z_range` 内で選んだ [`FlexId`] を生成する `proptest` 用の [`Strategy`] を返す。
    #[cfg(test)]
    pub fn arb_within(z_range: RangeInclusive<u8>) -> impl Strategy<Value = Self> {
        let clamped = ZoomLevel::clamp_range(z_range);
        (clamped.clone(), clamped.clone(), clamped).prop_flat_map(|(fz_u8, xz_u8, yz_u8)| {
            let fz = ZoomLevel::new(fz_u8).expect("clamped zoom must be valid");
            let xz = ZoomLevel::new(xz_u8).expect("clamped zoom must be valid");
            let yz = ZoomLevel::new(yz_u8).expect("clamped zoom must be valid");

            (
                Just(fz),
                fz.f_min()..=fz.f_max(),
                Just(xz),
                0..=xz.xy_max(),
                Just(yz),
                0..=yz.xy_max(),
            )
                .prop_map(|(fz, f, xz, x, yz, y)| {
                    FlexId::new(fz, f, xz, x, yz, y).expect("Strategy generated invalid FlexId")
                })
        })
    }

    /// 空間ズーム範囲 `z_range` と時間ズーム範囲 `tz_range` から [`FlexId`] を生成する `proptest` 用の [`Strategy`] を返す。
    ///
    /// `temporal_id` feature 無効時は時間軸を持たない [`FlexId`] を生成する。
    #[cfg(all(test, feature = "temporal_id"))]
    pub fn arb_with_time(
        z_range: RangeInclusive<u8>,
        tz_range: RangeInclusive<u8>,
    ) -> impl Strategy<Value = Self> {
        let clamped_tz = TZoomLevel::clamp_range(tz_range);
        (Self::arb_within(z_range), clamped_tz).prop_flat_map(|(id, tz_u8)| {
            let tz = TZoomLevel::new(tz_u8).expect("clamped tz must be valid");
            (Just(id), Just(tz), 0..=tz.max_index()).prop_map(|(id, tz, t)| {
                id.time_with_segment(tz, t)
                    .expect("Strategy generated invalid time segment")
            })
        })
    }

    /// 空間ズーム範囲 `z_range` と時間ズーム範囲 `tz_range` から [`FlexId`] を生成する `proptest` 用の [`Strategy`] を返す。
    ///
    /// `temporal_id` feature 無効時は時間軸を持たない [`FlexId`] を生成する。
    #[cfg(all(test, not(feature = "temporal_id")))]
    pub fn arb_with_time(
        z_range: RangeInclusive<u8>,
        _tz_range: RangeInclusive<u8>,
    ) -> impl Strategy<Value = Self> {
        Self::arb_within(z_range)
    }
}

#[cfg(test)]
mod tests {
    use crate::{FlexId, SpatialId};
    use rand_chacha::{ChaCha8Rng, rand_core::SeedableRng};

    #[test]
    fn random_within_and_time_respect_zoom_bounds() {
        let mut rng = ChaCha8Rng::seed_from_u64(0x5678);
        for _ in 0..50 {
            let id = FlexId::random_within_using(&mut rng, 3..=6)
                .time_random_within_using(&mut rng, 2..=5);
            assert!((3..=6).contains(&id.f_zoomlevel()));
            assert!((3..=6).contains(&id.x_zoomlevel()));
            assert!((3..=6).contains(&id.y_zoomlevel()));
            if cfg!(feature = "temporal_id") {
                assert!((2..=5).contains(&id.t_zoomlevel()));
            } else {
                assert!(id.time_is_whole());
            }
        }
    }
}
