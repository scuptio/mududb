/*
 * Minimal MessagePack reader/writer for the mududb C binding — see mpack.h
 * for the wire contract and the ownership rules. Started from the
 * mpm-crate C guest template (canonical write / lenient read) and extended
 * with the bool/nil/f32/bin/raw shapes, the decode arena and the slice
 * types the generated message codecs use.
 */
#include "mpack.h"

#include <string.h>

/* ---- value slices ---- */

mp_str mp_str_from(const char *data, uint32_t len) {
    mp_str s;
    s.data = data;
    s.len = len;
    return s;
}

mp_str mp_str_from_cstr(const char *s) {
    mp_str v;
    v.data = s;
    v.len = (uint32_t)strlen(s);
    return v;
}

mp_bin mp_bin_from(const uint8_t *data, uint32_t len) {
    mp_bin b;
    b.data = data;
    b.len = len;
    return b;
}

/* ---- decode arena ---- */

#define MPA_ALIGN 8u
#define MPA_BLOCK_CAP 4096u

void mpa_init(mp_arena *a) {
    a->blocks = 0;
    a->oom = 0;
}

int mpa_ok(const mp_arena *a) {
    return !a->oom;
}

void mpa_free(mp_arena *a) {
    mp_arena_block *b = a->blocks;
    while (b) {
        mp_arena_block *next = b->next;
        mp_realloc(b, sizeof(mp_arena_block) + b->cap, 0);
        b = next;
    }
    a->blocks = 0;
    a->oom = 0;
}

static size_t mpa_align_up(size_t n) {
    return (n + (MPA_ALIGN - 1u)) & ~(size_t)(MPA_ALIGN - 1u);
}

void *mpa_alloc(mp_arena *a, size_t size) {
    mp_arena_block *b;
    size_t cap;
    if (a->oom) {
        return 0;
    }
    if (size == 0) {
        /* Zero-size allocations still yield a usable pointer. */
        size = 1;
    }
    b = a->blocks;
    if (b && mpa_align_up(b->used) + size <= b->cap) {
        void *p = (uint8_t *)(b + 1) + mpa_align_up(b->used);
        b->used = mpa_align_up(b->used) + size;
        return p;
    }
    cap = MPA_BLOCK_CAP;
    if (cap < size) {
        cap = size;
    }
    b = (mp_arena_block *)mp_realloc(0, 0, sizeof(mp_arena_block) + cap);
    if (!b) {
        a->oom = 1;
        return 0;
    }
    b->next = a->blocks;
    b->used = size;
    b->cap = cap;
    a->blocks = b;
    return (void *)(b + 1);
}

/* ---- writer ---- */

void mpw_init(mp_writer *w) {
    w->buf = 0;
    w->len = 0;
    w->cap = 0;
    w->oom = 0;
    w->err = 0;
}

void mpw_free(mp_writer *w) {
    if (w->buf) {
        mp_realloc(w->buf, w->cap, 0);
    }
    mpw_init(w);
}

int mpw_ok(const mp_writer *w) {
    return !w->oom && !w->err;
}

void mpw_fail(mp_writer *w) {
    w->err = 1;
}

static void mpw_reserve(mp_writer *w, size_t extra) {
    if (w->oom) {
        return;
    }
    if (w->len + extra <= w->cap) {
        return;
    }
    {
        size_t cap = w->cap ? w->cap : 256;
        uint8_t *buf;
        while (cap < w->len + extra) {
            cap *= 2;
        }
        buf = (uint8_t *)mp_realloc(w->buf, w->cap, cap);
        if (!buf) {
            w->oom = 1;
            return;
        }
        w->buf = buf;
        w->cap = cap;
    }
}

static void mpw_put(mp_writer *w, uint8_t b) {
    mpw_reserve(w, 1);
    if (w->oom) {
        return;
    }
    w->buf[w->len++] = b;
}

static void mpw_put_be16(mp_writer *w, uint16_t v) {
    mpw_put(w, (uint8_t)(v >> 8));
    mpw_put(w, (uint8_t)v);
}

