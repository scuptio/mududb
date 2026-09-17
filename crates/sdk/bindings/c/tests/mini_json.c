/* Minimal JSON DOM parser and canonical serializer — see mini_json.h. */
#include "mini_json.h"

#include <stdlib.h>
#include <string.h>

/* ---- buffer ---- */

void mj_buf_init(mj_buf *b) {
    b->data = 0;
    b->len = 0;
    b->cap = 0;
    b->oom = 0;
}

void mj_buf_free(mj_buf *b) {
    free(b->data);
    mj_buf_init(b);
}

static void mj_buf_reserve(mj_buf *b, size_t extra) {
    if (b->oom || b->len + extra <= b->cap) {
        return;
    }
    {
        size_t cap = b->cap ? b->cap : 128;
        char *data;
        while (cap < b->len + extra) {
            cap *= 2;
        }
        data = (char *)realloc(b->data, cap);
        if (!data) {
            b->oom = 1;
            return;
        }
        b->data = data;
        b->cap = cap;
    }
}

void mj_buf_put(mj_buf *b, const char *data, size_t len) {
    mj_buf_reserve(b, len + 1);
    if (b->oom) {
        return;
    }
    memcpy(b->data + b->len, data, len);
    b->len += len;
    b->data[b->len] = '\0';
}

void mj_buf_puts(mj_buf *b, const char *cstr) {
    mj_buf_put(b, cstr, strlen(cstr));
}

void mj_buf_ch(mj_buf *b, char c) {
    mj_buf_put(b, &c, 1);
}

void mj_buf_put_escaped(mj_buf *b, const char *data, size_t len) {
    static const char hexd[] = "0123456789abcdef";
    size_t i;
    for (i = 0; i < len; i++) {
        unsigned char c = (unsigned char)data[i];
        switch (c) {
        case '"':
            mj_buf_puts(b, "\\\"");
            break;
        case '\\':
            mj_buf_puts(b, "\\\\");
            break;
        case '\n':
            mj_buf_puts(b, "\\n");
            break;
        case '\r':
            mj_buf_puts(b, "\\r");
            break;
        case '\t':
            mj_buf_puts(b, "\\t");
            break;
        default:
            if (c < 0x20) {
                mj_buf_puts(b, "\\u00");
                mj_buf_ch(b, hexd[c >> 4]);
                mj_buf_ch(b, hexd[c & 0x0f]);
            } else {
                mj_buf_ch(b, (char)c);
            }
            break;
        }
    }
}

/* ---- parser ---- */

typedef struct {
    const char *cur;
    const char *end;
    int err;
} mj_parser;

static void mjp_ws(mj_parser *p) {
    while (!p->err && p->cur < p->end) {
        char c = *p->cur;
        if (c == ' ' || c == '\t' || c == '\n' || c == '\r') {
            p->cur++;
        } else {
            return;
        }
    }
}

static int mjp_expect(mj_parser *p, char c) {
    mjp_ws(p);
    if (p->cur >= p->end || *p->cur != c) {
        p->err = 1;
        return -1;
    }
    p->cur++;
    return 0;
}

static int mjp_literal(mj_parser *p, const char *word) {
    size_t n = strlen(word);
    if ((size_t)(p->end - p->cur) < n || memcmp(p->cur, word, n) != 0) {
        p->err = 1;
        return -1;
    }
    p->cur += n;
    return 0;
}

static mj_value *mjp_value(mj_parser *p);

static mj_value *mjp_new(mj_kind kind) {
    mj_value *v = (mj_value *)calloc(1, sizeof(mj_value));
    if (v) {
        v->kind = kind;
    }
    return v;
}

