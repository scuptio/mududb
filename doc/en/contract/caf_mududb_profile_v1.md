# MuduDB Application Package Contract v1

## Scope

This document specifies the `.mpk` application package format MuduDB uses to deploy stored procedures. `.mpk` is a profile of the [Computable Artifact Format](https://github.com/scuptio/caf) (CAF) container, replacing the former `.mpk` (ZIP) archive.

The package body is a CAF container responsible for packing, content addressing, signing, and transport; the WASM modules inside are still executed by mududb's own wasmtime host over the MSSP syscall ABI, and the CAF container layer never instantiates them.

## Version history

| Version | Date | Summary |
|---------|------|---------|
| 1 | 2026-11-24 | Initial contract. Direct replacement of `.mpk` (ZIP) with `.mpk` (flat CAF). |

## Forms

| Form | Extension | Purpose |
|------|-----------|---------|
| flat | `.mpk` | server-side storage, runtime loading |
| ZIP export | `.mpk.zip` | distribution, transport (`stored`, byte-deterministic) |

- The flat `.mpk` is the authoritative form; `.mpk.zip` is its deterministic export and reconstructs a byte-identical flat file.
- Detection: flat starts with the `CAF\0` magic; `.mpk.zip` starts with the ZIP magic `PK\3\4`.

## Archive contents

A valid package contains one default payload tree (`mududb`) and zero or more WASM components.

Payload entries (single level):

| Entry | Required | Description |
|-------|----------|-------------|
| `package.cfg.json` | yes | application metadata (`AppInfo`) |
| `package.desc.json` | yes | module/procedure descriptors (`ModProcDesc`) |
| `ddl.sql` | yes | schema DDL |
| `initdb.sql` | packager: yes; loader: no | initial data SQL |
| `abi.json` | yes | ABI version declaration |

`abi.json` replaces the former `.mpk` `package.manifest.json`: `format_version` now lives in the CAF container header, and `syscall_abi_version` in `abi.json`.

```json
{ "syscall_abi_version": 1 }
```

WASM components (`ComponentRef`):

| Field | Value |
|-------|-------|
| `name` | module name (aligned with `package.desc.json`) |
| `world` | `""` (raw core module) |
| `exports` | raw WASM export names |
| `access` | `[]` |

Remaining manifest fields:

| Field | Value |
|-------|-------|
| `name` / `version` | `AppInfo.name` / `AppInfo.version` |
| `format_version` | `1` |
| `interface` | empty section |
| `capabilities` | `[]` |
| `execution` | `profile = "mududb-proc"`, `deterministic = false` |
| `resources` / `policy` | placeholder (mududb does not use caf-runtime) |
| `payloads` | single default payload |

`execution.profile = "mududb-proc"` makes caf-runtime reject the package, i.e. the package is executed only by mududb.

## Loading

The loader (`runtime_wasm`) reads a flat `.mpk`:

1. Verify the `CAF\0` magic and header `format_version`.
2. Parse the section directory and manifest.
3. Verify the signature (below).
4. Read and parse the five payload files; reject missing required entries or an empty `ddl.sql`; align a single module name with the descriptor.
5. Verify `syscall_abi_version`.
6. Collect module bytecode by name.

Sealed artifacts (`FLAG_ENCRYPTED`) are not supported and are rejected.

## Signature verification

- Algorithm ed25519, signature over the root hash (transitively covering all sections).
- The signature section is a CBOR list `[{ alg, key_id, pubkey, sig }]`, with `key_id = SHA256(pubkey)`.
- Every signature must verify; otherwise the package is rejected.
- The trust store is `trusted_key_fingerprints` (ed25519 public-key fingerprints). When non-empty, at least one valid signature from a trusted fingerprint is required; when empty, signatures are verified but trust is not enforced.
- The packager signs with an ed25519 seed (injected via CI/key management); test fixtures use a fixed public test key.

## Versioning and compatibility

- Container version: CAF `format_version` (currently `1`).
- Profile version: contract v1 pins the payload layout and the `abi.json` shape.
- Backward compatibility with the former `.mpk` (ZIP) is not retained; the loader accepts only `.mpk` (flat).

## References

- CAF container format: [container-format](https://github.com/scuptio/caf/blob/main/doc/spec/en/container-format.md)
- CAF deterministic round-trip: [round-trip](https://github.com/scuptio/caf/blob/main/doc/spec/en/round-trip.md)
- Container codec: `crates/common/mudu_caf`
- Loader: `crates/db-kernel/runtime_wasm/src/service/app_package.rs`
- Packager: `crates/tools/mpm_build`, `crates/tools/mpm_tool`
- syscall ABI: `crates/common/mudu_binding/src/codec/syscall_payload/`, `uni-syscall.wit`
