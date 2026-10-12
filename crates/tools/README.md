# crates/tools

Developer tooling group of the MuduDB workspace: the code generator, the
procedure transpiler, the `mpm` package tools, and the `mcli` client. Per
the layering rules in [ARCHITECTURE.md](../../ARCHITECTURE.md), crates in
this group may not depend on `sdk` (and `sdk` may not depend on `tools`).
See the root [AGENTS.md](../../AGENTS.md) for build and lint conventions.

## Crates

| Crate                               | Binary       | Purpose |
| ----------------------------------- | ------------ | ------- |
| [mudu_gen](mudu_gen/)               | `mgen`       | Code generator: entity code from DDL SQL and message/MSSP codec code from WIT, for all six guest languages (Rust, C#, AssemblyScript, Python, C, Go); also the `check-sql` static SQL literal checker. |
| [mudu_transpiler](mudu_transpiler/) | `mtp`        | Transpiler front-end: turns marked procedures (`/**mudu-proc**/` and friends) in Rust, AssemblyScript, Python, C#, C, and Go sources into Mudu procedure artifacts ready for wasm packaging. |
| [mpm_build](mpm_build/)             | `mpm-build`  | Builds `.mpk` package archives from package config/description, SQL, and WASM files; merges procedure-description files. |
| [mpm_crate](mpm_crate/)             | `mpm-crate`  | Scaffolds new `.mpk` application projects (`npm create` / `cargo new` style) for any supported guest language. |
| [mpm_install](mpm_install/)         | `mpm-install`| npm-style installer that pushes a `.mpk` to a server's HTTP management endpoint. |
| [mudu_cli](mudu_cli/)               | `mcli`       | CLI client and administration tool: SQL/KV/procedure data plane over TCP, app and replication administration over HTTP. |

## Installation

From the repository root:

```bash
cargo make install-tools
```

This runs `script/build/install_binaries.py --profile release`, which
installs `mgen`, `mtp`, `mpm-build`, `mpm-crate`, `mpm-install`, `mudud`,
and `mcli` into `~/.cargo/bin` (`mudud` itself is built from the
`db-kernel` group).
