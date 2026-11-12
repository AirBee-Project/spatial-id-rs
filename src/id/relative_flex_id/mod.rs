use crate::{
    Error, FlexId, SpatialIdError,
    id::{
        dimension::DimensionSet,
        zoom_level::{TZoomLevel, ZoomLevel},
    },
};

/// ある祖先の [FlexId] を起点とした相対的な位置を表す[FlexId]。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct RelativeFlexId(FlexId);

impl FlexId {
    /// `ancestor` を原点とした[RelativeFlexId]を返す。
    /// `ancestor`が完全に自身を包含していなければ [`SpatialIdError::NotAncestor`] を返す。
    #[inline]
    pub(crate) fn relative_to(&self, ancestor: &FlexId) -> Result<RelativeFlexId, Error> {
        if !ancestor.contains(self) {
            return Err(SpatialIdError::NotAncestor.into());
        }
        // 各次元で、祖先からの深さと、その深さぶんの下位ビットを求める
        let fz = self.f_zoomlevel() - ancestor.f_zoomlevel();
        let xz = self.x_zoomlevel() - ancestor.x_zoomlevel();
        let yz = self.y_zoomlevel() - ancestor.y_zoomlevel();
        let f = self.f_index() & ((1_i32 << fz) - 1);
        let x = self.x_index() & ((1_u32 << xz) - 1);
        let y = self.y_index() & ((1_u32 << yz) - 1);
        let tz = self.t_zoomlevel() - ancestor.t_zoomlevel();
        let t = self.t_index() & ((1_u64 << tz) - 1);
        // SAFETY: 祖先に含まれるので、深さは自身のズーム以下、下位ビットは `0..2^深さ` に収まる
        let relative = unsafe {
            FlexId::new_unchecked(fz, f, xz, x, yz, y).time_with_segment_unchecked(tz, t)
        };
        Ok(RelativeFlexId(relative))
    }
}

impl RelativeFlexId {
    /// 祖先から `dimension`において何段深いかを返す。
    #[cfg(test)]
    #[inline]
    pub fn depth_on(self, dimension: crate::id::dimension::Dimension) -> u8 {
        self.0.zoomlevel_on(dimension)
    }

    /// 祖先より深くなっている次元の集合。
    #[inline]
    pub fn deeper_dimensions(&self) -> DimensionSet {
        DimensionSet::from_bits(
            u8::from(self.0.f_zoomlevel() > 0)
                | (u8::from(self.0.x_zoomlevel() > 0) << 1)
                | (u8::from(self.0.y_zoomlevel() > 0) << 2)
                | (u8::from(self.0.t_zoomlevel() > 0) << 3),
        )
    }

    /// `ancestor` を原点として、[FlexId] を復元する。
    /// どれかの次元で最大ズームを超えるなら [`SpatialIdError::ZOutOfRange`] を返す。
    #[inline]
    pub fn to_absolute(self, ancestor: &FlexId) -> Result<FlexId, Error> {
        let r = &self.0;
        let fz = ancestor.f_zoomlevel() + r.f_zoomlevel();
        if fz > ZoomLevel::MAX.get() {
            return Err(SpatialIdError::ZOutOfRange { z: fz }.into());
        }
        let xz = ancestor.x_zoomlevel() + r.x_zoomlevel();
        if xz > ZoomLevel::MAX.get() {
            return Err(SpatialIdError::ZOutOfRange { z: xz }.into());
        }
        let yz = ancestor.y_zoomlevel() + r.y_zoomlevel();
        if yz > ZoomLevel::MAX.get() {
            return Err(SpatialIdError::ZOutOfRange { z: yz }.into());
        }
        let tz = ancestor.t_zoomlevel() + r.t_zoomlevel();
        if tz > TZoomLevel::MAX.get() {
            return Err(SpatialIdError::ZOutOfRange { z: tz }.into());
        }

        let f = (ancestor.f_index() << r.f_zoomlevel()) | r.f_index();
        let x = (ancestor.x_index() << r.x_zoomlevel()) | r.x_index();
        let y = (ancestor.y_index() << r.y_zoomlevel()) | r.y_index();
        let t = (ancestor.t_index() << r.t_zoomlevel()) | r.t_index();
        // SAFETY: ズームは上限以下。インデックスは祖先の区間を `2^深さ` 等分したうちの1つなので、そのズームの範囲に収まる
        let absolute = unsafe {
            FlexId::new_unchecked(fz, f, xz, x, yz, y).time_with_segment_unchecked(tz, t)
        };
        Ok(absolute)
    }

