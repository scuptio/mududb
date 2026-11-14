#!/usr/bin/env python3
"""Run the test suite of every workspace (cargo make test).

Cross-platform runner shared by the `test` task (all groups, fail-fast) and
the `test-group-*` tasks (`--group <id>` for a single workspace). Identical
command per group: cargo test --workspace -- --quiet. The `--quiet` after `--`
collapses libtest's one-line-per-test output into progress dots: with
thousands of tests across six workspaces the per-test lines bury failures
and summaries. Failures still print their full captured output; per-suite
`test result:` summaries are kept.
"""
import argparse
import subprocess
import sys
import time
from pathlib import Path
from typing import Dict

REPO_ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO_ROOT / "script" / "lib"))
from cli import normalized_argv  # noqa: E402
from output import cmd as echo_cmd  # noqa: E402
from output import error, format_duration, highlight, status  # noqa: E402

GROUPS: Dict[str, str] = {
    "common": "crates/common",
    "db-kernel": "crates/db-kernel",
    "sdk": "crates/sdk",
    "example-mpk": "example/mpk",
    "tools": "crates/tools",
    "mudu-api": "crates/sdk/mudu_api/rust",
}


def run_group(group: str) -> None:
    path = GROUPS[group]
    cmd = ["cargo", "test", "--workspace", "--", "--quiet"]
    status("Testing", highlight(f"{group} ({path})"))
    echo_cmd(cmd)
    started = time.monotonic()
    proc = subprocess.run(cmd, cwd=str(REPO_ROOT / path), text=True)
    if proc.returncode != 0:
        error(f"tests failed (exit {proc.returncode}): {group}")
        raise SystemExit(proc.returncode)
    status("Passed", f"{group} in {format_duration(time.monotonic() - started)}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--group",
        choices=sorted(GROUPS),
        default=None,
        help="Run a single workspace instead of all of them.",
    )
    args = parser.parse_args(normalized_argv())

    started = time.monotonic()
    for group in ([args.group] if args.group else GROUPS):
        run_group(group)
    status("Finished", f"all tests passed in {format_duration(time.monotonic() - started)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
