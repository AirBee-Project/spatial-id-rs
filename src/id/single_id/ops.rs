use crate::SpatialId;
use alloc::vec::Vec;

use crate::time::span::TimeSpan;
use crate::{RangeId, SingleId};

impl SingleId {
    /// 空間3軸（F/X/Y）だけで重なりを判定し、重なる場合は「深い側」の [`SingleId`] を返す。
    ///
    /// FlexIdは四分木（八分木）なので、異なるズームのSegmentは「入れ子」か「素」のどちらかしか
    /// あり得ない。したがって重なる場合の交差は必ず深い側のSegmentそのものになる。
    fn spatial_intersection<'a>(&'a self, other: &'a Self) -> Option<&'a Self> {
        let (deep, shallow) = if self.z() > other.z() {
            (self, other)
        } else {
            (other, self)
        };

        let shift = deep.z() - shallow.z();

        if (deep.f() >> shift) == shallow.f()
            && (deep.x() >> shift) == shallow.x()
            && (deep.y() >> shift) == shallow.y()
        {
            Some(deep)
        } else {
            None
        }
    }

    /// 相手の [`SingleId`] との差集合（`self - other`）を計算し、イテレータとして返す。
    ///
    /// 空間と時間の両方を考慮し、`other` に含まれる領域を取り除いた残りを、必要に応じて細かい
    /// ID に分割して返す。重なりがない場合は `self` をそのまま 1 件返し、完全に含まれる場合は
    /// 空になる。
    ///
    /// # 戻り値の型について
    ///
    /// 要素は [`SingleId`] ではなく [`RangeId`] である。時間を刳り抜いた残りは
    /// 単一Segment（`{i}/{t}`）で表せるとは限らないため（[`intersection`](Self::intersection)の
    /// 説明を参照）、空間が1Segmentであっても時間が範囲になりうる。
    ///
    /// # パラメーター
    /// * `other` - 差し引く相手の [`SingleId`] である。
    ///
    /// # 動作コスト
    /// 空間的な分割回数は、`self` と `other` の重なりを解消するために必要なズーム差に比例する。
    /// 占有空間が一部でも重なっている場合はエラーにならない。
    /// 時間的な差分は絶対秒区間の差集合（最大2区間）の結果個数に比例する。
    ///
    /// # 動作例
    ///
    /// 重なりがない場合:
    /// ```
    /// # use spatial_id::{RangeId, SingleId};
    /// let left = SingleId::new(2, 1, 1, 1).unwrap();
    /// let right = SingleId::new(2, 2, 1, 1).unwrap();
    /// let diff: Vec<_> = left.difference(&right).collect();
    /// assert_eq!(diff, vec![RangeId::from(&left)]);
    /// ```
    ///
    /// 一方が他方を含む場合:
    /// ```
    /// # use spatial_id::SingleId;
    /// let parent = SingleId::new(1, 0, 0, 0).unwrap();
    /// let child = SingleId::new(2, 0, 1, 1).unwrap();
    /// let diff: Vec<_> = parent.difference(&child).collect();
    /// assert_eq!(diff.len(), 7);
    /// ```
    pub fn difference(&self, other: &Self) -> impl Iterator<Item = RangeId> {
        let mut results: Vec<RangeId> = Vec::new();

        // 空間が重ならないなら、時間を見るまでもなく self がそのまま残る。
        if self.spatial_intersection(other).is_none() {
            results.push(RangeId::from(self));
            return results.into_iter();
        }

        // `other` の方が深い場合は、`other` と同じズームまで self を割り下げる。
        // 途中で生じた「`other` と重ならない兄弟」は、self の時間をそのまま持って確定する。
        let mut current = *self;
        while current.z() < other.z() {
            let next_z = current.z() + 1;
            let children: Vec<_> = current
                .spatial_children_at_zoom(next_z)
                .expect("next_z は current.z() より1段深く、other.z() 以下なので常に有効")
                .collect();

            for child in children {
                if child.spatial_intersection(other).is_some() {
                    current = child;
                } else {
                    results.push(RangeId::from(&child));
                }
            }
        }

        // 空間的に `other` と一致（または内包される）ところまで来たので、残るは時間の差分だけ。
        for time_span in TimeSpan::of(&current).difference(&TimeSpan::of(other)) {
            results.push(
                time_span
                    .apply_to(RangeId::from(&current), None)
                    .expect("差分は元の区間の部分なので常に有効"),
            );
        }

        results.into_iter()
    }

