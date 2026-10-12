# crates/bindings

Home of [`mudu_adapter_ffi`](mudu_adapter_ffi/), the standalone cdylib
workspace that exports the versioned native adapter ABI (dynamic library,
C header, and Node-API bridge) consumed by the SDK runtime.

## Why this group is isolated

`mudu_adapter_ffi` is the only crate in the repository built as a `cdylib`.
A cdylib crate-type makes cargo emit final artifacts under unhashed file
names (`libmudu_adapter_ffi.so`) in the target directory; inside the shared
repository-root `target/` those names would be overwritten by whichever
group workspace built last, and downstream compiles would fail with E0463
("can't find crate"). The crate therefore carries its own `[workspace]`,
stays out of every group workspace, and redirects its build artifacts to
`target/sdk-adapter/` through its own `.cargo/config.toml` (the same
location `script/sdk/adapter_runtime.py` builds into). See
[ARCHITECTURE.md](../../ARCHITECTURE.md) and the
[crate README](mudu_adapter_ffi/README.md).
