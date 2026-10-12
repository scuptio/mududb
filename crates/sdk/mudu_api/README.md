# mudu_api

Container directory for the standalone MuduDB language SDKs: the async Rust
SDK and the C# library. These are the two bindings that do not live under
[`../bindings/`](../bindings/) (which carries AssemblyScript, C, Go, and
Python).

## Contents

- [`rust/`](rust/) — the async Rust SDK, published/distributed as a
  self-contained package. It is **its own cargo workspace** (rooted at
  `rust/Cargo.toml`) and does not depend on repository workspace crates, so
  it can be copy/pasted or shipped independently. Its `universal/` sources,
  WIT file, and manifests are kept in sync with the main repository by the
  [`mudu_api_sync`](../mudu_api_sync/) build script. Includes a runnable
  demo under `rust/demo/`.
- [`csharp/`](csharp/) — the C# library, published as the NuGet package
  `mududb`. Provides the `mududb.types` / `mududb.codec` generated layer,
  the canonical `mududb.db` / `mududb.sql` / `mududb.result` facade, a
  SQLite-backed mock backend, and a demo under `csharp/demo/`.

Both sub-bindings are generated from the shared WIT source of truth in
[`../../common/mudu_binding/wit/`](../../common/mudu_binding/wit/); see
[doc/dev/binding_api_surface.md](../../../doc/dev/binding_api_surface.md)
for the canonical cross-language API surface.
