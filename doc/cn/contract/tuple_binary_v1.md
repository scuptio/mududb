# Tuple Binary Format Contract v1

## 范围

本文档规定 MuduDB tuple 的二进制格式，包括内存格式与落盘格式。Tuple 存储在 page payload、log entry 与网络 frame 中。该二进制布局独立于 SQL 类型系统，但由描述字段顺序、可空性以及定长/变长分类的 `TupleBinaryDesc` 派生而来。

## 版本历史

| 版本 | 日期 | 摘要 |
|------|------|------|
| 1 | 2026-6-25 | 初始 tuple 二进制格式：null bitmap、slot array、定长数据区与变长数据区。 |

## 术语

- **Field（字段）：** tuple 中的一个列值。
- **定长字段：** 编码大小可由类型描述符确定的字段（如 `INT4`、`BOOL`）。
- **变长字段：** 编码大小不固定的字段（如 `TEXT`、`BYTEA`、`NUMERIC`）。
- **Slot：** 8 字节描述符 `(offset: u32, length: u32)`，用于定位变长字段数据。
- **Null bitmap：** 指示哪些可空字段为 `NULL` 的位向量。

## 整体 tuple 布局

Tuple 是一个连续的字节缓冲区，布局如下：

```text
+----------------------------------+
| Null Bitmap（可变长度，按 8 字节 |
| 对齐）                           |
+----------------------------------+
| Slot Array（每个变长字段 8 字节）|
+----------------------------------+
| Fixed-Length Data Area           |
+----------------------------------+
| Variable-Length Data Area        |
+----------------------------------+
```

- null bitmap 位于偏移 `0`。
- slot array 紧跟 null bitmap 之后。
- 定长字段存储在 slot array 之后的固定偏移处。
- 变长字段附加在末尾；slot array 指向每个值的位置。

## Null bitmap

Null bitmap 占用 `aligned_byte_len(nullable_count)` 字节，向上取整到 8 的倍数。

| 属性 | 值 |
|------|-----|
| 位序 | 每个字节内为小端位序（`bit_idx % 8`）。 |
| `true` 含义 | 字段为 `NULL`。 |
| `false` 含义 | 字段非空。 |
| 对齐 | 长度按 8 字节对齐。 |

没有可空字段的 tuple，null bitmap 长度为 0。

## Slot

每个 slot 为 8 字节，小端序：

| 偏移 | 大小 | 字段 | 说明 |
|------|------|------|------|
| 0 | 4 | `offset` | 变长字段数据相对于 tuple 起始位置的字节偏移。 |
| 4 | 4 | `length` | 变长字段数据的字节长度。 |

Slot 按 schema 顺序为所有变长字段依次存储。

## 定长数据区

定长字段按 schema 定义偏移存放在 slot array 之后。每个定长字段的偏移和长度由 `TupleBinaryDesc` 派生，不重复存储在 tuple 字节中。

定长数据区会按需填充，使变长数据区在类型描述符要求时从 8 字节对齐偏移开始。

## 变长数据区

变长字段值连续附加在 tuple 末尾。每个值的位置由其 slot 描述。变长区中值的顺序与 schema 中变长字段的顺序一致。

变长字段值可能为 0 字节；此时其 slot 的 `length == 0`，`offset` 指向 tuple 末尾。

## TupleBinaryDesc

`TupleBinaryDesc` 是驱动编解码的外部 schema 描述符。它不属于 tuple 字节，但解释 tuple 字节所必需。描述符包含：

- `type_desc`：有序的字段 `DatType` 列表。
- `slot_all`：有序的 `FieldDesc` 记录列表，每条记录包含：
  - `is_fixed_len`：定长/变长分类。
  - `slot`：定长字段的偏移和长度；变长字段的 slot 存放位置。
  - `nullable` 与 `null_bit_idx`：可空性信息。
- `fixed_count`、`var_count`、`total_fixed_size`：派生计数。
- `nullable_count`：可空字段数量（null bitmap 大小）。
- `row_format_version`：保留给未来 tuple 格式版本使用。

## 编码过程

1. 计算 `min_tuple_size = null_bitmap_size + var_count * slot_size + total_fixed_size`。
2. 分配 `min_tuple_size` 字节的缓冲区并清零。
3. 按 schema 顺序处理每个字段：
   - 若值为 `NULL`，设置对应 null bitmap 位并跳过数据写入。
   - 若为定长字段，将编码后的值复制到定长数据偏移处。
   - 若为变长字段，将编码后的值附加到缓冲区末尾，并写入其 slot。
4. 将最终 null bitmap 复制到缓冲区起始位置。

## 解码过程

1. 验证 tuple 长度至少为 `min_tuple_size`。
2. 从 tuple 起始位置读取 null bitmap。
3. 按 schema 顺序处理每个字段：
   - 若 null bitmap 指示 `NULL`，返回 `NULL`。
   - 若为定长字段，从 `slot.offset` 读取 `slot.length` 字节。
   - 若为变长字段，从 `slot.offset` 读取 slot，再从该 slot 存储的偏移读取 `slot.length` 字节。

## 完整性机制

- **长度检查：** 解码器拒绝短于 `min_tuple_size` 的 tuple。
- **Null bitmap 边界：** bit index 必须小于 `nullable_count`。
- **Slot 边界：** 变长 slot 指向的位置必须在 tuple 缓冲区内。
- **定长大小检查：** 编码后的定长值大小必须与描述符一致。

## 兼容矩阵

| Reader \ Writer | v1 |
|-----------------|----|
| v1 | 兼容 |

当前仅支持版本 `1`。

## 升级与回滚规则

- **升级：** 未来的 v2 tuple 格式必须提升 `row_format_version`，并可添加 tuple 级 header、更宽的偏移或尾部校验和。v2 写入端必须在部署中所有读取端都理解 v2 后，才写入 v2 tuple。
- **回滚：** v1-only 读取端遇到 v2 tuple 时返回 `UnsupportedFormatVersion`，不会重写 tuple。
- **迁移：** tuple 格式迁移为离线操作。迁移工具使用旧描述符读取 v1 tuple，重新编码为 v2，并更新表级 tuple format version。

## 废弃策略

版本 `1` 仅在以下全部满足后才可废弃：
1. 存在针对所有 v1 tuple 的离线迁移工具。
2. 新格式已作为默认值至少经历一个发布周期。
3. CI 持续解码 v1 golden tuple。

## 参考

- Page header：[page_header_v1.md](page_header_v1.md)
- Log frame：[log_frame_v1.md](log_frame_v1.md)
- File layout：[file_layout_v1.md](file_layout_v1.md)
- 实现：
  - [mudu_contract/src/tuple/tuple_binary.rs](../../../mudu_contract/src/tuple/tuple_binary.rs)
  - [mudu_contract/src/tuple/tuple_binary_desc.rs](../../../mudu_contract/src/tuple/tuple_binary_desc.rs)
  - [mudu_contract/src/tuple/slot.rs](../../../mudu_contract/src/tuple/slot.rs)
  - [mudu_contract/src/tuple/bitmap.rs](../../../mudu_contract/src/tuple/bitmap.rs)
  - [mudu_contract/src/tuple/nullable_tuple.rs](../../../mudu_contract/src/tuple/nullable_tuple.rs)
