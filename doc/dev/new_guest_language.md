# Implementing a New Guest Language

This guide walks through adding a new guest language to MuduDB so that it is
**fully aligned** with the existing six (Rust, AssemblyScript, C#, Python, C,
Go): scaffolded by `mpm-crate`, transpiled by `mtp`, covered by `mgen`
entity/check-sql/message backends, verified against the golden corpus, and
exercised end-to-end in the `testing` suite.

It complements, and does not replace:

- [`../en/abi/guest_host_abi.md`](../en/abi/guest_host_abi.md) — the MSSP v1
  wire contract (frame format, MessagePack body rules, the per-language
  codec checklist). Read it first; nothing here restates it.
- [`binding_api_surface.md`](binding_api_surface.md) — the canonical module
  paths and facade surface every binding must follow.
- [`custom_types.md`](custom_types.md) — user-defined record types in
  procedure signatures (the `--type-wit` flow); relevant when your new
  language needs more than scalar parameters.

## The big picture

A guest procedure is a WASM component exporting one function per procedure:

```wit
export mp2-<proc-name>: async func(param: list<u8>) -> list<u8>;
```

That is the whole contract at the component boundary: **opaque bytes in,
opaque bytes out**. The bytes are an MSSP v1 frame (16-byte big-endian
header + MessagePack body). Inside the guest, the same codec also encodes
the *outgoing* syscalls (SQL/KV/fs) imported from the host
(`uni-syscall.wit`, 23 functions, all `list<u8> -> list<u8>`).

Consequences:

- The host never sees your language's types — only bytes. A new language
  needs no host changes at all.
- Everything a language needs is: (1) produce/consume the exact frame
  bytes, (2) export `mp2-*` functions in canonical-ABI form, (3) package as
  an `.mpk`. Every tool below exists to make (1)-(3) ergonomic.

## What "fully aligned" means

| Capability | Tool | Entry point |
|---|---|---|
| Project scaffolding | `mpm-crate` | `crates/tools/mpm_crate/templates/<lang>/` |
| Procedure transpile (adapter + world.wit + package.desc.json) | `mtp` | `crates/tools/mudu_transpiler/src/<lang>/` |
| Entity codegen from `sql/ddl.sql` | `mgen entity` | `crates/tools/mudu_gen/src/lang_impl/<lang>/` + `templates/<lang>/entity.*` |
| SQL literal checking | `mgen check-sql` | `crates/tools/mudu_gen/src/src_check/extract_<lang>.rs` |
| Formal binding (generated Uni\* types + hand-written codec) | `mgen message` | `crates/tools/mudu_gen/src/lang_impl/<lang>/` + `crates/sdk/bindings/<lang>/` |
| Golden-corpus verification | per-language runner | `crates/db-kernel/testing/fixtures/golden/v1/` |
| End-to-end guest in CI | `testing` suite | `crates/sdk/example/wallet-<lang>/` + `crates/db-kernel/testing/` |

Pick how far to go: a minimal guest needs only the first two rows (the
scaffold carries a self-contained codec); full alignment is all seven.

## Step 0 — register the language identity

One enum, four registration points:

- `crates/tools/mudu_gen/src/lang_impl/lang/lang_kind.rs` — add the
  `LangKind` variant: canonical name (`to_str`), `from_name`, file
  `extension`. Note C's extension is `h` (single self-contained header per
  generated unit), chosen so consumers need no source-list changes.
- `crates/tools/mudu_gen/src/src_check/check_driver.rs` — `CheckLang` with
  CLI aliases (`python|py`, `c|cc|cpp`, `go|golang`, ...).
- `crates/tools/mpm_crate/src/scaffold.rs` — the clap value enum for
  `--lang` with the same aliases.
- `crates/tools/mudu_transpiler/src/mtp.rs` — the clap subcommand + alias.

Keep canonical names and aliases identical across all four.

## Step 1 — the mpm-crate template (minimal viable language)

`crates/tools/mpm_crate/templates/<lang>/` holds `*.tmpl` files rendered
with the project-name placeholders (`{{kebab_name}}` and friends; see
`scaffold.rs` for the derived snake_case/PascalCase names). A template
contains: procedure sources with the `mudu-proc` annotation, `sql/ddl.sql`
+ `init.sql`, `package/package.cfg.json`, a standalone `Makefile.toml`
driving the whole pipeline (`mtp` → compile → `wasm-tools validate` →
`mpm-build`), a `readme.md` listing toolchain requirements, and a
self-contained syscall/codec layer.

Two reference styles:

