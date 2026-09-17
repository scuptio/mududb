# Cross-Language Binding Surface

This document is the anchor for keeping the MuduDB language bindings uniform:
same logical module paths, same capabilities, minimal per-language
hand-written code.

Bindings exist for AssemblyScript (guest), C# (guest/standalone), Python
(guest/tool), C (types/codec + corpus verification), Go (types/codec +
corpus verification), and Rust (app-facing facade).

## Canonical module paths

Every binding is rooted at `mududb` and uses the same segment names. On npm
the org scope makes the root `@mududb/mududb`; in Go the module is
`github.com/ybbh/mududb_p/bindings/go` with packages `types`/`codec`; the
subpaths below are identical across languages.

| Logical path | Content | AS (`@mududb/mududb`) | C# (`mududb` nupkg) | Python (`mududb`) | C (`bindings/c`) | Go (`bindings/go`) | Rust (`mududb` crate) |
|---|---|---|---|---|---|---|---|
| `mududb.types` | Generated Uni* value types (Oid, Value, UniError, UniMessage, …) | `/types` | `mududb.types` | `mududb.types` | `mududb/types/Uni*.h` | `types` | `mududb::types` |
| `mududb.codec` | MessagePack runtime + MSSP frame codec (`uni_syscall`) | `/codec` | `mududb.codec` | `mududb.codec` | `mududb/codec/mpack.h` (+ generated frame helpers in `UniSyscall.h`) | `codec` | `mududb::codec` (alias of `binding`) |
| `mududb.sql` | SqlStmt / params / ValueList | `/sql` | — (UniSqlStmt lives in `types`) | `mududb.sql` | — | — | `mududb::sql` (alias of `contract`) |
| `mududb.db` | Database entry (`Database.open`, query/command) | `/db` | `mududb.Mudu` (low-level), `mududb.db.Database` | `mududb.db.Database` | — | — | `mududb::db.Database` |
| `mududb.fs` | Filesystem facade | `/fs` | `mududb.fs.MuduFileSystem` | `mududb.fs` | — | — | `mududb::fs` |
| `mududb.result` | Result / ResultSet / Row | `/result` | `mududb.result` | `mududb.result` | in `types` | in `types` | `mududb::result` |
| `mududb.sys` | Low-level syscall API | `/sys` | `mududb.sys.MuduSysCallApi` | `mududb.sys` | — | — | `mududb::sys` |
| `mududb.mock` | SQLite-backed mock backend | — | `mududb.mock` | — | — | — | — |

The root entry (`@mududb/mududb`, `using mududb;`, `import mududb`,
`mududb::…`) re-exports the full public surface in every language, so
generated code only ever imports the root.

## Layers and the single source of truth

```
crates/common/mudu_binding/wit/  (23 × uni-*.wit — THE source of truth)
        │  mgen (crates/tools/mudu_gen, backends: assemblyscript/c/csharp/go/python/rust)
        ▼
generated layer  — mududb.types + mududb.codec in every language.
                   100% mgen output, marked "Do not edit manually".
hand-written layer — thin facades only: db/fs/sys entry points, the
                   MessagePack runtime (AS `mpack.ts`, py `codec/mpack.py`,
                   C `codec/mpack.c` arena writer/reader, Go `codec/mpack.go`
                   + `codec/frame.go`; C# uses MessagePack-CSharp), the
                   per-language record bridges for user-defined types
                   (AS `assembly/record.ts`, C# `mudu_sys/RecordBridge.cs`,
                   py `codec/bridge.py`, C `codec/record_bridge.{h,c}`,
                   Go `types/bridge.go` — see dev/custom_types.md), and the
                   C# mock backend.
```

Rules:

1. **Only WIT changes types and wire formats.** Never hand-edit generated
   files; regenerate all languages in the same commit.
2. **The hand-written layer stays thin.** Facade APIs follow the canonical
   surface below; when adding a facade capability, update this document first,
   then implement it in every language that carries that segment.
3. Known principled exception: `mudu_sys/UniRelationDelta.cs` is hand-written
   because `uni-syscall.wit` carries relation deltas as inline
   `tuple<u64, u8, list<u8>>` elements — no WIT type exists for mgen to emit.
   `MuduSysCallApi.SysRelationUpdate` converts `UniRelationDelta[]` to that
   tuple shape before encoding (same `[attr, op, datum]` wire shape).

## Canonical facade surface (operation level)

