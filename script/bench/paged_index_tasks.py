"""Single-thread task concurrency reporting; never label task counts as threads."""
import json
import statistics
from paged_index_analysis import write_records


def report_tasks(rows, output):
    """Keep backend ratios and local scheduling effects separate from CPU scaling."""
    groups = {}
    for row in rows:
        groups.setdefault((row['case'], row['backend']), []).append(row)
    result = []
    for trials in groups.values():
        row = trials[0]
        item = {key: row[key] for key in ('case', 'backend', 'workload', 'tasks')}
        for metric in ('ops_s', 'p99_us'):
            item[metric] = statistics.median(float(t[metric]) for t in trials)
        rates = [float(t['ops_s']) for t in trials]
        item.update(min_ops_s=min(rates), max_ops_s=max(rates))
        result.append(item)
    by_case = {(r['case'], r['backend']): r for r in result}
    for row in result:
        baseline = by_case.get((row['case'], 'baseline'))
        row['vs_baseline'] = row['ops_s'] / baseline['ops_s'] if baseline else None
    (output / 'task_analysis.json').write_text(json.dumps(result, indent=2) + '\n')
    write_records(output, 'task_analysis.csv', list(result[0]), result)
    with (output / 'summary.md').open('a') as file:
        file.write('\n## Single-thread task concurrency\n\n'
                   'One partition owns both indexes in separate matched trials. All tasks execute on one OS thread; no tree is shared across threads. '
                   'Rates are aggregate operations/s, not per-task rates. More tasks do not add CPU cores. '
                   'Each task receives --ops operations (delete is capped by keys/tasks); increasing tasks increases total work. '
                   'Latency includes waits inside an operation, but not time before the task starts that operation. '
                   'Use min–max trial ranges and p99 as well as throughput.\n')


def plot_tasks(plt, rows, directory, save_plot):
    """Plot aggregate throughput and latency against local task count."""
    workloads = list(dict.fromkeys(r['workload'] for r in rows))
    columns = min(4, len(workloads))
    bands = (len(workloads) + columns - 1) // columns
    fig, axes = plt.subplots(2 * bands, columns, figsize=(4 * columns, 7 * bands), squeeze=False, layout='constrained')
    for index, workload in enumerate(workloads):
        col, top = index % columns, 2 * (index // columns)
        for backend, color in [('baseline', '#4267a6'), ('paged', '#d77825')]:
            points = sorted((r for r in rows if r['workload'] == workload and r['backend'] == backend), key=lambda r: int(r['tasks']))
            if not points:
                continue
            x = [int(r['tasks']) for r in points]
            rate = [r['ops_s'] for r in points]
            errors = [[r['ops_s'] - r['min_ops_s'] for r in points], [r['max_ops_s'] - r['ops_s'] for r in points]]
            axes[top, col].errorbar(x, rate, yerr=errors, fmt='o-', capsize=3, label=backend, color=color)
            axes[top + 1, col].plot(x, [r['p99_us'] for r in points], 'o-', label=backend, color=color)
        axes[top, col].set(title=workload, ylabel='Aggregate operations/s')
        axes[top + 1, col].set(ylabel='p99 microseconds', xlabel='Local tasks (one thread)')
        for ax in axes[top:top + 2, col]:
            ax.grid(alpha=0.2)
            ax.legend(fontsize=8)
    for index in range(len(workloads), bands * columns):
        for row in (2 * (index // columns), 2 * (index // columns) + 1):
            axes[row, index % columns].set_visible(False)
    save_plot(plt, fig, directory)
