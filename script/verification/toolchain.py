#!/usr/bin/env python3
"""Pinned, user-level verification tools. Invoked by the repository doctor."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shlex
import shutil
import subprocess
import sys
import tarfile
import time
import tomllib
import urllib.request

try:
    from . import reporting
except ImportError:
    import reporting

ROOT = Path(__file__).resolve().parents[2]
LOCK = ROOT / "verification/toolchain.lock.toml"
CONFIG = tomllib.loads(LOCK.read_text())
CACHE = ROOT / "target/verification"
TOOLS = CACHE / "tools"
AENEAS = TOOLS / "aeneas-release"
LEAN = TOOLS / f"lean-{CONFIG['lean_version']}-linux"
STAMP = TOOLS / "installed.json"
REPORT: reporting.RunReport | None = None


def environment() -> dict[str, str]:
    env = os.environ.copy()
    env["PATH"] = str(LEAN / "bin") + os.pathsep + env.get("PATH", "")
    env["RUSTUP_TOOLCHAIN"] = CONFIG["rust"]
    env["CARGO_TARGET_DIR"] = str(CACHE / "charon")
    env["CARGO_BUILD_JOBS"] = "4"
    env["LEAN_NUM_THREADS"] = "4"
    return env


def run(argv: list[str], *, cwd: Path = ROOT, log: str = "install") -> Path:
    """Keep complete commands/output; return this invocation's unique log path."""
    if REPORT is not None:
        path = REPORT.log_path(log)
        REPORT.emit("Command", shlex.join(map(str, argv)))
        REPORT.emit("Working dir", str(cwd))
        REPORT.emit("Log", str(path))
    else:
        logs = CACHE / "logs"
        logs.mkdir(parents=True, exist_ok=True)
        path = logs / f"{log}.log"
        print(f"[{log}] {shlex.join(map(str, argv))}\n  log: {path}", flush=True)
    with path.open("w" if REPORT is not None else "a") as out:
        out.write(f"\n$ {argv!r}\n")
        out.flush()
        with subprocess.Popen(argv, cwd=cwd, env=environment(), stdout=out, stderr=subprocess.STDOUT) as proc:
            try:
                code = proc.wait()
            except BaseException:
                proc.terminate()
                try:
                    proc.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    proc.wait()
                raise
    if code:
        raise reporting.CommandError(log, code, path, cwd)
    return path


def digest(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def prerequisites() -> list[str]:
    if platform.system() != "Linux" or platform.machine() not in ("x86_64", "AMD64"):
        return ["supported platform: Linux x86_64 (including WSL2)"]
    return [name for name in ("rustup", "git", "cc", "tar", "zstd") if not shutil.which(name)]


def probe() -> tuple[bool, str]:
    missing = prerequisites()
    if missing:
        return False, "prerequisites missing: " + ", ".join(missing)
    try:
        stamp = json.loads(STAMP.read_text())
        if stamp["lock"] != digest(LOCK):
            return False, "verification tool lock changed; rerun doctor install"
        for path, expected in stamp["binaries"].items():
            if digest(ROOT / path) != expected:
                return False, f"tool missing or modified: {path}"
        result = subprocess.run(
            ["rustup", "run", CONFIG["rust"], "rustc", "--version"],
            capture_output=True, text=True,
        )
        if result.returncode:
            return False, "pinned Rust toolchain missing"
        components = subprocess.run(
            ["rustup", "component", "list", "--installed", "--toolchain", CONFIG["rust"]],
            capture_output=True, text=True,
        )
        if components.returncode or not all(name in components.stdout for name in ("rustc-dev", "rust-src", "llvm-tools")):
            return False, "pinned Rust compiler components missing"
        if not (ROOT / "verification/lean/.lake/packages/mathlib/lakefile.lean").exists():
            return False, "Lean dependencies missing; rerun doctor install"
        return True, f"Aeneas {CONFIG['aeneas_version']}; Lean {CONFIG['lean_version']}"
    except (OSError, ValueError, KeyError):
        return False, "not installed; cargo make doctor -- install --only formal-verification"


def download(url: str, dest: Path, expected: str) -> None:
    if dest.exists() and digest(dest) == expected:
        return
    dest.parent.mkdir(parents=True, exist_ok=True)
    part = dest.with_suffix(dest.suffix + ".part")
    for attempt in range(3):
        try:
            print(f"Downloading {url}", flush=True)
            with urllib.request.urlopen(url, timeout=60) as response, part.open("wb") as out:
                shutil.copyfileobj(response, out)
            if digest(part) != expected:
                raise RuntimeError(f"SHA256 mismatch for {url}; refusing to install")
            part.replace(dest)
            return
        except (OSError, TimeoutError):
            if attempt == 2:
                raise
            time.sleep(2)


def install(dry_run: bool = False) -> None:
    missing = prerequisites()
    if missing:
        raise RuntimeError("Missing prerequisites: " + ", ".join(missing)
                           + ". See verification/README.md; system packages are never installed automatically.")
    print("Install SHA256-checked Aeneas/Charon and Lean bundles, pinned Rust components, and Lean dependencies.")
    if dry_run:
        return
    installed, detail = probe()
    if installed:
        # A passing probe already validated the lock digest, every bundled
        # binary, the pinned Rust components, and the Lean dependency cache,
        # so cache-restored tools need no re-extraction. Skipping it keeps
        # the tarballs' footprint and the install time off disk-constrained
        # CI runners.
        print(f"Verification tools already installed ({detail}).")
        return
    TOOLS.mkdir(parents=True, exist_ok=True)
    for name in ("aeneas", "lean"):
        archive = CACHE / "downloads" / ("aeneas.tar.gz" if name == "aeneas" else "lean.tar.zst")
        download(CONFIG[f"{name}_url"], archive, CONFIG[f"{name}_sha256"])
        if name == "aeneas":
            AENEAS.mkdir(exist_ok=True)
            with tarfile.open(archive) as tar:
                tar.extractall(AENEAS, filter="data")
        else:
            # Archive authenticated above; tar supports zstd without a Python extension.
            run(["tar", "--zstd", "-xf", str(archive), "-C", str(TOOLS)])
        # Only the extracted tree is cached; the archive is dead weight once
        # extraction succeeds.
        archive.unlink()
    run(["rustup", "toolchain", "install", CONFIG["rust"], "--profile", "minimal",
         "--component", "rustc-dev", "--component", "llvm-tools", "--component", "rust-src"])
    run([str(LEAN / "bin/lake"), "build", "--help"], log="lake-probe")
    # Uses the committed Lake manifest; never updates dependency revisions during install.
    run([str(LEAN / "bin/lake"), "exe", "cache", "get"], cwd=ROOT / "verification/lean", log="lean-cache")
    paths = [AENEAS / "aeneas", AENEAS / "charon", AENEAS / "charon-driver",
             LEAN / "bin/lean", LEAN / "bin/lake"]
    STAMP.write_text(json.dumps({"lock": digest(LOCK), "binaries": {
        str(p.relative_to(ROOT)): digest(p) for p in paths}}, indent=2) + "\n")
    print("Verification tools installed. Run cargo make verify.")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=["install", "probe"])
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()
    try:
        if args.action == "install":
            install(args.dry_run)
        else:
            ok, detail = probe()
            print(detail)
            return 0 if ok else 1
        return 0
    except (OSError, RuntimeError, subprocess.SubprocessError) as exc:
        print(f"verification toolchain: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
