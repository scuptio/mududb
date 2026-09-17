# Git Hooks

This directory contains git hooks for the project.

## Setup

Configure git to use hooks from this directory:

```bash
git config core.hooksPath .githooks
```

> **Note:** This configuration is local to your repository clone.

## Platforms

- **Linux / macOS / Git Bash on Windows**: The `pre-commit` bash script works out of the box.
- **PowerShell on Windows**: Use `pre-commit.ps1` instead. Rename or symlink it to `pre-commit` after setup.

## Pre-commit Hook

The pre-commit hook runs the following checks before each commit, once per
group workspace (`crates/common`, `crates/db-kernel`, `crates/sdk`,
`crates/tools` — there is no root cargo workspace):

- `cargo fmt -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo deny check --config <repo-root>/deny.toml bans licenses advisories sources`

If any check fails, the commit is aborted.
