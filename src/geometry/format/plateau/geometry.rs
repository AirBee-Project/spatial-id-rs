//! CityGML の形状を、図形の型へ変換する途中の表現。

use alloc::vec::Vec;
use hashbrown::HashSet;

use crate::{Coordinate, CoverSingleIds, Error, Polygon, SingleId, Vec3, Vec3Ecef};

/// 読み取る LOD。LOD0 は足跡や屋根の輪郭で立体にならないため扱わない。
#[derive(Debug, Clone, Copy)]
pub(super) struct Lod(usize);

impl Lod {
    const COUNT: usize = 3;

    /// `lod1Solid` や `lod2MultiSurface` のような要素名が示す LOD。
    pub(super) fn from_element(local_name: &[u8]) -> Option<Self> {
        let [b'l', b'o', b'd', digit @ b'1'..=b'3', rest @ ..] = local_name else {
            return None;
        };
        (rest == b"Solid" || rest == b"MultiSurface").then(|| Self((digit - b'1') as usize))
    }
}

/// 面の境界の 1 周。始点と同じ終点は持たない。
#[derive(Debug, Clone)]
pub(super) struct Ring(Vec<Coordinate>);

impl Ring {
    pub(super) fn new(mut points: Vec<Coordinate>) -> Self {
        if points.len() > 1 && points.first() == points.last() {
            points.pop();
        }
        Self(points)
    }

    /// 外周に、各穴を最も近い頂点どうしの往復の辺（ブリッジ）でつなぎ、1 つのリングにする。
    ///
    /// [`Polygon`] は穴を持てないため。ブリッジの辺は同じ面に 2 回現れるので、
    /// 立体の閉じ判定では打ち消し合い、穴の縁の辺だけが隣の面と対になる。
    /// 三角形分割（耳切り）では、ブリッジで重なる頂点は境界上にあるので内側の点とみなされない。
    pub(super) fn with_holes(self, holes: impl IntoIterator<Item = Ring>) -> Self {
        if self.0.len() < 3 {
            return self;
        }
        let normal = self.normal();
        let mut outer = self.0;
        for mut hole in holes.into_iter().filter(|h| h.0.len() >= 3) {
            // 穴を外周と逆向きにそろえないと、つないだリングが自己交差する。
            if hole.normal().dot(&normal) > 0.0 {
                hole.0.reverse();
            }
            let (i, j) = Self::closest_vertices(&outer, &hole.0);
            let mut merged = Vec::with_capacity(outer.len() + hole.0.len() + 2);
            merged.extend_from_slice(&outer[..=i]);
            merged.extend_from_slice(&hole.0[j..]);
            merged.extend_from_slice(&hole.0[..=j]);
            merged.extend_from_slice(&outer[i..]);
            outer = merged;
        }
        Self(outer)
    }

    /// 頂点が 3 つ以上残る場合だけ [`Polygon`] にする。
    fn into_polygon(self) -> Option<Polygon> {
        let polygon = Polygon::new(self.0);
        (!polygon.vertices().is_empty()).then_some(polygon)
    }

    /// ECEF での面の法線（Newell 法）。地球中心からの大きな座標の桁落ちを避けるため、始点を原点にする。
    fn normal(&self) -> Vec3Ecef {
        let points: Vec<Vec3Ecef> = self.0.iter().map(|&c| c.into()).collect();
        let origin = points[0];
        points
            .iter()
            .zip(points.iter().cycle().skip(1))
            .map(|(&a, &b)| (a - origin).cross(&(b - origin)))
            .fold(Vec3Ecef::new(0.0, 0.0, 0.0), |sum, n| sum + n)
    }

    /// 2 つのリングの頂点のうち、ECEF で最も近い組の添字。
    fn closest_vertices(a: &[Coordinate], b: &[Coordinate]) -> (usize, usize) {
        let a: Vec<Vec3Ecef> = a.iter().map(|&c| c.into()).collect();
        let b: Vec<Vec3Ecef> = b.iter().map(|&c| c.into()).collect();
        let mut best = (0, 0, f64::INFINITY);
        for (i, &p) in a.iter().enumerate() {
            for (j, &q) in b.iter().enumerate() {
                let d = (p - q).norm_squared();
                if d < best.2 {
                    best = (i, j, d);
                }
            }
        }
        (best.0, best.1)
    }
}

/// 地物 1 件の、LOD ごとの面。
#[derive(Debug, Clone, Default)]
pub(super) struct LodRings([Vec<Ring>; Lod::COUNT]);

impl LodRings {
    pub(super) fn push(&mut self, lod: Lod, ring: Ring) {
        self.0[lod.0].push(ring);
    }

    pub(super) fn clear(&mut self) {
        self.0.iter_mut().for_each(Vec::clear);
    }

    /// 面を持つ LOD の面を、LOD の高い順に返す。
    pub(super) fn highest_first(self) -> impl Iterator<Item = Vec<Polygon>> {
        self.0
            .into_iter()
            .rev()
            .filter(|rings| !rings.is_empty())
            .map(|rings| rings.into_iter().filter_map(Ring::into_polygon).collect())
    }
}

/// 面の集まり。面だけを覆い、内部は覆わない。
#[derive(Clone, Copy)]
pub(super) struct Surfaces<'a>(pub(super) &'a [Polygon]);

impl Surfaces<'_> {
    /// 隣り合う面は境界の ID を共有するので、重複を除く。
    pub(super) fn cover(self, z: u8) -> Result<HashSet<SingleId>, Error> {
        let mut ids = HashSet::new();
        for polygon in self.0 {
            ids.extend(polygon.cover_single_ids(z)?);
        }
        Ok(ids)
    }
}
