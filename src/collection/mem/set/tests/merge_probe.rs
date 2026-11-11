//! マージ（圧縮）正当性の回帰テスト。
//!
//! 「merge できる FlexId が merge されない」漏れが無いことを、(1) 内部不変条件
//! （挿入し直した木と一致する）、(2) 再帰 collapse のカスケード、の2観点で
//! insert / union / intersection / difference にわたり検査する。
#![cfg(test)]

#[cfg(test)]
use alloc::vec::Vec;

use crate::collection::set::tests::arb_random_set_case;
use crate::{SingleId, SpatialIdMap, SpatialIdSet};
use proptest::prelude::*;

/// 道路（平面ストリップ）の X/Y 方向マージ対称性。
/// ガード付きの異方 collapse により、最深軸であれば X 方向ストリップも畳まれる。
#[test]
fn road_strip_xy_symmetry() {
    let z = 4u8;

    // Y 方向 2 TimeSegment → 1。
    let mut y_strip = SpatialIdSet::new();
    y_strip.insert(SingleId::new(z, 0, 0, 0).unwrap());
    y_strip.insert(SingleId::new(z, 0, 0, 1).unwrap());
    assert_eq!(y_strip.count(), 1, "Y-strip merges");

    // X 方向 2 TimeSegment → 最深 X の冗長分割が畳まれ 1（異方Segment x_zoom=z-1, y_zoom=z）。
    let mut x_strip = SpatialIdSet::new();
    x_strip.insert(SingleId::new(z, 0, 0, 0).unwrap());
    x_strip.insert(SingleId::new(z, 0, 1, 0).unwrap());
    assert_eq!(x_strip.count(), 1, "X-strip now merges (anisotropic)");

    // 2x2 ブロック → 1。
    let mut block = SpatialIdSet::new();
    for x in 0..2 {
        for y in 0..2 {
            block.insert(SingleId::new(z, 0, x, y).unwrap());
        }
    }
    assert_eq!(block.count(), 1, "2x2 block merges");

    // X 方向に長さ 8 のアライン道（Y=1Segment幅）→ 1 つの異方Segmentへ畳まれる。
    let mut long_x_road = SpatialIdSet::new();
    for x in 0..8 {
        long_x_road.insert(SingleId::new(z, 0, x, 0).unwrap());
    }
    assert_eq!(
        long_x_road.count(),
        1,
        "aligned X-road merges to a single anisotropic time_segment"
    );
}

/// フル充填の葉 collapse で max_zoom が減るケース。
/// zoom4 の 2×2×2 ブロック（f,x,y∈{0,1}）は zoom3 等方Segment（葉）へ畳まれ、
/// max_zoomlevel は 4→3 になる。
#[test]
fn f_collapse_reduces_max_zoom() {
    let mut set = SpatialIdSet::new();
    for f in 0..2 {
        for x in 0..2 {
            for y in 0..2 {
                set.insert(SingleId::new(4, f, x, y).unwrap());
            }
        }
    }
    assert_eq!(set.count(), 1, "2x2x2 block collapses to 1");
    assert_eq!(
        set.max_zoomlevel().map(u8::from),
        Some(3),
        "f-collapse must drop max_zoom 4 -> 3"
    );
}

/// Table（値あり）: 同値の Y 兄弟は merge して 1、異値は merge せず 2。
#[test]
fn table_merges_same_value_only() {
    let mut same = SpatialIdMap::<i32>::new();
    same.insert(SingleId::new(2, 0, 0, 0).unwrap(), 7);
    same.insert(SingleId::new(2, 0, 0, 1).unwrap(), 7);
    assert_eq!(same.count(), 1, "same-value y-siblings must merge");

    let mut diff = SpatialIdMap::<i32>::new();
    diff.insert(SingleId::new(2, 0, 0, 0).unwrap(), 7);
    diff.insert(SingleId::new(2, 0, 0, 1).unwrap(), 8);
    assert_eq!(
        diff.count(),
        2,
        "different-value time_segments must NOT merge"
    );
}

