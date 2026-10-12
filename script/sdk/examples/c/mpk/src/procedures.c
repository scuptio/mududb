/* Wallet business functions shared by MPK and native adapter entry points. */
#include "mudu_sys.h"

#include <string.h>


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
