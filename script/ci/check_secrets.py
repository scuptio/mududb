#!/usr/bin/env python3
"""Secret-scanning entry point: run gitleaks with the repository's
.gitleaks.toml config. Findings are redacted and inline suppression comments
are disallowed, so no report containing credential values is produced.

Cross-platform port of the former script/ci/check_secrets.sh.

Usage:
  python3 script/ci/check_secrets.py [directory-to-scan]

Without an argument, scans all fetched git history of this repository.
Set GITLEAKS_BIN to override the gitleaks binary.
"""
import os
import shutil
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO_ROOT / "script" / "lib"))
from cli import normalized_argv  # noqa: E402
from output import error  # noqa: E402


def main() -> int:
    argv = normalized_argv()
    if len(argv) > 1:
        error("usage: python3 script/ci/check_secrets.py [directory-to-scan]")
        return 2
    gitleaks = os.environ.get("GITLEAKS_BIN", "gitleaks")
    if not shutil.which(gitleaks):
        error(f"gitleaks not found: {gitleaks} (set GITLEAKS_BIN to override)")
        return 127
    common = [
        "--config", str(REPO_ROOT / ".gitleaks.toml"),
        "--redact=100", "--verbose", "--no-banner", "--ignore-gitleaks-allow",
    ]
    if argv:
        cmd = [gitleaks, "dir", *common, argv[0]]
    else:
        cmd = [gitleaks, "git", *common, "--log-opts=--all", str(REPO_ROOT)]
    return subprocess.run(cmd, cwd=REPO_ROOT).returncode


if __name__ == "__main__":
    sys.exit(main())
