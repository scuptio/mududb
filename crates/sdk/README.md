# crates/sdk

The SDK group of the MuduDB workspace: the app-facing `mududb` guest SDK
crate, the standalone `mudu_api` language SDKs (Rust and C#), the
`mudu_api_sync` mirroring helper, the AssemblyScript / C / Go / Python
language bindings under `bindings/`, and the `example/` consumer workspace
of guest example applications. Per the layering rules in
[ARCHITECTURE.md](../../ARCHITECTURE.md), crates in this group may depend on
`common` but not on `db-kernel` or `tools`. See the root
[AGENTS.md](../../AGENTS.md) for build and lint conventions.

## Members

| Entry                             | Purpose |
| --------------------------------- | ------- |
| [mududb](mududb/)                 | App-facing facade crate for MuduDB guest code (the Rust SDK): re-exports core types, contract helpers, bindings, and syscalls behind a small set of feature flags. |
| [mudu_api](mudu_api/)             | Container for the standalone sub-bindings: `rust/` (async Rust SDK, its own cargo workspace) and `csharp/` (C# library, published on NuGet as `mududb`). |
| [mudu_api_sync](mudu_api_sync/)   | Workspace-only helper whose `build.rs` mirrors the `mudu_binding` `universal/` sources, WIT file, and manifests into `mudu_api/rust` so the standalone SDK stays in sync. |
| [bindings](bindings/)             | Language bindings for AssemblyScript, C, Go, and Python. |
| [example](example/)               | Separate consumer workspace with the guest-SDK example applications (wallet and its language variants, vote, key-value, tpcc, ycsb, ...). |

## Workspace layout

- The cargo workspace rooted here has two members: `mududb` and
  `mudu_api_sync`.
- `mudu_api/rust` is a standalone workspace so it can be copy/pasted or
  distributed on its own; it does not depend on repository workspace crates.
- `example/` is a separate consumer workspace that validates the SDK from a
  consumer's point of view.
- `bindings/` holds non-Rust language projects (AssemblyScript, C, Go,
  Python) and is not part of the cargo workspace.

The shared source of truth for all bindings is
[`../common/mudu_binding/wit/`](../common/mudu_binding/wit/); see
[doc/dev/binding_api_surface.md](../../doc/dev/binding_api_surface.md) for
the canonical cross-language API surface.