- **C (self-contained minimal route)** — `templates/c/src/mpack.{c,h}` +
  `mudu_sys.{c,h}`: a hand-rolled MessagePack writer/reader and a
  freestanding syscall shim, no external binding dependency. Right choice
  when the language has no package manager story or you want a zero-dep
  template.
- **Go (vendored component-model route)** — `templates/go/` vendors
  `go.bytecodealliance.org/cm` plus pre-generated `wit-bindgen` output
  under `binding/`, because TinyGo guests need the canonical-ABI glue in
  pure Go. Right choice when the language ecosystem already has
  component-model tooling.

Templates are embedded into the binary (`include_str!`), so after editing
you must reinstall before validating:

```bash
python3 script/build/install_binaries.py --profile dev   # or cargo install --force --locked --path crates/tools/mpm_crate --bin mpm-crate --profile dev --root ~/.cargo
```

Validate by scaffolding + building + installing + invoking for real (the
per-language e2e procedure is in each template's `readme.md`; Python needs
`--sdk-path` pointing at the repo checkout).

## Step 2 — the mtp front-end

`mtp` turns marked procedure sources into the adapter source +
`wit/world.wit` + `package.desc.json` trio. To add a front-end:

1. Create `crates/tools/mudu_transpiler/src/<lang>/` (copy the smallest
   existing one — C's is a good minimal read).
2. Reuse `src/common/`: `wit.rs` (world/adapter rendering, with
   `extra_world_lines` for special worlds — Go uses it for
   `include wasi:cli/imports@0.2.0;`), `desc.rs` (package.desc.json),
   `ident.rs` (identifier mapping). Do not fork these.
3. Define the annotation convention for the language (`# mudu-proc`,
   `// mudu-proc`, with a typed signature where the language ABI is
   untyped — see the C front-end's `(item_id: i64, name: string) -> i64`
   header comment) and the WIT-type ↔ language-type map.
4. Register the subcommand in `mtp.rs` and migrate the template's
   `Makefile.toml` to the mtp pipeline (hand-maintained wit/desc files are
   forbidden afterwards — mtp output must be byte-identical on re-run).

## Step 3 — mgen entity backend

`mgen entity -l <lang>` turns `sql/ddl.sql` tables into row structs +
encode/decode + typed helpers. Add a backend under
`crates/tools/mudu_gen/src/lang_impl/<lang>/` plus
`templates/<lang>/entity.*.jinja`, then wire the `mgen entity` task into
the template's `Makefile.toml` and make the sample procedure use the
generated entity.

Constraints are the backend's job to enforce at generation time (the C
backend rejects `Blob` and U128/I128 columns; Go avoids generics for
TinyGo). Follow the C/Go precedent (commit `552771b3`).

## Step 4 — mgen check-sql

`mgen check-sql` validates SQL string literals in procedure sources against
`sql/ddl.sql` (see [`sql_subset.md`](sql_subset.md)). To add a language:

1. Add its tree-sitter grammar to `crates/tools/mudu_gen/Cargo.toml`
   (crate-local pin compatible with the workspace tree-sitter line).
2. Write `src/src_check/extract_<lang>.rs`: walk the parse tree, find
   string literals passed as SQL, hand them to the shared checker.
3. Register in `check_driver.rs` (`CheckLang` + aliases).
4. Wire the `check-sql` task into the template pipeline (`--input` per
   source file).

## Step 5 — the formal binding + golden corpus

This is the deepest step; budget accordingly (C and Go each took a focused
session — see commit `323edec4`).

**mgen message backend** (`crates/tools/mudu_gen/src/lang_impl/<lang>/`):

- Implement `render_<lang>.rs` branches for Record / Variant / Enum /
  FuncHeader / Func plus the statement-emitter `<lang>_codec.rs`, and
  `templates/<lang>/{file,record,variant,enum,func_header,func}.*`.
- The AssemblyScript backend is the reference for explicit statement
  emission; the Go backend shows the value-model alternative.
- `src_gen/gen_message.rs` decides the header comment style and module
  index per language — check your language lands in the right branch (C and
  Go needed nothing: `//` comments, no index).
- Output must be **byte-stable**: two runs produce identical bytes, and the
  checked-in binding files must match a fresh regen exactly (enforced by
  `script/ci/check_bindings_regen.sh`).

**Binding package** (`crates/sdk/bindings/<lang>/`): canonical segments
`mududb.types` (generated `Uni*` files) + `mududb.codec` (hand-written
MessagePack runtime + MSSP frame codec, unless the frame codec is
generated like the C/AS/Py ones), plus a `README.md` documenting ownership
conventions and the type map. Keep the hand-written layer thin.

