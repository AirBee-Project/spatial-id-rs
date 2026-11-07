//! 地理空間データのフォーマット。

#[cfg(all(feature = "std", feature = "unstable"))]
mod geojson;
#[cfg(all(feature = "std", feature = "unstable"))]
pub mod plateau;
#[cfg(feature = "unstable")]
pub(crate) mod regionmesh;

#[cfg(feature = "unstable")]
pub use regionmesh::RegionMesh;
