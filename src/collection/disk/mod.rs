//! ディスク永続化コレクション。

mod core;
mod io;
mod value;

pub use self::core::FlexTreeCore;
pub use self::value::DiskValue;
