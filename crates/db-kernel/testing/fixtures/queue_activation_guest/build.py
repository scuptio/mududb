#!/usr/bin/env python3
"""Rebuild the real queue activation MPK using the repository tools."""
from pathlib import Path
import subprocess
import tempfile

fixture = Path(__file__).resolve().parent
root = fixture.parents[4]
package = fixture / "package"


def run(*args):
    subprocess.run([str(arg) for arg in args], cwd=root, check=True)


def tool(binary, *args):
    run("cargo", "run", "--manifest-path", root / "crates/tools/Cargo.toml",
        "--bin", binary, "--", *args)


tool("mgen", "check-sql", "--lang", "rust", "--ddl", package / "ddl.sql",
     "--input", fixture / "src/procedure.rs")
tool("mtp", "--input", fixture / "src/procedure.rs", "--output",
     fixture / "src/generated.rs", "--module", "queue_activation_guest",
     "--src-mod", "procedure", "--dst-mod", "generated", "--async",
     "--package-desc", package / "package.desc.json", "rust")
run("cargo", "fmt", "--manifest-path", fixture / "Cargo.toml")
run("cargo", "build", "--locked", "--manifest-path", fixture / "Cargo.toml",
    "--target", "wasm32-wasip2")
with tempfile.TemporaryDirectory(prefix="queue-activation-guest-") as temp:
    wasm = Path(temp) / "queue_activation_guest.wasm"
    run("wasm-tools", "strip", root / "target/wasm32-wasip2/debug/queue_activation_guest.wasm",
        "-o", wasm)
    tool("mpm-build", "create", "--package-cfg", package / "package.cfg.json",
         "--package-desc", package / "package.desc.json", "--ddl-sql", package / "ddl.sql",
         "--initdb-sql", package / "init.sql", "--wasm-files", wasm,
         "--output", fixture.parents[1] / "mpk/queue_activation_guest.mpk")
