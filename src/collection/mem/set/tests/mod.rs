#![cfg_attr(test, allow(dead_code))]

#[cfg(test)]
use alloc::string::String;
#[cfg(test)]
use alloc::vec::Vec;

pub(crate) mod corner_cases;
pub(crate) mod difference;
pub(crate) mod equal;
pub(crate) mod flex;
pub(crate) mod insert;
pub(crate) mod intersection;
pub(crate) mod merge_probe;
pub(crate) mod union;

#[cfg(test)]
use crate::ZoomLevel;
#[cfg(test)]
use crate::{RangeId, SingleId, SpatialIdSet};
#[cfg(test)]
use hashbrown::HashSet;
#[cfg(test)]
use proptest::prelude::*;

#[cfg(test)]
/// ランダム生成時に使うズームレベルの下限。
const RANDOM_SET_MIN_ZOOM: u8 = 0;
#[cfg(test)]
/// ランダム生成時に使うズームレベルの上限。
const RANDOM_SET_MAX_ZOOM: u8 = 4;
#[cfg(test)]
/// 1ケースあたりの最小挿入回数。
const RANDOM_SET_MIN_INSERTS: usize = 1;
#[cfg(test)]
/// 1ケースあたりの最大挿入回数。
const RANDOM_SET_MAX_INSERTS: usize = 4;
#[cfg(test)]
/// RangeId 生成時に許可する F 方向の長さの最大値。
const RANDOM_SET_MAX_RANGE_SPAN_F: i32 = 6;
#[cfg(test)]
/// RangeId 生成時に許可する X/Y 方向の長さの最大値。
const RANDOM_SET_MAX_RANGE_SPAN_XY: u32 = 6;

/// テスト用のランダム Set 生成で使う挿入パターン。
#[cfg(test)]
#[derive(Clone, Debug)]
pub(crate) enum RandomSetInsert {
    Single(SingleId),
    Range(RangeId),
}

#[cfg(test)]
impl RandomSetInsert {
    fn insert_into(&self, set: &mut SpatialIdSet) {
        match self {
            RandomSetInsert::Single(single_id) => set.insert(*single_id),
            RandomSetInsert::Range(range_id) => set.insert(*range_id),
        }
    }

    fn estimated_single_count(&self) -> usize {
        match self {
            RandomSetInsert::Single(_) => 1,
            RandomSetInsert::Range(range_id) => {
                let f = range_id.f();
                let x = range_id.x();
                let y = range_id.y();

                let f_len = (f[1] - f[0] + 1) as usize;
                let x_len = (x[1] - x[0] + 1) as usize;
                let y_len = (y[1] - y[0] + 1) as usize;

                f_len.saturating_mul(x_len).saturating_mul(y_len)
            }
        }
    }
}

/// 演算子テスト向けのランダム Set ケース。
///
/// # 使い方
/// ランダムな [`SpatialIdSet`] が必要なときは、まず [`arb_random_set_case`] で
/// `RandomSetCase` を生成し、`build_set()` で `SpatialIdSet` を構築する。
///
/// ```ignore
/// # use spatial_id::collection::SpatialIdSet;
/// # use proptest::prelude::*;
/// # use spatial_id::collection::set::test::arb_random_set_case;
/// proptest! {
///     #[test]
///     fn random_set_example(case in arb_random_set_case()) {
///         let set: SpatialIdSet = case.build_set();
///
///         // 例: 空でないことを期待する（失敗時は case の内容で再現しやすい）
///         prop_assert!(!set.is_empty(), "{}", case.debug_summary());
///     }
/// }
/// ```
#[cfg(test)]
#[derive(Clone, Debug)]
pub(crate) struct RandomSetCase {
    pub inserts: Vec<RandomSetInsert>,
    pub estimated_single_count: usize,
}

#[cfg(test)]
impl RandomSetCase {
    /// ケースから [`SpatialIdSet`] を構築する。
    pub fn build_set(&self) -> SpatialIdSet {
        let mut set = SpatialIdSet::new();
        for insert in &self.inserts {
            insert.insert_into(&mut set);
        }
        set
    }

    /// 失敗時ログ向けの要約文字列。
    pub fn debug_summary(&self) -> String {
        format!(
            "insert_count={}, estimated_single_count={}, inserts={:#?}",
            self.inserts.len(),
            self.estimated_single_count,
            self.inserts
        )
    }
}

