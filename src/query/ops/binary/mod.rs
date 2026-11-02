use crate::id::dimension::Dimension;
use crate::id::flex_id::ZOrderRel;
use crate::query::SafeValue;
use crate::query::query_iter::QueryIter;
use crate::{Error, FlexId, Side};
use alloc::vec::Vec;
use core::iter::Fuse;

pub(crate) mod difference;
pub(crate) mod intersection;
pub(crate) mod merge;

/// 二項演算で [`QueryIter`] の先頭要素の参照と、分割した断片の差し戻しを行うカーソル。
struct StreamCursor<'a, V> {
    iter: Fuse<QueryIter<'a, V>>,
    /// 先頭から順に取り出す残り要素（末尾が先頭）。
    stack: Vec<(FlexId, V)>,
}

impl<'a, V: Clone> StreamCursor<'a, V> {
    fn new(iter: QueryIter<'a, V>) -> Self {
        Self {
            iter: iter.fuse(),
            stack: Vec::new(),
        }
    }

    /// 先頭要素を参照する。`stack` が空ならイテレーターから 1 要素だけ読み込む。
    fn peek(&mut self) -> Result<Option<&(FlexId, V)>, Error> {
        if self.stack.is_empty()
            && let Some(item) = self.iter.next()
        {
            self.stack.push(item?);
        }
        Ok(self.stack.last())
    }

    /// 直前に [`peek`](Self::peek) で確認した先頭要素を取り出す。
    fn pop(&mut self) -> (FlexId, V) {
        self.stack.pop().expect("peek 済みの要素が存在する")
    }

    /// `enclosing` に含まれる先頭要素群を `dim` で 2 分割し、FlexId の Z-order 列を保って `stack` に戻す。
    ///
    /// 先頭要素が `enclosing` そのものであれば 1 要素を 2 分割するだけで済むが、`first_id != enclosing`
    /// （異方性により粗い `dim` を飛ばして後続の次元で先に割られている場合）は、`dim` の下側・上側の順に
    /// 並べ替えるため `enclosing` 内の要素をすべて `stack` に積む。そのため、粗い次元を共有する要素が
    /// 広い範囲に多数存在すると、その範囲全体がメモリに載る。
    fn split_enclosing(&mut self, enclosing: FlexId, dim: Dimension) -> Result<(), Error> {
        let (first_id, first_val) = self.pop();
        // QueryIter の要素は互いに素なので、先頭が enclosing そのものなら後続要素は enclosing に含まれない。
        // 次要素の peek すら行わずに済むため、不要な先読みと一時 Vec 確保を回避できる。
        if first_id == enclosing {
            let lower = first_id.split_on(dim, Side::Lower).unwrap();
            let upper = first_id.split_on(dim, Side::Upper).unwrap();
            self.stack.push((upper, first_val.clone()));
            self.stack.push((lower, first_val));
            return Ok(());
        }

        // 先頭が dim を飛ばして FlexId の PartialOrd の後続の位置で先に割られている場合、QueryIter の順序制約から
        // enclosing 内の後続要素もすべて dim では未分割であることが保証される。
        let mut items = alloc::vec![(first_id, first_val)];
        while let Some((id, _)) = self.peek()? {
            if !enclosing.contains(id) {
                break;
            }
            items.push(self.pop());
        }
        self.stack.reserve(items.len() * 2);
        for (id, val) in items.iter().rev() {
            let upper = id.split_on(dim, Side::Upper).unwrap();
            self.stack.push((upper, val.clone()));
        }
        for (id, val) in items.into_iter().rev() {
            let lower = id.split_on(dim, Side::Lower).unwrap();
            self.stack.push((lower, val));
        }
        Ok(())
    }
}

