#!/usr/bin/env python3
"""Package a MuduDB server release tarball.

Single implementation shared by the Release workflow
(.github/workflows/build-release.yaml) and local runs (`cargo make
package-release`): build (or reuse) release binaries, collect the binaries and
prerequisite scripts, compute the runtime glibc floor, write the manifest, and
emit <archive>.tar.gz + <archive>.sha256 + CHANGELOG_RELEASE.md under
<output-dir>/<version>/.

This script never publishes anything; publishing stays in the Release
workflow (GitHub Releases) or a separate publish command.
"""
import argparse
import hashlib
import os
import platform
import re
import shutil
import subprocess
import sys
import tarfile
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import List, Optional, Tuple

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "lib"))
from cli import normalized_argv  # noqa: E402
from output import cmd as echo_cmd  # noqa: E402
from output import dim, error, format_duration, highlight, status, warn  # noqa: E402

REPO_ROOT = Path(__file__).resolve().parents[2]

# The release binaries live in two independent group workspaces; all builds
# share the repository-root target/ directory.
BUILD_GROUPS = ["crates/db-kernel", "crates/tools"]

# package name -> binary name, as shipped in the release tarball.
RELEASE_BINS = [
    ("mpm_build", "mpm-build"),
    ("mpm_crate", "mpm-crate"),
    ("mudu_gen", "mgen"),
    ("mudu_transpiler", "mtp"),
    ("mudud", "mudud"),
    ("mudu_cli", "mcli"),
]

# build-release/ sources copied into the package, and their destinations.
SOURCE_LIB_LIST = Path("build-release/lib-list.txt")
SOURCE_PREREQUISITE_ENTRY = Path("build-release/prerequisite.sh")
SOURCE_PREREQUISITE = Path("build-release/prerequisite-deb.sh")
SOURCE_PREREQUISITE_PACKAGE_LIST = Path("build-release/package/deb/package-list.txt")


def run(cmd: List[str], cwd: Path, env: Optional[dict] = None) -> subprocess.CompletedProcess:
    return subprocess.run(cmd, cwd=str(cwd), env=env, text=True, capture_output=True)


def host_target() -> str:
    proc = run(["rustc", "-vV"], REPO_ROOT)
    if proc.returncode != 0:
        raise RuntimeError("failed to run `rustc -vV` to detect the host target")
    for line in proc.stdout.splitlines():
        if line.startswith("host:"):
            return line.split(": ", 1)[1].strip()
    raise RuntimeError("could not parse the host target from `rustc -vV`")


def default_version() -> str:
    return "v" + datetime.now(timezone.utc).strftime("%Y%m%d.%H%M")


def repository_slug() -> str:
    proc = run(["git", "remote", "get-url", "origin"], REPO_ROOT)
    if proc.returncode == 0:
        url = proc.stdout.strip()
        match = re.search(r"github\.com[:/]([^/]+/[^/]+?)(?:\.git)?$", url)
        if match:
            return match.group(1)
    return "scuptio/mududb"


def build_release_binaries(target: str, dry_run: bool) -> None:
    env = os.environ.copy()
    if platform.system() == "Linux":
        # Binaries resolve bundled shared libraries relative to bin/.
        env["RUSTFLAGS"] = "-C link-arg=-Wl,-rpath,$ORIGIN/../lib"
    else:
        warn("skipping the $ORIGIN rpath RUSTFLAGS on non-Linux")
    for group in BUILD_GROUPS:
        cmd = ["cargo", "build", "--release", "--target", target]
        status("Building", highlight(group))
        echo_cmd(cmd)
        if dry_run:
            continue
        proc = subprocess.run(cmd, cwd=str(REPO_ROOT / group), env=env, text=True)
        if proc.returncode != 0:
            raise RuntimeError(f"release build failed in {group} (exit {proc.returncode})")


def collect_binaries(package_bin_dir: Path, target: str, dry_run: bool) -> List[str]:
    target_dir = REPO_ROOT / "target" / target / "release"
    collected = []
    for _package, bin_name in RELEASE_BINS:
        bin_path = target_dir / bin_name
        if bin_path.is_file():
            if not dry_run:
                shutil.copy2(bin_path, package_bin_dir / bin_name)
            print(dim(f"copied {bin_name}"))
            collected.append(bin_name)
        else:
            warn(f"binary {bin_name} not found at {bin_path}")
    if not collected and not dry_run:
        raise RuntimeError(f"no release binaries found under {target_dir}")
    return collected


