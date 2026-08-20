# Filesystem 类型实现 TODO 需求

## 背景

设计文档已定稿：`doc/cn/filesystem.cn.md`。核心决策：

- DDL `CREATE TYPE FILESYSTEM FILE|DIRECTORY|... name` 注册 fs object 类型（§2.1）；列直接以类型名声明 FS 列（§3.4）；
- per-tuple 对象采用 M1：1 行 1 对象，OID（u128）标识；对象创建只走 FS 列的 `INSERT`/`UPDATE`，OID 获取只走 `SELECT` FS 列（§2.2、§3.3）；
- 内容是不可变 generation，布局 `{data_dir}/fs/{fs_id}/{oidhex}.{generation}[.{entry}]`（oidhex 为 u128 的 32 位小写 hex，generation 为 u64 十进制；FILE 对象无 entry 后缀，DIRECTORY entry 可含 `/`，§2.3）；元数据 `_fs_object` 与普通表共享 CC/WAL，generation 级 MVCC（§3.7）；
- guest 寻址 = `oid: u128` + `path: string`（对象内相对路径）；fd 为 `u32`（§2.4）；
- syscall 面 11 个：`fs-open/close/read/write/pread/pwrite/lseek/fstat/stat/fsync/readdir`（§4.1）；guest API 为纯函数（§6.1）。

已核实的实现锚点：

- DDL 手解析先例：`sql_parser/src/ast/parser/entry.rs` 的 `try_parse_custom_statement`（`CREATE PARTITION RULE` 同款），kernel 侧链路 `Binder::bind_command`（`mudu_kernel/src/sql/binder.rs:55`）→ `BoundCommand`（`bound_stmt.rs:20`）→ `Planner::plan_command`（`planner.rs:41`）→ `CmdExec` executor；
- 命名类型列：`sql_parser/src/ast/parser/column.rs:82` `visit_data_type_kind` 当前对未知标识符 `NotImplemented`；`binder.rs` `schema_column_from_ast`（:630）负责落到 `SchemaColumn`；
- catalog 先例：`mudu_kernel/src/meta/schema_catalog.rs`（固定 table id、`Relation` 持久化 KV、catalog xid 非事务）；`MetaMgrImpl` 内存 `scc::HashMap` + `initialize_inner` replay + `ddl_lock`；`MetaMgr` trait 默认方法可加成员不破坏 mock；
- `_fs_object` 将是首个**随用户事务**写入的系统表：`TxMgr::put_relation/delete_relation`（`mudu_kernel/src/x_engine/tx_mgr.rs`），同事务同 WAL 同 MVCC；
- DML 钩子：`mudu_kernel/src/command/insert_key_value.rs:62`、`update_key_value.rs:61`、`delete_key_value.rs:58`（delete 需先 `read_key` 取旧 OID）；
- `WorkerLocal`（`mudu_kernel/src/server/worker_local.rs:28`）加默认方法 `fs_service()`，真实状态挂 `WorkerRuntime`；mock 实现（`mudu_prepared_stmt.rs:148`、`handshake_test.rs:208`）不受破坏；
- 恢复挂钩：`recover_worker_log_tokio` 之后（`server.rs:479` tokio；`mudu_kernel/src/server/linux/server_iouring.rs` io_uring）；GC 为首个周期后台任务（`spawn_local_task` + `stop_channel` 模式，`server.rs:481` 先例）；
- 测试：`command/*_test.rs` colocated 单测（`MockIoProvider` 先例 `save_to_file_test.rs:160`）；集成测试在 `testing/tests/`（`testing::support`）。

## 分阶段实施策略

实现按 S0–S5 六个阶段推进，每个阶段切在**可演示的语义边界**上，都有可独立验收的中间产物：

