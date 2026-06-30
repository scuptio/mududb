# Page Header 契约 v1

## 范围

本文档规定 MuduDB 关系表和时序文件使用的磁盘 page 格式。page 是持久化数据的最小 I/O 单元；page header 位于 page 的头部，后跟记录数据区、slot 数组和 12 字节 tailer。page 大小在数据库创建时配置，默认为 4 KiB。

## 版本历史

| 版本 | 日期 | 摘要 |
|------|------|------|
| 1 | 2026-6-25 | 初始 128 字节 page header，包含 tuple format version 与 schema hash。 |

## 整体 page 布局

```text
+----------------------------------+
| Page Header（128 字节）          |
+----------------------------------+
| Record payload area（向下增长）  |
| ...                              |
+----------------------------------+
| Slot array（向上增长）           |
+----------------------------------+
| Page Tailer（12 字节）           |
+----------------------------------+
```

有效 page 长度必须恰好为 `page_size` 字节，默认 `page_size` 为 4096（`DEFAULT_PAGE_SIZE`）。slot 数组与 payload 区相向增长。页面大小是数据库目录的持久化属性，在未进行离线迁移前不得更改。

## Header 字节布局

除非特别说明，所有多字节整数字段均为小端序。

| 偏移 | 大小 | 字段 | 说明 |
|------|------|------|------|
| 0 | 4 | `magic` | 魔数 `0x5041_4745`（ASCII `PAGE`；磁盘字节序为 `45 47 41 50`）。 |
| 4 | 4 | `version` | Page header 格式版本。当前值：`1`。 |
| 8 | 8 | `page_id` | 逻辑 page 标识符。 |
| 16 | 8 | `prev_page` | 链中上一页 id，无则为 `0xFFFF_FFFF_FFFF_FFFF`。 |
| 24 | 8 | `next_page` | 链中下一页 id，无则为 `0xFFFF_FFFF_FFFF_FFFF`。 |
| 32 | 8 | `lsn` | 日志序列号（`LSN`/`u64`）。 |
| 40 | 8 | `flags` | Page 级标志位。保留，写 `0`。 |
| 48 | 8 | `tuple_flags` | Tuple 级标志位。保留，写 `0`。 |
| 56 | 4 | `record_count` | 当前 page 中记录 slot 数量。 |
| 60 | 4 | `first_free_offset` | header 之后第一个空闲字节偏移。 |
| 64 | 4 | `free_bytes` | payload 区与 slot 数组之间连续空闲字节数。 |
| 68 | 4 | `last_record_offset` | 最后插入记录的偏移。 |
| 72 | 4 | `tuple_format_version` | 本页记录使用的 [tuple 二进制格式](tuple_binary_v1.md) 版本。 |
| 76 | 8 | `tuple_schema_hash` | Tuple 二进制描述符的 64 位类 FNV-1a 哈希。 |
| 84 | 44 | `reserved` | 保留字节；写入时必须为 0，读取时忽略。 |

Header 总大小：**128 字节**。

## Page tailer

Page tailer 位于 page 末尾，共 12 字节：

| 偏移 | 大小 | 字段 | 说明 |
|------|------|------|------|
| 0 | 8 | `lsn` | 与 header 中的 LSN 一致（冗余副本，用于恢复）。 |
| 8 | 4 | `checksum` | 整个 page 除末尾 12 字节 tailer 外内容的 CRC32。 |

## 完整性机制

- **魔数检查：** 解码器拒绝前 4 字节不是 `45 47 41 50` 的 page。
- **版本检查：** 解码器拒绝 `version == 0` 以及任何大于 `1` 的版本。
- **长度检查：** 解码器要求至少有 128 字节 header 和 12 字节 tailer。
- **CRC32：** tailer 保存 `[0, page.len() - 12)` 范围的 CRC32。
- **LSN 一致性：** 布局验证确保 header LSN 等于 tailer LSN。
- **Slot payload CRC16：** 每条记录 slot 携带其 payload 的 CRC16。

## 兼容矩阵

| Reader \ Writer | v1 |
|-----------------|----|
| v1 | 兼容 |

仅支持版本 `1`。版本 `0` 视为非法并拒绝。

## 升级与回滚规则

- **升级：** 未来 v2 writer 必须写入 v2 header/tailer，并在写入任何 v2 page 前提升文件级格式版本。
- **回滚：** 若 v1-only 二进制打开 v2 数据库，解码器返回 `UnsupportedFormatVersion`，包含实际版本与支持范围；原始数据不会被修改。
- **迁移：** 引入 v1 → v2 迁移时，必须提供离线迁移工具重写受影响的 page 并更新文件级格式标记；迁移结果需通过 golden fixture 验证。

## 废弃策略

版本 `1` 为当前稳定格式，仅在满足以下条件后方可废弃：
1. 存在针对所有已发布 v1 数据的迁移工具。
2. 新格式至少已作为默认格式经历一个完整发布周期。
3. 在废弃完成前，CI 持续解码 v1 golden fixture。

## 参考

- Tuple binary format：[tuple_binary_v1.md](tuple_binary_v1.md)
- 实现：[mudu_kernel/src/storage/page/format/latest.rs](../../../mudu_kernel/src/storage/page/format/latest.rs)
- Tailer：[mudu_kernel/src/storage/page/page_tailer.rs](../../../mudu_kernel/src/storage/page/page_tailer.rs)
- Slot 布局：[mudu_kernel/src/storage/page/record_slot.rs](../../../mudu_kernel/src/storage/page/record_slot.rs)
