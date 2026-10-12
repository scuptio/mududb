#!/usr/bin/env bash
#
# Run GitHub Actions workflows or jobs locally with `act` and capture logs.
#
# Usage:
#   ./script/ci/act.sh                          # run .github/workflows/build.yaml
#   ./script/ci/act.sh .github/workflows/nightly-checks.yaml
#   ./script/ci/act.sh -j cargo-deny            # run a single job
#   ./script/ci/act.sh -j build -j rustdoc      # run multiple jobs
#   ./script/ci/act.sh -W .github/workflows/build-release.yaml -- --verbose

set -euo pipefail

LOG_DIR="logs"
JOBS=()
WORKFLOW=""
ACT_ARGS=()

usage() {
    cat <<EOF
Usage: $0 [-j JOB] [-W WORKFLOW] [WORKFLOW] [-- ACT_ARGS...]

Run GitHub Actions workflows or jobs locally using \`act\` and summarize
failures from captured logs.
Paths are relative to the repository root; the default workflow is build.yaml.

Options:
  -j JOB            Run a specific job with \`act -j JOB\` (can be repeated).
  -W WORKFLOW       Run a specific workflow file with \`act -W WORKFLOW\`.
  -h, --help        Show this help message.
  --                Pass remaining arguments to act (bind mode is disabled).

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
        --)
            shift
            ACT_ARGS=("$@")
            break
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

REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd -P)"
cd "$REPO_ROOT"

if ! command -v act >/dev/null 2>&1; then
    echo "Error: \`act\` is not installed." >&2
    echo "Install it from https://github.com/nektos/act or your package manager." >&2
    exit 1
fi

python3 "$REPO_ROOT/script/ci/act_preflight.py" "${ACT_ARGS[@]}"

mkdir -p "$LOG_DIR"

CONTAINER_OPTIONS="--privileged --security-opt=seccomp=unconfined --security-opt=apparmor=unconfined"

trap 'exit 130' INT
trap 'exit 143' TERM

run_act() {
    act --container-options "$CONTAINER_OPTIONS" "$@" "${ACT_ARGS[@]}" --bind=false
}

FAILED=()
PASSED=()

run_job() {
    local job="$1"
    local log_file="$LOG_DIR/${job}.log"
    local workflow_args=()
    if [[ -n "$WORKFLOW" ]]; then
        workflow_args=(-W "$WORKFLOW")
    fi

    echo
    echo "=========================================="
    echo "Running job: $job"
    echo "Log file:    $log_file"
    echo "=========================================="

    if run_act "${workflow_args[@]}" -j "$job" 2>&1 | tee "$log_file"; then
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

    if run_act -W "$workflow" 2>&1 | tee "$log_file"; then
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
        log_file="$LOG_DIR/${f}.log"
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
