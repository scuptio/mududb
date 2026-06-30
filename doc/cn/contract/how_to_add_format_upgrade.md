# 如何添加一次格式升级

本文描述当某个持久化格式或协议格式从版本 `n` 演进到 `n+1` 时，开发者需要完成的全部工作。

## 1. 判断是否需要格式升级

以下情况通常需要一次格式版本升级：

- 新增了持久化字段。
- 删除了不再使用的字段。
- 字段的位数、编码顺序或语义发生变化。
- 魔数（magic）不变，但布局变化。

如果仅是内存中的中间表示变化，**不**需要格式升级。

## 2. 总体流程

```text
更新 contract 文档
       │
       ▼
实现 upgrade / rollback 函数
       │
       ▼
在 crate 的 migrate 模块中注册 handler
       │
       ▼
更新 CompatibilityMatrix
       │
       ▼
添加 golden fixture 和 round-trip 测试
```

## 3. 更新 contract 文档

每种格式都有双语 contract 文档：

- 英语：`doc/en/contract/<name>_v{n+1}.md`
- 汉语：`doc/cn/contract/<name>_v{n+1}.md`

新文档需要包含：

- 版本号、魔数、header 大小。
- 完整的字段布局、偏移、长度、字节序。
- 与上一版本的差异说明。
- 对于被删除的字段：说明默认值或墓碑语义；如果回滚无法恢复，必须标注为不可逆。

同时更新 `doc/cn/contract/README.md` 和 `doc/en/contract/README.md`，把新文档加入索引。

## 4. 实现 migrate handler

migrate handler 放在**拥有该格式 encode/decode 代码的 crate** 中，与格式代码同级：

| FormatKind          | migrate 目录                                            |
|---------------------|---------------------------------------------------------|
| `Page`              | `mudu_kernel/src/storage/page/migrate/`                 |
| `LogFrame`          | `mudu_kernel/src/wal/migrate/`                          |
| `ProtocolFrame`     | `mudu_contract/src/protocol/migrate/`                   |
| `TupleBinary`       | `mudu_contract/src/tuple/migrate/`                      |
| `FileLayout`        | 不需要独立 migrate，兼容性由 page/log frame 迁移保证    |
| `MpkManifest`       | 预留，暂不实现                                          |
| `ServerConfig`      | 预留，暂不实现                                          |

`FormatKind::Page` 的 migrate handler 输入/输出必须是**完整的 page binary**（按当前配置的页面大小，默认 4 KiB），而不是仅 128 字节的 page header。页格式升级时，slot 数组、tailer、记录数据都可能变化。

在对应的 `migrate/mod.rs` 中新增 `v{n}_to_v{n+1}` 函数，返回 [`MigrateHandler`]：

```rust
use mudu_compat_migrate::handler::{MigrateHandler, MigrateOption};
use mudu_compat_migrate::MigrateError;
use mudu::compat::FormatKind;

fn upgrade_v1_to_v2(
    old: &[u8],
    option: Option<&MigrateOption>,
) -> Result<Vec<u8>, MigrateError> {
    // 1. 按 v1 解码 old
    // 2. 将 v1 表示转换为 v2 表示（填充默认值、重排字段等）
    // 3. 按 v2 编码并返回
}

fn rollback_v2_to_v1(
    new: &[u8],
    option: Option<&MigrateOption>,
) -> Result<Vec<u8>, MigrateError> {
    // 1. 按 v2 解码 new
    // 2. 转换回 v1 表示；无法恢复的字段返回 MigrateError::Irreversible
    // 3. 按 v1 编码并返回
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

约束：

- `upgrade` 和 `rollback` 必须是纯函数：确定性、无副作用、不执行 IO、不使用随机/时间/环境变量。
- 输出必须是目标版本的完整有效二进制，可以直接交给目标版本的解码器。
- 当 `MigrateOption` 存在时，先读取 `option.version`，再按该版本解码 `option.payload`。

## 5. 注册到 CompatibilityRouter

全局 [`CompatibilityRouter`] 由 `mudu_kernel::compat::install_compatibility_router()` 安装。当新增格式升级时，需要更新该函数：

```rust
// mudu_kernel/src/compat.rs
router.register(FormatKind::Page, page_migrate::v1_to_v2());
```

同时调整该组件的支持窗口：

```rust
router.set_supported_window(FormatKind::Page, 1, 2);
```

## 6. 更新当前版本常量与 CompatibilityMatrix

每种格式在 `mudu/src/compat/mod.rs` 中都有一个静态当前版本常量，例如：

```rust
pub const PAGE_CURRENT_VERSION: u32 = 1;
```

升级时先把对应常量改为 `n+1`，[`CompatibilityMatrix`] 会自动跟随：

```rust
pub const PAGE_CURRENT_VERSION: u32 = 2;
```

[`CompatibilityMatrix::supported_versions`] 和 [`CompatibilityMatrix::latest_version`] 均引用这些常量，无需再手动修改每个 match 分支。

## 7. 添加测试

### 7.1 单元测试

在对应 crate 的 `migrate/mod.rs` 中增加 `#[cfg(test)]` 模块：

- `upgrade(v1) -> decode(v2)` 成功。
- `rollback(v2) -> decode(v1)` 成功。
- 尽可能验证 `rollback(upgrade(v1)) ≈ v1`。
- 对不可逆字段验证返回 `MigrateError::Irreversible`。

### 7.2 Golden fixture

1. 生成一个 v`n+1` 的 canonical fixture，保存到 `testing/fixtures/golden/v{n+1}/`。
2. 如果旧版本 fixture 需要更新迁移链，一并重新生成。
3. 在 `testing/tests/compat_golden.rs` 中新增 round-trip 测试。

生成 fixture 的测试通常是 `#[ignore]` 的一次性工具：

```rust
#[test]
#[ignore = "one-shot fixture generator"]
fn generate_golden_v2_fixtures() { ... }
```

### 7.3 跨版本 round-trip

在 `mudu_compat_migrate` 或对应 crate 中测试：

```rust
let migrated = router.upgrade_to_current(FormatKind::Page, 1, &v1_bytes, &NoopOptionProvider)?;
let decoded = PageHeader::decode(&migrated)?;
```

## 8. 验收清单

- [ ] 已新增 `doc/en/contract/<name>_v{n+1}.md` 和 `doc/cn/contract/<name>_v{n+1}.md`。
- [ ] 已更新 `README.md` 索引。
- [ ] 已在对应 crate 的 `migrate/` 目录实现 `upgrade` / `rollback`。
- [ ] `upgrade` / `rollback` 是纯函数，不执行 IO。
- [ ] 已更新 `mudu_kernel/src/compat.rs` 中的 router 注册和支持窗口。
- [ ] 已更新 `mudu/src/compat/mod.rs` 中的 `CompatibilityMatrix`。
- [ ] 已添加单元测试验证 upgrade/rollback round-trip。
- [ ] 已添加或更新 golden fixture。
- [ ] `cargo test --workspace` 全量通过。
- [ ] CI 兼容性检查通过。

[`MigrateHandler`]: ../../../mudu_compat_migrate/src/handler.rs
[`CompatibilityRouter`]: ../../../mudu_compat_migrate/src/router.rs