    /// [`encode`](Self::encode) / [`decode`](Self::decode) が扱う固定長バイト列の長さ。
    pub(crate) const ENCODED_LEN: usize = 20;

    /// 自身を固定長バイト列に変換する。
    pub(crate) fn encode(&self) -> [u8; Self::ENCODED_LEN] {
        let dimensions = dimensions(&self.0);
        let index = dimensions
            .iter()
            .zip(MAX_ZOOM)
            .rev()
            .fold(0u128, |packed, (&(_, i), bits)| packed << bits | i as u128);
        let mut out = [0; Self::ENCODED_LEN];
        out[..4].copy_from_slice(&dimensions.map(|(z, _)| z));
        out[4..].copy_from_slice(&index.to_le_bytes());
        out
    }

    /// 固定長バイト列から [`RelativeFlexId`] を復元する（[`encode`](Self::encode) の逆変換）。
    pub(crate) fn decode(bytes: &[u8; Self::ENCODED_LEN]) -> Result<Self, Error> {
        let [fz, xz, yz, tz, ..] = *bytes;
        let mut index = u128::from_le_bytes(bytes[4..].try_into().unwrap());
        let [f, x, y, t] = MAX_ZOOM.map(|bits| {
            let i = index & ((1 << bits) - 1);
            index >>= bits;
            i as u64
        });
        let relative =
            FlexId::new(fz, f as i32, xz, x as u32, yz, y as u32)?.time_with_segment(tz, t)?;
        Ok(RelativeFlexId(relative))
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
    #[inline]
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

    #[inline]
    fn zooms(self) -> [u8; 3] {
        [
            (self.0 & 0b111) as u8,
            ((self.0 >> 3) & 0b111) as u8,
            ((self.0 >> 6) & 0b111) as u8,
        ]
    }

    /// 祖先より深くなっている次元の集合。
    #[inline]
    pub fn deeper_dimensions(self) -> DimensionSet {
        let raw = self.0;
        DimensionSet::from_bits(
            u8::from((raw & 0b111) != 0)
                | (u8::from(((raw >> 3) & 0b111) != 0) << 1)
                | (u8::from(((raw >> 6) & 0b111) != 0) << 2),
        )
    }

    /// `ancestor` を原点として、[FlexId] を復元する。
    ///
    /// `ancestor` から数段深いだけなので、最大ズームを超えないことは呼び出し側（FlexTreeCore の Node）が保証する。
    #[inline]
    pub fn to_absolute(self, ancestor: &FlexId) -> FlexId {
        let [zf, zx, zy] = self.zooms();
        let bits = u32::from(self.0 >> 9);
        let low = |shift: u8, zoom: u8| (bits >> shift) & ((1 << zoom) - 1);
        let f = (ancestor.f_index() << zf) | low(0, zf) as i32;
        let x = (ancestor.x_index() << zx) | low(zf, zx);
        let y = (ancestor.y_index() << zy) | low(zf + zx, zy);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::dimension::Dimension;
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

    #[test]
    fn encode_round_trips() {
        let ancestor = FlexId::new(2, 1, 1, 0, 3, 5).unwrap();
        let descendant = FlexId::new(5, 12, 4, 7, 3, 5).unwrap();
        let rel = descendant.relative_to(&ancestor).unwrap();
        let encoded = rel.encode();
        assert_eq!(RelativeFlexId::decode(&encoded).unwrap(), rel);
    }
}
