#!/usr/bin/env bash
#
# Run GitHub Actions workflows or jobs locally with `act` and capture logs.
#
# Usage:
#   ./script/ci/run-local.sh                          # run .github/workflows/build.yaml
#   ./script/ci/run-local.sh .github/workflows/nightly-checks.yaml
#   ./script/ci/run-local.sh -j cargo-deny            # run a single job
#   ./script/ci/run-local.sh -j build -j rustdoc      # run multiple jobs

set -euo pipefail

LOG_DIR="logs"
JOBS=()
WORKFLOW=""

usage() {
    cat <<EOF
Usage: $0 [-j JOB] [-W WORKFLOW] [WORKFLOW]

Run GitHub Actions workflows or jobs locally using \`act\` and summarize
failures from captured logs.

Options:
  -j JOB            Run a specific job with \`act -j JOB\` (can be repeated).
  -W WORKFLOW       Run a specific workflow file with \`act -W WORKFLOW\`.
  -h, --help        Show this help message.

Examples:
  $0
  $0 .github/workflows/nightly-checks.yaml
  $0 -j cargo-deny
  $0 -j build -j rustdoc
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        -j)
            if [[ $# -lt 2 ]]; then
                echo "Error: -j requires a job name." >&2
                usage >&2
                exit 1
            fi
            JOBS+=("$2")
            shift 2
            ;;
        -W)
            if [[ $# -lt 2 ]]; then
                echo "Error: -W requires a workflow file path." >&2
                usage >&2
                exit 1
            fi
            WORKFLOW="$2"
            shift 2
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        -*)
            echo "Error: unknown option $1" >&2
            usage >&2
            exit 1
            ;;
        *)
            if [[ -n "$WORKFLOW" ]]; then
                echo "Error: only one workflow file may be specified." >&2
                usage >&2
                exit 1
            fi
            WORKFLOW="$1"
            shift
            ;;
    esac
done

if ! command -v act >/dev/null 2>&1; then
    echo "Error: \`act\` is not installed." >&2
    echo "Install it from https://github.com/nektos/act or your package manager." >&2
    exit 1
fi

mkdir -p "$LOG_DIR"

FAILED=()
PASSED=()

run_job() {
    local job="$1"
    local log_file="$LOG_DIR/${job}.log"

    echo
    echo "=========================================="
    echo "Running job: $job"
    echo "Log file:    $log_file"
    echo "=========================================="

    if act -j "$job" 2>&1 | tee "$log_file"; then
        PASSED+=("$job")
        echo "[OK] $job"
    else
        FAILED+=("$job")
        echo "[FAIL] $job"
    fi
}

run_workflow() {
    local workflow="$1"
    local name
    name=$(basename "$workflow" .yaml)
    name=$(basename "$name" .yml)
    local log_file="$LOG_DIR/${name}.log"

    echo
    echo "=========================================="
    echo "Running workflow: $workflow"
    echo "Log file:         $log_file"
    echo "=========================================="

    if act -W "$workflow" 2>&1 | tee "$log_file"; then
        PASSED+=("$name")
        echo "[OK] $name"
    else
        FAILED+=("$name")
        echo "[FAIL] $name"
    fi
}

if [[ ${#JOBS[@]} -gt 0 ]]; then
    for job in "${JOBS[@]}"; do
        run_job "$job"
    done
elif [[ -n "$WORKFLOW" ]]; then
    run_workflow "$WORKFLOW"
else
    run_workflow ".github/workflows/build.yaml"
fi

echo
echo "=========================================="
echo "Summary"
echo "=========================================="
echo "Passed: ${#PASSED[@]}"
for p in "${PASSED[@]}"; do
    echo "  [OK] $p"
done

if [[ ${#FAILED[@]} -gt 0 ]]; then
    echo "Failed: ${#FAILED[@]}"
    for f in "${FAILED[@]}"; do
        echo "  [FAIL] $f"
    done

    echo
    echo "Error snippets (last 20 matching lines per failed run):"
    for f in "${FAILED[@]}"; do
        local log_file="$LOG_DIR/${f}.log"
        echo
        echo "---- $f ----"
        if [[ -f "$log_file" ]]; then
            grep -E -i "(ERROR|FAIL|failed|::error::|panicked|thread '.*' panicked|warning:)" "$log_file" | tail -n 20 || true
        else
            echo "(log file not found)"
        fi
    done

    exit 1
else
    echo "All passed."
fi
