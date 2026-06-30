# Page Header Contract v1

## Scope

This document specifies the on-disk page header format used by MuduDB relation and time-series files. A page is the smallest unit of I/O for persistent data. The page header precedes the slot array and record payload area inside a page whose size is configured at database creation time (default 4 KiB).

## Version history

| Version | Date | Summary |
|---------|------|---------|
| 1 | 2026-6-25 | Initial 128-byte page header with tuple format version and schema hash. |

## Overall page layout

```text
+----------------------------------+
| Page Header (128 bytes)          |
+----------------------------------+
| Record payload area (grows down) |
| ...                              |
+----------------------------------+
| Slot array (grows up)            |
+----------------------------------+
| Page Tailer (12 bytes)           |
+----------------------------------+
```

A valid page is exactly `page_size` bytes. The default `page_size` is 4096 (`DEFAULT_PAGE_SIZE`). The header is followed by the payload area, then the slot array, then a 12-byte tailer. The slot array and payload area grow toward each other. The page size is a persistent property of a database directory and must not change without an offline migration.

## Header byte layout

All multi-byte integer fields are little-endian unless noted otherwise.

| Offset | Size | Field | Description |
|--------|------|-------|-------------|
| 0 | 4 | `magic` | Magic `0x5041_4745` (ASCII `PAGE`; on-disk bytes `45 47 41 50`). |
| 4 | 4 | `version` | Page header format version. Current value: `1`. |
| 8 | 8 | `page_id` | Logical page identifier. |
| 16 | 8 | `prev_page` | Previous page id in the chain, or `0xFFFF_FFFF_FFFF_FFFF` for none. |
| 24 | 8 | `next_page` | Next page id in the chain, or `0xFFFF_FFFF_FFFF_FFFF` for none. |
| 32 | 8 | `lsn` | Log sequence number (`LSN`/`u64`). |
| 40 | 8 | `flags` | Page-level flags. Reserved for future use; set to `0`. |
| 48 | 8 | `tuple_flags` | Tuple-level flags. Reserved for future use; set to `0`. |
| 56 | 4 | `record_count` | Number of record slots currently stored in the page. |
| 60 | 4 | `first_free_offset` | Offset of the first free byte after the header. |
| 64 | 4 | `free_bytes` | Number of contiguous free bytes between payload and slot array. |
| 68 | 4 | `last_record_offset` | Offset of the last record inserted. |
| 72 | 4 | `tuple_format_version` | Version of the [tuple binary format](tuple_binary_v1.md) used by records in this page. |
| 76 | 8 | `tuple_schema_hash` | 64-bit FNV-1a-like hash of the tuple binary descriptor. |
| 84 | 44 | `reserved` | Reserved bytes; must be zero on write and ignored on read. |

Total header size: **128 bytes**.

## Page tailer

The page tailer is 12 bytes at the end of the page:

| Offset | Size | Field | Description |
|--------|------|-------|-------------|
| 0 | 8 | `lsn` | Same LSN as the header (redundant copy for recovery). |
| 8 | 4 | `checksum` | CRC32 of the entire page except the trailing 12-byte tailer. |

## Integrity mechanisms

- **Magic check:** decoders reject pages whose first four bytes are not `45 47 41 50`.
- **Version check:** decoders reject `version == 0` and any version greater than `1`.
- **Length check:** decoders require at least 128 bytes for the header and 12 bytes for the tailer.
- **CRC32:** the tailer stores a CRC32 over `[0, page.len() - 12)`.
- **LSN consistency:** layout validation ensures the header LSN equals the tailer LSN.
- **Slot payload CRC16:** each record slot carries a CRC16 over its payload.

## Compatibility matrix

| Reader \ Writer | v1 |
|-----------------|----|
| v1 | Compatible |

Only version `1` is supported. Version `0` is invalid and rejected.

## Upgrade and rollback rules

- **Upgrade:** A future v2 writer must write a v2 header and tailer, and must bump the file-level format version before any v2 page is written.
- **Rollback:** If a v1-only binary opens a v2 database, the decoder returns `UnsupportedFormatVersion` with the actual version and supported range. The original data is not modified.
- **Migration:** When a v1 → v2 migration is introduced, an offline migration tool must rewrite affected pages and update the file-level format marker. Migration must be verified against golden fixtures before release.

## Deprecation policy

Version `1` is the current stable format. It may be deprecated only after:
1. A migration tool exists for all released v1 data.
2. The new format has been the default for at least one full release cycle.
3. Golden fixtures for v1 continue to be decoded in CI until deprecation is complete.

## References

- Tuple binary format: [tuple_binary_v1.md](tuple_binary_v1.md)
- Implementation: [`mudu_kernel/src/storage/page/format/latest.rs`](../../../mudu_kernel/src/storage/page/format/latest.rs)
- Tailer: [`mudu_kernel/src/storage/page/page_tailer.rs`](../../../mudu_kernel/src/storage/page/page_tailer.rs)
- Slot layout: [`mudu_kernel/src/storage/page/record_slot.rs`](../../../mudu_kernel/src/storage/page/record_slot.rs)
