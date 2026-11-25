"""Regression tests for independent work, dependency failures and persistent logs."""
import argparse
import contextlib
import io
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'lib'))
from task_report import Report


class TaskReportTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.out = contextlib.redirect_stdout(io.StringIO())
        self.out.__enter__()
        self.addCleanup(self.out.__exit__, None, None, None)

    def report(self, fail_fast=False):
        return Report(argparse.Namespace(log_dir=Path(self.tmp.name), color='never', fail_fast=fail_fast), 'test')

    def command(self, code):
        return [sys.executable, '-c', code]

    def test_failure_continues_and_blocks_only_dependents(self):
        report = self.report()
        report.run('bad', self.command("import sys; print('failure reason', file=sys.stderr); sys.exit(7)"))
        report.run('dependent', self.command("raise Exception('must not run')"), needs=['bad'])
        report.run('good', self.command("print('success')"))
        self.assertEqual(report.finish(), 1)
        summary = json.loads((report.directory / 'summary.json').read_text())
        self.assertEqual([r['status'] for r in summary['steps']], ['failed', 'skipped', 'passed'])
        self.assertIn('bad', summary['steps'][1]['reason'])
        self.assertIn('failure reason', (report.directory / 'failures.log').read_text())
        self.assertNotIn('success', (report.directory / 'failures.log').read_text())

    def test_advisory_is_visible_but_nonblocking(self):
        report = self.report(True)
        report.run('outdated', self.command('raise SystemExit(1)'), advisory=True)
        report.run('next', self.command('pass'))
        self.assertEqual(report.finish(), 0)
        self.assertEqual(report.results[0]['status'], 'advisory')

    def test_fail_fast(self):
        report = self.report(True)
        report.run('bad', self.command('raise SystemExit(1)'))
        report.run('next', self.command('pass'))
        self.assertEqual(report.results[-1]['status'], 'skipped')
        self.assertEqual(report.finish(), 1)

    def test_missing_executable_continues(self):
        report = self.report()
        report.run('missing', [str(Path(self.tmp.name) / 'missing')])
        report.run('next', self.command('pass'))
        self.assertEqual(report.results[0]['exit_code'], 127)
        self.assertEqual(report.results[1]['status'], 'passed')

    def test_full_plain_failure_logs(self):
        report = self.report()
        report.run('bad', self.command("print('\\x1b[31mfirst\\x1b[0m'); print('x' * 100000); print('last'); raise SystemExit(1)"))
        text = (report.directory / 'failures.log').read_text()
        # The command header contains escaped sequences; actual ANSI must not survive.
        self.assertNotIn('\x1b', text)
        self.assertIn('x' * 100000, text)
        self.assertIn('last', text)

    def test_interrupt_is_reported_and_stops_remaining_work(self):
        report = self.report()
        with patch('task_report.subprocess.Popen', side_effect=KeyboardInterrupt):
            report.run('interrupted', ['unused'])
        report.run('next', self.command('pass'))
        self.assertEqual(report.finish(), 130)
        self.assertEqual(report.results[1]['status'], 'skipped')


if __name__ == '__main__':
    unittest.main()
