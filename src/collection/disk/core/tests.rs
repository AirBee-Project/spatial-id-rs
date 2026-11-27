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

/// ルート以外のページ（RefPage の行き先）が Skip で始まらないことを確かめる。
async fn assert_ref_pages_never_point_to_skip<V>(tree: &FlexTreeCore<V>) {
    for (page_id, node) in tree.live_pages_in_preorder().await.unwrap() {
        assert!(
            tree.roots.contains(&Some(page_id)) || !matches!(node, Node::Skip { .. }),
            "RefPage の先が Skip になっている: {page_id:?}"
        );
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
    let t_index = if cfg!(feature = "temporal_id") { 42 } else { 0 };
    let deep = |x| {
        FlexId::new(25, -12345, 30, x, 30, 123)
            .unwrap()
            .time_with_segment(T_MAX_ZOOM, t_index)
            .unwrap()
    };
    let (a, b) = (deep(987_654), deep(987_655));
    let parent = FlexId::new(25, -12345, 29, 987_654 / 2, 30, 123)
        .unwrap()
        .time_with_segment(T_MAX_ZOOM, t_index)
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
        assert_ref_pages_never_point_to_skip(&tree).await;

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
    round_trip(());
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

    assert!(matches!(<()>::decode(&[0]), Err(DiskError::Corrupt)));
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
        <()>::type_name(),
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

/// ランダムな insert / upsert / remove を `count` 回施し、同じ操作を施した期待値を返す。
async fn apply_random_operations(
    tree: &mut Tree,
    rng: &mut ChaCha8Rng,
    expected: &mut HashMap<FlexId, u64>,
    count: usize,
) {
    for _ in 0..count {
        let target = random_target(rng);
        let value = rng.random_range(0..4u64);
        let cells = grid().filter(|cell| target.contains(cell));
        match rng.random_range(0..3) {
            0 => {
                tree.insert(target, value).await.unwrap();
                cells.for_each(|cell| {
                    expected.insert(cell, value);
                });
            }
            1 => {
                tree.upsert(target, value).await.unwrap();
                cells.for_each(|cell| {
                    expected.entry(cell).or_insert(value);
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
}

/// 全セルに異なる値を書いた、RefPage を含む木。
async fn insert_distinct_values(tree: &mut Tree) -> HashMap<FlexId, u64> {
    let expected: HashMap<FlexId, u64> = grid().zip(0..).collect();
    for (&cell, &value) in &expected {
        tree.insert(cell, value).await.unwrap();
    }
    expected
}

/// compact したファイルは、生きているページだけが行きがけ順にページ 2 から並び、その長さに切り詰められている。
async fn assert_compacted(tree: &Tree, path: &Path) {
    let pages = tree.live_pages_in_preorder().await.unwrap();
    let slot_count = MetaPage::SLOTS.len() as u64;
    let ids: Vec<u64> = pages.iter().map(|(id, _)| id.0).collect();
    let preorder: Vec<u64> = (slot_count..slot_count + pages.len() as u64).collect();
    assert_eq!(ids, preorder);
    assert_eq!(tree.file.len(), slot_count + pages.len() as u64);
    assert_eq!(
        fs::metadata(path).unwrap().len(),
        (slot_count + pages.len() as u64) * Page::SIZE as u64
    );
}

/// 乱択の書き換えとコミットを重ねたファイルを compact しても内容は変わらず、ファイルは縮み、開き直しても読める。
#[test]
fn compact_keeps_contents_and_shrinks_file() {
    for seed in 0..4 {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let file = TempFile::new();
        let mut expected = HashMap::new();
        block_on(async {
            let mut tree = Tree::open(&file.0).await.unwrap();
            for _ in 0..4 {
                apply_random_operations(&mut tree, &mut rng, &mut expected, 40).await;
                tree.commit().await.unwrap();
            }
            let before = fs::metadata(&file.0).unwrap().len();

            tree.compact().await.unwrap();
            assert!(fs::metadata(&file.0).unwrap().len() < before);
            assert_compacted(&tree, &file.0).await;
            assert_roots(&tree, &expected).await;
            tree.close().await.unwrap();

            let tree = Tree::open(&file.0).await.unwrap();
            assert_cells(&tree, &expected).await;
            tree.close().await.unwrap();
        });
    }
}

/// RefPage が続く木も行きがけ順に並び、compact の後も書き換えと compact を続けられる。
#[test]
fn compact_orders_multi_page_tree_and_allows_further_edits() {
    let file = TempFile::new();
    block_on(async {
        let mut tree = Tree::open(&file.0).await.unwrap();
        let mut expected = insert_distinct_values(&mut tree).await;
        tree.compact().await.unwrap();
        assert!(tree.live_pages_in_preorder().await.unwrap().len() > 2);
        assert_compacted(&tree, &file.0).await;
        assert_cells(&tree, &expected).await;

        for cell in grid().step_by(5) {
            tree.remove(cell).await.unwrap();
            expected.remove(&cell);
        }
        tree.compact().await.unwrap();
        assert_compacted(&tree, &file.0).await;
        tree.close().await.unwrap();

        let tree = Tree::open(&file.0).await.unwrap();
        assert_cells(&tree, &expected).await;
        tree.close().await.unwrap();
    });
}

/// 2回続けて compact しても、ファイルの長さと内容は変わらない。
#[test]
fn compact_is_idempotent() {
    let file = TempFile::new();
    block_on(async {
        let mut tree = Tree::open(&file.0).await.unwrap();
        let expected = insert_distinct_values(&mut tree).await;
        tree.compact().await.unwrap();
        let len = fs::metadata(&file.0).unwrap().len();
        tree.compact().await.unwrap();
        assert_eq!(fs::metadata(&file.0).unwrap().len(), len);
        assert_compacted(&tree, &file.0).await;
        assert_roots(&tree, &expected).await;
        tree.close().await.unwrap();
    });
}

/// 空の木の compact はメタページ2枚だけのファイルにし、その後も書き込める。
#[test]
fn compact_empty_tree() {
    let file = TempFile::new();
    let id = FlexId::new(3, 1, 3, 2, 3, 4).unwrap();
    block_on(async {
        let mut tree = Tree::open(&file.0).await.unwrap();
        tree.insert(id, 1).await.unwrap();
        tree.remove(id).await.unwrap();
        tree.compact().await.unwrap();
        assert_compacted(&tree, &file.0).await;
        assert_eq!(tree.file.len(), MetaPage::SLOTS.len() as u64);

        tree.insert(id, 2).await.unwrap();
        tree.commit().await.unwrap();
        tree.close().await.unwrap();

        let tree = Tree::open(&file.0).await.unwrap();
        assert_eq!(tree.get(id).await.unwrap(), [(id, 2)]);
        tree.close().await.unwrap();
    });
}

/// compact はコミットを兼ね、まだコミットしていない変更もファイルに残す。
#[test]
fn compact_commits_pending_changes() {
    let file = TempFile::new();
    let id = FlexId::new(3, 1, 3, 2, 3, 4).unwrap();
    block_on(async {
        let mut tree = Tree::open(&file.0).await.unwrap();
        tree.insert(id, 1).await.unwrap();
        tree.compact().await.unwrap();
        tree.close().await.unwrap();

        let tree = Tree::open(&file.0).await.unwrap();
        assert_eq!(tree.get(id).await.unwrap(), [(id, 1)]);
        tree.close().await.unwrap();
    });
}

/// compact 後は両方のメタページが同じ木を指すので、片方が壊れても同じ内容で開ける。
#[test]
fn compact_leaves_both_meta_slots_valid() {
    let file = TempFile::new();
    block_on(async {
        let mut tree = Tree::open(&file.0).await.unwrap();
        let expected = insert_distinct_values(&mut tree).await;
        tree.compact().await.unwrap();
        tree.close().await.unwrap();

        for slot in MetaPage::SLOTS {
            flip_byte(&file.0, page_offset(slot));
            let tree = Tree::open(&file.0).await.unwrap();
            assert_roots(&tree, &expected).await;
            tree.close().await.unwrap();
            flip_byte(&file.0, page_offset(slot));
        }
    });
}

/// compact の各手順の後で落ちても、同じ内容で開け、そこから compact し直せる。
///
/// 止める位置は、末尾へ写した後（メタページは元の木）、メタページを写しに向けた後、先頭へ書き戻した後
/// （メタページはまだ写し）、メタページを書き戻した先に向けた後（切り詰める前）の4つ。
#[test]
fn compact_survives_interruption_at_each_step() {
    for stop in 1..=4 {
        let file = TempFile::new();
        block_on(async {
            let mut tree = Tree::open(&file.0).await.unwrap();
            let expected = insert_distinct_values(&mut tree).await;
            tree.commit().await.unwrap();

            let roots = tree.roots;
            let pages = tree.live_pages_in_preorder().await.unwrap();
            let copy = tree.relocate(roots, &pages, tree.file.len()).await.unwrap();
            if stop >= 2 {
                tree.roots = copy;
                tree.persist_to_both_slots().await.unwrap();
            }
            if stop >= 3 {
                tree.roots = tree
                    .relocate(roots, &pages, MetaPage::SLOTS.len() as u64)
                    .await
                    .unwrap();
            }
            if stop >= 4 {
                tree.persist_to_both_slots().await.unwrap();
            }
            tree.close().await.unwrap();

            let mut tree = Tree::open(&file.0).await.unwrap();
            assert_cells(&tree, &expected).await;
            tree.compact().await.unwrap();
            assert_compacted(&tree, &file.0).await;
            assert_roots(&tree, &expected).await;
            tree.close().await.unwrap();
        });
    }
}

/// RefPage が自分のページを指す（循環した）壊れた木は、辿り続けずに Corrupt を返す。
#[test]
fn compact_rejects_cyclic_ref_page() {
    let file = TempFile::new();
    block_on(async {
        let mut tree = Tree::open(&file.0).await.unwrap();
        let page_id = tree.file.alloc();
        let node = Node::Branch {
            dimension: Dimension::X,
            split_dimensions: DimensionSet::from(Dimension::X),
            lower: Box::new(Node::RefPage {
                page_id,
                split_dimensions: DimensionSet::from(Dimension::X),
            }),
            upper: Box::new(value_node(1)),
        };
        tree.file
            .write(page_id, Page::of(&*NodePage::encode(&node)))
            .await
            .unwrap();
        tree.roots = [Some(page_id), None];
        tree.commit().await.unwrap();

        assert!(is_corrupt(tree.compact().await));
        tree.close().await.unwrap();
    });
}

/// `PagedFile::read_sync` で書いたページを同期的に読み戻せる。
#[test]
fn paged_file_read_sync_round_trips() {
    let file = TempFile::new();
    block_on(async {
        let mut paged = PagedFile::open(&file.0).await.unwrap();
        assert!(matches!(
            paged.read_sync(PageId(0)),
            Err(DiskError::Corrupt)
        ));

        let id = paged.alloc();
        let meta = MetaPage::new(10, [Some(PageId(3)), None], 1);
        paged.write(id, Page::of(&meta)).await.unwrap();
        paged.sync().await.unwrap();

        let got = paged.read_sync(id).unwrap();
        assert_eq!(got.try_head_as::<MetaPage>().unwrap().generation.get(), 10);
        paged.close().await.unwrap();
    });
}

/// 粗い次元での後から挿入（Skip・Branch・RefPage を別次元で割る経路）でも、メモリ版と同じカノニカル形・
/// 厳密 Z-order 昇順を保ち、複数の親から共有された RefPage を含んでも compact が通る。
#[test]
fn edit_preserves_canonical_zorder_and_matches_mem() {
    let file = TempFile::new();
    let x_half = FlexId::new(0, 0, 1, 0, 0, 0).unwrap();
    let x_upper = FlexId::new(0, 0, 1, 1, 0, 0).unwrap();
    let f_lower = FlexId::new(1, 0, 0, 0, 0, 0).unwrap();

    block_on(async {
        let mut disk = Tree::open(&file.0).await.unwrap();
        let mut mem = crate::collection::core::FlexTreeCore::<u64>::default();
        disk.insert(x_half, 1).await.unwrap();
        mem.insert(x_half, 1);
        disk.insert(f_lower, 2).await.unwrap();
        mem.insert(f_lower, 2);

        let disk_items = disk.get(FlexId::UPPER_MAX).await.unwrap();
        let mem_items: Vec<(FlexId, u64)> = mem.iter().map(|(id, &v)| (id, v)).collect();
        assert_eq!(disk_items, mem_items);
        assert!(disk_items.windows(2).all(|w| w[0].0 < w[1].0));

        disk.insert(x_upper, 3).await.unwrap();
        mem.insert(x_upper, 3);
        disk.insert(f_lower, 4).await.unwrap();
        mem.insert(f_lower, 4);

        let disk_items = disk.get(FlexId::UPPER_MAX).await.unwrap();
        let mem_items: Vec<(FlexId, u64)> = mem.iter().map(|(id, &v)| (id, v)).collect();
        assert_eq!(disk_items, mem_items);
        assert!(disk_items.windows(2).all(|w| w[0].0 < w[1].0));
        disk.compact().await.unwrap();
        disk.close().await.unwrap();
    });

    // RefPage を跨ぐ規模でも別次元分割後にメモリ版と完全一致し、compact が通る
    let file = TempFile::new();
    block_on(async {
        let mut disk = Tree::open(&file.0).await.unwrap();
        let mut mem = crate::collection::core::FlexTreeCore::<u64>::default();

        // F=0 のまま X/Y を細かく埋めて RefPage を作ってから、F=1 の粗い領域を上書きする
        let mut next = 1u64;
        for x in 0..32u32 {
            for y in 0..16u32 {
                let id = FlexId::new(0, 0, 5, x, 4, y).unwrap();
                disk.insert(id, next).await.unwrap();
                mem.insert(id, next);
                next += 1;
            }
        }
        assert!(has_ref_page(
            &disk.read_page(disk.roots[0].unwrap()).await.unwrap()
        ));

        let coarse_f = FlexId::new(1, 0, 2, 1, 2, 1).unwrap();
        disk.insert(coarse_f, 999_999).await.unwrap();
        mem.insert(coarse_f, 999_999);
        disk.upsert(FlexId::new(1, 1, 0, 0, 0, 0).unwrap(), 777)
            .await
            .unwrap();
        mem.insert_with(FlexId::new(1, 1, 0, 0, 0, 0).unwrap(), 777, |old, _| *old);

        let disk_items = disk.get(FlexId::UPPER_MAX).await.unwrap();
        let mem_items: Vec<(FlexId, u64)> = mem.iter().map(|(id, &v)| (id, v)).collect();
        assert_eq!(disk_items, mem_items);
        assert!(disk_items.windows(2).all(|w| w[0].0 < w[1].0));

        disk.compact().await.unwrap();
        assert_eq!(disk.get(FlexId::UPPER_MAX).await.unwrap(), mem_items);
        disk.close().await.unwrap();
    });
}

/// 順序規則に反した非正規形の木（旧フォーマットや壊れた順序のファイル）を編集しても、
/// `side_toward` のアンダーフローや子孫の座標ずれを起こさず正しく動作する。
#[test]
fn non_canonical_tree_edits_without_underflow_or_corruption() {
    // (1) Skip(x+1/0, Branch(F, Value(1), Value(2))) に target == UPPER_MAX で upsert してもアンダーフローしない
    let file = TempFile::new();
    block_on(async {
        let mut tree = Tree::open(&file.0).await.unwrap();
        let x0 = FlexId::new(0, 0, 1, 0, 0, 0).unwrap();
        let skip_path = x0.relative_to(&FlexId::UPPER_MAX).unwrap();
        let non_canonical = Node::Skip {
            path: skip_path,
            child: Box::new(Node::Branch {
                dimension: Dimension::F,
                split_dimensions: DimensionSet::from(Dimension::F),
                lower: Box::new(value_node(1)),
                upper: Box::new(value_node(2)),
            }),
        };
        let root_id = tree.store(non_canonical).await.unwrap();
        tree.roots = [Some(root_id), None];
        tree.commit().await.unwrap();

        tree.upsert(FlexId::UPPER_MAX, 9).await.unwrap();
        assert_eq!(
            tree.get(FlexId::new(1, 0, 1, 0, 0, 0).unwrap())
                .await
                .unwrap(),
            [(FlexId::new(1, 0, 1, 0, 0, 0).unwrap(), 1)]
        );
        assert_eq!(
            tree.get(FlexId::new(1, 1, 1, 0, 0, 0).unwrap())
                .await
                .unwrap(),
            [(FlexId::new(1, 1, 1, 0, 0, 0).unwrap(), 2)]
        );
        assert_eq!(
            tree.get(FlexId::new(0, 0, 1, 1, 0, 0).unwrap())
                .await
                .unwrap(),
            [(FlexId::new(0, 0, 1, 1, 0, 0).unwrap(), 9)]
        );
        tree.close().await.unwrap();
    });

    // (2) Branch(X, Branch(F, 1, 2), Skip(f+1/0, 3)) に対して f=1/0 へ insert しても f=1/1 側が1段ずれない
    let file = TempFile::new();
    block_on(async {
        let mut tree = Tree::open(&file.0).await.unwrap();
        let f_lower_rel = FlexId::new(1, 0, 1, 1, 0, 0)
            .unwrap()
            .relative_to(&FlexId::new(0, 0, 1, 1, 0, 0).unwrap())
            .unwrap();
        let non_canonical = Node::Branch {
            dimension: Dimension::X,
            split_dimensions: DimensionSet::from(Dimension::X) | DimensionSet::from(Dimension::F),
            lower: Box::new(Node::Branch {
                dimension: Dimension::F,
                split_dimensions: DimensionSet::from(Dimension::F),
                lower: Box::new(value_node(1)),
                upper: Box::new(value_node(2)),
            }),
            upper: Box::new(Node::Skip {
                path: f_lower_rel,
                child: Box::new(value_node(3)),
            }),
        };
        let root_id = tree.store(non_canonical).await.unwrap();
        tree.roots = [Some(root_id), None];
        tree.commit().await.unwrap();

        let target = FlexId::new(1, 0, 0, 0, 0, 0).unwrap();
        tree.insert(target, 7).await.unwrap();
        // f=1/0 は全域 7 に上書きされ、f=1/1 は元の (x=0 -> 2, x=1 -> 空) のまま座標がずれない
        assert_eq!(
            tree.get(FlexId::new(1, 0, 1, 0, 0, 0).unwrap())
                .await
                .unwrap(),
            [(FlexId::new(1, 0, 1, 0, 0, 0).unwrap(), 7)]
        );
        assert_eq!(
            tree.get(FlexId::new(1, 0, 1, 1, 0, 0).unwrap())
                .await
                .unwrap(),
            [(FlexId::new(1, 0, 1, 1, 0, 0).unwrap(), 7)]
        );
        assert_eq!(
            tree.get(FlexId::new(1, 1, 1, 0, 0, 0).unwrap())
                .await
                .unwrap(),
            [(FlexId::new(1, 1, 1, 0, 0, 0).unwrap(), 2)]
        );
        assert!(
            tree.get(FlexId::new(1, 1, 1, 1, 0, 0).unwrap())
                .await
                .unwrap()
                .is_empty()
        );
        tree.close().await.unwrap();
    });
}

/// 粗い次元の編集で未変更の RefPage を丸ごと書き直さず、変更なしの編集は追記せず、
/// 通常の編集で未編集側の RefPage を読み込まない。
#[test]
fn edits_share_ref_pages_and_avoid_unnecessary_io() {
    // (1) F=0 で複数 RefPage を作った後、F=1 の狭い範囲を1件挿入しても全ページ追記にならず、無変更編集は0追記
    let file = TempFile::new();
    block_on(async {
        let mut tree = FlexTreeCore::<[u8; 32]>::open(&file.0).await.unwrap();
        let mut next = 1u64;
        for x in 0..32u32 {
            for y in 0..32u32 {
                let id = FlexId::new(0, 0, 5, x, 5, y).unwrap();
                let mut val = [0u8; 32];
                val[..8].copy_from_slice(&next.to_be_bytes());
                tree.insert(id, val).await.unwrap();
                next += 1;
            }
        }
        tree.compact().await.unwrap();
        let live_before = tree.live_pages_in_preorder().await.unwrap().len();
        assert!(live_before >= 10, "live_before={live_before}");

        let len_before = tree.file.len();
        let updated = [0xAA; 32];
        tree.insert(FlexId::new(1, 0, 5, 0, 5, 0).unwrap(), updated)
            .await
            .unwrap();
        let appended = (tree.file.len() - len_before) as usize;
        assert!(
            appended < live_before / 2,
            "未変更 RefPage の共有が効いていない: appended={appended}, live_before={live_before}"
        );

        let len_after_insert = tree.file.len();
        tree.upsert(FlexId::new(1, 1, 0, 0, 0, 0).unwrap(), [0xBB; 32])
            .await
            .unwrap();
        tree.insert(FlexId::new(1, 0, 5, 0, 5, 0).unwrap(), updated)
            .await
            .unwrap();
        assert_eq!(tree.file.len(), len_after_insert);

        tree.compact().await.unwrap();
        tree.close().await.unwrap();
    });

    // (2) f=1/0 と f=1/1 で割っている次元が違えば中身が等しくなりえないので、f=1/1 側のページが壊れていても
    // f=1/0 側の編集はそれを読まずに完了する
    let file = TempFile::new();
    block_on(async {
        let mut tree = Tree::open(&file.0).await.unwrap();
        let mut next = 1u64;
        for x in 0..32u32 {
            for y in 0..16u32 {
                tree.insert(FlexId::new(1, 0, 5, x, 4, y).unwrap(), next)
                    .await
                    .unwrap();
                next += 1;
            }
        }
        for x in 0..512u32 {
            tree.insert(FlexId::new(1, 1, 9, x, 0, 0).unwrap(), next)
                .await
                .unwrap();
            next += 1;
        }
        tree.compact().await.unwrap();
        let root = tree.read_page(tree.roots[0].unwrap()).await.unwrap();
        let Node::Branch {
            dimension: Dimension::F,
            upper,
            ..
        } = root
        else {
            panic!("ルートが F 分岐になっていない");
        };
        fn first_ref_page(node: &Node) -> Option<PageId> {
            match node {
                Node::RefPage { page_id, .. } => Some(*page_id),
                Node::Branch { lower, upper, .. } => {
                    first_ref_page(lower).or_else(|| first_ref_page(upper))
                }
                Node::Skip { child, .. } => first_ref_page(child),
                Node::Value(_) => None,
            }
        }
        let upper_page =
            first_ref_page(&upper).expect("upper 側の部分木に RefPage が含まれているはず");
        tree.close().await.unwrap();

        flip_byte(&file.0, page_offset(upper_page) + 5);

        let mut tree = Tree::open(&file.0).await.unwrap();
        // lower 側(f=1/0)の編集は upper 側ページを一切読まないため成功する
        tree.insert(FlexId::new(1, 0, 5, 0, 4, 0).unwrap(), 888_888)
            .await
            .unwrap();
        assert_eq!(
            tree.get(FlexId::new(1, 0, 5, 0, 4, 0).unwrap())
                .await
                .unwrap(),
            [(FlexId::new(1, 0, 5, 0, 4, 0).unwrap(), 888_888)]
        );
        tree.close().await.unwrap();
    });
}

/// 複数ホップの循環参照（A -> B -> A）も compact が Corrupt で弾く。
#[test]
fn compact_rejects_multi_hop_cyclic_ref_page() {
    let file = TempFile::new();
    block_on(async {
        let mut tree = Tree::open(&file.0).await.unwrap();
        let page_a = tree.file.alloc();
        let page_b = tree.file.alloc();
        let node_a = Node::Branch {
            dimension: Dimension::X,
            split_dimensions: DimensionSet::from(Dimension::X),
            lower: Box::new(Node::RefPage {
                page_id: page_b,
                split_dimensions: DimensionSet::from(Dimension::X),
            }),
            upper: Box::new(value_node(1)),
        };
        let node_b = Node::Branch {
            dimension: Dimension::X,
            split_dimensions: DimensionSet::from(Dimension::X),
            lower: Box::new(Node::RefPage {
                page_id: page_a,
                split_dimensions: DimensionSet::from(Dimension::X),
            }),
            upper: Box::new(value_node(2)),
        };
        tree.file
            .write(page_a, Page::of(&*NodePage::encode(&node_a)))
            .await
            .unwrap();
        tree.file
            .write(page_b, Page::of(&*NodePage::encode(&node_b)))
            .await
            .unwrap();
        tree.roots = [Some(page_a), None];
        tree.commit().await.unwrap();

        assert!(is_corrupt(tree.compact().await));
        tree.close().await.unwrap();
    });
}

/// 乱択操作列で disk::FlexTreeCore と mem::FlexTreeCore の全要素・Z-order 順序が完全一致する。
#[test]
fn canonical_output_matches_mem_on_random_ops() {
    for seed in 0..4 {
        let mut rng = ChaCha8Rng::seed_from_u64(100 + seed);
        let file = TempFile::new();
        block_on(async {
            let mut disk = Tree::open(&file.0).await.unwrap();
            let mut mem = crate::collection::core::FlexTreeCore::<u64>::default();
            for _ in 0..120 {
                let target = random_target(&mut rng);
                let value = rng.random_range(0..4u64);
                match rng.random_range(0..3) {
                    0 => {
                        disk.insert(target, value).await.unwrap();
                        mem.insert(target, value);
                    }
                    1 => {
                        disk.upsert(target, value).await.unwrap();
                        mem.insert_with(target, value, |old, _| *old);
                    }
                    _ => {
                        disk.remove(target).await.unwrap();
                        mem.remove(target);
                    }
                }
            }
            let mut disk_all = disk.get(FlexId::UPPER_MAX).await.unwrap();
            disk_all.extend(disk.get(FlexId::LOWER_MAX).await.unwrap());
            let mem_all: Vec<(FlexId, u64)> = mem.iter().map(|(id, &v)| (id, v)).collect();
            assert_eq!(disk_all, mem_all);
            assert!(disk_all.windows(2).all(|w| w[0].0 < w[1].0));
            assert_ref_pages_never_point_to_skip(&disk).await;

            disk.compact().await.unwrap();
            disk.close().await.unwrap();
        });
    }
}

/// 片側だけ書き換えて RefPage と分かれた半分も、中身が元に戻ればメモリ版と同じく1つにまとまる。
#[test]
fn halves_merge_again_when_contents_become_equal() {
    let file = TempFile::new();
    block_on(async {
        let mut disk = Tree::open(&file.0).await.unwrap();
        let mut mem = crate::collection::core::FlexTreeCore::<u64>::default();
        let mut next = 1u64;
        for x in 0..32u32 {
            for y in 0..16u32 {
                let id = FlexId::new(0, 0, 5, x, 4, y).unwrap();
                disk.insert(id, next).await.unwrap();
                mem.insert(id, next);
                next += 1;
            }
        }
        disk.compact().await.unwrap();

        let assert_matches_mem =
            async |disk: &Tree, mem: &crate::collection::core::FlexTreeCore<u64>| {
                let disk_items = disk.get(FlexId::UPPER_MAX).await.unwrap();
                let mem_items: Vec<(FlexId, u64)> = mem.iter().map(|(id, &v)| (id, v)).collect();
                assert_eq!(disk_items, mem_items);
            };

        let lower = FlexId::new(1, 0, 5, 3, 4, 2).unwrap();
        let upper = FlexId::new(1, 1, 5, 3, 4, 2).unwrap();
        let original = disk.get(lower).await.unwrap()[0].1;
        for (target, value) in [(lower, 999), (lower, original), (upper, 999), (lower, 999)] {
            disk.insert(target, value).await.unwrap();
            mem.insert(target, value);
            assert_matches_mem(&disk, &mem).await;
        }
        assert_ref_pages_never_point_to_skip(&disk).await;
        disk.compact().await.unwrap();
        assert_matches_mem(&disk, &mem).await;
        disk.close().await.unwrap();
    });
}

/// 中身が Value だけのページを指す RefPage に同じ値を書いても、変更なしとして扱い、片側だけ割らない。
#[test]
fn inserting_same_value_over_value_page_changes_nothing() {
    let file = TempFile::new();
    block_on(async {
        let mut tree = Tree::open(&file.0).await.unwrap();
        let value_page = tree.store(value_node(5)).await.unwrap();
        let root = tree
            .store(Node::Branch {
                dimension: Dimension::X,
                split_dimensions: DimensionSet::from(Dimension::X),
                lower: Box::new(Node::RefPage {
                    page_id: value_page,
                    split_dimensions: DimensionSet::EMPTY,
                }),
                upper: Box::new(value_node(3)),
            })
            .await
            .unwrap();
        tree.roots = [Some(root), None];
        tree.commit().await.unwrap();

        let len_before = tree.file.len();
        tree.insert(FlexId::new(1, 0, 1, 0, 0, 0).unwrap(), 5)
            .await
            .unwrap();
        assert_eq!(tree.file.len(), len_before);
        assert_eq!(
            tree.get(FlexId::UPPER_MAX).await.unwrap(),
            [
                (FlexId::new(0, 0, 1, 0, 0, 0).unwrap(), 5),
                (FlexId::new(0, 0, 1, 1, 0, 0).unwrap(), 3),
            ]
        );
        tree.close().await.unwrap();
    });
}

/// ページへ出す子が Skip なら行き先だけを出し、ルート以外のページが Skip で始まらない。
#[test]
fn split_never_spills_skip_as_page_root() {
    let mut next = 0;
    let path = FlexId::new(0, 0, 1, 0, 1, 0)
        .unwrap()
        .relative_to(&FlexId::UPPER_MAX)
        .unwrap();
    let lower = Node::Skip {
        path,
        child: Box::new(balanced(9, &mut next)),
    };
    let upper = balanced(9, &mut next);
    let node = Node::Branch {
        dimension: Dimension::F,
        split_dimensions: DimensionSet::from(Dimension::F)
            | lower.split_dimensions()
            | upper.split_dimensions(),
        lower: Box::new(lower),
        upper: Box::new(upper),
    };
    let mut ids = 0;
    let (root_id, node_pages) = NodePage::split(node.clone(), || {
        ids += 1;
        PageId(ids)
    });
    let pages: HashMap<u64, Node> = node_pages
        .into_iter()
        .map(|(id, page)| (id.0, page.decode().unwrap()))
        .collect();
    assert!(
        pages
            .iter()
            .all(|(&id, node)| id == root_id.0 || !matches!(node, Node::Skip { .. }))
    );
    assert_eq!(resolve(pages[&root_id.0].clone(), &pages), node);
}
