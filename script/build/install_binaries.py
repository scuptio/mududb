#!/usr/bin/env python3
import argparse
import json
import os
import platform
import shutil
import subprocess
import sys
import time
import tomllib
from pathlib import Path
from typing import Dict, List, Optional, Tuple

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "lib"))
from cli import normalized_argv  # noqa: E402
from output import cmd as echo_cmd  # noqa: E402
from output import dim, error, format_duration, highlight, status, status_kind, warn  # noqa: E402


def run(cmd: List[str], cwd: Path) -> subprocess.CompletedProcess:
    return subprocess.run(cmd, cwd=str(cwd), text=True, capture_output=True)


SUPPORTED_BINS: List[Dict[str, str]] = [
    {"package": "mpm_build", "bin": "mpm", "description": "portable project builder"},
    {
        "package": "mpm_build",
        "bin": "mpm-build",
        "description": "package builder",
    },
    {
        "package": "mpm_crate",
        "bin": "mpm-crate",
        "description": "project scaffolder",
    },
    {
        "package": "mpm_install",
        "bin": "mpm-install",
        "description": "package installer",
    },
    {
        "package": "mudu_gen",
        "bin": "mgen",
        "description": "source generator",
    },
    {
        "package": "mudu_transpiler",
        "bin": "mtp",
        "description": "transpiler",
    },
    {
        "package": "mudud",
        "bin": "mudud",
        "description": "MuduDB server",
    },
    {
        "package": "mudu_cli",
        "bin": "mcli",
        "description": "TCP protocol client CLI",
    },
]


# The standalone cdylib workspace for the native adapter ABI. It builds into
# target/sdk-adapter (its local .cargo/config.toml), never the shared target/.
ADAPTER_FFI_CRATE_DIR = Path("crates/bindings/mudu_adapter_ffi")
ADAPTER_FFI_HEADER = ADAPTER_FFI_CRATE_DIR / "include" / "mudu_adapter.h"
ADAPTER_FFI_TARGET_DIR = Path("target/sdk-adapter")
ADAPTER_FFI_LIB_NAMES = {
    "Linux": "libmudu_adapter_ffi.so",
    "Darwin": "libmudu_adapter_ffi.dylib",
    "Windows": "mudu_adapter_ffi.dll",
}


def bin_file_name(bin_name: str) -> str:
    return bin_name + (".exe" if os.name == "nt" else "")


def format_size(size: Optional[int]) -> str:
    if size is None:
        return "-"
    if size >= 1024 * 1024:
        return f"{size / (1024 * 1024):.1f} MiB"
    if size >= 1024:
        return f"{size / 1024:.1f} KiB"
    return f"{size} B"


def probe_version(path: Path) -> str:
    try:
        proc = subprocess.run(
            [str(path), "--version"], text=True, capture_output=True, timeout=15
        )
    except (OSError, subprocess.TimeoutExpired):
        return "-"
    output = (proc.stdout or proc.stderr).strip()
    if proc.returncode != 0 or not output:
        return "-"
    return output.splitlines()[0]


def resolve_install_dirs(install_root: str | None) -> Tuple[Path, Path, Path]:
    """Match cargo install's layout: bins under <root>/bin; libs/headers alongside."""
    if install_root:
        base = Path(install_root).resolve()
    else:
        cargo_home = os.environ.get("CARGO_HOME", str(Path.home() / ".cargo"))
        base = Path(cargo_home).resolve()
    return base / "bin", base / "lib", base / "include"


def adapter_ffi_version(crate_dir: Path) -> str:
    try:
        data = tomllib.loads((crate_dir / "Cargo.toml").read_text(encoding="utf-8"))
        return str(data["package"]["version"])
    except (OSError, KeyError, tomllib.TOMLDecodeError):
        return "-"


def make_result(
    name: str, status: str, path: Path, version: str = "-", size: Optional[int] = None
) -> Dict[str, object]:
    return {
        "name": name,
        "status": status,
        "path": path,
        "version": version,
        "size": size,
    }


def load_workspace_metadata(workspace_root: Path) -> dict:
    cmd = ["cargo", "metadata", "--format-version", "1", "--no-deps"]
    proc = run(cmd, workspace_root)
    if proc.returncode != 0:
        raise RuntimeError(
            "Failed to run cargo metadata:\n"
            f"STDOUT:\n{proc.stdout}\n"
            f"STDERR:\n{proc.stderr}"
        )
    return json.loads(proc.stdout)


def find_group_workspaces(repo_root: Path) -> List[Path]:
    """Return the independent group workspaces under crates/."""
    groups = []
    for group in ("common", "db-kernel", "sdk", "tools"):
        group_dir = repo_root / "crates" / group
        if (group_dir / "Cargo.toml").exists():
            groups.append(group_dir)
    return groups


