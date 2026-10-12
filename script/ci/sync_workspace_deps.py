#!/usr/bin/env python3
"""Keep the external dependency specs of the four group workspaces in sync.

The repository groups its crates into four independent cargo workspaces
(crates/{common,db-kernel,sdk,tools}), each with its own `Cargo.toml` and
`[workspace.dependencies]` block. Any external dependency that is shared across
two or more of those workspaces must be declared with an identical spec in each
of them, otherwise the per-workspace `Cargo.lock` files can resolve different
versions of the same crate and downstream cross-group path dependencies break.

`versions.toml` (repository root) is the single source of truth for those
shared specs. This script has two modes:

  * sync   (default): rewrite the matching `[workspace.dependencies]` lines in
            the four `Cargo.toml` files so they match `versions.toml` exactly.
            Only the external-dependency lines listed in `versions.toml` are
            touched; path dependencies, workspace-unique dependencies, comments
            and every other section are left byte-for-byte unchanged.

  * --check: perform the sync in memory and fail (exit 1) if any file would
            change. It additionally enforces two invariants that the plain sync
            does not: every dependency shared by two or more workspaces must be
            listed in `versions.toml`, and every `versions.toml` entry must
            actually be used by at least two workspaces.

Exit code 0 means consistent; exit code 1 means drift was found (or an invalid
invocation); exit code 2 means an internal error (unreadable/malformed input).
"""

from __future__ import annotations

import argparse
import json
import re
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
VERSIONS_FILE = ROOT / "versions.toml"
# Group workspaces whose [workspace.dependencies] external blocks are kept in
# sync. `example/mpk` is a separate consumer workspace.
GROUPS = ["crates/common", "crates/db-kernel", "crates/sdk", "example/mpk", "crates/tools"]

_SECTION_RE = re.compile(r"^\[([^\]]+)\]\s*$")
_KEY_VALUE_RE = re.compile(r"^([A-Za-z0-9_][A-Za-z0-9_-]*)\s*=\s*(.+?)\s*$")


def _fmt(value):
    """Render a parsed TOML value back to the canonical inline style used by the
    workspace `Cargo.toml` files: `{ version = "x.y.z", features = ["a", "b"] }`.
    This must stay byte-identical to the hand-written format so that a sync over
    an already-consistent tree produces no diff."""
    if isinstance(value, str):
        return json.dumps(value)
    if isinstance(value, list):
        return "[" + ", ".join(json.dumps(v) for v in value) + "]"
    if isinstance(value, bool):
        return "true" if value else "false"
    return str(value)


def render_spec(spec):
    parts = [f"{key} = {_fmt(value)}" for key, value in spec.items()]
    return "{ " + ", ".join(parts) + " }"


def load_versions():
    with VERSIONS_FILE.open("rb") as handle:
        data = tomllib.load(handle)
    versions = data.get("versions")
    if not isinstance(versions, dict):
        raise SystemExit(f"{VERSIONS_FILE}: missing [versions] table")
    return versions


def load_workspace_deps(manifest: Path):
    """Return (ordered lines, [(name, value_str, line_index, indentation)]).

    Only entries inside the `[workspace.dependencies]` section are considered.
    Multi-line dependency values are not supported (and not used anywhere in
    this repository); if one is encountered it is reported as an error."""
    text = manifest.read_text(encoding="utf-8")
    lines = text.splitlines()
    start = None
    for i, line in enumerate(lines):
        match = _SECTION_RE.match(line.strip())
        if match and match.group(1) == "workspace.dependencies":
            start = i + 1
            break
    if start is None:
        return lines, []

    entries = []
    end = len(lines)
    for i in range(start, len(lines)):
        stripped = lines[i].strip()
        if _SECTION_RE.match(stripped):
            end = i
            break
        if not stripped or stripped.startswith("#"):
            continue
        match = _KEY_VALUE_RE.match(lines[i])
        if not match:
            continue
        name, value = match.group(1), match.group(2)
        # A multi-line value opens with an unterminated `{`/`[`; detect it so we
        # fail loudly instead of silently corrupting the file.
        if value.count("{") != value.count("}") or value.count("[") != value.count("]"):
            raise SystemExit(
                f"{manifest}: multi-line dependency value for '{name}' is not supported"
            )
        indentation = lines[i][: len(lines[i]) - len(lines[i].lstrip())]
        entries.append((name, value, i, indentation))
    return lines, entries