/// Table: 8 オクタントを同値で満たすと 1 葉へ collapse。異値が混じると collapse しない。
#[test]
fn table_full_cube_same_value_collapses() {
    let mut t = SpatialIdMap::<i32>::new();
    for f in 0..2 {
        for x in 0..2 {
            for y in 0..2 {
                t.insert(SingleId::new(1, f, x, y).unwrap(), 1);
            }
        }
    }
    assert_eq!(t.count(), 1);

    // 1 つだけ別値にすると collapse しない。
    t.insert(SingleId::new(1, 1, 1, 1).unwrap(), 2);
    assert!(t.count() > 1, "mixed values must not fully collapse");
}

/// 未適用のマージが残っていないこと。FlexTreeCore は形状に対して一意に定まるので、
/// 全Segmentを挿入し直した木と構造が一致するかで判定する。
fn set_is_fully_merged(set: &SpatialIdSet) -> bool {
    rebuilt(set) == *set
}

/// union(F下半分, F上半分) は完全立方体 → 1 葉へ collapse（演算をまたぐマージ）。
#[test]
fn union_of_f_halves_collapses() {
    let mut lower = SpatialIdSet::new();
    let mut upper = SpatialIdSet::new();
    for x in 0..2 {
        for y in 0..2 {
            lower.insert(SingleId::new(1, 0, x, y).unwrap());
            upper.insert(SingleId::new(1, 1, x, y).unwrap());
        }
    }
    // 各半分は (f z1, x z0, y z0) の 1 葉へ collapse 済みのはず。
    assert_eq!(lower.count(), 1);
    assert_eq!(upper.count(), 1);
    let u = &lower | &upper;
    assert_eq!(u.count(), 1, "union of F-halves must collapse to 1");
}

/// 深いズームの立方体を完全に満たすと、多段の collapse が再帰的に伝播して
/// 1 葉になる。zoom Z の等方立方体 = (2^Z)^3 TimeSegment、ツリー深さ 3*Z 段の cascade。
#[test]
fn deep_recursive_cascade_collapses() {
    for z in 1u8..=3 {
        let side: u32 = 1 << z; // 2^z
        let mut set = SpatialIdSet::new();
        for f in 0..side as i32 {
            for x in 0..side {
                for y in 0..side {
                    set.insert(SingleId::new(z, f, x, y).unwrap());
                }
            }
        }
        assert_eq!(
            set.count(),
            1,
            "zoom {z} full cube must cascade-collapse to 1"
        );
        assert!(set_is_fully_merged(&set));
    }
}

/// 最後の 1 Segmentを入れた瞬間に、全レベルの collapse が一気にカスケードすること。
/// 直前は複数葉、最後の insert 後に 1 葉。
#[test]
fn final_insert_triggers_full_cascade() {
    let z = 2u8;
    let side: u32 = 1 << z;
    let coords: Vec<(i32, u32, u32)> = (0..side as i32)
        .flat_map(|f| (0..side).flat_map(move |x| (0..side).map(move |y| (f, x, y))))
        .collect();

    let mut set = SpatialIdSet::new();
    for &(f, x, y) in &coords[..coords.len() - 1] {
        set.insert(SingleId::new(z, f, x, y).unwrap());
    }
    let before = set.count();
    assert!(
        before > 1,
        "before last insert should be multi-leaf, got {before}"
    );

    let (f, x, y) = coords[coords.len() - 1];
    set.insert(SingleId::new(z, f, x, y).unwrap());
    assert_eq!(set.count(), 1, "last insert must cascade-collapse to 1");
    assert!(set_is_fully_merged(&set));
}

/// 部分木（ルートではない深い位置）での再帰 collapse。
/// 1つの zoom2 Segmentの 8 つの zoom3 子を満たすと、その部分木だけが 1 葉に畳まれ、
/// 離れた別Segmentは別葉として残る（全体 count == 2）。
#[test]
fn recursive_collapse_in_subtree() {
    let mut set = SpatialIdSet::new();
    // (f,x,y)=(0,0,0) の zoom2 Segmentを、その zoom3 子 8 個で満たす。
    for f in 0..2 {
        for x in 0..2 {
            for y in 0..2 {
                set.insert(SingleId::new(3, f, x, y).unwrap());
            }
        }
    }
    // 別の zoom2 親に属する離れたSegment（zoom3 の有効範囲 0..=7）。
    set.insert(SingleId::new(3, 5, 5, 5).unwrap());

    assert_eq!(
        set.count(),
        2,
        "subtree collapses to 1 leaf, plus 1 distant leaf"
    );
    assert!(set_is_fully_merged(&set));
}

