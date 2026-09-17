# Shared helpers for the corpus runners: fixture location, segment
# unpacking, sidecar `expect` shape construction through the generated
# codec, and the deterministic encode inputs pinned against the fixture
# frames. Imported by run_mp_corpus_test.py, run_syscall_corpus_test.py and
# run_lenient_decode_test.py; not part of the package's public surface.
import json
import os
import sys

_PACKAGE_ROOT = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, _PACKAGE_ROOT)

from mududb.generated.uni_command_argv import UniCommandArgv
from mududb.generated.uni_command_result import UniCommandResult
from mududb.generated.uni_data_type import UniDataTypeKind, UniDataTypeScalar
from mududb.generated.uni_error import UniError
from mududb.generated.uni_fs_dirent import UniFsDirent
from mududb.generated.uni_fs_open_argv import UniFsOpenArgv
from mududb.generated.uni_fs_stat import UniFsStat
from mududb.generated.uni_oid import UniOid
from mududb.generated.uni_query_argv import UniQueryArgv
from mududb.generated.uni_query_result import UniQueryResult
from mududb.generated.uni_scalar import UniScalar
from mududb.generated.uni_sql_param import UniSqlParam
from mududb.generated.uni_sql_stmt import UniSqlStmt
from mududb.generated.uni_syscall import (
    MessageKind,
    WireResult,
    decode_batch_request,
    decode_batch_result,
    decode_close_session_request,
    decode_close_session_result,
    decode_command_request,
    decode_command_result,
    decode_delete_request,
    decode_delete_result,
    decode_fs_close_request,
    decode_fs_close_result,
    decode_fs_fstat_request,
    decode_fs_fstat_result,
    decode_fs_fsync_request,
    decode_fs_fsync_result,
    decode_fs_lseek_request,
    decode_fs_lseek_result,
    decode_fs_open_request,
    decode_fs_open_result,
    decode_fs_pread_request,
    decode_fs_pread_result,
    decode_fs_pwrite_request,
    decode_fs_pwrite_result,
    decode_fs_read_request,
    decode_fs_read_result,
    decode_fs_readdir_request,
    decode_fs_readdir_result,
    decode_fs_stat_request,
    decode_fs_stat_result,
    decode_fs_write_request,
    decode_fs_write_result,
    decode_get_request,
    decode_get_result,
    decode_header,
    decode_open_session_request,
    decode_open_session_result,
    decode_put_request,
    decode_put_result,
    decode_query_request,
    decode_query_result,
    decode_range_request,
    decode_range_result,
    decode_relation_get_request,
    decode_relation_get_result,
    decode_relation_insert_request,
    decode_relation_insert_result,
    decode_relation_update_request,
    decode_relation_update_result,
    encode_batch_request,
    encode_batch_result,
    encode_close_session_request,
    encode_close_session_result,
    encode_command_request,
    encode_command_result,
    encode_delete_request,
    encode_delete_result,
    encode_fs_close_request,
    encode_fs_close_result,
    encode_fs_fstat_request,
    encode_fs_fstat_result,
    encode_fs_fsync_request,
    encode_fs_fsync_result,
    encode_fs_lseek_request,
    encode_fs_lseek_result,
    encode_fs_open_request,
    encode_fs_open_result,
    encode_fs_pread_request,
    encode_fs_pread_result,
    encode_fs_pwrite_request,
    encode_fs_pwrite_result,
    encode_fs_read_request,
    encode_fs_read_result,
    encode_fs_readdir_request,
    encode_fs_readdir_result,
    encode_fs_stat_request,
    encode_fs_stat_result,
    encode_fs_write_request,
    encode_fs_write_result,
    encode_get_request,
    encode_get_result,
    encode_open_session_request,
    encode_open_session_result,
    encode_put_request,
    encode_put_result,
    encode_query_request,
    encode_query_result,
    encode_range_request,
    encode_range_result,
    encode_relation_get_request,
    encode_relation_get_result,
    encode_relation_insert_request,
    encode_relation_insert_result,
    encode_relation_update_request,
    encode_relation_update_result,
)