| 阶段 | 内容 | 工作项 | 里程碑验收 |
|---|---|---|---|
| **S0 契约先行** | uni-fs schema + codec | W1 | 11 函数帧往返单测通过，接口冻结 |
| **S1 SQL 层闭环** | DDL → 类型 catalog → FS 列绑定 → `_fs_object` 事务钩子 | W2→W3→W4→W5 | mcli 可 `CREATE TYPE FILESYSTEM`、建 FS 列表、INSERT/SELECT 得 OID，提交/回滚可见性正确（对象尚无内容 IO） |
| **S2 对象 IO 核心** | FsService + fd 表 + generation 存储 → GC/恢复 | W6→W7 | host 层 fd 全生命周期（MockIoProvider）；崩溃恢复清孤儿；GC 按 horizon 回收 |
| **S3 guest 端到端** | WIT 边界 → Rust API 双版本 → mtp 注册 | W8→W9→W12 | WASM procedure 经 `mudu_fs_*` 完成读写全流程；`mtp rust --async` 生成正确异步代码 |
| **S4 硬化** | 端到端测试 + 全量验证 | W10 | `testing/tests/` 全流程用例通过；fmt/clippy/test 三件套零告警 |
| **S5 后续（按需独立排期）** | 复制通道、AS/C#、payload 路由 | W11、Phase 2、Phase 3 | 各自验收 |

要点：

- S1 完成 = SQL 语义完整；S2 完成 = host 崩溃安全；S3 完成 = guest 可用。避免全部工作绑到最后才能验证。
- 并行：S0 ∥ S1（W1 与 W2 无依赖）；W8 可在 S2 末段提前启动（依赖 W1+W6）；W10 的单测部分**贯穿** S0–S3，S4 只做端到端与全量验证。
- 风险与缓解：`_fs_object` 是首个随用户事务写的系统表（S1 重点验证隔离级别行为）；GC 是首个周期后台任务（S2 重点验证干净退出与崩溃窗口）；mtp 裸名导入约束（S3 同步写入用户文档）。

## S0 契约先行

### W1 canonical schema + codec（mudu_binding）

- 文件：`mudu_binding/wit/uni-fs-open-argv.wit`、`uni-fs-stat.wit`、`uni-fs-dirent.wit`（新增）；`mudu_binding/wit/uni-syscall.wit`（声明 11 个函数签名）；`mudu_binding/src/codec/handle_sys_fs.rs` + `handle_sys_fs_test.rs`（新增）；`mudu_binding/src/codec/mod.rs`（登记）。
- 要点：`uni-fs-stat = { oid: uni-oid, generation: u64, entry: string, length: u64, state: u32 }`；`uni-fs-dirent = { name: string, is_dir: bool, length: u64 }`；codec 为手写大端帧 + `MERR` 错误封套（复用 `handle_sys_session.rs` 原语）；`cargo make generate`（mgen）后刷新 `mudu_binding/wit/contract.md5.txt`。
- 依赖：无。
- 验收：`handle_sys_fs_test.rs` 覆盖 11 个函数的参数/结果/错误帧往返；`cargo test -p mudu_binding` 通过。

## S1 SQL 层闭环

### W2 DDL：CREATE/DROP TYPE FILESYSTEM（sql_parser + kernel binder/planner/command）

- 文件：`sql_parser/src/ast/stmt_create_fs_type.rs`、`stmt_drop_type.rs`（新增）；`sql_parser/src/ast/mod.rs`、`stmt_type.rs`（`StmtCommand::CreateFsType`/`DropType`）；`sql_parser/src/ast/parser/entry.rs`（`create type filesystem` / `drop type` 前缀分支）；
  kernel 侧：`mudu_kernel/src/sql/binder.rs`（`bind_command`）、`bound_stmt.rs`（`BoundCreateFsType`/`BoundDropType`）、`planner.rs`（`plan_command`）、`mudu_kernel/src/command/create_fs_type.rs`、`drop_fs_type.rs`（新增，`CmdExec`：`prepare/run/affected_rows`，参数结构放 `x_engine/x_param.rs`）、`command/mod.rs`（登记）。
