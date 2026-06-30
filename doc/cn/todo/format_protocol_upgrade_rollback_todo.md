# 格式与协议升级/回滚机制设计 TODO

## 目标

为所有落盘格式（page header、log frame、tuple、WAL/XL/PL 等）和外部协议帧（protocol frame、handshake 等）建立一套**可扩展、可验证、可回滚**的版本迁移机制。

核心要求：

1. 任何格式从版本 `n` 升级到 `n+1`，开发者必须同时提供：
   - `upgrade(old_binary, option_binary) -> new_binary`
   - `rollback(new_binary, option_binary) -> old_binary`
2. 这两个函数必须是**纯函数**：确定性、无副作用、不执行 IO、不访问全局状态。
3. 数据库上层提供**兼容路由管理器**，按模块/组件名和版本注册/查找迁移函数，支持：
   - 向上升级：`n -> n + k`（`k > 0`，不一定是 1）
   - 向下回滚：`n -> n - k`（`k > 0`，不一定是 1）
4. 运行时策略：
   - 版本等于当前期望版本 → 走正常解码路径。
   - 版本低于当前期望版本 → 走兼容路径，按最近支持的版本链式调用升级函数。
   - 版本高于当前期望版本 → 走兼容路径，按最近支持的版本链式调用回滚函数。

---

## 核心抽象

### 1. 组件标识

每个需要版本迁移的模块/格式称为一个 **component**，用字符串或枚举唯一标识：

```rust
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Component {
    PageHeader,
    LogFrame,
    TupleFormat,
    ProtocolFrame,
    Handshake,
    // 未来扩展：MPK manifest、server config 等
}
```

### 2. 版本号

版本号使用无符号整数（建议 `u32`），从 `1` 开始递增。`0` 保留为“未设置/未知”。

```rust
pub type FormatVersion = u32;
```

### 3. 迁移函数签名

```rust
/// 可选辅助信息。
///
/// `payload` 的具体结构随 `version` 不同而不同，因此它必须自带版本号。
/// 迁移函数根据 `version` 选择对应的解码器来解析 `payload`。
pub struct MigrateOption {
    pub version: FormatVersion,
    pub payload: Vec<u8>,
}

/// 升级函数：把旧版本二进制转换为新版本二进制。
///
/// - `old`: 旧版本完整二进制数据。
/// - `option`: 可选的辅助信息（schema、字段映射表、checksum 上下文等）。
///   对于无状态迁移，可传 `None`。
///   当 `option` 存在时，函数必须先读取 `option.version`，再按该版本解码
///   `option.payload`。
pub type UpgradeFn =
    fn(old: &[u8], option: Option<&MigrateOption>) -> Result<Vec<u8>, MigrateError>;

/// 回滚函数：把新版本二进制转换为旧版本二进制。
pub type RollbackFn =
    fn(new: &[u8], option: Option<&MigrateOption>) -> Result<Vec<u8>, MigrateError>;
```

要求：

- **确定性**：相同输入必须产生相同输出，禁止依赖随机、时间、环境变量。
- **无副作用**：不修改文件、不访问网络、不打印日志、不修改全局变量。
- **完整性**：输出必须是目标版本的完整有效二进制，可以直接交给目标版本的解码器。
- **可逆性**：理想情况下 `rollback(upgrade(x)) == x`（允许在语义等价范围内存在差异，例如填充字段）。
- **选项版本感知**：`option` 的解码方式取决于其自身的 `version` 字段，不能假设所有步骤使用同一种结构。

### 4. 迁移句柄

```rust
pub struct MigrateHandler {
    pub from: FormatVersion,
    pub to: FormatVersion,
    pub upgrade: UpgradeFn,
    pub rollback: RollbackFn,
}
```

### 5. 选项二进制自身的版本化

`MigrateOption` 的二进制表示必须能够自描述版本。建议的最小结构：

```text
+-------------+------------------+
| version(u32) | payload(variable) |
+-------------+------------------+
```

`CompatibilityRouter` 本身不解析 `payload`，只使用 `version` 来匹配迁移步骤。
迁移函数内部根据 `option.version` 决定解码逻辑：

```rust
fn upgrade_page_header_v1_to_v2(
    old: &[u8],
    option: Option<&MigrateOption>,
) -> Result<Vec<u8>, MigrateError> {
    let schema = match option {
        Some(opt) if opt.version == 1 => decode_v1_schema(&opt.payload)?,
        Some(opt) if opt.version == 2 => decode_v2_schema(&opt.payload)?,
        Some(_) => return Err(MigrateError::UnsupportedOptionVersion),
        None => Default::default(),
    };
    // ... 执行迁移
}
```

