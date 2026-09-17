# mududb Python bindings

The `mududb` Python package has two roles:

1. **Guest SDK** — write stored procedures in Python against the canonical
   facade (`mududb.db` / `mududb.sql` / `mududb.result` / `mududb.fs` /
   `mududb.sys`, operation-aligned with the AssemblyScript, C#, and Rust
   guests) and compile them to a WASM component with componentize-py; see
   `crates/sdk/example/wallet-py` for the full pipeline and
   `doc/dev/binding_api_surface.md` for the canonical surface.
2. **Tooling / corpus verification** — the generated MSSP codecs and the
   canonical MessagePack rules are validated byte-exact against the
   host-side Rust codec (`crates/common/mudu_binding/src/codec/
   syscall_payload/`, rmp_serde 1.3.x) with the shared cross-language
   golden fixtures.

Pure stdlib, Python 3.9+, zero install for the test entries: every test entry
bootstraps `sys.path` itself, so nothing needs `pip install`. As a package it
is `pip install mududb` and then the canonical import paths are
`mududb.types`, `mududb.codec.mpack`, `mududb.generated.*`, and the facade
modules — the same `mududb.<seg>` layout as the other language bindings.

## Layout

```
mududb/
  __init__.py            # re-exports all subpackages plus the mpack API
  codec/
    __init__.py          # re-exports the mpack API + procedure + uni_syscall
    mpack.py             # handwritten MessagePack runtime
    procedure.py         # mp2 byte-pipe helpers (decode param / encode result)
  types/
    __init__.py          # facade re-exporting the Uni* types from
                         # mududb.generated (uni_syscall is not a type
                         # module and stays in mududb.codec)
  generated/             # mgen output: 23 uni_*.py modules incl.
                         # uni_syscall.py + syscall_schema.desc.json
  db.py                  # canonical facade: Database session handle
  sql.py                 # SqlStmt + Params (positional/named bind rules)
  result.py              # ResultSet / Row + typed as_* accessors
  fs.py                  # canonical fs segment + FS_O_*/FS_SEEK_* constants
  sys.py                 # syscall transport (set_transport) + low-level wrappers
  errors.py              # MuduError
```

- `mududb/codec/mpack.py` — handwritten MessagePack runtime. Byte-exact with
  rmp-serde 1.3.x canonical encoding (minimal-width integers by value,
  str8/bin8 thresholds, fixarray/fixmap); lenient decoding (any integer
  width, f32/f64 cross-marker reads). `F32` is a `float` marker subclass
  that makes the writer emit f32 (0xCA) instead of f64 (0xCB). Exposes both
  the generic `write_value`/`read_value` API used by the generated codecs
  and the full typed API (`write_u64`/`read_i64`/`read_map_key`/`skip_value`
  /...) aligned with the AssemblyScript `mpack.ts`.
- `mududb/generated/` — the mgen Python backend output (`-l python
  --with-func-codec`), 24 files: 23 modules (including `uni_syscall.py`
  with the per-message-kind frame codecs) plus `__init__.py`. **Do not edit
  manually.** `mududb/generated/syscall_schema.desc.json` is a copy of
  `crates/common/mudu_binding/src/codec/syscall_payload/syscall_schema.desc.json`
  (the host codec's schema descriptor), kept next to the generated code for
  tooling / future descriptor-driven scenarios; the current codecs are
  generated code and do not consume it.
- `tests/test_mpack.py` — unit tests for `mududb.codec.mpack` (minimal
  integer boundaries, str8/bin8 thresholds, lenient reads, nested
  `skip_value`, `F32` marker, out-of-range rejection, `is_done`).
- `corpus_common.py` — shared runner helpers: fixture location (walks up to
  `crates/db-kernel/testing/fixtures/golden/v1/`, no hardcoded absolute
  paths), segment unpacking, sidecar `expect`-shape construction through the
  generated codec, and the deterministic encode inputs pinned against the
  fixture frames.
- `run_mp_corpus_test.py` — MessagePack primitive corpus: 44 vectors,
  encode byte-exact + decode semantic per vector.
- `run_syscall_corpus_test.py` — MSSP syscall corpus: 47 frames (one
  request + one ok response per message kind 1..23, plus a trailing `get`
  UniError response). Header routing, request encode byte-exact, decode vs
  sidecar `expect`, response encode byte-exact + decode/re-encode
  byte-identical + roundtrip decode, err frame field-level (`err_src`
  carries the host's `"None"` quirk verbatim).
- `run_lenient_decode_test.py` — 8 hand-built non-canonical frames (integer
  width widening, unsigned markers for signed values, wide negatives, map
  key reordering, unknown/skipped keys, missing parameters/fields). Decode
  only, no re-encode comparison.

## Regenerating `mududb/generated/`

From `crates/tools`:

```sh
cargo run -p mudu_gen -- message \
    -i ../common/mudu_binding/wit \
    -o <scratch dir> \
    -l python --with-func-codec
```

then copy all `<scratch dir>/*.py` into `mududb/generated/` (the template
already emits `from mududb.codec.mpack import ...`, so no import rewriting is
needed). Refresh `mududb/generated/syscall_schema.desc.json` from the host
codec path above when the schema changes.

## Running the tests

From this directory (or anywhere — the scripts locate the repo fixtures by
walking up):

```sh
python3 -m unittest discover tests -v   # mpack unit tests
python3 run_mp_corpus_test.py           # 44 primitive vectors
python3 run_syscall_corpus_test.py      # 47 MSSP frames
python3 run_lenient_decode_test.py      # 8 non-canonical frames
```

The golden fixtures under `crates/db-kernel/testing/fixtures/golden/v1/` are
produced by the host-side Rust generator and are authoritative — never edit
them; if a comparison fails, suspect the Python implementation first.
