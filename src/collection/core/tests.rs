use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Debug;
use triomphe::Arc;

use super::node::{Branch, Node, Skip, common_descent};
use super::FlexTreeCore;
use crate::id::relative_flex_id::ShortPath;
use crate::{FlexId, RangeId, Side, SingleId, id::dimension::Dimension};

/// Leaf と Branch の数。Skip は数えない。
fn node_count<V>(node: &Node<V>) -> usize {
    match node {
        Node::Empty | Node::Leaf(_) | Node::LeafAt(..) => 1,
        Node::Branch(branch) | Node::BranchAt(_, branch) => {
            1 + node_count(&branch.lower) + node_count(&branch.upper)
        }
        // Skip は位置を移すだけで、値も分岐も持たない
        Node::Skip(skip) => node_count(&skip.child),
    }
}

/// 領域 `this` の Node が FlexTreeCore の規則を守っているか検査する。
fn check_rules<V: PartialEq + Debug>(
    node: &Node<V>,
    this: FlexId,
) -> Result<(), String> {
    match node {
        Node::Empty | Node::Leaf(_) => Ok(()),
        Node::Branch(branch) => check_branch(branch, this),
        Node::LeafAt(path, _) => check_path(this, path.to_absolute(&this), 0),
        Node::BranchAt(path, branch) => {
            let region = path.to_absolute(&this);
            check_path(this, region, branch.split_dimensions)?;
            check_branch(branch, region)
        }
        Node::Skip(skip) => {
            let Skip {
                path,
                split_dimensions,
                child,
            } = &**skip;
            if !matches!(child, Node::Leaf(_) | Node::Branch(_)) {
                return Err(format!("{this:?}: Skip の先が Leaf でも Branch でもない"));
            }
            if ShortPath::new(*path).is_some() {
                return Err(format!("{this:?}: 埋め込める Skip を確保している"));
            }
            if *split_dimensions != (path.deeper_dimensions() | child.split_dimensions()) {
                return Err(format!("{this:?}: Skip の split_dimensions が不正"));
            }
            let region = path.to_absolute(&this).unwrap();
            check_path(this, region, child.split_dimensions())?;
            check_rules(child, region)
        }
    }
}

/// 領域 `this` の Branch が規則を守っているか検査する。
fn check_branch<V: PartialEq + Debug>(
    branch: &Branch<V>,
    this: FlexId,
) -> Result<(), String> {
    let Branch {
        dimension,
        split_dimensions,
        height,
        count,
        lower,
        upper,
    } = branch;
    let expected = dimension.bit() | lower.split_dimensions() | upper.split_dimensions();
    if *split_dimensions != expected {
        return Err(format!("{this:?}: split_dimensions のキャッシュが古い"));
    }
    if lower.is_empty() || upper.is_empty() {
        return Err(format!("{this:?}: 片側が空の Branch（Skip にすべき）"));
    }
    if lower.split_dimensions() & dimension.bit() == 0 && lower == upper {
        return Err(format!("{this:?}: 割る必要のない Branch"));
    }
    if this.coarsest_dimension_in(*split_dimensions) != Some(*dimension) {
        return Err(format!("{this:?}: 一番粗い次元で割っていない"));
    }
    let expected_height = 1 + lower.height().max(upper.height());
    if *height != expected_height {
        return Err(format!(
            "{this:?}: height が不正: 実際 {height}, 期待 {expected_height}"
        ));
    }
    let expected_count = lower.count() + upper.count();
    if *count != expected_count {
        return Err(format!(
            "{this:?}: count が不正: 実際 {count}, 期待 {expected_count}"
        ));
    }
    check_rules(lower, this.split_on(*dimension, Side::Lower).unwrap())?;
    check_rules(upper, this.split_on(*dimension, Side::Upper).unwrap())
}

/// 領域 `this` から `region` への Skip が規則を守っているか検査する。`split` は先の Node が割っている次元。
fn check_path(this: FlexId, region: FlexId, split: u8) -> Result<(), String> {
    if region == this {
        return Err(format!("{this:?}: 移動しない Skip"));
    }
    // 一本道の各段で、行き先が狭い次元から割っていること
    let mut at = this;
    while at != region {
        let finer = region.finer_dimensions_than(&at);
        let d = at.coarsest_dimension_in(finer | split).unwrap();
        if finer & d.bit() == 0 {
            return Err(format!("{at:?}: Skip の途中で中身の次元を先に割るべき"));
        }
        at = at.split_toward(d, &region).unwrap();
    }
    Ok(())
}

