use super::Query;
use super::query_iter::QueryIter;
use super::traits::{Source, UnaryOperator};
use crate::query::Node;
use crate::query::SafeValue;
use crate::query::cancellation::CancellationToken;
use crate::{Error, QueryError, RangeId};
use alloc::vec::Vec;

mod output;
mod targets;
#[cfg(test)]
mod test;

pub use output::QueryOutput;
pub use targets::IntoTargets;

// Queryの検証
impl<V: SafeValue + 'static> Query<V> {
    /// 実行までに全てのQueryのパラメーターが正常値の範囲内か検証する。
    pub fn validate(&self) -> Result<(), Error> {
        match &self.0 {
            Node::Source(_) => Ok(()),
            Node::Unary(ops, input) => {
                input.validate()?;
                for op in ops {
                    op.validate()?;
                }
                Ok(())
            }
            Node::Binary(op, lhs, rhs) => {
                lhs.validate()?;
                rhs.validate()?;
                op.validate()
            }
            Node::Error(e) => Err(e.clone()),
        }
    }
}

// Queryの評価
impl<V: SafeValue + 'static> Query<V> {
    /// Query を評価し、`targets` の中にある部分を返す。
    ///
    /// 戻り値の [`QueryOutput`] は `Result<(FlexId, V), Error>` を返すイテレーターで、
    /// 評価は最初の要素を読むときに始まり、読んだ分だけ進む。
    /// まとめて [`SpatialIdMap`](crate::SpatialIdMap) で受け取るには [`QueryOutput::into_map`] を、[`SpatialIdSet`](crate::SpatialIdSet) で受け取るには [`QueryOutput::into_set`] を使う。
    ///
    /// # 評価する範囲の指定
    ///
    /// `targets` には [`IntoTargets`] を実装した値を渡す。
    /// 範囲を指定すると、その範囲の出力を得るのに必要な入力領域を各演算から逆算し、その部分だけを評価する。
    ///
    /// 全体を評価するには `..` を渡す。
    ///
    /// ```
    /// # use spatial_id::collection::SpatialIdMap;
    /// # use spatial_id::query::Source;
    /// # let table = SpatialIdMap::<u32>::new();
    /// let result = table
    ///     .query()
    ///     .filter_in(10..20)
    ///     .get(..)
    ///     .into_map()?;
    /// # Ok::<(), spatial_id::Error>(())
    /// ```
    ///
    /// 1つの範囲は、[`SingleId`](crate::SingleId)・[`RangeId`]・[`FlexId`](crate::FlexId) のどれでも渡せる。
    ///
    /// ```
    /// # use spatial_id::collection::SpatialIdMap;
    /// # use spatial_id::query::Source;
    /// # use spatial_id::SingleId;
    /// # let table = SpatialIdMap::<u32>::new();
    /// let result = table
    ///     .query()
    ///     .filter_in(10..20)
    ///     .get(SingleId::new(20, 0, 100, 0)?)
    ///     .into_map()?;
    /// # Ok::<(), spatial_id::Error>(())
    /// ```
    ///
    /// ```
    /// # use spatial_id::collection::SpatialIdMap;
    /// # use spatial_id::query::Source;
    /// # use spatial_id::RangeId;
    /// # let table = SpatialIdMap::<u32>::new();
    /// let result = table
    ///     .query()
    ///     .filter_in(10..20)
    ///     .get(RangeId::new(20, [0, 3], [100, 199], [0, 99])?)
    ///     .into_map()?;
    /// # Ok::<(), spatial_id::Error>(())
    /// ```
    ///
    /// ```
    /// # use spatial_id::collection::SpatialIdMap;
    /// # use spatial_id::query::Source;
    /// # use spatial_id::FlexId;
    /// # let table = SpatialIdMap::<u32>::new();
    /// let result = table
    ///     .query()
    ///     .filter_in(10..20)
    ///     .get(FlexId::new(20, 0, 18, 25, 18, 0)?)
    ///     .into_map()?;
    /// # Ok::<(), spatial_id::Error>(())
    /// ```
    ///
    /// 複数の範囲も指定できる。
    ///
    /// ```
    /// # use spatial_id::collection::SpatialIdMap;
    /// # use spatial_id::query::Source;
    /// # use spatial_id::SingleId;
    /// # let table = SpatialIdMap::<u32>::new();
    /// let a = SingleId::new(20, 0, 100, 0)?;
    /// let b = SingleId::new(20, 0, 101, 0)?;
    /// let result = table
    ///     .query()
    ///     .filter_in(10..20)
    ///     .get([a, b])
    ///     .into_map()?;
    /// # Ok::<(), spatial_id::Error>(())
    /// ```
    ///
    /// ```
    /// # use spatial_id::collection::SpatialIdMap;
    /// # use spatial_id::query::Source;
    /// # use spatial_id::SingleId;
    /// # let table = SpatialIdMap::<u32>::new();
    /// # let (a, b) = (SingleId::new(20, 0, 100, 0)?, SingleId::new(20, 0, 101, 0)?);
    /// let targets = vec![a, b];
    /// let result = table
    ///     .query()
    ///     .filter_in(10..20)
    ///     .get(targets)
    ///     .into_map()?;
    /// # Ok::<(), spatial_id::Error>(())
    /// ```
    ///
    /// ```
    /// # use spatial_id::collection::SpatialIdMap;
    /// # use spatial_id::query::Source;
    /// # use spatial_id::SingleId;
    /// # let table = SpatialIdMap::<u32>::new();
    /// # let targets = vec![SingleId::new(20, 0, 100, 0)?, SingleId::new(20, 0, 101, 0)?];
    /// let result = table
    ///     .query()
    ///     .filter_in(10..20)
    ///     .get(&targets[..])
    ///     .into_map()?;
    /// # Ok::<(), spatial_id::Error>(())
    /// ```
    ///
    /// 返す要素は `targets` で切り取る。`targets` が互いに重なっていても、同じ場所を2回返すことはない。
    /// 切り取った要素は互いに素だが、FlexId の Z-order 列になるとは限らない。
    /// 切り取らずに、`targets` と交差する要素をそのまま受け取るには [`get_overlapping`](Self::get_overlapping) を使う。
    ///
    /// # 結果の受け取り方
    ///
    /// [`SpatialIdMap`](crate::SpatialIdMap) で受け取る。
    ///
    /// ```
    /// # use spatial_id::collection::SpatialIdMap;
    /// # use spatial_id::query::Source;
    /// # let table = SpatialIdMap::<u32>::new();
    /// let result = table
    ///     .query()
    ///     .filter_in(10..20)
    ///     .get(..)
    ///     .into_map()?;
    /// # Ok::<(), spatial_id::Error>(())
    /// ```
    ///
    /// [`SpatialIdSet`](crate::SpatialIdSet) で受け取る。
    ///
    /// ```
    /// # use spatial_id::collection::SpatialIdMap;
    /// # use spatial_id::query::Source;
    /// # let table = SpatialIdMap::<u32>::new();
    /// let result = table
    ///     .query()
    ///     .filter_in(10..20)
    ///     .get(..)
    ///     .into_set()?;
    /// # Ok::<(), spatial_id::Error>(())
    /// ```
    ///
    /// イテレーターで受け取る。読んだ分だけ評価が進むので、途中で読むのをやめれば残りは評価しない。
    ///
    /// ```
    /// # use spatial_id::collection::SpatialIdMap;
    /// # use spatial_id::query::Source;
    /// # let table = SpatialIdMap::<u32>::new();
    /// for item in table.query().filter_in(10..20).get(..) {
    ///     let (id, value) = item?;
    /// }
    /// # Ok::<(), spatial_id::Error>(())
    /// ```
    ///
    /// # エラー
    ///
    /// - 演算のパラメーターの検証に失敗すると、最初の要素として [`Err`] を返して終わる。
    /// - 入力源の読み出しや演算に失敗すると、その位置で [`Err`] を返す。
    /// - [`QueryOutput::into_map`] や [`QueryOutput::into_set`] は、最初に出た [`Err`] を返す。
    ///
    /// # 借用
    ///
    /// [`QueryOutput`] は Query を借用する。1つの式や `for` 文の中で使うならそのまま書けるが、
    /// [`QueryOutput`] を変数に取っておくときは、先に Query を変数に束縛する。
    ///
    /// ```
    /// # use spatial_id::collection::SpatialIdMap;
    /// # use spatial_id::query::Source;
    /// # let table = SpatialIdMap::<u32>::new();
    /// let query = table.query().filter_in(10..20);
    /// let output = query.get(..);
    /// # Ok::<(), spatial_id::Error>(())
    /// ```
    ///
    /// 評価を途中で打ち切りたいときは [`get_cancellable`](Self::get_cancellable) を使う。
    pub fn get(&self, targets: impl IntoTargets) -> QueryOutput<'_, V> {
        self.get_cancellable(targets, &CancellationToken::never())
    }

    /// [`get`](Self::get) と同じく評価する。`token` がキャンセルされると、評価の途中で気づき次第 [`QueryError::Cancelled`] を返す。
    ///
    /// ```
    /// # use spatial_id::Error;
    /// # use spatial_id::query::QueryError;
    /// # use spatial_id::collection::SpatialIdMap;
    /// # use spatial_id::query::{CancellationToken, Source};
    /// # let table = SpatialIdMap::<u32>::new();
    /// let token = CancellationToken::new();
    /// token.cancel(); // 別のスレッドなどから呼ぶ
    ///
    /// let result = table
    ///     .query()
    ///     .filter_in(10..20)
    ///     .get_cancellable(.., &token)
    ///     .into_map();
    /// assert_eq!(result, Err(Error::Query(QueryError::Cancelled)));
    /// ```
    pub fn get_cancellable(
        &self,
        targets: impl IntoTargets,
        token: &CancellationToken,
    ) -> QueryOutput<'_, V> {
        self.evaluate(targets, token, true)
    }

    /// [`get`](Self::get) と同じく評価するが、返す要素を `targets` で切り取らない。
    ///
    /// `targets` と交差する要素をそのまま返すので、`targets` からはみ出した、より大きな [`FlexId`](crate::FlexId) が返ることがある。
    ///
    /// ```
    /// # use spatial_id::collection::SpatialIdMap;
    /// # use spatial_id::query::Source;
    /// # use spatial_id::SingleId;
    /// # let table = SpatialIdMap::<u32>::new();
    /// let result = table
    ///     .query()
    ///     .filter_in(10..20)
    ///     .get_overlapping(SingleId::new(20, 0, 100, 0)?)
    ///     .into_map()?;
    /// # Ok::<(), spatial_id::Error>(())
    /// ```
    pub fn get_overlapping(&self, targets: impl IntoTargets) -> QueryOutput<'_, V> {
        self.evaluate(targets, &CancellationToken::never(), false)
    }

    /// [`get_overlapping`](Self::get_overlapping) と同じく評価する。`token` がキャンセルされると、評価の途中で気づき次第 [`QueryError::Cancelled`] を返す。
    ///
    /// ```
    /// # use spatial_id::collection::SpatialIdMap;
    /// # use spatial_id::query::{CancellationToken, Source};
    /// # use spatial_id::SingleId;
    /// # let table = SpatialIdMap::<u32>::new();
    /// let token = CancellationToken::new();
    /// let result = table
    ///     .query()
    ///     .filter_in(10..20)
    ///     .get_overlapping_cancellable(SingleId::new(20, 0, 100, 0)?, &token)
    ///     .into_map()?;
    /// # Ok::<(), spatial_id::Error>(())
    /// ```
    pub fn get_overlapping_cancellable(
        &self,
        targets: impl IntoTargets,
        token: &CancellationToken,
    ) -> QueryOutput<'_, V> {
        self.evaluate(targets, token, false)
    }

    fn evaluate(
        &self,
        targets: impl IntoTargets,
        token: &CancellationToken,
        clip: bool,
    ) -> QueryOutput<'_, V> {
        QueryOutput::new(self, targets.into_targets(), token, clip)
    }
}

