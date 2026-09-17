/*
 * Syscall layer for the wallet-c guest: the same facade contract the
 * mpm-crate C template's `mudu_sys.h` declares (the mtp-generated adapter
 * translation unit includes this header and drives `mudu_run_proc`), but
 * implemented over the formal C binding (`mududb/codec/mpack.h` +
 * `mududb/types/Uni*.h`, corpus-verified) instead of a hand-rolled codec.
 * See mudu_sys.c for the framing contract; see the binding's readme.md for
 * the ownership conventions (encode borrows, decode copies into an arena).
 *
 * This translation unit also carries the minimal freestanding runtime the
 * libc-less build needs: the bump allocator behind `cabi_realloc`, the
 * `realloc`/`free` shims behind the binding's default `mp_realloc`
 * (mpack_alloc.c), and memcpy & friends.
 *
 * Freestanding C99.
 */
#ifndef MUDU_SYS_H
#define MUDU_SYS_H

#include <stddef.h>
#include <stdint.h>

#include "mududb/types/UniDataValue.h"
#include "mududb/types/UniOid.h"

#ifdef __cplusplus
extern "C" {
#endif

/* Numeric mirrors of the host's `mudu::error::ErrorCode` discriminants
 * carried in the `UniError` result arm. */
#define MUDU_EC_INTERNAL 50000u
#define MUDU_EC_ENTITY_NOT_FOUND 50009u
#define MUDU_EC_DOMAIN_VIOLATION 50017u
#define MUDU_EC_INVALID_ARGUMENT 50029u

/* The session OID: an alias of the binding's `uni_oid` so the decoded
 * procedure parameter feeds straight into the syscall DTOs. */
typedef uni_oid mudu_oid;

/* One SQL datum: the scalar subset of `UniDataValue` this guest speaks,
 * plus the record passthrough for user-defined record parameters/returns.
 * Strings are byte slices, not necessarily NUL-terminated. */
enum mudu_datum_kind {
    MUDU_DATUM_NULL = 0,
    MUDU_DATUM_I64 = 1,
    MUDU_DATUM_F64 = 2,
    MUDU_DATUM_STR = 3,
    /* A user-defined record: the decoded `uni_data_value` record-case
     * envelope, positionally ordered (WIT declaration order, names dropped
     * by the host). The pointer references the binding's decode arena,
     * which lives in the guest arena until the guest call ends; a procedure
     * decodes it through the record bridge (`mp_record_bridge_write`)
     * composed with the mgen-generated `<type>_decode`, and builds a return
     * record symmetrically (`<type>_encode` + `mp_record_bridge_read`). */
    MUDU_DATUM_RECORD = 4
};

typedef struct {
    int kind; /* one of enum mudu_datum_kind */
    int64_t i64;
    double f64;
    const char *str;
    uint32_t str_len;
    const uni_data_value *record; /* MUDU_DATUM_RECORD payload (arena-owned) */
} mudu_datum;

mudu_datum mudu_null(void);
mudu_datum mudu_i64(int64_t value);
mudu_datum mudu_f64(double value);
mudu_datum mudu_str(const char *data, uint32_t len);
mudu_datum mudu_record(const uni_data_value *value);

/* Guest error: set by the syscall wrappers and procedures, encoded as the
 * `{1: UniError}` arm of the procedure result by `mudu_run_proc`. */
typedef struct {
    uint32_t code;
    const char *msg; /* byte slice, not necessarily NUL-terminated */
    uint32_t msg_len;
} mudu_error;

void mudu_error_set(mudu_error *err, uint32_t code, const char *msg);
int mudu_error_is_set(const mudu_error *err);

/* Rows returned by `mudu_query`; every cell is decoded as a scalar datum.
 * All memory lives in the guest arena until the guest call is done. */
typedef struct {
    uint32_t n_fields;
    mudu_datum *fields;
} mudu_row;

typedef struct {
    uint32_t n_rows;
    mudu_row *rows;
} mudu_rows;

/* SQL over the `mududb:api/system` byte pipe, MSSP-framed with the binding's
 * `uni_syscall_*_request_encode` / `*_result_decode` codecs. Both return 0
 * on success and -1 on a host error (`err` filled from the `UniError` arm)
 * or a framing/codec failure (`MUDU_EC_INTERNAL`). */
int mudu_query(mudu_oid session, const char *sql, const mudu_datum *args, uint32_t n_args,
               mudu_rows *out, mudu_error *err);
int mudu_command(mudu_oid session, const char *sql, const mudu_datum *args, uint32_t n_args,
                 uint64_t *affected, mudu_error *err);

/* ---- procedure byte-pipe ---- */

/* Decoded `UniProcedureParam`: `{1: procedure, 2: session, 3: param_list}`. */
typedef struct {
    uint64_t procedure;
    mudu_oid session;
    uint32_t n_params;
    mudu_datum *params;
} mudu_proc_param;

/* One procedure implementation: read `param`, write the single return
 * datum and return 0; or set `err` and return -1. */
typedef int (*mudu_proc_fn)(const mudu_proc_param *param, mudu_datum *result, mudu_error *err);

/* Drives one `mp2-*` export: decode the `UniProcedureParam`, invoke `fn`,
 * encode the `UniResult<UniProcedureResult, UniError>` reply, and hand the
 * reply (ptr, len) pair to the canonical-ABI lift glue: the returned value
 * points at 8 bytes holding the reply pointer and length as two u32s (the
 * lift convention for `func(param: list<u8>) -> list<u8>`). */
uint32_t mudu_run_proc(const uint8_t *param_ptr, uint32_t param_len, mudu_proc_fn fn);

/* The guest arena: a bump allocator over linear memory (procedure calls
 * are short-lived; nothing is ever freed). It also backs `cabi_realloc`,
 * which the canonical-ABI glue calls into. */
void *mudu_arena_alloc(size_t size, size_t align);

/* libc symbols the compiler may emit calls to (struct copies, large
 * initializers, loop idioms); provided here because the build has no libc.
 * `-fno-builtin` in Makefile.toml stops the compiler from recognizing these
 * loops and rewriting them into calls of themselves. */
void *memcpy(void *dst, const void *src, size_t n);
void *memmove(void *dst, const void *src, size_t n);
void *memset(void *dst, int c, size_t n);
int memcmp(const void *a, const void *b, size_t n);
size_t strlen(const char *s);

/* libc allocation shims behind the binding's default `mp_realloc`
 * (mpack_alloc.c routes `mp_writer` growth and `mp_arena` blocks through
 * `realloc`/`free`): arena-backed, `free` is a no-op (the bump arena is
 * released en masse when the guest call ends). */
void *realloc(void *old_ptr, size_t new_size);
void free(void *ptr);

#ifdef __cplusplus
}
#endif

#endif /* MUDU_SYS_H */
