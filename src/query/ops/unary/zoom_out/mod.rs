pub(crate) mod query;

#[cfg(test)]
mod test;

use crate::collection::core::FlexTreeCore;
use crate::query::SafeValue;
use crate::query::merge_policy::MergePolicy;
use crate::query::ops::unary::sort_by_flex_id;
use crate::query::{QueryIter, UnaryOperator};
use crate::{Error, FlexId, ZoomLevel};
use alloc::vec::Vec;
use core::mem::replace;

/// 指定されたズームレベルまで情報を落とす演算子。
pub struct ZoomOut<V, P> {
    pub target_z: ZoomLevel,
    _marker: core::marker::PhantomData<fn() -> (V, P)>,
}

impl<V, P> ZoomOut<V, P> {
    pub fn new(target_z: ZoomLevel) -> Self {
        Self {
            target_z,
            _marker: core::marker::PhantomData,
        }
    }
}

impl<V: SafeValue + 'static, P> UnaryOperator<V> for ZoomOut<V, P>
where
    P: MergePolicy<V>,
{
    fn run<'a>(&'a self, input: QueryIter<'a, V>) -> Result<QueryIter<'a, V>, Error> {
        let target_z = self.target_z.get();
        let mut input = input.fuse();
        // 組に T が `target_z` より細かい親があるときだけ、その組のキーを持つ。
        // なければ組のキーは `current` の親と同じなので、親が変われば必ず別の組になる
        let mut fine_group: Option<FlexId> = None;
        // いまの親と、可換なら畳み込んだ値、非可換なら最後に来た値
        let mut current: Option<(FlexId, V)> = None;
        // 非可換のとき、`current` の親にそれより前に来た値
        let mut values: Vec<V> = Vec::new();
        // 同じ組で `current` より前に出た親の値
        let mut earlier: Vec<(FlexId, V)> = Vec::new();
        // 返す順の逆に積む
        let mut ready: Vec<(FlexId, V)> = Vec::new();

        let iter = core::iter::from_fn(move || {
            // `ready` は `close_group` が先頭を返したあとの残りだけなので、ループの中では空のまま
            if let Some(item) = ready.pop() {
                return Some(Ok(item));
            }
            loop {
                let Some(item) = input.next() else {
                    return close_group::<V, P>(
                        &mut current,
                        &mut values,
                        &mut earlier,
                        &mut ready,
                    )
                    .map(Ok);
                };
                let (id, val) = match item {
                    Ok(item) => item,
                    Err(e) => return Some(Err(e)),
                };
                let parent = match id.spatial_parent_at_zoom(target_z) {
                    Ok(p) => p,
                    Err(e) => return Some(Err(e)),
                };
                if let Some((last, acc)) = &mut current
                    && *last == parent
                {
                    if P::IS_COMMUTATIVE {
                        *acc = P::resolve(acc.clone(), val);
                    } else {
                        values.push(replace(acc, val));
                    }
                    continue;
                }
                // 親は T のズームを落とさないので、T が `target_z` より細かいと同じ親の要素が連続するとは限らない。
                // T も `target_z` まで落とした領域なら、入力列の中で同じ領域の要素は必ず連続するので、それを組の単位にする
                let fine = parent.t_zoomlevel() > target_z;
                let key = if fine {
                    parent.ancestor_at([
                        parent.f_zoomlevel(),
                        parent.x_zoomlevel(),
                        parent.y_zoomlevel(),
                        target_z,
                    ])
                } else {
                    parent
                };
                let same_group = match fine_group {
                    Some(group) => group == key,
                    None => fine && current.as_ref().is_some_and(|(last, _)| *last == key),
                };
                let closed = if same_group {
                    if let Some((last, acc)) = current.take() {
                        earlier.extend(values.drain(..).chain([acc]).map(|v| (last, v)));
                    }
                    fine_group = Some(key);
                    None
                } else {
                    fine_group = fine.then_some(key);
                    close_group::<V, P>(&mut current, &mut values, &mut earlier, &mut ready)
                };
                current = Some((parent, val));
                if let Some(item) = closed {
                    return Some(Ok(item));
                }
            }
        });

        Ok(QueryIter::new(iter))
    }

    fn validate(&self) -> Result<(), crate::Error> {
        Ok(())
    }

    fn reverse_target(&self, bounds: crate::RangeId, out: &mut Vec<crate::RangeId>) {
        if bounds.z() > self.target_z.get() {
            out.push(bounds.spatial_parent_at_zoom(self.target_z.get()).unwrap());
        } else {
            out.push(bounds);
        }
    }
}

/// 組を閉じ、親ごとに `resolve_many` で合成した結果を FlexId の Z-order 列で返す。
/// 先頭の 1 件を戻り値で返し、残りは逆順で `ready` に積む。
fn close_group<V: SafeValue + 'static, P: MergePolicy<V>>(
    current: &mut Option<(FlexId, V)>,
    values: &mut Vec<V>,
    earlier: &mut Vec<(FlexId, V)>,
    ready: &mut Vec<(FlexId, V)>,
) -> Option<(FlexId, V)> {
    let (id, last) = current.take()?;
    if earlier.is_empty() {
        if P::IS_COMMUTATIVE {
            return Some((id, last));
        }
        return P::resolve_many(values.drain(..).chain([last])).map(|v| (id, v));
    }
    earlier.extend(values.drain(..).chain([last]).map(|v| (id, v)));
    resolve_overlapping::<V, P>(earlier, ready)
}

/// 同じ組に親が複数あるときの [`close_group`]。T が `target_z` より細かいときだけ通る。
#[cold]
#[inline(never)]
fn resolve_overlapping<V: SafeValue + 'static, P: MergePolicy<V>>(
    earlier: &mut Vec<(FlexId, V)>,
    ready: &mut Vec<(FlexId, V)>,
) -> Option<(FlexId, V)> {
    sort_by_flex_id(earlier);
    let mut drained = earlier.drain(..).peekable();
    // T のズームが違う親どうしは重なりうるので、重なりも MergePolicy で合成する
    let resolve = |a: &V, b: &V| P::resolve(a.clone(), b.clone());
    let mut tree = FlexTreeCore::<V>::default();
    while let Some((id, first)) = drained.next() {
        let rest = core::iter::from_fn(|| drained.next_if(|(next, _)| *next == id).map(|(_, v)| v));
        if let Some(value) = P::resolve_many(core::iter::once(first).chain(rest)) {
            tree.insert_with([id], value, resolve);
        }
        // resolve_many が読み残した同じ親の値を、次の親として扱わない
        while drained.next_if(|(next, _)| *next == id).is_some() {}
    }
    ready.extend(tree);
    ready.reverse();
    ready.pop()
}
