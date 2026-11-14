#!/usr/bin/env python3
"""Cargo-style status output for the MuduDB scripts: colored, right-aligned
status verbs on a terminal, plain text everywhere else.

Colors are emitted only when stdout is a terminal, NO_COLOR is unset, and
TERM is not "dumb", so piped output and CI logs stay plain. On Windows,
colorama is used when installed; otherwise output degrades to plain text.
"""
import os
import sys
from typing import List

_RESET = "\033[0m"
_BOLD = "\033[1m"
_DIM = "\033[2m"
_UNDERLINE = "\033[4m"
_GREEN = "\033[32m"
_CYAN = "\033[36m"
_YELLOW = "\033[33m"
_RED = "\033[31m"

STATUS_WIDTH = 12

STATUS_COLORS = {
    "installed": _GREEN,
    "failed": _RED,
    "skipped": _YELLOW,
    "planned": _CYAN,
}


def _detect_color() -> bool:
    if os.environ.get("NO_COLOR") is not None:
        return False
    if os.environ.get("TERM") == "dumb":
        return False
    if not sys.stdout.isatty():
        return False
    if sys.platform == "win32":
        try:
            import colorama

            colorama.init()
        except ImportError:
            return False
    return True


_enabled = _detect_color()


def set_enabled(value: bool) -> None:
    """Override color detection (used by tests and --color flags)."""
    global _enabled
    _enabled = bool(value)


def paint(text: object, *styles: str) -> str:
    if not _enabled:
        return str(text)
    return "".join(styles) + str(text) + _RESET


def highlight(text: object) -> str:
    """Accent color for paths, versions, and artifact names."""
    return paint(text, _CYAN, _BOLD)


def bold(text: object) -> str:
    return paint(text, _BOLD)


def good(text: object) -> str:
    return paint(text, _GREEN, _BOLD)


def bad(text: object) -> str:
    return paint(text, _YELLOW, _BOLD)


def fail(text: object) -> str:
    return paint(text, _RED, _BOLD)


def link(text: object, url: str) -> str:
    """Clickable OSC 8 terminal hyperlink when the terminal supports it.

    Enabled under the same conditions as color. When disabled (piped output,
    CI logs), the bare URL is returned so it stays copy-pasteable."""
    if not _enabled:
        return url
    return f"\033]8;;{url}\033\\{paint(text, _CYAN, _UNDERLINE)}\033]8;;\033\\"


def dim(text: object) -> str:
    return paint(text, _DIM)


def status(verb: str, message: object) -> None:
    """Print a cargo-style status line: `     Installing <message>`."""
    print(paint(f"{verb:>{STATUS_WIDTH}}", _GREEN, _BOLD), message)


def status_kind(kind: str, width: int = 0) -> str:
    """Color a summary-table status word (installed/failed/skipped/planned).

    Padding is applied before coloring so escape codes never break alignment."""
    text = kind.ljust(width) if width else kind
    color = STATUS_COLORS.get(kind)
    return paint(text, *( (color, _BOLD) if color else () ))


def cmd(parts: List[object]) -> None:
    """Echo a command about to run, dimmed like `cargo -v` output."""
    print(dim("> " + " ".join(str(part) for part in parts)))


def warn(message: object) -> None:
    print(paint("warning:", _YELLOW, _BOLD), message, file=sys.stderr)


def error(message: object) -> None:
    print(paint("error:", _RED, _BOLD), message, file=sys.stderr)


def format_duration(seconds: float) -> str:
    if seconds >= 60:
        return f"{int(seconds // 60)}m {int(seconds % 60)}s"
    return f"{seconds:.1f}s"
