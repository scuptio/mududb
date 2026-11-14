#!/usr/bin/env python3
"""Plot a paged_index.py report (requires matplotlib; no benchmark is rerun)."""
import argparse
import json
import os
from pathlib import Path


def load_pyplot(directory):
    """Use a headless backend and a writable cache beside the report."""
    os.environ.setdefault("MPLCONFIGDIR", str(Path(directory).resolve() / ".matplotlib-cache"))
    import matplotlib
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
    return plt


def legend(ax, **kwargs):
    """Avoid empty legends when a selected backend has no data for an axis."""
    if ax.get_legend_handles_labels()[0]:
        ax.legend(**kwargs)


def selected_cases(plt, rows):
    """Plot every measured case/backend for small or single-case runs."""
    cases = list(dict.fromkeys(row["case"] for row in rows))
    backends = list(dict.fromkeys(row["backend"] for row in rows))
    data = {(row["case"], row["backend"]): row for row in rows}
    fig, axes = plt.subplots(1, 2, figsize=(max(9, len(cases) * 1.6), 4.5), layout="constrained")
    colors = {"baseline": "#4267a6", "paged": "#d77825"}
    width = 0.8 / len(backends)
    for ax, metric, title, unit in zip(axes, ["ops_s", "p99_us"],
                                      ["Median throughput (min–max whiskers)", "Median per-trial p99 latency"],
                                      ["Operations/s", "Microseconds"]):
        for i, backend in enumerate(backends):
            entries = [(j, data[(case, backend)]) for j, case in enumerate(cases) if (case, backend) in data]
            positions = [j + (i - (len(backends) - 1) / 2) * width for j, _ in entries]
            values = [row[metric] for _, row in entries]
            errors = None
            if metric == "ops_s":
                errors = [[row[metric] - row["min_ops_s"] for _, row in entries],
                          [row["max_ops_s"] - row[metric] for _, row in entries]]
            ax.bar(positions, values, width=width, label=backend, color=colors[backend], yerr=errors, capsize=3)
        ax.set_xticks(range(len(cases)), cases, rotation=20 if len(cases) > 1 else 0)
        ax.set(title=title, ylabel=unit)
        ax.ticklabel_format(axis="y", style="sci", scilimits=(-3, 5))
        ax.grid(axis="y", alpha=0.2)
        legend(ax)
    return fig


def cache_plot(plt, rows, directory):
    """Separate capacity sensitivity from backend comparison on identical data."""
    workloads = list(dict.fromkeys(r['workload'] for r in rows))
    fig, axes = plt.subplots(2, len(workloads), figsize=(5 * len(workloads), 8), squeeze=False, layout='constrained')
    for column, workload in enumerate(workloads):
        for backend, color in [('baseline', '#4267a6'), ('paged', '#d77825')]:
            points = sorted((r for r in rows if r['workload'] == workload and r['backend'] == backend), key=lambda r: int(r['frames']))
            if not points:
                continue
            axes[0, column].plot([int(r['frames']) for r in points], [r['ops_s'] for r in points], 'o-', label=backend, color=color)
            if backend == 'paged':
                full = [r for r in points if r['resident']]
                axes[0, column].scatter([int(r['frames']) for r in full], [r['ops_s'] for r in full], marker='s', s=90, facecolors='none', edgecolors='green', label='verified resident')
                axes[1, column].plot([int(r['frames']) for r in points], [r['reads_op'] for r in points], 'o-', label='backing reads/op')
                axes[1, column].plot([int(r['frames']) for r in points], [r['evictions_op'] for r in points], 's--', label='evictions/op')
        axes[0, column].set(title=workload, ylabel='Operations/s')
        axes[1, column].set(ylabel='Events / logical operation', xlabel='Frame budget (not RSS)')
        for row in range(2):
            axes[row, column].set_xscale('log', base=2)
            axes[row, column].grid(alpha=0.2)
            legend(axes[row, column], fontsize=8)
    save_plot(plt, fig, directory)


