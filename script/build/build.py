#!/usr/bin/env python3
import argparse
import os
import platform
import shutil
import subprocess
from enum import Enum
from typing import List, Optional

import sys


class Platform(Enum):
    LINUX = "linux"
    WINDOWS = "windows"
    MACOS = "darwin"

class Color:
    """ANSI color codes (Windows uses colorama for compatibility)"""
    RED = '\033[0;31m'
    GREEN = '\033[0;32m'
    YELLOW = '\033[1;33m'
    BLUE = '\033[0;34m'
    NC = '\033[0m'  # No Color

class Logger:
    """Handles colored logging output with Windows compatibility"""

    def __init__(self):
        self.system = platform.system().lower()
        self.init_colors()

    def init_colors(self):
        """Initialize color support for Windows"""
        if self.system == "windows":
            try:
                import colorama
                colorama.init()
                # Map ANSI codes to colorama
                self.RED = colorama.Fore.RED
                self.GREEN = colorama.Fore.GREEN
                self.YELLOW = colorama.Fore.YELLOW
                self.BLUE = colorama.Fore.BLUE
                self.NC = colorama.Style.RESET_ALL
            except ImportError:
                # Fallback: disable colors on Windows if colorama not available
                self.RED = self.GREEN = self.YELLOW = self.BLUE = self.NC = ""

    def info(self, message: str):
        print(f"{self.BLUE}[INFO]{self.NC} {message}")

    def success(self, message: str):
        print(f"{self.GREEN}[SUCCESS]{self.NC} {message}")

    def warning(self, message: str):
        print(f"{self.YELLOW}[WARNING]{self.NC} {message}")

    def error(self, message: str):
        print(f"{self.RED}[ERROR]{self.NC} {message}")

class PackageManager:
    """Abstract package manager for different platforms"""

    def __init__(self, platform_name: str):
        self.platform = platform_name
        self.logger = Logger()

    def install_dependencies(self, verbose: bool) -> bool:
        """Install platform-specific dependencies"""
        raise NotImplementedError()

    def update_packages(self, verbose: bool) -> bool:
        """Update package list"""
        raise NotImplementedError()

class AptPackageManager(PackageManager):
    """APT package manager for Linux"""

    def update_packages(self, verbose: bool) -> bool:
        cmd = "sudo apt-get update -y"
        if not verbose:
            cmd += " > /dev/null 2>&1"
        return self.run_command(cmd)

    def install_dependencies(self, verbose: bool) -> bool:
        deps = [
            "build-essential",
            "curl",
            "liburing-dev"
        ]

        cmd = f"sudo apt-get install -y {' '.join(deps)}"
        if not verbose:
            cmd += " > /dev/null 2>&1"

        return self.run_command(cmd)

    def run_command(self, command: str) -> bool:
        try:
            subprocess.run(command, shell=True, check=True)
            return True
        except subprocess.CalledProcessError as e:
            self.logger.error(f"Command failed: {e}")
            return False

class ChocoPackageManager(PackageManager):
    """Chocolatey package manager for Windows"""

    def __init__(self, platform_name: str):
        super().__init__(platform_name)
        self.check_chocolatey()

    def check_chocolatey(self):
        """Check if Chocolatey is installed"""
        try:
            subprocess.run(["choco", "--version"],
                           check=True,
                           capture_output=True,
                           shell=True)
        except (subprocess.CalledProcessError, FileNotFoundError):
            self.logger.warning("Chocolatey not found. Attempting to install...")
            self.install_chocolatey()

    def install_chocolatey(self) -> bool:
        """Install Chocolatey package manager"""
        self.logger.info("Installing Chocolatey...")

        # PowerShell command to install Chocolatey
        ps_command = """
        Set-ExecutionPolicy Bypass -Scope Process -Force;
        [System.Net.ServicePointManager]::SecurityProtocol = [System.Net.ServicePointManager]::SecurityProtocol -bor 3072;
        iex ((New-Object System.Net.WebClient).DownloadString('https://community.chocolatey.org/install.ps1'))
        """

        try:
            subprocess.run(["powershell", "-Command", ps_command],
                           check=True,
                           capture_output=True)
            self.logger.success("Chocolatey installed successfully")
            return True
        except subprocess.CalledProcessError as e:
            self.logger.error(f"Failed to install Chocolatey: {e}")
            return False

    def update_packages(self, verbose: bool) -> bool:
        # Chocolatey doesn't need explicit update before install
        return True

    def install_dependencies(self, verbose: bool) -> bool:
        """Install Windows dependencies"""
        deps = [
            "cmake",
            "python",
            "rust"  # Install Rust via Chocolatey if not present
        ]

        for dep in deps:
            cmd = f"choco install {dep} -y"
            if not verbose:
                cmd += " --no-progress"

            self.logger.info(f"Installing {dep}...")
            try:
                result = subprocess.run(cmd,
                                        shell=True,
                                        check=True,
                                        capture_output=not verbose)
                if result.returncode != 0:
                    self.logger.error(f"Failed to install {dep}")
                    return False
            except subprocess.CalledProcessError as e:
                self.logger.error(f"Failed to install {dep}: {e}")
                return False

        return True