/// union で 2 つの大きな相補部分木を結合しても再帰 collapse する。
/// 偶数 f 半分と奇数 f 半分を別 Set で作り、union で zoom3 立方体を完成 → 1。
#[test]
fn union_recursive_collapse_deep() {
    let z = 3u8;
    let side: u32 = 1 << z;
    let mut a = SpatialIdSet::new();
    let mut b = SpatialIdSet::new();
    for f in 0..side as i32 {
        for x in 0..side {
            for y in 0..side {
                if f % 2 == 0 {
                    a.insert(SingleId::new(z, f, x, y).unwrap());
                } else {
                    b.insert(SingleId::new(z, f, x, y).unwrap());
                }
            }
        }
    }
    let u = &a | &b;
    assert_eq!(
        u.count(),
        1,
        "union must recursively collapse deep cube to 1"
    );
    assert!(set_is_fully_merged(&u));
}

/// 木の現在の内容を iter() で取り出し、新しい Set へ再挿入したものを返す。
fn rebuilt(set: &SpatialIdSet) -> SpatialIdSet {
    let mut out = SpatialIdSet::new();
    for flex_id in set.iter() {
        out.insert(flex_id);
    }
    out
}

proptest! {
    /// 内部不変条件: union/intersection/difference の結果にも未適用マージが無い。
    #[ignore]
    #[test]
    fn ops_no_unmerged(a in arb_random_set_case(), b in arb_random_set_case()) {
        let sa = a.build_set();
        let sb = b.build_set();
        prop_assert!(set_is_fully_merged(&(&sa | &sb)), "unmerged after union: a={} b={}", a.debug_summary(), b.debug_summary());
        prop_assert!(set_is_fully_merged(&(&sa & &sb)), "unmerged after intersection: a={} b={}", a.debug_summary(), b.debug_summary());
        prop_assert!(set_is_fully_merged(&(&sa - &sb)), "unmerged after difference: a={} b={}", a.debug_summary(), b.debug_summary());
    }
}

/// 挿入順を変えても最終的な count（マージ結果）は同じであるべき。
#[test]
fn insert_order_independence() {
    let coords: Vec<(i32, u32, u32)> = (0..2)
        .flat_map(|f| (0..2).flat_map(move |x| (0..2).map(move |y| (f, x, y))))
        .collect();

    let mut forward = SpatialIdSet::new();
    for &(f, x, y) in &coords {
        forward.insert(SingleId::new(1, f, x, y).unwrap());
    }

    let mut reverse = SpatialIdSet::new();
    for &(f, x, y) in coords.iter().rev() {
        reverse.insert(SingleId::new(1, f, x, y).unwrap());
    }

    assert_eq!(forward.count(), reverse.count(), "order changed count");
    assert_eq!(forward.count(), 1);
}

#[test]
fn verify_f_strip_coverage_preserved() {
    use crate::id::zoom_level::ZoomLevel;
    // corner_cases と同じ 10 個の F 隣接Segment（z=30, x=y=0）。
    let mut set = SpatialIdSet::new();
    let mut expected = std::collections::BTreeSet::new();
    for i in 0..10 {
        let id = SingleId::new(ZoomLevel::MAX.get(), ZoomLevel::MAX.f_max() - i, 0, 0).unwrap();
        set.insert(id);
        expected.insert((ZoomLevel::MAX.f_max() - i, 0u32, 0u32));
    }
    // マージ後 count は減るが、最細Segmentへ展開すると元の 10 Segmentと一致するはず（被覆不変）。
    let mut got = std::collections::BTreeSet::new();
    for (sid, _) in set.flat_single_ids(None).map(|s| (s, ())) {
        assert_eq!(sid.z(), ZoomLevel::MAX.get());
        got.insert((sid.f(), sid.x(), sid.y()));
    }
    assert_eq!(got, expected, "merge must preserve exact coverage");
}
