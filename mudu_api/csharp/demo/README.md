# Mudu.Api Demo

This demo shows how to use the C# `Mudu.Api` library with the SQLite mock backend.

It does the following:

- creates a local SQLite database
- creates a demo table
- inserts sample rows
- queries rows through `Mudu.Command(...)` and `Mudu.Query(...)`

## Prerequisites

- .NET SDK 8.0 or newer

## Build

From the repository root:

```bash
dotnet build mudu_api/csharp/demo/Mudu.Api.Demo.csproj
```

## Run

```bash
dotnet run --project mudu_api/csharp/demo/Mudu.Api.Demo.csproj
```

The demo writes the SQLite database to:

```text
<demo output directory>/demo.db
```

It sets `MUDU_MOCK_SQLITE_PATH` automatically in `Program.cs`.

## Conditional Compilation

This demo uses the mock backend by default.

The project file already defines:

```xml
<DefineConstants>$(DefineConstants);MUDU_MOCK_SQLITE</DefineConstants>
```

See:

- [`Mudu.Api.Demo.csproj`](Mudu.Api.Demo.csproj)

### Option 1: Define in the project file

To enable a compile-time symbol in a `.csproj`, add it inside `DefineConstants`:

```xml
<PropertyGroup>
  <DefineConstants>$(DefineConstants);MUDU_MOCK_SQLITE</DefineConstants>
</PropertyGroup>
```

You can add multiple symbols:

```xml
<PropertyGroup>
  <DefineConstants>$(DefineConstants);FOO;BAR;MUDU_MOCK_SQLITE</DefineConstants>
</PropertyGroup>
```

### Option 2: Define from the command line

You can also pass symbols at build time:

```bash
dotnet build mudu_api/csharp/demo/Mudu.Api.Demo.csproj -p:DefineConstants="MUDU_MOCK_SQLITE"
```

Or run with symbols:

```bash
dotnet run --project mudu_api/csharp/demo/Mudu.Api.Demo.csproj -p:DefineConstants="MUDU_MOCK_SQLITE"
```

If you need multiple symbols:

```bash
dotnet build mudu_api/csharp/demo/Mudu.Api.Demo.csproj -p:DefineConstants="FOO;BAR;MUDU_MOCK_SQLITE"
```

## Backend Selection

Backend selection happens inside:

- [`MuduSysCallApi.cs`](../mudu_sys/MuduSysCallApi.cs)

Rules:

- when `MUDU_MOCK_SQLITE` is defined, `MockSqliteMuduSysCall` is used
- otherwise, the wasm syscall implementation is used

## Notes

- The demo is intended for local development and integration testing.
- The mock backend currently supports primitive and binary SQL parameters.
