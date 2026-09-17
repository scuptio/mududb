# Guest→Host 系统调用负载契约 v1

## 适用范围

本文档规定 MuduDB guest→host 系统调用所使用的、由项目自有的二进制线格式。

该线格式保留 Phase 1 引入的 16 字节头部，并用 **MessagePack** 编码消息体。
MessagePack 布局是**项目可控的**：它派生自
[`mudu_binding/wit/`](../../../mudu_binding/wit/) 中的权威 WIT 定义，由 `mgen`
生成到 Rust 的自定义 `serde` 实现以及 C# 的自定义 MessagePack formatter。
实现仅把 `rmp_serde`（或 C#/AssemblyScript/Python 端的等效 MessagePack 运行时）作为底层编解码器；
**不**使用默认的 `rmp_serde` derive 行为，因为后者会把 Rust 枚举变体序列化成 map 或字符串，
无法在不同 guest 语言之间保持稳定。

权威的 syscall 边界是 [`uni-syscall.wit`](../../../mudu_binding/wit/uni-syscall.wit)
中声明的 WIT 函数接口。本文所述的头部与 MessagePack 消息体是运行时内部对那些 WIT 函数调用的
序列化；guest 代码应直接调用 WIT 函数，由生成的绑定处理线格式。

逻辑数据模型仍是现有的 `mudu_binding::universal` 类型族；本契约定义这些类型的**线编码**。

