# MuduDB Filesystem 类型设计

本文档描述为 MuduDB 新增 **filesystem 类型** 的设计：一种由 DDL 注册、由 DB 组织与管理存储的 **fs object 类型**。该类型对象的全部文件都由 DB 组织——fs object 以 OID 唯一标识，内容是不可变 generation 文件，元数据 `_fs_object` 与普通表共享同一套并发控制（CC）与日志（WAL）机制。Mudu Procedure（WASM guest）通过一组 POSIX 风格系统调用访问对象，每个 IO 操作对应一个 MuduDB 内部系统调用，并封装为各 guest 语言的原生风格 API（替代对应语言 std library 的文件 API）。guest 不能绕过 DB 直接创建文件，filesystem 也不映射任何用户可见的宿主目录。

核心使用模型：

1. 管理员用 DDL 注册 fs object 类型：`CREATE TYPE FILESYSTEM FILE photo_fs;`；
2. 表声明 FS 列（列类型即 fs object 类型名）：`photo photo_fs`；
3. guest（procedure）在事务内 `INSERT` 一行（kernel 为 FS 列创建对象），`SELECT` 该列得到 **OID**，`fs-open(oid, "", O_WRONLY)` 写入内容后关闭；
4. 读取时 `SELECT` 得到 OID，`fs-open(oid, "", O_RDONLY)` 返回 fd，之后的 `read` / `pread` / `lseek` / `close` 等调用与 POSIX IO 语义一致；
5. 事务提交后对象内容对其他事务可见；删除行后对象由 GC 回收。

## 1. 设计目标与非目标

### 目标

- guest（WASM 组件）获得实用的文件读写能力，且 **host 保留全部控制权**：guest 只接触 OID 与 fd，永远接触不到宿主路径；容器内不存在未经 DB 登记的文件。
- fs object 与普通表**共享同一套 CC 和 WAL 机制**：对象的可见性、写写冲突、隔离级别与普通表一致（见 §3.7）。
- 每个文件操作是一个独立的 MuduDB 系统调用，复用现有 guest↔host 边界（`mududb:api/system` 与 `mududb:async-api/system` 字节管道 WIT 接口，见 `mudu_runtime/wit/api.wit`、`mudu_runtime/wit/async-api.wit`）。
- host 侧文件 IO 全部经由 `mudu_sys::fs::*`（`mudu_sys_contract` 的 `AsyncFs` / `AsyncFile` trait），遵守 workspace 的 clippy 禁令（`mudu_sys_impl` 之外不得直接使用 `std::fs`）。
- 多语言 guest API：Rust（`sys_interface`）、AssemblyScript（`bindings/assemblyscript`）、C#（`mudu_api/csharp`）。

### 非目标

- 不提供跨对象的目录 / 路径操作：对象间命名空间是扁平的 OID 空间；目录结构只存在于 DIRECTORY 对象内部（不可变），没有链接、重命名概念。
- 不实现完整 VFS / inode 语义，不支持权限位管理、硬链接。
- 不向 guest 暴露宿主路径、文件描述符或 `RawFd`。
- 不改变现有 SQL / KV / 会话系统调用的语义。

## 2. 概念模型

### 2.1 fs object 类型注册（DDL）

fs object 类型通过 DDL 声明，元数据进入 kernel 的 catalog：

```sql
CREATE TYPE FILESYSTEM FILE photo_fs;
CREATE TYPE FILESYSTEM DIRECTORY asset_fs;
DROP TYPE photo_fs;
```

- filesystem 是一种 **fs object 类型**：`FILE` 为单内容对象，`DIRECTORY` 为可含多个 entry 的目录树对象；kind 集合可扩展（`CREATE TYPE FILESYSTEM <DIRECTORY | FILE | ...> name`）。
- 类型对象的全部文件布局与存储位置由 DB 管理（见 §2.3）；filesystem **不映射**任何用户可见的宿主目录，guest 不接触、也不能在容器内自行创建文件。
- 类型的存储根由 DB 从 `ServerCfg::data_dir` 派生（host 内部细节，不进 DDL 语义）。未来可为类型增加存储根覆盖选项（便于运维挂载独立卷），其语义仅是 DB 管理的存储位置，不构成对宿主目录的映射。
- 读写模式由 `fs-open` 的 flags（`O_RDONLY` / `O_WRONLY` / `O_RDWR`）按 POSIX 语义指定，DDL 层面不设只读属性。

实现位置：

- DDL 解释器：`mudu_kernel/src/command/` 新增 `create_fs_type.rs` / `drop_fs_type.rs`（参照现有 `save_to_file.rs` 等命令的组织方式）。
- 元数据：`mudu_kernel/src/meta/` 新增 fs object 类型 catalog，表项为 `{ name, fs_id, kind }`，生命周期与现有 meta catalog（如 `schema_catalog`）一致。
- DDL 权限：已实现最小权限基建——client 协议会话（`session_create` / `open_session_with_config`）为 admin，procedure 内部会话非 admin；`MuduConnCore::execute_inner` 对 `CREATE TYPE FILESYSTEM` / `DROP TYPE` 校验，非 admin 报 `PermissionDenied`。`DROP TYPE` 时校验是否仍被 FS 列引用（见 §3.3）。

### 2.2 fs_id（内部）

- catalog 在 `CREATE TYPE FILESYSTEM` 时分配一个持久 `u64` 作为 **fs_id**（实例内唯一）。
- fs_id 是 **host 内部 id**：`_fs_object` 记录对象所属类型、派生类型的存储根（§2.3）都使用它；它不对 guest 暴露——guest 获取对象 OID 的唯一途径是 `SELECT` FS 列（含本事务内未提交的行，MVCC 可见性与普通列一致），创建对象的唯一途径是 FS 列的 `INSERT` / `UPDATE`（见 §3.4），不存在独立的 select / create 系统调用。

### 2.3 对象寻址与 host 侧布局

guest 侧唯一的寻址形式是**对象寻址**，由两个独立参数组成：

- `oid: u128`：fs object 的 OID（二进制值，非字符串形式）；MVP 实现为位段 `{8-bit tag = 0xF5, 120-bit random}`（`gen_fs_oid`，`mudu_kernel/src/meta/fs_object.rs`），随机位段保持不可猜测；partition 身份内嵌用于路由是后续项（见 §3.8）；
- `path: string`：对象内相对路径——FILE 类型对象必须为空或 `.`；DIRECTORY 类型对象为 entry 相对路径，需做**对象内规范化**：拒绝 `..` 越出对象根（`PermissionDenied`）、绝对路径分量、NUL 等非法字符（`InvalidFilename`，errno 36）——即对象内沙箱。

host 处理流程：

1. 按 OID 查 `_fs_object` 元数据表（见 §3.4 / §3.7）得到 `{ fs_id, kind, 当前 generation, length, state }`；未知 OID 报 `NotFound`。
2. 由 fs_id 查 fs object 类型 catalog 得到 `{ kind }`，并由 fs_id 派生存储根。
3. 按 DB 管理的布局定位内容文件：

   ```text
   {存储根}/{oidhex}.{generation}[.{entry 相对路径}]
   ```

   存储根 = `{data_dir}/fs/{fs_id}`，由 `ServerCfg::data_dir` 派生（host 内部细节）。`oidhex` 为 u128 的 32 位小写 hex；`generation` 为 u64 十进制。FILE 对象的内容即文件 `{oidhex}.{generation}`（无 entry 后缀）；DIRECTORY 对象的 entry 内容在 `{oidhex}.{generation}.{entry}`——entry 相对路径可含 `/`，写入时按需创建父目录，中间前缀形成真实宿主目录（如 entry `a/b.txt` 即文件 `{oidhex}.{generation}.a/b.txt`）。GC/恢复扫描按前缀规则回收：宿主条目名 `== "{oidhex}.{gen}"` 或以 `"{oidhex}.{gen}."` 开头（点号边界，避免 gen 1 误配 gen 12）。
