#!/bin/bash
# Install prerequisites for the TPC-C cross-database benchmark.
#
# This script installs:
#   - Rust toolchain (cargo, rustup) and wasm32-wasip2 target
#   - cargo-make
#   - PostgreSQL server binaries (initdb, pg_ctl, psql)
#   - MySQL server binaries (mysqld, mysqladmin, mysql)
#   - Python 3 packages (PyYAML, matplotlib) via the system package manager
#
# Usage:
#   ./install_prerequisites.sh           # interactive, asks before system installs
#   ./install_prerequisites.sh --yes     # non-interactive, auto-answer yes
#
# Environment variables:
#   MYSQL_PKG    MySQL server package name (default: mysql-server). Use
#                mysql-community-server if your system uses MySQL community edition.

set -euo pipefail

YES=false
for arg in "$@"; do
    case "$arg" in
        -y|--yes)
            YES=true
            ;;
        -h|--help)
            echo "Usage: $0 [--yes]"
            exit 0
            ;;
        *)
            echo "Unknown argument: $arg" >&2
            echo "Usage: $0 [--yes]" >&2
            exit 1
            ;;
    esac
done

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log_info() { echo -e "${GREEN}[INFO]${NC} $*"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $*"; }
log_error() { echo -e "${RED}[ERROR]${NC} $*" >&2; }
log_step() { echo -e "${BLUE}[STEP]${NC} $*"; }

confirm() {
    if [[ "$YES" == true ]]; then
        return 0
    fi
    local prompt="$1"
    read -r -p "$prompt [y/N] " response
    case "$response" in
        [yY][eE][sS]|[yY])
            return 0
            ;;
        *)
            return 1
            ;;
    esac
}

command_exists() {
    command -v "$1" >/dev/null 2>&1
}

add_postgres_to_path() {
    # Debian/Ubuntu install PostgreSQL server binaries under
    # /usr/lib/postgresql/<version>/bin/, while RHEL/CentOS/Rocky install them
    # under /usr/pgsql-<version>/bin/. Neither location is on the default PATH.
    local pg_bin_dir
    pg_bin_dir=$(
        {
            ls -d /usr/lib/postgresql/*/bin 2>/dev/null
            ls -d /usr/pgsql-*/bin 2>/dev/null
        } | sort -V | tail -n 1
    )
    if [[ -z "$pg_bin_dir" ]]; then
        return
    fi

    case ":$PATH:" in
        *:"$pg_bin_dir":*)
            # Already on PATH.
            ;;
        *)
            PATH="$pg_bin_dir:$PATH"
            export PATH
            ;;
    esac
}

detect_shell_rc() {
    # Return the path to the current user's shell rc file, or empty string if
    # we do not recognize the shell (bash, zsh, fish). $SHELL is the user's
    # login shell and is the most reliable source here.
    local shell_name=""
    if [[ -n "${SHELL:-}" ]]; then
        shell_name="$(basename "$SHELL")"
    fi

    case "$shell_name" in
        bash)
            echo "$HOME/.bashrc"
            ;;
        zsh)
            echo "$HOME/.zshrc"
            ;;
        fish)
            echo "$HOME/.config/fish/config.fish"
            ;;
        *)
            echo ""
            ;;
    esac
}

