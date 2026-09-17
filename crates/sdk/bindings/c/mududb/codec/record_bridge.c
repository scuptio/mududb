/*
 * Record bridge between the `uni-data-value` envelope and the mgen-generated
 * C record codecs — see record_bridge.h for the wire contract and the
 * ownership rules.
 */
#include "record_bridge.h"

#include <string.h>

/* ---- envelope -> MessagePack (host record argument -> generated decoder) ---- */

/* `list<u8>` in record context: an array of u8, NOT a bin blob (the pinned
 * dual semantics the generated record codecs use). */
static void bridge_write_bytes(const uint8_t *data, uint32_t len, mp_writer *w) {
    uint32_t i;
    mpw_array_header(w, len);
    for (i = 0; i < len; i++) {
        mpw_u64(w, data[i]);
    }
}

static void bridge_write_scalar(const uni_scalar_value *s, mp_writer *w) {
    switch (s->kind) {
    case UNI_SCALAR_VALUE_BOOL:
        mpw_bool(w, s->as.bool_);
        break;
    case UNI_SCALAR_VALUE_U8:
        mpw_u64(w, s->as.u8);
        break;
    case UNI_SCALAR_VALUE_I8:
        mpw_i64(w, s->as.i8);
        break;
    case UNI_SCALAR_VALUE_U16:
        mpw_u64(w, s->as.u16);
        break;
    case UNI_SCALAR_VALUE_I16:
        mpw_i64(w, s->as.i16);
        break;
    case UNI_SCALAR_VALUE_U32:
        mpw_u64(w, s->as.u32);
        break;
    case UNI_SCALAR_VALUE_I32:
        mpw_i64(w, s->as.i32);
        break;
    case UNI_SCALAR_VALUE_U64:
        mpw_u64(w, s->as.u64);
        break;
    case UNI_SCALAR_VALUE_I64:
        mpw_i64(w, s->as.i64);
        break;
    case UNI_SCALAR_VALUE_U128:
    case UNI_SCALAR_VALUE_I128:
        /* the C backend has no u128/i128 record codec: flag the writer
         * instead of emitting a value no generated decoder accepts */
        mpw_fail(w);
        break;
    case UNI_SCALAR_VALUE_F32:
        mpw_f32(w, s->as.f32);
        break;
    case UNI_SCALAR_VALUE_F64:
        mpw_f64(w, s->as.f64);
        break;
    case UNI_SCALAR_VALUE_CHAR:
        mpw_str(w, &s->as.char_, 1u);
        break;
    case UNI_SCALAR_VALUE_STRING:
        mpw_str(w, (s->as.string).data, (s->as.string).len);
        break;
    case UNI_SCALAR_VALUE_BLOB:
        bridge_write_bytes((s->as.blob).data, (s->as.blob).len, w);
        break;
    case UNI_SCALAR_VALUE_NUMERIC:
        mpw_str(w, (s->as.numeric).data, (s->as.numeric).len);
        break;
    case UNI_SCALAR_VALUE_DATE:
        mpw_str(w, (s->as.date).data, (s->as.date).len);
        break;
    case UNI_SCALAR_VALUE_TIME:
        mpw_str(w, (s->as.time).data, (s->as.time).len);
        break;
    case UNI_SCALAR_VALUE_TIMESTAMP:
        mpw_str(w, (s->as.timestamp).data, (s->as.timestamp).len);
        break;
    case UNI_SCALAR_VALUE_TIMESTAMP_TZ:
        mpw_str(w, (s->as.timestamp_tz).data, (s->as.timestamp_tz).len);
        break;
    case UNI_SCALAR_VALUE_NULL:
        mpw_nil(w);
        break;
    default:
        mpw_fail(w);
        break;
    }
}

static void bridge_write_value(const uni_data_value *v, mp_writer *w);

