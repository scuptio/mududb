#!/usr/bin/env python3
"""Run all workspace tests, preserving logs and reporting every failed group.

By default Cargo continues after failing test targets and the runner continues
with subsequent workspaces. Compilation failures can prevent tests in that
workspace from running. Use --fail-fast to stop at the first failed target/group.
"""
import argparse
from datetime import datetime, timezone
import json
import re
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from typing import Dict

REPO_ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO_ROOT / "script" / "lib"))
from cli import normalized_argv  # noqa: E402
from output import cmd as echo_cmd  # noqa: E402
from output import error, format_duration, set_enabled, status  # noqa: E402
import test_display  # noqa: E402

GROUPS: Dict[str, str] = {
    "common": "crates/common",
    "db-kernel": "crates/db-kernel",
    "sdk": "crates/sdk",
    "example-mpk": "example/mpk",
    "tools": "crates/tools",
    "mudu-api": "crates/sdk/mudu_api/rust",
}


class Diagnostics:
    """Keep a bounded excerpt; the group's log always retains the full output."""

    def __init__(self):
        self.failed_tests = set()
        self.test_failures = []
        self.current_target = "unknown"
        self.excerpt = []
        self.truncated = False
        self.in_failure = False
        self.context = 0
        self.test_counts = {"passed": 0, "failed": 0, "ignored": 0}
        self.failed_targets = 0

    def consume(self, line):
        text = line.rstrip()
        summary = re.match(
            r"test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;", text,
        )
        if summary:
            for key, value in zip(self.test_counts, summary.groups()):
                self.test_counts[key] += int(value)
            if text.startswith("test result: FAILED"):
                self.failed_targets += 1
        if text.lstrip().startswith(("Running ", "Doc-tests ")):
            self.current_target = text.strip()
        match = re.match(r"---- (.+) (?:stdout|stderr) ----", text)
        if match:
            self.failed_tests.add(match.group(1))
            failure = {"target": self.current_target, "test": match.group(1)}
            if failure not in self.test_failures:
                self.test_failures.append(failure)
        if text == "failures:":
            self.in_failure = True
        diagnostic = text.startswith(("error", "Error:", "Caused by:")) or "panicked at" in text
        if diagnostic:
            self.context = 8
        if self.in_failure or diagnostic or self.context:
            if len(self.excerpt) < 120:
                self.excerpt.append(text)
            else:
                self.truncated = True
        self.context = max(0, self.context - 1)
        if text.startswith("test result:"):
            self.in_failure = False


def run_group(group: str, log_dir: Path, fail_fast: bool = False) -> dict:
    path = GROUPS[group]
    command = ["cargo", "test", "--workspace", "--color", "never"]
    if not fail_fast:
        command.append("--no-fail-fast")
    command.extend(["--", "--quiet"])
    log_path = log_dir / f"{group}.log"
    echo_cmd(command)
    started = time.monotonic()
    diagnostics = Diagnostics()
    interrupted = False
    proc = None
    with log_path.open("w", encoding="utf-8") as log:
        log.write(f"cwd: {REPO_ROOT / path}\ncommand: {command!r}\n\n")
        log.flush()
        try:
            proc = subprocess.Popen(
                command, cwd=REPO_ROOT / path, stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT, text=True, encoding="utf-8", errors="replace",
            )
            with proc.stdout:
                for line in proc.stdout:
                    line = test_display.plain(line)
                    log.write(line)
                    log.flush()
                    sys.stdout.write(test_display.styled_line(line))
                    sys.stdout.flush()
                    diagnostics.consume(line)
            returncode = proc.wait()
        except OSError as exc:
            message = f"Error: could not execute workspace tests: {exc}\n"
            log.write(message)
            diagnostics.consume(message)
            returncode = 127
        except KeyboardInterrupt:
            interrupted = True
            if proc is not None:
                proc.terminate()
                try:
                    proc.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    proc.wait()
            log.write("\nInterrupted by user.\n")
            diagnostics.consume("Error: interrupted by user")
            returncode = 130
    elapsed = time.monotonic() - started
    outcome = "interrupted" if interrupted else "failed" if returncode else "passed"
    if returncode:
        error(f"{group}: {outcome} (exit {returncode}); log: {log_path}")
    else:
        status("Passed", f"{group} in {format_duration(elapsed)}")
    return {
        "group": group, "workspace": path, "command": command, "status": outcome,
        "exit_code": returncode, "duration_seconds": round(elapsed, 3),
        "log": str(log_path), "failed_tests": sorted(diagnostics.failed_tests),
        "tests": diagnostics.test_counts, "failed_test_targets": diagnostics.failed_targets,
        "test_failures": diagnostics.test_failures,
        "diagnostics": diagnostics.excerpt, "diagnostics_truncated": diagnostics.truncated,
    }


