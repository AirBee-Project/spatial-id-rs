use super::super::node::NodeKind;
use super::super::node_page::NodePage;
use super::super::tests::{Lcg, block_on};
use super::*;
use crate::collection::disk::io::PageId;
use crate::{DiskError, SpatialIdMap};
use alloc::collections::BTreeMap;
use core::cell::Cell;
use zerocopy::FromZeros;

/// ページを読まない前提のダミーローダー。
async fn no_page(_: PageId, _: FlexId) -> Result<Node, Error> {
    panic!("ページは読まれない")
}

/// ランダムな操作。`remove` なら削除も混ぜる。
fn random_op(rng: &mut Lcg, remove: bool) -> Op {
    let value = u64::from(rng.below(30));
    match rng.below(if remove { 3 } else { 2 }) {
        0 => Op::Insert(value),
        1 => Op::Upsert(value),
        _ => Op::Remove,
    }
}

/// 上下のルートの Node。
struct Roots([Option<Node>; 2]);

impl Roots {
    fn edit(&mut self, target: FlexId, op: Op) {
        let i = target.root_index();
        self.0[i] = block_on(edit(
            self.0[i].take(),
            &FlexId::ROOTS[i],
            &[target],
            op,
            &no_page,
        ))
        .unwrap();
    }

    /// 中身を、比べられるよう SpatialIdMap に集めたもの。
    fn collect(&self) -> SpatialIdMap<u64> {
        let mut map = SpatialIdMap::new();
        for (node, region) in self.0.iter().zip(FlexId::ROOTS) {
            if let Some(node) = node {
                collect_node(node, region, &mut map);
            }
        }
        map
    }
}

fn collect_node(node: &Node, this: FlexId, map: &mut SpatialIdMap<u64>) {
    if let Node::Value(value) = node {
        map.insert(this, *value);
    }
    for (child, region) in node.children(&this).unwrap() {
        collect_node(child, region, map);
    }
}

/// 複数ページにまたがる Node の中身を SpatialIdMap に集める。
fn collect_paged(
    node: &Node,
    this: FlexId,
    pages: &BTreeMap<PageId, Box<NodePage>>,
    map: &mut SpatialIdMap<u64>,
) {
    match node {
        Node::Value(value) => map.insert(this, *value),
        Node::RefPage { page_id, .. } => {
            let child = pages[page_id].decode(&this).unwrap();
            collect_paged(&child, this, pages, map);
        }
        Node::Branch { .. } | Node::Skip { .. } => {
            for (child, region) in node.children(&this).unwrap() {
                collect_paged(child, region, pages, map);
            }
        }
    }
}

/// SpatialIdMap に同じ操作を施す。
fn apply(map: &mut SpatialIdMap<u64>, target: FlexId, op: Op) {
    match op {
        Op::Insert(value) => map.insert(target, value),
        Op::Upsert(value) => map.upsert(target, value),
        Op::Remove => {
            let _ = map.remove_flex_ids(target);
        }
    }
}

/// 施した結果が、SpatialIdMap に同じ順で施したときと一致する。
#[test]
fn edit_matches_mem_map() {
    let mut rng = Lcg(1);
    let mut roots = Roots([None, None]);
    let mut expected = SpatialIdMap::new();
    for i in 0..3000 {
        let target = rng.flex_id();
        // 前半は削除を混ぜずに Node を育て、後半で削除も混ぜる
        let op = random_op(&mut rng, i >= 1500);
        apply(&mut expected, target, op);
        roots.edit(target, op);
        if i % 300 == 0 {
            assert_eq!(roots.collect(), expected);
        }
    }
    assert_eq!(roots.collect(), expected);
}

/// 全体を削除すると、ルートは空になる。
#[test]
fn remove_all_empties_roots() {
    let mut rng = Lcg(3);
    let mut roots = Roots([None, None]);
    for _ in 0..3000 {
        let target = rng.flex_id();
        roots.edit(target, random_op(&mut rng, false));
    }
    roots.edit(FlexId::UPPER_MAX, Op::Remove);
    roots.edit(FlexId::LOWER_MAX, Op::Remove);
    assert!(roots.0.iter().all(Option::is_none));
}

