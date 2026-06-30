use super::TimeSeriesRecord;
use crate::storage::page::page_block_ref::PAGE_SIZE;
use crate::storage::page::page_block_ref_mut::PageBlockRefMut;
use crate::storage::page::PageId;
use mudu::common::result::RS;

pub(super) fn build_entries_page_image(
    page_id: PageId,
    prev_page_id: PageId,
    next_page_id: PageId,
    entries: &[TimeSeriesRecord],
    tuple_format_version: u32,
    tuple_schema_hash: u64,
    tuple_flags: u64,
) -> RS<Vec<u8>> {
    let mut page_buf = empty_page_image(
        page_id,
        tuple_format_version,
        tuple_schema_hash,
        tuple_flags,
    )?;
    {
        let mut page = PageBlockRefMut::new(&mut page_buf);
        page.set_page_links(prev_page_id, next_page_id)?;
    }
    for entry in entries {
        let mut page = PageBlockRefMut::new(&mut page_buf);
        page.insert_record(entry.timestamp, entry.tuple_id, &entry.payload)?;
    }
    Ok(page_buf)
}

pub(super) fn empty_page_image(
    page_id: PageId,
    tuple_format_version: u32,
    tuple_schema_hash: u64,
    tuple_flags: u64,
) -> RS<Vec<u8>> {
    let mut page_buf = vec![0u8; PAGE_SIZE];
    {
        let mut page = PageBlockRefMut::new(&mut page_buf);
        page.init_empty_with_tuple_meta(
            page_id,
            tuple_format_version,
            tuple_schema_hash,
            tuple_flags,
        )?;
    }
    Ok(page_buf)
}

