#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

# Ensure cargo is on PATH
source "$HOME/.cargo/env"

cd "$REPO_ROOT"

echo "=== MuduDB Build & Install ==="
echo ""

# 1. Build the full workspace (34 crates)
echo "[1/4] Building workspace (cargo build --release)..."
cargo build --release
echo "Workspace build complete."
echo ""

# 2. Install binaries (mudud, mcli, mpk, mgen, mtp)
echo "[2/4] Installing binaries..."
python3 script/build/install_binaries.py
echo "Binaries installed to \$HOME/.cargo/bin/."
echo ""

# 3. Add wasm target and build wallet example
echo "[3/4] Building wallet example (.mpk)..."
cd "$REPO_ROOT/example/wallet"
cargo make
cd "$REPO_ROOT"
echo "Wallet example built."
echo ""

# 4. Show results
echo "[4/4] Build summary:"
echo "  mudud : $(which mudud 2>/dev/null || echo 'not found')"
echo "  mcli  : $(which mcli 2>/dev/null || echo 'not found')"
echo "  mpk   : $(which mpk 2>/dev/null || echo 'not found')"
echo "  wallet.mpk : $(ls target/wasm32-wasip2/release/wallet.mpk 2>/dev/null || echo 'not found')"
echo ""

echo "=== Build complete ==="
