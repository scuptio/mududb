# app1 test package

`app1.mpk` is the prebuilt guest package used by the `runtime_wasm`
component/procedure tests (`procedure_invoke_component`,
`procedure_instance_pool`, `concurrent_wasm_suspension_test`,
`wt_runtime_component_test`). It is built from the in-repo guest crate
[`example/mpk/app1`](../../../../../example/mpk/app1/) (three procedures
`proc_mtp` / `proc2_mtp` / `proc_sys_call_mtp`):

```sh
cd example/mpk/app1 && cargo make package
cp ../../../target/wasm32-wasip2/release/mod_0.mpk \
   ../../../crates/db-kernel/runtime_wasm/data/wasm_module/app1.mpk
```

Rebuild and redeploy it whenever the guest↔host wire format changes;
`proc.toml` mirrors the package descriptor for reference.
