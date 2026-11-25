"""Terminal presentation for workspace tests; persisted reports remain plain text."""
import re

from output import bad, bold, dim, fail, format_duration, good, highlight, link

ANSI_ESCAPE = re.compile(r"\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)|\x1b\[[0-?]*[ -/]*[@-~]")


def plain(text):
    return ANSI_ESCAPE.sub("", text)


def styled_line(line):
    """Apply emphasis only to recognizable Cargo/libtest lines."""
    text = line.rstrip("\r\n")
    ending = line[len(text):]
    stripped = text.lstrip()
    if stripped.startswith(("error", "Error:", "ERROR ", "failures:")) or "panicked at" in text:
        return fail(text) + ending
    if stripped.startswith(("warning", "WARN ")):
        return bad(text) + ending
    if stripped.startswith("test result:"):
        return (fail(text) if "FAILED" in text else good(text)) + ending
    if stripped.startswith(("Running ", "Doc-tests ", "running ")):
        return highlight(text) + ending
    if stripped.startswith(("Compiling ", "Checking ", "Finished ", "Blocking ")):
        return dim(text) + ending
    if stripped.endswith(("... FAILED", "--- FAILED")):
        return fail(text) + ending
    if stripped and set(stripped) <= {".", "i"}:
        return dim(text) + ending
    return line


def header(groups, log_dir, fail_fast):
    print()
    print(highlight("MuduDB") + "  " + bold("Workspace tests"))
    print(dim("=" * 64))
    mode = "fail fast" if fail_fast else "continue after failures"
    print(f"  {bold(str(len(groups)))} workspaces  |  {mode}")
    print(f"  Logs  {link(str(log_dir), log_dir.as_uri())}")
    print(flush=True)


def group_start(index, total, group, workspace):
    print()
    print(highlight(f"[{index}/{total}]") + "  " + bold(group) + "  " + dim(workspace))
    print(dim("-" * 64), flush=True)


def summary(selected, results, log_dir, elapsed):
    by_group = {result["group"]: result for result in results}
    failed = any(r["exit_code"] for r in results)
    complete = len(results) == len(selected)
    interrupted = any(r["status"] == "interrupted" for r in results)
    title = "TEST RUN INTERRUPTED" if interrupted else "TEST RUN FAILED" if failed else "ALL TESTS PASSED" if complete else "TEST RUN INCOMPLETE"
    print()
    print(dim("=" * 64))
    print((bad if interrupted else fail if failed else good if complete else bad)(title) + "  " + dim(format_duration(elapsed)))
    print()
    print(dim(f"  {'WORKSPACE':<16} {'RESULT':<12} {'PASS':>7} {'FAIL':>7} {'IGNORE':>7} {'TIME':>9}"))
    for group in selected:
        result = by_group.get(group)
        if result is None:
            print(f"  {group:<16} {bad('NOT RUN'.ljust(12))} {'-':>7} {'-':>7} {'-':>7} {'-':>9}")
            continue
        outcome = result["status"]
        style = good if outcome == "passed" else bad if outcome == "interrupted" else fail
        tests = result["tests"]
        print(
            f"  {group:<16} {style(outcome.upper().ljust(12))} "
            f"{tests['passed']:>7} {fail(str(tests['failed']).rjust(7)) if tests['failed'] else str(0).rjust(7)} "
            f"{tests['ignored']:>7} {format_duration(result['duration_seconds']):>9}"
        )
    counts = {key: sum(r["tests"][key] for r in results) for key in ("passed", "failed", "ignored")}
    print()
    print("  " + good(f"{counts['passed']} passed") + "  /  "
          + (fail if counts['failed'] else dim)(f"{counts['failed']} failed") + "  /  "
          + bad(f"{counts['ignored']} ignored") + dim("  (reported by libtest)"))
    failed_groups = [r["group"] for r in results if r["exit_code"]]
    if failed_groups:
        print("  " + fail(f"{len(failed_groups)} unsuccessful workspaces: " + ", ".join(failed_groups)))
    uncounted = [r["group"] for r in results if r["exit_code"] and not r["tests"]["failed"]]
    if uncounted:
        print("  " + bad("Failure/interruption without a test count: " + ", ".join(uncounted)))
    print()
    for label, filename in (("Report", "summary.txt"), ("JSON", "summary.json"), ("Errors", "failures.log")):
        path = log_dir / filename
        print(f"  {bold(label.ljust(7))} {link(str(path), path.as_uri())}")
    print(dim("=" * 64), flush=True)