- 要点：语法 `CREATE TYPE FILESYSTEM FILE|DIRECTORY name` / `DROP TYPE name`；解析复用 `partition.rs` 的 `split_top_level_csv`/`find_keyword_position` 风格；DDL catalog 持久化沿用现状（非事务 catalog xid）。
- 依赖：无（与 W1 并行）。
- 验收：`create_fs_type` / `drop_fs_type` colocated 单测（仿 `create_table_test.rs`）；重名报 `AlreadyExists`；仅管理员会话可执行。

### W3 fs object 类型 catalog（kernel meta）

- 文件：`mudu_kernel/src/meta/fs_type_catalog.rs`（新增，固定 table id 如 `0x5`，`open_fs_type_catalog` / `write_fs_type_to_catalog` / `delete_fs_type_from_catalog`）；`meta/mod.rs`、`meta_mgr.rs`（内存 `scc::HashMap` 对、`CatalogRelation` 扩展、`initialize_inner` replay、`create_fs_type_inner`/`drop_fs_type_inner` 走 `ddl_lock`）；`mudu_kernel/src/contract/meta_mgr.rs`（trait 加默认方法）。
- 要点：表项 `{ name, fs_id, kind }`；fs_id 单调分配、实例内唯一；存储根 `{data_dir}/fs/{fs_id}` 派生（`ServerCfg::data_dir`）。
- 依赖：W2（executor 调用）。
- 验收：重启后 replay 恢复类型映射；`DROP TYPE` 时校验仍被 FS 列引用则报错。

### W4 FS 列绑定（parser + binder）

- 文件：`sql_parser/src/ast/parser/column.rs`（`visit_data_type_kind` 接受未知标识符为命名类型引用）；`mudu_kernel/src/sql/binder.rs`（`schema_column_from_ast`：命中 fs 类型 catalog 则落物理 `U128` `SchemaColumn` 并记录列 → 类型绑定）。
- 要点：绑定记录随表 schema 持久化（TableDesc 扩展或绑定字段）；未知类型名报 `NotFound`。
- 依赖：W3。
- 验收：`CREATE TABLE product (id U64 PRIMARY KEY, photo photo_fs)` 绑定成功；未注册类型名报错。

### W5 `_fs_object` 系统表 + DML 钩子

- 文件：`mudu_kernel/src/meta/` 或 `storage/` 内 `_fs_object` Relation 定义（固定 table id）；`mudu_kernel/src/command/insert_key_value.rs`（`insert_inner` 循环内钩子）、`update_key_value.rs`（run 钩子）、`delete_key_value.rs`（run 前先 `read_key` 取旧 OID）。
- 要点：行格式 `(oid -> { fs_id, kind, generation, length, state })`，rmp-serde 编码；钩子经 `param.tx_mgr.put_relation(fs_rel_id, oid_key, value)` 写入——同事务同 WAL 同 MVCC（设计文档 §3.7）；INSERT/UPDATE 绑定新 OID 时插 PENDING 行；DELETE/换绑使旧对象行版本不可见；`_fs_object` 物理 relation 与所属行的 `(table_id, partition_id)` 对齐；OID 内嵌 partition 身份为后续项（MVP OID = `{8-bit tag 0xF5, 120-bit random}`，设计文档 §3.8）。
- 依赖：W3、W4。
- 验收：事务提交后 `_fs_object` 行对其他事务可见；回滚不可见；与 SQL 行一致的隔离级别行为。

## S2 对象 IO 核心

### W6 fs 服务 + fd 表 + generation 存储（kernel server）

