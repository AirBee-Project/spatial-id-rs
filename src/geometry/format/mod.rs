//! 地理空間データのフォーマット。

#[cfg(feature = "geojson")]
mod geojson;
#[cfg(feature = "unstable")]
pub(crate) mod regionmesh;

#[cfg(feature = "unstable")]
pub use regionmesh::RegionMesh;
