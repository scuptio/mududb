# MuduDB 格式与协议契约

本目录包含 MuduDB 持久化格式、网络协议与部署制品的正式版本化契约。每份契约是布局、版本、兼容性、升级规则与废弃策略的唯一事实来源。

## 活跃契约

| 契约 | 版本 | 状态 | 实现 |
|------|------|------|------|
| Page Header | [v1](page_header_v1.md) | 稳定 | [mudu_kernel/src/storage/page/format/latest.rs](../../../mudu_kernel/src/storage/page/format/latest.rs) |
| Tuple Binary Format | [v1](tuple_binary_v1.md) | 稳定 | [mudu_contract/src/tuple/tuple_binary.rs](../../../mudu_contract/src/tuple/tuple_binary.rs) |
| Log Frame（WAL/XL/PL） | [v1](log_frame_v1.md) | 稳定 | [mudu_kernel/src/wal/format/latest.rs](../../../mudu_kernel/src/wal/format/latest.rs) |
| TCP 协议帧 | [v1](protocol_frame_v1.md) | 稳定 | [mudu_contract/src/protocol/format/latest.rs](../../../mudu_contract/src/protocol/format/latest.rs) |
| MPK 包清单 | [v1](mpk_manifest_v1.md) | 稳定 | [mudu_runtime/src/service/mudu_package.rs](../../../mudu_runtime/src/service/mudu_package.rs) |
| 服务端配置 | [v1](mududb_cfg_v1.md) | 稳定 | [mudu_runtime/src/backend/mududb_cfg.rs](../../../mudu_runtime/src/backend/mududb_cfg.rs) |
| 文件布局 | [v1](file_layout_v1.md) | 稳定 | [mudu_kernel/src/storage/time_series/time_series_file.rs](../../../mudu_kernel/src/storage/time_series/time_series_file.rs) |

## 新增或修改契约

1. 在独立 PR 中提出变更。
2. 同时更新 `en/` 与 `cn/` 中的对应契约文档。
3. 更新兼容矩阵、升级规则与废弃策略。
4. 在 `testing/fixtures/golden/` 中添加或更新 golden fixture。
5. 添加兼容性与损坏测试。
6. 确保 CI 通过后再合并。

详细步骤请参考[《如何添加一次格式升级》](how_to_add_format_upgrade.md)。

## English versions

English versions are in [../en/](../en/).
