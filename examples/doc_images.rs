//! ドキュメント用の図の元データを出力する。
//!
//! 各図形と、それが覆う空間IDを Madori の埋め込みデータ形式（EmbedData）の JSON にする。
//! 出力を Madori の `scripts/capture.mjs` に渡すと、図形ごとに次の画像が `docs/images/` にできる。
//!
//! - `{name}.jpg`: 図形のみ
//! - `{name}_cover.jpg`: 図形のみ（左）と、図形が覆う `SingleId`（右）を並べた図。立体は表面の ID だけを描く
//! - `{name}_cover_range.jpg`: 同じく `RangeId` を並べた図（`CoverRangeIds` を実装する図形のみ）
//!
//! ```sh
//! cargo run --example doc_images --features unstable > scenes.json
//! CAPTURE_WIDTH=640 CAPTURE_HEIGHT=480 node ../Madori/scripts/capture.mjs scenes.json docs/images
//! ```

use std::collections::HashSet;

use spatial_id::geometry::format::RegionMesh;
use spatial_id::geometry::{
    Coordinate, CoverRangeIds, CoverSingleIds, Cylinder, Ecef, Line, Polygon, Solid, Sphere,
    Triangle, Tube,
};

type Vec3 = [f64; 3];

/// 曲面を分割する数。少ないと角張って見える。
const SEGMENTS: usize = 48;

/// ID に重ねて描く図形。曲面は細かい平面に分けて `polygons` で塗る。
#[derive(Default)]
struct Figure {
    lines: Vec<Vec<Coordinate>>,
    points: Vec<Coordinate>,
    /// 1 要素が 1 枚の平らな面で、外周のリングに穴のリングが続く。
    polygons: Vec<Vec<Vec<Coordinate>>>,
}

impl Figure {
    fn closed(mut vertices: Vec<Coordinate>) -> Vec<Coordinate> {
        vertices.push(vertices[0]);
        vertices
    }

    /// 輪郭を線で描き、内側を塗る。
    fn face(&mut self, rings: Vec<Vec<Coordinate>>) {
        let rings: Vec<_> = rings.into_iter().map(Self::closed).collect();
        self.lines.extend(rings.iter().cloned());
        self.polygons.push(rings);
    }

    fn quad(&mut self, corners: [Vec3; 4]) {
        self.polygons
            .push(vec![corners.iter().map(|&c| coordinate_of(c)).collect()]);
    }

    fn cylinder(&mut self, start: Coordinate, end: Coordinate, radius: f64) {
        let (s, e) = (ecef(start), ecef(end));
        let (u, v) = perpendicular_basis(sub(e, s));
        let offsets: Vec<Vec3> = (0..=SEGMENTS)
            .map(|i| rotate(u, v, radius, i as f64 / SEGMENTS as f64))
            .collect();
        for w in offsets.windows(2) {
            self.quad([add(s, w[0]), add(s, w[1]), add(e, w[1]), add(e, w[0])]);
        }
    }

    /// 円柱の両端のふた。
    fn caps(&mut self, start: Coordinate, end: Coordinate, radius: f64) {
        let (s, e) = (ecef(start), ecef(end));
        let (u, v) = perpendicular_basis(sub(e, s));
        for center in [s, e] {
            let rim = circle(center, u, v, radius);
            self.lines.push(rim.clone());
            self.polygons.push(vec![rim]);
        }
    }

    fn sphere(&mut self, center: Coordinate, radius: f64) {
        let c = ecef(center);
        let up = normalize(c);
        let east = normalize(cross([0.0, 0.0, 1.0], up));
        let north = cross(up, east);
        let rings = SEGMENTS / 2;
        let at = |ring: usize, i: usize| {
            let lat = (ring as f64 / rings as f64 - 0.5) * core::f64::consts::PI;
            let horizontal = rotate(
                east,
                north,
                radius * libm::cos(lat),
                i as f64 / SEGMENTS as f64,
            );
            add(c, add(horizontal, scale(up, radius * libm::sin(lat))))
        };
        for ring in 0..rings {
            for i in 0..SEGMENTS {
                self.quad([
                    at(ring, i),
                    at(ring, i + 1),
                    at(ring + 1, i + 1),
                    at(ring + 1, i),
                ]);
            }
        }
    }
}

