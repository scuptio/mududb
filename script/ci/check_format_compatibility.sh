#!/bin/bash
# Release gate: ensure format-related code changes are accompanied by updates
# to the compatibility matrix (mudu/src/compat) and the format contracts
# (doc/en/contract/ and doc/cn/contract/).
#
# Usage:
#   ./script/ci/check_format_compatibility.sh [BASE_REF]
#
# BASE_REF defaults to origin/main.  In CI, pass the pull-request base branch.

set -euo pipefail

BASE_REF="${1:-origin/main}"

FORMAT_PATHS=(
  "mudu_kernel/src/storage/page/format/"
  "mudu_kernel/src/wal/format/"
  "mudu_contract/src/protocol/format/"
)

COMPAT_PATH="mudu/src/compat/"
CONTRACT_EN_PATH="doc/en/contract/"
CONTRACT_CN_PATH="doc/cn/contract/"

echo "Checking format compatibility against ${BASE_REF}"

if ! git rev-parse --verify "${BASE_REF}" >/dev/null 2>&1; then
  echo "WARNING: base ref '${BASE_REF}' not found; skipping compatibility gate."
  exit 0
fi

changed_files=$(
  (git diff --name-only "${BASE_REF}" || true)
  printf '\n'
  (git status --porcelain | sed 's/^...//' || true)
)

format_changed=false
for path in "${FORMAT_PATHS[@]}"; do
  if echo "${changed_files}" | grep -q "^${path}"; then
    format_changed=true
    echo "  format change detected: ${path}"
  fi
done

if [ "${format_changed}" = false ]; then
  echo "No format-related changes detected; compatibility gate passes."
  exit 0
fi

compat_changed=$(echo "${changed_files}" | grep "^${COMPAT_PATH}" || true)
contract_en_changed=$(echo "${changed_files}" | grep "^${CONTRACT_EN_PATH}" || true)
contract_cn_changed=$(echo "${changed_files}" | grep "^${CONTRACT_CN_PATH}" || true)

if [ -z "${compat_changed}" ] && [ -z "${contract_en_changed}" ] && [ -z "${contract_cn_changed}" ]; then
  echo
  echo "ERROR: Format implementation changed but neither of the following was updated:"
  echo "  - ${COMPAT_PATH}          (compatibility matrix)"
  echo "  - ${CONTRACT_EN_PATH}  (English format contracts)"
  echo "  - ${CONTRACT_CN_PATH}  (Chinese format contracts)"
  echo
  echo "When changing a persistent or wire format, update the compatibility matrix"
  echo "and the corresponding contract document before releasing."
  exit 1
fi

if [ -n "${compat_changed}" ]; then
  echo "  compatibility matrix updated: ${COMPAT_PATH}"
fi
if [ -n "${contract_en_changed}" ]; then
  echo "  English contract documents updated: ${CONTRACT_EN_PATH}"
fi
if [ -n "${contract_cn_changed}" ]; then
  echo "  Chinese contract documents updated: ${CONTRACT_CN_PATH}"
fi

echo "Compatibility gate passes."
