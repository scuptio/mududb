/*
 * Golden-corpus conformance driver for the mududb C binding: drives the
 * cross-language fixtures in crates/db-kernel/testing/fixtures/golden/v1/
 * through the mgen-generated codecs in mududb/types/ and the hand-written
 * runtime in mududb/codec/. Three suites, mirroring the Python runners in
 * crates/sdk/bindings/python/ (run_mp_corpus_test.py,
 * run_syscall_corpus_test.py, run_lenient_decode_test.py):
 *
 *   - mp_primitives_v1: canonical encoder is byte-exact against rmp_serde
 *     golden segments; decoder reads each segment back to the same value.
 *   - syscall_payload_v1_all (47 frames): request/response encodes are
 *     byte-exact against the golden frames; decodes render to the sidecar
 *     `expect` shape and compare equal; ok/err responses re-encode
 *     byte-identically after decode (roundtrip).
 *   - lenient_decode_v1 (8 vectors): non-canonical frames (integer width
 *     widening, reordered/unknown/non-integer map keys, missing
 *     fields/params) decode to the sidecar `expect` shape.
 *   - record bridge (self-contained, no fixture): the
 *     `mududb/codec/record_bridge.{h,c}` envelope <-> integer-keyed-map
 *     conversions, plus the bool-as-i32 `mpr_bool` leniency they rely on.
 *
 * The sidecar JSON is the single source of truth for semantic equality;
 * decoded values are rendered into the same canonical JSON form (object
 * keys sorted, u64/i64 as decimal strings, bytes as lowercase hex, UniOid
 * as {"h","l"}, unit as {"unit": true}) and string-compared against the
 * re-serialized sidecar `expect` object.
 *
 * Native program (libc/POSIX); not part of the binding's public surface.
 * Exit code 0 when every check passes.
 */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

#include "mini_json.h"
#include "mududb/codec/mpack.h"
#include "mududb/codec/record_bridge.h"
#include "mududb/types/UniCommandArgv.h"
#include "mududb/types/UniCommandResult.h"
#include "mududb/types/UniDataType.h"
#include "mududb/types/UniError.h"
#include "mududb/types/UniFsDirent.h"
#include "mududb/types/UniFsOpenArgv.h"
#include "mududb/types/UniFsStat.h"
#include "mududb/types/UniOid.h"
#include "mududb/types/UniQueryArgv.h"
#include "mududb/types/UniQueryResult.h"
#include "mududb/types/UniScalar.h"
#include "mududb/types/UniSqlParam.h"
#include "mududb/types/UniSqlStmt.h"
#include "mududb/types/UniSyscall.h"

/* ---- checks ---- */

static long g_checks;
static int g_failures;

static void check_true(const char *label, const char *what, int cond) {
    g_checks++;
    if (!cond) {
        g_failures++;
        fprintf(stderr, "FAIL %s: %s\n", label, what);
    }
}

static void check_bytes(const char *label, const uint8_t *actual, size_t actual_len,
                        const uint8_t *expected, size_t expected_len) {
    size_t n = actual_len < expected_len ? actual_len : expected_len;
    size_t i;
    g_checks++;
    for (i = 0; i < n; i++) {
        if (actual[i] != expected[i]) {
            g_failures++;
            fprintf(stderr,
                    "FAIL %s: not byte-exact: len %lu vs %lu, first diff at %lu "
                    "(0x%02x vs 0x%02x)\n",
                    label, (unsigned long)actual_len, (unsigned long)expected_len,
                    (unsigned long)i, actual[i], expected[i]);
            return;
        }
    }
    if (actual_len != expected_len) {
        g_failures++;
        fprintf(stderr, "FAIL %s: not byte-exact: len %lu vs %lu, prefix equal\n", label,
                (unsigned long)actual_len, (unsigned long)expected_len);
    }
}

static void check_text(const char *label, const char *actual, const char *expected) {
    g_checks++;
    if (strcmp(actual, expected) != 0) {
        g_failures++;
        fprintf(stderr, "FAIL %s:\n  actual   %s\n  expected %s\n", label, actual, expected);
    }
}

/* ---- fixture location (walk up from the cwd, the corpus_common.py
 * pattern; the Makefile runs the driver from the package directory) ---- */

static char g_fixture_dir[4096];

static int find_fixture_dir(void) {
    char dir[4096];
    if (!getcwd(dir, sizeof(dir))) {
        return -1;
    }
    for (;;) {
        struct stat st;
        int n = snprintf(g_fixture_dir, sizeof(g_fixture_dir),
                         "%s/crates/db-kernel/testing/fixtures/golden/v1", dir);
        if (n > 0 && (size_t)n < sizeof(g_fixture_dir) && stat(g_fixture_dir, &st) == 0
            && S_ISDIR(st.st_mode)) {
            return 0;
        }
        {
            char *slash = strrchr(dir, '/');
            if (!slash || slash == dir) {
                return -1;
            }
            *slash = '\0';
        }
    }
}

static uint8_t *read_file(const char *path, size_t *out_len) {
    FILE *f = fopen(path, "rb");
    uint8_t *data;
    long size;
    if (!f) {
        return 0;
    }
    if (fseek(f, 0, SEEK_END) != 0 || (size = ftell(f)) < 0 || fseek(f, 0, SEEK_SET) != 0) {
        fclose(f);
        return 0;
    }
    data = (uint8_t *)malloc((size_t)size + 1);
    if (!data) {
        fclose(f);
        return 0;
    }
    if (size > 0 && fread(data, 1, (size_t)size, f) != (size_t)size) {
        free(data);
        fclose(f);
        return 0;
    }
    fclose(f);
    data[size] = 0;
    *out_len = (size_t)size;
    return data;
}

typedef struct {
    const uint8_t *data;
    size_t len;
} segment;

/* Split the length-prefixed container (big-endian u32 length per segment). */
static segment *unpack_segments(const uint8_t *data, size_t len, size_t *out_count) {
    size_t offset = 0;
    size_t count = 0;
    segment *segs;
    while (offset < len) {
        uint32_t seg_len;
        if (offset + 4 > len) {
            return 0;
        }
        seg_len = ((uint32_t)data[offset] << 24) | ((uint32_t)data[offset + 1] << 16)
            | ((uint32_t)data[offset + 2] << 8) | (uint32_t)data[offset + 3];
        offset += 4 + seg_len;
        if (offset > len) {
            return 0;
        }
        count++;
    }
    segs = (segment *)malloc((count ? count : 1) * sizeof(segment));
    if (!segs) {
        return 0;
    }
    offset = 0;
    count = 0;
    while (offset < len) {
        uint32_t seg_len = ((uint32_t)data[offset] << 24) | ((uint32_t)data[offset + 1] << 16)
            | ((uint32_t)data[offset + 2] << 8) | (uint32_t)data[offset + 3];
        offset += 4;
        segs[count].data = data + offset;
        segs[count].len = seg_len;
        offset += seg_len;
        count++;
    }
    *out_count = count;
    return segs;
}

typedef struct {
    segment *segments;
    size_t count;
    mj_value *sidecar;
    uint8_t *bin_data;
    uint8_t *json_data;
} fixture;

static int load_fixture(const char *stem, fixture *out) {
    char path[4400];
    size_t bin_len = 0;
    size_t json_len = 0;
    memset(out, 0, sizeof(*out));
    snprintf(path, sizeof(path), "%s/%s.bin", g_fixture_dir, stem);
    out->bin_data = read_file(path, &bin_len);
    snprintf(path, sizeof(path), "%s/%s.json", g_fixture_dir, stem);
    out->json_data = read_file(path, &json_len);
    if (!out->bin_data || !out->json_data) {
        return -1;
    }
    out->segments = unpack_segments(out->bin_data, bin_len, &out->count);
    out->sidecar = mj_parse((const char *)out->json_data, json_len);
    if (!out->segments || !out->sidecar) {
        return -1;
    }
    return 0;
}

static void free_fixture(fixture *f) {
    free(f->segments);
    mj_free(f->sidecar);
    free(f->bin_data);
    free(f->json_data);
}

/* ---- small value builders (encode side borrows) ---- */

static uni_oid mk_oid(uint64_t l) {
    uni_oid o;
    o.h = 0;
    o.l = l;
    return o;
}

static mp_str mk_str(const char *s) {
    return mp_str_from_cstr(s);
}

static mp_bin mk_bin(const uint8_t *data, uint32_t len) {
    return mp_bin_from(data, len);
}

static uni_sql_stmt mk_sql(const char *sql) {
    uni_sql_stmt v;
    memset(&v, 0, sizeof(v));
    v.sql_string = mk_str(sql);
    return v;
}

static uni_sql_param mk_empty_params(void) {
    uni_sql_param v;
    memset(&v, 0, sizeof(v));
    v.param_names_is_null = 1;
    return v;
}

static uni_query_argv mk_query_argv(uint64_t l, const char *sql) {
    uni_query_argv v;
    memset(&v, 0, sizeof(v));
    v.oid = mk_oid(l);
    v.query = mk_sql(sql);
    v.param_list = mk_empty_params();
    v.param_desc_is_null = 1;
    return v;
}

static uni_command_argv mk_command_argv(uint64_t l, const char *sql) {
    uni_command_argv v;
    memset(&v, 0, sizeof(v));
    v.oid = mk_oid(l);
    v.command = mk_sql(sql);
    v.param_list = mk_empty_params();
    v.param_desc_is_null = 1;
    return v;
}

static uni_fs_stat mk_golden_fs_stat(void) {
    uni_fs_stat v;
    memset(&v, 0, sizeof(v));
    v.oid = mk_oid(5);
    v.generation = 1;
    v.entry = mk_str("");
    v.length = 100;
    v.state = 1;
    return v;
}

static uni_error mk_golden_err(void) {
    /* The trailing corpus frame: get result Err(NotFound, "no such entry").
     * err_src is the host's JSON form of ErrorSource::None; err_details
     * encodes as a MessagePack ARRAY, not bin — both quirks pinned on
     * purpose. */
    uni_error v;
    memset(&v, 0, sizeof(v));
    v.err_code = 2;
    v.err_msg = mk_str("no such entry");
    v.err_src = mk_str("\"None\"");
    v.err_loc = mk_str("");
    v.err_details = mk_bin(0, 0);
    return v;
}

/* ---- mp_primitives_v1 ---- */

static int value_u64(const mj_value *vector, uint64_t *out) {
    const mj_value *value = mj_get(vector, "value");
    if (!value) {
        return -1;
    }
    *out = strtoull(mj_text(value), 0, 10);
    return 0;
}

static int value_i64(const mj_value *vector, int64_t *out) {
    const mj_value *value = mj_get(vector, "value");
    if (!value) {
        return -1;
    }
    *out = strtoll(mj_text(value), 0, 10);
    return 0;
}

static int value_f64(const mj_value *vector, double *out) {
    const mj_value *value = mj_get(vector, "value");
    if (!value) {
        return -1;
    }
    *out = strtod(mj_text(value), 0);
    return 0;
}

static int value_len(const mj_value *vector, uint32_t *out) {
    const mj_value *len = mj_get(vector, "len");
    if (!len) {
        return -1;
    }
    *out = (uint32_t)strtoul(mj_text(len), 0, 10);
    return 0;
}

/* Sidecar `bin_fill` rule: byte i = i mod 251. */
static void pattern_bin(uint8_t *out, uint32_t n) {
    uint32_t i;
    for (i = 0; i < n; i++) {
        out[i] = (uint8_t)(i % 251);
    }
}

