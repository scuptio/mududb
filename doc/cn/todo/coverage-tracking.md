# MuduDB 代码覆盖率跟踪机制设计

> 对应 `todo.md` 第 13 项：引入代码覆盖率跟踪。
> 目标：为 `mududb` workspace 建立一套基于 `cargo llvm-cov` + 固定 nightly 工具链的覆盖率报告机制，作为 nightly CI 的一部分持续运行。

## 1. 目标与非目标

### 1.1 目标
- 使用 `cargo llvm-cov` 生成基于 LLVM source-based coverage 的覆盖率报告。
- 复用仓库已有的固定 nightly 工具链 `nightly-2026-06-17`（见 `.rust-nightly-version`）。
- 在 nightly CI 中自动生成 HTML / JSON / `lcov` 三种格式的报告，并上传为 GitHub Actions artifact。
- 第一阶段优先覆盖核心 crate：`mudu`、`mudu_type`、`mudu_contract`、`mudu_kernel`。
- 为开发者提供可本地复现的一键命令。

### 1.2 非目标
- 不将覆盖率作为 PR 合并门槛（第一阶段不设阈值拦截）。
- 不引入第三方 SaaS（Codecov / Coveralls）服务，报告先以内置 artifact 形式保存。
- 不修改业务源码来迎合覆盖率数字。

## 2. 工具链选择：cargo llvm-cov

| 维度 | cargo llvm-cov | cargo tarpaulin |
|---|---|---|
| 底层技术 | LLVM source-based instrumentation | ptrace / LD_PRELOAD 采样 |
| async/await 准确性 | 高，能正确映射 `await` 点 | 在复杂异步路径上可能漏报 |
| proc-macro / build script | 支持 | 支持有限 |
| 与 workspace 兼容性 | 好，支持 `--workspace`、`--package` | 部分 crate 需要排除 |
| 工具链要求 | 需要 nightly LLVM | stable 即可 |
| 已有依赖 | 仓库 nightly CI 已装 `llvm-dev` | 额外依赖 |

结论：选用 `cargo llvm-cov`，因为它与现有 nightly 基础设施最匹配，且对数据库内核中大量 async 代码更精确。

## 3. 环境前提

### 3.1 已具备条件
- `.rust-nightly-version` 固定为 `nightly-2026-06-17`。
- `.github/actions/rust-nightly-setup` 已安装 `llvm-dev`、`clang`、`libclang-dev`。
- 仓库已有 `cargo fuzz` 覆盖率经验（见 `mudu_kernel/fuzz/fuzz.md`），团队对 `llvm-tools-preview` 不陌生。

### 3.2 需要额外安装的组件
在 nightly 工具链上添加 `llvm-tools-preview`：

```bash
rustup component add llvm-tools-preview --toolchain nightly-2026-06-17
```

安装 `cargo-llvm-cov`（建议锁定版本以保证可复现）：

```bash
cargo +nightly-2026-06-17 install cargo-llvm-cov --version 0.8.7 --locked
```

> 版本号 0.8.7 为当前较新稳定版，与 pinned nightly `nightly-2026-06-17` 兼容；CI 与本地脚本均已锁定该版本。

### 3.3 环境配置步骤

如果你尚未安装 pinned nightly 工具链，按以下步骤配置：

#### 方式一：运行仓库提供的开发环境初始化脚本（推荐）

```bash
script/setup_dev_env.sh
```

该脚本会自动安装 stable/nightly 工具链、系统依赖以及 Python venv，是本地开发的最简入口。

#### 方式二：手动安装

1. 安装 pinned nightly 工具链：

```bash
NIGHTLY_TOOLCHAIN="$(tr -d '[:space:]' < .rust-nightly-version)"
rustup toolchain install "${NIGHTLY_TOOLCHAIN}" --profile minimal
rustup target add x86_64-unknown-linux-gnu --toolchain "${NIGHTLY_TOOLCHAIN}"
```

2. 安装 LLVM tools 组件：

```bash
rustup component add llvm-tools-preview --toolchain "${NIGHTLY_TOOLCHAIN}"
```

3. 安装 `cargo-llvm-cov`：

```bash
cargo +"${NIGHTLY_TOOLCHAIN}" install cargo-llvm-cov --version 0.8.7 --locked
```

#### 方式三：直接运行覆盖率脚本（自动补全依赖）

```bash
script/coverage/run_coverage.sh
```

该脚本会自动完成上述 1-3 步（安装 nightly 工具链、`llvm-tools-preview`、`cargo-llvm-cov`），适合已经安装过 `rustup` 的环境。

## 4. 本地开发工作流

### 4.1 基本流程：先跑测试，再生成报告

