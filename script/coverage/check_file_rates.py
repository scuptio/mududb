#!/usr/bin/env python3
"""List files that do not strictly exceed a coverage threshold in every metric.

Consumes llvm-cov export JSON without changing the coverage collection scope.
An optional baseline keeps disappeared files visible instead of counting them
as improvements. Outputs a machine-readable audit and a Markdown checklist.
"""

import argparse
import json
from pathlib import Path

METRICS = ("functions", "lines", "regions")


def source_path(filename):
    parts = Path(filename).parts
    for index, part in enumerate(parts):
        if part in ("crates", "example"):
            return "/".join(parts[index:])
    return filename


def read_rates(path):
    report = json.loads(path.read_text())
    return {
        source_path(file["filename"]): {
            metric: file["summary"][metric] for metric in METRICS
        }
        for data in report["data"]
        for file in data["files"]
    }


def passes(rates, threshold):
    return all(
        count["count"] == 0
        or count["covered"] * 100 > threshold * count["count"]
        for count in rates.values()
    )


def audit(current, baseline, threshold):
    targets = {name for name, rates in baseline.items() if not passes(rates, threshold)}
    targets.update(name for name, rates in current.items() if not passes(rates, threshold))
    rows = []
    for name in sorted(targets):
        rates = current.get(name)
        state = "missing" if rates is None else "passed" if passes(rates, threshold) else "pending"
        rows.append({"path": name, "status": state, "metrics": rates})
    return rows


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("report", type=Path)
    parser.add_argument("--baseline", type=Path, help="original llvm-cov export JSON")
    parser.add_argument("--threshold", type=float, default=80)
    parser.add_argument("--output-dir", type=Path, default=Path("target/coverage-audit"))
    args = parser.parse_args()
    if not 0 <= args.threshold < 100:
        parser.error("threshold must be at least 0 and less than 100")
    current = read_rates(args.report)
    baseline = read_rates(args.baseline) if args.baseline else current
    rows = audit(current, baseline, args.threshold)
    counts = {state: sum(row["status"] == state for row in rows)
              for state in ("passed", "pending", "missing")}
    args.output_dir.mkdir(parents=True, exist_ok=True)
    output = {"threshold_exclusive": args.threshold, "counts": counts, "files": rows}
    (args.output_dir / "audit.json").write_text(json.dumps(output, indent=2) + "\n")
    lines = [f"# Per-file coverage audit (> {args.threshold:g}%)", "",
             "Function, line and region coverage must each exceed the threshold.",
             "Zero-count metrics are not applicable; missing baseline files do not pass.", "",
             "| File | Status | Functions | Lines | Regions |",
             "|---|---|---:|---:|---:|"]
    for row in rows:
        metrics = row["metrics"]
        values = [f'{metrics[m]["covered"]}/{metrics[m]["count"]}' if metrics else "—"
                  for m in METRICS]
        lines.append("| " + " | ".join([row["path"], row["status"], *values]) + " |")
    (args.output_dir / "audit.md").write_text("\n".join(lines) + "\n")
    print(f"Coverage audit: {counts}; {args.output_dir / 'audit.md'}")
    return 1 if counts["pending"] or counts["missing"] else 0


if __name__ == "__main__":
    raise SystemExit(main())
