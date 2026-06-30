# MuduDB File Layout Contract v1

## Scope

This document specifies the on-disk layout of MuduDB database files, directories, and package artifacts. It binds together the lower-level contracts for [page headers](page_header_v1.md), [tuple binary format](tuple_binary_v1.md), [log frames](log_frame_v1.md), [protocol frames](protocol_frame_v1.md), [MPK manifests](mpk_manifest_v1.md), and [server configuration](mududb_cfg_v1.md).

## Version history

| Version | Date | Summary |
|---------|------|---------|
| 1 | 2026-6-25 | Initial file layout: time-series/relation page files, WAL chunk files, MPK packages, and TOML configuration. |

## Logical data organization

A MuduDB deployment consists of:

- One or more **relation files** storing tables and indexes.
- One or more **time-series files** storing time-series data.
- **WAL chunk files** (`*.xl`) storing write-ahead log frames.
- An optional **MPK package** (`*.mpk`) for distribution.
- A **server configuration file** (`mududb.toml`).

## Relation file layout

A relation is physically stored as a pair of `TimeSeriesFile` instances:

```text
{base_path}_key   -> key file
{base_path}_value -> value file
```

Each file is a sequence of pages whose size is configured at database creation time. The default page size is 4 KiB (`DEFAULT_PAGE_SIZE`).

### Key file page layout

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

Each slot stores a key tuple. See [Page Header Contract v1](page_header_v1.md) for header and tailer details.

### Value file page layout

Same physical layout as the key file. Each slot stores a value tuple or a version chain.

## Time-series file layout

A standalone time-series file follows the same page-sequence layout as relation files. Pages are linked by `prev_page` / `next_page` in the page header.

```text
+--------+--------+--------+-----+
| Page 0 | Page 1 | Page 2 | ... |
+--------+--------+--------+-----+
```

Each page is `page_size` bytes (default 4096). New pages are appended at the end of the file. The page size is chosen when the database directory is created and must remain consistent for the lifetime of that directory unless an offline migration tool is used.

## WAL chunk file layout

WAL chunk files live in a per-worker or per-relation directory:

```text
{log_dir}/{short_oid}.{sequence}.xl
```

- `short_oid`: short UUID derived from the log OID.
- `sequence`: monotonically increasing chunk number (`u64`).

A chunk file is an opaque concatenation of one or more log frames:

```text
+----------------+----------------+----------------+
| Log Frame 0    | Log Frame 1    | ...            |
+----------------+----------------+----------------+
```

There is no per-chunk header or footer; decoding scans frames by calling `frame_len` on each frame boundary. See [Log Frame Contract v1](log_frame_v1.md).

### Chunk rotation

A new chunk is started when the current chunk reaches `chunk_size`. The default relation WAL chunk size is 256 KiB.

## MPK package layout

An MPK package is a ZIP archive containing:

```text
package.desc.json
type.desc.json
<artifact files...>
```

The manifest semantics are specified in [MPK Manifest Contract v1](mpk_manifest_v1.md). The package itself does not add extra framing beyond the ZIP container.

## Server configuration

The server configuration is a TOML file. See [MuduDB Server Configuration Contract v1](mududb_cfg_v1.md).

## Directory conventions

- Relation files: `{data_dir}/{partition_id}/{table_id}/`
- WAL chunks: `{wal_dir}/{log_oid_short}/`
- MPK packages: arbitrary path ending in `.mpk`

## Integrity mechanisms

- **Page level:** magic, version, CRC32 tailer, LSN consistency.
- **Log frame level:** magic, version, CRC32, header/tailer `n_part` match.
- **File level:** no extra checksum; integrity comes from page and frame checks.

## Compatibility matrix

| Reader \ Writer | v1 |
|-----------------|----|
| v1 | Compatible |

Only version `1` is supported.

## Upgrade and rollback rules

- **Upgrade:** A future v2 file layout must bump a file-level format version stored in a new file header or directory marker before writing v2 pages or frames.
- **Rollback:** A v1-only binary opening a v2 file returns `UnsupportedFormatVersion` and does not modify the file.
- **Migration:** File layout migrations are offline. A migration tool must rewrite affected files, preserve logical content, and produce new golden fixtures.

## Deprecation policy

Version `1` may be deprecated only after:
1. An offline migration tool exists for all v1 file types.
2. The new layout has been the default for at least one release cycle.
3. CI continues to decode v1 golden fixtures.

## References

- Page header: [page_header_v1.md](page_header_v1.md)
- Tuple binary format: [tuple_binary_v1.md](tuple_binary_v1.md)
- Log frame: [log_frame_v1.md](log_frame_v1.md)
- Protocol frame: [protocol_frame_v1.md](protocol_frame_v1.md)
- MPK manifest: [mpk_manifest_v1.md](mpk_manifest_v1.md)
- Server config: [mududb_cfg_v1.md](mududb_cfg_v1.md)
- Implementation:
  - [`mudu_kernel/src/storage/time_series/time_series_file.rs`](../../../mudu_kernel/src/storage/time_series/time_series_file.rs)
  - [`mudu_kernel/src/storage/relation/relation.rs`](../../../mudu_kernel/src/storage/relation/relation.rs)
  - [`mudu_kernel/src/wal/worker_wal_backend/layout.rs`](../../../mudu_kernel/src/wal/worker_wal_backend/layout.rs)
