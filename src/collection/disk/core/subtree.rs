use crate::{
    Dimension, Error, FlexId, SpatialId,
    collection::disk::io::{Page, PageId},
    dimension::DimensionSet,
};

/// 1Pageに収まる部分木を表現した型
struct SubTree {
    inner: [u8; Page::CAPACITY],
}

/// [SubTree]にまたがる処理の1ステップを表現する
//  これを受け取った上位の[FlexTreeCore]型が次の指示を適切に次のSubTreeに出す
enum Step<V> {
    /// 処理が完了した
    /// 返す結果がある場合にはVに入る
    Ok(V),

    /// 処理の結果として[SubTree]に結果が入りきらなくなったので、複数のPageに分裂する
    Split(Vec<(PageId, SubTree)>),

    /// 子にInsertする
    //  PageIdは挿入を指示する[SubTree]が書いてある位置
    //  FlexIdとu64は書き込む位置と書き込む値
    Insert(Vec<(PageId, Vec<(FlexId, u64)>)>),

    /// 子からRemoveする
    //  PageIdは削除を指示する[SubTree]が書いてある位置
    //  FlexIdは削除する位置
    Remove(Vec<(PageId, Vec<FlexId>)>),

    /// 子から情報を取得する
    //  PageIdは読み取りを指示する[SubTree]が書いてある位置
    //  FlexIdは読み取る位置
    Get(Vec<(PageId, Vec<FlexId>)>),
}

impl SubTree {
    /// 空のページを作成する
    /// [SubTree]のrootは必ずBranchになるためそのBranchで切断する次元を作成時に入力する
    fn new(split_dimension: Dimension) -> Self {
        Self {
            // Todo:Branchを書く
            inner: [0u8; Page::CAPACITY],
        }
    }

    fn insert(&mut self, target: impl SpatialId, value: u64) -> Result<Step<()>, Error> {
        todo!()
    }

    fn remove(&mut self, target: impl SpatialId) -> Result<Step<u64>, Error> {
        todo!()
    }

    fn get(&self, target: impl SpatialId) -> Result<Step<Vec<(FlexId, u64)>>, Error> {
        todo!()
    }
}
