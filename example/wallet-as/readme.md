# wallet-as

AssemblyScript wallet MPK example.

This example mirrors the `example/wallet` schema and shows the AssemblyScript
procedure component flow:

1. `assembly/procedures.ts` contains procedures marked with `/**mudu-proc*/`.
2. `mtp assembly-script` generates `generated/procedures.gen.ts`,
   `generated/procedures.gen.rs`, `generated/procedures.gen.wit`, and
   `package/package.desc.json`.
3. `asc` compiles the generated adapter into an AssemblyScript core wasm.
4. `wasm-tools component embed/new` componentizes the AssemblyScript wasm.
5. Cargo builds the generated Rust P2 wrapper component.
6. `wasm-tools compose` links the Rust wrapper imports to the AssemblyScript
   adapter exports and writes `wasm/wallet_as.wasm`.
7. `mpk create` packages the composed component, generated desc, and wallet SQL
   into `target/wallet-as.mpk`.

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
AssemblyScript core wasm, componentizes it, composes it with the Rust wrapper,
validates the result, and writes `target/wallet-as.mpk`.

For the older packaging-only path that uses the checked-in placeholder wasm:

```sh
cargo make package-placeholder
```
