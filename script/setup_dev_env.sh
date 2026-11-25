#!/usr/bin/env bash
set -euo pipefail

# Reproducible MuduDB development environment setup for Ubuntu/Debian:
# system packages (sudo apt), rustup, the pinned stable and nightly Rust
# toolchains, Python packages, and cargo-make.
#
# Scope: this is the new-machine bootstrap. For day-to-day checks ("is my
# toolchain still complete?") and user-level tool installs, use
# `cargo make doctor` / `cargo make doctor -- install` instead — they never
# touch sudo.
#
# Flags:
#   --system-pip   install Python packages system-wide (pip --break-system-packages)
#                  instead of creating .venv — for deployment/container machines
#                  (script/shell/install_deps.sh is a thin wrapper for this mode).

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

log_info() { echo "[INFO] $*"; }
log_success() { echo "[SUCCESS] $*"; }
log_error() { echo "[ERROR] $*" >&2; }

SYSTEM_PIP=0
for arg in "$@"; do
    case "${arg}" in
        --system-pip) SYSTEM_PIP=1 ;;
        *) log_error "unknown argument: ${arg} (supported: --system-pip)"; exit 2 ;;
    esac
done

install_system_deps() {
    log_info "Installing system dependencies..."
    if command -v sudo >/dev/null 2>&1 && [ "$(id -u)" -ne 0 ]; then
        SUDO="sudo"
    else
        SUDO=""
    fi
    ${SUDO} apt-get update -y
    ${SUDO} apt-get install -y \
        python3 \
        python3-pip \
        python3-venv \
        python-is-python3 \
        build-essential \
        curl \
        liburing-dev \
        clang \
        libclang-dev \
        llvm-dev \
        pkgconf \
        iproute2 \
        git
    # Ensure SSL root certificates are current (fixes Docker git SSL errors).
    ${SUDO} apt-get install -y --reinstall ca-certificates || true
    ${SUDO} update-ca-certificates || true
}

install_rustup() {
    log_info "Installing rustup if missing..."
    if ! command -v rustup >/dev/null 2>&1; then
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain none
    fi
}

install_stable_toolchain() {
    log_info "Installing pinned stable toolchain..."
    cd "${PROJECT_ROOT}"
    STABLE_TOOLCHAIN="$(python3 -c 'import tomllib; print(tomllib.load(open("rust-toolchain.toml", "rb"))["toolchain"]["channel"])')"
    rustup toolchain install "${STABLE_TOOLCHAIN}" \
        --profile minimal \
        --component clippy,rustfmt \
        --target x86_64-unknown-linux-gnu,wasm32-wasip2
    # Pin the stable toolchain as the default so plain `cargo`/`rustc` work
    # without a `+toolchain` override, even outside this repo directory.
    rustup default "${STABLE_TOOLCHAIN}"
    rustup show active-toolchain
    rustc --version --verbose
    cargo --version
}

install_nightly_toolchain() {
    log_info "Installing pinned nightly toolchain for auxiliary checks..."
    cd "${PROJECT_ROOT}"
    NIGHTLY_TOOLCHAIN="$(tr -d '[:space:]' < .rust-nightly-version)"
    case "${NIGHTLY_TOOLCHAIN}" in
        nightly-????-??-??) ;;
        *) log_error "invalid pinned nightly: ${NIGHTLY_TOOLCHAIN}"; exit 1 ;;
    esac
    rustup toolchain install "${NIGHTLY_TOOLCHAIN}" --profile minimal
    rustup target add x86_64-unknown-linux-gnu --toolchain "${NIGHTLY_TOOLCHAIN}"
    cargo "+${NIGHTLY_TOOLCHAIN}" --version
}

install_python_packages() {
    # tomli-w is required by script/build/transpiler.py, which the example
    # Makefile.tomls invoke with the `python3` found on PATH.
    if [[ "${SYSTEM_PIP}" -eq 1 ]]; then
        log_info "Installing Python packages system-wide..."
        python3 -m pip install --break-system-packages tomli-w
    else
        log_info "Setting up Python virtual environment (.venv)..."
        cd "${PROJECT_ROOT}"
        if [[ ! -d .venv ]]; then
            python3 -m venv .venv
        fi
        .venv/bin/python -m pip install --upgrade pip
        .venv/bin/python -m pip install tomli-w
    fi
}

install_cargo_make() {
    log_info "Installing cargo-make..."
    cargo install cargo-make --locked
}

configure_cargo() {
    # Use system git for fetching (more robust SSL handling than libgit2).
    mkdir -p "${HOME}/.cargo"
    grep -q "git-fetch-with-cli" "${HOME}/.cargo/config.toml" 2>/dev/null || cat >> "${HOME}/.cargo/config.toml" <<'CARGOEOF'
[net]
git-fetch-with-cli = true
CARGOEOF
}

main() {
    install_system_deps
    install_rustup
    install_stable_toolchain
    install_nightly_toolchain
    install_python_packages
    install_cargo_make
    configure_cargo

    log_success "Development environment is ready."
    if [[ "${SYSTEM_PIP}" -eq 0 ]]; then
        log_info "Activate the Python venv with: source ${PROJECT_ROOT}/.venv/bin/activate"
    fi
    log_info "Verify the toolchain any time with: cargo make doctor"
    log_info "Run checks with: cargo make check; run tests with: cargo make test"
}

main "$@"