Guest-authoring languages align on the same capability set, operation
semantics and names; syntax stays idiomatic (Rust returns `RS<T>`, AS throws,
C# throws). AssemblyScript is the reference implementation. The Python
facade is live since the componentize-py guest toolchain landed (see
`crates/sdk/example/wallet-py`): it ships in the `mududb` package
(`mududb.db` / `mududb.sql` / `mududb.result` / `mududb.fs` / `mududb.sys`)
and runs inside CPython-in-WASM guests.

### `mududb.db` — session handle

| Operation | Semantics |
|---|---|
| `open(uri = "")` | Empty uri → default worker; otherwise a decimal u128 worker object id. Returns a session handle. |
| `close()` | Close the session (consumes the handle). |
| `query(stmt, params)` | Run a SELECT; returns the full row set (host drains all rows into the first response; iteration never goes back to the wire). |
| `command(stmt, params)` | Run INSERT/UPDATE/DELETE; returns affected row count. |
| `batch(stmt, params)` | Batch path; same argv/return shape as `command`. |

Errors surface as the language's native error channel (`RS<T>` in Rust,
thrown exceptions in AS/C#).

### `mududb.sql` — statement and parameters

- `SqlStmt(sql: string)` — the statement text.
- Parameter list: `bind(index, value)` for positional `?` placeholders
  (indices contiguous from 0, no re-binding) **or** `bindNamed(name, value)`
  for `:name` placeholders (resolved host-side; a repeated `:name` reuses the
  value). Positional and named binding cannot be mixed in one list.

### `mududb.result` — row set

- Row set: `next()` advances the cursor (starts before the first row);
  `currentRow()` is valid only after a successful `next()`;
  `columnCount()`; `columnName(i)`; `findColumn(name)`;
  `eof()` once the cursor passes the last row.
- Row: `value` / `isNull` by column index **or** by column name. Invalid
  index / unknown column name / `currentRow()` before `next()` are errors.

### `mududb.fs` — filesystem (session id first)

`fsOpen/fsClose/fsRead/fsWrite/fsPread/fsPwrite/fsLseek/fsFstat/fsStat/
fsFsync/fsReaddir`; flags `FS_O_RDONLY=0, FS_O_WRONLY=1, FS_O_RDWR=2`;
whence `FS_SEEK_SET=0, FS_SEEK_CUR=1, FS_SEEK_END=2`.

### Idiomatic shapes per language

| | AS | C# | Rust | Python |
|---|---|---|---|---|
| open | `Database.open(uri?)` | `Database.Open(uri?)` | `Database::open()` / `open_uri(uri)` | `Database.open(uri="")` |
| query | `db.query(stmt, values?)` | `db.Query(stmt, values?)` | `db.query(&stmt, &params)?` | `db.query(stmt, values?)` |
| params | `new ValueList().bind(0, v)` | `new SqlParams().Bind(0, v)` | `SQLParamValue::from_vec(_)` / `from_vec_named(_)` | `Params().bind(0, v)` |
| row set | `rs.next()` / `rs.currentRow()` | `rs.Next()` / `rs.CurrentRow()` | `rs.next()` / `rs.current_row()?` | `rs.next()` / `rs.current_row()` |
| value | `row.value(0)` / `valueByName` | `row.Value(0)` / `ValueByName` | `row.value(0)?` / `value_by_name(_)?` | `row.value(0)` / `value_by_name(_)` (+ `as_i64` etc.) |

## Behavioral consistency scenario

Every facade implementation covers this exact 7-step scenario in its
integration tests (assertion-identical across languages); harness cleanup
(e.g. `DROP TABLE IF EXISTS`) is allowed before step 1 but is not part of
the scenario:

1. `open` (default worker).
2. `command("CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT)")`.
3. `command("INSERT INTO t (id, name) VALUES (?, ?)")` twice with positional
   params `(1, "alice")`, `(2, "bob")` — each returns affected `1`.
4. `query("SELECT id, name FROM t ORDER BY id")`: `columnCount == 2`,
   `columnName(0) == "id"`, `findColumn("name") == 1`; row 1 = `(1, "alice")`
   (read by index and by name); row 2 = `(2, "bob")`; then `next()` is false
   and `eof()`.
5. `command("UPDATE t SET name = :name WHERE id = :id")` with named params
   `name="carol", id=2` — affected `1`; `SELECT name FROM t WHERE id = 2`
   returns `"carol"`.
6. fs roundtrip: `fsOpen(oid=7, "docs/hello.txt", FS_O_WRONLY)` →
   `fsWrite("hello") == 5` → `fsClose`; reopen `FS_O_RDONLY` →
   `fsLseek(0, FS_SEEK_SET) == 0` → `fsRead(5) == "hello"` →
   `fsPread(1, 3) == "ell"` → `fsFstat().length == 5` → `fsClose`;
   `fsReaddir("docs")` contains `hello.txt`.
7. `close`.

Implementations: Rust `crates/sdk/mududb/tests/facade_scenario.rs`
(standalone adapter), C# `Mudu.Api.Tests/FacadeScenarioTests.cs` (mock
backend), Python `bindings/python/tests/test_facade_scenario.py` (scripted
in-memory transport over the real wire frames), AS covered end-to-end by
the wallet-as example (and Python end-to-end by `wallet-py` +
`testing/tests/linux/wallet_py_mpk.rs`).

