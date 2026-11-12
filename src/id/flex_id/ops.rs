#[cfg(feature = "temporal_id")]
use crate::SpatialId;
use crate::{FlexId, Side};
use alloc::vec::Vec;

impl FlexId {
    /// 相手の [`FlexId`] との差集合（`self - other`）を計算し、イテレーターとして返す。
    ///
    /// 4軸（F / X / Y / T）とも同じ手順で、「交差と同じ深さになるまで2分し、交差しない側を
    /// 結果へ確定させる」ことを繰り返す。
    pub fn difference(&self, other: &FlexId) -> impl Iterator<Item = FlexId> + use<> {
        let mut results = Vec::new();

        let intersect = match self.intersection(other) {
            Some(i) => i,
            None => {
                results.push(*self);
                return results.into_iter();
            }
        };

        if self == &intersect {
            return results.into_iter();
        }

        let mut current = *self;

        for dim in crate::id::Dimension::ALL {
            while current.zoomlevel_on(dim) < intersect.zoomlevel_on(dim) {
                let lower = current
                    .split_on(dim, Side::Lower)
                    .expect("交差より浅いので必ず割れる");
                let upper = current
                    .split_on(dim, Side::Upper)
                    .expect("交差より浅いので必ず割れる");
                match current.side_toward(dim, &intersect) {
                    Side::Lower => {
                        results.push(upper);
                        current = lower;
                    }
                    Side::Upper => {
                        results.push(lower);
                        current = upper;
                    }
                }
            }
        }

        results.into_iter()
    }

    /// 2つの [`FlexId`] が重なっているかを判定する。
    ///
    /// [`intersection`](Self::intersection) と違い、重なっている領域の [`FlexId`] は作成しない。
    ///
    /// ```
    /// # use spatial_id::FlexId;
    /// let parent = FlexId::new(2, 0, 2, 0, 2, 0).unwrap();
    /// let child = FlexId::new(3, 1, 3, 0, 3, 1).unwrap();
    /// let other = FlexId::new(2, 1, 2, 0, 2, 0).unwrap();
    /// assert!(parent.intersects(&child));
    /// assert!(!parent.intersects(&other));
    /// ```
    #[inline]
    pub fn intersects(&self, other: &FlexId) -> bool {
        let f_ok = if self.f_zoomlevel >= other.f_zoomlevel {
            (self.f_index >> (self.f_zoomlevel.get() - other.f_zoomlevel.get())) == other.f_index
        } else {
            (other.f_index >> (other.f_zoomlevel.get() - self.f_zoomlevel.get())) == self.f_index
        };
        if !f_ok {
            return false;
        }
        let x_ok = if self.x_zoomlevel >= other.x_zoomlevel {
            (self.x_index >> (self.x_zoomlevel.get() - other.x_zoomlevel.get())) == other.x_index
        } else {
            (other.x_index >> (other.x_zoomlevel.get() - self.x_zoomlevel.get())) == self.x_index
        };
        if !x_ok {
            return false;
        }
        let y_ok = if self.y_zoomlevel >= other.y_zoomlevel {
            (self.y_index >> (self.y_zoomlevel.get() - other.y_zoomlevel.get())) == other.y_index
        } else {
            (other.y_index >> (other.y_zoomlevel.get() - self.y_zoomlevel.get())) == self.y_index
        };
        if !y_ok {
            return false;
        }

        // 時間軸は `temporal_id` 無効時には存在しない（双方とも全時間で必ず入れ子）
        #[cfg(feature = "temporal_id")]
        {
            if self.t_zoomlevel >= other.t_zoomlevel {
                (self.t_index >> (self.t_zoomlevel.get() - other.t_zoomlevel.get()))
                    == other.t_index
            } else {
                (other.t_index >> (other.t_zoomlevel.get() - self.t_zoomlevel.get()))
                    == self.t_index
            }
        }
        #[cfg(not(feature = "temporal_id"))]
        {
            true
        }
    }

    /// 2つの [`FlexId`] の重なっている領域（Intersection）を計算して返す。
    /// 重なりがない場合は [`None`] を返す。
    ///
    /// どの軸も「浅い側のSegmentが深い側を含むか」で判定する。四分木（八分木）／2進トライでは
    /// 異なるズームのSegmentは入れ子か素のどちらかしかないので、重なる場合の交差は必ず
    /// 深い側のSegmentそのものになる。
    #[inline]
    pub fn intersection(&self, other: &FlexId) -> Option<FlexId> {
        let (f_zoomlevel, f_index) = if self.f_zoomlevel >= other.f_zoomlevel {
            if (self.f_index >> (self.f_zoomlevel.get() - other.f_zoomlevel.get())) != other.f_index
            {
                return None;
            }
            (self.f_zoomlevel, self.f_index)
        } else {
            if (other.f_index >> (other.f_zoomlevel.get() - self.f_zoomlevel.get())) != self.f_index
            {
                return None;
            }
            (other.f_zoomlevel, other.f_index)
        };

        let (x_zoomlevel, x_index) = if self.x_zoomlevel >= other.x_zoomlevel {
            if (self.x_index >> (self.x_zoomlevel.get() - other.x_zoomlevel.get())) != other.x_index
            {
                return None;
            }
            (self.x_zoomlevel, self.x_index)
        } else {
            if (other.x_index >> (other.x_zoomlevel.get() - self.x_zoomlevel.get())) != self.x_index
            {
                return None;
            }
            (other.x_zoomlevel, other.x_index)
        };

        let (y_zoomlevel, y_index) = if self.y_zoomlevel >= other.y_zoomlevel {
            if (self.y_index >> (self.y_zoomlevel.get() - other.y_zoomlevel.get())) != other.y_index
            {
                return None;
            }
            (self.y_zoomlevel, self.y_index)
        } else {
            if (other.y_index >> (other.y_zoomlevel.get() - self.y_zoomlevel.get())) != self.y_index
            {
                return None;
            }
            (other.y_zoomlevel, other.y_index)
        };

        // 時間軸は `temporal_id` 無効時には存在しない（双方とも全時間で必ず入れ子）ので、
        // 判定ごと消して空間3軸だけの費用に戻す。
        #[cfg(feature = "temporal_id")]
        let (t_zoomlevel, t_index) = if self.t_zoomlevel >= other.t_zoomlevel {
            if (self.t_index >> (self.t_zoomlevel.get() - other.t_zoomlevel.get())) != other.t_index
            {
                return None;
            }
            (self.t_zoomlevel, self.t_index)
        } else {
            if (other.t_index >> (other.t_zoomlevel.get() - self.t_zoomlevel.get())) != self.t_index
            {
                return None;
            }
            (other.t_zoomlevel, other.t_index)
        };

        Some(FlexId {
            f_zoomlevel,
            f_index,
            x_zoomlevel,
            x_index,
            y_zoomlevel,
            y_index,
            #[cfg(feature = "temporal_id")]
            t_zoomlevel,
            #[cfg(feature = "temporal_id")]
            t_index,
        })
    }

