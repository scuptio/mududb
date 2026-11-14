#!/usr/bin/env python3
"""Pack the npm/NuGet/PyPI registry packages into a verified dist directory.

Credential-free counterpart to publish.py: runs `npm pack`, `dotnet pack`, or
`python -m build`, copies the artifacts to target/sdk-dist/registry/<target>/,
and writes a manifest.json (name, version, tag, per-file sha256) that
publish.py re-verifies before any upload. No registry credentials are needed
or accepted here.

Usage: pack_registry.py [npm] [nuget] [pypi]   (default: all three)
"""
import argparse
import hashlib
import json
import re
import shutil
import subprocess
import sys
import tomllib
import xml.etree.ElementTree as ET
from pathlib import Path
from typing import Dict, List

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "lib"))
from cli import normalized_argv  # noqa: E402
from output import cmd as echo_cmd  # noqa: E402
from output import dim, error, highlight, status  # noqa: E402

REPO_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_DIST = REPO_ROOT / "target" / "sdk-dist" / "registry"

NPM_SOURCE = Path("crates/sdk/bindings/assemblyscript")
NUGET_SOURCE = Path("crates/sdk/mudu_api/csharp/Mudu.Api.csproj")
PYPI_SOURCE = Path("crates/sdk/bindings/python")

# tag prefixes per .github/workflows/bindings.yaml (as-v* / cs-v* / py-v*).
TAG_PREFIXES = {"npm": "as-v", "nuget": "cs-v", "pypi": "py-v"}


def run(cmd: List[str], cwd: Path) -> None:
    echo_cmd(cmd)
    proc = subprocess.run([str(part) for part in cmd], cwd=str(cwd), text=True)
    if proc.returncode != 0:
        raise RuntimeError(f"command failed with exit {proc.returncode}: {cmd[0]}")


def git_info() -> Dict[str, object]:
    def git(args: List[str]) -> str:
        proc = subprocess.run(
            ["git", *args], cwd=str(REPO_ROOT), text=True, capture_output=True
        )
        return proc.stdout.strip() if proc.returncode == 0 else ""

    commit = git(["rev-parse", "HEAD"])
    return {"source_commit": commit or "unknown", "dirty": bool(git(["status", "--porcelain"]))}


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def read_versions() -> Dict[str, str]:
    package_json = json.loads((REPO_ROOT / NPM_SOURCE / "package.json").read_text(encoding="utf-8"))
    csproj = ET.parse(REPO_ROOT / NUGET_SOURCE).getroot()
    version_element = csproj.find(".//Version")
    if version_element is None or not version_element.text:
        raise RuntimeError(f"<Version> not found in {NUGET_SOURCE}")
    pyproject = tomllib.loads((REPO_ROOT / PYPI_SOURCE / "pyproject.toml").read_text(encoding="utf-8"))
    return {
        "npm": package_json["version"],
        "nuget": version_element.text.strip(),
        "pypi": pyproject["project"]["version"],
    }


def read_names() -> Dict[str, str]:
    package_json = json.loads((REPO_ROOT / NPM_SOURCE / "package.json").read_text(encoding="utf-8"))
    csproj = ET.parse(REPO_ROOT / NUGET_SOURCE).getroot()
    package_id = csproj.find(".//PackageId")
    pyproject = tomllib.loads((REPO_ROOT / PYPI_SOURCE / "pyproject.toml").read_text(encoding="utf-8"))
    return {
        "npm": package_json["name"],
        "nuget": package_id.text.strip() if package_id is not None else "mududb",
        "pypi": pyproject["project"]["name"],
    }


def pack_npm(dist_dir: Path, version: str) -> List[Path]:
    source = REPO_ROOT / NPM_SOURCE
    before = set(dist_dir.glob("*.tgz"))
    run(["npm", "pack", "--pack-destination", str(dist_dir)], source)
    produced = [path for path in set(dist_dir.glob("*.tgz")) - before]
    if len(produced) != 1:
        raise RuntimeError(f"npm pack did not produce exactly one tarball in {dist_dir}")
    if version not in produced[0].name:
        raise RuntimeError(f"npm tarball {produced[0].name} does not match version {version}")
    return produced


def pack_nuget(dist_dir: Path, version: str) -> List[Path]:
    run(
        ["dotnet", "pack", "-c", "Release", str(REPO_ROOT / NUGET_SOURCE), "-o", str(dist_dir)],
        REPO_ROOT,
    )
    produced = list(dist_dir.glob("*.nupkg"))
    if not produced:
        raise RuntimeError(f"dotnet pack produced no .nupkg in {dist_dir}")
    for package in produced:
        if not package.name.endswith(f".{version}.nupkg"):
            raise RuntimeError(f"nupkg {package.name} does not match version {version}")
    return produced


def pack_pypi(dist_dir: Path, version: str) -> List[Path]:
    source_dist = REPO_ROOT / PYPI_SOURCE / "dist"
    if source_dist.exists():
        shutil.rmtree(source_dist)
    run([sys.executable, "-m", "build", "--outdir", str(dist_dir)], REPO_ROOT / PYPI_SOURCE)
    produced = [p for p in dist_dir.iterdir() if p.suffix == ".whl" or p.name.endswith(".tar.gz")]
    if not produced:
        raise RuntimeError(f"python -m build produced no distributions in {dist_dir}")
    for distribution in produced:
        if version not in distribution.name:
            raise RuntimeError(f"distribution {distribution.name} does not match version {version}")
    return produced


PACKERS = {"npm": pack_npm, "nuget": pack_nuget, "pypi": pack_pypi}


def pack_target(target: str, dist_root: Path) -> Path:
    versions = read_versions()
    names = read_names()
    version = versions[target]
    status("Packing", f"{highlight(names[target])} {version} ({target})")
    dist_dir = dist_root / target
    if dist_dir.exists():
        shutil.rmtree(dist_dir)
    dist_dir.mkdir(parents=True)

    artifacts = PACKERS[target](dist_dir, version)
    manifest = {
        "schema": 1,
        "target": target,
        "name": names[target],
        "version": version,
        "tag": f"{TAG_PREFIXES[target]}{version}",
        **git_info(),
        "artifacts": [
            {"file": path.name, "sha256": sha256(path)} for path in sorted(artifacts)
        ],
    }
    # The manifest is a sibling of the artifact directory so the directory
    # itself only holds uploadable files (pypa/gh-action-pypi-publish and
    # twine reject extra files in the packages dir).
    manifest_path = dist_root / f"{target}.manifest.json"
    manifest_path.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")

    status("Packed", f"{highlight(names[target])} {version} (tag {manifest['tag']})")
    for entry in manifest["artifacts"]:
        size = (dist_dir / entry["file"]).stat().st_size
        print(dim(f"  {size:>12}  {entry['file']}"))
    print(dim(f"  manifest: {manifest_path}"))
    return manifest_path


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "targets",
        nargs="*",
        choices=sorted(PACKERS),
        default=None,
        help="Registry targets to pack (default: all).",
    )
    parser.add_argument(
        "--dist",
        default=str(DEFAULT_DIST),
        help="Registry dist root (default: target/sdk-dist/registry).",
    )
    return parser.parse_args(normalized_argv())


def main() -> int:
    args = parse_args()
    targets = args.targets or sorted(PACKERS)
    dist_root = Path(args.dist).resolve()
    try:
        for target in targets:
            pack_target(target, dist_root)
    except (RuntimeError, OSError, KeyError) as exc:
        error(exc)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
