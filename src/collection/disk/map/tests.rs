use crate::collection::disk::SpatialIdMap as DiskMap;
use crate::collection::mem::SpatialIdMap as MemMap;
use crate::{AllowedIntervals, DiskError, Error, FlexId, RangeId, SingleId};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

struct TempFile(PathBuf);

impl TempFile {
    fn new() -> Self {
        static COUNT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "spatial-id-disk-map-{}-{}",
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
fn basic_operations_match_mem_map() {
    let file = TempFile::new();
    let s1 = SingleId::new(20, 0, 10, 20).unwrap();
    let s2 = SingleId::new(20, 0, 11, 20).unwrap();
    let r = RangeId::new(20, [0, 1], [10, 11], [20, 20]).unwrap();
    let f = FlexId::new(19, 0, 19, 5, 20, 20).unwrap();

    block_on(async {
        let mut disk = DiskMap::<i32>::open(&file.0).await.unwrap();
        let mut mem = MemMap::<i32>::new();
        assert!(disk.is_empty());
        assert_eq!(disk.value_range().await.unwrap(), None);

        disk.insert(s1, 10).await.unwrap();
        mem.insert(s1, 10);
        disk.upsert(r, 20).await.unwrap();
        mem.upsert(r, 20);
        disk.insert(s2, 30).await.unwrap();
        mem.insert(s2, 30);

        assert_eq!(disk.is_empty(), mem.is_empty());
        assert_eq!(disk.count().await.unwrap(), mem.count());
        assert_eq!(disk.max_zoomlevel().await.unwrap(), mem.max_zoomlevel());
        assert_eq!(
            disk.value_range().await.unwrap(),
            mem.value_range().map(|(&lo, &hi)| (lo, hi))
        );

        let expected_get: Vec<(RangeId, i32)> = mem.get(f).map(|(id, &v)| (id, v)).collect();
        assert_eq!(disk.get(f).await.unwrap(), expected_get);
        let expected_overlap: Vec<(RangeId, i32)> =
            mem.get_overlapping(s1).map(|(id, &v)| (id, v)).collect();
        assert_eq!(disk.get_overlapping(s1).await.unwrap(), expected_overlap);

        // iter / &map / into_iter がメモリ版と一致
        let expected_all: Vec<(RangeId, i32)> = mem.iter().map(|(id, &v)| (id, v)).collect();
        assert_eq!(
            disk.iter().map(Result::unwrap).collect::<Vec<_>>(),
            expected_all
        );
        assert_eq!(
            (&disk).into_iter().map(Result::unwrap).collect::<Vec<_>>(),
            expected_all
        );

        disk.remove(s1).await.unwrap();
        let _ = mem.remove(s1);
        disk.commit().await.unwrap();
        let expected_after: Vec<(RangeId, i32)> = mem.iter().map(|(id, &v)| (id, v)).collect();
        assert_eq!(
            disk.into_iter().map(Result::unwrap).collect::<Vec<_>>(),
            expected_after
        );

        let mut reopened = DiskMap::<i32>::open(&file.0).await.unwrap();
        assert_eq!(reopened.to_mem().await.unwrap(), mem);
        reopened.compact().await.unwrap();
        reopened.clear();
        assert!(reopened.is_empty());
        reopened.close().await.unwrap();
    });
}

#[test]
fn from_mem_and_to_mem_round_trip() {
    let file = TempFile::new();
    let mut mem = MemMap::<String>::new();
    mem.insert(SingleId::new(12, 0, 10, 20).unwrap(), "alpha".into());
    mem.insert(SingleId::new(12, -1, 10, 20).unwrap(), "beta".into());

    block_on(async {
        let disk = DiskMap::from_mem(&file.0, &mem).await.unwrap();
        assert_eq!(disk.to_mem().await.unwrap(), mem);
        disk.close().await.unwrap();

        // 既存ファイルへ別内容を上書きできる
        let mut mem2 = MemMap::<String>::new();
        mem2.insert(SingleId::new(10, 0, 1, 2).unwrap(), "gamma".into());
        let disk2 = DiskMap::from_mem(&file.0, &mem2).await.unwrap();
        assert_eq!(disk2.to_mem().await.unwrap(), mem2);
        disk2.close().await.unwrap();

        // 別型で開くと Incompatible
        assert!(matches!(
            DiskMap::<u64>::open(&file.0).await,
            Err(Error::Disk(DiskError::Incompatible))
        ));
    });
}

#[test]
fn json_round_trip_matches_mem_map() {
    let file1 = TempFile::new();
    let file2 = TempFile::new();
    let file3 = TempFile::new();

    let mut mem = MemMap::<i32>::new();
    mem.insert(SingleId::new(12, 0, 0, 0).unwrap(), 10);
    mem.insert(SingleId::new(12, 1, 0, 0).unwrap(), 20);
    mem.insert(SingleId::new(12, 2, 0, 0).unwrap(), 10);
    #[cfg(feature = "temporal_id")]
    mem.insert(
        SingleId::new(12, 0, 3638, 1614)
            .unwrap()
            .time_with(1800, 809712)
            .unwrap(),
        7,
    );

    block_on(async {
        let disk = DiskMap::from_mem(&file1.0, &mem).await.unwrap();
        let mem_json = serde_json::to_string(&mem).unwrap();
        let disk_serde_json = serde_json::to_string(&disk).unwrap();
        let disk_async_json = disk.to_json_string().await.unwrap();
        assert_eq!(disk_serde_json, mem_json);
        assert_eq!(disk_async_json, mem_json);

        let restored_str = DiskMap::<i32>::from_json_str(&file2.0, &disk_async_json)
            .await
            .unwrap();
        assert_eq!(restored_str.to_mem().await.unwrap(), mem);

        let restored_reader =
            DiskMap::<i32>::from_json_reader(&file3.0, disk_async_json.as_bytes())
                .await
                .unwrap();
        assert_eq!(restored_reader.to_mem().await.unwrap(), mem);

        let default_intervals = AllowedIntervals::default();
        assert_eq!(
            restored_reader
                .flat_single_ids(Some(&default_intervals))
                .await
                .unwrap(),
            mem.flat_single_ids(Some(&default_intervals))
                .map(|(id, &v)| (id, v))
                .collect::<Vec<_>>()
        );

        // 不正なズームレベルはエラー
        let bad_json = r#"{"$schema":"https://airbee-project.github.io/schemas/json/v1.0.json","meta":{"version":"v1.0","description":""},"option":{},"data":[{"name":"","value":[10],"ids":[{"z":68,"f":[0],"x":[0],"y":[0],"ref":0}]}]}"#;
        let bad_file = TempFile::new();
        assert!(
            DiskMap::<i32>::from_json_str(&bad_file.0, bad_json)
                .await
                .is_err()
        );

        disk.close().await.unwrap();
        restored_str.close().await.unwrap();
        restored_reader.close().await.unwrap();
    });
}
