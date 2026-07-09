# MPK Package Manifest Contract v1

## Scope

This document specifies the `.mpk` application package format used to deploy MuduDB stored procedures. An `.mpk` file is a ZIP archive containing app metadata, schema DDL, procedure descriptors, initial data SQL, and one or more Wasm component modules.

## Version history

| Version | Date | Summary |
|---------|------|---------|
| 1 | 2026-6-25 | Initial manifest with `format_version` and file list. Packages without a manifest are accepted for backward compatibility. |

## Archive contents

A valid `.mpk` archive contains at least the following entries:

| Entry | Required | Description |
|-------|----------|-------------|
| `package.cfg.json` | Yes | App metadata (`AppInfo`). |
| `package.desc.json` | Yes | Procedure/module descriptor (`ModProcDesc`). |
| `ddl.sql` | Yes | Schema DDL statements. |
| `initdb.sql` | Packager: Yes; loader: No | Initial data SQL statements. The packager emits this entry, but the current loader accepts packages where it is missing or empty. |
| `package.manifest.json` | No | Forward/backward-compat manifest. |
| `*.wasm` | Packager: Yes; loader: No | One or more Wasm component modules. The current loader collects modules when present but does not reject an archive with no modules. |

## `package.cfg.json`

A JSON object with the following fields:

| Field | Type | Description |
|-------|------|-------------|
| `name` | string | Application name. |
| `lang` | string | Source language (e.g., `rust`). |
| `version` | string | Application version (semantic string, not the format version). |
| `use_async` | boolean | Whether the app uses the async ABI. |

## `package.manifest.json`

A JSON object with the following fields:

| Field | Type | Description |
|-------|------|-------------|
| `format_version` | integer | Package manifest format version. Current value: `1`. |
| `files` | array of strings | List of files present in the archive. Must include all required entries. |

## Integrity mechanisms

- **Manifest presence:** if a manifest is present, the loader validates `format_version`.
- **Format version check:** the loader rejects `format_version` values other than `1`.
- **Manifest file list:** if a manifest is present, the loader verifies that `files` contains `package.cfg.json`, `package.desc.json`, `ddl.sql`, and `initdb.sql`.
- **Runtime required entries:** independently of the manifest, the current loader rejects missing `package.cfg.json`, missing `package.desc.json`, and missing or empty `ddl.sql`.
- **Module alignment:** if the package contains exactly one `.wasm` file and the descriptor contains exactly one module, the loader aligns the module name to the descriptor.

## Compatibility matrix

| Reader \ Writer | no manifest | v1 manifest |
|-----------------|-------------|-------------|
| current | Accepted (legacy) | Compatible |

Packages without a manifest are still accepted but are considered legacy. New packages must include a v1 manifest.

## Upgrade and rollback rules

- **Upgrade:** A future v2 manifest may add new required or optional fields. V2 writers must bump `format_version` and continue to include all v1-required files.
- **Rollback:** A v1-only loader opening a v2 package rejects it with `UnsupportedFormatVersion` and does not extract or execute any archive contents.
- **Migration:** V1 → V2 migration is a packaging-time transformation. No runtime migration is required.

## Deprecation policy

Version `1` may be deprecated only after:
1. All example apps and release tooling emit the new format version.
2. The build pipeline rejects legacy packages without a manifest.
3. Golden v1 `.mpk` fixtures continue to load in CI until deprecation is complete.

## References

- Package loader: [`mudu_runtime/src/service/app_package.rs`](../../../mudu_runtime/src/service/app_package.rs)
- Packager: [`mpm_build/src/main.rs`](../../../mpm_build/src/main.rs)
- File name constants: [`mudu_runtime/src/service/file_name.rs`](../../../mudu_runtime/src/service/file_name.rs)
- App metadata: [`mudu/src/common/app_info.rs`](../../../mudu/src/common/app_info.rs)
