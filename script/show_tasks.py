#!/usr/bin/env python3
"""Pretty-print the repository's cargo-make tasks (runs as `cargo make help`).

Reads the root Makefile.toml directly, so the listing contains only the
project's own tasks, grouped by category with cargo-style colors. For the raw
listing including cargo-make's built-in flows, use `cargo make
--list-all-steps`.
"""
import sys
import tomllib
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO_ROOT / "script" / "lib"))
from output import bold, dim, highlight  # noqa: E402

# Preferred display order; tasks in other categories are appended after.
CATEGORY_ORDER = ["Build", "Install", "Package", "Release", "Check", "Test", "Coverage", "Audit", "Utility", "Verification"]


def load_tasks() -> list:
    data = tomllib.loads((REPO_ROOT / "Makefile.toml").read_text(encoding="utf-8"))
    tasks = []
    for name, spec in data.get("tasks", {}).items():
        if not isinstance(spec, dict):
            continue
        tasks.append(
            {
                "name": name,
                "description": spec.get("description") or "-",
                "category": spec.get("category") or "Other",
            }
        )
    return tasks


def main() -> int:
    tasks = load_tasks()
    if not tasks:
        print(dim("no tasks found in Makefile.toml"))
        return 0

    categories = [c for c in CATEGORY_ORDER if any(t["category"] == c for t in tasks)]
    extras = sorted({t["category"] for t in tasks} - set(categories))
    width = max(len(task["name"]) for task in tasks)

    print(bold("MuduDB cargo-make tasks"))
    for category in [*categories, *extras]:
        print()
        print(bold(category))
        for task in tasks:
            if task["category"] == category:
                print(f"  {highlight(task['name'].ljust(width))}  {task['description']}")
    print()
    print(dim("run a task:  cargo make <task> [-- args]"))
    print(dim("raw listing incl. cargo-make built-ins:  cargo make --list-all-steps"))
    return 0


if __name__ == "__main__":
    sys.exit(main())
