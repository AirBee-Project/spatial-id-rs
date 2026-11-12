use triomphe::Arc;

use crate::Dimension;
use crate::id::dimension::DimensionSet;
use crate::id::relative_flex_id::RelativeFlexId;

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Node {
    Value(u64),
    Branch(Arc<Branch>),
    LongSkip(Arc<Skip>),
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Branch {
    pub(super) dimension: Dimension,
    pub(super) split_dimensions: DimensionSet,
    pub(super) lower: Node,
    pub(super) upper: Node,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Skip {
    pub(super) path: RelativeFlexId,
    pub(super) split_dimensions: DimensionSet,
    pub(super) child: Node,
}
