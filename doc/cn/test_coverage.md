# MuduDB 测试覆盖率指南

本指南介绍如何在本地和 CI 中收集、查看 MuduDB 的代码覆盖率，以及如何通过补充测试来提升覆盖率。

## 1. 覆盖率工具

MuduDB 使用 [`cargo-llvm-cov`](https://github.com/taiki-e/cargo-llvm-cov) 生成基于 LLVM source-based coverage 的报告。与基于 ptrace 采样的工具相比，它对 async/await、proc-macro 和 workspace 项目的映射更准确。

- **line coverage**：语句/行覆盖。
- **function coverage**：函数覆盖。
- **branch coverage**：分支覆盖（nightly 不稳定特性，**默认关闭**——pinned nightly 的 LLVM 在报告阶段处理 branch 数据可能 SIGSEGV，可用 `--branch` 显式开启）。

## 2. 前置条件

仓库已固定 nightly 工具链，例如 `nightly-2026-06-17`（见 `.rust-nightly-version`）。运行覆盖率前需要：

1. 已安装 `rustup`。
2. 已安装 pinned nightly（脚本会自动安装）。
3. 已安装 `llvm-tools-preview` 和 `cargo-llvm-cov`（脚本会自动安装）。

如果你已经运行过 `script/setup_dev_env.sh`，那么只需要再执行一次覆盖率脚本即可。

## 3. 一键运行覆盖率

```bash
cargo make coverage
```

该命令委托给跨平台 Python 脚本（支持 Linux / macOS / Windows）；直接调用脚本等价：

```bash
python3 script/coverage/run_coverage.py
```

默认行为：

- 覆盖范围：**全部五个 group workspace**（`crates/common`、`crates/db-kernel`、`crates/sdk`、`example/mpk`、`crates/tools`）的 unit test，**外加 `testing` crate 的端到端测试**。
- 报告：所有数据合并为**一份**报告（见下文"合并原理"）。
- 输出格式：`all`（同时生成 HTML、JSON、lcov）。
- 输出目录：`target/llvm-cov`（`html/index.html`、`coverage.json`、`coverage.lcov`）。
- branch coverage：**关闭**（`--branch` 显式开启；不稳定，见 §8.2）。
- unit test 固定 `--test-threads=1` 串行；e2e 保持默认并行（与 `cargo make test` 一致）。

### 合并原理

所有 group workspace 共享仓库根的 `target/` 目录，所有 profraw 都平铺落在 `target/llvm-cov-target/`。报告阶段用 pinned nightly 自带的 `llvm-profdata` 合并全部 profraw，再以"实际跑过的测试二进制"为 objects 用 `llvm-cov show/export` 生成单一报告（`cargo llvm-cov report` 本身无法跨 workspace 合并）。

### e2e 覆盖率

`testing` crate 的集成测试在**进程内**启动完整 server，因此 e2e 覆盖无需特殊处理：测试二进制的 profraw 照常落盘。e2e 阶段前脚本会尽力构建 `wallet.mpk`（缺失时相关测试软跳过）；e2e 结束后会把 `crates/db-kernel/testing/mpk/` 下已提交的夹具恢复为运行前的字节，因此 coverage 运行不会弄脏工作区（确需刷新夹具时走 `cargo make copy-wallet-mpk` 单独提交）。用 `--no-e2e` 可完全跳过 e2e；用 `--e2e-filter <子串>` 可做快速冒烟。

### 3.1 常用选项

```bash
# 只跑 unit test（快得多）
python3 script/coverage/run_coverage.py --no-e2e

# 只跑名字匹配子串的 e2e 测试（冒烟）
python3 script/coverage/run_coverage.py --e2e-filter string_literal

# 仅 HTML
python3 script/coverage/run_coverage.py -f html

# 强制开启 branch coverage（不稳定，可能崩溃）
python3 script/coverage/run_coverage.py --branch

# 自定义输出目录
python3 script/coverage/run_coverage.py -o /tmp/mudu-cov
```

完整选项见：

```bash
python3 script/coverage/run_coverage.py --help
```

## 4. 查看报告

### 4.1 HTML 报告

生成完成后，打开：

```bash
# 默认输出目录
xdg-open target/llvm-cov/html/index.html
```

报告会展示每个文件的行、函数、分支覆盖情况，并高亮未覆盖的代码。

### 4.2 JSON 摘要

`target/llvm-cov/coverage.json` 包含整体摘要：

```bash
python3 - <<'PY'
import json
with open('target/llvm-cov/coverage.json') as f:
    data = json.load(f)
totals = data['data'][0]['totals']
for key in ('lines', 'functions', 'branches'):
    print(f"{key}: {totals[key]['percent']:.2f}%")
PY
```

### 4.3 lcov

`target/llvm-cov/coverage.lcov` 可导入支持 lcov 的工具（如 VS Code 插件、genhtml）查看。

## 5. 手动命令

如需更精细的控制，可按脚本的流水线手动执行：

```bash
NIGHTLY=nightly-2026-06-17
LLVM_BIN="$(rustc +"${NIGHTLY}" --print sysroot)/lib/rustlib/$(rustc -vV | sed -n 's/^host: //p')/bin"

# 1. 逐 group workspace 收集（unit 阶段排除 testing crate）
(cd crates/common    && cargo +"${NIGHTLY}" llvm-cov --workspace --lib --tests --no-report -- --test-threads=1)
(cd crates/sdk       && cargo +"${NIGHTLY}" llvm-cov --workspace --lib --tests --no-report -- --test-threads=1)
(cd crates/tools     && cargo +"${NIGHTLY}" llvm-cov --workspace --lib --tests --no-report -- --test-threads=1)
(cd crates/db-kernel && cargo +"${NIGHTLY}" llvm-cov --workspace --exclude testing --lib --tests --no-report -- --test-threads=1)
(cd crates/db-kernel && cargo +"${NIGHTLY}" llvm-cov --package testing --lib --tests --no-report)  # e2e 阶段

# 2. 合并全部 profraw（都平铺在 target/llvm-cov-target/）
"${LLVM_BIN}/llvm-profdata" merge -sparse target/llvm-cov-target/*.profraw -o target/llvm-cov/merged.profdata

# 3. 以实际跑过的测试二进制为 objects 生成一份报告
"${LLVM_BIN}/llvm-cov" show   --instr-profile=target/llvm-cov/merged.profdata <test-binary> [-object <test-binary>...] -format=html --output-dir=target/llvm-cov/html
"${LLVM_BIN}/llvm-cov" export --instr-profile=target/llvm-cov/merged.profdata <test-binary> [-object <test-binary>...] -format=text > target/llvm-cov/coverage.json
"${LLVM_BIN}/llvm-cov" export --instr-profile=target/llvm-cov/merged.profdata <test-binary> [-object <test-binary>...] -format=lcov > target/llvm-cov/coverage.lcov
```

精确的 objects 清单来自用 `cargo +"${NIGHTLY}" llvm-cov show-env --export-prefix` 打印的环境变量（外加 `CARGO_TARGET_DIR=target/llvm-cov-target`）执行的 `cargo test --no-run --message-format=json`；完整细节见 `script/coverage/run_coverage.py`。

## 6. CI 中的覆盖率

`.github/workflows/nightly-checks.yaml` 中包含 `coverage` job：

- 每晚自动跑合并后的 unit test 覆盖率（e2e 仅限本地：`--no-e2e`）。
- 生成 HTML / JSON / lcov 并上传为 artifact（保留 30 天）。
- 在 job summary 中输出覆盖率摘要。

CI 失败时的本地复现命令会在日志中打印。

## 7. 通过补充测试提升覆盖率

新增测试文件并放到对应 crate 的 `src/` 下，以 `*_test.rs` 命名，然后在 `mod.rs` 或 `lib.rs` 中引入：

```rust
#[cfg(test)]
mod my_feature_test;
```

建议优先覆盖：

- 条件分支较多的工具函数（如 `case_convert`、`xid`、`result_of`）。
- 类型转换与边界检查（如 `scalar_type`、tuple binary/json 转换）。
- 当前 coverage 报告显示为 `0%` 的核心公共 API。

运行覆盖率后，通过 HTML 报告定位未覆盖代码，再针对性补测试。

## 8. 常见问题

### 8.1 报告中 branch coverage 显示为 `0/0`

`--branch` 需要在**测试收集阶段**和**报告生成阶段**都指定。如果只在测试阶段加 `--branch`、而 `report` 阶段未加，branch 列会显示 `0/0`。脚本和 CI 已自动处理，手动执行时请检查两阶段命令。

### 8.2 报告生成崩溃（SIGSEGV）

当前 pinned nightly 下，kernel crate 的 branch coverage 数据可能触发 LLVM bug，导致报告生成阶段 SIGSEGV（`getInstantiationGroups`）。因此 branch coverage **默认关闭**；如确有需要再用 `--branch` 显式开启：

```bash
python3 script/coverage/run_coverage.py --branch
```

`mudu`、`mudu_type`、`mudu_contract` 单独或合并使用 `--branch` 均正常。

### 8.3 `rustup` 下载失败

脚本已对 `rustup` 命令加入 3 次重试。如果仍失败，可清理后重试：

```bash
rm -rf ~/.rustup/downloads/*.partial
rustup toolchain uninstall nightly-2026-06-17 2>/dev/null || true
python3 script/coverage/run_coverage.py
```

### 8.4 首次运行较慢

首次运行需要安装 pinned nightly、llvm-tools-preview 和 cargo-llvm-cov，并重新编译依赖，耗时可能数分钟。后续运行会直接使用已安装的工具和缓存。

## 9. 相关文件

- `script/coverage/run_coverage.py`：本地覆盖率入口脚本（跨平台 Python，也可通过 `cargo make coverage` 调用）。
- `script/coverage/install_deps.py`：覆盖率工具链依赖安装脚本（由 run_coverage.py 自动调用）。
- `.github/workflows/nightly-checks.yaml`：CI 覆盖率 job。
- `.github/scripts/summarize_coverage.py`：覆盖率摘要脚本。