---

## 开发者契约

当某个组件的格式需要从 `n` 变更到 `n+1` 时：

1. 在 `doc/en/contract/` 和 `doc/cn/contract/` 下更新或新增 contract 文档。
2. 在代码中实现 `n -> n+1` 的 `upgrade` 和 `rollback`。
3. 在兼容路由管理器中注册该迁移句柄。
4. 为该迁移函数编写 property test：
   - 随机旧版本二进制 → upgrade → 解码成功。
   - 新版本二进制 → rollback → 解码成功。
   - 尽可能验证 `rollback(upgrade(x)) ≈ x`。
5. 如果 `n -> n+1` 不可逆（例如丢失了某些字段），`rollback` 必须返回明确的 `MigrateError::Irreversible` 错误，而不是静默构造错误数据。

---

## 高层兼容路由管理器

### 1. 设计目标

- 集中管理所有组件的所有版本迁移函数。
- 根据“组件 + 当前版本 + 目标版本”自动计算迁移链。
- 支持跨多版本跳跃（`n -> n+k`），优先使用直接注册的处理器；否则拆分为单步链式调用。
- 暴露简单 API 给解码/编码入口使用。

### 2. 注册接口

```rust
/// 选项提供器：为某次具体迁移步骤提供对应版本的 `MigrateOption`。
///
/// 因为 `MigrateOption` 的格式本身也可能随版本变化，路由器不能把一个
/// 固定的 option 直接传给所有步骤。调用方可以实现该 trait，根据
/// `(component, version)` 返回合适的选项，或在不需要时返回 `None`。
pub trait OptionProvider {
    fn get(
        &self,
        component: Component,
        version: FormatVersion,
    ) -> Option<MigrateOption>;
}

pub struct CompatibilityRouter {
    current_versions: HashMap<Component, FormatVersion>,
    handlers: HashMap<Component, Vec<MigrateHandler>>,
}

impl CompatibilityRouter {
    /// 设置某组件的当前期望版本。
    pub fn set_current_version(&mut self, component: Component, version: FormatVersion);

    /// 注册一个迁移句柄。
    pub fn register(&mut self, component: Component, handler: MigrateHandler);

    /// 把 `binary` 从 `from_version` 升级到当前版本。
    pub fn upgrade_to_current(
        &self,
        component: Component,
        from_version: FormatVersion,
        binary: &[u8],
        options: &dyn OptionProvider,
    ) -> Result<Vec<u8>, MigrateError>;

    /// 把 `binary` 从当前版本回滚到 `to_version`。
    pub fn rollback_from_current(
        &self,
        component: Component,
        to_version: FormatVersion,
        binary: &[u8],
        options: &dyn OptionProvider,
    ) -> Result<Vec<u8>, MigrateError>;
}
```

### 3. 路径计算

管理器内部把版本看作有向图节点，每个 `MigrateHandler` 提供两条有向边：

- `upgrade`: `from -> to`（向上）
- `rollback`: `to -> from`（向下）

给定起始版本和目标版本，使用 BFS/DFS 在版本图中寻找最短路径。路径上的每条边调用对应的迁移函数。

示例：

```text
注册处理器：
  PageHeader: 1 -> 2, 2 -> 3

场景 1：读取 v1 page header，当前期望 v3
  路径：v1 --upgrade--> v2 --upgrade--> v3

场景 2：需要向只支持 v1 的组件输出 page header
  路径：v3 --rollback--> v2 --rollback--> v1

场景 3：直接注册了 v1 -> v3 的升级处理器（优化）
  路径：v1 --upgrade--> v3
```

### 4. 运行时决策

在格式/协议解码入口：

```rust
let version = parse_version(&bytes)?;
let current = router.current_version(Component::PageHeader);

if version == current {
    // 正常路径：直接使用最新解码器
    PageHeader::decode(&bytes)
} else if version < current {
    // 兼容路径：先升级，再解码
    let migrated = router.upgrade_to_current(
        Component::PageHeader,
        version,
        &bytes,
        &SchemaOptionProvider { schema: &schema },
    )?;
    PageHeader::decode(&migrated)
} else {
    // 版本高于当前：回滚后解码（通常用于服务端收到新客户端数据时拒绝或降级处理）
    let migrated = router.rollback_from_current(
        Component::PageHeader,
        version,
        &bytes,
        &SchemaOptionProvider { schema: &schema },
    )?;
    PageHeader::decode(&migrated)
}
```

---

## 与现有代码的集成点

当前已有：

