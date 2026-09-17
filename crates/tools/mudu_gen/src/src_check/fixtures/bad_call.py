# Negative fixture: arity mismatch and unknown table.
from mududb.db import Database
from mududb.sql import Params, SqlStmt


def get_balance(db: Database, user_id: int) -> int:
    rows = db.query(
        SqlStmt("SELECT balance FROM wallets WHERE user_id = ?"),
        Params().bind(0, user_id).bind(1, 0),
    )
    return 0


def delete_wallet(db: Database, user_id: int) -> int:
    return db.command(
        SqlStmt("DELETE FROM wallet WHERE user_id = ?"),
        Params().bind(0, user_id),
    )