4. 宿主路径完全由 host 按上述规则生成：guest 输入只有 OID 与对象内相对路径，且后者经对象内规范化——不存在路径注入面；host 内部仍保证一切文件操作限定在存储根之内。

### 2.4 fd 模型

- `fs-open` 成功返回一个 **fd**（`u32`），fd 是**会话作用域**的句柄：fd 表挂在会话上，`mudu_close(session_id)` 时回收该会话的全部 fd。宽度对齐 POSIX 的 `int`：fd 会话局部、数值可复用，32 位对「每会话 fd 上限」（§7）远远过剩；跨会话的唯一性由对象 OID（u128）承担，不由 fd 承担。
- fd 绑定的是「对象 generation 内的一个 entry 文件」（FILE 对象即其唯一 entry）；fd 表项内容：`{ oid, fs_id, generation, entry 相对路径, Arc<dyn AsyncFile>, 游标 u64, open flags }`。
- **只读 fd** 锚定打开时对本事务可见的 generation：该 generation 不可变，fd 存活期内内容不变（快照一致读，见 §3.7）。
- **写 fd** 仅允许在 PENDING（构建期）对象上打开，写的是本事务**私有的新 generation**：其他事务不可见；`fs-close` 时 fsync 并把 length 记入 `_fs_object`；事务提交时 pointer swap 生效，回滚则丢弃该 generation。已提交 generation 的 entry 一律只读。
- 游标语义与 POSIX 一致，作用于 fd 绑定的那份 entry 文件：
  - `fs-read` / `fs-write` 从游标处读写并推进游标；
  - `fs-pread` / `fs-pwrite` 在指定偏移读写，不动游标，直接映射 `AsyncFile::read_exact_at` / `write_all_at`；
  - `fs-lseek` 仅修改 fd 表项中的游标（host 侧纯内存操作，不产生 IO），无需进入 `AsyncFs` trait。
- 没有原地 append：对已提交 generation 的「追加」等价于写出新 generation（或为频繁部分写准备的 segment 扩展，见 §3.7）。
- 未使用的 fd 数值可复用；对已关闭 fd 的操作报 `BadFileDescriptor`（errno 9）。

## 3. FS 列类型

### 3.1 per-tuple fs object 的关系模型

**FS 列** = 列类型直接声明为一个已注册的 fs object 类型；每行的列值对应该类型容器内的一个 fs object。各 tuple 的 fs object 之间可能存在四种关系模型：

- **M1 行私有独立对象**：每个 tuple 对应一个独立对象，对象名由系统生成（OID），严格 1 行 1 对象，行间对象无任何关系。
- **M2 共享命名空间下的兄弟对象**：各行的对象是同前缀下的兄弟文件，名字由行键（主键/唯一列）派生，行间唯一的纽带是命名约定。
- **M3 行私有子树**：每行对应一个 `{row_key}/` 子目录，行间子树互不重叠，整体构成对容器命名空间按行的划分（一行多文件场景）。
- **M4 多行共享对象**：多行指向同一对象（同路径或内容寻址哈希），多对一关系，需要引用计数与共享 GC。

其中 DIRECTORY kind 的 fs object 类型对应**受控的 M3**：行私有子树，但子树不可变、整树替换（见 §3.3）。

### 3.2 业界参考设计

| 设计 | 列值物理存储 | 对象标识 | 生命周期归属 | 事务性 | 行间关系（模型） |
|---|---|---|---|---|---|
| Oracle BFILE | locator（DIRECTORY 对象 + 文件名） | 目录 + 路径字符串 | 外部文件系统，DB 不管内容 | 无（只读） | 无约束，可撞名/共享（M2） |
| SQL Server FILESTREAM | varbinary(max) + ROWGUIDCOL | 系统 GUID 派生文件路径 | DB 管理，tombstone + GC | 有（走事务日志） | 严格 1 行 1 文件（M1） |
| PostgreSQL Large Object | oid 列 | 全局 OID | 对象独立存在，vacuumlo 清孤儿 | 有 | 可多行共享 OID（M1/M4） |
| SQL/MED DATALINK（SQL:1999 标准，DB2 实现） | URL + LINK CONTROL 选项 | URL | 可选 FILE LINK CONTROL：DB 托管完整性；ON UNLINK RESTORE/DELETE | 可选 RECOVERY | 链接语义（M2/M4） |
| MongoDB GridFS | fs.files + fs.chunks 两个集合 | 文档 _id | 应用/驱动管理 | 集合级 | 1 文件 = N 块文档，文件间独立（M1） |

要点提炼：

- 业界只有两种基本语义——**引用语义**（DB 存 locator，内容生命周期在外部：BFILE、DATALINK）与**拥有语义**（DB 管理对象生死，对象与所属行同生共死：FILESTREAM、Large Object、GridFS）。
- 文件内容是否纳入事务是第二个正交维度，仅 FILESTREAM 与 Large Object 做到（前者走事务日志，后者内容本身就存在系统表里）。

### 3.3 mududb 的关系模型决策（已定）

- **FILE 类型 = M1**：每个 tuple 对应一个独立 fs object，系统生成 OID 作对象名，严格 1 行 1 对象；行间对象完全独立，无任何命名或结构上的关系。
- **DIRECTORY 类型 = 受控 M3**：一行多文件场景由「单对象内不可变目录树」承载——子树归属单个对象，整树是一个 generation，替换粒度为整树。原先拒绝 M3 的理由（目录层级管理）被不可变性消解：已提交的树永不修改，不存在 entry 级的并发与回收问题。
- **不采用 M2 / M4**：行键派生命名会把行键的生命周期耦合进文件布局；共享对象需要引用计数与共享 GC——与「对象生死只跟随所属行」的语义相悖。
- FS 列是**逻辑域**而非新物理类型（`mudu_type` 的 `TypeFamily` 是封闭枚举、函数表驱动，新增物理类型代价大）：物理存储复用 `U128`（对象 OID），列类型声明为 fs object 类型名，catalog 记录列 → fs object 类型绑定；`DROP TYPE` 时校验是否仍被列引用。

### 3.4 FILESTREAM 式实现（M1 的落地）

对应 SQL Server FILESTREAM 的能力，在 mududb 的落地分四部分：

1. **列声明**：

   ```sql
   CREATE TABLE product (
       id    U64 PRIMARY KEY,
       photo photo_fs
   );
   ```

   列物理存储为 `U128` 对象 OID（对应 FILESTREAM 中 ROWGUIDCOL 的 GUID 角色），catalog 记录列 → fs object 类型绑定，不新增 `TypeFamily`。

2. **对象命名与布局**：每行一个对象，内容路径 = `{oidhex}.{generation}[.{entry}]`（oidhex 为 u128 的 32 位小写 hex；FILE 对象无 entry 后缀，见 §2.3）——与 FILESTREAM 的 GUID 派生路径同理，且行键与文件名彻底解耦（改主键、导数据都不影响文件布局）。

