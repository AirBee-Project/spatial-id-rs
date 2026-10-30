use alloc::vec::Vec;

use crate::geometry::Epsilon;
use crate::geometry::shape::polygon::Polygon;
use crate::{CoverSingleIds as _, Ecef, Error, ExpandTriangles, GeometryError, SingleId, Triangle};
use hashbrown::{HashMap, HashSet};

pub mod geometry_relation;
pub mod impls;
#[cfg(test)]
mod tests;

/// 立体を表す型。
///
/// 複数の [Polygon] によって構成される閉じた領域を表現する。
/// この型は、作成時に必ず閉合性が検証されるため、作成できている場合は「閉じている」ことが保証される。
#[derive(Debug, Clone)]
pub struct Solid {
    /// 立体を構成する面のリスト
    polygons: Vec<Polygon>,
}

impl Solid {
    /// 構成する面のリストからデフォルト許容誤差（[Epsilon::DEFAULT]）を使用して [Solid] を作成。
    ///
    /// # 処理内容
    /// - 各面を[Polygon] に変換。
    /// - 閉合性の検証。
    /// - 穴がある場合は [GeometryError::SolidNotWatertight] を返す。
    ///
    /// # 引数
    /// - `polygons` - 立体を構成する面のリスト。
    pub fn new(polygons: Vec<Polygon>) -> Result<Self, Error> {
        Self::with_epsilon(polygons, Epsilon::DEFAULT)
    }

    /// 構成する面のリストと指定された許容誤差から [Solid] を作成。
    ///
    /// # 処理内容
    /// - 各面を[Polygon] に変換。
    /// - 閉合性の検証。
    /// - 穴がある場合は [GeometryError::SolidNotWatertight] を返す。
    ///
    /// # 引数
    /// - `polygons` - 立体を構成する面のリスト。
    /// - `epsilon` - 同一点とみなす許容誤差。
    pub fn with_epsilon(polygons: Vec<Polygon>, epsilon: Epsilon) -> Result<Self, Error> {
        let filtered_polygons: Vec<Polygon> = polygons
            .into_iter()
            .filter(|p| !p.vertices().is_empty()) // 無効な面は除外
            .collect();

        if filtered_polygons.is_empty() {
            return Err(GeometryError::SolidNotWatertight { open_edge_count: 0 }.into());
        }

        // 閉合性チェック
        let open_edges = Self::count_open_edges(&filtered_polygons, epsilon);
        if open_edges > 0 {
            return Err(GeometryError::SolidNotWatertight {
                open_edge_count: open_edges,
            }
            .into());
        }

        Ok(Self {
            polygons: filtered_polygons,
        })
    }

    /// [Solid] 全体を三角形分割し、構成する [Triangle] のリストを返す。
    pub fn triangles(&self) -> Vec<Triangle> {
        self.expand_triangles().collect()
    }

    /// 閉じていないエッジの数を数える内部ヘルパー関数
    fn count_open_edges(polygons: &[Polygon], epsilon: Epsilon) -> usize {
        // エッジの出現回数を記録するマップ
        let mut edge_counts: HashMap<(QuantizedCoord, QuantizedCoord), usize> = HashMap::new();
        let precision = epsilon.as_meters();

        for polygon in polygons {
            let vertices = &polygon.vertices();
            let len = vertices.len();
            if len < 3 {
                continue;
            }

            for i in 0..len {
                let p1 = &vertices[i];
                let p2 = &vertices[(i + 1) % len]; // 最後の点と最初の点を結ぶ

                // ECEF座標系で量子化
                let e1: Ecef = (*p1).into();
                let e2: Ecef = (*p2).into();
                let q1 = QuantizedCoord::new(&e1, precision);
                let q2 = QuantizedCoord::new(&e2, precision);

                // 縮退辺は無視
                if q1 == q2 {
                    continue;
                }

                // エッジの向きを正規化（小さい方を先に）してカウント
                let edge_key = if q1 < q2 { (q1, q2) } else { (q2, q1) };
                *edge_counts.entry(edge_key).or_insert(0) += 1;
            }
        }

        // 奇数回出現するエッジの数をカウント
        edge_counts.values().filter(|&count| count % 2 != 0).count()
    }

    /// 指定されたズームレベル `z` における、この [Solid] の表面を覆う [SingleId] の集合を返す。
    pub fn surface_single_ids(
        &self,
        z: impl Into<u8>,
    ) -> Result<impl Iterator<Item = SingleId>, Error> {
        let z = z.into();
        // HashSetで重複を除去
        let mut unique_ids = HashSet::new();

        for polygon in &self.polygons {
            let triangles = polygon.expand_triangles();
            for triangle in triangles {
                let ids_iter = triangle.cover_single_ids(z)?;
                for id in ids_iter {
                    unique_ids.insert(id);
                }
            }
        }

        Ok(unique_ids.into_iter())
    }
}

/// ハッシュマップのキーにするために座標を整数化するラッパー
#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy, PartialOrd, Ord)]
struct QuantizedCoord {
    x: i64,
    y: i64,
    z: i64,
}

impl QuantizedCoord {
    fn new(e: &Ecef, precision: f64) -> Self {
        if precision == 0.0 {
            // -0.0 と 0.0 を同じビット列にそろえる
            Self {
                x: (e.x() + 0.0).to_bits() as i64,
                y: (e.y() + 0.0).to_bits() as i64,
                z: (e.z() + 0.0).to_bits() as i64,
            }
        } else {
            Self {
                x: libm::round(e.x() / precision) as i64,
                y: libm::round(e.y() / precision) as i64,
                z: libm::round(e.z() / precision) as i64,
            }
        }
    }
}
