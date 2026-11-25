# MuduDB 应用包契约 v1

## 范围

本文档规定 MuduDB 部署存储过程所用的 `.mpk` 应用包格式。`.mpk` 是
[Computable Artifact Format](https://github.com/scuptio/caf)（CAF）容器的一个
profile，取代此前的 `.mpk`（ZIP）归档。

包体是 CAF 容器，负责打包、内容寻址、签名与传输；包内 WASM 模块仍由 mududb
的 wasmtime 主机按 MSSP syscall ABI 执行，CAF 容器层不实例化模块。

## 版本历史

| 版本 | 日期 | 摘要 |
|------|------|------|
| 1 | 2026-11-24 | 初始契约。`.mpk`（ZIP）→ `.mpk`（flat CAF）直接替换。 |

## 形态

| 形态 | 扩展名 | 用途 |
|------|--------|------|
| flat | `.mpk` | 服务端落盘、运行时加载 |
| ZIP 导出 | `.mpk.zip` | 分发、传输（`stored`，字节确定性） |

- flat `.mpk` 是权威形态；`.mpk.zip` 是其确定性导出，可无损重建为字节一致的 flat。
- 检测：flat 以 `CAF\0` 魔数开头；`.mpk.zip` 以 ZIP 魔数 `PK\3\4` 开头。

## 归档内容

有效包包含一个默认 payload 树（`mududb`）与零或多个 WASM 组件。

payload 条目（单层）：

| 条目 | 必需 | 说明 |
|------|------|------|
| `package.cfg.json` | 是 | 应用元数据（`AppInfo`） |
| `package.desc.json` | 是 | 模块/过程描述符（`ModProcDesc`） |
| `ddl.sql` | 是 | Schema DDL |
| `initdb.sql` | 打包器：是；加载器：否 | 初始数据 SQL |
| `abi.json` | 是 | ABI 版本声明 |

`abi.json` 取代旧 `.mpk` 的 `package.manifest.json`：`format_version` 现由 CAF
容器头承担，`syscall_abi_version` 由 `abi.json` 承担。

```json
{ "syscall_abi_version": 1 }
```

WASM 组件（`ComponentRef`）：

| 字段 | 值 |
|------|-----|
| `name` | 模块名（与 `package.desc.json` 对齐） |
| `world` | `""`（raw core module） |
| `exports` | 裸 WASM 导出名 |
| `access` | `[]` |

Manifest 其余字段：

| 字段 | 值 |
|------|-----|
| `name` / `version` | `AppInfo.name` / `AppInfo.version` |
| `format_version` | `1` |
| `interface` | 空段 |
| `capabilities` | `[]` |
| `execution` | `profile = "mududb-proc"`，`deterministic = false` |
| `resources` / `policy` | 占位值（mududb 不经 caf-runtime，不生效） |
| `payloads` | 单个默认 payload |

`execution.profile = "mududb-proc"` 使包被 caf-runtime 打开时拒绝，即该包只由
mududb 执行。

## 加载

加载器（`runtime_wasm`）读取 flat `.mpk`：

1. 校验 `CAF\0` 魔数与头部 `format_version`。
2. 解析 section directory 与 manifest。
3. 验签（见下）。
4. 读取 payload 五文件并解析；缺必需条目或空 `ddl.sql` 拒绝；单模块名与描述符对齐。
5. 校验 `syscall_abi_version`。
6. 按 name 收集模块字节码。

不实现密封（`FLAG_ENCRYPTED`），遇该标志拒绝。

## 验签

- 算法 ed25519，签名覆盖 root hash（传递性覆盖全部分段）。
- signature 段为 CBOR 签名条目列表 `[{ alg, key_id, pubkey, sig }]`，`key_id = SHA256(pubkey)`。
- 每条签名须验证通过，否则拒绝。
- 信任库为 `trusted_key_fingerprints`（ed25519 公钥指纹列表）。非空时要求至少一条
  受信且有效的签名；为空时仅验签、不强制受信。
- 打包器以 ed25519 seed 签名（CI/密钥管理注入）；测试 fixture 用固定公开测试私钥。

## 版本与兼容

- 容器版本：CAF `format_version`（当前 `1`）。
- Profile 版本：本契约 v1 约束 payload 布局与 `abi.json` 结构。
- 不保留旧 `.mpk`（ZIP）的向后兼容；加载器只接受 `.mpk`（flat）。

## 参考

- CAF 容器格式：[container-format](https://github.com/scuptio/caf/blob/main/doc/spec/en/container-format.md)
- CAF 确定性往返：[round-trip](https://github.com/scuptio/caf/blob/main/doc/spec/en/round-trip.md)
- 容器 codec：`crates/common/mudu_caf`
- 加载端：`crates/db-kernel/runtime_wasm/src/service/app_package.rs`
- 打包端：`crates/tools/mpm_build`、`crates/tools/mpm_tool`
- syscall ABI：`crates/common/mudu_binding/src/codec/syscall_payload/`、`uni-syscall.wit`
