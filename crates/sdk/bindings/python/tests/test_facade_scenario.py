"""The cross-language 7-step behavioral consistency scenario for the Python
facade (doc/dev/binding_api_surface.md), run against a scripted in-memory
transport that answers the real wire frames — proving the facade's framing
and semantics end to end without a host.
"""
import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from mududb import sys as mudu_sys
from mududb.db import Database
from mududb.fs import FS_O_RDONLY, FS_O_WRONLY, FS_SEEK_SET
from mududb.generated import uni_syscall as sc
from mududb.generated.uni_data_value import UniDataValueScalar
from mududb.generated.uni_fs_dirent import UniFsDirent
from mududb.generated.uni_fs_stat import UniFsStat
from mududb.generated.uni_oid import UniOid
from mududb.generated.uni_query_result import UniQueryResult
from mududb.generated.uni_record_type import UniRecordField, UniRecordType
from mududb.generated.uni_result_set import UniResultSet
from mududb.generated.uni_scalar_value import UniScalarValueI64, UniScalarValueString
from mududb.generated.uni_tuple_row import UniTupleRow
from mududb.sql import Params, SqlStmt


def _i64(v):
    return UniDataValueScalar(inner=UniScalarValueI64(inner=v))


def _text(v):
    return UniDataValueScalar(inner=UniScalarValueString(inner=v))


def _query_result(columns, rows):
    return UniQueryResult(
        tuple_desc=UniRecordType(
            record_fields=[UniRecordField(field_name=c) for c in columns]
        ),
        result_set=UniResultSet(
            eof=True, row_set=[UniTupleRow(fields=r) for r in rows]
        ),
    )


class FakeTransport:
    """A scripted in-memory host: session counter plus a dict filesystem."""

    def __init__(self):
        self.session = UniOid(h=0, l=1)
        self.files = {}
        self.fds = {}
        self.next_fd = 1

    def open(self, _request):
        return sc.encode_open_session_result(sc.WireResult.ok(self.session))

    def close(self, _request):
        return sc.encode_close_session_result(sc.WireResult.ok())

    def command(self, request):
        argv = sc.decode_command_request(request)
        sql = argv.command.sql_string
        affected = 0 if sql.startswith("CREATE TABLE") or sql.startswith("DROP TABLE") else 1
        return sc.encode_command_result(sc.WireResult.ok(_command_result(affected)))

    def batch(self, request):
        return self.command(request)

    def query(self, request):
        argv = sc.decode_query_request(request)
        sql = argv.query.sql_string
        if "ORDER BY id" in sql:
            result = _query_result(
                ["id", "name"],
                [[_i64(1), _text("alice")], [_i64(2), _text("bob")]],
            )
        else:
            result = _query_result(["name"], [[_text("carol")]])
        return sc.encode_query_result(sc.WireResult.ok(result))

    def fs_open(self, request):
        argv = sc.decode_fs_open_request(request)
        fd = self.next_fd
        self.next_fd += 1
        self.fds[fd] = {"path": argv.path, "flags": argv.flags, "pos": 0}
        return sc.encode_fs_open_result(sc.WireResult.ok(fd))

    def fs_close(self, request):
        fd = sc.decode_fs_close_request(request)
        self.fds.pop(fd, None)
        return sc.encode_fs_close_result(sc.WireResult.ok())

    def fs_write(self, request):
        fd, data = sc.decode_fs_write_request(request)
        state = self.fds[fd]
        self.files[state["path"]] = data
        return sc.encode_fs_write_result(sc.WireResult.ok(len(data)))

    def fs_read(self, request):
        fd, length = sc.decode_fs_read_request(request)
        state = self.fds[fd]
        data = self.files[state["path"]]
        return sc.encode_fs_read_result(
            sc.WireResult.ok(data[state["pos"] : state["pos"] + length])
        )

    def fs_pread(self, request):
        fd, offset, length = sc.decode_fs_pread_request(request)
        data = self.files[self.fds[fd]["path"]]
        return sc.encode_fs_pread_result(sc.WireResult.ok(data[offset : offset + length]))

    def fs_lseek(self, request):
        fd, offset, whence = sc.decode_fs_lseek_request(request)
        self.fds[fd]["pos"] = offset
        return sc.encode_fs_lseek_result(sc.WireResult.ok(offset))

    def fs_fstat(self, request):
        fd = sc.decode_fs_fstat_request(request)
        path = self.fds[fd]["path"]
        stat = UniFsStat(oid=UniOid(), entry=path, length=len(self.files[path]))
        return sc.encode_fs_fstat_result(sc.WireResult.ok(stat))

    def fs_readdir(self, request):
        _oid, path = sc.decode_fs_readdir_request(request)
        entries = [
            UniFsDirent(name=p.rsplit("/", 1)[-1], is_dir=False, length=len(data))
            for p, data in self.files.items()
            if p.startswith(path.rstrip("/") + "/")
        ]
        return sc.encode_fs_readdir_result(sc.WireResult.ok(entries))

    def fs_fsync(self, _request):
        return sc.encode_fs_fsync_result(sc.WireResult.ok())

    def fs_stat(self, request):
        oid, path = sc.decode_fs_stat_request(request)
        stat = UniFsStat(oid=oid, entry=path, length=len(self.files.get(path, b"")))
        return sc.encode_fs_stat_result(sc.WireResult.ok(stat))

    def fs_pwrite(self, request):
        fd, offset, data = sc.decode_fs_pwrite_request(request)
        path = self.fds[fd]["path"]
        buf = bytearray(self.files.get(path, b""))
        buf[offset : offset + len(data)] = data
        self.files[path] = bytes(buf)
        return sc.encode_fs_pwrite_result(sc.WireResult.ok())