static void run_mp_primitives(void) {
    fixture f;
    const mj_value *vectors;
    size_t i;
    char label[128];
    if (load_fixture("mp_primitives_v1", &f) != 0) {
        check_true("mp_primitives_v1", "fixture load", 0);
        return;
    }
    vectors = mj_get(f.sidecar, "vectors");
    check_true("mp_primitives_v1", "segment/vector count",
               mj_is(vectors, MJ_ARRAY) && mj_len(vectors) == f.count);
    for (i = 0; i < f.count; i++) {
        const mj_value *vector = mj_at(vectors, i);
        const char *kind = mj_str(mj_get(vector, "kind"));
        segment seg = f.segments[i];
        mp_writer w;
        mp_reader r;
        snprintf(label, sizeof(label), "mp#%lu %s", (unsigned long)i, kind);
        mpw_init(&w);
        mpr_init(&r, seg.data, seg.len);

        if (strcmp(kind, "u64") == 0) {
            uint64_t v = 0;
            value_u64(vector, &v);
            mpw_u64(&w, v);
            check_bytes(label, w.buf, w.len, seg.data, seg.len);
            check_true(label, "u64 decode", mpr_u64(&r) == v && mpr_done(&r));
        } else if (strcmp(kind, "i64") == 0) {
            int64_t v = 0;
            value_i64(vector, &v);
            mpw_i64(&w, v);
            check_bytes(label, w.buf, w.len, seg.data, seg.len);
            check_true(label, "i64 decode", mpr_i64(&r) == v && mpr_done(&r));
        } else if (strcmp(kind, "f32") == 0) {
            double dv = 0;
            float v;
            float back;
            value_f64(vector, &dv);
            /* truncate the double to single precision */
            v = (float)dv;
            mpw_f32(&w, v);
            check_bytes(label, w.buf, w.len, seg.data, seg.len);
            back = mpr_f32(&r);
            check_true(label, "f32 decode", memcmp(&back, &v, sizeof(v)) == 0 && mpr_done(&r));
        } else if (strcmp(kind, "f64") == 0) {
            double v = 0;
            double back;
            value_f64(vector, &v);
            mpw_f64(&w, v);
            check_bytes(label, w.buf, w.len, seg.data, seg.len);
            back = mpr_f64(&r);
            check_true(label, "f64 decode", memcmp(&back, &v, sizeof(v)) == 0 && mpr_done(&r));
        } else if (strcmp(kind, "nil") == 0) {
            mpw_nil(&w);
            check_bytes(label, w.buf, w.len, seg.data, seg.len);
            check_true(label, "nil decode", mpr_try_nil(&r) && mpr_done(&r));
        } else if (strcmp(kind, "bool") == 0) {
            int v = mj_bool(mj_get(vector, "value"));
            mpw_bool(&w, v);
            check_bytes(label, w.buf, w.len, seg.data, seg.len);
            check_true(label, "bool decode", mpr_bool(&r) == v && mpr_done(&r));
        } else if (strcmp(kind, "str") == 0) {
            const mj_value *value = mj_get(vector, "value");
            mp_str s;
            char *fill = 0;
            if (value) {
                s.data = mj_str(value);
                s.len = (uint32_t)mj_str_len(value);
            } else {
                /* str_fill: that many ASCII 'a' bytes */
                uint32_t n = 0;
                uint32_t k;
                value_len(vector, &n);
                fill = (char *)malloc(n ? n : 1);
                for (k = 0; k < n; k++) {
                    fill[k] = 'a';
                }
                s.data = fill;
                s.len = n;
            }
            mpw_str(&w, s.data, s.len);
            check_bytes(label, w.buf, w.len, seg.data, seg.len);
            {
                uint32_t back_len = 0;
                const char *back = mpr_str(&r, &back_len);
                check_true(label, "str decode",
                           back_len == s.len && memcmp(back, s.data, s.len) == 0
                               && mpr_done(&r));
            }
            free(fill);
        } else if (strcmp(kind, "bin") == 0) {
            uint32_t n = 0;
            uint8_t *fill;
            value_len(vector, &n);
            fill = (uint8_t *)malloc(n ? n : 1);
            pattern_bin(fill, n);
            mpw_bin(&w, fill, n);
            check_bytes(label, w.buf, w.len, seg.data, seg.len);
            {
                uint32_t back_len = 0;
                const uint8_t *back = mpr_bin(&r, &back_len);
                check_true(label, "bin decode",
                           back_len == n && memcmp(back, fill, n) == 0 && mpr_done(&r));
            }
            free(fill);
        } else if (strcmp(kind, "array") == 0) {
            uint32_t n = 0;
            uint32_t k;
            value_len(vector, &n);
            mpw_array_header(&w, n);
            for (k = 0; k < n; k++) {
                mpw_u64(&w, k);
            }
            check_bytes(label, w.buf, w.len, seg.data, seg.len);
            {
                uint32_t back_n = mpr_array_header(&r);
                int ok = back_n == n;
                for (k = 0; k < back_n && ok; k++) {
                    ok = mpr_u64(&r) == k;
                }
                check_true(label, "array decode", ok && mpr_done(&r));
            }
        } else if (strcmp(kind, "combo") == 0) {
            /* the 3-array [u64 42, str "hi", bin 0xDEADBEEF] */
            static const uint8_t dead[] = {0xde, 0xad, 0xbe, 0xef};
            mpw_array_header(&w, 3);
            mpw_u64(&w, 42);
            mpw_str(&w, "hi", 2);
            mpw_bin(&w, dead, 4);
            check_bytes(label, w.buf, w.len, seg.data, seg.len);
            {
                int ok = mpr_array_header(&r) == 3 && mpr_u64(&r) == 42;
                uint32_t str_len = 0;
                uint32_t bin_len = 0;
                const char *back_str = mpr_str(&r, &str_len);
                const uint8_t *back_bin;
                ok = ok && str_len == 2 && memcmp(back_str, "hi", 2) == 0;
                back_bin = mpr_bin(&r, &bin_len);
                ok = ok && bin_len == 4 && memcmp(back_bin, dead, 4) == 0;
                check_true(label, "combo decode", ok && mpr_done(&r));
            }
        } else {
            check_true(label, "unknown vector kind", 0);
        }
        mpw_free(&w);
    }
    free_fixture(&f);
}

/* ---- canonical expect-shape renderers (sidecar conventions: u64/i64 as
 * decimal strings, bytes as lowercase hex, UniOid as {"h","l"} decimal
 * strings, unit as {"unit": true}, relation cells as hex-or-null; object
 * keys emitted in sorted order) ---- */

static void put_u64_str(mj_buf *b, uint64_t v) {
    char tmp[24];
    int n = snprintf(tmp, sizeof(tmp), "%llu", (unsigned long long)v);
    mj_buf_ch(b, '"');
    mj_buf_put(b, tmp, (size_t)n);
    mj_buf_ch(b, '"');
}

static void put_i64_str(mj_buf *b, int64_t v) {
    char tmp[24];
    int n = snprintf(tmp, sizeof(tmp), "%lld", (long long)v);
    mj_buf_ch(b, '"');
    mj_buf_put(b, tmp, (size_t)n);
    mj_buf_ch(b, '"');
}

static void put_u32_num(mj_buf *b, uint32_t v) {
    char tmp[16];
    int n = snprintf(tmp, sizeof(tmp), "%u", v);
    mj_buf_put(b, tmp, (size_t)n);
}

static void put_hex(mj_buf *b, const uint8_t *data, uint32_t len) {
    static const char hexd[] = "0123456789abcdef";
    uint32_t i;
    mj_buf_ch(b, '"');
    for (i = 0; i < len; i++) {
        mj_buf_ch(b, hexd[data[i] >> 4]);
        mj_buf_ch(b, hexd[data[i] & 0x0f]);
    }
    mj_buf_ch(b, '"');
}

static void put_json_str(mj_buf *b, mp_str s) {
    mj_buf_ch(b, '"');
    mj_buf_put_escaped(b, s.data, s.len);
    mj_buf_ch(b, '"');
}

static void put_oid(mj_buf *b, const uni_oid *o) {
    mj_buf_puts(b, "{\"h\":");
    put_u64_str(b, o->h);
    mj_buf_puts(b, ",\"l\":");
    put_u64_str(b, o->l);
    mj_buf_ch(b, '}');
}

static void put_unit(mj_buf *b) {
    mj_buf_puts(b, "{\"unit\":true}");
}

static void put_err(mj_buf *b, const uni_error *err) {
    mj_buf_puts(b, "{\"err_code\":");
    put_u32_num(b, err->err_code);
    mj_buf_puts(b, ",\"err_details_hex\":");
    put_hex(b, err->err_details.data, err->err_details.len);
    mj_buf_puts(b, ",\"err_loc\":");
    put_json_str(b, err->err_loc);
    mj_buf_puts(b, ",\"err_msg\":");
    put_json_str(b, err->err_msg);
    mj_buf_puts(b, ",\"err_src\":");
    put_json_str(b, err->err_src);
    mj_buf_ch(b, '}');
}

/* {"attr":"<u64>","datum_hex":"<hex>"} */
static void put_attr_datum(mj_buf *b, uint64_t attr, mp_bin datum) {
    mj_buf_puts(b, "{\"attr\":");
    put_u64_str(b, attr);
    mj_buf_puts(b, ",\"datum_hex\":");
    put_hex(b, datum.data, datum.len);
    mj_buf_ch(b, '}');
}

static void put_fs_stat(mj_buf *b, const uni_fs_stat *s) {
    mj_buf_puts(b, "{\"entry\":");
    put_json_str(b, s->entry);
    mj_buf_puts(b, ",\"generation\":");
    put_u64_str(b, s->generation);
    mj_buf_puts(b, ",\"length\":");
    put_u64_str(b, s->length);
    mj_buf_puts(b, ",\"oid\":");
    put_oid(b, &s->oid);
    mj_buf_puts(b, ",\"state\":");
    put_u32_num(b, s->state);
    mj_buf_ch(b, '}');
}

static const char *scalar_name(uni_scalar s) {
    static const char *names[] = {
        "bool", "u8", "i8", "u16", "i16", "u32", "i32", "u64", "u128", "i64", "i128",
        "f32", "f64", "char", "string", "blob", "numeric", "date", "time", "timestamp",
        "timestamp_tz",
    };
    if ((unsigned)s < sizeof(names) / sizeof(names[0])) {
        return names[(unsigned)s];
    }
    return "?";
}

/* `scalar(<name>)`; the sidecar conventions only cover scalar field types
 * (a UniDataType left at its proto3-style default renders as scalar(bool)). */
static int put_data_type_name(mj_buf *b, const uni_data_type *t) {
    if (t->kind != UNI_DATA_TYPE_SCALAR) {
        return -1;
    }
    mj_buf_puts(b, "\"scalar(");
    mj_buf_puts(b, scalar_name(t->as.scalar));
    mj_buf_puts(b, ")\"");
    return 0;
}

static int put_record_field(mj_buf *b, const uni_record_field *f) {
    uint32_t i;
    mj_buf_puts(b, "{\"field_attrs\":[");
    for (i = 0; i < f->field_attrs.len; i++) {
        if (i) {
            mj_buf_ch(b, ',');
        }
        mj_buf_puts(b, "{\"attr_name\":");
        put_json_str(b, f->field_attrs.items[i].attr_name);
        mj_buf_puts(b, ",\"attr_value\":");
        put_json_str(b, f->field_attrs.items[i].attr_value);
        mj_buf_ch(b, '}');
    }
    mj_buf_puts(b, "],\"field_name\":");
    put_json_str(b, f->field_name);
    mj_buf_puts(b, ",\"field_type\":");
    if (put_data_type_name(b, &f->field_type) != 0) {
        return -1;
    }
    mj_buf_ch(b, '}');
    return 0;
}

