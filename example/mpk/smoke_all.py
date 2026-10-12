#!/usr/bin/env python3
"""Build every example under example/mpk and smoke-test the resulting .mpk.

For each example the script:
  1. builds the package (`cargo make package`)
     when its external toolchain is available, otherwise marks it SKIP;
  2. starts a disposable real `mudud` server, installs the .mpk over the
     HTTP API, and asserts the app and its procedures are listed;
  3. for the wallet family additionally invokes create_user / deposit /
     balance and checks the returned values.

Basic liveness only — deep functional coverage lives in crates/db-kernel/testing.

Usage:
  python3 example/mpk/smoke_all.py              # build + smoke everything
  python3 example/mpk/smoke_all.py --skip-build # smoke already-built mpks
  python3 example/mpk/smoke_all.py --only wallet,wallet-py
"""
import argparse
import base64
from contextlib import contextmanager
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request

REPO_ROOT = Path(__file__).resolve().parents[2]
EXAMPLE_ROOT = Path(__file__).resolve().parent
WASM_RELEASE = "target/wasm32-wasip2/release"

BUILD_TIMEOUT_SECS = 1800
SERVER_START_TIMEOUT_SECS = 60

# name, build command (None = cargo make package), required tools, mpk path
# (repo-root relative), wallet (app, module) if it is a wallet variant
EXAMPLES = [
    ("app1", None, [], f"{WASM_RELEASE}/mod_0.mpk", None),
    ("key-value", None, [], f"{WASM_RELEASE}/key-value.mpk", None),
    ("tpcc", None, [], f"{WASM_RELEASE}/tpcc.mpk", None),
    ("vote", None, [], f"{WASM_RELEASE}/vote.mpk", None),
    ("wallet", None, [], f"{WASM_RELEASE}/wallet.mpk", ("wallet", "wallet")),
    ("ycsb", None, [], f"{WASM_RELEASE}/ycsb.mpk", None),
    ("wallet-as", None, ["node", "npm"], f"{WASM_RELEASE}/wallet-as.mpk",
     ("wallet-as", "wallet_as")),
    ("wallet-c", None, ["clang", "wasm-tools"], "target/c-guest/wallet-c.mpk",
     ("wallet-c", "wallet_c")),
    ("wallet-cs", None, ["dotnet"], f"{WASM_RELEASE}/wallet-cs.mpk",
     ("wallet-cs", "wallet_cs")),
    ("wallet-go", None, ["tinygo"], "target/go-guest/wallet-go.mpk",
     ("wallet-go", "wallet_go")),
    ("wallet-py", None, ["componentize-py", "wasm-tools"],
     "target/py-guest/wallet-py.mpk", ("wallet-py", "wallet_py")),
]


def free_port():
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


@contextmanager
def mudud_server():
    """A disposable real mudud (Tokio mode) on a temporary data directory."""
    with tempfile.TemporaryDirectory(prefix="mpk-smoke-") as temp:
        root = Path(temp)
        ports = set()
        while len(ports) < 3:
            ports.add(free_port())
        http, tcp, pg = sorted(ports)
        (root / "mudud.toml").write_text(
            f"""mpk_path = {json.dumps(str(root / "mpk"))}
db_path = {json.dumps(str(root / "data"))}
listen_ip = "127.0.0.1"
http_listen_port = {http}
pg_listen_port = {pg}
tcp_listen_port = {tcp}
http_worker_threads = 1
worker_threads = 2
server_mode = "Tokio"
enable_async = true
component_target = "p2"
""",
            encoding="utf-8",
        )
        (root / "mpk").mkdir()
        (root / "data").mkdir()
        with (root / "server.log").open("w+") as log:
            process = subprocess.Popen(
                ["mudud", "serve", "--cfg", str(root / "mudud.toml")],
                cwd=root, stdout=log, stderr=subprocess.STDOUT)
            base = f"http://127.0.0.1:{http}"

            def request(path, body=None):
                data = json.dumps(body).encode() if body is not None else None
                req = urllib.request.Request(
                    base + path, data=data,
                    headers={"Content-Type": "application/json"})
                try:
                    with urllib.request.urlopen(req, timeout=120) as response:
                        value = json.load(response)
                except urllib.error.HTTPError as error:
                    raise RuntimeError(
                        f"{path}: HTTP {error.code}: "
                        f"{error.read().decode()}") from None
                if isinstance(value, dict) and "ok" in value and "data" in value:
                    if not value["ok"]:
                        raise RuntimeError(f"{path}: {value}")
                    return value["data"]
                return value

            try:
                deadline = time.monotonic() + SERVER_START_TIMEOUT_SECS
                while True:
                    if process.poll() is not None:
                        log.seek(0)
                        raise RuntimeError(
                            "mudud exited during startup: " + log.read()[-3000:])
                    try:
                        request("/mudu/app/list")
                        break
                    except (OSError, RuntimeError):
                        if time.monotonic() > deadline:
                            raise RuntimeError(
                                "timed out waiting for mudud") from None
                        time.sleep(0.1)
                yield request
            finally:
                process.terminate()
                try:
                    process.wait(timeout=15)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()


