# script/

Build, CI, benchmark, coverage, and SDK automation for the MuduDB
repository. Nothing here is part of a cargo workspace; see the root
[AGENTS.md](../AGENTS.md) for how these scripts fit into the check and
build flows.

| Path                                     | Purpose |
| ---------------------------------------- | ------- |
| [bench/](bench/)                         | TPC-C benchmark orchestration (`tpcc_benchmark.py`): single or multi-run benchmarks, backend comparison (iouring / tokio / legacy), TPS and latency plotting. |
| [build/](build/)                         | Release build driver (`build.py`), binary installation (`install_binaries.py`), io_uring environment check (`inspect_iouring_env.sh`), cargo-make pre-processing hook (`transpiler.py`). |
| [build-cfg/](build-cfg/)                 | `package-list.json`: the example packages included in release/e2e package builds. |
| [ci/](ci/)                               | CI checks: layering enforcement (`check_arch.py`), production `test`-feature ban (`check_test_feature.py`), bindings regen gate (`check_bindings_regen.{py,sh}`), shared dependency sync (`sync_workspace_deps.py`), secret scanning (`check_secrets.sh`, `test_secret_scan.py`), format-compatibility release gate (`check_format_compatibility.sh`), local GitHub Actions runner (`act.sh`), LSAN suppressions. |
| [coverage/](coverage/)                   | `run_coverage.sh`: local coverage runner using cargo-llvm-cov with the pinned nightly toolchain. |
| [install/](install/)                     | `mudup-init.sh`: POSIX-sh installer that downloads and installs released MuduDB binaries. |
| [sdk/](sdk/)                             | SDK build and publish tooling: native adapter runtime build (`adapter_runtime.py`), backend smoke tests (`backend_smoke.py`), MPK smoke tests (`mpk_smoke.py`), distribution-repo build/publish (`sdk_build.py`, `publish.py`, `publish_runtime.py`), wallet helpers, per-language example fixtures (`examples/`). |
| [shell/](shell/)                         | Orchestration shell scripts: `check_all.sh` (fmt + clippy + test-compile across the group workspaces), `build_all.sh`, `run_test.sh`, `regenerate_examples.sh`, `install_deps.sh`. |
| [ai_tool/](ai_tool/)                     | AI-assisted development prompts and helpers; not part of any build. |
| [setup_dev_env.sh](setup_dev_env.sh)     | Reproducible Ubuntu/Debian development-environment setup: system dependencies, rustup, pinned stable and nightly toolchains, Python venv, cargo-make. |