def load_workspace_bins(workspace_roots: List[Path]) -> List[Tuple[Path, str]]:
    bins: List[Tuple[Path, str]] = []
    seen = set()

    for workspace_root in workspace_roots:
        metadata = load_workspace_metadata(workspace_root)
        workspace_members = set(metadata.get("workspace_members", []))
        for package in metadata.get("packages", []):
            if package.get("id") not in workspace_members:
                continue

            manifest_path = Path(package["manifest_path"]).resolve()
            package_dir = manifest_path.parent

            for target in package.get("targets", []):
                kinds = set(target.get("kind", []))
                if "bin" not in kinds:
                    continue

                bin_name = target["name"]
                key = (str(package_dir), bin_name)
                if key in seen:
                    continue

                seen.add(key)
                bins.append((package_dir, bin_name))

    bins.sort(key=lambda x: (str(x[0]).lower(), x[1]))
    return bins


def load_supported_bins(workspace_roots: List[Path]) -> List[Tuple[Path, str, str]]:
    package_index = {}

    for workspace_root in workspace_roots:
        metadata = load_workspace_metadata(workspace_root)
        workspace_members = set(metadata.get("workspace_members", []))
        for package in metadata.get("packages", []):
            if package.get("id") not in workspace_members:
                continue
            package_index[package["name"]] = package

    bins: List[Tuple[Path, str, str]] = []
    for item in SUPPORTED_BINS:
        package = package_index.get(item["package"])
        if package is None:
            raise RuntimeError(f"Workspace package not found: {item['package']}")

        bin_name = item["bin"]
        target_names = {
            target["name"]
            for target in package.get("targets", [])
            if "bin" in set(target.get("kind", []))
        }
        if bin_name not in target_names:
            raise RuntimeError(
                f"Binary target '{bin_name}' not found in package '{item['package']}'"
            )

        manifest_path = Path(package["manifest_path"]).resolve()
        bins.append((manifest_path.parent, bin_name, item["description"]))

    return bins


def same_file_content(a: Path, b: Path) -> bool:
    if not a.exists() or not b.exists():
        return False
    if a.stat().st_size != b.stat().st_size:
        return False
    with open(a, "rb") as fa, open(b, "rb") as fb:
        while True:
            ca = fa.read(1 << 20)
            cb = fb.read(1 << 20)
            if ca != cb:
                return False
            if not ca:
                return True


def install_bins(
    workspace_root: Path,
    bins: List[Tuple[Path, str, str]],
    profile: str,
    bin_dir: Path,
    dry_run: bool,
) -> Tuple[List[Dict[str, object]], int]:
    results: List[Dict[str, object]] = []
    if not bins:
        print("No workspace binaries found.")
        return results, 0

    status("Plan", f"{len(bins)} binaries (profile={profile})")
    for package_dir, bin_name, description in bins:
        rel_dir = package_dir.relative_to(workspace_root)
        print(f"  - {highlight(bin_name)} ({rel_dir})")
        print(dim(f"    {description}"))

    # Resolve each package's workspace root and package name (one metadata
    # call per package directory, cached).
    pkg_meta: Dict[Path, Tuple[Path, str, Path]] = {}
    for package_dir in {package_dir for package_dir, _, _ in bins}:
        metadata = load_workspace_metadata(package_dir)
        ws_root = Path(metadata["workspace_root"]).resolve()
        target_dir = Path(metadata["target_directory"]).resolve()
        pkg_name = None
        for package in metadata.get("packages", []):
            if Path(package["manifest_path"]).resolve().parent == package_dir:
                pkg_name = package["name"]
                break
        if pkg_name is None:
            raise RuntimeError(f"package manifest not found in metadata: {package_dir}")
        pkg_meta[package_dir] = (ws_root, pkg_name, target_dir)

    # One cargo build per workspace. cargo build is fingerprint-incremental:
    # on a fresh tree it is a sub-second no-op, after which installing is a
    # file copy. (The previous `cargo install --force` relinked every binary
    # unconditionally, costing tens of seconds per package on every run.)
    groups: List[Tuple[Path, Path, List[str], List[str]]] = []
    for package_dir, bin_name, _description in bins:
        ws_root, pkg_name, target_dir = pkg_meta[package_dir]
        if groups and groups[-1][0] == ws_root:
            groups[-1][2].append(pkg_name)
            groups[-1][3].append(bin_name)
        else:
            groups.append((ws_root, target_dir, [pkg_name], [bin_name]))

    profile_dir = "debug" if profile == "dev" else profile

    def mark_group_failed(group_index: int, bin_names: List[str]) -> None:
        for bin_name in bin_names:
            results.append(
                make_result(bin_name, "failed", bin_dir / bin_file_name(bin_name))
            )
        for _ws, _td, _pkgs, skipped_names in groups[group_index + 1 :]:
            for skipped_name in skipped_names:
                results.append(
                    make_result(
                        skipped_name,
                        "skipped",
                        bin_dir / bin_file_name(skipped_name),
                    )
                )

    for group_index, (ws_root, target_dir, pkg_names, bin_names) in enumerate(groups):
        cmd = ["cargo", "build", "--locked", "--profile", profile, "--bins"]
        for pkg_name in pkg_names:
            cmd.extend(["-p", pkg_name])
        status("Installing", highlight(" ".join(bin_names)))
        echo_cmd(cmd)
        if dry_run:
            for bin_name in bin_names:
                print(
                    dim(
                        f"  would copy target/{profile_dir}/{bin_file_name(bin_name)}"
                        f" -> {bin_dir / bin_file_name(bin_name)}"
                    )
                )
                results.append(
                    make_result(bin_name, "planned", bin_dir / bin_file_name(bin_name))
                )
            continue

        proc = subprocess.run(cmd, cwd=str(ws_root), text=True)
        if proc.returncode != 0:
            error(f"failed to build: {' '.join(bin_names)}")
            mark_group_failed(group_index, bin_names)
            return results, proc.returncode

        bin_dir.mkdir(parents=True, exist_ok=True)
        for bin_name in bin_names:
            artifact = target_dir / profile_dir / bin_file_name(bin_name)
            installed_path = bin_dir / bin_file_name(bin_name)
            if not artifact.exists():
                error(f"built artifact not found: {artifact}")
                results.append(make_result(bin_name, "failed", installed_path))
                continue
            if same_file_content(artifact, installed_path):
                results.append(
                    make_result(
                        bin_name,
                        "unchanged",
                        installed_path,
                        version=probe_version(installed_path),
                        size=installed_path.stat().st_size,
                    )
                )
                continue
            shutil.copy2(artifact, installed_path)
            results.append(
                make_result(
                    bin_name,
                    "installed",
                    installed_path,
                    version=probe_version(installed_path),
                    size=installed_path.stat().st_size,
                )
            )

    return results, 0


