use super::node::{EncodedValue, Node, NodeKind};
use super::node_page::NodePage;
use super::{FlexTreeCore, MetaPage};
use crate::collection::disk::io::{Page, PageId, PagedFile};
use crate::collection::disk::{DiskValue, MAX_VALUE_LEN};
use crate::id::dimension::{Dimension, DimensionSet};
use crate::{DiskError, Error, FlexId, RangeId};
use compio::buf::{IoBuf, IoBufMut};
use core::fmt::Debug;
use core::mem::MaybeUninit;
use rand::RngExt;
use rand_chacha::{ChaCha8Rng, rand_core::SeedableRng};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use zerocopy::FromBytes;

type Tree = FlexTreeCore<u64>;

/// テストごとに別の一時ファイル。drop で消す。
struct TempFile(PathBuf);

impl TempFile {
    fn new() -> Self {
        static COUNT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "spatial-id-disk-{}-{}",
            std::process::id(),
            COUNT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_file(&path);
        TempFile(path)
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn block_on<F: Future>(future: F) -> F::Output {
    compio::runtime::Runtime::new().unwrap().block_on(future)
}

fn is_corrupt<T>(result: Result<T, Error>) -> bool {
    matches!(result, Err(Error::Disk(DiskError::Corrupt)))
}

fn write_at(path: &Path, offset: u64, bytes: &[u8]) {
    let mut file = fs::OpenOptions::new().write(true).open(path).unwrap();
    file.seek(SeekFrom::Start(offset)).unwrap();
    file.write_all(bytes).unwrap();
}

fn flip_byte(path: &Path, offset: u64) {
    let mut file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .unwrap();
    let mut byte = [0];
    file.seek(SeekFrom::Start(offset)).unwrap();
    file.read_exact(&mut byte).unwrap();
    file.seek(SeekFrom::Start(offset)).unwrap();
    file.write_all(&[!byte[0]]).unwrap();
}

fn page_offset(id: PageId) -> u64 {
    id.0 * Page::SIZE as u64
}

const T_ZOOM: u8 = if cfg!(feature = "temporal_id") { 2 } else { 0 };
const T_MAX_ZOOM: u8 = if cfg!(feature = "temporal_id") { 35 } else { 0 };

/// 乱択テストで照合に使う最小単位。操作はこれより粗いズームでしか行わないので、どの操作の領域にも丸ごと入るか外れる。
fn cell(f: i32, x: u32, y: u32, t: u64) -> FlexId {
    FlexId::new(3, f, 3, x, 3, y)
        .unwrap()
        .time_with_segment(T_ZOOM, t)
        .unwrap()
}

fn grid() -> impl Iterator<Item = FlexId> {
    (-8..8).flat_map(|f| {
        (0..8).flat_map(move |x| {
            (0..8).flat_map(move |y| (0..1u64 << T_ZOOM).map(move |t| cell(f, x, y, t)))
        })
    })
}

/// 各次元のズームが [`cell`] 以下の、ランダムな FlexId。F の正負どちらのルートにも入る。
fn random_target(rng: &mut ChaCha8Rng) -> FlexId {
    let fz = rng.random_range(0..=3u8);
    let xz = rng.random_range(0..=3u8);
    let yz = rng.random_range(0..=3u8);
    let tz = rng.random_range(0..=T_ZOOM);
    FlexId::new(
        fz,
        rng.random_range(-(1 << fz)..(1 << fz)),
        xz,
        rng.random_range(0..(1 << xz)),
        yz,
        rng.random_range(0..(1 << yz)),
    )
    .unwrap()
    .time_with_segment(tz, rng.random_range(0..(1u64 << tz)))
    .unwrap()
}

/// 全セルを1つずつ get し、期待する値と一致するか確かめる。
async fn assert_cells<V: DiskValue + Clone + PartialEq + Debug>(
    tree: &FlexTreeCore<V>,
    expected: &HashMap<FlexId, V>,
) {
    for cell in grid() {
        let want: Vec<_> = expected
            .get(&cell)
            .map(|v| (cell, v.clone()))
            .into_iter()
            .collect();
        assert_eq!(tree.get(cell).await.unwrap(), want, "cell {cell:?}");
    }
}

/// ルート全体を get し、返った領域が重ならず、各セルの値が期待どおりか確かめる。
async fn assert_roots<V: DiskValue + PartialEq + Debug>(
    tree: &FlexTreeCore<V>,
    expected: &HashMap<FlexId, V>,
) {
    for root in FlexId::ROOTS {
        let pieces = tree.get(root).await.unwrap();
        assert!(pieces.iter().all(|(piece, _)| root.contains(piece)));
        for cell in grid().filter(|cell| root.contains(cell)) {
            let found: Vec<_> = pieces
                .iter()
                .filter(|(piece, _)| piece.contains(&cell))
                .map(|(_, v)| v)
                .collect();
            let want: Vec<_> = expected.get(&cell).into_iter().collect();
            assert_eq!(found, want, "cell {cell:?}");
        }
    }
}

/// RefPage を、`pages` にある行き先の Node で置き換える。RefPage が写した分割次元が行き先と一致することも確かめる。
fn resolve(node: Node, pages: &HashMap<u64, Node>) -> Node {
    match node {
        Node::RefPage {
            page_id,
            split_dimensions,
        } => {
            let target = resolve(pages[&page_id.0].clone(), pages);
            assert_eq!(split_dimensions, target.split_dimensions());
            target
        }
        Node::Branch {
            dimension,
            split_dimensions,
            lower,
            upper,
        } => Node::Branch {
            dimension,
            split_dimensions,
            lower: Box::new(resolve(*lower, pages)),
            upper: Box::new(resolve(*upper, pages)),
        },
        Node::Skip { path, child } => Node::Skip {
            path,
            child: Box::new(resolve(*child, pages)),
        },
        value @ Node::Value(_) => value,
    }
}

fn has_ref_page(node: &Node) -> bool {
    match node {
        Node::RefPage { .. } => true,
        Node::Branch { lower, upper, .. } => has_ref_page(lower) || has_ref_page(upper),
        Node::Skip { child, .. } => has_ref_page(child),
        Node::Value(_) => false,
    }
}

fn value_node(value: u64) -> Node {
    Node::Value(EncodedValue::from_bytes(&value.to_be_bytes()))
}

/// `node` を NodePage に分けて書き出し、読み戻して組み立て直す。
fn split_and_restore(node: &Node) -> (Node, usize) {
    let mut next = 100;
    let (root_id, node_pages) = NodePage::split(node.clone(), || {
        next += 1;
        PageId(next)
    });
    let page_count = node_pages.len();
    let pages: HashMap<u64, Node> = node_pages
        .into_iter()
        .map(|(id, page)| (id.0, page.decode().unwrap()))
        .collect();
    (resolve(pages[&root_id.0].clone(), &pages), page_count)
}

/// 深さ `depth` の、値がすべて異なる完全二分木。
fn balanced(depth: usize, next: &mut u64) -> Node {
    if depth == 0 {
        *next += 1;
        return value_node(*next);
    }
    let dimension = Dimension::ALL[depth % Dimension::ALL.len()];
    let lower = balanced(depth - 1, next);
    let upper = balanced(depth - 1, next);
    Node::Branch {
        dimension,
        split_dimensions: DimensionSet::from(dimension)
            | lower.split_dimensions()
            | upper.split_dimensions(),
        lower: Box::new(lower),
        upper: Box::new(upper),
    }
}

/// 新しいファイルは空として開け、世代 0 のメタページだけが書かれる。
#[test]
fn opens_new_file_as_empty() {
    let file = TempFile::new();
    block_on(async {
        let tree = Tree::open(&file.0).await.unwrap();
        assert_eq!(tree.generation, 0);
        assert_eq!(tree.roots, [None, None]);
        tree.close().await.unwrap();

        let tree = Tree::open(&file.0).await.unwrap();
        assert_eq!(tree.generation, 0);
        assert!(tree.get(FlexId::ROOTS[0]).await.unwrap().is_empty());
        tree.close().await.unwrap();
    });
}

/// 一度もコミットせずに閉じたファイルは、空として開き直せる。
#[test]
fn discards_changes_before_first_commit() {
    let file = TempFile::new();
    let id = FlexId::new(3, 1, 3, 2, 3, 4).unwrap();
    block_on(async {
        let mut tree = Tree::open(&file.0).await.unwrap();
        tree.insert(id, 7).await.unwrap();
        tree.close().await.unwrap();

        let tree = Tree::open(&file.0).await.unwrap();
        assert!(tree.get(id).await.unwrap().is_empty());
        tree.close().await.unwrap();
    });
}

/// コミット後の変更は、コミットせずに閉じると捨てられ、直前のコミットに戻る。
#[test]
fn discards_changes_after_last_commit() {
    let file = TempFile::new();
    let a = FlexId::new(3, 1, 3, 2, 3, 4).unwrap();
    let b = FlexId::new(3, -2, 3, 5, 3, 6).unwrap();
    block_on(async {
        let mut tree = Tree::open(&file.0).await.unwrap();
        tree.insert(a, 1).await.unwrap();
        tree.commit().await.unwrap();
        tree.insert(b, 2).await.unwrap();
        tree.remove(a).await.unwrap();
        tree.close().await.unwrap();

        let tree = Tree::open(&file.0).await.unwrap();
        assert_eq!(tree.get(a).await.unwrap(), [(a, 1)]);
        assert!(tree.get(b).await.unwrap().is_empty());
        tree.close().await.unwrap();
    });
}

/// 何度コミットしても世代が増え、開き直すと最後のコミットの内容が読める。
#[test]
fn reopens_latest_of_repeated_commits() {
    let file = TempFile::new();
    let id = FlexId::new(3, 1, 3, 2, 3, 4).unwrap();
    block_on(async {
        let mut tree = Tree::open(&file.0).await.unwrap();
        for value in 1..=5 {
            tree.insert(id, value).await.unwrap();
            tree.commit().await.unwrap();
        }
        // 変更のないコミットも世代を進める
        tree.commit().await.unwrap();
        tree.close().await.unwrap();

        let tree = Tree::open(&file.0).await.unwrap();
        assert_eq!(tree.generation, 6);
        assert_eq!(tree.get(id).await.unwrap(), [(id, 5)]);
        tree.close().await.unwrap();
    });
}

/// 最新のメタページが壊れていれば、もう一方のスロットにある1つ前のコミットに戻る。
#[test]
fn falls_back_when_latest_meta_is_torn() {
    let file = TempFile::new();
    let a = FlexId::new(3, 1, 3, 2, 3, 4).unwrap();
    let b = FlexId::new(3, -2, 3, 5, 3, 6).unwrap();
    block_on(async {
        let mut tree = Tree::open(&file.0).await.unwrap();
        tree.insert(a, 1).await.unwrap();
        tree.commit().await.unwrap();
        tree.insert(b, 2).await.unwrap();
        tree.commit().await.unwrap();
        let latest = MetaPage::SLOTS[(tree.generation % 2) as usize];
        tree.close().await.unwrap();

        flip_byte(&file.0, page_offset(latest) + 3);

        let tree = Tree::open(&file.0).await.unwrap();
        assert_eq!(tree.generation, 1);
        assert_eq!(tree.get(a).await.unwrap(), [(a, 1)]);
        assert!(tree.get(b).await.unwrap().is_empty());
        tree.close().await.unwrap();
    });
}

/// Node のページがあるのにメタページが両方壊れていれば、開けない。
#[test]
fn rejects_file_without_valid_meta() {
    let file = TempFile::new();
    block_on(async {
        let mut tree = Tree::open(&file.0).await.unwrap();
        tree.insert(FlexId::new(3, 1, 3, 2, 3, 4).unwrap(), 1)
            .await
            .unwrap();
        tree.commit().await.unwrap();
        tree.close().await.unwrap();

        for slot in MetaPage::SLOTS {
            flip_byte(&file.0, page_offset(slot));
        }
        assert!(is_corrupt(Tree::open(&file.0).await));
    });
}

/// メタページの場所までしかない、ゼロ埋めのファイル（初期化の途中で落ちたもの）は空として開け、そのまま使える。
#[test]
fn opens_zero_filled_meta_area_as_empty() {
    let id = FlexId::new(3, 1, 3, 2, 3, 4).unwrap();
    for pages in 1..=MetaPage::SLOTS.len() {
        let file = TempFile::new();
        fs::write(&file.0, vec![0; pages * Page::SIZE]).unwrap();
        block_on(async {
            let mut tree = Tree::open(&file.0).await.unwrap();
            assert_eq!(tree.roots, [None, None]);
            tree.insert(id, 1).await.unwrap();
            tree.commit().await.unwrap();
            tree.close().await.unwrap();

            let tree = Tree::open(&file.0).await.unwrap();
            assert_eq!(tree.get(id).await.unwrap(), [(id, 1)]);
            tree.close().await.unwrap();
        });
    }
}

/// 末尾の半端なページは無視され、続けて書き足せる。
#[test]
fn ignores_trailing_partial_page() {
    let file = TempFile::new();
    let a = FlexId::new(3, 1, 3, 2, 3, 4).unwrap();
    let b = FlexId::new(3, -2, 3, 5, 3, 6).unwrap();
    block_on(async {
        let mut tree = Tree::open(&file.0).await.unwrap();
        tree.insert(a, 1).await.unwrap();
        tree.commit().await.unwrap();
        tree.close().await.unwrap();

        let len = fs::metadata(&file.0).unwrap().len();
        write_at(&file.0, len, &[0xAB; 100]);

        let mut tree = Tree::open(&file.0).await.unwrap();
        assert_eq!(tree.get(a).await.unwrap(), [(a, 1)]);
        tree.insert(b, 2).await.unwrap();
        tree.commit().await.unwrap();
        tree.close().await.unwrap();

        let tree = Tree::open(&file.0).await.unwrap();
        assert_eq!(tree.get(a).await.unwrap(), [(a, 1)]);
        assert_eq!(tree.get(b).await.unwrap(), [(b, 2)]);
        tree.close().await.unwrap();
    });
}

/// Node のページが壊れていれば、開けても読むときに Corrupt を返す。
#[test]
fn reports_corrupted_node_page() {
    let file = TempFile::new();
    let id = FlexId::new(3, 1, 3, 2, 3, 4).unwrap();
    block_on(async {
        let mut tree = Tree::open(&file.0).await.unwrap();
        tree.insert(id, 1).await.unwrap();
        tree.commit().await.unwrap();
        let root = tree.roots[id.root_index()].unwrap();
        tree.close().await.unwrap();

        flip_byte(&file.0, page_offset(root) + 5);

        let mut tree = Tree::open(&file.0).await.unwrap();
        assert!(is_corrupt(tree.get(id).await));
        assert!(is_corrupt(tree.insert(id, 2).await));
        tree.close().await.unwrap();
    });
}

/// メタページがファイルの外のページをルートに指していれば、読むときに Corrupt を返す。
#[test]
fn reports_root_outside_file() {
    let file = TempFile::new();
    block_on(async {
        let tree = Tree::open(&file.0).await.unwrap();
        let value_type = tree.value_type;
        tree.close().await.unwrap();
        let meta = Page::of(&MetaPage::new(1, [Some(PageId(999)), None], value_type));
        write_at(&file.0, page_offset(MetaPage::SLOTS[1]), meta.as_init());

        let tree = Tree::open(&file.0).await.unwrap();
        assert_eq!(tree.generation, 1);
        assert!(is_corrupt(tree.get(FlexId::ROOTS[0]).await));
        assert!(tree.get(FlexId::ROOTS[1]).await.unwrap().is_empty());
        tree.close().await.unwrap();
    });
}

/// 狭い領域の値は、広い target で読むと元の領域のまま、狭い target で読むと target に切り取られて返る。
#[test]
fn get_clips_to_target() {
    let file = TempFile::new();
    let region = FlexId::new(2, 1, 2, 1, 2, 1).unwrap();
    let inner = FlexId::new(4, 5, 4, 6, 4, 7).unwrap();
    let outer = FlexId::new(1, 0, 1, 0, 1, 0).unwrap();
    let disjoint = FlexId::new(2, 2, 2, 1, 2, 1).unwrap();
    block_on(async {
        let mut tree = Tree::open(&file.0).await.unwrap();
        tree.insert(region, 5).await.unwrap();
        assert_eq!(tree.get(inner).await.unwrap(), [(inner, 5)]);
        assert_eq!(tree.get(outer).await.unwrap(), [(region, 5)]);
        assert!(tree.get(disjoint).await.unwrap().is_empty());
        tree.close().await.unwrap();
    });
}

/// insert は既存の値を上書きし、upsert は値のない場所にだけ書く。
#[test]
fn insert_overwrites_and_upsert_fills_only_gaps() {
    let file = TempFile::new();
    let outer = FlexId::new(1, 0, 1, 0, 1, 0).unwrap();
    let region = FlexId::new(2, 1, 2, 1, 2, 1).unwrap();
    let inner = FlexId::new(3, 2, 3, 2, 3, 2).unwrap();
    let sibling = FlexId::new(3, 3, 3, 3, 3, 3).unwrap();
    let gap = FlexId::new(2, 0, 2, 0, 2, 0).unwrap();
    block_on(async {
        let mut tree = Tree::open(&file.0).await.unwrap();
        tree.insert(region, 1).await.unwrap();
        tree.insert(inner, 2).await.unwrap();
        assert_eq!(tree.get(inner).await.unwrap(), [(inner, 2)]);
        assert_eq!(tree.get(sibling).await.unwrap(), [(sibling, 1)]);

        tree.upsert(outer, 3).await.unwrap();
        assert_eq!(tree.get(inner).await.unwrap(), [(inner, 2)]);
        assert_eq!(tree.get(sibling).await.unwrap(), [(sibling, 1)]);
        assert_eq!(tree.get(gap).await.unwrap(), [(gap, 3)]);
        tree.close().await.unwrap();
    });
}

/// 両側に同じ値を書けば1つの領域にまとまり、違う値なら分かれたまま返る。
#[test]
fn merges_equal_halves() {
    let file = TempFile::new();
    let parent = FlexId::new(2, 1, 2, 1, 2, 1).unwrap();
    let [lower, upper] = parent.halves(Dimension::X).unwrap();
    block_on(async {
        let mut tree = Tree::open(&file.0).await.unwrap();
        tree.insert(lower, 1).await.unwrap();
        tree.insert(upper, 1).await.unwrap();
        assert_eq!(tree.get(parent).await.unwrap(), [(parent, 1)]);

        tree.insert(upper, 2).await.unwrap();
        let mut pieces = tree.get(parent).await.unwrap();
        pieces.sort_by_key(|&(_, v)| v);
        assert_eq!(pieces, [(lower, 1), (upper, 2)]);
        tree.close().await.unwrap();
    });
}

/// 一部を remove すると残りの値だけが残り、全部 remove するとルートがなくなる。空の場所や重ならない場所の remove は何も変えない。
#[test]
fn removes_partially_and_entirely() {
    let file = TempFile::new();
    let region = FlexId::new(2, 1, 2, 1, 2, 1).unwrap();
    let inner = FlexId::new(4, 5, 4, 6, 4, 7).unwrap();
    let disjoint = FlexId::new(2, 2, 2, 1, 2, 1).unwrap();
    block_on(async {
        let mut tree = Tree::open(&file.0).await.unwrap();
        tree.remove(region).await.unwrap();
        assert_eq!(tree.roots, [None, None]);

        tree.insert(region, 1).await.unwrap();
        tree.remove(disjoint).await.unwrap();
        assert_eq!(tree.get(region).await.unwrap(), [(region, 1)]);

        tree.remove(inner).await.unwrap();
        assert!(tree.get(inner).await.unwrap().is_empty());
        let rest = tree.get(region).await.unwrap();
        assert!(!rest.is_empty());
        assert!(
            rest.iter()
                .all(|(piece, v)| *v == 1 && region.contains(piece) && !piece.intersects(&inner))
        );

        tree.remove(region).await.unwrap();
        assert_eq!(tree.roots, [None, None]);
        tree.commit().await.unwrap();
        tree.close().await.unwrap();

        let tree = Tree::open(&file.0).await.unwrap();
        assert_eq!(tree.roots, [None, None]);
        tree.close().await.unwrap();
    });
}

/// F の正負にまたがる target は両方のルートに書かれ、開き直しても読める。
#[test]
fn handles_target_spanning_both_roots() {
    let file = TempFile::new();
    let range = RangeId::new(1, [-1, 0], 0, 0).unwrap();
    let negative = FlexId::new(1, -1, 1, 0, 1, 0).unwrap();
    let positive = FlexId::new(1, 0, 1, 0, 1, 0).unwrap();
    block_on(async {
        let mut tree = Tree::open(&file.0).await.unwrap();
        tree.insert(range, 9).await.unwrap();
        assert!(tree.roots.iter().all(Option::is_some));
        tree.commit().await.unwrap();
        tree.close().await.unwrap();

        let tree = Tree::open(&file.0).await.unwrap();
        assert_eq!(tree.get(negative).await.unwrap(), [(negative, 9)]);
        assert_eq!(tree.get(positive).await.unwrap(), [(positive, 9)]);
        assert_eq!(tree.get(range).await.unwrap().len(), 2);
        tree.close().await.unwrap();
    });
}

/// 最大ズームの隣り合う ID（ShortRelativeFlexId に収まらない深い Skip）を区別して書き、開き直しても読める。
#[test]
fn round_trips_ids_at_max_zoom() {
    let file = TempFile::new();
    let deep = |x| {
        FlexId::new(25, -12345, 30, x, 30, 123)
            .unwrap()
            .time_with_segment(T_MAX_ZOOM, 42)
            .unwrap()
    };
    let (a, b) = (deep(987_654), deep(987_655));
    let parent = FlexId::new(25, -12345, 29, 987_654 / 2, 30, 123)
        .unwrap()
        .time_with_segment(T_MAX_ZOOM, 42)
        .unwrap();
    block_on(async {
        let mut tree = Tree::open(&file.0).await.unwrap();
        tree.insert(a, 1).await.unwrap();
        tree.insert(b, 2).await.unwrap();
        tree.commit().await.unwrap();
        tree.close().await.unwrap();

        let mut tree = Tree::open(&file.0).await.unwrap();
        assert_eq!(tree.get(a).await.unwrap(), [(a, 1)]);
        assert_eq!(tree.get(b).await.unwrap(), [(b, 2)]);
        assert_eq!(tree.get(parent).await.unwrap().len(), 2);

        tree.remove(a).await.unwrap();
        assert!(tree.get(a).await.unwrap().is_empty());
        assert_eq!(tree.get(parent).await.unwrap(), [(b, 2)]);
        tree.close().await.unwrap();
    });
}

/// 1ページに収まらない木（RefPage を含む）を書き、開き直して読み、RefPage の先を書き換えても値が合う。
#[test]
fn spills_large_tree_across_pages() {
    let file = TempFile::new();
    let mut expected: HashMap<FlexId, u64> = grid().zip(0..).collect();
    block_on(async {
        let mut tree = Tree::open(&file.0).await.unwrap();
        for (&cell, &value) in &expected {
            tree.insert(cell, value).await.unwrap();
        }
        tree.commit().await.unwrap();
        tree.close().await.unwrap();

        let mut tree = Tree::open(&file.0).await.unwrap();
        for root in tree.roots {
            let root = tree.read_page(root.unwrap()).await.unwrap();
            assert!(has_ref_page(&root));
        }
        assert_cells(&tree, &expected).await;

        for cell in grid().step_by(3) {
            tree.remove(cell).await.unwrap();
            expected.remove(&cell);
        }
        for cell in grid() {
            tree.upsert(cell, 999_999).await.unwrap();
            expected.entry(cell).or_insert(999_999);
        }
        tree.commit().await.unwrap();
        tree.close().await.unwrap();

        let mut tree = Tree::open(&file.0).await.unwrap();
        assert_cells(&tree, &expected).await;
        assert_roots(&tree, &expected).await;

        for root in FlexId::ROOTS {
            tree.remove(root).await.unwrap();
        }
        assert_eq!(tree.roots, [None, None]);
        tree.close().await.unwrap();
    });
}

/// ランダムな insert / upsert / remove と、コミットあり・なしの開き直しを繰り返し、素朴な実装と同じ結果になるか確かめる。
fn check_against_naive_model<V: DiskValue + Clone + PartialEq + Debug>(
    seeds: core::ops::Range<u64>,
    mut random_value: impl FnMut(&mut ChaCha8Rng) -> V,
) {
    for seed in seeds {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let file = TempFile::new();
        let mut expected: HashMap<FlexId, V> = HashMap::new();
        let mut committed = expected.clone();
        block_on(async {
            let mut tree = FlexTreeCore::<V>::open(&file.0).await.unwrap();
            for _ in 0..5 {
                for _ in 0..40 {
                    let target = random_target(&mut rng);
                    let value = random_value(&mut rng);
                    let cells = grid().filter(|cell| target.contains(cell));
                    match rng.random_range(0..3) {
                        0 => {
                            tree.insert(target, value.clone()).await.unwrap();
                            cells.for_each(|cell| {
                                expected.insert(cell, value.clone());
                            });
                        }
                        1 => {
                            tree.upsert(target, value.clone()).await.unwrap();
                            cells.for_each(|cell| {
                                expected.entry(cell).or_insert_with(|| value.clone());
                            });
                        }
                        _ => {
                            tree.remove(target).await.unwrap();
                            cells.for_each(|cell| {
                                expected.remove(&cell);
                            });
                        }
                    }
                }
                assert_roots(&tree, &expected).await;

                if rng.random_range(0..2) == 0 {
                    tree.commit().await.unwrap();
                    committed = expected.clone();
                } else {
                    expected = committed.clone();
                }
                tree.close().await.unwrap();
                tree = FlexTreeCore::open(&file.0).await.unwrap();
                assert_roots(&tree, &expected).await;
            }
            assert_cells(&tree, &expected).await;
            tree.close().await.unwrap();
        });
    }
}

/// 少ない種類の u64 で、乱択の操作が素朴な実装と同じ結果になる。
#[test]
fn matches_naive_model_with_u64() {
    check_against_naive_model(0..8, |rng| rng.random_range(0..4u64));
}

/// 長さ 0 から格納できる最大までの文字列を混ぜても、素朴な実装と同じ結果になる。
#[test]
fn matches_naive_model_with_strings_of_mixed_lengths() {
    let lengths = [0, 5, 17, MAX_VALUE_LEN];
    check_against_naive_model(0..4, |rng| {
        let len = lengths[rng.random_range(0..lengths.len())];
        let fill = char::from(b'a' + rng.random_range(0..2u8));
        core::iter::repeat_n(fill, len).collect::<String>()
    });
}

/// 書いたページは、チェックサムが合えば読め、1バイトでも変われば Corrupt になる。
#[test]
fn page_checksum_detects_flipped_byte() {
    let mut page = Page::of(&MetaPage::new(7, [Some(PageId(3)), None], 0));
    let meta = page.try_head_as::<MetaPage>().unwrap();
    assert_eq!(meta.generation.get(), 7);
    assert_eq!(meta.roots(), [Some(PageId(3)), None]);

    for offset in [0, 100, Page::CAPACITY - 1, Page::SIZE - 1] {
        let original = page.as_init()[offset];
        page.as_uninit()[offset] = MaybeUninit::new(!original);
        assert!(matches!(
            page.try_head_as::<MetaPage>(),
            Err(DiskError::Corrupt)
        ));
        page.as_uninit()[offset] = MaybeUninit::new(original);
    }
}

/// ファイルにないページの読み込みは、I/O に出す前に Corrupt で弾く。
#[test]
fn paged_file_rejects_pages_outside_file() {
    let file = TempFile::new();
    block_on(async {
        let mut paged = PagedFile::open(&file.0).await.unwrap();
        assert!(matches!(
            paged.read(PageId(0)).await,
            Err(DiskError::Corrupt)
        ));

        let id = paged.alloc();
        paged.write(id, Page::new()).await.unwrap();
        assert!(paged.read(id).await.is_ok());
        assert!(matches!(
            paged.read_many(&[id, PageId(5)]).await,
            Err(DiskError::Corrupt)
        ));
        paged.close().await.unwrap();
    });
}

/// 3bit のうち未使用の種類のコードは Corrupt になる。
#[test]
fn node_kind_rejects_unknown_codes() {
    for code in 0..=u8::MAX {
        assert_eq!(NodeKind::try_from(code).is_ok(), code <= 4, "code {code}");
    }
}

/// NodePage の先頭が、ルートが RefPage、本体より長い、格納できる長さを超える値、未使用の種類、
/// のどれかなら Corrupt になる。
#[test]
fn node_page_rejects_invalid_heads() {
    let value = NodeKind::Value as u16;
    let heads = [
        (NodeKind::RefPage as u16) << 12 | 6,
        value << 12 | 0xFFF,
        value << 12 | (MAX_VALUE_LEN as u16 + 1),
        7 << 12 | 8,
    ];
    for head in heads {
        let mut bytes = vec![0; Page::CAPACITY];
        bytes[..2].copy_from_slice(&head.to_be_bytes());
        let node_page = NodePage::ref_from_bytes(&bytes).unwrap();
        assert!(
            matches!(node_page.decode(), Err(DiskError::Corrupt)),
            "head {head:#06x}"
        );
    }
}

/// Node のバイト列が足りない、余る、格納できる長さを超える値、深さの合計が上限を超える ShortSkip、
/// のどれも Corrupt になる。
#[test]
fn node_decode_rejects_malformed_bytes() {
    let too_long = [0; MAX_VALUE_LEN + 1];
    let cases: [(NodeKind, &[u8]); 4] = [
        (NodeKind::Value, &too_long),
        (NodeKind::Branch, &[0; 2]),
        (NodeKind::RefPage, &[0; 5]),
        // 深さ 7 / 7 / 7 の ShortSkip と、その子の Value
        (
            NodeKind::ShortSkip,
            &[0x01, 0xFF, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        ),
    ];
    for (kind, bytes) in cases {
        assert!(
            matches!(Node::decode(kind, bytes), Err(DiskError::Corrupt)),
            "{kind:?} {bytes:?}"
        );
    }
}

/// 1ページに収まる Node は1ページのまま、収まらない Node は RefPage で分けて書き、読み戻すと元の Node に戻る。
#[test]
fn node_page_split_round_trips() {
    let short = FlexId::new(2, 1, 2, 1, 2, 1)
        .unwrap()
        .relative_to(&FlexId::ROOTS[0])
        .unwrap();
    let long = FlexId::new(20, 5, 20, 5, 20, 5)
        .unwrap()
        .relative_to(&FlexId::ROOTS[0])
        .unwrap();
    let mut next = 0;
    let small = balanced(3, &mut next);
    let large = balanced(10, &mut next);
    let short_skip = Node::Skip {
        path: short,
        child: Box::new(large.clone()),
    };
    let long_skip = Node::Skip {
        path: long,
        child: Box::new(large.clone()),
    };
    assert_eq!(short_skip.kind(), NodeKind::ShortSkip);
    assert_eq!(long_skip.kind(), NodeKind::LongSkip);

    assert_eq!(split_and_restore(&value_node(42)), (value_node(42), 1));
    assert_eq!(split_and_restore(&small), (small, 1));
    for node in [large, short_skip, long_skip] {
        let (restored, page_count) = split_and_restore(&node);
        assert!(page_count > 1);
        assert_eq!(restored, node);
    }
}

/// 格納できる範囲のどの長さの値も、書いたとおりに読め、開き直しても変わらない。
#[test]
fn round_trips_values_of_each_length() {
    let file = TempFile::new();
    let lengths = [0, 1, 16, 17, MAX_VALUE_LEN - 1, MAX_VALUE_LEN];
    let entries: Vec<(FlexId, Vec<u8>)> = lengths
        .iter()
        .zip(grid().step_by(97))
        .map(|(&len, cell)| (cell, (0..len).map(|i| (i % 251) as u8).collect()))
        .collect();
    block_on(async {
        let mut tree = FlexTreeCore::<Vec<u8>>::open(&file.0).await.unwrap();
        for (cell, value) in &entries {
            tree.insert(*cell, value.clone()).await.unwrap();
        }
        for (cell, value) in &entries {
            assert_eq!(tree.get(*cell).await.unwrap(), [(*cell, value.clone())]);
        }
        tree.commit().await.unwrap();
        tree.close().await.unwrap();

        let tree = FlexTreeCore::<Vec<u8>>::open(&file.0).await.unwrap();
        for (cell, value) in &entries {
            assert_eq!(tree.get(*cell).await.unwrap(), [(*cell, value.clone())]);
        }
        tree.close().await.unwrap();
    });
}

/// ルート全体を長さ 0 の値1つにした（ページの中身が 0 バイトの）木も、書いて読み直せる。
#[test]
fn stores_empty_value_covering_whole_root() {
    let file = TempFile::new();
    let root = FlexId::ROOTS[0];
    block_on(async {
        let mut tree = FlexTreeCore::<String>::open(&file.0).await.unwrap();
        tree.insert(root, String::new()).await.unwrap();
        tree.commit().await.unwrap();
        tree.close().await.unwrap();

        let tree = FlexTreeCore::<String>::open(&file.0).await.unwrap();
        assert_eq!(tree.get(root).await.unwrap(), [(root, String::new())]);
        tree.close().await.unwrap();
    });
}

/// 別々に書いた同じバイト列の値は、格納できる最大の長さでも1つの領域にまとまる。
#[test]
fn merges_equal_values_of_max_len() {
    let file = TempFile::new();
    let parent = FlexId::new(2, 1, 2, 1, 2, 1).unwrap();
    let [lower, upper] = parent.halves(Dimension::X).unwrap();
    let value = "x".repeat(MAX_VALUE_LEN);
    block_on(async {
        let mut tree = FlexTreeCore::<String>::open(&file.0).await.unwrap();
        tree.insert(lower, value.clone()).await.unwrap();
        tree.insert(upper, value.clone()).await.unwrap();
        assert_eq!(tree.get(parent).await.unwrap(), [(parent, value)]);
        tree.close().await.unwrap();
    });
}

/// 格納できる長さを超える値の insert / upsert は ValueTooLarge を返し、木は変わらない。
#[test]
fn rejects_values_longer_than_max_len() {
    let file = TempFile::new();
    let id = FlexId::new(3, 1, 3, 2, 3, 4).unwrap();
    let too_long = "z".repeat(MAX_VALUE_LEN + 1);
    let is_too_large =
        |result: Result<(), Error>| matches!(result, Err(Error::Disk(DiskError::ValueTooLarge)));
    block_on(async {
        let mut tree = FlexTreeCore::<String>::open(&file.0).await.unwrap();
        assert!(is_too_large(tree.insert(id, too_long.clone()).await));
        assert!(is_too_large(tree.upsert(id, too_long).await));
        assert_eq!(tree.roots, [None, None]);
        tree.close().await.unwrap();
    });
}

/// 別の値の型で作ったファイルは開けず、同じ型なら開ける。
#[test]
fn rejects_files_of_other_value_type() {
    let file = TempFile::new();
    let is_incompatible =
        |result: Result<_, Error>| matches!(result, Err(Error::Disk(DiskError::Incompatible)));
    block_on(async {
        let mut tree = Tree::open(&file.0).await.unwrap();
        tree.insert(FlexId::new(3, 1, 3, 2, 3, 4).unwrap(), 1)
            .await
            .unwrap();
        tree.commit().await.unwrap();
        tree.close().await.unwrap();

        assert!(is_incompatible(
            FlexTreeCore::<String>::open(&file.0).await.map(drop)
        ));
        assert!(is_incompatible(
            FlexTreeCore::<i64>::open(&file.0).await.map(drop)
        ));
        Tree::open(&file.0).await.unwrap().close().await.unwrap();
    });
}

fn round_trip<V: DiskValue + PartialEq + Debug>(value: V) {
    let mut bytes = Vec::new();
    value.encode(&mut bytes);
    assert_eq!(V::decode(&bytes).unwrap(), value);
}

/// 標準の型の DiskValue は、書いた値に戻り、型として不正なバイト列を Corrupt で弾き、型名が互いに異なる。
#[test]
fn standard_disk_values_round_trip() {
    round_trip(u8::MAX);
    round_trip(u16::MAX);
    round_trip(u32::MAX);
    round_trip(u64::MAX);
    round_trip(u128::MAX);
    round_trip(i8::MIN);
    round_trip(i16::MIN);
    round_trip(i32::MIN);
    round_trip(i64::MIN);
    round_trip(i128::MIN);
    round_trip(true);
    round_trip(false);
    round_trip([1u8, 2, 3]);
    round_trip(vec![0u8, 255]);
    round_trip(String::from("空間ID"));
    round_trip(String::new());

    assert!(matches!(u64::decode(&[0; 7]), Err(DiskError::Corrupt)));
    assert!(matches!(i32::decode(&[0; 5]), Err(DiskError::Corrupt)));
    assert!(matches!(bool::decode(&[2]), Err(DiskError::Corrupt)));
    assert!(matches!(bool::decode(&[]), Err(DiskError::Corrupt)));
    assert!(matches!(
        <[u8; 4]>::decode(&[0; 3]),
        Err(DiskError::Corrupt)
    ));
    assert!(matches!(String::decode(&[0xFF]), Err(DiskError::Corrupt)));

    let names = [
        u8::type_name(),
        u16::type_name(),
        u32::type_name(),
        u64::type_name(),
        u128::type_name(),
        i8::type_name(),
        i16::type_name(),
        i32::type_name(),
        i64::type_name(),
        i128::type_name(),
        bool::type_name(),
        <[u8; 3]>::type_name(),
        <[u8; 4]>::type_name(),
        <Vec<u8>>::type_name(),
        String::type_name(),
    ];
    assert_eq!(names.iter().collect::<HashSet<_>>().len(), names.len());
}
