# MuduDB 测试覆盖率指南

本指南介绍如何在本地和 CI 中收集、查看 MuduDB 的代码覆盖率，以及如何通过补充测试来提升覆盖率。

## 1. 覆盖率工具

MuduDB 使用 [`cargo-llvm-cov`](https://github.com/taiki-e/cargo-llvm-cov) 生成基于 LLVM source-based coverage 的报告。与基于 ptrace 采样的工具相比，它对 async/await、proc-macro 和 workspace 项目的映射更准确。

- **line coverage**：语句/行覆盖。
- **function coverage**：函数覆盖。
- **branch coverage**：分支覆盖（nightly 不稳定特性，默认开启，可关闭）。

## 2. 前置条件

仓库已固定 nightly 工具链，例如 `nightly-2026-06-17`（见 `.rust-nightly-version`）。运行覆盖率前需要：

1. 已安装 `rustup`。
2. 已安装 pinned nightly（脚本会自动安装）。
3. 已安装 `llvm-tools-preview` 和 `cargo-llvm-cov`（脚本会自动安装）。

如果你已经运行过 `script/setup_dev_env.sh`，那么只需要再执行一次覆盖率脚本即可。

## 3. 一键运行覆盖率

```bash
script/coverage/run_coverage.sh
```

默认行为：

- 覆盖范围：`core` profile，即 `mudu`、`mudu_type`、`mudu_contract`、`mudu_kernel` 四个核心 crate。
- 输出格式：`all`（同时生成 HTML、JSON、lcov）。
- 输出目录：`target/llvm-cov`。
- branch coverage：**开启**（`--branch`）。
- 测试线程：固定为 `--test-threads=1`，避免并发导致的不确定性。

> `workspace` profile 默认关闭 branch coverage（`--no-branch`），因为当前 pinned nightly 下包含 `mudu_kernel` 的合并报告会触发 LLVM SIGSEGV。如需强制开启，可显式加 `--branch`。

### 3.1 常用选项

```bash
# 关闭 branch coverage
script/coverage/run_coverage.sh --no-branch

# 全 workspace，仅 HTML（默认 line coverage）
script/coverage/run_coverage.sh -p workspace -f html

# 全 workspace，强制 branch coverage（可能崩溃）
script/coverage/run_coverage.sh -p workspace --branch

# 核心 crate，仅 JSON 摘要
script/coverage/run_coverage.sh -p core -f json

# 自定义输出目录
script/coverage/run_coverage.sh -o /tmp/mudu-cov
```

完整选项见：

```bash
script/coverage/run_coverage.sh --help
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

如果你希望更精细地控制，可以使用 `cargo llvm-cov` 直接运行。

### 5.1 基本流程

`cargo llvm-cov` 不能在一次命令中多次使用 `--output-path`，因此多格式报告需要分两步：先用 `--no-report` 跑测试收集数据，再用 `cargo llvm-cov report` 生成具体格式。

```bash
export NIGHTLY_TOOLCHAIN="nightly-2026-06-17"
export CARGO_INCREMENTAL=0

# 第一步：收集覆盖率数据
cargo +"${NIGHTLY_TOOLCHAIN}" llvm-cov \
  --package mudu \
  --package mudu_type \
  --package mudu_contract \
  --package mudu_kernel \
  --lib --tests \
  --no-report \
  --branch \
  -- \
  --test-threads=1

# 第二步：生成报告
cargo +"${NIGHTLY_TOOLCHAIN}" llvm-cov report \
  --branch \
  --html --output-dir ./target/llvm-cov/html \
  --json --output-path ./target/llvm-cov/coverage.json \
  --lcov --output-path ./target/llvm-cov/coverage.lcov
```

### 5.2 关闭 branch coverage

去掉所有 `--branch` 即可。此时报告中的 branch 列会显示为 `0/0`。

### 5.3 只生成单一格式

```bash
# 仅 HTML
cargo +"${NIGHTLY_TOOLCHAIN}" llvm-cov \
  --package mudu --package mudu_type --package mudu_contract --package mudu_kernel \
  --lib --tests --branch \
  --html --output-dir ./target/llvm-cov/html \
  -- --test-threads=1

# 仅 JSON
cargo +"${NIGHTLY_TOOLCHAIN}" llvm-cov \
  --package mudu --package mudu_type --package mudu_contract --package mudu_kernel \
  --lib --tests --branch \
  --json --output-path ./target/llvm-cov/coverage.json \
  -- --test-threads=1
```

## 6. CI 中的覆盖率

`.github/workflows/nightly-checks.yaml` 中包含 `coverage` job：

- 每晚自动对 `core` profile 跑覆盖率。
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

### 8.2 运行 `core` profile 时 `llvm-cov export` 崩溃

当前 pinned nightly 下，`mudu_kernel` 的 branch coverage 数据可能触发 LLVM bug，导致 `llvm-cov export` SIGSEGV。此时请关闭 branch coverage：

```bash
script/coverage/run_coverage.sh --no-branch
```

`mudu`、`mudu_type`、`mudu_contract` 单独或合并使用 `--branch` 均正常。

### 8.3 `rustup` 下载失败

脚本已对 `rustup` 命令加入 3 次重试。如果仍失败，可清理后重试：

```bash
rm -rf ~/.rustup/downloads/*.partial
rustup toolchain uninstall nightly-2026-06-17 2>/dev/null || true
script/coverage/run_coverage.sh
```

### 8.4 首次运行较慢

首次运行需要安装 pinned nightly、llvm-tools-preview 和 cargo-llvm-cov，并重新编译依赖，耗时可能数分钟。后续运行会直接使用已安装的工具和缓存。

## 9. 相关文件

- `script/coverage/run_coverage.sh`：本地覆盖率入口脚本。
- `.github/workflows/nightly-checks.yaml`：CI 覆盖率 job。
- `.github/scripts/summarize_coverage.py`：覆盖率摘要脚本。
- `doc/cn/todo/coverage-tracking.md`：覆盖率机制设计文档。