def install_adapter_ffi(
    workspace_root: Path,
    profile: str,
    lib_dir: Path,
    include_dir: Path,
    dry_run: bool,
) -> Tuple[List[Dict[str, object]], int]:
    system = platform.system()
    lib_name = ADAPTER_FFI_LIB_NAMES.get(system)
    if lib_name is None:
        error(f"unsupported platform for the adapter library: {system}")
        return [], 1

    crate_dir = workspace_root / ADAPTER_FFI_CRATE_DIR
    header_src = workspace_root / ADAPTER_FFI_HEADER
    profile_dir = "release" if profile == "release" else "debug"
    artifact = workspace_root / ADAPTER_FFI_TARGET_DIR / profile_dir / lib_name
    lib_dest = lib_dir / lib_name
    header_dest = include_dir / "mudu_adapter.h"
    version = adapter_ffi_version(crate_dir)

    build_cmd = ["cargo", "build", "--locked"]
    if profile == "release":
        build_cmd.append("--release")

    status("Building", f"adapter library {highlight(lib_name)}")
    echo_cmd(build_cmd)
    if dry_run:
        print(dim(f"  would copy {artifact} -> {lib_dest}"))
        print(dim(f"  would copy {header_src} -> {header_dest}"))
        return [
            make_result(lib_name, "planned", lib_dest, version=version),
            make_result("mudu_adapter.h", "planned", header_dest),
        ], 0

    proc = subprocess.run(build_cmd, cwd=str(crate_dir), text=True)
    if proc.returncode != 0:
        error("failed to build the adapter library (mudu_adapter_ffi)")
        return [make_result(lib_name, "failed", lib_dest, version=version)], proc.returncode

    if not artifact.exists():
        error(f"adapter library artifact not found: {artifact}")
        return [make_result(lib_name, "failed", lib_dest, version=version)], 1

    lib_dir.mkdir(parents=True, exist_ok=True)
    include_dir.mkdir(parents=True, exist_ok=True)
    shutil.copy2(artifact, lib_dest)
    shutil.copy2(header_src, header_dest)
    return [
        make_result(
            lib_name,
            "installed",
            lib_dest,
            version=version,
            size=lib_dest.stat().st_size,
        ),
        make_result(
            "mudu_adapter.h",
            "installed",
            header_dest,
            size=header_dest.stat().st_size,
        ),
    ], 0


