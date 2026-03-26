#!/usr/bin/env python3
import argparse
import json
import subprocess
import sys
from pathlib import Path
from typing import Dict, List, Tuple


def run(cmd: List[str], cwd: Path) -> subprocess.CompletedProcess:
    return subprocess.run(cmd, cwd=str(cwd), text=True, capture_output=True)


SUPPORTED_BINS: List[Dict[str, str]] = [
    {
        "package": "mudu_package",
        "bin": "mpk",
        "description": "package builder",
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


def load_workspace_bins(workspace_root: Path) -> List[Tuple[Path, str]]:
    metadata = load_workspace_metadata(workspace_root)
    bins: List[Tuple[Path, str]] = []
    seen = set()

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


def load_supported_bins(workspace_root: Path) -> List[Tuple[Path, str, str]]:
    metadata = load_workspace_metadata(workspace_root)
    package_index = {}

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


def install_bins(
    workspace_root: Path,
    bins: List[Tuple[Path, str, str]],
    profile: str,
    install_root: str | None,
    dry_run: bool,
) -> int:
    if not bins:
        print("No workspace binaries found.")
        return 0

    print("Binaries to install:")
    for package_dir, bin_name, description in bins:
        rel_dir = package_dir.relative_to(workspace_root)
        print(f"  - {bin_name} ({rel_dir})")
        print(f"    {description}")

    for package_dir, bin_name, _description in bins:
        cmd = [
            "cargo",
            "install",
            "--force",
            "--path",
            str(package_dir),
            "--bin",
            bin_name,
            "--profile",
            profile,
        ]

        if install_root:
            cmd.extend(["--root", install_root])

        print("\n> " + " ".join(cmd))
        if dry_run:
            continue

        proc = subprocess.run(cmd, cwd=str(workspace_root), text=True)
        if proc.returncode != 0:
            print(f"Failed to install binary: {bin_name}", file=sys.stderr)
            return proc.returncode

    return 0


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Build and install all workspace binary targets."
    )
    parser.add_argument(
        "--workspace-root",
        default=str(Path(__file__).resolve().parents[2]),
        help="Workspace root path (default: auto-detected).",
    )
    parser.add_argument(
        "--profile",
        default="release",
        choices=["release", "dev"],
        help="Build profile used by cargo install.",
    )
    parser.add_argument(
        "--root",
        default=None,
        help="Optional install root passed to cargo install --root.",
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
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    workspace_root = Path(args.workspace_root).resolve()

    if not (workspace_root / "Cargo.toml").exists():
        print(
            f"Invalid workspace root: {workspace_root}. Cargo.toml not found.",
            file=sys.stderr,
        )
        return 2

    try:
        if args.all_workspace_bins:
            bins = [
                (package_dir, bin_name, "workspace binary target")
                for package_dir, bin_name in load_workspace_bins(workspace_root)
            ]
        else:
            bins = load_supported_bins(workspace_root)
    except Exception as exc:
        print(str(exc), file=sys.stderr)
        return 1

    return install_bins(
        workspace_root=workspace_root,
        bins=bins,
        profile=args.profile,
        install_root=args.root,
        dry_run=args.dry_run,
    )


if __name__ == "__main__":
    sys.exit(main())
