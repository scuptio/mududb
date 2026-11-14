#!/usr/bin/env python3
"""Local coverage runner for the mududb workspace.

Cross-platform (Linux / macOS / Windows), standard library only.

Collects coverage with cargo llvm-cov (pinned nightly toolchain) across all
five group workspaces plus the `testing` crate's end-to-end tests, then merges
everything into ONE unified report under target/llvm-cov/:

  - unit stage: `cargo llvm-cov --workspace --lib --tests` per group workspace
    (the e2e-heavy `testing` crate is excluded from db-kernel here)
  - e2e stage (default on): `cargo llvm-cov --package testing --lib --tests`;
    the server runs in-process inside the test binaries, so its profraw lands
    in the shared target/llvm-cov-target/ like any other test process
  - report stage: `llvm-profdata merge` over all profraw + `llvm-cov
    show/export` over exactly the test binaries that ran, producing a single
    merged HTML/JSON/LCOV report (cargo llvm-cov report cannot merge across
    workspaces, so the report step drives the toolchain's llvm-cov directly);
    a pre-merge guard aborts the run when any collected workspace produced
    no profraw (silent group loss otherwise reads as a bogus near-0% row)
  - bindings stage (default on): the guest-language facades cannot share the
    llvm-cov report (their tests run in CPython / on the .NET runtime), so
    each is measured with its own toolchain — coverage.py for the Python
    bindings, coverlet for the C# SDK — into target/llvm-cov/bindings/<lang>/,
    and the per-language totals join the terminal summary. Missing toolchains
    skip that language with a hint instead of failing the run

Examples:
  run_coverage.py                          # everything: unit + e2e, all formats
  run_coverage.py --no-e2e                 # unit tests only (fast)
  run_coverage.py --e2e-filter string_literal -f html   # e2e smoke
  run_coverage.py --branch                 # branch coverage (unstable, see below)
  run_coverage.py --no-bindings            # Rust-only report
"""

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import time
import xml.etree.ElementTree as ET
from pathlib import Path
from typing import Dict, List, Optional, Tuple

sys.path.insert(0, str(Path(__file__).resolve().parent))
from install_deps import (
    PROJECT_ROOT,
    host_target_triple,
    read_nightly_toolchain,
)

sys.path.insert(0, str(PROJECT_ROOT / "script" / "lib"))
from output import (  # noqa: E402
    STATUS_WIDTH,
    bad,
    bold,
    dim,
    error,
    fail,
    format_duration,
    good,
    highlight,
    link,
    status,
    warn,
)

# Independent group workspaces, each with its own Cargo.lock. All share the
# repository-root target/ directory, so every stage's profraw lands flat in
# LLVM_COV_TARGET_DIR regardless of which group produced it.
WS_GROUPS = ["crates/common", "crates/db-kernel", "crates/sdk", "example/mpk", "crates/tools"]
LLVM_COV_TARGET_DIR = PROJECT_ROOT / "target" / "llvm-cov-target"
WALLET_MPK_FIXTURE = PROJECT_ROOT / "crates" / "db-kernel" / "testing" / "mpk" / "wallet.mpk"

# Branch coverage defaults off: branch-instrumented data makes llvm-cov
# (LLVM in the pinned nightly) SIGSEGV at report time in
# getInstantiationGroups. --branch opts in explicitly.

# Files outside the workspace sources are excluded from reports: registry
# dependencies, toolchain/sysroot sources, and build outputs under target/.
# `stmt_path_bench` is a `#[ignore]`d micro-benchmark module that only runs on
# demand (`cargo test -p server stmt_path_bench -- --ignored`), so it can never
# appear as covered in a regular coverage run. `test_queue/churn.rs` is an
# `#[ignore]`d queue storage baseline and `relation/relation/tests_bench.rs`
# the `#[ignore]`d write-path micro-benchmark — same on-demand category.
IGNORE_FILENAME_REGEX = (
    r"(\.cargo[\\/]registry)|(rustup[\\/]toolchains)|([\\/]target[\\/])"
    # Std-internal sources are embedded as /rustc/<hash>/library/... paths,
    # which exist only with the rust-src component installed; without this
    # exclusion llvm-cov show prints a noisy (non-fatal) error per file.
    r"|([\\/]rustc[\\/])"
    # Benchmark-only helper binaries (never run under coverage).
    r"|(stmt_path_bench\.rs)|([\\/]test_queue[\\/]churn\.rs)|([\\/]tests_bench\.rs)"
    # mpk-gated e2e harnesses: they need guest toolchains (componentize-py,
    # TinyGo, wasi-sdk) and a bootable local server; excluded until the
    # local mpk pipeline is reliable again.
    r"|(test_mpk\.rs)|(test_tpcc_concurrent_procedure)|(tpcc_mpk\.rs)"
    # Dead test module (needs a live external server; its tests never run).
    r"|(runtime_simple_test\.rs)"
)

