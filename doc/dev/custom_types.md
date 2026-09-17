# User-Defined Types in Procedures (`.wit` Entities)

Guest procedures are no longer limited to scalar parameters. A project can
declare its own **record types** in a `.wit` file, have mgen generate the
per-language types and codecs, and use those types directly in procedure
signatures — end to end from `mcli --json` through the host down to the
guest and back.

This document is the authoring guide. For the wire contract see
[`../en/abi/guest_host_abi.md`](../en/abi/guest_host_abi.md); for adding a
whole new guest language see [`new_guest_language.md`](new_guest_language.md).

## Quick start

Declare types in `wit/types.wit` (WIT syntax, same files mgen consumes for
the bindings):

```wit
package mududb:my-app;

interface types {
    record address {
        city: string,
        zip: string,
    }

    record profile {
        display-name: string,
        level: u32,
        vip: bool,
        tags: list<string>,
        home: option<address>,
    }
}
```

Generate the language types and transpile with the registry wired (the
mpm-crate templates wire both steps in their `Makefile.toml`):

```bash
mgen message -i wit/types.wit -o gentypes -l go --namespace gentypes
mtp go -i procedures.go -o main_gen.go -m my_app \
    --type-wit wit/types.wit --type-import my_module/gentypes \
    -p package/package.desc.json
```

Then just use the types in procedure signatures (Go shown; every byte-pipe
language has the same capability):

```go
// mudu-proc
func updateProfile(session muduOid, userId int64, p gentypes.Profile) (gentypes.Profile, error)
```

Invoke with nested JSON — object keys are the **WIT field names**:

```bash
mcli app-invoke --app my_app --module my_app --proc update_profile \
  --json '{"user_id":1,"p":{"display-name":"ada","level":7,"vip":1,"tags":["a","b"],"home":{"city":"sh","zip":"200"}}}'
```

## How it works (the 30-second version)

The `mp2-*` component boundary stays an opaque byte pipe
(`list<u8> -> list<u8>`); custom types never change the host or the wire
format:

1. `mgen message` turns `types.wit` into ordinary generated types + codecs
   in your language — the same machinery that generates the `Uni*` binding
   types.
2. `mtp --type-wit` loads the same `.wit` files into a type registry.
   When a procedure signature names a type that is not a builtin scalar,
   mtp resolves it against the registry and emits adapter code that decodes
   the argument with the generated codec.
3. On the wire, a record argument travels as a `uni-data-value` **record
   case**: a positional list of fields in WIT declaration order. A small
   hand-written **bridge** in each binding converts between that envelope
   and the integer-keyed map shape the generated codecs consume.
4. `package.desc.json` describes record parameters with the host's existing
   `Record` type family, so `mcli` knows how to turn nested JSON into the
   wire values (already supported — no host or mcli changes were needed).

## Rules of the JSON / wire surface

- **JSON keys = WIT field names, verbatim** (`display-name`, not
  `displayName` or `display_name`).
- **Booleans cross as 0/1.** The host type system has no Bool family; a
  WIT `bool` field is described as `I32` in the desc and accepts/returns
  JSON `0` and `1` (not `true`/`false`).
- **Records are positional on the wire.** Field names are for the JSON
  surface only; the wire list is in declaration order. Decoders are lenient:
  missing fields default, unknown fields are skipped.
- **`option<T>` fields**: put them last in the record declaration when you
  can — an omitted option field simply leaves the trailing positions empty.
- **Missing field in JSON** is an error (the host's record JSON converter
  is strict); **unknown JSON keys are tolerated**.

## What is supported today (v1)

| Position | record | enum | option<T> | list<T> | nested record | variant |
|---|---|---|---|---|---|---|
| Parameter | ✓ | ✓ (as i32 ordinal) | ✓ (AS: string/record only; C#: not yet) | ✓ | ✓ | ✗ |
| Return value | ✓ | C#/Go-parity (AS/Go/py/c rejected) | ✓ (same per-language limits) | ✓ | ✓ | ✗ |

- Field types may be: any WIT scalar, `string`, `blob`, `option<T>`,
  `list<T>`, or another record from the same registry (no cycles).
- **Variants are rejected** with a clear error: the host type system has no
  Variant family. Express the shape as a record with a tag field for now.
- Recursive/cyclic records are rejected at `mtp` load time (the error names
  the cycle).
- `u128`/`i128` record fields are not expressible in v1 (the C backend has
  no 128-bit record codec; the registry rejects them).
- Byte-array caveats: C# rejects `binary`/`list<u8>` record fields (use
  `blob`); AS accepts them for parameters only (record *returns* with blob
  fields are rejected at mtp time).
