$ErrorActionPreference = "Stop"

# The repository has no root cargo workspace: crates/{common,db-kernel,sdk,tools}
# are four independent workspaces, checked one at a time.
$Groups = @("crates/common", "crates/db-kernel", "crates/sdk", "crates/tools")
$RepoRoot = Split-Path -Parent $PSScriptRoot

Write-Host "Running cargo fmt --check (per group workspace)..."

foreach ($group in $Groups) {
    Write-Host "  [fmt] $group"
    Push-Location (Join-Path $RepoRoot $group)
    try {
        cargo fmt -- --check | Out-String
        if ($LASTEXITCODE -ne 0) {
            Write-Host ""
            Write-Host "Formatting check failed in $group. Run 'cargo fmt' and try again." -ForegroundColor Red
            exit 1
        }
    } finally {
        Pop-Location
    }
}

Write-Host "Formatting check passed."
Write-Host ""
Write-Host "Running cargo clippy --workspace --all-targets (per group workspace)..."

foreach ($group in $Groups) {
    Write-Host "  [clippy] $group"
    Push-Location (Join-Path $RepoRoot $group)
    try {
        cargo clippy --workspace --all-targets -- -D warnings | Out-String
        if ($LASTEXITCODE -ne 0) {
            Write-Host ""
            Write-Host "Clippy check failed in $group. Commit aborted." -ForegroundColor Red
            exit 1
        }
    } finally {
        Pop-Location
    }
}

Write-Host "Clippy check passed."
Write-Host ""
Write-Host "Running cargo deny check bans licenses advisories sources (per group workspace)..."

foreach ($group in $Groups) {
    Write-Host "  [deny] $group"
    Push-Location (Join-Path $RepoRoot $group)
    try {
        cargo deny check --config (Join-Path $RepoRoot "deny.toml") bans licenses advisories sources | Out-String
        if ($LASTEXITCODE -ne 0) {
            Write-Host ""
            Write-Host "Cargo deny check failed in $group. Commit aborted." -ForegroundColor Red
            exit 1
        }
    } finally {
        Pop-Location
    }
}

Write-Host "Cargo deny check passed."
