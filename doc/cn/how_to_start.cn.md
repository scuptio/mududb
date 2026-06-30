# 如何开始

本指南提供两种上手方式：

- **[使用 `mudup` 安装 MuduDB](#使用-mudup-安装-mududb)** – 通过 `mudup` 安装发布产物，无需源码构建。
- **[从源码开发 MuduDB](#从源码开发-mududb)** – 使用固定工具链从源码构建。

---

## 使用 `mudup` 安装 MuduDB

该路径用于服务器部署和日常使用。

### 1. 安装 `mudup`

```bash
curl --proto '=https' --tlsv1.2 -fsSL https://github.com/scuptio/mudup/releases/download/latest/mudup-init.sh | sh
mudup --help
```

### 2. 安装 MuduDB 及其工具链

```bash
mudup install
```

该命令会安装并激活最新版本：`mudud`、`mcli`、`mpk`、`mgen`、`mtp`。

### 3. 验证安装

```bash
mudud --version
mcli --version
```

如果 `${HOME}/.mududb/mududb_cfg.toml` 不存在，`mudup install` 会自动创建默认配置文件。

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

然后验证构建：

```bash
cargo build
cargo test
cargo clippy --workspace --all-targets -- -D warnings
cargo doc --workspace --no-deps
```

### 方式 B：Dev Container / Docker

如果你偏好容器化环境，可直接构建并运行开发镜像：

```bash
docker build -f Dockerfile.dev -t mududb-dev .
docker run -it --rm -v "$(pwd):/workspace" mududb-dev
```

或者使用支持 [Dev Containers](https://containers.dev/) 的编辑器打开项目，配置已提供在 `.devcontainer/devcontainer.json`。

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
python -m pip install toml tomli-w
```

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

- `mpk`：打包构建工具
- `mgen`：源码生成工具
- `mtp`：转译器
- `mudud`：MuduDB 服务器
- `mcli`：TCP 协议客户端 CLI

如需安装 workspace 中的全部二进制目标：

```bash
python script/build/install_binaries.py --all-workspace-bins
```

---

## 工具链策略

- **stable Rust**（由 `rust-toolchain.toml` 固定）是正式工具链，用于 `cargo build`、`cargo check`、`cargo test`、`cargo clippy`、`cargo doc` 以及发布产物构建。
- **固定日期的 nightly**（由 `.rust-nightly-version` 固定）仅用于确实依赖 nightly 的辅助检查：`cargo fuzz`、`cargo-udeps`、Miri 和 sanitizer。
- **禁止**使用 `RUSTC_BOOTSTRAP` 让 nightly feature 进入 stable 构建。
- 产品代码原则上不得引入新的 nightly feature；确需引入时必须经过架构评审，说明替代方案、影响范围和退出计划。

## 本地运行辅助检查

```bash
NIGHTLY_TOOLCHAIN="$(tr -d '[:space:]' < .rust-nightly-version)"

# 代码覆盖率（详见 doc/cn/test_coverage.md）
script/coverage/run_coverage.sh

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

默认情况下，`mudud` 读取 `${HOME}/.mududb/mududb_cfg.toml`。先创建其父目录：

```bash
mkdir -p ${HOME}/.mududb
```

不要创建空的 `mududb_cfg.toml`：服务端只要发现文件存在，就会把它当作用户配置解析。如果该文件不存在，`mudud` 首次启动时会按默认值自动创建它。

使用其他位置的配置文件：

```bash
mudud --cfg /path/to/mududb_cfg.toml
```

使用示例配置：

```bash
cp doc/cfg/mududb_cfg.toml ${HOME}/.mududb/mududb_cfg.toml
```

另见：[mududb_cfg.toml 示例](../cfg/mududb_cfg.toml)。

---

## 使用 MuduDB

可选阅读：[`mcli` 管理接口（HTTP）](./mcli_admin.cn.md)。

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
cat /proc/$(pgrep -x mudud)/limits | rg 'open files'
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

#### 构建 wallet `.mpk` 包

```bash
cd example/wallet
cargo make
```

生成的包路径为：

```bash
target/wasm32-wasip2/release/wallet.mpk
```

#### 使用 `mcli` 安装 wallet 包

```bash
mcli --http-addr 127.0.0.1:8300 app-install --mpk target/wasm32-wasip2/release/wallet.mpk
```

安装后，可通过 HTTP 管理命令确认状态：

```bash
mcli --http-addr 127.0.0.1:8300 app-list
mcli --http-addr 127.0.0.1:8300 app-detail --app wallet
mcli --http-addr 127.0.0.1:8300 app-detail --app wallet --module wallet --proc create_user
mcli --http-addr 127.0.0.1:8300 server-topology
```

#### 调用 wallet 过程

创建两个用户：

```bash
mcli --addr 127.0.0.1:9527 --http-addr 127.0.0.1:8300 app-invoke --app wallet --module wallet --proc create_user --json '{
  "user_id": 1001,
  "name": "Alice",
  "email": "alice@example.com"
}'

mcli --addr 127.0.0.1:9527 --http-addr 127.0.0.1:8300 app-invoke --app wallet --module wallet --proc create_user --json '{
  "user_id": 1002,
  "name": "Bob",
  "email": "bob@example.com"
}'
```

说明：`app-invoke` 通过 TCP 调用过程；当前命令仍需要 `--http-addr` 来获取过程描述信息。

充值并转账：

```bash
mcli --addr 127.0.0.1:9527 --http-addr 127.0.0.1:8300 app-invoke --app wallet --module wallet --proc deposit --json '{
  "user_id": 1001,
  "amount": 5000
}'

mcli --addr 127.0.0.1:9527 --http-addr 127.0.0.1:8300 app-invoke --app wallet --module wallet --proc transfer --json '{
  "from_user_id": 1001,
  "to_user_id": 1002,
  "amount": 1200
}'
```

在 shell 中验证钱包余额：

```bash
mcli --addr 127.0.0.1:9527 shell --app wallet
```

```sql
SELECT user_id, balance FROM wallets WHERE user_id IN (1001, 1002);
```

---

## 常见问题 / 故障排查

### `mudud` 启动时报 io_uring 错误

MuduDB 需要 Linux 内核 5.1+ 并启用 `CONFIG_IO_URING=y`。检查：

```bash
uname -r                          # 必须 >= 5.1
grep io_uring /proc/filesystems   # 应列出 io_uring
```

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

### 为什么 crate 叫 `mod_0` 而不是 `mudu_wasm`？

目录和可读名称是 `mudu_wasm`，但 Cargo 包名以及运行时所使用的组件模块名是 `mod_0`。在 Rust 中导入该库时请使用 `mod_0::generated` 和 `mod_0::wasm_mtp`。详见 [`mudu_wasm/README.md`](../../mudu_wasm/README.md)。

### 想了解更多？

- 核心概念：[`concepts.cn.md`](concepts.cn.md)
- 部署脚本：[`DEPLOY.md`](DEPLOY.md)
- 文档索引：[`../README.md`](../README.md)

---

## 贡献代码

提交修改前请确认：

1. 在固定 stable 工具链上 `cargo fmt`、`cargo clippy`、`cargo test` 均通过。
2. 保持改动聚焦，遵循现有代码风格。
3. 如果改动影响构建、配置或公开行为，请同步更新相关文档。

完整的开发环境配置说明见上文。
