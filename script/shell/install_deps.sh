#!/bin/bash
set -euo pipefail

echo "=== Installing MuduDB Dependencies ==="
echo ""

# 1. System packages
echo "[1/5] Installing system packages..."
if [ "$(id -u)" -eq 0 ]; then
    apt-get update -y
    apt-get install -y python3 python3-pip python-is-python3 clang libclang-dev llvm-dev build-essential curl liburing-dev pkgconf iproute2 git
else
    sudo apt-get update -y
    sudo apt-get install -y python3 python3-pip python-is-python3 clang libclang-dev llvm-dev build-essential curl liburing-dev pkgconf iproute2 git
fi
# Ensure SSL root certificates are current (fixes Docker git SSL errors)
apt-get install -y --reinstall ca-certificates 2>/dev/null || sudo apt-get install -y --reinstall ca-certificates 2>/dev/null || true
update-ca-certificates 2>/dev/null || true
echo "System packages installed."
echo ""

# 2. Rust nightly toolchain
echo "[2/5] Installing Rust nightly toolchain..."
if ! command -v rustup &> /dev/null; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
fi
rustup toolchain install nightly
rustup default nightly
rustup component add rustfmt --toolchain nightly
rustup update nightly
rustup target add wasm32-wasip2
echo "Rust nightly toolchain installed."
echo ""

# 3. Python packages
echo "[3/5] Installing Python packages..."
python3 -m pip install --break-system-packages toml tomli-w
echo "Python packages installed."
echo ""

# 4. cargo-make
echo "[4/5] Installing cargo-make..."
cargo install cargo-make
echo "cargo-make installed."
echo ""

# 5. Verify
echo "[5/5] Verifying installation..."
echo "  rustc: $(rustc --version)"
echo "  cargo: $(cargo --version)"
echo "  rustfmt: $(rustfmt +nightly --version 2>&1)"
echo "  wasm32-wasip2 target: $(rustup target list | grep wasm32-wasip2 | grep installed)"
echo "  python3: $(python3 --version)"
echo "  cargo-make: $(cargo make --version 2>&1)"
echo ""

# Use system git for fetching (more robust SSL handling than libgit2)
mkdir -p "$HOME/.cargo"
grep -q "git-fetch-with-cli" "$HOME/.cargo/config.toml" 2>/dev/null || cat >> "$HOME/.cargo/config.toml" <<'CARGOEOF'
[net]
git-fetch-with-cli = true
CARGOEOF

echo "=== All dependencies installed successfully ==="
echo "Run: source \$HOME/.cargo/env   (if this is a fresh rustup install)"
