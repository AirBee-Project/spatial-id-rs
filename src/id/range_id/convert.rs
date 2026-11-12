use core::iter::Chain;

use crate::time::{segment::TimeSegment, span::TimeSegments};
use crate::{FlexId, RangeId, SingleId};

/// XYにおけるセグメントの最適配置関数
pub fn split_xy(z: u8, range: [u32; 2]) -> impl Iterator<Item = (u8, u32)> {
    SegmentIter::xy(z, range).map(xy_segment)
}

/// Fにおけるセグメントの最適配置関数
pub fn split_f(z: u8, range: [i32; 2]) -> impl Iterator<Item = (u8, i32)> {
    SegmentIter::f(z, range).map(f_segment)
}

#[inline]
fn xy_segment((z, dim): (u8, i64)) -> (u8, u32) {
    (z, dim as u32)
}

/// [`SegmentIter::f`] の Segment を、F の元のインデックスに戻す。
#[inline]
fn f_segment((z, dim): (u8, i64)) -> (u8, i32) {
    (z, (dim - (1i64 << z)) as i32)
}

impl From<SingleId> for RangeId {
    #[inline]
    fn from(id: SingleId) -> Self {
        RangeId::from(&id)
    }
}

impl From<&SingleId> for RangeId {
    #[inline]
    fn from(id: &SingleId) -> Self {
        // 単一Segmentは範囲の退化した形であり、SingleId の構築時に検証済みなので常に変換できる。
        unsafe {
            RangeId::new_unchecked(id.z(), [id.f(), id.f()], [id.x(), id.x()], [id.y(), id.y()])
                .time_with_unchecked(id.i(), [id.t(), id.t()])
        }
    }
}

impl RangeId {
    /// この [RangeId]を[SingleId]に分解する。
    pub fn single_ids(self) -> impl Iterator<Item = SingleId> {
        let z = self.z.get();
        let xy_max = self.z.xy_max();
        let [f_min, f_max] = self.f;
        let [x0, x1] = self.x;
        let x_max_offset = x1.wrapping_sub(x0) & xy_max;
        let [y_min, y_max] = self.y;
        let (interval, [t_min, t_max]) = (self.i, self.t());

        let mut f = f_min;
        let mut dx = 0u32;
        let mut y = y_min;
        let mut t = t_min;
        let mut done = false;

        core::iter::from_fn(move || {
            if done {
                return None;
            }
            let x = (x0 + dx) & xy_max;
            let id =
                unsafe { SingleId::new_unchecked(z, f, x, y).time_with_unchecked(interval, t) };

            if t < t_max {
                t += 1;
            } else {
                t = t_min;
                if y < y_max {
                    y += 1;
                } else {
                    y = y_min;
                    if dx < x_max_offset {
                        dx += 1;
                    } else {
                        dx = 0;
                        if f < f_max {
                            f += 1;
                        } else {
                            done = true;
                        }
                    }
                }
            }
            Some(id)
        })
    }
}

/// [`RangeId`] を FlexTree のノードアドレス（[`FlexId`]）へ展開するイテレータ。
///
/// F・X・Y・時間の各軸の Segment の直積を、F を外側、時間を最も内側にして返す。
/// 各軸の Segment の列は小さな状態から何度でも同じ順に作れるので、外側の軸が1つ進むたびに内側の軸を
/// 最初から作り直す。中間の列をためないので、ヒープ確保をしない。
pub struct RangeIdFlexIds {
    f: SegmentIter,
    x_start: Chain<SegmentIter, SegmentIter>,
    y_start: SegmentIter,
    t_first: TimeSegment,
    t_rest_start: TimeSegments,
    x: Chain<SegmentIter, SegmentIter>,
    y: SegmentIter,
    t: TimeSegments,
    /// 今の F の Segment。
    f_segment: Option<(u8, i32)>,
    /// 今の F・X の Segment。
    fx_segment: Option<(u8, i32, u8, u32)>,
    /// 今の F・X・Y の Segment からなる空間部分（時間Segmentが2個以上あるときの2個目以降用）。
    spatial: Option<FlexId>,
}

impl Iterator for RangeIdFlexIds {
    type Item = FlexId;