def plot_report(directory, pyplot=None):
    """Render existing summaries; never rerun benchmark operations."""
    directory = Path(directory)
    plt = pyplot if pyplot is not None else load_pyplot(directory)
    if (directory / 'task_analysis.json').exists():
        from paged_index_tasks import plot_tasks
        plot_tasks(plt, json.loads((directory / 'task_analysis.json').read_text()), directory, save_plot)
        return
    if (directory / 'cache_analysis.json').exists():
        cache_plot(plt, json.loads((directory / 'cache_analysis.json').read_text()), directory)
        return
    rows = json.loads((directory / "summary.json").read_text())
    if not rows:
        raise ValueError("summary.json contains no measurements")
    if len({row["case"] for row in rows}) <= 6:
        fig = selected_cases(plt, rows)
        save_plot(plt, fig, directory)
        return
    data = {(row["case"], row["backend"]): row for row in rows}
    fig, axes = plt.subplots(2, 2, figsize=(13, 9), layout="constrained")
    colors = {"baseline": "#4267a6", "paged": "#d77825"}
    ax = axes[0, 0]
    cases = ["read", "mixed90", "update", "insert", "delete", "scan32", "scan256"]
    cases = [case for case in cases if any((case, b) in data for b in colors)]
    for offset, backend in enumerate(colors):
        entries = [(i, data[(case, backend)]) for i, case in enumerate(cases) if (case, backend) in data]
        if not entries:
            continue
        values = [row["ops_s"] for _, row in entries]
        low = [row["ops_s"] - row["min_ops_s"] for _, row in entries]
        high = [row["max_ops_s"] - row["ops_s"] for _, row in entries]
        ax.bar([i + (offset - 0.5) * 0.38 for i, _ in entries], values,
               width=0.38, label=backend, color=colors[backend], yerr=[low, high], capsize=2)
    ax.set_xticks(range(len(cases)), cases, rotation=25)
    ax.set_yscale("log")
    ax.set_ylabel("Operations/s (log scale)")
    ax.set_title("Workloads: median throughput, min–max whiskers")
    legend(ax)

    ax = axes[0, 1]
    for backend in colors:
        for workload, style in [("read", "-"), ("mixed90", "--")]:
            points = [(tasks, data[(workload if tasks == 1 else f"{workload}-t{tasks}", backend)]["ops_s"] / 1000)
                      for tasks in [1, 2, 4] if (workload if tasks == 1 else f"{workload}-t{tasks}", backend) in data]
            if points:
                ax.plot(*zip(*points), style, marker="o", color=colors[backend], label=f"{backend} {workload}")
    ax.set(xticks=[1, 2, 4], xlabel="Local tasks (one thread)", ylabel="Thousand operations/s", title="Local task concurrency")
    legend(ax, fontsize=8)

    ax = axes[1, 0]
    for backend in colors:
        points = [(size, data[(case, backend)]["ops_s"] / 1000)
                  for size, case in [(8, "read"), (128, "key-bytes-128"), (2048, "key-bytes-2048")]
                  if (case, backend) in data]
        if points:
            ax.plot(*zip(*points), marker="o", color=colors[backend], label=backend)
    lexical = [(size, data[(f"lexical-{size}", "paged")]["ops_s"] / 1000)
               for size in [8, 2048] if (f"lexical-{size}", "paged") in data]
    if lexical:
        ax.plot(*zip(*lexical), "--", marker="s", color="#38834b", label="paged, explicit lexical")
    ax.set_xscale("log", base=2)
    ax.set_yscale("log")
    ax.set(xlabel="Key bytes", ylabel="Thousand reads/s (log scale)", title="Key size (dispersed suffixes)")
    legend(ax)

    ax = axes[1, 1]
    points = [(frames, data[(f"frames-{frames}", "paged")]) for frames in [16, 64, 256]
              if (f"frames-{frames}", "paged") in data]
    if points:
        ax.plot([p[0] for p in points], [p[1]["ops_s"] / 1000 for p in points], "o-", color=colors["paged"])
        for frames, row in points:
            ax.annotate(f'{row["reads"]:.0f} reads', (frames, row["ops_s"] / 1000), xytext=(0, 7), textcoords="offset points", ha="center", fontsize=8)
    ax.set_xscale("log", base=2)
    ax.set(xlabel="Frames × 4 KiB (not total memory)", ylabel="Thousand reads/s", title="Paged cache pressure: 8,192 keys, memory backing")
    for ax in axes.flat:
        if not ax.has_data():
            ax.text(0.5, 0.5, "No measurements for this axis", ha="center", va="center", transform=ax.transAxes)
            ax.set_axis_off()
            continue
        ax.grid(axis="y", alpha=0.2)
    save_plot(plt, fig, directory)


def save_plot(plt, fig, directory):
    """Persist both a shareable bitmap and a scalable figure."""
    fig.suptitle("MuduDB index microbenchmark — descriptive medians, closed-loop load")
    fig.savefig(directory / "comparison.svg")
    fig.savefig(directory / "comparison.png", dpi=160)
    plt.close(fig)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.ArgumentDefaultsHelpFormatter)
    parser.add_argument("directory", type=Path, help="benchmark directory containing summary.json")
    args = parser.parse_args(argv)
    plot_report(args.directory)
    print(f"Plots: {args.directory / 'comparison.png'} and {args.directory / 'comparison.svg'}")


if __name__ == "__main__":
    main()
