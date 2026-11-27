use core::ops::RangeInclusive;
#[cfg(test)]
use proptest::prelude::{Just, Strategy};
use rand::{Rng, RngExt};

use crate::{RangeId, ZoomLevel};

impl RangeId {
    /// 外部から渡された乱数生成器を使って、全ズーム範囲からランダムな [`RangeId`] を生成する。
    ///
    /// 高ズームかつ広範囲の [`RangeId`] が選ばれるため、`.single_ids()` で全展開すると
    /// 大量のメモリを消費する可能性がある。要素数を抑えたい場合は
    /// [`random_bounded_using`](Self::random_bounded_using) を使うこと。
    pub fn random_using<R: Rng>(rng: &mut R) -> Self {
        Self::random_within_using(rng, 0..=ZoomLevel::MAX.get())
    }

    /// 外部から渡された乱数生成器を使用して、指定したズームレベルでランダムに [`RangeId`] を生成する。
    pub fn random_at_using<R: Rng>(rng: &mut R, z: u8) -> Self {
        Self::random_within_using(rng, z..=z)
    }

    /// 外部から渡された乱数生成器を使用して、指定したズームレベル範囲内でランダムに [`RangeId`] を生成する。
    pub fn random_within_using<R: Rng>(rng: &mut R, z_range: RangeInclusive<u8>) -> Self {
        let zoom = ZoomLevel::random_in(rng, z_range);
        let f_min = zoom.f_min();
        let f_max = zoom.f_max();
        let xy_max = zoom.xy_max();

        let f1 = rng.random_range(f_min..=f_max);
        let f2 = rng.random_range(f_min..=f_max);
        let x1 = rng.random_range(0..=xy_max);
        let x2 = rng.random_range(0..=xy_max);
        let y1 = rng.random_range(0..=xy_max);
        let y2 = rng.random_range(0..=xy_max);

        RangeId::new(zoom, [f1, f2], [x1, x2], [y1, y2])
            .expect("Generated parameters should be always valid")
    }

    /// 外部から渡された乱数生成器を使用して、各軸のスパン幅を `max_span` 以下に抑えたランダムな [`RangeId`] を生成する。
    ///
    /// 1 軸あたりの最大要素数は `max_span + 1`（最大総セル数は `(max_span + 1)^3`）となるため、
    /// `.single_ids()` による全展開やテストでの OOM を防げる。
    pub fn random_bounded_using<R: Rng>(
        rng: &mut R,
        z_range: RangeInclusive<u8>,
        max_span: u32,
    ) -> Self {
        let zoom = ZoomLevel::random_in(rng, z_range);
        let f_min = zoom.f_min();
        let f_max = zoom.f_max();
        let xy_max = zoom.xy_max();

        let span_f_max = (i64::from(f_max) - i64::from(f_min)).min(i64::from(max_span)) as u32;
        let span_xy_max = xy_max.min(max_span);

        let f_start = rng.random_range(f_min..=f_max);
        let f_end = f_start
            .saturating_add_unsigned(rng.random_range(0..=span_f_max))
            .min(f_max);

        let x_start = rng.random_range(0..=xy_max);
        let x_end = x_start
            .saturating_add(rng.random_range(0..=span_xy_max))
            .min(xy_max);

        let y_start = rng.random_range(0..=xy_max);
        let y_end = y_start
            .saturating_add(rng.random_range(0..=span_xy_max))
            .min(xy_max);

        RangeId::new(zoom, [f_start, f_end], [x_start, x_end], [y_start, y_end])
            .expect("Generated bounded parameters should be always valid")
    }

    /// 全空間（Z=0〜MAX）からランダムに [`RangeId`] を生成する。
    pub fn random() -> Self {
        let mut rng = rand::rng();
        Self::random_using(&mut rng)
    }

    /// 指定したズームレベルでランダムに [`RangeId`] を生成する。
    pub fn random_at(z: u8) -> Self {
        let mut rng = rand::rng();
        Self::random_at_using(&mut rng, z)
    }

    /// 指定ズーム範囲からランダムな [`RangeId`] を生成する。
    ///
    /// 内部で `rand::rng()` を作成して [`random_within_using`](Self::random_within_using) に委譲する。
    pub fn random_within(z_range: RangeInclusive<u8>) -> Self {
        let mut rng = rand::rng();
        Self::random_within_using(&mut rng, z_range)
    }

