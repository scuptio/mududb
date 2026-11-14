#!/usr/bin/env python3
"""Release gate: ensure format-related code changes are accompanied by updates
to the compatibility matrix (mudu/src/compat) and the format contracts
(doc/en/contract/ and doc/cn/contract/).

Cross-platform port of the former script/ci/check_format_compatibility.sh.

Usage:
  python3 script/ci/check_format_compatibility.py [BASE_REF]

BASE_REF defaults to origin/main. In CI, pass the pull-request base branch.
"""
import subprocess
import sys
from pathlib import Path
from typing import List

REPO_ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO_ROOT / "script" / "lib"))
from cli import normalized_argv  # noqa: E402
from output import error, status, warn  # noqa: E402

FORMAT_PATHS = [
    "crates/db-kernel/mudu_kernel/src/storage/page/format/",
    "crates/db-kernel/mudu_kernel/src/wal/format/",
    "crates/common/mudu_contract/src/protocol/format/",
    "crates/common/mudu_binding/src/codec/syscall_payload/",
    "crates/common/mudu_binding/wit/",
]

COMPAT_PATH = "crates/common/mudu/src/compat/"
CONTRACT_EN_PATH = "doc/en/contract/"
CONTRACT_CN_PATH = "doc/cn/contract/"


def git(*args: str) -> List[str]:
    proc = subprocess.run(["git", *args], cwd=REPO_ROOT, text=True, capture_output=True)
    return proc.stdout.splitlines() if proc.returncode == 0 else []


def changed_files(base_ref: str) -> List[str]:
    files = git("diff", "--name-only", base_ref)
    for line in git("status", "--porcelain"):
        path = line[3:] if len(line) > 3 else ""
        if " -> " in path:  # renamed entry: keep the new path
            path = path.split(" -> ", 1)[1]
        if path:
            files.append(path)
    return files


def main() -> int:
    argv = normalized_argv()
    if len(argv) > 1:
        error("usage: python3 script/ci/check_format_compatibility.py [BASE_REF]")
        return 2
    base_ref = argv[0] if argv else "origin/main"

    status("Checking", f"format compatibility against {base_ref}")
    if subprocess.run(
        ["git", "rev-parse", "--verify", base_ref], cwd=REPO_ROOT, capture_output=True
    ).returncode != 0:
        warn(f"base ref '{base_ref}' not found; skipping compatibility gate.")
        return 0

    files = changed_files(base_ref)
    format_changed = False
    for prefix in FORMAT_PATHS:
        if any(path.startswith(prefix) for path in files):
            format_changed = True
            print(f"  format change detected: {prefix}")
    if not format_changed:
        print("No format-related changes detected; compatibility gate passes.")
        return 0

    hits = {
        path: any(f.startswith(path) for f in files)
        for path in (COMPAT_PATH, CONTRACT_EN_PATH, CONTRACT_CN_PATH)
    }
    if not any(hits.values()):
        print()
        error("Format implementation changed but none of the following was updated:")
        print(f"  - {COMPAT_PATH}    (compatibility matrix)")
        print(f"  - {CONTRACT_EN_PATH}  (English format contracts)")
        print(f"  - {CONTRACT_CN_PATH}  (Chinese format contracts)")
        print()
        print("When changing a persistent or wire format, update the compatibility matrix")
        print("and the corresponding contract document before releasing.")
        return 1
    for path, hit in hits.items():
        if hit:
            print(f"  updated: {path}")
    status("Finished", "compatibility gate passes")
    return 0


if __name__ == "__main__":
    sys.exit(main())
