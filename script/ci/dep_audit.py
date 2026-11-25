#!/usr/bin/env python3
"""Dependency-hygiene audits across the group workspaces.

Local runner for the two dependency subcommands, executed per group workspace
because each group has its own Cargo.lock:

  cargo make udeps      find unused dependencies (needs cargo-udeps + nightly)
  cargo make outdated   report stale direct dependencies (needs cargo-outdated)
  cargo make dep-audit  both, in that order

udeps requires the pinned nightly toolchain (read from .rust-nightly-version
at the repository root) because cargo-udeps only runs on nightly. outdated
runs on the workspace's default stable toolchain. Each subcommand accepts
`--group <id>` to restrict the run to a single workspace.

Mirrors nightly-checks.yaml (udeps) and ci-hardening.yaml (outdated): udeps
covers the four crates/* groups, outdated also covers example/mpk. cargo-deny
drift is a `check` step (`cargo make check-deny`), not part of this audit.

udeps aggregates failures across workspaces; outdated is advisory (CI sets
`continue-on-error`), so it continues past a group that cargo-outdated cannot
process and reports these groups as advisory failures at the end.
"""
import argparse
import sys
from pathlib import Path
from typing import Dict

REPO_ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO_ROOT / "script" / "lib"))
from task_report import Report, add_options
from cli import normalized_argv  # noqa: E402

GROUPS: Dict[str, str] = {
    "common": "crates/common",
    "db-kernel": "crates/db-kernel",
    "sdk": "crates/sdk",
    "example-mpk": "example/mpk",
    "tools": "crates/tools",
}

# cargo-udeps CI matrix covers the four crates/* groups only.
UDEPS_GROUPS = ["common", "db-kernel", "sdk", "tools"]


def nightly_toolchain() -> str:
    version_file = REPO_ROOT / ".rust-nightly-version"
    try:
        toolchain = version_file.read_text(encoding="utf-8").strip()
    except OSError:
        return "nightly"
    return toolchain or "nightly"


# outdated remains advisory; required udeps failures make the batch fail.
SUBCOMMANDS = {"udeps": UDEPS_GROUPS, "outdated": list(GROUPS)}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "subcommand",
        choices=[*SUBCOMMANDS, "all"],
        help="Which audit to run.",
    )
    parser.add_argument(
        "--group",
        choices=sorted(GROUPS),
        default=None,
        help="Restrict the audit to a single workspace.",
    )
    add_options(parser, "dependency-audit")
    args = parser.parse_args(normalized_argv())

    subcommands = [*SUBCOMMANDS] if args.subcommand == "all" else [args.subcommand]

    for name in subcommands:
        if args.group and args.group not in SUBCOMMANDS[name]:
            parser.error(f"{name} does not cover workspace '{args.group}'")
    report = Report(args, "MuduDB dependency audit")
    for name in subcommands:
        groups = [args.group] if args.group else SUBCOMMANDS[name]
        for group in groups:
            argv = (["cargo", f"+{nightly_toolchain()}", "udeps", "--workspace", "--all-targets"]
                    if name == 'udeps' else ["cargo", "outdated", "--workspace", "--root-deps-only"])
            env = {"TMPDIR": str((REPO_ROOT / GROUPS[group]).parent)} if name == 'outdated' else None
            report.run(f"{name}: {group}", argv, REPO_ROOT / GROUPS[group],
                       advisory=name == 'outdated', env=env)
    return report.finish()



if __name__ == "__main__":
    sys.exit(main())
