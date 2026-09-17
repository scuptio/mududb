# mududb C# Library

This directory contains a reusable C# library for calling MuduDB system APIs,
published as the NuGet package `mududb`.

It includes:

- `uni/`: MessagePack models used by the syscall layer (namespace `mududb.types`)
- `mudu_sys/`: real wasm syscall bindings, the generated MSSP frame codec (`UniSyscall.cs`, namespace `mududb.codec`) and `MuduSysCallApi` (namespace `mududb.sys`)
- `mock/`: a SQLite-backed mock implementation (namespace `mududb.mock`) compatible with `SysCommand` and `SysQuery`, plus an in-memory fs emulation
- `db/`, `sql/`, `result/`: the canonical guest facade (namespaces `mududb.db`, `mududb.sql`, `mududb.result`) — `Database`, `SqlStmt`/`SqlParams`, `ResultSet`/`Row`
- `Mudu.cs`: the main public wrapper entry for application code (namespace `mududb`)
- `MuduFileSystem.cs`: the public fs facade (`mududb.fs.MuduFileSystem`)
- `Mudu.Api.csproj`: the library project file

## Dependencies

The library project already references:

- `MessagePack`
- `Microsoft.Data.Sqlite`

## Consuming the library

From NuGet:

```bash
dotnet add package mududb
```

then in code:

```csharp
using mududb;        // Mudu entry point, CommandResponse / QueryResponse
using mududb.types;  // UniCommandArgv, UniQueryArgv, UniSqlStmt, ...
using mududb.fs;     // MuduFileSystem
using mududb.sys;    // MuduSysCallApi (low-level)
```

For development against this repository, a project reference works too:

```xml
<ItemGroup>
  <ProjectReference Include="path\to\mudu_api\csharp\Mudu.Api.csproj" />
</ItemGroup>
```

## Public Entry

Application code should reference:

- `mududb.Mudu`

This file also exports common `uni` types through `global using`, so consumers can directly use types such as:

- `UniCommandArgv`
- `UniQueryArgv`
- `UniCommandResult`
- `UniQueryResult`
- `UniError`
- `UniSqlStmt`
- `UniSqlParam`
- `UniTupleRow`

## Runtime Modes

`mududb.sys.MuduSysCallApi` supports two backends:

1. Real wasm syscall backend
2. SQLite + in-memory fs mock backend

The backend is selected at runtime by `MuduSysCallApi.UseMockBackend`.

### Real syscall backend

Default behavior (`UseMockBackend == false`).

This uses the implementation under:

- `mudu_sys/`

and calls the imported WIT functions:

- `system.query`
- `system.fetch`
- `system.command`
- the `system.fs-*` family

All syscalls except `fetch` transport SyscallPayload v1 (MSSP) frames:
a 16-byte big-endian header (magic `MSSP`, version 1, flags 0, message kind)
plus a MessagePack body, encoded/decoded by the generated `mudu_sys/UniSyscall.cs` codec.
`fetch` has no MSSP route on the host yet and keeps its raw byte path.

### SQLite mock backend

Enable it at startup:

```csharp
using mududb.sys;

MuduSysCallApi.UseMockBackend = true;
```

(The flag defaults to `true` only when the library itself is compiled with
the `MUDU_MOCK_SQLITE` symbol; referencing projects set the flag explicitly.)

In this mode, `MuduSysCallApi` uses:

- `mock/MockSqliteMuduSysCall.cs` (MSSP routing + SQLite for query/command)
- `mock/MockFsEmulation.cs` (in-memory fs for the `fs-*` kinds)

The mock stores SQL data in a local SQLite file; fs content lives in process
memory only.

Database path selection:

- environment variable `MUDU_MOCK_SQLITE_PATH`
- otherwise defaults to `AppContext.BaseDirectory/mudu_mock.db`

## Facade (`mududb.db` / `mududb.sql` / `mududb.result`)

