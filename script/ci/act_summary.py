"""Render a consolidated job/step summary from `act` console logs.

`script/ci/act.sh` tees every invocation's console output to a log file and
calls this script afterwards. The report lists each job's outcome, the step
that failed (with a short error excerpt), and per-invocation / overall
totals, so a long run does not require scrolling the full log.

Usage:
    python3 script/ci/act_summary.py NAME=LOG [NAME=LOG ...]

NAME is the invocation display name (job id or workflow base name), LOG the
captured console output. Exit code is always 0: the caller owns the run's
exit status.
"""

import argparse
import json
from pathlib import Path
import re
import sys
from dataclasses import dataclass, field

# Standalone stdlib-only parser: also used when the preflight blocks source copying.
ANSI_RE = re.compile(r"\x1b\[[0-?]*[ -/]*[@-~]|\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)")


def plain(text):
    return ANSI_RE.sub('', text)


JOB_LINE_RE = re.compile(r"^\[(?P<job>[^\]]+?)\s*\]\s(?P<body>.*)$")
JOB_RESULT_RE = re.compile(r"^🏁\s+Job\s+(?P<word>succeeded|failed)\b")
STEP_START_RE = re.compile(r"^⭐\s+Run\s+(?:(?P<stage>Pre|Main|Post)\s+)?(?P<name>.*)$")
STEP_RESULT_RE = re.compile(
    r"^\s*(?P<mark>✅|❌|⏭)\s+(?P<word>Success|Failure|Skipped)\s+-\s+"
    r"(?:(?P<stage>Pre|Main|Post)\s+)?(?P<name>.*?)(?:\s+\[(?P<dur>[0-9a-zµ.]+)\])?$"
)
ERROR_RE = re.compile(r"(::error::|\berror(\[[^\]]*\])?:|\bfailed\b|\bfailure\b|panicked)", re.IGNORECASE)
DURATION_PART_RE = re.compile(r"(\d+(?:\.\d+)?)(ns|us|µs|ms|s|m|h)")
DURATION_FACTOR = {"ns": 1e-9, "us": 1e-6, "µs": 1e-6, "ms": 1e-3, "s": 1.0, "m": 60.0, "h": 3600.0}

EXCERPT_MAX_LINES = 12


@dataclass
class Step:
    stage: str
    name: str
    word: str
    duration: str
    start_line: int
    end_line: int


@dataclass
class Job:
    label: str
    status: str = "aborted"  # succeeded | failed | aborted (no 🏁 line seen)
    steps: list = field(default_factory=list)
    lines: list = field(default_factory=list)  # body text of every line of this job

    @property
    def duration_secs(self):
        total = 0.0
        seen = False
        for step in self.steps:
            if step.duration:
                total += parse_go_duration(step.duration)
                seen = True
        return total if seen else None

    line_numbers: list = field(default_factory=list)

    @property
    def failed_steps(self):
        return [step for step in self.steps if step.word == "Failure"]

    @property
    def failed_step(self):
        for step in self.steps:
            if step.word == "Failure":
                return step
        return None


def parse_go_duration(text):
    """Parses a Go duration string such as `1m5.2s` or `48.2ms` to seconds."""
    total = 0.0
    for value, unit in DURATION_PART_RE.findall(text):
        total += float(value) * DURATION_FACTOR[unit]
    return total


def format_duration(secs):
    if secs is None:
        return "-"
    if secs >= 90:
        return f"~{int(secs // 60)}m{int(secs % 60):02d}s"
    return f"~{secs:.0f}s" if secs >= 1 else "<1s"


def short_label(label):
    """Drops the leading `<workflow>/` segment from act's job display name."""
    return label.split("/", 1)[-1]


