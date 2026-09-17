#!/bin/bash
# Run the full local verification suite across the four independent group
# workspaces (crates/{common,db-kernel,sdk,tools}). One cargo invocation at a
# time to stay within the shared memory budget (.cargo/config.toml: jobs = 4,
# RUST_TEST_THREADS = 4).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

# Note: GROUPS is a special bash variable (the user's group IDs) and cannot be
# assigned to, so this array uses a different name.
WS_GROUPS=(crates/common crates/db-kernel crates/sdk crates/tools)

for group in "${WS_GROUPS[@]}"; do
    echo "=================================================================="
    echo "=== ${group}: cargo fmt -- --check"
    echo "=================================================================="
    (cd "${REPO_ROOT}/${group}" && cargo fmt -- --check)

    echo "=== ${group}: cargo clippy --workspace --all-targets -- -D warnings"
    (cd "${REPO_ROOT}/${group}" && cargo clippy --workspace --all-targets -- -D warnings)

    echo "=== ${group}: cargo test --no-run --workspace"
    (cd "${REPO_ROOT}/${group}" && cargo test --no-run --workspace)
done

echo ""
echo "All group workspaces passed fmt, clippy, and test compilation."