static int put_query_result(mj_buf *b, const uni_query_result *r) {
    uint32_t i;
    mj_buf_puts(b, "{\"cursor_hex\":");
    put_hex(b, r->result_set.cursor.data, r->result_set.cursor.len);
    mj_buf_puts(b, ",\"eof\":");
    mj_buf_puts(b, r->result_set.eof ? "true" : "false");
    mj_buf_puts(b, ",\"fields\":[");
    for (i = 0; i < r->tuple_desc.record_fields.len; i++) {
        if (i) {
            mj_buf_ch(b, ',');
        }
        if (put_record_field(b, &r->tuple_desc.record_fields.items[i]) != 0) {
            return -1;
        }
    }
    mj_buf_puts(b, "],\"record_name\":");
    put_json_str(b, r->tuple_desc.record_name);
    mj_buf_puts(b, ",\"rows\":[]}");
    return 0;
}

/* ---- deterministic encode inputs pinned byte-exactly against the fixture
 * frames (mirrors corpus_common.py's encoders) ---- */

static int request_encode(int kind, mp_writer *w) {
    static const uint8_t k1[] = {'k', '1'};
    static const uint8_t v1[] = {'v', '1'};
    static const uint8_t hi[] = {'h', 'i'};
    static const uint8_t a[] = {'a'};
    static const uint8_t z[] = {'z'};
    static const uint8_t b01[] = {0x01};
    static const uint8_t b0203[] = {0x02, 0x03};
    static const uint8_t b0a[] = {0x0a};
    static const uint8_t b05[] = {0x05};
    switch (kind) {
    case UNI_SYSCALL_QUERY: {
        uni_query_argv v = mk_query_argv(1, "select 1");
        return uni_syscall_query_request_encode(w, &v);
    }
    case UNI_SYSCALL_COMMAND: {
        uni_command_argv v = mk_command_argv(2, "update t set a = 1");
        return uni_syscall_command_request_encode(w, &v);
    }
    case UNI_SYSCALL_BATCH: {
        uni_command_argv v = mk_command_argv(3, "insert into t values (1)");
        return uni_syscall_batch_request_encode(w, &v);
    }
    case UNI_SYSCALL_OPEN_SESSION: {
        uni_oid v = mk_oid(4);
        return uni_syscall_open_session_request_encode(w, &v);
    }
    case UNI_SYSCALL_CLOSE_SESSION: {
        uni_oid v = mk_oid(5);
        return uni_syscall_close_session_request_encode(w, &v);
    }
    case UNI_SYSCALL_GET: {
        uni_oid v = mk_oid(6);
        return uni_syscall_get_request_encode(w, &v, mk_bin(k1, 2));
    }
    case UNI_SYSCALL_PUT: {
        uni_oid v = mk_oid(7);
        return uni_syscall_put_request_encode(w, &v, mk_bin(k1, 2), mk_bin(v1, 2));
    }
    case UNI_SYSCALL_DELETE: {
        uni_oid v = mk_oid(8);
        return uni_syscall_delete_request_encode(w, &v, mk_bin(k1, 2));
    }
    case UNI_SYSCALL_RANGE: {
        uni_oid v = mk_oid(9);
        return uni_syscall_range_request_encode(w, &v, mk_bin(a, 1), mk_bin(z, 1));
    }
    case UNI_SYSCALL_FS_OPEN: {
        uni_fs_open_argv v;
        memset(&v, 0, sizeof(v));
        v.session = mk_oid(10);
        v.oid = mk_oid(11);
        v.path = mk_str("docs/a.txt");
        v.flags = 2;
        return uni_syscall_fs_open_request_encode(w, &v);
    }
    case UNI_SYSCALL_FS_CLOSE:
        return uni_syscall_fs_close_request_encode(w, 3);
    case UNI_SYSCALL_FS_READ:
        return uni_syscall_fs_read_request_encode(w, 3, 4);
    case UNI_SYSCALL_FS_WRITE:
        return uni_syscall_fs_write_request_encode(w, 3, mk_bin(hi, 2));
    case UNI_SYSCALL_FS_PREAD:
        return uni_syscall_fs_pread_request_encode(w, 3, 8, 4);
    case UNI_SYSCALL_FS_PWRITE:
        return uni_syscall_fs_pwrite_request_encode(w, 3, 8, mk_bin(hi, 2));
    case UNI_SYSCALL_FS_LSEEK:
        return uni_syscall_fs_lseek_request_encode(w, 3, -2, 1);
    case UNI_SYSCALL_FS_FSTAT:
        return uni_syscall_fs_fstat_request_encode(w, 3);
    case UNI_SYSCALL_FS_STAT: {
        uni_oid v = mk_oid(18);
        return uni_syscall_fs_stat_request_encode(w, &v, mk_str("a"));
    }
    case UNI_SYSCALL_FS_FSYNC:
        return uni_syscall_fs_fsync_request_encode(w, 3);
    case UNI_SYSCALL_FS_READDIR: {
        uni_oid v = mk_oid(20);
        return uni_syscall_fs_readdir_request_encode(w, &v, mk_str("d"));
    }
    case UNI_SYSCALL_RELATION_GET: {
        uni_oid v = mk_oid(21);
        uni_syscall_relation_get_key_tuple kt[2];
        uint64_t sel[2] = {3, 4};
        uni_syscall_relation_get_key_list key;
        uni_syscall_relation_get_select_list select;
        kt[0].f0 = 1;
        kt[0].f1 = mk_bin(b01, 1);
        kt[1].f0 = 2;
        kt[1].f1 = mk_bin(b0203, 2);
        key.items = kt;
        key.len = 2;
        select.items = sel;
        select.len = 2;
        return uni_syscall_relation_get_request_encode(w, &v, mk_str("t"), key, select);
    }
    case UNI_SYSCALL_RELATION_UPDATE: {
        uni_oid v = mk_oid(22);
        uni_syscall_relation_update_key_tuple kt[1];
        uni_syscall_relation_update_values_tuple vt[1];
        uni_syscall_relation_update_deltas_tuple dt[1];
        uni_syscall_relation_update_key_list key;
        uni_syscall_relation_update_values_list values;
        uni_syscall_relation_update_deltas_list deltas;
        kt[0].f0 = 1;
        kt[0].f1 = mk_bin(b01, 1);
        key.items = kt;
        key.len = 1;
        vt[0].f0 = 2;
        vt[0].f1 = mk_bin(b0a, 1);
        values.items = vt;
        values.len = 1;
        dt[0].f0 = 3;
        dt[0].f1 = 0;
        dt[0].f2 = mk_bin(b05, 1);
        deltas.items = dt;
        deltas.len = 1;
        return uni_syscall_relation_update_request_encode(w, &v, mk_str("t"), key, values,
                                                          deltas);
    }
    case UNI_SYSCALL_RELATION_INSERT: {
        uni_oid v = mk_oid(23);
        uni_syscall_relation_insert_key_tuple kt[1];
        uni_syscall_relation_insert_values_tuple vt[1];
        uni_syscall_relation_insert_key_list key;
        uni_syscall_relation_insert_values_list values;
        kt[0].f0 = 1;
        kt[0].f1 = mk_bin(b01, 1);
        key.items = kt;
        key.len = 1;
        vt[0].f0 = 2;
        vt[0].f1 = mk_bin(b0a, 1);
        values.items = vt;
        values.len = 1;
        return uni_syscall_relation_insert_request_encode(w, &v, mk_str("t"), key, values);
    }
    default:
        return -1;
    }
}

static int response_encode_ok(int kind, mp_writer *w) {
    static const uint8_t v1[] = {'v', '1'};
    static const uint8_t hi[] = {'h', 'i'};
    static const uint8_t ka[] = {'a'};
    static const uint8_t kb[] = {'b'};
    static const uint8_t d1[] = {'1'};
    static const uint8_t d2[] = {'2'};
    static const uint8_t b0a[] = {0x0a};
    switch (kind) {
    case UNI_SYSCALL_QUERY: {
        uni_syscall_query_result res;
        memset(&res, 0, sizeof(res));
        return uni_syscall_query_result_encode(w, &res);
    }
    case UNI_SYSCALL_COMMAND: {
        uni_syscall_command_result res;
        memset(&res, 0, sizeof(res));
        res.value.affected_rows = 3;
        return uni_syscall_command_result_encode(w, &res);
    }
    case UNI_SYSCALL_BATCH: {
        uni_syscall_batch_result res;
        memset(&res, 0, sizeof(res));
        res.value.affected_rows = 2;
        return uni_syscall_batch_result_encode(w, &res);
    }
    case UNI_SYSCALL_OPEN_SESSION: {
        uni_syscall_open_session_result res;
        memset(&res, 0, sizeof(res));
        res.value = mk_oid(4);
        return uni_syscall_open_session_result_encode(w, &res);
    }
    case UNI_SYSCALL_CLOSE_SESSION: {
        uni_syscall_close_session_result res;
        memset(&res, 0, sizeof(res));
        return uni_syscall_close_session_result_encode(w, &res);
    }
    case UNI_SYSCALL_GET: {
        uni_syscall_get_result res;
        memset(&res, 0, sizeof(res));
        res.value.has_value = 1;
        res.value.value = mk_bin(v1, 2);
        return uni_syscall_get_result_encode(w, &res);
    }
    case UNI_SYSCALL_PUT: {
        uni_syscall_put_result res;
        memset(&res, 0, sizeof(res));
        return uni_syscall_put_result_encode(w, &res);
    }
    case UNI_SYSCALL_DELETE: {
        uni_syscall_delete_result res;
        memset(&res, 0, sizeof(res));
        return uni_syscall_delete_result_encode(w, &res);
    }
    case UNI_SYSCALL_RANGE: {
        uni_syscall_range_result res;
        uni_syscall_range_result_tuple items[2];
        memset(&res, 0, sizeof(res));
        items[0].f0 = mk_bin(ka, 1);
        items[0].f1 = mk_bin(d1, 1);
        items[1].f0 = mk_bin(kb, 1);
        items[1].f1 = mk_bin(d2, 1);
        res.value.items = items;
        res.value.len = 2;
        return uni_syscall_range_result_encode(w, &res);
    }
    case UNI_SYSCALL_FS_OPEN: {
        uni_syscall_fs_open_result res;
        memset(&res, 0, sizeof(res));
        res.value = 9;
        return uni_syscall_fs_open_result_encode(w, &res);
    }
    case UNI_SYSCALL_FS_CLOSE: {
        uni_syscall_fs_close_result res;
        memset(&res, 0, sizeof(res));
        return uni_syscall_fs_close_result_encode(w, &res);
    }
    case UNI_SYSCALL_FS_READ: {
        uni_syscall_fs_read_result res;
        memset(&res, 0, sizeof(res));
        res.value = mk_bin(hi, 2);
        return uni_syscall_fs_read_result_encode(w, &res);
    }
    case UNI_SYSCALL_FS_WRITE: {
        uni_syscall_fs_write_result res;
        memset(&res, 0, sizeof(res));
        res.value = 2;
        return uni_syscall_fs_write_result_encode(w, &res);
    }
    case UNI_SYSCALL_FS_PREAD: {
        uni_syscall_fs_pread_result res;
        memset(&res, 0, sizeof(res));
        res.value = mk_bin(hi, 2);
        return uni_syscall_fs_pread_result_encode(w, &res);
    }
    case UNI_SYSCALL_FS_PWRITE: {
        uni_syscall_fs_pwrite_result res;
        memset(&res, 0, sizeof(res));
        return uni_syscall_fs_pwrite_result_encode(w, &res);
    }
    case UNI_SYSCALL_FS_LSEEK: {
        uni_syscall_fs_lseek_result res;
        memset(&res, 0, sizeof(res));
        res.value = 6;
        return uni_syscall_fs_lseek_result_encode(w, &res);
    }
    case UNI_SYSCALL_FS_FSTAT: {
        uni_syscall_fs_fstat_result res;
        memset(&res, 0, sizeof(res));
        res.value = mk_golden_fs_stat();
        return uni_syscall_fs_fstat_result_encode(w, &res);
    }
    case UNI_SYSCALL_FS_STAT: {
        uni_syscall_fs_stat_result res;
        memset(&res, 0, sizeof(res));
        res.value = mk_golden_fs_stat();
        return uni_syscall_fs_stat_result_encode(w, &res);
    }
    case UNI_SYSCALL_FS_FSYNC: {
        uni_syscall_fs_fsync_result res;
        memset(&res, 0, sizeof(res));
        return uni_syscall_fs_fsync_result_encode(w, &res);
    }
    case UNI_SYSCALL_FS_READDIR: {
        uni_syscall_fs_readdir_result res;
        uni_fs_dirent entries[2];
        memset(&res, 0, sizeof(res));
        memset(entries, 0, sizeof(entries));
        entries[0].name = mk_str("a.txt");
        entries[0].is_dir = 0;
        entries[0].length = 3;
        entries[1].name = mk_str("docs");
        entries[1].is_dir = 1;
        entries[1].length = 0;
        res.value.items = entries;
        res.value.len = 2;
        return uni_syscall_fs_readdir_result_encode(w, &res);
    }
    case UNI_SYSCALL_RELATION_GET: {
        uni_syscall_relation_get_result res;
        uni_syscall_relation_get_result_opt2 cells[2];
        memset(&res, 0, sizeof(res));
        cells[0].has_value = 1;
        cells[0].value = mk_bin(b0a, 1);
        cells[1].has_value = 0;
        res.value.has_value = 1;
        res.value.value.items = cells;
        res.value.value.len = 2;
        return uni_syscall_relation_get_result_encode(w, &res);
    }
    case UNI_SYSCALL_RELATION_UPDATE: {
        uni_syscall_relation_update_result res;
        memset(&res, 0, sizeof(res));
        res.value = 1;
        return uni_syscall_relation_update_result_encode(w, &res);
    }
    case UNI_SYSCALL_RELATION_INSERT: {
        uni_syscall_relation_insert_result res;
        memset(&res, 0, sizeof(res));
        return uni_syscall_relation_insert_result_encode(w, &res);
    }
    default:
        return -1;
    }
}

