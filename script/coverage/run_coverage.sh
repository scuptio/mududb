#!/usr/bin/env bash
set -euo pipefail

# Local coverage runner for the mududb workspace.
# Uses cargo llvm-cov with the pinned nightly toolchain.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

CARGO_LLVM_COV_VERSION="0.8.7"
OUTPUT_DIR="${PROJECT_ROOT}/target/llvm-cov"

log_info() { echo "[INFO] $*"; }
log_success() { echo "[SUCCESS] $*"; }
log_error() { echo "[ERROR] $*" >&2; }

usage() {
    cat <<EOF
Usage: $0 [OPTIONS]

Run code coverage locally using cargo llvm-cov.

Options:
  -p, --profile <profile>   Coverage scope: 'core' (default) or 'workspace'
  -f, --format <format>     Output format: 'html', 'json', 'lcov', or 'all' (default)
  --[no-]branch             Enable/disable branch coverage (default: enabled for core, disabled for workspace)
  -o, --output-dir <dir>    Output directory (default: target/llvm-cov)
  -h, --help                Show this help message

Examples:
  $0                        # coverage for core crates, all formats, with branch coverage
  $0 --no-branch            # disable branch coverage
  $0 -p workspace -f html   # full workspace, HTML only (line coverage by default)
  $0 -p workspace --branch  # full workspace with branch coverage (may crash due to LLVM bug)
  $0 -p core -f json        # core crates, JSON summary only
EOF
}

parse_args() {
    PROFILE="core"
    FORMAT="all"
    BRANCH=1
    BRANCH_SET=0
    while [[ $# -gt 0 ]]; do
        case "$1" in
            -p|--profile)
                PROFILE="$2"
                shift 2
                ;;
            -f|--format)
                FORMAT="$2"
                shift 2
                ;;
            --branch)
                BRANCH=1
                BRANCH_SET=1
                shift
                ;;
            --no-branch)
                BRANCH=0
                BRANCH_SET=1
                shift
                ;;
            -o|--output-dir)
                OUTPUT_DIR="$2"
                shift 2
                ;;
            -h|--help)
                usage
                exit 0
                ;;
            *)
                log_error "Unknown option: $1"
                usage
                exit 1
                ;;
        esac
    done

    # workspace profile currently crashes with branch coverage due to an LLVM bug
    # when mudu_kernel is included. Default to line coverage unless explicitly requested.
    if [[ "${PROFILE}" == "workspace" && "${BRANCH_SET}" -eq 0 ]]; then
        BRANCH=0
        log_info "workspace profile defaults to line coverage (--no-branch); use --branch to force branch coverage."
    fi
}

read_nightly_toolchain() {
    cd "${PROJECT_ROOT}"
    NIGHTLY_TOOLCHAIN="$(tr -d '[:space:]' < .rust-nightly-version)"
    case "${NIGHTLY_TOOLCHAIN}" in
        nightly-????-??-??) ;;
        *) log_error "invalid pinned nightly: ${NIGHTLY_TOOLCHAIN}"; exit 1 ;;
    esac
    log_info "Using pinned nightly toolchain: ${NIGHTLY_TOOLCHAIN}"
}

run_with_retry() {
    local max_attempts=3
    local attempt=1
    local delay=5
    while [[ ${attempt} -le ${max_attempts} ]]; do
        if "$@"; then
            return 0
        fi
        log_error "Command failed (attempt ${attempt}/${max_attempts}): $*"
        if [[ ${attempt} -lt ${max_attempts} ]]; then
            log_info "Retrying in ${delay} seconds..."
            sleep ${delay}
        fi
        attempt=$((attempt + 1))
    done
    return 1
}

ensure_nightly_toolchain() {
    log_info "Ensuring pinned nightly toolchain ${NIGHTLY_TOOLCHAIN} is installed..."
    if ! rustup toolchain list 2>/dev/null | grep -q "^${NIGHTLY_TOOLCHAIN}"; then
        run_with_retry rustup toolchain install "${NIGHTLY_TOOLCHAIN}" --profile minimal
        run_with_retry rustup target add x86_64-unknown-linux-gnu --toolchain "${NIGHTLY_TOOLCHAIN}"
    fi
}

ensure_llvm_tools() {
    log_info "Ensuring llvm-tools-preview is installed..."
    run_with_retry rustup component add llvm-tools-preview --toolchain "${NIGHTLY_TOOLCHAIN}"
}

