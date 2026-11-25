# MuduDB Format and Protocol Contracts

This directory contains the formal, versioned contracts for MuduDB's persistent formats, network protocols, and deployment artifacts. Each contract is the source of truth for layout, versioning, compatibility, upgrade rules, and deprecation policy.

## Active contracts

| Contract | Version | Status | Implementation |
|----------|---------|--------|----------------|
| Page Header | [v1](page_header_v1.md) | Stable | [`storage/src/page/format/latest.rs`](../../../crates/db-kernel/storage/src/page/format/latest.rs) |
| Tuple Binary Format | [v1](tuple_binary_v1.md) | Stable | [`mudu_contract/src/tuple/tuple_binary.rs`](../../../crates/common/mudu_contract/src/tuple/tuple_binary.rs) |
| Log Frame (WAL/XL/PL) | [v1](log_frame_v1.md) | Stable | [`wal/src/format/latest.rs`](../../../crates/db-kernel/wal/src/format/latest.rs) |
| TCP Protocol Frame | [v1](protocol_frame_v1.md) | Stable | [`mudu_contract/src/protocol/format/latest.rs`](../../../crates/common/mudu_contract/src/protocol/format/latest.rs) |
| MPK Application Package | [v1](caf_mududb_profile_v1.md) | Stable | [`runtime_wasm/src/service/app_package.rs`](../../../crates/db-kernel/runtime_wasm/src/service/app_package.rs) |
| Server Configuration | [v1](mudud_cfg_v1.md) | Stable | [`mudu_runtime/src/backend/mudud_cfg/mod.rs`](../../../crates/db-kernel/mudu_runtime/src/backend/mudud_cfg/mod.rs) |
| File Layout | [v1](file_layout_v1.md) | Stable | [`storage/src/time_series/time_series_file.rs`](../../../crates/db-kernel/storage/src/time_series/time_series_file.rs) |
| Guest→Host Syscall Payload | [v1](syscall_payload_v1.md) | Stable | [`mudu_binding/src/codec/syscall_payload/`](../../../crates/common/mudu_binding/src/codec/syscall_payload/) (host router; registry in `mudu/src/compat/mod.rs`) |

## Adding or changing a contract

1. Propose the change in a dedicated PR.
2. Update the relevant contract document in both `en/` and `zh-CN/`.
3. Update the compatibility matrix, upgrade rules, and deprecation policy.
4. Add or update golden fixtures in `testing/fixtures/golden/`.
5. Add compatibility and corruption tests.
6. Ensure CI passes before merging.

See [How to Add a Format Upgrade](how_to_add_format_upgrade.md) for the detailed workflow.

## Chinese versions

中文版本见 [zh-CN contracts](../../zh-CN/contract/)。

- [Queue envelope and durable watermark v1](queue_v1.md)
