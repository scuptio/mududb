#!/usr/bin/env python3
"""Unified publishing entry for MuduDB artifacts: verify, then push.

Every target defaults to a harmless rehearsal (dry-run); pass --apply for the
real upload. This script never compiles or packs: registry targets consume the
verified output of pack_registry.py (target/sdk-dist/registry/<target>/), and
the GitHub targets delegate to script/sdk/publish.py / publish_runtime.py,
which consume the outputs of `cargo make build-sdk` / the adapter runtime
build. Triggers, tags, and secrets stay in the CI workflows; this command is
the single execution path they (or a maintainer) call.

Examples:
  publish.py npm                        # verify + npm publish --dry-run
  publish.py npm,nuget --apply          # real upload (needs credentials)
  publish.py pypi --tag py-v0.1.0 --apply
  publish.py github-sdk --expected-commit <sha>
  publish.py github-runtime --artifacts target/sdk-dist/adapter-runtime

Credentials (real uploads only): NODE_AUTH_TOKEN (npm), NUGET_API_KEY
(NuGet), TWINE_USERNAME/TWINE_PASSWORD or PYPI_TOKEN (PyPI),
SDK_GITHUB_TOKEN_FILE (GitHub targets, see script/sdk/publish.py).
"""
import argparse
import hashlib
import importlib.util
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import urllib.error
import urllib.request
from pathlib import Path
from typing import Dict, List, Optional

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "lib"))
from cli import normalized_argv  # noqa: E402
from output import cmd as echo_cmd  # noqa: E402
from output import dim, error, highlight, status, warn  # noqa: E402

REPO_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_REGISTRY_DIST = REPO_ROOT / "target" / "sdk-dist" / "registry"
SDK_PUBLISH = REPO_ROOT / "script" / "sdk" / "publish.py"
RUNTIME_PUBLISH = REPO_ROOT / "script" / "sdk" / "publish_runtime.py"

REGISTRY_SOURCES = {
    "npm": REPO_ROOT / "crates/sdk/bindings/assemblyscript/package.json",
    "nuget": REPO_ROOT / "crates/sdk/mudu_api/csharp/Mudu.Api.csproj",
    "pypi": REPO_ROOT / "crates/sdk/bindings/python/pyproject.toml",
}
NUGET_INDEX = "https://api.nuget.org/v3-flatcontainer/{name}/index.json"
PYPI_INDEX = "https://pypi.org/pypi/{name}/{version}/json"


def fail(message: str) -> int:
    error(message)
    return 1


def run(cmd: List[str], env: Optional[dict] = None, cwd: Optional[Path] = None) -> int:
    echo_cmd(cmd)
    proc = subprocess.run(
        [str(part) for part in cmd], cwd=str(cwd or REPO_ROOT), env=env, text=True
    )
    return proc.returncode


# ---------------------------------------------------------------------------
# registry targets (npm / nuget / pypi)
# ---------------------------------------------------------------------------


def load_registry_manifest(dist_root: Path, target: str) -> dict:
    manifest_path = dist_root / f"{target}.manifest.json"
    if not manifest_path.is_file():
        raise ValueError(
            f"{manifest_path} not found; run `cargo make package-sdk-registry -- {target}` first"
        )
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    if manifest.get("schema") != 1 or manifest.get("target") != target:
        raise ValueError(f"invalid registry manifest: {manifest_path}")
    for entry in manifest.get("artifacts", []):
        artifact = dist_root / target / entry["file"]
        if not artifact.is_file():
            raise ValueError(f"artifact missing: {artifact}")
        if hashlib.sha256(artifact.read_bytes()).hexdigest() != entry["sha256"]:
            raise ValueError(f"artifact modified after packing: {artifact}")
    return manifest


def source_version(target: str) -> str:
    """Re-read the version from the source tree to catch pack-then-bump drift."""
    source = REGISTRY_SOURCES[target]
    if target == "npm":
        return json.loads(source.read_text(encoding="utf-8"))["version"]
    if target == "nuget":
        import xml.etree.ElementTree as ET

        element = ET.parse(source).getroot().find(".//Version")
        if element is None or not element.text:
            raise ValueError(f"<Version> not found in {source}")
        return element.text.strip()
    import tomllib

    return tomllib.loads(source.read_text(encoding="utf-8"))["project"]["version"]


