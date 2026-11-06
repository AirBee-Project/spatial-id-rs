use super::FalloffAxis;
use crate::{FlexId, ZoomLevel};
use alloc::vec::Vec;

impl FlexId {
    /// `range` の各距離について、移した先と距離を `emit` へ渡す。`range` は `Falloff::new` で確かめておく。
    pub(crate) fn falloff_each(
        &self,
        axis: FalloffAxis,
        zoom: ZoomLevel,
        [start, end]: [i32; 2],
        moved: &mut Vec<FlexId>,
        mut emit: impl FnMut(&[FlexId], i32),
    ) {
        let z = zoom.get();
        for d in start..=end {
            moved.clear();
            // 軸の端を越える距離は移せないので、その距離の寄与は捨てる
            match axis {
                FalloffAxis::F => moved.extend(self.shift_f(z, d).into_iter().flatten()),
                FalloffAxis::X => moved.extend(self.shift_x(z, d).into_iter().flatten()),
                FalloffAxis::Y => moved.extend(self.shift_y(z, d).into_iter().flatten()),
            }
            if !moved.is_empty() {
                emit(moved, d);
            }
        }
    }
}
