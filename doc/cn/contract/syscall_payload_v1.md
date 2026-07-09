# Guest→Host 系统调用负载契约 v1

## 适用范围

本文档规定 MuduDB guest→host 系统调用所使用的、由项目自有的二进制线格式。

该线格式保留 Phase 1 引入的 16 字节头部，并用 **MessagePack** 编码消息体。
MessagePack 布局是**项目可控的**：它派生自
[`mudu_binding/wit/`](../../../mudu_binding/wit/) 中的权威 WIT 定义，由 `mgen`
生成到 Rust 的自定义 `serde` 实现以及 C# 的自定义 MessagePack formatter。
实现仅把 `rmp_serde`（或 C#/AssemblyScript 端的等效 MessagePack 库）作为底层编解码器；
**不**使用默认的 `rmp_serde` derive 行为，因为后者会把 Rust 枚举变体序列化成 map 或字符串，
无法在不同 guest 语言之间保持稳定。

权威的 syscall 边界是 [`uni-syscall.wit`](../../../mudu_binding/wit/uni-syscall.wit)
中声明的 WIT 函数接口。本文所述的头部与 MessagePack 消息体是运行时内部对那些 WIT 函数调用的
序列化；guest 代码应直接调用 WIT 函数，由生成的绑定处理线格式。

逻辑数据模型仍是现有的 `mudu_binding::universal` 类型族；本契约定义这些类型的**线编码**。

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

## 版本历史

| 版本 | 日期 | 摘要 |
|------|------|------|
| 1 | 2026-07-01 | 16 字节头部（魔数、版本、标志、消息类型）加上 MessagePack 消息体。MessagePack 布局由 `mgen` 从 WIT 生成，并由项目控制，不是默认的 `rmp_serde` derive 输出。 |

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

`version` 是**负载格式版本**，而非 WIT 接口版本。

`uni-syscall.wit` 中的 WIT 函数接口是存在哪些系统调用、以及它们的请求/响应类型的权威定义。
运行时把每个 WIT 函数映射到同一条内部 16 字节头部 + MessagePack 路径；因此 `message_kind`
判别值作为实现细节保留，用于运行时路由。

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
| `uni-syscall.wit` | syscall 函数接口 |

`mgen` 读取这些 WIT 文件并输出语言相关的源文件。Rust 使用
[`mudu_gen/templates/rust/`](../../../mudu_gen/templates/rust/) 中的模板；C# 使用
[`mudu_gen/templates/csharp/`](../../../mudu_gen/templates/csharp/) 中的模板。

### MessagePack 编码规则

生成代码遵守以下规则，覆盖所有默认 `rmp_serde` 行为：

| WIT 构造 | MessagePack 编码 |
|----------|-----------------|
| `record` | 定长 MessagePack 数组，字段按声明顺序各一个元素。 |
| `variant` | 两元素 MessagePack 数组 `[tag, payload]`。`tag` 是按声明顺序从 `0` 开始分配的 `u32` 判别值。若某个 case 无负载，则 `payload` 为一个占位 `0u8`，保证数组长度始终为 2。 |
| `enum` | 裸 `u32` 判别值，按声明顺序从 `0` 开始分配。 |
| `list<T>` | 编码后的 `T` 组成的 MessagePack 数组。 |
| `option<T>` | `none` 编码为 MessagePack nil；`some` 编码为对应的 `T`。 |
| `result<T, E>` | MessagePack 数组 `[ok_tag, value]`，`ok_tag` 为 `0u8` 表示 `ok`、`1u8` 表示 `err`，随后为编码后的 `T` 或 `E`。 |
| `string` | MessagePack str。 |
| `list<u8>` / `blob` | MessagePack bin。 |
| 原始标量（`u8`、`i32`、`u64`、`f32`、`bool`、`char`、...） | 标准 MessagePack 整数/浮点/布尔/字符串表示。 |
| `box<T>` | 与 `T` 编码相同；`box` 在线上是透明的。 |

由于布局来自 WIT，相同的规则也适用于 C# 与 AssemblyScript guest。Rust 端使用
`rmp_serde` 配合自定义的 `Serialize` / `Deserialize` 实现；C# 端使用从同一份 WIT
生成的自定义 `IMessagePackFormatter<T>` 实现。

### `OID` / `UniOid`

`OID` 是逻辑上的 128 位对象标识符（`mudu::common::id::OID`）。在线编码为含有两个
`u64` 字段 `h` 与 `l` 的 `record`，即两元素 MessagePack 数组 `[h, l]`。解码器将逻辑
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

`UniDataValueField` 是 `record`，线形状为 `[field_name: string, field_value: UniDataValue]`。

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

每个 record/variant 的 MessagePack 编码遵循上述通用规则。以下列出字段顺序与负载形状，
供快速参考：

