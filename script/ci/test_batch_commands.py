"""Exercise orchestration paths without invoking expensive toolchains or Docker."""
import argparse
import contextlib
import importlib.util
import io
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'script/lib'))
sys.path.insert(0, str(ROOT / 'script/sdk'))
from task_report import Report


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, ROOT / path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class BatchCommandsTests(unittest.TestCase):
    def execute(self, module, options, failures=()):
        original = Report.run
        def fake(report, name, argv, cwd=ROOT, **kwargs):
            code = 1 if name in failures else 0
            return original(report, name, [sys.executable, '-c', f'print("diagnostic: {name}"); raise SystemExit({code})'],
                            ROOT, **kwargs)
        with tempfile.TemporaryDirectory() as directory, contextlib.redirect_stdout(io.StringIO()), \
                patch.object(sys, 'argv', ['runner', *options, '--log-dir', directory]), \
                patch.object(Report, 'run', fake):
            code = module.main()
            summary = json.loads(next(Path(directory).glob('*/summary.json')).read_text())
            return code, {r['name']: r for r in summary['steps']}

    def test_check_runs_every_workspace(self):
        module = load('checks', 'script/ci/check_all.py')
        first = 'crates/common: cargo clippy --workspace --all-targets -- -D warnings'
        code, steps = self.execute(module, ['--only', 'clippy'], [first])
        self.assertEqual(code, 1)
        self.assertEqual(len(steps), 5)
        self.assertEqual(list(steps.values())[-1]['status'], 'passed')

    def test_audit_runs_both_subcommands_after_udeps_failure(self):
        module = load('audit', 'script/ci/dep_audit.py')
        code, steps = self.execute(module, ['all'], ['udeps: common', 'outdated: tools'])
        self.assertEqual(code, 1)
        self.assertEqual(len(steps), 9)
        self.assertEqual(steps['outdated: tools']['status'], 'advisory')

    def test_benchmark_continues_and_blocks_missing_package(self):
        module = load('bench', 'script/bench/bench_gate.py')
        code, steps = self.execute(module, ['all'], ['paged-index', 'package-tpcc'])
        self.assertEqual(code, 1)
        self.assertEqual(steps['package-tpcc']['status'], 'failed')
        self.assertEqual(steps['tpcc']['status'], 'skipped')

    def test_examples_continue_languages(self):
        module = load('packages', 'script/ci/package_examples.py')
        code, steps = self.execute(module, [], ['as'])
        self.assertEqual(code, 1)
        self.assertEqual(steps['c']['status'], 'passed')
        self.assertEqual(len(steps), 7)

    def test_scenarios_continue_after_failure_and_cleanup(self):
        module = load('distributed', 'script/test/distributed_e2e.py')
        code, steps = self.execute(module, ['--scenario', 'raft,stream'], ['raft'])
        self.assertEqual(code, 1)
        self.assertEqual(steps['cleanup-raft']['status'], 'passed')
        self.assertEqual(steps['stream']['status'], 'passed')

    def test_cleanup_failure_blocks_next_scenario(self):
        module = load('distributed', 'script/test/distributed_e2e.py')
        code, steps = self.execute(module, ['--scenario', 'raft,stream'], ['cleanup-raft'])
        self.assertEqual(code, 1)
        self.assertEqual(steps['stream']['status'], 'skipped')

    def test_image_failure_blocks_scenarios(self):
        module = load('distributed', 'script/test/distributed_e2e.py')
        code, steps = self.execute(module, ['--scenario', 'raft,stream'], ['images'])
        self.assertEqual(code, 1)
        self.assertEqual(steps['raft']['status'], 'skipped')
        self.assertEqual(steps['stream']['status'], 'skipped')

    def test_sdk_failure_removes_stale_manifest_and_continues(self):
        module = load('sdk_build', 'script/sdk/sdk_build.py')
        with tempfile.TemporaryDirectory() as directory:
            manifest = Path(directory) / 'manifest.json'
            manifest.write_text('{}')
            code, steps = self.execute(module, ['--languages', 'as,py', '--output', directory], ['as'])
            self.assertEqual(code, 1)
            self.assertEqual(steps['py']['status'], 'passed')
            self.assertEqual(steps['distribution-manifest']['status'], 'skipped')
            self.assertFalse(manifest.exists())


if __name__ == '__main__':
    unittest.main()