def build_example(name, command):
    env = dict(os.environ, CARGO_INCREMENTAL="0")
    cmd = command if command is not None else ["cargo", "make", "package"]
    started = time.monotonic()
    result = subprocess.run(
        cmd, cwd=EXAMPLE_ROOT / name, env=env,
        stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
        timeout=BUILD_TIMEOUT_SECS)
    elapsed = time.monotonic() - started
    if result.returncode != 0:
        tail = result.stdout.decode(errors="replace")[-3000:]
        raise RuntimeError(f"build command failed ({result.returncode}):\n{tail}")
    return elapsed


def smoke_mpk(name, mpk, wallet):
    with mudud_server() as request:
        blob = base64.b64encode(mpk.read_bytes()).decode()
        request("/mudu/app/install", {"mpk_base64": blob})
        apps = request("/mudu/app/list")
        if not apps:
            raise AssertionError("no apps listed after install")
        counts = {}
        proc_lists = {}
        for app in apps:
            listing = request(f"/mudu/app/list/{app}")
            # The endpoint answers {"app_name": ..., "procedures": [...]}.
            procedures = (
                listing["procedures"] if isinstance(listing, dict) else listing)
            if not procedures:
                raise AssertionError(f"app {app}: no procedures listed")
            counts[app] = len(procedures)
            proc_lists[app] = procedures
        detail = ", ".join(f"{app}({n} procs)" for app, n in counts.items())
        if wallet is not None:
            smoke_wallet(request, *wallet, proc_lists[wallet[0]])
        return detail


def smoke_wallet(request, app, module, procedures):
    def invoke(proc, **params):
        return request(f"/mudu/app/invoke/{app}/{module}/{proc}", params)

    invoke("create_user", user_id=1001, name="Alice", email="alice@example.com")
    invoke("create_user", user_id=1002, name="Bob", email="bob@example.com")
    invoke("deposit", user_id=1001, amount=1000)
    # Procedure list entries are "module/proc"; not every wallet variant
    # exports a balance procedure (the Rust wallet answers balance via SQL).
    exported = {entry.rsplit("/", 1)[-1] for entry in procedures}
    if "balance" not in exported:
        return
    result = invoke("balance", user_id=1001)
    returned = result.get("return_list") if isinstance(result, dict) else None
    # Guest wallets return the i64 balance as [[0, [9, value]]]; variants
    # whose procedures answer with an empty return_list are only checked for
    # a successful invoke above.
    if returned and returned != [[0, [9, 1000]]]:
        raise AssertionError(f"balance: unexpected return {returned}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--skip-build", action="store_true",
                        help="only smoke-test mpks that already exist")
    parser.add_argument("--only", default="",
                        help="comma-separated example names to run")
    args = parser.parse_args()

    only = {item.strip() for item in args.only.split(",") if item.strip()}
    selected = [e for e in EXAMPLES if not only or e[0] in only]
    if not selected:
        print(f"no examples match --only {args.only!r}", file=sys.stderr)
        return 2

    results = []
    for name, command, requires, mpk_rel, wallet in selected:
        mpk = REPO_ROOT / mpk_rel
        missing = [tool for tool in requires if shutil.which(tool) is None]
        if missing and not (args.skip_build and mpk.exists()):
            results.append((name, "SKIP", f"missing toolchain: {', '.join(missing)}"))
            continue
        try:
            if not args.skip_build:
                elapsed = build_example(name, command)
                build_note = f"built in {elapsed:.0f}s"
            else:
                build_note = "build skipped"
            if not mpk.exists():
                raise RuntimeError(f"mpk not found at {mpk_rel}")
            detail = smoke_mpk(name, mpk, wallet)
            results.append((name, "PASS", f"{build_note}; {detail}"))
        except Exception as error:  # noqa: BLE001 - report and continue
            results.append((name, "FAIL", str(error)))

    width = max(len(name) for name, _, _ in results)
    print("\n================ mpk smoke summary ================")
    for name, status, note in results:
        first, _, rest = note.partition("\n")
        print(f"{name:<{width}}  {status:<4}  {first}")
        if rest:
            print(f"{'':<{width}}  {'':<4}  {rest}")
    failed = [name for name, status, _ in results if status == "FAIL"]
    print(f"\n{sum(1 for _, s, _ in results if s == 'PASS')} passed, "
          f"{sum(1 for _, s, _ in results if s == 'SKIP')} skipped, "
          f"{len(failed)} failed")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