def copy_prerequisites(package_dir: Path, dry_run: bool) -> None:
    lib_dir = package_dir / "lib"
    script_dir = package_dir / "script"
    deb_dir = script_dir / "package" / "deb"
    copies = [
        (SOURCE_LIB_LIST, lib_dir / "lib-list.txt", False),
        (SOURCE_PREREQUISITE_ENTRY, script_dir / "prerequisite.sh", True),
        (SOURCE_PREREQUISITE, script_dir / "prerequisite-deb.sh", True),
        (SOURCE_PREREQUISITE_PACKAGE_LIST, deb_dir / "package-list.txt", False),
    ]
    for source, dest, executable in copies:
        source_path = REPO_ROOT / source
        if not source_path.is_file():
            raise RuntimeError(f"required release source not found: {source_path}")
        if not dry_run:
            dest.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source_path, dest)
            if executable:
                dest.chmod(0o755)


def detect_min_glibc(bin_dir: Path, bin_names: List[str]) -> str:
    """Derive the runtime glibc floor from the produced ELF binaries by scanning
    referenced GLIBC_* symbol versions and taking the maximum."""
    if platform.system() != "Linux":
        warn("min_glibc detection is Linux-only; writing an empty value")
        return ""
    versions = set()
    for name in bin_names:
        proc = run(["readelf", "--version-info", str(bin_dir / name)], REPO_ROOT)
        if proc.returncode != 0:
            continue
        versions.update(re.findall(r"GLIBC_([0-9]+\.[0-9]+)", proc.stdout))
    if not versions:
        raise RuntimeError(f"unable to determine min_glibc from binaries in {bin_dir}")
    ordered = sorted(versions, key=lambda v: tuple(int(part) for part in v.split(".")))
    return ordered[-1]


def write_manifest(
    package_dir: Path,
    version: str,
    target: str,
    archive_name: str,
    repository: str,
    min_glibc: str,
) -> None:
    archive_uri = (
        f"https://github.com/{repository}/releases/download/{version}/{archive_name}"
    )
    files = sorted(
        str(path.relative_to(package_dir))
        for path in package_dir.rglob("*")
        if path.is_file()
    )
    lines = [
        "name=mududb",
        f"version={version}",
        f"target={target}",
        f"archive={archive_name}",
        f"archive_uri={archive_uri}",
        "bin_dir=bin",
        "lib_dir=lib",
        "lib_list=lib/lib-list.txt",
        f"min_glibc={min_glibc}",
        "",
        "[files]",
        *files,
    ]
    (package_dir / "manifest.txt").write_text("\n".join(lines) + "\n", encoding="utf-8")


def make_tarball(package_dir: Path, artifact_dir: Path, version: str, archive_name: str) -> Path:
    archive_path = artifact_dir / archive_name
    with tarfile.open(archive_path, "w:gz") as tar:
        tar.add(package_dir, arcname=version)
    digest = hashlib.sha256(archive_path.read_bytes()).hexdigest()
    archive_path.with_suffix(archive_path.suffix + ".sha256").write_text(
        f"{digest}  {archive_name}\n", encoding="utf-8"
    )
    return archive_path


def generate_changelog(artifact_dir: Path, version: str, repository: str, dry_run: bool) -> Optional[Path]:
    changelog = artifact_dir / "CHANGELOG_RELEASE.md"
    proc = run(["git", "tag", "--sort=-v:refname"], REPO_ROOT)
    tags = [tag for tag in proc.stdout.splitlines() if tag and tag != version]
    prev_tag = tags[0] if tags else ""
    if prev_tag:
        header = f"Changes since {prev_tag}:"
        log_args = ["git", "log", f"{prev_tag}..HEAD", "--pretty=format:- %s (%h)"]
    else:
        header = "Initial release:"
        log_args = ["git", "log", "--pretty=format:- %s (%h)"]
    log_proc = run(log_args, REPO_ROOT)
    if log_proc.returncode != 0:
        warn("git log failed; changelog lists no changes")
        entries = ""
    else:
        entries = log_proc.stdout
    content = (
        f"# Changelog: {version}\n"
        "\n"
        "## Changes\n"
        "\n"
        f"{header}\n"
        "\n"
        f"{entries}\n"
        "\n"
        f"**Full Commit History**: https://github.com/{repository}/commits/{version}\n"
    )
    if dry_run:
        print(dim(f"would write changelog ({header})"))
        return changelog
    changelog.write_text(content, encoding="utf-8")
    return changelog