- 文件：`mudu_kernel/src/server/fs_service.rs`（新增）；`mudu_kernel/src/server/worker_local.rs`（trait 加默认方法 `fs_service()`）；`mudu_kernel/src/server/worker.rs`（`WorkerRuntime` 持有实例）。
- 要点：实现设计文档 §2.3 寻址（oid → `_fs_object` → kind/存储根/布局；MVP resolve 仅在本 worker 查 `_fs_object`，OID 内嵌 partition 身份路由为后续项，跨 worker 访问由客户端 port sharding，§3.8）、§2.4 fd 表（按 session 分组、u32 数值复用、`close_async` 回收、PENDING 私有 generation、`fs-close` fsync 并写 length、提交 pointer swap）；对象内规范化（`..`/绝对路径/NUL）；IO 经注入的 `Arc<dyn AsyncIoProvider>`（`ServerRuntimeDeps::async_runtime`，不用全局默认）。
- 依赖：W1（codec 类型）、W5。
- 验收：fd 全生命周期集成测试（MockIoProvider 注入）；关闭 fd 访问报 `BadFileDescriptor`；READ flags 语义对齐 §4.5 错误表。实现结构以设计文档 §5 为准。

### W7 GC worker + 启动恢复扫描

- 文件：`mudu_kernel/src/server/fs_gc.rs`（新增）；`mudu_kernel/src/server/server.rs`（tokio 后端 `recover_worker_log_tokio` 后挂钩，:479）；`mudu_kernel/src/server/linux/server_iouring.rs`（io_uring 同款）。
- 要点：恢复扫描 = `_fs_object` replay 完成后，按 `{oidhex}.{gen}` 前缀规则扫描各存储根，清理无任何可见行引用的 generation；GC 为首个周期后台任务——tokio 后端为 `spawn_local_task` + `stop_channel` 常驻 loop，io_uring 后端无法睡眠、由 service loop 每轮检查间隔并 re-spawn 单轮 GC 任务（`submit_fs_gc_round_if_due`）——以 oldest snapshot 为 horizon 回收老 generation（`AsyncFs::remove_dir_all`/`remove_file_if_exists`）。
- 依赖：W6。
- 验收：恢复扫描以「generation 描述记录 + `_fs_object` 状态」为输入（不仅是目录扫描，见「同一份日志的恢复/复制分析」）；模拟崩溃后孤儿 generation 被清理；删行越过 horizon 后文件消失；服务停止时 GC 任务干净退出。

## S3 guest 端到端

### W8 WIT 边界（mudu_runtime + sys_interface/wit）

- 文件：`mudu_runtime/wit/api.wit`、`async-api.wit`（各加 11 个函数）；`sys_interface/wit/sync/api.wit`、`sys_interface/wit/async/async-api.wit`（同步副本，顺带补 sync 副本缺失的 `delete`）；`mudu_runtime/src/service/wasi_context_component.rs`（sync/async 两个 Host impl）；`mudu_runtime/src/service/kernel_function_p2.rs`、`kernel_function_p2_async.rs`；`mudu_runtime/src/interface/kernel.rs`（`fs_open_internal` 等 11 个）。
- 要点：字节管道 `list<u8>` 风格与 KV 一致；错误统一 `serialize_error_result`；bindgen 重新生成 trait 后编译错误即实现清单。
- 依赖：W1、W6。
- 验收：`cargo build -p mudu_runtime` 通过；host 端 handler 单测（直接喂帧）。

### W9 Rust guest API（sys_interface + mudu_adapter）

- 文件：`sys_interface/src/fs.rs`（新增，11 个 `mudu_fs_*` 纯函数）；`sys_interface/src/host.rs`（`invoke_host_fs_*`）；`sys_interface/src/inner_component.rs`、`inner_component_async.rs`（WIT 调用）；`sys_interface/src/api_impl/sync_wasm.rs`、`async_wasm.rs`（导出）；`mudu_adapter/src/local_fs.rs`（standalone 路径的本地文件模拟，sqlite/postgres/mysql 驱动共享，根目录 `{db_path}.fs/`，单 generation=1，无 MVCC/DDL/catalog；mudud 驱动返回 `NotImplemented`）。
- 要点：API 形态见设计文档 §6.4（纯函数，首参 `session_id: OID` 与 KV 惯例一致，`flags` 取 libc `O_*` 值）；sync_api / async_api 双版本（§6.4 的双版本结构）。
- 依赖：W8。
- 验收：standalone-adapter 下原生测试直接调用 11 个函数全通过；POSIX 语义与 errno 映射以设计文档 §6.1–6.3 为准。

