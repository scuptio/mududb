#!/usr/bin/env python3
"""Forbid the `test` feature in production dependency declarations.

Several crates define a `test` feature that exposes test-only helper code
(cfg-gated with `#[cfg(any(test, feature = "test"))]`). Enabling that feature
from a production dependency (`[dependencies]`, `[build-dependencies]`, or
`[target.*.dependencies]`) would ship test scaffolding into release builds, so
it is forbidden. `[dev-dependencies]` may enable `test` freely — that is the
sanctioned way to share test helpers across crates.

This script scans every `crates/**/Cargo.toml` (skipping `target/` and `.git`)
with tomllib and reports any production dependency whose `features` list
contains `"test"`.

Exit 0 = no production dependency enables `test`; exit 1 = a violation was found.
"""

from __future__ import annotations

import argparse
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

# Known, documented exceptions. Keep this minimal: every entry must carry a
# reason and should be removed once the underlying need is refactored away.
ALLOWED_MANIFESTS = {
    # The standalone cargo-fuzz workspace deliberately enables mudu_kernel/test
    # in its production dependencies: fuzz targets must link the test-instrumented
    # kernel (arbitrary generators, schema fixtures). Fuzz builds never ship.
    Path("crates/db-kernel/mudu_kernel/fuzz/Cargo.toml"),
}

# Sections scanned per manifest, on top of `[target.*.dependencies]` /
# `[target.*.build-dependencies]`. `dev-dependencies` is intentionally absent.
PRODUCTION_SECTIONS = ("dependencies", "build-dependencies")


def find_manifests():
    """Yield every Cargo.toml under crates/, excluding target/ and .git."""
    for manifest in sorted(ROOT.glob("crates/**/Cargo.toml")):
        relative = manifest.relative_to(ROOT)
        if any(part in ("target", ".git") for part in relative.parts):
            continue
        yield relative


def production_dependency_tables(data):
    """Yield (section_name, table) for every production dependency section."""
    for section in PRODUCTION_SECTIONS:
        table = data.get(section)
        if isinstance(table, dict):
            yield section, table
    targets = data.get("target")
    if isinstance(targets, dict):
        for target_name, target in targets.items():
            if not isinstance(target, dict):
                continue
            for section in PRODUCTION_SECTIONS:
                table = target.get(section)
                if isinstance(table, dict):
                    yield f"target.{target_name}.{section}", table


def check_manifest(relative: Path):
    """Return the list of violations in one manifest."""
    with (ROOT / relative).open("rb") as handle:
        data = tomllib.load(handle)
    violations = []
    for section, table in production_dependency_tables(data):
        for name, value in table.items():
            if not isinstance(value, dict):
                continue
            features = value.get("features")
            if isinstance(features, list) and "test" in features:
                violations.append(
                    f"{relative}: [{section}] {name} enables feature \"test\""
                )
    return violations


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.parse_args()

    violations = []
    for relative in find_manifests():
        if relative in ALLOWED_MANIFESTS:
            continue
        violations.extend(check_manifest(relative))

    if violations:
        print("test feature enabled by production dependencies:")
        for violation in violations:
            print(f"  - {violation}")
        return 1

    print("no production dependency enables the test feature")
    return 0


if __name__ == "__main__":
    sys.exit(main())
