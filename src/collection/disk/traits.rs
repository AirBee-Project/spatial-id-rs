use crate::collection::disk::core::FlexTreeCore;
use crate::collection::disk::{DiskValue, SpatialIdMap, SpatialIdSet};
use crate::query::{CancellationToken, QueryIter, SafeValue, Source};
use crate::{Error, RangeId};

impl Source for SpatialIdSet {
    type Value = ();

    fn read_flex_ids<'a>(
        &'a self,
        targets: &[RangeId],
        token: &CancellationToken,
    ) -> Result<QueryIter<'a, ()>, Error> {
        Ok(read(&self.inner, targets, token))
    }
}

impl<V> Source for SpatialIdMap<V>
where
    V: DiskValue + SafeValue + 'static,
{
    type Value = V;

    fn read_flex_ids<'a>(
        &'a self,
        targets: &[RangeId],
        token: &CancellationToken,
    ) -> Result<QueryIter<'a, V>, Error> {
        Ok(read(&self.inner, targets, token))
    }
}

fn read<'a, V: DiskValue + SafeValue + 'static>(
    tree: &'a FlexTreeCore<V>,
    targets: &[RangeId],
    token: &CancellationToken,
) -> QueryIter<'a, V> {
    if targets.contains(&RangeId::WHOLE) {
        QueryIter::new(tree.iter())
    } else {
        QueryIter::new(tree.overlapping_sync(targets, token))
    }
}

#[cfg(test)]
mod tests {
    use crate::collection::disk::{SpatialIdMap as DiskMap, SpatialIdSet as DiskSet};
    use crate::collection::mem::{SpatialIdMap as MemMap, SpatialIdSet as MemSet};
    use crate::query::merge_policy::{Max, Overwrite};
    use crate::query::{CancellationToken, QueryError, Source};
    use crate::{Error, RangeId, SingleId};
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct TempFile(PathBuf);

