/*
 * wallet-c business logic: mirrors wallet-py/procedures.py
 * procedure-for-procedure (create_user, deposit, withdraw, transfer_funds,
 * balance), written against the syscall layer in mudu_sys.c, which frames
 * SyscallPayload v1 (MSSP) messages over the `mududb:api/system` byte pipe
 * with the formal C binding (`mududb/types` + `mududb/codec`).
 * update_profile demonstrates a user-defined record type (wit/types.wit,
 * generated into gentypes/Types.h by mgen) as a procedure parameter and
 * return type, decoded/encoded through the binding's record bridge
 * (`mududb/codec/record_bridge.h`) composed with the generated codecs.
 *
 * Convention (same as every guest): each `// mudu-proc` function reads the
 * bound session OID from `param->session` (injected by the mtp-generated
 * adapter from `UniProcedureParam.session`); the remaining parameters
 * arrive positionally in `param->params`, with arity and `mudu_datum.kind`
 * checked by the generated preamble against the annotation.
 *
 * To add a procedure: write a non-static `mudu_proc_fn`-shaped function
 * with a `// mudu-proc (name: type, ...) -> type` comment immediately
 * above it and rebuild — mtp regenerates the `mp2-*` export wrappers (with
 * their arity/kind check preambles), the world WIT and the procedure
 * descriptor (see readme.md). Annotation types: `i64`, `f64`, `string`,
 * `option<T>` for nullable parameters, and the user-defined record/enum
 * type names registered via `mtp --type-wit wit/types.wit` — the
 * `mudu_datum` support surface of `mudu_sys.h` (`MUDU_DATUM_RECORD` for
 * records, `MUDU_DATUM_I64` ordinals for enums).
 */
#include "mudu_sys.h"

#include <string.h>

#include "gentypes/Types.h"
#include "mududb/codec/record_bridge.h"

static int query_balance(mudu_oid session, int64_t user_id, int64_t *balance, mudu_error *err) {
    mudu_datum args[1];
    mudu_rows rows;
    args[0] = mudu_i64(user_id);
    if (mudu_query(session, "SELECT balance FROM wallets WHERE user_id = ?", args, 1, &rows, err)
        != 0) {
        return -1;
    }
    if (rows.n_rows == 0 || rows.rows[0].n_fields == 0) {
        mudu_error_set(err, MUDU_EC_ENTITY_NOT_FOUND, "wallet not found");
        return -1;
    }
    if (rows.rows[0].fields[0].kind != MUDU_DATUM_I64) {
        mudu_error_set(err, MUDU_EC_INTERNAL, "wallet balance is not an i64");
        return -1;
    }
    *balance = rows.rows[0].fields[0].i64;
    return 0;
}

static int set_balance(mudu_oid session, int64_t user_id, int64_t balance, int64_t *out,
                       mudu_error *err) {
    mudu_datum args[2];
    uint64_t affected = 0;
    args[0] = mudu_i64(balance);
    args[1] = mudu_i64(user_id);
    if (mudu_command(session, "UPDATE wallets SET balance = ? WHERE user_id = ?", args, 2,
                     &affected, err)
        != 0) {
        return -1;
    }
    if (affected != 1) {
        mudu_error_set(err, MUDU_EC_DOMAIN_VIOLATION, "wallet update failed");
        return -1;
    }
    *out = balance;
    return 0;
}

