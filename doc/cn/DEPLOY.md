# MuduDB 一键部署指南

在纯净 Ubuntu 24.04 环境中一键完成依赖安装、编译、启动、测试。

## 快速开始

```bash
# 1. 安装系统依赖 + Rust 工具链
bash script/shell/install_deps.sh

# 2. 编译全部组件并打包示例应用
bash script/shell/build_all.sh

# 3. 启动服务器并运行 CRUD 测试
bash script/shell/run_test.sh
```

三步全部通过即部署成功。

## 环境要求

- Ubuntu 24.04 LTS (x86_64)
- Linux 内核 5.1+，需启用 `CONFIG_IO_URING=y`
- 8GB+ 内存, 20GB+ 磁盘
- 外网访问 (下载 Rust 工具链和依赖)

### 检查 io_uring 支持

```bash
# 内核版本
uname -r  # 必须 ≥ 5.1

# io_uring 是否可用
grep io_uring /proc/filesystems && echo "io_uring OK" || echo "NOT supported"

# memlock 限制 (io_uring 需要锁定内存)
ulimit -l  # 建议 ≥ 65536 或 unlimited
```

### Docker 环境

如果在 Docker 中运行，需要 `--privileged` 以访问宿主机内核的 io_uring：

```bash
docker run --privileged -v /path/to/repo:/mududb:ro ubuntu:24.04 ...
```

或使用更精细的权限：
```bash
docker run --cap-add CAP_SYS_ADMIN --ulimit memlock=-1:-1 ...
```

## 脚本说明

### `script/shell/install_deps.sh`

安装运行 MuduDB 所需的全部依赖：

| 类别 | 内容 |
|------|------|
| 系统包 | python3, pip, python-is-python3, build-essential, curl, liburing-dev, clang, libclang-dev, llvm-dev, pkgconf, iproute2 |
| Rust | pinned stable 工具链（含 rustfmt、clippy、x86_64-unknown-linux-gnu 和 wasm32-wasip2 target）+ 辅助 nightly 工具链 |
| Python | toml, tomli-w |
| 工具 | cargo-make |

> 实现参考：`script/shell/install_deps.sh`、`rust-toolchain.toml`。

### `script/shell/build_all.sh`

编译并安装全部组件：

1. `cargo build --release` — 编译工作区 crate
2. `python3 script/build/install_binaries.py` — 安装二进制文件到 `~/.cargo/bin/`（mudud、mcli、mpm-build、mgen、mtp）
3. 在 `example/wallet` 中执行 `cargo make` — 重新生成实体代码、转译过程、编译 wallet 示例并生成 `.mpk` 包

wallet 的 `Makefile.toml` 会在生成代码前从源码重新安装工作区 CLI 工具，因此包总是用当前 commit 的工具链构建。

### `script/shell/run_test.sh`

启动 MuduDB 服务器并执行 CRUD 测试：

- 创建临时数据目录并写入默认配置文件
- 启动 mudud 服务器 (HTTP 8300 / TCP 9527)
- 安装 wallet 示例应用
- 测试 CREATE / READ / UPDATE / INVOKE / DELETE
- 测试结束后自动清理

### `script/shell/debug_test.sh`

带诊断信息的集成测试，输出更详细的日志和中间状态。

## 一键运行全部

```bash
#!/bin/bash
set -euo pipefail
bash script/shell/install_deps.sh
source "$HOME/.cargo/env"
bash script/shell/build_all.sh
bash script/shell/run_test.sh
bash script/shell/debug_test.sh
echo "All tests passed."
```
