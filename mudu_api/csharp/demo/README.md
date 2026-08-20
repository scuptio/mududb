# Mudu.Api Demo

This demo shows how to use the C# `Mudu.Api` library with the SQLite mock backend.

It does the following:

- creates a local SQLite database
- creates a demo table
- inserts sample rows
- queries rows through `Mudu.Command(...)` and `Mudu.Query(...)`
- runs an fs write/read roundtrip through `MuduFileSystem` against the
  in-memory fs emulation

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

## Backend Selection

This demo uses the mock backend. The backend is a **runtime** choice made by
`Program.cs` at startup:

```csharp
MuduSysCallApi.UseMockBackend = true;
```

Backend dispatch happens inside:

- [`MuduSysCallApi.cs`](../mudu_sys/MuduSysCallApi.cs)

Rules:

- when `UseMockBackend` is `true`, `MockSqliteMuduSysCall` (SQLite + in-memory
  fs emulation) is used
- otherwise, the wasm syscall implementation (`ISystem` DllImports) is used

The default of `UseMockBackend` is `true` only when the library itself is
compiled with the `MUDU_MOCK_SQLITE` symbol; a plain consumer (like this
demo) sets the flag explicitly. Note the symbol must be defined for the
**library** build to affect the default — a `DefineConstants` entry in a
referencing project does not propagate into `Mudu.Api`.

## Notes

- The demo is intended for local development and integration testing.
- The mock backend currently supports scalar and binary SQL parameters.
- The fs syscalls are served by the in-memory fs emulation
  ([`MockFsEmulation.cs`](../mock/MockFsEmulation.cs)); its files live in
  process memory only.