def write_report(log_dir, selected, results):
    completed = {result["group"] for result in results}
    not_run = [group for group in selected if group not in completed]
    failed = [result for result in results if result["status"] == "failed"]
    totals = {
        "workspaces_planned": len(selected), "workspaces_run": len(results),
        "workspaces_passed": sum(r["status"] == "passed" for r in results),
        "workspaces_failed": len(failed),
        "workspaces_interrupted": sum(r["status"] == "interrupted" for r in results),
        "test_targets_failed": sum(r["failed_test_targets"] for r in results),
        **{f"tests_{key}": sum(r["tests"][key] for r in results)
           for key in ("passed", "failed", "ignored")},
    }
    # Some failures (compile errors, process aborts, launch errors) have no
    # libtest summary. Report them separately rather than inventing a test count.
    uncounted = [r["group"] for r in results if r["exit_code"] and not r["tests"]["failed"]]
    report = {"totals": totals, "groups": results, "not_run": not_run,
              "failures_without_test_count": uncounted,
              "failure_log": str(log_dir / "failures.log")}
    (log_dir / "summary.json").write_text(
        json.dumps(report, indent=2, ensure_ascii=False) + "\n", encoding="utf-8",
    )
    lines = ["Workspace test report", ""]
    lines.extend(f"{key}: {value}" for key, value in totals.items())
    lines.extend([f"Full failure output: {log_dir / 'failures.log'}", ""])
    if uncounted:
        lines.append("Failures/interruption without a reported test count: " + ", ".join(uncounted))
    # Stream full output from failed groups into a single file. Do not cap this
    # file: the bounded excerpts below are only for the readable/JSON summaries.
    with (log_dir / "failures.log").open("w", encoding="utf-8") as combined:
        for result in results:
            if not result["exit_code"]:
                continue
            combined.write(f"\n=== {result['group']} (exit {result['exit_code']}) ===\n")
            combined.write(f"Original log: {result['log']}\n")
            with Path(result["log"]).open(encoding="utf-8") as source:
                shutil.copyfileobj(source, combined)
            combined.write("\n")
    for result in results:
        lines.extend([
            f"{result['group']}: {result['status']} (exit {result['exit_code']}, "
            f"{result['duration_seconds']}s)",
            f"Log: {result['log']}",
        ])
        if result["failed_tests"]:
            lines.append("Failed tests: " + ", ".join(result["failed_tests"]))
            lines.extend(f"  {item['target']}: {item['test']}" for item in result["test_failures"])
        lines.extend(result["diagnostics"] if result["status"] != "passed" else [])
        if result["diagnostics_truncated"]:
            lines.append("Excerpt truncated; see the complete group log.")
        lines.append("")
    if not_run:
        lines.append("Not run: " + ", ".join(not_run))
    (log_dir / "summary.txt").write_text("\n".join(lines) + "\n", encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--group", choices=sorted(GROUPS), help="Run one workspace.")
    parser.add_argument("--fail-fast", action="store_true", help="Stop after the first failure.")
    parser.add_argument("--color", choices=("auto", "always", "never"), default="auto",
                        help="Terminal colors (auto honors NO_COLOR and terminal support).")
    parser.add_argument(
        "--log-dir", type=Path, default=REPO_ROOT / "logs" / "tests",
        help="Parent directory for a unique run directory (default: logs/tests).",
    )
    args = parser.parse_args(normalized_argv())
    if args.color != "auto":
        set_enabled(args.color == "always")
    args.log_dir.mkdir(parents=True, exist_ok=True)
    stamp = datetime.now(timezone.utc).strftime("%Y%m%d-%H%M%S-")
    log_dir = Path(tempfile.mkdtemp(prefix=stamp, dir=args.log_dir)).resolve()
    selected = [args.group] if args.group else list(GROUPS)
    test_display.header(selected, log_dir, args.fail_fast)
    results = []
    started = time.monotonic()
    write_report(log_dir, selected, results)
    try:
        for index, group in enumerate(selected, 1):
            test_display.group_start(index, len(selected), group, GROUPS[group])
            result = run_group(group, log_dir, args.fail_fast)
            results.append(result)
            write_report(log_dir, selected, results)
            if result["status"] == "interrupted" or (args.fail_fast and result["exit_code"]):
                break
    except KeyboardInterrupt:
        write_report(log_dir, selected, results)
        test_display.summary(selected, results, log_dir, time.monotonic() - started)
        error(f"interrupted; report: {log_dir / 'summary.txt'}")
        return 130
    failed = [result["group"] for result in results if result["exit_code"]]
    test_display.summary(selected, results, log_dir, time.monotonic() - started)
    if failed:
        return 130 if any(r["status"] == "interrupted" for r in results) else 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
