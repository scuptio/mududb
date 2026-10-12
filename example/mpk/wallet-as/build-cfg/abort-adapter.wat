;; Adapter module satisfying the AssemblyScript runtime's `env.abort` import
;; (signature: (message, fileName, line, column) -> void). The `abort` import
;; is only reached on unrecoverable guest errors (failed assertions, OOB
;; checks, explicit `unreachable`), so trapping here preserves AS semantics.
;; Consumed by `wasm-tools component new --adapt env=...` in Makefile.toml.
(module
  (func $abort (param i32 i32 i32 i32) unreachable)
  (export "abort" (func $abort)))