/// 単項演算の並びを順に適用する。
pub(crate) fn run_unary_chain<'a, V: SafeValue + 'static>(
    ops: &'a [impl core::borrow::Borrow<dyn UnaryOperator<V>>],
    mut stream: QueryIter<'a, V>,
    token: &CancellationToken,
) -> Result<QueryIter<'a, V>, Error> {
    for op in ops {
        if token.is_cancelled() {
            return Err(Error::Query(QueryError::Cancelled));
        }

        stream = op.borrow().run(stream)?;
    }
    Ok(stream)
}

/// 入力源から `bounds` と重なる要素を読み出し、読みながら `token` のキャンセルを確かめる列を返す。
fn read_source<'a, V: SafeValue + 'static>(
    source: &'a dyn Source<Value = V>,
    bounds: &[RangeId],
    token: &CancellationToken,
) -> Result<QueryIter<'a, V>, Error> {
    let token = token.clone();
    let mut counter = 0u32;
    Ok(QueryIter::new(source.read_flex_ids(bounds, &token)?.map(
        move |item| {
            token.check_amortized(&mut counter)?;
            item
        },
    )))
}

impl<V: SafeValue + 'static> Query<V> {
    /// 検証を行わずに、出力領域 `bounds` を得るのに必要な部分だけを評価した列を返す。
    ///
    /// `token` がキャンセルされると、AST を辿る途中か列を読む途中で [`QueryError::Cancelled`] を返す。
    pub(crate) fn stream_within<'a>(
        &'a self,
        bounds: Vec<RangeId>,
        token: &CancellationToken,
    ) -> Result<QueryIter<'a, V>, Error> {
        if token.is_cancelled() {
            return Err(Error::Query(QueryError::Cancelled));
        }
        if bounds.is_empty() {
            return Ok(QueryIter::new(core::iter::empty()));
        }

        match &self.0 {
            Node::Source(s) => read_source(&**s, &bounds, token),
            Node::Unary(ops, input) => {
                let mut req = bounds;
                for op in ops.iter().rev() {
                    let mut next = Vec::with_capacity(req.len());
                    for r in req {
                        op.reverse_target(r, &mut next);
                    }
                    next.sort_unstable();
                    next.dedup();
                    req = next;
                    if req.is_empty() {
                        return Ok(QueryIter::new(core::iter::empty()));
                    }
                }
                let input_stream = input.stream_within(req, token)?;
                run_unary_chain(ops, input_stream, token)
            }
            Node::Binary(op, lhs, rhs) => {
                let lhs_bounds = bounds.clone();
                #[cfg(feature = "rayon")]
                let (l, r) = rayon::join(
                    || lhs.stream_within(lhs_bounds, token),
                    || rhs.stream_within(bounds, token),
                );
                #[cfg(not(feature = "rayon"))]
                let (l, r) = (
                    lhs.stream_within(lhs_bounds, token),
                    rhs.stream_within(bounds, token),
                );
                op.run(l?, r?)
            }
            Node::Error(e) => Err(e.clone()),
        }
    }
}