def print_summary(results: List[Dict[str, object]], profile: str) -> None:
    if not results:
        return
    status("Summary", f"install (profile={profile})")
    name_width = max(len(str(row["name"])) for row in results)
    name_width = max(name_width, len("NAME"))
    header = (
        f"  {'NAME':<{name_width}}  {'STATUS':<9}  {'SIZE':>9}  "
        f"{'VERSION':<24}  PATH"
    )
    print(header)
    print("  " + "-" * (len(header) - 2))
    for row in results:
        print(
            f"  {str(row['name']):<{name_width}}  {status_kind(str(row['status']), 9)}  "
            f"{format_size(row['size']):>9}  {highlight(str(row['version'])[:24].ljust(24))}  "
            f"{row['path']}"
        )


def print_path_hint(bin_dir: Path, dry_run: bool) -> None:
    if dry_run:
        return
    path_entries = [
        os.path.realpath(entry)
        for entry in os.environ.get("PATH", "").split(os.pathsep)
        if entry
    ]
    if os.path.realpath(str(bin_dir)) not in path_entries:
        warn(f"{bin_dir} is not in PATH; add it with:")
        print(f'  export PATH="{bin_dir}:$PATH"')


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Build and install all workspace binary targets, then print a "
        "summary of every installed binary (and optionally the adapter library)."
    )
    parser.add_argument(
        "--workspace-root",
        default=str(Path(__file__).resolve().parents[2]),
        help="Repository root path; group workspaces are discovered under crates/ (default: auto-detected).",
    )
    parser.add_argument(
        "--profile",
        default=None,
        choices=["release", "dev"],
        help="Build profile of the binaries to install (default: release; "
        "mutually exclusive with --release/--debug).",
    )
    profile_group = parser.add_mutually_exclusive_group()
    profile_group.add_argument(
        "--release",
        action="store_true",
        help="Shortcut for --profile release (the default).",
    )
    profile_group.add_argument(
        "--debug",
        action="store_true",
        help="Shortcut for --profile dev.",
    )
    parser.add_argument(
        "--root",
        default=None,
        help="Optional install root (like cargo install --root). "
        "Binaries go to <root>/bin; the adapter library/header go to "
        "<root>/lib and <root>/include.",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Print commands only without executing installs.",
    )
    parser.add_argument(
        "--all-workspace-bins",
        action="store_true",
        help="Install every workspace binary target instead of the supported release tool set.",
    )
    parser.add_argument(
        "--with-adapter-ffi",
        action="store_true",
        help="Also build crates/bindings/mudu_adapter_ffi and install the "
        "adapter library plus its C header.",
    )
    args = parser.parse_args(normalized_argv())
    if args.profile is not None and (args.release or args.debug):
        parser.error("--release/--debug cannot be combined with --profile")
    return args


def main() -> int:
    started = time.monotonic()
    args = parse_args()
    profile = args.profile or ("dev" if args.debug else "release")
    workspace_root = Path(args.workspace_root).resolve()

    workspace_roots = find_group_workspaces(workspace_root)
    if not workspace_roots:
        print(
            f"Invalid repository root: {workspace_root}. "
            "No group workspaces found under crates/.",
            file=sys.stderr,
        )
        return 2

    try:
        if args.all_workspace_bins:
            bins = [
                (package_dir, bin_name, "workspace binary target")
                for package_dir, bin_name in load_workspace_bins(workspace_roots)
            ]
        else:
            bins = load_supported_bins(workspace_roots)
    except Exception as exc:
        print(str(exc), file=sys.stderr)
        return 1

    bin_dir, lib_dir, include_dir = resolve_install_dirs(args.root)
    results, exit_code = install_bins(
        workspace_root=workspace_root,
        bins=bins,
        profile=profile,
        bin_dir=bin_dir,
        dry_run=args.dry_run,
    )

    if args.with_adapter_ffi:
        if exit_code == 0:
            adapter_results, adapter_code = install_adapter_ffi(
                workspace_root=workspace_root,
                profile=profile,
                lib_dir=lib_dir,
                include_dir=include_dir,
                dry_run=args.dry_run,
            )
            results.extend(adapter_results)
            if adapter_code != 0:
                exit_code = adapter_code
        else:
            print(
                "Skipping the adapter library because binary installation failed.",
                file=sys.stderr,
            )

    print_summary(results, profile)
    print_path_hint(bin_dir, args.dry_run)
    elapsed = format_duration(time.monotonic() - started)
    if exit_code == 0:
        if args.dry_run:
            status("Dry-run", f"finished in {elapsed}; no changes made")
        elif any(row["status"] in ("installed", "unchanged") for row in results):
            status("Finished", f"install in {elapsed}")
            status("Installed to", highlight(bin_dir))
            if args.with_adapter_ffi:
                status("Installed to", highlight(lib_dir))
    return exit_code


if __name__ == "__main__":
    sys.exit(main())
