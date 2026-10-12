"""Wallet business functions shared by MPK and native adapter entry points."""

from mududb.db import Database
from mududb.generated.uni_oid import UniOid
from mududb.result import as_i64
from mududb.sql import Params, SqlStmt


def _command(db: Database, sql: str, values: Params | None = None) -> int:
    return db.command(SqlStmt(sql), values)


def _query_balance(db: Database, user_id: int) -> int:
    rows = db.query(
        SqlStmt("SELECT balance FROM wallets WHERE user_id = ?"),
        Params().bind(0, user_id),
    )
    if not rows.next():
        raise Exception("wallet not found")
    return as_i64(rows.current_row().value_by_name("balance"))


def _set_balance(db: Database, user_id: int, balance: int) -> int:
    updated = _command(
        db,
        "UPDATE wallets SET balance = ? WHERE user_id = ?",
        Params().bind(0, balance).bind(1, user_id),
    )
    if updated != 1:
        raise Exception("wallet update failed")
    return balance


# mudu-proc
def create_user(session: UniOid, user_id: int, name: str, email: str) -> int:
    db = Database(session)
    users = _command(
        db,
        "INSERT INTO users (user_id, name, email, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
        Params().bind(0, user_id).bind(1, name).bind(2, email).bind(3, 0).bind(4, 0),
    )
    if users != 1:
        raise Exception("create user failed")

    wallets = _command(
        db,
        "INSERT INTO wallets (user_id, balance, updated_at) VALUES (?, ?, ?)",
        Params().bind(0, user_id).bind(1, 0).bind(2, 0),
    )
    if wallets != 1:
        raise Exception("create wallet failed")
    return user_id


# mudu-proc
def deposit(session: UniOid, user_id: int, amount: int) -> int:
    if amount <= 0:
        raise Exception("amount must be positive")
    db = Database(session)
    balance = _query_balance(db, user_id)
    return _set_balance(db, user_id, balance + amount)


# mudu-proc
def withdraw(session: UniOid, user_id: int, amount: int) -> int:
    if amount <= 0:
        raise Exception("amount must be positive")
    db = Database(session)
    current = _query_balance(db, user_id)
    if current < amount:
        raise Exception("insufficient funds")
    return _set_balance(db, user_id, current - amount)


# mudu-proc
def transfer_funds(session: UniOid, from_user_id: int, to_user_id: int, amount: int) -> int:
    if amount <= 0:
        raise Exception("amount must be positive")
    if from_user_id == to_user_id:
        raise Exception("cannot transfer to self")
    db = Database(session)
    current_from = _query_balance(db, from_user_id)
    current_to = _query_balance(db, to_user_id)
    if current_from < amount:
        raise Exception("insufficient funds")

    new_from = _set_balance(db, from_user_id, current_from - amount)
    _set_balance(db, to_user_id, current_to + amount)
    return new_from


# mudu-proc
def balance(session: UniOid, user_id: int) -> int:
    db = Database(session)
    return _query_balance(db, user_id)

