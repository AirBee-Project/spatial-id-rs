//! Shape モジュールの相互関係の概要。
//!
//! - [`Line`](crate::geometry::Line) は 2 つの [`Coordinate`](crate::geometry::Coordinate) で表される。
//! - [`Triangle`](crate::geometry::Triangle) は 3 本の [`Line`](crate::geometry::Line)（または 3 つの [`Coordinate`](crate::geometry::Coordinate)）で表される。
//! - [`Polygon`](crate::geometry::Polygon) は複数の [`Triangle`](crate::geometry::Triangle) に分解できる。
//! - [`Solid`](crate::geometry::Solid) は複数の [`Polygon`](crate::geometry::Polygon) で構成される。
//! - [`Sphere`](crate::geometry::Sphere) は中心点と半径を持つ独立した [`Shape`](crate::geometry::Shape) として扱う。
//!
//! 詳細な図と説明は次のドキュメントを参照:
//! [docs/geometry-relation.md](https://github.com/AirBee-Project/spatial-id-rs/blob/main/docs/geometry-relation.md)

pub(crate) mod cylinder;
pub(crate) mod line;
pub(crate) mod polygon;
pub(crate) mod solid;
pub(crate) mod sphere;
pub(crate) mod traits;
pub(crate) mod triangle;
pub(crate) mod tube;
