use std::io;
use std::mem::MaybeUninit;
use std::ops::{Deref, DerefMut};
use std::path::Path;

use alloc::boxed::Box;
use bytemuck::{CheckedBitPattern, NoUninit, Zeroable};
use compio::buf::{IoBuf, IoBufMut, SetLen};
use compio::fs::{File, OpenOptions};
use compio::io::{AsyncReadAtExt, AsyncWriteAtExt};

use crate::error::DiskError;

/// Direct I/O でファイルを開く。
pub(crate) async fn open_direct(path: impl AsRef<Path>, options: &OpenOptions) -> io::Result<File> {
    let mut options = options.clone();
    #[cfg(all(unix, not(target_vendor = "apple")))]
    options.custom_flags(libc::O_DIRECT);

    #[cfg(windows)]
    options.custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_NO_BUFFERING);
    let file = options.open(path).await?;

    #[cfg(target_vendor = "apple")]
    {
        use std::os::fd::AsRawFd;
        if unsafe { libc::fcntl(file.as_raw_fd(), libc::F_NOCACHE, 1) } == -1 {
            return Err(io::Error::last_os_error());
        }
    }

    Ok(file)
}

#[repr(C, align(4096))]
#[derive(Debug, Zeroable)]
pub(crate) struct Page([u8; Page::SIZE]);

const _: () = assert!(align_of::<Page>() == Page::SIZE);

impl Page {
    pub(crate) const SIZE: usize = 4096;

    /// 中身に使えるバイト長。末尾の 4 バイトは CRC32C チェックサム。
    pub(crate) const CAPACITY: usize = Self::SIZE - size_of::<u32>();

    pub(crate) fn new() -> Box<Self> {
        bytemuck::zeroed_box()
    }

    /// 先頭に `value` を書き、残りをゼロ埋めし、末尾にチェックサムを付与したページを生成する。
    pub(crate) fn of<T: NoUninit>(value: &T) -> Box<Self> {
        const { assert!(size_of::<T>() <= Page::CAPACITY) };
        let bytes = bytemuck::bytes_of(value);
        let mut page = Self::new();
        page[..bytes.len()].copy_from_slice(bytes);
        let checksum = page.checksum();
        page[Self::CAPACITY..].copy_from_slice(&checksum.to_le_bytes());
        page
    }

    /// 先頭を `T` として読み取る。チェックサムが一致しないか、`T` として不正なビット列ならエラーを返す。
    pub(crate) fn try_head_as<T: CheckedBitPattern>(&self) -> Result<&T, DiskError> {
        const { assert!(size_of::<T>() <= Page::CAPACITY) };

        if self[Self::CAPACITY..] != self.checksum().to_le_bytes() {
            return Err(DiskError::Corrupt);
        }
        Ok(bytemuck::checked::try_from_bytes(&self[..size_of::<T>()])?)
    }

    fn checksum(&self) -> u32 {
        crc32c::crc32c(&self[..Self::CAPACITY])
    }
}

/// ページを末尾に追記していくファイル。
pub(crate) struct PageFile {
    file: File,
    /// ページ数。
    len: u64,
}

impl PageFile {
    /// ファイルを開く。存在しなければ空のファイルを作る。
    pub(crate) async fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true);
        let file = open_direct(path, &options).await?;
        let len = file.metadata().await?.len() / Page::SIZE as u64;
        Ok(PageFile { file, len })
    }

    pub(crate) fn len(&self) -> u64 {
        self.len
    }

    /// `index` 番目のページを読み出す。
    pub(crate) async fn read(&self, index: u64) -> Result<Box<Page>, DiskError> {
        if index >= self.len {
            return Err(DiskError::Corrupt);
        }
        let (result, page) = self
            .file
            .read_exact_at(Page::new(), index * Page::SIZE as u64)
            .await
            .into();
        result?;
        Ok(page)
    }

    /// 末尾にページを足し、その番号を返す。
    pub(crate) async fn append(&mut self, page: Box<Page>) -> io::Result<u64> {
        let index = self.len;
        self.file
            .write_all_at(page, index * Page::SIZE as u64)
            .await
            .0?;
        self.len += 1;
        Ok(index)
    }

    /// 先頭の `len` ページだけを残す。
    pub(crate) async fn truncate(&mut self, len: u64) -> io::Result<()> {
        self.file.set_len(len * Page::SIZE as u64).await?;
        self.len = len;
        Ok(())
    }

    /// ここまでに書いたページを永続化する。
    pub(crate) async fn sync(&self) -> io::Result<()> {
        self.file.sync_data().await
    }

    pub(crate) async fn close(self) -> io::Result<()> {
        self.file.close().await
    }
}

impl Deref for Page {
    type Target = [u8; Self::SIZE];
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Page {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl IoBuf for Page {
    fn as_init(&self) -> &[u8] {
        &self.0
    }
}

impl IoBufMut for Page {
    fn as_uninit(&mut self) -> &mut [MaybeUninit<u8>] {
        self.0.as_uninit()
    }
}

impl SetLen for Page {
    unsafe fn set_len(&mut self, len: usize) {
        debug_assert!(len <= Self::SIZE);
    }
}
