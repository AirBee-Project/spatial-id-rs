use core::ops::BitOr;

/// 時空間IDの次元。F/X/Yは空間3軸（各最大ズーム30）、Tは時間軸の生の2分岐Segment
/// （最大ズーム[`TZoomLevel::MAX`](crate::time::TZoomLevel::MAX)=35）。並び順（F→X→Y→T）は木が次元を選ぶときの規約で、
/// 他に意味は無い。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Dimension {
    F = 0,
    X = 1,
    Y = 2,
    T = 3,
}

impl Dimension {
    pub const ALL: [Dimension; 4] = [Dimension::F, Dimension::X, Dimension::Y, Dimension::T];
}

/// [`Dimension`] の集合。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(
    feature = "disk",
    derive(
        zerocopy::FromBytes,
        zerocopy::IntoBytes,
        zerocopy::Immutable,
        zerocopy::KnownLayout,
        zerocopy::Unaligned
    )
)]
#[repr(transparent)]
pub(crate) struct DimensionSet(u8);

impl DimensionSet {
    pub(crate) const EMPTY: Self = DimensionSet(0);

    pub(crate) const fn contains(self, dimension: Dimension) -> bool {
        self.0 & 1 << dimension as u8 != 0
    }

    pub(crate) fn iter(self) -> impl Iterator<Item = Dimension> {
        Dimension::ALL
            .into_iter()
            .filter(move |&d| self.contains(d))
    }
}

impl From<Dimension> for DimensionSet {
    fn from(dimension: Dimension) -> Self {
        DimensionSet(1 << dimension as u8)
    }
}

impl BitOr for DimensionSet {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self {
        DimensionSet(self.0 | rhs.0)
    }
}

impl FromIterator<Dimension> for DimensionSet {
    fn from_iter<I: IntoIterator<Item = Dimension>>(iter: I) -> Self {
        iter.into_iter()
            .fold(DimensionSet::EMPTY, |set, d| set | d.into())
    }
}
