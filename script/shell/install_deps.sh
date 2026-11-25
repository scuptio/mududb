#!/usr/bin/env bash
# Compatibility entry point kept for DEPLOY.md and the example readmes:
# delegates to setup_dev_env.sh in --system-pip mode (system-wide Python
# packages, no venv), which is what deployment/container machines want.
# New code should reference script/setup_dev_env.sh directly.
set -euo pipefail

exec "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/../setup_dev_env.sh" --system-pip "$@"
