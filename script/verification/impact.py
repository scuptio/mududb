#!/usr/bin/env python3
"""Map changed files to affected verification targets; check CI path triggers.

Usage:
  impact.py --affected-by FILE [FILE ...]   print affected target names
  impact.py --check-ci WORKFLOW             verify the workflow covers every target
"""
from __future__ import annotations

import argparse
from pathlib import Path
import re
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[2]

# Changes here can move every extraction/proof, regardless of the target crate.
GLOBAL_FILES = {
    "verification/targets.toml",
    "verification/toolchain.lock.toml",
    "script/doctor.py",
    "Makefile.toml",
    ".github/workflows/formal-verification.yaml",
}
GLOBAL_PREFIXES = (
    "verification/lean/",
    "verification/extract/",
    "script/verification/",
    "script/lib/",
    ".cargo/",
)
# crates/common holds mudu, a transitive path dependency of the kernel crates.
COMMON_PREFIX = "crates/common/"

# Workflow path entries that are intentionally present without covering a
# registered target source (group manifests, known callers, fixed triggers).
KNOWN_CI_EXTRAS = {
    "crates/common/Cargo.toml",
    "crates/common/Cargo.lock",
    "crates/common/mudu_type/src/data_type_impl/fn_string.rs",
    "script/doctor.py",
    "Makefile.toml",
    ".github/workflows/formal-verification.yaml",
}


def load_targets() -> dict:
    return tomllib.loads((ROOT / "verification/targets.toml").read_text())["target"]


def target_crate_prefix(source: str) -> str:
    parts = Path(source).parts  # crates/<group>/<crate>/src/...
    return "/".join(parts[:3]) + "/"


def affected_targets(changed: list[str], targets: dict) -> set[str]:
    """Conservative: a change inside a target's crate, crates/common, or the
    verification machinery itself affects every (or that crate's) targets."""
    affected: set[str] = set()
    for path in changed:
        path = path.strip()
        if not path:
            continue
        if path in GLOBAL_FILES or any(path.startswith(prefix) for prefix in GLOBAL_PREFIXES):
            return set(targets)
        if path.startswith(COMMON_PREFIX):
            return set(targets)
        for name, target in targets.items():
            if path.startswith(target_crate_prefix(target["source"])):
                affected.add(name)
    return affected


def glob_to_regex(pattern: str) -> re.Pattern:
    """Minimal GitHub path-filter glob: `*` within a segment, trailing `**`."""
    body = []
    segments = pattern.split("/")
    for index, segment in enumerate(segments):
        last = index == len(segments) - 1
        if segment == "**":
            body.append(r".*" if last else r"(?:[^/]+/)*")
        else:
            part = re.escape(segment).replace(r"\*", r"[^/]*")
            body.append(part if last else part + "/")
    return re.compile("^" + "".join(body) + "$")


def glob_covers(pattern: str, path: str) -> bool:
    return bool(glob_to_regex(pattern).match(path))


def workflow_paths(text: str) -> list[str]:
    """Quoted list entries in the `on:` trigger blocks (before `jobs:`)."""
    on_block = text.split("jobs:")[0]
    return re.findall(r"^\s+- '([^']+)'", on_block, flags=re.MULTILINE)


def check_ci(workflow: Path, targets: dict) -> tuple[list[str], list[str]]:
    entries = workflow_paths(workflow.read_text())
    problems, warnings = [], []
    for name, target in targets.items():
        source = target["source"]
        if not any(glob_covers(entry, source) for entry in entries):
            problems.append(f"CI paths do not cover target {name}: {source}")
    for entry in entries:
        if entry in KNOWN_CI_EXTRAS or entry in GLOBAL_FILES:
            continue
        if any(entry.startswith(prefix.rstrip("/") + "/") or entry == prefix.rstrip("/") + "/**"
               for prefix in ("verification/", "script/", ".cargo/")):
            continue
        if any(glob_covers(entry, target["source"]) for target in targets.values()):
            continue
        warnings.append(f"CI path covers no registered target (stale?): {entry}")
    return problems, warnings


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--affected-by", nargs="*", help="changed files; prints affected targets")
    parser.add_argument("--check-ci", metavar="WORKFLOW", help="verify workflow path coverage")
    args = parser.parse_args()
    targets = load_targets()
    if args.check_ci:
        problems, warnings = check_ci(Path(args.check_ci), targets)
        for warning in warnings:
            print(f"warning: {warning}")
        if problems:
            print("\n".join(problems))
            return 1
        print(f"PASS: {args.check_ci} covers all {len(targets)} registered targets")
        return 0
    if args.affected_by is not None:
        for name in sorted(affected_targets(args.affected_by, targets)):
            print(name)
        return 0
    parser.print_help()
    return 0


if __name__ == "__main__":
    sys.exit(main())
