use crate::{FlexId, RangeId, Source, SpatialIdMap};

/// 再現可能な疑似乱数（xorshift64）。
fn next(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

/// ズーム 3〜6 の混ざった表。粗い要素が細かい要素に合わせて木の中で割られる。
fn random_table(state: &mut u64) -> SpatialIdMap<u32> {
    (0..20)
        .map(|_| {
            let mut zoom = || 3 + (next(state) % 4) as u8;
            let (fz, xz, yz) = (zoom(), zoom(), zoom());
            let id = FlexId::new(
                fz,
                (next(state) % (2 << fz)) as i32 - (1 << fz),
                xz,
                (next(state) % (1 << xz)) as u32,
                yz,
                (next(state) % (1 << yz)) as u32,
            )
            .unwrap();
            (id, (next(state) % 4) as u32)
        })
        .collect()
}

/// 移動量の組。負の向き、X の周回、F・Y の範囲外を含む。
const DELTAS: [(i32, i32, i32); 6] = [
    (1, 0, 0),
    (0, -3, 0),
    (0, 0, 2),
    (2, 70, -1),
    (-5, -1, 3),
    (200, 0, 0),
];

#[test]
fn shift_fxy_matches_chained_shifts() {
    let mut state = 0x2545_F491_4F6C_DD1D;
    let mut compared = 0;
    for _ in 0..50 {
        let table = random_table(&mut state);
        for (f, x, y) in DELTAS {
            let fused = table
                .clone()
                .query()
                .shift_fxy(6, f, x, y)
                .get(..)
                .into_map();
            let chained = table
                .clone()
                .query()
                .shift_f(6, f)
                .shift_x(6, x)
                .shift_y(6, y)
                .get(..)
                .into_map();
            assert_eq!(fused.is_ok(), chained.is_ok(), "({f}, {x}, {y})");
            if let (Ok(fused), Ok(chained)) = (fused, chained) {
                assert_eq!(fused, chained, "({f}, {x}, {y})");
                compared += 1;
            }
        }
    }
    assert!(compared >= 100, "比較できたのが {compared} 件だけ");
}

#[test]
fn shift_fxy_partial_get_matches_whole() {
    let mut state = 0x9E37_79B9_7F4A_7C15;
    let target = RangeId::new(4, [-3, 5], [2, 9], [4, 12]).unwrap();
    let mut compared = 0;
    for _ in 0..50 {
        let table = random_table(&mut state);
        for (f, x, y) in DELTAS {
            let query = table.clone().query().shift_fxy(6, f, x, y);
            let Ok(expected) = table
                .clone()
                .query()
                .shift_fxy(6, f, x, y)
                .get(..)
                .into_map()
            else {
                continue;
            };
            let within = query.get(target).into_map().unwrap();
            let clip = |t: &SpatialIdMap<u32>| -> SpatialIdMap<u32> {
                t.get(target).map(|(id, v)| (id, *v)).collect()
            };
            assert_eq!(clip(&within), clip(&expected), "({f}, {x}, {y})");
            compared += usize::from(!clip(&expected).is_empty());
        }
    }
    assert!(compared >= 50, "空でない比較が {compared} 件だけ");
}

#[test]
fn shift_across_multiple_chunks_matches_count() {
    use crate::SingleId;

    // `CHUNK = 65536` を跨ぐ件数で `next_chunk` の再利用経路を通す
    let table: SpatialIdMap<u32> = (0..70_000u32)
        .map(|i| {
            let x = i >> 2;
            let y = i & 3;
            (SingleId::new(20, 0, x, y).unwrap(), i)
        })
        .collect();
    let shifted = table.query().shift_x(20, 1).get(..).into_map().unwrap();
    assert_eq!(shifted.count(), 70_000);
}