persist_postgres_to_path() {
    # Add the PostgreSQL bin directory to the user's shell rc file so that
    # initdb/pg_ctl/psql remain available after this script exits.
    local pg_bin_dir
    pg_bin_dir=$(
        {
            ls -d /usr/lib/postgresql/*/bin 2>/dev/null
            ls -d /usr/pgsql-*/bin 2>/dev/null
        } | sort -V | tail -n 1
    )
    if [[ -z "$pg_bin_dir" ]]; then
        return
    fi

    local rc_file
    rc_file=$(detect_shell_rc)
    if [[ -z "$rc_file" ]]; then
        log_warn "PostgreSQL binaries installed at $pg_bin_dir"
        log_warn "Could not detect shell rc file; add the following line manually:"
        log_warn "  export PATH=\"$pg_bin_dir:\$PATH\""
        return
    fi

    mkdir -p "$(dirname "$rc_file")"

    if [[ -f "$rc_file" ]] && grep -qxF "export PATH=\"$pg_bin_dir:\$PATH\"" "$rc_file"; then
        log_info "PostgreSQL bin directory already in $rc_file"
        return
    fi

    if ! confirm "Add $pg_bin_dir to PATH in $rc_file?"; then
        log_warn "Skipping PATH persistence; add the following line manually to $rc_file:"
        log_warn "  export PATH=\"$pg_bin_dir:\$PATH\""
        return
    fi

    {
        echo ""
        echo "# Added by mududb TPC-C install_prerequisites.sh"
        echo "export PATH=\"$pg_bin_dir:\$PATH\""
    } >> "$rc_file"

    log_info "Added $pg_bin_dir to PATH in $rc_file"
    log_info "Run 'source $rc_file' or open a new terminal to use initdb/pg_ctl/psql"
}

postgres_binaries_exist() {
    add_postgres_to_path
    command_exists initdb && command_exists pg_ctl && command_exists psql
}

mysql_binaries_exist() {
    command_exists mysqld && command_exists mysqladmin && command_exists mysql
}

broken_packages_exist() {
    # dpkg --audit exits non-zero when there are no broken packages.
    dpkg --audit 2>/dev/null | grep -q .
}

fix_broken_packages() {
    if ! broken_packages_exist; then
        return 0
    fi

    log_warn "Detected broken/incomplete packages on the system."
    if ! confirm "Run 'sudo apt --fix-broken install' to repair them"; then
        log_warn "Skipping broken-package repair; installation may fail"
        return 0
    fi

    sudo apt --fix-broken install -y || {
        log_error "Failed to repair broken packages"
        exit 1
    }
}

detect_os() {
    if [[ ! -f /etc/os-release ]]; then
        log_error "Cannot detect OS: /etc/os-release not found"
        exit 1
    fi

    # shellcheck source=/dev/null
    source /etc/os-release

    case "$ID" in
        ubuntu|debian|linuxmint|pop)
            echo "debian"
            ;;
        rhel|centos|rocky|almalinux|fedora|ol)
            echo "rhel"
            ;;
        *)
            log_error "Unsupported OS: $ID (supported: ubuntu, debian, rhel, centos, rocky, almalinux, fedora)"
            exit 1
            ;;
    esac
}

install_system_packages_debian() {
    log_step "Installing system packages with apt..."

    fix_broken_packages

    local pkg_list=(
        build-essential
        curl
        python3
        python3-pip
        python3-venv
        python3-yaml
        python3-matplotlib
    )

    if postgres_binaries_exist; then
        log_info "PostgreSQL binaries already available, skipping postgresql packages"
    else
        pkg_list+=(postgresql postgresql-client)
    fi

    if mysql_binaries_exist; then
        log_info "MySQL binaries already available, skipping mysql packages"
    else
        local mysql_pkg="${MYSQL_PKG:-mysql-server}"
        pkg_list+=("$mysql_pkg" mysql-client)
        if [[ "$mysql_pkg" != "mysql-server" ]]; then
            log_info "Using custom MySQL package: $mysql_pkg"
        fi
    fi

    if ! confirm "Run 'sudo apt-get install' for: ${pkg_list[*]}?"; then
        log_warn "Skipping system package installation"
        return
    fi

    sudo apt-get update
    sudo apt-get install -y --no-install-recommends "${pkg_list[@]}" || {
        log_error "apt installation failed"
        exit 1
    }
}

