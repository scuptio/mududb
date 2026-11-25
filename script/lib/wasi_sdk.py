#!/usr/bin/env python3
"""Locate an extracted wasi-sdk and validate its sysroot.

Resolution order, first hit wins:

1. the ``WASI_SDK`` environment variable (explicit root),
2. the ``CLANG`` environment variable (derive the root from the compiler path),
3. ``clang`` on ``PATH``,
4. the documented default ``~/.wasi-sdk/wasi-sdk-29.0``.

Every candidate is validated by requiring both ``<root>/bin/clang`` and the
wasip1 sysroot headers ``<root>/share/wasi-sysroot/include/wasm32-wasip1`` to
exist, so a plain host ``clang`` on ``PATH`` is never mistaken for wasi-sdk.

Import ``resolve_wasi_sdk`` for library use, or run as a script (``--clang``
prints the compiler path instead of the root).
"""
from __future__ import annotations

import argparse
import os
import shutil
import sys
from pathlib import Path

_SYSROOT_INCLUDE = ("share", "wasi-sysroot", "include", "wasm32-wasip1")
DEFAULT_WASI_SDK = Path.home() / ".wasi-sdk" / "wasi-sdk-29.0"

_NOT_FOUND = (
    "wasi-sdk 29 not found: set WASI_SDK to an extracted wasi-sdk, add its "
    f"bin/ to PATH, or unpack it to {DEFAULT_WASI_SDK}; see "
    "example/mpk/wallet-c/readme.md"
)


def _is_wasi_sdk(root: Path) -> bool:
    has_clang = (root / "bin" / "clang").is_file() or (root / "bin" / "clang.exe").is_file()
    return has_clang and (root.joinpath(*_SYSROOT_INCLUDE)).is_dir()


def _require_valid(root: Path, source: str) -> Path:
    if not _is_wasi_sdk(root):
        raise RuntimeError(
            f"{source}={root} does not look like a wasi-sdk 29 "
            f"(expected bin/clang and {'/'.join(_SYSROOT_INCLUDE)}); "
            "set WASI_SDK to an extracted wasi-sdk "
            "(see example/mpk/wallet-c/readme.md)"
        )
    return root


def resolve_wasi_sdk(env: dict | None = None) -> Path:
    """Return a validated wasi-sdk root directory, or raise ``RuntimeError``."""
    env = os.environ if env is None else env

    if env.get("WASI_SDK"):
        return _require_valid(Path(env["WASI_SDK"]), "WASI_SDK")

    if env.get("CLANG"):
        return _require_valid(Path(env["CLANG"]).resolve().parent.parent, "CLANG")

    clang = shutil.which("clang")
    if clang:
        root = Path(clang).resolve().parent.parent
        if _is_wasi_sdk(root):
            return root

    if _is_wasi_sdk(DEFAULT_WASI_SDK):
        return DEFAULT_WASI_SDK

    raise RuntimeError(_NOT_FOUND)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--clang",
        action="store_true",
        help="print the compiler path instead of the root directory",
    )
    args = parser.parse_args()
    try:
        root = resolve_wasi_sdk()
    except RuntimeError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1
    print(root / "bin" / "clang" if args.clang else root)
    return 0


if __name__ == "__main__":
    sys.exit(main())