static void mpw_put_be32(mp_writer *w, uint32_t v) {
    mpw_put(w, (uint8_t)(v >> 24));
    mpw_put(w, (uint8_t)(v >> 16));
    mpw_put(w, (uint8_t)(v >> 8));
    mpw_put(w, (uint8_t)v);
}

static void mpw_put_be64(mp_writer *w, uint64_t v) {
    int i;
    for (i = 7; i >= 0; i--) {
        mpw_put(w, (uint8_t)(v >> (i * 8)));
    }
}

void mpw_array_header(mp_writer *w, uint32_t count) {
    if (count < 16) {
        mpw_put(w, (uint8_t)(0x90 | count));
    } else if (count <= 0xffffu) {
        mpw_put(w, 0xdc);
        mpw_put_be16(w, (uint16_t)count);
    } else {
        mpw_put(w, 0xdd);
        mpw_put_be32(w, count);
    }
}

void mpw_map_header(mp_writer *w, uint32_t count) {
    if (count < 16) {
        mpw_put(w, (uint8_t)(0x80 | count));
    } else if (count <= 0xffffu) {
        mpw_put(w, 0xde);
        mpw_put_be16(w, (uint16_t)count);
    } else {
        mpw_put(w, 0xdf);
        mpw_put_be32(w, count);
    }
}

void mpw_u64(mp_writer *w, uint64_t value) {
    if (value < 0x80u) {
        mpw_put(w, (uint8_t)value);
    } else if (value <= 0xffu) {
        mpw_put(w, 0xcc);
        mpw_put(w, (uint8_t)value);
    } else if (value <= 0xffffu) {
        mpw_put(w, 0xcd);
        mpw_put_be16(w, (uint16_t)value);
    } else if (value <= 0xffffffffu) {
        mpw_put(w, 0xce);
        mpw_put_be32(w, (uint32_t)value);
    } else {
        mpw_put(w, 0xcf);
        mpw_put_be64(w, value);
    }
}

void mpw_i64(mp_writer *w, int64_t value) {
    if (value >= 0) {
        mpw_u64(w, (uint64_t)value);
    } else if (value >= -32) {
        mpw_put(w, (uint8_t)(0xe0 | (uint8_t)(value + 32)));
    } else if (value >= -128) {
        mpw_put(w, 0xd0);
        mpw_put(w, (uint8_t)(int8_t)value);
    } else if (value >= -32768) {
        mpw_put(w, 0xd1);
        mpw_put_be16(w, (uint16_t)(int16_t)value);
    } else if (value >= -2147483648LL) {
        mpw_put(w, 0xd2);
        mpw_put_be32(w, (uint32_t)(int32_t)value);
    } else {
        mpw_put(w, 0xd3);
        mpw_put_be64(w, (uint64_t)value);
    }
}

void mpw_f32(mp_writer *w, float value) {
    union {
        float f;
        uint32_t u;
    } cv;
    cv.f = value;
    mpw_put(w, 0xca);
    mpw_put_be32(w, cv.u);
}

void mpw_f64(mp_writer *w, double value) {
    union {
        double d;
        uint64_t u;
    } cv;
    cv.d = value;
    mpw_put(w, 0xcb);
    mpw_put_be64(w, cv.u);
}

void mpw_bool(mp_writer *w, int value) {
    mpw_put(w, value ? 0xc3 : 0xc2);
}

void mpw_nil(mp_writer *w) {
    mpw_put(w, 0xc0);
}

static void mpw_bytes(mp_writer *w, const uint8_t *data, uint32_t len) {
    mpw_reserve(w, len);
    if (w->oom || len == 0) {
        return;
    }
    memcpy(w->buf + w->len, data, len);
    w->len += len;
}

void mpw_str(mp_writer *w, const char *data, uint32_t len) {
    if (len < 32) {
        mpw_put(w, (uint8_t)(0xa0 | len));
    } else if (len <= 0xffu) {
        mpw_put(w, 0xd9);
        mpw_put(w, (uint8_t)len);
    } else if (len <= 0xffffu) {
        mpw_put(w, 0xda);
        mpw_put_be16(w, (uint16_t)len);
    } else {
        mpw_put(w, 0xdb);
        mpw_put_be32(w, len);
    }
    mpw_bytes(w, (const uint8_t *)data, len);
}