# ---- fixture location (walk up to the repo's golden fixture dir; the C#
# FixturePath approach — no absolute paths hardcoded) ----


def find_fixture_dir():
    rel = os.path.join("crates", "db-kernel", "testing", "fixtures", "golden", "v1")
    d = _PACKAGE_ROOT
    while True:
        candidate = os.path.join(d, rel)
        if os.path.isdir(candidate):
            return candidate
        parent = os.path.dirname(d)
        if parent == d:
            raise FileNotFoundError(
                f"could not locate {rel} by walking up from {_PACKAGE_ROOT}"
            )
        d = parent


def load_fixture(fixture_dir, stem):
    """Return (segments, sidecar) for a fixture pair <stem>.bin / <stem>.json."""
    with open(os.path.join(fixture_dir, stem + ".bin"), "rb") as f:
        corpus = f.read()
    with open(os.path.join(fixture_dir, stem + ".json"), "r", encoding="utf-8") as f:
        sidecar = json.load(f)
    return unpack_segments(corpus), sidecar


def unpack_segments(data):
    """Split the length-prefixed container (big-endian u32 length per segment)."""
    segments = []
    offset = 0
    while offset < len(data):
        length = int.from_bytes(data[offset : offset + 4], "big")
        offset += 4
        segments.append(data[offset : offset + length])
        offset += length
    return segments


def diff_detail(actual, expected):
    """First-difference description for two byte strings."""
    n = min(len(actual), len(expected))
    at = -1
    for i in range(n):
        if actual[i] != expected[i]:
            at = i
            break
    if at == -1 and len(actual) != len(expected):
        at = n
    if at < 0:
        return ""
    a = f"0x{actual[at]:02x}" if at < len(actual) else "eof"
    b = f"0x{expected[at]:02x}" if at < len(expected) else "eof"
    return f"len {len(actual)} vs {len(expected)}, first diff at {at} ({a} vs {b})"


# ---- sidecar `expect` shape construction (conventions: u64/i64/u128 as
# decimal strings, bytes as lowercase hex, UniOid as {"h","l"} decimal
# strings, unit as {"unit": true}, relation cells as hex-or-null) ----


def oid_json(o):
    return {"h": str(o.h), "l": str(o.l)}


def attr_datum_json(attr, datum):
    return {"attr": str(attr), "datum_hex": datum.hex()}


_SCALAR_NAMES = {
    UniScalar.BOOL: "bool",
    UniScalar.U8: "u8",
    UniScalar.I8: "i8",
    UniScalar.U16: "u16",
    UniScalar.I16: "i16",
    UniScalar.U32: "u32",
    UniScalar.I32: "i32",
    UniScalar.U64: "u64",
    UniScalar.U128: "u128",
    UniScalar.I64: "i64",
    UniScalar.I128: "i128",
    UniScalar.F32: "f32",
    UniScalar.F64: "f64",
    UniScalar.CHAR: "char",
    UniScalar.STRING: "string",
    UniScalar.BLOB: "blob",
    UniScalar.NUMERIC: "numeric",
    UniScalar.DATE: "date",
    UniScalar.TIME: "time",
    UniScalar.TIMESTAMP: "timestamp",
    UniScalar.TIMESTAMP_TZ: "timestamp_tz",
}


def data_type_name(t):
    if t.kind == UniDataTypeKind.SCALAR:
        # A UniDataType left at its proto3-style default (an absent variant
        # field) renders as scalar(bool), matching the host.
        s = t.inner if isinstance(t, UniDataTypeScalar) else UniScalar.BOOL
        return "scalar(" + _SCALAR_NAMES[s] + ")"
    raise ValueError("field_type serialization not covered by the sidecar conventions")


def record_field_json(f):
    return {
        "field_name": f.field_name,
        "field_type": data_type_name(f.field_type),
        "field_attrs": [
            {"attr_name": a.attr_name, "attr_value": a.attr_value} for a in f.field_attrs
        ],
    }


