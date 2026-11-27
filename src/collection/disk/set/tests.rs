use crate::collection::disk::{SpatialIdMap as DiskMap, SpatialIdSet as DiskSet};
use crate::collection::mem::SpatialIdSet as MemSet;
use crate::{DiskError, Error, RangeId, SingleId};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

struct TempFile(PathBuf);

impl TempFile {
    fn new() -> Self {
        static COUNT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "spatial-id-disk-set-{}-{}",
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
fn basic_operations_match_mem_set() {
    let file = TempFile::new();
    let s1 = SingleId::new(20, 0, 10, 20).unwrap();
    let s2 = SingleId::new(20, 0, 11, 20).unwrap();
    let r = RangeId::new(20, [0, 1], [10, 11], [20, 20]).unwrap();

    block_on(async {
        let mut disk = DiskSet::open(&file.0).await.unwrap();
        let mut mem = MemSet::new();
        assert!(disk.is_empty());

        disk.insert(s1).await.unwrap();
        mem.insert(s1);
        disk.insert(r).await.unwrap();
        mem.insert(r);
        disk.remove(s2).await.unwrap();
        let _ = mem.remove(s2);
        disk.commit().await.unwrap();

        assert_eq!(disk.is_empty(), mem.is_empty());
        assert_eq!(disk.count().await.unwrap(), mem.count());
        assert_eq!(disk.max_zoomlevel().await.unwrap(), mem.max_zoomlevel());
        assert_eq!(disk.get(r).await.unwrap(), mem.get(r).collect::<Vec<_>>());
        assert_eq!(
            disk.get_overlapping(s1).await.unwrap(),
            mem.get_overlapping(s1).collect::<Vec<_>>()
        );
        assert_eq!(
            disk.flat_single_ids(None).await.unwrap(),
            mem.flat_single_ids(None).collect::<Vec<_>>()
        );

        let expected: Vec<RangeId> = mem.iter().collect();
        assert_eq!(
            disk.iter().map(Result::unwrap).collect::<Vec<_>>(),
            expected
        );
        assert_eq!(
            (&disk).into_iter().map(Result::unwrap).collect::<Vec<_>>(),
            expected
        );
        assert_eq!(
            disk.into_iter().map(Result::unwrap).collect::<Vec<_>>(),
            expected
        );

        let reopened = DiskSet::open(&file.0).await.unwrap();
        assert_eq!(reopened.to_mem().await.unwrap(), mem);
        reopened.close().await.unwrap();
    });
}

#[test]
fn from_mem_and_from_map_work() {
    let map_file = TempFile::new();
    let set_file = TempFile::new();
    let set_file2 = TempFile::new();

    let s1 = SingleId::new(20, 0, 0, 0).unwrap();
    let s2 = SingleId::new(20, 5, 0, 0).unwrap();

    block_on(async {
        let mut map = DiskMap::<u8>::open(&map_file.0).await.unwrap();
        map.insert(s1, 7).await.unwrap();
        map.insert(s2, 9).await.unwrap();
        map.commit().await.unwrap();

        // Map ファイルは Set として開けない
        assert!(matches!(
            DiskSet::open(&map_file.0).await,
            Err(Error::Disk(DiskError::Incompatible))
        ));

        let set = DiskSet::from_map(&set_file.0, &map).await.unwrap();
        let mem_set = set.to_mem().await.unwrap();
        assert_eq!(mem_set.count(), 2);
        assert!(mem_set.get(s1).next().is_some());
        assert!(mem_set.get(s2).next().is_some());

        let set2 = DiskSet::from_mem(&set_file2.0, &mem_set).await.unwrap();
        assert_eq!(set2.to_mem().await.unwrap(), mem_set);

        map.close().await.unwrap();
        set.close().await.unwrap();
        set2.close().await.unwrap();
    });
}

#[test]
fn json_round_trip_matches_mem_set() {
    let file1 = TempFile::new();
    let file2 = TempFile::new();
    let file3 = TempFile::new();

    let mut mem = MemSet::new();
    mem.insert(SingleId::new(20, 0, 0, 0).unwrap());
    mem.insert(RangeId::new(20, [0, 0], [0, 1], [0, 0]).unwrap());
    #[cfg(feature = "temporal_id")]
    {
        let base = SingleId::new(12, 0, 3638, 1614).unwrap();
        mem.insert(base.time_with(crate::Interval::HOUR, 0).unwrap());
        mem.insert(base.time_with(crate::Interval::HOUR, 1).unwrap());
    }

    block_on(async {
        let disk = DiskSet::from_mem(&file1.0, &mem).await.unwrap();
        let mem_json = serde_json::to_string(&mem).unwrap();
        let disk_serde_json = serde_json::to_string(&disk).unwrap();
        let disk_async_json = disk.to_json_string().await.unwrap();
        assert_eq!(disk_serde_json, mem_json);
        assert_eq!(disk_async_json, mem_json);

        let restored_str = DiskSet::from_json_str(&file2.0, &disk_async_json)
            .await
            .unwrap();
        assert_eq!(restored_str.to_mem().await.unwrap(), mem);

        let restored_reader = DiskSet::from_json_reader(&file3.0, disk_async_json.as_bytes())
            .await
            .unwrap();
        assert_eq!(restored_reader.to_mem().await.unwrap(), mem);

        disk.close().await.unwrap();
        restored_str.close().await.unwrap();
        restored_reader.close().await.unwrap();
    });
}
