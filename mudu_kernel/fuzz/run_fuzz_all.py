#!/usr/bin/env python3
"""Run all mudu_kernel fuzz targets and manage their corpora.

Pipeline per target (see fuzz.md for the underlying manual commands):

1. Seed ``corpus/<target>/`` from ``golden_corpus/<target>/`` (files that
   already exist are skipped), so historical inputs take part in mutation.
2. Long run: ``cargo +<nightly> fuzz run <target> -- -max_total_time=<t>``;
   libFuzzer writes newly-covered inputs into ``corpus/<target>/`` (this is
   how the corpus grows).
3. Crash check: if ``artifacts/<target>/`` gained a file, print the
   single-shot reproduction command and stop with a non-zero exit code.
4. Golden-corpus regression sink: replay the whole corpus with
   ``GOLDEN_CORPUS=1`` and ``-runs=0`` (executes each corpus input once,
   then exits) so every input is also dumped (md5-named) into
   ``golden_corpus/<target>/``; plain ``cargo test -p mudu_kernel`` then
   replays them via ``_test_target``.
5. Minimize with ``cargo +<nightly> fuzz cmin <target>``.

Both ``corpus/`` and ``golden_corpus/`` are git-tracked: after the run the
script prints ``git status --short`` for them; committing is left to the
user (this script never mutates git state).

Usage:
    python3 mudu_kernel/fuzz/run_fuzz_all.py [-t SECONDS] [-j N]
                                             [--rss-limit-mb MB]
                                             [--targets a,b,c]
                                             [--with-coverage] [--list]

Memory: libFuzzer caps each worker at ``--rss-limit-mb`` (default 2048);
with ``-j N`` peak fuzzing memory is roughly N times that. The build itself
is already capped by the workspace ``.cargo/config.toml`` (``jobs = 4``).
"""

from __future__ import annotations

import argparse
import os
import shutil
import subprocess
import sys
from pathlib import Path

FUZZ_DIR = Path(__file__).resolve().parent
REPO_ROOT = FUZZ_DIR.parent.parent
NIGHTLY_FILE = REPO_ROOT / ".rust-nightly-version"
CORPUS_DIR = FUZZ_DIR / "corpus"
GOLDEN_DIR = FUZZ_DIR / "golden_corpus"
ARTIFACTS_DIR = FUZZ_DIR / "artifacts"
COVERAGE_DIR = FUZZ_DIR / "coverage"

CARGO_FUZZ_INSTALL = "cargo +{nightly} install cargo-fuzz --version 0.13.2 --locked"


def log(msg: str) -> None:
    print(f"[run_fuzz_all] {msg}", flush=True)


def warn(msg: str) -> None:
    print(f"[run_fuzz_all] warning: {msg}", file=sys.stderr, flush=True)


def die(msg: str) -> "NoReturn":  # noqa: F821
    print(f"[run_fuzz_all] error: {msg}", file=sys.stderr)
    sys.exit(1)


def nightly_toolchain() -> str:
    if not NIGHTLY_FILE.exists():
        die(f"{NIGHTLY_FILE} not found; cannot determine the pinned nightly")
    toolchain = NIGHTLY_FILE.read_text().strip()
    if not toolchain:
        die(f"{NIGHTLY_FILE} is empty")
    return toolchain