static void bridge_write_record(const uni_data_value_record_list *record, mp_writer *w) {
    uint32_t i;
    /* positional: key i+1 is envelope field i (the host drops field names) */
    mpw_map_header(w, record->len);
    for (i = 0; i < record->len; i++) {
        mpw_u64(w, (uint64_t)(i + 1u));
        bridge_write_value(&(record->items[i].field_value), w);
    }
}

static void bridge_write_value(const uni_data_value *v, mp_writer *w) {
    uint32_t i;
    switch (v->kind) {
    case UNI_DATA_VALUE_SCALAR:
        if (v->as.scalar == 0) {
            mpw_fail(w);
        } else {
            bridge_write_scalar(v->as.scalar, w);
        }
        break;
    case UNI_DATA_VALUE_ARRAY:
        mpw_array_header(w, (v->as.array).len);
        for (i = 0; i < (v->as.array).len; i++) {
            bridge_write_value(&(v->as.array).items[i], w);
        }
        break;
    case UNI_DATA_VALUE_RECORD:
        bridge_write_record(&(v->as.record), w);
        break;
    case UNI_DATA_VALUE_BINARY:
        bridge_write_bytes((v->as.binary).data, (v->as.binary).len, w);
        break;
    default:
        mpw_fail(w);
        break;
    }
}

int mp_record_bridge_write(const uni_data_value *value, mp_writer *w) {
    if (value->kind != UNI_DATA_VALUE_RECORD) {
        mpw_fail(w);
        return -1;
    }
    bridge_write_record(&(value->as.record), w);
    return mpw_ok(w) ? 0 : -1;
}

/* ---- MessagePack -> envelope (generated encoder -> host record result) ---- */

/* One decoded (key, value) pair of a record map, before the gap-filled
 * positional field array is built. */
typedef struct {
    uint64_t key;
    uni_data_value value;
} bridge_pair;

static int bridge_read_value(mp_reader *r, mp_arena *a, uni_data_value *out);

/* MessagePack integer families: positive/negative fixint and u8..i64. */
static int bridge_is_integer_code(int code) {
    return code <= 0x7f || code >= 0xe0 || (code >= 0xcc && code <= 0xd3);
}

/* Allocate the scalar payload of `out` in the arena, tag both, and return
 * the scalar for the caller to fill; 0 (with the reader flagged) on arena
 * exhaustion. */
static uni_scalar_value *bridge_scalar(mp_reader *r, mp_arena *a, uni_data_value *out,
                                       uni_scalar_value_kind kind) {
    uni_scalar_value *s = (uni_scalar_value *)mpa_alloc(a, sizeof(uni_scalar_value));
    if (s == 0) {
        mpr_fail(r);
        return 0;
    }
    memset(s, 0, sizeof(*s));
    s->kind = kind;
    out->kind = UNI_DATA_VALUE_SCALAR;
    out->as.scalar = s;
    return s;
}

static int bridge_read_scalar_int(mp_reader *r, mp_arena *a, uni_data_value *out) {
    int64_t v = mpr_i64(r);
    uni_scalar_value *s;
    if (!mpr_ok(r)) {
        return -1;
    }
    if (v >= (int64_t)INT32_MIN && v <= (int64_t)INT32_MAX) {
        s = bridge_scalar(r, a, out, UNI_SCALAR_VALUE_I32);
        if (s == 0) {
            return -1;
        }
        s->as.i32 = (int32_t)v;
    } else {
        s = bridge_scalar(r, a, out, UNI_SCALAR_VALUE_I64);
        if (s == 0) {
            return -1;
        }
        s->as.i64 = v;
    }
    return 0;
}

static int bridge_read_scalar_str(mp_reader *r, mp_arena *a, uni_data_value *out) {
    uint32_t n = 0;
    const char *p = mpr_str(r, &n);
    char *copy;
    uni_scalar_value *s;
    if (!mpr_ok(r)) {
        return -1;
    }
    /* decode copies are NUL-terminated for convenience (not counted) */
    copy = (char *)mpa_alloc(a, (size_t)n + 1u);
    if (copy == 0) {
        mpr_fail(r);
        return -1;
    }
    memcpy(copy, p, n);
    copy[n] = '\0';
    s = bridge_scalar(r, a, out, UNI_SCALAR_VALUE_STRING);
    if (s == 0) {
        return -1;
    }
    s->as.string = mp_str_from(copy, n);
    return 0;
}

