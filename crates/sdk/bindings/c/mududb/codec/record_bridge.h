/*
 * Record bridge between the `uni-data-value` envelope a Mudu host passes
 * across the procedure byte pipe and the MessagePack byte shapes the
 * mgen-generated C record codecs (`<type>_decode` / `<type>_encode` of the
 * project's own `types.wit`) consume and produce — the C analogue of the
 * Go binding's `types/bridge.go`.
 *
 * A user-defined record argument travels as the `uni_data_value` record
 * case: field entries in WIT declaration order, field names possibly empty
 * (the host drops them — positional semantics). `mp_record_bridge_write`
 * serializes that envelope as the integer-keyed MessagePack map (key i+1 is
 * field i) a generated `<type>_decode` reads directly; `profile_decode` &
 * friends stay lenient about missing or extra field numbers, so a host
 * record shorter or longer than the declared shape still decodes.
 *
 * The inverse (`mp_record_bridge_read`) wraps a record return value: the
 * generated `<type>_encode` renders the integer-keyed map and the bridge
 * reads it back into the record-case envelope the host expects, in the
 * host's transactable scalar vocabulary (see `uni_to` on the host side):
 * MessagePack bool -> I32 0/1 (the host has no Bool case), integers -> I32
 * when they fit and I64 otherwise, nil -> the Null scalar, maps/arrays
 * recurse into record/array cases. Middle gaps between present map keys
 * (an omitted `option` field) decode as the Null scalar so declaration
 * positions are preserved; trailing omitted keys cannot be recovered and
 * shrink the record — declare `option` fields LAST.
 *
 * Bool fields need one leniency on the decode side, and the runtime
 * provides it: the host sends a bool as I32 0/1, so `mpr_bool` accepts
 * integer markers 0/1 in addition to the bool markers (see mpack.h).
 *
 * Field shapes on the write side: scalars unwrap into their plain
 * MessagePack values (string-family scalars to str, `blob` and the binary
 * case to the record-context ARRAY of u8 the generated record codecs pin,
 * Null to nil), arrays and nested records recurse. 128-bit integers flag
 * the writer: the C backend has no u128/i128 record codec. On the read
 * side a MessagePack bin becomes the Blob scalar; str/bin payloads are
 * copied into the arena like every decoded value.
 *
 * Ownership (the readme.md contract): `mp_record_bridge_write` only borrows
 * the envelope; `mp_record_bridge_read` copies everything it builds into
 * the caller's `mp_arena`, released with one `mpa_free`. Record maps are
 * sanity-bounded (MP_RECORD_BRIDGE_MAX_FIELDS entries/keys) so a malformed
 * count cannot drive a wild allocation.
 *
 * C99 (libc); also compiles as C++ — see readme.md.
 */
#ifndef MUDUDB_CODEC_RECORD_BRIDGE_H
#define MUDUDB_CODEC_RECORD_BRIDGE_H

#include "mududb/codec/mpack.h"
#include "mududb/types/UniDataValue.h"

#ifdef __cplusplus
extern "C" {
#endif

/* Sanity bound on the entry count and the largest field key of a record map
 * the read side accepts; guards the arena allocations against malformed
 * counts. Generated record encoders emit exactly one entry per declared
 * field, so no legitimate record comes near it. */
#define MP_RECORD_BRIDGE_MAX_FIELDS 4096u

/* Serialize the record-case envelope `value` as the integer-keyed
 * MessagePack map the mgen-generated C record decoders read (see the file
 * comment). Only borrows `value`. 0 on success; -1 — with the writer
 * flagged (`mpw_ok` reports 0) — when `value` is not the record case or a
 * field shape has no C record codec form. */
int mp_record_bridge_write(const uni_data_value *value, mp_writer *w);

/* Read one MessagePack map (as produced by a generated `<type>_encode`)
 * and build the record-case envelope the host expects back, in the host's
 * transactable scalar vocabulary (see the file comment). Everything built
 * lands in `a`. 0 on success; -1 — with the reader's sticky error set — on
 * malformed input, a map over the sanity bound, or arena exhaustion. */
int mp_record_bridge_read(mp_reader *r, mp_arena *a, uni_data_value *out);

#ifdef __cplusplus
}
#endif

#endif /* MUDUDB_CODEC_RECORD_BRIDGE_H */
