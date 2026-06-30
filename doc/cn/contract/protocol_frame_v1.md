# TCP 协议帧契约 v1

## 范围

本文档规定 MuduDB 自定义 TCP 客户端/服务器连接使用的定长头二进制帧格式。它承载 `mcli`/客户端库与 `mudud` 之间的请求/响应消息。

## 版本历史

| 版本 | 日期 | 摘要 |
|------|------|------|
| 1 | 2026-6-25 | 初始 40 字节 header，包含魔数、版本、32 位消息类型、64 位 flags、request id、trace id 与 payload 长度。 |

## 帧布局

```text
+-----------------------------+
| Header（40 字节）           |
+-----------------------------+
| Payload（变长）             |
+-----------------------------+
```

每帧由单个 header 和 payload 组成。传输层负责交付精确字节范围；header 之外没有额外的长度前缀。

### Header

Header 所有字段均为大端序。

| 偏移 | 大小 | 字段 | 说明 |
|------|------|------|------|
| 0 | 4 | `magic` | 魔数 `0x4D53_464D`（ASCII `MSFM`）。 |
| 4 | 4 | `version` | 协议帧版本。当前值：`1`。 |
| 8 | 4 | `message_type` | 消息类型判别值（见下表）。 |
| 12 | 8 | `flags` | 帧标志位。第 0 位（`0x1`）为追踪采样标志；其余位保留，必须为 `0`。 |
| 20 | 8 | `request_id` | 客户端分配的请求标识符。 |
| 28 | 8 | `trace_id` | 分布式追踪标识符。 |
| 36 | 4 | `payload_len` | payload 字节长度。 |

### 消息类型

| 值 | 名称 | Payload 编码 |
|----|------|--------------|
| 1 | Handshake | `rmp_serde` 编码的 `HandshakeRequest` / `HandshakeResponse` |
| 2 | Auth | `rmp_serde` 编码的认证负载 |
| 3 | Query | `rmp_serde` 编码的 `ClientRequest` |
| 4 | Execute | `rmp_serde` 编码的 `ClientRequest` |
| 5 | Batch | `rmp_serde` 编码的 `ClientRequest` |
| 6 | Response | `rmp_serde` 编码的 `ServerResponse` |
| 7 | Error | `rmp_serde` 编码的 `ErrorResponse` |
| 8 | Get | `rmp_serde` 编码的 `GetRequest` |
| 9 | Put | `rmp_serde` 编码的 `PutRequest` |
| 10 | RangeScan | `rmp_serde` 编码的 `RangeScanRequest` |
| 11 | ProcedureInvoke | `rmp_serde` 编码的 `ProcedureInvokeRequest` |
| 12 | SessionCreate | `rmp_serde` 编码的 `SessionCreateRequest` |
| 13 | SessionClose | `rmp_serde` 编码的 `SessionCloseRequest` |

## 握手与版本协商

协议定义握手消息用于版本协商：

- `HandshakeRequest` 包含 `supported_versions: Vec<u32>` 与可选的 `capabilities: Vec<String>`。
- `HandshakeResponse` 包含 `selected_version: u32` 与可选的服务端能力标签。

当前服务端实现仅选择版本 `1`。若客户端未在 `supported_versions` 中包含 `1`，则返回 `IncompatibleProtocolVersion`。

## 完整性机制

- **魔数检查：** 解码器拒绝魔数不是 `0x4D53_464D` 的帧。
- **版本检查：** 解码器拒绝任何非 `1` 的版本。
- **长度检查：** 解码器要求至少有 40 字节 header 和 `payload_len` 字节 payload。
- **消息类型校验：** 未知消息类型值被拒绝。
- **Payload 长度校验：** 解码器要求 header 后至少有 `payload_len` 字节，并只把这一段传给 `Frame::from_parts`。输入缓冲区可以继续包含后续 frame 的字节。
- **标志位校验：** 解码器拒绝设置了未定义标志位的帧（目前仅定义第 0 位）。

## 兼容矩阵

| Reader \ Writer | v1 |
|-----------------|----|
| v1 | 兼容 |

仅支持版本 `1`。

## 升级与回滚规则

- **升级：** 未来 v2 帧可扩展 header（如增加能力标志位）或定义新消息类型。V2 客户端与服务端必须在握手阶段协商版本，之后才能发送 v2 帧。
- **回滚：** v1 服务端收到仅支持 v2 的帧时返回 `UnsupportedFormatVersion`；v2 服务端收到 v1-only 客户端时握手阶段选择版本 `1`。
- **破坏性变更：** 小版本协议可增加新消息类型；删除或重命名消息类型需要主版本号提升。

## 废弃策略

版本 `1` 仅在满足以下条件后方可废弃：
1. 所有附带客户端与驱动支持新版本。
2. 新版本至少已作为默认版本经历一个发布周期。
3. 兼容性测试矩阵覆盖 v1 client / v2 server 与 v2 client / v1 server。

## 参考

- 帧格式：[mudu_contract/src/protocol/format/latest.rs](../../../mudu_contract/src/protocol/format/latest.rs)
- 消息类型与 payload：[mudu_contract/src/protocol/mod.rs](../../../mudu_contract/src/protocol/mod.rs)
- 握手处理：[mudu_kernel/src/server/handlers/handshake.rs](../../../mudu_kernel/src/server/handlers/handshake.rs)
