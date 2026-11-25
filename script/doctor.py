#!/usr/bin/env python3
"""Probe the development toolchain and report what each tool unlocks.

Runs as `cargo make doctor`. The bare command is a read-only report and
always exits 0. Each missing entry shows a copy-pasteable shell command to
install the tool, a descriptive note (version constraints, in-repo readme
pointers, ...), and a clickable hyperlink to the tool's install documentation
(OSC 8 on terminals, bare URL when piped). The header names the shell the
commands are written for (from $SHELL on Linux/macOS, PowerShell/cmd on
Windows).

The `install` subcommand installs missing tools, restricted to user-level,
deterministic recipes (rustup/cargo/pip/pipx and pinned verification
bundles) — it never uses sudo and never
touches system packages; tools without such a recipe are reported with their
manual install hint instead.

Install commands and notes are OS-specific (Linux / macOS / Windows). Set
DOCTOR_OS to linux|macos|windows to preview another platform's hints.

  doctor.py                      # read-only report
  doctor.py install --dry-run    # print the install plan only
  doctor.py install              # install missing user-level tools
  doctor.py install --only wasm-tools,nightly
"""
import argparse
import os
import platform as platform_module
import re
import shutil
import subprocess
import sys
import tomllib
from pathlib import Path
from typing import Callable, Dict, List, Optional, Tuple

REPO_ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO_ROOT / "script" / "lib"))
from cli import normalized_argv  # noqa: E402
from output import (  # noqa: E402
    bad,
    bold,
    cmd as echo_cmd,
    dim,
    error,
    fail,
    good,
    highlight,
    link,
    status,
    warn,
)
from wasi_sdk import resolve_wasi_sdk  # noqa: E402

Probe = Callable[[], Tuple[bool, str]]
Hints = Dict[str, str]  # "linux" / "macos" / "windows"


def per_os(linux: str, macos: Optional[str] = None, windows: Optional[str] = None) -> Hints:
    """Per-OS text; macOS/Windows default to the Linux one, "" hides it."""
    return {
        "linux": linux,
        "macos": macos if macos is not None else linux,
        "windows": windows if windows is not None else linux,
    }


def platform_key() -> str:
    override = os.environ.get("DOCTOR_OS")
    if override:
        return override
    return {"Linux": "linux", "Darwin": "macos", "Windows": "windows"}.get(
        platform_module.system(), "linux"
    )


def shell_name(os_key: str) -> str:
    """The shell the printed install commands are written for."""
    if os.environ.get("DOCTOR_OS"):
        # Previewing another platform: name its assumed shell, not ours.
        return {"linux": "bash", "macos": "zsh", "windows": "PowerShell / cmd"}.get(
            os_key, "POSIX sh"
        )
    if os_key == "windows":
        return "PowerShell / cmd"
    shell = os.environ.get("SHELL", "")
    return Path(shell).name if shell else "POSIX sh"


class Entry:
    def __init__(
        self,
        entry_id: str,
        name: str,
        required: bool,
        probe: Probe,
        unlocks: str,
        cmd: Optional[Hints] = None,
        note: Optional[Hints] = None,
        docs: Optional[str] = None,
        install: Optional[List[List[str]]] = None,
    ):
        self.id = entry_id
        self.name = name
        self.required = required
        self.probe = probe
        self.unlocks = unlocks
        # Per-OS shell command that installs the tool ("" = no single command).
        self.cmd = cmd or {}
        # Per-OS descriptive note shown next to the command, or as the
        # install hint when there is no command.
        self.note = note or {}
        # URL of the tool's install documentation, shown as a hyperlink.
        self.docs = docs
        # User-level install recipe (list of argv commands), or None when the
        # install is platform/system-specific and stays a manual hint.
        self.install = install

    def cmd_for(self, os_key: str) -> str:
        # per_os fills all three keys; an explicit "" must survive (no
        # command/hint on that platform), so only fall back for unknown keys.
        value = self.cmd.get(os_key)
        return self.cmd.get("linux", "") if value is None else value

    def note_for(self, os_key: str) -> str:
        value = self.note.get(os_key)
        return self.note.get("linux", "") if value is None else value


def docs_link(url: str) -> str:
    """Hyperlink with a short scheme-less label: `link('rustup.rs', url)`."""
    label = re.sub(r"^https?://", "", url).rstrip("/")
    return link(label, url)


