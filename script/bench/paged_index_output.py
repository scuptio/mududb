"""Human-readable benchmark progress and comparisons, separate from CSV data."""
import json
import sys

from output import status


def print_trial(row, repeats, completed):
    """Show each completed measurement without exposing the wire format."""
    status("Measured", f"[{completed}] {row['case']} / {row['backend']} "
           f"trial {int(row['trial']) + 1}/{repeats}: "
           f"{float(row['ops_s']):,.0f} ops/s; p99 {float(row['p99_us']):.2f} us")
    # Flush even when redirected so long-running jobs expose progress promptly.
    sys.stdout.flush()


def print_summary(directory):
    """Compare medians; missing backends remain explicit rather than zero-valued."""
    cases = {}
    for row in json.loads((directory / "summary.json").read_text()):
        cases.setdefault(row["case"], {})[row["backend"]] = row
    width = max(4, *(len(case) for case in cases))
    print(f"\n  {'Case':<{width}}  {'Baseline ops/s':>14}  {'Paged ops/s':>14}  "
          f"{'Paged/baseline':>12}  {'Baseline p99 us':>14}  {'Paged p99 us':>14}")
    for case, pair in cases.items():
        old, new = pair.get("baseline"), pair.get("paged")
        ratio = f"{new['ops_s'] / old['ops_s']:.3f}x" if old and new and old['ops_s'] else "-"
        rates = [f"{row['ops_s']:,.0f}" if row else "-" for row in (old, new)]
        latency = [f"{row['p99_us']:.2f}" if row else "-" for row in (old, new)]
        print(f"  {case:<{width}}  {rates[0]:>14}  {rates[1]:>14}  {ratio:>12}  "
              f"{latency[0]:>14}  {latency[1]:>14}")
    print("  Medians across trials; p99 is the median of per-trial percentiles.\n")
