"""Regression tests for strict per-file coverage accounting."""

import unittest

from check_file_rates import METRICS, audit, passes, source_path


def rates(covered, count=100):
    return {metric: {"covered": covered, "count": count} for metric in METRICS}


class CoverageAuditTests(unittest.TestCase):
    def test_requires_strictly_more_than_eighty_in_every_metric(self):
        self.assertFalse(passes(rates(80), 80))
        self.assertTrue(passes(rates(81), 80))
        for metric in METRICS:
            mixed = rates(100)
            mixed[metric] = {"covered": 4, "count": 5}
            self.assertFalse(passes(mixed, 80))

    def test_uses_counts_without_rounding_and_accepts_empty_metrics(self):
        self.assertTrue(passes(rates(80001, 100000), 80))
        self.assertTrue(passes(rates(0, 0), 80))

    def test_missing_baseline_files_are_not_improvements(self):
        baseline = {"old.rs": rates(40), "improved.rs": rates(80)}
        current = {"improved.rs": rates(81), "new.rs": rates(0)}
        rows = audit(current, baseline, 80)
        self.assertEqual({row["path"]: row["status"] for row in rows}, {
            "old.rs": "missing", "improved.rs": "passed", "new.rs": "pending",
        })

    def test_normalizes_checkout_roots(self):
        self.assertEqual(source_path("/tmp/a/crates/common/a.rs"), "crates/common/a.rs")
        self.assertEqual(source_path("example/mpk/a.rs"), "example/mpk/a.rs")


if __name__ == "__main__":
    unittest.main()