def print_missing_lines(entry: Entry, os_key: str, indent: str = "      ") -> None:
    """Print the install command, descriptive note, and doc link of a
    missing entry."""
    cmd_text = entry.cmd_for(os_key)
    note_text = entry.note_for(os_key)
    if cmd_text:
        # No `$` prompt or trailing comment: the line must paste into a
        # shell and run as-is.
        print(f"{indent}{highlight(cmd_text)}")
    if note_text:
        label = "note:" if cmd_text else "install:"
        print(f"{indent}{dim(label.ljust(8))} {note_text}")
    if entry.docs:
        print(f"{indent}{dim('docs:'.ljust(8))} {docs_link(entry.docs)}")


def which(name: str) -> Optional[str]:
    return shutil.which(name)


def version_of(cmd: List[str], timeout: int = 15) -> str:
    try:
        proc = subprocess.run(cmd, text=True, capture_output=True, timeout=timeout)
    except (OSError, subprocess.TimeoutExpired):
        return ""
    out = (proc.stdout or proc.stderr).strip()
    return out.splitlines()[0] if out else ""


def tool_probe(name: str, version_args=("--version",)) -> Probe:
    def probe() -> Tuple[bool, str]:
        found = which(name)
        return (bool(found), version_of([found, *version_args]) if found else "")

    return probe


def rustup_toolchains() -> List[str]:
    try:
        proc = subprocess.run(["rustup", "toolchain", "list"], text=True, capture_output=True, timeout=15)
        return proc.stdout.splitlines()
    except (OSError, subprocess.TimeoutExpired):
        return []


def pinned_channel() -> str:
    return tomllib.loads((REPO_ROOT / "rust-toolchain.toml").read_text())["toolchain"]["channel"]


def pinned_nightly() -> str:
    return (REPO_ROOT / ".rust-nightly-version").read_text(encoding="utf-8").strip()


def rustc_probe() -> Tuple[bool, str]:
    if not which("rustc"):
        return False, ""
    installed = version_of(["rustc", "--version"])
    channel = pinned_channel()
    match = " (matches rust-toolchain.toml)" if channel in installed else f" (rust-toolchain.toml pins {channel}!)"
    return True, installed + match


def wasm_target_probe() -> Tuple[bool, str]:
    try:
        proc = subprocess.run(
            ["rustup", "target", "list", "--installed"], text=True, capture_output=True, timeout=15
        )
        return "wasm32-wasip2" in proc.stdout.splitlines(), "rustup target"
    except (OSError, subprocess.TimeoutExpired):
        return False, ""


def nightly_probe() -> Tuple[bool, str]:
    pin = pinned_nightly()
    installed = any(line.startswith(pin) for line in rustup_toolchains())
    return installed, f"pinned {pin}"


def dotnet_probe() -> Tuple[bool, str]:
    candidates = []
    on_path = which("dotnet")
    if on_path:
        candidates.append(on_path)
    fallback = str(Path.home() / ".dotnet" / "dotnet")
    if fallback != on_path and Path(fallback).is_file():
        candidates.append(fallback)
    for dotnet in candidates:
        try:
            proc = subprocess.run([dotnet, "--list-sdks"], text=True, capture_output=True, timeout=15)
        except (OSError, subprocess.TimeoutExpired):
            continue
        majors = [v for v in proc.stdout.split() if re.fullmatch(r"\d+\..*", v)]
        if any(v.startswith("10.") for v in majors):
            location = "" if dotnet == on_path else f" ({dotnet}; not on PATH)"
            return True, ", ".join(majors[:4]) + location
    return False, ""


def wasi_sdk_probe() -> Tuple[bool, str]:
    try:
        return True, str(resolve_wasi_sdk())
    except RuntimeError:
        return False, ""


def pipx_bin(name: str) -> Optional[str]:
    # pipx installs entry points into ~/.local/bin, which may not be on PATH.
    path = Path.home() / ".local" / "bin" / name
    return str(path) if path.is_file() else None


def twine_probe() -> Tuple[bool, str]:
    found = which("twine") or pipx_bin("twine")
    if found:
        return True, version_of([found, "--version"])
    proc = subprocess.run(
        [sys.executable, "-m", "twine", "--version"], text=True, capture_output=True, timeout=15
    )
    return proc.returncode == 0, "python module" if proc.returncode == 0 else ""


