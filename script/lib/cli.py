#!/usr/bin/env python3
"""Shared CLI helpers for the MuduDB scripts."""
import sys
from typing import List


def normalized_argv() -> List[str]:
    """Tolerate a standalone `--` separator so cargo-make task argument
    pass-through works whether written `task --flag` or `task -- --flag`;
    `${@}` can place the separator after literal arguments too."""
    argv = sys.argv[1:]
    if "--" in argv:
        argv.remove("--")
    return argv