GROUP_DIR_REGEX = re.compile(
    r"[\\/]crates[\\/](common|db-kernel|sdk|tools)[\\/]|[\\/]example[\\/]mpk[\\/]"
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run code coverage locally and produce one merged report.",
    )
    parser.add_argument(
        "-f",
        "--format",
        choices=["html", "json", "lcov", "all"],
        default="all",
        help="output format (default: all)",
    )
    parser.add_argument(
        "-o",
        "--output-dir",
        type=Path,
        default=PROJECT_ROOT / "target" / "llvm-cov",
        help="output directory (default: target/llvm-cov)",
    )
    parser.add_argument(
        "--branch",
        dest="branch",
        action="store_true",
        help="enable branch coverage (default: disabled; llvm-cov in the pinned "
        "nightly SIGSEGVs on branch data at report time)",
    )
    parser.add_argument(
        "--no-branch",
        dest="branch",
        action="store_false",
        help="disable branch coverage (default)",
    )
    parser.set_defaults(branch=False)
    parser.add_argument(
        "--e2e",
        dest="e2e",
        action="store_true",
        help="also run the testing crate's end-to-end tests (default)",
    )
    parser.add_argument(
        "--no-e2e",
        dest="e2e",
        action="store_false",
        help="skip the end-to-end tests (unit tests only, much faster)",
    )
    parser.set_defaults(e2e=True)
    parser.add_argument(
        "--bindings",
        dest="bindings",
        action="store_true",
        help="also measure the guest-language bindings with their own "
        "toolchains (coverage.py for Python, coverlet for C#); missing tools "
        "skip that language with a hint (default)",
    )
    parser.add_argument(
        "--no-bindings",
        dest="bindings",
        action="store_false",
        help="skip the guest-language bindings coverage stages",
    )
    parser.set_defaults(bindings=True)
    parser.add_argument(
        "--e2e-filter",
        metavar="SUBSTRING",
        default=None,
        help="run only e2e tests whose name contains SUBSTRING (smoke runs)",
    )
    args = parser.parse_args()
    if args.e2e_filter and not args.e2e:
        parser.error("--e2e-filter requires e2e (do not pass --no-e2e)")
    return args


def cargo_llvm_cov(toolchain: str) -> List[str]:
    return ["cargo", f"+{toolchain}", "llvm-cov"]


def install_deps() -> None:
    subprocess.run(
        [sys.executable, str(Path(__file__).resolve().parent / "install_deps.py")],
        check=True,
    )


# ---------------------------------------------------------------------------
# Collection
# ---------------------------------------------------------------------------


def unit_selector(group: str) -> List[str]:
    # The testing crate's e2e tests run in the e2e stage with normal
    # parallelism; keep them out of the serial unit-test stage.
    selector = ["--workspace"]
    if group == "crates/db-kernel":
        selector += ["--exclude", "testing"]
    return selector


def clean_coverage_artifacts(toolchain: str, output_dir: Path) -> None:
    status("Cleaning", "old coverage artifacts")
    for group in WS_GROUPS:
        subprocess.run(
            cargo_llvm_cov(toolchain) + ["clean"],
            cwd=PROJECT_ROOT / group,
            check=True,
        )
    shutil.rmtree(output_dir, ignore_errors=True)


def run_unit_tests(toolchain: str, group: str, branch: bool) -> None:
    status("Running", f"unit tests with coverage instrumentation in {highlight(group)}")
    cmd = (
        cargo_llvm_cov(toolchain)
        + unit_selector(group)
        + ["--lib", "--tests", "--no-report"]
    )
    if branch:
        cmd.append("--branch")
    cmd += ["--", "--test-threads=1"]
    env = dict(os.environ, CARGO_INCREMENTAL="0")
    subprocess.run(cmd, cwd=PROJECT_ROOT / group, env=env, check=True)