/// 2 つの [`QueryIter`] を FlexId の PartialOrd で突き合わせて二項演算を行う。
///
/// - `on_left`: 左辺のみに存在する領域の値を変換する関数。[`None`] なら出力せず、右辺枯渇時に即終了する。
/// - `on_right`: 右辺のみに存在する領域の値を変換する関数。[`None`] なら出力せず、左辺枯渇時に右辺をポーリングせず即終了する。
/// - `on_both`: 両辺で一致した領域の値を合成する関数。[`None`] なら出力しない。
pub(crate) fn run_binary<'a, V: SafeValue + 'static>(
    lhs: QueryIter<'a, V>,
    rhs: QueryIter<'a, V>,
    on_left: Option<impl Fn(V) -> V + Send + 'a>,
    on_right: Option<impl Fn(V) -> V + Send + 'a>,
    on_both: Option<impl Fn(V, V) -> V + Send + 'a>,
) -> Result<QueryIter<'a, V>, Error> {
    let mut cur_a = StreamCursor::new(lhs);
    let mut cur_b = StreamCursor::new(rhs);
    let mut done = false;

    Ok(QueryIter::new(core::iter::from_fn(move || {
        if done {
            return None;
        }
        loop {
            let a = match cur_a.peek() {
                Ok(Some((id, _))) => Some(*id),
                Ok(None) if on_right.is_none() => {
                    done = true;
                    return None;
                }
                Ok(None) => None,
                Err(e) => {
                    done = true;
                    return Some(Err(e));
                }
            };
            let b = match cur_b.peek() {
                Ok(Some((id, _))) => Some(*id),
                Ok(None) if on_left.is_none() => {
                    done = true;
                    return None;
                }
                Ok(None) => None,
                Err(e) => {
                    done = true;
                    return Some(Err(e));
                }
            };
            match (a, b) {
                (None, None) => {
                    done = true;
                    return None;
                }
                (Some(_), None) => {
                    let (id, val) = cur_a.pop();
                    let f = on_left.as_ref().unwrap();
                    return Some(Ok((id, f(val))));
                }
                (None, Some(_)) => {
                    let (id, val) = cur_b.pop();
                    let f = on_right.as_ref().unwrap();
                    return Some(Ok((id, f(val))));
                }
                (Some(a_id), Some(b_id)) => match a_id.zorder_rel(&b_id) {
                    ZOrderRel::Less => {
                        let (id, val) = cur_a.pop();
                        if let Some(ref f) = on_left {
                            return Some(Ok((id, f(val))));
                        }
                    }
                    ZOrderRel::Greater => {
                        let (id, val) = cur_b.pop();
                        if let Some(ref f) = on_right {
                            return Some(Ok((id, f(val))));
                        }
                    }
                    ZOrderRel::Equal => {
                        let (id, val_a) = cur_a.pop();
                        let (_, val_b) = cur_b.pop();
                        if let Some(ref f) = on_both {
                            return Some(Ok((id, f(val_a, val_b))));
                        }
                    }
                    ZOrderRel::Overlap {
                        enclosing,
                        split_dim,
                        a_is_unsplit,
                    } => {
                        let res = if a_is_unsplit {
                            cur_a.split_enclosing(enclosing, split_dim)
                        } else {
                            cur_b.split_enclosing(enclosing, split_dim)
                        };
                        if let Err(e) = res {
                            done = true;
                            return Some(Err(e));
                        }
                    }
                },
            }
        }
    })))
}

#[cfg(test)]
mod test {
    use crate::query::merge_policy::Sum;
    use crate::query::query_iter::QueryIter;
    use crate::{CancellationToken, Error, FlexId, RangeId, Source, SpatialIdMap};

    fn rng(mut seed: u64) -> impl FnMut(u64) -> u64 {
        move |n| {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (seed >> 33) % n
        }
    }

    fn random_id(next: &mut impl FnMut(u64) -> u64, max_zoom: u64) -> FlexId {
        let fz = next(max_zoom) as u8;
        let xz = next(max_zoom) as u8;
        let yz = next(max_zoom) as u8;
        let f = next(2 << fz) as i32 - (1 << fz);
        let x = next(1 << xz) as u32;
        let y = next(1 << yz) as u32;
        let tz = if cfg!(feature = "temporal_id") {
            next(max_zoom) as u8
        } else {
            0
        };
        let t = next(1 << tz);
        FlexId::new(fz, f, xz, x, yz, y)
            .unwrap()
            .with_time_segment(tz, t)
            .unwrap()
    }

    #[test]
    fn binary_ops_stream_matches_tree_operations_on_anisotropic_ids() {
        let mut next = rng(0x5eed_cafe);
        for max_zoom in [4, 12] {
            for _ in 0..40 {
                let mut a = SpatialIdMap::<i32>::new();
                let mut b = SpatialIdMap::<i32>::new();
                for _ in 0..25 {
                    a.insert(random_id(&mut next, max_zoom), next(10) as i32 + 1);
                    b.insert(random_id(&mut next, max_zoom), next(10) as i32 + 1);
                }

                let inter = a
                    .clone()
                    .query()
                    .intersection(b.clone().query())
                    .get(..)
                    .into_map()
                    .unwrap();
                assert_eq!(inter, a.intersection(&b));

                let diff = a
                    .clone()
                    .query()
                    .difference(b.clone().query())
                    .get(..)
                    .into_map()
                    .unwrap();
                assert_eq!(diff, a.difference(&b));

                let merged = a
                    .clone()
                    .query()
                    .merge(b.clone().query(), Sum)
                    .get(..)
                    .into_map()
                    .unwrap();
                let mut expected_merge = a.clone();
                for (id, &val) in b.iter() {
                    expected_merge.insert_with(id, val, |x, y| x + y);
                }
                assert_eq!(merged, expected_merge);
            }
        }
    }

    struct PanicSource;

    impl Source for PanicSource {
        type Value = i32;

        fn read_flex_ids<'a>(
            &'a self,
            _targets: &[RangeId],
            _token: &CancellationToken,
        ) -> Result<QueryIter<'a, i32>, Error> {
            Ok(QueryIter::new(core::iter::from_fn(|| {
                panic!("短絡評価されるべき右辺がポーリングされた")
            })))
        }
    }

    #[test]
    fn empty_lhs_short_circuits_rhs_in_intersection_and_difference() {
        let empty = SpatialIdMap::<i32>::new();
        let inter = empty
            .clone()
            .query()
            .intersection(PanicSource.query())
            .get(..)
            .into_map()
            .unwrap();
        assert!(inter.is_empty());

        let diff = empty
            .query()
            .difference(PanicSource.query())
            .get(..)
            .into_map()
            .unwrap();
        assert!(diff.is_empty());
    }
}