## Python guest toolchain notes (componentize-py)

- Pipeline: `procedures.py` → `mtp ... python` (adapter + world WIT + desc)
  → `componentize-py` → `wasm-tools validate` → `mpm-build` (see
  `crates/sdk/example/wallet-py/Makefile.toml`; minimal spike:
  `crates/sdk/example/py-spike`).
- The mtp Python front-end discovers `# mudu-proc` top-level functions; the
  first parameter named `session` is bound from `UniProcedureParam.session`
  (mirroring the AS `id: Oid` rule) and the remaining positional parameters
  arrive as plain Python values (`int`/`str`/`float`/`bool`/`bytes`/`None`).
- Measured: component ~6–19 MB, first instantiation dominated by the
  CPython cold start (tens of seconds in e2e) — inherent to the
  interpreter-in-WASM route; acceptable for install-once, invoke-many
  procedures.

## Versioning and compatibility

- The wire schema is versioned by the golden fixtures:
  `crates/db-kernel/testing/fixtures/golden/v1/` (produced by the Rust host
  codec; authoritative). A schema bump creates `v2/` and all six languages
  add corpus coverage for it.
- Each binding package versions independently (semver), published from tags:
  `as-v*` (npm `@mududb/mududb`), `cs-v*` (NuGet `mududb`), `py-v*` (PyPI
  `mududb`). CI: `.github/workflows/bindings.yaml`. The C and Go bindings
  ship as in-repo source only (no registry/tag release).

## Regeneration

After any WIT or mgen-template change, regenerate everything in one commit:

```bash
cd crates/tools
cargo build -p mudu_gen --bin mgen
MGEN=../../target/debug/mgen

# C# → uni/*.cs (mududb.types), mudu_sys/UniSyscall.cs (mududb.codec)
$MGEN message -i ../common/mudu_binding/wit -o /tmp/mgen_cs -l csharp --with-func-codec
cp /tmp/mgen_cs/Uni*.cs ../sdk/mudu_api/csharp/uni/
rm ../sdk/mudu_api/csharp/uni/UniSyscall.cs
cp /tmp/mgen_cs/UniSyscall.cs ../sdk/mudu_api/csharp/mudu_sys/UniSyscall.cs

# Python → mududb/generated/
$MGEN message -i ../common/mudu_binding/wit -o /tmp/mgen_py -l python --with-func-codec
cp /tmp/mgen_py/*.py ../sdk/bindings/python/mududb/generated/
cp ../common/mudu_binding/src/codec/syscall_payload/syscall_schema.desc.json \
   ../sdk/bindings/python/mududb/generated/

# AssemblyScript → assembly/generated/ (fix the mpack import path one level up)
$MGEN message -i ../common/mudu_binding/wit -o /tmp/mgen_as -l assemblyscript --with-func-codec
for f in /tmp/mgen_as/*.ts; do
  sed 's|from "./mpack"|from "../mpack"|' "$f" > "../sdk/bindings/assemblyscript/assembly/generated/$(basename "$f")"
done

# Go → types/ (package types; wire.go is hand-written — copy only Uni*.go)
$MGEN message -i ../common/mudu_binding/wit -o /tmp/mgen_go -l go --with-func-codec
cp /tmp/mgen_go/Uni*.go ../sdk/bindings/go/types/

# C → mududb/types/ (mududb/codec is hand-written — copy only Uni*.h)
$MGEN message -i ../common/mudu_binding/wit -o /tmp/mgen_c -l c --with-func-codec
cp /tmp/mgen_c/Uni*.h ../sdk/bindings/c/mududb/types/
```

`bash script/ci/check_bindings_regen.sh` verifies the checked-in generated
files match a fresh regen byte-for-byte (CI runs it on every PR touching the
binding sources). Then run every language's corpus tests against the golden
fixtures — that is the capability-parity gate.