def check_version_and_tag(manifest: dict, tag_override: Optional[str]) -> str:
    target = manifest["target"]
    tag = tag_override or manifest["tag"]
    if tag != manifest["tag"]:
        raise ValueError(f"tag {tag} does not match the packed tag {manifest['tag']}")
    if not re.fullmatch(r"(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)", manifest["version"]):
        raise ValueError(f"invalid semver in manifest: {manifest['version']}")
    current = source_version(target)
    if current != manifest["version"]:
        raise ValueError(
            f"source tree version {current} differs from the packed version "
            f"{manifest['version']}; re-run pack_registry.py"
        )
    return tag


def already_published(manifest: dict) -> bool:
    target, name, version = manifest["target"], manifest["name"], manifest["version"]
    try:
        if target == "npm":
            proc = subprocess.run(
                ["npm", "view", f"{name}@{version}", "version"],
                text=True,
                capture_output=True,
                timeout=60,
            )
            return proc.returncode == 0 and proc.stdout.strip() == version
        url = (
            NUGET_INDEX.format(name=name.lower())
            if target == "nuget"
            else PYPI_INDEX.format(name=name, version=version)
        )
        with urllib.request.urlopen(url, timeout=30) as response:
            if target == "pypi":
                return True
            versions = json.loads(response.read()).get("versions", [])
            return version in versions
    except urllib.error.HTTPError as error:
        if error.code == 404:
            return False
        warn(f"registry check failed (HTTP {error.code}); continuing")
    except (urllib.error.URLError, subprocess.SubprocessError, OSError) as error:
        warn(f"registry check failed ({error}); continuing")
    return False


def twine_command() -> List[str]:
    twine = shutil.which("twine")
    if not twine:
        # pipx installs entry points into ~/.local/bin, which may not be on PATH.
        candidate = Path.home() / ".local" / "bin" / "twine"
        twine = str(candidate) if candidate.is_file() else None
    if twine:
        return [twine]
    if importlib.util.find_spec("twine"):
        return [sys.executable, "-m", "twine"]
    raise ValueError("twine not found; install it with `pipx install twine` or `pip install twine`")


def publish_npm(dist_dir: Path, manifest: dict, apply: bool, provenance: bool) -> int:
    artifacts = [dist_dir / entry["file"] for entry in manifest["artifacts"]]
    env = os.environ.copy()
    if not apply:
        env.setdefault("NODE_AUTH_TOKEN", "dry-run-placeholder")
        return run(["npm", "publish", *artifacts, "--access", "public", "--dry-run"], env=env)
    cmd = ["npm", "publish", *artifacts, "--access", "public"]
    if provenance:
        # Requires CI OIDC (id-token); pass --no-provenance for a manual upload.
        cmd.append("--provenance")
    if "NODE_AUTH_TOKEN" not in env:
        warn("NODE_AUTH_TOKEN is not set; falling back to the npm login session")
    return run(cmd, env=env)


def publish_nuget(dist_dir: Path, manifest: dict, apply: bool) -> int:
    artifacts = [dist_dir / entry["file"] for entry in manifest["artifacts"]]
    if not apply:
        with tempfile.TemporaryDirectory(prefix="nuget-dry-run-feed-") as feed:
            code = run(["dotnet", "nuget", "push", *artifacts, "--source", feed])
            if code == 0:
                print(dim("dry-run feed: " + ", ".join(p.name for p in Path(feed).iterdir())))
            return code
    api_key = os.environ.get("NUGET_API_KEY")
    if not api_key:
        return fail("NUGET_API_KEY is not set")
    return run(
        [
            "dotnet",
            "nuget",
            "push",
            *artifacts,
            "--source",
            "https://api.nuget.org/v3/index.json",
            "--api-key",
            api_key,
        ]
    )


def publish_pypi(dist_dir: Path, manifest: dict, apply: bool) -> int:
    artifacts = [dist_dir / entry["file"] for entry in manifest["artifacts"]]
    twine = twine_command()
    if not apply:
        return run([*twine, "check", *artifacts])
    env = os.environ.copy()
    token = os.environ.get("PYPI_TOKEN")
    if token:
        env["TWINE_USERNAME"] = "__token__"
        env["TWINE_PASSWORD"] = token
    if "TWINE_USERNAME" not in env or "TWINE_PASSWORD" not in env:
        return fail("set TWINE_USERNAME/TWINE_PASSWORD or PYPI_TOKEN for a real upload")
    return run([*twine, "upload", "--non-interactive", *artifacts], env=env)