### W12 mtp 注册 fs syscall 家族

- 文件：`mudu_transpiler/src/rust/parse_context.rs`（`sys_call` 集合加入 11 个 `mudu_fs_*` 名字，与 `mudu_query` 等同处）；`mudu_transpiler/src/rust/test_rs/proc1.rs`（fixture 加 fs 调用用例）；`mudu_transpiler/src/rust/rust_parser.rs` 内联测试（断言 `async_api` 路径替换与 `.await` 拼接）。
- 要点：mtp 的 sync→async 机制（模块路径替换 + `async`/`.await` 位置拼接 + `tran_to_async` 调用图传播）对注册名字自动生效，仅此一处注册点；调用须为裸名导入（约束写入用户文档）；AssemblyScript 路径不涉及（无 sync→async 概念）。
- 依赖：W9（`async_api` 的 fs 函数先存在）。
- 验收：含 `mudu_fs_*` 调用的 proc 经 `mtp rust --async` 后生成 `use sys_interface::async_api::...` 且调用点带 `.await`；`cargo test -p mudu_transpiler` 通过。

## S4 硬化

### W10 测试与全量验证

- 单测部分**贯穿** S0–S3 随各工作项交付：codec 单测（W1）；catalog/DDL 单测（W2/W3）；kernel 集成（W6/W7）；DIRECTORY 用例（构建期写 entry → 提交后只读；`..` 逃逸负例；对 FILE 对象 `fs-readdir` 报 `NotADirectory`）；安全负例（伪造/穷举 OID）。
- 本阶段（S4）只做端到端与全量验证：
  - 端到端：进程内全流程（mudu_kernel 集成测试，真实 tokio provider 驱动 FsService）+ `testing/tests` 真实客户端 SQL 层测试，完成「INSERT 行 → SELECT 得 OID → open 写 → fsync → close → 提交 → 重开读 → stat」全流程；未新增 wasm 工件（WASM procedure 级 e2e 为后续项）。
  - 全量验证（AGENTS.md 要求）：`cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --no-run --workspace`；`cargo clippy -p testing --all-targets -- -D warnings`。

## 同一份日志的恢复/复制分析

**问题**：同一份日志格式能否同时实现 filesystem 和普通数据的恢复/复制？

**结论**：能，但分两层——元数据与普通数据天然共用同一条 WAL（恢复/复制自动统一）；内容字节不进 WAL，但用**同一日志格式**新增少量「generation 描述记录」，把内容生命周期锚定进同一 LSN 序列；复制时由日志驱动一个伴随的内容传输通道。

### 1. 元数据层：已经统一（无需新工作）

- `_fs_object` 行经 `TxMgr::put_relation` 写入，与普通行落同一个 `xl_batch`（`mudu_kernel/src/x_engine/tx_mgr.rs` 的 `staged_relation_ops` / `build_write_ops` / `xl_batch`）——同一 LSN 顺序、同一原子提交、同一 MVCC。
- 恢复：WAL replay 同时恢复普通行与 `_fs_object` 行，原子性一致。
- 复制（WAL shipping）：follower replay 同一日志即得 fs 对象的存在性/可见性。
- 这是设计文档 §3.7「共享日志」的直接推论，零额外工作。

### 2. 内容层：同一日志格式 + 描述记录（不装内容字节）