3. **对象访问语法**：guest 经 `fs-open(oid, path, flags)` 打开 fs object（或其 entry），返回 fd；fd 上的 `fs-read` / `fs-write` / `fs-pread` / `fs-pwrite` / `fs-lseek` / `fs-fsync` / `fs-close` 完全遵循 §2.4 的 POSIX IO 语义：
   - oid + 对象内相对路径是 guest 唯一的寻址形式（§2.3）；OID 的唯一来源是 `SELECT` FS 列（含本事务内未提交的行，MVCC 可见性与普通列一致）；host 经 `_fs_object` 定位对象；
   - **OID 即能力**：持有 OID 即可访问该对象；
   - SQL 通道：另提供标量函数 `fs_read(oid) -> Binary` 读取小对象，避免为小内容走 fd 流程。

4. **事务与生命周期**：由 `_fs_object` 系统表的版本化行承载（机制详述见 §3.7，此处给出状态流转）：
   - `INSERT` / `UPDATE` 绑定 FS 列：kernel 在事务内插入 `(oid, fs_id, generation = 0, state = PENDING)` 的 `_fs_object` 行（对象创建只能经由 FS 列 DML——与 M1 的「对象随行生灭」一致）；
   - 写 fd 关闭：私有 generation 文件 fsync，length 记入该行；
   - 事务提交：行版本随 WAL 转正（pointer swap 生效）；
   - 事务回滚：PENDING 行版本不可见，私有 generation 文件由 GC 清理；
   - 删行 / 换绑 OID：旧对象行版本随事务不可见，越过 oldest snapshot horizon 后由 GC 删除该 generation；
   - 崩溃恢复：WAL 重放 `_fs_object`；扫描类型存储根，无任何可见行引用的 generation → 清理；
   - 与最初估计的差距已被 §3.7 的机制消除：内容不是「最后写赢」，而是 generation 级 MVCC——读者获得快照一致的内容；与 SQL Server 内容级日志的差异只剩版本粒度（generation 级而非字节级）。

### 3.5 FILESTREAM 式的优势与代价

优势：

- 大二进制不进入 page / record 存储，存储引擎页不膨胀；WAL 只记 `_fs_object` 元数据（每次操作几十字节），体积小；
- 1:1 行拥有语义，无共享计数，GC 规则简单确定（对比 PostgreSQL Large Object 的孤儿问题需要 `vacuumlo` 外部清理）；
- 对象内容是 DB 数据目录内的普通文件（append-only 的 generation 文件）：备份、迁移、离线处理可用 rsync 等普通文件工具；单对象大小不受行大小限制（对应 FILESTREAM 突破 varbinary 2GB 上限的卖点）；
- 流式通道性能：`pread` / `pwrite` 定位 IO 直接落宿主文件，绕过记录层序列化，io_uring provider 下有 per-worker ring 亲和；
- 与内核机制完全自洽：只增加「OID 命名 + `_fs_object` 元数据事务 + generation GC」，复用普通表的 CC / WAL 与既有 fd 模型；
- OID 命名使行键与文件名解耦。

代价：

- 小对象（小于页大小量级）不如内联 `Binary` 列高效（多次系统调用 + 文件系统开销）——小对象仍建议用 `Binary` 列；
- 写是 generation 级整体替换（见 §3.7），频繁部分写的对象需要 segment 扩展；
- 需要 `_fs_object` 系统表、GC worker、启动恢复扫描三套基础设施。

另一条路线——把内容存入 DB 内部 page 布局——的完整评估见 §3.6。

### 3.6 存储位置评估：内容放入 DB 内部 page 布局

一个自然的追问是：既然 mududb 自己就是带 page 存储引擎的数据库，为什么不把全部 fs object 的内容直接存进 DB 内部的 page 布局？本节给出可用技术、成熟产品参照与问题清单。

**方案定义**：fs object 内容不存宿主文件，而是作为 chunk 存进 DB 自身的 page 存储——逻辑上是一个 `(oid, seq_no) -> chunk` 的内部关系（复用 `mudu_kernel/src/storage/page/` 的定长 page + slot 布局），列值仍存 OID。内容因此自动获得 WAL、缓冲池与事务语义。

**可用技术与成熟产品**：

| 技术路线 | 代表产品 | 做法 |
|---|---|---|
| 溢出页链 | InnoDB | `ROW_FORMAT=DYNAMIC` 的大字段 off-page 存储，记录内仅留 20 字节指针 |
| 溢出页链 + 增量 IO | SQLite | b-tree cell 放不下的部分进 overflow page 链，另提供增量 blob IO API 支持定位读写 |
| 分块行表 | PostgreSQL TOAST | 大字段值压缩后切成约 2KB chunk 存入旁表，PLAIN/EXTENDED/EXTERNAL/MAIN 四种存储策略 |
| 分块行表 | PostgreSQL Large Object | `pg_largeobject` 按 `(loid, pageno)` 存约 2KB 的页，支持 `lo_lseek` 定位 |
| 分块文档 | MongoDB GridFS | 文件切成固定大小（约 255KB）chunk 文档，存入普通集合 |
| LOB 段 + 块索引 | Oracle SecureFiles | CHUNK 取块大小的倍数，LOB index 把逻辑偏移映射到物理块，行内阈值内联，CACHE/NOCACHE 控制缓存 |
| LOB 分配单元 | SQL Server | LOB_DATA 分配单元 + IAM 页链管理大对象页 |
| KV 分离（反向实践） | WiscKey 论文、RocksDB BlobDB、TiKV Titan、Badger | 把大 value 移出主存储结构（LSM 外追加写 value log + 指针），专为降低写放大 |

最后一行值得注意：KV 分离是近年存储界的**反向**实践——把大 value 从主结构里搬出去。它与 FILESTREAM 的外移动机一致，说明「大对象不入 page」是工业界的收敛方向。

**这样做的问题**：

1. **WAL 放大**：内容既进日志又进数据页，同一字节写两遍；大对象使日志体积与 checkpoint 压力激增——这正是当年 SQL Server 把 FILESTREAM 移出页存储的首要动机。
2. **缓冲池污染与双重缓冲**：大文件顺序读会把热的关系页挤出 buffer pool；同时 DB buffer 与 OS page cache 双份缓存浪费内存（除非全链路 direct IO，而 page 布局恰恰是为 buffer pool 设计的）。
3. **部分写退化为 read-modify-write**：`fs-pwrite` 落到非 chunk 对齐偏移时，必须先读出整个 chunk 再改写写回——POSIX 定位写语义在 page 布局下变成写放大。
4. **访问路径变长、零拷贝失效**：每次 `fs-pread` 要经 chunk 索引（B-tree / LOB index）定位若干 chunk 再逐页装配，无法 sendfile / mmap；既定设计里 io_uring 定位 IO 直落宿主文件的优势全部丧失，POSIX fd 语义还需要一层厚实的状态机在记录层上模拟。
5. **空间碎片与回收复杂**：删除对象留下空洞，需要空闲页管理 / vacuum / reorg 配合回收（PG 靠 vacuum，InnoDB 靠 purge 与表重建），在线收缩困难。
6. **备份粒度丧失**：对象不再是普通文件，不能 rsync / 文件级快照，只能逻辑备份，体积与时间膨胀；PITR 必须带着全部 blob 内容。
7. **并发粒度变粗**：同一对象不同 offset 的并发写可能撞在同一个 chunk 的行锁 / 页闩上；宿主文件的无锁定位 IO 没有这个问题。
8. **存储引擎侵入面大**（mududb 具体情况）：需要新增 chunk 关系类型、页格式变体、WAL 记录类型，牵动 checkpoint、崩溃恢复与 fuzz 确定性测试。相比之下，「外部宿主文件 + 元数据事务 + GC」侵入的是引擎外围而非核心。

**收益（如实列出）**：内容级 ACID（对象内容与行同事务提交/回滚）；单一备份制品；无孤儿问题、不需要 journal/GC；权限与加密随库统一；复制与 HA 自动覆盖文件内容。

