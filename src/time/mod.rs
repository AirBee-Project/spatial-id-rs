//! 時間の区間と、時間のズームレベルを表現する。

pub(crate) mod allowed_intervals;
pub(crate) mod interval;
pub(crate) mod segment;
pub(crate) mod span;

pub use crate::id::zoom_level::TZoomLevel;
pub use allowed_intervals::AllowedIntervals;
pub use interval::Interval;

/// テストで時間付きの ID を式として書くための補助。
#[cfg(all(test, feature = "temporal_id"))]
mod test_support {
    use crate::{Error, Interval, RangeId, SingleId, id::helpers::IntoRange};

    impl SingleId {
        pub(crate) fn time_with<I>(mut self, interval: I, t: u64) -> Result<Self, Error>
        where
            I: TryInto<Interval>,
            Error: From<I::Error>,
        {
            self.set_time(interval, t)?;
            Ok(self)
        }
    }

    impl RangeId {
        pub(crate) fn time_with<I, T>(mut self, interval: I, t: T) -> Result<Self, Error>
        where
            I: TryInto<Interval>,
            Error: From<I::Error>,
            T: IntoRange<u64>,
        {
            self.set_time(interval, t)?;
            Ok(self)
        }
    }
}
