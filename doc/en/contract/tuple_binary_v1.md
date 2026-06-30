# Tuple Binary Format Contract v1

## Scope

This document specifies the in-memory and on-disk binary format of a MuduDB tuple. Tuples are stored inside page payloads, in log entries, and in network frames. The binary layout is independent of the SQL type system but is derived from a `TupleBinaryDesc` that describes field order, nullability, and fixed/variable-length classification.

## Version history

| Version | Date | Summary |
|---------|------|---------|
| 1 | 2026-6-25 | Initial tuple binary format with null bitmap, slot array, fixed data, and variable-length data. |

## Terminology

- **Field:** One column value within a tuple.
- **Fixed-length field:** A field whose encoded size is known from its type descriptor (e.g., `INT4`, `BOOL`).
- **Variable-length field:** A field whose encoded size is not fixed (e.g., `TEXT`, `BYTEA`, `NUMERIC`).
- **Slot:** An 8-byte descriptor `(offset: u32, length: u32)` that locates a variable-length field's data.
- **Null bitmap:** A bit vector indicating which nullable fields are `NULL`.

## Overall tuple layout

A tuple is a single contiguous byte buffer laid out as:

```text
+----------------------------------+
| Null Bitmap (variable, 8-byte    |
| aligned)                         |
+----------------------------------+
| Slot Array (8 bytes per variable |
| field)                           |
+----------------------------------+
| Fixed-Length Data Area           |
+----------------------------------+
| Variable-Length Data Area        |
+----------------------------------+
```

- The null bitmap is at offset `0`.
- The slot array immediately follows the null bitmap.
- Fixed-length fields are stored at fixed offsets after the slot array.
- Variable-length fields are appended at the end; the slot array points to each value.

## Null bitmap

The null bitmap occupies `aligned_byte_len(nullable_count)` bytes, rounded up to the next multiple of 8.

| Property | Value |
|----------|-------|
| Bit ordering | Little-endian bit order within each byte (`bit_idx % 8`). |
| `true` meaning | Field is `NULL`. |
| `false` meaning | Field is non-null. |
| Alignment | 8-byte aligned length. |

A tuple with no nullable fields has a zero-byte null bitmap.

## Slot

Each slot is 8 bytes, little-endian:

| Offset | Size | Field | Description |
|--------|------|-------|-------------|
| 0 | 4 | `offset` | Byte offset of the variable-length field data, relative to the start of the tuple. |
| 4 | 4 | `length` | Length of the variable-length field data in bytes. |

Slots are stored sequentially for all variable-length fields in schema order.

## Fixed-length data area

Fixed-length fields are placed at schema-defined offsets after the slot array. The offset and length of each fixed-length field are derived from the `TupleBinaryDesc` and are not duplicated inside the tuple bytes.

The fixed-length data area is padded so that the variable-length data area starts at an 8-byte aligned offset if required by the type descriptor.

## Variable-length data area

Variable-length field values are appended contiguously at the end of the tuple. Each value's location is described by its slot. The order of values in the variable-length area is the same as the order of variable-length fields in the schema.

A variable-length field value may be zero bytes long; in that case its slot has `length == 0` and `offset` points to the end of the tuple.

## TupleBinaryDesc

`TupleBinaryDesc` is the external schema descriptor that drives encoding and decoding. It is not part of the tuple bytes but is required to interpret them. The descriptor contains:

- `type_desc`: ordered list of field `DatType`s.
- `slot_all`: ordered list of `FieldDesc` records, each containing:
  - `is_fixed_len`: classification.
  - `slot`: offset and length for fixed fields; slot location for variable fields.
  - `nullable` and `null_bit_idx`: nullability information.
- `fixed_count`, `var_count`, `total_fixed_size`: derived counts.
- `nullable_count`: number of nullable fields (size of null bitmap).
- `row_format_version`: reserved for future tuple format revisions.

## Encoding procedure

1. Compute `min_tuple_size = null_bitmap_size + var_count * slot_size + total_fixed_size`.
2. Allocate a buffer of `min_tuple_size` bytes and zero it.
3. For each field in schema order:
   - If the value is `NULL`, set the corresponding null bitmap bit and skip data writing.
   - If the field is fixed-length, copy the encoded value to the fixed-data offset.
   - If the field is variable-length, append the encoded value to the end of the buffer and write its slot.
4. Copy the final null bitmap to the start of the buffer.

## Decoding procedure

1. Verify the tuple length is at least `min_tuple_size`.
2. Read the null bitmap from the start of the tuple.
3. For each field in schema order:
   - If the null bitmap indicates `NULL`, return `NULL`.
   - If the field is fixed-length, read `slot.length` bytes at `slot.offset`.
   - If the field is variable-length, read the slot at `slot.offset`, then read `slot.length` bytes at the offset stored in that slot.

## Integrity mechanisms

- **Length check:** decoders reject tuples shorter than `min_tuple_size`.
- **Null bitmap bounds:** bit indices are validated against `nullable_count`.
- **Slot bounds:** variable-length slots must point inside the tuple buffer.
- **Fixed-length size check:** encoded fixed-length values must match the descriptor size.

## Compatibility matrix

| Reader \ Writer | v1 |
|-----------------|----|
| v1 | Compatible |

Only version `1` is supported.

## Upgrade and rollback rules

- **Upgrade:** A future v2 tuple format must bump `row_format_version` and may add a tuple-level header, wider offsets, or a trailing checksum. V2 writers must not write v2 tuples until all readers in the deployment understand v2.
- **Rollback:** A v1-only reader encountering a v2 tuple returns `UnsupportedFormatVersion` and does not rewrite the tuple.
- **Migration:** Tuple format migration is offline. A migration tool reads v1 tuples using the old descriptor, re-encodes them as v2, and updates the table-level tuple format version.

## Deprecation policy

Version `1` may be deprecated only after:
1. An offline migration tool exists for all v1 tuples.
2. The new format has been the default for at least one release cycle.
3. CI continues to decode v1 golden tuples.

## References

- Page header: [page_header_v1.md](page_header_v1.md)
- Log frame: [log_frame_v1.md](log_frame_v1.md)
- File layout: [file_layout_v1.md](file_layout_v1.md)
- Implementation:
  - [`mudu_contract/src/tuple/tuple_binary.rs`](../../../mudu_contract/src/tuple/tuple_binary.rs)
  - [`mudu_contract/src/tuple/tuple_binary_desc.rs`](../../../mudu_contract/src/tuple/tuple_binary_desc.rs)
  - [`mudu_contract/src/tuple/slot.rs`](../../../mudu_contract/src/tuple/slot.rs)
  - [`mudu_contract/src/tuple/bitmap.rs`](../../../mudu_contract/src/tuple/bitmap.rs)
  - [`mudu_contract/src/tuple/nullable_tuple.rs`](../../../mudu_contract/src/tuple/nullable_tuple.rs)