class BrewPackageManager(PackageManager):
    """Homebrew package manager for macOS"""

    def update_packages(self, verbose: bool) -> bool:
        cmd = "brew update"
        if not verbose:
            cmd += " > /dev/null 2>&1"
        return self.run_command(cmd)

    def install_dependencies(self, verbose: bool) -> bool:
        deps = [
            "cmake",
            "pkg-config"
        ]

        for dep in deps:
            cmd = f"brew install {dep}"
            if not verbose:
                cmd += " > /dev/null 2>&1"

            if not self.run_command(cmd):
                return False

        return True

    def run_command(self, command: str) -> bool:
        try:
            subprocess.run(command, shell=True, check=True)
            return True
        except subprocess.CalledProcessError as e:
            self.logger.error(f"Command failed: {e}")
            return False

class PlatformManager:
    """Manages platform-specific operations"""

    def __init__(self):
        self.system = platform.system().lower()
        self.logger = Logger()

        # Map system to platform enum
        if self.system == "linux":
            self.platform = Platform.LINUX
        elif self.system == "windows":
            self.platform = Platform.WINDOWS
        elif self.system == "darwin":
            self.platform = Platform.MACOS
        else:
            self.logger.warning(f"Unsupported platform: {self.system}")
            self.platform = None

        # Create appropriate package manager
        self.package_manager = self.create_package_manager()

    def create_package_manager(self) -> Optional[PackageManager]:
        """Create platform-specific package manager"""
        if self.platform == Platform.LINUX:
            return AptPackageManager("linux")
        elif self.platform == Platform.WINDOWS:
            return ChocoPackageManager("windows")
        elif self.platform == Platform.MACOS:
            return BrewPackageManager("darwin")
        return None

    def get_binary_extension(self) -> str:
        """Get platform-specific binary extension"""
        if self.platform == Platform.WINDOWS:
            return ".exe"
        return ""

    def get_target_directory(self) -> str:
        """Get platform-specific target directory"""
        if self.platform == Platform.WINDOWS:
            # Windows typically doesn't have /tmp like Unix systems
            return os.path.join(os.environ.get("TEMP", "C:\\Windows\\Temp"), "rust-build")
        return "/tmp/rust-build"

    def is_admin(self) -> bool:
        """Check if running with admin/root privileges"""
        if self.platform == Platform.WINDOWS:
            try:
                import ctypes
                return ctypes.windll.shell32.IsUserAnAdmin() != 0
            except:
                return False
        else:
            return os.geteuid() == 0

    def check_required_tools(self) -> List[str]:
        """Check for required tools on the platform"""
        missing_tools = []

        # Common tools
        tools_to_check = ["cargo", "rustc"]

        if self.platform == Platform.WINDOWS:
            # Additional Windows-specific checks
            tools_to_check.extend(["link"])  # MSVC tools

        for tool in tools_to_check:
            if shutil.which(tool) is None:
                missing_tools.append(tool)

        return missing_tools


