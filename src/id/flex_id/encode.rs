use crate::{FlexId, id::zoom_level::ZoomLevel};

impl FlexId {
    /// [`encode`](Self::encode) / [`decode`](Self::decode) が扱う固定長バイト列の長さ。
    ///
    /// `temporal_id` feature の有無で軸数（3軸 or 4軸）が変わるため、長さも変わる。
    /// バッファ確保などで長さが必要な場合は、ハードコードせずこの定数を使うこと。
    #[cfg(feature = "temporal_id")]
    pub const ENCODED_LEN: usize = 23;
    #[cfg(not(feature = "temporal_id"))]
    pub const ENCODED_LEN: usize = 14;

    /// 自身を固定長バイト列に変換する。
    ///
    /// # フォーマット
    ///
    /// ```text
    /// byte 0      : [zf: 5bit][zx: 5bit 上位3bit]
    /// byte 1      : [zx: 5bit 下位2bit][zy: 5bit][padding: 1bit]
    /// byte 2..=5  : f_shifted (u32, big-endian)
    ///               f_shifted = f_index - f_min(zf)  （0 以上の符号なし整数として格納）
    /// byte 6..=9  : x_index (u32, big-endian)
    /// byte 10..=13: y_index (u32, big-endian)
    /// byte 14     : t_zoomlevel (u8)
    /// byte 15..=22: t_index (u64, big-endian)
    /// ```
    ///
    /// `temporal_id` feature 無効時は byte 14 以降が存在せず、14バイトで終わる。
    ///
    /// # 動作例
    ///
    /// ```
    /// # use spatial_id::FlexId;
    /// let id = FlexId::new(5, 3, 2, 3, 10, 1).unwrap();
    /// let bytes = id.encode();
    /// assert_eq!(bytes.len(), FlexId::ENCODED_LEN);
    /// let decoded = FlexId::decode(&bytes).unwrap();
    /// assert_eq!(decoded, id);
    /// ```
    pub fn encode(&self) -> [u8; Self::ENCODED_LEN] {
        let zf = self.f_zoomlevel();
        let zx = self.x_zoomlevel();
        let zy = self.y_zoomlevel();

        // f_index をオフセット付きの符号なし整数へ変換
        let f_min = self.f_zoomlevel.f_min();
        let f_shifted = (self.f_index() - f_min) as u32;

        // ズームレベル 3 つを 15 ビットに詰める
        // byte 0: [zf(5bit)][zx(5bit) 上位3bit]
        // byte 1: [zx(5bit) 下位2bit][zy(5bit)][padding(1bit)=0]
        let header = ((zf as u16) << 10) | ((zx as u16) << 5) | (zy as u16);
        let byte0 = (header >> 7) as u8;
        let byte1 = ((header & 0x7F) << 1) as u8; // 下位 7bit を左に 1bit シフト（padding bit = 0）

        let mut out = [0u8; Self::ENCODED_LEN];
        out[0] = byte0;
        out[1] = byte1;
        out[2..6].copy_from_slice(&f_shifted.to_be_bytes());
        out[6..10].copy_from_slice(&self.x_index().to_be_bytes());
        out[10..14].copy_from_slice(&self.y_index().to_be_bytes());

        #[cfg(feature = "temporal_id")]
        {
            out[14] = self.t_zoomlevel();
            out[15..23].copy_from_slice(&self.t_index().to_be_bytes());
        }

        out
    }