/// 1 ページに収まらない Node は、どのページも収まるよう分けられ、ルートのページ番号が返る。
#[test]
fn node_page_split_fits_each_page() {
    let mut rng = Lcg(4);
    let mut roots = Roots([None, None]);
    for _ in 0..3000 {
        let target = rng.flex_id();
        roots.edit(target, random_op(&mut rng, false));
    }
    let [Some(root), _] = roots.0 else {
        panic!("上側のルートは空でない")
    };
    assert!(root.len() > NodePage::BODY_LEN);
    let mut next = 0;
    // 内部の encode が収まらない Node で panic するので、返れば各ページは収まっている
    let (root_id, node_pages) = NodePage::split(root, || {
        next += 1;
        PageId(next)
    });
    assert!(node_pages.len() > 1);
    assert_eq!(root_id, PageId(node_pages.len() as u64));
}

/// RefPage をオンデマンドに読み込みながら編集でき、不要な読み込みは起きない。
#[test]
fn edit_loads_ref_page_on_demand() {
    let mut rng = Lcg(5);
    let mut roots = Roots([None, None]);
    let mut expected = SpatialIdMap::new();
    for _ in 0..3000 {
        let target = rng.flex_id();
        let op = random_op(&mut rng, false);
        apply(&mut expected, target, op);
        roots.edit(target, op);
    }
    let [Some(upper), _] = roots.0 else {
        panic!("上側のルートは空でない")
    };
    let mut next = 0;
    let (root_id, split) = NodePage::split(upper, || {
        next += 1;
        PageId(next)
    });
    let pages: BTreeMap<PageId, Box<NodePage>> = split.into_iter().collect();
    let root = pages[&root_id].decode(&FlexId::UPPER_MAX).unwrap();

    // ページを跨ぐランダム編集が SpatialIdMap と一致する
    let reads = Cell::new(0usize);
    let mut edited = Some(root);
    for _ in 0..200 {
        let target = rng.flex_id();
        if target.root_index() != 0 {
            continue;
        }
        let op = random_op(&mut rng, true);
        apply(&mut expected, target, op);
        edited = block_on(edit(
            edited,
            &FlexId::UPPER_MAX,
            &[target],
            op,
            &async |page_id, this| {
                reads.set(reads.get() + 1);
                pages[&page_id].decode(&this)
            },
        ))
        .unwrap();
    }
    assert!(reads.get() > 0);
    let mut actual = SpatialIdMap::new();
    if let Some(node) = &edited {
        collect_paged(node, FlexId::UPPER_MAX, &pages, &mut actual);
    }
    let mut expected_upper = SpatialIdMap::new();
    for (id, value) in expected.get(FlexId::UPPER_MAX) {
        expected_upper.insert(id, *value);
    }
    assert_eq!(actual, expected_upper);

    // 領域全体への Insert / Remove はページを読まない
    let ref_value = Node::RefPage {
        page_id: PageId(1),
        kind: NodeKind::Value,
        split_dimensions: DimensionSet::EMPTY,
    };
    let over = block_on(edit(
        Some(ref_value.clone()),
        &FlexId::UPPER_MAX,
        &[FlexId::UPPER_MAX],
        Op::Insert(42),
        &no_page,
    ))
    .unwrap();
    assert_eq!(over, Some(Node::Value(42)));

    let removed = block_on(edit(
        Some(ref_value),
        &FlexId::UPPER_MAX,
        &[FlexId::UPPER_MAX],
        Op::Remove,
        &no_page,
    ))
    .unwrap();
    assert_eq!(removed, None);
}

/// 空の NodePage と、ルートが RefPage の NodePage は DiskError::Corrupt になる。
#[test]
fn corrupt_empty_node_page_and_ref_page_root() {
    let empty = NodePage::new_box_zeroed().unwrap();
    assert!(matches!(
        empty.decode(&FlexId::UPPER_MAX),
        Err(Error::Disk(DiskError::Corrupt))
    ));

    let ref_root = Node::RefPage {
        page_id: PageId(1),
        kind: NodeKind::Branch,
        split_dimensions: DimensionSet::EMPTY,
    };
    let (_, node_pages) = NodePage::split(ref_root, || PageId(1));
    assert!(matches!(
        node_pages[0].1.decode(&FlexId::UPPER_MAX),
        Err(Error::Disk(DiskError::Corrupt))
    ));
}
