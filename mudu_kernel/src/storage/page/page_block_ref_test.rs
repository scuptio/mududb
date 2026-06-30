//! Unit tests for `PageBlockRef` header decoding and layout validation.

#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]

use crate::storage::page::page_block_ref::{PageBlockRef, PAGE_SIZE};
use crate::storage::page::page_header::{
    PageHeader, PAGE_HEADER_MAGIC, PAGE_HEADER_OFF_MAGIC, PAGE_HEADER_OFF_VERSION,
    PAGE_HEADER_SIZE, VERSION as PAGE_VERSION,
};
use crate::storage::page::page_tailer::{PageTailer, PAGE_TAILER_SIZE};
use crate::storage::page::PageId;
use crate::wal::lsn::LSN;
use byteorder::{ByteOrder, LittleEndian};
use mudu::common::result::RS;

fn build_valid_page() -> Vec<u8> {
    let mut page = vec![0u8; PAGE_SIZE];
    let slot_start = PAGE_SIZE - PAGE_TAILER_SIZE;

    let mut header = PageHeader::new(PageId::new(0));
    header.set_first_free_offset(PAGE_HEADER_SIZE as u32);
    header.set_free_bytes((slot_start - PAGE_HEADER_SIZE) as u32);
    header.encode(&mut page).unwrap();

    let checksum = PageTailer::checksum_for_page(&page).unwrap();
    let tailer = PageTailer::new(LSN::new(0), checksum);
    tailer
        .encode(&mut page[PAGE_SIZE - PAGE_TAILER_SIZE..])
        .unwrap();

    page
}

#[test]
fn try_new_rejects_page_shorter_than_header() {
    let short = vec![0u8; PAGE_HEADER_SIZE - 1];
    assert!(PageBlockRef::try_new(&short).is_err());
}

#[test]
fn header_fails_when_magic_is_corrupted() {
    let mut page = build_valid_page();
    LittleEndian::write_u32(
        &mut page[PAGE_HEADER_OFF_MAGIC..PAGE_HEADER_OFF_MAGIC + 4],
        !PAGE_HEADER_MAGIC,
    );
    let block = PageBlockRef::try_new(&page).unwrap();
    assert!(block.header().is_err());
}

#[test]
fn try_new_rejects_page_with_unsupported_version() {
    let mut page = build_valid_page();
    LittleEndian::write_u32(
        &mut page[PAGE_HEADER_OFF_VERSION..PAGE_HEADER_OFF_VERSION + 4],
        PAGE_VERSION + 100,
    );
    assert!(PageBlockRef::try_new(&page).is_err());
}

#[test]
fn header_accessors_return_expected_values() -> RS<()> {
    let page = build_valid_page();
    let block = PageBlockRef::try_new(&page)?;

    assert_eq!(block.header_magic()?, PAGE_HEADER_MAGIC);
    assert_eq!(block.header_version()?, PAGE_VERSION);
    assert_eq!(block.header_page_id()?, PageId::new(0));
    assert_eq!(block.header_prev_page()?, PageId::MAX);
    assert_eq!(block.header_next_page()?, PageId::MAX);
    assert_eq!(block.header_lsn()?, LSN::new(0));
    assert_eq!(block.header_record_count()?, 0);
    assert_eq!(block.header_first_free_offset()?, PAGE_HEADER_SIZE as u32);
    assert_eq!(
        block.header_free_bytes()? as usize,
        PAGE_SIZE - PAGE_TAILER_SIZE - PAGE_HEADER_SIZE
    );
    assert_eq!(block.header_tuple_format_version()?, 0);
    assert_eq!(block.header_tuple_schema_hash()?, 0);
    Ok(())
}

#[test]
fn validate_layout_accepts_well_formed_empty_page() {
    let page = build_valid_page();
    let block = PageBlockRef::try_new(&page).unwrap();
    assert!(block.validate_layout().is_ok());
}

#[test]
fn validate_layout_rejects_bad_magic() {
    let mut page = build_valid_page();
    LittleEndian::write_u32(
        &mut page[PAGE_HEADER_OFF_MAGIC..PAGE_HEADER_OFF_MAGIC + 4],
        0,
    );
    let block = PageBlockRef::new(&page);
    assert!(block.validate_layout().is_err());
}

#[test]
fn validate_layout_rejects_free_bytes_mismatch() {
    let mut page = build_valid_page();
    // Re-encode a header with a wrong free_bytes value.
    let mut header = PageHeader::new(PageId::new(0));
    header.set_first_free_offset(PAGE_HEADER_SIZE as u32);
    header.set_free_bytes(0); // wrong
    header.encode(&mut page).unwrap();

    let checksum = PageTailer::checksum_for_page(&page).unwrap();
    let tailer = PageTailer::new(LSN::new(0), checksum);
    tailer
        .encode(&mut page[PAGE_SIZE - PAGE_TAILER_SIZE..])
        .unwrap();

    let block = PageBlockRef::try_new(&page).unwrap();
    assert!(block.validate_layout().is_err());
}

#[test]
fn new_allows_access_without_version_check() {
    let mut page = build_valid_page();
    // Corrupt version after the magic; `new` does not check version on construction.
    LittleEndian::write_u32(
        &mut page[PAGE_HEADER_OFF_VERSION..PAGE_HEADER_OFF_VERSION + 4],
        PAGE_VERSION + 1,
    );
    let block = PageBlockRef::new(&page);
    assert_eq!(block.page().len(), PAGE_SIZE);
    // header_magic() only checks page length, so it still works.
    assert_eq!(block.header_magic().unwrap(), PAGE_HEADER_MAGIC);
}