static char *mjp_string_raw(mj_parser *p, size_t *out_len) {
    mj_buf b;
    if (mjp_expect(p, '"') != 0) {
        return 0;
    }
    mj_buf_init(&b);
    while (!p->err) {
        unsigned char c;
        if (p->cur >= p->end) {
            p->err = 1;
            break;
        }
        c = (unsigned char)*p->cur;
        if (c == '"') {
            p->cur++;
            *out_len = b.len;
            if (b.oom) {
                p->err = 1;
            }
            if (p->err) {
                mj_buf_free(&b);
                return 0;
            }
            return b.data ? b.data : (char *)calloc(1, 1);
        }
        if (c == '\\') {
            p->cur++;
            if (p->cur >= p->end) {
                p->err = 1;
                break;
            }
            c = (unsigned char)*p->cur;
            p->cur++;
            switch (c) {
            case '"':
                mj_buf_ch(&b, '"');
                break;
            case '\\':
                mj_buf_ch(&b, '\\');
                break;
            case '/':
                mj_buf_ch(&b, '/');
                break;
            case 'b':
                mj_buf_ch(&b, '\b');
                break;
            case 'f':
                mj_buf_ch(&b, '\f');
                break;
            case 'n':
                mj_buf_ch(&b, '\n');
                break;
            case 'r':
                mj_buf_ch(&b, '\r');
                break;
            case 't':
                mj_buf_ch(&b, '\t');
                break;
            case 'u': {
                unsigned code = 0;
                int i;
                if (p->end - p->cur < 4) {
                    p->err = 1;
                    break;
                }
                for (i = 0; i < 4; i++) {
                    unsigned char h = (unsigned char)p->cur[i];
                    unsigned d;
                    if (h >= '0' && h <= '9') {
                        d = h - '0';
                    } else if (h >= 'a' && h <= 'f') {
                        d = h - 'a' + 10;
                    } else if (h >= 'A' && h <= 'F') {
                        d = h - 'A' + 10;
                    } else {
                        p->err = 1;
                        break;
                    }
                    code = (code << 4) | d;
                }
                if (p->err) {
                    break;
                }
                p->cur += 4;
                /* Minimal UTF-8 re-encode (sufficient for the sidecars). */
                if (code < 0x80) {
                    mj_buf_ch(&b, (char)code);
                } else if (code < 0x800) {
                    mj_buf_ch(&b, (char)(0xc0 | (code >> 6)));
                    mj_buf_ch(&b, (char)(0x80 | (code & 0x3f)));
                } else {
                    mj_buf_ch(&b, (char)(0xe0 | (code >> 12)));
                    mj_buf_ch(&b, (char)(0x80 | ((code >> 6) & 0x3f)));
                    mj_buf_ch(&b, (char)(0x80 | (code & 0x3f)));
                }
                break;
            }
            default:
                p->err = 1;
                break;
            }
            continue;
        }
        if (c < 0x20) {
            p->err = 1;
            break;
        }
        mj_buf_ch(&b, (char)c);
        p->cur++;
    }
    mj_buf_free(&b);
    return 0;
}

static mj_value *mjp_string(mj_parser *p) {
    mj_value *v = mjp_new(MJ_STRING);
    if (!v) {
        p->err = 1;
        return 0;
    }
    v->as.string.data = mjp_string_raw(p, &v->as.string.len);
    if (p->err) {
        mj_free(v);
        return 0;
    }
    return v;
}

static mj_value *mjp_number(mj_parser *p) {
    const char *start = p->cur;
    mj_value *v;
    while (p->cur < p->end) {
        char c = *p->cur;
        if ((c >= '0' && c <= '9') || c == '-' || c == '+' || c == '.' || c == 'e' || c == 'E') {
            p->cur++;
        } else {
            break;
        }
    }
    if (p->cur == start) {
        p->err = 1;
        return 0;
    }
    v = mjp_new(MJ_NUMBER);
    if (!v) {
        p->err = 1;
        return 0;
    }
    v->as.number.raw = start;
    v->as.number.len = (size_t)(p->cur - start);
    return v;
}

static int mjp_push(mj_value ***items, size_t *len, size_t *cap, mj_value *v) {
    if (*len == *cap) {
        size_t ncap = *cap ? *cap * 2 : 8;
        mj_value **n = (mj_value **)realloc(*items, ncap * sizeof(mj_value *));
        if (!n) {
            return -1;
        }
        *items = n;
        *cap = ncap;
    }
    (*items)[(*len)++] = v;
    return 0;
}

static mj_value *mjp_array(mj_parser *p) {
    mj_value *v = mjp_new(MJ_ARRAY);
    if (!v) {
        p->err = 1;
        return 0;
    }
    if (mjp_expect(p, '[') != 0) {
        mj_free(v);
        return 0;
    }
    mjp_ws(p);
    if (p->cur < p->end && *p->cur == ']') {
        p->cur++;
        return v;
    }
    for (;;) {
        mj_value *item = mjp_value(p);
        if (p->err || mjp_push(&v->as.array.items, &v->as.array.len, &v->as.array.cap, item) != 0) {
            mj_free(item);
            mj_free(v);
            p->err = 1;
            return 0;
        }
        mjp_ws(p);
        if (p->cur < p->end && *p->cur == ',') {
            p->cur++;
            continue;
        }
        if (mjp_expect(p, ']') != 0) {
            mj_free(v);
            return 0;
        }
        return v;
    }
}