**结论与建议**：维持既定的 M1 + 外部宿主文件方案为默认。page 布局适合的是**小对象**——后续可以做阈值式分层：小对象内联 `Binary` 列或 TOAST 式 chunk 表，大对象走外部宿主文件，这与 PG / InnoDB 的 inline 阈值策略一致；业界大对象趋势（FILESTREAM 外移、BlobDB / Titan KV 分离）也支持这一分层。

### 3.7 与普通表共享 CC 和日志机制（参考 PostgreSQL）

一个更进一步的问题是：能否让 fs object 与普通表**共享同一种并发控制（CC）和日志（WAL）机制**——像 PostgreSQL 的大对象那样——同时避开 §3.6 的全部问题？答案是可以，关键在于认清 §3.6 的问题不来自「共享 CC / WAL」，而来自「**内容**进 page」。

**PostgreSQL 的对照**：PG 的 large object 把内容直接作为 `pg_largeobject` 的行，从而获得内容级 MVCC 与 WAL——事务语义完整，但也正是因此吞下 WAL 放大、缓冲池污染、碎片回收等全部代价。要保留的是 PG 的「同一套 CC / 可见性框架」，要换掉的是「内容入行」。

**mududb 的机制设计**——元数据与内容分离：

- **共享 CC / WAL 的载体是元数据**：系统表 `_fs_object`（`oid, fs_id, generation, length, state, ...`）是一张普通系统表。它的可见性（MVCC）、行锁与写写冲突检测、隔离级别、DDL/DML 与普通表走**完全相同**的 CC；对它的更新随事务进同一个 WAL。这就是「fs 与普通表共享 CC 和日志机制」的准确含义。
- **内容是不可变 generation 文件**：按 §2.3 的布局存于类型存储根之下。写对象 = 写一个**新** generation（FILE 为单文件，DIRECTORY 为整棵树），fsync 之后在同一事务内做元数据 pointer swap；已提交的 generation 永不修改。崩溃安全由不可变性保证，因此内容不需要进 WAL。
- **WAL 只记元数据**：generation 的提交 / 弃用只是几十字节的操作记录，字节级内容不进 WAL。
- **提交协议**：新 generation 写完并 fsync → 同事务更新 `_fs_object` 行（pointer swap）→ WAL commit。崩溃恢复时 WAL 重放元数据；未被任何可见元数据版本引用的 generation 即为垃圾。
- **快照一致读**：读者按 MVCC 可见的元数据行拿到 generation 号，对该 generation 下的 entry 文件直接 `pread`——不可变文件保证无撕裂读、无需加锁，io_uring / 零拷贝优势完整保留。
- **GC 与 vacuum 同一思想**：以 oldest snapshot 为 horizon（类比 PG vacuum 的回收界），不再被任何活跃快照可见的老 generation 延迟回收。对象从 PENDING 到 COMMITTED 再到 tombstone 的状态流转，由 `_fs_object` 行的版本与 state 字段承载。
- **并发语义统一**：两个事务同时写同一对象 = 同时更新同一 `_fs_object` 行，走普通行锁 / 冲突检测，行为与普通表一致；各自的新 generation 互不干扰。

**如何逐条避开 §3.6 的问题**：

| §3.6 问题 | 本机制下的结果 |
|---|---|
| WAL 放大 | WAL 只记元数据（每次操作几十字节） |
| 缓冲池污染 / 双重缓冲 | 内容不进 buffer pool，只有元数据行进 |
| 部分写 read-modify-write | 不可变 generation 永远整写，无原地改 |
| 访问路径长、零拷贝失效 | 读者直读 generation 文件，pread / io_uring 保留 |
| 空间碎片与回收 | generation 整体删除（按前缀规则），空间归还宿主文件系统 |
| 备份粒度丧失 | generation 文件 append-only，仍可 rsync；元数据随库备份 |
| 并发粒度粗 | 内容读无锁；写冲突集中在单行元数据锁，与普通表同粒度 |
| 存储引擎侵入 | 只加一张系统表与少量 WAL 记录类型，不动 page 格式 |

**代价与边界（如实）**：

- 写语义是 **generation 级整体替换**：改 1 字节也产生一个新 generation（DIRECTORY 对象则整树替换）。适合「一次写、多次读」的对象（图片、附件、导出文件等典型场景）；频繁部分写的对象给出可选扩展——对象内部再分不可变 segment + manifest，manifest 的更新同样走 `_fs_object` 元数据事务，把替换粒度降下来。
- 与 PG lo 的差异：PG 做到**字节级**内容 MVCC，本设计是 **generation 级** MVCC——可见性框架相同，内容版本粒度不同。
- fd 语义映射：以写方式 `fs-open` 一个对象（或其 entry）时，打开的是本事务**私有的新 generation**（其他事务不可见）；`fs-close` 落定内容，事务提交时才 pointer swap；事务回滚则丢弃该 generation。只读打开的 fd 锚定打开时可见的 generation，内容在 fd 存活期内不变。

**参照的成熟实践**：DB2 DATALINKS（SQL 标准 DATALINK 类型，`FILE LINK CONTROL` + `RECOVERY YES`：外部文件管理器与数据库事务管理器以两阶段提交协调，外部文件操作纳入数据库恢复）；SQL Server FILESTREAM（事务日志携带文件操作记录，日志 checkpoint 驱动外部文件 GC）；Iceberg / Delta Lake（不可变数据文件 + catalog 原子指针交换实现快照隔离）；FoundationDB blob granules（版本化元数据留在事务内核，bulk 内容放到外部 blob 存储）。

### 3.8 与 partition 的关系

mududb 的 partition 机制（见 `doc/cn/partition.cn.md`）把表数据按 RANGE rule 分布到不同 worker，`PhysicalRelationId { table_id, partition_id }` 贯穿事务暂存、写冲突检查、WAL 记录与 replay，且当前**没有跨 worker 原子提交（2PC）**。fs object 与该机制的关系遵循一条对齐原则：

**对象随行**——fs object 的元数据与内容都与所属行同 partition、同 worker、同事务、同 WAL。

具体展开：

1. **`_fs_object` 的物理分布**：不按 oid 独立分区——那会让 DML 钩子（§5.4）变成跨 worker 写，撞上无 2PC 的现状。它与所属用户行的 `(table_id, partition_id)` 对齐：钩子在同 partition 的 `_fs_object` 物理 relation 内同事务写入，直接复用 `PhysicalRelationId` 体系（事务暂存 / 冲突检查 / WAL 记录 / replay 本已携带 partition 身份）。非分区表退化为 partition 0。
2. **OID 位段（MVP 偏差）**：实现的 fs 对象 OID 位段为 `{8-bit tag = 0xF5, 120-bit random}`（`gen_fs_oid`，`mudu_kernel/src/meta/fs_object.rs`）——partition 身份内嵌暂未实现，列为后续项；MVP 下 resolve 仅在本 worker 查 `_fs_object`，跨 worker 访问由客户端 port sharding（item 3 的既定立场）。random 位段保持不可猜测性（§7 的 OID 即能力不受影响）。
3. **内容文件**：写在行所在 worker 的存储根（§2.3 的布局是该 worker 的本地路径）；读取由客户端按行 partition 复用 port sharding（`partition-route` + `server-topology`）直连目标 worker（MVP 下 OID 不内嵌路由身份，见 item 2）。
4. **类型 catalog 不分区**：name → fs_id 的映射是全局元数据，与 partition rule catalog 同款（固定 table id、全局可见）。
5. **GC / 恢复 / 复制全部 per worker 对齐**：各 worker 只扫描自己的 storage root（§5.6）；复制通道（见 `doc/cn/todo/filesystem_todo.md` W11）按 partition 对齐拉取 generation 文件。
6. **限制（如实）**：placement 变更（rebalance）初版不自动迁移 generation 文件（由管理员迁移，或 GC 清理后重建）；行 UPDATE 跨 partition 迁移时对象随行迁移 = 「新 partition PENDING + 旧 partition tombstone」两步，其跨 worker 原子性受限于系统当前无分布式 2PC——与 partition 文档声明的现状一致，本设计不新增承诺；FS 列的 INSERT/UPDATE/DELETE 仅支持行 partition 属本 worker——经 partition RPC 路由到远程 worker 的行，整个 DML 报 `NotImplemented`（不静默丢对象）。

