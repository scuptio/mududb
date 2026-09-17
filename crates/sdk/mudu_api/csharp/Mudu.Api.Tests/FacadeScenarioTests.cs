#nullable enable

using mududb.db;
using mududb.fs;
using mududb.sql;
using mududb.sys;
using mududb.types;
using Xunit;

namespace mududb.tests;

/// <summary>
/// Cross-language facade consistency scenario
/// (`doc/dev/binding_api_surface.md`, section "Behavioral consistency
/// scenario"), assertion-identical with the Rust
/// `crates/sdk/mududb/tests/facade_scenario.rs`. Runs against the in-process
/// mock backend (SQLite + in-memory fs).
/// </summary>
[Collection("MockBackend")]
public class FacadeScenarioTests
{
    /// <summary>`FS_SEEK_SET` whence from the canonical fs surface.</summary>
    private const uint FsSeekSet = 0;

    public FacadeScenarioTests()
    {
        MuduSysCallApi.UseMockBackend = true;
    }

    [Fact]
    public void FacadeScenario()
    {
        // 1. open (default worker)
        var db = Database.Open();

        // Harness cleanup: the mock backend keeps its SQLite state across
        // runs, so re-runs must start idempotently (not part of the
        // canonical scenario).
        db.Command(new SqlStmt("DROP TABLE IF EXISTS t"));

        // 2. create table
        db.Command(new SqlStmt("CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT)"));

        // 3. two inserts with positional parameters, one affected row each
        var affected = db.Command(
            new SqlStmt("INSERT INTO t (id, name) VALUES (?, ?)"),
            new SqlParams().Bind(0, 1).Bind(1, "alice"));
        Assert.Equal(1UL, affected);
        affected = db.Command(
            new SqlStmt("INSERT INTO t (id, name) VALUES (?, ?)"),
            new SqlParams().Bind(0, 2).Bind(1, "bob"));
        Assert.Equal(1UL, affected);

        // 4. query: columns, values by index and by name, eof
        var rs = db.Query(new SqlStmt("SELECT id, name FROM t ORDER BY id"));
        Assert.Equal(2, rs.ColumnCount());
        Assert.Equal("id", rs.ColumnName(0));
        Assert.Equal("name", rs.ColumnName(1));
        Assert.Equal(1, rs.FindColumn("name"));

        Assert.True(rs.Next());
        var row = rs.CurrentRow();
        Assert.Equal(1L, ExpectI64(row.Value(0)));
        Assert.Equal("alice", ExpectString(row.ValueByName("name")));
        Assert.False(row.IsNull(0));

        Assert.True(rs.Next());
        row = rs.CurrentRow();
        Assert.Equal(2L, ExpectI64(row.ValueByName("id")));
        Assert.Equal("bob", ExpectString(row.Value(1)));

        Assert.False(rs.Next());
        Assert.True(rs.Eof());

        // 5. update with named parameters, one affected row
        affected = db.Command(
            new SqlStmt("UPDATE t SET name = :name WHERE id = :id"),
            new SqlParams().BindNamed("name", "carol").BindNamed("id", 2));
        Assert.Equal(1UL, affected);
        rs = db.Query(new SqlStmt("SELECT name FROM t WHERE id = 2"));
        Assert.True(rs.Next());
        Assert.Equal("carol", ExpectString(rs.CurrentRow().Value(0)));

        // 6. fs roundtrip: write "hello", seek back, read/pread, fstat, readdir
        var session = db.Id;
        var fsOid = new UniOid { H = 0, L = 7 };
        var fd = MuduFileSystem.FsOpen(session, fsOid, "docs/hello.txt", MuduFileSystem.OpenWriteOnly).RequireOk();
        Assert.Equal(5u, MuduFileSystem.FsWrite(session, fd, "hello"u8.ToArray()).RequireOk());
        MuduFileSystem.FsClose(session, fd).RequireOk();

        fd = MuduFileSystem.FsOpen(session, fsOid, "docs/hello.txt", MuduFileSystem.OpenReadOnly).RequireOk();
        Assert.Equal(0UL, MuduFileSystem.FsLseek(session, fd, 0, FsSeekSet).RequireOk());
        Assert.Equal("hello"u8.ToArray(), MuduFileSystem.FsRead(session, fd, 5).RequireOk());
        Assert.Equal("ell"u8.ToArray(), MuduFileSystem.FsPread(session, fd, 1, 3).RequireOk());
        Assert.Equal(5UL, MuduFileSystem.FsFstat(session, fd).RequireOk().Length);
        MuduFileSystem.FsClose(session, fd).RequireOk();

        var entries = MuduFileSystem.FsReaddir(session, fsOid, "docs").RequireOk();
        Assert.Contains(entries, entry => entry.Name == "hello.txt" && !entry.IsDir);

        // 7. close
        db.Close();
    }

    // The mock maps SQLite INTEGER columns to the i64 scalar case and TEXT
    // columns to the string case; the Rust reference asserts the same
    // numeric/text values through its own DataValue accessors.
    private static long ExpectI64(UniDataValue value)
    {
        return UniScalarValueI64.AsI64(UniDataValueScalar.AsScalar(value).Inner).Inner;
    }

    private static string ExpectString(UniDataValue value)
    {
        return UniScalarValueString.AsString(UniDataValueScalar.AsScalar(value).Inner).Inner;
    }
}
