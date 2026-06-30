#!/usr/bin/env python3
"""Summarize cargo-llvm-cov JSON output as a GitHub Markdown table."""

import json
import sys


def summarize(path: str) -> None:
    with open(path, encoding="utf-8") as f:
        report = json.load(f)

    # cargo-llvm-cov --json produces a list of data entries; use the first one.
    data_entries = report.get("data", [])
    if not data_entries:
        print("No coverage data found.")
        return

    totals = data_entries[0].get("totals", {})

    print("| Metric | Percent |")
    print("|---|---|")
    for key in ("lines", "functions", "regions"):
        value = totals.get(key, {})
        percent = value.get("percent")
        if percent is not None:
            print(f"| {key} | {percent:.2f}% |")
        else:
            print(f"| {key} | N/A |")


if __name__ == "__main__":
    if len(sys.argv) != 2:
        print(f"Usage: {sys.argv[0]} <coverage.json>", file=sys.stderr)
        sys.exit(1)
    summarize(sys.argv[1])