def parse_log(path):
    """Parses one act console log into an ordered list of Job records."""
    jobs = {}
    order = []

    with open(path, encoding="utf-8", errors="replace") as handle:
        for lineno, raw in enumerate(handle):
            match = JOB_LINE_RE.match(plain(raw.rstrip("\n")))
            if not match:
                continue
            label, body = match.group("job"), match.group("body")
            job = jobs.get(label)
            if job is None:
                job = jobs[label] = Job(label=label)
                order.append(label)
            job.lines.append(body)
            job.line_numbers.append(lineno + 1)

            result = JOB_RESULT_RE.match(body)
            if result:
                job.status = result.group("word")
                continue

            start = STEP_START_RE.match(body)
            if start:
                job.steps.append(
                    Step(
                        stage=start.group("stage") or "",
                        name=start.group("name").strip(),
                        word="",
                        duration="",
                        start_line=lineno,
                        end_line=lineno,
                    )
                )
                continue

            step_result = STEP_RESULT_RE.match(body)
            if step_result:
                stage = step_result.group("stage") or ""
                name = step_result.group("name").strip()
                step = next((s for s in reversed(job.steps)
                             if s.stage == stage and s.name == name and not s.word), None)
                if step is None:
                    step = Step(stage, name, "", "", lineno, lineno)
                    job.steps.append(step)
                step.word = step_result.group("word")
                step.duration = step_result.group("dur") or ""
                step.end_line = lineno
                continue
    return [jobs[label] for label in order]


def step_content(job, step):
    """Extract the full step section, retaining diagnostics after its result."""
    end = next((s.start_line for s in job.steps if s.start_line > step.end_line), float('inf'))
    return [body for number, body in zip(job.line_numbers, job.lines)
            if step.start_line < number <= end]


def diagnostics(lines):
    """Include preceding diagnostics as well as following panic context."""
    content = []
    for line in lines:
        text = line.strip()
        if STEP_START_RE.match(text) or STEP_RESULT_RE.match(text) or JOB_RESULT_RE.match(text):
            continue
        if text.startswith('|'):
            text = text[1:].strip()
        content.append(text)
    indexes = set()
    for i, line in enumerate(content):
        if ERROR_RE.search(line) and not re.search(r'test result: ok\.|0 failed', line):
            indexes.update(range(max(0, i - 2), min(i + 3, len(content))))
    chosen = [content[i] for i in sorted(indexes) if content[i]]
    return (chosen or [line for line in content if line][-5:])[-EXCERPT_MAX_LINES:]


def failed_tests(lines):
    """Extract libtest failure identities without counting application ERRORs."""
    found = []
    for line in lines:
        text = line.strip().removeprefix('| ').strip()
        match = re.match(r'---- (.+) (?:stdout|stderr) ----', text)
        if not match:
            match = re.match(r'test (.+?) \.\.\. FAILED$', text)
        if match and match[1] not in found:
            found.append(match[1])
    return found


def step_excerpt(job, step):
    return diagnostics(step_content(job, step))


def safe_name(text):
    return re.sub(r'[^A-Za-z0-9_.-]+', '-', text).strip('-')[:120] or 'job'


def render_invocation(name, path, out, directory=None, exit_code=None, records=None):
    path = Path(path).resolve()
    jobs = parse_log(path)
    counts = {"succeeded": 0, "failed": 0, "aborted": 0}
    out.append(f"■ {name}  (log: {path})")
    if exit_code is not None:
        out.append(f"  invocation exit code: {exit_code}")
    for job in jobs:
        counts[job.status] += 1
        mark = {"succeeded": "✅", "failed": "❌", "aborted": "⏸"}[job.status]
        out.append(f"  {mark} {job.status.upper():9} {short_label(job.label)}  {format_duration(job.duration_secs)}")
        job_path = None
        if directory:
            job_path = directory / f'{len(records or []) + 1:02d}-{safe_name(name)}-{safe_name(job.label)}.log'
            job_path.write_text('\n'.join(job.lines) + '\n', encoding='utf-8')
        record = dict(invocation=name, job=job.label, status=job.status,
                      console_log=str(path), log=str(job_path) if job_path else None, failures=[])
        for i, step in enumerate(job.failed_steps):
            stage = f"{step.stage} " if step.stage else ""
            dur = f" [{step.duration}]" if step.duration else ""
            content = step_content(job, step)
            tests = failed_tests(content)
            excerpt = diagnostics(content)
            step_path = None
            if directory:
                step_path = directory / f'{job_path.stem}-failure-{i + 1}.log'
                step_path.write_text('\n'.join(content) + '\n', encoding='utf-8')
            out.append(f"          failed step: {stage}{step.name}{dur}")
            if tests:
                out.append('          failed tests: ' + ', '.join(tests))
            out.extend(f"            {line}" for line in excerpt)
            out.append(f"          console: {path}:{step.start_line + 1}")
            if step_path:
                out.append(f"          failure log: {step_path}")
            record['failures'].append(dict(step=step.name, stage=step.stage, tests=tests,
                                          diagnostics=excerpt, log=str(step_path) if step_path else None))
        if job.status != 'succeeded' and not job.failed_steps:
            out.append('          no completed failed-step marker; inspect setup/interruption diagnostics:')
            out.extend('            ' + line for line in diagnostics(job.lines))
            out.append(f"          job log: {job_path or path}")
        if records is not None:
            records.append(record)
    out.append(f"  jobs {len(jobs)}:  ✅ {counts['succeeded']} succeeded,  ❌ {counts['failed']} failed,  ⏸ {counts['aborted']} no-result")
    if not jobs:
        out.append('  (no job output captured; result is not verified)')
        out.extend('    ' + line for line in diagnostics(plain(path.read_text(encoding='utf-8', errors='replace')).splitlines()))
    out.append('')
    return counts


