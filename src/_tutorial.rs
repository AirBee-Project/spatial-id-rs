#![doc = include_str!("../docs/tutorial/00_index.md")]

#[doc = include_str!("../docs/tutorial/01_spatial_id.md")]
#[doc = doc_image!("spatial_id_grid", "docs/images/tutorial/spatial_id_grid.jpg")]
#[doc = doc_image!("spatial_id_single", "docs/images/tutorial/spatial_id_single.jpg")]
#[doc = doc_image!("spatial_id_parent", "docs/images/tutorial/spatial_id_parent.jpg")]
#[doc = doc_image!("spatial_id_children", "docs/images/tutorial/spatial_id_children.jpg")]
#[doc = doc_image!("spatial_id_neighbors", "docs/images/tutorial/spatial_id_neighbors.jpg")]
#[doc = doc_image!("spatial_id_range", "docs/images/tutorial/spatial_id_range.jpg")]
#[doc = doc_image!("spatial_id_range_move", "docs/images/tutorial/spatial_id_range_move.jpg")]
pub mod _01_spatial_id {}

#[doc = include_str!("../docs/tutorial/02_collection.md")]
#[doc = doc_image!("collection_get", "docs/images/tutorial/collection_get.jpg")]
#[doc = doc_image!("query_merge", "docs/images/query_merge.jpg")]
#[doc = doc_image!("query_intersection", "docs/images/query_intersection.jpg")]
#[doc = doc_image!("query_difference", "docs/images/query_difference.jpg")]
#[doc = doc_image!("query_filter_in", "docs/images/query_filter_in.jpg")]
pub mod _02_collection {}

#[doc = include_str!("../docs/tutorial/03_geometry.md")]
#[doc = doc_image!("sphere_cover", "docs/images/sphere_cover.jpg")]
#[doc = doc_image!("point_cover", "docs/images/point_cover.jpg")]
#[doc = doc_image!("line_cover", "docs/images/line_cover.jpg")]
#[doc = doc_image!("triangle_cover", "docs/images/triangle_cover.jpg")]
#[doc = doc_image!("polygon_cover", "docs/images/polygon_cover.jpg")]
#[doc = doc_image!("solid_cover", "docs/images/solid_cover.jpg")]
#[doc = doc_image!("solid_cover_range", "docs/images/solid_cover_range.jpg")]
#[doc = doc_image!("cylinder_cover", "docs/images/cylinder_cover.jpg")]
#[doc = doc_image!("tube_cover", "docs/images/tube_cover.jpg")]
#[doc = doc_image!("geojson_cover", "docs/images/geojson_cover.jpg")]
#[doc = doc_image!("geometry_ecef", "docs/images/tutorial/geometry_ecef.jpg")]
#[doc = doc_image!("geometry_fractional_id", "docs/images/tutorial/geometry_fractional_id.jpg")]
#[doc = doc_image!("geometry_triangle_divide", "docs/images/tutorial/geometry_triangle_divide.jpg")]
pub mod _03_geometry {}

#[doc = include_str!("../docs/tutorial/04_query.md")]
#[doc = doc_image!("riskmap_flow", "docs/images/riskmap_flow.jpg")]
#[doc = doc_image!("riskmap_result", "docs/images/riskmap_result.jpg")]
#[doc = doc_image!("query_shift_x", "docs/images/query_shift_x.jpg")]
#[doc = doc_image!("query_extrude_f", "docs/images/query_extrude_f.jpg")]
#[doc = doc_image!("query_falloff_x", "docs/images/query_falloff_x.jpg")]
#[doc = doc_image!("query_zoom_out", "docs/images/query_zoom_out.jpg")]
#[doc = doc_image!("query_filter_in", "docs/images/query_filter_in.jpg")]
#[doc = doc_image!("query_merge", "docs/images/query_merge.jpg")]
#[doc = doc_image!("query_intersection", "docs/images/query_intersection.jpg")]
#[doc = doc_image!("query_difference", "docs/images/query_difference.jpg")]
#[doc = doc_image!("query_falloff_pattern", "docs/images/tutorial/query_falloff_pattern.jpg")]
pub mod _04_query {}

#[doc = include_str!("../docs/tutorial/05_temporal.md")]
#[doc = doc_image!("temporal_range", "docs/images/tutorial/temporal_range.jpg")]
pub mod _05_temporal {}

#[doc = include_str!("../docs/tutorial/06_flextree.md")]
#[doc = doc_image!("flextree_segment", "docs/images/tutorial/flextree_segment.jpg")]
#[doc = doc_image!("flextree_flex_id", "docs/images/tutorial/flextree_flex_id.jpg")]
#[doc = doc_image!("flextree_point", "docs/images/tutorial/flextree_point.jpg")]
#[doc = doc_image!("flextree_aabb", "docs/images/tutorial/flextree_aabb.jpg")]
#[doc = doc_image!("flextree_voxel_count", "docs/images/tutorial/flextree_voxel_count.jpg")]
pub mod _06_flextree {}