def parse_spec(spec_str: str):
    """Parse a raw `name = value` line into its TOML value."""
    return tomllib.loads(f"_x = {spec_str}")["_x"]


def classify(spec_str: str) -> str:
    """Classify a raw dependency value as internal (`path`), git, or external."""
    value = parse_spec(spec_str)
    if isinstance(value, dict):
        if "path" in value:
            return "path"
        if "git" in value:
            return "git"
    return "external"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--check",
        action="store_true",
        help="fail instead of rewriting if any workspace Cargo.toml would change",
    )
    args = parser.parse_args()

    versions = load_versions()

    # (group, manifest_path, entries) for the four group workspaces.
    workspaces = []
    for group in GROUPS:
        manifest = ROOT / group / "Cargo.toml"
        lines, entries = load_workspace_deps(manifest)
        workspaces.append((group, manifest, lines, entries))

    # Count, per external dependency name, which workspaces declare it.
    external_usage = {}
    parsed_by_workspace = {}
    for group, manifest, lines, entries in workspaces:
        parsed_by_workspace[group] = {}
        for name, value_str, _idx, _indent in entries:
            if classify(value_str) != "external":
                continue
            parsed_by_workspace[group][name] = parse_spec(value_str)
            external_usage.setdefault(name, set()).add(group)

    errors = []

    # Invariant 1: every shared external dep (>=2 workspaces) is in versions.toml.
    for name in sorted(external_usage):
        if len(external_usage[name]) >= 2 and name not in versions:
            errors.append(
                f"{name} is declared in {sorted(external_usage[name])} but is missing "
                f"from {VERSIONS_FILE.name}; add it there and re-run sync"
            )

    # Invariant 2: every versions.toml entry is actually shared by >=2 workspaces.
    for name in sorted(versions):
        users = sorted(external_usage.get(name, set()))
        if len(users) < 2:
            errors.append(
                f"{name} is listed in {VERSIONS_FILE.name} but only used by {users or 'no'}"
                f" workspace(s); remove it or make it shared"
            )

    # Compute the canonical line for every versions.toml entry and diff against
    # the current files.
    drift = []
    for group, manifest, lines, entries in workspaces:
        new_lines = list(lines)
        for name, value_str, idx, indent in entries:
            if name not in versions:
                continue
            if classify(value_str) != "external":
                errors.append(f"{manifest}: {name} is a path/git dependency but is listed in {VERSIONS_FILE.name}")
                continue
            canonical = render_spec(versions[name])
            new_line = f"{indent}{name} = {canonical}"
            current = lines[idx]
            if current != new_line:
                drift.append((group, name, current, new_line))
                if not args.check:
                    new_lines[idx] = new_line
        if not args.check:
            manifest.write_text("\n".join(new_lines) + "\n", encoding="utf-8")

    if args.check:
        if drift:
            print(f"{VERSIONS_FILE.name}: shared dependency specs have drifted:")
            for group, name, current, new in drift:
                print(f"  {group}/Cargo.toml: {name}")
                print(f"    - {current}")
                print(f"    + {new}")
        # Report as drift/error too, but still exit non-zero below.
        for err in errors:
            print(f"{VERSIONS_FILE.name}: {err}")
        if drift or errors:
            return 1
        print("workspace dependencies are consistent with versions.toml")
        return 0

    for err in errors:
        print(f"{VERSIONS_FILE.name}: warning: {err}", file=sys.stderr)

    if drift:
        print("synced shared dependency specs:")
        for group, name, _current, new in drift:
            print(f"  {group}/Cargo.toml: {name} = {new}")
    else:
        print("workspace dependencies are already consistent")
    return 0


if __name__ == "__main__":
    sys.exit(main())