- `mudu/src/compat/mod.rs`：`CompatibilityMatrix`、`FormatKind`、`VersionRange`。
- `mudu_kernel/src/storage/page/format/latest.rs`：page header v1。
- `mudu_kernel/src/wal/format/latest.rs`：log frame v1。
- `mudu_contract/src/protocol/frame.rs`：protocol frame v1。
- `testing/tests/compat_golden.rs`：golden fixture 兼容性测试。

新增集成：

1. 新建独立 crate `mudu_compat_migrate`（仅依赖纯基础 crate `mudu`），作为迁移框架，存放：
   - `src/router.rs`：`CompatibilityRouter`
   - `src/handler.rs`：`MigrateHandler`、`UpgradeFn`、`RollbackFn`、`MigrateOption`、通用 identity 函数
   - `src/error.rs`：`MigrateError`
2. 每种 `FormatKind` 的 migrate handler 下沉到拥有该格式 encode/decode 代码的 crate，与格式代码同级：
   - `mudu_kernel/src/storage/page/migrate/mod.rs` → `FormatKind::Page`
   - `mudu_kernel/src/wal/migrate/mod.rs` → `FormatKind::LogFrame`
   - `mudu_contract/src/protocol/migrate/mod.rs` → `FormatKind::ProtocolFrame`
   - `mudu_contract/src/tuple/migrate/mod.rs` → `FormatKind::TupleBinary`
   - `file_layout` 不需要独立 migrate 文件夹，其兼容性由 page/log frame 的兼容性保证。
3. 在 `Cargo.toml` workspace 中注册 `mudu_compat_migrate`，并在 `mudu_kernel` / `mudu_contract` 中添加对该 crate 的依赖。
4. 在 `mudu_kernel` 初始化时构建全局 `CompatibilityRouter`（或通过依赖注入传入）。
5. 解码入口统一先查版本，再决定正常路径或兼容路径。
6. 扩展 golden fixture 测试：为每个组件的每个历史版本生成 fixture，并验证 `upgrade_to_current` 和 `rollback_from_current` 的 round-trip。

---

## 长期可维护增强（建议）

为了让这套机制在格式/协议持续演化时仍然易于维护，建议补充以下设计或工具：

### 1. 每个历史版本的 golden fixture 与迁移矩阵 CI

不要只保留 v1 和当前版本的 fixture。为每个中间版本都保留 canonical fixture，并在 CI 中：

- 对注册过的每条直接边 `(n -> n+1)` 做 upgrade/rollback round-trip；
- 对最短迁移路径上的每个 `(from, to)` 组合做端到端 round-trip；
- 对损坏/截断数据验证错误码不变。

这样任何一次对迁移函数的意外改动都会被立即捕获。

### 2. 纯函数的静态强制

迁移函数必须无副作用，但口头约定容易违背。建议：

- 把所有迁移实现放到独立 crate（如 `mudu_compat_migrate`），仅依赖纯计算库；
- 在该 crate 中禁止 `std::fs::*`、`std::net::*`、随机、时间、环境变量；
- 使用 clippy 自定义 lint 或 `cargo-deny` 做依赖审计。

### 3. 兼容矩阵作为唯一事实来源

`CompatibilityMatrix` 应由 `CompatibilityRouter` 的注册信息自动生成，而不是手写。CI 增加门禁：

- 代码中的 `current_version` 与 contract 文档声明的版本不一致则失败；
- 新增格式版本但未注册迁移函数则失败；
- 迁移路径存在断点（从 `min_supported_version` 到 `current_version` 不可达）则失败。

### 4. 被删除字段的默认值与墓碑语义

contract 中应显式声明：

- 升级时移除的字段在旧版本中如何映射到新版本（默认值、聚合计算、丢弃）；
- 回滚时无法恢复的字段应标记为 `irreversible`，`rollback` 返回 `MigrateError::Irreversible`。

`MigrateOption` 可以携带这些默认值/映射表，避免迁移函数硬编码。

### 5. Dry-run / 路径验证 API

在真正解码数据之前，调用方可以执行：

```rust
router.validate_path(Component::PageHeader, 1, current)?;
```

用于启动时自检，提前发现版本窗口配置错误，而不是等到读数据时才失败。

### 6. 管理器层面的可观测性

迁移函数内部保持纯函数、不打印日志；但 `CompatibilityRouter` 可以记录：

- 本次走了哪条迁移链；
- 每一步的耗时与结果；
- 失败时的完整上下文（component、from、to、step index、原始错误）。

这对线上排障至关重要。

### 7. 协议版本协商

对于网络协议，服务端应在握手阶段广播自己支持的版本范围：

