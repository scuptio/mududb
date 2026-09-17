# wallet-as

AssemblyScript wallet MPK example.

This example mirrors the `example/wallet` schema and shows the AssemblyScript
procedure component flow — the same direct byte-pipe architecture as the C#
guest (`example/wallet-cs`): the component imports `mududb:api/system` and
exports root-level `mp2-*` byte-pipe functions; no companion Rust component
is built or composed.

1. `assembly/procedures.ts` contains procedures marked with `/**mudu-proc*/`.
2. `mtp assembly-script` generates `generated/procedures.gen.ts` (per-procedure
   `mp2_<name>` byte-pipe adapters), `generated/procedures.gen.wit` (the
   procedure world: `import mududb:api/system` plus one root-level
   `mp2-<kebab>` export per procedure), and `package/package.desc.json`.
3. `asc` compiles the generated adapter into an AssemblyScript core wasm.
4. `scripts/patch-component-exports.py` renames the `mp2_<name>` /
   `cabi_post_mp2_<name>` core exports to the kebab-case names declared by
   the world WIT.
5. `wasm-tools component embed` uses the generated world WIT (staged to
   `wit/procedures.gen.wit`) together with the checked-in
   `wit/deps/mududb-api/api.wit` copy of the host syscall interface;
   `wasm-tools component new` then componentizes the wasm, with
   `--adapt env=build-cfg/abort-adapter.wat` satisfying the AS runtime's
   `env.abort` import. The result is written directly to
   `target/wasm32-wasip2/release/wallet_as.wasm`.
6. `mpm-build create` packages the component, generated desc, and wallet SQL
   into `target/wasm32-wasip2/release/wallet-as.mpk`.

Build the full component pipeline:

```sh
cargo make package
```

Required tools:

- `wasm32-wasip2` Rust target
- `wasm-tools`
- Node.js, using the checked-in AssemblyScript dependency under
  `bindings/assemblyscript/node_modules`

Current status: the full `cargo make package` pipeline builds the
AssemblyScript core wasm, componentizes it, validates the result, and writes
`target/wasm32-wasip2/release/wallet-as.mpk`.
The resulting package runs end-to-end on a real backend: see
`crates/db-kernel/testing/tests/wallet_as_mpk.rs`, which installs the MPK into
a full `mudud` server and invokes every procedure over HTTP.

For the older packaging-only path that uses the checked-in placeholder wasm:

```sh
cargo make package-placeholder
```
