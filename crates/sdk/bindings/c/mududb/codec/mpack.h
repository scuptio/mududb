/*
 * Minimal MessagePack reader/writer for the mududb C binding, covering
 * exactly the wire shapes the `uni-*.wit` messages use (see
 * `mudu_binding::codec::syscall_payload` on the host side).
 *
 * MSSP v1 shapes: records and request argument lists are MessagePack maps
 * keyed by the 1-based field/parameter numbers (WIT declaration order);
 * variants stay `[tag, payload]` 2-arrays; result bodies are
 * `[ok_tag, value]` with `0` = ok and `1` = `UniError`. Writes are canonical
 * (shortest integer/str form, matching rmp_serde); reads are lenient
 * (integer keys of any width, unknown keys skipped, missing fields
 * defaulted).
 *
 * Ownership contract (see readme.md for the full rules):
 *  - the writer owns its growable buffer; `mpw_free` releases it,
 *  - decode never points into the input buffer: every string, byte blob,
 *    array and boxed value the generated decoders produce is copied into a
 *    caller-provided `mp_arena`; `mpa_free` releases everything at once,
 *  - encode only borrows its input values.
 *
 * C99 (libc); also compiles as C++ — see readme.md.
 */
#ifndef MUDUDB_CODEC_MPACK_H
#define MUDUDB_CODEC_MPACK_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Linkage for the header-only generated codecs and MSSP frame helpers:
 * `static inline` in C99 (each translation unit keeps its own copy), plain
 * `inline` in C++ (definitions merge at link time and unused-function
 * warnings do not fire for the functions a given TU does not call). */
#ifdef __cplusplus
#define MP_INLINE inline
#else
#define MP_INLINE static inline
#endif

/* Allocation hook behind `mp_writer` growth and `mp_arena` blocks.
 * `old_ptr` may be NULL (fresh allocation); a `new_size` of 0 releases
 * `old_ptr` and returns NULL. Returns NULL on failure. `old_size` is
 * informational for hooks that track sizes. The default libc-backed
 * implementation lives in mpack_alloc.c; replace that file to retarget. */
void *mp_realloc(void *old_ptr, size_t old_size, size_t new_size);

/* ---- value slices ---- */

/* UTF-8 byte slice (string-family scalars): not necessarily NUL-terminated
 * on the encode side; decode copies land in the arena and ARE
 * NUL-terminated for convenience (the terminator is not counted in `len`). */
typedef struct {
    const char *data;
    uint32_t len;
} mp_str;

/* Raw byte slice (`list<u8>` / binary values). */
typedef struct {
    const uint8_t *data;
    uint32_t len;
} mp_bin;

mp_str mp_str_from(const char *data, uint32_t len);
mp_str mp_str_from_cstr(const char *s); /* length computed with strlen */
mp_bin mp_bin_from(const uint8_t *data, uint32_t len);

/* ---- decode arena ---- */

/* Bump allocator over a chain of grow-only blocks. Every allocation the
 * generated decoders make comes from the arena the caller passes in; the
 * caller releases all of it with one `mpa_free`. Allocation failure is
 * sticky (`mpa_ok` returns 0 afterwards) and generated decoders convert it
 * into a decode failure. Allocations are 8-byte aligned; a zero size yields
 * a valid non-NULL pointer. */
typedef struct mp_arena_block {
    struct mp_arena_block *next;
    size_t used;
    size_t cap;
    /* `cap` payload bytes follow */
} mp_arena_block;

typedef struct {
    mp_arena_block *blocks; /* chain, newest first */
    int oom;                /* sticky: an allocation failed */
} mp_arena;

void mpa_init(mp_arena *a);
void mpa_free(mp_arena *a);
void *mpa_alloc(mp_arena *a, size_t size);
int mpa_ok(const mp_arena *a);

/* ---- writer ---- */

typedef struct {
    uint8_t *buf; /* owned; grows via mp_realloc; released by mpw_free */
    size_t len;
    size_t cap;
    int oom;      /* sticky: an allocation failed */
    int err;      /* sticky: an invalid value was flagged with mpw_fail */
} mp_writer;

