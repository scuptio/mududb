"""Sequential task execution with dependency skips and durable plain-text reports."""
from collections import deque
import contextlib
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import tempfile
import time

from output import bad, bold, fail, good, highlight, set_enabled
from test_display import plain, styled_line

ROOT = Path(__file__).resolve().parents[2]


def add_options(parser, name):
    parser.description = (parser.description or '') + (
        '\nIndependent steps continue after errors; prerequisites block dependent steps. '
        'Reports include summary.txt, summary.json, full per-step logs and failures.log. '
        'Failures return nonzero (advisory steps are reported separately).')
    parser.add_argument('--fail-fast', action='store_true', help='Stop after the first required failure')
    parser.add_argument('--log-dir', type=Path, default=ROOT / 'logs' / name,
                        help='Parent directory for a unique run and its reports')
    parser.add_argument('--color', choices=['auto', 'always', 'never'], default='auto',
                        help='Terminal colors; saved logs always remain plain text')


class Report:
    """Run subprocess steps serially; publish a report after every outcome."""
    def __init__(self, args, title):
        if args.color != 'auto':
            set_enabled(args.color == 'always')
        args.log_dir.mkdir(parents=True, exist_ok=True)
        self.directory = Path(tempfile.mkdtemp(prefix=time.strftime('%Y%m%d-%H%M%S-'), dir=args.log_dir)).resolve()
        self.results = []
        self.fail_fast = args.fail_fast
        self.stopped = False
        self.interrupted = False
        print(bold(title), highlight(self.directory), flush=True)
        self.save()

    def run(self, name, argv, cwd=ROOT, needs=(), advisory=False, env=None):
        previous = {r['name']: r for r in self.results}
        blocked = [dep for dep in needs if dep not in previous or previous[dep]['status'] != 'passed']
        reason = ('blocked by: ' + ', '.join(blocked)) if blocked else ''
        if self.stopped:
            reason = 'interrupted' if self.interrupted else 'stopped by --fail-fast'
        result = dict(name=name, command=[str(a) for a in argv], cwd=str(cwd),
                      status='skipped', exit_code=None, duration_seconds=0, reason=reason, log=None)
        if reason:
            self.results.append(result)
            self.save()
            return False
        filename = f'{len(self.results) + 1:02d}-{re.sub(r"[^A-Za-z0-9_.-]", "-", name)}.log'
        path = self.directory / filename
        result['log'] = str(path)
        print(highlight(f'\n[{len(self.results) + 1}] {name}'), flush=True)
        started = time.monotonic()
        proc = None
        tail = deque(maxlen=30)
        with path.open('w', encoding='utf-8') as log:
            log.write(f'cwd: {cwd}\ncommand: {json.dumps(result["command"])}\n\n')
            try:
                proc = subprocess.Popen(result['command'], cwd=cwd,
                    env={**os.environ, 'CARGO_TERM_COLOR': 'never', 'NO_COLOR': '1', **(env or {})},
                    stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
                    encoding='utf-8', errors='replace', start_new_session=os.name != 'nt')
                for line in proc.stdout:
                    line = plain(line)
                    tail.append(line.rstrip()[:1000])
                    log.write(line)
                    log.flush()
                    print(styled_line(line), end='', flush=True)
                code = proc.wait()
            except OSError as exc:
                code = 127
                log.write(f'error: {exc}\n')
                result['reason'] = str(exc)
            except KeyboardInterrupt:
                self.interrupted = self.stopped = True
                code = 130
                log.write('\nInterrupted by user\n')
                if proc is not None:
                    if os.name != 'nt':
                        with contextlib.suppress(ProcessLookupError):
                            os.killpg(proc.pid, signal.SIGINT)
                    else:
                        proc.terminate()
                    try:
                        proc.wait(timeout=15)
                    except subprocess.TimeoutExpired:
                        if os.name != 'nt':
                            with contextlib.suppress(ProcessLookupError):
                                os.killpg(proc.pid, signal.SIGKILL)
                        else:
                            proc.kill()
                        proc.wait()
            finally:
                if proc is not None and proc.stdout is not None:
                    proc.stdout.close()
        result.update(exit_code=code, duration_seconds=round(time.monotonic() - started, 3),
                      status='interrupted' if self.interrupted else ('passed' if code == 0 else ('advisory' if advisory else 'failed')))
        if code and not result['reason']:
            result['reason'] = f'command exited {code}; see full log'
        result["diagnostics"] = list(tail) if code else []
        self.results.append(result)
        if code and not advisory and self.fail_fast:
            self.stopped = True
        self.save()
        return code == 0

    def save(self):
        totals = {s: sum(r['status'] == s for r in self.results)
                  for s in ('passed', 'failed', 'skipped', 'advisory', 'interrupted')}
        (self.directory / 'summary.json').write_text(json.dumps(
            dict(totals=totals, steps=self.results), indent=2) + '\n', encoding='utf-8')
        lines = ['Task report', '  '.join(f'{k}: {v}' for k, v in totals.items()), '']
        with (self.directory / 'failures.log').open('w', encoding='utf-8') as failures:
            for r in self.results:
                lines.append(f"{r['status'].upper():12} {r['name']} ({r['duration_seconds']:.1f}s) {r['reason']}")
                if r['log']:
                    lines.append(f"  log: {r['log']}")
                if r.get("diagnostics"):
                    lines.extend("  " + line for line in r["diagnostics"])
                if r['status'] in ('failed', 'advisory', 'interrupted') and r['log']:
                    failures.write(f"\n===== {r['name']} =====\n")
                    with Path(r['log']).open(encoding='utf-8') as source:
                        shutil.copyfileobj(source, failures)
        (self.directory / 'summary.txt').write_text('\n'.join(lines) + '\n', encoding='utf-8')

    def finish(self):
        print(bold('\nTask summary'), flush=True)
        for r in self.results:
            style = good if r['status'] == 'passed' else fail if r['status'] == 'failed' else bad
            print(style(f"{r['status'].upper():12}"), r['name'], f"{r['duration_seconds']:.1f}s", r['reason'])
        print('  '.join(f'{state}: {sum(r["status"] == state for r in self.results)}'
                        for state in ('passed', 'failed', 'skipped', 'advisory', 'interrupted')))
        print(highlight(self.directory / 'summary.txt'))
        print('Full failure output:', self.directory / 'failures.log')
        return 130 if self.interrupted else int(any(r['status'] in ('failed', 'skipped') for r in self.results))