def main(argv):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("logs", nargs="+", metavar="NAME=LOG")
    parser.add_argument('--report-dir', type=Path, help='Save summary.txt/json, full job logs and failures.log here')
    parser.add_argument('--exit-code', action='append', default=[], metavar='NAME=CODE', help='Actual invocation exit status (including failures before job startup)')
    args = parser.parse_args(argv)
    codes = {}
    for item in args.exit_code:
        name, code = item.rsplit('=', 1)
        codes[name] = int(code)
    directory = args.report_dir.resolve() if args.report_dir else None
    if directory:
        directory.mkdir(parents=True, exist_ok=True)
    out = ['', '=' * 60, 'act run summary', '=' * 60]
    total = {'succeeded': 0, 'failed': 0, 'aborted': 0}
    records, problems, invocations = [], [], []
    for item in args.logs:
        if '=' not in item:
            problems.append(f'{item}: expected NAME=LOG')
            continue
        name, path = item.split('=', 1)
        invocations.append(dict(name=name, log=str(Path(path).resolve()), exit_code=codes.get(name)))
        try:
            counts = render_invocation(name, path, out, directory, codes.get(name), records)
        except OSError as error:
            problems.append(f'{name}: log unreadable: {error}')
            continue
        if not sum(counts.values()):
            problems.append(f'{name}: no job results captured')
        if codes.get(name, 0):
            problems.append(f'{name}: act exited {codes[name]}')
        for key in total:
            total[key] += counts[key]
    out.extend('! ' + problem for problem in problems)
    out.append('-' * 60)
    out.append(f"overall jobs: ✅ {total['succeeded']} succeeded,  ❌ {total['failed']} failed,  ⏸ {total['aborted']} no-result")
    result = 'FAILURE' if total['failed'] or total['aborted'] or problems else 'SUCCESS'
    out.append('result: ' + result)
    if directory:
        failure_path = directory / 'failures.log'
        with failure_path.open('w', encoding='utf-8') as handle:
            for record in records:
                if record['status'] != 'succeeded' or record['failures']:
                    handle.write(f"\n===== {record['job']} =====\n")
                    handle.write(Path(record['log']).read_text(encoding='utf-8'))
            # Capture unprefixed setup/engine errors even when there is no job.
            for invocation in invocations:
                if invocation['exit_code'] or not any(r['invocation'] == invocation['name'] for r in records):
                    try:
                        handle.write(f"\n===== invocation: {invocation['name']} =====\n")
                        handle.write(plain(Path(invocation['log']).read_text(encoding='utf-8', errors='replace')))
                    except OSError as error:
                        handle.write(str(error) + '\n')
        out.extend([f"Summary: {directory / 'summary.txt'}", f"JSON: {directory / 'summary.json'}", f"Failure logs: {failure_path}"])
        (directory / 'summary.json').write_text(json.dumps(dict(result=result, totals=total, jobs=records,
            invocations=invocations, problems=problems, failure_log=str(failure_path)), indent=2) + '\n', encoding='utf-8')
    out.append('=' * 60)
    text = '\n'.join(out) + '\n'
    if directory:
        (directory / 'summary.txt').write_text(text, encoding='utf-8')
    print(text, end='')
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