def publish_registry_target(target: str, args: argparse.Namespace) -> int:
    try:
        manifest = load_registry_manifest(Path(args.dist).resolve(), target)
        tag = check_version_and_tag(manifest, args.tag)
        name, version = manifest["name"], manifest["version"]
    except ValueError as exc:
        return fail(str(exc))

    status("Verifying", f"{highlight(name)} {version} (tag {tag})")
    if already_published(manifest):
        status("Skipped", f"{name} {version} (already published)")
        return 0

    dist_dir = Path(args.dist).resolve() / target
    status("Publishing" if args.apply else "Rehearsing", f"{name} {version}")
    if target == "npm":
        code = publish_npm(dist_dir, manifest, args.apply, provenance=not args.no_provenance)
    elif target == "nuget":
        code = publish_nuget(dist_dir, manifest, args.apply)
    else:
        code = publish_pypi(dist_dir, manifest, args.apply)
    if code == 0:
        if args.apply:
            status("Published", highlight(f"{name} {version}"))
        else:
            status("Dry-run ok", f"{name} {version}")
    return code


# ---------------------------------------------------------------------------
# GitHub targets (delegate to the verified SDK publishers)
# ---------------------------------------------------------------------------


def publish_github_sdk(args: argparse.Namespace) -> int:
    cmd = [sys.executable, str(SDK_PUBLISH), "--dist", args.sdk_dist]
    if args.expected_commit:
        cmd += ["--expected-commit", args.expected_commit]
    if args.create_repos:
        cmd.append("--create-repos")
    if not args.apply:
        cmd.append("--dry-run")
    return run(cmd)


def publish_github_runtime(args: argparse.Namespace) -> int:
    artifacts = Path(args.artifacts).resolve()
    if not artifacts.is_dir():
        return fail(f"artifacts directory not found: {artifacts}")
    archives = sorted(artifacts.rglob("*.tar.gz"))
    if not archives:
        return fail(f"no runtime archives under {artifacts}")
    if not args.apply:
        status("Dry-run", "adapter runtime archives ready to publish:")
        for archive in archives:
            print(dim(f"  {archive}"))
        print(dim("re-run with --apply to publish (SDK_GITHUB_TOKEN_FILE, "
                  "GITHUB_REPOSITORY, GITHUB_SHA must be set)"))
        return 0
    return run([sys.executable, str(RUNTIME_PUBLISH), "--artifacts", str(artifacts)])


# ---------------------------------------------------------------------------


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "targets",
        help="Comma-separated publish targets: npm,nuget,pypi,github-sdk,github-runtime",
    )
    parser.add_argument(
        "--apply",
        action="store_true",
        help="Perform the real upload. Default is a dry-run rehearsal.",
    )
    parser.add_argument(
        "--tag",
        default=None,
        help="Expected publishing tag (must match the tag recorded at pack time).",
    )
    parser.add_argument(
        "--dist",
        default=str(DEFAULT_REGISTRY_DIST),
        help="Registry dist root written by pack_registry.py.",
    )
    parser.add_argument(
        "--sdk-dist",
        default=str(REPO_ROOT / "target" / "sdk-dist"),
        help="github-sdk: SDK distribution directory from `cargo make build-sdk`.",
    )
    parser.add_argument("--expected-commit", default=None, help="github-sdk: required source commit.")
    parser.add_argument("--create-repos", action="store_true", help="github-sdk: create missing distribution repositories.")
    parser.add_argument("--artifacts", default=None, help="github-runtime: adapter runtime artifact directory.")
    parser.add_argument(
        "--no-provenance",
        action="store_true",
        help="npm: publish without --provenance (for manual uploads outside CI).",
    )
    return parser.parse_args(normalized_argv())


def main() -> int:
    args = parse_args()
    known: Dict[str, str] = {
        "npm": "registry",
        "nuget": "registry",
        "pypi": "registry",
        "github-sdk": "github-sdk",
        "github-runtime": "github-runtime",
    }
    targets = args.targets.split(",")
    unknown = [target for target in targets if target not in known]
    if unknown or len(set(targets)) != len(targets):
        return fail(f"invalid or duplicate targets: {', '.join(targets)}")

    exit_code = 0
    for target in targets:
        kind = known[target]
        if kind == "registry":
            code = publish_registry_target(target, args)
        elif kind == "github-sdk":
            code = publish_github_sdk(args)
        else:
            if not args.artifacts:
                code = fail("github-runtime requires --artifacts")
            else:
                code = publish_github_runtime(args)
        if code != 0:
            exit_code = code
            break
    return exit_code


if __name__ == "__main__":
    sys.exit(main())
