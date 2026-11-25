"""Checks for truthful cache baselines and normalized diagnostic accounting."""
import json
from pathlib import Path
import tempfile
import unittest

from paged_index_analysis import analyze_cache, diagnostic_report


class CacheAnalysisTests(unittest.TestCase):
    def row(self, capacity, reads, trial='0', backend='paged'):
        return dict(case=f'cache-read-f{capacity}', workload='read', backend=backend,
                    trial=trial, keys='4096', key_bytes='8', page_size='4096', frames=str(capacity),
                    seconds='0.001', ops_s='100000', p99_us='15', reads=str(reads),
                    evictions=str(reads), rows='100')

    def cache(self, row):
        return dict(case=row['case'], backend=row['backend'], trial=row['trial'], pages='80', operations='100')

    def test_baseline_requires_capacity_and_zero_loads_in_every_trial(self):
        rows = [self.row(128, 0), self.row(16, 200), self.row(256, 0), self.row(256, 1, '1')]
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)
            analyze_cache(rows, [self.cache(r) for r in rows], output)
            result = json.loads((output / 'cache_analysis.json').read_text())
        self.assertTrue(result[0]['resident'])
        self.assertFalse(result[2]['resident'])
        self.assertEqual(result[1]['baseline_case'], 'cache-read-f128')
        self.assertEqual(result[1]['reads_op'], 2.0)  # Not a probability.
        self.assertEqual(result[0]['extra_ns_op'], 0)

    def test_no_baseline_is_not_silently_inferred_from_zero_reads(self):
        row = self.row(16, 0)
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)
            analyze_cache([row], [self.cache(row)], output)
            result = json.loads((output / 'cache_analysis.json').read_text())[0]
            self.assertIsNone(result['relative_ns'])
            self.assertIn('No verified resident baseline', (output / 'cache_analysis.md').read_text())

    def test_work_statistics_use_exact_operations_and_actual_returned_rows(self):
        row = self.row(128, 0)
        row['rows'] = '3200'
        diag = dict(case=row['case'], backend='paged', trial='0', moved_bytes='6400', cow_bytes='0')
        alloc = dict(case=row['case'], backend='paged', trial='0', alloc_calls='3200', alloc_bytes='25600', realloc_calls='4')
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)
            diagnostic_report([row], [self.cache(row)], [diag], [alloc], output)
            r = json.loads((output / 'work_stats.json').read_text())[0]
        self.assertEqual(r['moved_bytes_per_op'], 64)
        self.assertEqual(r['alloc_bytes_per_row'], 8)
        self.assertEqual(r['alloc_calls_per_op'], 32)

    def test_baseline_page_counter_placeholders_are_not_reported_as_zero_work(self):
        row = self.row(128, 0, backend='baseline')
        diag = dict(case=row['case'], backend='baseline', trial='0', moved_bytes='0')
        alloc = dict(case=row['case'], backend='baseline', trial='0', alloc_calls='2')
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)
            diagnostic_report([row], [self.cache(row)], [diag], [alloc], output)
            r = json.loads((output / 'work_stats.json').read_text())[0]
        self.assertNotIn('moved_bytes_per_op', r)
        self.assertEqual(r['alloc_calls_per_op'], 0.02)


if __name__ == '__main__':
    unittest.main()