// mudu-proc (user_id: i64, name: string, email: string) -> i64
int create_user(const mudu_proc_param *param, mudu_datum *result, mudu_error *err) {
    int64_t user_id = param->params[0].i64;
    mudu_datum user_args[5];
    mudu_datum wallet_args[3];
    uint64_t affected = 0;

    user_args[0] = mudu_i64(user_id);
    user_args[1] = param->params[1];
    user_args[2] = param->params[2];
    user_args[3] = mudu_i64(0);
    user_args[4] = mudu_i64(0);
    if (mudu_command(param->session,
                     "INSERT INTO users (user_id, name, email, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
                     user_args, 5, &affected, err)
        != 0) {
        return -1;
    }
    if (affected != 1) {
        mudu_error_set(err, MUDU_EC_DOMAIN_VIOLATION, "create user failed");
        return -1;
    }

    wallet_args[0] = mudu_i64(user_id);
    wallet_args[1] = mudu_i64(0);
    wallet_args[2] = mudu_i64(0);
    if (mudu_command(param->session,
                     "INSERT INTO wallets (user_id, balance, updated_at) VALUES (?, ?, ?)",
                     wallet_args, 3, &affected, err)
        != 0) {
        return -1;
    }
    if (affected != 1) {
        mudu_error_set(err, MUDU_EC_DOMAIN_VIOLATION, "create wallet failed");
        return -1;
    }
    *result = mudu_i64(user_id);
    return 0;
}

// mudu-proc (user_id: i64, amount: i64) -> i64
int deposit(const mudu_proc_param *param, mudu_datum *result, mudu_error *err) {
    int64_t user_id = param->params[0].i64;
    int64_t amount = param->params[1].i64;
    int64_t balance = 0;
    if (amount <= 0) {
        mudu_error_set(err, MUDU_EC_DOMAIN_VIOLATION, "amount must be positive");
        return -1;
    }
    if (query_balance(param->session, user_id, &balance, err) != 0) {
        return -1;
    }
    if (set_balance(param->session, user_id, balance + amount, &balance, err) != 0) {
        return -1;
    }
    *result = mudu_i64(balance);
    return 0;
}

// mudu-proc (user_id: i64, amount: i64) -> i64
int withdraw(const mudu_proc_param *param, mudu_datum *result, mudu_error *err) {
    int64_t user_id = param->params[0].i64;
    int64_t amount = param->params[1].i64;
    int64_t current = 0;
    if (amount <= 0) {
        mudu_error_set(err, MUDU_EC_DOMAIN_VIOLATION, "amount must be positive");
        return -1;
    }
    if (query_balance(param->session, user_id, &current, err) != 0) {
        return -1;
    }
    if (current < amount) {
        mudu_error_set(err, MUDU_EC_DOMAIN_VIOLATION, "insufficient funds");
        return -1;
    }
    if (set_balance(param->session, user_id, current - amount, &current, err) != 0) {
        return -1;
    }
    *result = mudu_i64(current);
    return 0;
}

// mudu-proc (from_user_id: i64, to_user_id: i64, amount: i64) -> i64
int transfer_funds(const mudu_proc_param *param, mudu_datum *result, mudu_error *err) {
    int64_t from_user_id = param->params[0].i64;
    int64_t to_user_id = param->params[1].i64;
    int64_t amount = param->params[2].i64;
    int64_t current_from = 0;
    int64_t current_to = 0;
    int64_t new_from = 0;
    int64_t new_to = 0;
    if (amount <= 0) {
        mudu_error_set(err, MUDU_EC_DOMAIN_VIOLATION, "amount must be positive");
        return -1;
    }
    if (from_user_id == to_user_id) {
        mudu_error_set(err, MUDU_EC_DOMAIN_VIOLATION, "cannot transfer to self");
        return -1;
    }
    if (query_balance(param->session, from_user_id, &current_from, err) != 0) {
        return -1;
    }
    if (query_balance(param->session, to_user_id, &current_to, err) != 0) {
        return -1;
    }
    if (current_from < amount) {
        mudu_error_set(err, MUDU_EC_DOMAIN_VIOLATION, "insufficient funds");
        return -1;
    }

    if (set_balance(param->session, from_user_id, current_from - amount, &new_from, err) != 0) {
        return -1;
    }
    if (set_balance(param->session, to_user_id, current_to + amount, &new_to, err) != 0) {
        return -1;
    }
    *result = mudu_i64(new_from);
    return 0;
}

