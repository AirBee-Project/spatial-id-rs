use crate::ZoomLevel;
use crate::query::merge_policy::Sum;
use crate::query::traits::UnaryOperator;

use super::{Extrude, ExtrudeAxis};

/// `bounds` のX軸が折り返し（`x[0] > x[1]`）の場合でも、固定範囲との重なり判定が
/// 正しく行われることを確認する。
///
/// `bounds.x = [5, 2]`（`z = 3`、`{5, 6, 7, 0, 1, 2}` を表す折り返し範囲）は `[1, 1]` と
/// `x = 1` で重なるため、`out` に逆算範囲が追加される。
#[test]
fn extrude_x_reverse_target_overlaps_wrapped_bounds() {
    let op = Extrude::<Sum>::new(ZoomLevel::new(3).unwrap(), ExtrudeAxis::X(1, 1));

    let mut bounds = crate::RangeId::new(3, 0, 0, 0).unwrap();
    bounds.set_x([5, 2]).unwrap();

    let mut out = alloc::vec::Vec::new();
    <Extrude<Sum> as UnaryOperator<u32>>::reverse_target(&op, bounds, &mut out);
    assert!(
        !out.is_empty(),
        "折り返した bounds との重なりを見落としている"
    );
}

#[test]
fn extrude_x_wrapped_emits_both_boundary_sides() {
    use crate::{SingleId, Source, SpatialIdMap};

    let mut table = SpatialIdMap::new();
    table.insert(SingleId::new(3, 0, 0, 0).unwrap(), 10u32);

    let out = table
        .query()
        .extrude_x(3, 6, 1, Sum)
        .get(..)
        .into_map()
        .unwrap();

    let xs: alloc::collections::BTreeSet<u32> =
        out.flat_single_ids(None).map(|(sid, _)| sid.x()).collect();
    assert_eq!(xs, alloc::collections::BTreeSet::from([0, 1, 6, 7]));
}
