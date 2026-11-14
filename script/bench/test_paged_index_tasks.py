"""Task count and baseline ratios must describe one-thread aggregate throughput."""
import json
from pathlib import Path
import tempfile
import unittest
from paged_index_tasks import report_tasks
from paged_index import parse_args


class LocalTaskReportTests(unittest.TestCase):
    def test_default_model_and_backend_names(self):
        args = parse_args([])
        self.assertEqual(args.suite, 'tasks')
        self.assertEqual(args.tasks, '1,4,16')
        self.assertEqual(parse_args(['--backend', 'baseline']).backend, 'baseline')

    def test_ratio_uses_matching_task_count_and_aggregate_rate(self):
        rows = []
        for tasks in (1, 4):
            for backend, rates in [('baseline', [100, 120, 110]), ('paged', [200, 240, 220])]:
                for rate in rates:
                    rows.append(dict(case=f'read-t{tasks}', backend=backend, workload='read',
                                     tasks=str(tasks), ops_s=str(rate), p99_us='1'))
        with tempfile.TemporaryDirectory() as directory:
            report_tasks(rows, Path(directory))
            result = json.loads((Path(directory) / 'task_analysis.json').read_text())
            report = (Path(directory) / 'summary.md').read_text()
        paged = [r for r in result if r['backend'] == 'paged']
        self.assertEqual([r['ops_s'] for r in paged], [220, 220])
        self.assertTrue(all(r['vs_baseline'] == 2 for r in paged))
        self.assertIn('one OS thread', report)
        self.assertIn('not per-task rates', report)


if __name__ == '__main__':
    unittest.main()
