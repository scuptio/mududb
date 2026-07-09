# MPK 包清单契约 v1

## 范围

本文档规定 MuduDB 用于部署存储过程的 `.mpk` 应用包格式。`.mpk` 是一个 ZIP 归档，包含应用元数据、schema DDL、过程描述符、初始数据 SQL 以及一个或多个 Wasm 组件模块。

## 版本历史

| 版本 | 日期 | 摘要 |
|------|------|------|
| 1 | 2026-6-25 | 初始清单，包含 `format_version` 与文件列表。无清单的包仍被接受以保证向后兼容。 |

## 归档内容

有效的 `.mpk` 归档至少包含以下条目：

| 条目 | 必需 | 说明 |
|------|------|------|
| `package.cfg.json` | 是 | 应用元数据（`AppInfo`）。 |
| `package.desc.json` | 是 | 过程/模块描述符（`ModProcDesc`）。 |
| `ddl.sql` | 是 | Schema DDL 语句。 |
| `initdb.sql` | 打包器：是；加载器：否 | 初始数据 SQL 语句。打包器会写入该文件，但当前加载器接受缺失或为空的 `initdb.sql`。 |
| `package.manifest.json` | 否 | 前后向兼容清单。 |
| `*.wasm` | 打包器：是；加载器：否 | 一个或多个 Wasm 组件模块。当前加载器会收集存在的模块，但不会拒绝没有模块的 archive。 |

## `package.cfg.json`

JSON 对象，字段如下：

| 字段 | 类型 | 说明 |
|------|------|------|
| `name` | string | 应用名称。 |
| `lang` | string | 源语言（例如 `rust`）。 |
| `version` | string | 应用版本（语义化字符串，不是格式版本）。 |
| `use_async` | boolean | 应用是否使用异步 ABI。 |

## `package.manifest.json`

JSON 对象，字段如下：

| 字段 | 类型 | 说明 |
|------|------|------|
| `format_version` | integer | 包清单格式版本。当前值：`1`。 |
| `files` | string 数组 | 归档中的文件列表，必须包含所有必需条目。 |

## 完整性机制

- **清单存在性：** 若存在清单，加载器会验证 `format_version`。
- **格式版本检查：** 加载器拒绝非 `1` 的 `format_version`。
- **Manifest 文件列表：** 如果 manifest 存在，加载器验证 `files` 包含 `package.cfg.json`、`package.desc.json`、`ddl.sql`、`initdb.sql`。
- **运行时必需条目：** 独立于 manifest，当前加载器会拒绝缺少 `package.cfg.json`、缺少 `package.desc.json` 以及缺失或为空的 `ddl.sql`。
- **模块对齐：** 若包中恰好有一个 `.wasm` 文件且描述符中恰好有一个模块，加载器会将模块名称与描述符对齐。

## 兼容矩阵

| Reader \ Writer | 无清单 | v1 清单 |
|-----------------|--------|---------|
| current | 接受（遗留） | 兼容 |

无清单的包仍被接受，但视为遗留格式。新包必须包含 v1 清单。

## 升级与回滚规则

- **升级：** 未来 v2 清单可增加新的必需或可选字段。V2 writer 必须提升 `format_version` 并继续包含所有 v1 必需文件。
- **回滚：** v1-only 加载器打开 v2 包时拒绝，返回 `UnsupportedFormatVersion`，不会解压或执行任何归档内容。
- **迁移：** v1 → v2 迁移是打包时转换，不需要运行时迁移。

## 废弃策略

版本 `1` 仅在满足以下条件后方可废弃：
1. 所有示例应用与发布工具都输出新格式版本。
2. 构建流水线拒绝无清单的遗留包。
3. 在废弃完成前，CI 持续加载 v1 golden `.mpk` fixture。

## 参考

- 包加载器：[mudu_runtime/src/service/app_package.rs](../../../mudu_runtime/src/service/app_package.rs)
- 打包工具：[mpm_build/src/main.rs](../../../mpm_build/src/main.rs)
- 文件名常量：[mudu_runtime/src/service/file_name.rs](../../../mudu_runtime/src/service/file_name.rs)
- 应用元数据：[mudu/src/common/app_info.rs](../../../mudu/src/common/app_info.rs)
