# wallet-cs — C# wallet procedures as a WASI 0.2 component

A C# port of the wallet example procedures, compiled to a WebAssembly
component with componentize-dotnet and packaged as `wallet-cs.mpk`. It mirrors
the AssemblyScript `wallet-as` example one to one: same five procedures, same
i64 signatures, same semantics.

| Procedure | Parameters | Returns |
|---|---|---|
| `create_user` | `user_id: i64, name: string, email: string` | `user_id` |
| `deposit` | `user_id: i64, amount: i64` | new balance |
| `withdraw` | `user_id: i64, amount: i64` | new balance |
| `transfer_funds` | `from_user_id: i64, to_user_id: i64, amount: i64` | new source balance |
| `balance` | `user_id: i64` | current balance |

## How it works

- **mtp-driven artifacts.** `src/Procedures.cs` marks each procedure with a
  `// mudu-proc` comment immediately above the `public static` method. The
  `transpile` task (`mtp csharp`, see `Makefile.toml`) generates the three
  artifacts that used to be maintained by hand and kept in sync manually:
  `src/WalletCsWorldExportsImpl.cs` (the byte-pipe exports class),
  `wit/wallet-cs.wit` (the procedure world), and
  `package/package.desc.json` (the procedure descriptors). Adding a procedure
  means writing the method plus its marker and rebuilding.
- **Byte-pipe procedure ABI.** `wit/wallet-cs.wit` exports one
  `mp2-<proc-name>` function per procedure (`func(list<u8>) -> list<u8>`), the
  names the runtime looks up for component-target P2 guests
  (`MUDU_PROC_P2_PREFIX`, kebab-case). The argument is the
  MessagePack-encoded `UniProcedureParam` (`{1: procedure, 2: session, 3:
  param_list}`, record-as-map keyed by 1-based field numbers); the return
  value is the MessagePack-encoded
  `UniResult<UniProcedureResult, UniError>` — a single-entry map, key `0` =
  ok, key `1` = `UniError` — the same wire format
  `mudu_binding::codec::handle_procedure` produces for Rust guests.
- **Syscalls.** SQL runs through the sync `mududb:api/system.query/command`
  imports framed as SyscallPayload v1 (MSSP): a 16-byte big-endian header
  (magic `MSSP`, version 1, flags 0, message kind) plus a MessagePack body
  (an integer-keyed argument map `{1: argv}` on request, `[ok_tag, value]`
  on response).
- **Codec.** `src/MiniMsgPack.cs` + `src/MuduSys.cs` implement exactly the
  wire shapes the wallet needs, with tag numbers verified against the Rust
  host (`mudu_binding::universal`, `mudu_binding::codec::syscall_payload`).

## Why not the Mudu.Api C# SDK codec?

The original plan was to compile the SDK's `SyscallPayload.cs` /
`MuduSysCallApi.cs` / `uni` DTO sources into the guest. Two hard blockers
surfaced, both in the SDK, neither fixable from the example:

1. **MessagePack-CSharp cannot initialize on wasi-wasm.** Every
   `MessagePackSerializerOptions` construction touches `MessagePackSecurity`,
   whose static initializer eagerly builds a `SipHash` keyed by
   `System.Security.Cryptography.RandomNumberGenerator` — unsupported on
   WASI (`PlatformNotSupportedException` at module-initializer time, still
   true on upstream master). The SDK codec is MessagePack-CSharp based, so it
   is unusable inside the guest. (Separately, its default resolver is
   Reflection.Emit based, which NativeAOT-LLVM cannot run either.)
2. **The SDK's `uni` scalar tags do not match the host.** The host's
   `uni_scalar_value.rs` encodes `I64` as tag `9` and `String` as `14`; the
   SDK's `uni/UniPrimitiveValue.cs` declares `[Union(8, …I64)]` and
   `[Union(12, …String)]`. The SDK was only ever exercised against its own
   mock, so the skew went unnoticed.

Both blockers have since been fixed in the SDK: the scalar tags were
corrected and are pinned against the host by
`Mudu.Api.Tests/UniScalarTagTests.cs`, and the mgen-generated codec
(`uni/*.cs` + `mudu_sys/UniSyscall.cs`) is resolver-free on the default
path, so it runs on the wasi-wasm NativeAOT target (see the regen section of
`crates/sdk/mudu_api/csharp/README.md`). The example deliberately keeps its
own minimal codec — it passes the end-to-end test and has no remaining
reason to switch. The Mudu.Api library sources are **not** modified.

## Requirements

- .NET 10 SDK at `~/.dotnet` (10.0.400 or later; per-user install, no
  system-wide packages). Override with `DOTNET=/path/to/dotnet cargo make`.
- NuGet feeds (already in `nuget.config`): nuget.org plus
  `dotnet-experimental` (hosts
  `BytecodeAlliance.Componentize.DotNet.Wasm.SDK` and the pinned
  `runtime.linux-x64.Microsoft.DotNet.ILCompiler.LLVM`
  `10.0.0-rc.1.26357.1` runtime pack; the build prints a version-skew warning
  against `Microsoft.DotNet.ILCompiler.LLVM` `10.0.0-rc.1.26306.1` — known
  and harmless, same as the spike).
- `wasm-tools` and `cargo-make` on PATH (workspace tooling).
- The csproj sets `IlcExportUnmanagedEntrypoints` (required for the
  component's exported functions) and keeps `wit/deps/mududb-api/api.wit` as
  an unmodified copy of `crates/db-kernel/mudu_runtime/wit/api.wit`.

## Build

```bash
cd crates/sdk/example/wallet-cs
cargo make package
```

This runs `mtp csharp` (transpile the procedures into
`src/WalletCsWorldExportsImpl.cs` + `wit/wallet-cs.wit` +
`package/package.desc.json`), then `dotnet build -c Release` (restore →
compile → componentize; **not** `dotnet publish` — the Wasm.SDK chains
componentization `AfterTargets="Build"`, so a direct publish hits a circular
target dependency), validates the component with `wasm-tools`, and packages
`target/wasm32-wasip2/release/wallet-cs.mpk` with `mpm-build`. The component
file is named `wallet_cs.wasm` so its stem matches the `wallet_cs` module in
`package/package.desc.json` (an `mpm-build` requirement).

## Test

The integration test `crates/db-kernel/testing/tests/wallet_cs_mpk.rs`
installs the packaged app on a real `mudud` and invokes every procedure over
HTTP, asserting balances and the overdraft error path — the same assertions
as the wallet-as test. The prebuilt fixture lives at
`crates/db-kernel/testing/mpk/wallet-cs.mpk`; refresh it with
`cargo make copy-wallet-cs-mpk` in `crates/db-kernel/testing`.

## Runtime notes

Sync-world guests like this one (`use_async: false` in `package.cfg.json`)
are invoked through `ProcedureInvokeComponent::call`, which runs the whole
component on a dedicated thread with no Tokio runtime: the .NET runtime
issues WASI calls at startup whose wasmtime-wasi sync wrappers panic when a
Tokio runtime drives the current thread ("Cannot start a runtime from within
a runtime"). The TCP invoker routes per-app (`package.cfg.json`'s
`use_async`), so async-world apps are unaffected.
