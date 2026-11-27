use crate::{SingleId, ZoomLevel};
use core::ops::RangeInclusive;
#[cfg(test)]
use proptest::prelude::*;
use rand::{Rng, RngExt};

impl SingleId {
    /// ランダムな [`SingleId`] を作成する。
    pub fn random() -> Self {
        let mut rng = rand::rng();
        Self::random_using(&mut rng)
    }

    /// 特定のズームレベルにおいて、ランダムな [`SingleId`] を作成する。
    pub fn random_at(z: u8) -> Self {
        let mut rng = rand::rng();
        Self::random_at_using(&mut rng, z)
    }

    /// 特定のズームレベル間において、ランダムな [`SingleId`] を作成する。
    pub fn random_within(z_range: RangeInclusive<u8>) -> Self {
        let mut rng = rand::rng();
        Self::random_within_using(&mut rng, z_range)
    }

    /// 全ズーム範囲から [`SingleId`] を生成する `proptest` 用の [`Strategy`] を返す。
    #[cfg(test)]
    pub fn arb() -> impl Strategy<Value = Self> {
        Self::arb_within(0..=ZoomLevel::MAX.get())
    }

    /// 指定ズーム `z` の [`SingleId`] を生成する `proptest` 用の [`Strategy`] を返す。
    #[cfg(test)]
    pub fn arb_at(z: u8) -> impl Strategy<Value = Self> {
        Self::arb_within(z..=z)
    }

    /// 指定ズーム範囲の [`SingleId`] を生成する `proptest` 用の [`Strategy`] を返す。
    ///
    /// `z_range` の終端は [`ZoomLevel::MAX`] でクリップされ、`start > end` の場合は `end` のみを使う。
    #[cfg(test)]
    pub fn arb_within(z_range: RangeInclusive<u8>) -> impl Strategy<Value = Self> {
        ZoomLevel::clamp_range(z_range).prop_flat_map(|z| {
            let zoom = ZoomLevel::new(z).expect("clamped zoom must be valid");
            let f_strategy = zoom.f_min()..=zoom.f_max();
            let x_strategy = 0..=zoom.xy_max();
            let y_strategy = 0..=zoom.xy_max();

            (Just(z), f_strategy, x_strategy, y_strategy).prop_map(|(z, f, x, y)| {
                Self::new(z, f, x, y).expect("Strategy generated invalid ID")
            })
        })
    }

    /// 外部から渡された乱数生成器を使って、全ズーム範囲からランダムな [`SingleId`] を生成する。
    pub fn random_using<R: Rng>(rng: &mut R) -> Self {
        Self::random_within_using(rng, 0..=ZoomLevel::MAX.get())
    }

    /// 外部から渡された乱数生成器を使って、指定ズームのランダムな [`SingleId`] を生成する。
    pub fn random_at_using<R: Rng>(rng: &mut R, z: u8) -> Self {
        Self::random_within_using(rng, z..=z)
    }

    /// 外部から渡された乱数生成器を使って、指定ズーム範囲のランダムな [`SingleId`] を生成する。
    ///
    /// `z_range` の終端は [`ZoomLevel::MAX`] でクリップされ、`start > end` の場合は `end` を採用する。
    pub fn random_within_using<R: Rng>(rng: &mut R, z_range: RangeInclusive<u8>) -> Self {
        let zoom = ZoomLevel::random_in(rng, z_range);
        let f = rng.random_range(zoom.f_min()..=zoom.f_max());
        let x = rng.random_range(0..=zoom.xy_max());
        let y = rng.random_range(0..=zoom.xy_max());

        SingleId::new(zoom, f, x, y).expect("Failed to generate random SingleId")
    }
}
