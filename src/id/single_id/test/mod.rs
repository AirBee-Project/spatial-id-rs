pub mod ops;

#[cfg(test)]
#[cfg(feature = "unstable")]
mod encode;

#[cfg(test)]
mod tests {
    use crate::{Error, SingleId, SpatialIdError};
    use alloc::vec::Vec;

    #[test]
    fn children_at_zoom_works() {
        let id = SingleId::new(3, 3, 2, 7).unwrap();

        let children: Vec<_> = id.spatial_children_at_zoom(4).unwrap().collect();

        assert_eq!(children.len(), 8);
        assert_eq!(children.first().unwrap().z(), 4);
        assert_eq!(children.first().unwrap().f(), 6);
        assert_eq!(children.first().unwrap().x(), 4);
        assert_eq!(children.first().unwrap().y(), 14);
    }

    #[test]
    fn parent_at_zoom_works() {
        let id = SingleId::new(4, 6, 9, 14).unwrap();

        let parent = id.spatial_parent_at_zoom(3).unwrap();

        assert_eq!(parent.z(), 3u8);
        assert_eq!(parent.f(), 3i32);
        assert_eq!(parent.x(), 4u32);
        assert_eq!(parent.y(), 7u32);
    }

    #[test]
    fn spatial_siblings_are_returned_for_non_root_ids() {
        let id = SingleId::new(3, 3, 2, 7).unwrap();

        let siblings = id.spatial_siblings().unwrap();

        assert_eq!(siblings.len(), 7);
        assert!(!siblings.contains(&id));

        let parent = id.spatial_parent_at_zoom(2).unwrap();
        assert!(
            siblings
                .iter()
                .all(|s| s.spatial_parent_at_zoom(2).unwrap() == parent)
        );
    }

    #[test]
    fn spatial_siblings_is_none_at_root() {
        let id = SingleId::new(0, 0, 0, 0).unwrap();

        assert!(id.spatial_siblings().is_none());
    }

    #[test]
    fn parents_are_returned_from_nearest_to_root() {
        let id = SingleId::new(3, 3, 2, 7).unwrap();

        let parents: Vec<_> = id.spatial_parents().collect();

        assert_eq!(parents.len(), 3);
        assert_eq!(parents[0], SingleId::new(2, 1, 1, 3).unwrap());
        assert_eq!(parents[1], SingleId::new(1, 0, 0, 1).unwrap());
        assert_eq!(parents[2], SingleId::new(0, 0, 0, 0).unwrap());
    }

    #[test]
    fn parents_is_empty_at_root() {
        let id = SingleId::new(0, 0, 0, 0).unwrap();

        #[allow(clippy::needless_collect)]
        let parents: Vec<_> = id.spatial_parents().collect();

        assert!(parents.is_empty());
    }

    #[test]
    fn zoom_direction_mismatch_returns_error() {
        let id = SingleId::new(3, 3, 2, 7).unwrap();

        let result = id.spatial_children_at_zoom(2);

        assert!(matches!(
            result,
            Err(Error::SpatialId(
                SpatialIdError::ZoomLevelTransitionOutOfRange {
                    current_z: 3,
                    target_z: 2
                }
            ))
        ));
    }

    #[test]
    fn six_neighbors_are_generated() {
        let id = SingleId::new(4, 6, 9, 10).unwrap();
        let neighbors: Vec<_> = id.spatial_face_neighbors().collect();

        assert_eq!(neighbors.len(), 6);
        assert!(neighbors.contains(&SingleId::new(4, 7, 9, 10).unwrap()));
        assert!(neighbors.contains(&SingleId::new(4, 5, 9, 10).unwrap()));
        assert!(neighbors.contains(&SingleId::new(4, 6, 10, 10).unwrap()));
        assert!(neighbors.contains(&SingleId::new(4, 6, 8, 10).unwrap()));
        assert!(neighbors.contains(&SingleId::new(4, 6, 9, 11).unwrap()));
        assert!(neighbors.contains(&SingleId::new(4, 6, 9, 9).unwrap()));
    }

