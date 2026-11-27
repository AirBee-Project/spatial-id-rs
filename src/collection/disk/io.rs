use crate::error::DiskError;
use alloc::boxed::Box;
use alloc::vec::Vec;
use compio::buf::{IoBuf, IoBufMut, SetLen};
use compio::fs::{File, OpenOptions};
use compio::io::{AsyncReadAtExt, AsyncWriteAtExt};
use compio::runtime::Runtime;
use std::io;
use std::mem::MaybeUninit;
use std::path::Path;
use std::thread;
use zerocopy::{FromZeros, Immutable, IntoBytes, KnownLayout, TryFromBytes};

/// ファイル先頭からの 0 始まりのページ番号。
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct PageId(pub(crate) u64);

#[repr(C, align(4096))]
#[derive(FromZeros)]
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
        page.0[..bytes.len()].copy_from_slice(bytes);
        let checksum = page.checksum();
        page.0[Self::CAPACITY..].copy_from_slice(&checksum.to_be_bytes());
        page
    }

    /// 先頭を `T` として読み取る。チェックサムが一致しないか、`T` として不正なビット列ならエラーを返す。
    pub(crate) fn try_head_as<T: TryFromBytes + KnownLayout + Immutable>(
        &self,
    ) -> Result<&T, DiskError> {
        const { assert!(size_of::<T>() <= Page::CAPACITY) };

        if self.0[Self::CAPACITY..] != self.checksum().to_be_bytes() {
            return Err(DiskError::Corrupt);
        }
        T::try_ref_from_prefix(&self.0[..Self::CAPACITY])
            .map(|(head, _)| head)
            .map_err(|_| DiskError::Corrupt)
    }

    /// `id` が示すページのファイル上の位置。
    fn offset(id: PageId) -> u64 {
        id.0 * Page::SIZE as u64
    }

    fn checksum(&self) -> u32 {
        crc32c::crc32c(&self.0[..Self::CAPACITY])
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

/// ページ単位で読み書きするファイル。
///
/// I/O は専用スレッドで動く compio ランタイム（io_uring / IOCP）が担うため、呼び出し側の非同期ランタイムは問わない。
#[derive(Debug)]
pub(crate) struct PagedFile {
    requests: flume::Sender<Request>,
    /// 割り当て済みのページ数。開いた時点のファイルのページ数から始まり、書く前の割り当てでも増える。
    len: u64,
}

impl PagedFile {
    /// ファイルを開く。存在しなければ空のファイルを作る。
    pub(crate) async fn open(path: impl AsRef<Path>) -> Result<Self, DiskError> {
        let path = path.as_ref().to_path_buf();
        let (requests, receiver) = flume::unbounded();
        let (opened, ready) = flume::bounded(1);
        thread::Builder::new()
            .name("spatial-id-disk".into())
            .spawn(move || Self::serve(&path, opened, receiver))?;
        let len = ready
            .recv_async()
            .await
            .map_err(|_| DiskError::WorkerStopped)??;
        Ok(PagedFile { requests, len })
    }

    pub(crate) fn len(&self) -> u64 {
        self.len
    }

    /// 先頭の `len` ページを割り当て済みにする。
    pub(crate) fn reserve(&mut self, len: u64) {
        self.len = self.len.max(len);
    }

    /// 末尾に 1 ページを割り当てる。
    pub(crate) fn alloc(&mut self) -> PageId {
        let id = PageId(self.len);
        self.len += 1;
        id
    }

    /// `id` が示すページを読み出す。
    pub(crate) async fn read(&self, id: PageId) -> Result<Box<Page>, DiskError> {
        if id.0 >= self.len {
            return Err(DiskError::Corrupt);
        }
        self.call(|reply| Request::Read { id, reply }).await
    }

    /// `ids` が示すページを、`ids` の順に読み出す。
    ///
    /// 要求をすべて送ってから応答を待つので、各ページの読み込みは並行に進む。
    pub(crate) async fn read_many(&self, ids: &[PageId]) -> Result<Vec<Box<Page>>, DiskError> {
        let mut pages = Vec::with_capacity(ids.len());
        for response in self.send_reads(ids)? {
            pages.push(Self::receive(response).await?);
        }
        Ok(pages)
    }

    /// [`read_many`](Self::read_many) と同じだが、応答を待つ間は呼び出したスレッドを止める。
    pub(crate) fn read_many_sync(&self, ids: &[PageId]) -> Result<Vec<Box<Page>>, DiskError> {
        self.send_reads(ids)?
            .into_iter()
            .map(|response| Ok(response.recv().map_err(|_| DiskError::WorkerStopped)??))
            .collect()
    }

    /// `ids` が示すページの読み込みをすべて専用スレッドへ送り、応答を待つ受け口を `ids` の順に返す。
    fn send_reads(&self, ids: &[PageId]) -> Result<Vec<Response<Box<Page>>>, DiskError> {
        if ids.iter().any(|id| id.0 >= self.len) {
            return Err(DiskError::Corrupt);
        }
        ids.iter()
            .map(|&id| self.send(|reply| Request::Read { id, reply }))
            .collect()
    }

    /// 割り当て済みの `id` が示すページに書く。ファイルの末尾より先なら、ファイルを伸ばす。
    pub(crate) async fn write(&self, id: PageId, page: Box<Page>) -> Result<(), DiskError> {
        debug_assert!(id.0 < self.len);
        self.call(|reply| Request::Write { id, page, reply }).await
    }

    /// ファイルを先頭の `len` ページに切り詰める。
    ///
    /// 切り詰めは永続化を待たない。落ちて切り詰めが消えても、残るのはどこからも辿られないページだけになる。
    pub(crate) async fn truncate(&mut self, len: u64) -> Result<(), DiskError> {
        self.call(|reply| Request::Truncate { len, reply }).await?;
        self.len = len;
        Ok(())
    }

    /// ここまでに書いたページを永続化する。
    pub(crate) async fn sync(&self) -> Result<(), DiskError> {
        self.call(Request::Sync).await
    }

    /// ファイルを閉じ、専用スレッドを止める。閉じる際のエラーが要らなければ drop するだけでよい。
    pub(crate) async fn close(self) -> Result<(), DiskError> {
        self.call(Request::Close).await
    }

    /// 要求を専用スレッドへ送り、応答を待つ。
    async fn call<T>(&self, request: impl FnOnce(Reply<T>) -> Request) -> Result<T, DiskError> {
        Self::receive(self.send(request)?).await
    }

    /// 要求を専用スレッドへ送り、応答を待つ受け口を返す。
    fn send<T>(&self, request: impl FnOnce(Reply<T>) -> Request) -> Result<Response<T>, DiskError> {
        let (reply, response) = flume::bounded(1);
        self.requests
            .send(request(reply))
            .map_err(|_| DiskError::WorkerStopped)?;
        Ok(response)
    }

    /// [`send`](Self::send) で送った要求の応答を待つ。
    async fn receive<T>(response: Response<T>) -> Result<T, DiskError> {
        Ok(response
            .recv_async()
            .await
            .map_err(|_| DiskError::WorkerStopped)??)
    }

    /// Direct I/O でファイルを開く。
    async fn open_direct(path: &Path) -> io::Result<File> {
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true);
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
            let mut file = match Self::open_direct(path).await {
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
                    Request::Read { id, reply } => {
                        let file = file.clone();
                        compio::runtime::spawn(async move {
                            let (result, page) = file
                                .read_exact_at(Page::new(), Page::offset(id))
                                .await
                                .into();
                            let _ = reply.send(result.map(|()| page));
                        })
                        .detach();
                    }
                    Request::Write { id, page, reply } => {
                        let _ = reply.send(file.write_all_at(page, Page::offset(id)).await.0);
                    }
                    Request::Truncate { len, reply } => {
                        let _ = reply.send(file.set_len(len * Page::SIZE as u64).await);
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
}

type Reply<T> = flume::Sender<io::Result<T>>;
type Response<T> = flume::Receiver<io::Result<T>>;

enum Request {
    Read {
        id: PageId,
        reply: Reply<Box<Page>>,
    },
    Write {
        id: PageId,
        page: Box<Page>,
        reply: Reply<()>,
    },
    Truncate {
        len: u64,
        reply: Reply<()>,
    },
    Sync(Reply<()>),
    Close(Reply<()>),
}
