#!/usr/bin/env python3
"""Capture workflow commands and report failed steps for both act and GitHub."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

sys.path.insert(0, str(Path(__file__).resolve().parent))

from act_summary import diagnostics, failed_tests, plain, safe_name


def initialize():
    """Allocate fresh logs, including when act reuses an old container."""
    base = Path(os.environ['GITHUB_WORKSPACE']) / 'logs' / 'ci' / safe_name(os.environ['GITHUB_JOB'])
    base.mkdir(parents=True, exist_ok=True)
    folder = tempfile.mkdtemp(prefix='run-', dir=base)
    with open(os.environ['GITHUB_ENV'], 'a', encoding='utf-8') as env:
        env.write(f'MUDU_JOB_LOG_DIR={folder}\n')
        env.write(f'MUDU_WORKFLOW_REPORT={Path(__file__).resolve()}\n')
    print(f'Runner/container step logs: {folder}', flush=True)


def capture(step, shell, script):
    """Keep the original shell's error behavior and command exit status."""
    commands = {
        'bash': ['bash', '--noprofile', '--norc', '-e', '-o', 'pipefail'],
        'default': ['bash', '-e'],
        'python': ['python'],
    }
    folder = Path(os.environ['MUDU_JOB_LOG_DIR'])
    with (folder / f'{safe_name(step)}.log').open('wb') as log:
        with subprocess.Popen(commands[shell] + [script], stdout=subprocess.PIPE,
                              stderr=subprocess.STDOUT) as child:
            while chunk := os.read(child.stdout.fileno(), 65536):
                log.write(chunk)
                log.flush()
                sys.stdout.buffer.write(chunk)
                sys.stdout.buffer.flush()
            code = child.wait()
    return code if code >= 0 else 128 - code


def report():
    """Use step outcomes, never ERROR keywords, to determine failures."""
    steps = json.loads(os.environ['REPORT_STEPS'])
    folder = Path(os.environ['MUDU_JOB_LOG_DIR'])
    matrix = json.loads(os.environ.get('REPORT_MATRIX', 'null'))
    title = f"{os.environ['GITHUB_WORKFLOW']} / {os.environ['GITHUB_JOB']}"
    if matrix:
        title += ' / ' + ', '.join(f'{key}={value}' for key, value in matrix.items())
    lines = [f'Job report: {title}']
    records = []
    with (folder / 'failures.log').open('w', encoding='utf-8') as failures:
        for key, result in steps.items():
            if result.get('outcome') != 'failure':
                continue
            allowed = result.get('conclusion') == 'success'
            path = folder / f'{safe_name(key)}.log'
            content = plain(path.read_text(encoding='utf-8', errors='replace')) if path.is_file() else ''
            excerpt = diagnostics(content.splitlines())
            tests = failed_tests(content.splitlines())
            lines.append(f"{'ALLOWED FAILURE' if allowed else 'FAILED'} step: {key}")
            if tests:
                lines.append('Failed tests: ' + ', '.join(tests))
            lines.extend('  ' + line for line in excerpt)
            lines.append(f'Runner/container log: {path}' if path.is_file() else
                         'No captured command log (action/setup failure); see this step in the console output.')
            failures.write(f'===== {key} =====\n{content}\n')
            records.append(dict(step=key, allowed=allowed, diagnostics=excerpt, tests=tests,
                                log=str(path) if path.is_file() else None))
    blocking = sum(not record['allowed'] for record in records)
    lines.append(f'Failed steps: {blocking}; allowed failures: {len(records) - blocking}')
    lines.append(f'Runner/container summary: {folder / "summary.txt"}')
    lines.append(f'Runner/container failure log: {folder / "failures.log"}')
    lines.append('Host logs and cross-workflow summaries: use script/ci/act.sh.')
    text = '\n'.join(lines) + '\n'
    (folder / 'summary.txt').write_text(text, encoding='utf-8')
    (folder / 'summary.json').write_text(json.dumps(dict(job=title, matrix=matrix, failures=records), indent=2) + '\n', encoding='utf-8')
    print(text, flush=True)
    if os.environ.get('GITHUB_STEP_SUMMARY'):
        with open(os.environ['GITHUB_STEP_SUMMARY'], 'a', encoding='utf-8') as summary:
            summary.write('```text\n' + text.replace('```', "'''") + '```\n')
    # Reporting must not turn continue-on-error checks into blocking failures.
    return 0


if __name__ == '__main__':
    command, *args = sys.argv[1:]
    if command == 'init':
        initialize()
    elif command == 'run':
        sys.exit(capture(*args))
    elif command == 'report':
        sys.exit(report())
    else:
        raise SystemExit(f'Unknown command: {command}')
