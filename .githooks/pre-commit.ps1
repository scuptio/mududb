$ErrorActionPreference = "Stop"

Write-Host "Running cargo fmt --check..."

try {
    cargo fmt -- --check | Out-String
    if ($LASTEXITCODE -ne 0) {
        throw "Formatting check failed."
    }
} catch {
    Write-Host ""
    Write-Host "Formatting check failed. Run 'cargo fmt' and try again." -ForegroundColor Red
    exit 1
}

Write-Host "Formatting check passed."
Write-Host ""
Write-Host "Running cargo clippy --workspace --all-targets..."

try {
    cargo clippy --workspace --all-targets -- -D warnings | Out-String
    if ($LASTEXITCODE -ne 0) {
        throw "Clippy check failed."
    }
} catch {
    Write-Host ""
    Write-Host "Clippy check failed. Commit aborted." -ForegroundColor Red
    exit 1
}

Write-Host "Clippy check passed."
Write-Host ""
Write-Host "Running cargo deny check bans licenses advisories sources..."

try {
    cargo deny check bans licenses advisories sources | Out-String
    if ($LASTEXITCODE -ne 0) {
        throw "Cargo deny check failed."
    }
} catch {
    Write-Host ""
    Write-Host "Cargo deny check failed. Commit aborted." -ForegroundColor Red
    exit 1
}

Write-Host "Cargo deny check passed."