- **Rust guests** already have their own custom-entity mechanism
  (`mtp rust --type-desc`); the two mechanisms coexist and will be unified
  later. This document's `--type-wit` flow covers the five byte-pipe
  front-ends (AssemblyScript, C#, Python, C, Go).

## Per-language mechanics

`--type-import` tells the generated adapter where your generated types
live; its meaning is per-language:

| Language | `mgen message` output | `--type-import` | Record bridge |
|---|---|---|---|
| Go | `-o gentypes/types.go --namespace gentypes` (plus an emitted `wire.go` of helpers) | module import path, e.g. `my_module/gentypes` | `bindings/go/types/bridge.go` |
| Python | `-o gentypes.py` (single self-contained module) | module path, e.g. `gentypes` (dotted paths import under an underscore-joined alias) | `bindings/python/mududb/codec/bridge.py` |
| C# | generated `.cs` under your project (`--namespace` sets the C# namespace; dots are sanitized — `WalletCs.Types` becomes `WalletCsTypes`, and `--type-import` must match the generated name) | the generated C# namespace | `mudu_api/csharp/mudu_sys/RecordBridge.cs` |
| C | `-o gentypes/Types.h` (one self-contained header per WIT file, arena-decoded) | include specifier, e.g. `gentypes/Types.h` | `bindings/c/mududb/codec/record_bridge.{h,c}` |
| AssemblyScript | `-o gentypes` (generated `.ts`, importing `./mpack` — the template adds a re-export shim) | import specifier, e.g. `./gentypes` | `bindings/assemblyscript/assembly/record.ts` |

The adapter call shape follows the Go reference (commit `e609f3c6`):
params decode as `TFromValue(RecordFieldValues(p.Params[i]))` and returns
encode as `RecordFromFieldValues(TToValue(result))`, with the exact
spelling per language in the mpm-crate templates. Type syntax in
procedure signatures is the language's own: Go/Python use their type
syntax (`gentypes.Profile`, `Optional[Address]` / `Address | None`), C
uses the annotation form `// mudu-proc (user_id: i64, profile: Profile)
-> Profile` with `option<T>` for nullable.

Notes:

- Return-value integer width differs cosmetically by language (Python
  encodes ints as I64, Go narrows u32 to I32); the host accepts both.
- The C bridge rejects u128/i128 record fields (no 128-bit record codec
  in the C backend).
- C# name shadowing: a table whose name matches a WIT record (e.g.
  `item_info` vs `item-info`) yields a global-namespace mgen entity that
  shadows the record type — qualify record types namespace-fully in
  procedure signatures (mtp strips the qualifier when resolving).
- AS: each mgen entity file defines its own nominal `Box<T>` — import the
  entity's own `Box` (aliased) when two entities with nullable value
  fields are used together.
- Keep DDL comments ASCII-only: a multi-byte character in a `--` comment
  currently derails the SQL tokenizer's offsets for the rest of the file
  (pre-existing `sql_parser` bug).

## Reference example

`crates/sdk/example/wallet-go` carries the full vertical: `wit/types.wit`
(address + profile), the `update_profile` procedure, the flattened
`profile` / `profile_tags` tables, and the e2e assertions in
`crates/db-kernel/testing/tests/linux/wallet_go_mpk.rs`.

Operational note for smoke tests: starting mudud with a pre-populated mpk
directory auto-loads packages at startup — the initdb DDL is deferred and
applied once the kernel is RPC-ready (previously this failed with
"connection refused" because the session opened before the TCP listener).
Installing with `mcli app-install` after startup works as well.

## Future directions

- **Variant support** requires a Variant/tagged family in the host type
  system (`mudu_type`), then registry + adapter support — deliberately
  deferred.
- **ddl.sql table rows as parameter types** (entity unification): table
  columns map to record fields by position; the `mgen entity` backends
  would additionally emit record codecs.
- **Rust front-end unification** onto `--type-wit`.
