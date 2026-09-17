/* Negative fixture: arity mismatch and unknown table. */
#include "mudu_sys.h"

static int get_balance(mudu_oid session, mudu_datum *args, mudu_rows *rows, mudu_error *err) {
    return mudu_query(session, "SELECT balance FROM wallets WHERE user_id = ?", args, 2,
                      rows, err);
}

static int delete_wallet(mudu_oid session, mudu_datum *args, mudu_rows *rows, mudu_error *err) {
    return mudu_command(session, "DELETE FROM wallet WHERE user_id = ?", args, 1, rows, err);
}