`cargo llvm-cov` 不允许同时多次使用 `--output-path`，因此多格式报告需要分两步：先用 `--no-report` 跑一次测试生成原始覆盖率数据，再用 `cargo llvm-cov report` 生成具体格式。默认启用 branch coverage（`--branch`）， nightly 工具链支持该不稳定特性。

```bash
export NIGHTLY_TOOLCHAIN="nightly-2026-06-17"
export CARGO_INCREMENTAL=0

# 第一步：运行测试并收集覆盖率数据（不生成报告）
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

# 第二步：分别生成 HTML / JSON / lcov 报告（同样带 --branch）
cargo +"${NIGHTLY_TOOLCHAIN}" llvm-cov report \
  --branch \
  --html --output-dir ./target/llvm-cov/html \
  --json --output-path ./target/llvm-cov/coverage.json \
  --lcov --output-path ./target/llvm-cov/coverage.lcov
```

如果不需要 branch coverage，去掉所有 `--branch` 即可；若去掉，则后续报告中的 branch 列会显示为 `0/0`。

> **注意**：当前 pinned nightly 下的 `cargo-llvm-cov` 在生成包含 `mudu_kernel` 的合并报告时，`--branch` 可能触发 `llvm-cov export` SIGSEGV（LLVM bug）。对 `mudu_kernel` 单独跑 `--no-branch` 可正常生成 line coverage；对 `mudu`、`mudu_type`、`mudu_contract` 单独或合并跑 `--branch` 均正常。因此 `core` profile 默认仍会尝试 `--branch`，如遇崩溃请使用 `run_coverage.sh --no-branch`。

报告入口：`target/llvm-cov/html/index.html`。

### 4.2 仅生成单一格式

如果只需要 HTML：

```bash
cargo +"${NIGHTLY_TOOLCHAIN}" llvm-cov \
  --package mudu --package mudu_type --package mudu_contract --package mudu_kernel \
  --lib --tests \
  --branch \
  --html --output-dir ./target/llvm-cov/html \
  -- --test-threads=1
```

如果只需要 JSON 摘要：

```bash
cargo +"${NIGHTLY_TOOLCHAIN}" llvm-cov \
  --package mudu --package mudu_type --package mudu_contract --package mudu_kernel \
  --lib --tests \
  --branch \
  --json --output-path ./target/llvm-cov/coverage.json \
  -- --test-threads=1
```

`coverage.json` 中 `data[*].totals.lines.percent` 等字段可直接用于趋势比较。

### 4.5 本地便捷脚本

为避免开发者记忆 toolchain、版本、包名等细节，提供封装脚本：

```bash
script/coverage/run_coverage.sh
```

用法：

```bash
# 默认：第一阶段核心 crate，生成 html/json/lcov，启用 branch coverage
script/coverage/run_coverage.sh

# 关闭 branch coverage
script/coverage/run_coverage.sh --no-branch

# 全 workspace，仅 HTML（默认 line coverage，避免 LLVM bug）
script/coverage/run_coverage.sh -p workspace -f html

# 全 workspace，强制 branch coverage（可能崩溃）
script/coverage/run_coverage.sh -p workspace --branch

# 核心 crate，仅 JSON 摘要
script/coverage/run_coverage.sh -p core -f json

# 自定义输出目录
script/coverage/run_coverage.sh -o /tmp/mudu-cov
```

脚本职责：
- 自动读取 `.rust-nightly-version` 中的固定 nightly 版本。
- 自动安装 pinned nightly 工具链（如未安装）。
- 自动安装 `llvm-tools-preview`（如未安装）。
- 自动安装固定版本 `cargo-llvm-cov 0.8.7`（如未安装）。
- 统一设置 `CARGO_INCREMENTAL=0` 和 `--test-threads=1`。
- 默认启用 branch coverage（`--branch`），可通过 `--no-branch` 关闭。
- 输出报告路径摘要。

该脚本与 CI 使用相同的参数和工具链版本，保证本地与 CI 行为一致。

## 5. CI / Nightly 工作流设计（已落地）

`coverage` job 已加入 `.github/workflows/nightly-checks.yaml`，与 udeps/fuzz/Miri/ASan 共享 nightly 环境。

### 5.1 新增 job（已落地）

完整定义见 `.github/workflows/nightly-checks.yaml` 中的 `coverage:` job。关键行为：

- 使用 `rust-nightly-setup` action 设置固定 nightly 工具链。
- 安装 `llvm-tools-preview` 和 `cargo-llvm-cov 0.8.7`。
- 对第一阶段核心 crate 运行覆盖率测试：
  - `mudu`
  - `mudu_type`
  - `mudu_contract`
  - `mudu_kernel`
