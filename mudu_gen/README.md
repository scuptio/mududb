# mudu_gen

`mudu_gen` is the code generation crate for MuduDB. Its CLI binary is `mgen`.

`mgen` generates source code from:

- DDL SQL files, for entity/table model code
- WIT files, for message types used in serialization and deserialization

## Install

```bash
cargo install --path ./mudu_gen
```

After installation, the executable name is:

```bash
mgen
```

## Supported Languages

- `rust`
- `csharp`

## Command Overview

```bash
mgen [OPTIONS] [COMMAND]
```

Global options:

- `-v`, `--verbose`: enable verbose output
- `-h`, `--help`: print help
- `-V`, `--version`: print version

Subcommands:

- `entity`: generate entity source files from DDL SQL
- `message`: generate message source files from WIT

## `entity`

Generate entity code from one or more DDL SQL files.

```bash
mgen entity \
  --input-source-files <FILE>... \
  --output-source-folder <FOLDER> \
  --type-desc <FILE> \
  --lang <LANG>
```

Arguments:

- `-i`, `--input-source-files <FILE>...`: one or more input DDL SQL files
- `-o`, `--output-source-folder <FOLDER>`: output directory for generated source files
- `-t`, `--type-desc <FILE>`: output JSON file for generated type descriptions
- `-l`, `--lang <LANG>`: target language, currently `rust` or `csharp`

Behavior:

- Creates the output directory if it does not exist
- Generates one source file per parsed table/entity
- Writes a type description JSON file to the `--type-desc` path
- Uses the target language file extension: `.rs` for Rust and `.cs` for C#

Example:

```bash
mgen entity \
  -i ./mudu_gen/tool/test_data/sql/ddl.sql ./mudu_gen/tool/test_data/sql/type.sql \
  -o ./tmp/generated \
  -t ./tmp/types.desc.json \
  -l rust
```

## `message`

Generate message type code from WIT definitions.

```bash
mgen message \
  --input-wit-file <FILE> \
  --output-source-file <FILE> \
  --lang <LANG> \
  [--namespace <NAME>]
```

Arguments:

- `-i`, `--input-wit-file <FILE>`: input WIT file or a directory containing `.wit` files
- `-o`, `--output-source-file <FILE>`: output file path, or output directory when the input is a directory
- `-l`, `--lang <LANG>`: target language, currently `rust` or `csharp`
- `-n`, `--namespace <NAME>`: optional namespace override

Behavior:

- Accepts either a single `.wit` file or a directory of `.wit` files
- In single-file mode, writes exactly one output file to the given path
- In directory mode, generates one file per `.wit` file into the output directory
- Creates the output directory, or the parent directory of the output file, if needed
- Formats Rust output with `rustfmt` through the embedded formatter
- If `--namespace` is not provided, the namespace is inferred from the WIT interface name

Examples:

Generate one Rust file from one WIT file:

```bash
mgen message \
  -i ./mudu_gen/tool/test_data/wit/uni-command-argv.wit \
  -o ./tmp/uni_command_argv.rs \
  -l rust
```

Generate C# files from a folder of WIT files:

```bash
mgen message \
  -i ./mudu_gen/tool/test_data/wit-schema \
  -o ./tmp/wit_out \
  -l csharp \
  -n Demo.Messages
```

## Notes

- The package name is `mudu_gen`, but the installed CLI binary is `mgen`.
- `message` also has the alias `msg`.
- The current implementation for `entity` is based on DDL SQL parsing.