    /// 2つの [`SingleId`] の重なっている領域（Intersection）を計算して返す。
    ///
    /// 空間軸については、より深いズームレベル側の座標を浅い側に合わせて比較し、両者が同じ領域に
    /// 属する場合に重なりありとする。時間軸については
    /// 絶対秒区間の重なりで求める。重なりがない場合は [`None`] を返す。
    ///
    /// # 戻り値の型について
    ///
    /// 戻り値は [`Option`]`<`[`SingleId`]`>` ではなく [`Option`]`<`[`RangeId`]`>` である。空間の交差は
    /// 必ず単一Segmentになるが、**時間の交差は単一Segmentにならないことがある**ためである。
    /// 例えば `5/1`（`[5, 10)`）と `7/1`（`[7, 14)`）の交差は `[7, 10)` で、これを単一Segment
    /// `{i}/{t}` として表すには `i = 3` かつ `t = 7/3` が必要になるが、`7` は `3` の倍数では
    /// ないので表せない。
    ///
    /// # パラメーター
    /// * `other` - 交差判定する相手の [`SingleId`] である。
    ///
    /// # 動作コスト
    /// 各辺（F、X、Y）ごとに1次元での区間の重なりを計算し、全次元で重なりがあればその交差部分を返す。
    /// 時間軸の判定は絶対秒区間どうしの比較なので定数時間である。
    ///
    /// # 動作例
    ///
    /// 祖先と子孫の重なり:
    /// ```
    /// # use spatial_id::{RangeId, SingleId};
    /// let ancestor = SingleId::new(2, 1, 1, 1).unwrap();
    /// let descendant = SingleId::new(3, 2, 2, 3).unwrap();
    /// assert_eq!(
    ///     ancestor.intersection(&descendant).unwrap(),
    ///     RangeId::from(&descendant)
    /// );
    /// ```
    ///
    /// 重なりがない場合:
    /// ```
    /// # use spatial_id::SingleId;
    /// let left = SingleId::new(3, 1, 1, 1).unwrap();
    /// let right = SingleId::new(3, 4, 1, 1).unwrap();
    /// assert!(left.intersection(&right).is_none());
    /// ```
    pub fn intersection(&self, other: &Self) -> Option<RangeId> {
        let deep = self.spatial_intersection(other)?;
        let time_span = TimeSpan::of(self).intersect(&TimeSpan::of(other))?;

        Some(
            time_span
                .apply_to(RangeId::from(deep), None)
                .expect("交差は両者の部分なので常に有効"),
        )
    }

    /// `self` が `other` を空間的・時間的に完全に包含するか判定する。
    ///
    /// ```
    /// # use spatial_id::SingleId;
    /// let parent = SingleId::new(2, 0, 0, 0).unwrap();
    /// let child = SingleId::new(3, 0, 0, 0).unwrap();
    /// let other = SingleId::new(2, 1, 0, 0).unwrap();
    /// assert!(parent.contains(&child));
    /// assert!(!parent.contains(&other));
    /// assert!(!child.contains(&parent));
    /// assert!(parent.contains(&parent));
    /// ```
    pub fn contains(&self, other: &Self) -> bool {
        if self.z() > other.z() {
            return false;
        }
        let shift = other.z() - self.z();
        if (other.f() >> shift) != self.f()
            || (other.x() >> shift) != self.x()
            || (other.y() >> shift) != self.y()
        {
            return false;
        }
        let (s1, e1) = self.time_range();
        let (s2, e2) = other.time_range();
        s1 <= s2 && e2 <= e1
    }

    /// 2 つの [`SingleId`] が空間的・時間的に重なっているかを判定する。
    ///
    /// ```
    /// # use spatial_id::SingleId;
    /// let parent = SingleId::new(2, 0, 0, 0).unwrap();
    /// let child = SingleId::new(3, 0, 0, 0).unwrap();
    /// let other = SingleId::new(2, 1, 0, 0).unwrap();
    /// assert!(parent.intersects(&child));
    /// assert!(!parent.intersects(&other));
    /// ```
    pub fn intersects(&self, other: &Self) -> bool {
        let (shallow, deep) = if self.z() <= other.z() {
            (self, other)
        } else {
            (other, self)
        };
        let shift = deep.z() - shallow.z();
        if (deep.f() >> shift) != shallow.f()
            || (deep.x() >> shift) != shallow.x()
            || (deep.y() >> shift) != shallow.y()
        {
            return false;
        }
        let (s1, e1) = self.time_range();
        let (s2, e2) = other.time_range();
        s1 < e2 && s2 < e1
    }
}
