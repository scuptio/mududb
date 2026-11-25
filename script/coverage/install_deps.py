#!/usr/bin/env python3
"""Install the toolchain dependencies required for coverage collection.

Cross-platform (Linux / macOS / Windows), standard library only.

Installed on demand:
  1. the pinned nightly toolchain from <repo>/.rust-nightly-version
  2. the llvm-tools-preview rustup component for that toolchain
  3. cargo-llvm-cov (pinned version below)

rustup itself is a prerequisite and is not installed by this script.
"""

import re
import shutil
import subprocess
import sys
import time
from pathlib import Path
from typing import List

SCRIPT_DIR = Path(__file__).resolve().parent
PROJECT_ROOT = SCRIPT_DIR.parent.parent

CARGO_LLVM_COV_VERSION = "0.8.7"

MAX_ATTEMPTS = 3
RETRY_DELAY_SECS = 5


def log_info(message: str) -> None:
    # flush: keep [INFO] lines in order with child-process output when piped.
    print(f"[INFO] {message}", flush=True)


def log_error(message: str) -> None:
    print(f"[ERROR] {message}", file=sys.stderr, flush=True)


def run_with_retry(cmd: List[str]) -> None:
    for attempt in range(1, MAX_ATTEMPTS + 1):
        proc = subprocess.run(cmd)
        if proc.returncode == 0:
            return
        log_error(f"Command failed (attempt {attempt}/{MAX_ATTEMPTS}): {' '.join(cmd)}")
        if attempt < MAX_ATTEMPTS:
            log_info(f"Retrying in {RETRY_DELAY_SECS} seconds...")
            time.sleep(RETRY_DELAY_SECS)
    raise RuntimeError(f"Command failed after {MAX_ATTEMPTS} attempts: {' '.join(cmd)}")


def read_nightly_toolchain() -> str:
    version_file = PROJECT_ROOT / ".rust-nightly-version"
    toolchain = version_file.read_text(encoding="utf-8").strip()
    if not re.fullmatch(r"nightly-\d{4}-\d{2}-\d{2}", toolchain):
        raise RuntimeError(f"invalid pinned nightly: {toolchain}")
    log_info(f"Using pinned nightly toolchain: {toolchain}")
    return toolchain


def require_rustup() -> None:
    if shutil.which("rustup") is None:
        raise RuntimeError(
            "rustup is not installed. Install it from https://rustup.rs/ first."
        )


def host_target_triple() -> str:
    proc = subprocess.run(
        ["rustc", "-vV"], text=True, capture_output=True, check=True
    )
    for line in proc.stdout.splitlines():
        if line.startswith("host:"):
            return line.split(":", 1)[1].strip()
    raise RuntimeError(f"could not determine host target triple:\n{proc.stdout}")


def ensure_nightly_toolchain(toolchain: str) -> None:
    log_info(f"Ensuring pinned nightly toolchain {toolchain} is installed...")
    proc = subprocess.run(
        ["rustup", "toolchain", "list"], text=True, capture_output=True
    )
    installed = proc.returncode == 0 and any(
        line.startswith(toolchain) for line in proc.stdout.splitlines()
    )
    if not installed:
        run_with_retry(
            ["rustup", "toolchain", "install", toolchain, "--profile", "minimal"]
        )
        run_with_retry(
            ["rustup", "target", "add", host_target_triple(), "--toolchain", toolchain]
        )


def ensure_llvm_tools(toolchain: str) -> None:
    log_info("Ensuring llvm-tools-preview is installed...")
    run_with_retry(
        ["rustup", "component", "add", "llvm-tools-preview", "--toolchain", toolchain]
    )


def ensure_cargo_llvm_cov(toolchain: str) -> None:
    log_info(f"Ensuring cargo-llvm-cov {CARGO_LLVM_COV_VERSION} is installed...")
    proc = subprocess.run(
        ["cargo", f"+{toolchain}", "llvm-cov", "--version"],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    if proc.returncode != 0:
        run_with_retry(
            [
                "cargo",
                f"+{toolchain}",
                "install",
                "cargo-llvm-cov",
                "--version",
                CARGO_LLVM_COV_VERSION,
                "--locked",
            ]
        )


def main() -> None:
    require_rustup()
    toolchain = read_nightly_toolchain()
    ensure_nightly_toolchain(toolchain)
    ensure_llvm_tools(toolchain)
    ensure_cargo_llvm_cov(toolchain)
    log_info("All coverage dependencies are installed.")


if __name__ == "__main__":
    try:
        main()
    except RuntimeError as error:
        log_error(str(error))
        sys.exit(1)