- **`UniOid`** — `[h: u64, l: u64]`（record）。
- **`UniSqlStmt`** — `[sql_string: string]`（record）。
- **`UniSqlParam`** — `[params: list<UniDataValue>]`（record）。
- **`UniTupleRow`** — `[fields: list<UniDataValue>]`（record）。
- **`UniFieldAttr`** — `[attr_name: string, attr_value: string]`（record）。
- **`UniRecordField`** — `[field_name: string, field_type: UniDataType, field_attrs: list<UniFieldAttr>]`（record）。
- **`UniRecordType`** — `[record_name: string, record_fields: list<UniRecordField>]`（record）。
- **`UniResultType`** — `[ok: option<UniDataType>, err: option<UniDataType>]`（record）。
- **`UniResultSet`** — `[eof: bool, row_set: list<UniTupleRow>, cursor: list<u8>]`（record）。
- **`UniError`** — `[err_code: u32, err_msg: string, err_src: string, err_loc: string, err_details: list<u8>]`（record）。
- **`UniQueryArgv`** — `[oid: UniOid, query: UniSqlStmt, param_list: UniSqlParam]`（record）。
- **`UniCommandArgv`** — `[oid: UniOid, command: UniSqlStmt, param_list: UniSqlParam]`（record）。
- **`UniQueryResult`** — `[tuple_desc: UniRecordType, result_set: UniResultSet]`（record）。
- **`UniCommandResult`** — `[affected_rows: u64]`（record）。
- **`UniCommandReturn`** — variant：`ok(UniCommandResult)` 或 `err(UniError)`。
- **`UniQueryReturn`** — variant：`ok(UniQueryResult)` 或 `err(UniError)`。
- **`UniSessionOpenArgv`** — 当前为手写 Rust record，线形状为
  `[worker_id: UniOid]`。它尚未对应独立的 WIT 文件；若未来纳入 WIT，编码规则与本节相同。

## 与当前 session 编解码器的关系

KV 系统调用（`Open`..`Range`）在
[`handle_sys_session.rs`](../../../mudu_binding/src/codec/handle_sys_session.rs)
中仍是手写二进制。其现有约定与本契约的基础类型一致。v1 将这些负载包进 16 字节头部，
并使用项目控制的 MessagePack `Result<T, E>` 编码处理错误，替代旧版 `MERR` 魔数逃逸。
将 session 编解码器统一迁移到同一条 `mgen` 生成的 MessagePack 路径是 Phase 2 的工作。

## 完整性机制

- **魔数校验：** 解码器拒绝魔数不为 `0x4D53_5350` 的消息。
- **版本校验：** 解码器拒绝任何超出 `[1, 1]` 的版本，映射为
  `ErrorCode::UnsupportedFormatVersion`。
- **标志校验：** 解码器拒绝任何非零 `flags` 值。
- **消息类型校验：** 解码器拒绝未知/`0` 的 `message_kind` 值。
- **长度校验：** 解码器要求头部至少 16 字节。
- **MessagePack 结构校验：** 解码器拒绝格式错误的 MessagePack 负载、未知变体标签、
  以及长度不符合预期的 record 数组。
- **UTF-8 校验：** `string` 字段必须为合法 UTF-8。

消息体结构性解码失败映射为 `ErrorCode::Decode`。魔数不匹配经兼容性注册表映射为
`ErrorCode::CorruptedData`。

## 兼容矩阵

| 读方 \ 写方 | v1 |
|-------------|----|
| v1 | 兼容 |

仅支持版本 `1`。

## 升级与回滚规则

- **升级：** 未来的 v2 会更改头部 `version` 与/或消息体布局。引入 v2 时，MPK 包针对
  匹配的运行时重新构建，离线迁移处理器转换已存的 fixture/测试数据。
- **无旧版解码器：** 运行时只解码当前版本。版本不支持的消息以
  `ErrorCode::UnsupportedFormatVersion` 快速失败。
- **无旧版 MPK 支持：** 包在每次 ABI 版本递增时重新构建；版本机制的存在是为了让未来
  升级显式可控，而非保留跨版本兼容性。

## 废弃策略

版本 `1` 仅在满足以下条件后方可废弃：

1. 所有 guest 绑定（Rust、C#、AssemblyScript）都能发出并接受新版本。
2. 新版本已作为默认至少一个发布周期。
3. MPK 包已针对新运行时重新构建。

## 参考

- 注册表：[mudu/src/compat/mod.rs](../../../mudu/src/compat/mod.rs)
- WIT 模式源：[mudu_binding/wit/](../../../mudu_binding/wit/)
- Syscall 函数接口：[mudu_binding/wit/uni-syscall.wit](../../../mudu_binding/wit/uni-syscall.wit)
- `mgen` Rust 模板：[mudu_gen/templates/rust/](../../../mudu_gen/templates/rust/)
- `mgen` C# 模板：[mudu_gen/templates/csharp/](../../../mudu_gen/templates/csharp/)
- `universal` 类型族（Rust）：[mudu_binding/src/universal/](../../../mudu_binding/src/universal/)
- 现有手写 KV 编解码器：[mudu_binding/src/codec/handle_sys_session.rs](../../../mudu_binding/src/codec/handle_sys_session.rs)
- SQL 请求/响应编解码器：[mudu_binding/src/codec/handle_sys_incoming.rs](../../../mudu_binding/src/codec/handle_sys_incoming.rs)、[handle_sys_outcoming.rs](../../../mudu_binding/src/codec/handle_sys_outcoming.rs)
- 宿主系统调用入口：[mudu_runtime/src/interface/kernel.rs](../../../mudu_runtime/src/interface/kernel.rs)
- 设计方案：[doc/cn/todo/project-controlled-guest-host-abi.md](../todo/project-controlled-guest-host-abi.md)