def _command_result(affected):
    from mududb.generated.uni_command_result import UniCommandResult

    return UniCommandResult(affected_rows=affected)


class TestFacadeScenario(unittest.TestCase):
    def setUp(self):
        mudu_sys.set_transport(FakeTransport())

    def tearDown(self):
        mudu_sys.set_transport(None)

    def test_scenario(self):
        # 1. open (default worker)
        db = Database.open()

        # 2. create table (+ harness cleanup, mirroring rs/cs)
        db.command(SqlStmt("DROP TABLE IF EXISTS t"))
        db.command(SqlStmt("CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT)"))

        # 3. two positional inserts, one affected row each
        affected = db.command(
            SqlStmt("INSERT INTO t (id, name) VALUES (?, ?)"),
            Params().bind(0, 1).bind(1, "alice"),
        )
        self.assertEqual(affected, 1)
        affected = db.command(
            SqlStmt("INSERT INTO t (id, name) VALUES (?, ?)"),
            Params().bind(0, 2).bind(1, "bob"),
        )
        self.assertEqual(affected, 1)

        # 4. query: columns, values by index and by name, eof
        rs = db.query(SqlStmt("SELECT id, name FROM t ORDER BY id"))
        self.assertEqual(rs.column_count(), 2)
        self.assertEqual(rs.column_name(0), "id")
        self.assertEqual(rs.find_column("name"), 1)

        self.assertTrue(rs.next())
        row = rs.current_row()
        self.assertEqual(row.value(0).inner.inner, 1)
        self.assertEqual(row.value_by_name("name").inner.inner, "alice")
        self.assertFalse(row.is_null(0))

        self.assertTrue(rs.next())
        row = rs.current_row()
        self.assertEqual(row.value_by_name("id").inner.inner, 2)
        self.assertEqual(row.value(1).inner.inner, "bob")

        self.assertFalse(rs.next())
        self.assertTrue(rs.eof())

        # 5. named-parameter update, one affected row
        affected = db.command(
            SqlStmt("UPDATE t SET name = :name WHERE id = :id"),
            Params().bind_named("name", "carol").bind_named("id", 2),
        )
        self.assertEqual(affected, 1)
        rs = db.query(SqlStmt("SELECT name FROM t WHERE id = 2"))
        self.assertTrue(rs.next())
        self.assertEqual(rs.current_row().value(0).inner.inner, "carol")

        # 6. fs roundtrip
        session = db.id
        fs_oid = UniOid(h=0, l=7)
        fd = mudu_sys.fs_open(session, fs_oid, "docs/hello.txt", FS_O_WRONLY)
        self.assertEqual(mudu_sys.fs_write(fd, b"hello"), 5)
        mudu_sys.fs_close(fd)

        fd = mudu_sys.fs_open(session, fs_oid, "docs/hello.txt", FS_O_RDONLY)
        self.assertEqual(mudu_sys.fs_lseek(fd, 0, FS_SEEK_SET), 0)
        self.assertEqual(mudu_sys.fs_read(fd, 5), b"hello")
        self.assertEqual(mudu_sys.fs_pread(fd, 1, 3), b"ell")
        self.assertEqual(mudu_sys.fs_fstat(fd).length, 5)
        mudu_sys.fs_close(fd)

        entries = mudu_sys.fs_readdir(fs_oid, "docs")
        self.assertTrue(any(e.name == "hello.txt" and not e.is_dir for e in entries))

        # 7. close
        db.close()


if __name__ == "__main__":
    unittest.main()