def py_build_probe() -> Tuple[bool, str]:
    # Probe by execution: some distributions ship a `build` namespace package
    # without __main__, which importlib would report as present.
    try:
        proc = subprocess.run(
            [sys.executable, "-m", "build", "--version"], text=True, capture_output=True, timeout=15
        )
        return proc.returncode == 0, proc.stdout.strip() if proc.returncode == 0 else ""
    except (OSError, subprocess.TimeoutExpired):
        return False, ""


def pip_user_allowed() -> bool:
    # PEP 668: externally managed interpreters refuse pip --user installs.
    import sysconfig

    return not (Path(sysconfig.get_path("stdlib")) / "EXTERNALLY-MANAGED").exists()


def go_probe() -> Tuple[bool, str]:
    go = which("go")
    return (bool(go), version_of([go, "version"]) if go else "")


def llvm_cov_probe() -> Tuple[bool, str]:
    return (bool(which("cargo-llvm-cov")), version_of(["cargo", "llvm-cov", "--version"]))


def cargo_make_probe() -> Tuple[bool, str]:
    return (bool(which("cargo-make")), version_of(["cargo", "make", "--version"]))


def componentize_py_install() -> Optional[List[List[str]]]:
    # pipx is the documented install channel (example/mpk/wallet-py/readme.md).
    if which("pipx"):
        return [["pipx", "install", "componentize-py==0.25.1"]]
    return None


def formal_verification_entry() -> Entry:
    from verification.toolchain import probe

    supported = platform_module.system() == "Linux" and platform_module.machine() in ("x86_64", "AMD64")
    # DOCTOR_OS is a report preview, never authorization to install for another OS.
    preview = bool(os.environ.get("DOCTOR_OS"))
    recipe = [[sys.executable, str(REPO_ROOT / "script/verification/toolchain.py"), "install"]]
    return Entry(
        "formal-verification", "Aeneas + Charon + Lean (pinned)", False, probe,
        "cargo make verify / verify-extract / verify-self-test",
        cmd=per_os("cargo make doctor -- install --only formal-verification", macos="", windows=""),
        note=per_os("Linux x86_64 / WSL2; prerequisites and troubleshooting: verification/README.md",
                    macos="Not supported in v1; use Linux x86_64",
                    windows="Run inside an x86_64 WSL2 Linux distribution"),
        docs="https://github.com/AeneasVerif/aeneas",
        install=recipe if supported and not preview else None,
    )