def parse_args() -> argparse.Namespace:
    """Parse command line arguments"""
    parser = argparse.ArgumentParser(
        description=f"Cross-platform Build Script for Rust Project (Running on: {platform.system()})",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=f"""
Platform-specific notes:
Windows: Requires Chocolatey for package management and Visual Studio Build Tools
Linux: Requires apt-get and standard build tools
macOS: Requires Homebrew

Examples:
build.py                                        # Default, build project and test(equal build.py --build --test)
build.py --prerequisite                         # Install prerequisite and setup toolchains
build.py --build                                # Build project (with release)
build.py --build --mode debug --features X      # Debug build with X feature
build.py --clean --test                         # Clean cache and run tests
"""
    )


    parser.add_argument(
        "-p", "--prerequisite",
        action="store_true",
        help="Install prerequisite and setup"
    )
    parser.add_argument(
        "--install-deps",
        action="store_true",
        help="Skip installing dependencies"
    )
    parser.add_argument(
        "--install-rust",
        action="store_true",
        help="Install rustup"
    )
    parser.add_argument(
        "--setup-rust",
        action="store_true",
        help="Setup rust"
    )
    parser.add_argument(
        "--install-tools",
        action="store_true",
        help="Install mudu tools"
    )

    parser.add_argument(
        "-m", "--mode",
        choices=["debug", "release"],
        help="Build mode (debug, release) [default: release]"
    )
    parser.add_argument(
        "-f", "--features",
        help="Build with specific features (comma-separated)"
    )
    parser.add_argument(
        "-t", "--toolchain",
        help="Rust toolchain to use [default: nightly]"
    )
    parser.add_argument(
        "-c", "--clean",
        action="store_true",
        help="Clean cargo cache before building"
    )
    parser.add_argument(
        "--test",
        action="store_true",
        help="Running test"
    )
    parser.add_argument(
        "-v", "--verbose",
        action="store_true",
        help="Enable verbose output"
    )
    parser.add_argument(
        "-b", "--build",
        action="store_true",
        help="Build the project"
    )
    parser.add_argument(
        "-k", "--package",
        action="store_true",
        help="Create example .mpk package"
    )

    return parser.parse_args()