// mudu-proc (user_id: i64) -> i64
int balance(const mudu_proc_param *param, mudu_datum *result, mudu_error *err) {
    int64_t value = 0;
    if (query_balance(param->session, param->params[0].i64, &value, err) != 0) {
        return -1;
    }
    *result = mudu_i64(value);
    return 0;
}

/* ---- update_profile: a user-defined record type end to end ---- */

/* Store the decoded profile record: the record flattens into the `profile`
 * table columns (bool/u32 as INT — the host carries both as i32); the
 * nested optional address flattens into the nullable home_city/home_zip
 * columns (both NULL when the option is absent); the tag list lands in the
 * `profile_tags` side table keyed by (user_id, idx). */
static int store_profile(mudu_oid session, int64_t user_id, const profile *p, mudu_error *err) {
    mudu_datum args[6];
    mudu_rows old_tags;
    uint64_t affected = 0;
    uint32_t i;
    args[0] = mudu_str(p->display_name.data, p->display_name.len);
    args[1] = mudu_i64((int64_t)p->level);
    args[2] = mudu_i64(p->vip ? 1 : 0);
    if (p->home_is_null) {
        args[3] = mudu_null();
        args[4] = mudu_null();
    } else {
        args[3] = mudu_str(p->home.city.data, p->home.city.len);
        args[4] = mudu_str(p->home.zip.data, p->home.zip.len);
    }
    args[5] = mudu_i64(user_id);
    if (mudu_command(session,
                     "UPDATE profile SET display_name = ?, level = ?, vip = ?, home_city = ?, home_zip = ? WHERE user_id = ?",
                     args, 6, &affected, err)
        != 0) {
        return -1;
    }
    if (affected == 0) {
        args[0] = mudu_i64(user_id);
        args[1] = mudu_str(p->display_name.data, p->display_name.len);
        args[2] = mudu_i64((int64_t)p->level);
        args[3] = mudu_i64(p->vip ? 1 : 0);
        if (p->home_is_null) {
            args[4] = mudu_null();
            args[5] = mudu_null();
        } else {
            args[4] = mudu_str(p->home.city.data, p->home.city.len);
            args[5] = mudu_str(p->home.zip.data, p->home.zip.len);
        }
        if (mudu_command(session,
                         "INSERT INTO profile (user_id, display_name, level, vip, home_city, home_zip) VALUES (?, ?, ?, ?, ?, ?)",
                         args, 6, &affected, err)
            != 0) {
            return -1;
        }
        if (affected != 1) {
            mudu_error_set(err, MUDU_EC_DOMAIN_VIOLATION, "insert profile failed");
            return -1;
        }
    }
    /* The engine requires a complete primary key for DELETE, so the old
     * tags are first read by the key-prefix predicate and then deleted one
     * by one. */
    args[0] = mudu_i64(user_id);
    if (mudu_query(session, "SELECT idx, tag FROM profile_tags WHERE user_id = ?", args, 1,
                   &old_tags, err)
        != 0) {
        return -1;
    }
    for (i = 0; i < old_tags.n_rows; i++) {
        mudu_datum del_args[2];
        if (old_tags.rows[i].n_fields == 0
            || old_tags.rows[i].fields[0].kind != MUDU_DATUM_I64) {
            mudu_error_set(err, MUDU_EC_INTERNAL, "profile tag idx is not an i64");
            return -1;
        }
        del_args[0] = mudu_i64(user_id);
        del_args[1] = old_tags.rows[i].fields[0];
        if (mudu_command(session, "DELETE FROM profile_tags WHERE user_id = ? AND idx = ?",
                         del_args, 2, &affected, err)
            != 0) {
            return -1;
        }
        if (affected != 1) {
            mudu_error_set(err, MUDU_EC_DOMAIN_VIOLATION, "delete profile tag failed");
            return -1;
        }
    }
    for (i = 0; i < p->tags.len; i++) {
        mudu_datum ins_args[3];
        ins_args[0] = mudu_i64(user_id);
        ins_args[1] = mudu_i64((int64_t)i);
        ins_args[2] = mudu_str(p->tags.items[i].data, p->tags.items[i].len);
        if (mudu_command(session, "INSERT INTO profile_tags (user_id, idx, tag) VALUES (?, ?, ?)",
                         ins_args, 3, &affected, err)
            != 0) {
            return -1;
        }
        if (affected != 1) {
            mudu_error_set(err, MUDU_EC_DOMAIN_VIOLATION, "insert profile tag failed");
            return -1;
        }
    }
    return 0;
}