## 4. 系统调用接口层

### 4.1 WIT 边界

遵循现有 KV 系统调用模式：在 `mudu_runtime/wit/api.wit`（同步 world `api`）与 `mudu_runtime/wit/async-api.wit`（异步 world `async-api`）的 `interface system` 中，**每个操作新增一个函数**，参数与返回值均为 `list<u8>` 字节管道，按 WIT 函数名 dispatch（本项目没有数字系统调用号表）。

新增函数（同步与异步 world 保持一致）：

| 函数 | 语义 | 主要参数 | 返回 |
|---|---|---|---|
| `fs-open` | 打开对象或其 entry，返回 fd | `oid: u128`、`path: string`（对象内相对路径，FILE 对象为空）、`flags: u32`（O_RDONLY/O_WRONLY/O_RDWR） | `fd: u32` |
| `fs-close` | 关闭 fd（写 fd 落定 generation） | `fd` | — |
| `fs-read` | 从游标读 | `fd`、`len: u32` | `data`（短读表示 EOF） |
| `fs-write` | 从游标写（私有 generation） | `fd`、`data` | `n: u32` |
| `fs-pread` | 定位读 | `fd`、`offset: u64`、`len: u32` | `data` |
| `fs-pwrite` | 定位写（私有 generation） | `fd`、`offset: u64`、`data` | — |
| `fs-lseek` | 移动游标 | `fd`、`offset: i64`、`whence: u32`（SEEK_SET/CUR/END） | 新游标 `u64` |
| `fs-fstat` | fd 元数据 | `fd` | `uni-fs-stat` |
| `fs-stat` | 对象或 entry 元数据（不开 fd） | `oid: u128`、`path: string` | `uni-fs-stat` |
| `fs-fsync` | 刷盘 | `fd` | — |
| `fs-readdir` | 列 DIRECTORY 对象的目录（仅 DIRECTORY） | `oid: u128`、`path: string`（对象内目录路径） | `list<uni-fs-dirent>` |

说明：

- 所有系统调用参数帧都内嵌 `session_id`（OID，u128 大端），与现有 KV 调用一致（见 `mudu_binding/src/codec/handle_sys_session.rs`）。
- `open flags` 直接采用 libc `O_*` 位标志取值，与 `mudu_sys_contract` 的 `FileOptions::new(flags: i32, mode: u32)`（`mudu_sys_contract/src/contract/file_options.rs`）对齐，host 侧免转换；创建/截断/追加语义不由 flags 表达——对象创建走 FS 列的 `INSERT` / `UPDATE`（§3.4），内容替换走写 fd 的新 generation（§2.4）。
- `uni-fs-stat` 包含 `{ oid, generation, entry, length, state }`；`uni-fs-dirent` 包含 `{ name, is_dir, length }`。
- 单次 `fs-read` / `fs-write` 的数据量上限由 host 配置约束（初版建议 16 MiB），超限报 `InvalidArgument`。

### 4.2 payload 编解码

新增 `mudu_binding/src/codec/handle_sys_fs.rs`：

- 手写大端二进制帧，复用 `handle_sys_session.rs` 的读写原语（`write_u128` / `read_u128`、u32 长度前缀字节串等）；
- 错误一律走 `MERR` 魔法前缀 + `UniError` 封套（`serialize_error_result` / `decode_error_result`），与 KV 调用一致；
- 配套单测 `handle_sys_fs_test.rs`，仿 `handle_sys_session_test.rs`。

### 4.3 canonical schema

- `mudu_binding/wit/` 新增 `uni-fs-open-argv.wit`、`uni-fs-stat.wit`、`uni-fs-dirent.wit` 等类型定义；
- `mudu_binding/wit/uni-syscall.wit` 的 `interface universal` 中声明对应的类型化函数签名（如 `fs-open: func(argv: uni-fs-open-argv) -> result<u64, uni-error>`）；
- 该 schema 由 `mgen`（`mudu_binding/makefile.toml` 的 `generate` / `generate-csharp` 任务）生成 Rust / C# / AssemblyScript 类型，并为将来纳入 SyscallPayload v1 路由器（`doc/cn/todo/project-controlled-guest-host-abi.md` Phase 3）做好准备；WIT 集合变更后需同步刷新 `mudu_binding/wit/contract.md5.txt`。

### 4.4 host 处理链

与现有系统调用完全同构：

```text
wasi_context_component.rs   (bindgen! 生成的 Host / HostWithStore impl，sync + async 各一份)
  → kernel_function_p2.rs / kernel_function_p2_async.rs   (host_fs_* / async_host_fs_* 薄封装)
  → interface/kernel_sync.rs / kernel_async.rs   (fs_*_internal：decode → 取 WorkerLocalRef → fs 服务 → encode)
```

- `mudu_runtime/src/service/wasi_context_component.rs`：为两个 world 实现新增方法（bindgen 在构建期重新生成 trait，缺失实现会直接编译报错，可作为实现清单）。
- `mudu_runtime/src/interface/kernel_sync.rs` / `kernel_async.rs`：新增 `fs_open_internal`、`fs_read_internal` 等；错误路径统一 `serialize_error_result`。

### 4.5 错误模型

不引入新错误编号：`mudu::error::ErrorCode`（`mudu/src/error/ec.rs`）已是 POSIX errno 语义，直接复用：

| 场景 | ErrorCode |
|---|---|
| 未知对象 OID / entry 路径 / fd | `NotFound` (2) |
| inner/path 逃逸对象根、host 侧存储根约束触发、权限拒绝 | `PermissionDenied` (13) |
| 对 FILE 对象使用 inner/path、对 FILE 对象 `fs-readdir` | `NotADirectory` (20) |
| 以文件方式打开 DIRECTORY entry 中的目录 | `IsADirectory` (21) |
| inner/path 格式非法 | `InvalidFilename` (36) |
| 非法参数（flags、长度超限等） | `InvalidArgument` (50029) |
| 已关闭 fd | `BadFileDescriptor` (9) |

host 侧 `std::io::Error` → `ErrorCode` 的转换已有现成实现（`ErrorCode::from_raw_os_error`）。

## 5. host 端实现设计

### 5.1 模块结构

- `mudu_kernel/src/server/fs_service.rs`：`FsService`——寻址解析 + fd 表 + generation 读写，经 `WorkerLocal::fs_service()` 暴露（trait 默认方法，真实状态挂 `WorkerRuntime`，与 `x_contract` / `meta_mgr` 同款）；选用 `WorkerLocal` 作为挂载点的理由：现有 KV / 会话系统调用都经由它，且天然具备 worker 亲和性（io_uring per-worker ring）。
- `mudu_kernel/src/server/fs_fd_table.rs`：会话 fd 表。
- `mudu_kernel/src/server/fs_gc.rs`：恢复扫描 + GC loop（§5.6）。
- `mudu_runtime` 侧 WIT 处理链（§4.4、§5.5）只做 decode/encode，业务全部在 `FsService`。

