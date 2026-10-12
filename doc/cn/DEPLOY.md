# Ubuntu 本地构建与验证

本页介绍仓库的构建和测试脚本。`run_test.sh` 会启动临时服务器，测试结束后清理；这些步骤不会安装常驻服务，也不涵盖生产环境的访问控制、备份和升级。

首次使用建议先完成[环境配置](how_to_start.cn.md)，再按[第一个 MPK](your_first_mpk.cn.md)手动启动和调用示例。下列脚本均从仓库根目录运行，执行前检查对应脚本的路径和前置条件。

## 环境要求

- Ubuntu 24.04 LTS (x86_64)
- Linux 内核及安全策略需支持所选 I/O 模式
- 8GB+ 内存, 20GB+ 磁盘
- 外网访问 (下载 Rust 工具链和依赖)

### 检查运行环境

在仓库根目录运行诊断脚本：

```bash
bash script/build/inspect_iouring_env.sh
```

脚本列出内核开关、seccomp 状态和容器环境，供排查使用；它不是完整的功能测试。`io_uring` 不是文件系统，不能通过 `/proc/filesystems` 判断是否可用。

容器也受宿主内核和安全策略限制。具体的本地 CI 参数见 [act 使用说明](../../.github/run-actions-locally.md)。常驻服务配置见[启动 mudud](running_mudud.cn.md)。

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

1. 在 `crates/` 下的四个组 workspace（`common`、`db-kernel`、`sdk`、`tools`）中分别执行 `cargo build --release` — 编译全部 crate
2. `python3 script/build/install_binaries.py` — 安装二进制文件到 `~/.cargo/bin/`（mpm-build、mpm-crate、mpm-install、mgen、mtp、mudud、mcli）
3. 在 `example/mpk/wallet` 中执行 `cargo make` — 重新生成实体代码、转译过程、编译 wallet 示例并生成 `.mpk` 包

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

## 串行运行脚本

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