- 使用 `--no-report` 先收集覆盖率数据，默认启用 `--branch` branch coverage，再分别生成 HTML、JSON、`lcov` 三种报告（避免 `--output-path` 重复使用）。
- 通过 `actions/upload-artifact@v4` 上传报告，保留 30 天。
- 通过 `.github/scripts/summarize_coverage.py` 在 GitHub Actions job summary 中展示覆盖率表格。
- 失败时输出本地复现命令。

### 5.2 为什么放在 `nightly-checks.yaml`
- 复用 `rust-nightly-setup` action，减少环境差异。
- 与 udeps/fuzz/Miri/ASan 保持一致的“夜间质量看板”心智模型。
- 避免 stable build job 时间进一步拉长。

### 5.3 覆盖率摘要脚本（已创建）
`.github/scripts/summarize_coverage.py` 已创建，作用：

- 解析 `cargo llvm-cov --json` 输出的 `coverage.json`。
- 在 GitHub Actions job summary 中输出 Markdown 表格，展示 `lines` / `functions` / `regions` 覆盖率。
- 字段缺失时优雅降级为 `N/A`。

## 6. 分阶段 Crate 覆盖策略

### 6.1 第一阶段：核心纯逻辑 crate
目标 crate：`mudu`、`mudu_type`、`mudu_contract`、`mudu_kernel`。
理由：
- 它们依赖少、测试稳定、运行快。
- `mudu_kernel` 已有 fuzz 目标，可对比 fuzz coverage 与单元测试 coverage 的互补性。

### 6.2 第二阶段：runtime / parser / transpiler
目标 crate：`mudu_runtime`、`mudu_sys_impl`、`sql_parser`、`mudu_transpiler`。
理由：
- 这些 crate 包含大量业务逻辑，但部分测试依赖 I/O 或外部服务，运行时间更长。
- 待第一阶段稳定后再纳入，避免一次性报告过大。

### 6.3 暂不纳入
- `example/*`：示例代码不是核心质量指标。
- `mudu_wasm`、`bindings/*`：需要 wasm 工具链，增加 CI 复杂度。
- `mudu_kernel/fuzz` 的 fuzz target：已在 fuzz job 中单独运行。
- 长时间 integration tests：若单次测试超过 30 分钟，建议拆分到独立 job。

## 7. 报告存储与趋势追踪

### 7.1 Artifact 命名与保留
- 名称：`coverage-${{ github.run_id }}-${{ github.sha }}`
- 保留：30 天（与大多数开源项目一致）。
- 内容：HTML（人工查看）、JSON（机器解析）、lcov（本地工具链）。

### 7.2 趋势追踪建议
-  nightly job 将 `coverage.json` 上传后，可用一个简单脚本解析 `totals.lines.percent` 并写入仓库的 `doc/coverage-trend.md` 或 issue 评论（第二阶段）。
- 初期不强制 delta 阈值；先积累 2-4 周数据，再决定是否设定“下降超过 X% 则告警”规则。

### 7.3 失败处理
- 覆盖率 job 失败不应阻塞 PR（因为 nightly 不是 PR 必需检查）。
- 保留 `Local reproduction command` 步骤，方便开发者复现。

## 8. 与现有 CI 的兼容点

- `CARGO_INCREMENTAL=0`：与 `build.yaml`、`nightly-checks.yaml` 一致，避免增量编译干扰覆盖率。
- `--test-threads=1`：与 `build.yaml` 中测试命令一致，避免并发导致的非确定性。
- `MUDU_TEST_BACKEND_READY_TIMEOUT_SECS=120`：与 `build.yaml` 一致，给后端服务足够启动时间。
- 工具链：复用 `.rust-nightly-version`，不引入新的 Rust 版本。

## 9. 风险与缓解

| 风险 | 缓解 |
|---|---|
| `cargo-llvm-cov` 与某些 crate 的 build script 冲突 | 先在本地对第一阶段 crate 跑通，必要时用 `--ignore-filename-regex` 过滤外部依赖。 |
| 全 workspace 测试时间过长 | 第一阶段仅覆盖 4 个核心 crate；后续逐步扩展。 |
| 覆盖率数据波动大 |  nightly 跑法固定命令、固定线程数、固定工具链版本，减少波动。 |
| 报告体积过大 | 仅保留核心 crate 的 HTML；JSON/lcov 压缩上传。 |

### 9.5 本地运行常见问题

#### Q1: `rustup toolchain install` 报错 `could not rename downloaded file`

这是 rustup 下载组件时的偶发网络/文件系统错误，通常由以下原因导致：

- 网络抖动导致下载不完整。
- 多个 rustup 进程同时操作 `~/.rustup/downloads/`。
- 磁盘空间不足。

**解决方法：**

1. 直接重新运行脚本，`script/coverage/run_coverage.sh` 已对 `rustup` 命令加入 3 次重试。
2. 如果仍然失败，清理部分下载文件后重试：