def snapshot_wallet_mpk() -> Optional[bytes]:
    # None means the fixture did not exist before the run.
    return WALLET_MPK_FIXTURE.read_bytes() if WALLET_MPK_FIXTURE.is_file() else None


def restore_wallet_mpk(snapshot: Optional[bytes]) -> None:
    # prepare_wallet_mpk rebuilds the fixture in place; the build is
    # deterministic per input state but drifts from the committed bytes as
    # dependencies move, which would leave the working tree dirty after
    # every coverage run. Roll the fixture back to its pre-run state — when
    # the wire format genuinely changes, rebuilding and committing the
    # fixture happens outside this script.
    if snapshot is None:
        if WALLET_MPK_FIXTURE.exists():
            WALLET_MPK_FIXTURE.unlink()
            status("Restored", "wallet.mpk fixture removed (absent before the run)")
        return
    if WALLET_MPK_FIXTURE.is_file() and WALLET_MPK_FIXTURE.read_bytes() == snapshot:
        return
    WALLET_MPK_FIXTURE.write_bytes(snapshot)
    status("Restored", "wallet.mpk fixture to its pre-run bytes")


def prepare_wallet_mpk() -> None:
    # Wallet e2e tests look for testing/mpk/wallet.mpk and soft-skip when it
    # is missing. The testing crate's copy-wallet-mpk task rebuilds the CLI
    # tools and packages example/mpk/wallet; failure here only shrinks e2e
    # coverage, so warn and continue.
    status("Building", "wallet.mpk for e2e tests (best effort)")
    proc = subprocess.run(
        ["cargo", "make", "copy-wallet-mpk"],
        cwd=PROJECT_ROOT / "crates" / "db-kernel" / "testing",
    )
    if proc.returncode != 0:
        error("wallet.mpk build failed; wallet-dependent e2e tests will be skipped")


def run_e2e_tests(toolchain: str, branch: bool, e2e_filter: Optional[str]) -> None:
    status("Running", "end-to-end tests (testing crate) with coverage instrumentation")
    cmd = cargo_llvm_cov(toolchain) + [
        "--package",
        "testing",
        "--lib",
        "--tests",
        "--no-report",
    ]
    if branch:
        cmd.append("--branch")
    cmd.append("--")
    # No --test-threads=1 here: e2e keeps the default parallelism
    # (RUST_TEST_THREADS=4 from .cargo/config.toml), like `cargo make test`.
    if e2e_filter:
        cmd.append(e2e_filter)
    env = dict(os.environ, CARGO_INCREMENTAL="0")
    subprocess.run(cmd, cwd=PROJECT_ROOT / "crates/db-kernel", env=env, check=True)


# ---------------------------------------------------------------------------
# Merged report (llvm-cov / llvm-profdata from the pinned nightly toolchain)
# ---------------------------------------------------------------------------


def llvm_tool_path(toolchain: str, name: str) -> Path:
    proc = subprocess.run(
        ["rustc", f"+{toolchain}", "--print", "sysroot"],
        text=True,
        capture_output=True,
        check=True,
    )
    sysroot = Path(proc.stdout.strip())
    exe = name + (".exe" if sys.platform == "win32" else "")
    path = sysroot / "lib" / "rustlib" / host_target_triple() / "bin" / exe
    if not path.exists():
        raise RuntimeError(f"{exe} not found at {path}; install llvm-tools-preview")
    return path


def parse_show_env(toolchain: str, group: str) -> Dict[str, str]:
    # `cargo llvm-cov show-env --export-prefix` prints `export KEY='value'`
    # lines describing the exact build environment (RUSTC_WRAPPER, injected
    # RUSTFLAGS, ...) used by the collection run. Reusing it makes the
    # subsequent `cargo test --no-run` reuse the instrumented artifacts
    # instead of rebuilding.
    proc = subprocess.run(
        cargo_llvm_cov(toolchain) + ["show-env", "--export-prefix"],
        cwd=PROJECT_ROOT / group,
        text=True,
        capture_output=True,
        check=True,
    )
    env: Dict[str, str] = {}
    for line in proc.stdout.splitlines():
        line = line.strip()
        if line.startswith("export "):
            line = line[len("export ") :]
        match = re.match(r"^([A-Za-z_][A-Za-z0-9_]*)=(.*)$", line)
        if not match:
            continue
        value = match.group(2).strip()
        if len(value) >= 2 and value.startswith("'") and value.endswith("'"):
            value = value[1:-1]
        env[match.group(1)] = value
    return env


