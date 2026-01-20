#!/usr/bin/env python3
"""
Transpiler tool mudu project.
Called from cargo-make for Pre Processing Stage .
"""

import argparse
import subprocess
import sys
import tomllib
from datetime import datetime
from pathlib import Path
from typing import Dict, Set, Any

import tomli_w


def run_command(command, timeout=60, verbose=False):
    """run command line"""
    try:
        if verbose:
            print("run command", command)
        result = subprocess.run(
            command,
            capture_output=True,
            text=True,
            timeout=timeout,
            check=True,
            encoding='utf-8',
            errors='replace'
        )

        return {
            'success': True,
            'return': result.returncode,
            'stdout': result.stdout,
            'error': result.stderr
        }

    except subprocess.TimeoutExpired:
        return {'success': False, 'error': 'command {} run timeout'.format(command)}
    except subprocess.CalledProcessError as e:
        print(f"error run command: {' '.join(str(arg) for arg in command)}")
        return {
            'success': False,
            'return': e.returncode,
            'stdout': e.stdout,
            'error': e.stderr
        }
    except Exception as e:
        return {'success': False, 'error': str(e)}


def run_mudu_transpiler(
        input_source: Path,
        output_path: Path,
        lang: str,
        src_mod:str,
        dst_mod:str,
        desc_path: Path,
        type_desc_path:Path,
        enable_async: bool,
        verbose: bool
):
    desc_argv = []
    type_desc_argv = []
    async_argv = []
    if desc_path is not None:
        desc_argv = ["--package-desc", desc_path]
    if type_desc_path is not None:
        type_desc_argv = ["--type-desc", type_desc_path]
    if enable_async:
        async_argv = ["--async"]
    command = (["mtp", "--input", input_source, "--output", output_path] +
               desc_argv +
               type_desc_argv +
               ["--src-mod", src_mod, "--dst-mod", dst_mod]
               +
               async_argv +
               [lang]
               )

    ret = run_command(command, timeout=60, verbose=verbose)
    if not ret['success']:
        print(ret['error'])


def run_merge_desc(
        input_folder: Path,
        output_desc_file: Path,
        verbose: bool
):
    command = ["mpk", "merge-desc", "--input-folder", input_folder, "--output-desc-file", output_desc_file]
    ret = run_command(command, timeout=60, verbose=verbose)
    if not ret['success']:
        print(ret['error'])
def extension(lang: str) -> str:
    match lang:
        case "rust":
            return "*.rs"
        case _:
            print("not support language {}".format(lang))
            return ""


def _load_config(config_path: Path = None) -> Dict[str, Any]:
    """Load transpiler configuration from TOML file."""
    default_config = {
        "lang": "rust",
        "async": True,
        "patterns": {
            "include": ["**/*.rs"],
            "exclude": ["**/*_test.rs", "**/test_*.rs", "**/tests/**"]
        },
    }

    if config_path and config_path.exists():
        try:
            f = open(config_path, "rb")
            user_config = tomllib.load(f)

            # Deep merge with defaults
            def deep_merge(default: Any, user: Any) -> Any:
                if isinstance(default, dict) and isinstance(user, dict):
                    merged = default.copy()
                    for key, value in user.items():
                        if key in merged and isinstance(merged[key], dict) and isinstance(value, dict):
                            merged[key] = deep_merge(merged[key], value)
                        else:
                            merged[key] = value
                    return merged
                return user if user is not None else default

            return deep_merge(default_config, user_config)

        except Exception as e:
            print(f"Warning: Failed to load config {config_path}: {e}")
            print("Using default configuration")

    return default_config