def collect_entries() -> List[Entry]:
    return [
        formal_verification_entry(),
        Entry("cargo", "cargo", True, tool_probe("cargo"), "all builds",
              cmd=per_os("curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh",
                         windows="winget install Rustlang.Rustup"),
              docs="https://rustup.rs/"),
        Entry("rustc", "rustc (pinned)", True, rustc_probe, "all builds",
              cmd=per_os(f"rustup toolchain install {pinned_channel()}"),
              note=per_os("channel pinned in rust-toolchain.toml"),
              docs="https://rust-lang.github.io/rustup/overrides.html"),
        Entry("cargo-make", "cargo-make", True, cargo_make_probe, "cargo make <task>",
              cmd=per_os("cargo install cargo-make --locked"),
              docs="https://github.com/sagiegurari/cargo-make#installation",
              install=[["cargo", "install", "cargo-make", "--locked"]]),
        Entry("python3", "python3", True, tool_probe("python3"), "all script automation",
              cmd=per_os("sudo apt install python3",
                         macos="brew install python@3.12",
                         windows="winget install Python.Python.3.12"),
              note=per_os("Debian/Ubuntu; other distros via their package manager",
                          macos="", windows="or the python.org installer"),
              docs="https://www.python.org/downloads/"),
        Entry("wasm-target", "wasm32-wasip2 target", True, wasm_target_probe, "guest examples / mpk packages",
              cmd=per_os("rustup target add wasm32-wasip2"),
              docs="https://doc.rust-lang.org/rustc/platform/wasm32-wasip2.html",
              install=[["rustup", "target", "add", "wasm32-wasip2"]]),
        Entry("node", "node", False, tool_probe("node"), "wallet-as example",
              cmd=per_os("nvm install 22",
                         macos="brew install node@22",
                         windows="winget install OpenJS.NodeJS.LTS"),
              note=per_os("requires nvm (github.com/nvm-sh/nvm); or the nodejs.org installer",
                          macos="", windows=""),
              docs="https://nodejs.org/en/download"),
        Entry("dotnet", "dotnet (.NET 10)", False, dotnet_probe, "wallet-cs example",
              cmd=per_os("curl -sSL https://dot.net/v1/dotnet-install.sh | bash -s -- --channel 10.0",
                         windows="winget install Microsoft.DotNet.SDK.10"),
              note=per_os("installs into ~/.dotnet; details: example/mpk/wallet-cs/readme.md"),
              docs="https://dotnet.microsoft.com/en-us/download"),
        Entry("componentize-py", "componentize-py", False, tool_probe("componentize-py"), "wallet-py example",
              cmd=per_os("pipx install componentize-py==0.25.1"),
              docs="https://github.com/bytecodealliance/componentize-py",
              install=componentize_py_install()),
        Entry("wasm-tools", "wasm-tools", False, tool_probe("wasm-tools"), "wallet-py / wallet-go / wallet-c examples",
              cmd=per_os("cargo install wasm-tools --locked"),
              docs="https://github.com/bytecodealliance/wasm-tools",
              install=[["cargo", "install", "wasm-tools", "--locked"]]),
        Entry("tinygo", "tinygo", False, tool_probe("tinygo"), "wallet-go example",
              cmd=per_os("", macos="brew install tinygo", windows="winget install TinyGo.TinyGo"),
              note=per_os("TinyGo 0.42 tarball from the releases page; extract and put tinygo/bin on PATH",
                          macos="check that it is 0.42", windows=""),
              docs="https://tinygo.org/getting-started/install/"),
        Entry("go", "go", False, go_probe, "wallet-go example",
              cmd=per_os("", macos="brew install go", windows="winget install GoLang.Go"),
              note=per_os("Go 1.27 tarball from go.dev/dl (distro packages are often older)",
                          macos="", windows=""),
              docs="https://go.dev/doc/install"),
        Entry("wasi-sdk", "wasi-sdk clang", False, wasi_sdk_probe, "wallet-c example",
              note=per_os("wasi-sdk 29 tarball from the releases page, unpacked to ~/.wasi-sdk "
                          "(or on PATH / WASI_SDK); details: example/mpk/wallet-c/readme.md"),
              docs="https://github.com/WebAssembly/wasi-sdk"),
        Entry("llvm-cov", "cargo-llvm-cov", False, llvm_cov_probe, "cargo make coverage",
              cmd=per_os("cargo install cargo-llvm-cov --locked"),
              docs="https://github.com/taiki-e/cargo-llvm-cov",
              install=[["cargo", "install", "cargo-llvm-cov", "--locked"]]),
        Entry("nightly", "nightly toolchain (pinned)", False, nightly_probe, "cargo make coverage / miri",
              cmd=per_os(f"rustup toolchain install {pinned_nightly()}"),
              docs="https://rust-lang.github.io/rustup/concepts/toolchains.html",
              install=[["rustup", "toolchain", "install", pinned_nightly()]]),
        Entry("act", "act", False, tool_probe("act"), "run GitHub Actions locally (script/ci/act.sh)",
              cmd=per_os("curl -s https://raw.githubusercontent.com/nektos/act/master/install.sh | sudo bash",
                         macos="brew install act",
                         windows="winget install nektos.act"),
              docs="https://github.com/nektos/act"),
        Entry("twine", "twine", False, twine_probe, "script/release/publish.py pypi",
              cmd=per_os("pipx install twine"),
              note=per_os("or use a venv"),
              docs="https://twine.readthedocs.io/en/stable/"),
        Entry("py-build", "python -m build", False, py_build_probe, "cargo make package-sdk-registry -- pypi",
              cmd=per_os("python3 -m venv .venv && .venv/bin/pip install build",
                         windows="py -m venv .venv && .venv\\Scripts\\pip install build"),
              note=per_os("then RELEASE_PYTHON=.venv/bin/python",
                          windows="then set RELEASE_PYTHON=.venv\\Scripts\\python"),
              docs="https://pypa-build.readthedocs.io/en/latest/",
              install=[[sys.executable, "-m", "pip", "install", "--user", "build"]] if pip_user_allowed() else None),
    ]


