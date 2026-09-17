# py-spike: Python guest toolchain spike

Minimal componentize-py guest proving the Python→WASM guest path end to end:
a CPython component exporting `mp2-hello` (the mp2 byte-pipe shape) is
installed into a real mudud backend, invoked over HTTP, and performs an
open/close-session roundtrip through the `mududb:api/system` imports —
both directions of the MSSP ABI, with the `mududb` Python package
(`crates/sdk/bindings/python`) framing MSSP inside the guest.

## Layout

- `app.py` — the guest module: decodes the `UniProcedureParam` argument,
  opens/closes a session via the host imports, returns a text result through
  the canonical `{0: UniProcedureResult}` encoding.
- `wit/world.wit` — the spike world: `import mududb:api/system` +
  `export mp2-hello: func(list<u8>) -> list<u8>`.
- `wit/deps/api/api.wit` — copy of `crates/common/sys_interface/wit/sync/api.wit`.
- `package.cfg.json` / `package.desc.json` / `ddl.sql` / `initdb.sql` — mpk metadata.
- `build.sh` — componentize + validate + zip into `spike.mpk`.

## Build and test

```sh
pipx install componentize-py==0.25.1
bash build.sh   # produces spike.mpk

# end-to-end (real mudud backend; skips gracefully when spike.mpk is absent):
cd ../../../db-kernel  # crates/db-kernel
PY_SPIKE_MPK=../sdk/example/py-spike/spike.mpk \
    cargo test -p testing --test py_spike_mpk

# runtime-path unit test (no worker; ignores import side effects):
cargo test -p mudu_runtime --lib python_guest_spike -- --ignored
```

## Measured numbers (componentize-py 0.25.1, Python 3.14)

- component size: **~18.4 MB** (CPython runtime dominates; the wallet-scale
  app code and the `mududb` codec package are negligible).
- e2e test wall time: ~40 s, dominated by CPython cold start on first
  instantiation (AS/Rust guests are effectively instant in comparison).
  This is the route's inherent cost; fine for stored procedures that are
  instantiated once and invoked repeatedly.