    #[inline]
    fn next(&mut self) -> Option<FlexId> {
        loop {
            if let Some(spatial) = self.spatial {
                if let Some(time_segment) = self.t.next() {
                    return Some(unsafe {
                        spatial.time_with_segment_unchecked(
                            time_segment.zoom().get(),
                            time_segment.index(),
                        )
                    });
                }
                self.spatial = None;
            }
            if let Some((f_z, f_i, x_z, x_i)) = self.fx_segment {
                if let Some((y_z, y_i)) = self.y.next().map(xy_segment) {
                    // SAFETY: 各軸の Segment は、検証済みの RangeId の範囲を分割したもの
                    let spatial = unsafe { FlexId::new_unchecked(f_z, f_i, x_z, x_i, y_z, y_i) };
                    if !self.t_rest_start.is_empty() {
                        self.spatial = Some(spatial);
                        self.t = self.t_rest_start.clone();
                    }
                    return Some(unsafe {
                        spatial.time_with_segment_unchecked(
                            self.t_first.zoom().get(),
                            self.t_first.index(),
                        )
                    });
                }
                self.fx_segment = None;
            }
            if let Some((f_z, f_i)) = self.f_segment {
                if let Some((x_z, x_i)) = self.x.next().map(xy_segment) {
                    self.fx_segment = Some((f_z, f_i, x_z, x_i));
                    self.y = self.y_start.clone();
                    continue;
                }
                self.f_segment = None;
            }
            self.f_segment = Some(self.f.next().map(f_segment)?);
            self.x = self.x_start.clone();
        }
    }
}

impl RangeId {
    /// この [`RangeId`] を [FlexId]に分解する。
    pub fn flex_ids(self) -> RangeIdFlexIds {
        let z = self.z.get();
        // X は最大値を越えて 0 へ回り込む範囲を取れるので、回り込む前と後の2つに分ける
        let x_start = if self.x[0] <= self.x[1] {
            SegmentIter::xy(z, self.x).chain(SegmentIter::EMPTY)
        } else {
            SegmentIter::xy(z, [self.x[0], self.z.xy_max()])
                .chain(SegmentIter::xy(z, [0, self.x[1]]))
        };
        let y_start = SegmentIter::xy(z, self.y);
        // 時間は `Interval` が2の冪とは限らないので、2分岐Segmentへ分解してから掛け合わせる
        // （時間を使っていなければ全時間の1Segmentだけになる）
        let mut t_rest_start = self.time_segments();
        let t_first = t_rest_start
            .next()
            .expect("有効な RangeId の時間Segmentは少なくとも1つ存在する");
        RangeIdFlexIds {
            f: SegmentIter::f(z, self.f),
            x: x_start.clone(),
            y: y_start.clone(),
            t: t_rest_start.clone(),
            x_start,
            y_start,
            t_first,
            t_rest_start,
            f_segment: None,
            fx_segment: None,
            spatial: None,
        }
    }
}

/// 区間木的にセグメントを最適配置するイテレータ。
///
/// `l`/`r`は`i32`ではなく`i64`で保持する。[`split_f`]がFのインデックスへ`2^z`のオフセットを
/// 加えるため、`z = [`ZoomLevel::MAX`](crate::ZoomLevel::MAX)`（30）かつFが最大値のときに`2^31 - 1`（[`i32::MAX`]）へ達し、
/// 最後の要素を返した直後の`l += 1`が`i32`ではオーバーフローするからである。
#[derive(Clone)]
struct SegmentIter {
    l: i64,
    r: i64,
    cur_z: i8,
}

impl SegmentIter {
    const EMPTY: SegmentIter = SegmentIter {
        l: 1,
        r: 0,
        cur_z: 0,
    };

    /// X・Y のズーム `z` の範囲 `range` を分ける。
    #[inline]
    fn xy(z: u8, [l, r]: [u32; 2]) -> SegmentIter {
        let mut iter = SegmentIter {
            l: l as i64,
            r: r as i64,
            cur_z: z as i8,
        };
        iter.ascend();
        iter
    }

    /// F のズーム `z` の範囲 `range` を分ける。負のインデックスを扱えるよう `2^z` ずらして持つので、
    /// 返す Segment は [`f_segment`] で元のインデックスに戻す。
    #[inline]
    fn f(z: u8, [l, r]: [i32; 2]) -> SegmentIter {
        let diff = 1i64 << z;
        let mut iter = SegmentIter {
            l: l as i64 + diff,
            r: r as i64 + diff,
            cur_z: z as i8,
        };
        iter.ascend();
        iter
    }