fn add(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn scale(a: Vec3, k: f64) -> Vec3 {
    [a[0] * k, a[1] * k, a[2] * k]
}

fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn normalize(a: Vec3) -> Vec3 {
    scale(a, 1.0 / libm::sqrt(a[0] * a[0] + a[1] * a[1] + a[2] * a[2]))
}

/// `axis` に垂直な正規直交基底。
fn perpendicular_basis(axis: Vec3) -> (Vec3, Vec3) {
    let w = normalize(axis);
    let helper = if libm::fabs(w[2]) < 0.9 {
        [0.0, 0.0, 1.0]
    } else {
        [1.0, 0.0, 0.0]
    };
    let u = normalize(cross(w, helper));
    (u, cross(w, u))
}

/// 基底 `u`, `v` の平面上で、一周の `turn` 割合だけ回った長さ `radius` のベクトル。
fn rotate(u: Vec3, v: Vec3, radius: f64, turn: f64) -> Vec3 {
    let angle = turn * core::f64::consts::TAU;
    add(
        scale(u, radius * libm::cos(angle)),
        scale(v, radius * libm::sin(angle)),
    )
}

fn circle(center: Vec3, u: Vec3, v: Vec3, radius: f64) -> Vec<Coordinate> {
    (0..=SEGMENTS)
        .map(|i| {
            coordinate_of(add(
                center,
                rotate(u, v, radius, i as f64 / SEGMENTS as f64),
            ))
        })
        .collect()
}

fn ecef(c: Coordinate) -> Vec3 {
    let e: Ecef = c.into();
    [e.x(), e.y(), e.z()]
}

fn coordinate_of(v: Vec3) -> Coordinate {
    Ecef::new(v[0], v[1], v[2]).unwrap().into()
}

fn coordinate(latitude: f64, longitude: f64, altitude: f64) -> Coordinate {
    Coordinate::new(latitude, longitude, altitude).unwrap()
}

fn positions(coordinates: &[Coordinate]) -> String {
    let items: Vec<String> = coordinates
        .iter()
        .map(|c| format!("[{},{},{}]", c.longitude(), c.latitude(), c.altitude()))
        .collect();
    format!("[{}]", items.join(","))
}

fn single_ids<S: CoverSingleIds>(shape: &S, z: u8) -> Vec<String> {
    shape
        .cover_single_ids(z)
        .unwrap()
        .map(|id| id.to_string())
        .collect()
}

/// 立体の図では内部のボクセルの枠線が重なって中の図形が見えなくなるため、表面のボクセルだけを返す。
fn shell_ids<S: CoverSingleIds>(shape: &S, z: u8) -> Vec<String> {
    let ids: HashSet<(i32, u32, u32)> = shape
        .cover_single_ids(z)
        .unwrap()
        .map(|id| (id.f(), id.x(), id.y()))
        .collect();
    let inner = |&(f, x, y): &(i32, u32, u32)| {
        [
            (f - 1, x, y),
            (f + 1, x, y),
            (f, x.wrapping_sub(1), y),
            (f, x + 1, y),
            (f, x, y.wrapping_sub(1)),
            (f, x, y + 1),
        ]
        .iter()
        .all(|n| ids.contains(n))
    };
    ids.iter()
        .filter(|id| !inner(id))
        .map(|(f, x, y)| format!("{z}/{f}/{x}/{y}"))
        .collect()
}

fn range_ids<S: CoverRangeIds>(shape: &S, z: u8) -> Vec<String> {
    shape
        .cover_range_ids(z)
        .unwrap()
        .map(|id| id.to_string())
        .collect()
}

/// 図形のみの図と、`SingleId` と並べた図のシーンを作る。`ranges` があれば `RangeId` と並べた図も作る。
fn scenes(
    name: &str,
    figure: &Figure,
    singles: Vec<String>,
    ranges: Option<Vec<String>>,
    zoom: f64,
) -> Vec<String> {
    let lines: Vec<String> = figure.lines.iter().map(|l| positions(l)).collect();
    let polygons: Vec<String> = figure
        .polygons
        .iter()
        .map(|rings| {
            let rings: Vec<String> = rings.iter().map(|r| positions(r)).collect();
            format!("[{}]", rings.join(","))
        })
        .collect();
    let common = format!(
        r#""lines":[{}],"points":{},"polygons":[{}],"border":true,"idColor":{{"r":14,"g":165,"b":233,"a":50}},"idLineColor":{{"r":3,"g":105,"b":161,"a":150}},"view":{{"zoom":{zoom},"pitch":50,"bearing":-25}}"#,
        lines.join(","),
        positions(&figure.points),
        polygons.join(","),
    );
    let with_ids = |ids: &[String]| format!(r#""ids":["{}"],{common}"#, ids.join(","));
    let singles = with_ids(&singles);
    // 2 枚の視点がそろうよう、図形のみの図でも ID を渡して視点を決めさせる
    let mut out = vec![
        format!(r#"{{"name":"{name}","hideIds":true,{singles}}}"#),
        format!(r#"{{"name":"{name}_cover","compare":true,{singles}}}"#),
    ];
    if let Some(ranges) = ranges {
        let ranges = with_ids(&ranges);
        out.push(format!(
            r#"{{"name":"{name}_cover_range","compare":true,{ranges}}}"#
        ));
    }
    out
}

fn main() {
    let mut out = Vec::new();

    let point = coordinate(35.6812, 139.7671, 40.0);
    let figure = Figure {
        points: vec![point],
        ..Default::default()
    };
    out.extend(scenes("point", &figure, single_ids(&point, 20), None, 18.0));

    let line = Line::new([
        coordinate(35.6800, 139.7660, 10.0),
        coordinate(35.6820, 139.7695, 150.0),
    ]);
    let figure = Figure {
        lines: vec![line.points().to_vec()],
        ..Default::default()
    };
    out.extend(scenes("line", &figure, single_ids(&line, 21), None, 15.9));

    let triangle = Triangle::new([
        coordinate(35.6800, 139.7660, 10.0),
        coordinate(35.6825, 139.7675, 80.0),
        coordinate(35.6805, 139.7700, 150.0),
    ]);
    let mut figure = Figure::default();
    figure.face(vec![triangle.points().to_vec()]);
    out.extend(scenes(
        "triangle",
        &figure,
        single_ids(&triangle, 21),
        None,
        15.9,
    ));

    // 傾いた L 字形。高度を経度に比例させて同一平面に載せる
    let tilted = |latitude: f64, longitude: f64| {
        coordinate(
            latitude,
            longitude,
            20.0 + (longitude - 139.7660) * 40_000.0,
        )
    };
    let polygon = Polygon::new(vec![
        tilted(35.6800, 139.7660),
        tilted(35.6800, 139.7690),
        tilted(35.6808, 139.7690),
        tilted(35.6808, 139.7671),
        tilted(35.6822, 139.7671),
        tilted(35.6822, 139.7660),
    ]);
    let mut figure = Figure::default();
    figure.face(vec![polygon.vertices().clone()]);
    out.extend(scenes(
        "polygon",
        &figure,
        single_ids(&polygon, 21),
        None,
        15.9,
    ));

    // 四角錐
    let base = [
        coordinate(35.6802, 139.7662, 0.0),
        coordinate(35.6802, 139.7686, 0.0),
        coordinate(35.6820, 139.7686, 0.0),
        coordinate(35.6820, 139.7662, 0.0),
    ];
    let apex = coordinate(35.6811, 139.7674, 180.0);
    let mut faces = vec![Polygon::new(vec![base[0], base[3], base[2], base[1]])];
    for i in 0..4 {
        faces.push(Polygon::new(vec![base[i], base[(i + 1) % 4], apex]));
    }
    let mut figure = Figure::default();
    for f in &faces {
        figure.face(vec![f.vertices().clone()]);
    }
    let solid = Solid::new(faces).unwrap();
    out.extend(scenes(
        "solid",
        &figure,
        shell_ids(&solid, 21),
        Some(range_ids(&solid, 21)),
        15.9,
    ));

    let center = coordinate(35.6811, 139.7674, 100.0);
    let sphere = Sphere::new(center, 90.0).unwrap();
    let mut figure = Figure::default();
    figure.sphere(center, 90.0);
    out.extend(scenes(
        "sphere",
        &figure,
        shell_ids(&sphere, 21),
        None,
        16.2,
    ));

    let (start, end) = (
        coordinate(35.6802, 139.7660, 30.0),
        coordinate(35.6818, 139.7690, 150.0),
    );
    let cylinder = Cylinder::new(start, end, 40.0).unwrap();
    let mut figure = Figure::default();
    figure.cylinder(start, end, 40.0);
    figure.caps(start, end, 40.0);
    out.extend(scenes(
        "cylinder",
        &figure,
        shell_ids(&cylinder, 21),
        Some(range_ids(&cylinder, 21)),
        15.9,
    ));

    let points = vec![
        coordinate(35.6800, 139.7660, 30.0),
        coordinate(35.6815, 139.7668, 90.0),
        coordinate(35.6806, 139.7682, 120.0),
        coordinate(35.6820, 139.7692, 60.0),
    ];
    // Tube は各区間の円柱と各頂点の球を合わせた形
    let mut figure = Figure::default();
    for pair in points.windows(2) {
        figure.cylinder(pair[0], pair[1], 30.0);
    }
    for &p in &points {
        figure.sphere(p, 30.0);
    }
    let tube = Tube::new(points, 30.0).unwrap();
    out.extend(scenes("tube", &figure, shell_ids(&tube, 21), None, 15.9));

    // 東京駅を含む第3次メッシュ（約 1 km 四方）
    let mesh = RegionMesh::new(53394611).unwrap();
    // メッシュコード 5339-46-11 の南西端
    let (lat, lon) = (
        53.0 / 1.5 + 4.0 / 12.0 + 1.0 / 120.0,
        139.0 + 6.0 / 8.0 + 1.0 / 80.0,
    );
    let mut figure = Figure::default();
    figure.face(vec![vec![
        coordinate(lat, lon, 0.0),
        coordinate(lat, lon + 1.0 / 80.0, 0.0),
        coordinate(lat + 1.0 / 120.0, lon + 1.0 / 80.0, 0.0),
        coordinate(lat + 1.0 / 120.0, lon, 0.0),
    ]]);
    out.extend(scenes(
        "regionmesh",
        &figure,
        single_ids(&mesh, 20),
        Some(range_ids(&mesh, 20)),
        14.3,
    ));

    // 点・線・穴あきポリゴンをまとめた FeatureCollection
    let point = [139.7656, 35.6822, 60.0];
    let line_string = [
        [139.7658, 35.6800, 20.0],
        [139.7668, 35.6812, 80.0],
        [139.7660, 35.6822, 140.0],
    ];
    let exterior = [
        [139.7672, 35.6800, 40.0],
        [139.7700, 35.6800, 40.0],
        [139.7700, 35.6822, 40.0],
        [139.7672, 35.6822, 40.0],
        [139.7672, 35.6800, 40.0],
    ];
    let hole = [
        [139.7680, 35.6807, 40.0],
        [139.7692, 35.6807, 40.0],
        [139.7692, 35.6815, 40.0],
        [139.7680, 35.6815, 40.0],
        [139.7680, 35.6807, 40.0],
    ];
    let json = |ps: &[[f64; 3]]| {
        let items: Vec<String> = ps
            .iter()
            .map(|p| format!("[{},{},{}]", p[0], p[1], p[2]))
            .collect();
        format!("[{}]", items.join(","))
    };
    let geojson: geojson::GeoJson = format!(
        r#"{{"type":"FeatureCollection","features":[
            {{"type":"Feature","properties":{{}},"geometry":{{"type":"Point","coordinates":[{},{},{}]}}}},
            {{"type":"Feature","properties":{{}},"geometry":{{"type":"LineString","coordinates":{}}}}},
            {{"type":"Feature","properties":{{}},"geometry":{{"type":"Polygon","coordinates":[{},{}]}}}}
        ]}}"#,
        point[0],
        point[1],
        point[2],
        json(&line_string),
        json(&exterior),
        json(&hole),
    )
    .parse()
    .unwrap();
    let to_coordinates = |ps: &[[f64; 3]]| -> Vec<Coordinate> {
        ps.iter().map(|p| coordinate(p[1], p[0], p[2])).collect()
    };
    // GeoJSON のリングは始点を末尾で繰り返すので、それを除いて渡す
    let ring = |ps: &[[f64; 3]]| to_coordinates(&ps[..ps.len() - 1]);
    let mut figure = Figure {
        lines: vec![to_coordinates(&line_string)],
        points: to_coordinates(&[point]),
        ..Default::default()
    };
    figure.face(vec![ring(&exterior), ring(&hole)]);
    out.extend(scenes(
        "geojson",
        &figure,
        single_ids(&geojson, 21),
        None,
        15.6,
    ));

    println!("[{}]", out.join(","));
}