static int bridge_read_scalar_bin(mp_reader *r, mp_arena *a, uni_data_value *out) {
    uint32_t n = 0;
    const uint8_t *p = mpr_bin(r, &n);
    uint8_t *copy;
    uni_scalar_value *s;
    if (!mpr_ok(r)) {
        return -1;
    }
    copy = (uint8_t *)mpa_alloc(a, n ? (size_t)n : 1u);
    if (copy == 0) {
        mpr_fail(r);
        return -1;
    }
    memcpy(copy, p, n);
    s = bridge_scalar(r, a, out, UNI_SCALAR_VALUE_BLOB);
    if (s == 0) {
        return -1;
    }
    s->as.blob = mp_bin_from(copy, n);
    return 0;
}

static int bridge_read_array(mp_reader *r, mp_arena *a, uni_data_value *out) {
    uint32_t n = mpr_array_header(r);
    uni_data_value *items;
    uint32_t i;
    if (!mpr_ok(r)) {
        return -1;
    }
    items = (uni_data_value *)mpa_alloc(a, sizeof(uni_data_value) * (size_t)(n ? n : 1u));
    if (items == 0) {
        mpr_fail(r);
        return -1;
    }
    for (i = 0; i < n && mpr_ok(r); i++) {
        if (bridge_read_value(r, a, &items[i]) != 0) {
            return -1;
        }
    }
    if (!mpr_ok(r)) {
        return -1;
    }
    out->kind = UNI_DATA_VALUE_ARRAY;
    (out->as.array).items = items;
    (out->as.array).len = n;
    return 0;
}

static int bridge_read_record(mp_reader *r, mp_arena *a, uni_data_value *out) {
    uint32_t count = mpr_map_header(r);
    bridge_pair *pairs;
    uni_data_value_field *fields;
    uni_scalar_value *gap_null = 0;
    uint64_t max_key = 0;
    uint32_t n_pairs = 0;
    uint32_t i;
    if (!mpr_ok(r)) {
        return -1;
    }
    if (count > MP_RECORD_BRIDGE_MAX_FIELDS) {
        mpr_fail(r);
        return -1;
    }
    pairs = (bridge_pair *)mpa_alloc(a, sizeof(bridge_pair) * (size_t)(count ? count : 1u));
    if (pairs == 0) {
        mpr_fail(r);
        return -1;
    }
    for (i = 0; i < count && mpr_ok(r); i++) {
        if (mpr_next_is_integer(r)) {
            uint64_t key = mpr_u64(r);
            if (key == 0) {
                /* 0 is not a valid field number: skip the pair (lenient) */
                mpr_skip(r);
                continue;
            }
            if (key > (uint64_t)MP_RECORD_BRIDGE_MAX_FIELDS) {
                /* the gap-filled field array is sized by the largest key */
                mpr_fail(r);
                return -1;
            }
            if (bridge_read_value(r, a, &pairs[n_pairs].value) != 0) {
                return -1;
            }
            pairs[n_pairs].key = key;
            n_pairs++;
            if (key > max_key) {
                max_key = key;
            }
        } else {
            /* non-integer key: skip key and value (lenient, mirroring the
             * generated record decoders) */
            mpr_skip(r);
            mpr_skip(r);
        }
    }
    if (!mpr_ok(r)) {
        return -1;
    }
    fields = (uni_data_value_field *)mpa_alloc(
        a, sizeof(uni_data_value_field) * (size_t)(max_key ? max_key : 1u));
    if (fields == 0) {
        mpr_fail(r);
        return -1;
    }
    for (i = 0; i < (uint32_t)max_key; i++) {
        /* names ride empty like the host's; a skipped middle key (an
         * omitted option field) defaults to the Null scalar */
        (fields[i].field_name).data = "";
        (fields[i].field_name).len = 0;
        if (gap_null == 0) {
            gap_null = (uni_scalar_value *)mpa_alloc(a, sizeof(uni_scalar_value));
            if (gap_null == 0) {
                mpr_fail(r);
                return -1;
            }
            memset(gap_null, 0, sizeof(*gap_null));
            gap_null->kind = UNI_SCALAR_VALUE_NULL;
        }
        fields[i].field_value.kind = UNI_DATA_VALUE_SCALAR;
        (fields[i].field_value).as.scalar = gap_null;
    }
    for (i = 0; i < n_pairs; i++) {
        /* shallow struct copy: the payload pointers are arena-owned */
        fields[pairs[i].key - 1u].field_value = pairs[i].value;
    }
    out->kind = UNI_DATA_VALUE_RECORD;
    (out->as.record).items = fields;
    (out->as.record).len = (uint32_t)max_key;
    return 0;
}

