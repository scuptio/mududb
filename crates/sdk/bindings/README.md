# MuduDB Language Bindings

This directory is the proposed home for generated bindings and thin language wrappers.

Rules:

- Wasm/component bindings should derive from `wit/`.
- Native bindings should remain explicitly separated from component bindings.
- Language wrappers should stay thin and must not reimplement core database logic.

## Contents

- `assemblyscript/` — the npm package `@mududb/mududb`: the `assembly/mpack.ts`
  MessagePack runtime plus the mgen-generated MSSP codec in
  `assembly/generated/`, with Node.js corpus runners
  (`run_*_test.mjs`) verifying byte parity against the golden fixtures. The
  `assembly/` DX layer (database/sql/result/fs/syscall/procedure) lets
  AssemblyScript guests import `mududb:api/system` directly and frame MSSP
  in-language — the same direct byte-pipe architecture as the C# guest (see
  `crates/sdk/example/wallet-as`). Canonical subpath entries
  (`@mududb/mududb/types`, `/codec`, `/db`, …) mirror the other languages'
  `mududb.<seg>` paths; see `doc/dev/binding_api_surface.md`.
- `python/` — the PyPI package `mududb` (pure stdlib, zero-dependency)
  counterpart of the AssemblyScript package for tooling/test verification:
  handwritten `mududb/codec/mpack.py` MessagePack runtime (`F32` marker
  class), mgen-generated MSSP codec in `mududb/generated/`
  (`-l python --with-func-codec`), `unittest` suite in `tests/`, and three
  corpus runners (`run_mp_corpus_test.py`, `run_syscall_corpus_test.py`,
  `run_lenient_decode_test.py`). Not a guest path; see `python/README.md` for
  regen and test commands.
