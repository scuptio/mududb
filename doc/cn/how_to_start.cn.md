# 如何开始

## 克隆仓库

```bash
git clone https://github.com/scuptio/mududb.git
```

## 前置环境配置（Ubuntu 或 Debian）

### 系统软件包

请先安装原生构建依赖：

```bash
sudo apt-get update -y
sudo apt-get install -y python3 python3-pip build-essential curl liburing-dev
```

这些软件包的用途如下：

- `python3` 和 `python3-pip`：示例构建脚本运行时需要
- `build-essential`：Linux 上原生编译所需
- `curl`：用于通过 `rustup` 安装 Rust
- `liburing-dev`：仅 Linux 上由 `mudu_kernel` 使用原生 `io_uring` 后端时需要

如果你是在 Windows 上构建，则不需要 `liburing-dev`，因为原生 `io_uring` 路径仅适用于 Linux。

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

## 安装工具与 MuduDB Server

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
touch ${HOME}/.mudu/mududb_cfg.toml
```

## 使用 MuduDB

### 1. 启动 `mudud`

```bash
mudud
```

`mudud` 启动后，可以先验证内置 key/value 访问，再构建并安装一个 `.mpk` 示例包。

### 2. 使用 mcli 读写 key/value

每条 `mcli` 命令都会自动创建并关闭一个临时 session，因此不需要显式传入 `session_id`。

```bash
mcli put --json '{
  "key": "user-1",
  "value": "value-1"
}'

mcli get --json '{
  "key": "user-1"
}'
```

`get` 应返回：

```json
"value-1"
```

### 3. 构建、安装和使用 MuduDB 应用

#### 构建 key-value `.mpk` 包

```bash
cd example/key-value
cargo make
```

生成的包路径为：

```bash
target/wasm32-wasip2/release/key-value.mpk
```

#### 使用 mcli 安装 `.mpk` 包

```bash
mcli app-install --mpk target/wasm32-wasip2/release/key-value.mpk
```

#### 使用 mcli 调用已安装 `.mpk` 中的过程

通过 `kv_insert` 过程写入一条记录：

```bash
mcli app-invoke --app kv --module key_value --proc kv_insert --json '{
  "user_key": "user-1",
  "value": "value-from-mpk"
}'
```

再通过 `kv_read` 过程读取：

```bash
mcli app-invoke --app kv --module key_value --proc kv_read --json '{
  "user_key": "user-1"
}'
```