def check_cargo_fuzz(nightly: str) -> None:
    proc = subprocess.run(
        ["cargo", f"+{nightly}", "fuzz", "--version"],
        cwd=FUZZ_DIR,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    if proc.returncode != 0:
        die(
            f"cargo-fuzz not available for {nightly}; install it with:\n"
            f"  {CARGO_FUZZ_INSTALL.format(nightly=nightly)}"
        )


def available_targets() -> list[str]:
    return sorted(p.stem for p in (FUZZ_DIR / "fuzz_targets").glob("*.rs"))


def snapshot_artifacts(target: str) -> set[Path]:
    d = ARTIFACTS_DIR / target
    if not d.exists():
        return set()
    return {p for p in d.iterdir() if p.is_file()}


def run(cmd: list[str], env: dict | None = None) -> int:
    log("+ " + " ".join(cmd))
    return subprocess.run(cmd, cwd=FUZZ_DIR, env=env).returncode


def seed_corpus(target: str) -> None:
    src = GOLDEN_DIR / target
    dst = CORPUS_DIR / target
    dst.mkdir(parents=True, exist_ok=True)
    if not src.exists():
        return
    copied = 0
    for f in src.iterdir():
        if f.is_file() and not (dst / f.name).exists():
            shutil.copy2(f, dst / f.name)
            copied += 1
    if copied:
        log(f"seeded corpus/{target} with {copied} golden input(s)")


def fuzz_run(nightly: str, target: str, seconds: int, jobs: int | None, rss_limit_mb: int) -> int:
    cmd = [
        "cargo",
        f"+{nightly}",
        "fuzz",
        "run",
        target,
        "--",
        f"-max_total_time={seconds}",
        f"-rss_limit_mb={rss_limit_mb}",
        "-use_value_profile=1",
        "-print_final_stats=1",
    ]
    if jobs:
        cmd += [f"-jobs={jobs}", f"-workers={jobs}"]
    return run(cmd)


def dump_golden_corpus(nightly: str, target: str) -> None:
    corpus = CORPUS_DIR / target
    has_files = corpus.exists() and any(p.is_file() for p in corpus.iterdir())
    if not has_files:
        log(f"corpus/{target} is empty; skipping golden-corpus dump")
        return
    # `-runs=0` makes libFuzzer execute every corpus input once while reading
    # the corpus, then exit. Going through `cargo fuzz run` (rather than
    # invoking the binary directly) keeps CARGO_MANIFEST_DIR set, which
    # `golden_corpus_path()` resolves through at runtime.
    env = dict(os.environ, GOLDEN_CORPUS="1")
    rc = run(["cargo", f"+{nightly}", "fuzz", "run", target, "--", "-runs=0"], env=env)
    if rc != 0:
        die(
            f"golden-corpus replay of corpus/{target} failed; an input "
            f"crashes outside fuzzing — inspect {target} first"
        )


def minimize_corpus(nightly: str, target: str) -> None:
    rc = run(["cargo", f"+{nightly}", "fuzz", "cmin", target])
    if rc != 0:
        warn(f"corpus minimization failed for {target} (exit {rc}); keeping corpus as-is")


def coverage_report(nightly: str, target: str) -> None:
    """Best-effort HTML coverage under fuzz/coverage/<target>/.

    Requires llvm-tools-preview for the pinned nightly; missing tools or
    binaries downgrade the step to a warning instead of failing the run.
    """
    rc = run(["cargo", f"+{nightly}", "fuzz", "coverage", target])
    if rc != 0:
        warn(f"cargo fuzz coverage failed for {target} (exit {rc}); skipping report")
        return
    sysroot = subprocess.run(
        ["rustc", f"+{nightly}", "--print", "sysroot"],
        check=True,
        stdout=subprocess.PIPE,
    ).stdout.decode().strip()
    host = subprocess.run(
        ["rustc", f"+{nightly}", "-vV"],
        check=True,
        stdout=subprocess.PIPE,
    ).stdout.decode()
    host_triple = next(
        (line.split(":", 1)[1].strip() for line in host.splitlines() if line.startswith("host:")),
        None,
    )
    if not host_triple:
        warn("cannot determine host triple; skipping llvm-cov report")
        return
    tool_dir = Path(sysroot) / "lib" / "rustlib" / host_triple / "bin"
    llvm_cov = tool_dir / "llvm-cov"
    if not llvm_cov.exists():
        warn(
            f"{llvm_cov} not found; install with "
            f"`rustup +{nightly} component add llvm-tools-preview`; skipping report"
        )
        return
    binary = FUZZ_DIR / "target" / host_triple / "coverage" / host_triple / "release" / target
    if not binary.exists():
        warn(f"coverage binary {binary} not found; skipping llvm-cov report")
        return
    profdatas = sorted(
        (FUZZ_DIR / "target").rglob(f"*{target}*.profdata"),
        key=lambda p: p.stat().st_mtime,
    )
    if not profdatas:
        warn(f"no .profdata found for {target}; skipping llvm-cov report")
        return
    out = COVERAGE_DIR / target
    rc = run(
        [
            str(llvm_cov),
            "show",
            str(binary),
            f"-instr-profile={profdatas[-1]}",
            "-format=html",
            f"-output-dir={out}",
        ]
    )
    if rc != 0:
        warn(f"llvm-cov report failed for {target} (exit {rc})")
    else:
        log(f"coverage report: {out}/index.html")


def git_status_summary() -> None:
    proc = subprocess.run(
        [
            "git",
            "status",
            "--short",
            "--",
            str(CORPUS_DIR.relative_to(REPO_ROOT)),
            str(GOLDEN_DIR.relative_to(REPO_ROOT)),
        ],
        cwd=REPO_ROOT,
        stdout=subprocess.PIPE,
    )
    if proc.returncode != 0:
        return
    changed = proc.stdout.decode().strip()
    if changed:
        log("corpus changes (commit to keep them):")
        print(changed)
    else:
        log("no corpus changes")


def main() -> int:
    parser = argparse.ArgumentParser(
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument(
        "-t",
        "--time",
        type=int,
        default=300,
        help="-max_total_time per target in seconds (default: 300)",
    )
    parser.add_argument(
        "-j",
        "--jobs",
        type=int,
        default=None,
        help="libFuzzer -jobs/-workers for parallel fuzzing (peak memory "
        "scales with jobs x --rss-limit-mb; leave unset on small machines)",
    )
    parser.add_argument(
        "--rss-limit-mb",
        type=int,
        default=2048,
        help="per-worker -rss_limit_mb (libFuzzer default: 2048; 0 disables "
        "the limit — not recommended on memory-constrained machines)",
    )
    parser.add_argument(
        "--targets",
        default=None,
        help="comma-separated subset of fuzz targets (default: all)",
    )
    parser.add_argument(
        "--with-coverage",
        action="store_true",
        help="also generate HTML coverage reports under fuzz/coverage/",
    )
    parser.add_argument(
        "--list",
        action="store_true",
        help="list available fuzz targets and exit",
    )
    args = parser.parse_args()

    targets_all = available_targets()
    if args.list:
        for t in targets_all:
            print(t)
        return 0

    if args.targets:
        targets = [t.strip() for t in args.targets.split(",") if t.strip()]
        unknown = [t for t in targets if t not in targets_all]
        if unknown:
            die(f"unknown fuzz target(s): {', '.join(unknown)} (see --list)")
    else:
        targets = targets_all
    if not targets:
        die("no fuzz targets found")
    if args.time <= 0:
        die("--time must be positive")
    if args.rss_limit_mb < 0:
        die("--rss-limit-mb must be >= 0")

    nightly = nightly_toolchain()
    check_cargo_fuzz(nightly)
    log(
        f"toolchain: {nightly}; targets: {', '.join(targets)}; "
        f"budget: {args.time}s each; rss limit: {args.rss_limit_mb} MB/worker"
    )

    for target in targets:
        log(f"=== {target} ===")
        seed_corpus(target)
        before = snapshot_artifacts(target)
        rc = fuzz_run(nightly, target, args.time, args.jobs, args.rss_limit_mb)
        after = snapshot_artifacts(target)
        new_artifacts = sorted(after - before)
        if new_artifacts or rc != 0:
            for artifact in new_artifacts:
                print(f"[run_fuzz_all] CRASH artifact: {artifact}", file=sys.stderr)
                print(
                    f"[run_fuzz_all] reproduce: cargo +{nightly} fuzz run {target} {artifact}",
                    file=sys.stderr,
                )
            if not new_artifacts:
                warn(f"fuzz run for {target} exited {rc} without a new artifact")
            return 1
        dump_golden_corpus(nightly, target)
        minimize_corpus(nightly, target)
        if args.with_coverage:
            coverage_report(nightly, target)

    log("all targets done")
    git_status_summary()
    return 0


if __name__ == "__main__":
    sys.exit(main())
