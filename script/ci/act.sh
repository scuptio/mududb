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

LOG_DIR="${ACT_LOG_DIR:-logs/act}"
JOBS=()
WORKFLOW=""
ACT_ARGS=()

usage() {
    cat <<EOF
Usage: $0 [-j JOB] [-W WORKFLOW] [WORKFLOW] [-- ACT_ARGS...]

Run GitHub Actions workflows or jobs locally using \`act\` and summarize
failures from captured logs.
Paths are relative to the repository root; the default workflow is build.yaml.
The final report names failed matrix jobs, steps and tests, with absolute local
paths to summary.txt/json, complete console/job logs and failures.log.

Options:
  -j JOB            Run a specific job with \`act -j JOB\` (can be repeated).
  -W WORKFLOW       Run a specific workflow file with \`act -W WORKFLOW\`.
  --log-dir DIR     Parent directory for a unique run (default: logs/act).
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
        --log-dir)
            if [[ $# -lt 2 ]]; then
                echo "Error: --log-dir requires a directory." >&2
                exit 2
            fi
            LOG_DIR="$2"
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

mkdir -p "$LOG_DIR"
LOG_DIR="$(mktemp -d "$LOG_DIR/$(date +%Y%m%d-%H%M%S)-XXXXXX")"
LOG_DIR="$(cd "$LOG_DIR" && pwd -P)"
FAILED=()
SUMMARY_ARGS=()
EXIT_ARGS=()

finish_report() {
    local code=$?
    trap - EXIT
    if [[ ${#SUMMARY_ARGS[@]} -gt 0 ]]; then
        if [[ $code -eq 130 || $code -eq 143 ]]; then
            local last_invocation="${SUMMARY_ARGS[${#SUMMARY_ARGS[@]}-1]}"
            EXIT_ARGS+=(--exit-code "${last_invocation%%=*}=$code")
        fi
        python3 "$REPO_ROOT/script/ci/act_summary.py" --report-dir "$LOG_DIR" "${EXIT_ARGS[@]}" "${SUMMARY_ARGS[@]}" || {
            echo "Report generation failed; full console logs: $LOG_DIR" >&2
            if [[ $code -eq 0 ]]; then code=1; fi
        }
    fi
    exit "$code"
}
trap finish_report EXIT

if python3 "$REPO_ROOT/script/ci/act_preflight.py" "${ACT_ARGS[@]}" > "$LOG_DIR/preflight.log" 2>&1; then
    cat "$LOG_DIR/preflight.log"
else
    code=$?
    cat "$LOG_DIR/preflight.log" >&2
    SUMMARY_ARGS+=("preflight=$LOG_DIR/preflight.log")
    EXIT_ARGS+=(--exit-code "preflight=$code")
    exit "$code"
fi

CONTAINER_OPTIONS="--privileged --security-opt=seccomp=unconfined --security-opt=apparmor=unconfined"

trap 'exit 130' INT
trap 'exit 143' TERM

run_act() {
    act --container-options "$CONTAINER_OPTIONS" "$@" "${ACT_ARGS[@]}" --bind=false
}

run_job() {
    local job="$1"
    local invocation="$((${#SUMMARY_ARGS[@]} + 1))-${job//\//-}"
    local log_file="$LOG_DIR/${invocation}.console.log"
    # Without -W, act matches the job name in EVERY workflow file (e.g. the
    # SDK Publish workflow also has a "build" job); pin the default workflow.
    local workflow="${WORKFLOW:-.github/workflows/build.yaml}"
    local workflow_args=(-W "$workflow")

    echo
    echo "=========================================="
    echo "Running job: $job"
    echo "Log file:    $log_file"
    echo "=========================================="

    SUMMARY_ARGS+=("$invocation=$log_file")
    local code=0
    if run_act "${workflow_args[@]}" -j "$job" 2>&1 | tee "$log_file"; then
        echo "[OK] $job"
    else
        code=$?
        FAILED+=("$job")
        echo "[FAIL] $job"
    fi
    EXIT_ARGS+=(--exit-code "$invocation=$code")
}

run_workflow() {
    local workflow="$1"
    local name
    name=$(basename "$workflow" .yaml)
    name=$(basename "$name" .yml)
    local invocation="$((${#SUMMARY_ARGS[@]} + 1))-${name}"
    local log_file="$LOG_DIR/${invocation}.console.log"

    echo
    echo "=========================================="
    echo "Running workflow: $workflow"
    echo "Log file:         $log_file"
    echo "=========================================="

    SUMMARY_ARGS+=("$invocation=$log_file")
    local code=0
    if run_act -W "$workflow" 2>&1 | tee "$log_file"; then
        echo "[OK] $name"
    else
        code=$?
        FAILED+=("$name")
        echo "[FAIL] $name"
    fi
    EXIT_ARGS+=(--exit-code "$invocation=$code")
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


if [[ ${#FAILED[@]} -gt 0 ]]; then
    exit 1
fi
