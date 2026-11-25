#!/usr/bin/env python3
"""Full local verification suite across the group workspaces (cargo make check).

Cross-platform port of the former script/shell/check_all.sh, with identical
coverage and aggregated failure reporting: one cargo invocation at a time to stay
within the shared memory budget (.cargo/config.toml: jobs = 4,
RUST_TEST_THREADS = 4).

Per group workspace: cargo fmt --check, cargo clippy --workspace --all-targets
-- -D warnings, cargo test --no-run --workspace. Then the repository-wide
invariants: versions.toml consistency, ARCHITECTURE.md layering, the
production test-feature ban, and code metrics over crates/ and example/.

`--only` selects a subset by comma-separated step ids: per-workspace `fmt`,
`clippy`, `test-compile`, `deny` (opt-in — requires cargo-deny), `group`
(fmt+clippy+test-compile), or repo-level `deps`, `arch`, `test-feature`,
`metrics`, `todo-panic`. The pre-commit hooks use `--only fmt,clippy,deny` as
a fast gate.
"""
import argparse
import sys
from pathlib import Path
from typing import List, Optional, Set, Tuple

REPO_ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO_ROOT / "script" / "lib"))
from task_report import Report, add_options
from cli import normalized_argv  # noqa: E402
from output import error  # noqa: E402

WS_GROUPS = ["crates/common", "crates/db-kernel", "crates/sdk", "example/mpk", "crates/tools"]

# Per-workspace steps: (id, label, argv). fmt/clippy/test-compile run by
# default; deny is opt-in via --only because it requires cargo-deny.
# The deny step deliberately covers only bans/licenses/sources: those are
# deterministic functions of the lockfiles. The advisories check depends on
# the freshly fetched advisory DB, so its result changes over time without
# any commit — it runs in CI (ci-hardening.yaml), not in local gates.
GroupStep = Tuple[str, str, List[str]]
PER_GROUP_STEPS: List[GroupStep] = [
    ("fmt", "cargo fmt -- --check", ["cargo", "fmt", "--", "--check"]),
    (
        "clippy",
        "cargo clippy --workspace --all-targets -- -D warnings",
        ["cargo", "clippy", "--workspace", "--all-targets", "--", "-D", "warnings"],
    ),
    (
        "test-compile",
        "cargo test --no-run --workspace",
        ["cargo", "test", "--no-run", "--workspace"],
    ),
]
DENY_STEP: GroupStep = (
    "deny",
    "cargo deny check bans licenses sources",
    [
        "cargo", "deny", "check", "--config", str(REPO_ROOT / "deny.toml"),
        "bans", "licenses", "sources",
    ],
)
ALL_GROUP_STEPS: List[GroupStep] = [*PER_GROUP_STEPS, DENY_STEP]

REPO_STEPS = [
    (
        "deps",
        "workspace dependency consistency (versions.toml)",
        [sys.executable, str(REPO_ROOT / "script/ci/sync_workspace_deps.py"), "--check"],
        REPO_ROOT,
    ),
    (
        "arch",
        "crate layering (ARCHITECTURE.md)",
        [sys.executable, str(REPO_ROOT / "script/ci/check_arch.py")],
        REPO_ROOT,
    ),
    (
        "test-feature",
        "test feature kept out of production dependencies",
        [sys.executable, str(REPO_ROOT / "script/ci/check_test_feature.py")],
        REPO_ROOT,
    ),
    (
        "metrics",
        "code metrics (crates/)",
        [
            "cargo", "run", "--manifest-path", "crates/tools/Cargo.toml",
            "-p", "mudu_metrics", "--", "--root", "crates",
            "--config", "code_metrics.toml",
        ],
        REPO_ROOT,
    ),
    (
        "metrics",
        "code metrics (example/)",
        [
            "cargo", "run", "--manifest-path", "crates/tools/Cargo.toml",
            "-p", "mudu_metrics", "--", "--root", "example",
            "--config", "code_metrics.toml",
        ],
        REPO_ROOT,
    ),
    (
        "todo-panic",
        "TODO / panic-surface report (advisory)",
        [sys.executable, str(REPO_ROOT / "script/ci/check_todo_panic.py")],
        REPO_ROOT,
    ),
    (
        "wit-ddl",
        "DDL -> WIT projection drift (object-definition pipeline)",
        [sys.executable, str(REPO_ROOT / "script/ci/check_wit_ddl.py")],
        REPO_ROOT,
    ),
]

DEFAULT_GROUP_IDS = {step_id for step_id, _, _ in PER_GROUP_STEPS}


def parse_only(only: Optional[str]) -> Tuple[Set[str], Set[str]]:
    """Map --only to (per-group step ids, repo-level step ids)."""
    if only is None:
        return set(DEFAULT_GROUP_IDS), {step_id for step_id, _, _, _ in REPO_STEPS}
    group_ids = {step_id for step_id, _, _ in ALL_GROUP_STEPS}
    repo_ids = {step_id for step_id, _, _, _ in REPO_STEPS}
    valid = group_ids | repo_ids | {"group"}
    wanted_group: Set[str] = set()
    wanted_repo: Set[str] = set()
    ids = [part.strip() for part in only.split(",")]
    unknown = [step_id for step_id in ids if step_id not in valid]
    if unknown:
        error(f"unknown step id(s): {', '.join(unknown)} (valid: {', '.join(sorted(valid))})")
        raise SystemExit(2)
    for step_id in ids:
        if step_id == "group":
            wanted_group.update(DEFAULT_GROUP_IDS)
        elif step_id in group_ids:
            wanted_group.add(step_id)
        else:
            wanted_repo.add(step_id)
    return wanted_group, wanted_repo


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--only",
        metavar="STEPS",
        help="Comma-separated step ids to run instead of everything: per-workspace "
        "'fmt', 'clippy', 'test-compile', 'deny' (opt-in, needs cargo-deny), "
        "'group' (fmt+clippy+test-compile), or repo-level 'deps', 'arch', "
        "'test-feature', 'metrics', 'todo-panic'.",
    )
    add_options(parser, "checks")
    args = parser.parse_args(normalized_argv())
    wanted_group, wanted_repo = parse_only(args.only)

    report = Report(args, "MuduDB checks")
    for group in WS_GROUPS:
        for step_id, label, cmd in ALL_GROUP_STEPS:
            if step_id in wanted_group:
                report.run(f"{group}: {label}", cmd, REPO_ROOT / group)

    for step_id, title, cmd, cwd in REPO_STEPS:
        if step_id in wanted_repo:
            report.run(title, cmd, cwd)

    return report.finish()


if __name__ == "__main__":
    sys.exit(main())
