# Log Frame 契约 v1

## 范围

本文档规定 MuduDB 预写日志（WAL）、跨分区事务日志（XL）和物理日志（PL）使用的日志帧格式。日志帧是 chunk 文件中的持久化单元。

## 版本历史

| 版本 | 日期 | 摘要 |
|------|------|------|
| 1 | 2026-6-25 | 初始帧格式，包含 header、payload、tailer 与每帧 CRC32。 |

## 帧布局

```text
+-----------------------------+
| Header（24 字节）           |
+-----------------------------+
| Payload（变长）             |
+-----------------------------+
| Tailer（8 字节）            |
+-----------------------------+
```

### Header

Header 所有字段均为大端序。

| 偏移 | 大小 | 字段 | 说明 |
|------|------|------|------|
| 0 | 4 | `magic` | 魔数 `0x4C47_464D`（ASCII `LGFM`）。 |
| 4 | 4 | `version` | 日志帧格式版本。当前值：`1`。 |
| 8 | 8 | `lsn` | 日志序列号（`LSN`/`u64`）。 |
| 16 | 4 | `size` | payload 字节数。 |
| 20 | 4 | `n_part` | 同一逻辑条目在当前帧之后还剩多少帧。`0` 表示最后一帧。 |

### Tailer

Tailer 共 8 字节，大端序：

| 偏移 | 大小 | 字段 | 说明 |
|------|------|------|------|
| 0 | 4 | `n_part` | 必须与 `header.n_part` 相等。 |
| 4 | 4 | `checksum` | payload 的 CRC32（取 CRC64-XZ 的低 32 位）。 |

## 完整性机制

- **魔数检查：** 解码器拒绝魔数不是 `0x4C47_464D` 的帧。
- **版本检查：** 解码器拒绝不在 `1..=1` 范围内的版本。
- **长度检查：** 单帧至少需要 32 字节（header + tailer）；header 的 `size` 字段必须在可用字节范围内。
- **Header/tailer `n_part` 匹配：** 解码器校验两者相等。
- **CRC32：** payload checksum 与 tailer 校验值比对。
- **帧顺序：** 重组多帧条目时，后续每帧的 `n_part` 必须恰好递减 1。

## 多帧条目

当单条逻辑日志条目超过最大分片大小时，可拆分为多帧。帧按顺序连续存储，重组时按顺序拼接 payload。`n_part` 表示当前帧之后剩余的帧数，因此最后一帧的 `n_part = 0`。

## 兼容矩阵

| Reader \ Writer | v1 |
|-----------------|----|
| v1 | 兼容 |

仅支持版本 `1`。

## 升级与回滚规则

- **升级：** 未来 v2 帧需提升 `version`，可通过保留区扩展 header 或扩展 tailer。V2 writer 在数据库文件格式版本未提升前不得产生 v2 帧。
- **回滚：** v1-only reader 打开 v2 日志时返回 `UnsupportedFormatVersion`，包含实际版本与支持范围；不会重写任何日志数据。
- **迁移：** v1 → v2 迁移为离线操作：使用新帧格式重写 chunk 文件并更新文件级格式标记。

## 废弃策略

版本 `1` 仅在满足以下条件后方可废弃：
1. 存在离线迁移工具。
2. 新格式至少已作为默认格式经历一个发布周期。
3. CI 持续解码 v1 golden log frame。

## 参考

- 实现：[mudu_kernel/src/wal/format/latest.rs](../../../mudu_kernel/src/wal/format/latest.rs)
- XL 记录：[mudu_kernel/src/wal/xl_entry.rs](../../../mudu_kernel/src/wal/xl_entry.rs)
- PL 记录：[mudu_kernel/src/wal/pl_entry.rs](../../../mudu_kernel/src/wal/pl_entry.rs)
- Chunk 布局：[mudu_kernel/src/wal/worker_wal_backend/layout.rs](../../../mudu_kernel/src/wal/worker_wal_backend/layout.rs)
