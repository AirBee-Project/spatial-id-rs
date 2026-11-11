use core::ops::{BitAnd, BitOr, Sub};

use crate::SpatialIdSet;

impl BitOr<&SpatialIdSet> for &SpatialIdSet {
    type Output = SpatialIdSet;

    fn bitor(self, rhs: &SpatialIdSet) -> Self::Output {
        SpatialIdSet {
            inner: self.inner.union(&rhs.inner),
        }
    }
}

impl BitAnd<&SpatialIdSet> for &SpatialIdSet {
    type Output = SpatialIdSet;

    fn bitand(self, rhs: &SpatialIdSet) -> Self::Output {
        SpatialIdSet {
            inner: self.inner.intersection(&rhs.inner),
        }
    }
}

impl Sub<&SpatialIdSet> for &SpatialIdSet {
    type Output = SpatialIdSet;

    fn sub(self, rhs: &SpatialIdSet) -> Self::Output {
        SpatialIdSet {
            inner: self.inner.difference(&rhs.inner),
        }
    }
}

impl BitOr for SpatialIdSet {
    type Output = SpatialIdSet;

    fn bitor(self, rhs: Self) -> Self::Output {
        &self | &rhs
    }
}

impl BitAnd for SpatialIdSet {
    type Output = SpatialIdSet;

    fn bitand(self, rhs: Self) -> Self::Output {
        &self & &rhs
    }
}

impl Sub for SpatialIdSet {
    type Output = SpatialIdSet;

    fn sub(self, rhs: Self) -> Self::Output {
        &self - &rhs
    }
}
