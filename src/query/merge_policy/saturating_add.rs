mod sealed {
    pub trait Sealed {}
}

/// [`Sum`](super::Sum) と [`Average`](super::Average) が値を足すときに使うTrait。
///
/// 整数型はオーバーフローすると最大値で止まり、浮動小数点数型は通常の足し算を行う。
/// 実装はこのクレートの数値型に限り、外部の型には実装できない。
pub trait SaturatingAdd: Sized + sealed::Sealed {
    fn saturating_add(self, rhs: Self) -> Self;
}

macro_rules! impl_saturating_add_int {
    ($($t:ty),* $(,)?) => {
        $(
            impl sealed::Sealed for $t {}
            impl SaturatingAdd for $t {
                fn saturating_add(self, rhs: Self) -> Self {
                    <$t>::saturating_add(self, rhs)
                }
            }
        )*
    };
}
impl_saturating_add_int!(
    i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize
);

macro_rules! impl_saturating_add_float {
    ($($t:ty),* $(,)?) => {
        $(
            impl sealed::Sealed for $t {}
            impl SaturatingAdd for $t {
                fn saturating_add(self, rhs: Self) -> Self {
                    self + rhs
                }
            }
        )*
    };
}
impl_saturating_add_float!(f32, f64);
