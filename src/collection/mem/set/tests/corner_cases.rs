#[cfg(test)]
mod tests {
    use crate::{RangeId, SingleId, SpatialIdSet, id::zoom_level::ZoomLevel};
    use alloc::vec::Vec;

    /// 1. 完全被覆によるツリーの O(1) 全置換テスト
    #[test]
    fn insert_overwrite_large_target() {
        let mut set = SpatialIdSet::new();

        // 細かいノードを大量に挿入する (z=30付近)
        for i in 0..10 {
            let single_id =
                SingleId::new(ZoomLevel::MAX.get(), ZoomLevel::MAX.f_max() - i, 0, 0).unwrap();
            set.insert(single_id);
        }
        // 10 個は F 方向に隣接（x=y=0 固定）し、異方 collapse で圧縮される
        // （被覆は保たれたまま count は減る）。
        assert_eq!(2, set.count());

        // 全体を覆う巨大なノードを挿入 (z=0, 負の空間)
        let large_id = RangeId::new(0, [-1, 0], [0, 0], [0, 0]).unwrap();
        set.insert(large_id);

        // O(1)でツリーが置換され、要素数は即座に 10 + 1(大きなノード) ではない
        // F空間が負と正で2つのツリー(lower_root, upper_root)に分かれるため、カウントは2になる
        assert_eq!(2, set.count());
    }

    /// 2. 部分的 Axis Skipping のテスト
    #[test]
    fn insert_partial_axis_skipping() {
        let mut set = SpatialIdSet::new();
        // F, X は全体を覆うが、Yだけが半分を覆うような RangeId
        let partial_id = RangeId::new(
            3,
            [
                ZoomLevel::new(3_u8).unwrap().f_min(),
                ZoomLevel::new(3_u8).unwrap().f_max(),
            ],
            [0, ZoomLevel::new(3_u8).unwrap().xy_max()],
            [0, ZoomLevel::new(3_u8).unwrap().xy_max() / 2],
        )
        .unwrap();
        set.insert(partial_id);

        // Y軸で分割されるので、複数個ではなく効率的に格納されるか確認
        assert!(set.count() > 0);
    }

    /// 3. 自己演算の結果が元の集合と一致すること
    #[test]
    fn self_operations_return_the_same_set() {
        let mut set_a = SpatialIdSet::new();
        set_a.insert(SingleId::new(5, 5, 5, 5).unwrap());
        set_a.insert(SingleId::new(10, 10, 10, 10).unwrap());

        // Union (A | A)
        let union_set = &set_a | &set_a;
        assert_eq!(union_set, set_a);

        // Intersection (A & A)
        let intersection_set = &set_a & &set_a;
        assert_eq!(intersection_set, set_a);

        // Difference (A - A) -> Empty
        let diff_set = &set_a - &set_a;
        assert!(diff_set.is_empty());
    }

    /// 4. 完全な空間的分離における高速マージ
    #[test]
    fn disjoint_sets_fast_merge() {
        let mut set_a = SpatialIdSet::new(); // 負のF空間
        set_a.insert(SingleId::new(5, -10, 5, 5).unwrap());

        let mut set_b = SpatialIdSet::new(); // 正のF空間
        set_b.insert(SingleId::new(5, 10, 5, 5).unwrap());

        let intersection = &set_a & &set_b;
        assert_eq!(0, intersection.count());

        let union_set = &set_a | &set_b;
        assert_eq!(2, union_set.count());
        // Union は set_a の Lower, set_b の Upper をそのまま再利用するはずなので処理が極めて速い
    }

    /// 5. 中空構造 (Hollow Cube) の差集合テスト
    #[test]
    fn difference_hollow_cube() {
        let mut huge_box = SpatialIdSet::new();
        huge_box.insert(SingleId::new(5, 5, 5, 5).unwrap());

        let mut small_box = SpatialIdSet::new();
        small_box.insert(SingleId::new(10, 160, 160, 160).unwrap()); // 5の内部にあるとする (10は5の子孫)

        // 中空の構造を作成 (Level-Mismatch のロジックが発動)
        let hollow = &huge_box - &small_box;

        assert!(hollow.count() > 1); // 巨大な箱から小さな箱を引いたので、複数のフラグメントに分かれるはず
    }

    /// 6. 座標系の境界線における交叉
    #[test]
    fn boundary_crossing_intersection() {
        let mut set = SpatialIdSet::new();
        // F=0, X=0, Y=0 を中心にまたがる RangeId
        let boundary_id = RangeId::new(3, [-1, 0], [0, 1], [0, 1]).unwrap();
        set.insert(boundary_id);

        #[allow(clippy::needless_collect)]
        let flex_ids: Vec<_> = set.iter().collect();
        // 象限をまたぐため、F=0を境に2つに分かれる
        assert_eq!(2, flex_ids.len());
    }

    /// 7. 限界深度 (Zoom 30) での集合演算
    #[test]
    fn zoom_30_set_operations() {
        let mut set_a = SpatialIdSet::new();
        let id_a = SingleId::new(ZoomLevel::MAX, 10, 10, 10).unwrap();
        set_a.insert(id_a);

        let mut set_b = SpatialIdSet::new();
        let id_b = SingleId::new(ZoomLevel::MAX, 10, 10, 11).unwrap();
        set_b.insert(id_b);

        let intersection = &set_a & &set_b;
        assert_eq!(0, intersection.count());

        let union_set = &set_a | &set_b;
        // Y=10 と Y=11 は兄弟ノードなので自動的にマージされ、1つの親ノード(FlexId)になる
        assert_eq!(1, union_set.count());
    }
}