def report(entries: List[Tuple[Entry, bool, str]], os_key: str) -> Tuple[int, int]:
    name_width = max(len(entry.name) for entry, _, _ in entries)
    missing_required = 0
    missing_optional = 0
    sections = (
        ("Required", "core build / check / test", True),
        ("Optional", "examples, e2e, coverage, release", False),
    )
    for title, subtitle, required in sections:
        group = [(entry, ok, detail) for entry, ok, detail in entries if entry.required == required]
        found = sum(1 for _, ok, _ in group if ok)
        print()
        print(bold(f"{title} — {subtitle} ") + dim(f"({found}/{len(group)} found)"))
        for entry, ok, detail in group:
            if ok:
                mark = good("✓")
            elif required:
                mark = fail("✗")
                missing_required += 1
            else:
                mark = bad("!")
                missing_optional += 1
            print(f"  {mark} {entry.name.ljust(name_width)}  {detail or 'not found'}"
                  + dim(f" — {entry.unlocks}"))
            if not ok:
                print_missing_lines(entry, os_key)
    print()
    return missing_required, missing_optional


def cmd_report() -> int:
    os_key = platform_key()
    print()
    print(bold("MuduDB toolchain doctor"))
    print(f"  {dim('platform:')} {os_key}")
    print(f"  {dim('shell:')}    {highlight(shell_name(os_key))}"
          + dim("  — the install commands below are written for this shell"))
    print(f"  {dim('pinned:')}   rust {pinned_channel()} (rust-toolchain.toml)"
          f" · nightly {pinned_nightly()} (.rust-nightly-version)")
    if os.environ.get("DOCTOR_OS"):
        print(dim(f"  (previewing {os_key} hints via DOCTOR_OS)"))
    entries = [(entry, *entry.probe()) for entry in collect_entries()]
    missing_required, missing_optional = report(entries, os_key)
    if missing_required:
        status("Doctor", f"{missing_required} required tool(s) missing; install them first")
    elif missing_optional:
        status("Doctor", f"core toolchain ready; {missing_optional} optional tool(s) missing (see above)")
    else:
        status("Doctor", "all tools present")
    if missing_required or missing_optional:
        print(dim("install the user-level ones automatically:  cargo make doctor -- install [--dry-run]"))
    return 0


def cmd_install(argv: List[str]) -> int:
    parser = argparse.ArgumentParser(description="Install missing user-level tools (no sudo).")
    parser.add_argument("--only", default=None, help="Comma-separated entry ids to install.")
    parser.add_argument("--dry-run", action="store_true", help="Print the install plan only.")
    args = parser.parse_args(argv)

    os_key = platform_key()
    probed = [(entry, *entry.probe()) for entry in collect_entries()]
    missing = [(entry, detail) for entry, ok, detail in probed if not ok]
    if args.only:
        wanted = args.only.split(",")
        known = {entry.id for entry, _, _ in probed}
        unknown = [item for item in wanted if item not in known]
        if unknown:
            error(f"unknown ids: {', '.join(unknown)} (known: {', '.join(sorted(known)) or 'none'})")
            return 2
        missing = [(entry, detail) for entry, detail in missing if entry.id in wanted]

    installable = [(entry, detail) for entry, detail in missing if entry.install]
    manual = [(entry, detail) for entry, detail in missing if not entry.install]

    status("Plan", f"{len(installable)} to install, {len(manual)} manual")
    for entry, _detail in installable:
        for command in entry.install:
            echo_cmd(command)
    for entry, _detail in manual:
        print(dim(f"  manual: {entry.name}"))
        print_missing_lines(entry, os_key, indent="          ")
    if args.dry_run or not installable:
        return 0

    failed = 0
    for entry, _detail in installable:
        command_failed = False
        for command in entry.install:
            status("Installing", entry.name)
            echo_cmd(command)
            if subprocess.run(command, text=True).returncode != 0:
                command_failed = True
        # The probe is the source of truth: some installers exit non-zero
        # from a post-install action after the tool itself landed.
        if entry.probe()[0]:
            if command_failed:
                warn(f"{entry.name}: an installer command exited non-zero, but the tool is now available")
            status("Installed", entry.name)
        else:
            failed += 1
            error(f"failed to install {entry.name} ({entry.cmd_for(os_key) or entry.note_for(os_key)})")

    if manual:
        print(dim("still manual: " + ", ".join(entry.name for entry, _ in manual)))
    if failed:
        error(f"{failed} install(s) failed")
        return 1
    status("Finished", f"{len(installable)} installed, {len(manual)} need manual install")
    return 0


def main() -> int:
    argv = normalized_argv()
    if argv and argv[0] == "install":
        return cmd_install(argv[1:])
    return cmd_report()


if __name__ == "__main__":
    sys.exit(main())
