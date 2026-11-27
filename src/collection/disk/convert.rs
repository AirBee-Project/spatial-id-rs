use crate::Error;
use crate::collection::disk::{DiskValue, SpatialIdMap as DiskMap, SpatialIdSet as DiskSet};
use crate::collection::mem::{SpatialIdMap as MemMap, SpatialIdSet as MemSet};

impl<V: DiskValue + PartialEq + Clone> TryFrom<&DiskMap<V>> for MemMap<V> {
    type Error = Error;

    fn try_from(disk: &DiskMap<V>) -> Result<Self, Self::Error> {
        disk.iter_flex_ids().collect()
    }
}

impl<V: DiskValue + PartialEq + Clone> TryFrom<DiskMap<V>> for MemMap<V> {
    type Error = Error;

    fn try_from(disk: DiskMap<V>) -> Result<Self, Self::Error> {
        disk.into_iter_flex_ids().collect()
    }
}

impl TryFrom<&DiskSet> for MemSet {
    type Error = Error;

    fn try_from(disk: &DiskSet) -> Result<Self, Self::Error> {
        disk.iter_flex_ids().collect()
    }
}

impl TryFrom<DiskSet> for MemSet {
    type Error = Error;

    fn try_from(disk: DiskSet) -> Result<Self, Self::Error> {
        disk.into_iter_flex_ids().collect()
    }
}

#[cfg(test)]
mod tests {
    use crate::SingleId;
    use crate::collection::disk::{SpatialIdMap as DiskMap, SpatialIdSet as DiskSet};
    use crate::collection::mem::{SpatialIdMap as MemMap, SpatialIdSet as MemSet};
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct TempFile(PathBuf);

    impl TempFile {
        fn new() -> Self {
            static COUNT: AtomicUsize = AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "spatial-id-disk-convert-{}-{}",
                std::process::id(),
                COUNT.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = fs::remove_file(&path);
            TempFile(path)
        }
    }

    impl Drop for TempFile {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }

    fn block_on<F: Future>(future: F) -> F::Output {
        compio::runtime::Runtime::new().unwrap().block_on(future)
    }

    #[test]
    fn try_from_disk_map_to_mem_map() {
        let file = TempFile::new();
        let mut mem = MemMap::<u32>::new();
        mem.insert(SingleId::new(15, 0, 10, 20).unwrap(), 1);
        mem.insert(SingleId::new(15, -1, 10, 20).unwrap(), 2);

        block_on(async {
            let disk = DiskMap::from_mem(&file.0, &mem).await.unwrap();
            let from_ref = MemMap::try_from(&disk).unwrap();
            assert_eq!(from_ref, mem);
            let from_owned: MemMap<u32> = disk.try_into().unwrap();
            assert_eq!(from_owned, mem);
        });
    }

    #[test]
    fn try_from_disk_set_to_mem_set() {
        let file = TempFile::new();
        let mut mem = MemSet::new();
        mem.insert(SingleId::new(15, 0, 10, 20).unwrap());
        mem.insert(SingleId::new(15, -1, 10, 20).unwrap());

        block_on(async {
            let disk = DiskSet::from_mem(&file.0, &mem).await.unwrap();
            let from_ref = MemSet::try_from(&disk).unwrap();
            assert_eq!(from_ref, mem);
            let from_owned: MemSet = disk.try_into().unwrap();
            assert_eq!(from_owned, mem);
        });
    }

    #[test]
    fn try_from_corrupted_file_returns_corrupt() {
        use crate::{DiskError, Error};
        use std::io::{Read, Seek, SeekFrom, Write};

        fn flip_root_page(path: &std::path::Path) {
            let offset = 2 * 4096 + 5;
            let mut f = fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(path)
                .unwrap();
            let mut byte = [0u8; 1];
            f.seek(SeekFrom::Start(offset)).unwrap();
            f.read_exact(&mut byte).unwrap();
            f.seek(SeekFrom::Start(offset)).unwrap();
            f.write_all(&[!byte[0]]).unwrap();
        }

        let map_file = TempFile::new();
        let set_file = TempFile::new();
        let id = SingleId::new(15, 0, 10, 20).unwrap();

        block_on(async {
            let mut map = DiskMap::<u32>::open(&map_file.0).await.unwrap();
            map.insert(id, 42).await.unwrap();
            map.commit().await.unwrap();
            map.close().await.unwrap();
            flip_root_page(&map_file.0);

            let broken_map = DiskMap::<u32>::open(&map_file.0).await.unwrap();
            assert!(matches!(
                MemMap::try_from(&broken_map),
                Err(Error::Disk(DiskError::Corrupt))
            ));
            broken_map.close().await.unwrap();

            let mut set = DiskSet::open(&set_file.0).await.unwrap();
            set.insert(id).await.unwrap();
            set.commit().await.unwrap();
            set.close().await.unwrap();
            flip_root_page(&set_file.0);

            let broken_set = DiskSet::open(&set_file.0).await.unwrap();
            assert!(matches!(
                MemSet::try_from(&broken_set),
                Err(Error::Disk(DiskError::Corrupt))
            ));
            broken_set.close().await.unwrap();
        });
    }
}
