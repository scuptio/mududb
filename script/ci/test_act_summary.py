"""Regression tests for script/ci/act_summary.py log parsing."""

import io
import sys
import tempfile
import unittest
from contextlib import redirect_stdout
from pathlib import Path

import act_summary


SAMPLE_LOG = """\
[Fmt/good       ] ⭐ Run Set up job
[Fmt/good       ] 🚀  Start image=ghcr.io/catthehacker/ubuntu:act-latest
[Fmt/good       ]   ✅  Success - Set up job
[Fmt/good       ] ⭐ Run Main echo hello
[Fmt/good       ]   | hello
[Fmt/good       ]   ✅  Success - Main echo hello [48.242718ms]
[Fmt/good       ] ⭐ Run Complete job
[Fmt/good       ]   ✅  Success - Complete job
[Fmt/good       ] 🏁  Job succeeded
[Fmt/bad        ] ⭐ Run Set up job
[Fmt/bad        ]   ✅  Success - Set up job
[Fmt/bad        ] ⭐ Run Main Boom
[Fmt/bad        ]   | compiling things
[Fmt/bad        ]   | error: something broke
[Fmt/bad        ]   ❌  Failure - Main Boom [47.31868ms]
[Fmt/bad        ] exitcode '1': failure
[Fmt/bad        ] ⭐ Run Complete job
[Fmt/bad        ]   ✅  Success - Complete job
[Fmt/bad        ] 🏁  Job failed
Error: Job 'bad' failed
"""

MATRIX_LOG = """\
[Build/Build - ubuntu-latest - crates/common-1] ⭐ Run Main Check (debug)
[Build/Build - ubuntu-latest - crates/common-1]   ✅  Success - Main Check (debug) [1m5.2s]
[Build/Build - ubuntu-latest - crates/common-1] 🏁  Job succeeded
[Build/Build - ubuntu-latest - crates/common-2] ⭐ Run Main Test (debug)
[Build/Build - ubuntu-latest - crates/common-2]   ❌  Failure - Main Test (debug) [12.331915572s]
[Build/Build - ubuntu-latest - crates/common-2] 🏁  Job failed
"""


def write_log(content):
    handle = tempfile.NamedTemporaryFile("w", suffix=".log", delete=False, encoding="utf-8")
    handle.write(content)
    handle.close()
    return handle.name


class ParseLogTests(unittest.TestCase):
    def test_success_and_failure_jobs(self):
        jobs = act_summary.parse_log(write_log(SAMPLE_LOG))
        self.assertEqual([j.status for j in jobs], ["succeeded", "failed"])
        bad = jobs[1]
        self.assertEqual(bad.failed_step.name, "Boom")
        self.assertEqual(bad.failed_step.stage, "Main")
        self.assertEqual(bad.failed_step.duration, "47.31868ms")
        excerpt = act_summary.step_excerpt(bad, bad.failed_step)
        self.assertTrue(any("error: something broke" in line for line in excerpt))

    def test_matrix_label_and_durations(self):
        jobs = act_summary.parse_log(write_log(MATRIX_LOG))
        self.assertEqual(
            act_summary.short_label(jobs[0].label),
            "Build - ubuntu-latest - crates/common-1",
        )
        self.assertAlmostEqual(jobs[0].duration_secs, 65.2, places=3)
        self.assertEqual(jobs[1].status, "failed")

    def test_aborted_job_has_no_result(self):
        log = "[W/j] ⭐ Run Main step\n[W/j]   | partial\n"
        (job,) = act_summary.parse_log(write_log(log))
        self.assertEqual(job.status, "aborted")
        self.assertIsNone(job.failed_step)

    def test_go_duration(self):
        self.assertAlmostEqual(act_summary.parse_go_duration("48.242718ms"), 0.048242718)
        self.assertAlmostEqual(act_summary.parse_go_duration("1m5.2s"), 65.2)
        self.assertAlmostEqual(act_summary.parse_go_duration("2h3m"), 7380.0)


class MainTests(unittest.TestCase):
    def run_main(self, *items):
        buffer = io.StringIO()
        with redirect_stdout(buffer):
            code = act_summary.main(list(items))
        return code, buffer.getvalue()

    def test_summary_output(self):
        path = write_log(SAMPLE_LOG)
        code, text = self.run_main(f"sample={path}")
        self.assertEqual(code, 0)
        self.assertIn("✅ 1 succeeded", text)
        self.assertIn("❌ 1 failed", text)
        self.assertIn("failed step: Main Boom [47.31868ms]", text)
        self.assertIn("result: FAILURE", text)

    def test_all_pass_output(self):
        log = "[W/j] ⭐ Run Main s\n[W/j]   ✅  Success - Main s [1s]\n[W/j] 🏁  Job succeeded\n"
        code, text = self.run_main(f"ok={write_log(log)}")
        self.assertIn("result: SUCCESS", text)
        self.assertIn("✅ 1 succeeded", text)

    def test_missing_log_is_reported_not_raised(self):
        code, text = self.run_main("gone=/nonexistent/act.log")
        self.assertEqual(code, 0)
        self.assertIn("log unreadable", text)


if __name__ == "__main__":
    unittest.main()
