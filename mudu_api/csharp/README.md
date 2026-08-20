# Mudu.Api C# Library

This directory contains a reusable C# library for calling MuduDB system APIs.

It includes:

- `uni/`: MessagePack models used by the syscall layer
- `mudu_sys/`: real wasm syscall bindings, the MSSP frame codec (`SyscallPayload.cs`) and `MuduSysCallApi`
- `mock/`: a SQLite-backed mock implementation compatible with `SysCommand` and `SysQuery`, plus an in-memory fs emulation
- `Mudu.cs`: the main public wrapper entry for application code
- `MuduFileSystem.cs`: the public fs facade (`MuduFileSystem`)
- `Mudu.Api.csproj`: the library project file

## Dependencies

The library project already references:

- `MessagePack`
- `Microsoft.Data.Sqlite`

## Project Reference

Reference this project from another C# project:

```xml
<ItemGroup>
  <ProjectReference Include="path\to\mudu_api\csharp\Mudu.Api.csproj" />
</ItemGroup>
```

## Public Entry

Application code should reference:

- `Mudu.Api.Mudu`

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

`MuduSysCallApi` supports two backends:

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
plus a MessagePack body, encoded/decoded by `mudu_sys/SyscallPayload.cs`.
`fetch` has no MSSP route on the host yet and keeps its raw byte path.

### SQLite mock backend

Enable it at startup:

```csharp
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

## Basic Usage

### Command

```csharp
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
            new universal.UniDatValueScalar
            {
                Inner = new universal.UniScalarValueString
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
            new universal.UniDatValueScalar
            {
                Inner = new universal.UniScalarValueString
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

- `Mudu.Command(UniCommandArgv)`
- `Mudu.Query(UniQueryArgv)`
- `MuduFileSystem.FsOpen/FsClose/FsRead/FsWrite/FsPread/FsPwrite/FsLseek/FsFstat/FsStat/FsFsync/FsReaddir`

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

- `MuduSysCallApi.SysCommand(UniCommandArgv)`
- `MuduSysCallApi.SysQuery(UniQueryArgv)`
- `MuduSysCallApi.SysFsOpen/SysFsClose/.../SysFsReaddir` (the 11 fs syscalls)
- `MuduSysCallApi.CommandRaw(byte[])`
- `MuduSysCallApi.QueryRaw(byte[])`
- `MuduSysCallApi.FetchRaw(byte[])`
- `MuduSysCallApi.FsOpenRaw(byte[])` and the other fs raw entry points

## Notes

- The mock backend currently supports scalar and binary parameter values.
- The mock query path currently maps SQLite result columns into `uni` result rows and tuple descriptions.
- `fetch` in mock mode currently returns the input bytes unchanged.

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
