# mpm_crate

`mpm-crate` — scaffold new MuduDB `.mpk` application projects. Inspired by
`npm create` / `cargo new` / `dotnet new`: given a project name and a guest
language, it renders a minimal, buildable project template (DDL, one example
procedure, build pipeline) that `cargo make package` turns into an
installable `.mpk` package.

## Role

- Part of the `mpm` package toolchain alongside `mpm_build` (package
  builder) and `mpm_install` (package installer); installed as the
  `mpm-crate` binary by `cargo make install-tools`.
- Depends on `clap` for the CLI and `mudu_sys` for filesystem access; the
  project templates are embedded at compile time, so the binary is
  self-contained.
- Consumed by end users starting a new MuduDB application; not a library
  dependency of other workspace crates.

Supported guest languages: `rust`, `assemblyscript`, `csharp`, `python`,
`c`, and `go`.

## Key modules

- `scaffold` — project scaffolding: name validation, template rendering, and
  file materialization.
- `templates` — the static template table: every project file `mpm-crate`
  can materialize, embedded from `templates/<lang>/` with `include_str!`.