pub(super) fn page_entries_fit(entries: &[TimeSeriesRecord]) -> bool {
    let mut buf = vec![0u8; PAGE_SIZE];
    let mut page = PageBlockRefMut::new(&mut buf);
    if page.init_empty(PageId::new(0)).is_err() {
        return false;
    }
    for entry in entries {
        if page
            .insert_record(entry.timestamp, entry.tuple_id, &entry.payload)
            .is_err()
        {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::storage::page::page_block_ref::{PageBlockRef, PAGE_SIZE};

    #[test]
    fn empty_page_image_has_page_size_and_valid_layout() {
        let page_id = PageId::new(7);
        let tuple_format_version = 3u32;
        let tuple_schema_hash = 42u64;
        let tuple_flags = 1u64;
        let buf = empty_page_image(
            page_id,
            tuple_format_version,
            tuple_schema_hash,
            tuple_flags,
        )
        .unwrap();

        assert_eq!(buf.len(), PAGE_SIZE);
        let page = PageBlockRef::try_new(&buf).unwrap();
        assert_eq!(page.header_page_id().unwrap(), page_id);
        assert_eq!(
            page.header_tuple_format_version().unwrap(),
            tuple_format_version
        );
        assert_eq!(page.header_tuple_schema_hash().unwrap(), tuple_schema_hash);
        assert_eq!(page.header_tuple_flags().unwrap(), tuple_flags);
        page.validate_layout().unwrap();
    }

    #[test]
    fn build_entries_page_image_sorts_records_and_preserves_links() {
        let entries = vec![
            TimeSeriesRecord {
                timestamp: 30,
                tuple_id: 1,
                payload: b"ccc".to_vec(),
                page_id: PageId::new(0),
                slot_index: 0,
            },
            TimeSeriesRecord {
                timestamp: 10,
                tuple_id: 2,
                payload: b"aaa".to_vec(),
                page_id: PageId::new(0),
                slot_index: 0,
            },
            TimeSeriesRecord {
                timestamp: 20,
                tuple_id: 3,
                payload: b"bbb".to_vec(),
                page_id: PageId::new(0),
                slot_index: 0,
            },
        ];
        let buf = build_entries_page_image(
            PageId::new(1),
            PageId::new(0),
            PageId::new(2),
            &entries,
            5,
            99,
            7,
        )
        .unwrap();

        assert_eq!(buf.len(), PAGE_SIZE);
        let page = PageBlockRef::try_new(&buf).unwrap();
        page.validate_layout().unwrap();
        assert_eq!(page.header_page_id().unwrap(), PageId::new(1));
        assert_eq!(page.header_prev_page().unwrap(), PageId::new(0));
        assert_eq!(page.header_next_page().unwrap(), PageId::new(2));
        assert_eq!(page.header_record_count().unwrap(), 3);
        assert_eq!(page.header_tuple_format_version().unwrap(), 5);
        assert_eq!(page.header_tuple_schema_hash().unwrap(), 99);
        assert_eq!(page.header_tuple_flags().unwrap(), 7);

        assert_eq!(page.slot(0).unwrap().timestamp(), 10);
        assert_eq!(page.slot(1).unwrap().timestamp(), 20);
        assert_eq!(page.slot(2).unwrap().timestamp(), 30);
        assert_eq!(page.record_bytes(0).unwrap(), b"aaa");
        assert_eq!(page.record_bytes(1).unwrap(), b"bbb");
        assert_eq!(page.record_bytes(2).unwrap(), b"ccc");
    }

    #[test]
    fn build_entries_page_image_empty_entries_valid() {
        let buf =
            build_entries_page_image(PageId::new(5), PageId::new(4), PageId::new(6), &[], 1, 2, 3)
                .unwrap();

        assert_eq!(buf.len(), PAGE_SIZE);
        let page = PageBlockRef::try_new(&buf).unwrap();
        page.validate_layout().unwrap();
        assert_eq!(page.header_record_count().unwrap(), 0);
        assert_eq!(page.header_prev_page().unwrap(), PageId::new(4));
        assert_eq!(page.header_next_page().unwrap(), PageId::new(6));
    }

    #[test]
    fn page_entries_fit_empty_returns_true() {
        assert!(page_entries_fit(&[]));
    }

    #[test]
    fn page_entries_fit_few_small_records_returns_true() {
        let entries = vec![
            TimeSeriesRecord {
                timestamp: 1,
                tuple_id: 1,
                payload: b"a".to_vec(),
                page_id: PageId::new(0),
                slot_index: 0,
            },
            TimeSeriesRecord {
                timestamp: 2,
                tuple_id: 2,
                payload: b"bb".to_vec(),
                page_id: PageId::new(0),
                slot_index: 0,
            },
        ];
        assert!(page_entries_fit(&entries));
    }

    #[test]
    fn page_entries_fit_one_oversized_record_returns_false() {
        let entries = vec![TimeSeriesRecord {
            timestamp: 1,
            tuple_id: 1,
            payload: vec![0u8; PAGE_SIZE],
            page_id: PageId::new(0),
            slot_index: 0,
        }];
        assert!(!page_entries_fit(&entries));
    }

    #[test]
    fn page_entries_fit_many_small_records_exceeding_capacity_returns_false() {
        let payload = vec![0u8; 100];
        let entries: Vec<TimeSeriesRecord> = (0..100)
            .map(|i| TimeSeriesRecord {
                timestamp: i as u64,
                tuple_id: i as u64,
                payload: payload.clone(),
                page_id: PageId::new(0),
                slot_index: 0,
            })
            .collect();
        assert!(!page_entries_fit(&entries));
    }

    #[test]
    fn page_entries_fit_boundary_exact_true_one_more_byte_false() {
        let mut size = 1;
        loop {
            let entries = vec![TimeSeriesRecord {
                timestamp: 1,
                tuple_id: 1,
                payload: vec![0u8; size],
                page_id: PageId::new(0),
                slot_index: 0,
            }];
            if !page_entries_fit(&entries) {
                break;
            }
            size += 1;
        }
        assert!(size > 1);
        let exact = vec![TimeSeriesRecord {
            timestamp: 1,
            tuple_id: 1,
            payload: vec![0u8; size - 1],
            page_id: PageId::new(0),
            slot_index: 0,
        }];
        let too_large = vec![TimeSeriesRecord {
            timestamp: 1,
            tuple_id: 1,
            payload: vec![0u8; size],
            page_id: PageId::new(0),
            slot_index: 0,
        }];
        assert!(page_entries_fit(&exact));
        assert!(!page_entries_fit(&too_large));
    }
}