void mpw_bin(mp_writer *w, const uint8_t *data, uint32_t len) {
    if (len <= 0xffu) {
        mpw_put(w, 0xc4);
        mpw_put(w, (uint8_t)len);
    } else if (len <= 0xffffu) {
        mpw_put(w, 0xc5);
        mpw_put_be16(w, (uint16_t)len);
    } else {
        mpw_put(w, 0xc6);
        mpw_put_be32(w, len);
    }
    mpw_bytes(w, data, len);
}

void mpw_raw(mp_writer *w, const void *data, uint32_t len) {
    mpw_bytes(w, (const uint8_t *)data, len);
}

/* ---- reader ---- */

void mpr_init(mp_reader *r, const uint8_t *data, size_t len) {
    r->data = data;
    r->len = len;
    r->pos = 0;
    r->err = 0;
}

int mpr_ok(const mp_reader *r) {
    return !r->err;
}

void mpr_fail(mp_reader *r) {
    r->err = 1;
}

int mpr_done(const mp_reader *r) {
    return r->pos == r->len;
}

int mpr_peek_code(mp_reader *r) {
    if (r->err || r->pos >= r->len) {
        return -1;
    }
    return r->data[r->pos];
}

static uint8_t mpr_take(mp_reader *r) {
    if (r->err || r->pos >= r->len) {
        r->err = 1;
        return 0;
    }
    return r->data[r->pos++];
}

static uint8_t mpr_peek(mp_reader *r) {
    if (r->err || r->pos >= r->len) {
        r->err = 1;
        return 0;
    }
    return r->data[r->pos];
}

static void mpr_advance(mp_reader *r, size_t count) {
    if (r->err || r->pos + count > r->len) {
        r->err = 1;
        return;
    }
    r->pos += count;
}

static uint16_t mpr_take_be16(mp_reader *r) {
    uint16_t hi = mpr_take(r);
    return (uint16_t)((hi << 8) | mpr_take(r));
}

static uint32_t mpr_take_be32(mp_reader *r) {
    uint32_t v = mpr_take_be16(r);
    return (v << 16) | mpr_take_be16(r);
}

static uint64_t mpr_take_be64(mp_reader *r) {
    uint64_t v = mpr_take_be32(r);
    return (v << 32) | mpr_take_be32(r);
}

uint32_t mpr_array_header(mp_reader *r) {
    uint8_t code = mpr_take(r);
    if ((code & 0xf0) == 0x90) {
        return (uint32_t)(code & 0x0f);
    }
    switch (code) {
    case 0xdc:
        return mpr_take_be16(r);
    case 0xdd:
        return mpr_take_be32(r);
    default:
        r->err = 1;
        return 0;
    }
}

uint32_t mpr_map_header(mp_reader *r) {
    uint8_t code = mpr_take(r);
    if ((code & 0xf0) == 0x80) {
        return (uint32_t)(code & 0x0f);
    }
    switch (code) {
    case 0xde:
        return mpr_take_be16(r);
    case 0xdf:
        return mpr_take_be32(r);
    default:
        r->err = 1;
        return 0;
    }
}

uint64_t mpr_u64(mp_reader *r) {
    uint8_t code = mpr_take(r);
    int64_t sv;
    if (r->err) {
        return 0;
    }
    if (code <= 0x7f) {
        return code;
    }
    switch (code) {
    case 0xcc:
        return mpr_take(r);
    case 0xcd:
        return mpr_take_be16(r);
    case 0xce:
        return mpr_take_be32(r);
    case 0xcf:
        return mpr_take_be64(r);
    case 0xd0:
        sv = (int8_t)mpr_take(r);
        break;
    case 0xd1:
        sv = (int16_t)mpr_take_be16(r);
        break;
    case 0xd2:
        sv = (int32_t)mpr_take_be32(r);
        break;
    case 0xd3:
        sv = (int64_t)mpr_take_be64(r);
        break;
    default:
        r->err = 1;
        return 0;
    }
    if (sv < 0) {
        r->err = 1;
        return 0;
    }
    return (uint64_t)sv;
}

