use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::vec::Vec;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::collection::disk::io::{Page, PageFile};
use crate::error::DiskError;
use crate::id::dimension::DimensionSet;
use crate::{Dimension, Error, FlexId, Side};

use node::{Branch, Kind, Node, RawHeader, RawNode, Skip, common_descent, load_page, store};

pub(crate) mod node;
#[cfg(test)]
mod tests;

/// ディスク上の永続化された空間インデックス（FlexTreeCore）。
///
/// ファイルはページを末尾に追記していくだけで、書いたページは書き換えない（CoW）。
/// 各ページには、つながった部分木の Branch・LongSkip を詰める。変えた Node を含むページは、根までの経路ごと
/// 新しいページとして追記し、最後にルートを指すヘッダーのページを追記してコミットする。
/// 開くときは末尾から遡って最後のヘッダーを探す。使われなくなったページは [`compact`](Self::compact) で取り除く。
pub struct FlexTreeCore {
    file: PageFile,
    path: PathBuf,
    /// [`FlexId::ROOTS`] の各領域のルート。
    roots: [Option<Node>; 2],
}

impl FlexTreeCore {
    /// ファイルを開く。ファイルが存在しない場合は新規作成する。
    pub async fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        let path = path.as_ref().to_path_buf();
        let mut file = PageFile::open(&path).await?;
        if file.len() == 0 {
            let roots = commit(&mut file, [None, None]).await?;
            return Ok(FlexTreeCore { file, path, roots });
        }
        for index in (0..file.len()).rev() {
            if let Some(header) = RawHeader::parse(&*file.read(index).await?) {
                // ヘッダーより後ろは、コミットし終えなかったページ
                file.truncate(index + 1).await?;
                return Ok(FlexTreeCore {
                    roots: header.roots()?,
                    file,
                    path,
                });
            }
        }
        Err(DiskError::Corrupt.into())
    }

    /// `target` の領域に `value` を書き込む。既に値がある場所は上書きされる。
    pub async fn insert(
        &mut self,
        target: impl IntoIterator<Item = FlexId>,
        value: u64,
    ) -> Result<(), Error> {
        // 途中で失敗しても手元のルートが壊れないよう、複製に書き込んでから差し替える
        let mut roots = self.roots.clone();
        for id in target {
            let i = id.root_index();
            roots[i] = Some(
                self.write(FlexId::ROOTS[i], roots[i].take(), id, value)
                    .await?,
            );
        }
        self.roots = commit(&mut self.file, roots).await?;
        Ok(())
    }

    /// `target` を含む領域の値を返す。
    pub async fn get(&self, target: &FlexId) -> Result<Option<u64>, Error> {
        let root_idx = target.root_index();
        let mut stack = Vec::new();
        if let Some(root) = self.roots[root_idx].clone() {
            stack.push((root, FlexId::ROOTS[root_idx]));
        }
        while let Some((node, this)) = stack.pop() {
            if !this.intersects(target) {
                continue;
            }
            match node {
                Node::Value(value) => {
                    if this.contains(target) {
                        return Ok(Some(value));
                    }
                }
                Node::OnDisk { page, .. } => {
                    let node = self.read(page).await?;
                    stack.push((node, this));
                }
                Node::Branch(branch) => {
                    let dim = branch.dimension;
                    let upper_id = this.split_on(dim, Side::Upper).unwrap();
                    let lower_id = this.split_on(dim, Side::Lower).unwrap();
                    if upper_id.intersects(target) {
                        stack.push((branch.upper.clone(), upper_id));
                    }
                    if lower_id.intersects(target) {
                        stack.push((branch.lower.clone(), lower_id));
                    }
                }
                Node::LongSkip(skip) => {
                    let region = skip.path.to_absolute(&this)?;
                    if region.intersects(target) {
                        stack.push((skip.child.clone(), region));
                    }
                }
            }
        }
        Ok(None)
    }

    /// 今のルートから辿れるページだけを新しいファイルへ写し、元のファイルと置き換える。
    pub async fn compact(self) -> Result<Self, Error> {
        let temp = self.path.with_extension("compacting");
        // 前回の compact が途中で止まった残り
        match compio::fs::remove_file(&temp).await {
            Err(err) if err.kind() != std::io::ErrorKind::NotFound => return Err(err.into()),
            _ => {}
        }
        let mut dest = PageFile::open(&temp).await?;
        let mut moved = HashMap::new();
        let mut roots = [None, None];
        for (root, node) in roots.iter_mut().zip(self.roots.clone()) {
            if let Some(node) = node {
                *root = Some(self.relocate(&mut dest, node, &mut moved).await?);
            }
        }
        commit(&mut dest, roots).await?;
        let FlexTreeCore { file, path, .. } = self;
        file.close().await?;
        dest.close().await?;
        compio::fs::rename(&temp, &path).await?;
        FlexTreeCore::open(path).await
    }

    /// `node` から辿れるページを `dest` へ写した Node を返す。`moved` は写し終えたページの元と先の番号。
    async fn relocate(
        &self,
        dest: &mut PageFile,
        node: Node,
        moved: &mut HashMap<u64, RawNode>,
    ) -> Result<Node, Error> {
        Ok(match node {
            Node::Value(_) => node,
            Node::OnDisk { page, .. } => {
                let raw = match moved.get(&page) {
                    Some(raw) => *raw,
                    None => {
                        let loaded = self.read(page).await?;
                        let loaded = Box::pin(self.relocate(dest, loaded, moved)).await?;
                        let raw = store(dest, loaded).await?;
                        moved.insert(page, raw);
                        raw
                    }
                };
                Option::<Node>::try_from(raw)?.ok_or(DiskError::Corrupt)?
            }
            Node::Branch(branch) => {
                let lower = Box::pin(self.relocate(dest, branch.lower.clone(), moved)).await?;
                let upper = Box::pin(self.relocate(dest, branch.upper.clone(), moved)).await?;
                Branch::new(branch.dimension, lower, upper).into()
            }
            Node::LongSkip(skip) => {
                let child = Box::pin(self.relocate(dest, skip.child.clone(), moved)).await?;
                Skip::new(skip.path, child).into()
            }
        })
    }

    /// `page` 番目のページの部分木をメモリ上に展開する。
    async fn read(&self, page: u64) -> Result<Node, Error> {
        load_page(&self.file, page).await
    }

    /// `node` が `kind` の [`Node::OnDisk`] なら読み込む。
    async fn load(&self, node: Node, kind: Kind) -> Result<Node, Error> {
        match node {
            Node::OnDisk { page, kind: k, .. } if k == kind => self.read(page).await,
            node => Ok(node),
        }
    }

    /// 領域 `this` の `node` のうち、`region` を `value` で上書きした Node を返す。
    async fn write(
        &self,
        this: FlexId,
        node: Option<Node>,
        region: FlexId,
        value: u64,
    ) -> Result<Node, Error> {
        if region == this {
            return Ok(Node::Value(value));
        }
        let node = match node {
            None => {
                return Ok(Skip::new(region.relative_to(&this)?, Node::Value(value)).into());
            }
            Some(node) => match self.load(node, Kind::LongSkip).await? {
                Node::LongSkip(skip) => {
                    let target = skip.path.to_absolute(&this)?;
                    let at = common_descent(&this, &target, skip.child.split_dimensions(), &region);
                    if at == target {
                        let child =
                            Box::pin(self.write(at, Some(skip.child.clone()), region, value))
                                .await?;
                        return self.rebase(child, &at, &this).await;
                    }
                    if at != this {
                        let inner = self.rebase(Node::LongSkip(skip), &this, &at).await?;
                        let inner = Box::pin(self.write(at, Some(inner), region, value)).await?;
                        return self.rebase(inner, &at, &this).await;
                    }
                    Node::LongSkip(skip)
                }
                node => node,
            },
        };

        let heads = node
            .head_dimension(&this)
            .map_or(DimensionSet::EMPTY, DimensionSet::from)
            | region.finer_dimensions_than(&this);
        let dimension = this
            .coarsest_dimension_in(heads)
            .expect("region が this より狭いので割る次元がある");
        let halves = [Side::Lower, Side::Upper].map(|side| this.split_on(dimension, side).unwrap());

        let mut children = match self.load(node, Kind::Branch(dimension)).await? {
            Node::Branch(branch) if branch.dimension == dimension => {
                [Some(branch.lower.clone()), Some(branch.upper.clone())]
            }
            node => self.split(node, &this, dimension).await?,
        };
        for (child, half) in children.iter_mut().zip(halves) {
            if let Some(region) = half.intersection(&region) {
                *child = Some(Box::pin(self.write(half, child.take(), region, value)).await?);
            }
        }
        let [lower, upper] = children;
        self.join(&this, dimension, lower, upper)
            .await
            .map(|n| n.expect("書き込み後は空にならない"))
    }

    /// 領域 `this` の `node` を `dimension` で割った `[下, 上]`。
    async fn split(
        &self,
        node: Node,
        this: &FlexId,
        dimension: Dimension,
    ) -> Result<[Option<Node>; 2], Error> {
        let node = self.load(node, Kind::LongSkip).await?;
        if let Node::LongSkip(skip) = &node
            && skip.path.depth_on(dimension) > 0
        {
            let region = skip.path.to_absolute(this)?;
            let next = this.split_toward(dimension, &region).unwrap();
            let rest = self.rebase(node, this, &next).await?;
            return Ok(if next == this.split_on(dimension, Side::Upper).unwrap() {
                [None, Some(rest)]
            } else {
                [Some(rest), None]
            });
        }
        Ok([Some(node.clone()), Some(node)])
    }

    /// 領域 `this` を `dimension` で割った左右 `lower`,`upper` を、カノニカル形の1つの[Node]に合わせる。
    async fn join(
        &self,
        this: &FlexId,
        dimension: Dimension,
        lower: Option<Node>,
        upper: Option<Node>,
    ) -> Result<Option<Node>, Error> {
        let (lower, upper) = match (lower, upper) {
            (None, None) => return Ok(None),
            (None, Some(upper)) => {
                let upper_id = this.split_on(dimension, Side::Upper).unwrap();
                return Ok(Some(self.rebase(upper, &upper_id, this).await?));
            }
            (Some(lower), None) => {
                let lower_id = this.split_on(dimension, Side::Lower).unwrap();
                return Ok(Some(self.rebase(lower, &lower_id, this).await?));
            }
            (Some(lower), Some(upper)) => (lower, upper),
        };
        if !lower.split_dimensions().contains(dimension) && self.same(&lower, &upper).await? {
            return Ok(Some(lower));
        }
        Ok(Some(Branch::new(dimension, lower, upper).into()))
    }

    /// 領域 `from` から見た `node` を、領域 `to` から見た Node にする。
    async fn rebase(&self, node: Node, from: &FlexId, to: &FlexId) -> Result<Node, Error> {
        if from == to {
            return Ok(node);
        }
        Ok(match self.load(node, Kind::LongSkip).await? {
            Node::LongSkip(skip) => {
                let region = skip.path.to_absolute(from)?;
                if region == *to {
                    skip.child.clone()
                } else {
                    Skip::new(region.relative_to(to)?, skip.child.clone()).into()
                }
            }
            node => Skip::new(from.relative_to(to)?, node).into(),
        })
    }

    /// `a` と `b` が同じ中身なら true。
    async fn same(&self, a: &Node, b: &Node) -> Result<bool, Error> {
        if let (Node::OnDisk { page: a, .. }, Node::OnDisk { page: b, .. }) = (a, b)
            && a == b
        {
            return Ok(true);
        }
        if a.split_dimensions() != b.split_dimensions() {
            return Ok(false);
        }
        let (a, b) = (self.view(a).await?, self.view(b).await?);
        Ok(match (&*a, &*b) {
            (Node::Value(a), Node::Value(b)) => a == b,
            (Node::Branch(a), Node::Branch(b)) => {
                a.dimension == b.dimension
                    && Box::pin(self.same(&a.lower, &b.lower)).await?
                    && Box::pin(self.same(&a.upper, &b.upper)).await?
            }
            (Node::LongSkip(a), Node::LongSkip(b)) => {
                a.path == b.path && Box::pin(self.same(&a.child, &b.child)).await?
            }
            _ => false,
        })
    }

    /// `node` の中身を見られる形にする。
    async fn view<'a>(&self, node: &'a Node) -> Result<Cow<'a, Node>, Error> {
        Ok(match node {
            Node::OnDisk { page, .. } => Cow::Owned(self.read(*page).await?),
            node => Cow::Borrowed(node),
        })
    }
}

/// `roots` をページへ書き、それを指すヘッダーを追記してコミットする。書いた後のルートを返す。
///
/// ページを永続化してからヘッダーを書くので、読めるヘッダーが指すページはすべて書き終わっている。
async fn commit(file: &mut PageFile, roots: [Option<Node>; 2]) -> Result<[Option<Node>; 2], Error> {
    let mut raw_roots = [RawNode::EMPTY; 2];
    for (raw, root) in raw_roots.iter_mut().zip(roots) {
        if let Some(node) = root {
            *raw = store(file, node).await?;
        }
    }
    file.sync().await?;
    let header = RawHeader::new(raw_roots);
    file.append(Page::of(&header)).await?;
    file.sync().await?;
    header.roots()
}
