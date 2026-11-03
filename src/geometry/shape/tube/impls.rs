use crate::{CoverSingleIds, Cylinder, Error, SingleId, Sphere, Tube};
use hashbrown::HashSet;

/// パイプ（左）を、それが覆う空間ID（右）へ変換する。図はズームレベル 21 の例。
///
/// ![パイプと、それが覆う空間ID][tube_cover]
#[doc = doc_image!("tube_cover", "docs/images/tube_cover.jpg")]
impl CoverSingleIds for Tube {
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
        let z: u8 = z.into();
        let mut ids: HashSet<_> = Sphere::new(self.points[0], self.radius_m)?
            .cover_single_ids(z)?
            .collect();
        for coos in self.points.windows(2) {
            ids.extend(Cylinder::new(coos[0], coos[1], self.radius_m)?.cover_single_ids(z)?);
            ids.extend(Sphere::new(coos[1], self.radius_m)?.cover_single_ids(z)?);
        }
        Ok(ids.into_iter().map(move |id| (id, value.clone())))
    }
}