class BuildScript:
    """Cross-platform build script for Rust projects"""

    def __init__(self):
        self.logger = Logger()
        self.platform_manager = PlatformManager()
        self.args = parse_args()
        self.targets = ["wasm32-wasip1", "wasm32-wasip2"]
        # Set default values
        self.build_mode = "release"
        self.toggle_clean_cache = self.args.clean
        self.toggle_run_tests = self.args.test
        self.toggle_install_deps = self.args.install_deps
        self.toggle_install_rust = self.args.install_rust
        self.toggle_setup_rust = self.args.setup_rust
        self.toggle_build_project = self.args.build
        self.toggle_build_package = self.args.package
        self.toggle_install_tools = self.args.install_tools
        self.verbose = self.args.verbose
        self.build_features = ""

        self.rust_toolchain = "nightly"
        if len(sys.argv) == 1:
            self.toggle_build_project = True
            self.toggle_run_tests = True
        if self.args.mode:
            self.build_mode = self.args.mode
        if self.args.prerequisite:
            self.toggle_install_deps = True
            self.toggle_install_rust = True
            self.toggle_setup_rust = True
            self.toggle_install_tools = True
        if self.args.features:
            self.build_features = self.args.features
        if self.args.toolchain:
            self.rust_toolchain = self.args.toolchain

        # Platform-specific settings
        self.is_windows = self.platform_manager.platform == Platform.WINDOWS
        self.binary_ext = self.platform_manager.get_binary_extension()

    def run_command(self, command: str, capture_output: bool = None) -> bool:
        """Execute shell command with real-time output only"""
        if capture_output is None:
            capture_output = not self.verbose

        self.logger.info(f"Running: {command}")

        try:
            shell = True

            process = subprocess.Popen(
                command,
                shell=shell,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
                encoding='utf-8',
                bufsize=1,
                universal_newlines=True
            )

            while capture_output:
                output = process.stdout.readline()
                if output == '' and process.poll() is not None:
                    break
                if output:
                    print(output.rstrip())
                    sys.stdout.flush()

            return_code = process.poll()

            if return_code == 0:
                return True
            else:
                self.logger.error(f"Command failed with code: {return_code}")
                return False

        except Exception as e:
            self.logger.error(f"Failed to run command: {e}")
            return False

    def install_dependencies(self) -> bool:
        """Install platform-specific dependencies"""
        if not self.args.toggle_install_deps:
            return True

        self.logger.info(f"Installing {self.platform_manager.platform.value} dependencies...")

        if not self.platform_manager.package_manager:
            self.logger.error(f"No package manager available for {self.platform_manager.platform.value}")
            return False

        # Check admin privileges
        if not self.platform_manager.is_admin():
            if self.is_windows:
                self.logger.warning("Administrator privileges required for installing dependencies on Windows")
                self.logger.info("Please run this script as Administrator")
                return False
            else:
                self.logger.warning("Root privileges required for installing system dependencies")
                self.logger.info("Please run with sudo or as root")
                return False

        # Update packages
        if not self.platform_manager.package_manager.update_packages(self.verbose):
            self.logger.error("Failed to update packages")
            return False

        # Install dependencies
        if not self.platform_manager.package_manager.install_dependencies(self.verbose):
            self.logger.error("Failed to install dependencies")
            return False

        self.logger.success("Dependencies installed successfully")
        return True

    def install_rust(self) -> bool:
        """Install Rust if not present"""
        if not self.toggle_install_rust:
            return True

        # Check if rustup is installed
        rustup_check = subprocess.run(
            "rustup --version" if not self.is_windows else "rustup --version",
            shell=True,
            capture_output=True,
            text=True
        )

        if rustup_check.returncode != 0:
            self.logger.info("Rust not found. Installing rustup...")

            if self.is_windows:
                # Windows installation
                install_cmd = "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y"
                if not self.verbose:
                    install_cmd += " >nul 2>&1"
            else:
                # Unix-like installation
                install_cmd = "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y"
                if not self.verbose:
                    install_cmd += " > /dev/null 2>&1"

            if not self.run_command(install_cmd):
                self.logger.error("Failed to install Rust")
                return False

            # Add cargo to PATH (simplified)
            if self.is_windows:
                cargo_bin = os.path.join(os.environ.get("USERPROFILE", ""), ".cargo", "bin")
                if cargo_bin not in os.environ["PATH"]:
                    os.environ["PATH"] = cargo_bin + ";" + os.environ["PATH"]

            self.logger.success("Rust installed successfully")
        else:
            self.logger.info("Rust is already installed")

        return True

    def setup_toolchain(self) -> bool:
        """Setup Rust toolchain"""
        if not self.toggle_setup_rust:
            return True

        self.logger.info(f"Setting up Rust toolchain: {self.rust_toolchain}")

        commands = [
            f"rustup toolchain install {self.rust_toolchain}",
            f"rustup default {self.rust_toolchain}",
            f"rustup component add rustfmt --toolchain {self.rust_toolchain}",
            f"rustup update {self.rust_toolchain}"
        ]

        for target in self.targets:
            commands.append(f"rustup target add {target}")

        for cmd in commands:
            if not self.run_command(cmd):
                self.logger.error(f"Failed to setup toolchain: {cmd}")
                return False

        self.logger.success("Toolchain setup completed")
        return True

    def clean_cache(self) -> bool:
        """Clean cargo cache if requested"""
        if not self.toggle_clean_cache:
            return True

        self.logger.info("Cleaning cargo cache...")

        if not self.run_command("cargo clean"):
            self.logger.error("Failed to clean cache")
            return False

        self.logger.success("Cache cleaned")

        return True

    def build_project(self) -> bool:
        """Build the project"""
        if not self.toggle_build_project:
            return True

        self.logger.info(f"Building project (mode: {self.build_mode})...")

        build_cmd = "cargo build"

        # Add mode flag
        if self.build_mode == "release":
            build_cmd += " --release"

        # Add features if specified
        if self.build_features:
            self.logger.info(f"Building with features: {self.build_features}")
            build_cmd += f" --features {self.build_features}"

        # Platform-specific adjustments

        # Execute build
        if not self.run_command(build_cmd, capture_output=True):
            self.logger.error("Build failed")
            return False

        self.show_artifact()
        self.logger.success("Build completed successfully")
        return True

    def run_tests(self) -> bool:
        """Run tests if requested"""
        if not self.toggle_run_tests:
            return True

        self.logger.info("Running tests...")

        test_cmd = "cargo test"

        if self.build_mode == "release":
            test_cmd += " --release"

        if not self.run_command(test_cmd):
            self.logger.error("Tests failed")
            return False

        self.logger.success("All tests passed")

        return True

    def install_tools(self):
        """Run install mudu-tools"""
        if not self.toggle_install_tools:
            return True

        for path in [
            "mudu_gen",
            "mudu_transpiler",
            "mudu_package"]:
            cmd = "cargo install --force --path {}".format(path)
            if not self.run_command(cmd, capture_output=True):
                self.logger.error("{} Build failed".format(path))
                return False
        if self.verbose:
            self.logger.success("Install tools completed successfully")

        return True

    def build_package(self):
        """Build mpk package"""
        if not self.toggle_build_package:
            return True

        work_dir = os.getcwd()

        for path in [
            "mudu_wasm",
            "example/vote",
            "example/wallet"
        ]:
            os.chdir(path)
            cmd = "cargo make package".format(path)
            if not self.run_command(cmd, capture_output=True):
                self.logger.error("{}, build package failed".format(path))
                return False
            os.chdir(work_dir)
        self.show_artifact()

        return True

    def show_configuration(self):
        """Display build configuration"""
        self.logger.info(f"Platform: {self.platform_manager.platform.value}")
        self.logger.info("Build configuration:")
        self.logger.info(f"  Mode: {self.build_mode}")
        self.logger.info(f"  Toolchain: {self.rust_toolchain}")
        self.logger.info(f"  Clean cache: {self.clean_cache}")
        self.logger.info(f"  Run tests: {self.run_tests}")
        self.logger.info(f"  Features: {self.build_features if self.build_features else 'none'}")
        self.logger.info(f"  Verbose: {self.verbose}")
        if self.args.install_deps:
            self.logger.info(f"  Install dependencies: {self.args.install_deps}")

    def check_environment(self) -> bool:
        """Check if all required tools are available"""
        missing_tools = self.platform_manager.check_required_tools()

        if missing_tools:
            self.logger.warning(f"Missing tools: {', '.join(missing_tools)}")
            self.logger.info("Attempting to install missing tools...")

            # Try to install rust via package manager if missing
            if "cargo" in missing_tools or "rustc" in missing_tools:
                return self.install_rust()

        return True

    def show_artifact(self):
        # Show binary location
        target_dir_list = [f"target/{self.build_mode}/"]
        for target in self.targets:
            target_dir_list.append(f"target/{target}/{self.build_mode}/")

        self.logger.info("Generated files:")
        for target_dir in target_dir_list:
            if self.verbose:
                self.logger.info(f"Binary location: {target_dir}")
            # List generated binaries
            if os.path.exists(target_dir):
                binaries = [f for f in os.listdir(target_dir)
                            if f.endswith(self.binary_ext) or
                            f.endswith('.dll') or
                            f.endswith('.so') or
                            f.endswith('.wasm') or
                            f.endswith('.mpk')
                            ]
                if binaries:
                    for binary in binaries:
                        self.logger.info(f"  - {target_dir}{binary}")

    def run(self) -> bool:
        """Main execution flow"""
        self.logger.info(f"Starting {self.platform_manager.platform.value} build process...")

        current_dir = os.getcwd()
        if os.path.exists(current_dir.join(".project.home")):
            self.logger.error(f"Build script must be run in the project home directory")
            return False

        self.show_configuration()

        # Check environment first
        if not self.check_environment():
            return False

        steps = [
            ("Installing dependencies", self.install_dependencies),
            ("Installing Rust", self.install_rust),
            ("Setting up toolchain", self.setup_toolchain),
            ("Cleaning cache", self.clean_cache),
            ("Building project", self.build_project),
            ("Running tests", self.run_tests),
            ("Install tools", self.install_tools),
            ("Build package", self.build_package)
        ]

        # Skip dependency installation if requested
        if self.args.install_deps:
            steps = steps[0:]
        else:
            steps = steps[1:] # Remove first step
        for step_name, step_func in steps:
            self.logger.info(f"Step: {step_name}")
            if not step_func():
                self.logger.error(f"Failed at step: {step_name}")
                return False

        self.logger.success("Build process completed successfully!")
        print("")

        return True

def main():
    """Entry point"""
    script = BuildScript()

    try:
        success = script.run()
        sys.exit(0 if success else 1)
    except KeyboardInterrupt:
        script.logger.warning("Build interrupted by user")
        sys.exit(1)
    except Exception as e:
        script.logger.error(f"Unexpected error: {e}")
        import traceback
        if script.verbose:
            traceback.print_exc()
        sys.exit(1)

if __name__ == "__main__":
    main()