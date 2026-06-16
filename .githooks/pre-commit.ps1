$ErrorActionPreference = "Stop"

Write-Host "Running cargo clippy --workspace..."

try {
    cargo clippy --workspace -- -D warnings | Out-String
    if ($LASTEXITCODE -ne 0) {
        throw "Clippy check failed."
    }
} catch {
    Write-Host ""
    Write-Host "Clippy check failed. Commit aborted." -ForegroundColor Red
    exit 1
}

Write-Host "Clippy check passed."