fn assert_rules<V: PartialEq + Debug>(tree: &FlexTreeCore<V>) {
    check_rules(&tree.upper_root, FlexId::UPPER_MAX).unwrap();
    check_rules(&tree.lower_root, FlexId::LOWER_MAX).unwrap();
}

/// FlexId と値の列から FlexTreeCore を組み立てる。
fn build<'a, V: Clone + Ord + 'a>(
    leaves: impl IntoIterator<Item = (FlexId, &'a V)>,
) -> FlexTreeCore<V> {
    let mut tree = FlexTreeCore::default();
    for (id, value) in leaves {
        tree.insert(id, value.clone());
    }
    tree
}

/// FlexTreeCore の中で `point` を含む FlexId の値。
fn value_at<V: Clone>(tree: &FlexTreeCore<V>, point: &FlexId) -> Option<V> {
    tree.iter()
        .find(|(id, _)| id.contains(point))
        .map(|(_, v)| v.clone())
}

/// 参照モデル：書き込みの履歴から `point` の値を求める（後の書き込みが勝つ。`None` は削除）。
fn model_value_at<V: Clone>(writes: &[(FlexId, Option<V>)], point: &FlexId) -> Option<V> {
    writes
        .iter()
        .rev()
        .find(|(id, _)| id.contains(point))
        .and_then(|(_, v)| v.clone())
}

/// 決定的な擬似乱数（線形合同法）。`next(n)` は `0..n` を返す。
fn rng(mut seed: u64) -> impl FnMut(u64) -> u64 {
    move |n| {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (seed >> 33) % n
    }
}

/// 各次元のズームが `0..max_zoom` のランダムな FlexId を作る。
fn random_id(next: &mut impl FnMut(u64) -> u64, max_zoom: u64) -> FlexId {
    let fz = next(max_zoom) as u8;
    let xz = next(max_zoom) as u8;
    let yz = next(max_zoom) as u8;
    let f = next(2 << fz) as i32 - (1 << fz);
    let x = next(1 << xz) as u32;
    let y = next(1 << yz) as u32;
    // 時間次元が無いビルドでは全時間（ズーム0）しか作れない
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

/// 標本点のズーム。[`random_id`] の `max_zoom` 以下の ID より必ず細かい。
const POINT_ZOOM: u8 = 20;

/// `region` の中のランダムな標本点（各次元のズーム [`POINT_ZOOM`]）。
fn random_point_in(next: &mut impl FnMut(u64) -> u64, region: &FlexId) -> FlexId {
    let mut deeper = |z: u8, i: i64| (i << (POINT_ZOOM - z)) + next(1 << (POINT_ZOOM - z)) as i64;
    let f = deeper(region.f_zoomlevel(), region.f_index() as i64);
    let x = deeper(region.x_zoomlevel(), region.x_index() as i64);
    let y = deeper(region.y_zoomlevel(), region.y_index() as i64);
    let (tz, t) = if cfg!(feature = "temporal_id") {
        (
            POINT_ZOOM,
            deeper(region.t_zoomlevel(), region.t_index() as i64),
        )
    } else {
        (0, 0)
    };
    FlexId::new(
        POINT_ZOOM, f as i32, POINT_ZOOM, x as u32, POINT_ZOOM, y as u32,
    )
    .unwrap()
    .with_time_segment(tz, t as u64)
    .unwrap()
}

/// 書き込んだ各 ID の内側と、全体からランダムに選んだ標本点。
fn sample_points(next: &mut impl FnMut(u64) -> u64, ids: &[FlexId]) -> Vec<FlexId> {
    let mut points = Vec::new();
    for id in ids {
        for _ in 0..2 {
            points.push(random_point_in(next, id));
        }
    }
    for _ in 0..20 {
        let root = if next(2) == 0 {
            FlexId::UPPER_MAX
        } else {
            FlexId::LOWER_MAX
        };
        points.push(random_point_in(next, &root));
    }
    points
}

/// Node は値か Arc 1つ分にタグを足した大きさで、Branch・Skip の中身の大きさに引きずられない。
#[test]
fn node_size_is_value_or_pointer() {
    use core::mem::size_of;
    assert!(size_of::<Node<u64>>() <= size_of::<u64>().max(size_of::<Arc<()>>()) + 8);
}

#[test]
fn f_zero_goes_to_upper_root() {
    let mut tree = FlexTreeCore::<u64>::default();
    tree.insert(FlexId::new(3, 0, 3, 0, 3, 0).unwrap(), 1u64);
    assert_eq!(tree.iter().count(), 1);
    assert!(tree.lower_root.is_empty());
}

#[test]
fn insert_whole_root_becomes_single_leaf() {
    let mut tree = FlexTreeCore::<u64>::default();
    tree.insert(FlexId::UPPER_MAX, 7u64);
    assert!(matches!(tree.upper_root, Node::Leaf(7)));
}

#[test]
fn sibling_halves_with_same_value_merge() {
    let mut tree = FlexTreeCore::<u64>::default();
    tree.insert(FlexId::new(1, 0, 0, 0, 0, 0).unwrap(), 5u64);
    tree.insert(FlexId::new(1, 1, 0, 0, 0, 0).unwrap(), 5u64);
    assert!(matches!(tree.upper_root, Node::Leaf(5)));
}

#[test]
fn overwrite_inside_filled_leaf_splits() {
    let mut tree = FlexTreeCore::<u64>::default();
    tree.insert(FlexId::UPPER_MAX, 1u64);
    tree.insert(FlexId::new(2, 1, 2, 3, 2, 0).unwrap(), 2u64);
    assert!(tree.iter().count() > 2);
    // 周りに値があるので段は飛ばさない
    assert!(matches!(tree.upper_root, Node::Branch(_)));

    tree.insert(FlexId::new(2, 1, 2, 3, 2, 0).unwrap(), 1u64);
    assert!(matches!(tree.upper_root, Node::Leaf(1)));
}

/// 空の FlexTreeCore へ細かい点を入れても、一本道の Branch を作らず Skip 1 つで降りる。
#[test]
fn deep_point_in_empty_tree_skips_levels() {
    let mut tree = FlexTreeCore::<u64>::default();
    tree.insert(FlexId::new(20, 12345, 20, 54321, 20, 999).unwrap(), 1u64);
    assert!(matches!(&tree.upper_root, Node::Skip(skip) if matches!(skip.child, Node::Leaf(1))));
}

/// 離れた2点は、分かれる地点の Branch 1 つの下に並ぶ。
#[test]
fn two_distant_points_share_one_fork() {
    let mut tree = FlexTreeCore::<u64>::default();
    tree.insert(FlexId::new(20, 1, 20, 1, 20, 1).unwrap(), 1u64);
    tree.insert(FlexId::new(20, 1, 20, 900_000, 20, 1).unwrap(), 2u64);
    // 分岐の Branch 1つと、各点の Leaf
    assert_eq!(node_count(&tree.upper_root), 3);
    assert_eq!(tree.iter().count(), 2);
    assert_rules(&tree);
}

/// 挿入した点を消すと空の FlexTreeCore に戻る。
#[test]
fn remove_restores_empty_tree() {
    let mut tree = FlexTreeCore::<u64>::default();
    let a = FlexId::new(20, 1, 20, 1, 20, 1).unwrap();
    let b = FlexId::new(20, 1, 20, 900_000, 20, 1).unwrap();
    tree.insert(a, 1u64);
    tree.insert(b, 2u64);
    tree.remove(a);
    tree.remove(b);
    assert_eq!(tree, FlexTreeCore::default());
}

/// 乱数で上書き挿入と削除を繰り返し、参照モデルと値が一致し、常に FlexTreeCore の規則を守っていることを確かめる。
/// さらに、別の分け方の FlexId（各 FlexId を半分に割ったもの・自分の FlexId を逆順）から組み直しても同じ形になることを確かめる。
/// 粗い ID が重なり合う場合と、細かい ID が疎に散る場合（段飛ばしが多い）の両方を試す。
#[test]
fn random_updates_match_model_and_keep_rules() {
    let mut next = rng(0x1234_5678_9abc_def0);
    for max_zoom in [5, 20] {
        for _ in 0..50 {
            let mut tree = FlexTreeCore::default();
            let mut writes: Vec<(FlexId, Option<u64>)> = Vec::new();

            for _ in 0..40 {
                let id = random_id(&mut next, max_zoom);
                if next(4) == 0 {
                    tree.remove(id);
                    writes.push((id, None));
                } else {
                    let value = next(3);
                    tree.insert(id, value);
                    writes.push((id, Some(value)));
                }
                assert_rules(&tree);
            }

            let ids: Vec<FlexId> = writes.iter().map(|(id, _)| *id).collect();
            for point in sample_points(&mut next, &ids) {
                assert_eq!(value_at(&tree, &point), model_value_at(&writes, &point));
            }

            let halves: Vec<(FlexId, &u64)> = tree
                .iter()
                .flat_map(|(id, v)| {
                    match Dimension::ALL
                        .into_iter()
                        .find(|&d| id.split_on(d, Side::Upper).is_some())
                    {
                        Some(d) => [Side::Upper, Side::Lower]
                            .map(|side| (id.split_on(d, side).unwrap(), v))
                            .to_vec(),
                        None => alloc::vec![(id, v)],
                    }
                })
                .collect();
            assert_eq!(build(halves), tree);

            let mut own_leaves: Vec<(FlexId, &u64)> = tree.iter().collect();
            own_leaves.reverse();
            assert_eq!(build(own_leaves), tree);
        }
    }
}

/// 集合演算が参照モデルと一致し、結果も FlexTreeCore の規則を守っていることを確かめる。
#[test]
fn set_operations_match_model() {
    let mut next = rng(7);
    for max_zoom in [5, 20] {
        for _ in 0..50 {
            let (mut a, mut b) = (FlexTreeCore::<()>::default(), FlexTreeCore::<()>::default());
            let (mut ids_a, mut ids_b) = (Vec::new(), Vec::new());
            for _ in 0..20 {
                let id = random_id(&mut next, max_zoom);
                a.insert(id, ());
                ids_a.push(id);
                let id = random_id(&mut next, max_zoom);
                b.insert(id, ());
                ids_b.push(id);
            }

            let (union, intersection, difference) =
                (a.union(&b), a.intersection(&b), a.difference(&b));
            for tree in [&union, &intersection, &difference] {
                assert_rules(tree);
            }

            let all_ids: Vec<FlexId> = ids_a.iter().chain(&ids_b).copied().collect();
            for point in sample_points(&mut next, &all_ids) {
                let in_a = ids_a.iter().any(|id| id.contains(&point));
                let in_b = ids_b.iter().any(|id| id.contains(&point));
                assert_eq!(value_at(&union, &point).is_some(), in_a || in_b);
                assert_eq!(value_at(&intersection, &point).is_some(), in_a && in_b);
                assert_eq!(value_at(&difference, &point).is_some(), in_a && !in_b);
            }
        }
    }
}

/// 参照実装：再帰でたどって、値を持つ領域を下側から順に集める。
fn collect_recursive<'a, V>(node: &'a Node<V>, this: FlexId, out: &mut Vec<(FlexId, &'a V)>) {
    match node {
        Node::Empty => {}
        Node::Leaf(value) => out.push((this, value)),
        Node::LeafAt(path, value) => out.push((path.to_absolute(&this), value)),
        Node::BranchAt(path, branch) => {
            let this = path.to_absolute(&this);
            let dimension = branch.dimension;
            collect_recursive(
                &branch.lower,
                this.split_on(dimension, Side::Lower).unwrap(),
                out,
            );
            collect_recursive(
                &branch.upper,
                this.split_on(dimension, Side::Upper).unwrap(),
                out,
            );
        }
        Node::Branch(branch) => {
            let dimension = branch.dimension;
            collect_recursive(
                &branch.lower,
                this.split_on(dimension, Side::Lower).unwrap(),
                out,
            );
            collect_recursive(
                &branch.upper,
                this.split_on(dimension, Side::Upper).unwrap(),
                out,
            );
        }
        Node::Skip(skip) => {
            collect_recursive(&skip.child, skip.path.to_absolute(&this).unwrap(), out);
        }
    }
}

/// `iter`・`&tree` と `tree` の `IntoIterator` が、再帰でたどった結果と同じ順・同じ中身を返し、
/// `FromIterator`・`Extend` で組み直すと元の FlexTreeCore に戻ることを確かめる。
#[test]
fn iterators_match_recursive_walk_and_round_trip() {
    let mut next = rng(0xfeed);
    for max_zoom in [5, 20] {
        for _ in 0..50 {
            let mut tree = FlexTreeCore::default();
            for _ in 0..40 {
                let id = random_id(&mut next, max_zoom);
                if next(4) == 0 {
                    tree.remove(id);
                } else {
                    tree.insert(id, next(3));
                }
            }

            let mut expected = Vec::new();
            collect_recursive(&tree.upper_root, FlexId::UPPER_MAX, &mut expected);
            collect_recursive(&tree.lower_root, FlexId::LOWER_MAX, &mut expected);

            assert_eq!(tree.iter().collect::<Vec<_>>(), expected);
            assert_eq!((&tree).into_iter().collect::<Vec<_>>(), expected);
            let owned: Vec<(FlexId, u64)> = expected.iter().map(|&(id, v)| (id, *v)).collect();

            // 他の FlexTreeCore と Node を共有している場合（値をクローンする経路）
            let shared = tree.clone();
            assert_eq!(shared.into_iter().collect::<Vec<_>>(), owned);

            let rebuilt: FlexTreeCore<u64> = owned.iter().copied().collect();
            assert_eq!(rebuilt, tree);
            let mut extended = FlexTreeCore::default();
            extended.extend(owned.iter().copied());
            assert_eq!(extended, tree);

            // どこにも共有されていない場合（値をムーブする経路）
            assert_eq!(tree.into_iter().collect::<Vec<_>>(), owned);
        }
    }
}

/// 空の FlexTreeCore のイテレーターは何も返さず、使い切った後も `None` を返し続ける。
#[test]
fn empty_tree_iterators_are_fused() {
    let tree: FlexTreeCore<u64> = FlexTreeCore::default();
    let mut iter = tree.iter();
    assert_eq!(iter.next(), None);
    assert_eq!(iter.next(), None);
    let mut owned = tree.into_iter();
    assert_eq!(owned.next(), None);
    assert_eq!(owned.next(), None);
}

/// 書き込みの履歴。`None` は削除。
type Writes<V> = Vec<(FlexId, Option<V>)>;

/// 乱数で上書き挿入と削除を繰り返した FlexTreeCore と、その書き込み履歴を作る。
fn random_tree<V: PartialEq + Clone>(
    next: &mut impl FnMut(u64) -> u64,
    max_zoom: u64,
    random_value: impl Fn(&mut dyn FnMut(u64) -> u64) -> V,
) -> (FlexTreeCore<V>, Writes<V>) {
    let mut tree = FlexTreeCore::default();
    let mut writes = Vec::new();
    for _ in 0..40 {
        let id = random_id(next, max_zoom);
        if next(4) == 0 {
            tree.remove(id);
            writes.push((id, None));
        } else {
            let value = random_value(next);
            tree.insert(id, value.clone());
            writes.push((id, Some(value)));
        }
    }
    (tree, writes)
}

/// `count` は値を持つ領域の数と一致し、イテレーターの `size_hint` は残りの数を正確に返す。
#[test]
fn count_and_size_hint_match_iteration() {
    let mut next = rng(99);
    for max_zoom in [5, 20] {
        for _ in 0..50 {
            let (tree, _) = random_tree::<u64>(&mut next, max_zoom, |n| n(3));
            let total = tree.iter().count();
            assert_eq!(tree.count(), total);
            assert_eq!(tree.is_empty(), total == 0);

            let mut iter = tree.iter();
            for remaining in (0..=total).rev() {
                assert_eq!(iter.size_hint(), (remaining, Some(remaining)));
                iter.next();
            }
            let owned = tree.clone().into_iter();
            assert_eq!(owned.size_hint(), (total, Some(total)));
        }
    }
}

/// `Ord` でない値も持て、FlexTreeCore どうしで集合演算ができる。
#[test]
fn unordered_values_support_set_operations() {
    let a_id = FlexId::new(1, 0, 0, 0, 0, 0).unwrap();
    let b_id = FlexId::new(1, 1, 0, 0, 0, 0).unwrap();
    let mut tree: FlexTreeCore<f64> = FlexTreeCore::default();
    tree.insert(a_id, 0.5);
    tree.insert(b_id, 1.5);
    assert_eq!(tree.count(), 2);

    let mut region: FlexTreeCore<u64> = FlexTreeCore::default();
    region.insert(a_id, 7);
    let kept: Vec<_> = tree.intersection(&region).into_iter().collect();
    assert_eq!(kept, [(a_id, 0.5)]);
}

/// 重ならないとは限らない、ランダムな FlexId の列。
fn random_targets(next: &mut impl FnMut(u64) -> u64, max_zoom: u64) -> Vec<FlexId> {
    let count = 1 + next(3);
    (0..count).map(|_| random_id(next, max_zoom)).collect()
}

/// FlexId の列が覆う領域。
fn region_of(ids: &[FlexId]) -> FlexTreeCore<()> {
    let mut region = FlexTreeCore::default();
    region.insert(ids.iter().copied(), ());
    region
}

/// 複数の領域への `insert` は、1つずつ `insert` した結果と同じになる。
#[test]
fn insert_many_ids_matches_inserting_one_by_one() {
    let mut next = rng(11);
    for max_zoom in [5, 20] {
        for _ in 0..50 {
            let (base, _) = random_tree::<u64>(&mut next, max_zoom, |n| n(3));
            let targets = random_targets(&mut next, max_zoom);

            let mut at_once = base.clone();
            at_once.insert(targets.iter().copied(), 9);
            let mut one_by_one = base.clone();
            for id in &targets {
                one_by_one.insert(*id, 9);
            }
            assert_rules(&at_once);
            assert_eq!(at_once, one_by_one);
        }
    }
}

/// `insert_with` は、既に値がある場所を `resolve(既存, 新しい値)` にし、空の場所には新しい値を置く。
#[test]
fn insert_with_matches_model() {
    let mut next = rng(12);
    for max_zoom in [5, 20] {
        for _ in 0..50 {
            let mut tree = FlexTreeCore::<u64>::default();
            let mut writes: Writes<u64> = Vec::new();
            for _ in 0..30 {
                let id = random_id(&mut next, max_zoom);
                if next(4) == 0 {
                    tree.remove(id);
                    writes.push((id, None));
                } else {
                    let value = 1 + next(3);
                    tree.insert_with(id, value, |old, new| old + new);
                    writes.push((id, Some(value)));
                }
                assert_rules(&tree);
            }

            let ids: Vec<FlexId> = writes.iter().map(|(id, _)| *id).collect();
            for point in sample_points(&mut next, &ids) {
                // 削除（None）で空に戻り、書き込みは既存の値に足される
                let expected = writes
                    .iter()
                    .filter(|(id, _)| id.contains(&point))
                    .fold(None, |current, (_, write)| {
                        write.map(|v| current.map_or(v, |c: u64| c + v))
                    });
                assert_eq!(value_at(&tree, &point), expected);
            }
        }
    }
}

/// `get` は、`target` との共通部分に切り取った FlexId を返す。組み直すと積集合と一致する。
#[test]
fn get_matches_intersection() {
    let mut next = rng(13);
    for max_zoom in [5, 20] {
        for _ in 0..50 {
            let (tree, _) = random_tree::<u64>(&mut next, max_zoom, |n| n(3));
            let targets = random_targets(&mut next, max_zoom);

            let got: FlexTreeCore<u64> = tree
                .get(targets.iter().copied())
                .map(|(id, v)| (id, *v))
                .collect();
            assert_eq!(got, tree.intersection(&region_of(&targets)));
            for (id, _) in tree.get(targets.iter().copied()) {
                assert!(targets.iter().any(|t| t.contains(&id)));
            }
        }
    }
}

/// `get_overlapping` は、重なる FlexId をそのまま、1回ずつ、`iter` と同じ順で返す。`RangeId` を渡しても同じ。
#[test]
fn get_overlapping_returns_whole_leaves_once() {
    let mut next = rng(14);
    for max_zoom in [5, 20] {
        for _ in 0..50 {
            let (tree, _) = random_tree::<u64>(&mut next, max_zoom, |n| n(3));

            let targets = random_targets(&mut next, max_zoom);
            let expected: Vec<_> = tree
                .iter()
                .filter(|(leaf, _)| targets.iter().any(|t| leaf.intersection(t).is_some()))
                .collect();
            let got: Vec<_> = tree.get_overlapping(targets.iter().copied()).collect();
            assert_eq!(got, expected);

            let z = next(max_zoom) as u8;
            let span = 1i64 << z;
            let f0 = next(2 << z) as i64 - span;
            let (x0, y0) = (next(1 << z), next(1 << z));
            let range = RangeId::new(
                z,
                [f0 as i32, (f0 + next(3) as i64).min(span - 1) as i32],
                [x0 as u32, (x0 + next(3)).min(span as u64 - 1) as u32],
                [y0 as u32, (y0 + next(3)).min(span as u64 - 1) as u32],
            )
            .unwrap();
            let expected: Vec<_> = tree
                .iter()
                .filter(|(leaf, _)| leaf.intersects_range(&range))
                .collect();
            let got: Vec<_> = tree.get_overlapping(range).collect();
            assert_eq!(got, expected);
        }
    }
}

/// `remove` は `target` の部分を取り除き、取り除いた部分（積集合）を返す。
#[test]
fn remove_returns_removed_part() {
    let mut next = rng(15);
    for max_zoom in [5, 20] {
        for _ in 0..50 {
            let (mut tree, _) = random_tree::<u64>(&mut next, max_zoom, |n| n(3));
            let before = tree.clone();
            let targets = random_targets(&mut next, max_zoom);
            let region = region_of(&targets);

            let removed = tree.remove(targets.iter().copied());
            assert_rules(&tree);
            assert_rules(&removed);
            assert_eq!(removed, before.intersection(&region));
            assert_eq!(tree, before.difference(&region));
        }
    }
}

/// `remove_overlapping` は、`target` と重なる FlexId を丸ごと取り除く。
#[test]
fn remove_overlapping_removes_whole_leaves() {
    let mut next = rng(16);
    for max_zoom in [5, 20] {
        for _ in 0..50 {
            let (mut tree, _) = random_tree::<u64>(&mut next, max_zoom, |n| n(3));
            let before = tree.clone();
            let targets = random_targets(&mut next, max_zoom);
            let overlaps = |leaf: &FlexId| targets.iter().any(|t| leaf.intersection(t).is_some());

            let removed = tree.remove_overlapping(targets.iter().copied());
            assert_rules(&tree);
            assert_rules(&removed);
            assert_eq!(removed.union(&tree), before);
            assert!(removed.iter().all(|(leaf, _)| overlaps(&leaf)));
            assert!(tree.iter().all(|(leaf, _)| !overlaps(&leaf)));
        }
    }
}

/// `spatial_face_neighbors` は、面で接する領域だけを返し、自身や角で接する領域は返さない。
#[test]
fn spatial_face_neighbors_returns_face_neighbors_only() {
    let at = |f, x, y| SingleId::new(4, f, x, y).unwrap();
    let center = at(3, 5, 6);

    let mut tree = FlexTreeCore::<u64>::default();
    tree.insert(center, 0);
    for (value, cell) in [
        (1, at(2, 5, 6)),
        (2, at(4, 5, 6)),
        (3, at(3, 4, 6)),
        (4, at(3, 6, 6)),
        (5, at(3, 5, 5)),
        (6, at(3, 5, 7)),
        (7, at(3, 6, 7)),
    ] {
        tree.insert(cell, value);
    }

    let mut values: Vec<u64> = tree
        .spatial_face_neighbors(center)
        .map(|(_, v)| *v)
        .collect();
    values.sort();
    assert_eq!(values, [1, 2, 3, 4, 5, 6]);
}

/// `clear` で空になる。
#[test]
fn clear_empties_tree() {
    let mut tree = FlexTreeCore::<u64>::default();
    tree.insert(FlexId::UPPER_MAX, 1);
    tree.clear();
    assert!(tree.is_empty());
    assert_eq!(tree, FlexTreeCore::default());
}

/// Node を辿るスタックは、確保した大きさ（高さ + 2）を超えない。超えると走査の途中で再確保が起きる。
#[test]
fn traversal_stack_fits_height_bound() {
    let mut next = rng(17);
    for max_zoom in [5, 20] {
        for _ in 0..50 {
            let (tree, _) = random_tree::<u64>(&mut next, max_zoom, |n| n(3));
            let bound = usize::from(tree.upper_root.height().max(tree.lower_root.height())) + 2;

            let mut stack = Vec::from([
                (&tree.lower_root, FlexId::LOWER_MAX),
                (&tree.upper_root, FlexId::UPPER_MAX),
            ]);
            let mut deepest = stack.len();
            while let Some((node, this)) = stack.pop() {
                stack.extend(node.children(this).rev());
                deepest = deepest.max(stack.len());
            }
            assert!(
                deepest <= bound,
                "スタック {deepest} が上限 {bound} を超えた"
            );
        }
    }
}

#[derive(Debug, Clone)]
struct Counted(u64);

static COUNTED_EQ_CALLS: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);

impl PartialEq for Counted {
    fn eq(&self, other: &Self) -> bool {
        COUNTED_EQ_CALLS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        self.0 == other.0
    }
}

impl Eq for Counted {}

/// 共有している部分木は中身を比べない。1か所だけ変えた木との比較でも、値を比べるのは変えた経路の上だけ。
#[test]
fn eq_skips_shared_subtrees() {
    use core::sync::atomic::Ordering::Relaxed;

    let mut next = rng(19);
    let mut tree = FlexTreeCore::<Counted>::default();
    for i in 0..1000 {
        tree.insert(random_id(&mut next, 20), Counted(i % 5));
    }
    let height = usize::from(tree.upper_root.height().max(tree.lower_root.height()));

    let calls = COUNTED_EQ_CALLS.load(Relaxed);
    assert!(tree == tree.clone());
    assert_eq!(COUNTED_EQ_CALLS.load(Relaxed) - calls, 0);

    let mut changed = tree.clone();
    changed.insert(random_id(&mut next, 20), Counted(99));
    let calls = COUNTED_EQ_CALLS.load(Relaxed);
    assert!(tree != changed);
    let compared = COUNTED_EQ_CALLS.load(Relaxed) - calls;
    assert!(
        compared <= height + 2,
        "値を {compared} 回比べた（高さ {height}、Leaf {}）",
        tree.count()
    );
}

/// 書き込みは Node をその場で書き換えることがあるが、clone した FlexTreeCore には影響しない。
#[test]
fn writes_do_not_change_clones() {
    let mut next = rng(20);
    for max_zoom in [5, 20] {
        for _ in 0..50 {
            let (mut tree, _) = random_tree::<u64>(&mut next, max_zoom, |n| n(3));
            let snapshot = tree.clone();
            let before: Vec<(FlexId, u64)> = snapshot.iter().map(|(id, v)| (id, *v)).collect();

            for _ in 0..10 {
                let id = random_id(&mut next, max_zoom);
                match next(3) {
                    0 => tree.insert(id, 9),
                    1 => tree.insert_with(id, 9, |old, new| old + new),
                    _ => {
                        tree.remove(id);
                    }
                }
            }
            assert_rules(&snapshot);
            assert_eq!(
                snapshot.iter().map(|(id, v)| (id, *v)).collect::<Vec<_>>(),
                before
            );
        }
    }
}

/// `common_descent` を1段ずつ降りて求める素朴な版。行き先 `a` の外へは降りない。
fn common_descent_stepwise(this: &FlexId, a: (&FlexId, u8), b: (&FlexId, u8)) -> FlexId {
    let mut zoom = Dimension::ALL.map(|d| this.zoomlevel_on(d));
    let head = |zoom: &[u8; 4], region: &FlexId, child_split: u8| {
        let finer = Dimension::mask(|d| region.zoomlevel_on(d) > zoom[d as usize]);
        (finer != 0).then_some(())?;
        Dimension::ALL
            .into_iter()
            .filter(|&d| (finer | child_split) & d.bit() != 0)
            .min_by_key(|&d| (zoom[d as usize], d as u8))
    };
    while let (Some(d), Some(db)) = (head(&zoom, a.0, a.1), head(&zoom, b.0, b.1))
        && d == db
    {
        let next = zoom[d as usize] + 1;
        let (za, zb) = (a.0.zoomlevel_on(d), b.0.zoomlevel_on(d));
        if za < next
            || zb < next
            || a.0.index_on(d) >> (za - next) != b.0.index_on(d) >> (zb - next)
        {
            break;
        }
        zoom[d as usize] = next;
    }
    a.0.ancestor_at(zoom)
}

/// `common_descent` は、1段ずつ降りた場合と同じ領域で止まる。
#[test]
fn common_descent_matches_stepwise_descent() {
    let mut next = rng(31);
    let coarser = |next: &mut dyn FnMut(u64) -> u64, point: &FlexId, this: &FlexId| {
        let zoom = Dimension::ALL.map(|d| {
            let (lo, hi) = (this.zoomlevel_on(d), point.zoomlevel_on(d));
            lo + next(u64::from(hi - lo) + 1) as u8
        });
        point.ancestor_at(zoom)
    };
    for max_zoom in [3, 8, 20] {
        for _ in 0..20_000 {
            let this = random_id(&mut next, max_zoom);
            let pa = random_point_in(&mut next, &this);
            // 近い2点を作り、行き先が深いところまで重なる場合も十分に出す
            let near = Dimension::ALL.map(|d| {
                let up = next(4) as u8;
                this.zoomlevel_on(d)
                    .max(pa.zoomlevel_on(d).saturating_sub(up))
            });
            let around = if next(2) == 0 {
                pa.ancestor_at(near)
            } else {
                this
            };
            let pb = random_point_in(&mut next, &around);
            let region_a = coarser(&mut next, &pa, &this);
            let region_b = coarser(&mut next, &pb, &this);
            let (split_a, split_b) = (next(16) as u8, next(16) as u8);
            let expected =
                common_descent_stepwise(&this, (&region_a, split_a), (&region_b, split_b));
            let got = common_descent(&this, (&region_a, split_a), (&region_b, split_b));
            assert_eq!(
                got, expected,
                "this={this} a={region_a}/{split_a:04b} b={region_b}/{split_b:04b}"
            );
        }
    }
}

/// 1つの `target` への `get` は、全ての FlexId を `iter` の順に切り取ったものと一致する。
#[test]
fn get_single_target_matches_clipped_iteration() {
    let mut next = rng(41);
    for max_zoom in [3, 5, 20] {
        for _ in 0..300 {
            let (tree, _) = random_tree::<u64>(&mut next, max_zoom, |n| n(3));
            let target = random_id(&mut next, max_zoom);
            let expected: Vec<_> = tree
                .iter()
                .filter_map(|(id, v)| Some((id.intersection(&target)?, v)))
                .collect();
            let got: Vec<_> = tree.get(target).collect();
            assert_eq!(got, expected, "target {target}");
        }
    }
}
