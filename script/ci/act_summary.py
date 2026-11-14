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
import re
import sys
from dataclasses import dataclass, field

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
    open_step = {}  # job label -> index of its current step in Job.steps

    with open(path, encoding="utf-8", errors="replace") as handle:
        for lineno, raw in enumerate(handle):
            match = JOB_LINE_RE.match(raw.rstrip("\n"))
            if not match:
                continue
            label, body = match.group("job"), match.group("body")
            job = jobs.get(label)
            if job is None:
                job = jobs[label] = Job(label=label)
                order.append(label)
            job.lines.append(body)

            result = JOB_RESULT_RE.match(body)
            if result:
                job.status = result.group("word")
                continue

            start = STEP_START_RE.match(body)
            if start:
                open_step[label] = len(job.steps)
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
                index = open_step.get(label)
                if index is not None and index < len(job.steps):
                    step = job.steps[index]
                    step.word = step_result.group("word")
                    step.duration = step_result.group("dur") or ""
                    step.end_line = lineno
                continue
    return [jobs[label] for label in order]


def step_excerpt(job, step):
    """Returns error-looking body lines from the failed step's section."""
    # Slice the job's lines from the step's ⭐ Run marker to its own result
    # line, then keep trailing diagnostics (e.g. `exitcode '1': failure`)
    # until the next step/job marker.
    collecting = False
    section = []
    for body in job.lines:
        start = STEP_START_RE.match(body)
        if start:
            if collecting:
                break
            collecting = (start.group("name").strip() == step.name) and (
                (start.group("stage") or "") == step.stage
            )
            section = [] if collecting else section
            continue
        if not collecting:
            continue
        if JOB_RESULT_RE.match(body):
            break
        if STEP_RESULT_RE.match(body):
            continue
        section.append(body)
    content = []
    for body in section:
        text = body.strip()
        if text.startswith("| "):
            text = text[2:]
        if text:
            content.append(text)
    errors = [line for line in content if ERROR_RE.search(line)]
    chosen = errors if errors else content[-5:]
    return chosen[-EXCERPT_MAX_LINES:]


def render_invocation(name, path, out):
    jobs = parse_log(path)
    counts = {"succeeded": 0, "failed": 0, "aborted": 0}
    for job in jobs:
        counts[job.status] += 1
    out.append(f"■ {name}  (log: {path})")
    out.append(
        f"  jobs {len(jobs)}:  ✅ {counts['succeeded']} succeeded,"
        f"  ❌ {counts['failed']} failed,"
        f"  ⏸ {counts['aborted']} no-result"
    )
    if not jobs:
        out.append("  (no job output captured)")
    width = max((len(short_label(j.label)) for j in jobs), default=0)
    for job in jobs:
        mark = {"succeeded": "✅", "failed": "❌", "aborted": "⏸"}[job.status]
        word = {"succeeded": "PASS", "failed": "FAIL", "aborted": "ABORT"}[job.status]
        line = f"  {mark} {word:<5} {short_label(job.label):<{width}}  {format_duration(job.duration_secs)}"
        out.append(line)
        step = job.failed_step
        if job.status == "failed" and step:
            stage = f"{step.stage} " if step.stage else ""
            dur = f" [{step.duration}]" if step.duration else ""
            out.append(f"          failed step: {stage}{step.name}{dur}")
            for excerpt_line in step_excerpt(job, step):
                out.append(f"            {excerpt_line}")
    out.append("")
    return counts


def main(argv):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("logs", nargs="+", metavar="NAME=LOG", help="invocation name and log file")
    args = parser.parse_args(argv)

    out = ["", "=" * 60, "act run summary", "=" * 60]
    total = {"succeeded": 0, "failed": 0, "aborted": 0}
    for item in args.logs:
        if "=" not in item:
            out.append(f"■ {item}  (unparsed: expected NAME=LOG)")
            continue
        name, path = item.split("=", 1)
        try:
            counts = render_invocation(name, path, out)
        except OSError as error:
            out.append(f"■ {name}  (log unreadable: {error})")
            out.append("")
            continue
        for key in total:
            total[key] += counts[key]

    out.append("-" * 60)
    out.append(
        f"overall jobs: ✅ {total['succeeded']} succeeded,"
        f"  ❌ {total['failed']} failed,"
        f"  ⏸ {total['aborted']} no-result"
    )
    out.append("result: " + ("FAILURE" if total["failed"] or total["aborted"] else "SUCCESS"))
    out.append("=" * 60)
    print("\n".join(out))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
