"""Stage progress, durable per-run logs, and diagnostics for verification."""
from __future__ import annotations

from contextlib import contextmanager
from datetime import datetime, timezone
import json
from pathlib import Path
import re
import sys
import tempfile
import threading
import time

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "lib"))
import output

ANSI = re.compile(r"\x1b\[[0-?]*[ -/]*[@-~]")
HEARTBEAT_SECONDS = 10.0


def first_error(text: str) -> str:
    """Keep the first compiler diagnostic and context, not just its final cascade."""
    lines = ANSI.sub("", text).splitlines()
    for index, line in enumerate(lines):
        if re.search(r"(?:^|\s)error(?:\[[^]]+\]|\([^)]*\))?:|fatal error:|Traceback \(", line, re.I):
            return "\n".join(lines[index:index + 12])
    return "\n".join(lines[-25:]) or "Command exited without diagnostics."


def remediation(stage: str) -> str:
    if "charon" in stage:
        return "Inspect the Rust diagnostic and extraction entry; check pinned tools with cargo make doctor."
    if "aeneas" in stage:
        return "Inspect the unsupported Rust construct; do not replace the implementation with an opaque assumption."
    if "audit" in stage:
        return "Inspect the named theorem's axiom dependencies; remove sorry or unreviewed assumptions."
    if "lean" in stage or "mutant" in stage:
        return "Inspect the first Lean file:line:column and goal; compare source, generated implementation and specification."
    return "Inspect the full command log and run cargo make doctor to check prerequisites."


class CommandError(RuntimeError):
    """A failed subprocess, retaining its own log (including mutant diagnostics)."""

    def __init__(self, stage: str, code: int, path: Path, cwd: Path):
        self.stage, self.code, self.path, self.cwd = stage, code, path, cwd
        text = path.read_text(errors="replace")
        self.diagnostic = first_error(text)
        tail = "\n".join(ANSI.sub("", text).splitlines()[-25:])
        super().__init__(
            f"{stage} failed (exit {code})\nFirst diagnostic:\n{self.diagnostic}\n"
            f"Last 25 log lines:\n{tail}\n"
            f"Resolve relative source paths from: {cwd}\n"
            f"Next: {remediation(stage)}\nFull log: {path}"
        )


class RunReport:
    """One execution, with numbered stages and a readable + machine-readable summary."""

    def __init__(self, cache: Path, action: str, total: int, target: str, versions: dict):
        runs = cache / "runs"
        runs.mkdir(parents=True, exist_ok=True)
        stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%S.%fZ")
        self.directory = Path(tempfile.mkdtemp(prefix=f"{stamp}-{action}-", dir=runs))
        self.action, self.total, self.target, self.versions = action, total, target, versions
        self.started = time.monotonic()
        self.stages: list[dict] = []
        self.lock = threading.Lock()
        self.paths: dict[str, int] = {}
        self.verified_functions = 0
        self.verified_theorems = 0
        self.mutation_rejected = False
        self.emit("Verifying", f"{target} ({action})")
        self.emit("Toolchain", " · ".join(f"{key} {value}" for key, value in versions.items()))
        self.emit("Logs", str(self.directory))

    def emit(self, label: str, message: str, *, kind: str = "info") -> None:
        color = {"pass": output.good, "fail": output.fail, "wait": output.bad}.get(kind, output.highlight)
        plain = f"{label:>12} {message}"
        with self.lock:
            print(f"{color(f'{label:>12}')} {message}", flush=True)
            with (self.directory / "output.log").open("a") as stream:
                stream.write(ANSI.sub("", plain) + "\n")

    def log_path(self, name: str) -> Path:
        count = self.paths.get(name, 0) + 1
        self.paths[name] = count
        suffix = "" if count == 1 else f"-{count}"
        return self.directory / f"{name}{suffix}.log"

    @contextmanager
    def stage(self, title: str):
        index = len(self.stages) + 1
        started = time.monotonic()
        item = {"index": index, "title": title, "status": "RUNNING"}
        self.stages.append(item)
        self.emit(f"[{index}/{self.total}]", f"RUNNING {title}")
        stop = threading.Event()

        def heartbeat():
            while not stop.wait(HEARTBEAT_SECONDS):
                self.emit(f"[{index}/{self.total}]", f"Still running: {title} · {output.format_duration(time.monotonic() - started)}", kind="wait")

        thread = threading.Thread(target=heartbeat, daemon=True)
        thread.start()
        try:
            yield
        except BaseException as exc:
            item["status"] = "INTERRUPTED" if isinstance(exc, KeyboardInterrupt) else "FAIL"
            raise
        else:
            item["status"] = "PASS"
        finally:
            stop.set()
            thread.join()
            elapsed = time.monotonic() - started
            item["seconds"] = round(elapsed, 3)
            self.emit(f"[{index}/{self.total}]", f"{item['status']} {title} · {output.format_duration(elapsed)}",
                      kind="pass" if item["status"] == "PASS" else "fail")

    def finish(self, success: bool, error: str | None = None) -> None:
        elapsed = time.monotonic() - self.started
        passed = sum(s["status"] == "PASS" for s in self.stages)
        summary = {
            "action": self.action, "target": self.target, "versions": self.versions,
            "status": "PASS" if success else "FAIL", "seconds": round(elapsed, 3),
            "stages": self.stages, "planned_stages": self.total,
            "verified_functions": self.verified_functions,
            "audited_theorems": self.verified_theorems,
            "mutation_rejected": self.mutation_rejected, "error": error,
        }
        (self.directory / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
        if error:
            self.emit("Error", error, kind="fail")
        label = "Generated" if self.action == "extract" else "Verified"
        self.emit(label if success else "Failed",
                  f"{passed}/{self.total} stages passed · {self.verified_functions} functions · "
                  f"{self.verified_theorems} audited theorems · {output.format_duration(elapsed)}",
                  kind="pass" if success else "fail")
        if self.action == "extract" and success:
            self.emit("Next", "Generated Lean updated; review the diff and run cargo make verify. Proofs have not been checked.")
        if self.mutation_rejected:
            self.emit("Self-test", "PASS: all registered mutants compiled; each was rejected by its target proof", kind="pass")
        self.emit("Logs", str(self.directory))
