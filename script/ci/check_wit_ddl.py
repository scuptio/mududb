#!/usr/bin/env python3
"""Regenerate DDL→WIT projections and compare bytes with the checkout.

Each target maps a set of DDL SQL files to a committed WIT module produced by
`mgen object --emit-wit`. The gate regenerates the WIT and requires byte-equal
output (the same drift convention as `check_bindings_regen.py`).

M3 migrates each example to a single DDL definition and populates `TARGETS`
below; until then the manifest is empty and the gate passes trivially.
"""
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]

# (ddl_files, package, interface, committed_wit_path)
TARGETS = [
    (
        ["example/mpk/wallet/sql/ddl.sql"],
        "mududb:wallet",
        "types",
        "example/mpk/wallet/wit/types.wit",
    ),
    (
        ["example/mpk/vote/sql/ddl.sql", "example/mpk/vote/sql/type.sql"],
        "mududb:vote",
        "types",
        "example/mpk/vote/wit/types.wit",
    ),
]


def run(argv, cwd):
    return subprocess.run(argv, cwd=cwd, check=False, capture_output=True, text=True)


def regenerate(ddl_files, package, interface, output):
    argv = [
        "cargo",
        "run",
        "--quiet",
        "--manifest-path",
        "crates/tools/Cargo.toml",
        "-p",
        "mudu_gen",
        "--bin",
        "mgen",
        "--",
        "object",
        "-i",
        *ddl_files,
        "-o",
        str(output.parent / "src"),
        "-t",
        str(output.parent / "src" / "types.json"),
        "-l",
        "rust",
        "--emit-wit",
        "--wit-package",
        package,
        "--wit-interface",
        interface,
        "--wit-output",
        str(output),
    ]
    proc = run(argv, REPO_ROOT)
    if proc.returncode != 0:
        raise RuntimeError(f"mgen failed: {proc.stderr}")


def check(update: bool) -> int:
    import tempfile

    failed = 0
    for ddl_files, package, interface, committed in TARGETS:
        committed_path = REPO_ROOT / committed
        if not committed_path.is_file():
            print(f"MISSING: {committed}")
            failed += 1
            continue
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / "types.wit"
            regenerate([str(REPO_ROOT / d) for d in ddl_files], package, interface, out)
            generated = out.read_bytes()
            checked = committed_path.read_bytes()
            if generated == checked:
                print(f"OK: {committed}")
            elif update:
                committed_path.write_bytes(generated)
                print(f"UPDATED: {committed}")
            else:
                print(f"STALE: {committed}")
                failed += 1
    return 1 if failed else 0


def main() -> int:
    update = "--update" in sys.argv
    return check(update)


if __name__ == "__main__":
    raise SystemExit(main())