def collect_objects(toolchain: str, scopes: List[Tuple[str, List[str]]]) -> List[str]:
    # List the exact instrumented test executables for each collected scope
    # via `cargo test --no-run` compiler-artifact messages.
    objects: List[str] = []
    for group, selector in scopes:
        env = dict(os.environ)
        env.update(parse_show_env(toolchain, group))
        env["CARGO_TARGET_DIR"] = str(LLVM_COV_TARGET_DIR)
        env["CARGO_INCREMENTAL"] = "0"
        cmd = [
            "cargo",
            f"+{toolchain}",
            "test",
            "--no-run",
            "--message-format=json-render-diagnostics",
        ] + selector + ["--lib", "--tests"]
        proc = subprocess.run(
            cmd, cwd=PROJECT_ROOT / group, env=env, text=True, capture_output=True
        )
        if proc.returncode != 0:
            error(f"cargo test --no-run failed in {group}:\n{proc.stderr[-4000:]}")
            raise RuntimeError(f"object collection failed for {group}")
        for line in proc.stdout.splitlines():
            try:
                message = json.loads(line)
            except ValueError:
                continue
            if message.get("reason") != "compiler-artifact":
                continue
            executable = message.get("executable")
            if not executable:
                continue
            if "custom-build" in message.get("target", {}).get("kind", []):
                continue
            if executable not in objects:
                objects.append(executable)
    if not objects:
        raise RuntimeError("no test executables found for the collected scopes")
    return objects


def write_response_file(path: Path, args: List[str]) -> None:
    path.write_text("\n".join(args) + "\n", encoding="utf-8")


def verify_group_coverage_data(scopes: List[Tuple[str, List[str]]]) -> None:
    """Fail loudly when a collected workspace produced no profraw at all.

    cargo-llvm-cov names profraws `<workspace-dir-name>-<pid>-<hash>_<n>`
    under the shared target/llvm-cov-target, so a zero count for a scope
    means its data was lost (a concurrent `cargo llvm-cov` clean wipes the
    shared directory) or its tests ran uninstrumented — both otherwise
    surface only as a silently near-0% group in the merged report.
    """
    profraws = [p.name for p in LLVM_COV_TARGET_DIR.glob("*.profraw")]
    missing = []
    for group in {group for group, _ in scopes}:
        basename = Path(group).name
        if not any(name.startswith(f"{basename}-") for name in profraws):
            missing.append(group)
    if missing:
        raise RuntimeError(
            "no coverage data (.profraw) was produced for: "
            + ", ".join(missing)
            + "; the merged report would silently miss those workspaces. A "
            "concurrent `cargo llvm-cov` run (its clean step wipes the shared "
            "target/llvm-cov-target) or a stale uninstrumented build is the "
            "usual cause — rerun `cargo make coverage`."
        )
    status("Verifying", f"per-workspace coverage data presence ({len(profraws)} profraws)")


def merge_profdata(llvm_profdata: Path, out_dir: Path) -> Path:
    profraw = sorted(str(p) for p in LLVM_COV_TARGET_DIR.glob("*.profraw"))
    if not profraw:
        raise RuntimeError(f"no profraw files found in {LLVM_COV_TARGET_DIR}")
    status("Merging", f"{len(profraw)} profraw files")
    rsp = out_dir / "profraw.rsp"
    write_response_file(rsp, profraw)
    merged = out_dir / "merged.profdata"
    subprocess.run(
        [str(llvm_profdata), "merge", "-sparse", f"@{rsp}", "-o", str(merged)],
        check=True,
    )
    return merged


def llvm_cov_cmd(llvm_cov: Path, subcommand: str, merged: Path, objects: List[str], out_dir: Path) -> List[str]:
    rsp = out_dir / f"objects-{subcommand}.rsp"
    write_response_file(rsp, [objects[0]] + sum((["-object", o] for o in objects[1:]), []))
    return [
        str(llvm_cov),
        subcommand,
        f"--instr-profile={merged}",
        f"@{rsp}",
        f"--ignore-filename-regex={IGNORE_FILENAME_REGEX}",
    ]


