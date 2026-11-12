use std::path::Path;

use crate::Error;
use crate::collection::disk::io::PagedFile;

pub(crate) mod node;

/// ディスク上の空間インデックス。呼び出し側の非同期ランタイムは問わない。
#[derive(Debug)]
pub struct FlexTreeCore {
    file: PagedFile,
}

impl FlexTreeCore {
    /// ファイルを開く。存在しなければ空のファイルを作る。
    pub async fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        let file = PagedFile::open(path).await?;
        Ok(FlexTreeCore { file })
    }

    pub fn path(&self) -> &Path {
        self.file.path()
    }

    /// ファイルを閉じる。閉じる際のエラーが要らなければ drop するだけでよい。
    pub async fn close(self) -> Result<(), Error> {
        Ok(self.file.close().await?)
    }
}