void mpw_init(mp_writer *w);
void mpw_free(mp_writer *w);
int mpw_ok(const mp_writer *w); /* 0 once any write ran out of memory or failed */
void mpw_fail(mp_writer *w);    /* flag an invalid value (e.g. unknown variant tag) */

void mpw_array_header(mp_writer *w, uint32_t count);
void mpw_map_header(mp_writer *w, uint32_t count);
void mpw_u64(mp_writer *w, uint64_t value);
void mpw_i64(mp_writer *w, int64_t value);
void mpw_f32(mp_writer *w, float value);
void mpw_f64(mp_writer *w, double value);
void mpw_bool(mp_writer *w, int value);
void mpw_nil(mp_writer *w);
void mpw_str(mp_writer *w, const char *data, uint32_t len);
void mpw_bin(mp_writer *w, const uint8_t *data, uint32_t len);
/* Append `len` raw bytes (no MessagePack marker); used for MSSP headers. */
void mpw_raw(mp_writer *w, const void *data, uint32_t len);

/* ---- reader ---- */

typedef struct {
    const uint8_t *data;
    size_t len;
    size_t pos;
    int err; /* sticky: malformed or truncated input */
} mp_reader;

void mpr_init(mp_reader *r, const uint8_t *data, size_t len);
int mpr_ok(const mp_reader *r); /* 0 once any read failed */
void mpr_fail(mp_reader *r);    /* flag a semantic violation (e.g. unknown tag) */
int mpr_done(const mp_reader *r); /* 1 when the input was consumed exactly */
/* The marker byte of the next value without consuming it (e.g. 0x80-0x8f is
 * fixmap); -1 when the input is exhausted or the reader is already in error.
 * Lets a generic value dispatcher (the record bridge) choose the read
 * function for the upcoming value. */
int mpr_peek_code(mp_reader *r);

uint32_t mpr_array_header(mp_reader *r);
uint32_t mpr_map_header(mp_reader *r);
/* Any MessagePack integer family as u64 / i64 (rmp_serde picks the smallest
 * form per value, so every width must be accepted); out-of-range values set
 * the sticky error flag. */
uint64_t mpr_u64(mp_reader *r);
int64_t mpr_i64(mp_reader *r);
float mpr_f32(mp_reader *r);   /* f32 or f64 marker, truncated to float */
double mpr_f64(mp_reader *r);  /* f32 or f64 marker */
/* Bool marker — or, as a lenient read, ANY MessagePack integer family with
 * value 0 or 1: bool fields cross the Mudu procedure byte pipe as i32 0/1
 * (the host's uni-data-value vocabulary has no Bool case), and the generated
 * record codecs decode them through this function. Any other integer (or a
 * non-bool/non-integer marker) sets the sticky error flag. */
int mpr_bool(mp_reader *r);
/* Consume a nil marker if one is next; 1 when consumed, 0 otherwise. */
int mpr_try_nil(mp_reader *r);
/* Strings/blobs are NOT copied: the result borrows from the input buffer
 * and is NOT NUL-terminated; *out_len receives the byte length. Generated
 * decoders copy the slice into the arena before storing it. */
const char *mpr_str(mp_reader *r, uint32_t *out_len);
const uint8_t *mpr_bin(mp_reader *r, uint32_t *out_len);
/* Copy `len` raw bytes (no MessagePack marker); -1 (sticky error) on a
 * short read. Used for MSSP headers. */
int mpr_raw(mp_reader *r, uint8_t *out, uint32_t len);
/* Record/request map keys are field numbers; a non-integer key is meant to
 * be skipped together with its value (lenient decode, mirroring the
 * generated host decoders). */
int mpr_next_is_integer(mp_reader *r);
void mpr_skip(mp_reader *r); /* skip one complete value of any shape */

#ifdef __cplusplus
}
#endif

#endif /* MUDUDB_CODEC_MPACK_H */
