use crate::query::Query;
use crate::query::SafeValue;
use crate::query::cancellation::CancellationToken;
use crate::query::query_iter::QueryIter;
use crate::{Error, FlexId, RangeId, SpatialIdMap, SpatialIdSet};
use alloc::vec::Vec;

/// [`Query::get`] などの結果を順に返すイテレーター。
///
/// 検証や評価の開始に失敗すると、最初の要素として [`Err`] を返して終わる。
/// 読み出しや演算に失敗すると、その位置で [`Err`] を返す。
///
/// 返す要素は互いに素である。全体を評価したときと [`Query::get_overlapping`] の結果は FlexId の Z-order 列になるが、
/// [`Query::get`] で `targets` に切り取った結果は、`targets` の形によって順序が前後することがある。
pub struct QueryOutput<'a, V: SafeValue + 'static> {
    query: &'a Query<V>,
    targets: Vec<RangeId>,
    // 全体を評価するときは、範囲との交差も切り取りも要らない
    whole: bool,
    clip: bool,
    // `targets` の和。要素との共通部分を、互いに素な FlexId の Z-order 列で取り出すのに使う
    clipper: Option<SpatialIdSet>,
    // 切り取った断片を、返す順の逆に積んでおく
    pending: Vec<(FlexId, V)>,
    token: CancellationToken,
    stream: Option<QueryIter<'a, V>>,
    finished: bool,
}

impl<'a, V: SafeValue + 'static> QueryOutput<'a, V> {
    pub(super) fn new(
        query: &'a Query<V>,
        targets: Vec<RangeId>,
        token: &CancellationToken,
        clip: bool,
    ) -> Self {
        Self {
            query,
            whole: targets == [RangeId::WHOLE],
            targets,
            clip,
            clipper: None,
            pending: Vec::new(),
            token: token.clone(),
            stream: None,
            finished: false,
        }
    }

    /// 結果を [`SpatialIdMap`] にして返す。
    pub fn into_map(self) -> Result<SpatialIdMap<V>, Error> {
        self.collect()
    }

    /// 結果を [`SpatialIdSet`] にして返す。
    pub fn into_set(self) -> Result<SpatialIdSet, Error> {
        self.map(|res| res.map(|(id, _)| id)).collect()
    }

    fn start(&mut self) -> Result<QueryIter<'a, V>, Error> {
        self.query.validate()?;
        if self.clip && !self.whole {
            self.clipper = Some(self.targets.iter().copied().collect());
        }
        self.query.stream_within(self.targets.clone(), &self.token)
    }
}

impl<V: SafeValue + 'static> Iterator for QueryOutput<'_, V> {
    type Item = Result<(FlexId, V), Error>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.stream.is_none() {
            if self.finished {
                return None;
            }
            match self.start() {
                Ok(stream) => self.stream = Some(stream),
                Err(e) => {
                    self.finished = true;
                    return Some(Err(e));
                }
            }
        }
        loop {
            if let Some(piece) = self.pending.pop() {
                return Some(Ok(piece));
            }
            let (id, value) = match self.stream.as_mut()?.next()? {
                Ok(item) => item,
                Err(e) => return Some(Err(e)),
            };
            if self.whole {
                return Some(Ok((id, value)));
            }
            // 入力源や演算は範囲の外にはみ出した要素も返しうるので、範囲の中にある部分だけを残す
            match &self.clipper {
                None => {
                    if self.targets.iter().any(|t| id.intersects_range(t)) {
                        return Some(Ok((id, value)));
                    }
                }
                Some(clipper) => {
                    let start = self.pending.len();
                    self.pending
                        .extend(clipper.get(id).map(|piece| (piece, value.clone())));
                    self.pending[start..].reverse();
                }
            }
        }
    }
}
