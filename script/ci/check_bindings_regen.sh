#!/usr/bin/env bash
# Compatibility entry point. SDK builds and CI invoke the Python script directly.
set -euo pipefail
exec "${SDK_PYTHON:-python3}" "$(dirname "$0")/check_bindings_regen.py" "$@"