    impl TempFile {
        fn new() -> Self {
            static COUNT: AtomicUsize = AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "spatial-id-disk-query-{}-{}",
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
    fn disk_map_query_and_into_disk_map_match_mem() {
        let in_file1 = TempFile::new();
        let in_file2 = TempFile::new();
        let out_map_file = TempFile::new();
        let out_set_file = TempFile::new();

        let mut mem1 = MemMap::<u32>::new();
        let mut mem2 = MemMap::<u32>::new();
        for x in 0..8u32 {
            mem1.insert(SingleId::new(10, 0, x, 0).unwrap(), x * 10);
            mem2.insert(SingleId::new(10, 0, x + 4, 0).unwrap(), 50);
        }

        block_on(async {
            let disk1 = DiskMap::from_mem(&in_file1.0, &mem1).await.unwrap();
            let disk2 = DiskMap::from_mem(&in_file2.0, &mem2).await.unwrap();

            let expected_map = mem1
                .clone()
                .query()
                .merge(mem2.clone(), Max)
                .filter_values(|&v| v >= 30)
                .into_map()
                .unwrap();

            let out_map = disk1
                .query()
                .merge(disk2, Max)
                .filter_values(|&v| v >= 30)
                .into_disk_map(&out_map_file.0)
                .await
                .unwrap();
            assert_eq!(out_map.to_mem().await.unwrap(), expected_map);
            out_map.close().await.unwrap();

            // コミット済みなので開き直しても一致する
            let reopened = DiskMap::<u32>::open(&out_map_file.0).await.unwrap();
            assert_eq!(reopened.to_mem().await.unwrap(), expected_map);

            // 部分範囲の get(..) + into_disk_set
            let target = RangeId::new(10, [0, 0], [3, 6], [0, 0]).unwrap();
            let expected_set = expected_map.clone().query().get(target).into_set().unwrap();
            let out_set = reopened
                .query()
                .get(target)
                .into_disk_set(&out_set_file.0)
                .await
                .unwrap();
            assert_eq!(out_set.to_mem().await.unwrap(), expected_set);
            out_set.close().await.unwrap();
        });
    }

    #[test]
    fn disk_set_query_and_cancellation() {
        let in_file = TempFile::new();
        let out_file = TempFile::new();

        let mut mem_a = MemSet::new();
        let mut mem_b = MemSet::new();
        for x in 0..4u32 {
            mem_a.insert(SingleId::new(10, 0, x, 0).unwrap());
            mem_b.insert(SingleId::new(10, 0, x + 2, 0).unwrap());
        }

        block_on(async {
            let disk_a = DiskSet::from_mem(&in_file.0, &mem_a).await.unwrap();
            let expected = mem_a
                .clone()
                .query()
                .merge(mem_b.clone(), Overwrite)
                .into_set()
                .unwrap();

            let out = disk_a
                .query()
                .merge(mem_b, Overwrite)
                .into_disk_set(&out_file.0)
                .await
                .unwrap();
            assert_eq!(out.to_mem().await.unwrap(), expected);

            // キャンセル確認
            let token = CancellationToken::new();
            token.cancel();
            let res = out.query().get_cancellable(.., &token).into_set();
            assert_eq!(res, Err(Error::Query(QueryError::Cancelled)));
        });
    }

    #[test]
    fn query_unary_and_binary_ops_match_mem() {
        let f1 = TempFile::new();
        let f2 = TempFile::new();

        let mut mem1 = MemMap::<u32>::new();
        let mut mem2 = MemMap::<u32>::new();
        for x in 0..8u32 {
            mem1.insert(SingleId::new(10, 0, x, 0).unwrap(), x + 1);
            mem2.insert(SingleId::new(10, 0, x + 4, 0).unwrap(), 100);
        }

        block_on(async {
            // shift + zoom_out
            let d1 = DiskMap::from_mem(&f1.0, &mem1).await.unwrap();
            let expected_unary = mem1
                .clone()
                .query()
                .shift_x(10, 2)
                .zoom_out(9, Max)
                .into_map()
                .unwrap();
            let got_unary = d1
                .query()
                .shift_x(10, 2)
                .zoom_out(9, Max)
                .into_map()
                .unwrap();
            assert_eq!(got_unary, expected_unary);

            // intersection
            let d1 = DiskMap::from_mem(&f1.0, &mem1).await.unwrap();
            let d2 = DiskMap::from_mem(&f2.0, &mem2).await.unwrap();
            let expected_inter = mem1
                .clone()
                .query()
                .intersection(mem2.clone())
                .into_map()
                .unwrap();
            let got_inter = d1.query().intersection(d2).into_map().unwrap();
            assert_eq!(got_inter, expected_inter);

            // difference
            let d1 = DiskMap::from_mem(&f1.0, &mem1).await.unwrap();
            let d2 = DiskMap::from_mem(&f2.0, &mem2).await.unwrap();
            let expected_diff = mem1
                .clone()
                .query()
                .difference(mem2.clone())
                .into_map()
                .unwrap();
            let got_diff = d1.query().difference(d2).into_map().unwrap();
            assert_eq!(got_diff, expected_diff);
        });
    }

    #[test]
    fn query_prunes_untouched_ref_pages() {
        use crate::{DiskError, FlexId};
        use std::io::{Read, Seek, SeekFrom, Write};

        let file = TempFile::new();
        let mut mem = MemMap::<u64>::new();
        let mut next = 1u64;
        for f in [0i32, 1i32] {
            for x in 0..32u32 {
                for y in 0..16u32 {
                    mem.insert(FlexId::new(1, f, 5, x, 4, y).unwrap(), next);
                    next += 1;
                }
            }
        }

        block_on(async {
            let mut disk = DiskMap::from_mem(&file.0, &mem).await.unwrap();
            disk.compact().await.unwrap();
            disk.close().await.unwrap();

            // compact 後は行きがけ順に並ぶため、末尾ページは upper (f=1/1) 側の葉ページ
            let file_len = fs::metadata(&file.0).unwrap().len();
            assert!(file_len >= 5 * 4096, "RefPage が発生しているはず");
            let offset = file_len - 4096 + 5;
            let mut f = fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&file.0)
                .unwrap();
            let mut byte = [0u8; 1];
            f.seek(SeekFrom::Start(offset)).unwrap();
            f.read_exact(&mut byte).unwrap();
            f.seek(SeekFrom::Start(offset)).unwrap();
            f.write_all(&[!byte[0]]).unwrap();
            drop(f);

            let lower_target = RangeId::new(1, [0, 0], [0, 0], [0, 0]).unwrap();
            let upper_target = RangeId::new(1, [1, 1], [0, 0], [0, 0]).unwrap();

            // lower 側への範囲クエリは upper 側の壊れたページを読まずに成功し、メモリ版と一致する
            let expected = mem.clone().query().get(lower_target).into_map().unwrap();
            let disk = DiskMap::<u64>::open(&file.0).await.unwrap();
            let got = disk.query().get(lower_target).into_map().unwrap();
            assert_eq!(got, expected);

            // upper 側への範囲クエリは壊れたページを読んで Corrupt になる
            let disk = DiskMap::<u64>::open(&file.0).await.unwrap();
            assert!(matches!(
                disk.query().get(upper_target).into_map(),
                Err(Error::Disk(DiskError::Corrupt))
            ));
        });
    }

    #[test]
    fn into_disk_map_error_preserves_previous_commit() {
        use crate::DiskError;
        use crate::collection::disk::MAX_VALUE_LEN;

        let out_file = TempFile::new();
        let initial_id = SingleId::new(10, 0, 1, 2).unwrap();
        let mut initial = MemMap::<String>::new();
        initial.insert(initial_id, "keep".into());

        let mut too_large = MemMap::<String>::new();
        too_large.insert(
            SingleId::new(10, 0, 3, 4).unwrap(),
            "x".repeat(MAX_VALUE_LEN + 1),
        );

        block_on(async {
            let disk = DiskMap::from_mem(&out_file.0, &initial).await.unwrap();
            disk.close().await.unwrap();

            let err = too_large
                .query()
                .into_disk_map(&out_file.0)
                .await
                .unwrap_err();
            assert_eq!(err, Error::Disk(DiskError::ValueTooLarge));

            // 失敗しても直前のコミット内容は残っている
            let reopened = DiskMap::<String>::open(&out_file.0).await.unwrap();
            assert_eq!(reopened.to_mem().await.unwrap(), initial);
            reopened.close().await.unwrap();
        });
    }
}