**Golden corpus runner**: replay the authoritative fixtures produced by
the Rust host codec — `mp_primitives_v1` (44 vectors), `syscall_payload_v1_all`
(47 frames), `lenient_decode_v1` (8 vectors) — with native tooling (no WASM
needed): byte-exact request/response encodes, semantic equality against the
JSON sidecars, decode → re-encode roundtrips. The C driver (`tests/corpus_driver.c`)
and the Go `corpus/` package are the two reference harnesses.

**Gates**: add your language to `script/ci/check_bindings_regen.sh` and a
corpus job to `.github/workflows/bindings.yaml`; document the module path
in [`binding_api_surface.md`](binding_api_surface.md) and `AGENTS.md`.

## Step 6 — e2e wallet guest

Clone `crates/sdk/example/wallet-py` (or `-go`) into `wallet-<lang>` with
identical procedure semantics, built through the mtp pipeline. Then in
`crates/db-kernel/testing/`:

- add build/copy tasks to `Makefile.toml` (mirror the `wallet-py` tasks)
  and mirror any root `Makefile.toml` references;
- add `tests/wallet_<lang>_mpk.rs` + `tests/linux/wallet_<lang>_mpk.rs`
  (install → list → invoke every procedure → error arm → SQL
  verification), reusing `testing::support` helpers;
- **commit the built `mpk/wallet-<lang>.mpk`** — CI runs the suite without
  guest toolchains.

Validate with `cd crates/db-kernel/testing && cargo make test` (whole
suite green) and `cargo clippy -p testing --all-targets -- -D warnings`.

## Battle scars (read before starting)

- **`list<u8>` has two wire shapes.** MessagePack **bin** in func
  signatures, **array of u8** inside records/variants; decoders must
  accept both everywhere. This is the single most common interop bug.
- **Canonical encoding is pinned.** Minimal-width integers, str8 for
  32..=255, map keys ascending — the corpus checks byte-exactness, not just
  semantic equality.
- **Canonical ABI details matter.** For `func(list<u8>) -> list<u8>`
  exports, parameters lower to a `(ptr, len)` pair and the result returns
  through a caller-supplied return area (retptr); the guest must export
  `cabi_realloc`. Hand-rolled wrappers (the C template's `mp2-*` shims)
  must match this exactly or the component traps on instantiation.
- **C freestanding builds need `-fno-builtin`.** Otherwise clang compiles
  your own `memcpy`/`memset` into recursive calls to themselves. The
  template and binding both build with `-Wall -Wextra -Werror` under
  **both** `clang -std=c99` and `clang++ -x c++` — headers must be clean
  in C++ too (`static inline` triggers `-Wunused-function` there; the
  `MP_INLINE` macro abstracts this).
- **C header cycles**: cross-file type references (UniDataType ⇄
  UniRecordType ⇄ UniResultType) only compile with forward declarations,
  pointer payloads for named record/variant cases, and two-phase
  (early/late) includes. Copy the C binding's layout rather than inventing
  your own.
- **TinyGo is a second toolchain.** Building the guest needs TinyGo while
  corpus tests and tools need stock Go — keep both on `PATH`, pin the
  `go.mod` directive to the minimum supported version, and vendor the `cm`
  package (template does this already). Go worlds need
  `include wasi:cli/imports@0.2.0;` (mtp `extra_world_lines`) plus a
  bindings-regen step.
- **componentize-py guests start cold** (tens of seconds in e2e) — that is
  inherent, not a regression; and the python template's e2e needs
  `--sdk-path` or the bindings placeholder stays `CHANGE_ME`.
- **Tools embed their templates.** After touching `mpm-crate`/`mgen`/`mtp`
  templates or backends, reinstall the binaries before validating e2e —
  stale installed binaries are the classic "my fix doesn't work" trap.
- **mudud e2e hygiene**: use a scratch `mudud.cfg` with a non-default
  `pg_listen_port`, artifacts only under `target/tmp/`, and kill with
  `pkill -x mudud` (never `pkill -f "mudud serve"` — that pattern matches
  your own shell).

## Final validation checklist

```bash
# tools workspace quality gates
cd crates/tools && cargo fmt -- --check \
  && cargo clippy --workspace --all-targets -- -D warnings \
  && cargo test --workspace

# binding regen byte-clean across ALL languages
bash script/ci/check_bindings_regen.sh

# golden corpus for the new language (native runner)
#   e.g. cd crates/sdk/bindings/<lang> && <its test command>

# template e2e: scaffold → cargo make package → mudud install → invoke (+ error arm)

# full e2e suite
cd crates/db-kernel/testing && cargo make test
```
