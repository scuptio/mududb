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

The pre-commit hook runs the following checks before each commit:

- `cargo fmt -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo deny check bans licenses advisories sources`

If any check fails, the commit is aborted.
