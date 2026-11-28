//! [`SpatialIdSet`](crate::collection::mem::SpatialIdSet) や [`SpatialIdMap`](crate::collection::mem::SpatialIdMap) などのデータを組み合わせて変換する [`Query`]。

mod cancellation;
mod execution;
pub mod merge_policy;
pub(crate) mod ops;
mod query_iter;
mod traits;

pub use crate::error::QueryError;
pub use cancellation::CancellationToken;
pub use execution::{IntoTargets, QueryOutput};
pub use ops::unary::falloff::FalloffPattern;
pub use query_iter::QueryIter;
pub(crate) use traits::{BinaryOperator, UnaryOperator};
pub use traits::{SafeValue, Source};

use crate::Error;
use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;

/// 空間IDのデータを組み合わせて変換するクエリ。
pub struct Query<V: SafeValue + 'static>(Node<V>);

enum Node<V: SafeValue + 'static> {
    /// 演算の起点
    Source(Box<dyn Source<Value = V>>),

    /// 連続した単項演算子
    Unary(Vec<Box<dyn UnaryOperator<V>>>, Box<Query<V>>),

    /// 二項演算
    Binary(Box<dyn BinaryOperator<V>>, Box<Query<V>>, Box<Query<V>>),

    /// エラー状態を保持
    Error(Error),
}

// Queryの構築
impl<V: SafeValue + 'static> Query<V> {
    /// `self` を単項演算子で包む。
    pub(crate) fn wrap_unary<O>(self, op: O) -> Self
    where
        O: UnaryOperator<V> + 'static,
    {
        match self {
            Query(Node::Unary(mut ops, input)) => {
                ops.push(Box::new(op));
                Query(Node::Unary(ops, input))
            }
            other => Query(Node::Unary(
                vec![Box::new(op) as Box<dyn UnaryOperator<V>>],
                Box::new(other),
            )),
        }
    }
}

/// [`Source`] を実装する型を、二項演算子の引数などで直接 [`Query`] として渡せるようにする。
impl<V: SafeValue + 'static, S: Source<Value = V> + 'static> From<S> for Query<V> {
    fn from(source: S) -> Self {
        source.query()
    }
}
