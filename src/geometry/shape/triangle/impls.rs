use hashbrown::HashSet;

use crate::{
    Coordinate, Error, ExpandCoordinates, Point, Shape, SingleId, Triangle, Vec3, Vec3FractionalId,
    geometry::traits::CoverSingleIds,
};

impl Shape for Triangle {
    fn center(&self) -> Coordinate {
        Coordinate::centroid(self.expand_coordinates())
    }
}

/// 三角形（左）を、それが覆う空間ID（右）へ変換する。図はズームレベル 21 の例。
///
/// ![三角形と、それが覆う空間ID][triangle_cover]
#[cfg_attr(
    doc,
    embed_doc_image::embed_doc_image("triangle_cover", "docs/images/triangle_cover.jpg")
)]
impl CoverSingleIds for Triangle {
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
        let z = z.into();
        let points: [Vec3FractionalId; 3] = [
            Vec3FractionalId::from(self.points[0].fractional_id(z)?),
            Vec3FractionalId::from(self.points[1].fractional_id(z)?),
            Vec3FractionalId::from(self.points[2].fractional_id(z)?),
        ];
        let diff_f = libm::floor(points[0].a().max(points[1].a()).max(points[2].a()))
            - libm::floor(points[0].a().min(points[1].a()).min(points[2].a()));
        let diff_x = libm::floor(points[0].b().max(points[1].b()).max(points[2].b()))
            - libm::floor(points[0].b().min(points[1].b()).min(points[2].b()));
        let diff_y = libm::floor(points[0].c().max(points[1].c()).max(points[2].c()))
            - libm::floor(points[0].c().min(points[1].c()).min(points[2].c()));
        let steps = libm::ceil(diff_f.max(diff_x).max(diff_y) / 8.0) as u32;
        let mut seen = HashSet::new();
        let voxels = self
            .divide(steps)?
            .flat_map(move |tri| tri.single_ids_limited(z).ok().into_iter().flatten())
            .filter(move |voxel| seen.insert(*voxel));
        Ok(voxels.map(move |id| (id, value.clone())))
    }
}
