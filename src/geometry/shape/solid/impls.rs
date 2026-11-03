use alloc::vec::Vec;

use alloc::collections::VecDeque;
use hashbrown::HashSet;

use crate::{
    Coordinate, Error, ExpandCoordinates, RangeId, Shape, SingleId, Solid, SpatialId, SpatialIdMap,
    geometry::traits::{CoverRangeIds, CoverSingleIds},
};

impl Shape for Solid {
    fn center(&self) -> Coordinate {
        Coordinate::centroid(self.expand_coordinates())
    }
}

/// 立体（左）を、それが覆う空間ID（右）へ変換する。図はズームレベル 21 の例。
///
/// ![立体と、それが覆う空間ID][solid_cover]
#[doc = doc_image!("solid_cover", "docs/images/solid_cover.jpg")]
impl CoverSingleIds for Solid {
    type Value<'a> = ();

    fn cover_single_ids_with<'a, V, F>(
        &'a self,
        z: impl Into<u8>,
        mut f: F,
    ) -> Result<impl Iterator<Item = (SingleId, V)> + 'a, Error>
    where
        V: Clone + 'a,
        F: FnMut(Self::Value<'a>) -> V + 'a,
    {
        let value = f(());
        let attach = move |id| (id, value.clone());
        let z: u8 = z.into();
        let surface_set: HashSet<SingleId> = self.surface_single_ids(z)?.collect();

        if surface_set.is_empty() {
            return Ok(HashSet::new().into_iter().map(attach));
        }

        let existence_range = surface_set.iter().fold(None, |acc, s| {
            match acc {
                None => Some((s.f(), s.x(), s.y(), s.f(), s.x(), s.y())), // 最初の要素を初期値にする
                Some((min_f, min_x, min_y, max_f, max_x, max_y)) => Some((
                    min_f.min(s.f()),
                    min_x.min(s.x()),
                    min_y.min(s.y()),
                    max_f.max(s.f()),
                    max_x.max(s.x()),
                    max_y.max(s.y()),
                )),
            }
        });
        let result = existence_range.unwrap();
        let mut cuboid_set: HashSet<SingleId> = RangeId::new(
            z,
            [result.0, result.3],
            [result.1, result.4],
            [result.2, result.5],
        )?
        .single_ids()
        .collect();
        let mut open_list: VecDeque<SingleId> = VecDeque::new();

        cuboid_set.retain(|id: &SingleId| {
            let is_boundary = id.f() == result.0
                || id.f() == result.3
                || id.x() == result.1
                || id.x() == result.4
                || id.y() == result.2
                || id.y() == result.5;

            if is_boundary && !surface_set.contains(id) {
                open_list.push_back(*id);
                false
            } else {
                true
            }
        });
        let directions = [
            (1, 0, 0),
            (-1, 0, 0),
            (0, 1, 0),
            (0, -1, 0),
            (0, 0, 1),
            (0, 0, -1),
        ];
        while let Some(current) = open_list.pop_front() {
            for (df, dx, dy) in directions {
                let mut neighbor = current;

                // 各方向に移動を試みる
                // move_x は循環するため Ok 固定、move_f と move_y は範囲外ならエラーになる
                let move_result = if df != 0 {
                    neighbor.move_f(df)
                } else if dx != 0 {
                    neighbor.move_x(dx);
                    Ok(())
                } else {
                    neighbor.move_y(dy)
                };

                // 移動が成功（範囲内）した場合のみ判定
                if move_result.is_ok() {
                    // 条件：cuboidに含まれる かつ surfaceに含まれない
                    if cuboid_set.contains(&neighbor) && !surface_set.contains(&neighbor) {
                        cuboid_set.remove(&neighbor);
                        open_list.push_back(neighbor);
                    }
                }
            }
        }
        Ok(cuboid_set.into_iter().map(attach))
    }

    fn cover_map_with<'a, V, F>(&'a self, z: impl Into<u8>, f: F) -> Result<SpatialIdMap<V>, Error>
    where
        V: PartialEq + Clone + Send + Sync + 'a,
        F: FnMut(Self::Value<'a>) -> V + 'a,
    {
        // SingleId へ展開せず RangeId のままツリーを構築して負荷を抑える。
        let mut map = SpatialIdMap::new();
        for (id, value) in self.cover_range_ids_with(z, f)? {
            map.insert(id, value);
        }
        Ok(map)
    }
}