数据流：

```text
syscall 帧 → kernel.rs fs_*_internal → WorkerLocalRef → FsService
           →（读 _fs_object）TxMgr
           →（内容 IO）AsyncFs / AsyncFile（注入的 AsyncIoProvider）
```

### 5.2 寻址解析（resolve）

- `resolve(oid) -> { fs_id, kind, generation, length, state }`：在本 worker 经当前会话的 `TxMgr` 读 `_fs_object`（MVP 不做 OID 内嵌身份路由——跨 worker 访问由客户端 port sharding，§3.8.3），可见性 = 当前事务快照（天然 MVCC，§5.4）；未知 oid 报 `NotFound`。
- 路径构造（§2.3）：`storage_root(fs_id) = {data_dir}/fs/{fs_id}`；FILE 内容 = `{root}/{oidhex}.{generation}`；DIRECTORY entry 内容 = `{root}/{oidhex}.{generation}.{entry 相对路径}`（`oidhex` 为 u128 的 32 位小写 hex，`generation` 为 u64 十进制；entry 含 `/` 时写入按需创建父目录）。
- 对象内规范化：逐分量检查——拒绝 `..` 越出对象根（`PermissionDenied`）、绝对路径分量、NUL（`InvalidFilename`）；非 DIRECTORY 对象不允许非空 path（`NotADirectory`）。

### 5.3 fd 表与打开语义

```rust
struct FdEntry {
    oid: OID, fs_id: u64, generation: u64,
    entry_rel: PathBuf,            // 相对 storage root
    file: Arc<dyn AsyncFile>,
    cursor: u64,
    write: bool,                   // O_WRONLY / O_RDWR
}
```

- 每会话一张 `scc::HashMap<u32, FdEntry>`；fd（`u32`）取最小空闲数值（可复用，POSIX 语义）；`close_async(session_id)` 整表回收——未关闭的写 fd 其私有 generation 随事务结束按回滚处理。
- `fs_open` **读模式**：锚定当前事务可见的已提交 generation，`AsyncFs::open` 只读。
- `fs_open` **写模式**：对象须为本事务创建的 PENDING 对象；按需 `create_dir_all` 创建 entry 的父目录（中间前缀形成真实宿主目录），`O_CREAT|O_WRONLY` 打开 entry；对已提交 generation 写打开报 `PermissionDenied`。
- `read` / `write` / `pread` / `pwrite` / `lseek` / `fstat` / `fsync` 直接映射 `AsyncFile` + fd 游标（§2.4）；`stat` / `readdir` = resolve + `metadata_len` / `read_dir`。
- 底层 IO 经注入的 `AsyncIoProvider` 获取 `Arc<dyn AsyncFs>`（`ServerRuntimeDeps::async_runtime`，不用全局 `default_sys_io_context()`，便于 `MockIoProvider` 测试注入）；**无需扩展 `mudu_sys_contract`**：`open` / `create_dir_all` / `read_dir` / `remove_file_if_exists` / `remove_dir_all` / `path_exists` / `metadata_len` 已覆盖，不可变 generation 永远整写新文件，不需要 truncate / rename。

### 5.4 事务钩子（`_fs_object` 写入）

- 触发点：`insert_key_value.rs` 的 insert 循环内、`update_key_value.rs`、`delete_key_value.rs`（先 `read_key` 取旧 OID），在 `x_contract` 调用成功后对 FS 绑定列执行。
- INSERT / UPDATE 新 OID：`tx_mgr.put_relation(fs_rel, oid_key, encode{ fs_id, kind, generation = 0, length = 0, state = PENDING })`。
- UPDATE 换绑 / DELETE 旧 OID：`tx_mgr.delete_relation(fs_rel, old_oid_key)`——MVCC 下旧版本对新快照不可见、老快照仍可见，**指针切换语义由 MVCC 免费获得**，无需显式 swap。
- 写 fd `fs-close`：fsync 后 `put_relation` 更新该行为 `{ length, state = SEALED }`（仍在同事务内）；回滚则 staged ops 整体丢弃，私有 generation 文件由 GC 清理。
- 崩溃一致性不变式：「entry fsync + length 落 `_fs_object`」先于「事务 commit 记录」——§3.7 提交协议在代码层的落点。
- 综上：fs object 的元数据与内容可见性都与普通表共享 CC / WAL（generation 级 MVCC）：未提交写入其他事务不可见，回滚即丢弃；已提交 generation 不可变；内容字节不进 WAL。

### 5.5 WIT 处理链（mudu_runtime）

- `kernel.rs` 的 `fs_open_internal` 等 11 个 handler = decode（`handle_sys_fs`）→ `require_worker_local` → `fs_service()` → encode；错误统一 `serialize_error_result`。
- sync / async 双 world 薄封装（§4.4），本层无业务逻辑。

### 5.6 GC 与恢复扫描（fs_gc.rs）

- **恢复扫描**（`recover_worker_log_*` 之后，tokio 与 io_uring 两个后端同位置）：各 worker 只扫描自己的 storage root——按前缀规则枚举（宿主条目名 `== "{oidhex}.{gen}"` 或以 `"{oidhex}.{gen}."` 开头，点号边界避免 gen 1 误配 gen 12），对照 `_fs_object` 可见行与 xl generation 描述记录（见 `doc/cn/todo/filesystem_todo.md` W11），删除无任何引用的 generation；扫描完成后才允许 GC loop 启动。
- **GC loop**（首个周期后台任务）：周期间隔取 oldest snapshot horizon，对「行版本已不可见且越过 horizon」的对象按前缀规则删除过期 generation 的全部宿主路径。两个后端的驱动方式不同：tokio 后端是 `spawn_local_task` + `stop_channel` 的常驻 GC loop（干净退出）；io_uring 后端无法睡眠，由 service loop 每轮检查间隔并 re-spawn 单轮 GC 任务（`submit_fs_gc_round_if_due`）。
- horizon 含副本协调：取所有副本 oldest snapshot 的最小值（同上，见 todo W11）。

### 5.7 并发与错误

- 并发写同一对象 = `_fs_object` 行 MVCC 写写冲突，行为与普通表一致；fd 表用 `scc` 免全局锁；写 fd 归属单一 session + 事务。
- fs_id 与 fd 都不是 `OID`，只在系统调用帧内部出现；fs object 的 OID 是 `OID`（u128），可出现在列值与 guest API 中。
- 错误映射：`std::io::Error` → `ErrorCode::from_raw_os_error`；规范化 / 寻址错误在 resolve 处显式构造（§4.5 表）。

## 6. guest 端 POSIX 兼容实现

### 6.1 POSIX 兼容子集与常量

- 访问模式位（libc 值）：`O_RDONLY = 0`、`O_WRONLY = 1`、`O_RDWR = 2`；`whence`：`SEEK_SET = 0`、`SEEK_CUR = 1`、`SEEK_END = 2`。
- **有意省略的 flags**（host 收到即报 `EINVAL`）：`O_CREAT`（对象创建走 FS 列 `INSERT`）、`O_TRUNC`（内容替换走新 generation）、`O_APPEND`（无原地 append，§2.4）、`O_EXCL`（对象身份由 DB 分配）。
- guest 函数 ↔ POSIX ↔ syscall 对照：