The canonical guest facade (`doc/dev/binding_api_surface.md`, section
"Canonical facade surface") sits on top of the sys tier: `mududb.db.Database`
is the session handle, `mududb.sql.SqlStmt` / `mududb.sql.SqlParams` carry the
statement and its values, and `mududb.result.ResultSet` / `mududb.result.Row`
expose query rows by index or column name. The static `mududb.Mudu` class and
`mududb.sys.MuduSysCallApi` (used in the sections below) are the low-level
sys-tier API behind the facade — prefer the facade for application code.

```csharp
using mududb.db;
using mududb.sql;
using mududb.sys;

MuduSysCallApi.UseMockBackend = true; // mock only; omit under the wasm host

var db = Database.Open(); // default worker; Database.Open("1234") opens worker oid 1234
db.Command(new SqlStmt("CREATE TABLE demo (id INTEGER PRIMARY KEY, name TEXT)"));
var affected = db.Command(
    new SqlStmt("INSERT INTO demo (id, name) VALUES (?, ?)"),
    new SqlParams().Bind(0, 1).Bind(1, "alice")); // positional; BindNamed("name", v) for :name

var rs = db.Query(new SqlStmt("SELECT id, name FROM demo ORDER BY id"));
while (rs.Next())
{
    var row = rs.CurrentRow();
    var id = row.Value(0);               // UniDataValue by index
    var name = row.ValueByName("name");  // ... or by column name
}

db.Close();
```

## Basic Usage

### Command

```csharp
using mududb;
using mududb.types;

var argv = new UniCommandArgv
{
    Oid = new UniOid { H = 0, L = 0 },
    Command = new UniSqlStmt
    {
        SqlString = "insert into demo(name) values(?)"
    },
    ParamList = new UniSqlParam
    {
        Params = new()
        {
            new mududb.types.UniDataValueScalar
            {
                Inner = new mududb.types.UniScalarValueString
                {
                    Inner = "alice"
                }
            }
        }
    }
};

var result = Mudu.Command(argv);
if (result.IsOk)
{
    var affectedRows = result.AffectedRows;
}
else
{
    var error = result.Error;
}
```

### Query

```csharp
using mududb;
using mududb.types;

var argv = new UniQueryArgv
{
    Oid = new UniOid { H = 0, L = 0 },
    Query = new UniSqlStmt
    {
        SqlString = "select id, name from demo where name = ?"
    },
    ParamList = new UniSqlParam
    {
        Params = new()
        {
            new mududb.types.UniDataValueScalar
            {
                Inner = new mududb.types.UniScalarValueString
                {
                    Inner = "alice"
                }
            }
        }
    }
};

var result = Mudu.Query(argv);
if (result.IsOk)
{
    var tupleDesc = result.TupleDesc;
    var rows = result.ResultSet;
}
else
{
    var error = result.Error;
}
```

## API Layers

### High-level wrapper

Use these in normal application code:

- `mududb.Mudu.Command(UniCommandArgv)`
- `mududb.Mudu.Query(UniQueryArgv)`
- `mududb.fs.MuduFileSystem.FsOpen/FsClose/FsRead/FsWrite/FsPread/FsPwrite/FsLseek/FsFstat/FsStat/FsFsync/FsReaddir`

Return values:

- `CommandResponse`
- `QueryResponse`
- `FsResponse<T>` / `FsResponse`

These wrappers provide:

- `IsOk`
- `IsErr`
- `Result` (value-returning calls)
- `Error`
- `RequireOk()`

`FsResponse.RequireOk()` maps the fs errno to a BCL exception:
ENOENT (2) -> `FileNotFoundException`, EACCES (13) ->
`UnauthorizedAccessException`, EINVAL (22) -> `ArgumentException`,
anything else (including EBADF/ENOTDIR/EISDIR) -> `IOException`.

### Low-level syscall API

Use these only when you need raw transport or custom serialization handling:

