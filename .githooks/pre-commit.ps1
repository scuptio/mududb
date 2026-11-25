# Fast pre-commit gate: fmt + clippy + cargo deny per group workspace.
# The checks live in script/ci/check_all.py (single source of truth, shared
# with `cargo make check`); this wrapper only selects the fast steps so the
# hook stays quick. Run the full suite with: cargo make check
$ErrorActionPreference = "Stop"

$RepoRoot = Split-Path -Parent $PSScriptRoot

& python (Join-Path $RepoRoot "script/ci/check_all.py") --only fmt,clippy,deny --fail-fast
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
