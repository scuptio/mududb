# script/

Build, CI, benchmark, coverage, and SDK automation for the MuduDB
repository. Nothing here is part of a cargo workspace; see the root
[AGENTS.md](../AGENTS.md) for how these scripts fit into the check and
build flows.

| Path                                     | Purpose |
| ---------------------------------------- | ------- |
| [bench/](bench/)                         | TPC-C benchmark orchestration (`tpcc_benchmark.py`): single or multi-run benchmarks, backend comparison (iouring / tokio / legacy), TPS and latency plotting. |
| [build/](build/)                         | Binary installation (`install_binaries.py`, the backend of `cargo make install`), io_uring environment check (`inspect_iouring_env.sh`), cargo-make pre-processing hook (`transpiler.py`). |
| [ci/](ci/)                               | CI checks and the local verification suite: `check_all.py` (`cargo make check`: fmt + clippy + test-compile + invariants), layering enforcement (`check_arch.py`), production `test`-feature ban (`check_test_feature.py`), bindings regen gate (`check_bindings_regen.py`), shared dependency sync (`sync_workspace_deps.py`), secret scanning (`check_secrets.py`, `test_secret_scan.py`), format-compatibility release gate (`check_format_compatibility.py`), local GitHub Actions runner (`act.sh`), LSAN suppressions. |
| [coverage/](coverage/)                   | `run_coverage.py`: cross-platform local coverage runner (cargo-llvm-cov, pinned nightly) producing one merged unit+e2e report plus per-language bindings coverage (coverage.py / coverlet, best effort); `install_deps.py`: coverage toolchain dependency installer. |
| [install/](install/)                     | `mudup-init.sh`: POSIX-sh installer that downloads and installs released MuduDB binaries. |
| [lib/](lib/)                             | Shared Python helpers for the scripts here: `output.py` (cargo-style colored status output; plain text when piped), `cli.py` (`--` argument pass-through normalization). |
| [release/](release/)                     | Release packaging and publishing: `package_release.py` (server tarball, shared with the Release workflow / `cargo make package-release`), `pack_registry.py` (npm/NuGet/PyPI packing, credential-free), `publish.py` (unified verify+push entry, dry-run by default), unit tests in `tests/`. |
| [sdk/](sdk/)                             | SDK build and publish tooling: native adapter runtime build (`adapter_runtime.py`), backend smoke tests (`backend_smoke.py`), MPK smoke tests (`mpk_smoke.py`), distribution-repo build/publish (`sdk_build.py`, `publish.py`, `publish_runtime.py`), wallet helpers, per-language example fixtures (`examples/`). |
| [shell/](shell/)                         | Manual smoke/debug scripts documented in DEPLOY.md (`run_test.sh`, `debug_test.sh`), plus `install_deps.sh` — a compatibility wrapper delegating to `setup_dev_env.sh --system-pip`. |
| [verification/](verification/)           | Aeneas + Lean formal verification: `toolchain.py` (SHA256-pinned, user-level tool install, the backend of `cargo make doctor -- install --only formal-verification`), `check.py` (`cargo make verify` / `verify-extract` / `verify-self-test`: re-extract Lean from production Rust, reject drift, build proofs, audit axioms, inject a mutation that must be rejected), `reporting.py` (staged run logs), unit tests. |
| [ai_tool/](ai_tool/)                     | AI-assisted development prompts and helpers; not part of any build. |
| [setup_dev_env.sh](setup_dev_env.sh)     | Reproducible Ubuntu/Debian new-machine bootstrap: system dependencies (sudo), rustup, pinned stable and nightly toolchains, Python packages (`.venv` by default, `--system-pip` for deploy machines), cargo-make. Day-to-day toolchain checks: `cargo make doctor`. |
| [show_tasks.py](show_tasks.py)           | `cargo make help` backend: grouped, colored listing of the root Makefile.toml tasks (built-in cargo-make flows excluded). |
| [doctor.py](doctor.py)                   | `cargo make doctor` backend: probes the dev toolchain and reports what each tool unlocks; missing entries show a per-OS shell install command, a descriptive note, and a hyperlink to the tool's install docs; the `install` subcommand installs missing user-level tools (rustup/cargo/pip/pipx recipes and the pinned verification bundles, never sudo). |