- 不变式（设计文档 §3.7 提交协议）：「generation 文件写完并 fsync」先于「`_fs_object` 提交记录进 WAL」。
- 在 `xl` 日志系列（`mudu_kernel/src/wal/xl_data_op.rs` 等）新增少量记录类型：generation `CREATE`（oid、generation、length、checksum）与 `ABORT/GC`——每条只有几十字节元数据，不触发设计文档 §3.6 拒绝的 WAL 放大。
- **恢复**：replay 时对照描述记录与 `_fs_object` 状态，判定 generation 文件为已提交/未提交/孤儿 → 本地清理（W7 恢复扫描的消费输入）。
- **复制**：follower replay 元数据后，按 `(oid, generation)` 经伴随通道拉取内容文件，用日志里的 **checksum 校验**后才对读可见；文件不可变使拉取天然幂等、可断点续传。读一致性 = 元数据 replay 点 + 所需 generation 文件已就位。
- 参照：SQL Server FILESTREAM（日志记录协调外部文件）、Iceberg/Delta（事务日志/manifest + 不可变数据文件复制）、FoundationDB blob granules（版本化元数据在日志内核，bulk 内容走外部通道）。

### 3. GC 协调

主侧 GC 的 horizon 需取所有副本 oldest snapshot 的最小值，避免 follower 仍在读取的老 generation 被提前回收。

### 4. W11 复制通道（可选，S5）

> **状态：明确跳过（2026-07）**——推迟至未来的复制项目；当前代码库尚无任何复制/follower 基础设施，W11 的前置条件不存在。

- 文件：`mudu_kernel/src/wal/xl_data_op.rs`（新增 generation `CREATE`/`ABORT` 记录类型）；`mudu_kernel/src/server/fs_replication.rs`（新增：follower 内容拉取 + checksum 校验）；`mudu_kernel/src/server/fs_gc.rs`（horizon 副本协调）。
- 要点：描述记录随 generation 生命周期写入（fsync 后、`_fs_object` 提交前）；follower 以 `(oid, generation)` 为单位幂等拉取，校验通过才对读开放。
- 依赖：W6、W7。
- 验收：主从切换后 fs 对象可读且内容 checksum 校验通过；follower 读取期间主侧不误回收老 generation。

## S5 后续阶段

### Phase 2：AssemblyScript / C# 绑定

- AS：`bindings/{component-shim,rs-shim,assemblyscript}/wit/api.wit` 三份同步声明；rs-shim 薄封装透传；`bindings/assemblyscript/assembly/fs.ts` 纯函数集合（`fsOpen`/`fsRead`/`fsWrite`/`fsReaddir` 等，与 Rust 侧一一对应，手写 canonical ABI 同 `wit.ts` 风格）。先同步版；异步版随 AS async ABI 支持跟进（设计文档 §6.5）。
- C#：`mgen message -l csharp` 生成 `uni-fs-*` 模型；`mudu_api/csharp` 的 `MuduSysCallApi` 扩展，`Mudu.cs` 门面暴露 `MuduFileSystem`。
- 验收：`example/wallet-as` 级 AS guest 完成对象写入/读取；C# demo 通过。

### Phase 3：SyscallPayload v1 路由

- 按 `message_kind` 将 11 个 fs 函数纳入 MSSP 路由器，对齐 `doc/cn/todo/project-controlled-guest-host-abi.md` Phase 3；必要时 bump `SYSCALL_PAYLOAD_CURRENT_VERSION`（`mudu/src/compat/mod.rs`）。
- **状态：已完成（2026-07）**——11 个 fs 函数已作为 `message_kind` 10–20 落地于 `mudu_binding::codec::syscall_payload` 路由器，格式版本保持 v1 未 bump。

## 依赖顺序

```text
S0（W1）∥ S1（W2→W3→W4→W5）→ S2（W6→W7）→ S3（W8→W9→W12）→ S4（W10 端到端）
W8 依赖 W1+W6，可与 S2 末段重叠启动；W10 的单测部分贯穿 S0–S3
S5：W11 依赖 W6/W7；Phase 2 依赖 W8/W9；Phase 3 依赖 S0–S4 全部
```