def sql_params_json(params):
    # The sidecar conventions do not define a rendering for UniDataValue
    # params; every corpus/lenient frame carries an empty parameter list.
    if len(params.params) != 0:
        raise ValueError("sql params serialization not covered by the sidecar conventions")
    return []


def sql_argv_json(oid, sql, params):
    return {"oid": oid_json(oid), "sql": sql, "params": sql_params_json(params)}


def query_result_json(r):
    # The sidecar conventions do not define a rendering for query rows; every
    # corpus/lenient frame carries an empty row set.
    if len(r.result_set.row_set) != 0:
        raise ValueError("query row serialization not covered by the sidecar conventions")
    return {
        "record_name": r.tuple_desc.record_name,
        "fields": [record_field_json(f) for f in r.tuple_desc.record_fields],
        "eof": r.result_set.eof,
        "rows": [],
        "cursor_hex": r.result_set.cursor.hex(),
    }


def fs_stat_json(s):
    return {
        "oid": oid_json(s.oid),
        "generation": str(s.generation),
        "entry": s.entry,
        "length": str(s.length),
        "state": s.state,
    }


def dirents_json(entries):
    return [
        {"name": d.name, "is_dir": d.is_dir, "length": str(d.length)} for d in entries
    ]


def relation_row_json(row):
    if row is None:
        return None
    return [None if cell is None else cell.hex() for cell in row]


def err_json(err):
    return {
        "err_code": err.err_code,
        "err_msg": err.err_msg,
        "err_src": err.err_src,
        "err_loc": err.err_loc,
        "err_details_hex": err.err_details.hex(),
    }


_UNIT_JSON = {"unit": True}

# ---- decode a frame and render the decoded values in the sidecar expect
# shape (the sidecar is the single source of truth; the runners compare with
# == against frame["expect"]) ----


def _expect_ok(r, what):
    if not r.is_ok:
        raise ValueError(f"expected ok {what} result, got err {err_json(r.error)}")
    return r.value