/* The engine has no ORDER BY: insertion-sort the side-table rows in place
 * by their i64 key field. */
static void sort_rows_by_i64_key(mudu_rows *rows, uint32_t key_field) {
    uint32_t i;
    for (i = 1; i < rows->n_rows; i++) {
        mudu_row key = rows->rows[i];
        int64_t key_value = key.fields[key_field].i64;
        uint32_t j = i;
        while (j > 0 && rows->rows[j - 1].fields[key_field].i64 > key_value) {
            rows->rows[j] = rows->rows[j - 1];
            j--;
        }
        rows->rows[j] = key;
    }
}

/* Rebuild the stored profile record: flat columns, the nullable address
 * columns, and the side-table tags re-sorted by idx. The slices the record
 * borrows point into the query decode arena, which lives in the guest arena
 * until the call ends — the same lifetime the result encode needs. */
static int query_profile(mudu_oid session, int64_t user_id, profile *out, mudu_error *err) {
    mudu_datum args[1];
    mudu_rows rows;
    mudu_rows tag_rows;
    uint32_t i;
    memset(out, 0, sizeof(*out));
    out->home_is_null = 1;
    args[0] = mudu_i64(user_id);
    if (mudu_query(session,
                   "SELECT display_name, level, vip, home_city, home_zip FROM profile WHERE user_id = ?",
                   args, 1, &rows, err)
        != 0) {
        return -1;
    }
    if (rows.n_rows == 0) {
        mudu_error_set(err, MUDU_EC_ENTITY_NOT_FOUND, "profile not found");
        return -1;
    }
    if (rows.rows[0].n_fields != 5) {
        mudu_error_set(err, MUDU_EC_INTERNAL, "profile row shape mismatch");
        return -1;
    }
    {
        mudu_datum *f = rows.rows[0].fields;
        if (f[0].kind != MUDU_DATUM_STR) {
            mudu_error_set(err, MUDU_EC_INTERNAL, "profile display_name is not a string");
            return -1;
        }
        if (f[1].kind != MUDU_DATUM_I64 || f[1].i64 < 0 || f[1].i64 > 4294967295LL) {
            mudu_error_set(err, MUDU_EC_INTERNAL, "profile level is not a u32");
            return -1;
        }
        if (f[2].kind != MUDU_DATUM_I64) {
            mudu_error_set(err, MUDU_EC_INTERNAL, "profile vip is not an i64");
            return -1;
        }
        out->display_name = mp_str_from(f[0].str, f[0].str_len);
        out->level = (uint32_t)f[1].i64;
        out->vip = f[2].i64 != 0;
        if (f[3].kind == MUDU_DATUM_STR && f[4].kind == MUDU_DATUM_STR) {
            out->home_is_null = 0;
            out->home.city = mp_str_from(f[3].str, f[3].str_len);
            out->home.zip = mp_str_from(f[4].str, f[4].str_len);
        } else if (f[3].kind != MUDU_DATUM_NULL || f[4].kind != MUDU_DATUM_NULL) {
            mudu_error_set(err, MUDU_EC_INTERNAL, "profile address columns are not strings");
            return -1;
        }
    }

    args[0] = mudu_i64(user_id);
    if (mudu_query(session, "SELECT idx, tag FROM profile_tags WHERE user_id = ?", args, 1,
                   &tag_rows, err)
        != 0) {
        return -1;
    }
    for (i = 0; i < tag_rows.n_rows; i++) {
        if (tag_rows.rows[i].n_fields != 2
            || tag_rows.rows[i].fields[0].kind != MUDU_DATUM_I64
            || tag_rows.rows[i].fields[1].kind != MUDU_DATUM_STR) {
            mudu_error_set(err, MUDU_EC_INTERNAL, "profile tag row shape mismatch");
            return -1;
        }
    }
    sort_rows_by_i64_key(&tag_rows, 0);
    if (tag_rows.n_rows) {
        mp_str *items =
            (mp_str *)mudu_arena_alloc((size_t)tag_rows.n_rows * sizeof(mp_str), 8);
        if (!items) {
            mudu_error_set(err, MUDU_EC_INTERNAL, "out of memory");
            return -1;
        }
        for (i = 0; i < tag_rows.n_rows; i++) {
            items[i] = mp_str_from(tag_rows.rows[i].fields[1].str,
                                   tag_rows.rows[i].fields[1].str_len);
        }
        out->tags.items = items;
        out->tags.len = tag_rows.n_rows;
    }
    return 0;
}