- `mududb.sys.MuduSysCallApi.SysCommand(UniCommandArgv)`
- `mududb.sys.MuduSysCallApi.SysQuery(UniQueryArgv)`
- `mududb.sys.MuduSysCallApi.SysFsOpen/SysFsClose/.../SysFsReaddir` (the 11 fs syscalls)
- `mududb.sys.MuduSysCallApi.CommandRaw(byte[])`
- `mududb.sys.MuduSysCallApi.QueryRaw(byte[])`
- `mududb.sys.MuduSysCallApi.FetchRaw(byte[])`
- `mududb.sys.MuduSysCallApi.FsOpenRaw(byte[])` and the other fs raw entry points

## Notes

- The mock backend currently supports scalar and binary parameter values.
- The mock query path currently maps SQLite result columns into `uni` result rows and tuple descriptions.
- `fetch` in mock mode currently returns the input bytes unchanged.

## Regenerating the universal codec

`uni/*.cs` (the universal DTOs, namespace `mududb.types`) and
`mudu_sys/UniSyscall.cs` (`MessageKind`, `SyscallResult<T>`, the
`SyscallPayload` MSSP frame codec, the per-function `UniSyscall` stubs and
the resolver-free `UniCodec` helper, namespace `mududb.codec`) are
**mgen-generated** (`// Generated by mudu_gen. Do not edit manually.`) from
`crates/common/mudu_binding/wit/`. mgen emits the canonical
`mududb.types` / `mududb.codec` namespaces directly. To regenerate after a
WIT change:

```bash
cd crates/tools
cargo run -p mudu_gen --bin mgen -- message \
    -i ../common/mudu_binding/wit -o /tmp/mgen_cs -l csharp --with-func-codec

# universal DTOs (namespace mududb.types)
cp /tmp/mgen_cs/Uni*.cs ../sdk/mudu_api/csharp/uni/
rm ../sdk/mudu_api/csharp/uni/UniSyscall.cs

# MSSP frame codec (namespace mududb.codec): same generated file, kept
# alongside the hand-written syscall interop
cp /tmp/mgen_cs/UniSyscall.cs ../sdk/mudu_api/csharp/mudu_sys/UniSyscall.cs
```

Hand-maintained files that regeneration must NOT delete:

- `mudu_sys/UniRelationDelta.cs` — `uni-syscall.wit` carries relation deltas
  as inline `tuple<u64, u8, list<u8>>` elements, so no WIT type exists for
  mgen to emit; `MuduSysCallApi.SysRelationUpdate` converts
  `UniRelationDelta[]` to that tuple form before encoding (same
  `[attr, op, datum]` wire shape).
- `mudu_sys/MuduSysCallApi.cs`, `mudu_sys/WasmMuduSysCall.cs`,
  `mudu_sys/Api*.cs` (WIT import interop), `mock/*` — these route through
  the generated codec but are hand-written.

The generated C# is **resolver-free on the default path**: with `null`
`MessagePackSerializerOptions` it never constructs options (and therefore
never touches `MessagePackSecurity` or the Reflection.Emit resolver chain),
so it runs on the wasi-wasm NativeAOT target. Passing explicit options still
delegates to `MessagePackSerializer` unchanged. Inside generated DTO
formatters, nested fields are written/read statically (raw
`MessagePackWriter`/`MessagePackReader` calls and direct `XFormatter`
instances), byte-identical to the resolver path; custom-resolver options are
honored only at the top-level entry points.


## Demo

A minimal runnable demo is available under:

- `demo/`
- [`demo/README.md`](demo/README.md)

Run it with:

```bash
dotnet run --project mudu_api/csharp/demo/Mudu.Api.Demo.csproj
```

The demo:

- selects the mock backend via `MuduSysCallApi.UseMockBackend`
- creates a local SQLite database
- creates a table
- inserts sample rows
- queries the rows through `Mudu.Query(...)`
- runs an fs write/read roundtrip through `MuduFileSystem`