def decode_request_expect(kind, frame):
    k = MessageKind(kind)
    if k == MessageKind.QUERY:
        v = decode_query_request(frame)
        return sql_argv_json(v.oid, v.query.sql_string, v.param_list)
    if k == MessageKind.COMMAND:
        v = decode_command_request(frame)
        return sql_argv_json(v.oid, v.command.sql_string, v.param_list)
    if k == MessageKind.BATCH:
        v = decode_batch_request(frame)
        return sql_argv_json(v.oid, v.command.sql_string, v.param_list)
    if k == MessageKind.OPEN_SESSION:
        return {"worker_oid": oid_json(decode_open_session_request(frame))}
    if k == MessageKind.CLOSE_SESSION:
        return {"oid": oid_json(decode_close_session_request(frame))}
    if k == MessageKind.GET:
        oid, key = decode_get_request(frame)
        return {"oid": oid_json(oid), "key_hex": key.hex()}
    if k == MessageKind.PUT:
        oid, key, value = decode_put_request(frame)
        return {"oid": oid_json(oid), "key_hex": key.hex(), "value_hex": value.hex()}
    if k == MessageKind.DELETE:
        oid, key = decode_delete_request(frame)
        return {"oid": oid_json(oid), "key_hex": key.hex()}
    if k == MessageKind.RANGE:
        oid, start, end = decode_range_request(frame)
        return {"oid": oid_json(oid), "start_hex": start.hex(), "end_hex": end.hex()}
    if k == MessageKind.FS_OPEN:
        v = decode_fs_open_request(frame)
        return {
            "session": oid_json(v.session),
            "oid": oid_json(v.oid),
            "path": v.path,
            "flags": v.flags,
        }
    if k == MessageKind.FS_CLOSE:
        return {"fd": decode_fs_close_request(frame)}
    if k == MessageKind.FS_READ:
        fd, length = decode_fs_read_request(frame)
        return {"fd": fd, "len": length}
    if k == MessageKind.FS_WRITE:
        fd, data = decode_fs_write_request(frame)
        return {"fd": fd, "data_hex": data.hex()}
    if k == MessageKind.FS_PREAD:
        fd, offset, length = decode_fs_pread_request(frame)
        return {"fd": fd, "offset": str(offset), "len": length}
    if k == MessageKind.FS_PWRITE:
        fd, offset, data = decode_fs_pwrite_request(frame)
        return {"fd": fd, "offset": str(offset), "data_hex": data.hex()}
    if k == MessageKind.FS_LSEEK:
        fd, offset, whence = decode_fs_lseek_request(frame)
        return {"fd": fd, "offset": str(offset), "whence": whence}
    if k == MessageKind.FS_FSTAT:
        return {"fd": decode_fs_fstat_request(frame)}
    if k == MessageKind.FS_STAT:
        oid, path = decode_fs_stat_request(frame)
        return {"oid": oid_json(oid), "path": path}
    if k == MessageKind.FS_FSYNC:
        return {"fd": decode_fs_fsync_request(frame)}
    if k == MessageKind.FS_READDIR:
        oid, path = decode_fs_readdir_request(frame)
        return {"oid": oid_json(oid), "path": path}
    if k == MessageKind.RELATION_GET:
        oid, table, key, select = decode_relation_get_request(frame)
        return {
            "oid": oid_json(oid),
            "table": table,
            "key": [attr_datum_json(a, d) for a, d in key],
            "select": [str(s) for s in select],
        }
    if k == MessageKind.RELATION_UPDATE:
        oid, table, key, values, deltas = decode_relation_update_request(frame)
        return {
            "oid": oid_json(oid),
            "table": table,
            "key": [attr_datum_json(a, d) for a, d in key],
            "values": [attr_datum_json(a, d) for a, d in values],
            "deltas": [
                {"attr": str(a), "op": op, "datum_hex": d.hex()} for a, op, d in deltas
            ],
        }
    if k == MessageKind.RELATION_INSERT:
        oid, table, key, values = decode_relation_insert_request(frame)
        return {
            "oid": oid_json(oid),
            "table": table,
            "key": [attr_datum_json(a, d) for a, d in key],
            "values": [attr_datum_json(a, d) for a, d in values],
        }
    raise ValueError(f"unknown request kind {kind}")