install_system_packages_rhel() {
    log_step "Installing system packages with dnf/yum..."

    local pkg_list=(
        curl
        gcc
        gcc-c++
        make
        python3
        python3-pip
        python3-pyyaml
        python3-matplotlib
    )

    if postgres_binaries_exist; then
        log_info "PostgreSQL binaries already available, skipping postgresql packages"
    else
        pkg_list+=(postgresql-server postgresql-contrib)
    fi

    if mysql_binaries_exist; then
        log_info "MySQL binaries already available, skipping mysql packages"
    else
        local mysql_pkg="${MYSQL_PKG:-mysql-server}"
        pkg_list+=("$mysql_pkg" mysql)
        if [[ "$mysql_pkg" != "mysql-server" ]]; then
            log_info "Using custom MySQL package: $mysql_pkg"
        fi
    fi

    local pkg_manager="dnf"
    if ! command_exists dnf; then
        pkg_manager="yum"
    fi

    if ! confirm "Run 'sudo $pkg_manager install' for: ${pkg_list[*]}?"; then
        log_warn "Skipping system package installation"
        return
    fi

    sudo "$pkg_manager" install -y "${pkg_list[@]}" || {
        log_error "$pkg_manager installation failed"
        exit 1
    }
}

install_rust() {
    log_step "Checking Rust toolchain..."

    if command_exists cargo && command_exists rustc && command_exists rustup; then
        log_info "Rust already installed: $(rustc --version)"
    else
        if ! confirm "Rust not found. Install via rustup?"; then
            log_warn "Skipping Rust installation"
            return
        fi
        log_info "Installing rustup..."
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
        # shellcheck source=/dev/null
        source "$HOME/.cargo/env"
    fi

    # Ensure cargo is in PATH for the rest of this script
    # shellcheck source=/dev/null
    source "$HOME/.cargo/env" 2>/dev/null || true

    log_step "Adding wasm32-wasip2 target..."
    rustup target add wasm32-wasip2
}

install_cargo_make() {
    log_step "Checking cargo-make..."

    if command_exists cargo-make; then
        log_info "cargo-make already installed: $(cargo make --version)"
        return
    fi

    if ! command_exists cargo; then
        log_error "cargo not found; cannot install cargo-make"
        exit 1
    fi

    if ! confirm "Install cargo-make?"; then
        log_warn "Skipping cargo-make installation"
        return
    fi

    cargo install cargo-make
}

verify() {
    log_step "Verifying installation..."

    local failed=false

    echo ""
    echo "Rust toolchain"
    if command_exists rustc; then
        log_info "rustc: $(rustc --version)"
    else
        log_error "rustc not found"
        failed=true
    fi

    if rustup target list --installed 2>/dev/null | grep -q wasm32-wasip2; then
        log_info "wasm32-wasip2 target: installed"
    else
        log_error "wasm32-wasip2 target: NOT installed"
        failed=true
    fi

    echo ""
    echo "cargo-make"
    if command_exists cargo-make; then
        log_info "cargo-make: $(cargo make --version)"
    else
        log_error "cargo-make not found"
        failed=true
    fi

    echo ""
    echo "PostgreSQL"
    for bin in initdb pg_ctl psql; do
        if command_exists "$bin"; then
            log_info "$bin: $(command -v "$bin")"
        else
            log_error "$bin not found"
            failed=true
        fi
    done

    echo ""
    echo "MySQL"
    for bin in mysqld mysqladmin mysql; do
        if command_exists "$bin"; then
            log_info "$bin: $(command -v "$bin")"
        else
            log_error "$bin not found"
            failed=true
        fi
    done

    echo ""
    echo "Python"
    if python3 -c "import yaml" 2>/dev/null; then
        log_info "PyYAML: installed"
    else
        log_error "PyYAML: NOT installed"
        failed=true
    fi
    if python3 -c "import matplotlib" 2>/dev/null; then
        log_info "matplotlib: installed"
    else
        log_error "matplotlib: NOT installed"
        failed=true
    fi

    echo ""
    if [[ "$failed" == true ]]; then
        log_error "Some prerequisites are missing. Check the messages above."
        exit 1
    fi

    log_info "All prerequisites are installed."
}

main() {
    local os_type
    os_type=$(detect_os)
    log_info "Detected OS family: $os_type"

    case "$os_type" in
        debian)
            install_system_packages_debian
            ;;
        rhel)
            install_system_packages_rhel
            ;;
    esac

    install_rust
    install_cargo_make
    persist_postgres_to_path
    add_postgres_to_path
    verify
}

main "$@"
