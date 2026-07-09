#!/usr/bin/env bash
set -euo pipefail

# Reproducible MuduDB development environment setup for Ubuntu/Debian.
# This script installs system dependencies, rustup, the pinned stable and
# nightly Rust toolchains, a local Python venv, and cargo-make.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

log_info() { echo "[INFO] $*"; }
log_success() { echo "[SUCCESS] $*"; }
log_error() { echo "[ERROR] $*" >&2; }

install_system_deps() {
    log_info "Installing system dependencies..."
    if command -v sudo >/dev/null 2>&1; then
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
        pkgconf
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

install_python_venv() {
    log_info "Setting up Python virtual environment..."
    cd "${PROJECT_ROOT}"
    if [[ ! -d .venv ]]; then
        python3 -m venv .venv
    fi
    # shellcheck source=/dev/null
    source .venv/bin/activate
    python -m pip install --upgrade pip
    python -m pip install toml tomli-w
}

install_cargo_make() {
    log_info "Installing cargo-make..."
    cargo install cargo-make
}

main() {
    install_system_deps
    install_rustup
    install_stable_toolchain
    install_nightly_toolchain
    install_python_venv
    install_cargo_make

    log_success "Development environment is ready."
    log_info "Activate the Python venv with: source ${PROJECT_ROOT}/.venv/bin/activate"
    log_info "Run checks with: cargo build, cargo test, cargo clippy, cargo doc"
    log_info "Run auxiliary checks with the pinned nightly from ${PROJECT_ROOT}/.rust-nightly-version"
}

main "$@"
