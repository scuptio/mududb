# example/mpk

Guest-SDK example applications for MuduDB. This directory is a **separate
consumer workspace** (its own `Cargo.toml` / `Cargo.lock`): it depends on the
SDK exactly the way an external user would, so it validates the guest SDK,
the mgen/mtp/mpm toolchain, and the `.mpk` packaging flow from a consumer's
point of view.

The cargo workspace has six Rust members (`app1`, `wallet`, `vote`,
`key-value`, `ycsb`, `tpcc`); the remaining examples are
non-Rust projects built with their own language toolchains.

## Examples

| Example                            | Language / toolchain | Description |
| ---------------------------------- | -------------------- | ----------- |
| [app1](app1/)                      | Rust (wasm32-wasip2) | Test procedure package rebuilt from source for the `runtime_wasm` component/procedure tests. |
| [key-value](key-value/)            | Rust (wasm32-wasip2) | Key/value workload on the synchronous kv syscall API. |
| [tpcc](tpcc/)                      | Rust (wasm32-wasip2) | Portable TPC-C style benchmark package. |
| [vote](vote/)                      | Rust (wasm32-wasip2) | Phone-number-based voting application. |
| [wallet](wallet/)                  | Rust (wasm32-wasip2) | The reference example: users, wallets, transactions, and orders. Start here. |
| [wallet-as](wallet-as/)            | AssemblyScript (asc via Node.js/npm) | Wallet example in AssemblyScript. |
| [wallet-c](wallet-c/)              | C (wasm32-targeting clang, wasi-sdk) | Wallet example in C. |
| [wallet-cs](wallet-cs/)            | C# (.NET 10 SDK)      | Wallet example in C#. |
| [wallet-go](wallet-go/)            | Go (TinyGo)           | Wallet example in Go. |
| [wallet-py](wallet-py/)            | Python (componentize-py) | Wallet example in Python. |
| [ycsb](ycsb/)                      | Rust (wasm32-wasip2) | YCSB-style key/value benchmark with a native runner. |

The wallet application exists in six language variants (`wallet` plus the
five `wallet-*` directories), one per supported guest language.

## Building

Each example builds into an installable `.mpk` package with
`cargo make package` run from the example directory (Rust examples produce
`target/wasm32-wasip2/release/<name>.mpk` under the repository-root
`target/`). From the repository root, `cargo make package-wallet` builds the
Rust wallet package; `cargo make package-examples` builds every wallet
language variant.

## Smoke test

`python3 smoke_all.py` builds every example (skipping ones whose external
toolchain is missing), installs each produced `.mpk` into a disposable real
`mudud`, and checks that the app and its procedures respond — the wallet
variants additionally run a create_user/deposit/balance round-trip.
`--skip-build` re-tests the already-built packages, `--only a,b` selects a
subset.

## End-to-end tests

`cargo make test-e2e` (repository root) builds the wallet / wallet-as /
wallet-cs / wallet-py / wallet-go / wallet-c packages first and then runs
the `testing` crate's end-to-end suite. It requires the full external
toolchain set:

- Node.js/npm with the AssemblyScript `asc` compiler (wallet-as)
- .NET 10 SDK (wallet-cs)
- `componentize-py` and `wasm-tools` (wallet-py)
- TinyGo and the Go toolchain plus `wasm-tools` (wallet-go)
- a wasm32-targeting clang such as wasi-sdk, plus `wasm-tools` (wallet-c)

See each example's own README for its exact prerequisites.