static int err_response_encode(mp_writer *w) {
    uni_syscall_get_result res;
    memset(&res, 0, sizeof(res));
    res.is_err = 1;
    res.err = mk_golden_err();
    return uni_syscall_get_result_encode(w, &res);
}

/* ---- decode + render into the sidecar expect shape ---- */

static int header_kind(segment seg, int *kind) {
    mp_reader r;
    uni_syscall_message_kind k;
    mpr_init(&r, seg.data, seg.len);
    if (uni_syscall_decode_header(&r, &k) != 0) {
        return -1;
    }
    *kind = (int)k;
    return 0;
}

/* The sidecar conventions do not define a rendering for UniDataValue
 * params; every corpus/lenient frame carries an empty parameter list. */
static int put_sql_argv(mj_buf *b, const uni_oid *oid, mp_str sql, const uni_sql_param *params) {
    if (params->params.len != 0) {
        return -1;
    }
    mj_buf_puts(b, "{\"oid\":");
    put_oid(b, oid);
    mj_buf_puts(b, ",\"params\":[],\"sql\":");
    put_json_str(b, sql);
    mj_buf_ch(b, '}');
    return 0;
}

static void put_relation_row(mj_buf *b, const uni_syscall_relation_get_result_opt *row) {
    uint32_t i;
    if (!row->has_value) {
        mj_buf_puts(b, "null");
        return;
    }
    mj_buf_ch(b, '[');
    for (i = 0; i < row->value.len; i++) {
        if (i) {
            mj_buf_ch(b, ',');
        }
        if (row->value.items[i].has_value) {
            put_hex(b, row->value.items[i].value.data, row->value.items[i].value.len);
        } else {
            mj_buf_puts(b, "null");
        }
    }
    mj_buf_ch(b, ']');
}

static void request_expect(int kind, segment seg, mj_buf *out, int *ok) {
    mp_arena a;
    mp_reader r;
    mpa_init(&a);
    mpr_init(&r, seg.data, seg.len);
    *ok = 0;
    switch (kind) {
    case UNI_SYSCALL_QUERY: {
        uni_query_argv v;
        if (uni_syscall_query_request_decode(&r, &a, &v) == 0
            && put_sql_argv(out, &v.oid, v.query.sql_string, &v.param_list) == 0) {
            *ok = 1;
        }
        break;
    }
    case UNI_SYSCALL_COMMAND: {
        uni_command_argv v;
        if (uni_syscall_command_request_decode(&r, &a, &v) == 0
            && put_sql_argv(out, &v.oid, v.command.sql_string, &v.param_list) == 0) {
            *ok = 1;
        }
        break;
    }
    case UNI_SYSCALL_BATCH: {
        uni_command_argv v;
        if (uni_syscall_batch_request_decode(&r, &a, &v) == 0
            && put_sql_argv(out, &v.oid, v.command.sql_string, &v.param_list) == 0) {
            *ok = 1;
        }
        break;
    }
    case UNI_SYSCALL_OPEN_SESSION: {
        uni_oid v;
        if (uni_syscall_open_session_request_decode(&r, &a, &v) == 0) {
            mj_buf_puts(out, "{\"worker_oid\":");
            put_oid(out, &v);
            mj_buf_ch(out, '}');
            *ok = 1;
        }
        break;
    }
    case UNI_SYSCALL_CLOSE_SESSION: {
        uni_oid v;
        if (uni_syscall_close_session_request_decode(&r, &a, &v) == 0) {
            mj_buf_puts(out, "{\"oid\":");
            put_oid(out, &v);
            mj_buf_ch(out, '}');
            *ok = 1;
        }
        break;
    }
    case UNI_SYSCALL_GET: {
        uni_syscall_get_request v;
        if (uni_syscall_get_request_decode(&r, &a, &v) == 0) {
            mj_buf_puts(out, "{\"key_hex\":");
            put_hex(out, v.key.data, v.key.len);
            mj_buf_puts(out, ",\"oid\":");
            put_oid(out, &v.oid);
            mj_buf_ch(out, '}');
            *ok = 1;
        }
        break;
    }
    case UNI_SYSCALL_PUT: {
        uni_syscall_put_request v;
        if (uni_syscall_put_request_decode(&r, &a, &v) == 0) {
            mj_buf_puts(out, "{\"key_hex\":");
            put_hex(out, v.key.data, v.key.len);
            mj_buf_puts(out, ",\"oid\":");
            put_oid(out, &v.oid);
            mj_buf_puts(out, ",\"value_hex\":");
            put_hex(out, v.value.data, v.value.len);
            mj_buf_ch(out, '}');
            *ok = 1;
        }
        break;
    }
    case UNI_SYSCALL_DELETE: {
        uni_syscall_delete_request v;
        if (uni_syscall_delete_request_decode(&r, &a, &v) == 0) {
            mj_buf_puts(out, "{\"key_hex\":");
            put_hex(out, v.key.data, v.key.len);
            mj_buf_puts(out, ",\"oid\":");
            put_oid(out, &v.oid);
            mj_buf_ch(out, '}');
            *ok = 1;
        }
        break;
    }
    case UNI_SYSCALL_RANGE: {
        uni_syscall_range_request v;
        if (uni_syscall_range_request_decode(&r, &a, &v) == 0) {
            mj_buf_puts(out, "{\"end_hex\":");
            put_hex(out, v.end.data, v.end.len);
            mj_buf_puts(out, ",\"oid\":");
            put_oid(out, &v.oid);
            mj_buf_puts(out, ",\"start_hex\":");
            put_hex(out, v.start.data, v.start.len);
            mj_buf_ch(out, '}');
            *ok = 1;
        }
        break;
    }
    case UNI_SYSCALL_FS_OPEN: {
        uni_fs_open_argv v;
        if (uni_syscall_fs_open_request_decode(&r, &a, &v) == 0) {
            mj_buf_puts(out, "{\"flags\":");
            put_u32_num(out, v.flags);
            mj_buf_puts(out, ",\"oid\":");
            put_oid(out, &v.oid);
            mj_buf_puts(out, ",\"path\":");
            put_json_str(out, v.path);
            mj_buf_puts(out, ",\"session\":");
            put_oid(out, &v.session);
            mj_buf_ch(out, '}');
            *ok = 1;
        }
        break;
    }
    case UNI_SYSCALL_FS_CLOSE: {
        uint32_t v;
        if (uni_syscall_fs_close_request_decode(&r, &a, &v) == 0) {
            mj_buf_puts(out, "{\"fd\":");
            put_u32_num(out, v);
            mj_buf_ch(out, '}');
            *ok = 1;
        }
        break;
    }
    case UNI_SYSCALL_FS_READ: {
        uni_syscall_fs_read_request v;
        if (uni_syscall_fs_read_request_decode(&r, &a, &v) == 0) {
            mj_buf_puts(out, "{\"fd\":");
            put_u32_num(out, v.fd);
            mj_buf_puts(out, ",\"len\":");
            put_u32_num(out, v.len);
            mj_buf_ch(out, '}');
            *ok = 1;
        }
        break;
    }
    case UNI_SYSCALL_FS_WRITE: {
        uni_syscall_fs_write_request v;
        if (uni_syscall_fs_write_request_decode(&r, &a, &v) == 0) {
            mj_buf_puts(out, "{\"data_hex\":");
            put_hex(out, v.data.data, v.data.len);
            mj_buf_puts(out, ",\"fd\":");
            put_u32_num(out, v.fd);
            mj_buf_ch(out, '}');
            *ok = 1;
        }
        break;
    }
    case UNI_SYSCALL_FS_PREAD: {
        uni_syscall_fs_pread_request v;
        if (uni_syscall_fs_pread_request_decode(&r, &a, &v) == 0) {
            mj_buf_puts(out, "{\"fd\":");
            put_u32_num(out, v.fd);
            mj_buf_puts(out, ",\"len\":");
            put_u32_num(out, v.len);
            mj_buf_puts(out, ",\"offset\":");
            put_u64_str(out, v.offset);
            mj_buf_ch(out, '}');
            *ok = 1;
        }
        break;
    }
    case UNI_SYSCALL_FS_PWRITE: {
        uni_syscall_fs_pwrite_request v;
        if (uni_syscall_fs_pwrite_request_decode(&r, &a, &v) == 0) {
            mj_buf_puts(out, "{\"data_hex\":");
            put_hex(out, v.data.data, v.data.len);
            mj_buf_puts(out, ",\"fd\":");
            put_u32_num(out, v.fd);
            mj_buf_puts(out, ",\"offset\":");
            put_u64_str(out, v.offset);
            mj_buf_ch(out, '}');
            *ok = 1;
        }
        break;
    }
    case UNI_SYSCALL_FS_LSEEK: {
        uni_syscall_fs_lseek_request v;
        if (uni_syscall_fs_lseek_request_decode(&r, &a, &v) == 0) {
            mj_buf_puts(out, "{\"fd\":");
            put_u32_num(out, v.fd);
            mj_buf_puts(out, ",\"offset\":");
            put_i64_str(out, v.offset);
            mj_buf_puts(out, ",\"whence\":");
            put_u32_num(out, v.whence);
            mj_buf_ch(out, '}');
            *ok = 1;
        }
        break;
    }
    case UNI_SYSCALL_FS_FSTAT: {
        uint32_t v;
        if (uni_syscall_fs_fstat_request_decode(&r, &a, &v) == 0) {
            mj_buf_puts(out, "{\"fd\":");
            put_u32_num(out, v);
            mj_buf_ch(out, '}');
            *ok = 1;
        }
        break;
    }
    case UNI_SYSCALL_FS_STAT: {
        uni_syscall_fs_stat_request v;
        if (uni_syscall_fs_stat_request_decode(&r, &a, &v) == 0) {
            mj_buf_puts(out, "{\"oid\":");
            put_oid(out, &v.oid);
            mj_buf_puts(out, ",\"path\":");
            put_json_str(out, v.path);
            mj_buf_ch(out, '}');
            *ok = 1;
        }
        break;
    }
    case UNI_SYSCALL_FS_FSYNC: {
        uint32_t v;
        if (uni_syscall_fs_fsync_request_decode(&r, &a, &v) == 0) {
            mj_buf_puts(out, "{\"fd\":");
            put_u32_num(out, v);
            mj_buf_ch(out, '}');
            *ok = 1;
        }
        break;
    }
    case UNI_SYSCALL_FS_READDIR: {
        uni_syscall_fs_readdir_request v;
        if (uni_syscall_fs_readdir_request_decode(&r, &a, &v) == 0) {
            mj_buf_puts(out, "{\"oid\":");
            put_oid(out, &v.oid);
            mj_buf_puts(out, ",\"path\":");
            put_json_str(out, v.path);
            mj_buf_ch(out, '}');
            *ok = 1;
        }
        break;
    }
    case UNI_SYSCALL_RELATION_GET: {
        uni_syscall_relation_get_request v;
        uint32_t i;
        if (uni_syscall_relation_get_request_decode(&r, &a, &v) == 0) {
            mj_buf_puts(out, "{\"key\":[");
            for (i = 0; i < v.key.len; i++) {
                if (i) {
                    mj_buf_ch(out, ',');
                }
                put_attr_datum(out, v.key.items[i].f0, v.key.items[i].f1);
            }
            mj_buf_puts(out, "],\"oid\":");
            put_oid(out, &v.oid);
            mj_buf_puts(out, ",\"select\":[");
            for (i = 0; i < v.select.len; i++) {
                if (i) {
                    mj_buf_ch(out, ',');
                }
                put_u64_str(out, v.select.items[i]);
            }
            mj_buf_puts(out, "],\"table\":");
            put_json_str(out, v.table);
            mj_buf_ch(out, '}');
            *ok = 1;
        }
        break;
    }
    case UNI_SYSCALL_RELATION_UPDATE: {
        uni_syscall_relation_update_request v;
        uint32_t i;
        if (uni_syscall_relation_update_request_decode(&r, &a, &v) == 0) {
            mj_buf_puts(out, "{\"deltas\":[");
            for (i = 0; i < v.deltas.len; i++) {
                if (i) {
                    mj_buf_ch(out, ',');
                }
                mj_buf_puts(out, "{\"attr\":");
                put_u64_str(out, v.deltas.items[i].f0);
                mj_buf_puts(out, ",\"datum_hex\":");
                put_hex(out, v.deltas.items[i].f2.data, v.deltas.items[i].f2.len);
                mj_buf_puts(out, ",\"op\":");
                put_u32_num(out, v.deltas.items[i].f1);
                mj_buf_ch(out, '}');
            }
            mj_buf_puts(out, "],\"key\":[");
            for (i = 0; i < v.key.len; i++) {
                if (i) {
                    mj_buf_ch(out, ',');
                }
                put_attr_datum(out, v.key.items[i].f0, v.key.items[i].f1);
            }
            mj_buf_puts(out, "],\"oid\":");
            put_oid(out, &v.oid);
            mj_buf_puts(out, ",\"table\":");
            put_json_str(out, v.table);
            mj_buf_puts(out, ",\"values\":[");
            for (i = 0; i < v.values.len; i++) {
                if (i) {
                    mj_buf_ch(out, ',');
                }
                put_attr_datum(out, v.values.items[i].f0, v.values.items[i].f1);
            }
            mj_buf_puts(out, "]}");
            *ok = 1;
        }
        break;
    }
    case UNI_SYSCALL_RELATION_INSERT: {
        uni_syscall_relation_insert_request v;
        uint32_t i;
        if (uni_syscall_relation_insert_request_decode(&r, &a, &v) == 0) {
            mj_buf_puts(out, "{\"key\":[");
            for (i = 0; i < v.key.len; i++) {
                if (i) {
                    mj_buf_ch(out, ',');
                }
                put_attr_datum(out, v.key.items[i].f0, v.key.items[i].f1);
            }
            mj_buf_puts(out, "],\"oid\":");
            put_oid(out, &v.oid);
            mj_buf_puts(out, ",\"table\":");
            put_json_str(out, v.table);
            mj_buf_puts(out, ",\"values\":[");
            for (i = 0; i < v.values.len; i++) {
                if (i) {
                    mj_buf_ch(out, ',');
                }
                put_attr_datum(out, v.values.items[i].f0, v.values.items[i].f1);
            }
            mj_buf_puts(out, "]}");
            *ok = 1;
        }
        break;
    }
    default:
        break;
    }
    mpa_free(&a);
}