static int bridge_read_value(mp_reader *r, mp_arena *a, uni_data_value *out) {
    int code = mpr_peek_code(r);
    if (code < 0) {
        mpr_fail(r);
        return -1;
    }
    memset(out, 0, sizeof(*out));
    if (code == 0xc0) { /* nil -> the Null scalar */
        mpr_try_nil(r);
        return bridge_scalar(r, a, out, UNI_SCALAR_VALUE_NULL) != 0 && mpr_ok(r) ? 0 : -1;
    }
    if (code == 0xc2 || code == 0xc3) {
        /* the host vocabulary has no Bool case: booleans cross as i32 0/1 */
        int b = mpr_bool(r);
        uni_scalar_value *s;
        if (!mpr_ok(r)) {
            return -1;
        }
        s = bridge_scalar(r, a, out, UNI_SCALAR_VALUE_I32);
        if (s == 0) {
            return -1;
        }
        s->as.i32 = b ? 1 : 0;
        return 0;
    }
    if (bridge_is_integer_code(code)) {
        return bridge_read_scalar_int(r, a, out);
    }
    if (code == 0xca) {
        float v = mpr_f32(r);
        uni_scalar_value *s;
        if (!mpr_ok(r)) {
            return -1;
        }
        s = bridge_scalar(r, a, out, UNI_SCALAR_VALUE_F32);
        if (s == 0) {
            return -1;
        }
        s->as.f32 = v;
        return 0;
    }
    if (code == 0xcb) {
        double v = mpr_f64(r);
        uni_scalar_value *s;
        if (!mpr_ok(r)) {
            return -1;
        }
        s = bridge_scalar(r, a, out, UNI_SCALAR_VALUE_F64);
        if (s == 0) {
            return -1;
        }
        s->as.f64 = v;
        return 0;
    }
    if ((code & 0xe0) == 0xa0 || (code >= 0xd9 && code <= 0xdb)) {
        return bridge_read_scalar_str(r, a, out);
    }
    if (code >= 0xc4 && code <= 0xc6) {
        return bridge_read_scalar_bin(r, a, out);
    }
    if ((code & 0xf0) == 0x90 || code == 0xdc || code == 0xdd) {
        return bridge_read_array(r, a, out);
    }
    if ((code & 0xf0) == 0x80 || code == 0xde || code == 0xdf) {
        return bridge_read_record(r, a, out);
    }
    mpr_fail(r);
    return -1;
}

int mp_record_bridge_read(mp_reader *r, mp_arena *a, uni_data_value *out) {
    int code = mpr_peek_code(r);
    if (code < 0 || !((code & 0xf0) == 0x80 || code == 0xde || code == 0xdf)) {
        /* the generated record encoders always emit a map */
        mpr_fail(r);
        return -1;
    }
    memset(out, 0, sizeof(*out));
    return bridge_read_record(r, a, out);
}
