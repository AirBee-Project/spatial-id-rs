use crate::{
    Error, FlexId, SpatialIdError,
    id::{
        dimension::{Dimension, DimensionSet},
        zoom_level::{TZoomLevel, ZoomLevel},
    },
};

/// ある祖先の [FlexId] を起点とした相対的な位置を表す[FlexId]。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct RelativeFlexId(FlexId);

impl FlexId {
    /// `ancestor` を原点とした[RelativeFlexId]を返す。
    /// `ancestor`が完全に自身を包含していなければ [`SpatialIdError::NotAncestor`] を返す。
    pub(crate) fn relative_to(&self, ancestor: &FlexId) -> Result<RelativeFlexId, Error> {
        if !ancestor.contains(self) {
            return Err(SpatialIdError::NotAncestor.into());
        }
        // 各次元で、祖先からの深さと、その深さぶんの下位ビットを求める
        let [(fz, f), (xz, x), (yz, y), (tz, t)] =
            zip_dimensions(self, ancestor, |(z, i), (az, ai)| {
                let dz = z - az;
                (dz, i - (ai << dz))
            });
        // SAFETY: 祖先に含まれるので、深さは自身のズーム以下、下位ビットは `0..2^深さ` に収まる
        let relative = unsafe {
            FlexId::new_unchecked(fz, f as i32, xz, x as u32, yz, y as u32)
                .time_with_segment_unchecked(tz, t as u64)
        };
        Ok(RelativeFlexId(relative))
    }
}

impl RelativeFlexId {
    /// 祖先から `dimension`において何段深いかを返す。
    pub fn depth_on(self, dimension: Dimension) -> u8 {
        self.0.zoomlevel_on(dimension)
    }

    /// 祖先より深くなっている次元の集合。
    pub fn deeper_dimensions(&self) -> DimensionSet {
        Dimension::ALL
            .into_iter()
            .filter(|&d| self.depth_on(d) > 0)
            .collect()
    }

    /// `ancestor` を原点として、[FlexId] を復元する。
    /// どれかの次元で最大ズームを超えるなら [`SpatialIdError::ZOutOfRange`] を返す。
    pub fn to_absolute(self, ancestor: &FlexId) -> Result<FlexId, Error> {
        for (((az, _), (rz, _)), max) in dimensions(ancestor)
            .into_iter()
            .zip(dimensions(&self.0))
            .zip(MAX_ZOOM)
        {
            if az + rz > max {
                return Err(SpatialIdError::ZOutOfRange { z: az + rz }.into());
            }
        }
        let [(fz, f), (xz, x), (yz, y), (tz, t)] =
            zip_dimensions(ancestor, &self.0, |(az, ai), (rz, ri)| {
                (az + rz, (ai << rz) + ri)
            });
        // SAFETY: ズームは上限以下。インデックスは祖先の区間を `2^深さ` 等分したうちの1つなので、そのズームの範囲に収まる
        let absolute = unsafe {
            FlexId::new_unchecked(fz, f as i32, xz, x as u32, yz, y as u32)
                .time_with_segment_unchecked(tz, t as u64)
        };
        Ok(absolute)
    }
}

/// T 方向に深くならず、F / X / Y の深さの合計が [`ShortRelativeFlexId::MAX_STEPS`] 段以内の [RelativeFlexId] を u16 に詰めたもの。
///
/// 下位9ビットに F / X / Y の深さを3ビットずつ、残りの7ビットに F / X / Y の順で各次元の下位ビットを並べる。
/// 段の数や次元によらず同じ手順で復元できるので、分岐せずに行き先を求められる。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ShortRelativeFlexId(u16);

impl ShortRelativeFlexId {
    pub const MAX_STEPS: u8 = 7;

    /// `relative` が T 方向に深いか、深さの合計が [`Self::MAX_STEPS`] 段を超えるなら [`None`]。
    pub fn new(relative: RelativeFlexId) -> Option<ShortRelativeFlexId> {
        let id = relative.0;
        let (zf, zx, zy) = (id.f_zoomlevel(), id.x_zoomlevel(), id.y_zoomlevel());
        if id.t_zoomlevel() != 0 || zf + zx + zy > Self::MAX_STEPS {
            return None;
        }
        let bits =
            id.f_index() as u16 | (id.x_index() as u16) << zf | (id.y_index() as u16) << (zf + zx);
        Some(ShortRelativeFlexId(
            u16::from(zf) | u16::from(zx) << 3 | u16::from(zy) << 6 | bits << 9,
        ))
    }

    fn zooms(self) -> [u8; 3] {
        [0, 3, 6].map(|shift| (self.0 >> shift) as u8 & 0b111)
    }