def generate_reports(
    toolchain: str, objects: List[str], merged: Path, out_dir: Path, report_format: str
) -> dict:
    llvm_cov = llvm_tool_path(toolchain, "llvm-cov")
    # One full JSON export feeds the terminal summary, the per-group
    # breakdown, and (optionally) the coverage.json artifact.
    json_tmp = out_dir / "coverage.json.tmp"
    status("Exporting", "merged JSON coverage data")
    with open(json_tmp, "w", encoding="utf-8") as handle:
        subprocess.run(
            llvm_cov_cmd(llvm_cov, "export", merged, objects, out_dir) + ["-format=text"],
            stdout=handle,
            check=True,
        )
    report = json.loads(json_tmp.read_text(encoding="utf-8"))
    if report_format in ("json", "all"):
        json_tmp.replace(out_dir / "coverage.json")
    else:
        json_tmp.unlink()
    if report_format in ("html", "all"):
        status("Generating", "HTML report")
        html_dir = out_dir / "html"
        html_dir.mkdir(parents=True, exist_ok=True)
        subprocess.run(
            llvm_cov_cmd(llvm_cov, "show", merged, objects, out_dir)
            + ["-format=html", f"--output-dir={html_dir}"],
            check=True,
        )
    if report_format in ("lcov", "all"):
        status("Generating", "LCOV report")
        with open(out_dir / "coverage.lcov", "w", encoding="utf-8") as handle:
            subprocess.run(
                llvm_cov_cmd(llvm_cov, "export", merged, objects, out_dir)
                + ["-format=lcov"],
                stdout=handle,
                check=True,
            )
    return report


# ---------------------------------------------------------------------------
# Bindings coverage (guest-language facades, per-language toolchains)
#
# The guest facades cannot share the llvm-cov report: their tests run in
# CPython / on the .NET runtime, so each language produces its own report
# under <out-dir>/bindings/<lang>/ and only the totals join the terminal
# summary. Every stage is best effort — a missing toolchain skips the
# language with a hint instead of failing the run.
# ---------------------------------------------------------------------------


class BindingResult:
    def __init__(
        self,
        name: str,
        percent: Optional[float],
        covered: int,
        count: int,
        note: str,
    ) -> None:
        self.name = name
        self.percent = percent
        self.covered = covered
        self.count = count
        self.note = note


def skipped(name: str, reason: str) -> BindingResult:
    status("Skipping", f"{name} bindings coverage: {reason}")
    return BindingResult(name, None, 0, 0, f"skipped ({reason})")


def python_coverage_cmd() -> Optional[List[str]]:
    """How to invoke coverage.py: as a module of the current interpreter, the
    pipx-installed CLI on PATH, or the repository's `.venv` interpreter."""
    probe = subprocess.run(
        [sys.executable, "-c", "import coverage"],
        capture_output=True,
    )
    if probe.returncode == 0:
        return [sys.executable, "-m", "coverage"]
    cli = shutil.which("coverage")
    if cli:
        return [cli]
    venv_python = PROJECT_ROOT / ".venv" / "bin" / "python"
    if venv_python.exists():
        probe = subprocess.run(
            [str(venv_python), "-c", "import coverage"],
            capture_output=True,
        )
        if probe.returncode == 0:
            return [str(venv_python), "-m", "coverage"]
    return None


def run_python_bindings(out_dir: Path) -> BindingResult:
    name = "python"
    cov = python_coverage_cmd()
    if cov is None:
        return skipped(name, "pipx install coverage (or into .venv)")
    lang_dir = PROJECT_ROOT / "crates" / "sdk" / "bindings" / "python"
    dest = out_dir / "bindings" / name
    dest.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ, COVERAGE_FILE=str(dest / ".coverage"))
    status("Running", f"{name} bindings tests under coverage.py")
    # The unittest suite drives the facade against a scripted transport
    # answering the real wire frames; the corpus scripts cover the codecs.
    runs = [
        ["run", "--source=mududb", "-m", "unittest", "discover", "tests"],
        ["run", "-a", "run_mp_corpus_test.py"],
        ["run", "-a", "run_syscall_corpus_test.py"],
        ["run", "-a", "run_lenient_decode_test.py"],
    ]
    for args in runs:
        proc = subprocess.run(
            cov + args,
            cwd=lang_dir,
            env=env,
            capture_output=True,
            text=True,
        )
        if proc.returncode != 0:
            tail = [
                line
                for line in (proc.stderr or proc.stdout).strip().splitlines()
                if line.strip()
            ][-3:]
            return BindingResult(name, None, 0, 0, "failed: " + " | ".join(tail))
    json_path = dest / "coverage.json"
    for args in (["json", "-o", str(json_path)], ["html", "-d", str(dest / "html")]):
        subprocess.run(
            cov + args,
            cwd=lang_dir,
            env=env,
            check=True,
            capture_output=True,
        )
    totals = json.loads(json_path.read_text(encoding="utf-8"))["totals"]
    return BindingResult(
        name,
        totals["percent_covered"],
        totals["covered_lines"],
        totals["num_statements"],
        str(dest / "html" / "index.html"),
    )


