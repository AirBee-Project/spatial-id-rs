use crate::query::SafeValue;
use crate::query::{query_iter::QueryIter, traits::UnaryOperator};
use crate::{Error, FlexId, RangeId, SpatialIdMap, ZoomLevel};

/// FlexId単体に対する実装
pub(crate) mod primitive;

/// Query型への実装
pub(crate) mod query;

#[cfg(test)]
mod test;

/// 入力の各 FlexId を、ズームレベル `z` のインデックス値で F・X・Y 方向へそれぞれ `f`・`x`・`y` 個分だけ平行移動する単項演算。
pub struct Shift {
    z: ZoomLevel,
    f: i32,
    x: i32,
    y: i32,
}

impl Shift {
    /// ズーム `z` のインデックス値で F・X・Y 方向へ `f`・`x`・`y` 個分移す演算子を作る。
    pub fn new<T: Into<u8>>(z: T, f: i32, x: i32, y: i32) -> Result<Self, Error> {
        let z = ZoomLevel::new(z.into())?;
        Ok(Self { z, f, x, y })
    }

    /// `id` を移した先を、互いに素な [`FlexId`] で返す。移せなければエラーを返す。
    fn shift(&self, id: &FlexId) -> Result<impl Iterator<Item = FlexId> + use<>, Error> {
        let (z, x, y) = (self.z.get(), self.x, self.y);
        // 移せるかは各次元の成分だけで決まるので、元の `id` で確かめておけば、割れた片では失敗しない
        id.shift_x(z, x).map(drop)?;
        id.shift_y(z, y).map(drop)?;
        Ok(id
            .shift_f(z, self.f)?
            .flat_map(move |id| id.shift_x(z, x).into_iter().flatten())
            .flat_map(move |id| id.shift_y(z, y).into_iter().flatten()))
    }
}

impl<V: SafeValue + 'static> UnaryOperator<V> for Shift {
    fn validate(&self) -> Result<(), Error> {
        self.z.check_f(self.f)?;
        self.z.check_x(self.x.unsigned_abs())?;
        self.z.check_y(self.y.unsigned_abs())?;
        Ok(())
    }

    fn run<'a>(&'a self, input: QueryIter<'a, V>) -> Result<QueryIter<'a, V>, Error> {
        if (self.f, self.x, self.y) == (0, 0, 0) {
            return Ok(input);
        }

        // 移動の桁上がりで上位のビットが変わった要素は FlexId の PartialOrd で遠くへ跳ぶので、木に集めて並べ直す
        let move_into = |mut table: SpatialIdMap<V>, (id, value): &(FlexId, V)| {
            table.extend(self.shift(id)?.map(|moved| (moved, value.clone())));
            Ok::<_, Error>(table)
        };

        #[cfg(not(feature = "rayon"))]
        let moved = {
            let mut input = input;
            input.try_fold(SpatialIdMap::new(), |table, item| move_into(table, &item?))?
        };

        #[cfg(feature = "rayon")]
        let moved = {
            use rayon::prelude::*;
            // 入力を区切って読むので、一度に持つ入力はこの数までに収まる
            const CHUNK: usize = 1 << 16;
            let union = |a: SpatialIdMap<V>, b: SpatialIdMap<V>| a.union(&b);
            let mut input = input.peekable();
            // 塊ごとに蓄積した木へ union すると、木全体の複製が塊の数だけ走るので、最後に二分木状にまとめる
            let mut parts = Vec::new();
            while input.peek().is_some() {
                let chunk: Vec<(FlexId, V)> =
                    input.by_ref().take(CHUNK).collect::<Result<_, _>>()?;
                parts.push(
                    chunk
                        .par_iter()
                        .try_fold(SpatialIdMap::new, move_into)
                        .try_reduce(SpatialIdMap::new, |a, b| Ok(union(a, b)))?,
                );
            }
            parts.into_par_iter().reduce(SpatialIdMap::new, union)
        };

        Ok(moved.into())
    }

    fn reverse_target(&self, bounds: RangeId, out: &mut alloc::vec::Vec<RangeId>) {
        let (f, x, y) = (-(self.f as i64), -(self.x as i64), -(self.y as i64));
        bounds.shift_range(self.z.get(), [f, f], [x, x], [y, y], out);
    }
}
