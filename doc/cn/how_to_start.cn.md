# 如何开始

## 使用 `mudup` 快速安装

该路径用于服务器部署和日常使用，不需要从源码构建 MuduDB。

### 1. 安装 `mudup`

先从发布产物安装 `mudup`，然后执行：

```bash
curl --proto '=https' --tlsv1.2 -fsSL  https://github.com/scuptio/mudup/releases/download/latest/mudup-init.sh | sh
mudup --help
```

### 2. 安装 MuduDB及其工具链

```bash
mudup install
```

该命令会安装并激活最新版本（`mudud`、`mcli`、`mpk`、`mgen`、`mtp`）。

### 3. 验证安装

```bash
mudud --version
mcli --version
```

如果 `${HOME}/.mududb/mududb_cfg.toml` 不存在，`mudup install` 会自动创建默认配置文件。

## 源码安装（用于开发部署）

## 克隆仓库

```bash
git clone https://github.com/scuptio/mududb.git
```

## 前置环境配置（Ubuntu 或 Debian）

### 系统软件包

请先安装原生构建依赖：

```bash
sudo apt-get update -y
sudo apt-get install -y \
    python3 \
    python3-pip \
    python-is-python3 \
    build-essential \
    curl liburing-dev \
    clang \
    libclang-dev \
    llvm-dev \
    pkgconf
```

这些软件包的用途如下：

- `python3` 和 `python3-pip`：示例构建脚本运行时需要
- `build-essential`：Linux 上原生编译所需
- `curl`：用于通过 `rustup` 安装 Rust
- `liburing-dev`：仅 Linux 上由 `mudu_kernel` 使用原生 `io_uring` 后端时需要
- `clang`、`libclang-dev`、`llvm-dev`：*[bindgen](https://github.com/rust-lang/rust-bindgen)* 需要

### Rust 工具链

请使用 nightly Rust 工具链：

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup toolchain install nightly
rustup default nightly
rustup component add rustfmt --toolchain nightly
rustup update nightly
rustup target add wasm32-wasip2
```

### Python 包

示例构建脚本在运行时需要以下 Python 包：

```bash
python -m pip install toml tomli-w
```

### cargo make

示例应用通过 `cargo-make` 任务文件驱动，因此建议安装：

```bash
cargo install cargo-make
```

## 安装工具与 MuduDB Server（开发构建）

```bash
python script/build/install_binaries.py
```

默认会安装受支持的发布工具：

- `mpk`：打包构建工具
- `mgen`：源码生成工具
- `mtp`：转译器
- `mudud`：MuduDB 服务器
- `mcli`：TCP 协议客户端 CLI

如果你需要安装 workspace 中的全部二进制目标，可以使用：

```bash
python script/build/install_binaries.py --all-workspace-bins
```


## 创建配置文件

[mududb_cfg.toml 示例](../cfg/mududb_cfg.toml)

在以下位置创建配置文件：

```bash
mkdir -p ${HOME}/.mududb
touch ${HOME}/.mududb/mududb_cfg.toml
```

如果该文件不存在，`mudud` 首次启动时也会按默认值自动创建 `${HOME}/.mududb/mududb_cfg.toml`。

## 使用 MuduDB

可选阅读：[`mcli` 管理接口（HTTP）](./mcli_admin.cn.md)。

### 1. 启动 `mudud`

启动 `mudud` 前，请先确认服务进程拥有足够高的打开文件数限制。若软限制 `nofile` 仍是 `1024` 这类较低值，在较高连接数下可能出现 session 建立失败或整体卡住的问题，即使你当前交互 shell 的限制更高也是如此。

如果是在当前 shell 中直接启动本地 `mudud`，可以先提升限制再启动：

```bash
ulimit -n 65535
mudud
```

如果 `mudud` 由 `systemd` 或其他 supervisor 启动，还需要在对应服务配置中提升文件描述符限制，例如设置 `LimitNOFILE=65535`。

启动后可以用下面的命令确认实际生效的限制：

```bash
cat /proc/$(pgrep -x mudud)/limits | rg 'open files'
```

`mudud` 启动后，可以先用交互式 SQL 跑一遍 CRUD，再安装并调用 `wallet` 示例应用。

### 2. 使用 mcli 交互式执行 CRUD

先启动交互式 shell：

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

#### 使用 mcli 安装 wallet 包

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