/* update_profile stores a user-defined record argument and returns the
 * stored record. The `profile` record type is declared in wit/types.wit and
 * generated into gentypes/Types.h by mgen; the procedure decodes the
 * positional record envelope through the binding record bridge
 * (mp_record_bridge_write) composed with the generated profile_decode, and
 * encodes the returned record symmetrically (profile_encode +
 * mp_record_bridge_read). See store_profile for the storage design. */
// mudu-proc (user_id: i64, profile: Profile) -> Profile
int update_profile(const mudu_proc_param *param, mudu_datum *result, mudu_error *err) {
    int64_t user_id = param->params[0].i64;
    profile p;
    profile stored;
    /* decode the record envelope: the bridge serializes it as the
     * integer-keyed MessagePack map the generated profile_decode reads */
    {
        mp_writer w;
        mp_reader r;
        mp_arena a;
        mpw_init(&w);
        if (mp_record_bridge_write(param->params[1].record, &w) != 0) {
            mudu_error_set(err, MUDU_EC_INVALID_ARGUMENT, "profile record bridge failed");
            return -1;
        }
        mpr_init(&r, w.buf, (size_t)w.len);
        mpa_init(&a);
        if (profile_decode(&r, &a, &p) != 0 || !mpr_done(&r)) {
            mudu_error_set(err, MUDU_EC_INVALID_ARGUMENT, "profile record decode failed");
            return -1;
        }
        mpw_free(&w);
    }
    if (store_profile(param->session, user_id, &p, err) != 0) {
        return -1;
    }
    if (query_profile(param->session, user_id, &stored, err) != 0) {
        return -1;
    }
    /* encode the stored record symmetrically: profile_encode renders the
     * integer-keyed map and the bridge wraps it back into the record-case
     * envelope the host expects (arena-owned — outlives this frame, which
     * is what the deferred result encode in mudu_run_proc needs) */
    {
        mp_writer w;
        mp_reader r;
        mp_arena a;
        uni_data_value *envelope;
        mpw_init(&w);
        profile_encode(&w, &stored);
        if (!mpw_ok(&w)) {
            mudu_error_set(err, MUDU_EC_INTERNAL, "profile encode failed");
            return -1;
        }
        mpr_init(&r, w.buf, (size_t)w.len);
        mpa_init(&a);
        envelope = (uni_data_value *)mpa_alloc(&a, sizeof(uni_data_value));
        if (!envelope) {
            mudu_error_set(err, MUDU_EC_INTERNAL, "out of memory");
            return -1;
        }
        if (mp_record_bridge_read(&r, &a, envelope) != 0 || !mpr_done(&r)) {
            mudu_error_set(err, MUDU_EC_INTERNAL, "profile record bridge read failed");
            return -1;
        }
        mpw_free(&w);
        *result = mudu_record(envelope);
    }
    return 0;
}
