"""Test persistent act reports, raw-workflow reports and wrapper exit handling."""
import contextlib
import io
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys
import tempfile
import unittest

import act_summary

ROOT = Path(__file__).resolve().parents[2]
LABEL = 'Build/Build - ubuntu-latest - crates/db-kernel-2'


def sample():
    lines = ['⭐ Run Main Test (release)', '| first output',
             '| ---- stream_snapshot_gc_and_wiped_rejoin stdout ----',
             "| thread 'stream_snapshot_gc_and_wiped_rejoin' panicked:",
             '| snapshot resume requires an idle WAL', '| detail without error keyword',
             '| test result: FAILED. 1 passed; 1 failed;',
             '❌ Failure - Main Test (release) [13.64s]',
             '⭐ Run Post Cache Cargo Registry', '✅ Success - Post Cache Cargo Registry',
             '🏁 Job failed']
    return '\n'.join(f'[{LABEL}] {line}' for line in lines) + '\n'


class ReportingTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)

    def report(self, text, code=None):
        log = self.root / 'console.log'
        log.write_text(text)
        args = ['--report-dir', str(self.root / 'report'), f'build={log}']
        if code is not None:
            args += ['--exit-code', f'build={code}']
        with contextlib.redirect_stdout(io.StringIO()) as out:
            act_summary.main(args)
        return json.loads((self.root / 'report/summary.json').read_text()), out.getvalue()

    def test_matrix_step_test_and_durable_full_logs(self):
        summary, text = self.report(sample(), 1)
        failure = summary['jobs'][0]['failures'][0]
        self.assertIn('crates/db-kernel', text)
        self.assertEqual(failure['step'], 'Test (release)')
        self.assertEqual(failure['tests'], ['stream_snapshot_gc_and_wiped_rejoin'])
        self.assertIn('snapshot resume requires an idle WAL', text)
        self.assertIn('first output', Path(failure['log']).read_text())
        self.assertIn('detail without error keyword', Path(summary['failure_log']).read_text())
        self.assertIn(str(self.root / 'report/failures.log'), text)

    def test_ansi_prefix_is_parsed(self):
        colored = '\n'.join('\x1b[31m' + line + '\x1b[0m' for line in sample().splitlines())
        summary, text = self.report(colored)
        self.assertEqual(summary['totals']['failed'], 1)
        self.assertNotIn('\x1b', text)

    def test_engine_failure_without_job_is_not_success(self):
        summary, text = self.report('Error: failed to connect to Docker daemon\n', 1)
        self.assertEqual(summary['result'], 'FAILURE')
        self.assertIn('Docker daemon', text)
        self.assertIn('Docker daemon', Path(summary['failure_log']).read_text())

    def test_nonzero_exit_overrides_success_markers(self):
        summary, _ = self.report('[W/j] 🏁 Job succeeded\nError: engine failure\n', 1)
        self.assertEqual(summary['result'], 'FAILURE')

    def test_expected_error_in_success_is_not_failure(self):
        summary, _ = self.report('[W/j] ⭐ Run Main test\n[W/j] | ERROR expected failure\n[W/j] ✅ Success - Main test\n[W/j] 🏁 Job succeeded\n', 0)
        self.assertEqual(summary['result'], 'SUCCESS')
        self.assertEqual(Path(summary['failure_log']).read_text(), '')

    def test_multiple_failed_steps_and_nested_actions(self):
        text = ('[W/j] ⭐ Run Main outer\n[W/j] ⭐ Run Main inner\n'
                '[W/j] ❌ Failure - Main inner\n[W/j] ❌ Failure - Main outer\n'
                '[W/j] ⭐ Run Post cleanup\n[W/j] ❌ Failure - Post cleanup\n[W/j] 🏁 Job failed\n')
        summary, _ = self.report(text, 1)
        self.assertEqual([r['step'] for r in summary['jobs'][0]['failures']], ['outer', 'inner', 'cleanup'])

    def test_missing_log_cannot_report_success(self):
        with contextlib.redirect_stdout(io.StringIO()) as out:
            act_summary.main(['missing=/does/not/exist'])
        self.assertIn('result: FAILURE', out.getvalue())

    def test_wrapper_creates_unique_runs_and_preserves_failure(self):
        bin_dir = self.root / 'bin'
        bin_dir.mkdir()
        fake_python = bin_dir / 'python3'
        fake_python.write_text('#!/bin/sh\ncase "$1" in */act_preflight.py) exit 0;; esac\nexec ' + shlex.quote(sys.executable) + ' "$@"\n')
        fake_python.chmod(0o755)
        fake_act = bin_dir / 'act'
        fake_act.write_text('#!' + sys.executable + '\nimport sys\nprint(' + repr(sample()) + ')\nsys.exit(1)\n')
        fake_act.chmod(0o755)
        env = {**os.environ, 'PATH': str(bin_dir) + os.pathsep + os.environ['PATH']}
        logs = self.root / 'logs'
        for _ in range(2):
            run = subprocess.run(['bash', str(ROOT / 'script/ci/act.sh'), '--log-dir', str(logs), '-j', 'build'],
                                 env=env, text=True, capture_output=True)
            self.assertEqual(run.returncode, 1, run.stdout + run.stderr)
            self.assertIn('failed step: Main Test (release)', run.stdout)
            self.assertIn('Failure logs:', run.stdout)
        reports = list(logs.glob('*/summary.json'))
        self.assertEqual(len(reports), 2)
        self.assertTrue(all(json.loads(p.read_text())['result'] == 'FAILURE' for p in reports))


if __name__ == '__main__':
    unittest.main()