int64_t mpr_i64(mp_reader *r) {
    uint8_t code = mpr_take(r);
    uint64_t uv;
    if (r->err) {
        return 0;
    }
    if (code <= 0x7f) {
        return code;
    }
    if (code >= 0xe0) {
        return (int8_t)code;
    }
    switch (code) {
    case 0xcc:
        return mpr_take(r);
    case 0xcd:
        return mpr_take_be16(r);
    case 0xce:
        return mpr_take_be32(r);
    case 0xcf:
        uv = mpr_take_be64(r);
        if (uv > 0x7fffffffffffffffULL) {
            r->err = 1;
            return 0;
        }
        return (int64_t)uv;
    case 0xd0:
        return (int8_t)mpr_take(r);
    case 0xd1:
        return (int16_t)mpr_take_be16(r);
    case 0xd2:
        return (int32_t)mpr_take_be32(r);
    case 0xd3:
        return (int64_t)mpr_take_be64(r);
    default:
        r->err = 1;
        return 0;
    }
}

float mpr_f32(mp_reader *r) {
    return (float)mpr_f64(r);
}

double mpr_f64(mp_reader *r) {
    union {
        double d;
        uint64_t u;
    } cv;
    union {
        float f;
        uint32_t u;
    } cf;
    uint8_t code = mpr_take(r);
    switch (code) {
    case 0xca:
        cf.u = mpr_take_be32(r);
        return (double)cf.f;
    case 0xcb:
        cv.u = mpr_take_be64(r);
        return cv.d;
    default:
        r->err = 1;
        return 0;
    }
}

int mpr_bool(mp_reader *r) {
    uint8_t code = mpr_peek(r);
    if (r->err) {
        return 0;
    }
    switch (code) {
    case 0xc2:
        r->pos++;
        return 0;
    case 0xc3:
        r->pos++;
        return 1;
    default:
        break;
    }
    /* Lenient read: a bool field crossing the Mudu procedure byte pipe
     * arrives as an i32 (0/1) — see mpack.h. Accept any integer marker with
     * value 0/1 (mpr_u64 already rejects negative and >u64 values). */
    if (mpr_next_is_integer(r)) {
        uint64_t v = mpr_u64(r);
        if (r->err) {
            return 0;
        }
        if (v == 0) {
            return 0;
        }
        if (v == 1) {
            return 1;
        }
    }
    r->err = 1;
    return 0;
}

int mpr_try_nil(mp_reader *r) {
    uint8_t code = mpr_peek(r);
    if (r->err) {
        return 0;
    }
    if (code == 0xc0) {
        r->pos++;
        return 1;
    }
    return 0;
}

int mpr_next_is_integer(mp_reader *r) {
    uint8_t code = mpr_peek(r);
    if (r->err) {
        return 0;
    }
    return code <= 0x7f || code >= 0xe0 || (code >= 0xcc && code <= 0xd3);
}

static const char mpr_empty[] = "";

const char *mpr_str(mp_reader *r, uint32_t *out_len) {
    uint8_t code = mpr_take(r);
    uint32_t len;
    if (r->err) {
        *out_len = 0;
        return mpr_empty;
    }
    if ((code & 0xe0) == 0xa0) {
        len = (uint32_t)(code & 0x1f);
    } else {
        switch (code) {
        case 0xd9:
            len = mpr_take(r);
            break;
        case 0xda:
            len = mpr_take_be16(r);
            break;
        case 0xdb:
            len = mpr_take_be32(r);
            break;
        default:
            r->err = 1;
            *out_len = 0;
            return mpr_empty;
        }
    }
    if (r->err || r->pos + len > r->len) {
        r->err = 1;
        *out_len = 0;
        return mpr_empty;
    }
    {
        const char *s = (const char *)(r->data + r->pos);
        r->pos += len;
        *out_len = len;
        return s;
    }
}

const uint8_t *mpr_bin(mp_reader *r, uint32_t *out_len) {
    uint8_t code = mpr_take(r);
    uint32_t len;
    if (r->err) {
        *out_len = 0;
        return (const uint8_t *)mpr_empty;
    }
    switch (code) {
    case 0xc4:
        len = mpr_take(r);
        break;
    case 0xc5:
        len = mpr_take_be16(r);
        break;
    case 0xc6:
        len = mpr_take_be32(r);
        break;
    default:
        r->err = 1;
        *out_len = 0;
        return (const uint8_t *)mpr_empty;
    }
    if (r->err || r->pos + len > r->len) {
        r->err = 1;
        *out_len = 0;
        return (const uint8_t *)mpr_empty;
    }
    {
        const uint8_t *s = r->data + r->pos;
        r->pos += len;
        *out_len = len;
        return s;
    }
}

