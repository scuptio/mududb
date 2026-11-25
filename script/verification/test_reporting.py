"""Exercise visible diagnostics, isolation and progress without a Lean installation."""
import contextlib
import io
import json
from pathlib import Path
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import check
import reporting
import toolchain


class ReportingTests(unittest.TestCase):
    def report(self, cache, action="check", total=1):
        return reporting.RunReport(Path(cache), action, total, "test::buf", {"Lean": "test"})

    def test_first_error_survives_long_log_and_color(self):
        text = "setup\n\x1b[31merror: proof.lean:7:3: unsolved goals\x1b[0m\n" + "context\n" * 60
        result = reporting.first_error(text)
        self.assertTrue(result.startswith("error: proof.lean:7:3:"))
        self.assertNotIn("\x1b", result)
        self.assertLessEqual(len(result.splitlines()), 12)
        self.assertIn("error[E0425]", reporting.first_error("error[E0425]: no symbol\n --> lib.rs:3:4\n"))

    def test_run_directories_and_command_logs_never_overwrite(self):
        with tempfile.TemporaryDirectory() as tmp, contextlib.redirect_stdout(io.StringIO()):
            first = self.report(tmp)
            second = self.report(tmp)
            self.assertNotEqual(first.directory, second.directory)
            old = first.log_path("lean-build")
            old.write_text("original proof")
            new = first.log_path("lean-build")
            new.write_text("mutant proof")
            self.assertNotEqual(old, new)
            self.assertEqual(old.read_text(), "original proof")

    def test_stage_heartbeat_and_summary_are_plain_text(self):
        stream = io.StringIO()
        with tempfile.TemporaryDirectory() as tmp, contextlib.redirect_stdout(stream), \
             patch.object(reporting, "HEARTBEAT_SECONDS", 0.01), \
             patch.object(reporting.output, "_enabled", False):
            report = self.report(tmp)
            with report.stage("proof build"):
                time.sleep(0.05)
            report.finish(True)
            summary = json.loads((report.directory / "summary.json").read_text())
            self.assertEqual(summary["stages"][0]["status"], "PASS")
            self.assertGreater(summary["stages"][0]["seconds"], 0)
            self.assertIn("Still running: proof build", stream.getvalue())
            self.assertIn("[1/1]", stream.getvalue())
            self.assertNotIn("\x1b", stream.getvalue())
            self.assertIn("Still running", (report.directory / "output.log").read_text())

    def test_command_preserves_full_argv_and_first_diagnostic(self):
        stream = io.StringIO()
        with tempfile.TemporaryDirectory() as tmp, contextlib.redirect_stdout(stream):
            report = self.report(tmp)
            command = [sys.executable, "-c", "print('error: proof.lean:2:4: bad goal'); print('tail\\n' * 50); raise SystemExit(2)"]
            with patch.object(toolchain, "REPORT", report):
                with self.assertRaises(reporting.CommandError) as caught:
                    with report.stage("Lean proof build"):
                        toolchain.run(command, cwd=Path(tmp), log="lean-build")
            error = caught.exception
            self.assertEqual(error.code, 2)
            self.assertIn("proof.lean:2:4", error.diagnostic)
            self.assertIn("Next:", str(error))
            self.assertIn(command[0], stream.getvalue())
            self.assertIn("raise SystemExit(2)", stream.getvalue())
            self.assertIn("raise SystemExit(2)", error.path.read_text())
            report.finish(False, str(error))
            self.assertEqual(json.loads((report.directory / "summary.json").read_text())["status"], "FAIL")

    def test_missing_tools_fail_in_stage_one_with_preserved_summary(self):
        with tempfile.TemporaryDirectory() as tmp, contextlib.redirect_stdout(io.StringIO()), \
             patch.object(toolchain, "CACHE", Path(tmp)), \
             patch.object(toolchain, "probe", return_value=(False, "missing compiler")), \
             patch.object(sys, "argv", ["check.py", "check"]):
            self.assertEqual(check.main(), 1)
            summaries = list(Path(tmp).glob("runs/*/summary.json"))
            self.assertEqual(len(summaries), 1)
            summary = json.loads(summaries[0].read_text())
            self.assertEqual(summary["stages"][0]["status"], "FAIL")
            self.assertEqual(summary["audited_theorems"], 0)
            self.assertIn("cargo make doctor", summary["error"])
            self.assertIsNone(toolchain.REPORT)

    def test_extract_summary_does_not_claim_proofs_were_checked(self):
        stream = io.StringIO()
        with tempfile.TemporaryDirectory() as tmp, contextlib.redirect_stdout(stream):
            report = self.report(tmp, action="extract")
            with report.stage("update generated Lean"):
                pass
            report.finish(True)
            self.assertIn("Proofs have not been checked", stream.getvalue())
            self.assertIn("0 audited theorems", stream.getvalue())


if __name__ == "__main__":
    unittest.main()
