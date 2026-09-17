"""wallet-py procedures: the wallet example business logic, mirroring
wallet-as/assembly/procedures.ts, written against the canonical Python
facade (`mududb.db` / `mududb.sql`). update_profile demonstrates a
user-defined record type (wit/types.wit, generated into `gentypes.py` by
mgen) as a procedure parameter and return type.

Convention (same as the AssemblyScript guest): the first parameter of each
`# mudu-proc` function is the bound session `UniOid`, injected by the
mtp-generated adapter from `UniProcedureParam.session`; the remaining
parameters arrive positionally in `param_list`.
"""

from gentypes import Address, Profile
from mududb.db import Database
from mududb.generated.uni_oid import UniOid
from mududb.result import as_i64, as_string
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


# update_profile stores a user-defined record argument and returns the
# stored record. The `profile` record type is declared in wit/types.wit and
# generated into `gentypes.py` by mgen; the mtp adapter decodes the
# positional record envelope through the binding record bridge
# (mududb.codec.bridge.record_field_values) composed with
# gentypes.profile_from_value, and encodes the returned record
# symmetrically.
#
# Storage design: the record flattens into the `profile` table columns
# (bool/u32 as INT — the host carries both as i32); the nested optional
# address flattens into the nullable home_city/home_zip columns (both NULL
# when the option is absent); the tag list lands in the `profile_tags`
# side table keyed by (user_id, idx) — the engine requires a complete key
# for DELETE and has no ORDER BY, so _query_profile re-sorts by idx.
#
# mudu-proc
def update_profile(session: UniOid, user_id: int, profile: Profile) -> Profile:
    db = Database(session)
    vip = 1 if profile.vip else 0
    home_city = profile.home.city if profile.home is not None else None
    home_zip = profile.home.zip if profile.home is not None else None
    updated = _command(
        db,
        "UPDATE profile SET display_name = ?, level = ?, vip = ?, home_city = ?, home_zip = ? WHERE user_id = ?",
        Params()
        .bind(0, profile.display_name)
        .bind(1, profile.level)
        .bind(2, vip)
        .bind(3, home_city)
        .bind(4, home_zip)
        .bind(5, user_id),
    )
    if updated == 0:
        inserted = _command(
            db,
            "INSERT INTO profile (user_id, display_name, level, vip, home_city, home_zip) VALUES (?, ?, ?, ?, ?, ?)",
            Params()
            .bind(0, user_id)
            .bind(1, profile.display_name)
            .bind(2, profile.level)
            .bind(3, vip)
            .bind(4, home_city)
            .bind(5, home_zip),
        )
        if inserted != 1:
            raise Exception("insert profile failed")
    # The engine requires a complete primary key for DELETE, so the old
    # tags are first read by the key-prefix predicate and then deleted one
    # by one.
    old_idx = []
    old_tags = db.query(
        SqlStmt("SELECT idx, tag FROM profile_tags WHERE user_id = ?"),
        Params().bind(0, user_id),
    )
    while old_tags.next():
        old_idx.append(as_i64(old_tags.current_row().value_by_name("idx")))
    for idx in old_idx:
        deleted = _command(
            db,
            "DELETE FROM profile_tags WHERE user_id = ? AND idx = ?",
            Params().bind(0, user_id).bind(1, idx),
        )
        if deleted != 1:
            raise Exception("delete profile tag failed")
    for idx, tag in enumerate(profile.tags):
        inserted = _command(
            db,
            "INSERT INTO profile_tags (user_id, idx, tag) VALUES (?, ?, ?)",
            Params().bind(0, user_id).bind(1, idx).bind(2, tag),
        )
        if inserted != 1:
            raise Exception("insert profile tag failed")
    return _query_profile(db, user_id)


# _query_profile rebuilds the stored profile record: flat columns, the
# nullable address columns, and the side-table tags re-sorted by idx.
def _query_profile(db: Database, user_id: int) -> Profile:
    rows = db.query(
        SqlStmt(
            "SELECT display_name, level, vip, home_city, home_zip FROM profile WHERE user_id = ?"
        ),
        Params().bind(0, user_id),
    )
    if not rows.next():
        raise Exception("profile not found")
    row = rows.current_row()
    home = None
    if not row.is_null_by_name("home_city") and not row.is_null_by_name("home_zip"):
        home = Address(
            city=as_string(row.value_by_name("home_city")),
            zip=as_string(row.value_by_name("home_zip")),
        )
    indexed = []
    tag_rows = db.query(
        SqlStmt("SELECT idx, tag FROM profile_tags WHERE user_id = ?"),
        Params().bind(0, user_id),
    )
    while tag_rows.next():
        tag_row = tag_rows.current_row()
        indexed.append(
            (
                as_i64(tag_row.value_by_name("idx")),
                as_string(tag_row.value_by_name("tag")),
            )
        )
    indexed.sort(key=lambda item: item[0])
    return Profile(
        display_name=as_string(row.value_by_name("display_name")),
        level=as_i64(row.value_by_name("level")),
        vip=as_i64(row.value_by_name("vip")) != 0,
        tags=[tag for _, tag in indexed],
        home=home,
    )