static void response_expect(int kind, segment seg, mj_buf *out, int *ok) {
    mp_arena a;
    mp_reader r;
    mpa_init(&a);
    mpr_init(&r, seg.data, seg.len);
    *ok = 0;
    switch (kind) {
    case UNI_SYSCALL_QUERY: {
        uni_syscall_query_result res;
        if (uni_syscall_query_result_decode(&r, &a, &res) == 0 && !res.is_err
            && put_query_result(out, &res.value) == 0) {
            *ok = 1;
        }
        break;
    }
    case UNI_SYSCALL_COMMAND: {
        uni_syscall_command_result res;
        if (uni_syscall_command_result_decode(&r, &a, &res) == 0 && !res.is_err) {
            mj_buf_puts(out, "{\"affected_rows\":");
            put_u64_str(out, res.value.affected_rows);
            mj_buf_ch(out, '}');
            *ok = 1;
        }
        break;
    }
    case UNI_SYSCALL_BATCH: {
        uni_syscall_batch_result res;
        if (uni_syscall_batch_result_decode(&r, &a, &res) == 0 && !res.is_err) {
            mj_buf_puts(out, "{\"affected_rows\":");
            put_u64_str(out, res.value.affected_rows);
            mj_buf_ch(out, '}');
            *ok = 1;
        }
        break;
    }
    case UNI_SYSCALL_OPEN_SESSION: {
        uni_syscall_open_session_result res;
        /* The sidecar renders the session UniOid as its u128 decimal value,
         * which equals l for every h == 0 session id used by the fixtures. */
        if (uni_syscall_open_session_result_decode(&r, &a, &res) == 0 && !res.is_err
            && res.value.h == 0) {
            mj_buf_puts(out, "{\"session\":");
            put_u64_str(out, res.value.l);
            mj_buf_ch(out, '}');
            *ok = 1;
        }
        break;
    }
    case UNI_SYSCALL_CLOSE_SESSION: {
        uni_syscall_close_session_result res;
        if (uni_syscall_close_session_result_decode(&r, &a, &res) == 0 && !res.is_err) {
            put_unit(out);
            *ok = 1;
        }
        break;
    }
    case UNI_SYSCALL_GET: {
        uni_syscall_get_result res;
        /* Absent get values are outside the sidecar conventions. */
        if (uni_syscall_get_result_decode(&r, &a, &res) == 0 && !res.is_err
            && res.value.has_value) {
            mj_buf_puts(out, "{\"value_hex\":");
            put_hex(out, res.value.value.data, res.value.value.len);
            mj_buf_ch(out, '}');
            *ok = 1;
        }
        break;
    }
    case UNI_SYSCALL_PUT: {
        uni_syscall_put_result res;
        if (uni_syscall_put_result_decode(&r, &a, &res) == 0 && !res.is_err) {
            put_unit(out);
            *ok = 1;
        }
        break;
    }
    case UNI_SYSCALL_DELETE: {
        uni_syscall_delete_result res;
        if (uni_syscall_delete_result_decode(&r, &a, &res) == 0 && !res.is_err) {
            put_unit(out);
            *ok = 1;
        }
        break;
    }
    case UNI_SYSCALL_RANGE: {
        uni_syscall_range_result res;
        uint32_t i;
        if (uni_syscall_range_result_decode(&r, &a, &res) == 0 && !res.is_err) {
            mj_buf_puts(out, "{\"items\":[");
            for (i = 0; i < res.value.len; i++) {
                if (i) {
                    mj_buf_ch(out, ',');
                }
                mj_buf_puts(out, "{\"key_hex\":");
                put_hex(out, res.value.items[i].f0.data, res.value.items[i].f0.len);
                mj_buf_puts(out, ",\"value_hex\":");
                put_hex(out, res.value.items[i].f1.data, res.value.items[i].f1.len);
                mj_buf_ch(out, '}');
            }
            mj_buf_puts(out, "]}");
            *ok = 1;
        }
        break;
    }
    case UNI_SYSCALL_FS_OPEN: {
        uni_syscall_fs_open_result res;
        if (uni_syscall_fs_open_result_decode(&r, &a, &res) == 0 && !res.is_err) {
            mj_buf_puts(out, "{\"fd\":");
            put_u32_num(out, res.value);
            mj_buf_ch(out, '}');
            *ok = 1;
        }
        break;
    }
    case UNI_SYSCALL_FS_CLOSE: {
        uni_syscall_fs_close_result res;
        if (uni_syscall_fs_close_result_decode(&r, &a, &res) == 0 && !res.is_err) {
            put_unit(out);
            *ok = 1;
        }
        break;
    }
    case UNI_SYSCALL_FS_READ: {
        uni_syscall_fs_read_result res;
        if (uni_syscall_fs_read_result_decode(&r, &a, &res) == 0 && !res.is_err) {
            mj_buf_puts(out, "{\"data_hex\":");
            put_hex(out, res.value.data, res.value.len);
            mj_buf_ch(out, '}');
            *ok = 1;
        }
        break;
    }
    case UNI_SYSCALL_FS_WRITE: {
        uni_syscall_fs_write_result res;
        if (uni_syscall_fs_write_result_decode(&r, &a, &res) == 0 && !res.is_err) {
            mj_buf_puts(out, "{\"written\":");
            put_u32_num(out, res.value);
            mj_buf_ch(out, '}');
            *ok = 1;
        }
        break;
    }
    case UNI_SYSCALL_FS_PREAD: {
        uni_syscall_fs_pread_result res;
        if (uni_syscall_fs_pread_result_decode(&r, &a, &res) == 0 && !res.is_err) {
            mj_buf_puts(out, "{\"data_hex\":");
            put_hex(out, res.value.data, res.value.len);
            mj_buf_ch(out, '}');
            *ok = 1;
        }
        break;
    }
    case UNI_SYSCALL_FS_PWRITE: {
        uni_syscall_fs_pwrite_result res;
        if (uni_syscall_fs_pwrite_result_decode(&r, &a, &res) == 0 && !res.is_err) {
            put_unit(out);
            *ok = 1;
        }
        break;
    }
    case UNI_SYSCALL_FS_LSEEK: {
        uni_syscall_fs_lseek_result res;
        if (uni_syscall_fs_lseek_result_decode(&r, &a, &res) == 0 && !res.is_err) {
            mj_buf_puts(out, "{\"position\":");
            put_u64_str(out, res.value);
            mj_buf_ch(out, '}');
            *ok = 1;
        }
        break;
    }
    case UNI_SYSCALL_FS_FSTAT: {
        uni_syscall_fs_fstat_result res;
        if (uni_syscall_fs_fstat_result_decode(&r, &a, &res) == 0 && !res.is_err) {
            mj_buf_puts(out, "{\"stat\":");
            put_fs_stat(out, &res.value);
            mj_buf_ch(out, '}');
            *ok = 1;
        }
        break;
    }
    case UNI_SYSCALL_FS_STAT: {
        uni_syscall_fs_stat_result res;
        if (uni_syscall_fs_stat_result_decode(&r, &a, &res) == 0 && !res.is_err) {
            mj_buf_puts(out, "{\"stat\":");
            put_fs_stat(out, &res.value);
            mj_buf_ch(out, '}');
            *ok = 1;
        }
        break;
    }
    case UNI_SYSCALL_FS_FSYNC: {
        uni_syscall_fs_fsync_result res;
        if (uni_syscall_fs_fsync_result_decode(&r, &a, &res) == 0 && !res.is_err) {
            put_unit(out);
            *ok = 1;
        }
        break;
    }
    case UNI_SYSCALL_FS_READDIR: {
        uni_syscall_fs_readdir_result res;
        uint32_t i;
        if (uni_syscall_fs_readdir_result_decode(&r, &a, &res) == 0 && !res.is_err) {
            mj_buf_puts(out, "{\"entries\":[");
            for (i = 0; i < res.value.len; i++) {
                if (i) {
                    mj_buf_ch(out, ',');
                }
                mj_buf_puts(out, "{\"is_dir\":");
                mj_buf_puts(out, res.value.items[i].is_dir ? "true" : "false");
                mj_buf_puts(out, ",\"length\":");
                put_u64_str(out, res.value.items[i].length);
                mj_buf_puts(out, ",\"name\":");
                put_json_str(out, res.value.items[i].name);
                mj_buf_ch(out, '}');
            }
            mj_buf_puts(out, "]}");
            *ok = 1;
        }
        break;
    }
    case UNI_SYSCALL_RELATION_GET: {
        uni_syscall_relation_get_result res;
        if (uni_syscall_relation_get_result_decode(&r, &a, &res) == 0 && !res.is_err) {
            mj_buf_puts(out, "{\"row\":");
            put_relation_row(out, &res.value);
            mj_buf_ch(out, '}');
            *ok = 1;
        }
        break;
    }
    case UNI_SYSCALL_RELATION_UPDATE: {
        uni_syscall_relation_update_result res;
        if (uni_syscall_relation_update_result_decode(&r, &a, &res) == 0 && !res.is_err) {
            mj_buf_puts(out, "{\"affected\":");
            put_u64_str(out, res.value);
            mj_buf_ch(out, '}');
            *ok = 1;
        }
        break;
    }
    case UNI_SYSCALL_RELATION_INSERT: {
        uni_syscall_relation_insert_result res;
        if (uni_syscall_relation_insert_result_decode(&r, &a, &res) == 0 && !res.is_err) {
            put_unit(out);
            *ok = 1;
        }
        break;
    }
    default:
        break;
    }
    mpa_free(&a);
}