    /// `l` が偶数かつ `r` が奇数である間、`trailing_zeros` で共通の2冪因子を一括シフトして親階層へ引き上げる。
    #[inline]
    fn ascend(&mut self) {
        if self.l < self.r && self.cur_z > 0 {
            let k = (self.l as u64 | (self.r as u64 + 1) | (1u64 << self.cur_z)).trailing_zeros();
            self.l >>= k;
            self.r >>= k;
            self.cur_z -= k as i8;
        }
    }
}

impl Iterator for SegmentIter {
    type Item = (u8, i64); // (z, dimension)

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.l > self.r {
            return None;
        }

        let z = self.cur_z as u8;
        let v = if self.l & 1 == 1 || self.l == self.r || self.cur_z == 0 {
            let v = self.l;
            self.l += 1;
            v
        } else {
            let v = self.r;
            self.r -= 1;
            v
        };
        self.ascend();
        Some((z, v))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ZoomLevel;
    use alloc::vec::Vec;

    /// `split_f`はFのインデックスへ`2^z`を足すため、最深ズームかつF最大値では
    /// [`i32::MAX`]ちょうどに達する。内部を`i64`で保持していないと、最後の要素を返した直後の
    /// `l += 1`がオーバーフローしてパニックする。
    #[test]
    fn split_f_does_not_overflow_at_deepest_zoom_upper_bound() {
        let z = ZoomLevel::MAX.get();
        let f_max = ZoomLevel::MAX.f_max();

        let got: Vec<_> = split_f(z, [f_max, f_max]).collect();
        assert_eq!(got, [(z, f_max)]);

        let f_min = ZoomLevel::MAX.f_min();
        let got: Vec<_> = split_f(z, [f_min, f_min]).collect();
        assert_eq!(got, [(z, f_min)]);
    }

    /// [`RangeIdFlexIds`] は、各軸の Segment の列の直積（F が外側、時間が最も内側）と同じ列を同じ順に返す。
    #[test]
    fn range_id_segments_match_product_of_axis_segments() {
        let mut state = 0x2545_f491_4f6c_dd1du64;
        let mut next = |n: u64| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state % n
        };
        #[cfg(feature = "temporal_id")]
        let times = ["", "_1800/3:7", "_7200/0", "_3600/0:5"];
        #[cfg(not(feature = "temporal_id"))]
        let times = [""];
        for _ in 0..500 {
            let z = next(8) as u8 + 1;
            let width = 1u64 << z;
            let mut f = || next(2 * width) as i64 - width as i64;
            let (f0, f1) = (f(), f());
            // X は大小が逆なら最大値から 0 へ回り込む範囲になる
            let (x0, x1, y0, y1) = (next(width), next(width), next(width), next(width));
            let time = times[next(times.len() as u64) as usize];
            let id: RangeId = format!("{z}/{f0}:{f1}/{x0}:{x1}/{y0}:{y1}{time}")
                .parse()
                .unwrap();

            let [x_l, x_r] = id.x();
            let xs: Vec<_> = if x_l <= x_r {
                split_xy(z, id.x()).collect()
            } else {
                split_xy(z, [x_l, id.z.xy_max()])
                    .chain(split_xy(z, [0, x_r]))
                    .collect()
            };
            let mut expected = Vec::new();
            for (f_z, f_i) in split_f(z, id.f()) {
                for &(x_z, x_i) in &xs {
                    for (y_z, y_i) in split_xy(z, id.y()) {
                        for t in id.time_segments() {
                            expected.push(unsafe {
                                FlexId::new_unchecked(f_z, f_i, x_z, x_i, y_z, y_i)
                                    .time_with_segment_unchecked(t.zoom().get(), t.index())
                            });
                        }
                    }
                }
            }
            assert_eq!(id.flex_ids().collect::<Vec<_>>(), expected, "{id}");
        }
    }

    /// 同じ境界を [`RangeId`] の展開経路からも踏む（[`SingleId`]は内部でこれを通る）。
    #[test]
    fn range_id_at_deepest_zoom_upper_bound_expands() {
        let z = ZoomLevel::MAX.get();
        let f_max = ZoomLevel::MAX.f_max();
        let xy_max = ZoomLevel::MAX.xy_max();

        let id = SingleId::new(z, f_max, xy_max, xy_max).unwrap();
        assert_eq!(id.flex_ids().count(), 1);
    }
}