```bash
rm -rf ~/.rustup/downloads/*.partial
rustup toolchain uninstall nightly-2026-06-17 2>/dev/null || true
script/coverage/run_coverage.sh
```

3. 检查磁盘空间：

```bash
df -h ~
```

#### Q2: `cargo-llvm-cov` 编译时间过长

首次安装 `cargo-llvm-cov` 需要从 crates.io 下载并编译依赖，耗时约 2-5 分钟（取决于网络与机器性能）。后续运行会直接使用已安装的二进制，无需重新编译。

#### Q3: 测试运行时间过长或卡住

- 第一阶段 4 个核心 crate 通常可在数分钟内完成；全 workspace 可能需要 30 分钟以上。
- 如果测试卡住，检查是否有端口/文件资源冲突，或尝试减少测试线程数（脚本已固定 `--test-threads=1`）。

#### Q4: 报告中 branch coverage 显示为 `0/0`

`cargo llvm-cov` 的 `--branch` 选项需要在**测试收集阶段**和**报告生成阶段**都指定。如果只在测试阶段加 `--branch`，而 `report` 阶段未加，则报告中的 branch 列会显示 `0/0`。

本脚本和 CI 已在两个阶段都加入 `--branch`。手动执行时请注意：

```bash
# 收集阶段
cargo +nightly-2026-06-17 llvm-cov ... --no-report --branch -- --test-threads=1

# 报告阶段同样要带 --branch
cargo +nightly-2026-06-17 llvm-cov report --branch --html --output-dir target/llvm-cov/html
```

如果不需要 branch coverage，两个阶段都去掉 `--branch` 即可。

## 10. 待决策事项

1. ~~是否接受将 `coverage` job 加入 `.github/workflows/nightly-checks.yaml`，还是单独建 `.github/workflows/coverage.yaml`？~~ 已决定：加入 `nightly-checks.yaml`。
2. ~~是否安装固定版本 `cargo-llvm-cov 0.8.7`，还是始终安装最新版？~~ 已决定：锁定 0.8.7。
3. 是否在报告稳定后引入“覆盖率下降 >5% 则 nightly job 失败”的规则？
4. 是否希望将覆盖率趋势图接入 GitHub Pages 或仓库内 Markdown？

## 11. 参考命令速查

### 11.1 手动命令

```bash
# 1. 安装
rustup component add llvm-tools-preview --toolchain nightly-2026-06-17
cargo +nightly-2026-06-17 install cargo-llvm-cov --version 0.8.7 --locked

# 2. 先跑测试收集覆盖率数据（默认启用 branch coverage）
CARGO_INCREMENTAL=0 cargo +nightly-2026-06-17 llvm-cov \
  --package mudu --package mudu_type --package mudu_contract --package mudu_kernel \
  --lib --tests --no-report --branch -- --test-threads=1

# 3. 生成 HTML 报告（report 阶段仍需显式指定 --branch，否则 branch 列输出 0/0）
cargo +nightly-2026-06-17 llvm-cov report \
  --branch --html --output-dir target/llvm-cov/html

# 4. 生成 JSON 摘要
cargo +nightly-2026-06-17 llvm-cov report \
  --branch --json --output-path target/llvm-cov/coverage.json

# 5. 生成 lcov
cargo +nightly-2026-06-17 llvm-cov report \
  --branch --lcov --output-path target/llvm-cov/coverage.lcov
```

> 如需关闭 branch coverage，去掉上述命令中的 `--branch` 即可。`run_coverage.sh` 也支持 `--no-branch` 选项。

### 11.2 便捷脚本

```bash
# 核心 crate，全部格式
script/coverage/run_coverage.sh

# 全 workspace，仅 HTML
script/coverage/run_coverage.sh -p workspace -f html

# 核心 crate，仅 JSON
script/coverage/run_coverage.sh -p core -f json
```

---

**实施状态**：
- [x] 本地脚本 `script/coverage/run_coverage.sh` 已创建，版本锁定 `cargo-llvm-cov 0.8.7`，并自动安装 pinned nightly 工具链。
- [x] 覆盖率摘要脚本 `.github/scripts/summarize_coverage.py` 已创建。
- [x] `.github/workflows/nightly-checks.yaml` 中新增 `coverage` job。
- [x] 设计文档已补充环境配置说明。

**下一步行动**：
1. 确保本地已安装 `rustup`，然后直接运行 `script/coverage/run_coverage.sh`，脚本会自动补齐 nightly 工具链、`llvm-tools-preview` 和 `cargo-llvm-cov`。
2. 验证第一阶段 4 个 crate 的覆盖率测试能正常结束并生成报告。
3. 观察 1-2 周的 nightly 报告稳定性，再决定是否扩展第二阶段 crate 或引入覆盖率下降告警。
