using System.Text;
using Mudu.Api;

var dbPath = Path.Combine(AppContext.BaseDirectory, "demo.db");
Environment.SetEnvironmentVariable("MUDU_MOCK_SQLITE_PATH", dbPath);

// Route syscalls to the in-process mock (SQLite + in-memory fs emulation)
// instead of the wasm host imports; this is a native debug run.
MuduSysCallApi.UseMockBackend = true;

RunCommand(
    "create table if not exists demo_users (" +
    "id integer primary key autoincrement, " +
    "name text not null, " +
    "score integer not null" +
    ")"
);

RunCommand("delete from demo_users");

InsertUser("alice", 10);
InsertUser("bob", 20);

var queryResult = global::Mudu.Api.Mudu.Query(new UniQueryArgv
{
    Oid = new UniOid { H = 0, L = 0 },
    Query = new UniSqlStmt
    {
        SqlString = "select id, name, score from demo_users where score >= ? order by id"
    },
    ParamList = new UniSqlParam
    {
        Params = new()
        {
            new Universal.UniDatValueScalar
            {
                Inner = new Universal.UniScalarValueI32
                {
                    Inner = 10
                }
            }
        }
    }
});

if (queryResult.IsErr)
{
    var error = queryResult.Error!.Value;
    Console.WriteLine($"query failed: {error.ErrCode} {error.ErrMsg}");
    return;
}

var ok = queryResult.RequireOk();
Console.WriteLine($"db: {dbPath}");
Console.WriteLine($"rows: {ok.ResultSet.RowSet.Count}");

foreach (var row in ok.ResultSet.RowSet)
{
    var id = ((Universal.UniScalarValueI64)((Universal.UniDatValueScalar)row.Fields[0]).Inner).Inner;
    var name = ((Universal.UniScalarValueString)((Universal.UniDatValueScalar)row.Fields[1]).Inner).Inner;
    var score = ((Universal.UniScalarValueI64)((Universal.UniDatValueScalar)row.Fields[2]).Inner).Inner;
    Console.WriteLine($"{id}: {name} -> {score}");
}

// ---- fs syscall family (MSSP frames through the in-memory mock emulation) ----

var sessionId = new UniOid { H = 0, L = 0 };
var fsOid = new UniOid { H = 0, L = 7 };
var payload = "hello from the mudu fs demo"u8.ToArray();

var writeFd = MuduFileSystem.FsOpen(sessionId, fsOid, "docs/hello.txt", MuduFileSystem.OpenWriteOnly).RequireOk();
var written = MuduFileSystem.FsWrite(sessionId, writeFd, payload).RequireOk();
MuduFileSystem.FsFsync(sessionId, writeFd).RequireOk();
MuduFileSystem.FsClose(sessionId, writeFd).RequireOk();
Console.WriteLine($"fs: wrote {written} bytes to docs/hello.txt (fd {writeFd})");

var readFd = MuduFileSystem.FsOpen(sessionId, fsOid, "docs/hello.txt", MuduFileSystem.OpenReadOnly).RequireOk();
var content = MuduFileSystem.FsRead(sessionId, readFd, 4096).RequireOk();
Console.WriteLine($"fs: read back \"{Encoding.UTF8.GetString(content)}\"");

var slice = MuduFileSystem.FsPread(sessionId, readFd, 6, 4).RequireOk();
Console.WriteLine($"fs: pread(offset=6, len=4) -> \"{Encoding.UTF8.GetString(slice)}\"");

var end = MuduFileSystem.FsLseek(sessionId, readFd, 0, 2).RequireOk();
Console.WriteLine($"fs: lseek(END) -> cursor {end}");

var fstat = MuduFileSystem.FsFstat(sessionId, readFd).RequireOk();
Console.WriteLine($"fs: fstat entry={fstat.Entry} length={fstat.Length} state={fstat.State} generation={fstat.Generation}");
MuduFileSystem.FsClose(sessionId, readFd).RequireOk();

var stat = MuduFileSystem.FsStat(sessionId, fsOid, "docs/hello.txt").RequireOk();
Console.WriteLine($"fs: stat docs/hello.txt -> length {stat.Length}");

var entries = MuduFileSystem.FsReaddir(sessionId, fsOid, "").RequireOk();
foreach (var entry in entries)
{
    Console.WriteLine($"fs: readdir {(entry.IsDir ? "dir" : "file")} {entry.Name} ({entry.Length} bytes)");
}

var missing = MuduFileSystem.FsOpen(sessionId, fsOid, "no/such.txt", MuduFileSystem.OpenReadOnly);
Console.WriteLine($"fs: open missing -> errno {missing.Errno} ({missing.Error?.ErrMsg})");

static void InsertUser(string name, int score)
{
    var response = global::Mudu.Api.Mudu.Command(new UniCommandArgv
    {
        Oid = new UniOid { H = 0, L = 0 },
        Command = new UniSqlStmt
        {
            SqlString = "insert into demo_users(name, score) values(?, ?)"
        },
        ParamList = new UniSqlParam
        {
            Params = new()
            {
                new Universal.UniDatValueScalar
                {
                    Inner = new Universal.UniScalarValueString
                    {
                        Inner = name
                    }
                },
                new Universal.UniDatValueScalar
                {
                    Inner = new Universal.UniScalarValueI32
                    {
                        Inner = score
                    }
                }
            }
        }
    });

    if (response.IsErr)
    {
        var error = response.Error!.Value;
        throw new InvalidOperationException($"insert failed: {error.ErrCode} {error.ErrMsg}");
    }
}

static void RunCommand(string sql)
{
    var response = global::Mudu.Api.Mudu.Command(new UniCommandArgv
    {
        Oid = new UniOid { H = 0, L = 0 },
        Command = new UniSqlStmt
        {
            SqlString = sql
        },
        ParamList = new UniSqlParam
        {
            Params = new()
        }
    });

    if (response.IsErr)
    {
        var error = response.Error!.Value;
        throw new InvalidOperationException($"command failed: {error.ErrCode} {error.ErrMsg}");
    }
}