static void err_expect(int kind, segment seg, mj_buf *out, int *ok) {
    mp_arena a;
    mp_reader r;
    mpa_init(&a);
    mpr_init(&r, seg.data, seg.len);
    *ok = 0;
    /* Every corpus err frame is a `get` result; decode through the kind's
     * own result decoder so new err kinds slot in without changes. */
    switch (kind) {
    case UNI_SYSCALL_GET: {
        uni_syscall_get_result res;
        if (uni_syscall_get_result_decode(&r, &a, &res) == 0 && res.is_err) {
            put_err(out, &res.err);
            *ok = 1;
        }
        break;
    }
    default:
        break;
    }
    mpa_free(&a);
}

/* Decode one response frame and re-encode it (roundtrip byte-identity
 * check). */
static int response_reencode(int kind, segment seg, mp_writer *w) {
    mp_arena a;
    mp_reader r;
    int rc = -1;
    mpa_init(&a);
    mpr_init(&r, seg.data, seg.len);
    switch (kind) {
    case UNI_SYSCALL_QUERY: {
        uni_syscall_query_result res;
        if (uni_syscall_query_result_decode(&r, &a, &res) == 0) {
            rc = uni_syscall_query_result_encode(w, &res);
        }
        break;
    }
    case UNI_SYSCALL_COMMAND: {
        uni_syscall_command_result res;
        if (uni_syscall_command_result_decode(&r, &a, &res) == 0) {
            rc = uni_syscall_command_result_encode(w, &res);
        }
        break;
    }
    case UNI_SYSCALL_BATCH: {
        uni_syscall_batch_result res;
        if (uni_syscall_batch_result_decode(&r, &a, &res) == 0) {
            rc = uni_syscall_batch_result_encode(w, &res);
        }
        break;
    }
    case UNI_SYSCALL_OPEN_SESSION: {
        uni_syscall_open_session_result res;
        if (uni_syscall_open_session_result_decode(&r, &a, &res) == 0) {
            rc = uni_syscall_open_session_result_encode(w, &res);
        }
        break;
    }
    case UNI_SYSCALL_CLOSE_SESSION: {
        uni_syscall_close_session_result res;
        if (uni_syscall_close_session_result_decode(&r, &a, &res) == 0) {
            rc = uni_syscall_close_session_result_encode(w, &res);
        }
        break;
    }
    case UNI_SYSCALL_GET: {
        uni_syscall_get_result res;
        if (uni_syscall_get_result_decode(&r, &a, &res) == 0) {
            rc = uni_syscall_get_result_encode(w, &res);
        }
        break;
    }
    case UNI_SYSCALL_PUT: {
        uni_syscall_put_result res;
        if (uni_syscall_put_result_decode(&r, &a, &res) == 0) {
            rc = uni_syscall_put_result_encode(w, &res);
        }
        break;
    }
    case UNI_SYSCALL_DELETE: {
        uni_syscall_delete_result res;
        if (uni_syscall_delete_result_decode(&r, &a, &res) == 0) {
            rc = uni_syscall_delete_result_encode(w, &res);
        }
        break;
    }
    case UNI_SYSCALL_RANGE: {
        uni_syscall_range_result res;
        if (uni_syscall_range_result_decode(&r, &a, &res) == 0) {
            rc = uni_syscall_range_result_encode(w, &res);
        }
        break;
    }
    case UNI_SYSCALL_FS_OPEN: {
        uni_syscall_fs_open_result res;
        if (uni_syscall_fs_open_result_decode(&r, &a, &res) == 0) {
            rc = uni_syscall_fs_open_result_encode(w, &res);
        }
        break;
    }
    case UNI_SYSCALL_FS_CLOSE: {
        uni_syscall_fs_close_result res;
        if (uni_syscall_fs_close_result_decode(&r, &a, &res) == 0) {
            rc = uni_syscall_fs_close_result_encode(w, &res);
        }
        break;
    }
    case UNI_SYSCALL_FS_READ: {
        uni_syscall_fs_read_result res;
        if (uni_syscall_fs_read_result_decode(&r, &a, &res) == 0) {
            rc = uni_syscall_fs_read_result_encode(w, &res);
        }
        break;
    }
    case UNI_SYSCALL_FS_WRITE: {
        uni_syscall_fs_write_result res;
        if (uni_syscall_fs_write_result_decode(&r, &a, &res) == 0) {
            rc = uni_syscall_fs_write_result_encode(w, &res);
        }
        break;
    }
    case UNI_SYSCALL_FS_PREAD: {
        uni_syscall_fs_pread_result res;
        if (uni_syscall_fs_pread_result_decode(&r, &a, &res) == 0) {
            rc = uni_syscall_fs_pread_result_encode(w, &res);
        }
        break;
    }
    case UNI_SYSCALL_FS_PWRITE: {
        uni_syscall_fs_pwrite_result res;
        if (uni_syscall_fs_pwrite_result_decode(&r, &a, &res) == 0) {
            rc = uni_syscall_fs_pwrite_result_encode(w, &res);
        }
        break;
    }
    case UNI_SYSCALL_FS_LSEEK: {
        uni_syscall_fs_lseek_result res;
        if (uni_syscall_fs_lseek_result_decode(&r, &a, &res) == 0) {
            rc = uni_syscall_fs_lseek_result_encode(w, &res);
        }
        break;
    }
    case UNI_SYSCALL_FS_FSTAT: {
        uni_syscall_fs_fstat_result res;
        if (uni_syscall_fs_fstat_result_decode(&r, &a, &res) == 0) {
            rc = uni_syscall_fs_fstat_result_encode(w, &res);
        }
        break;
    }
    case UNI_SYSCALL_FS_STAT: {
        uni_syscall_fs_stat_result res;
        if (uni_syscall_fs_stat_result_decode(&r, &a, &res) == 0) {
            rc = uni_syscall_fs_stat_result_encode(w, &res);
        }
        break;
    }
    case UNI_SYSCALL_FS_FSYNC: {
        uni_syscall_fs_fsync_result res;
        if (uni_syscall_fs_fsync_result_decode(&r, &a, &res) == 0) {
            rc = uni_syscall_fs_fsync_result_encode(w, &res);
        }
        break;
    }
    case UNI_SYSCALL_FS_READDIR: {
        uni_syscall_fs_readdir_result res;
        if (uni_syscall_fs_readdir_result_decode(&r, &a, &res) == 0) {
            rc = uni_syscall_fs_readdir_result_encode(w, &res);
        }
        break;
    }
    case UNI_SYSCALL_RELATION_GET: {
        uni_syscall_relation_get_result res;
        if (uni_syscall_relation_get_result_decode(&r, &a, &res) == 0) {
            rc = uni_syscall_relation_get_result_encode(w, &res);
        }
        break;
    }
    case UNI_SYSCALL_RELATION_UPDATE: {
        uni_syscall_relation_update_result res;
        if (uni_syscall_relation_update_result_decode(&r, &a, &res) == 0) {
            rc = uni_syscall_relation_update_result_encode(w, &res);
        }
        break;
    }
    case UNI_SYSCALL_RELATION_INSERT: {
        uni_syscall_relation_insert_result res;
        if (uni_syscall_relation_insert_result_decode(&r, &a, &res) == 0) {
            rc = uni_syscall_relation_insert_result_encode(w, &res);
        }
        break;
    }
    default:
        break;
    }
    mpa_free(&a);
    return rc;
}

/* ---- syscall_payload_v1_all ---- */

static void run_syscall_corpus(void) {
    fixture f;
    const mj_value *frames;
    size_t i;
    char label[160];
    if (load_fixture("syscall_payload_v1_all", &f) != 0) {
        check_true("syscall_payload_v1_all", "fixture load", 0);
        return;
    }
    frames = mj_get(f.sidecar, "frames");
    check_true("syscall_payload_v1_all", "segment/frame count",
               mj_is(frames, MJ_ARRAY) && mj_len(frames) == f.count);
    for (i = 0; i < f.count; i++) {
        const mj_value *frame = mj_at(frames, i);
        const mj_value *expect = mj_get(frame, "expect");
        int kind = (int)strtol(mj_num(mj_get(frame, "message_kind")), 0, 10);
        const char *direction = mj_str(mj_get(frame, "direction"));
        segment seg = f.segments[i];
        int hdr_kind = -1;
        mp_writer w;
        mj_buf actual;
        mj_buf expected;
        int ok = 0;
        snprintf(label, sizeof(label), "%lu %s %s", (unsigned long)i,
                 mj_str(mj_get(frame, "message_kind_name")), direction);

        check_true(label, "header kind",
                   header_kind(seg, &hdr_kind) == 0 && hdr_kind == kind);

        mj_buf_init(&actual);
        mj_buf_init(&expected);
        mpw_init(&w);

        if (strcmp(direction, "request") == 0) {
            check_true(label, "request encode rc", request_encode(kind, &w) == 0);
            check_bytes(label, w.buf, w.len, seg.data, seg.len);
            request_expect(kind, seg, &actual, &ok);
        } else if (strcmp(direction, "response") == 0) {
            check_true(label, "response encode rc", response_encode_ok(kind, &w) == 0);
            check_bytes(label, w.buf, w.len, seg.data, seg.len);
            response_expect(kind, seg, &actual, &ok);
        } else if (strcmp(direction, "response_err") == 0) {
            check_true(label, "err response encode rc", err_response_encode(&w) == 0);
            check_bytes(label, w.buf, w.len, seg.data, seg.len);
            err_expect(kind, seg, &actual, &ok);
        } else {
            check_true(label, "unknown direction", 0);
        }

        /* decode -> sidecar expect shape == sidecar expect object */
        check_true(label, "decode rc", ok);
        if (ok) {
            const char *expected_text = mj_serialized(expect, &expected);
            check_text(label, actual.data, expected_text ? expected_text : "");
        }

        /* responses: decode -> re-encode byte-identically */
        if (strcmp(direction, "request") != 0) {
            mp_writer rw;
            mpw_init(&rw);
            check_true(label, "decode+re-encode rc", response_reencode(kind, seg, &rw) == 0);
            check_bytes(label, rw.buf, rw.len, seg.data, seg.len);
            mpw_free(&rw);
        }

        mpw_free(&w);
        mj_buf_free(&actual);
        mj_buf_free(&expected);
    }
    free_fixture(&f);
}

