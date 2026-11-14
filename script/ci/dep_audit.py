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

udeps is fail-fast (CI blocks on it); outdated is advisory (CI sets
`continue-on-error`), so it continues past a group that cargo-outdated cannot
process and reports the skipped groups at the end.
"""
import argparse
import os
import subprocess
import sys
import time
from pathlib import Path
from typing import Callable, Dict, List, Optional, Tuple

REPO_ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO_ROOT / "script" / "lib"))
from cli import normalized_argv  # noqa: E402
from output import cmd as echo_cmd  # noqa: E402
from output import error, format_duration, highlight, status, warn  # noqa: E402

GROUPS: Dict[str, str] = {
    "common": "crates/common",
    "db-kernel": "crates/db-kernel",
    "sdk": "crates/sdk",
    "example-mpk": "example/mpk",
    "tools": "crates/tools",
}

# cargo-udeps CI matrix covers the four crates/* groups only.
UDEPS_GROUPS = ["common", "db-kernel", "sdk", "tools"]


class GroupFailure(Exception):
    """A single group's audit command exited non-zero."""

    def __init__(self, code: int, stderr: str = "") -> None:
        self.code = code
        self.stderr = stderr
        super().__init__(f"exit {code}")


def nightly_toolchain() -> str:
    version_file = REPO_ROOT / ".rust-nightly-version"
    try:
        toolchain = version_file.read_text(encoding="utf-8").strip()
    except OSError:
        return "nightly"
    return toolchain or "nightly"


def run_group(
    title: str,
    group: str,
    cmd: List[str],
    env: Optional[dict] = None,
    capture: bool = False,
) -> None:
    path = GROUPS[group]
    status(title, highlight(f"{group} ({path})"))
    echo_cmd(cmd)
    started = time.monotonic()
    try:
        proc = subprocess.run(
            cmd,
            cwd=str(REPO_ROOT / path),
            text=True,
            env=env,
            capture_output=capture,
        )
    except FileNotFoundError as exc:
        error(f"command not found: {exc.filename}")
        raise SystemExit(1)
    if proc.returncode != 0:
        raise GroupFailure(proc.returncode, proc.stderr if capture else "")
    if capture:
        if proc.stdout:
            sys.stdout.write(proc.stdout)
        if proc.stderr:
            sys.stderr.write(proc.stderr)
    status("Passed", f"{group} in {format_duration(time.monotonic() - started)}")


def audit_udeps(group: str) -> None:
    cmd = ["cargo", f"+{nightly_toolchain()}", "udeps", "--workspace", "--all-targets"]
    run_group("Auditing", group, cmd)


def audit_outdated(group: str) -> None:
    cmd = ["cargo", "outdated", "--workspace", "--root-deps-only"]
    # cargo-outdated copies the workspace into $TMPDIR to apply --workspace
    # updates; keeping the temp dir beside the groups (as CI does) lets the
    # cross-group `../../<group>/<crate>` path dependencies still resolve.
    # Output is captured so a group that fails (e.g. the db-kernel collision)
    # can be reported as a single skip line instead of a raw error dump.
    env = {**os.environ, "TMPDIR": str((REPO_ROOT / GROUPS[group]).parent)}
    run_group("Auditing", group, cmd, env=env, capture=True)


# (groups, audit_fn, fail_fast). outdated is non-blocking in CI, so it does not
# abort the run when a single group cannot be processed.
SUBCOMMANDS: Dict[str, Tuple[List[str], Callable[[str], None], bool]] = {
    "udeps": (UDEPS_GROUPS, audit_udeps, True),
    "outdated": (list(GROUPS), audit_outdated, False),
}


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
    args = parser.parse_args(normalized_argv())

    subcommands = [*SUBCOMMANDS] if args.subcommand == "all" else [args.subcommand]

    started = time.monotonic()
    skipped: List[Tuple[str, str, int]] = []
    for name in subcommands:
        groups, audit, fail_fast = SUBCOMMANDS[name]
        if args.group:
            if args.group not in groups:
                error(f"{name} does not cover workspace '{args.group}' (covers: {', '.join(groups)})")
                raise SystemExit(2)
            groups = [args.group]
        for group in groups:
            try:
                audit(group)
            except GroupFailure as exc:
                if fail_fast:
                    error(f"{name} failed (exit {exc.code}): {group}")
                    raise SystemExit(exc.code)
                first = next((l for l in exc.stderr.strip().splitlines() if l.strip()), "")
                first = first.strip()
                if first.startswith("error:"):
                    first = first[len("error:"):].strip()
                warn(f"{name}: {group} skipped (exit {exc.code})" + (f": {first}" if first else ""))
                skipped.append((name, group, exc.code))

    if skipped:
        status(
            "Finished",
            f"audit completed with {len(skipped)} group(s) skipped in "
            f"{format_duration(time.monotonic() - started)}",
        )
    else:
        status("Finished", f"dependency audit passed in {format_duration(time.monotonic() - started)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
