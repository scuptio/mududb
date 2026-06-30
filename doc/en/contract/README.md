# MuduDB Format and Protocol Contracts

This directory contains the formal, versioned contracts for MuduDB's persistent formats, network protocols, and deployment artifacts. Each contract is the source of truth for layout, versioning, compatibility, upgrade rules, and deprecation policy.

## Active contracts

| Contract | Version | Status | Implementation |
|----------|---------|--------|----------------|
| Page Header | [v1](page_header_v1.md) | Stable | [`mudu_kernel/src/storage/page/format/latest.rs`](../../../mudu_kernel/src/storage/page/format/latest.rs) |
| Tuple Binary Format | [v1](tuple_binary_v1.md) | Stable | [`mudu_contract/src/tuple/tuple_binary.rs`](../../../mudu_contract/src/tuple/tuple_binary.rs) |
| Log Frame (WAL/XL/PL) | [v1](log_frame_v1.md) | Stable | [`mudu_kernel/src/wal/format/latest.rs`](../../../mudu_kernel/src/wal/format/latest.rs) |
| TCP Protocol Frame | [v1](protocol_frame_v1.md) | Stable | [`mudu_contract/src/protocol/format/latest.rs`](../../../mudu_contract/src/protocol/format/latest.rs) |
| MPK Package Manifest | [v1](mpk_manifest_v1.md) | Stable | [`mudu_runtime/src/service/mudu_package.rs`](../../../mudu_runtime/src/service/mudu_package.rs) |
| Server Configuration | [v1](mududb_cfg_v1.md) | Stable | [`mudu_runtime/src/backend/mududb_cfg.rs`](../../../mudu_runtime/src/backend/mududb_cfg.rs) |
| File Layout | [v1](file_layout_v1.md) | Stable | [`mudu_kernel/src/storage/time_series/time_series_file.rs`](../../../mudu_kernel/src/storage/time_series/time_series_file.rs) |

## Adding or changing a contract

1. Propose the change in a dedicated PR.
2. Update the relevant contract document in both `en/` and `cn/`.
3. Update the compatibility matrix, upgrade rules, and deprecation policy.
4. Add or update golden fixtures in `testing/fixtures/golden/`.
5. Add compatibility and corruption tests.
6. Ensure CI passes before merging.

See [How to Add a Format Upgrade](how_to_add_format_upgrade.md) for the detailed workflow.

## Chinese versions

中文版本见 [../cn/](../cn/)。
