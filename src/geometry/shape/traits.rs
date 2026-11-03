use crate::{Coordinate, Line, Polygon, Triangle};

/// 現実空間の図形に共通する性質。
///
/// <https://github.com/AirBee-Project/spatial-id-rs/blob/main/docs/geometry-relation.md>
pub trait Shape {
    fn center(&self) -> Coordinate;
}

/// [Coordinate] の集合へ分解可能であることを示す
///
/// <https://github.com/AirBee-Project/spatial-id-rs/blob/main/docs/geometry-relation.md>
///
/// # Examples
/// ```
/// use spatial_id::geometry::{Coordinate, ExpandCoordinates, Line};
///
/// let p0 = Coordinate::new(35.0, 139.0, 10.0).unwrap();
/// let p1 = Coordinate::new(35.0001, 139.0001, 11.0).unwrap();
/// let line = Line::new([p0, p1]);
///
/// let coords: Vec<Coordinate> = line.expand_coordinates().collect();
/// assert_eq!(coords.len(), 2);
/// ```
pub trait ExpandCoordinates {
    fn expand_coordinates(&self) -> impl Iterator<Item = Coordinate>;
}

/// [Line] の集合へ分解可能であることを示す
///
/// <https://github.com/AirBee-Project/spatial-id-rs/blob/main/docs/geometry-relation.md>
///
/// # Examples
/// ```
/// use spatial_id::geometry::{Coordinate, ExpandLines, Line, Triangle};
///
/// let p0 = Coordinate::new(35.0, 139.0, 10.0).unwrap();
/// let p1 = Coordinate::new(35.0002, 139.0, 10.0).unwrap();
/// let p2 = Coordinate::new(35.0, 139.0002, 10.0).unwrap();
/// let tri = Triangle::new([p0, p1, p2]);
///
/// let lines: Vec<Line> = tri.expand_lines().collect();
/// assert_eq!(lines.len(), 3);
/// ```
pub trait ExpandLines {
    fn expand_lines(&self) -> impl Iterator<Item = Line>;
}

/// [Triangle] の集合へ分解可能であることを示す
///
/// <https://github.com/AirBee-Project/spatial-id-rs/blob/main/docs/geometry-relation.md>
///
/// # Examples
/// ```
/// use spatial_id::geometry::{Coordinate, ExpandTriangles, Polygon, Triangle};
///
/// let p0 = Coordinate::new(35.0, 139.0, 10.0).unwrap();
/// let p1 = Coordinate::new(35.0003, 139.0, 10.0).unwrap();
/// let p2 = Coordinate::new(35.0003, 139.0003, 10.0).unwrap();
/// let p3 = Coordinate::new(35.0, 139.0003, 10.0).unwrap();
/// let polygon = Polygon::new(vec![p0, p1, p2, p3]);
///
/// let triangles: Vec<Triangle> = polygon.expand_triangles().collect();
/// assert_eq!(triangles.len(), 2);
/// ```
pub trait ExpandTriangles {
    fn expand_triangles(&self) -> impl Iterator<Item = Triangle>;
}

/// [Polygon] の集合へ分解可能であることを示す
///
/// <https://github.com/AirBee-Project/spatial-id-rs/blob/main/docs/geometry-relation.md>
///
/// # Examples
/// ```
/// use spatial_id::geometry::{Coordinate, ExpandPolygons, Polygon, Solid};
///
/// let p0 = Coordinate::new(35.0, 139.0, 10.0).unwrap();
/// let p1 = Coordinate::new(35.0003, 139.0, 10.0).unwrap();
/// let p2 = Coordinate::new(35.0, 139.0003, 10.0).unwrap();
/// let p3 = Coordinate::new(35.0002, 139.0002, 10.3).unwrap();
///
/// let surfaces = vec![
///     Polygon::new(vec![p0, p1, p2]),
///     Polygon::new(vec![p0, p3, p1]),
///     Polygon::new(vec![p1, p3, p2]),
///     Polygon::new(vec![p2, p3, p0]),
/// ];
///
/// let solid = Solid::new(surfaces).unwrap();
/// let polygons: Vec<Polygon> = solid.expand_polygons().collect();
/// assert_eq!(polygons.len(), 4);
/// ```
pub trait ExpandPolygons {
    fn expand_polygons(&self) -> impl Iterator<Item = Polygon>;
}