ensure_cargo_llvm_cov() {
    log_info "Ensuring cargo-llvm-cov ${CARGO_LLVM_COV_VERSION} is installed..."
    if ! cargo "+${NIGHTLY_TOOLCHAIN}" llvm-cov --version >/dev/null 2>&1; then
        cargo "+${NIGHTLY_TOOLCHAIN}" install cargo-llvm-cov --version "${CARGO_LLVM_COV_VERSION}" --locked
    fi
}

build_package_args() {
    if [[ "${PROFILE}" == "core" ]]; then
        PACKAGE_ARGS=(
            --package mudu
            --package mudu_type
            --package mudu_contract
            --package mudu_kernel
        )
        if [[ "${BRANCH}" -eq 1 ]]; then
            log_info "Note: branch coverage for the 'core' profile can trigger an LLVM bug in mudu_kernel; if report generation crashes, rerun with --no-branch."
        fi
    elif [[ "${PROFILE}" == "workspace" ]]; then
        PACKAGE_ARGS=(--workspace)
    else
        log_error "Unknown profile: ${PROFILE}. Use 'core' or 'workspace'."
        exit 1
    fi
}

branch_args() {
    if [[ "${BRANCH}" -eq 1 ]]; then
        echo --branch
    fi
}

clean_coverage_artifacts() {
    log_info "Cleaning old coverage artifacts..."
    cd "${PROJECT_ROOT}"
    cargo "+${NIGHTLY_TOOLCHAIN}" llvm-cov clean
    rm -rf "${OUTPUT_DIR}"
}

run_tests_with_coverage() {
    log_info "Running tests with coverage instrumentation for profile '${PROFILE}'..."
    cd "${PROJECT_ROOT}"
    CARGO_INCREMENTAL=0 \
        cargo "+${NIGHTLY_TOOLCHAIN}" llvm-cov \
        "${PACKAGE_ARGS[@]}" \
        --lib --tests \
        --no-report \
        $(branch_args) \
        -- \
        --test-threads=1
}

generate_html_report() {
    mkdir -p "${OUTPUT_DIR}/html"
    cargo "+${NIGHTLY_TOOLCHAIN}" llvm-cov report \
        $(branch_args) \
        --html \
        --output-dir "${OUTPUT_DIR}/html"
}

generate_json_report() {
    cargo "+${NIGHTLY_TOOLCHAIN}" llvm-cov report \
        $(branch_args) \
        --json \
        --output-path "${OUTPUT_DIR}/coverage.json"
}

generate_lcov_report() {
    cargo "+${NIGHTLY_TOOLCHAIN}" llvm-cov report \
        $(branch_args) \
        --lcov \
        --output-path "${OUTPUT_DIR}/coverage.lcov"
}

run_coverage() {
    run_tests_with_coverage

    case "${FORMAT}" in
        html)
            log_info "Generating HTML report..."
            generate_html_report
            ;;
        json)
            log_info "Generating JSON report..."
            generate_json_report
            ;;
        lcov)
            log_info "Generating LCOV report..."
            generate_lcov_report
            ;;
        all)
            log_info "Generating HTML / JSON / LCOV reports..."
            generate_html_report
            generate_json_report
            generate_lcov_report
            ;;
        *)
            log_error "Unknown format: ${FORMAT}. Use 'html', 'json', 'lcov', or 'all'."
            exit 1
            ;;
    esac
}

print_summary() {
    log_success "Coverage report generated in ${OUTPUT_DIR}"
    if [[ "${BRANCH}" -eq 1 ]]; then
        echo "  Branch coverage: enabled"
    else
        echo "  Branch coverage: disabled"
    fi
    case "${FORMAT}" in
        html|all)
            echo "  HTML report : file://${OUTPUT_DIR}/html/index.html"
            ;;
    esac
    case "${FORMAT}" in
        json|all)
            echo "  JSON summary: ${OUTPUT_DIR}/coverage.json"
            ;;
    esac
    case "${FORMAT}" in
        lcov|all)
            echo "  LCOV file   : ${OUTPUT_DIR}/coverage.lcov"
            ;;
    esac
}

main() {
    parse_args "$@"
    read_nightly_toolchain
    ensure_nightly_toolchain
    ensure_llvm_tools
    ensure_cargo_llvm_cov
    build_package_args
    clean_coverage_artifacts
    mkdir -p "${OUTPUT_DIR}"
    run_coverage
    print_summary
}

main "$@"