    /// 祖先より深くなっている次元の集合。
    pub fn deeper_dimensions(self) -> DimensionSet {
        [Dimension::F, Dimension::X, Dimension::Y]
            .into_iter()
            .zip(self.zooms())
            .filter(|&(_, zoom)| zoom != 0)
            .map(|(d, _)| d)
            .collect()
    }

    /// `ancestor` を原点として、[FlexId] を復元する。
    ///
    /// `ancestor` から数段深いだけなので、最大ズームを超えないことは呼び出し側（FlexTreeCore の Node）が保証する。
    pub fn to_absolute(self, ancestor: &FlexId) -> FlexId {
        let [zf, zx, zy] = self.zooms();
        let bits = u32::from(self.0 >> 9);
        let low = |shift: u8, zoom: u8| (bits >> shift) & ((1 << zoom) - 1);
        let f = (ancestor.f_index() << zf) + low(0, zf) as i32;
        let x = (ancestor.x_index() << zx) + low(zf, zx);
        let y = (ancestor.y_index() << zy) + low(zf + zx, zy);
        // SAFETY: Node の行き先は最大ズーム以下。インデックスは祖先の区間を `2^深さ` 等分したうちの1つなので、そのズームの範囲に収まる
        unsafe {
            FlexId::new_unchecked(
                ancestor.f_zoomlevel() + zf,
                f,
                ancestor.x_zoomlevel() + zx,
                x,
                ancestor.y_zoomlevel() + zy,
                y,
            )
            .time_with_segment_unchecked(ancestor.t_zoomlevel(), ancestor.t_index())
        }
    }
}

/// F / X / Y / T の各次元の最大ズーム。
const MAX_ZOOM: [u8; 4] = [
    ZoomLevel::MAX.get(),
    ZoomLevel::MAX.get(),
    ZoomLevel::MAX.get(),
    TZoomLevel::MAX.get(),
];

/// F / X / Y / T の各次元の `(ズーム, インデックス)`。
type Dimensions = [(u8, i64); 4];

fn dimensions(id: &FlexId) -> Dimensions {
    [
        (id.f_zoomlevel(), id.f_index() as i64),
        (id.x_zoomlevel(), id.x_index() as i64),
        (id.y_zoomlevel(), id.y_index() as i64),
        (id.t_zoomlevel(), id.t_index() as i64),
    ]
}

/// 2つの ID の各次元を `f` で組み合わせる。
fn zip_dimensions(
    a: &FlexId,
    b: &FlexId,
    f: impl Fn((u8, i64), (u8, i64)) -> (u8, i64),
) -> Dimensions {
    let (a, b) = (dimensions(a), dimensions(b));
    core::array::from_fn(|i| f(a[i], b[i]))
}

#[cfg(test)]
mod tests {
    use super::*;
    /// 自身を包含しない ID を祖先に渡すとエラーになる。
    #[test]
    fn relative_to_non_ancestor_is_error() {
        let id = FlexId::new(3, 1, 3, 2, 3, 3).unwrap();
        let sibling = FlexId::new(3, 0, 3, 2, 3, 3).unwrap();
        let child = FlexId::new(4, 2, 3, 2, 3, 3).unwrap();
        assert_eq!(
            id.relative_to(&sibling),
            Err(SpatialIdError::NotAncestor.into())
        );
        assert!(id.relative_to(&child).is_err());
    }

    /// 相対 ID は、作ったときの祖先から絶対 ID へ戻せる。
    #[test]
    fn to_absolute_restores_original() {
        let ancestor = FlexId::new(2, 1, 1, 0, 3, 5).unwrap();
        let id = FlexId::new(5, 12, 4, 7, 3, 5).unwrap();
        let relative = id.relative_to(&ancestor).unwrap();
        assert_eq!(relative.to_absolute(&ancestor), Ok(id));
    }

    /// 深い祖先に当てはめて最大ズームを超えるならエラーになる。
    #[test]
    fn to_absolute_beyond_max_zoom_is_error() {
        let deep = FlexId::new(30, 0, 0, 0, 0, 0).unwrap();
        let relative = deep.relative_to(&FlexId::UPPER_MAX).unwrap();
        assert_eq!(
            relative.to_absolute(&deep),
            Err(SpatialIdError::ZOutOfRange { z: 60 }.into())
        );
    }

    /// 各次元の深さを取得できる。
    #[test]
    fn depth_on_returns_depth_per_dimension() {
        let ancestor = FlexId::new(2, 1, 1, 0, 3, 5).unwrap();
        let descendant = FlexId::new(5, 12, 4, 7, 3, 5).unwrap();
        let rel = descendant.relative_to(&ancestor).unwrap();
        assert_eq!(rel.depth_on(Dimension::F), 3);
        assert_eq!(rel.depth_on(Dimension::X), 3);
        assert_eq!(rel.depth_on(Dimension::Y), 0);
        assert_eq!(rel.to_absolute(&ancestor).unwrap(), descendant);
    }
}
