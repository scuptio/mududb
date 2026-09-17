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
  --[no-]branch             Enable/disable branch coverage (default: disabled; llvm-cov
                            in the pinned nightly SIGSEGVs on branch data at report time)
  -o, --output-dir <dir>    Output directory (default: target/llvm-cov)
  -h, --help                Show this help message

Examples:
  $0                        # coverage for core crates, all formats, line coverage
  $0 --branch               # core crates with branch coverage (unstable, see above)
  $0 -p workspace -f html   # full workspace, HTML only
  $0 -p core -f json        # core crates, JSON summary only
EOF
}

parse_args() {
    PROFILE="core"
    FORMAT="all"
    # Branch coverage defaults off: branch-instrumented data makes llvm-cov
    # (LLVM in the pinned nightly) SIGSEGV at report time in
    # getInstantiationGroups, and cargo-llvm-cov auto-enables branch display
    # whenever the data contains branch records, so the only reliable
    # workaround is to not collect branch data. --branch opts in explicitly.
    BRANCH=0
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
                shift
                ;;
            --no-branch)
                BRANCH=0
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

# Independent group workspaces, each with its own Cargo.lock. Coverage runs
# one group at a time because no root workspace exists anymore.
# Note: GROUPS is a special bash variable (the user's group IDs) and cannot be
# assigned to, so this array uses a different name.
WS_GROUPS=(crates/common crates/db-kernel crates/sdk crates/tools)

# Sets PACKAGE_ARGS for the given group. Returns 1 when the selected profile
# covers no packages in that group (caller skips it).
group_package_args() {
    local group="$1"
    if [[ "${PROFILE}" == "core" ]]; then
        case "${group}" in
            crates/common)
                PACKAGE_ARGS=(
                    --package mudu
                    --package mudu_type
                    --package mudu_contract
                )
                ;;
            crates/db-kernel)
                PACKAGE_ARGS=(--package mudu_kernel)
                ;;
            *)
                return 1
                ;;
        esac
        if [[ "${BRANCH}" -eq 1 ]]; then
            log_info "Note: --branch is unstable with the pinned nightly's LLVM: llvm-cov can SIGSEGV at report time (getInstantiationGroups), failing the run."
        fi
    elif [[ "${PROFILE}" == "workspace" ]]; then
        PACKAGE_ARGS=(--workspace)
    else
        log_error "Unknown profile: ${PROFILE}. Use 'core' or 'workspace'."
        exit 1
    fi
    return 0
}

branch_args() {
    if [[ "${BRANCH}" -eq 1 ]]; then
        echo --branch
    fi
}

clean_coverage_artifacts() {
    log_info "Cleaning old coverage artifacts..."
    for group in "${WS_GROUPS[@]}"; do
        (cd "${PROJECT_ROOT}/${group}" && cargo "+${NIGHTLY_TOOLCHAIN}" llvm-cov clean)
    done
    rm -rf "${OUTPUT_DIR}"
}

run_tests_with_coverage() {
    local group="$1"
    log_info "Running tests with coverage instrumentation for profile '${PROFILE}' in ${group}..."
    cd "${PROJECT_ROOT}/${group}"
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
    local out_dir="$1"
    mkdir -p "${out_dir}/html"
    cargo "+${NIGHTLY_TOOLCHAIN}" llvm-cov report \
        --html \
        --output-dir "${out_dir}/html"
}

generate_json_report() {
    local out_dir="$1"
    cargo "+${NIGHTLY_TOOLCHAIN}" llvm-cov report \
        --json \
        --output-path "${out_dir}/coverage.json"
}

generate_lcov_report() {
    local out_dir="$1"
    cargo "+${NIGHTLY_TOOLCHAIN}" llvm-cov report \
        --lcov \
        --output-path "${out_dir}/coverage.lcov"
}

run_coverage_for_group() {
    local group="$1"
    local out_dir="${OUTPUT_DIR}/$(basename "${group}")"

    run_tests_with_coverage "${group}"

    case "${FORMAT}" in
        html)
            log_info "Generating HTML report..."
            generate_html_report "${out_dir}"
            ;;
        json)
            log_info "Generating JSON report..."
            generate_json_report "${out_dir}"
            ;;
        lcov)
            log_info "Generating LCOV report..."
            generate_lcov_report "${out_dir}"
            ;;
        all)
            log_info "Generating HTML / JSON / LCOV reports..."
            generate_html_report "${out_dir}"
            generate_json_report "${out_dir}"
            generate_lcov_report "${out_dir}"
            ;;
        *)
            log_error "Unknown format: ${FORMAT}. Use 'html', 'json', 'lcov', or 'all'."
            exit 1
            ;;
    esac
}

run_coverage() {
    local group
    for group in "${WS_GROUPS[@]}"; do
        if ! group_package_args "${group}"; then
            log_info "Profile '${PROFILE}' covers no packages in ${group}; skipping."
            continue
        fi
        run_coverage_for_group "${group}"
    done
}

print_summary() {
    log_success "Coverage report generated under ${OUTPUT_DIR}/<group>/"
    if [[ "${BRANCH}" -eq 1 ]]; then
        echo "  Branch coverage: enabled"
    else
        echo "  Branch coverage: disabled"
    fi
    local group out_dir
    for group in "${WS_GROUPS[@]}"; do
        out_dir="${OUTPUT_DIR}/$(basename "${group}")"
        [[ -d "${out_dir}" ]] || continue
        echo "  ${group}:"
        case "${FORMAT}" in
            html|all)
                echo "    HTML report : file://${out_dir}/html/index.html"
                ;;
        esac
        case "${FORMAT}" in
            json|all)
                echo "    JSON summary: ${out_dir}/coverage.json"
                ;;
        esac
        case "${FORMAT}" in
            lcov|all)
                echo "    LCOV file   : ${out_dir}/coverage.lcov"
                ;;
        esac
    done
}

main() {
    parse_args "$@"
    read_nightly_toolchain
    ensure_nightly_toolchain
    ensure_llvm_tools
    ensure_cargo_llvm_cov
    clean_coverage_artifacts
    mkdir -p "${OUTPUT_DIR}"
    run_coverage
    print_summary
}

main "$@"