- 客户端版本在当前窗口内 → 正常处理；
- 客户端版本低于窗口 → 服务端升级；
- 客户端版本高于窗口 → 服务端回滚或明确拒绝，并返回 `supported_versions`。

这比简单回滚更安全，也避免客户端收到无法理解的旧格式。

### 8. 组件间依赖与批量迁移

某些升级可能涉及多个格式同时变更（例如 tuple 格式与 page header 同时升级）。管理器应支持：

- 声明组件间依赖顺序；
- 一次批量迁移，全部成功则提交，任一失败则整体回滚。

### 9. Property-based 测试与快照回归测试

- 使用 proptest/fuzz 生成随机合法旧版本数据，验证 `upgrade -> decode -> rollback -> encode` 的 round-trip；
- 使用 `insta` 等工具对典型输入的迁移输出做快照，防止无意的输出变化。

### 10. 迁移链缓存

`CompatibilityRouter` 构建时预计算所有 `(from, to)` 的最短路径并缓存，运行时只做查表和顺序调用，避免每次 BFS。

### 11. 支持版本窗口与废弃策略

为每个组件定义 `min_supported_version`。低于该版本的数据不再迁移，而是返回 `UnsupportedFormatVersion`。这防止迁移链无限增长，也明确告知用户哪些旧数据需要外部工具先处理。

### 12. 自动生成人读兼容矩阵

从 `CompatibilityRouter` 的注册信息生成 `doc/cn/compat_matrix.md` 和 `doc/en/compat_matrix.md`，列出每个组件的当前版本、支持范围、已知不可逆变更。

---

## TODO

- [x] 定义 `MigrateError`、`MigrateOption`、`MigrateHandler`。
- [x] 设计 `OptionProvider` trait，使每个迁移步骤能拿到对应版本的辅助信息。
- [x] 实现 `CompatibilityRouter`：注册、路径计算、链式调用、按步骤获取 `MigrateOption`。
- [x] 将 migrate handler 下沉到各格式所属 crate（`mudu_kernel`、`mudu_contract`），每种格式提供 dummy identity handler。
- [ ] 为 `page_header` v1 -> v2 编写真正的示例 `upgrade` / `rollback`（当格式真正演进到 v2 时替换 dummy）。
- [ ] 为 `log_frame` 和 `protocol_frame` 编写真正的示例迁移函数。
- [x] 在解码入口（`PageHeader::decode`、`LogFrameHeader::decode`、`Frame::decode` 等）接入路由管理器。
- [x] 编写基础迁移测试：链式升级/回滚、版本窗口、缺失 handler、versioned option。
- [ ] 编写 property test：随机旧版本数据 → upgrade → decode → rollback → 与原始数据比较。
- [ ] 扩展 golden fixture：在 `testing/fixtures/golden/` 下为每个组件保留历史版本 fixture，CI 自动测试迁移链。
- [x] 文档化开发者契约：在 `doc/cn/contract/` 和 `doc/en/contract/` 下新增“如何添加一次格式升级”的指南。
- [x] 设计 `min_supported_version` 与版本窗口，防止迁移链无限增长。
- [x] 将迁移框架迁移到独立 crate `mudu_compat_migrate`（仅依赖纯基础 crate `mudu`，避免 IO）。
- [ ] 为迁移函数添加静态 purity 检查（clippy 自定义 lint / cargo-deny 禁止 IO/随机/时间/环境变量依赖）。
- [x] 设计 dry-run / `validate_path` API，用于启动时自检。
- [ ] 设计协议版本协商：握手阶段广播支持版本范围。
- [ ] 建立每个历史版本的 golden fixture，CI 自动跑迁移矩阵 round-trip。
- [ ] 设计组件间依赖与批量迁移机制。

---

## 验收标准

- [ ] 任意已注册组件在 `min_supported_version..=current_version` 范围内的迁移可自动完成。
- [ ] 迁移函数均为纯函数，可在单元测试中离线运行，不依赖文件系统或网络。
- [ ] 版本等于当前版本时，不执行任何迁移，直接走正常解码路径。
- [ ] 版本低于当前版本但在支持窗口内时，自动找到最短迁移链并升级。
- [ ] 版本高于当前版本但在支持窗口内时，自动找到最短迁移链并回滚（协议场景优先协商降级）。
- [ ] 版本超出支持窗口时返回结构化错误，包含组件名、实际版本和支持范围。
- [ ] 缺少迁移函数或迁移路径断开时返回结构化错误，包含组件名、起始版本、目标版本。
- [ ] CI 覆盖每条注册边和至少一条跨版本 round-trip 测试。
- [ ] 兼容矩阵由 `CompatibilityRouter` 注册信息自动生成，并与代码保持同步。
