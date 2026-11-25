"""Exercise the custom workflow shell and raw-act job reports without Docker."""
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys
import tempfile
import unittest

SCRIPT = Path(__file__).with_name('workflow_report.py').resolve()


class WorkflowReportTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix='workflow report ')
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.env = {**os.environ, 'GITHUB_WORKSPACE': str(self.root),
                    'GITHUB_JOB': 'workspace-invariants', 'GITHUB_WORKFLOW': 'CI Hardening',
                    'GITHUB_ENV': str(self.root / 'env'),
                    'GITHUB_STEP_SUMMARY': str(self.root / 'github-summary')}
        result = self.invoke('init')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.env.update(line.split('=', 1) for line in (self.root / 'env').read_text().splitlines())
        self.logs = Path(self.env['MUDU_JOB_LOG_DIR'])

    def invoke(self, *args):
        return subprocess.run([sys.executable, str(SCRIPT), *args], env=self.env,
                              cwd=self.root, capture_output=True, text=True)

    def capture(self, content, shell='bash', step='check'):
        script = self.root / 'step source'
        script.write_text(content)
        # Match the actual workflow's custom shell bootstrap, including spaces in paths.
        bootstrap = "import os,runpy; runpy.run_path(os.environ['MUDU_WORKFLOW_REPORT'],run_name='__main__')"
        return subprocess.run([sys.executable, '-c', bootstrap, 'run', step, shell, str(script)],
                              env=self.env, cwd=self.root, capture_output=True, text=True)

    def report(self, steps):
        self.env['REPORT_STEPS'] = json.dumps(steps)
        self.env['REPORT_MATRIX'] = json.dumps({'os': 'ubuntu-latest', 'group': 'crates/tools'})
        result = self.invoke('report')
        self.assertEqual(result.returncode, 0, result.stderr)
        return result.stdout, json.loads((self.logs / 'summary.json').read_text())

    def test_capture_preserves_failure_stderr_and_errexit(self):
        result = self.capture('echo before\necho "Error: fixture failure" >&2\nexit 17\necho unreachable\n')
        self.assertEqual(result.returncode, 17)
        self.assertIn('Error: fixture failure', result.stdout)
        self.assertNotIn('unreachable', result.stdout)
        self.assertEqual((self.logs / 'check.log').read_text(), result.stdout)

    def test_capture_preserves_pipefail(self):
        result = self.capture('false | cat\necho unreachable\n')
        self.assertEqual(result.returncode, 1)
        self.assertNotIn('unreachable', result.stdout)

    def test_default_shell_keeps_original_pipeline_behavior(self):
        result = self.capture('false | cat\necho reached\n', shell='default')
        self.assertEqual(result.returncode, 0)
        self.assertIn('reached', result.stdout)

    def test_python_shell_exit_status(self):
        result = self.capture('import sys\nprint("python failure", flush=True)\nsys.exit(9)\n', shell='python')
        self.assertEqual(result.returncode, 9)
        self.assertIn('python failure', result.stdout)

    def test_working_directory_and_output_files_are_preserved(self):
        self.env['GITHUB_OUTPUT'] = str(self.root / 'outputs')
        result = self.capture('pwd\necho "version=1" >> "$GITHUB_OUTPUT"\n')
        self.assertEqual(result.returncode, 0)
        self.assertIn(str(self.root), result.stdout)
        self.assertEqual((self.root / 'outputs').read_text(), 'version=1\n')

    def test_metrics_detail_before_error_is_reported(self):
        detail = 'server/control_plane_node.rs:92: function `start` is 101 lines (limit 100)'
        self.capture('printf "%s\\n" ' + shlex.quote(detail) + '\necho "mudu_metrics: 1 violation(s)"\necho "Error: code-metrics limits exceeded"\nexit 1\n')
        text, summary = self.report({'check': {'outcome': 'failure', 'conclusion': 'failure'}})
        self.assertIn(detail, text)
        self.assertIn('group=crates/tools', text)
        self.assertIn(detail, (self.logs / 'failures.log').read_text())
        self.assertEqual(summary['failures'][0]['step'], 'check')
        self.assertIn(detail, (self.root / 'github-summary').read_text())

    def test_outcomes_allowed_failures_missing_action_logs_and_stale_runs(self):
        self.capture('echo "ERROR expected"\n', step='passing')
        self.capture('echo "Error: advisory"\nexit 1\n', step='advisory')
        (self.logs / 'skipped.log').write_text('Error: OLD LOG\n')
        text, summary = self.report({
            'passing': {'outcome': 'success', 'conclusion': 'success'},
            'advisory': {'outcome': 'failure', 'conclusion': 'success'},
            'skipped': {'outcome': 'skipped', 'conclusion': 'skipped'},
            'setup': {'outcome': 'failure', 'conclusion': 'failure'},
        })
        self.assertIn('ALLOWED FAILURE step: advisory', text)
        self.assertIn('FAILED step: setup', text)
        self.assertNotIn('OLD LOG', text)
        self.assertNotIn('ERROR expected', text)
        self.assertEqual(len(summary['failures']), 2)
        self.assertIsNone(summary['failures'][1]['log'])
        self.assertEqual(self.invoke('init').returncode, 0)
        values = dict(line.split('=', 1) for line in (self.root / 'env').read_text().splitlines())
        self.assertNotEqual(values['MUDU_JOB_LOG_DIR'], str(self.logs))
        self.assertTrue((self.logs / 'summary.txt').is_file())


if __name__ == '__main__':
    unittest.main()
