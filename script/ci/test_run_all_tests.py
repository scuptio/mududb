"""Regression coverage for continue-on-failure and durable test reports."""
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from contextlib import redirect_stdout, redirect_stderr
from unittest.mock import patch

import run_all_tests as runner


class RunnerTests(unittest.TestCase):
    def run_runner(self, codes, arguments=(), spawn_error=None, output_suffix=""):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            groups = {"first": "first", "second": "second"}
            for path in groups.values():
                (root / path).mkdir()
            commands = []
            real_popen = subprocess.Popen

            def popen(command, **kwargs):
                commands.append(command)
                if spawn_error:
                    raise spawn_error
                code = codes[Path(kwargs["cwd"]).name]
                output = (
                    'Running tests/sample.rs (target/debug/deps/sample)\nfailures:\n\n---- broken_test stdout ----\n'
                    "thread 'broken_test' panicked at sample.rs:9:\n"
                    'assertion failed: expected primary\n'
                    'test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out\n'
                    'error: test failed, to rerun pass --test sample\n'
                ) if code else 'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n'
                output += output_suffix
                source = f"import sys; print({output!r}); sys.exit({code})"
                return real_popen([sys.executable, "-c", source], **kwargs)

            with patch.object(runner, "REPO_ROOT", root), \
                 patch.object(runner, "GROUPS", groups), \
                 patch.object(runner, "normalized_argv", return_value=list(arguments)), \
                 patch.object(runner.subprocess, "Popen", side_effect=popen), \
                 redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()):
                code = runner.main()
            runs = list((root / "logs" / "tests").iterdir())
            self.assertEqual(len(runs), 1)
            report = json.loads((runs[0] / "summary.json").read_text())
            text = (runs[0] / "summary.txt").read_text()
            logs = {path.stem: path.read_text() for path in runs[0].glob("*.log")}
            return code, commands, report, text, logs

    def test_failure_continues_and_preserves_reason(self):
        code, commands, report, text, logs = self.run_runner({"first": 101, "second": 0})
        self.assertEqual(code, 1)
        self.assertEqual([r["status"] for r in report["groups"]], ["failed", "passed"])
        self.assertEqual(report["not_run"], [])
        self.assertEqual(report["totals"]["tests_failed"], 1)
        self.assertEqual(report["totals"]["tests_passed"], 1)
        self.assertIn("sample.rs:9", logs["failures"])
        self.assertEqual(report["groups"][0]["failed_tests"], ["broken_test"])
        self.assertTrue(all("--no-fail-fast" in command for command in commands))
        self.assertIn("assertion failed: expected primary", text)
        self.assertIn("sample.rs:9", logs["first"])
        self.assertIn("test result: ok", logs["second"])

    def test_all_failed_groups_are_counted_and_logged(self):
        code, _, report, _, logs = self.run_runner({"first": 101, "second": 101})
        self.assertEqual(code, 1)
        self.assertEqual(report["totals"]["workspaces_failed"], 2)
        self.assertEqual(report["totals"]["tests_failed"], 2)
        self.assertEqual(report["totals"]["test_targets_failed"], 2)
        self.assertEqual(logs["failures"].count("assertion failed: expected primary"), 2)
        self.assertIn("=== first (exit 101) ===", logs["failures"])
        self.assertIn("=== second (exit 101) ===", logs["failures"])

    def test_fail_fast_stops_and_reports_unrun_groups(self):
        code, commands, report, _, logs = self.run_runner(
            {"first": 101, "second": 0}, ["--fail-fast"],
        )
        self.assertEqual(code, 1)
        self.assertEqual(len(commands), 1)
        self.assertNotIn("--no-fail-fast", commands[0])
        self.assertEqual(report["not_run"], ["second"])
        self.assertNotIn("second", logs)

    def test_success_and_single_group(self):
        code, commands, report, _, _ = self.run_runner(
            {"first": 0, "second": 0}, ["--group", "second"],
        )
        self.assertEqual(code, 0)
        self.assertEqual(len(commands), 1)
        self.assertEqual(report["groups"][0]["group"], "second")
        self.assertEqual(report["not_run"], [])

    def test_spawn_failure_is_reported_and_does_not_stop_other_groups(self):
        code, commands, report, text, _ = self.run_runner({}, spawn_error=FileNotFoundError("cargo missing"))
        self.assertEqual(code, 1)
        self.assertEqual(len(commands), 2)
        self.assertTrue(all(r["exit_code"] == 127 for r in report["groups"]))
        self.assertIn("cargo missing", text)
        self.assertEqual(report["failures_without_test_count"], ["first", "second"])

    def test_interrupt_stops_and_retains_partial_report(self):
        code, commands, report, text, _ = self.run_runner({}, spawn_error=KeyboardInterrupt())
        self.assertEqual(code, 130)
        self.assertEqual(len(commands), 1)
        self.assertEqual(report["groups"][0]["status"], "interrupted")
        self.assertEqual(report["not_run"], ["second"])
        self.assertIn("interrupted by user", text)

    def test_same_test_name_in_different_targets_is_preserved(self):
        diagnostics = runner.Diagnostics()
        for target in ("one", "two"):
            diagnostics.consume(f"Running tests/{target}.rs")
            diagnostics.consume("---- same_name stdout ----")
            diagnostics.consume("test result: FAILED. 0 passed; 1 failed; 0 ignored;")
        self.assertEqual(len(diagnostics.test_failures), 2)
        self.assertEqual(diagnostics.test_counts["failed"], 2)

    def test_full_failure_log_is_not_truncated_with_summary(self):
        code, _, report, text, logs = self.run_runner(
            {"first": 101, "second": 0},
            output_suffix="Error: detail\n" * 150 + "Error: final diagnostic\n",
        )
        self.assertEqual(code, 1)
        self.assertTrue(report["groups"][0]["diagnostics_truncated"])
        self.assertIn("Excerpt truncated", text)
        self.assertIn("Error: final diagnostic", logs["failures"])

    def test_diagnostic_excerpt_is_bounded(self):
        diagnostics = runner.Diagnostics()
        diagnostics.consume("failures:")
        for _ in range(1000):
            diagnostics.consume("failure detail")
        self.assertEqual(len(diagnostics.excerpt), 120)
        self.assertTrue(diagnostics.truncated)


if __name__ == "__main__":
    unittest.main()