    /// 固定長バイト列から [`FlexId`] を復元する（[`encode`](Self::encode) の逆変換）。
    ///
    /// # バリデーション
    /// - ズームレベルが [`ZoomLevel::MAX`] を超える場合は [`crate::SpatialIdError::ZOutOfRange`] を返す。
    /// - 復元した `f_index` / `x_index` / `y_index` が各ズームレベルの範囲外の場合は
    ///   各種範囲外エラーを返す。
    /// - `temporal_id` feature 有効時は、`t_zoomlevel` / `t_index` も
    ///   [`time_with_segment`](Self::time_with_segment) と同じ検証を通す。
    ///
    /// # 動作例
    ///
    /// ```
    /// # use spatial_id::FlexId;
    /// let id = FlexId::new(5, 3, 2, 3, 10, 1).unwrap();
    /// let decoded = FlexId::decode(&id.encode()).unwrap();
    /// assert_eq!(decoded, id);
    /// ```
    pub fn decode(bytes: &[u8; Self::ENCODED_LEN]) -> Result<Self, crate::error::Error> {
        // ヘッダー (15 ビット) を復元
        let header = ((bytes[0] as u16) << 7) | ((bytes[1] as u16) >> 1);
        let zf = ((header >> 10) & 0x1F) as u8;
        let zx = ((header >> 5) & 0x1F) as u8;
        let zy = (header & 0x1F) as u8;

        // ズームレベルのバリデーション
        if zf > ZoomLevel::MAX.get() {
            return Err(crate::SpatialIdError::ZOutOfRange { z: zf }.into());
        }
        if zx > ZoomLevel::MAX.get() {
            return Err(crate::SpatialIdError::ZOutOfRange { z: zx }.into());
        }
        if zy > ZoomLevel::MAX.get() {
            return Err(crate::SpatialIdError::ZOutOfRange { z: zy }.into());
        }

        let f_shifted = u32::from_be_bytes(bytes[2..6].try_into().unwrap());
        let x_index = u32::from_be_bytes(bytes[6..10].try_into().unwrap());
        let y_index = u32::from_be_bytes(bytes[10..14].try_into().unwrap());

        // f_index をオフセットから実際の値へ戻す
        let f_min = ZoomLevel::new(zf).unwrap().f_min();
        let f_index = f_shifted as i64 + f_min as i64;

        let id = FlexId::new(zf, f_index as i32, zx, x_index, zy, y_index)?;

        #[cfg(feature = "temporal_id")]
        {
            let t_zoomlevel = bytes[14];
            let t_index = u64::from_be_bytes(bytes[15..23].try_into().unwrap());
            id.time_with_segment(t_zoomlevel, t_index)
        }

        #[cfg(not(feature = "temporal_id"))]
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use crate::FlexId;

    #[test]
    fn round_trips() {
        let id = FlexId::new(5, 3, 2, 3, 10, 1).unwrap();
        let decoded = FlexId::decode(&id.encode()).unwrap();
        assert_eq!(decoded, id);
    }

    /// 空間座標が同じで時間だけが異なる2つの [`FlexId`] は区別できなければならない
    /// （区別できないと、時間で分割したノードをこのバイト列をキーに永続化したとき、
    /// 空間座標が同じで時間が違うだけの兄弟ノードが同一キーへ衝突して上書きし合う）。
    #[cfg(feature = "temporal_id")]
    #[test]
    fn distinguishes_same_space_different_time() {
        let base = FlexId::new(10, 3, 12, 909, 12, 403).unwrap();
        let a = base.time_with_segment(20u8, 5).unwrap();
        let b = base.time_with_segment(20u8, 6).unwrap();

        assert_ne!(a.encode(), b.encode());
    }

    /// 往復変換で元の値（空間3軸＋時間軸）が厳密に戻ること。ズーム0（全時間）と
    /// 最深ズーム（35）の両端、および中間の値で確認する。
    #[cfg(feature = "temporal_id")]
    #[test]
    fn round_trips_with_time() {
        let cases = [(0u8, 0u64), (35, (1u64 << 35) - 1), (20, 12345)];
        for (t_zoomlevel, t_index) in cases {
            let id = FlexId::new(18, -5, 18, 232848, 18, 103242)
                .unwrap()
                .time_with_segment(t_zoomlevel, t_index)
                .unwrap();
            let bytes = id.encode();
            assert_eq!(bytes.len(), FlexId::ENCODED_LEN);
            let decoded = FlexId::decode(&bytes).unwrap();
            assert_eq!(decoded, id);
            assert_eq!(decoded.t_zoomlevel(), t_zoomlevel);
            assert_eq!(decoded.t_index(), t_index);
        }
    }
}
