use alloc::vec::Vec;
use std::path::PathBuf;

use crate::collection::disk::core::FlexTreeCore;
use crate::collection::disk::core::node::Node;
use crate::collection::mem::core::FlexTreeCore as MemFlexTreeCore;
use crate::{FlexId, Side, SingleId};

struct TempFile(PathBuf);

impl TempFile {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("flextree-core-disk-{name}.db"));
        let _ = std::fs::remove_file(&path);
        TempFile(path)
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

async fn entries(tree: &FlexTreeCore) -> Vec<(FlexId, u64)> {
    let mut out = Vec::new();
    let mut stack: Vec<_> = tree
        .roots
        .iter()
        .cloned()
        .zip(FlexId::ROOTS)
        .filter_map(|(node, root)| node.map(|n| (n, root)))
        .rev()
        .collect();
    while let Some((node, this)) = stack.pop() {
        match node {
            Node::Value(value) => out.push((this, value)),
            Node::OnDisk { page, .. } => stack.push((tree.read(page).await.unwrap(), this)),
            Node::Branch(branch) => {
                let half = |side| this.split_on(branch.dimension, side).unwrap();
                stack.push((branch.upper.clone(), half(Side::Upper)));
                stack.push((branch.lower.clone(), half(Side::Lower)));
            }
            Node::LongSkip(skip) => {
                stack.push((skip.child.clone(), skip.path.to_absolute(&this).unwrap()))
            }
        }
    }
    out
}

#[compio::test]
async fn single_insert_round_trip() {
    let file = TempFile::new("single");
    let id = SingleId::new(13, 4, 4, 4).unwrap();
    let flex = id.flex_ids().next().unwrap();
    {
        let mut tree = FlexTreeCore::open(&file.0).await.unwrap();
        tree.insert(id.flex_ids(), 42).await.unwrap();
    }

    let tree = FlexTreeCore::open(&file.0).await.unwrap();
    let val = tree.get(&flex).await.unwrap();
    assert_eq!(val, Some(42));
}

#[compio::test]
async fn multiple_inserts_and_overwrites() {
    let file = TempFile::new("multi");
    let id1 = SingleId::new(5, 1, 2, 3).unwrap();
    let id2 = SingleId::new(5, 1, 2, 4).unwrap();
    let flex1 = id1.flex_ids().next().unwrap();
    let flex2 = id2.flex_ids().next().unwrap();

    let mut tree = FlexTreeCore::open(&file.0).await.unwrap();
    tree.insert(id1.flex_ids(), 100).await.unwrap();
    tree.insert(id2.flex_ids(), 200).await.unwrap();

    assert_eq!(tree.get(&flex1).await.unwrap(), Some(100));
    assert_eq!(tree.get(&flex2).await.unwrap(), Some(200));

    // 上書きテスト
    tree.insert(id1.flex_ids(), 999).await.unwrap();
    assert_eq!(tree.get(&flex1).await.unwrap(), Some(999));
}

#[compio::test]
async fn matches_in_memory_tree() {
    let file = TempFile::new("mem_match");
    let mut disk_tree = FlexTreeCore::open(&file.0).await.unwrap();
    let mut mem_tree = MemFlexTreeCore::<u64>::default();

    let ids = [
        (SingleId::new(4, 1, 1, 1).unwrap(), 10u64),
        (SingleId::new(4, 1, 2, 1).unwrap(), 20u64),
        (SingleId::new(4, 2, 1, 1).unwrap(), 30u64),
        (SingleId::new(4, 2, 2, 2).unwrap(), 40u64),
    ];

    for (id, val) in &ids {
        disk_tree.insert(id.flex_ids(), *val).await.unwrap();
        mem_tree.insert(id.flex_ids(), *val);
    }

    let disk_entries = entries(&disk_tree).await;
    let mem_entries: Vec<(FlexId, u64)> = mem_tree.iter().map(|(id, v)| (id, *v)).collect();
    assert_eq!(disk_entries, mem_entries);
}

#[compio::test]
async fn checkerboard_multi_pages() {
    let file = TempFile::new("checkerboard");
    let (tree, expected) = checkerboard(&file, usize::MAX).await;
    assert_eq!(entries(&tree).await, expected);

    drop(tree);
    let reloaded = FlexTreeCore::open(&file.0).await.unwrap();
    assert_eq!(entries(&reloaded).await, expected);
}

/// 市松模様の先頭 `count` 個を書き込んだ木と、その中身。
async fn checkerboard(file: &TempFile, count: usize) -> (FlexTreeCore, Vec<(FlexId, u64)>) {
    let mut disk_tree = FlexTreeCore::open(&file.0).await.unwrap();
    let mut mem_tree = MemFlexTreeCore::<u64>::default();
    let ids = (0..16u32)
        .flat_map(|x| (0..16u32).map(move |y| (x, y)))
        .filter(|(x, y)| (x + y) % 2 == 0)
        .map(|(x, y)| SingleId::new(4, 3, x, y).unwrap())
        .take(count);
    for (i, id) in ids.enumerate() {
        disk_tree.insert(id.flex_ids(), i as u64).await.unwrap();
        mem_tree.insert(id.flex_ids(), i as u64);
    }
    let expected = mem_tree.iter().map(|(id, v)| (id, *v)).collect();
    (disk_tree, expected)
}

#[compio::test]
async fn compact_keeps_entries_and_shrinks_file() {
    let file = TempFile::new("compact");
    let (tree, expected) = checkerboard(&file, 128).await;
    let before = std::fs::metadata(&file.0).unwrap().len();

    let tree = tree.compact().await.unwrap();
    let after = std::fs::metadata(&file.0).unwrap().len();
    assert!(after < before, "{after} < {before}");
    assert_eq!(entries(&tree).await, expected);

    drop(tree);
    let reopened = FlexTreeCore::open(&file.0).await.unwrap();
    assert_eq!(entries(&reopened).await, expected);
}

#[compio::test]
async fn open_ignores_uncommitted_tail() {
    let file = TempFile::new("torn");
    let (tree, expected) = checkerboard(&file, 32).await;
    drop(tree);

    // コミットの途中で止まり、ヘッダーを書く前のページと書きかけのページが残った状態
    let mut tail = std::fs::OpenOptions::new()
        .append(true)
        .open(&file.0)
        .unwrap();
    std::io::Write::write_all(&mut tail, &[0xAB; 4096 * 2 + 100]).unwrap();
    drop(tail);

    let mut tree = FlexTreeCore::open(&file.0).await.unwrap();
    assert_eq!(entries(&tree).await, expected);

    // 捨てた末尾の後ろに続けて書ける
    let id = SingleId::new(4, 3, 1, 0).unwrap();
    tree.insert(id.flex_ids(), 7).await.unwrap();
    drop(tree);
    let tree = FlexTreeCore::open(&file.0).await.unwrap();
    let flex = id.flex_ids().next().unwrap();
    assert_eq!(tree.get(&flex).await.unwrap(), Some(7));
}