/* ---- lenient_decode_v1 ---- */

static void run_lenient_decode(void) {
    fixture f;
    const mj_value *vectors;
    size_t i;
    char label[160];
    if (load_fixture("lenient_decode_v1", &f) != 0) {
        check_true("lenient_decode_v1", "fixture load", 0);
        return;
    }
    vectors = mj_get(f.sidecar, "vectors");
    check_true("lenient_decode_v1", "segment/vector count",
               mj_is(vectors, MJ_ARRAY) && mj_len(vectors) == f.count);
    for (i = 0; i < f.count; i++) {
        const mj_value *vector = mj_at(vectors, i);
        const mj_value *expect = mj_get(vector, "expect");
        int kind = (int)strtol(mj_num(mj_get(vector, "message_kind")), 0, 10);
        const char *direction = mj_str(mj_get(vector, "direction"));
        segment seg = f.segments[i];
        int hdr_kind = -1;
        int ok = 0;
        mj_buf actual;
        mj_buf expected;
        snprintf(label, sizeof(label), "%lu %s (%s %s)", (unsigned long)i,
                 mj_str(mj_get(vector, "kind")), mj_str(mj_get(vector, "message_kind_name")),
                 direction);

        check_true(label, "header kind",
                   header_kind(seg, &hdr_kind) == 0 && hdr_kind == kind);

        mj_buf_init(&actual);
        mj_buf_init(&expected);
        if (strcmp(direction, "request") == 0) {
            request_expect(kind, seg, &actual, &ok);
        } else if (strcmp(direction, "response") == 0) {
            response_expect(kind, seg, &actual, &ok);
        } else {
            check_true(label, "unknown direction", 0);
        }
        check_true(label, "decode rc", ok);
        if (ok) {
            const char *expected_text = mj_serialized(expect, &expected);
            check_text(label, actual.data, expected_text ? expected_text : "");
        }
        mj_buf_free(&actual);
        mj_buf_free(&expected);
    }
    free_fixture(&f);
}

/* ---- record bridge ----
 *
 * Self-contained (no fixture): the bridge converts between the host's
 * `uni-data-value` record-case envelope and the integer-keyed MessagePack
 * map shape the mgen-generated record codecs consume/produce. Envelopes are
 * built on the stack (the write side only borrows); read-side products land
 * in a local arena.
 */

static uni_data_value mk_scalar(uni_scalar_value *scalar) {
    uni_data_value v;
    memset(&v, 0, sizeof(v));
    v.kind = UNI_DATA_VALUE_SCALAR;
    v.as.scalar = scalar;
    return v;
}

static uni_scalar_value mk_string_scalar(const char *s) {
    uni_scalar_value v;
    memset(&v, 0, sizeof(v));
    v.kind = UNI_SCALAR_VALUE_STRING;
    v.as.string = mp_str_from_cstr(s);
    return v;
}

static uni_scalar_value mk_i32_scalar(int32_t n) {
    uni_scalar_value v;
    memset(&v, 0, sizeof(v));
    v.kind = UNI_SCALAR_VALUE_I32;
    v.as.i32 = n;
    return v;
}

/* The wallet-shaped record: {display-name: "Ada", level: 7, vip-as-i32: 1,
 * tags: ["a", "b"], home: {city: "sh", zip: "200"}} — the envelope the host
 * hands a guest (positional entries, empty names). */
static uni_data_value mk_profile_envelope(uni_scalar_value *scalars,
                                          uni_data_value *values,
                                          uni_data_value_field *home_fields,
                                          uni_data_value_field *fields) {
    uni_data_value home;
    uni_data_value envelope;
    scalars[0] = mk_string_scalar("Ada");
    scalars[1] = mk_i32_scalar(7);
    scalars[2] = mk_i32_scalar(1); /* a bool field crosses as i32 */
    scalars[3] = mk_string_scalar("a");
    scalars[4] = mk_string_scalar("b");
    scalars[5] = mk_string_scalar("sh");
    scalars[6] = mk_string_scalar("200");
    values[0] = mk_scalar(&scalars[0]);
    values[1] = mk_scalar(&scalars[1]);
    values[2] = mk_scalar(&scalars[2]);
    values[3] = mk_scalar(&scalars[3]);
    values[4] = mk_scalar(&scalars[4]);
    values[5] = mk_scalar(&scalars[5]);
    values[6] = mk_scalar(&scalars[6]);
    home_fields[0].field_name = mp_str_from(0, 0);
    home_fields[0].field_value = values[5];
    home_fields[1].field_name = mp_str_from(0, 0);
    home_fields[1].field_value = values[6];
    memset(&home, 0, sizeof(home));
    home.kind = UNI_DATA_VALUE_RECORD;
    home.as.record.items = home_fields;
    home.as.record.len = 2;

    memset(&envelope, 0, sizeof(envelope));
    fields[0].field_name = mp_str_from(0, 0);
    fields[0].field_value = values[0];
    fields[1].field_name = mp_str_from(0, 0);
    fields[1].field_value = values[1];
    fields[2].field_name = mp_str_from(0, 0);
    fields[2].field_value = values[2];
    fields[3].field_name = mp_str_from(0, 0);
    fields[3].field_value.kind = UNI_DATA_VALUE_ARRAY;
    fields[3].field_value.as.array.items = &values[3]; /* "a", "b" contiguous */
    fields[3].field_value.as.array.len = 2;
    fields[4].field_name = mp_str_from(0, 0);
    fields[4].field_value = home;
    envelope.kind = UNI_DATA_VALUE_RECORD;
    envelope.as.record.items = fields;
    envelope.as.record.len = 5;
    return envelope;
}

static void run_record_bridge_write(void) {
    static const uint8_t expected[] = {
        0x85, /* map(5) */
        0x01, 0xa3, 'A', 'd', 'a', /* 1: "Ada" */
        0x02, 0x07, /* 2: 7 */
        0x03, 0x01, /* 3: 1 */
        0x04, 0x92, 0xa1, 'a', 0xa1, 'b', /* 4: ["a", "b"] */
        0x05, 0x82, 0x01, 0xa2, 's', 'h', 0x02, 0xa3, '2', '0', '0' /* 5: map */
    };
    uni_scalar_value scalars[8];
    uni_data_value values[8];
    uni_data_value_field home_fields[2];
    uni_data_value_field fields[5];
    uni_data_value envelope =
        mk_profile_envelope(scalars, values, home_fields, fields);
    mp_writer w;
    mpw_init(&w);
    check_true("bridge write", "rc", mp_record_bridge_write(&envelope, &w) == 0);
    check_bytes("bridge write", w.buf, w.len, expected, sizeof(expected));
    mpw_free(&w);
}

static void run_record_bridge_write_edge(void) {
    uni_scalar_value scalars[2];
    uni_data_value_field fields[1];
    uni_data_value envelope;
    mp_writer w;

    /* Null scalar -> nil */
    memset(scalars, 0, sizeof(scalars));
    scalars[0].kind = UNI_SCALAR_VALUE_NULL;
    fields[0].field_name = mp_str_from(0, 0);
    fields[0].field_value = mk_scalar(&scalars[0]);
    envelope.kind = UNI_DATA_VALUE_RECORD;
    envelope.as.record.items = fields;
    envelope.as.record.len = 1;
    {
        static const uint8_t expected[] = {0x81, 0x01, 0xc0};
        mpw_init(&w);
        check_true("bridge write null", "rc", mp_record_bridge_write(&envelope, &w) == 0);
        check_bytes("bridge write null", w.buf, w.len, expected, sizeof(expected));
        mpw_free(&w);
    }

    /* blob -> the record-context array of u8 (NOT a bin blob) */
    {
        static const uint8_t blob[] = {0x01, 0x02, 0x03};
        static const uint8_t expected[] = {0x81, 0x01, 0x93, 0x01, 0x02, 0x03};
        scalars[0].kind = UNI_SCALAR_VALUE_BLOB;
        scalars[0].as.blob = mp_bin_from(blob, sizeof(blob));
        mpw_init(&w);
        check_true("bridge write blob", "rc", mp_record_bridge_write(&envelope, &w) == 0);
        check_bytes("bridge write blob", w.buf, w.len, expected, sizeof(expected));
        mpw_free(&w);
    }

    /* 128-bit integers have no C record codec: flagged, rc -1 */
    {
        static const uint8_t wide[] = {0};
        scalars[0].kind = UNI_SCALAR_VALUE_U128;
        scalars[0].as.u128 = mp_bin_from(wide, sizeof(wide));
        mpw_init(&w);
        check_true("bridge write u128", "rc", mp_record_bridge_write(&envelope, &w) == -1);
        check_true("bridge write u128", "writer flagged", !mpw_ok(&w));
        mpw_free(&w);
    }

    /* a non-record envelope is an error */
    {
        uni_data_value scalar_envelope = mk_scalar(&scalars[0]);
        mpw_init(&w);
        check_true("bridge write non-record", "rc",
                   mp_record_bridge_write(&scalar_envelope, &w) == -1);
        check_true("bridge write non-record", "writer flagged", !mpw_ok(&w));
        mpw_free(&w);
    }
}

static void run_record_bridge_bool_leniency(void) {
    static const uint8_t one[] = {0x01};
    static const uint8_t zero[] = {0x00};
    static const uint8_t wide_one[] = {0xce, 0x00, 0x00, 0x00, 0x01};
    static const uint8_t two[] = {0x02};
    static const uint8_t neg_one[] = {0xff};
    mp_reader r;

    /* bool fields cross the procedure byte pipe as i32: mpr_bool accepts
     * any integer marker with value 0/1, rejects the rest */
    mpr_init(&r, one, sizeof(one));
    check_true("bool leniency", "fixint 1", mpr_bool(&r) == 1 && mpr_done(&r));
    mpr_init(&r, zero, sizeof(zero));
    check_true("bool leniency", "fixint 0", mpr_bool(&r) == 0 && mpr_done(&r));
    mpr_init(&r, wide_one, sizeof(wide_one));
    check_true("bool leniency", "u32 1", mpr_bool(&r) == 1 && mpr_done(&r));
    mpr_init(&r, two, sizeof(two));
    (void)mpr_bool(&r);
    check_true("bool leniency", "2 rejected", !mpr_ok(&r));
    mpr_init(&r, neg_one, sizeof(neg_one));
    (void)mpr_bool(&r);
    check_true("bool leniency", "-1 rejected", !mpr_ok(&r));
}