    /// `self` が `other` を完全に包含するか判定する。
    ///
    /// ```
    /// # use spatial_id::FlexId;
    /// let parent = FlexId::new(2, 0, 2, 0, 2, 0).unwrap();
    /// let child = FlexId::new(3, 0, 3, 0, 3, 0).unwrap();
    /// let other = FlexId::new(2, 1, 2, 0, 2, 0).unwrap();
    ///
    /// assert!(parent.contains(&child));
    /// assert!(!parent.contains(&other));
    /// assert!(!child.contains(&parent)); // 子は親を包含しない
    /// assert!(parent.contains(&parent)); // 自身は自身を包含する
    /// ```
    #[inline]
    pub fn contains(&self, other: &FlexId) -> bool {
        if self.f_zoomlevel > other.f_zoomlevel
            || (other.f_index >> (other.f_zoomlevel.get() - self.f_zoomlevel.get())) != self.f_index
        {
            return false;
        }
        if self.x_zoomlevel > other.x_zoomlevel
            || (other.x_index >> (other.x_zoomlevel.get() - self.x_zoomlevel.get())) != self.x_index
        {
            return false;
        }
        if self.y_zoomlevel > other.y_zoomlevel
            || (other.y_index >> (other.y_zoomlevel.get() - self.y_zoomlevel.get())) != self.y_index
        {
            return false;
        }
        #[cfg(feature = "temporal_id")]
        if self.t_zoomlevel > other.t_zoomlevel
            || (other.t_index >> (other.t_zoomlevel.get() - self.t_zoomlevel.get())) != self.t_index
        {
            return false;
        }

        true
    }

    /// [`RangeId`](crate::RangeId) と交差するか判定する。**時間軸も含めて**判定する。
    ///
    /// X は経度方向に周期的なので、`x[0] > x[1]` の範囲は末尾から先頭へ折り返した
    /// 2区間として扱う（[`RangeId::set_x`](crate::RangeId::set_x) と同じ規約）。
    #[inline]
    pub fn intersects_range(&self, range: &crate::RangeId) -> bool {
        // 時間軸だけは「共通ズームでの整数範囲」に落とせない（`RangeId` の `Interval` は
        // 2の冪とは限らない）ので、絶対秒区間の重なりで判定する。
        // `temporal_id` 無効時は双方とも全時間で必ず重なるため、判定ごと消す。
        #[cfg(feature = "temporal_id")]
        {
            let (self_start, self_end) = self.seconds_range();
            let (range_start, range_end) = range.seconds_range();
            if self_start >= range_end || range_start >= self_end {
                return false;
            }
        }

        let rz = range.z();
        let [f_start, f_end] = range.f();
        if !overlaps_i32(self.f_zoomlevel(), self.f_index(), rz, f_start, f_end) {
            return false;
        }

        let [y_start, y_end] = range.y();
        if !overlaps_u32(self.y_zoomlevel(), self.y_index(), rz, y_start, y_end) {
            return false;
        }

        let [x_start, x_end] = range.x();
        let xz = self.x_zoomlevel();
        let xi = self.x_index();
        if x_start <= x_end {
            overlaps_u32(xz, xi, rz, x_start, x_end)
        } else {
            overlaps_u32(xz, xi, rz, x_start, (1_u32 << rz) - 1)
                || overlaps_u32(xz, xi, rz, 0, x_end)
        }
    }
}

#[inline]
fn overlaps_i32(
    segment_z: u8,
    segment_i: i32,
    range_z: u8,
    range_min: i32,
    range_max: i32,
) -> bool {
    if segment_z > range_z {
        let projected = segment_i >> (segment_z - range_z);
        projected >= range_min && projected <= range_max
    } else {
        let shift = range_z - segment_z;
        (range_min >> shift) <= segment_i && segment_i <= (range_max >> shift)
    }
}

#[inline]
fn overlaps_u32(
    segment_z: u8,
    segment_i: u32,
    range_z: u8,
    range_min: u32,
    range_max: u32,
) -> bool {
    if segment_z > range_z {
        let projected = segment_i >> (segment_z - range_z);
        projected >= range_min && projected <= range_max
    } else {
        let shift = range_z - segment_z;
        (range_min >> shift) <= segment_i && segment_i <= (range_max >> shift)
    }
}