| guest 函数 | POSIX 对应 | syscall |
|---|---|---|
| `mudu_fs_open` | `open(path, flags)` | `fs-open` |
| `mudu_fs_close` | `close(fd)` | `fs-close` |
| `mudu_fs_read` | `read(fd, buf, n)` | `fs-read` |
| `mudu_fs_write` | `write(fd, buf, n)` | `fs-write` |
| `mudu_fs_pread` | `pread(fd, buf, n, off)` | `fs-pread` |
| `mudu_fs_pwrite` | `pwrite(fd, buf, n, off)` | `fs-pwrite` |
| `mudu_fs_lseek` | `lseek(fd, off, whence)` | `fs-lseek` |
| `mudu_fs_fstat` | `fstat(fd, &st)` | `fs-fstat` |
| `mudu_fs_stat` | `stat(path, &st)` | `fs-stat` |
| `mudu_fs_fsync` | `fsync(fd)` | `fs-fsync` |
| `mudu_fs_readdir` | `readdir(dir)` + `d_type` | `fs-readdir` |

### 6.2 逐函数 POSIX 语义

- `fs_read`：返回 0 表示 EOF；短读（< len）不必然 EOF（与 POSIX 一致）；对无读权限的 fd 报 `EBADF`。
- `fs_write`：允许短写；对无写权限的 fd 报 `EBADF`；只作用于本事务私有 generation。
- `fs_pread` / `fs_pwrite`：定位 IO，不动游标；offset 超 EOF 时读返回 0、写产生空洞（宿主 sparse 文件，仅在私有 generation 内允许）。
- `fs_lseek`：允许 seek 超 EOF（POSIX）；`SEEK_END` 基于 fd 表项的 length（写 fd 随写增长）；结果为负报 `EINVAL`。
- `fs_fsync`：写 fd 上刷私有 generation；读 fd 上报 `EINVAL`。
- `fs_close`：重复 close 报 `EBADF`；写 fd 的 close 落定 generation（§2.4）。
- `fs_fstat` / `fs_stat`：POSIX `stat` 子集 `{ size, is_dir }` + 扩展字段 `{ generation, state }`。
- `fs_readdir`：一次性返回 `{ name, is_dir, length }` 列表（对应 `readdir` + `d_type`）。

### 6.3 错误与 errno 映射

- host `ErrorCode` 已是 errno 语义，guest 可见 errno 与 Linux 数值一致：`ENOENT=2`、`EACCES=13`、`EBADF=9`、`ENOTDIR=20`、`EISDIR=21`、`ENAMETOOLONG=36`（§4.5 的 host 错误表一一对应）。
- **例外**：host `InvalidArgument=50029`（应用级码）在 guest 封装层统一映射为 `EINVAL=22` 暴露，保证 POSIX 兼容面干净；syscall 帧内仍传 host 原码。
- Rust 侧：函数返回 `RS<T>`（`MuduError` 携带 errno）；AssemblyScript 侧：纯函数返回 `Result<T>`（复用 `bindings/assemblyscript/assembly/result.ts`），errno 内嵌其中。

### 6.4 Rust 实现结构（sys_interface）

`sys_interface` 新增 `fs` 模块，`sync_api` / `async_api` 双入口。API **只用纯函数**（无结构体/句柄封装），与现有 `mudu_open` / `mudu_query` 同风格；首参统一为 `session_id: OID`（与 `mudu_get(session_id, key)` 等 KV 惯例一致）：

```rust
// 同步入口示例（异步入口同名加 .await）
pub fn mudu_fs_open(session_id: OID, oid: OID, path: &str, flags: u32) -> RS<u32>;    // fs-open -> fd
pub fn mudu_fs_close(session_id: OID, fd: u32) -> RS<()>;
pub fn mudu_fs_read(session_id: OID, fd: u32, len: u32) -> RS<Vec<u8>>;
pub fn mudu_fs_write(session_id: OID, fd: u32, data: &[u8]) -> RS<u32>;               // 作用于私有 generation
pub fn mudu_fs_pread(session_id: OID, fd: u32, offset: u64, len: u32) -> RS<Vec<u8>>;
pub fn mudu_fs_pwrite(session_id: OID, fd: u32, offset: u64, data: &[u8]) -> RS<()>;  // 同上
pub fn mudu_fs_lseek(session_id: OID, fd: u32, offset: i64, whence: u32) -> RS<u64>;
pub fn mudu_fs_fstat(session_id: OID, fd: u32) -> RS<FsStat>;
pub fn mudu_fs_stat(session_id: OID, oid: OID, path: &str) -> RS<FsStat>;
pub fn mudu_fs_fsync(session_id: OID, fd: u32) -> RS<()>;
pub fn mudu_fs_readdir(session_id: OID, oid: OID, path: &str) -> RS<Vec<FsDirEntry>>; // 仅 DIRECTORY
```

- 每个函数的内部结构：`handle_sys_fs` 编码参数 → `sys_interface/src/host.rs` 的 invoke 闭包 → `inner_component.rs` / `inner_component_async.rs` 的 WIT import → 解码结果；错误经 `decode_error_result` 还原为 `MuduError`。
- `FsStat` / `FsDirEntry` 是纯数据 struct（非句柄封装，与纯函数决策兼容）。
- **同步 / 异步双版本**（与 `doc/cn/syscall.cn.md` 的两套稳定入口一致：同一函数名、同一参数与返回，仅执行模型不同）：
  - `sys_interface::sync_api::mudu_fs_*`：同步阻塞语义，invoke 闭包调 `mududb:api/system` 的同步 import（`inner_component.rs`）；面向手写原生过程代码；
  - `sys_interface::async_api::mudu_fs_*`：`async fn` + `.await`，invoke 闭包调 `mududb:async-api/system` 的异步 import（`inner_component_async.rs`，component-model async ABI）；面向 mtp 生成的异步过程代码；
  - 两个版本完全共享：codec（`handle_sys_fs`）、数据类型（`FsStat` / `FsDirEntry`）、错误与 errno 映射（§6.3）；唯一差异是 invoke 闭包——`host.rs` 中 `invoke_host_fs_*` 与异步孪生 `invoke_host_fs_*_async` 成对实现（同现有 `invoke_host_get` 等函数的模式）。
- **transpiler（mtp）支持**：mtp 把 `/**mudu-proc**/` 过程中的同步调用编译为异步的机制对 fs 家族自动生效——`sys_interface::sync_api` → `sys_interface::async_api` 模块路径替换（`mudu_transpiler/src/rust/parse_context.rs`）+ `async` / `.await` 按位置拼接并沿调用图传播（`tran_to_async`）；唯一需要做的是把 11 个 `mudu_fs_*` 函数名注册进 mtp 的 `sys_call` 集合（与 `mudu_query` 等现有名字同处）。约束与现有一致：调用须经 `use sys_interface::sync_api::{mudu_fs_open, ...}` 导入的裸名（按 callee 裸标识符匹配）。AssemblyScript 路径不经 mtp（无 sync→async 概念，异步在绑定层处理，见 §6.5）。
- `standalone-adapter` 路径同样提供同步 / 异步双版本，保证原生（非 WASM）测试可用：本地文件模拟实现于 `mudu_adapter/src/local_fs.rs`，sqlite/postgres/mysql 驱动共享——根目录 `{db_path}.fs/`，单 generation=1，无 MVCC/DDL/catalog，OID 由调用方给定，写打开即创建；mudud 驱动返回 `NotImplemented`（客户端协议扩展为后续工作项）。

### 6.5 AssemblyScript 实现结构