    /// 全ズーム範囲から [`RangeId`] を生成する `proptest` 用の [`Strategy`] を返す。
    #[cfg(test)]
    pub fn arb() -> impl Strategy<Value = Self> {
        Self::arb_within(0..=ZoomLevel::MAX.get())
    }

    /// 指定ズーム `z` の [`RangeId`] を生成する `proptest` 用の [`Strategy`] を返す。
    #[cfg(test)]
    pub fn arb_at(z: u8) -> impl Strategy<Value = Self> {
        Self::arb_within(z..=z)
    }

    /// 指定ズーム範囲の [`RangeId`] を生成する `proptest` 用の [`Strategy`] を返す。
    ///
    /// `z_range` の終端は [`ZoomLevel::MAX`] でクリップされ、`start > end` の場合は `end` のみを使う。
    #[cfg(test)]
    pub fn arb_within(z_range: RangeInclusive<u8>) -> impl Strategy<Value = Self> {
        ZoomLevel::clamp_range(z_range).prop_flat_map(|z| {
            let zoom = ZoomLevel::new(z).expect("clamped zoom must be valid");
            let f_min = zoom.f_min();
            let f_max = zoom.f_max();
            let xy_max = zoom.xy_max();

            let f_strat = (f_min..=f_max, f_min..=f_max);
            let x_strat = (0..=xy_max, 0..=xy_max);
            let y_strat = (0..=xy_max, 0..=xy_max);

            (Just(zoom), f_strat, x_strat, y_strat).prop_map(
                move |(zoom, (f1, f2), (x1, x2), (y1, y2))| {
                    RangeId::new(zoom, [f1, f2], [x1, x2], [y1, y2])
                        .expect("Generated parameters should be always valid")
                },
            )
        })
    }

    /// 各軸のスパン幅を `max_span` 以下に抑えた [`RangeId`] を生成する `proptest` 用の [`Strategy`] を返す。
    #[cfg(test)]
    pub fn arb_bounded(z_range: RangeInclusive<u8>, max_span: u32) -> impl Strategy<Value = Self> {
        ZoomLevel::clamp_range(z_range).prop_flat_map(move |z| {
            let zoom = ZoomLevel::new(z).expect("clamped zoom must be valid");
            let f_min = zoom.f_min();
            let f_max = zoom.f_max();
            let xy_max = zoom.xy_max();

            let span_f_max = (i64::from(f_max) - i64::from(f_min)).min(i64::from(max_span)) as u32;
            let span_xy_max = xy_max.min(max_span);

            (
                Just(zoom),
                f_min..=f_max,
                0..=span_f_max,
                0..=xy_max,
                0..=span_xy_max,
                0..=xy_max,
                0..=span_xy_max,
            )
                .prop_map(
                    move |(zoom, f_start, f_span, x_start, x_span, y_start, y_span)| {
                        let f_end = f_start.saturating_add_unsigned(f_span).min(zoom.f_max());
                        let x_end = x_start.saturating_add(x_span).min(zoom.xy_max());
                        let y_end = y_start.saturating_add(y_span).min(zoom.xy_max());

                        RangeId::new(zoom, [f_start, f_end], [x_start, x_end], [y_start, y_end])
                            .expect("Generated bounded range must be valid")
                    },
                )
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::RangeId;
    use rand_chacha::{ChaCha8Rng, rand_core::SeedableRng};

    #[test]
    fn random_bounded_using_caps_span_even_at_high_zoom() {
        let mut rng = ChaCha8Rng::seed_from_u64(0x1234);
        for _ in 0..50 {
            let id = RangeId::random_bounded_using(&mut rng, 25..=30, 4);
            assert!((25..=30).contains(&id.z()));
            assert!(id.f()[1] - id.f()[0] <= 4);
            assert!(id.x()[0] <= id.x()[1] && id.x()[1] - id.x()[0] <= 4);
            assert!(id.y()[0] <= id.y()[1] && id.y()[1] - id.y()[0] <= 4);
            assert!(id.single_ids().count() <= 125);
        }
    }
}