def run_csharp_bindings(out_dir: Path) -> BindingResult:
    name = "csharp"
    dotnet = shutil.which("dotnet")
    if dotnet is None:
        return skipped(name, "install the .NET SDK")
    probe = subprocess.run([dotnet, "--version"], capture_output=True)
    if probe.returncode != 0:
        # A bare runtime without an SDK answers --version with an error.
        return skipped(name, "no .NET SDK on this machine")
    lang_dir = PROJECT_ROOT / "crates" / "sdk" / "mudu_api" / "csharp"
    dest = out_dir / "bindings" / name
    dest.mkdir(parents=True, exist_ok=True)
    status("Running", f"{name} bindings tests under coverlet")
    proc = subprocess.run(
        [
            dotnet,
            "test",
            str(lang_dir / "Mudu.Api.Tests" / "Mudu.Api.Tests.csproj"),
            "--collect:XPlat Code Coverage",
            f"--results-directory:{dest}",
        ],
        cwd=lang_dir,
        capture_output=True,
        text=True,
    )
    reports = sorted(dest.glob("**/coverage.cobertura.xml"))
    if proc.returncode != 0 or not reports:
        tail = [
            line
            for line in (proc.stderr or proc.stdout).strip().splitlines()
            if line.strip()
        ][-3:]
        return BindingResult(name, None, 0, 0, "failed: " + " | ".join(tail))
    root = ET.parse(reports[-1]).getroot()
    percent = float(root.get("line-rate", "0")) * 100
    covered = int(root.get("lines-covered", "0"))
    count = int(root.get("lines-valid", "0"))
    return BindingResult(name, percent, covered, count, str(reports[-1]))


def run_bindings_coverage(out_dir: Path) -> List[BindingResult]:
    return [run_python_bindings(out_dir), run_csharp_bindings(out_dir)]


# ---------------------------------------------------------------------------
# Terminal summary
# ---------------------------------------------------------------------------


METRIC_ORDER = ("lines", "functions", "regions", "branches")

# Percent thresholds for summary coloring: green is healthy, yellow is
# warning territory, red needs attention.
PCT_GOOD = 80.0
PCT_WARN = 50.0

DETAIL_INDENT = " " * (STATUS_WIDTH + 1)


def group_summaries(report: dict) -> Dict[str, Dict[str, Dict[str, int]]]:
    """Aggregate per-file summaries into one summary per group workspace."""
    buckets: Dict[str, Dict[str, Dict[str, int]]] = {}
    for file_entry in report["data"][0].get("files", []):
        match = GROUP_DIR_REGEX.search(file_entry.get("filename", ""))
        if match:
            group = f"crates/{match.group(1)}" if match.group(1) else "example/mpk"
        else:
            group = "other"
        bucket = buckets.setdefault(group, {})
        file_summary = file_entry.get("summary", {})
        for key in METRIC_ORDER:
            metric = file_summary.get(key) or {}
            cell = bucket.setdefault(key, {"covered": 0, "count": 0})
            cell["covered"] += metric.get("covered", 0)
            cell["count"] += metric.get("count", 0)
    return buckets


def metric_cell(summary: dict, key: str) -> Tuple[str, Optional[float]]:
    metric = summary.get(key) or {}
    count = metric.get("count", 0)
    if count == 0:
        return "", None
    covered = metric.get("covered", 0)
    percent = covered / count * 100
    return f"{percent:.2f}% ({covered}/{count})", percent


def paint_percent(text: str, percent: Optional[float]) -> str:
    # Padding is applied before coloring so escape codes never break alignment.
    if percent is None:
        return text
    if percent >= PCT_GOOD:
        return good(text)
    if percent >= PCT_WARN:
        return bad(text)
    return fail(text)


