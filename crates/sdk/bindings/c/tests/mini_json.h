/*
 * Minimal JSON DOM parser and canonical serializer for the corpus driver.
 * Not part of the binding's public surface: it exists so the golden-corpus
 * sidecars can be parsed and their `expect` objects re-serialized in the
 * canonical form the driver's decoded-value renderers emit (object keys
 * sorted, minimal string escaping, numbers as raw text).
 */
#ifndef MINI_JSON_H
#define MINI_JSON_H

#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef enum {
    MJ_NULL,
    MJ_BOOL,
    MJ_NUMBER,
    MJ_STRING,
    MJ_ARRAY,
    MJ_OBJECT
} mj_kind;

typedef struct mj_value mj_value;

struct mj_value {
    mj_kind kind;
    union {
        int boolean;
        struct {
            const char *raw; /* borrows from the parsed text */
            size_t len;
        } number;
        struct {
            char *data; /* unescaped, owned */
            size_t len;
        } string;
        struct {
            mj_value **items; /* owned */
            size_t len;
            size_t cap;
        } array;
        struct {
            char **keys;      /* owned, in document order */
            mj_value **values; /* owned */
            size_t len;
            size_t cap;
        } object;
    } as;
};

/* Parse `len` bytes of JSON text; 0 on malformed input. Free with mj_free. */
mj_value *mj_parse(const char *text, size_t len);
void mj_free(mj_value *v);

const mj_value *mj_get(const mj_value *obj, const char *key);
size_t mj_len(const mj_value *arr);
const mj_value *mj_at(const mj_value *arr, size_t index);
int mj_is(const mj_value *v, mj_kind kind);
/* String value (not NUL-safe; use the length). */
const char *mj_str(const mj_value *v);
size_t mj_str_len(const mj_value *v);
/* Raw number text. */
const char *mj_num(const mj_value *v);
/* Text of a number or a string value (decimal-string conventions). */
const char *mj_text(const mj_value *v);
int mj_bool(const mj_value *v);

/* Growable byte buffer used by mj_serialize and the driver's renderers. */
typedef struct {
    char *data; /* owned (malloc) */
    size_t len;
    size_t cap;
    int oom;
} mj_buf;

void mj_buf_init(mj_buf *b);
void mj_buf_free(mj_buf *b);
void mj_buf_put(mj_buf *b, const char *data, size_t len);
void mj_buf_puts(mj_buf *b, const char *cstr);
void mj_buf_ch(mj_buf *b, char c);
/* Canonical string escaping: '"' '\\' and control characters. */
void mj_buf_put_escaped(mj_buf *b, const char *data, size_t len);
/* Append `value` serialized canonically (object keys sorted ascending). */
void mj_serialize(const mj_value *v, mj_buf *out);
/* Serialize and return the buffer as a NUL-terminated string (caller frees
 * with mj_buf_free; NUL not counted in len). */
const char *mj_serialized(const mj_value *v, mj_buf *scratch);

#ifdef __cplusplus
}
#endif

#endif /* MINI_JSON_H */