- 调用链：AS guest → `mududb:component-shim` → `bindings/rs-shim` → `sys_interface` → host。新增 fs 函数需要在 `bindings/component-shim/wit/api.wit`、`bindings/rs-shim/wit/api.wit`、`bindings/assemblyscript/wit/api.wit` 三份 WIT 中同步声明，rs-shim 薄封装透传。
- `bindings/assemblyscript/assembly/fs.ts` 手写 canonical ABI（与 `wit.ts` 同风格，已实现）：每个 fs 函数一个 `@external("mududb:component-shim/system", "fs-open")` 导入声明；`session` 与 `oid` 两个 u128 均以 `{ hi: u64, lo: u64 }` 传递（`rawQuery(idHi, idLo, ...)` 惯例）；string/bytes 手工 lowering；result 帧先解 errno tag 再解 payload。
- 上层 `fs.ts` 纯函数集合：`fsOpen(sessionHi, sessionLo, oidHi, oidLo, path, flags): Result<u32>`、`fsRead(sessionHi, sessionLo, fd, len): Result<ArrayBuffer>`、`fsWrite(...): Result<u32>`、`fsReaddir(...): Result<FsDirEntry[]>` 等，与 Rust 侧一一对应（首参为 session，与 Rust `mudu_fs_*` 一致）。
- 同步 / 异步双版本：Phase 2 先交付**同步版**（`@external` 同步 import，guest 纯函数直接返回 `Result<T>`）；**异步版**走 component-model async ABI（`async func` import），其实现依赖 AS 手写 ABI 对 async 的支持与 rs-shim 的 async world 组合，作为 Phase 2 的后续项——guest 侧函数签名保持不变。
- `mtp` 转译器无需改动——fs API 是普通 guest 库，不参与过程导出代码生成。

### 6.6 C#（Phase 2，已实现）

- `mgen message -l csharp` 从 `uni-fs-*.wit` 生成三个 MessagePack 模型（`UniFsOpenArgv`/`UniFsStat`/`UniFsDirent`，手工采用进 `mudu_api/csharp/uni/`）；`UniError` 顺带修正为与 Rust 对齐的 5 字段。
- `mudu_api/csharp` 的 `MuduSysCallApi` 已全量迁移到 SyscallPayload v1（MSSP 头 + MessagePack body，20 个 message_kind），并新增 11 个 `SysFs*` 方法；`mock/MockFsEmulation.cs` 提供进程内 fs 模拟（单 generation、fd 表、errno 语义子集），`Mudu.cs` 门面暴露 `MuduFileSystem`（errno → `IOException` 子类映射）；`demo/` 演示对象写入/读取全流程。

### 6.7 分期

- **Phase 1**（已完成）：DDL 类型 + fs object 类型 catalog + `_fs_object` + generation 存储 + GC / 恢复 + host 处理链 + codec + FS 列绑定 + Rust guest API（sync_api / async_api 双版本）+ mtp 注册 fs syscall 家族 + 测试。
- **Phase 2**（已完成）：AssemblyScript（同步版；异步版随 AS async ABI 支持跟进）与 C# 绑定。
- **Phase 3**（已完成）：11 个 fs 函数与既有 9 个家族一并纳入 SyscallPayload v1 路由器（`mudu_binding::codec::syscall_payload`，`message_kind` 1–20，MERR 退役，`testing/mpk/wallet.mpk` 已重建）。遗留：`fetch` 不在 uni-syscall 的 20 个 kind 内，暂未路由。

## 7. 安全与权限

- **OID 即能力**：guest 侧没有宿主路径输入，主要攻击面收敛为 OID 的伪造 / 穷举——u128 随机 OID 保证不可猜测；需要更细粒度授权时，可在 `_fs_object` 上追加 owner / acl 字段（后续扩展）。
- **对象内沙箱**：DIRECTORY 对象的 inner/path 经规范化后严格限定在对象根内（§2.3），拒绝 `..` 逃逸与绝对路径。
- **存储根约束**：全部内容文件路径由 host 按 §2.3 布局生成并严格限定在 DB 存储根内。
- **DDL 权限**：已实现——`CREATE TYPE FILESYSTEM` / `DROP TYPE` 仅 admin 会话可执行：client 协议会话（`session_create` / `open_session_with_config`）为 admin，procedure 内部会话非 admin；`MuduConnCore::execute_inner` 校验，非 admin 报 `PermissionDenied`。
- **资源限额**（后续可选）：每会话 fd 上限、单类型对象数上限、单次读写字节上限、每类型总容量配额。

## 8. 测试计划

- **codec 单测**：`handle_sys_fs_test.rs`，覆盖每个系统调用的参数/结果/错误帧往返。
- **catalog 与 DDL 单测**：注册两种 kind、重名（`AlreadyExists`）、`DROP TYPE` 引用校验、未知 OID。
- **kernel 集成测试**：复用 `MockIoProvider` 模式——INSERT 绑定 FS 列 → 写 fd → 提交后其他事务可读；回滚后对象不可见且 generation 被 GC；两事务写同一对象走行冲突；fd 生命周期（关闭后访问报 `BadFileDescriptor`）；session 关闭回收 fd。
- **DIRECTORY 用例**：构建期写多个 entry → 提交后 `fs-readdir` 与只读打开；`..` 逃逸与绝对路径负例（`PermissionDenied`）；对 FILE 对象 `fs-readdir` 报 `NotADirectory`。
- **GC 与恢复**：删行 → 越过 snapshot horizon 后该 generation 被回收；模拟崩溃 → 启动恢复扫描清理无引用 generation。
- **安全负例**：伪造 / 穷举 OID 访问。
- **端到端**：example 级 WASM 过程完成「INSERT 行 → SELECT 得 OID → open 写 → fsync → close → 提交 → 重开读 → stat」全流程。

## 9. 涉及文件清单

新增：

- `doc/cn/filesystem.cn.md`（本文档）
- `mudu_binding/src/codec/handle_sys_fs.rs`（+ `handle_sys_fs_test.rs`）
- `mudu_binding/wit/uni-fs-*.wit`
- `mudu_kernel/src/command/create_fs_type.rs`、`drop_fs_type.rs`
- `mudu_kernel/src/meta/` 内 fs object 类型 catalog（存储根由 `ServerCfg::data_dir` 派生）
- `_fs_object` 系统表定义与 GC worker（`mudu_kernel/src/server/` 内新模块），启动恢复扫描挂 `server_launch.rs`
- kernel fs 服务与 fd 表（`mudu_kernel/src/server/` 内新模块）
- `sys_interface/src/fs.rs` 及 `api_impl` 两侧的接线
- `bindings/assemblyscript/assembly/fs.ts`（Phase 2）

修改：

- `mudu_runtime/wit/api.wit`、`mudu_runtime/wit/async-api.wit`
- `sys_interface/wit/sync/api.wit`、`sys_interface/wit/async/async-api.wit`（guest 侧副本，并顺带修复现有 sync 副本缺 `delete` 的漂移）
- `mudu_binding/wit/uni-syscall.wit`、`mudu_binding/wit/contract.md5.txt`
- `mudu_runtime/src/service/wasi_context_component.rs`
- `mudu_runtime/src/service/kernel_function_p2.rs`、`kernel_function_p2_async.rs`
- `mudu_runtime/src/interface/kernel_sync.rs`、`kernel_async.rs`
- `mudu_kernel/src/server/worker_local.rs`
- `mudu_kernel/src/command/{insert,update,delete}_key_value.rs`（FS 列 DML 钩子：对象绑定 / 解绑随行进 `_fs_object` 事务）
- `mudu_adapter/src/local_fs.rs`（standalone 路径的本地文件模拟，sqlite/postgres/mysql 驱动共享；mudud 驱动返回 `NotImplemented`）
- `bindings/{component-shim,rs-shim,assemblyscript}/wit/api.wit`（Phase 2）
