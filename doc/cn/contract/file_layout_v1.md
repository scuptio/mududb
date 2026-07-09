# MuduDB 文件布局契约 v1

## 范围

本文档规定 MuduDB 数据库文件、目录与制品包在磁盘上的布局。它将 [page header](page_header_v1.md)、[tuple binary format](tuple_binary_v1.md)、[log frame](log_frame_v1.md)、[protocol frame](protocol_frame_v1.md)、[MPK manifest](mpk_manifest_v1.md) 与 [server configuration](mudud_cfg_v1.md) 等 lower-level 契约整合为文件级视图。

## 版本历史

| 版本 | 日期 | 摘要 |
|------|------|------|
| 1 | 2026-6-25 | 初始文件布局：time-series/relation 页文件、WAL chunk 文件、MPK 包与 TOML 配置。 |

## 逻辑数据组织

一个 MuduDB 部署包含：

- 一个或多个 **relation 文件**：存储表与索引。
- 一个或多个 **time-series 文件**：存储时序数据。
- **WAL chunk 文件**（`*.xl`）：存储 write-ahead log frame。
- 可选的 **MPK 包**（`*.mpk`）：用于分发。
- **服务端配置文件**（`mududb.toml`）。

## Relation 文件布局

一个 relation 在物理上由一对 `TimeSeriesFile` 组成：

```text
{base_path}_key   -> key 文件
{base_path}_value -> value 文件
```

每个文件都是一系列页面，页面大小在数据库创建时配置，默认为 4 KiB（`DEFAULT_PAGE_SIZE`）。

### Key 文件页布局

```text
+----------------------------------+
| Page Header (128 bytes)          |
+----------------------------------+
| Record payload area (grows down) |
| ...                              |
+----------------------------------+
| Slot array (grows up)            |
+----------------------------------+
| Page Tailer (12 bytes)           |
+----------------------------------+
```

每个 slot 存储一个 key tuple。header 与 tailer 细节参见 [Page Header Contract v1](page_header_v1.md)。

### Value 文件页布局

物理布局与 key 文件相同。每个 slot 存储 value tuple 或版本链。

## Time-series 文件布局

独立的 time-series 文件与 relation 文件采用相同的页序列布局。页面之间通过 page header 中的 `prev_page` / `next_page` 链接。

```text
+--------+--------+--------+-----+
| Page 0 | Page 1 | Page 2 | ... |
+--------+--------+--------+-----+
```

每页大小为 `page_size` 字节（默认 4096）。新页追加在文件末尾。页面大小是数据库目录的持久化属性，在未使用离线迁移工具前不得更改。

## WAL chunk 文件布局

WAL chunk 文件位于每个 worker 或每个 relation 的目录中：

```text
{log_dir}/{short_oid}.{sequence}.xl
```

- `short_oid`：由 log OID 派生的短 UUID。
- `sequence`：单调递增的 chunk 序号（`u64`）。

一个 chunk 文件由一个或多个 log frame 直接拼接而成：

```text
+----------------+----------------+----------------+
| Log Frame 0    | Log Frame 1    | ...            |
+----------------+----------------+----------------+
```

chunk 没有独立的 header 或 footer；解码时在每个 frame 边界调用 `frame_len` 进行扫描。详见 [Log Frame Contract v1](log_frame_v1.md)。

### Chunk 轮转

当前 chunk 达到 `chunk_size` 时开启新 chunk。relation WAL 的默认 chunk 大小为 256 KiB。

## MPK 包布局

MPK 包是一个 ZIP 归档，包含：

```text
package.desc.json
type.desc.json
<artifact files...>
```

manifest 语义参见 [MPK Manifest Contract v1](mpk_manifest_v1.md)。包本身在 ZIP 容器之外不添加额外帧结构。

## 服务端配置

服务端配置为 TOML 文件。参见 [MuduDB Server Configuration Contract v1](mudud_cfg_v1.md)。

## 目录约定

- Relation 文件：`{data_dir}/{partition_id}/{table_id}/`
- WAL chunk：`{wal_dir}/{log_oid_short}/`
- MPK 包：以 `.mpk` 结尾的任意路径

## 完整性机制

- **页级：** magic、version、CRC32 tailer、LSN 一致性。
- **Log frame 级：** magic、version、CRC32、header/tailer `n_part` 匹配。
- **文件级：** 无额外校验和；文件完整性由页级与 frame 级检查保证。

## 兼容矩阵

| Reader \ Writer | v1 |
|-----------------|----|
| v1 | 兼容 |

当前仅支持版本 `1`。

## 升级与回滚规则

- **升级：** 未来的 v2 文件布局必须在写入任何 v2 页或 frame 前，于新的文件头或目录标记中提升文件级格式版本。
- **回滚：** v1-only 二进制打开 v2 文件时返回 `UnsupportedFormatVersion`，不会修改文件。
- **迁移：** 文件布局迁移为离线操作。迁移工具必须重写受影响文件、保留逻辑内容，并生成新的 golden fixture。

## 废弃策略

版本 `1` 仅在以下全部满足后才可废弃：
1. 存在针对所有 v1 文件类型的离线迁移工具。
2. 新布局已作为默认值至少经历一个完整发布周期。
3. CI 持续解码 v1 golden fixture。

## 参考

- Page header：[page_header_v1.md](page_header_v1.md)
- Tuple binary format：[tuple_binary_v1.md](tuple_binary_v1.md)
- Log frame：[log_frame_v1.md](log_frame_v1.md)
- Protocol frame：[protocol_frame_v1.md](protocol_frame_v1.md)
- MPK manifest：[mpk_manifest_v1.md](mpk_manifest_v1.md)
- Server config：[mudud_cfg_v1.md](mudud_cfg_v1.md)
- 实现：
  - [mudu_kernel/src/storage/time_series/time_series_file.rs](../../../mudu_kernel/src/storage/time_series/time_series_file.rs)
  - [mudu_kernel/src/storage/relation/relation.rs](../../../mudu_kernel/src/storage/relation/relation.rs)
  - [mudu_kernel/src/wal/worker_wal_backend/layout.rs](../../../mudu_kernel/src/wal/worker_wal_backend/layout.rs)