def format_size(size: int) -> str:
    if size >= 1024 * 1024:
        return f"{size / (1024 * 1024):.1f} MiB"
    if size >= 1024:
        return f"{size / 1024:.1f} KiB"
    return f"{size} B"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--version",
        default=None,
        help="Release version/tag (default: v<UTC date>, matching the workflow fallback).",
    )
    parser.add_argument(
        "--target",
        default=None,
        help="Rust target triple to build/package (default: the host target).",
    )
    parser.add_argument(
        "--repository",
        default=None,
        help="GitHub owner/repo used in manifest and changelog URLs "
        "(default: parsed from the origin remote).",
    )
    parser.add_argument(
        "--output-dir",
        default=str(REPO_ROOT / "target" / "release-artifacts"),
        help="Artifact output root (default: target/release-artifacts).",
    )
    parser.add_argument(
        "--skip-build",
        action="store_true",
        help="Only package binaries already present under target/<target>/release.",
    )
    parser.add_argument(
        "--skip-changelog",
        action="store_true",
        help="Do not generate CHANGELOG_RELEASE.md.",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Print the plan without building or writing artifacts.",
    )
    return parser.parse_args(normalized_argv())


def main() -> int:
    started = time.monotonic()
    args = parse_args()
    version = args.version or default_version()
    target = args.target or host_target()
    repository = args.repository or repository_slug()
    artifact_dir = Path(args.output_dir).resolve() / version
    package_dir = artifact_dir / version
    archive_name = f"mududb-{version}-{target}.tar.gz"

    status("Packaging", f"{highlight(version)} for {target}")
    print(dim(f"  artifact dir: {artifact_dir}"))
    print(dim(f"  archive:      {archive_name}"))

    try:
        if not args.skip_build:
            build_release_binaries(target, args.dry_run)

        if args.dry_run:
            print(dim("would collect binaries: " + ", ".join(name for _, name in RELEASE_BINS)))
            print(dim(f"would write {archive_name} + .sha256 under {artifact_dir}"))
            if not args.skip_changelog:
                generate_changelog(artifact_dir, version, repository, dry_run=True)
            status("Dry-run", "no changes made")
            return 0

        if artifact_dir.exists():
            if artifact_dir.parent != Path(args.output_dir).resolve():
                raise RuntimeError(f"refusing to clean unexpected path: {artifact_dir}")
            shutil.rmtree(artifact_dir)
        (package_dir / "bin").mkdir(parents=True, exist_ok=True)
        (package_dir / "lib").mkdir(parents=True, exist_ok=True)
        (package_dir / "script" / "package" / "deb").mkdir(parents=True, exist_ok=True)

        collected = collect_binaries(package_dir / "bin", target, dry_run=False)
        copy_prerequisites(package_dir, dry_run=False)
        min_glibc = detect_min_glibc(package_dir / "bin", collected)
        status("Detected", f"min_glibc={highlight(min_glibc or '(none)')}")
        write_manifest(package_dir, version, target, archive_name, repository, min_glibc)
        archive_path = make_tarball(package_dir, artifact_dir, version, archive_name)
        changelog = None
        if not args.skip_changelog:
            changelog = generate_changelog(artifact_dir, version, repository, dry_run=False)
    except RuntimeError as exc:
        error(exc)
        return 1

    status(
        "Packaged",
        f"{highlight(archive_path.name)} ({format_size(archive_path.stat().st_size)})",
    )
    outputs = [archive_path, Path(str(archive_path) + ".sha256")]
    if changelog is not None:
        outputs.append(changelog)
    for path in outputs:
        print(dim(f"  {path}"))
    status("Finished", f"in {format_duration(time.monotonic() - started)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