class Transpiler:
    """Transpiler that processes Rust source files for WASM compilation."""

    def __init__(
            self,
            source_dir: Path,
            target_dir: Path,
            artifact_dir: Path,
            config_path: Path = None,
            package_desc: Path = None,
            type_desc:Path = None,
            verbose=False
    ):
        self.source_dir = source_dir
        self.target_dir = target_dir
        self.artifact_dir = artifact_dir
        self.package_desc = package_desc
        self.type_desc = type_desc
        self.verbose = verbose
        self.config = _load_config(config_path)

        # Track processed files
        self.processed_files: Set[Path] = set()
        self.generated_files: Set[Path] = set()
        # Total processing seconds
        self.processing_time = 0

    def should_process_file(self, file_path: Path) -> bool:
        """Check if a file should be processed based on patterns."""

        # Check exclude patterns
        exclude_patterns = self.config["patterns"]["exclude"]
        for pattern in exclude_patterns:
            if file_path.match(pattern):
                return False

        # Check include patterns
        include_patterns = self.config["patterns"]["include"]
        for pattern in include_patterns:
            if file_path.match(pattern):
                return True

        return False

    def create_transpiler_config(self):
        """Save the transpiler configuration for reference."""
        config_file = self.artifact_dir / "cfg" / "transpiler-cfg.toml"
        config_file.parent.mkdir(parents=True, exist_ok=True)

        with open(config_file, 'wb') as f:
            tomli_w.dump(self.config, f)

        if self.verbose:
            print(f"Saved transpiler configuration: {config_file}")

    def process_source_file(self, source_file: Path, lang: str, enable_async):
        """Process a single source file."""
        if self.verbose:
            print(f"Processing: {source_file}")

        # Write to generated directory
        rel_path = source_file.relative_to(self.source_dir)
        output_path = self.target_dir / rel_path
        output_path.parent.mkdir(parents=True, exist_ok=True)

        output_desc_file = None
        out_desc_dir = None
        if self.package_desc is not None:
            out_desc_dir = self.artifact_dir / "desc"
            Path(out_desc_dir).mkdir(parents=True, exist_ok=True)
            output_desc_file = self.artifact_dir / "desc" / (source_file.stem + ".desc.json")

        run_mudu_transpiler(
            source_file, output_path,
            lang,
            self.config["translate"]["src_mod"],
            self.config["translate"]["dst_mod"],
            output_desc_file,
            self.type_desc,
            enable_async,
            self.verbose)

        self.processed_files.add(source_file)
        self.generated_files.add(output_path)

        if self.package_desc is not None:
            run_merge_desc(out_desc_dir, self.package_desc, self.verbose)

    def generate_manifest(self):
        """Generate manifest of transpiled files."""
        import json

        manifest = {
            "timestamp": datetime.now(),
            "source_directory": str(self.source_dir),
            "target_directory": str(self.target_dir),
            "processed_files": [str(f.relative_to(self.source_dir)) for f in sorted(self.processed_files)],
            "generated_files": [str(f.relative_to(self.target_dir)) for f in sorted(self.generated_files)],
            "config": self.config,
            "stats": {
                "total_processed": len(self.processed_files),
                "total_generated": len(self.generated_files),
                "processing_time": self.processing_time
            }
        }

        manifest_file = self.artifact_dir / "transpilation-manifest.json"
        with open(manifest_file, 'w', encoding='utf-8') as f:
            json.dump(manifest, f, indent=2, default=str)

        if self.verbose:
            print(f"Generated manifest: {manifest_file}")
        return manifest_file

    def dry_run(self):
        pass

    def run(self) -> bool:
        """Execute the complete transpilation process."""
        if self.verbose:
            print("=" * 60)
            print("Transpiler starting...")
            print(f"Source: {self.source_dir}")
            print(f"Output: {self.target_dir}")
            print("=" * 60)

        try:
            start_time = datetime.now()

            # Save configuration
            self.create_transpiler_config()

            # Process Rust files
            source_files = list(self.source_dir.rglob("*"))
            if not source_files:
                print(f"Warning: No Rust files found in {self.source_dir}")

            for source_file in source_files:
                if self.should_process_file(source_file):
                    self.process_source_file(source_file, self.config['lang'], self.config['async'])

            end_time = datetime.now()
            duration = end_time - start_time
            self.processing_time = duration.total_seconds()
            # Generate manifest
            self.generate_manifest()

            # Summary
            if self.verbose:
                print("=" * 60)
                print("Transpiler completed successfully!")
                print(f"Processed {len(self.processed_files)} of {len(source_files)} source files")
                print(f"Generated {len(self.generated_files)} output files")
                print(f"Output directory: {self.target_dir}")
                print("=" * 60)

            return True

        except Exception as e:
            print(f"Transpiler failed: {e}", file=sys.stderr)
            import traceback
            traceback.print_exc()
            return False


def main():
    """Command-line interface for the transpiler."""
    parser = argparse.ArgumentParser(
        description="Transpiler for Rust to WebAssembly preprocessing",
        formatter_class=argparse.RawDescriptionHelpFormatter
    )

    parser.add_argument(
        "--source",
        required=True,
        type=Path,
        help="Source directory containing Rust files"
    )

    parser.add_argument(
        "--target",
        required=True,
        type=Path,
        help="Output target directory for generated files"
    )

    parser.add_argument(
        "--artifact",
        required=True,
        type=Path,
        help="Build artifact directory for generated files"
    )

    parser.add_argument(
        "--config",
        type=Path,
        default=None,
        help="TOML configuration file (optional)"
    )

    parser.add_argument(
        "--verbose",
        action="store_true",
        help="Enable verbose output"
    )

    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Show what would be processed without making changes"
    )

    parser.add_argument(
        "--package-desc",
        required=False,
        type=Path,
        default=None,
        help="Output procedure description file"
    )

    parser.add_argument(
        "--type-desc",
        required=False,
        type=Path,
        default=None,
        help="Custom type description file input"
    )

    args = parser.parse_args()

    # Validate arguments
    if not args.source.exists():
        print(f"Error: Source directory does not exist: {args.source}", file=sys.stderr)
        sys.exit(1)

    # Check for required Python packages
    try:
        import toml
    except ImportError:
        print("Error: 'toml' package is required. Install with: pip install toml", file=sys.stderr)
        sys.exit(1)

    # Check config file if provided
    if args.config and not args.config.exists():
        print(f"Warning: Config file does not exist: {args.config}")
        args.config = None

    if args.dry_run:
        print("DRY RUN MODE - No changes will be made")
        print(f"Would process files from: {args.source}")
        print(f"Would output to: {args.target}")

        # Count Rust files
        rust_files = list(args.source.rglob("*.rs"))
        print(f"Found {len(rust_files)} Rust files")

    # Initialize transpiler to check config
    transpiler = Transpiler(
        args.source, args.target, args.artifact, args.config,
        args.package_desc, args.type_desc, args.verbose)
    if args.dry_run:
        transpiler.dry_run()
        sys.exit(0)
    else:
        # Run transpiler
        success = transpiler.run()
        sys.exit(0 if success else 1)


if __name__ == "__main__":
    main()
