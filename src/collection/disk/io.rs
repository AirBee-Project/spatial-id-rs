use alloc::boxed::Box;
use compio::buf::{IoBuf, IoBufMut, SetLen};
use compio::fs::{File, OpenOptions};
use compio::io::{AsyncReadAtExt, AsyncWriteAtExt};
use compio::runtime::Runtime;
use std::io;
use std::mem::MaybeUninit;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::thread;
use zerocopy::{FromZeros, Immutable, IntoBytes, KnownLayout, TryFromBytes};

use crate::error::DiskError;

#[repr(C, align(4096))]
#[derive(Debug, FromZeros)]
pub(crate) struct Page([u8; Page::SIZE]);

const _: () = assert!(align_of::<Page>() == Page::SIZE);

impl Page {
    pub(crate) const SIZE: usize = 4096;

    /// 中身に使えるバイト長。末尾の 4 バイトは CRC32C チェックサム。
    pub(crate) const CAPACITY: usize = Self::SIZE - size_of::<u32>();

    pub(crate) fn new() -> Box<Self> {
        Self::new_box_zeroed().unwrap_or_else(|_| {
            alloc::alloc::handle_alloc_error(core::alloc::Layout::new::<Self>())
        })
    }

    /// 先頭に `value` を書き、残りをゼロ埋めし、末尾にチェックサムを付与したページを生成する。
    pub(crate) fn of<T: IntoBytes + Immutable>(value: &T) -> Box<Self> {
        const { assert!(size_of::<T>() <= Page::CAPACITY) };
        let bytes = value.as_bytes();
        let mut page = Self::new();
        page[..bytes.len()].copy_from_slice(bytes);
        let checksum = page.checksum();
        page[Self::CAPACITY..].copy_from_slice(&checksum.to_le_bytes());
        page
    }

    /// 先頭を `T` として読み取る。チェックサムが一致しないか、`T` として不正なビット列ならエラーを返す。
    pub(crate) fn try_head_as<T: TryFromBytes + KnownLayout + Immutable>(
        &self,
    ) -> Result<&T, DiskError> {
        const { assert!(size_of::<T>() <= Page::CAPACITY) };

        if self[Self::CAPACITY..] != self.checksum().to_le_bytes() {
            return Err(DiskError::Corrupt);
        }
        T::try_ref_from_prefix(&self[..Self::CAPACITY])
            .map(|(head, _)| head)
            .map_err(|_| DiskError::Corrupt)
    }

    /// `index` 番目のページのファイル上の位置。
    fn offset(index: u64) -> u64 {
        index * Page::SIZE as u64
    }

    fn checksum(&self) -> u32 {
        crc32c::crc32c(&self[..Self::CAPACITY])
    }
}

/// ページ単位で読み書きするファイル。
///
/// I/O は専用スレッドで動く compio ランタイム（io_uring / IOCP）が担うため、呼び出し側の非同期ランタイムは問わない。
#[derive(Debug)]
pub(crate) struct PagedFile {
    path: PathBuf,
    requests: flume::Sender<Request>,
    /// ページ数。
    len: u64,
}

impl PagedFile {
    /// ファイルを開く。存在しなければ空のファイルを作る。
    pub(crate) async fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref().to_path_buf();
        let (requests, receiver) = flume::unbounded();
        let (opened, ready) = flume::bounded(1);
        let path_clone = path.clone();
        thread::Builder::new()
            .name("spatial-id-disk".into())
            .spawn(move || Self::serve(&path_clone, opened, receiver))?;
        let len = ready.recv_async().await.map_err(|_| Self::stopped())??;
        Ok(PagedFile {
            path,
            requests,
            len,
        })
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn len(&self) -> u64 {
        self.len
    }

    /// `index` 番目のページを読み出す。
    pub(crate) async fn read(&self, index: u64) -> Result<Box<Page>, DiskError> {
        if index >= self.len {
            return Err(DiskError::Corrupt);
        }
        Ok(self.request(|reply| Request::Read { index, reply }).await?)
    }

    /// `index` 番目のページに書く。ファイルの末尾より先なら、ファイルを伸ばす。
    pub(crate) async fn write(&mut self, index: u64, page: Box<Page>) -> io::Result<()> {
        self.request(|reply| Request::Write { index, page, reply })
            .await?;
        self.len = self.len.max(index + 1);
        Ok(())
    }

    /// ここまでに書いたページを永続化する。
    pub(crate) async fn sync(&self) -> io::Result<()> {
        self.request(Request::Sync).await
    }

    /// ファイルを閉じ、専用スレッドを止める。閉じる際のエラーが要らなければ drop するだけでよい。
    pub(crate) async fn close(self) -> io::Result<()> {
        self.request(Request::Close).await
    }

    async fn request<T>(&self, request: impl FnOnce(Reply<T>) -> Request) -> io::Result<T> {
        let (reply, response) = flume::bounded(1);
        self.requests
            .send(request(reply))
            .map_err(|_| Self::stopped())?;
        response.recv_async().await.map_err(|_| Self::stopped())?
    }

    /// Direct I/O でファイルを開く。
    async fn open_direct(path: impl AsRef<Path>, options: &OpenOptions) -> io::Result<File> {
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

    /// 専用スレッドの本体。ファイルを開いてページ数を `opened` に返し、要求を処理し続ける。
    fn serve(path: &Path, opened: Reply<u64>, requests: flume::Receiver<Request>) {
        let runtime = match Runtime::new() {
            Ok(runtime) => runtime,
            Err(err) => {
                let _ = opened.send(Err(err));
                return;
            }
        };
        runtime.block_on(async {
            let mut options = OpenOptions::new();
            options.read(true).write(true).create(true);
            let mut file = match Self::open_direct(path, &options).await {
                Ok(file) => file,
                Err(err) => {
                    let _ = opened.send(Err(err));
                    return;
                }
            };
            let _ = opened.send(file.metadata().await.map(|m| m.len() / Page::SIZE as u64));

            while let Ok(request) = requests.recv_async().await {
                match request {
                    // 読み込みは互いに独立なので、完了を待たずに次の要求を受け付けて並行に投入する。
                    // close は複製した File がすべて drop されるのを待つため、読み込み中に閉じても安全。
                    Request::Read { index, reply } => {
                        let file = file.clone();
                        compio::runtime::spawn(async move {
                            let (result, page) = file
                                .read_exact_at(Page::new(), Page::offset(index))
                                .await
                                .into();
                            let _ = reply.send(result.map(|()| page));
                        })
                        .detach();
                    }
                    Request::Write { index, page, reply } => {
                        let _ = reply.send(file.write_all_at(page, Page::offset(index)).await.0);
                    }
                    Request::Sync(reply) => {
                        let _ = reply.send(file.sync_data().await);
                    }
                    Request::Close(reply) => {
                        let _ = reply.send(file.close().await);
                        return;
                    }
                }
            }
            let _ = file.close().await;
        });
    }

    fn stopped() -> io::Error {
        io::Error::other("ディスク I/O スレッドが停止した")
    }
}

type Reply<T> = flume::Sender<io::Result<T>>;

enum Request {
    Read {
        index: u64,
        reply: Reply<Box<Page>>,
    },
    Write {
        index: u64,
        page: Box<Page>,
        reply: Reply<()>,
    },
    Sync(Reply<()>),
    Close(Reply<()>),
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