static mj_value *mjp_object(mj_parser *p) {
    mj_value *v = mjp_new(MJ_OBJECT);
    if (!v) {
        p->err = 1;
        return 0;
    }
    if (mjp_expect(p, '{') != 0) {
        mj_free(v);
        return 0;
    }
    mjp_ws(p);
    if (p->cur < p->end && *p->cur == '}') {
        p->cur++;
        return v;
    }
    for (;;) {
        size_t key_len = 0;
        char *key;
        mj_value *val;
        mjp_ws(p);
        key = mjp_string_raw(p, &key_len);
        if (p->err) {
            mj_free(v);
            return 0;
        }
        if (mjp_expect(p, ':') != 0) {
            free(key);
            mj_free(v);
            return 0;
        }
        val = mjp_value(p);
        if (p->err) {
            free(key);
            mj_free(v);
            return 0;
        }
        {
            size_t *len = &v->as.object.len;
            size_t *cap = &v->as.object.cap;
            if (*len == *cap) {
                size_t ncap = *cap ? *cap * 2 : 8;
                char **nk = (char **)realloc(v->as.object.keys, ncap * sizeof(char *));
                mj_value **nv;
                if (!nk) {
                    free(key);
                    mj_free(val);
                    mj_free(v);
                    p->err = 1;
                    return 0;
                }
                v->as.object.keys = nk;
                nv = (mj_value **)realloc(v->as.object.values, ncap * sizeof(mj_value *));
                if (!nv) {
                    free(key);
                    mj_free(val);
                    mj_free(v);
                    p->err = 1;
                    return 0;
                }
                v->as.object.values = nv;
                *cap = ncap;
            }
            v->as.object.keys[*len] = key;
            (void)key_len;
            v->as.object.values[*len] = val;
            (*len)++;
        }
        mjp_ws(p);
        if (p->cur < p->end && *p->cur == ',') {
            p->cur++;
            continue;
        }
        if (mjp_expect(p, '}') != 0) {
            mj_free(v);
            return 0;
        }
        return v;
    }
}

static mj_value *mjp_value(mj_parser *p) {
    mjp_ws(p);
    if (p->cur >= p->end) {
        p->err = 1;
        return 0;
    }
    switch (*p->cur) {
    case '{':
        return mjp_object(p);
    case '[':
        return mjp_array(p);
    case '"':
        return mjp_string(p);
    case 't':
        if (mjp_literal(p, "true") == 0) {
            mj_value *v = mjp_new(MJ_BOOL);
            if (v) {
                v->as.boolean = 1;
            }
            return v;
        }
        return 0;
    case 'f':
        if (mjp_literal(p, "false") == 0) {
            return mjp_new(MJ_BOOL);
        }
        return 0;
    case 'n':
        if (mjp_literal(p, "null") == 0) {
            return mjp_new(MJ_NULL);
        }
        return 0;
    default:
        return mjp_number(p);
    }
}

mj_value *mj_parse(const char *text, size_t len) {
    mj_parser p;
    mj_value *v;
    p.cur = text;
    p.end = text + len;
    p.err = 0;
    v = mjp_value(&p);
    if (p.err) {
        mj_free(v);
        return 0;
    }
    mjp_ws(&p);
    if (p.cur != p.end) {
        mj_free(v);
        return 0;
    }
    return v;
}

void mj_free(mj_value *v) {
    size_t i;
    if (!v) {
        return;
    }
    switch (v->kind) {
    case MJ_STRING:
        free(v->as.string.data);
        break;
    case MJ_ARRAY:
        for (i = 0; i < v->as.array.len; i++) {
            mj_free(v->as.array.items[i]);
        }
        free(v->as.array.items);
        break;
    case MJ_OBJECT:
        for (i = 0; i < v->as.object.len; i++) {
            free(v->as.object.keys[i]);
            mj_free(v->as.object.values[i]);
        }
        free(v->as.object.keys);
        free(v->as.object.values);
        break;
    default:
        break;
    }
    free(v);
}

/* ---- accessors ---- */

