# 如何开始

本页用于配置开发环境。本指南提供两种上手方式：

- **[从源码开发 MuduDB](#从源码开发-mududb)** – 使用固定工具链从源码构建（推荐）。源码路径提供三种配置选项：首次使用建议选择方式 A，然后按[第一个 MPK](your_first_mpk.zh-CN.md)构建、安装并调用钱包示例。方式 B、C 是替代安装方法，无需全部执行。
- **[使用 `mudup` 安装 MuduDB](#使用-mudup-安装-mududb)** – 通过 `mudup` 安装发布产物，无需源码构建（计划中，尚不可用）。

---

## 从源码开发 MuduDB

### 克隆仓库

```bash
git clone https://github.com/scuptio/mududb.git
cd mududb
```

### 方式 A：一条命令完成环境配置（推荐）

在 Ubuntu/Debian 上，直接运行仓库提供的配置脚本。它会安装系统软件包、rustup、固定版本的 stable 与 nightly Rust 工具链、本地 Python 虚拟环境以及 `cargo-make`。

```bash
./script/setup_dev_env.sh
```

脚本执行完成后，激活 Python 虚拟环境：

```bash
source .venv/bin/activate
```

然后验证工具链——它会列出 MuduDB 构建、示例与发布流程所需的全部工具，并为缺失项给出按操作系统分派的安装命令：

```bash
cargo make doctor
```

然后验证构建。仓库由多个独立 Cargo workspace 组成：`crates/` 下四个（`common`、`db-kernel`、`sdk`、`tools`）外加 `example/mpk`；需进入各 workspace 目录分别构建：

```bash
cd crates/db-kernel
cargo build
cargo test
cargo clippy --workspace --all-targets -- -D warnings
cargo doc --workspace --no-deps
```

也可以在仓库根目录运行覆盖全部五个 group workspace 的完整检查：

```bash
cargo make check
```

使用 `cargo make test` 运行所有 workspace 的测试。测试失败后仍会继续后续测试目标和
workspace；只要任一组失败，最终就返回非零退出码。每次运行在
`logs/tests/<时间戳>-<唯一标识>/` 保存各组完整日志、`summary.txt` 和 `summary.json`，
汇总失败测试及目标名称、错误摘录、退出码、通过/失败/忽略测试数量和失败 workspace 数量。
`failures.log` 合并所有失败 workspace 的完整输出，不截断；编译错误、进程异常等没有
测试数量的失败会单独列出。编译失败可能导致当前 workspace
的测试无法运行，但后续 workspace 仍会继续。Ctrl+C 会停止执行并保留已有结果。

快速排错可用 `cargo make test -- --fail-fast`；只运行一组可用
`cargo make test -- --group db-kernel`。`test-group-*` 任务也使用相同日志机制。
`--log-dir <路径>` 可指定日志父目录；每次运行创建独立目录，不覆盖旧日志。
终端输出突出阶段进度和错误，最后以表格汇总各组结果、测试数量和耗时，并提供可点击的
报告链接。可用 `--color always` 强制彩色，或 `--color never` 关闭；自动模式遵循
`NO_COLOR`。日志文件及保存的报告始终使用纯文本。

### 方式 B：Dev Container / Docker

如果你偏好容器化环境，可直接构建并运行开发镜像：

```bash
docker build -f Dockerfile.dev -t mududb-dev .
docker run -it --rm -v "$(pwd):/workspace" -v mududb-dev-target:/workspace/target mududb-dev
```

或者使用支持 [Dev Containers](https://containers.dev/) 的编辑器打开项目，配置已提供在 `.devcontainer/devcontainer.json`。

容器构建不得共享宿主机的 `target/`。开发镜像在 `/workspace/target` 使用
Docker 卷；上述命令、Dev Containers 和分布式 E2E driver 分别使用独立的命名缓存。
修改此配置后，需要重新构建已有开发镜像并重建容器。
E2E 镜像构建使用独立的 `/opt/mududb-e2e-target`；本地 `act` 使用复制的工作区
（`--bind=false`）。

E2E 镜像构建需要 BuildKit，编译缓存和 Cargo 下载缓存分别保存，按架构隔离，
构建期间加锁。服务端二进制先导出到缓存之外，再复制到运行镜像。
driver 使用外部卷 `e2e_driver-target`、`e2e_driver-registry` 和 `e2e_driver-git`。
测试脚本自动创建这些卷；`down --volumes` 清理场景数据，但保留这些缓存。
直接运行 Compose 前，需要先用 `docker volume create <卷名>` 创建这三个卷。
不同检出目录需要独立缓存时，可通过 `MUDUDB_E2E_TARGET_VOLUME`、
`MUDUDB_E2E_REGISTRY_VOLUME` 和 `MUDUDB_E2E_GIT_VOLUME` 覆盖卷名。
已有 driver 在下次重建时获得下载缓存挂载。缓存仅手动清理：只删除不再使用的
命名缓存卷；清理 BuildKit 缓存也会丢弃镜像构建的增量编译产物。

### 方式 C：手动配置

如果无法使用脚本或容器，请按以下步骤操作。

#### 1. 安装系统软件包

```bash
sudo apt-get update -y
sudo apt-get install -y \
    python3 python3-pip python3-venv python-is-python3 \
    build-essential curl liburing-dev \
    clang libclang-dev llvm-dev pkgconf
```

各软件包用途：

- `python3`、`python3-pip`、`python3-venv`：为示例构建脚本创建隔离的 Python 环境。
- `build-essential`、`curl`：原生编译以及通过 rustup 安装 Rust。
- `liburing-dev`：Linux 上 `mudu_kernel` 使用的原生 `io_uring` 后端。
- `clang`、`libclang-dev`、`llvm-dev`：[bindgen](https://github.com/rust-lang/rust-bindgen) 需要。

#### 2. 安装 Rust 工具链

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

仓库内的 `rust-toolchain.toml` 已固定 stable 工具链及组件（`clippy`、`rustfmt`、目标平台）。直接执行 `cargo build`、`cargo test`、`cargo clippy`、`cargo doc` 时会自动使用该版本。

对于确实需要 nightly 的辅助检查，安装固定日期的 nightly：

```bash
NIGHTLY_TOOLCHAIN="$(tr -d '[:space:]' < .rust-nightly-version)"
rustup toolchain install "$NIGHTLY_TOOLCHAIN" --profile minimal
```

#### 3. 配置 Python 虚拟环境

```bash
python3 -m venv .venv
source .venv/bin/activate
python -m pip install --upgrade pip
```

转译脚本需要 Python 3.11+，仅使用标准库，无需安装额外的 pip 包。

在新的 shell 中，使用 `source .venv/bin/activate` 重新激活环境。

#### 4. 安装 `cargo-make`

```bash
cargo install cargo-make
```

---

## 安装开发构建的工具与 MuduDB Server

环境就绪后，安装工具与 MuduDB 服务器的开发构建：

```bash
python script/build/install_binaries.py
```

默认安装的工具有：

- `mpm`：portable 项目构建器
- `mpm-build`：打包构建工具
- `mpm-crate`：项目脚手架工具
- `mpm-install`：包安装工具
- `mpm-tool`：包检查/校验/转换工具
- `mgen`：源码生成工具
- `mtp`：转译器
- `mudud`：MuduDB 服务器
- `mcli`：TCP 协议客户端 CLI

如需安装 workspace 中的全部二进制目标：

```bash
python script/build/install_binaries.py --all-workspace-bins
```

---

## 使用 `mudup` 安装 MuduDB

> **注意：** `mudup` 是计划中的安装器，设计仍在调整中，详见[设计提案](../en/mudup.md)（英文）。下列命令目前尚不可用；现阶段可用的安装路径是从源码构建：运行 `cargo make`，或按[安装开发构建的工具与 MuduDB Server](#安装开发构建的工具与-mududb-server)一节运行 `python script/build/install_binaries.py`。

该路径发布后将用于服务器部署和日常使用。

### 1. 安装 `mudup`

```bash
curl --proto '=https' --tlsv1.2 -fsSL https://github.com/scuptio/mudup/releases/download/latest/mudup-init.sh | sh
mudup --help
```

### 2. 安装 MuduDB 及其工具链

```bash
mudup install
```

该命令将安装并激活最新版本：`mpm`、`mpm-build`、`mpm-crate`、`mpm-install`、`mpm-tool`、`mgen`、`mtp`、`mudud`、`mcli`。

### 3. 验证安装

```bash
mudud --version
mcli --version
```

然后按[启动 mudud](running_mudud.zh-CN.md)创建配置文件并启动服务端。

---

## 工具链策略

- **stable Rust**（由 `rust-toolchain.toml` 固定）是正式工具链，用于 `cargo build`、`cargo check`、`cargo test`、`cargo clippy`、`cargo doc` 以及发布产物构建。
- **固定日期的 nightly**（由 `.rust-nightly-version` 固定）仅用于确实依赖 nightly 的辅助检查：`cargo fuzz`、`cargo-udeps`、Miri 和 sanitizer。
- **禁止**使用 `RUSTC_BOOTSTRAP` 让 nightly feature 进入 stable 构建。
- 产品代码原则上不得引入新的 nightly feature；确需引入时必须经过架构评审，说明替代方案、影响范围和退出计划。

## 本地运行辅助检查

```bash
NIGHTLY_TOOLCHAIN="$(tr -d '[:space:]' < .rust-nightly-version)"

# 代码覆盖率（详见 doc/zh-CN/test_coverage.md）
python3 script/coverage/run_coverage.py

# cargo-udeps
cargo +"${NIGHTLY_TOOLCHAIN}" install cargo-udeps --version 0.1.61 --locked
cargo +"${NIGHTLY_TOOLCHAIN}" udeps --workspace --all-targets

# cargo-fuzz
cd mudu_kernel/fuzz
cargo +"${NIGHTLY_TOOLCHAIN}" install cargo-fuzz --version 0.13.2 --locked
cargo +"${NIGHTLY_TOOLCHAIN}" fuzz run <target> -- -max_total_time=60

# Miri
rustup component add miri --toolchain "${NIGHTLY_TOOLCHAIN}"
cargo +"${NIGHTLY_TOOLCHAIN}" miri test --workspace

# AddressSanitizer
RUSTFLAGS="-Z sanitizer=address" \
  LSAN_OPTIONS="suppressions=$(pwd)/script/ci/lsan_suppressions.txt" \
  cargo +"${NIGHTLY_TOOLCHAIN}" test --workspace --lib --tests --bins --target x86_64-unknown-linux-gnu
```

## 工具链升级流程

stable 与 nightly 工具链升级必须分别提交独立的 Pull Request：

- **stable 升级**：修改 `rust-toolchain.toml`（若 MSRV 变化，同步修改 `Cargo.toml` 中的 `workspace.package.rust-version`）。合并前必须运行完整的兼容性测试和基准测试。
- **nightly 升级**：修改 `.rust-nightly-version`。合并前必须运行所有 nightly-only 检查（`cargo-udeps`、`cargo fuzz`、Miri、sanitizer）。

两种升级都只能显式提交修改；CI 和发布流程中不允许使用浮动工具链版本。

---

## 配置文件

在不启动服务端的情况下，在当前目录生成默认的 `./mudud.cfg`：

```bash
mudud init-cfg
```

`mudud` 会加载查找到的第一个配置文件，不会自动创建。完整的查找顺序、配置字段说明以及自定义 `--cfg` / `-c` 路径见[启动 mudud](running_mudud.zh-CN.md)。

---

## 使用 MuduDB

可选阅读：[`mcli` 管理接口（HTTP）](./mcli_admin.zh-CN.md)。

### 1. 启动 `mudud`

启动前请确认服务进程拥有足够高的打开文件数限制。若软限制 `nofile` 仍是 `1024` 这类较低值，在较高连接数下可能出现 session 建立失败或整体卡住的问题。

在当前 shell 中直接启动本地 `mudud`：

```bash
ulimit -n 65535
mudud
```

如果 `mudud` 由 `systemd` 或其他 supervisor 启动，还需要在对应服务配置中提升文件描述符限制，例如设置 `LimitNOFILE=65535`。

启动后可以用下面的命令确认实际生效的限制：

```bash
cat /proc/$(pgrep -x mudud)/limits | grep 'open files'
```

### 2. 使用 `mcli` 交互式执行 CRUD

```bash
mcli --addr 127.0.0.1:9527 shell --app demo
```

在 shell 中执行完整 CRUD 示例：

```sql
CREATE TABLE users_demo (
  id INT PRIMARY KEY,
  name TEXT
);

INSERT INTO users_demo (id, name) VALUES (1, 'Alice');
SELECT id, name FROM users_demo WHERE id = 1;

UPDATE users_demo SET name = 'Alice-Updated' WHERE id = 1;
SELECT id, name FROM users_demo WHERE id = 1;

DELETE FROM users_demo WHERE id = 1;
SELECT id, name FROM users_demo;
```

退出 shell：

```text
\q
```

Shell 说明：

- 每条 SQL 语句都要以 `;` 结尾。
- 元命令包括：`\q`、`\help`、`\app <name>`。
- 在 TTY 下，查询结果默认以交互式表格展示。

### 3. 构建、安装和使用 wallet 应用

完整步骤集中在[第一个 MPK](your_first_mpk.zh-CN.md)，包括包路径、配置初始化、安装命令和预期结果。请从仓库根目录开始，避免将示例目录的相对路径与根目录的 `target/` 混用。

## 常见问题 / 故障排查

### `mudud` 启动时报 io_uring 错误

MuduDB 需要 Linux 内核 5.1+ 并启用 `CONFIG_IO_URING=y`。检查：

```bash
uname -r                                # 必须 >= 5.1
bash script/build/inspect_iouring_env.sh
```

该脚本会列出内核开关、seccomp 状态和容器环境。`io_uring` 不是文件系统，不能通过 `/proc/filesystems` 判断是否可用；环境详情见 [DEPLOY.md](DEPLOY.md)。

如果在 Docker 中运行，需加 `--privileged`，或添加 `--cap-add CAP_SYS_ADMIN --ulimit memlock=-1:-1`。

### `mudud` 启动后 session 建立失败或整体卡住

启动前提升打开文件数限制：

```bash
ulimit -n 65535
mudud
```

若通过 `systemd` 管理，请在服务配置中设置 `LimitNOFILE=65535`。

### `cargo build` 失败：缺少 `wasm32-wasip2` target

`rust-toolchain.toml` 中固定的 stable 工具链已包含该 target，也可手动安装：

```bash
rustup target add wasm32-wasip2
```

### `cargo build` 失败：缺少 `liburing` 头文件

安装系统依赖：

```bash
sudo apt-get install liburing-dev
```

### `mcli` 无法连接服务器

- TCP 协议端口默认为 `9527`（`--addr 127.0.0.1:9527`）。
- HTTP 管理端口默认为 `8300`（`--http-addr 127.0.0.1:8300`）。
- 部分命令（如 `app-invoke`）需要同时指定 TCP 和 HTTP 地址。
- 确认 `mudud` 已启动，且防火墙未阻断上述端口。

### `app-invoke` 提示找不到 app 或 procedure

先安装 MPK 包：

```bash
mcli --http-addr 127.0.0.1:8300 app-install --mpk path/to/package.mpk
```

然后用以下命令确认名称：

```bash
mcli --http-addr 127.0.0.1:8300 app-list
mcli --http-addr 127.0.0.1:8300 app-detail --app <app>
```

### 想了解更多？

- 核心概念：[`concepts.zh-CN.md`](concepts.zh-CN.md)
- 部署脚本：[`DEPLOY.md`](DEPLOY.md)
- 文档索引：[`../README.md`](../README.md)

---

## 贡献代码

提交修改前请确认：

1. 在固定 stable 工具链上 `cargo fmt`、`cargo clippy`、`cargo test` 均通过。
2. 保持改动聚焦，遵循现有代码风格。
3. 如果改动影响构建、配置或公开行为，请同步更新相关文档。

完整的开发环境配置说明见上文。

### 耗时任务的汇总报告

`check`（含 `check-*`）、`udeps`、`dep-audit`、`outdated`、`bench-smoke`
（含单项基准）、`build-sdk`、`package-examples`、`test-e2e-distributed`
在失败后继续执行独立步骤，最后统一报告。执行仍为串行，保留内存限制与 target 隔离。
运行 `cargo make <任务> -- --help` 查看选项：`--fail-fast`、
`--log-dir <目录>`、`--color auto|always|never`。

每次运行在 `logs/<任务类别>/<独立运行目录>/` 写入 `summary.txt`、`summary.json`、
各步骤的纯文本完整日志，以及汇集失败和建议性错误完整输出的 `failures.log`。
报告分别统计成功、失败、跳过、建议性错误和中断，包含退出码、错误摘要、耗时和日志路径。
必要步骤失败时，完成其他独立工作后返回非零；用户中断返回 130。

- `outdated` 保持建议性检查：错误写入报告，但单独的 outdated 错误不使整个任务失败。
- 分布式 e2e 保存 Compose 节点日志，并在每个场景后清理；镜像准备失败会跳过场景，
  清理失败会阻止后续场景，避免使用残留环境。
- 基准运行失败时不执行其结果比较；smoke 不更新基线。显式的
  `bench-baseline-update*` 继续保留严格失败处理。
- 示例打包只安装一次共享工具，然后按语言独立执行。
- SDK 只检查一次绑定再生成，并只构建一次所需 adapter runtime；各语言独立检查前置工具、
  构建和验证。成功语言的产物保留，部分失败时不生成可发布的 `manifest.json`。
- pre-commit 显式使用 `--fail-fast`，保留原有的快速失败反馈。