/// 立体（左）を、それが覆う [`RangeId`](crate::RangeId)（右）へ変換する。図はズームレベル 21 の例。
///
/// ![立体と、それが覆う範囲ID][solid_cover_range]
#[doc = doc_image!("solid_cover_range", "docs/images/solid_cover_range.jpg")]
impl CoverRangeIds for Solid {
    fn cover_range_ids_with<'a, V, F>(
        &'a self,
        z: impl Into<u8>,
        mut f: F,
    ) -> Result<impl Iterator<Item = (RangeId, V)> + 'a, Error>
    where
        V: Clone + 'a,
        F: FnMut(Self::Value<'a>) -> V + 'a,
    {
        let value = f(());
        let attach = move |id| (id, value.clone());
        let z: u8 = z.into();
        let surface_set: HashSet<SingleId> = self.surface_single_ids(z)?.collect();
        if surface_set.is_empty() {
            return Ok(Vec::new().into_iter().map(attach));
        }

        let first = surface_set.iter().next().unwrap();
        let mut min_f = first.f();
        let mut max_f = first.f();
        let mut min_x = first.x();
        let mut max_x = first.x();
        let mut min_y = first.y();
        let mut max_y = first.y();

        let mut surface_coords = HashSet::with_capacity(surface_set.len());
        for s in &surface_set {
            let (f, x, y) = (s.f(), s.x(), s.y());
            min_f = min_f.min(f);
            max_f = max_f.max(f);
            min_x = min_x.min(x);
            max_x = max_x.max(x);
            min_y = min_y.min(y);
            max_y = max_y.max(y);
            surface_coords.insert((f, x, y));
        }

        let mut outside_set = HashSet::new();
        let mut open_list = VecDeque::new();

        for x in min_x..=max_x {
            for y in min_y..=max_y {
                for f in [min_f, max_f] {
                    if !surface_coords.contains(&(f, x, y)) && outside_set.insert((f, x, y)) {
                        open_list.push_back(SingleId::new(z, f, x, y).unwrap());
                    }
                }
            }
        }
        for f in min_f..=max_f {
            for y in min_y..=max_y {
                for x in [min_x, max_x] {
                    if !surface_coords.contains(&(f, x, y)) && outside_set.insert((f, x, y)) {
                        open_list.push_back(SingleId::new(z, f, x, y).unwrap());
                    }
                }
            }
        }
        for f in min_f..=max_f {
            for x in min_x..=max_x {
                for y in [min_y, max_y] {
                    if !surface_coords.contains(&(f, x, y)) && outside_set.insert((f, x, y)) {
                        open_list.push_back(SingleId::new(z, f, x, y).unwrap());
                    }
                }
            }
        }

        // 外部領域の探索
        let directions = [
            (1, 0, 0),
            (-1, 0, 0),
            (0, 1, 0),
            (0, -1, 0),
            (0, 0, 1),
            (0, 0, -1),
        ];
        while let Some(current) = open_list.pop_front() {
            for (df, dx, dy) in directions {
                let mut neighbor = current;
                let move_ok = if df != 0 {
                    neighbor.move_f(df).is_ok()
                } else if dx != 0 {
                    neighbor.move_x(dx);
                    true
                } else {
                    neighbor.move_y(dy).is_ok()
                };

                if move_ok {
                    let (nf, nx, ny) = (neighbor.f(), neighbor.x(), neighbor.y());
                    // バウンディングボックス内に限定して探索
                    if nf >= min_f
                        && nf <= max_f
                        && nx >= min_x
                        && nx <= max_x
                        && ny >= min_y
                        && ny <= max_y
                        && !surface_coords.contains(&(nf, nx, ny))
                        && outside_set.insert((nf, nx, ny))
                    {
                        open_list.push_back(neighbor);
                    }
                }
            }
        }

        // 列ごとにスキャンして RangeId を生成
        let mut results = Vec::new();
        for x in min_x..=max_x {
            for y in min_y..=max_y {
                let mut current_start = None;
                for f in min_f..=max_f {
                    let is_inside = !outside_set.contains(&(f, x, y));

                    match (is_inside, current_start) {
                        (true, None) => current_start = Some(f),
                        (false, Some(start)) => {
                            results.push(RangeId::new(z, [start, f - 1], [x, x], [y, y])?);
                            current_start = None;
                        }
                        _ => {}
                    }
                }
                if let Some(start) = current_start {
                    results.push(RangeId::new(z, [start, max_f], [x, x], [y, y])?);
                }
            }
        }

        Ok(results.into_iter().map(attach))
    }
}
