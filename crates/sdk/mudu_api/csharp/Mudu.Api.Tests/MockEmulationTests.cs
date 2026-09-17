#nullable enable

using mududb;
using mududb.codec;
using mududb.sys;
using Xunit;

namespace mududb.tests;

/// <summary>
/// End-to-end behaviour of the new syscalls against the in-process mock
/// backend: open/close lifecycle, KV point and range operations, the relation
/// family, and `batch` through the SQLite emulation.
/// </summary>
[Collection("MockBackend")]
public class MockEmulationTests
{
    public MockEmulationTests()
    {
        MuduSysCallApi.UseMockBackend = true;
    }

    [Fact]
    public void KvSessionLifecycle()
    {
        var opened = MuduSysCallApi.SysOpen(new UniOid { H = 0, L = 42 });
        Assert.True(opened.IsOk);
        var session = opened.Value;

        // missing key -> ok with a null value (host's option<list<u8>> nil)
        var missing = MuduSysCallApi.SysGet(session, "k1"u8.ToArray());
        Assert.True(missing.IsOk);
        Assert.Null(missing.Value);

        Assert.Null(MuduSysCallApi.SysPut(session, "k1"u8.ToArray(), "v1"u8.ToArray()));
        Assert.Null(MuduSysCallApi.SysPut(session, "k2"u8.ToArray(), "v2"u8.ToArray()));
        Assert.Null(MuduSysCallApi.SysPut(session, "k3"u8.ToArray(), "v3"u8.ToArray()));

        var got = MuduSysCallApi.SysGet(session, "k2"u8.ToArray());
        Assert.True(got.IsOk);
        Assert.Equal("v2"u8.ToArray(), got.Value);

        // range is start-inclusive / end-exclusive and sorted by key
        var range = MuduSysCallApi.SysRange(session, "k1"u8.ToArray(), "k3"u8.ToArray());
        Assert.True(range.IsOk);
        Assert.Equal(2, range.Value!.Length);
        Assert.Equal("k1"u8.ToArray(), range.Value[0].Key);
        Assert.Equal("k2"u8.ToArray(), range.Value[1].Key);

        // an empty end key means unbounded
        var tail = MuduSysCallApi.SysRange(session, "k2"u8.ToArray(), []);
        Assert.True(tail.IsOk);
        Assert.Equal(2, tail.Value!.Length);
        Assert.Equal("k3"u8.ToArray(), tail.Value[1].Key);

        Assert.Null(MuduSysCallApi.SysDelete(session, "k2"u8.ToArray()));
        var deleted = MuduSysCallApi.SysGet(session, "k2"u8.ToArray());
        Assert.True(deleted.IsOk);
        Assert.Null(deleted.Value);

        Assert.Null(MuduSysCallApi.SysClose(session));
        var afterClose = MuduSysCallApi.SysGet(session, "k1"u8.ToArray());
        Assert.True(afterClose.IsErr);
        Assert.Equal(50009u, afterClose.Error.GetValueOrDefault().ErrCode);
    }

    [Fact]
    public void RelationLifecycle()
    {
        var session = MuduSysCallApi.SysOpen(new UniOid { H = 0, L = 42 }).Value;

        Assert.Null(MuduSysCallApi.SysRelationInsert(
            session,
            "users",
            [(0UL, I64(1))],
            [(1UL, I64(10)), (2UL, "hi"u8.ToArray())]));

        // projection follows select order; an attribute the row does not
        // carry projects as a null element
        var row = MuduSysCallApi.SysRelationGet(session, "users", [(0UL, I64(1))], [0UL, 1UL, 2UL, 9UL]);
        Assert.True(row.IsOk);
        Assert.Equal(4, row.Value!.Length);
        Assert.Equal(I64(1), row.Value[0]);
        Assert.Equal(I64(10), row.Value[1]);
        Assert.Equal("hi"u8.ToArray(), row.Value[2]);
        Assert.Null(row.Value[3]);

        // plain assignments apply first, then deltas on top
        var updated = MuduSysCallApi.SysRelationUpdate(
            session,
            "users",
            [(0UL, I64(1))],
            [(1UL, I64(20))],
            [UniRelationDelta.Add(1, I64(5))]);
        Assert.True(updated.IsOk);
        Assert.Equal(1UL, updated.Value);
        Assert.Equal(I64(25), MuduSysCallApi.SysRelationGet(session, "users", [(0UL, I64(1))], [1UL]).Value![0]);

        // a key that matches no row reports 0 affected rows
        var noMatch = MuduSysCallApi.SysRelationUpdate(session, "users", [(0UL, I64(99))], [], [UniRelationDelta.Add(1, I64(5))]);
        Assert.True(noMatch.IsOk);
        Assert.Equal(0UL, noMatch.Value);

        // a missing row reads back as ok-nil
        var absent = MuduSysCallApi.SysRelationGet(session, "users", [(0UL, I64(99))], [1UL]);
        Assert.True(absent.IsOk);
        Assert.Null(absent.Value);
    }

