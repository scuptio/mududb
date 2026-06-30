# How to Add a Format Upgrade

This guide describes the full workflow for evolving a persistent or wire format from version `n` to `n+1`.

## 1. When a Format Upgrade Is Needed

A format version bump is usually required when:

- New fields are persisted on disk or wire.
- Existing fields are removed.
- Field width, byte order, or semantics change.
- The magic value stays the same but the layout changes.

Pure in-memory representation changes do **not** require a format upgrade.

## 2. Overall Workflow

```text
Update the contract documents
            │
            ▼
Implement upgrade / rollback functions
            │
            ▼
Register the handler in the crate's migrate module
            │
            ▼
Update CompatibilityMatrix
            │
            ▼
Add golden fixtures and round-trip tests
```

## 3. Update the Contract Documents

Every format has bilingual contract documents:

- English: `doc/en/contract/<name>_v{n+1}.md`
- Chinese: `doc/cn/contract/<name>_v{n+1}.md`

The new document must contain:

- Version number, magic value, and header size.
- Complete field layout with offsets, lengths, and byte order.
- A changelog describing differences from the previous version.
- For removed fields: describe default values or tombstone semantics; mark any field as `irreversible` if rollback cannot reconstruct it.

Also update `doc/en/contract/README.md` and `doc/cn/contract/README.md` to list the new document.

## 4. Implement the Migrate Handler

Migrate handlers live in the crate that owns the format's encode/decode code, next to that code:

| FormatKind          | Migrate directory                                       |
|---------------------|---------------------------------------------------------|
| `Page`              | `mudu_kernel/src/storage/page/migrate/`                 |
| `LogFrame`          | `mudu_kernel/src/wal/migrate/`                          |
| `ProtocolFrame`     | `mudu_contract/src/protocol/migrate/`                   |
| `TupleBinary`       | `mudu_contract/src/tuple/migrate/`                      |
| `FileLayout`        | No independent migrate; compatibility relies on page/log frame migrations |
| `MpkManifest`       | Reserved; not implemented yet                           |
| `ServerConfig`      | Reserved; not implemented yet                           |

The `FormatKind::Page` migrate handler must operate on the **complete page binary** at the configured page size (default 4 KiB), not just the 128-byte page header.  When the page format evolves, the slot array, tailer, and record payloads may also change.

Add a `v{n}_to_v{n+1}` function in the corresponding `migrate/mod.rs` that returns a [`MigrateHandler`]:

```rust
use mudu_compat_migrate::handler::{MigrateHandler, MigrateOption};
use mudu_compat_migrate::MigrateError;
use mudu::compat::FormatKind;

fn upgrade_v1_to_v2(
    old: &[u8],
    option: Option<&MigrateOption>,
) -> Result<Vec<u8>, MigrateError> {
    // 1. Decode `old` as v1.
    // 2. Convert the v1 representation to v2 (fill defaults, reorder fields, etc.).
    // 3. Encode as v2 and return.
}

fn rollback_v2_to_v1(
    new: &[u8],
    option: Option<&MigrateOption>,
) -> Result<Vec<u8>, MigrateError> {
    // 1. Decode `new` as v2.
    // 2. Convert back to v1; return MigrateError::Irreversible for unrecoverable fields.
    // 3. Encode as v1 and return.
}

pub fn v1_to_v2() -> MigrateHandler {
    MigrateHandler {
        from: 1,
        to: 2,
        upgrade: upgrade_v1_to_v2,
        rollback: rollback_v2_to_v1,
    }
}
```

Constraints:

- `upgrade` and `rollback` must be pure functions: deterministic, side-effect free, no IO, no randomness, no time or environment access.
- The output must be a complete, valid binary for the target version and directly consumable by the target decoder.
- When a `MigrateOption` is present, read `option.version` first and decode `option.payload` accordingly.

## 5. Register with the CompatibilityRouter

The global [`CompatibilityRouter`] is installed by `mudu_kernel::compat::install_compatibility_router()`. When adding a format upgrade, update that function:

```rust
// mudu_kernel/src/compat.rs
router.register(FormatKind::Page, page_migrate::v1_to_v2());
```

Also adjust the supported window for that component:

```rust
router.set_supported_window(FormatKind::Page, 1, 2);
```

## 6. Update the Current Version Constant and CompatibilityMatrix

Each format has a static current-version constant in `mudu/src/compat/mod.rs`, e.g.:

```rust
pub const PAGE_CURRENT_VERSION: u32 = 1;
```

When bumping a format, update the corresponding constant to `n+1`; [`CompatibilityMatrix`] will follow automatically:

```rust
pub const PAGE_CURRENT_VERSION: u32 = 2;
```

[`CompatibilityMatrix::supported_versions`] and [`CompatibilityMatrix::latest_version`] reference these constants, so you do not need to edit every match arm by hand.

## 7. Add Tests

### 7.1 Unit Tests

Add a `#[cfg(test)]` module in the crate's `migrate/mod.rs`:

- `upgrade(v1) -> decode(v2)` succeeds.
- `rollback(v2) -> decode(v1)` succeeds.
- Verify `rollback(upgrade(v1)) ≈ v1` when possible.
- For irreversible fields, verify `MigrateError::Irreversible` is returned.

### 7.2 Golden Fixtures

1. Generate a canonical v`n+1` fixture under `testing/fixtures/golden/v{n+1}/`.
2. If older fixtures need to be re-generated to validate the migration chain, update them as well.
3. Add round-trip tests in `testing/tests/compat_golden.rs`.

Fixture generation tests are usually `#[ignore]` one-shot tools:

```rust
#[test]
#[ignore = "one-shot fixture generator"]
fn generate_golden_v2_fixtures() { ... }
```

### 7.3 Cross-Version Round-Trip

Test in `mudu_compat_migrate` or the owning crate:

```rust
let migrated = router.upgrade_to_current(FormatKind::Page, 1, &v1_bytes, &NoopOptionProvider)?;
let decoded = PageHeader::decode(&migrated)?;
```

## 8. Checklist

- [ ] Added `doc/en/contract/<name>_v{n+1}.md` and `doc/cn/contract/<name>_v{n+1}.md`.
- [ ] Updated the `README.md` index in both languages.
- [ ] Implemented `upgrade` / `rollback` in the owning crate's `migrate/` directory.
- [ ] `upgrade` / `rollback` are pure functions with no IO.
- [ ] Updated the router registration and supported window in `mudu_kernel/src/compat.rs`.
- [ ] Updated `CompatibilityMatrix` in `mudu/src/compat/mod.rs`.
- [ ] Added unit tests for upgrade/rollback round-trip.
- [ ] Added or updated golden fixtures.
- [ ] `cargo test --workspace` passes.
- [ ] CI compatibility gate passes.

[`MigrateHandler`]: ../../../mudu_compat_migrate/src/handler.rs
[`CompatibilityRouter`]: ../../../mudu_compat_migrate/src/router.rs
