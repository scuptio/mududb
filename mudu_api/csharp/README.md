# Mudu.Api C# Library

This directory contains a reusable C# library for calling MuduDB system APIs.

It includes:

- `uni/`: MessagePack models used by the syscall layer
- `mudu_sys/`: real wasm syscall bindings and `MuduSysCallApi`
- `mock/`: a SQLite-backed mock implementation compatible with `SysCommand` and `SysQuery`
- `Mudu.cs`: the main public wrapper entry for application code
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
2. SQLite mock backend

The backend is selected by conditional compilation.

### Real syscall backend

Default behavior when `MUDU_MOCK_SQLITE` is not defined.

This uses the implementation under:

- `mudu_sys/`

and calls the imported WIT functions:

- `system.query`
- `system.fetch`
- `system.command`

### SQLite mock backend

Enable the symbol:

```xml
<PropertyGroup>
  <DefineConstants>$(DefineConstants);MUDU_MOCK_SQLITE</DefineConstants>
</PropertyGroup>
```

In this mode, `MuduSysCallApi` uses:

- `mock/MockSqliteMuduSysCall.cs`

The mock stores data in a local SQLite file.

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
            new universal.UniDatValuePrimitive
            {
                Inner = new universal.UniPrimitiveValueString
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
            new universal.UniDatValuePrimitive
            {
                Inner = new universal.UniPrimitiveValueString
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

Return values:

- `CommandResponse`
- `QueryResponse`

These wrappers provide:

- `IsOk`
- `IsErr`
- `Result`
- `Error`
- `RequireOk()`

### Low-level syscall API

Use these only when you need raw transport or custom serialization handling:

- `MuduSysCallApi.SysCommand(UniCommandArgv)`
- `MuduSysCallApi.SysQuery(UniQueryArgv)`
- `MuduSysCallApi.CommandRaw(byte[])`
- `MuduSysCallApi.QueryRaw(byte[])`
- `MuduSysCallApi.FetchRaw(byte[])`

## Notes

- The mock backend currently supports primitive and binary parameter values.
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

- enables `MUDU_MOCK_SQLITE`
- creates a local SQLite database
- creates a table
- inserts sample rows
- queries the rows through `Mudu.Query(...)`
