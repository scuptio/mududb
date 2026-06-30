# mudu_package

A command-line tool and library for packaging Mudu APP archives. It creates `.mpk` package archives from configuration, description, SQL, and WASM files, and can merge multiple procedure-description files into a single description.

## Responsibility

- Build `.mpk` archives for Mudu APPs from `package.cfg.json`, `package.desc.json`, `ddl.sql`, `initdb.sql`, and one or more `.wasm` files.
- Validate that the modules listed in `package.desc.json` match the provided WASM file names.
- Merge multiple `*.desc.json` procedure-description files into a single `package.desc.json`.

## What does NOT belong here

- Runtime loading or execution of MPK packages — see `mudu_runtime`.
- Core APP / procedure contract types — see `mudu_contract`.
- File-system, JSON, or TOML utilities — see `mudu_sys` and `mudu_utils`.
- Building or compiling WASM modules — see `mudu_wasm` and `mudu_gen`.

## Main public entry points

- `mpk` binary — command-line tool with `create`, `create-from-toml`, and `merge-desc` subcommands.
- `mudu_package::merge_desc` — library module for merging procedure-description files.