def decode_response_expect(kind, frame):
    k = MessageKind(kind)
    if k == MessageKind.QUERY:
        return query_result_json(_expect_ok(decode_query_result(frame), "query"))
    if k == MessageKind.COMMAND:
        v = _expect_ok(decode_command_result(frame), "command")
        return {"affected_rows": str(v.affected_rows)}
    if k == MessageKind.BATCH:
        v = _expect_ok(decode_batch_result(frame), "batch")
        return {"affected_rows": str(v.affected_rows)}
    if k == MessageKind.OPEN_SESSION:
        v = _expect_ok(decode_open_session_result(frame), "open-session")
        # The sidecar renders the session UniOid as its u128 decimal value,
        # which equals l for every h == 0 session id used by the fixtures.
        if v.h != 0:
            raise ValueError("session oid serialization not covered by the sidecar conventions")
        return {"session": str(v.l)}
    if k == MessageKind.CLOSE_SESSION:
        _expect_ok(decode_close_session_result(frame), "close-session")
        return dict(_UNIT_JSON)
    if k == MessageKind.GET:
        v = _expect_ok(decode_get_result(frame), "get")
        if v is None:
            raise ValueError("absent get value serialization not covered by the sidecar conventions")
        return {"value_hex": v.hex()}
    if k == MessageKind.PUT:
        _expect_ok(decode_put_result(frame), "put")
        return dict(_UNIT_JSON)
    if k == MessageKind.DELETE:
        _expect_ok(decode_delete_result(frame), "delete")
        return dict(_UNIT_JSON)
    if k == MessageKind.RANGE:
        v = _expect_ok(decode_range_result(frame), "range")
        return {"items": [{"key_hex": key.hex(), "value_hex": val.hex()} for key, val in v]}
    if k == MessageKind.FS_OPEN:
        return {"fd": _expect_ok(decode_fs_open_result(frame), "fs-open")}
    if k == MessageKind.FS_CLOSE:
        _expect_ok(decode_fs_close_result(frame), "fs-close")
        return dict(_UNIT_JSON)
    if k == MessageKind.FS_READ:
        return {"data_hex": _expect_ok(decode_fs_read_result(frame), "fs-read").hex()}
    if k == MessageKind.FS_WRITE:
        return {"written": _expect_ok(decode_fs_write_result(frame), "fs-write")}
    if k == MessageKind.FS_PREAD:
        return {"data_hex": _expect_ok(decode_fs_pread_result(frame), "fs-pread").hex()}
    if k == MessageKind.FS_PWRITE:
        _expect_ok(decode_fs_pwrite_result(frame), "fs-pwrite")
        return dict(_UNIT_JSON)
    if k == MessageKind.FS_LSEEK:
        return {"position": str(_expect_ok(decode_fs_lseek_result(frame), "fs-lseek"))}
    if k == MessageKind.FS_FSTAT:
        return {"stat": fs_stat_json(_expect_ok(decode_fs_fstat_result(frame), "fs-fstat"))}
    if k == MessageKind.FS_STAT:
        return {"stat": fs_stat_json(_expect_ok(decode_fs_stat_result(frame), "fs-stat"))}
    if k == MessageKind.FS_FSYNC:
        _expect_ok(decode_fs_fsync_result(frame), "fs-fsync")
        return dict(_UNIT_JSON)
    if k == MessageKind.FS_READDIR:
        return {"entries": dirents_json(_expect_ok(decode_fs_readdir_result(frame), "fs-readdir"))}
    if k == MessageKind.RELATION_GET:
        return {"row": relation_row_json(_expect_ok(decode_relation_get_result(frame), "relation-get"))}
    if k == MessageKind.RELATION_UPDATE:
        return {"affected": str(_expect_ok(decode_relation_update_result(frame), "relation-update"))}
    if k == MessageKind.RELATION_INSERT:
        _expect_ok(decode_relation_insert_result(frame), "relation-insert")
        return dict(_UNIT_JSON)
    raise ValueError(f"unknown response kind {kind}")


def decode_err_expect(kind, frame):
    """Decode a result frame expected to be an error and render the UniError
    in the sidecar expect shape (field-level: err_src carries the host's
    '"None"' quirk verbatim)."""
    r = _RESULT_DECODERS[MessageKind(kind)](frame)
    if r.is_ok:
        raise ValueError(f"expected err {kind} result, got ok")
    return err_json(r.error)


# ---- deterministic encode inputs pinned byte-exactly against the fixture
# frames (mirrors the AS syscall_corpus_test.ts encode inputs) ----


def _oid(l):
    return UniOid(h=0, l=l)


def _query_argv(l, sql):
    return UniQueryArgv(oid=_oid(l), query=UniSqlStmt(sql_string=sql), param_list=UniSqlParam())


def _command_argv(l, sql):
    return UniCommandArgv(oid=_oid(l), command=UniSqlStmt(sql_string=sql), param_list=UniSqlParam())


def _fs_open_argv():
    return UniFsOpenArgv(session=_oid(10), oid=_oid(11), path="docs/a.txt", flags=2)


def _golden_fs_stat():
    return UniFsStat(oid=_oid(5), generation=1, entry="", length=100, state=1)


def _golden_dirents():
    return [UniFsDirent(name="a.txt", is_dir=False, length=3), UniFsDirent(name="docs", is_dir=True, length=0)]


def _golden_err():
    # The trailing corpus frame: get result Err(NotFound, "no such entry").
    # err_src is the host's JSON form of ErrorSource::None; err_details
    # encodes as a MessagePack ARRAY, not bin — both quirks pinned on purpose.
    return UniError(err_code=2, err_msg="no such entry", err_src='"None"', err_loc="", err_details=b"")