int mpr_raw(mp_reader *r, uint8_t *out, uint32_t len) {
    if (r->err || r->pos + len > r->len) {
        r->err = 1;
        return -1;
    }
    if (len > 0) {
        memcpy(out, r->data + r->pos, len);
    }
    r->pos += len;
    return 0;
}

void mpr_skip(mp_reader *r) {
    uint8_t code = mpr_peek(r);
    uint32_t count;
    uint32_t i;
    if (r->err) {
        return;
    }
    if (code <= 0x7f || code >= 0xe0 || code == 0xc0 || code == 0xc2 || code == 0xc3) {
        mpr_take(r);
        return;
    }
    switch (code) {
    case 0xcc:
    case 0xd0:
        mpr_advance(r, 2);
        return;
    case 0xcd:
    case 0xd1:
        mpr_advance(r, 3);
        return;
    case 0xce:
    case 0xd2:
    case 0xca: /* f32 */
        mpr_advance(r, 5);
        return;
    case 0xcf:
    case 0xd3:
    case 0xcb: /* f64 */
        mpr_advance(r, 9);
        return;
    case 0xd4: /* fixext 1 */
        mpr_advance(r, 2);
        return;
    case 0xd5: /* fixext 2 */
        mpr_advance(r, 3);
        return;
    case 0xd6: /* fixext 4 */
        mpr_advance(r, 5);
        return;
    case 0xd7: /* fixext 8 */
        mpr_advance(r, 9);
        return;
    case 0xd8: /* fixext 16 */
        mpr_advance(r, 17);
        return;
    default:
        break;
    }
    if ((code & 0xe0) == 0xa0) {
        mpr_advance(r, 1 + (size_t)(code & 0x1f));
        return;
    }
    if ((code & 0xf0) == 0x90) {
        count = (uint32_t)(code & 0x0f);
        mpr_take(r);
        for (i = 0; i < count && mpr_ok(r); i++) {
            mpr_skip(r);
        }
        return;
    }
    if ((code & 0xf0) == 0x80) {
        count = (uint32_t)(code & 0x0f);
        mpr_take(r);
        for (i = 0; i < count && mpr_ok(r); i++) {
            mpr_skip(r);
            mpr_skip(r);
        }
        return;
    }
    switch (code) {
    case 0xd9: /* str 8 */
    case 0xc4: /* bin 8 */
        mpr_take(r); /* code */
        count = mpr_take(r);
        mpr_advance(r, count);
        return;
    case 0xda: /* str 16 */
    case 0xc5: /* bin 16 */
        mpr_take(r); /* code */
        count = mpr_take_be16(r);
        mpr_advance(r, count);
        return;
    case 0xdb: /* str 32 */
    case 0xc6: /* bin 32 */
        mpr_take(r); /* code */
        count = mpr_take_be32(r);
        mpr_advance(r, count);
        return;
    case 0xdc: /* array 16 */
        mpr_take(r);
        count = mpr_take_be16(r);
        for (i = 0; i < count && mpr_ok(r); i++) {
            mpr_skip(r);
        }
        return;
    case 0xdd: /* array 32 */
        mpr_take(r);
        count = mpr_take_be32(r);
        for (i = 0; i < count && mpr_ok(r); i++) {
            mpr_skip(r);
        }
        return;
    case 0xde: /* map 16 */
        mpr_take(r);
        count = mpr_take_be16(r);
        for (i = 0; i < count && mpr_ok(r); i++) {
            mpr_skip(r);
            mpr_skip(r);
        }
        return;
    case 0xdf: /* map 32 */
        mpr_take(r);
        count = mpr_take_be32(r);
        for (i = 0; i < count && mpr_ok(r); i++) {
            mpr_skip(r);
            mpr_skip(r);
        }
        return;
    default:
        r->err = 1;
        return;
    }
}