def print_summary_table(report: dict, branch: bool) -> None:
    rows: List[Tuple[str, dict]] = [("total", report["data"][0]["totals"])]
    rows.extend(sorted(group_summaries(report).items()))
    keys = [key for key in METRIC_ORDER if branch or key != "branches"]
    keys = [
        key
        for key in keys
        if any(metric_cell(summary, key)[1] is not None for _, summary in rows)
    ]

    # Build the plain-text grid first so column widths never count ANSI codes.
    grid = [["", *keys]]
    percents: List[List[Optional[float]]] = []
    for name, summary in rows:
        cells = [metric_cell(summary, key) for key in keys]
        grid.append([name] + [text for text, _ in cells])
        percents.append([percent for _, percent in cells])
    widths = [max(len(line[i]) for line in grid) for i in range(len(grid[0]))]

    print()
    print(bold("Coverage summary (merged)"), flush=True)
    header = " " * (widths[0] + 4) + "  ".join(
        key.rjust(widths[i + 1]) for i, key in enumerate(keys)
    )
    print(dim(header))
    for (name, _), line, line_percents in zip(rows, grid[1:], percents):
        cells = [
            paint_percent(line[i + 1].rjust(widths[i + 1]), line_percents[i])
            for i in range(len(keys))
        ]
        padded = name.ljust(widths[0])
        label = bold(padded) if name == "total" else padded
        print(f"  {label}  {'  '.join(cells)}", flush=True)


def print_bindings_summary(results: List[BindingResult]) -> None:
    if not results:
        return
    print()
    print(bold("Bindings coverage (separate toolchains)"), flush=True)
    width = max(len(result.name) for result in results)
    for result in results:
        if result.percent is None:
            line = dim(result.note)
        else:
            text = f"{result.percent:.2f}% ({result.covered}/{result.count})"
            line = paint_percent(text, result.percent)
            if result.note:
                line += f"  {dim(result.note)}"
        print(f"  {result.name.ljust(width)}  {line}", flush=True)


def print_final_summary(args: argparse.Namespace, elapsed: float) -> None:
    out_dir = args.output_dir.resolve()
    print()
    status("Finished", f"coverage run in {format_duration(elapsed)}")
    print(f"{DETAIL_INDENT}branch coverage: {'enabled' if args.branch else 'disabled'}")
    if args.format in ("html", "all"):
        index = out_dir / "html" / "index.html"
        print(f"{DETAIL_INDENT}HTML report: {link(str(index), index.as_uri())}")
    if args.format in ("json", "all"):
        print(f"{DETAIL_INDENT}JSON summary: {highlight(out_dir / 'coverage.json')}")
    if args.format in ("lcov", "all"):
        print(f"{DETAIL_INDENT}LCOV file: {highlight(out_dir / 'coverage.lcov')}")
    sys.stdout.flush()


# ---------------------------------------------------------------------------


def main() -> None:
    args = parse_args()
    if args.branch:
        warn(
            "--branch is unstable with the pinned nightly's LLVM: llvm-cov "
            "can SIGSEGV at report time (getInstantiationGroups), failing the run."
        )
    started = time.monotonic()
    install_deps()
    toolchain = read_nightly_toolchain()
    clean_coverage_artifacts(toolchain, args.output_dir)
    args.output_dir.mkdir(parents=True, exist_ok=True)

    scopes: List[Tuple[str, List[str]]] = []
    for group in WS_GROUPS:
        run_unit_tests(toolchain, group, args.branch)
        scopes.append((group, unit_selector(group)))
    if args.e2e:
        wallet_snapshot = snapshot_wallet_mpk()
        try:
            prepare_wallet_mpk()
            run_e2e_tests(toolchain, args.branch, args.e2e_filter)
        finally:
            restore_wallet_mpk(wallet_snapshot)
        scopes.append(("crates/db-kernel", ["--package", "testing"]))

    status("Collecting", "instrumented test binaries")
    verify_group_coverage_data(scopes)
    objects = collect_objects(toolchain, scopes)
    merged = merge_profdata(llvm_tool_path(toolchain, "llvm-profdata"), args.output_dir)
    report = generate_reports(toolchain, objects, merged, args.output_dir, args.format)
    bindings = run_bindings_coverage(args.output_dir) if args.bindings else []
    print_summary_table(report, args.branch)
    print_bindings_summary(bindings)
    print_final_summary(args, time.monotonic() - started)


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, subprocess.CalledProcessError) as exc:
        error(str(exc))
        sys.exit(1)