    #[test]
    fn twelve_edge_neighbors_are_generated() {
        let id = SingleId::new(4, 6, 9, 10).unwrap();
        let neighbors: Vec<_> = id.spatial_edge_neighbors().collect();

        assert_eq!(neighbors.len(), 12);
        // f-x 平面
        assert!(neighbors.contains(&SingleId::new(4, 7, 10, 10).unwrap()));
        assert!(neighbors.contains(&SingleId::new(4, 7, 8, 10).unwrap()));
        assert!(neighbors.contains(&SingleId::new(4, 5, 10, 10).unwrap()));
        assert!(neighbors.contains(&SingleId::new(4, 5, 8, 10).unwrap()));
        // f-y 平面
        assert!(neighbors.contains(&SingleId::new(4, 7, 9, 11).unwrap()));
        assert!(neighbors.contains(&SingleId::new(4, 7, 9, 9).unwrap()));
        assert!(neighbors.contains(&SingleId::new(4, 5, 9, 11).unwrap()));
        assert!(neighbors.contains(&SingleId::new(4, 5, 9, 9).unwrap()));
        // x-y 平面
        assert!(neighbors.contains(&SingleId::new(4, 6, 10, 11).unwrap()));
        assert!(neighbors.contains(&SingleId::new(4, 6, 10, 9).unwrap()));
        assert!(neighbors.contains(&SingleId::new(4, 6, 8, 11).unwrap()));
        assert!(neighbors.contains(&SingleId::new(4, 6, 8, 9).unwrap()));
    }

    #[test]
    fn eight_vertex_neighbors_are_generated() {
        let id = SingleId::new(4, 6, 9, 10).unwrap();
        let neighbors: Vec<_> = id.spatial_vertex_neighbors().collect();

        assert_eq!(neighbors.len(), 8);
        assert!(neighbors.contains(&SingleId::new(4, 7, 10, 11).unwrap()));
        assert!(neighbors.contains(&SingleId::new(4, 7, 10, 9).unwrap()));
        assert!(neighbors.contains(&SingleId::new(4, 7, 8, 11).unwrap()));
        assert!(neighbors.contains(&SingleId::new(4, 7, 8, 9).unwrap()));
        assert!(neighbors.contains(&SingleId::new(4, 5, 10, 11).unwrap()));
        assert!(neighbors.contains(&SingleId::new(4, 5, 10, 9).unwrap()));
        assert!(neighbors.contains(&SingleId::new(4, 5, 8, 11).unwrap()));
        assert!(neighbors.contains(&SingleId::new(4, 5, 8, 9).unwrap()));
    }

    #[test]
    fn twenty_six_neighbors_are_generated() {
        let id = SingleId::new(4, 6, 9, 10).unwrap();
        let neighbors: Vec<_> = id.spatial_all_neighbors().collect();

        assert_eq!(neighbors.len(), 26);
        assert!(neighbors.contains(&SingleId::new(4, 7, 10, 11).unwrap()));
        assert!(neighbors.contains(&SingleId::new(4, 5, 8, 9).unwrap()));
        assert!(neighbors.contains(&SingleId::new(4, 6, 10, 11).unwrap()));
        assert!(neighbors.contains(&SingleId::new(4, 6, 8, 9).unwrap()));
    }

    #[test]
    fn parent_and_children_shortcuts() {
        let root = SingleId::new(0, 0, 0, 0).unwrap();
        assert_eq!(root.spatial_parent(), None);

        let child = SingleId::new(1, 0, 0, 0).unwrap();
        assert_eq!(child.spatial_parent(), Some(root));

        let children: Vec<_> = root.spatial_children().unwrap().collect();
        assert_eq!(children.len(), 8);
        assert!(children.contains(&child));
    }

    #[test]
    fn contains_and_intersects_work() {
        let parent = SingleId::new(2, 0, 0, 0).unwrap();
        let child = SingleId::new(3, 0, 0, 0).unwrap();
        let sibling = SingleId::new(2, 1, 0, 0).unwrap();

        assert!(parent.contains(&child));
        assert!(!child.contains(&parent));
        assert!(parent.contains(&parent));
        assert!(!parent.contains(&sibling));

        assert!(parent.intersects(&child));
        assert!(child.intersects(&parent));
        assert!(!parent.intersects(&sibling));
    }

    #[test]
    fn spatial_shift_moves_each_axis() {
        use crate::SpatialId;

        let mut id = SingleId::new(4, 6, 9, 10).unwrap();
        id.spatial_shift(1, -2, 3).unwrap();
        assert_eq!(id.f(), 7);
        assert_eq!(id.x(), 7);
        assert_eq!(id.y(), 13);
    }
}