const mj_value *mj_get(const mj_value *obj, const char *key) {
    size_t i;
    if (!obj || obj->kind != MJ_OBJECT) {
        return 0;
    }
    for (i = 0; i < obj->as.object.len; i++) {
        if (strcmp(obj->as.object.keys[i], key) == 0) {
            return obj->as.object.values[i];
        }
    }
    return 0;
}

size_t mj_len(const mj_value *arr) {
    if (!arr || arr->kind != MJ_ARRAY) {
        return 0;
    }
    return arr->as.array.len;
}

const mj_value *mj_at(const mj_value *arr, size_t index) {
    if (!arr || arr->kind != MJ_ARRAY || index >= arr->as.array.len) {
        return 0;
    }
    return arr->as.array.items[index];
}

int mj_is(const mj_value *v, mj_kind kind) {
    return v && v->kind == kind;
}

const char *mj_str(const mj_value *v) {
    if (!v || v->kind != MJ_STRING) {
        return "";
    }
    return v->as.string.data;
}

size_t mj_str_len(const mj_value *v) {
    if (!v || v->kind != MJ_STRING) {
        return 0;
    }
    return v->as.string.len;
}

const char *mj_num(const mj_value *v) {
    if (!v || v->kind != MJ_NUMBER) {
        return "";
    }
    return v->as.number.raw;
}

/* Text of a number (raw span; stops at the JSON delimiter when scanned) or
 * a string value, for the sidecars' number-or-decimal-string conventions. */
const char *mj_text(const mj_value *v) {
    if (!v) {
        return "";
    }
    if (v->kind == MJ_NUMBER) {
        return v->as.number.raw;
    }
    if (v->kind == MJ_STRING) {
        return v->as.string.data;
    }
    return "";
}

int mj_bool(const mj_value *v) {
    if (!v || v->kind != MJ_BOOL) {
        return 0;
    }
    return v->as.boolean;
}

/* ---- canonical serializer ---- */

static int mj_key_cmp(const void *a, const void *b) {
    const char *const *ka = (const char *const *)a;
    const char *const *kb = (const char *const *)b;
    return strcmp(*ka, *kb);
}

void mj_serialize(const mj_value *v, mj_buf *out) {
    size_t i;
    if (!v) {
        mj_buf_puts(out, "null");
        return;
    }
    switch (v->kind) {
    case MJ_NULL:
        mj_buf_puts(out, "null");
        break;
    case MJ_BOOL:
        mj_buf_puts(out, v->as.boolean ? "true" : "false");
        break;
    case MJ_NUMBER:
        mj_buf_put(out, v->as.number.raw, v->as.number.len);
        break;
    case MJ_STRING:
        mj_buf_ch(out, '"');
        mj_buf_put_escaped(out, v->as.string.data, v->as.string.len);
        mj_buf_ch(out, '"');
        break;
    case MJ_ARRAY:
        mj_buf_ch(out, '[');
        for (i = 0; i < v->as.array.len; i++) {
            if (i) {
                mj_buf_ch(out, ',');
            }
            mj_serialize(v->as.array.items[i], out);
        }
        mj_buf_ch(out, ']');
        break;
    case MJ_OBJECT: {
        size_t n = v->as.object.len;
        const char **order = (const char **)malloc((n ? n : 1) * sizeof(const char *));
        if (!order) {
            out->oom = 1;
            return;
        }
        for (i = 0; i < n; i++) {
            order[i] = v->as.object.keys[i];
        }
        qsort(order, n, sizeof(const char *), mj_key_cmp);
        mj_buf_ch(out, '{');
        for (i = 0; i < n; i++) {
            size_t k;
            if (i) {
                mj_buf_ch(out, ',');
            }
            mj_buf_ch(out, '"');
            mj_buf_put_escaped(out, order[i], strlen(order[i]));
            mj_buf_puts(out, "\":");
            for (k = 0; k < n; k++) {
                if (strcmp(v->as.object.keys[k], order[i]) == 0) {
                    mj_serialize(v->as.object.values[k], out);
                    break;
                }
            }
        }
        free(order);
        mj_buf_ch(out, '}');
        break;
    }
    }
}

const char *mj_serialized(const mj_value *v, mj_buf *scratch) {
    scratch->len = 0;
    if (scratch->data) {
        scratch->data[0] = '\0';
    }
    scratch->oom = 0;
    mj_serialize(v, scratch);
    if (scratch->oom) {
        return 0;
    }
    if (!scratch->data) {
        mj_buf_puts(scratch, "");
    }
    return scratch->data;
}