**状态：已实现并稳定。** 全部 23 种消息类型由宿主路由器
[`mudu_binding/src/codec/syscall_payload/`](../../../crates/common/mudu_binding/src/codec/syscall_payload/)
编解码，并在 `mudu_runtime`（`kernel_sync` / `kernel_async`）中分发。guest 端实现覆盖
workspace Rust guest、独立 Rust SDK、C# SDK、AssemblyScript（直连 byte-pipe guest，
语言内编解码器）
以及 Python（工具/测试验证绑定）；
见下文 [Guest 语言支持](#guest-语言支持)以及
[Guest/Host ABI 集成指南](../abi/guest_host_abi.cn.md)。各独立编解码器的字节级互操作性
通过单一宿主生成的 golden 语料验证（见[互操作验证](#互操作验证)）。

已废弃的 `UniDatTypeId` / `UniMuTypeFamily` 枚举不属于本契约，且已从绑定层删除。
当类型族值需要跨边界传递时，由 `UniDataType` / `UniScalar` 承载。

## 注册表

| 属性 | 值 |
|------|----|
| 格式族 | `FormatKind::SyscallPayload` |
| 魔数 | `0x4D53_5350`（ASCII `MSSP`） |
| 当前版本 | `1` |
| 支持范围 | `[1, 1]` |
| 注册表 | [`mudu/src/compat/mod.rs`](../../../mudu/src/compat/mod.rs) |

运行时只解码当前版本。遇到不支持的版本时以 `ErrorCode::UnsupportedFormatVersion`
快速失败。格式版本变更时，MPK 包会针对匹配的运行时重新构建；运行时**不**保留旧版解码器。

同一版本同时钉入每个 MPK 包：`package.manifest.json` 携带 `syscall_abi_version: u32`
字段。打包器（`mpm_build`）总是写入 `SYSCALL_PAYLOAD_CURRENT_VERSION`（`1`）；
[`mudu_runtime/src/service/app_package.rs`](../../../crates/db-kernel/mudu_runtime/src/service/app_package.rs)
中的加载器对缺失字段默认取 `1`，并在版本不匹配时以
`ErrorCode::UnsupportedFormatVersion` 拒绝（"unsupported syscall ABI version
{found}, expected {expected}"）。`FormatKind::SyscallPayload` 已注册进全局
`CompatibilityRouter`
（[`mudu_kernel/src/compat.rs`](../../../crates/db-kernel/mudu_kernel/src/compat.rs)），
v1 恒等迁移处理器位于
[`mudu_binding/src/codec/syscall_payload/migrate/`](../../../crates/common/mudu_binding/src/codec/syscall_payload/migrate/)；
迁移单元是完整帧（含 16 字节头部）。

## 版本历史

| 版本 | 日期 | 摘要 |
|------|------|------|
| 1 | 2026-07-01 | 16 字节头部（魔数、版本、标志、消息类型）加上 MessagePack 消息体。MessagePack 布局由 `mgen` 从 WIT 生成，并由项目控制，不是默认的 `rmp_serde` derive 输出。 |
| 1（原地修订） | 2026-09-11 | record/请求体从位置数组改为**整数键 MessagePack map**（字段号/参数号 = WIT 声明顺序 1-based，protobuf 风格）；解码宽松化（接受任意整数宽度、跳过未知 map 键、缺失字段取类型默认值，proto3 语义）。variant `[tag,payload]`、result 信封 `[0,v]/[1,e]` 与帧级校验不变。**版本号未升**：修订时无已部署的外部生态，全部 MPK 随修订重建；版本机制留给未来真正的跨版本场景（见[升级与回滚规则](#升级与回滚规则)）。 |

## 字段号演进政策

record 字段与请求参数的 map 键是 WIT 声明顺序的 1-based 整数（protobuf 风格）。
schema 一旦随发布落地，演进**只追加**：

- **新增字段/参数** 加到 WIT 声明末尾，取得下一个字段号/参数号。旧解码器按宽松
  解码规则跳过未知键，新解码器对缺失字段取类型默认值——新旧编解码器在同一版本内
  互操作。
- **字段号永不复用**；禁止重排或重新编号已有字段（声明顺序即线编号）。
- **删除字段** 的做法是保留号位：从声明中移除该字段，并在原位以注释标记
  `reserved`（字段号与字段名都保留），防止未来误用。
- **新 syscall** 追加到 `uni-syscall.wit` 声明末尾，取得下一个 `message_kind`；
  **variant 新 case** 追加到声明末尾，取得下一个 tag。

以上都属于**版本内 schema 演进**，不触发 `version` 递增——宽松解码保证同一版本内
新旧双方互通。`version` 只在编码规则本身变化时递增（见下文[头部](#头部)与
[升级与回滚规则](#升级与回滚规则)）。

## 负载布局

每个系统调用请求与每个系统调用响应都编码为一条自描述消息：

```text
+--------------------------------+
| Header（16 字节）              |
+--------------------------------+
| Body（可变，MessagePack）      |
+--------------------------------+
```

头部之外没有额外的长度前缀；WIT 传输层负责投递精确的字节区间。

### 头部

所有头部字段均为**大端**。

| 偏移 | 大小 | 字段 | 描述 |
|------|------|------|------|
| 0 | 4 | `magic` | 魔数 `0x4D53_5350`（ASCII `MSSP`）。 |
| 4 | 4 | `version` | 负载格式版本。当前值：`1`。 |
| 8 | 4 | `flags` | 保留。必须为 `0`；解码器拒绝任何非零值。 |
| 12 | 4 | `message_kind` | 消息类型判别值。线头部仍保留该字段用于路由，但合法取值集合由 [`uni-syscall.wit`](../../../mudu_binding/wit/uni-syscall.wit) 定义，而非本文档。 |

`version` 是**负载格式版本**，而非 WIT 接口版本。它只在**编码规则本身**变化时
递增——例如整数语义、帧头布局、map/数组结构规则的变化。schema 演进（追加字段、
追加 `message_kind`、追加 variant tag）由字段号机制在版本内吸收，不递增 `version`
（见[字段号演进政策](#字段号演进政策)）。

`uni-syscall.wit` 中的 WIT 函数接口是存在哪些系统调用、以及它们的请求/响应类型的权威定义。
运行时把每个 WIT 函数映射到同一条内部 16 字节头部 + MessagePack 路径；因此 `message_kind`
判别值作为实现细节保留，用于运行时路由。

### 消息类型

v1 定义了 23 种消息类型。下表列出每个判别值对应的 WIT 函数、请求体形状（WIT 参数编号的
整数键 MessagePack map，`{1: 参数1, 2: 参数2, ...}`，编号为声明顺序 1-based）以及结果负载
形状（`result<T, UniError>` 中的 `T`，编码为 `[0u8, T]` / `[1u8, UniError]`）。WIT 文件
仍是权威来源；本表供独立 guest 编解码器的实现者参考。

| 类型 | 名称 | WIT 函数 | 请求体 | 结果负载 |
|------|------|----------|--------|----------|
| 1 | `Query` | `query` | `{1: argv: UniQueryArgv}` | `UniQueryResult` |
| 2 | `Command` | `command` | `{1: argv: UniCommandArgv}` | `UniCommandResult` |
| 3 | `Batch` | `batch` | `{1: argv: UniCommandArgv}` | `UniCommandResult` |
| 4 | `Open` | `open-session` | `{1: worker_id: UniOid}` | `UniOid`（会话 OID） |
| 5 | `Close` | `close-session` | `{1: oid: UniOid}` | unit（占位 `0u8`） |
| 6 | `Get` | `get` | `{1: oid: UniOid, 2: key: list<u8>}` | `option<list<u8>>` |
| 7 | `Put` | `put` | `{1: oid: UniOid, 2: key: list<u8>, 3: value: list<u8>}` | unit |
| 8 | `Delete` | `delete` | `{1: oid: UniOid, 2: key: list<u8>}` | unit |
| 9 | `Range` | `range` | `{1: oid: UniOid, 2: start: list<u8>, 3: end: list<u8>}` | `list<tuple<list<u8>, list<u8>>>` |
| 10 | `FsOpen` | `fs-open` | `{1: argv: UniFsOpenArgv}` | `u32`（fd） |
| 11 | `FsClose` | `fs-close` | `{1: fd: u32}` | unit |
| 12 | `FsRead` | `fs-read` | `{1: fd: u32, 2: len: u32}` | `list<u8>` |
| 13 | `FsWrite` | `fs-write` | `{1: fd: u32, 2: data: list<u8>}` | `u32`（已写字节数） |
| 14 | `FsPread` | `fs-pread` | `{1: fd: u32, 2: offset: u64, 3: len: u32}` | `list<u8>` |
| 15 | `FsPwrite` | `fs-pwrite` | `{1: fd: u32, 2: offset: u64, 3: data: list<u8>}` | unit |
| 16 | `FsLseek` | `fs-lseek` | `{1: fd: u32, 2: offset: s64, 3: whence: u32}` | `u64`（新位置） |
| 17 | `FsFstat` | `fs-fstat` | `{1: fd: u32}` | `UniFsStat` |
| 18 | `FsStat` | `fs-stat` | `{1: oid: UniOid, 2: path: string}` | `UniFsStat` |
| 19 | `FsFsync` | `fs-fsync` | `{1: fd: u32}` | unit |
| 20 | `FsReaddir` | `fs-readdir` | `{1: oid: UniOid, 2: path: string}` | `list<UniFsDirent>` |
| 21 | `RelationGet` | `relation-get` | `{1: oid: UniOid, 2: table: string, 3: key: list<tuple<u64, list<u8>>>, 4: select: list<u64>}` | `option<list<option<list<u8>>>>` |
| 22 | `RelationUpdate` | `relation-update` | `{1: oid: UniOid, 2: table: string, 3: key: list<tuple<u64, list<u8>>>, 4: values: list<tuple<u64, list<u8>>>, 5: deltas: list<tuple<u64, u8, list<u8>>>}` | `u64`（受影响行数） |
| 23 | `RelationInsert` | `relation-insert` | `{1: oid: UniOid, 2: table: string, 3: key: list<tuple<u64, list<u8>>>, 4: values: list<tuple<u64, list<u8>>>}` | unit |

fs 相关 record 的线形状如下（record 均为字段号键的 MessagePack map，字段号为声明顺序
1-based）：

- **`UniFsOpenArgv`** — `{1: session: UniOid, 2: oid: UniOid, 3: path: string, 4: flags: u32}`。
- **`UniFsStat`** — `{1: oid: UniOid, 2: generation: u64, 3: entry: string, 4: length: u64, 5: state: u32}`。
- **`UniFsDirent`** — `{1: name: string, 2: is_dir: bool, 3: length: u64}`。

relation 系统调用中，`key` 是标识主键的 `(列序号, 列值)` 对列表，`select` 是要投影的
列序号列表，`values` 是待写入的 `(列序号, 列值)` 对列表，`deltas` 是用于原地数值更新的
`(列序号, 操作, 操作数)` 三元组列表。tuple 编码为定长 MessagePack 数组。

**例外：`fetch`。** 运行时 world
（[`mudu_runtime/wit/api.wit`](../../../crates/db-kernel/mudu_runtime/wit/api.wit)）中声明的
宿主 WIT 函数 `fetch(query-result: list<u8>) -> list<u8>` **不**经过 MSSP 路由：它直接收发
原始 `mp_wire` 字节，不带 16 字节头部。请求是 `UniOid` 的 `mp_wire` 编码（字段号键 map）
——即 query 响应中 `UniResultSet.cursor` 携带的字节；响应是
`UniResult<UniResultSet, UniError>` 的 `mp_wire` 编码（信封为一项 map
`{0: value}` / `{1: error}`）。宿主实现（`mudu_runtime::interface::kernel_sync::fetch_internal`）镜像
`sys_interface::api_impl`
（[`crates/common/sys_interface/src/api_impl/mod.rs`](../../../crates/common/sys_interface/src/api_impl/mod.rs)），
后者仍是线形状的权威来源。语义如下：

- `fetch` 排空该游标结果集的**全部**缓存行，并始终报告 `eof: true`。
- 结果集排空后再 `fetch` 返回空的 `row_set` 且 `eof: true`——这**不是**错误。
- 未知游标返回 `UniResult::Err`（`ErrorCode::EntityNotFound`）；畸形游标同样返回
  `UniResult::Err`（WIT ABI 返回裸 `list<u8>`、没有错误通道，因此所有失败都编码进
  `UniResult` 信封）。
- 同步连接上的 query 会把结果集缓存到会话 `Context` 上，供后续 `fetch` 排空；异步连接上的
  query 从不缓存，因此异步 query 之后的 `fetch` 返回空 `row_set` 且 `eof: true`。
- 缓存生命周期：缓存被析构式排空；`Context::query_next` 在结果集到达 EOF 时清除缓存；
  会话关闭时 `Context::remove` 丢弃未排空的缓存。

### 钉死的线格式细节

以下行为属于 v1 契约的一部分，并由 golden 语料逐字节验证；独立实现必须与之逐字节一致：

- **编码 canonical、解码宽松：** 编码器总是写出 canonical 形式（map 键按字段号
  升序、最短整数宽度、bin8 阈值）；解码器是宽松的——接受任意 MessagePack 宽度的
  整数（收窄时做范围检查）、跳过未知 map 键、缺失字段取类型默认值（proto3 语义）、
  map 键顺序任意。record/请求体必须是 map、tuple 长度精确、未知 variant 标签仍被
  拒绝。
- **`UniError.err_details` 与 `UniResultSet.cursor`** 编码为 MessagePack **整数数组**，
  **而不是** MessagePack bin——尽管二者都是 `Vec<u8>`（`list<u8>`）字段：record 上下文中的
  `list<u8>` 一律编码为数组（func 参数/结果中的 blob 才编码为 bin）。独立编解码器
  必须对这两个字段接受并生成数组形式。
- **整数紧致编码：** `rmp_serde` 生成最短的 MessagePack 整数形式。小的非负整数为
  fixint（如 `0` → `0x00`）；小的负整数为负 fixint（如 `i64 -2` → `0xFE`）。
  不要假设固定的整数宽度。
- **blob 编码：** `list<u8>` blob 在短于 256 字节时使用 bin8（`0xC4`），更长时使用
  bin16/bin32。
- **无错误来源的宿主错误**其 `err_src` 为 JSON 字符串 `"None"`（宿主把
  `Option<ErrorSource>` 序列化为 JSON 文本）。
- **fs errno 映射：** 宿主错误码 `50029`（`ErrorCode::InvalidArgument`）由
  `sys_interface::fs::map_fs_errno` 映射为 guest 侧的 `EINVAL`（`22`，
  `ErrorCode::InvalidInput`）。

## 消息体编码

消息体是单个 MessagePack 值。具体字节序列由 `mgen` 模板控制，不得从通用的
`rmp_serde` 默认行为推断。

### 权威来源

权威模式是 [`mudu_binding/wit/`](../../../mudu_binding/wit/) 下的 WIT 文件集合：

| WIT 文件 | Rust 生成类型 |
|----------|--------------|
| `uni-data-type.wit` | `UniDataType` |
| `uni-data-value.wit` | `UniDataValue` / `UniDataValueField` |
| `uni-scalar.wit` | `UniScalar` |
| `uni-scalar-value.wit` | `UniScalarValue` |
| `uni-record-type.wit` | `UniRecordType` / `UniRecordField` / `UniFieldAttr` |
| `uni-result-type.wit` | `UniResultType` |
| `uni-result-set.wit` | `UniResultSet` |
| `uni-tuple-row.wit` | `UniTupleRow` |
| `uni-sql-stmt.wit` | `UniSqlStmt` |
| `uni-sql-param.wit` | `UniSqlParam` |
| `uni-query-argv.wit` | `UniQueryArgv` |
| `uni-command-argv.wit` | `UniCommandArgv` |
| `uni-command-result.wit` | `UniCommandResult` / `UniCommandReturn` |
| `uni-query-result.wit` | `UniQueryResult` / `UniQueryReturn` |
| `uni-error.wit` | `UniError` |
| `uni-oid.wit` | `UniOid` |
| `uni-fs-open-argv.wit` | `UniFsOpenArgv` |
| `uni-fs-stat.wit` | `UniFsStat` |
| `uni-fs-dirent.wit` | `UniFsDirent` |
| `uni-syscall.wit` | syscall 函数接口 |

`mgen` 读取这些 WIT 文件并输出语言相关的源文件。Rust 使用
[`mudu_gen/templates/rust/`](../../../mudu_gen/templates/rust/) 中的模板；C# 使用
[`mudu_gen/templates/csharp/`](../../../mudu_gen/templates/csharp/) 中的模板。

在编解码器生成的落地过程中，WIT 模式本身也按已部署的线值做了修正：`uni-syscall.wit`
重排为 `relation-*` 函数跟在 `fs-*` 函数之后（声明顺序**即**线上 `message_kind`
编号 1–23）；`uni-scalar.wit` 新增 `u128`/`i128`，并把 `timestamptz` 改名为
`timestamp-tz`（`uni-scalar-value.wit` 同步改名）；`uni-data-type.wit` 新增 `%box`
case（标签 8）。这些钉死规则以及确切的重新生成命令记录在各 crate 的指南中：
[`mudu_binding/src/codec/syscall_payload/REGENERATE.md`](../../../crates/common/mudu_binding/src/codec/syscall_payload/REGENERATE.md)（宿主）、
[Rust SDK 的 `REGENERATE.md`](../../../crates/sdk/mudu_api/rust/REGENERATE.md)、
[C# SDK `README.md`](../../../crates/sdk/mudu_api/csharp/README.md) 的 regen 小节，以及
[AssemblyScript 生成代码的 `README`](../../../crates/sdk/bindings/assemblyscript/assembly/generated/README.md)。
旧的校验和文件 `mudu_binding/wit/contract.md5.txt` 早于这些修正，内容已过期，且不再被任何
构建任务引用。

### MessagePack 编码规则

生成代码遵守以下规则，覆盖所有默认 `rmp_serde` 行为：

| WIT 构造 | MessagePack 编码 |
|----------|-----------------|
| `record` | MessagePack **map**，键为字段号（声明顺序 1-based 的整数），值为对应字段的编码。编码时按键号升序写出全部字段。 |
| WIT func 参数列表 | 与 record 相同：参数号（声明顺序 1-based）为键的 MessagePack map。 |
| `variant` | 两元素 MessagePack 数组 `[tag, payload]`。`tag` 是按声明顺序从 `0` 开始分配的 `u32` 判别值。若某个 case 无负载，则 `payload` 为一个占位 `0u8`，保证数组长度始终为 2。 |
| `enum` | 裸 `u32` 判别值，按声明顺序从 `0` 开始分配。 |
| `list<T>` | 编码后的 `T` 组成的 MessagePack 数组。 |
| `option<T>` | `none` 编码为 MessagePack nil；`some` 编码为对应的 `T`。 |
| `result<T, E>` | MessagePack 数组 `[ok_tag, value]`，`ok_tag` 为 `0u8` 表示 `ok`、`1u8` 表示 `err`，随后为编码后的 `T` 或 `E`。 |
| `string` | MessagePack str。 |
| `list<u8>` / `blob` | MessagePack bin。例外：`UniError.err_details` 与 `UniResultSet.cursor` 编码为 MessagePack 数组（普通 serde），见[钉死的线格式细节](#钉死的线格式细节)。 |
| 原始标量（`u8`、`i32`、`u64`、`f32`、`bool`、`char`、...） | 标准 MessagePack 整数/浮点/布尔/字符串表示。 |
| `box<T>` | 与 `T` 编码相同；`box` 在线上是透明的。 |

所有消息体值都经过每种语言**唯一的手写通用运行时**（Rust 端为
`mudu_binding::universal::mp_wire`）：它按 `rmp_serde` 1.3.1 的规则做规范编码（最短整数、
str8/bin8 阈值），并做宽松解码——接受任意 MessagePack 宽度的整数（收窄时做范围检查）、
map 中未知键跳过、缺失字段取类型默认值（proto3 语义）、map 键顺序任意；record/request
的结构（必须是 map）、tuple 长度、未知 variant 标签仍然严格拒绝。C#、AssemblyScript
与 Python 端使用从同一份 WIT 生成的等价运行时代码。

### `OID` / `UniOid`

`OID` 是逻辑上的 128 位对象标识符（`mudu::common::id::OID`）。在线编码为含有两个
`u64` 字段的 `record`，即 MessagePack map `{1: h, 2: l}`。解码器将逻辑
OID 重建为 `((h as u128) << 64) | (l as u128)`。

### 变体标签表

以下标签派生自 [`mudu_binding/wit/`](../../../mudu_binding/wit/) 中的 WIT 声明。
此处列出仅为方便查阅，WIT 文件仍是权威来源。

**`UniScalarValue`** — `variant` 标签：

| 标签 | 变体 | 负载 |
|------|------|------|
| 0 | `Bool` | `bool` |
| 1 | `U8` | `u8` |
| 2 | `I8` | `s8` |
| 3 | `U16` | `u16` |
| 4 | `I16` | `s16` |
| 5 | `U32` | `u32` |
| 6 | `I32` | `s32` |
| 7 | `U64` | `u64` |
| 8 | `U128` | `list<u8>`（16 字节，大端） |
| 9 | `I64` | `s64` |
| 10 | `I128` | `list<u8>`（16 字节，大端） |
| 11 | `F32` | `f32` |
| 12 | `F64` | `f64` |
| 13 | `Char` | `char`（一个 Unicode 标量的 MessagePack str） |
| 14 | `String` | `string` |
| 15 | `Blob` | `list<u8>`（MessagePack bin） |
| 16 | `Numeric` | `string` |
| 17 | `Date` | `string` |
| 18 | `Time` | `string` |
| 19 | `Timestamp` | `string` |
| 20 | `TimestampTz` | `string` |

注意：WIT `uni-scalar-value` 用 `blob` 表示二进制标量负载；`UniScalarValue::Blob`
承载 `Vec<u8>`。`U128` / `I128` 使用 16 字节大端字节数组，以便在不支持原生 128 位整数
MessagePack 类型的 guest 语言之间保持可移植性。

**`UniDataValue`** — `variant` 标签：

| 标签 | 变体 | 负载 |
|------|------|------|
| 0 | `Scalar` | `UniScalarValue` |
| 1 | `Array` | `list<UniDataValue>` |
| 2 | `Record` | `list<UniDataValueField>` |
| 3 | `Binary` | `list<u8>`（MessagePack bin） |

`UniDataValueField` 是 `record`，线形状为 `{1: field_name: string, 2: field_value: UniDataValue}`。

**`UniDataType`** — `variant` 标签：

| 标签 | 变体 | 负载 |
|------|------|------|
| 0 | `Scalar` | `UniScalar` |
| 1 | `Array` | `UniDataType` |
| 2 | `Record` | `UniRecordType` |
| 3 | `Option` | `UniDataType` |
| 4 | `Tuple` | `list<UniDataType>` |
| 5 | `Result` | `UniResultType` |
| 6 | `Identifier` | `string` |
| 7 | `Binary` | `0u8` 占位（该变体无内部类型） |

注意：`box<T>` **不是**独立变体。它仅在 WIT 中作为递归类型（`array`、`option`）的语法糖出现，
在线上是透明的。

**`UniScalar`** — WIT `enum`，裸 `u32` 判别值：

| 值 | 名称 | | 值 | 名称 |
|----|------|-|----|------|
| 0 | `Bool` | | 11 | `F32` |
| 1 | `U8` | | 12 | `F64` |
| 2 | `I8` | | 13 | `Char` |
| 3 | `U16` | | 14 | `String` |
| 4 | `I16` | | 15 | `Blob` |
| 5 | `U32` | | 16 | `Numeric` |
| 6 | `I32` | | 17 | `Date` |
| 7 | `U64` | | 18 | `Time` |
| 8 | `U128` | | 19 | `Timestamp` |
| 9 | `I64` | | 20 | `TimestampTz` |
| 10 | `I128` | | | |

`UniScalar` 与 `UniScalarValue` 现在使用相同的标签编号。

### 复合类型布局

每个 record/variant 的 MessagePack 编码遵循上述通用规则。以下列出字段号与负载形状，
供快速参考（record 的键为声明顺序 1-based 字段号）：

- **`UniOid`** — `{1: h: u64, 2: l: u64}`。
- **`UniSqlStmt`** — `{1: sql_string: string}`。
- **`UniSqlParam`** — `{1: params: list<UniDataValue>}`。
- **`UniTupleRow`** — `{1: fields: list<UniDataValue>}`。
- **`UniFieldAttr`** — `{1: attr_name: string, 2: attr_value: string}`。
- **`UniRecordField`** — `{1: field_name: string, 2: field_type: UniDataType, 3: field_attrs: list<UniFieldAttr>}`。
- **`UniRecordType`** — `{1: record_name: string, 2: record_fields: list<UniRecordField>}`。
- **`UniResultType`** — `{1: ok: option<UniDataType>, 2: err: option<UniDataType>}`。
- **`UniResultSet`** — `{1: eof: bool, 2: row_set: list<UniTupleRow>, 3: cursor: list<u8>}`。
- **`UniError`** — `{1: err_code: u32, 2: err_msg: string, 3: err_src: string, 4: err_loc: string, 5: err_details: list<u8>}`。
- **`UniQueryArgv`** — `{1: oid: UniOid, 2: query: UniSqlStmt, 3: param_list: UniSqlParam}`。
- **`UniCommandArgv`** — `{1: oid: UniOid, 2: command: UniSqlStmt, 3: param_list: UniSqlParam}`。
- **`UniQueryResult`** — `{1: tuple_desc: UniRecordType, 2: result_set: UniResultSet}`。
- **`UniCommandResult`** — `{1: affected_rows: u64}`。
- **`UniCommandReturn`** — variant：`ok(UniCommandResult)` 或 `err(UniError)`。
- **`UniQueryReturn`** — variant：`ok(UniQueryResult)` 或 `err(UniError)`。
- **`UniSessionOpenArgv`** — 当前为手写 Rust record，线形状为
  `{1: worker_id: UniOid}`。它尚未对应独立的 WIT 文件；若未来纳入 WIT，编码规则与本节相同。
- **`UniFsOpenArgv`** / **`UniFsStat`** / **`UniFsDirent`** — 见[消息类型](#消息类型)。

## 与旧版 session 编解码器的关系

全部 23 种类型——包括 KV 系统调用（`Open`..`Range`）——现在都通过
[`mudu_binding/src/codec/syscall_payload/`](../../../crates/common/mudu_binding/src/codec/syscall_payload/)
中的带帧 MessagePack 路径编解码：每个负载包进 16 字节头部，错误使用项目控制的 MessagePack
`Result<T, E>` 编码，替代旧版 `MERR` 魔数逃逸。此前 `handle_sys_session.rs` 中的手写二进制
session 编解码器遵循的约定与本契约的基础类型一致；统一迁移到项目控制的 MessagePack 路径的
Phase 2 工作已完成。

## Guest 语言支持

| Guest 语言 | 机制 | 覆盖 |
|------------|------|------|
| Workspace Rust | 经 `sys_interface` → `mudu_binding` 自动获得：universal 类型与按函数编解码器（`codec/syscall_payload/generated/uni_syscall.rs`）均由 **mgen 从 WIT 生成**；`mod.rs`/`router.rs` 只是薄的 `MuduError` 适配层。重新生成见 [`REGENERATE.md`](../../../crates/common/mudu_binding/src/codec/syscall_payload/REGENERATE.md)。 | 全部 23 种 |
| 独立 Rust SDK（[`crates/sdk/mudu_api/rust`](../../../crates/sdk/mudu_api/rust/)，包名 `mudu_api_rust`） | **mgen 生成**的 universal DTO 加上 `src/mudu_sys/generated/uni_syscall.rs`（与宿主同一份 WIT、同一个生成器——该 crate 是独立 workspace，不依赖 `mudu_binding`），带类型化的异步 `Mudu` 封装与 `mock-sqlite` 内存 mock。重新生成见 crate 根目录 `REGENERATE.md`。 | 全部 23 种 |
| C# SDK（[`crates/sdk/mudu_api/csharp`](../../../crates/sdk/mudu_api/csharp/)） | **mgen 生成**的 `uni/*.cs`（22 个 DTO）加上 `mudu_sys/UniSyscall.cs`（命名空间改写为 `Mudu.Api.MuduSys`）；默认路径 resolver-free，可运行于 wasi-wasm NativeAOT 目标；标量标签由 `Mudu.Api.Tests/UniScalarTagTests.cs` 与宿主钉死；`MockSqliteMuduSysCall` 提供 KV/relation/fs 模拟；xunit 测试在 `Mudu.Api.Tests/`。重新生成见 crate `README.md`。 | 全部 23 种 |
| AssemblyScript | **mgen 生成**的编解码器位于 [`crates/sdk/bindings/assemblyscript/assembly/generated/`](../../../crates/sdk/bindings/assemblyscript/assembly/generated/)（24 个文件），运行时为 `assembly/mpack.ts`；guest 直接 import `mududb:api/system` 并在语言内完成 MSSP 成帧（与 C# guest 相同的直连 byte-pipe 架构），经 `wasm-tools component embed/new` 构建（见 [`crates/sdk/example/wallet-as/Makefile.toml`](../../../crates/sdk/example/wallet-as/Makefile.toml)）。 | 全部 23 种 |
| Python（工具/测试验证，[`crates/sdk/bindings/python`](../../../crates/sdk/bindings/python/)） | **mgen 生成**的编解码器位于 `mududb/generated/`（24 个文件，含 `uni_syscall.py`），运行时为手写的纯 stdlib `mududb/codec/mpack.py`（`F32` marker 类发出 f32）；角色是工具/测试验证而非 guest 路径，已由三套语料验证：`run_mp_corpus_test.py`（132 项检查）、`run_syscall_corpus_test.py`（188 项检查）、`run_lenient_decode_test.py`（16 项检查），外加 `tests/test_mpack.py` 单元测试。重新生成见包 `README.md`。 | 全部 23 种 |
| `fetch` | 宿主 WIT 函数，不经 MSSP 路由；已实现为原始 `mp_wire` 字节（见[消息类型](#消息类型)）。 | 不适用 |

## 互操作验证

各独立 guest 编解码器与权威宿主编解码器之间的字节级互操作性，通过单一宿主生成的 golden
语料
[`testing/fixtures/golden/v1/syscall_payload_v1_all.bin`](../../../crates/db-kernel/testing/fixtures/golden/v1/syscall_payload_v1_all.bin)
验证：共 47 帧，覆盖类型 1–23（按判别值顺序各一对请求 + 成功响应），外加一帧 `UniError`
get 响应。容器格式为大端 `u32` 长度前缀 MSSP 帧的顺序拼接。以下五个独立编解码器对它做了
逐字节验证：

- 宿主路由器 — `testing/tests/compat_golden.rs::golden_v1_all_kinds_roundtrip`；
- 独立 Rust SDK — `mudu_api/rust/tests/golden_frames_test.rs`；
- C# SDK — `Mudu.Api.Tests/GoldenFrameCorpusTests.cs`；
- AssemblyScript 生成的编解码器 —
  `bindings/assemblyscript/run_syscall_corpus_test.mjs`（172 项检查）；
- Python 生成的编解码器 —
  `bindings/python/run_syscall_corpus_test.py`（188 项检查）。

各语言验证命令：

```sh
cargo test -p testing --test compat_golden   # Rust，在 crates/db-kernel 下
dotnet test Mudu.Api.Tests                   # C#，在 crates/sdk/mudu_api/csharp 下
node run_syscall_corpus_test.mjs             # AS，在 crates/sdk/bindings/assemblyscript 下
python3 run_syscall_corpus_test.py           # Python，在 crates/sdk/bindings/python 下
```

另有第二个原语级语料——
[`mp_primitives_v1.bin`](../../../crates/db-kernel/testing/fixtures/golden/v1/mp_primitives_v1.bin)
及其 `mp_primitives_v1.json` sidecar（44 个向量）——固定原始 `rmp_serde` 编码规则，
由 `rmp_serde`（宿主）、MessagePack-CSharp（C# SDK）、`mpack.ts`（AssemblyScript）与
`mududb/codec/mpack.py`（Python）逐字节验证；详见[集成指南](../abi/guest_host_abi.cn.md#mp-原语对齐)。

宿主编解码器为权威实现。重新生成语料使用带 `#[ignore]` 的
`generate_golden_v1_fixtures` 测试：

```sh
cargo test -p testing --test compat_golden -- generate_golden_v1_fixtures --ignored
```

五个实现之间未发现任何差异。

## 完整性机制

- **魔数校验：** 解码器拒绝魔数不为 `0x4D53_5350` 的消息。
- **版本校验：** 解码器拒绝任何超出 `[1, 1]` 的版本，映射为
  `ErrorCode::UnsupportedFormatVersion`。
- **标志校验：** 解码器拒绝任何非零 `flags` 值。
- **消息类型校验：** 解码器拒绝未知/`0` 的 `message_kind` 值。
- **长度校验：** 解码器要求头部至少 16 字节。
- **MessagePack 结构校验：** 解码器拒绝格式错误的 MessagePack 负载、消息体尾部多余字节、
  未知变体标签，以及不是 map 的 record/请求体。map 内部的解码是宽松的（未知键跳过、
  缺失字段取默认值、整数宽度任意），但收窄溢出与类型不符仍被拒绝。
- **UTF-8 校验：** `string` 字段必须为合法 UTF-8。

消息体结构性解码失败映射为 `ErrorCode::Decode`。魔数不匹配经兼容性注册表映射为
`ErrorCode::CorruptedData`。

## 基准测试

宿主编解码器的基准测试位于
[`mudu_binding/tests/syscall_payload_bench.rs`](../../../crates/common/mudu_binding/tests/syscall_payload_bench.rs)，
遵循仓库手工计时 `#[test]` 基准的惯例（不使用 criterion）。运行方式：

```sh
cargo test -p mudu_binding --test syscall_payload_bench -- --nocapture --test-threads=1
```

### 方法

- **负载：** query 请求（短 SQL + 10 个参数；约 4 KB SQL + 100 个参数）；含 1、100、
  10 000 行的 query 结果（int/text/blob/float/bool/null 占位混合列）；KV
  get/put/delete/range 请求与成功结果（range 结果含 100 个键值对）；一个 fs-open
  请求与一个含 50 个目录项的 fs-readdir 结果；以及一个 `UniError` 错误响应。
- **基线：** 每个负载同时与一个*原始* `mp_wire` 基线对比计时，该基线对**字节完全
  相同的 MessagePack 消息体值**直接编解码，不带 16 字节头部、不做消息类型路由。帧开销按 `(mssp − raw) / raw` 分方向报告；该百分比是与机器无关的
  指标，绝对 µs/op 数值仅供参考。
- **计时：** 先跑迭代数 10% 的预热，再计时 N 次迭代，使用
  `mudu_sys::time::instant_now()` 取墙钟时间。每个负载在计时前先断言
  decode(encode(x)) 往返正确，并断言原始基线消息体与 MSSP 帧消息体字节一致。
- **不做分配统计：** 仓库没有分配跟踪基础设施，且基准惯例禁止新增依赖（不用
  criterion，也不用 `stats_alloc`/`dhat`），因此只报告墙钟时间与负载大小。
- **记录环境：** AMD Ryzen 9 3900X，dev（未优化）cargo profile，单测试线程，
  2026-08-26。当前数值请重新运行基准获得；下表仅作数量级参考。

### 实测结果

| 负载 | 迭代数 | 帧字节数 | MSSP 编码 µs/op | MSSP 解码 µs/op | 基线编码 µs/op | 基线解码 µs/op | 帧开销（编码） | 帧开销（解码） |
|------|------:|------------:|---------------:|---------------:|--------------:|--------------:|------------:|------------:|
| query 请求，短 SQL + 10 参数 | 20 000 | 156 | 8.17 | 12.42 | 7.88 | 8.20 | +3.7% | +51.4% |
| query 请求，约 4 KB SQL + 100 参数 | 2 000 | 5 078 | 62.17 | 105.17 | 67.26 | 74.74 | −7.6% | +40.7% |
| query 结果，1 行 | 20 000 | 156 | 14.10 | 21.78 | 13.30 | 15.05 | +6.0% | +44.7% |
| query 结果，100 行 | 1 000 | 6 584 | 567.77 | 842.34 | 565.25 | 577.51 | +0.4% | +45.9% |
| query 结果，10 000 行 | 100 | 766 680 | 63 675 | 91 153 | 61 550 | 64 722 | +3.5% | +40.8% |
| KV get 请求 | 20 000 | 50 | 1.34 | 1.79 | 1.22 | 0.89 | +10.5% | +100.6% |
| KV get 结果 | 20 000 | 83 | 1.02 | 1.35 | 0.73 | 0.53 | +38.4% | +153.5% |
| KV put 请求 | 20 000 | 115 | 1.74 | 2.33 | 1.55 | 1.15 | +11.7% | +103.0% |
| KV put 结果 | 20 000 | 19 | 0.69 | 0.82 | 0.42 | 0.27 | +65.2% | +200.2% |
| KV delete 请求 | 20 000 | 50 | 1.36 | 1.80 | 1.16 | 0.89 | +16.6% | +102.8% |
| KV delete 结果 | 20 000 | 19 | 0.69 | 0.83 | 0.42 | 0.26 | +66.2% | +221.4% |
| KV range 请求 | 20 000 | 64 | 1.62 | 2.64 | 1.40 | 1.12 | +15.7% | +136.4% |
| KV range 结果，100 键值对 | 20 000 | 8 021 | 55.10 | 126.12 | 50.41 | 62.73 | +9.3% | +101.0% |
| fs-open 请求 | 20 000 | 86 | 2.22 | 2.86 | 1.92 | 1.64 | +15.2% | +74.2% |
| fs-readdir 结果，50 目录项 | 20 000 | 1 016 | 27.48 | 58.62 | 25.62 | 37.22 | +7.3% | +57.5% |
| UniError 错误响应 | 20 000 | 89 | 2.89 | 3.78 | 1.74 | 1.58 | +66.4% | +139.6% |

结果解读：

- **消息体超过约 100 字节后，编码帧开销很小**（约 0–17%，多 KB 消息体时在噪声
  范围内）；固定成本是一次额外的帧分配加上 16 字节头部拷贝。只有在极小的帧
  （unit 结果、小错误/值消息体）上才明显，约 40–66%。
- **解码帧开销按比例更大**——多 KB 消息体仍有约 +40–50%，100 字节以下的帧可达
  约 3 倍。除头部校验与类型路由外，带帧解码器还做尾部字节拒绝与逐类型的值转换，
  而基线只对动态值解码。其绝对代价是每帧固定的亚微秒到微秒级成本，相对任何真实
  系统调用工作都可忽略。
- 设计方案（`doc/cn/todo/project-controlled-guest-host-abi.md` 第 4 节）原本假设
  使用大端标签（BE-tagged）的消息体编码，并计划将其与 MessagePack 基线对比测试。
  该带标签消息体从未实现——v1 消息体是项目自控的 MessagePack（经 `mp_wire`
  运行时编码）——因此基准改为测量实际落地的帧封装 + 路由相对于原始 `mp_wire`
  消息体的开销，如上表所示。

## 兼容矩阵

| 读方 \ 写方 | v1 |
|-------------|----|
| v1 | 兼容 |

仅支持版本 `1`。

## 升级与回滚规则

先区分两类变化：

- **编码规则变更**（整数语义、帧头布局、map/数组结构规则等）——递增 `version`，
  按下述升级流程处理。
- **schema 演进**（追加字段/参数、追加 `message_kind`、追加 variant tag）——在版本
  内由字段号机制吸收（见[字段号演进政策](#字段号演进政策)），**不**递增
  `version`，也**不**要求重建已部署的 MPK：旧包与新包靠宽松解码（未知键跳过、
  缺失字段默认）继续互通；只有需要使用新字段的 guest 才重新生成绑定并重打包。

规则：

- **升级（编码规则变更）：** 未来的 v2 会更改头部 `version`。引入 v2 时，MPK 包
  针对匹配的运行时重新构建，离线迁移处理器转换已存的 fixture/测试数据。
- **无旧版解码器：** 运行时只解码当前版本。版本不支持的消息以
  `ErrorCode::UnsupportedFormatVersion` 快速失败。
- **无跨编码版本 MPK 支持：** 包在每次 ABI 版本递增时重新构建；版本机制的存在是
  为了让未来的编码规则升级显式可控，而非保留跨版本兼容性。

## 废弃策略

版本 `1` 仅在满足以下条件后方可废弃：

1. 所有 guest 绑定（Rust、C#、AssemblyScript、Python）都能发出并接受后继编码版本（v2）。
2. 新版本已作为默认至少一个发布周期。
3. MPK 包已针对新运行时重新构建。

废弃针对的是编码版本；版本内 schema 演进（字段号追加）不产生可废弃的旧版本。

## 参考

- 集成指南：[doc/cn/abi/guest_host_abi.cn.md](../abi/guest_host_abi.cn.md)
- 注册表：[mudu/src/compat/mod.rs](../../../mudu/src/compat/mod.rs)
- WIT 模式源：[mudu_binding/wit/](../../../mudu_binding/wit/)
- Syscall 函数接口：[mudu_binding/wit/uni-syscall.wit](../../../mudu_binding/wit/uni-syscall.wit)
- 宿主带帧编解码器（权威）：[mudu_binding/src/codec/syscall_payload/](../../../crates/common/mudu_binding/src/codec/syscall_payload/)
- `mgen` Rust 模板：[mudu_gen/templates/rust/](../../../mudu_gen/templates/rust/)
- `mgen` C# 模板：[mudu_gen/templates/csharp/](../../../mudu_gen/templates/csharp/)
- `universal` 类型族（Rust）：[mudu_binding/src/universal/](../../../mudu_binding/src/universal/)
- 旧版手写 KV 编解码器：[mudu_binding/src/codec/handle_sys_session.rs](../../../crates/common/mudu_binding/src/codec/handle_sys_session.rs)
- SQL 请求/响应编解码器：[mudu_binding/src/codec/handle_sys_incoming.rs](../../../mudu_binding/src/codec/handle_sys_incoming.rs)、[handle_sys_outcoming.rs](../../../mudu_binding/src/codec/handle_sys_outcoming.rs)
- 宿主系统调用入口：[mudu_runtime/src/interface/kernel_sync.rs](../../../mudu_runtime/src/interface/kernel_sync.rs)、[kernel_async.rs](../../../mudu_runtime/src/interface/kernel_async.rs)
- Golden 语料：[testing/fixtures/golden/v1/syscall_payload_v1_all.bin](../../../crates/db-kernel/testing/fixtures/golden/v1/syscall_payload_v1_all.bin)
- 独立 Rust SDK：[crates/sdk/mudu_api/rust/](../../../crates/sdk/mudu_api/rust/)
- C# SDK：[crates/sdk/mudu_api/csharp/](../../../crates/sdk/mudu_api/csharp/)
- Python 绑定（工具/测试验证）：[crates/sdk/bindings/python/](../../../crates/sdk/bindings/python/)
- 设计方案：[doc/cn/todo/project-controlled-guest-host-abi.md](../todo/project-controlled-guest-host-abi.md)