_REQUEST_ENCODERS = {
    MessageKind.QUERY: lambda: encode_query_request(_query_argv(1, "select 1")),
    MessageKind.COMMAND: lambda: encode_command_request(_command_argv(2, "update t set a = 1")),
    MessageKind.BATCH: lambda: encode_batch_request(_command_argv(3, "insert into t values (1)")),
    MessageKind.OPEN_SESSION: lambda: encode_open_session_request(_oid(4)),
    MessageKind.CLOSE_SESSION: lambda: encode_close_session_request(_oid(5)),
    MessageKind.GET: lambda: encode_get_request(_oid(6), b"k1"),
    MessageKind.PUT: lambda: encode_put_request(_oid(7), b"k1", b"v1"),
    MessageKind.DELETE: lambda: encode_delete_request(_oid(8), b"k1"),
    MessageKind.RANGE: lambda: encode_range_request(_oid(9), b"a", b"z"),
    MessageKind.FS_OPEN: lambda: encode_fs_open_request(_fs_open_argv()),
    MessageKind.FS_CLOSE: lambda: encode_fs_close_request(3),
    MessageKind.FS_READ: lambda: encode_fs_read_request(3, 4),
    MessageKind.FS_WRITE: lambda: encode_fs_write_request(3, b"hi"),
    MessageKind.FS_PREAD: lambda: encode_fs_pread_request(3, 8, 4),
    MessageKind.FS_PWRITE: lambda: encode_fs_pwrite_request(3, 8, b"hi"),
    MessageKind.FS_LSEEK: lambda: encode_fs_lseek_request(3, -2, 1),
    MessageKind.FS_FSTAT: lambda: encode_fs_fstat_request(3),
    MessageKind.FS_STAT: lambda: encode_fs_stat_request(_oid(18), "a"),
    MessageKind.FS_FSYNC: lambda: encode_fs_fsync_request(3),
    MessageKind.FS_READDIR: lambda: encode_fs_readdir_request(_oid(20), "d"),
    MessageKind.RELATION_GET: lambda: encode_relation_get_request(
        _oid(21), "t", [(1, b"\x01"), (2, b"\x02\x03")], [3, 4]
    ),
    MessageKind.RELATION_UPDATE: lambda: encode_relation_update_request(
        _oid(22), "t", [(1, b"\x01")], [(2, b"\x0a")], [(3, 0, b"\x05")]
    ),
    MessageKind.RELATION_INSERT: lambda: encode_relation_insert_request(
        _oid(23), "t", [(1, b"\x01")], [(2, b"\x0a")]
    ),
}

_RESPONSE_ENCODERS = {
    MessageKind.QUERY: lambda r: encode_query_result(r),
    MessageKind.COMMAND: lambda r: encode_command_result(r),
    MessageKind.BATCH: lambda r: encode_batch_result(r),
    MessageKind.OPEN_SESSION: lambda r: encode_open_session_result(r),
    MessageKind.CLOSE_SESSION: lambda r: encode_close_session_result(r),
    MessageKind.GET: lambda r: encode_get_result(r),
    MessageKind.PUT: lambda r: encode_put_result(r),
    MessageKind.DELETE: lambda r: encode_delete_result(r),
    MessageKind.RANGE: lambda r: encode_range_result(r),
    MessageKind.FS_OPEN: lambda r: encode_fs_open_result(r),
    MessageKind.FS_CLOSE: lambda r: encode_fs_close_result(r),
    MessageKind.FS_READ: lambda r: encode_fs_read_result(r),
    MessageKind.FS_WRITE: lambda r: encode_fs_write_result(r),
    MessageKind.FS_PREAD: lambda r: encode_fs_pread_result(r),
    MessageKind.FS_PWRITE: lambda r: encode_fs_pwrite_result(r),
    MessageKind.FS_LSEEK: lambda r: encode_fs_lseek_result(r),
    MessageKind.FS_FSTAT: lambda r: encode_fs_fstat_result(r),
    MessageKind.FS_STAT: lambda r: encode_fs_stat_result(r),
    MessageKind.FS_FSYNC: lambda r: encode_fs_fsync_result(r),
    MessageKind.FS_READDIR: lambda r: encode_fs_readdir_result(r),
    MessageKind.RELATION_GET: lambda r: encode_relation_get_result(r),
    MessageKind.RELATION_UPDATE: lambda r: encode_relation_update_result(r),
    MessageKind.RELATION_INSERT: lambda r: encode_relation_insert_result(r),
}