static void run_record_bridge_read(void) {
    /* the same wallet shape, with the bool field as a MessagePack bool (the
     * generated encoders write) */
    static const uint8_t bytes[] = {
        0x85, 0x01, 0xa3, 'A', 'd', 'a', 0x02, 0x07, 0x03, 0xc3, 0x04,
        0x92, 0xa1, 'a', 0xa1, 'b', 0x05, 0x82, 0x01, 0xa2, 's', 'h', 0x02,
        0xa3, '2', '0', '0'
    };
    mp_reader r;
    mp_arena a;
    uni_data_value out;
    const uni_data_value_field *f;
    mpr_init(&r, bytes, sizeof(bytes));
    mpa_init(&a);
    check_true("bridge read", "rc", mp_record_bridge_read(&r, &a, &out) == 0);
    check_true("bridge read", "consumed", mpr_done(&r));
    check_true("bridge read", "record case", out.kind == UNI_DATA_VALUE_RECORD);
    if (out.kind != UNI_DATA_VALUE_RECORD) {
        mpa_free(&a);
        return;
    }
    check_true("bridge read", "field count", out.as.record.len == 5);
    if (out.as.record.len != 5) {
        mpa_free(&a);
        return;
    }
    f = out.as.record.items;
    check_true("bridge read", "names empty",
               f[0].field_name.len == 0 && f[1].field_name.len == 0
                   && f[4].field_name.len == 0);
    check_true("bridge read", "string field",
               f[0].field_value.kind == UNI_DATA_VALUE_SCALAR
                   && f[0].field_value.as.scalar->kind == UNI_SCALAR_VALUE_STRING
                   && f[0].field_value.as.scalar->as.string.len == 3
                   && memcmp(f[0].field_value.as.scalar->as.string.data, "Ada", 3) == 0);
    check_true("bridge read", "i32 field",
               f[1].field_value.as.scalar->kind == UNI_SCALAR_VALUE_I32
                   && f[1].field_value.as.scalar->as.i32 == 7);
    /* MessagePack bool -> I32 0/1 (the host vocabulary has no Bool case) */
    check_true("bridge read", "bool as i32",
               f[2].field_value.as.scalar->kind == UNI_SCALAR_VALUE_I32
                   && f[2].field_value.as.scalar->as.i32 == 1);
    check_true("bridge read", "array case",
               f[3].field_value.kind == UNI_DATA_VALUE_ARRAY
                   && f[3].field_value.as.array.len == 2
                   && f[3].field_value.as.array.items[1].as.scalar->kind
                          == UNI_SCALAR_VALUE_STRING
                   && f[3].field_value.as.array.items[1].as.scalar->as.string.len == 1
                   && f[3].field_value.as.array.items[1].as.scalar->as.string.data[0]
                          == 'b');
    check_true("bridge read", "nested record",
               f[4].field_value.kind == UNI_DATA_VALUE_RECORD
                   && f[4].field_value.as.record.len == 2
                   && f[4].field_value.as.record.items[0].field_value.as.scalar->kind
                          == UNI_SCALAR_VALUE_STRING
                   && f[4]
                              .field_value.as.record.items[0]
                              .field_value.as.scalar->as.string.len
                          == 2
                   && memcmp(f[4]
                                     .field_value.as.record.items[0]
                                     .field_value.as.scalar->as.string.data,
                                 "sh", 2)
                          == 0);
    mpa_free(&a);
}

static void run_record_bridge_read_edge(void) {
    mp_reader r;
    mp_arena a;
    uni_data_value out;

    /* a skipped middle key (an omitted option field) decodes as Null */
    {
        static const uint8_t gap[] = {0x82, 0x01, 0xa1, 'x', 0x03, 0x09};
        mpr_init(&r, gap, sizeof(gap));
        mpa_init(&a);
        check_true("bridge read gap", "rc", mp_record_bridge_read(&r, &a, &out) == 0);
        check_true("bridge read gap", "count", out.as.record.len == 3);
        if (out.as.record.len == 3) {
            check_true("bridge read gap", "gap is null",
                       out.as.record.items[1].field_value.as.scalar->kind
                           == UNI_SCALAR_VALUE_NULL);
            check_true("bridge read gap", "after gap",
                       out.as.record.items[2].field_value.as.scalar->kind
                               == UNI_SCALAR_VALUE_I32
                           && out.as.record.items[2].field_value.as.scalar->as.i32 == 9);
        }
        mpa_free(&a);
    }

    /* integer widths: i32 when it fits, i64 otherwise (both signs) */
    {
        static const uint8_t big[] = {0x81, 0x01, 0xcf, 0x00, 0x00,
                                      0x01, 0x00, 0x00, 0x00, 0x00, 0x00}; /* 2^40 */
        static const uint8_t neg[] = {0x81, 0x01, 0xd3, 0xff, 0xff,
                                      0xff, 0xfe, 0x00, 0x00, 0x00, 0x00}; /* -2^33 */
        static const uint8_t small_neg[] = {0x81, 0x01, 0xd0, 0xfb}; /* -5 */
        mpr_init(&r, big, sizeof(big));
        mpa_init(&a);
        check_true("bridge read widths", "u64 rc", mp_record_bridge_read(&r, &a, &out) == 0);
        check_true("bridge read widths", "u64 -> i64",
                   out.as.record.items[0].field_value.as.scalar->kind
                           == UNI_SCALAR_VALUE_I64
                       && out.as.record.items[0].field_value.as.scalar->as.i64
                              == 1099511627776LL);
        mpa_free(&a);
        mpr_init(&r, neg, sizeof(neg));
        mpa_init(&a);
        check_true("bridge read widths", "i64 rc", mp_record_bridge_read(&r, &a, &out) == 0);
        check_true("bridge read widths", "i64 stays i64",
                   out.as.record.items[0].field_value.as.scalar->kind
                           == UNI_SCALAR_VALUE_I64
                       && out.as.record.items[0].field_value.as.scalar->as.i64
                              == -8589934592LL);
        mpa_free(&a);
        mpr_init(&r, small_neg, sizeof(small_neg));
        mpa_init(&a);
        check_true("bridge read widths", "i8 rc", mp_record_bridge_read(&r, &a, &out) == 0);
        check_true("bridge read widths", "i8 -> i32",
                   out.as.record.items[0].field_value.as.scalar->kind
                           == UNI_SCALAR_VALUE_I32
                       && out.as.record.items[0].field_value.as.scalar->as.i32 == -5);
        mpa_free(&a);
    }

    /* floats, bin, nil */
    {
        static const uint8_t f32b[] = {0x81, 0x01, 0xca, 0x3f, 0x80, 0x00, 0x00};
        static const uint8_t f64b[] = {0x81, 0x01, 0xcb, 0x3f, 0xf0,
                                       0x00, 0x00, 0x00, 0x00, 0x00, 0x00};
        static const uint8_t binb[] = {0x81, 0x01, 0xc4, 0x02, 0xaa, 0xbb};
        static const uint8_t nilb[] = {0x81, 0x01, 0xc0};
        mpr_init(&r, f32b, sizeof(f32b));
        mpa_init(&a);
        check_true("bridge read scalars", "f32 rc",
                   mp_record_bridge_read(&r, &a, &out) == 0);
        check_true("bridge read scalars", "f32",
                   out.as.record.items[0].field_value.as.scalar->kind
                           == UNI_SCALAR_VALUE_F32
                       && out.as.record.items[0].field_value.as.scalar->as.f32 == 1.0f);
        mpa_free(&a);
        mpr_init(&r, f64b, sizeof(f64b));
        mpa_init(&a);
        check_true("bridge read scalars", "f64 rc",
                   mp_record_bridge_read(&r, &a, &out) == 0);
        check_true("bridge read scalars", "f64",
                   out.as.record.items[0].field_value.as.scalar->kind
                           == UNI_SCALAR_VALUE_F64
                       && out.as.record.items[0].field_value.as.scalar->as.f64 == 1.0);
        mpa_free(&a);
        mpr_init(&r, binb, sizeof(binb));
        mpa_init(&a);
        check_true("bridge read scalars", "bin rc",
                   mp_record_bridge_read(&r, &a, &out) == 0);
        check_true("bridge read scalars", "bin -> blob",
                   out.as.record.items[0].field_value.as.scalar->kind
                           == UNI_SCALAR_VALUE_BLOB
                       && out.as.record.items[0].field_value.as.scalar->as.blob.len == 2
                       && out.as.record.items[0].field_value.as.scalar->as.blob.data[0]
                              == 0xaa);
        mpa_free(&a);
        mpr_init(&r, nilb, sizeof(nilb));
        mpa_init(&a);
        check_true("bridge read scalars", "nil rc",
                   mp_record_bridge_read(&r, &a, &out) == 0);
        check_true("bridge read scalars", "nil -> null",
                   out.as.record.items[0].field_value.as.scalar->kind
                       == UNI_SCALAR_VALUE_NULL);
        mpa_free(&a);
    }

    /* malformed shapes */
    {
        static const uint8_t not_map[] = {0x01};
        static const uint8_t key_zero[] = {0x81, 0x00, 0x01};
        static const uint8_t key_big[] = {0x81, 0xcd, 0x10, 0x01, 0x01}; /* key 4097 */
        static const uint8_t count_big[] = {0xde, 0x13, 0x88}; /* map16, 5000 entries */
        mpr_init(&r, not_map, sizeof(not_map));
        mpa_init(&a);
        check_true("bridge read errors", "non-map", mp_record_bridge_read(&r, &a, &out) == -1);
        mpa_free(&a);
        mpr_init(&r, key_zero, sizeof(key_zero));
        mpa_init(&a);
        check_true("bridge read errors", "key 0 skipped",
                   mp_record_bridge_read(&r, &a, &out) == 0 && out.as.record.len == 0);
        mpa_free(&a);
        mpr_init(&r, key_big, sizeof(key_big));
        mpa_init(&a);
        check_true("bridge read errors", "key over bound",
                   mp_record_bridge_read(&r, &a, &out) == -1);
        mpa_free(&a);
        mpr_init(&r, count_big, sizeof(count_big));
        mpa_init(&a);
        check_true("bridge read errors", "count over bound",
                   mp_record_bridge_read(&r, &a, &out) == -1);
        mpa_free(&a);
    }
}

static void run_record_bridge_roundtrip(void) {
    /* read (host-vocabulary normalization) then write: the bool marker and
     * the gap come back in the canonical positional form */
    static const uint8_t input[] = {0x83, 0x01, 0xa1, 'x', 0x03, 0xc3, 0x04, 0x02};
    static const uint8_t expected[] = {0x84, 0x01, 0xa1, 'x', 0x02, 0xc0,
                                       0x03, 0x01, 0x04, 0x02};
    mp_reader r;
    mp_arena a;
    uni_data_value out;
    mp_writer w;
    mpr_init(&r, input, sizeof(input));
    mpa_init(&a);
    check_true("bridge roundtrip", "read rc", mp_record_bridge_read(&r, &a, &out) == 0);
    mpw_init(&w);
    check_true("bridge roundtrip", "write rc", mp_record_bridge_write(&out, &w) == 0);
    check_bytes("bridge roundtrip", w.buf, w.len, expected, sizeof(expected));
    mpw_free(&w);
    mpa_free(&a);
}

static void run_record_bridge(void) {
    run_record_bridge_write();
    run_record_bridge_write_edge();
    run_record_bridge_bool_leniency();
    run_record_bridge_read();
    run_record_bridge_read_edge();
    run_record_bridge_roundtrip();
}

int main(void) {
    if (find_fixture_dir() != 0) {
        fprintf(stderr, "could not locate crates/db-kernel/testing/fixtures/golden/v1 "
                        "by walking up from the cwd\n");
        return 2;
    }
    run_mp_primitives();
    run_syscall_corpus();
    run_lenient_decode();
    run_record_bridge();
    printf("%ld checks, %d failure(s)\n", g_checks, g_failures);
    return g_failures ? 1 : 0;
}