#[cfg(test)]
fn arb_compact_range_id(max_zoom: u8) -> impl Strategy<Value = RangeId> {
    (RANDOM_SET_MIN_ZOOM..=max_zoom).prop_flat_map(|z| {
        let idx = z as usize;

        let f_min = ZoomLevel::new(idx as u8).unwrap().f_min();
        let f_max = ZoomLevel::new(idx as u8).unwrap().f_max();
        let xy_max = ZoomLevel::new(idx as u8).unwrap().xy_max();

        let span_f_max = (f_max - f_min).clamp(0, RANDOM_SET_MAX_RANGE_SPAN_F) as u32;
        let span_xy_max = xy_max.min(RANDOM_SET_MAX_RANGE_SPAN_XY);

        (
            Just(z),
            f_min..=f_max,
            0..=span_f_max,
            0..=xy_max,
            0..=span_xy_max,
            0..=xy_max,
            0..=span_xy_max,
        )
            .prop_map(
                move |(z, f_start, f_span, x_start, x_span, y_start, y_span)| {
                    let idx = z as usize;
                    let f_end =
                        (f_start + f_span as i32).min(ZoomLevel::new(idx as u8).unwrap().f_max());
                    let x_end = x_start
                        .saturating_add(x_span)
                        .min(ZoomLevel::new(idx as u8).unwrap().xy_max());
                    let y_end = y_start
                        .saturating_add(y_span)
                        .min(ZoomLevel::new(idx as u8).unwrap().xy_max());

                    RangeId::new(z, [f_start, f_end], [x_start, x_end], [y_start, y_end])
                        .expect("Generated compact range must be valid")
                },
            )
    })
}

/// 演算子テスト向けに、処理可能なサイズに抑えたランダム Set ケースを生成する戦略である。
///
/// - ズームレベルは `RANDOM_SET_MIN_ZOOM..=RANDOM_SET_MAX_ZOOM` でランダム
/// - 挿入回数は `RANDOM_SET_MIN_INSERTS..=RANDOM_SET_MAX_INSERTS` でランダム
/// - `SingleId` と小さな `RangeId` を混ぜて生成
/// - 失敗時は [`RandomSetCase`] の `Debug` 出力で再現しやすい
///
/// # どれを使えばよいか
/// ランダムな Set を作る入口には、この [`arb_random_set_case`] を使う。
/// `RandomSetInsert` や `insert_into` は内部部品であり、通常は直接使う必要はない。
///
/// # テストテンプレート
/// ```
/// # use spatial_id::collection::set::test::arb_random_set_case;
/// # use proptest::prelude::*;
/// proptest! {
///     fn operator_test_template(case in arb_random_set_case()) {
///         let set = case.build_set();
///
///         // ここで演算子テストを書く
///         // 失敗時に case.debug_summary() をメッセージへ出すと追跡しやすい
///         prop_assert!(set.count() > 0, "{}", case.debug_summary());
///     }
/// }
/// ```
#[cfg(test)]
pub(crate) fn arb_random_set_case() -> impl Strategy<Value = RandomSetCase> {
    let single = SingleId::arb_within(RANDOM_SET_MIN_ZOOM..=RANDOM_SET_MAX_ZOOM)
        .prop_map(RandomSetInsert::Single);
    let range = arb_compact_range_id(RANDOM_SET_MAX_ZOOM).prop_map(RandomSetInsert::Range);

    let insert = prop_oneof![
        // 単一ID多め。全体の展開サイズを抑えつつ多様性を確保する。
        4 => single,
        1 => range,
    ];

    (RANDOM_SET_MIN_INSERTS..=RANDOM_SET_MAX_INSERTS)
        .prop_flat_map(move |insert_count| prop::collection::vec(insert.clone(), insert_count))
        .prop_map(|inserts| {
            let estimated_single_count = inserts
                .iter()
                .map(RandomSetInsert::estimated_single_count)
                .sum();

            RandomSetCase {
                inserts,
                estimated_single_count,
            }
        })
}

/// [`SpatialIdSet`] を指定したズームレベルの [`SingleId`] 集合へ分解する。
///
/// `target_z` は対象 Set の各 ID のズームレベル以上を指定する必要がある。
#[cfg(test)]
pub(crate) fn decompose_set_to_single_ids_at_zoom(
    set: &SpatialIdSet,
    target_z: u8,
) -> HashSet<SingleId> {
    let mut normalized = HashSet::new();

    for flex_id in set.iter_flex_ids() {
        let range = RangeId::from(&flex_id);
        let expanded = if range.z() == target_z {
            range
        } else {
            range
                .spatial_children_at_zoom(target_z)
                .expect("target_z must be >= range.z")
        };

        for single_id in expanded.single_ids() {
            normalized.insert(single_id);
        }
    }

    normalized
}

#[cfg(test)]
/// [`SpatialIdSet`] を指定したズームレベルで [`SingleId`] に分解し、昇順に並べて返す。
///
/// 比較系テストで結果同値性を判定しやすくするための補助関数である。
/// `target_z` は対象 Set 内の各 ID のズームレベル以上を指定する必要がある。
pub(crate) fn sorted_single_ids(set: &SpatialIdSet, target_z: u8) -> Vec<SingleId> {
    let mut ids: Vec<SingleId> = decompose_set_to_single_ids_at_zoom(set, target_z)
        .into_iter()
        .collect();
    ids.sort();
    ids
}
