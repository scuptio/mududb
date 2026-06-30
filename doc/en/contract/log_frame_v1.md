# Log Frame Contract v1

## Scope

This document specifies the log frame format used by MuduDB write-ahead logs (WAL), cross-partition transaction logs (XL), and physical logs (PL). A log frame is the unit of persistence inside chunk files.

## Version history

| Version | Date | Summary |
|---------|------|---------|
| 1 | 2026-6-25 | Initial frame format with header, payload, tailer, and per-frame CRC32. |

## Frame layout

```text
+-----------------------------+
| Header (24 bytes)           |
+-----------------------------+
| Payload (variable)          |
+-----------------------------+
| Tailer (8 bytes)            |
+-----------------------------+
```

### Header

All header fields are big-endian.

| Offset | Size | Field | Description |
|--------|------|-------|-------------|
| 0 | 4 | `magic` | Magic `0x4C47_464D` (ASCII `LGFM`). |
| 4 | 4 | `version` | Log frame format version. Current value: `1`. |
| 8 | 8 | `lsn` | Log sequence number (`LSN`/`u64`). |
| 16 | 4 | `size` | Size of the payload in bytes. |
| 20 | 4 | `n_part` | Number of remaining frames after this one in the same logical entry. `0` means this is the last frame. |

### Tailer

The tailer is 8 bytes, big-endian:

| Offset | Size | Field | Description |
|--------|------|-------|-------------|
| 0 | 4 | `n_part` | Must equal `header.n_part`. |
| 4 | 4 | `checksum` | CRC32 of the payload (lower 32 bits of CRC64-XZ). |

## Integrity mechanisms

- **Magic check:** decoders reject frames whose magic does not match `0x4C47_464D`.
- **Version check:** decoders reject versions outside the range `1..=1`.
- **Length checks:** a frame requires at least 32 bytes (header + tailer). The header `size` field must fit within the available bytes.
- **Header/tailer `n_part` match:** decoders verify the two values are equal.
- **CRC32:** the payload checksum is verified against the tailer.
- **Frame ordering:** when reassembling a multi-frame entry, each subsequent frame must decrement `n_part` by exactly one.

## Multi-frame entries

A logical log entry may be split into multiple frames when it exceeds the maximum part size. The frames are stored contiguously and reassembled by concatenating payloads in order. The `n_part` field counts remaining frames after the current one, so the final frame has `n_part = 0`.

## Compatibility matrix

| Reader \ Writer | v1 |
|-----------------|----|
| v1 | Compatible |

Only version `1` is supported.

## Upgrade and rollback rules

- **Upgrade:** A future v2 frame must bump `version` and may add new header fields from the reserved area or extend the tailer. V2 writers must not produce v2 frames until the database file format version is also bumped.
- **Rollback:** A v1-only reader opening a v2 log returns `UnsupportedFormatVersion` with the actual version and supported range. No log data is rewritten.
- **Migration:** V1 → V2 migration is offline: rewrite chunk files with the new frame format and update the file-level format marker.

## Deprecation policy

Version `1` may be deprecated only after:
1. An offline migration tool exists.
2. The new format has been the default for at least one release cycle.
3. CI continues to decode v1 golden log frames.

## References

- Implementation: [`mudu_kernel/src/wal/format/latest.rs`](../../../mudu_kernel/src/wal/format/latest.rs)
- XL records: [`mudu_kernel/src/wal/xl_entry.rs`](../../../mudu_kernel/src/wal/xl_entry.rs)
- PL records: [`mudu_kernel/src/wal/pl_entry.rs`](../../../mudu_kernel/src/wal/pl_entry.rs)
- Chunk layout: [`mudu_kernel/src/wal/worker_wal_backend/layout.rs`](../../../mudu_kernel/src/wal/worker_wal_backend/layout.rs)