    [Fact]
    public void RelationSubWrapDelta()
    {
        var session = MuduSysCallApi.SysOpen(new UniOid { H = 0, L = 42 }).Value;
        Assert.Null(MuduSysCallApi.SysRelationInsert(session, "stock", [(0UL, I64(2))], [(3UL, I64(100))]));

        // cur=100, q=30, floor=10, wrap=90 -> ((100 - 10 - 30) mod 90) + 10 = 70
        var updated = MuduSysCallApi.SysRelationUpdate(
            session,
            "stock",
            [(0UL, I64(2))],
            [],
            [UniRelationDelta.SubWrap(3, 30, 10, 90)]);
        Assert.True(updated.IsOk);
        Assert.Equal(1UL, updated.Value);
        Assert.Equal(I64(70), MuduSysCallApi.SysRelationGet(session, "stock", [(0UL, I64(2))], [3UL]).Value![0]);
    }

    [Fact]
    public void RelationErrorCases()
    {
        var session = MuduSysCallApi.SysOpen(new UniOid { H = 0, L = 42 }).Value;

        // duplicate primary key -> EntityAlreadyExists (50012)
        Assert.Null(MuduSysCallApi.SysRelationInsert(session, "users", [(0UL, I64(1))], []));
        var duplicate = MuduSysCallApi.SysRelationInsert(session, "users", [(0UL, I64(1))], []);
        Assert.NotNull(duplicate);
        Assert.Equal(50012u, duplicate.GetValueOrDefault().ErrCode);

        // table never inserted into -> EntityNotFound (50009)
        var noTable = MuduSysCallApi.SysRelationGet(session, "nope", [(0UL, I64(1))], [1UL]);
        Assert.True(noTable.IsErr);
        Assert.Equal(50009u, noTable.Error.GetValueOrDefault().ErrCode);

        // unknown session -> EntityNotFound (50009)
        var noSession = MuduSysCallApi.SysRelationGet(new UniOid { H = 9, L = 9 }, "users", [(0UL, I64(1))], [1UL]);
        Assert.True(noSession.IsErr);
        Assert.Equal(50009u, noSession.Error.GetValueOrDefault().ErrCode);

        // unknown delta op -> Decode (50001)
        var badOp = MuduSysCallApi.SysRelationUpdate(
            session,
            "users",
            [(0UL, I64(1))],
            [],
            [new UniRelationDelta { Attr = 1, Op = 99, Datum = I64(1) }]);
        Assert.True(badOp.IsErr);
        Assert.Equal(50001u, badOp.Error.GetValueOrDefault().ErrCode);
    }

    [Fact]
    public void BatchRunsThroughSqliteEmulation()
    {
        mududb.mock.MockSqliteMuduSysCall.DatabasePath = global::System.IO.Path.Combine(
            global::System.IO.Path.GetTempPath(),
            $"mudu_mock_test_{System.Guid.NewGuid():N}.db");
        try
        {
            var create = new UniCommandArgv
            {
                Oid = new UniOid { H = 0, L = 1 },
                Command = new UniSqlStmt { SqlString = "CREATE TABLE kv (k TEXT, v TEXT)" },
                ParamList = new UniSqlParam { Params = [] },
            };
            Assert.Equal(UniCommandReturnKind.Ok, MuduSysCallApi.SysCommand(create).Kind());

            var insert = new UniCommandArgv
            {
                Oid = new UniOid { H = 0, L = 1 },
                Command = new UniSqlStmt { SqlString = "INSERT INTO kv VALUES ('a', 'b')" },
                ParamList = new UniSqlParam { Params = [] },
            };
            var batched = MuduSysCallApi.SysBatch(insert);
            Assert.Equal(UniCommandReturnKind.Ok, batched.Kind());
            Assert.Equal(1UL, UniCommandReturnOk.AsOk(batched).Inner.AffectedRows);
        }
        finally
        {
            global::System.IO.File.Delete(mududb.mock.MockSqliteMuduSysCall.DatabasePath);
        }
    }

    private static byte[] I64(long value)
    {
        var datum = new byte[8];
        global::System.Buffers.Binary.BinaryPrimitives.WriteInt64BigEndian(datum, value);
        return datum;
    }
}