_RESULT_DECODERS = {
    MessageKind.QUERY: decode_query_result,
    MessageKind.COMMAND: decode_command_result,
    MessageKind.BATCH: decode_batch_result,
    MessageKind.OPEN_SESSION: decode_open_session_result,
    MessageKind.CLOSE_SESSION: decode_close_session_result,
    MessageKind.GET: decode_get_result,
    MessageKind.PUT: decode_put_result,
    MessageKind.DELETE: decode_delete_result,
    MessageKind.RANGE: decode_range_result,
    MessageKind.FS_OPEN: decode_fs_open_result,
    MessageKind.FS_CLOSE: decode_fs_close_result,
    MessageKind.FS_READ: decode_fs_read_result,
    MessageKind.FS_WRITE: decode_fs_write_result,
    MessageKind.FS_PREAD: decode_fs_pread_result,
    MessageKind.FS_PWRITE: decode_fs_pwrite_result,
    MessageKind.FS_LSEEK: decode_fs_lseek_result,
    MessageKind.FS_FSTAT: decode_fs_fstat_result,
    MessageKind.FS_STAT: decode_fs_stat_result,
    MessageKind.FS_FSYNC: decode_fs_fsync_result,
    MessageKind.FS_READDIR: decode_fs_readdir_result,
    MessageKind.RELATION_GET: decode_relation_get_result,
    MessageKind.RELATION_UPDATE: decode_relation_update_result,
    MessageKind.RELATION_INSERT: decode_relation_insert_result,
}

_OK_RESPONSE_VALUES = {
    MessageKind.QUERY: lambda: UniQueryResult(),
    MessageKind.COMMAND: lambda: UniCommandResult(affected_rows=3),
    MessageKind.BATCH: lambda: UniCommandResult(affected_rows=2),
    MessageKind.OPEN_SESSION: lambda: _oid(4),
    MessageKind.CLOSE_SESSION: lambda: None,
    MessageKind.GET: lambda: b"v1",
    MessageKind.PUT: lambda: None,
    MessageKind.DELETE: lambda: None,
    MessageKind.RANGE: lambda: [(b"a", b"1"), (b"b", b"2")],
    MessageKind.FS_OPEN: lambda: 9,
    MessageKind.FS_CLOSE: lambda: None,
    MessageKind.FS_READ: lambda: b"hi",
    MessageKind.FS_WRITE: lambda: 2,
    MessageKind.FS_PREAD: lambda: b"hi",
    MessageKind.FS_PWRITE: lambda: None,
    MessageKind.FS_LSEEK: lambda: 6,
    MessageKind.FS_FSTAT: _golden_fs_stat,
    MessageKind.FS_STAT: _golden_fs_stat,
    MessageKind.FS_FSYNC: lambda: None,
    MessageKind.FS_READDIR: _golden_dirents,
    MessageKind.RELATION_GET: lambda: [b"\x0a", None],
    MessageKind.RELATION_UPDATE: lambda: 1,
    MessageKind.RELATION_INSERT: lambda: None,
}


def header_kind(frame):
    return int(decode_header(frame))


def encode_request(kind):
    return _REQUEST_ENCODERS[MessageKind(kind)]()


def encode_ok_response(kind):
    k = MessageKind(kind)
    return _RESPONSE_ENCODERS[k](WireResult.ok(_OK_RESPONSE_VALUES[k]()))


def encode_get_err_response():
    return encode_get_result(WireResult.err(_golden_err()))


def decode_response(kind, frame):
    return _RESULT_DECODERS[MessageKind(kind)](frame)


def encode_response(kind, result):
    return _RESPONSE_ENCODERS[MessageKind(kind)](result)
