# MuduDB

![build](../../actions/workflows/build.yaml/badge.svg)

[<img src="doc/pic/mudu_logo.svg" width="10%">](doc/cn/name.cn.md)

[English](README.md)

---

MuduDB 是一个内置应用运行时的开源关系型数据库。

使用 C, C#, Go, JavaScript, Python 或 Rust 构建以数据为中心的实时应用，打包成 MuduDB 应用，直接部署到数据库中。

你的应用逻辑紧贴数据运行，减少数据搬运、简化系统架构，并实现低延迟执行。

应用服务器 + 数据库，合二为一。

让应用运行在数据所在的地方。

## 从这里开始

| 你想做什么 | 阅读入口 |
|---|---|
| 安装工具并运行第一个示例 | [环境配置](doc/cn/how_to_start.cn.md) → [第一个 MPK](doc/cn/your_first_mpk.cn.md) |
| 编写或调用过程 | [核心概念](doc/cn/concepts.cn.md) → [过程开发](doc/cn/procedure.cn.md) |
| 选择语言 SDK 或发布 SDK | [语言与运行方式](doc/dev/sdk_distribution.md#language-and-execution-modes) · [发布指南](doc/dev/sdk_publish_github_actions.zh-CN.md) |
| 修改数据库实现 | [代码架构](ARCHITECTURE.md) · [开发约定](AGENTS.md) |

## 执行方式

过程编译为 WebAssembly 组件，再与 DDL（数据定义语言，即过程所操作的表结构）、初始数据和描述符打包成 `.mpk`（MuduDB 软件包）。将软件包安装到 `mudud` 后，客户端即可调用其中的过程。

原生应用也可以显式选择独立分发的 adapter runtime，以本地应用的方式运行；该模式下各语言的入口见[语言与运行方式](doc/dev/sdk_distribution.md#language-and-execution-modes)。

Rust 过程通过 `Entity` trait 将查询结果映射为结构体。构建步骤中的 `mgen check-sql` 对照 DDL 检查 SQL；语言编译器负责类型检查，两者都不能代替运行时测试。

## 架构

<img src="doc/pic/cn/architecture.cn.svg" width="100%">

内核负责存储、事务、查询执行和调度，运行时负责加载和执行 WebAssembly 组件。过程通过系统调用访问内核，由内核统一管理事务与调度。

运行时采用按核 worker 模型，Linux I/O 路径使用 `io_uring`。设计取舍见[运行时架构](doc/cn/modern_hardware.cn.md)，源码组织见下表。

## 项目结构

仓库按 `crates/` 下的多个独立 Cargo workspace 组织，workspace 之间通过 path 依赖连接。所有 workspace 共享仓库根目录下的同一个 `target/` 目录（在 `.cargo/config.toml` 中配置）。

| 目录 | 内容 |
|------|------|
| `crates/common` | 基础与共享库：`mudu`（核心类型/错误码）、`mudu_sys*`（操作系统/异步抽象）、`mudu_type`、`mudu_binding`、`mudu_contract`（线协议）、`sql_parser`、`sys_interface`、`mudu_client`（TCP/JSON 客户端）以及各类工具库。 |
| `crates/db-kernel` | 数据库本体：`mudu_kernel`（引擎）、`mudu_runtime`（WASM 宿主、HTTP/pgwire 服务）、`mudud`（服务器守护进程）、`mudu_adapter`（存储适配器）、`sys_interface_standalone`、`testing`（集成测试）。 |
| `crates/sdk` | 应用开发套件：`mududb`（面向应用的 facade）、语言绑定（`bindings/`、`mudu_api/`）。 |
| `example/mpk` | 客端 SDK 示例应用（`wallet`、`tpcc`、`ycsb` 等），独立 workspace。 |
| `crates/tools` | 开发者工具链：`mcli`（命令行客户端）、`mgen`（代码生成器）、`mtp`（转译器）、`mpm-build` / `mpm-crate` / `mpm-install`（MPK 打包）。 |

依赖方向严格单向：`common` ← `db-kernel` ← `sdk`，`tools` 仅依赖 `common`。

构建或检查某个 workspace 时，进入对应目录执行：

```bash
cd crates/db-kernel && cargo build --release
```

`script/shell/check_all.sh` 会依次对所有 workspace 执行 `cargo fmt --check`、`cargo clippy --workspace --all-targets -- -D warnings` 和 `cargo test --no-run --workspace`。

## 文档导航

- [文档索引](doc/README.md)
- [如何开始](doc/cn/how_to_start.cn.md)
- [核心概念](doc/cn/concepts.cn.md)
- [第一个 MPK 教程](doc/cn/your_first_mpk.cn.md)
- [部署指南](doc/cn/DEPLOY.md)

## 许可证

MuduDB 采用 [Apache License 2.0](LICENSE) 许可证。
