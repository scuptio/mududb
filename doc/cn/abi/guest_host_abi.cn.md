# Guest/Host 系统调用 ABI（MSSP v1）：集成指南

本文档说明 MuduDB guest（编译为 WebAssembly 组件的存储过程）如何通过 guest→host
系统调用 ABI 与宿主内核通信，以及如何为一门新的 guest 语言接入该 ABI。

线格式的规范性、版本化定义见契约文档
[`../contract/syscall_payload_v1.md`](../contract/syscall_payload_v1.md)
（English: [`../../en/contract/syscall_payload_v1.md`](../../en/contract/syscall_payload_v1.md)）。
本文是配套的实践指南：整体架构、各语言入口，以及如何验证一个新实现。

## 总览

guest 发起的每一次系统调用——SQL、键值、relation、文件系统——都以**不透明的
`list<u8>`** 跨越组件边界。这些字节就是一帧 MSSP v1：16 字节大端头部加上 MessagePack
消息体。宿主路由器解码帧、按消息类型分发，并把结果编码为另一帧 MSSP 返回。

```text
+---------------------+        +----------------------+        +-------------------+
| Guest 语言          |        | 组件边界             |        | 宿主              |
|                     |        |                      |        |                   |
|  workspace Rust     |        |  uni-syscall.wit     |        |  mudu_runtime     |
|  独立 Rust SDK      | MSSP   |  中的 WIT 函数       | MSSP   |  kernel_sync /    |
|  C#                 |  帧    |  （不透明 list<u8>   |  帧    |  kernel_async     |
|  AssemblyScript     |=======>|  入 / list<u8> 出）  |=======>|        |          |
|                     |        |                      |        |  syscall_payload  |
|                     |        |                      |        |  路由器（23 种）  |
+---------------------+        +----------------------+        +--------+----------+
                                                                        |
                                                               +--------v----------+
                                                               | mudu_kernel       |
                                                               |（SQL/KV/relation/ |
                                                               |  fs 执行）        |
                                                               +-------------------+
```

- WIT 边界由
  [`uni-syscall.wit`](../../../crates/common/mudu_binding/wit/uni-syscall.wit)
  定义（23 个函数）；导入它的运行时 world 是
  [`mudu_runtime/wit/api.wit`](../../../crates/db-kernel/mudu_runtime/wit/api.wit)。
- 宿主侧带帧编解码器（权威实现）位于
  [`mudu_binding/src/codec/syscall_payload/`](../../../crates/common/mudu_binding/src/codec/syscall_payload/)
  （`mod.rs` 负责帧封装，`router.rs` 负责按类型编解码）。
- 例外是 `fetch`：一个**不**经 MSSP 路由的宿主 WIT 函数，直接收发原始 `rmp_serde`
  字节（见下文 [`fetch` 协议](#fetch-协议)）。

## MSSP v1 帧格式

每个请求与每个响应都是一条自描述的帧。没有长度前缀；WIT 传输层负责投递精确的
字节区间。

```text
+--------------------------------+
| 头部（16 字节，大端）          |
+--------------------------------+
| 消息体（可变，MessagePack）    |
+--------------------------------+
```

### 头部

| 偏移 | 大小 | 字段 | 描述 |
|------|------|------|------|
| 0 | 4 | `magic` | `0x4D53_5350`（ASCII `MSSP`）。 |
| 4 | 4 | `version` | 负载格式版本。当前值：`1`。 |
| 8 | 4 | `flags` | 保留。必须为 `0`；任何非零值都会被拒绝。 |
| 12 | 4 | `message_kind` | 消息类型判别值，1–23（见下表）。 |

### 消息体规则

- 消息体是单个 MessagePack 值；解码器**拒绝尾部多余字节**。
- **请求**是参数号（声明顺序 1-based）为键的 MessagePack map。
- **结果**为 `[0u8, value]` 表示 `ok`，`[1u8, UniError]` 表示 `err`。
- **字节 blob**（func 级 `list<u8>` 参数与结果）使用 MessagePack bin。
- record 是字段号（声明顺序 1-based）为键的 MessagePack map；variant 是
  `[tag, payload]` 二元组；`option<T>` 为 nil 或对应值。解码是宽松的：未知
  map 键跳过、缺失字段取类型默认值（proto3 语义）、接受任意宽度的整数、
  map 键顺序任意；record/request 必须为 map、tuple 长度、未知 variant 标签
  仍然严格。完整的类型表（`UniScalarValue`、`UniDataValue`、
  `UniDataType`、复合类型布局）见[契约文档](../contract/syscall_payload_v1.md)。

### 钉死的线格式细节

以下行为属于 v1 契约，并由 golden 语料逐字节验证；独立编解码器必须与之逐字节一致：

- `UniError.err_details` 与 `UniResultSet.cursor` 是 `Vec<u8>` 字段，但编码为
  MessagePack **整数数组**（record 上下文的 `list<u8>` 一律为数组），**而不是** bin。
  func 级的 blob 一律使用 bin。
- `rmp_serde` 整数紧致编码：总是使用最短的整数形式。小的非负整数为 fixint
  （`0` → `0x00`）；小的负整数为负 fixint（`i64 -2` → `0xFE`）。短于 256 字节的
  blob 使用 bin8（`0xC4`）。
- 无错误来源的宿主错误，其 `err_src` 为 JSON 字符串 `"None"`。
- fs errno 映射：宿主错误码 `50029`（`ErrorCode::InvalidArgument`）经
  `sys_interface::fs::map_fs_errno` 映射为 guest 侧的 `EINVAL`（`22`）。

## 系统调用映射

23 个系统调用及其 WIT 函数、消息类型：

| 类型 | WIT 函数 | 类别 | 结果负载 |
|------|----------|------|----------|
| 1 | `query` | SQL | `UniQueryResult` |
| 2 | `command` | SQL | `UniCommandResult` |
| 3 | `batch` | SQL | `UniCommandResult` |
| 4 | `open-session` | 会话 | `UniOid`（会话 OID） |
| 5 | `close-session` | 会话 | unit |
| 6 | `get` | KV | `option<list<u8>>` |
| 7 | `put` | KV | unit |
| 8 | `delete` | KV | unit |
| 9 | `range` | KV | `list<tuple<list<u8>, list<u8>>>` |
| 10 | `fs-open` | 文件系统 | `u32`（fd） |
| 11 | `fs-close` | 文件系统 | unit |
| 12 | `fs-read` | 文件系统 | `list<u8>` |
| 13 | `fs-write` | 文件系统 | `u32` |
| 14 | `fs-pread` | 文件系统 | `list<u8>` |
| 15 | `fs-pwrite` | 文件系统 | unit |
| 16 | `fs-lseek` | 文件系统 | `u64` |
| 17 | `fs-fstat` | 文件系统 | `UniFsStat` |
| 18 | `fs-stat` | 文件系统 | `UniFsStat` |
| 19 | `fs-fsync` | 文件系统 | unit |
| 20 | `fs-readdir` | 文件系统 | `list<UniFsDirent>` |
| 21 | `relation-get` | Relation | `option<list<option<list<u8>>>>` |
| 22 | `relation-update` | Relation | `u64`（受影响行数） |
| 23 | `relation-insert` | Relation | unit |

各类型的请求形状在[契约文档](../contract/syscall_payload_v1.md#消息类型)中有完整表格。

**例外：`fetch`。** `fetch(query-result: list<u8>) -> list<u8>` 在运行时 world
（`mudu_runtime/wit/api.wit`）中声明为宿主函数，直接收发原始 `rmp_serde` 字节。它**不**
经过 MSSP 帧封装；见下文 [`fetch` 协议](#fetch-协议)。

## `fetch` 协议

`fetch` 已在宿主侧实现（`mudu_runtime::interface::kernel_sync::fetch_internal`），并镜像
[`sys_interface::api_impl`](../../../crates/common/sys_interface/src/api_impl/mod.rs)——
后者仍是线形状的权威来源。线格式为不带 MSSP 头部的原始 `rmp_serde`：

- **请求：** `UniOid` 的 `rmp_serde` 序列化——即 query 响应中 `UniResultSet.cursor`
  携带的字节。
- **响应：** `UniResult<UniResultSet, UniError>` 的 `rmp_serde` 序列化。WIT ABI 返回裸
  `list<u8>`、没有错误通道，因此所有失败都编码进 `UniResult::Err` 信封。

语义：

- `fetch` 排空该游标结果集的**全部**缓存行，并始终报告 `eof: true`。
- 结果集排空后再 `fetch` 返回空的 `row_set` 且 `eof: true`——不是错误。
- 未知游标返回 `UniResult::Err`（`ErrorCode::EntityNotFound`）；畸形游标返回
  `UniResult::Err`。
- 同步连接上的 query 会把结果集缓存到会话 `Context` 上，供 `fetch` 之后排空；异步连接上的
  query 从不缓存，因此异步 query 之后的 `fetch` 返回空 `row_set` 且 `eof: true`。
- 缓存生命周期：缓存被析构式排空；`Context::query_next` 在 EOF 时清除缓存；会话关闭时
  `Context::remove` 丢弃未排空的缓存。

## 编解码器生成

全部七条编解码路径都由 **mgen 从 WIT 模式生成**——
[`mudu_binding/wit/`](../../../crates/common/mudu_binding/wit/) 下的 WIT 文件是唯一权威
来源。项目有意**不**新建共享的 `mudu_syscall_codec` crate：每个消费方（宿主
`mudu_binding`、独立 Rust SDK、C# SDK、AssemblyScript 包、Python 包、C 绑定、Go 绑定）
各自持有自己的
生成代码，漂移
由下面两套 golden 语料把守，而不是靠共享依赖。这样独立 SDK 与 guest 包不必依赖宿主
crate，同时靠构造（同一份 WIT、同一个生成器）加逐字节验证保证字节级一致。

### 重新生成命令

在 `crates/tools` 下执行（Rust——宿主 `mudu_binding` 与独立 Rust SDK 使用同一条命令）：

```sh
cargo run -p mudu_gen -- message \
    -i ../common/mudu_binding/wit \
    -o <scratch dir> \
    -l rust --with-func-codec          # 或：-l csharp / -l assemblyscript / -l python / -l c / -l go
```

`mgen message` 另支持 `-t/--type-desc <path>`：在生成代码的同时额外输出一份 schema
描述符 JSON（`UniSchemaDesc`——描述全部消息类型、record 字段号与 variant tag，
供运行期反射与工具消费）。描述符的落地副本是
[`mudu_binding/src/codec/syscall_payload/syscall_schema.desc.json`](../../../crates/common/mudu_binding/src/codec/syscall_payload/syscall_schema.desc.json)，
由 `syscall_payload::SYSCALL_SCHEMA_DESC_JSON` 以 `include_str!` 内嵌；宿主侧重新
生成命令（含 `-t`）见
[`mudu_binding/src/codec/syscall_payload/REGENERATE.md`](../../../crates/common/mudu_binding/src/codec/syscall_payload/REGENERATE.md)。

- **Rust（宿主）：** 把 `uni_syscall.rs` 拷到
  `mudu_binding/src/codec/syscall_payload/generated/`，把 `uni_<name>.rs` DTO 拷到
  `mudu_binding/src/universal/`；详见
  [`mudu_binding/src/codec/syscall_payload/REGENERATE.md`](../../../crates/common/mudu_binding/src/codec/syscall_payload/REGENERATE.md)。
- **Rust（SDK）：** 把 `uni_syscall.rs` 拷到
  `mudu_api/rust/src/mudu_sys/generated/`，DTO 拷到 `mudu_api/rust/src/universal/`，
  然后运行 `cargo fmt`；详见
  [`mudu_api/rust/REGENERATE.md`](../../../crates/sdk/mudu_api/rust/REGENERATE.md)。
- **C#：** 把 `Uni*.cs` 拷到 `mudu_api/csharp/uni/`；`UniSyscall.cs` 装入
  `mudu_api/csharp/mudu_sys/` 并把命名空间从 `Universal` 改写为 `Mudu.Api.MuduSys`
  （一条 `sed` 命令）；确切步骤见
  [`mudu_api/csharp/README.md`](../../../crates/sdk/mudu_api/csharp/README.md) 的 regen
  小节。
- **AssemblyScript：** 把所有 `*.ts` 拷到
  `bindings/assemblyscript/assembly/generated/`，然后用
  `sed -i 's|from "./mpack"|from "../mpack"|'` 改写运行时导入；详见
  [`assembly/generated/README.md`](../../../crates/sdk/bindings/assemblyscript/assembly/generated/README.md)。
- **Python：** 把所有 `*.py` 拷到 `bindings/python/mududb/generated/`（模板已直接发出
  `from mududb.codec.mpack import ...`，无需改写导入），并在 schema 变化时从宿主路径刷新
  `mududb/generated/syscall_schema.desc.json`；详见
  [`bindings/python/README.md`](../../../crates/sdk/bindings/python/README.md)。
- **C：** 把 `Uni*.h` 拷到 `bindings/c/mududb/types/`（生成的
  `#include "mududb/..."` 路径与签入布局逐字节一致，无需改写；`mududb/codec/`
  为手写层）；详见
  [`bindings/c/README.md`](../../../crates/sdk/bindings/c/README.md)。
- **Go：** 把 `Uni*.go` 拷到 `bindings/go/types/`（`types/wire.go` 为手写——切勿
  覆盖）；详见
  [`bindings/go/README.md`](../../../crates/sdk/bindings/go/README.md)。

### WIT 钉死规则

WIT 模式的下列属性是已部署的线值——不要轻易改动：

- `uni-syscall.wit` 的声明顺序**即**线上 `message_kind` 编号（1–23）：`relation-*`
  函数跟在 `fs-*` 函数之后。
- `uni-scalar.wit` 的判别值已钉死：`u128` 跟在 `u64` 之后（标签 8），`i128` 跟在
  `i64` 之后（标签 10），`string` 为 14，其最后一个 case 是 `timestamp-tz`
  （`TimestampTz`，标签 20）。`uni-scalar-value.wit` 对这些 case 使用相同编号，并
  额外地以 `null`（`Null`，标签 21）结尾——无负载的 SQL NULL 值，编码为
  `[21, 0u8]`。C# 侧用
  `Mudu.Api.Tests/UniScalarTagTests.cs` 钉住这些值（针对落地过程中发现的一个标签错位
  bug 的回归测试）。
- `uni-query-argv.wit` 与 `uni-command-argv.wit` 以
  `param-desc: option<uni-record-type>`（record 映射键 4）结尾：发送方的参数类型
  描述符。WIT option 字段在**没有值时从编码映射中省略**（proto3 存在语义），因此
  不带描述符的帧与新增字段之前的字节完全相同；接收方把缺失的键视为"无描述符"并
  跳过描述符/值一致性检查。
- `uni-sql-param.wit` 以 `param-names: option<list<string>>`（record 映射键 2）
  结尾：命名参数（`:name`）的名称列表，与值按声明顺序并行。宿主侧把 `:name` 改写为
  位置形式并按出现顺序展开值（重名复用同一份值）；缺失时同样被省略，不影响字节。
- `uni-data-type.wit` 以 `%box(box<uni-data-type>)` 结尾（标签 8，在标签 7 的
  `binary` 之后）。`Box` 变体同时充当 `mgen` 内部表示 WIT `box<T>` 类型的 IR，因此
  尽管没有任何 syscall 负载携带它，也必须保留。
- 旧的校验和文件 `mudu_binding/wit/contract.md5.txt` 早于这些修正，内容已过期，且不再
  被任何构建任务引用。

### 验证门禁

任何重新生成都必须通过两套语料的验证（见[互操作验证](#互操作验证)与
[MP 原语对齐](#mp-原语对齐)；前者为语义 + roundtrip，后者为逐字节）：

1. 47 帧 syscall 语料 `syscall_payload_v1_all.bin`——由宿主路由器、Rust SDK、C# SDK、
   AssemblyScript 生成的编解码器（172 项检查）以及 Python 生成的编解码器
   （188 项检查）验证。
2. 44 向量的原语语料 `mp_primitives_v1.bin|.json`——由 `rmp_serde`、
   MessagePack-CSharp、`mpack.ts` 与 `mpack.py` 验证。

如果重新生成的代码改动了任何帧字节，应修正 WIT 或 `mgen` 模板——绝不要"修" fixture。

## 各语言集成

### Workspace Rust guest

无需任何工作。基于仓库内 crate 编写的过程经 `sys_interface` → `mudu_binding`，其
universal 类型与按函数编解码器均为 mgen 生成
（`codec/syscall_payload/generated/uni_syscall.rs`），`mod.rs`/`router.rs` 只是薄的
`MuduError` 适配层。全部 23 种类型自动覆盖。

### 独立 Rust SDK（`mudu_api_rust`）

[`crates/sdk/mudu_api/rust`](../../../crates/sdk/mudu_api/rust/) 是一个独立的 cargo
workspace（包名 `mudu_api_rust`），面向在主仓库之外构建的 guest。它不能依赖
`mudu_binding`，因此自带同一份 mgen 输出——`src/universal/` 中的 universal DTO 加上
`src/mudu_sys/generated/uni_syscall.rs` 按函数编解码器——外围是手写胶水：

- `src/mudu_sys/batch.rs`、`session.rs`、`kv.rs`、`relation.rs`、`fs.rs` ——
  按类别划分的公开 `serialize_*` / `deserialize_*_result` API 与 `map_fs_errno`，
  内部走生成的编解码器。
- 其上提供类型化的异步 `Mudu` 封装：`open_session`、`get`、`put`、`delete`、
  `range`、`relation_get`、`relation_update`、`relation_insert`、`fs_*`，以及 SQL
  调用。
- `--features mock-sqlite` 构建进程内 mock，带内存 KV/relation/fs 模拟，无需运行中的
  `mudud` 即可在宿主上对过程逻辑做单元测试。

### C# SDK

[`crates/sdk/mudu_api/csharp`](../../../crates/sdk/mudu_api/csharp/)：

- `uni/*.cs` —— 22 个 mgen 生成的 universal DTO。
- `mudu_sys/UniSyscall.cs` —— mgen 生成的 MSSP 帧编解码器与按函数 stub（命名空间改写为
  `Mudu.Api.MuduSys`）。默认路径 resolver-free，可运行于 wasi-wasm NativeAOT 目标；
  标量标签由 `Mudu.Api.Tests/UniScalarTagTests.cs` 与宿主钉死。
- `mudu_sys/MuduSysCallApi.cs` —— 手写的类型化系统调用接口，覆盖全部 23 种类型：
  `SysBatch`、`SysOpen`、`SysClose`、`SysGet`、`SysPut`、`SysDelete`、`SysRange`、
  `SysRelationGet`、`SysRelationUpdate`、`SysRelationInsert`，以及既有的
  query/command/fs 调用。
- `mock/MockSqliteMuduSysCall.cs` 搭配 `MockKvEmulation` / `MockRelationEmulation` /
  `MockFsEmulation`，用于宿主侧单元测试。
- xunit 测试位于 `Mudu.Api.Tests/`：

```sh
~/.dotnet/dotnet test Mudu.Api.Tests
```

**真实 C# guest 已验证。** [`crates/sdk/example/wallet-cs`](../../../crates/sdk/example/wallet-cs/)
是 wallet 示例的 byte-pipe C# 移植（5 个过程，断言与 `wallet-as` 完全对齐），用
componentize-dotnet 构建（.NET 10 SDK、`dotnet-experimental` feed、
`IlcExportUnmanagedEntrypoints`、`wit/deps` 布局）并打包为 `wallet-cs.mpk`。它在真实
`mudud` 上通过
[`testing/tests/wallet_cs_mpk.rs`](../../../crates/db-kernel/testing/tests/wallet_cs_mpk.rs)。
构建要求与确切工具链见
[wallet-cs readme](../../../crates/sdk/example/wallet-cs/readme.md)。运行时备注：
sync-world 组件运行在专用的无运行时线程上（`WTInstancePre.requires_async` 由组件的
导入计算得出；导入异步 API 的组件仍走 `call_async` 路径），TCP 调用器按应用的
`enable_async`/`use_async` 路由。

### AssemblyScript

AssemblyScript guest 与 C# guest 采用相同的直连 byte-pipe 架构：组件直接 import
`mududb:api/system`，并用 mgen 生成、经语料逐字节验证的 MSSP 编解码器
（[`bindings/assemblyscript/assembly/generated/`](../../../crates/sdk/bindings/assemblyscript/assembly/generated/)，
24 个文件，运行时为 `assembly/mpack.ts`）自行成帧。`mtp assembly-script` 为每个过程生成
`mp2_<name>` byte-pipe 适配器（解码 `UniProcedureParam` → 位置类型化实参 → 调用用户函数 →
编码 `UniResult<UniProcedureResult, UniError>`）以及过程 world WIT（`import
mududb:api/system`，每个过程一个根级 `mp2-<kebab>` 导出），不再需要任何伴生 Rust 组件
（见
[`crates/sdk/example/wallet-as/Makefile.toml`](../../../crates/sdk/example/wallet-as/Makefile.toml)）：

```text
AssemblyScript 组件（mtp 适配器 + 生成的 MSSP 编解码器）
        |
        |  import mududb:api/system，导出根级 mp2-*
        v
wasm-tools component embed/new  ->  wallet-as 组件（.mpk）
```

端到端路径已由
[`testing/tests/wallet_as_mpk.rs`](../../../crates/db-kernel/testing/tests/wallet_as_mpk.rs)
证明：该测试把 `wallet-as.mpk` 安装到真实的 `mudud`（Tokio 模式）上，并通过 HTTP 调用
`create_user` / `deposit` / `withdraw` / `transfer_funds` / `balance`——包括经 MSSP
返回给调用方的透支错误。

### Python（工具/测试验证）

[`crates/sdk/bindings/python`](../../../crates/sdk/bindings/python/) 是 AssemblyScript
`assembly/generated/` 的 Python 对应物：它的角色是**工具/测试验证**，不是 guest 路径。
纯 stdlib、Python 3.9+、零安装（每个测试入口自行引导 `sys.path`）。包布局：

- `mududb/codec/mpack.py` —— 手写 MessagePack 运行时，与 rmp_serde 1.3.x 的 canonical 编码
  逐字节一致（按值取最小整数宽度、str8/bin8 阈值），宽松解码；`F32` marker 类使写入端
  发出 f32（`0xCA`）而非 f64（`0xCB`）。
- `mududb/generated/` —— mgen Python 后端输出（`-l python --with-func-codec`，24 个文件，
  含 `uni_syscall.py` 帧编解码器）外加 `syscall_schema.desc.json` 副本。
- `tests/test_mpack.py` —— `mududb.codec.mpack` 的 unittest（36 个用例）。
- `corpus_common.py` + 三个语料 runner——消费与 Rust/C#/AS 相同的宿主 golden
  fixture（见[互操作验证](#互操作验证)与[MP 原语对齐](#mp-原语对齐)）。

重新生成（在 `crates/tools` 下，然后把 `<scratch dir>/*.py` 拷入 `generated/`）：

```sh
cargo run -p mudu_gen -- message \
    -i ../common/mudu_binding/wit \
    -o <scratch dir> \
    -l python --with-func-codec
```

测试命令（在 `crates/sdk/bindings/python` 下）：

```sh
python3 -m unittest discover tests   # mpack 单元测试
python3 run_mp_corpus_test.py        # 44 个原语向量（132 项检查）
python3 run_syscall_corpus_test.py   # 47 帧 MSSP 语料（188 项检查）
python3 run_lenient_decode_test.py   # 8 个非 canonical 帧（16 项检查）
```

### C（语料验证）

[`crates/sdk/bindings/c`](../../../crates/sdk/bindings/c/) 是 Python 绑定的 C 对应物：
角色是**语料验证**，同时作为 C guest 的参考实现。C99/C++17 双语洁净，除 libc 外
零依赖。布局：

- `mududb/codec/mpack.{h,c}` —— 手写 MessagePack 运行时（canonical 写入端、宽松读取端、
  sticky 错误标志、`mp_arena` bump 分配器：解码值绝不指向输入缓冲；一次 `mpa_free`
  释放全部）。
- `mududb/types/Uni*.h` —— mgen C 后端输出（`-l c --with-func-codec`，23 个自包含
  头文件，含 `UniSyscall.h` 帧编解码器）。跨文件引用环由前置声明、具名 record/variant
  payload 一律指针化、两相（early/late）include 打破；`MP_INLINE` 宏使头文件在 C99
  与 C++ 下都保持洁净。
- `tests/corpus_driver.c` —— 原生语料 runner（无需 WASM）：与 Python runner 相同的
  三套断言，共 398 项检查，由 `make check` 构建并运行（同时承担 C99/C++17 逐头
  编译门禁）。

重新生成（在 `crates/tools` 下，然后把 `<scratch dir>/Uni*.h` 拷入 `mududb/types/`）：

```sh
cargo run -p mudu_gen -- message \
    -i ../common/mudu_binding/wit \
    -o <scratch dir> \
    -l c --with-func-codec
```

测试命令（在 `crates/sdk/bindings/c` 下）：

```sh
make check CC=clang CXX=clang++   # 头文件编译门禁 + 语料 driver（398 项检查）
```

### Go（语料验证）

[`crates/sdk/bindings/go`](../../../crates/sdk/bindings/go/) 是 Go 对应物：**语料验证**，
同时是 Go guest 引用的绑定（模块 `github.com/ybbh/mududb_p/bindings/go`）。纯 stdlib，
兼容 TinyGo（无 `unsafe`、无泛型、无反射）。布局：

- `codec/mpack.go` —— 手写 canonical MessagePack 写入端 / 宽松读取端，基于原生值模型
  （`nil`/`bool`/整数族/`float64`/`string`/`[]byte`/`[]any`/`map[uint64]any`），与
  rmp_serde 逐字节一致（最小宽度整数、32..=255 用 str8、map 键升序）。
- `codec/frame.go` —— 手写 MSSP v1 16 字节头 + 请求/结果消息体助手。
- `types/` —— mgen Go 后端输出（`-l go --with-func-codec`，23 个 `Uni*.go`）外加手写的
  `wire.go` 受检线格式转换（`types/` 中唯一无 "Generated" 头的文件）。
- `corpus/` —— 以 `go test` 回放三套 golden 语料的测试包。

重新生成（在 `crates/tools` 下，然后把 `<scratch dir>/Uni*.go` 拷入 `types/`）：

```sh
cargo run -p mudu_gen -- message \
    -i ../common/mudu_binding/wit \
    -o <scratch dir> \
    -l go --with-func-codec
```

测试命令（在 `crates/sdk/bindings/go` 下）：

```sh
go build ./... && go vet ./... && test -z "$(gofmt -l .)"
go test ./...   # 44 个原语向量（132 项检查）、47 帧 MSSP（188 项检查）、8 个宽松帧（16 项检查）
```

### 接入一门新语言

新 guest 语言实现的检查清单（本清单覆盖线格式侧；工具链侧——`mpm-crate`
模板、`mtp` 前端、`mgen` 后端、golden 语料与 e2e 接线——见
[`dev/new_guest_language.md`](../../dev/new_guest_language.md)，英文）：

1. **帧头部。** 16 字节，全部大端：魔数 `0x4D535350`、版本 `1`、flags `0`、
   消息类型 `u32`。解码时拒绝：错误魔数、非 `1` 的版本、非零 flags、未知或 `0`
   的类型。
2. **请求体。** 参数号（1-based）为键的 MessagePack map。
   record 是字段号（1-based）为键的 MessagePack map；variant 是 `[tag, payload]`；
   `option<T>` 为 nil 或对应值。解码宽松（未知键跳过、缺失字段默认、整数宽度任意）。
3. **结果体。** `[0u8, value]` / `[1u8, UniError]`。
4. **blob。** func 级 `list<u8>` 使用 MessagePack bin——但 record 上下文（含
   `UniError.err_details` 与 `UniResultSet.cursor`）一律为整数**数组**。
5. **整数紧致编码。** 生成并接受最短的 MessagePack 整数形式（`rmp_serde` 语义）：
   小值用 fixint（`-2` → `0xFE`），短 blob 用 bin8 `0xC4`。
6. **帧级严格解码。** 拒绝消息体后的尾部字节、畸形 MessagePack、未知 variant 标签、
   不是 map 的 record/请求体、收窄溢出、以及字符串中的非法 UTF-8。（map 内部保持宽松：
   未知键跳过、缺失字段默认。）
7. **错误表面。** 忠实映射 `UniError`，包括无来源宿主错误的 `err_src` 为 JSON 字符串
   `"None"`，以及 fs errno 映射（`50029` → `EINVAL 22`）。
8. **验证。** 在声称兼容之前，消费两份 JSON sidecar 完成验证：
   `syscall_payload_v1_all.json`（47 帧语义期望——decode → 字段级语义断言 →
   re-encode → 再 decode 语义相等；request 帧额外保持 re-encode 字节相等，钉死
   canonical encoder）与 `lenient_decode_v1.json`（8 个宽松解码向量——非
   canonical 字节形态按 sidecar 的 `expect` 重放同一组宽松规则）。详见
   [互操作验证](#互操作验证)与[宽松解码向量](#宽松解码向量)。

## 互操作验证

所有实现都以单一宿主生成的 golden 语料为基准：
[`testing/fixtures/golden/v1/syscall_payload_v1_all.bin`](../../../crates/db-kernel/testing/fixtures/golden/v1/syscall_payload_v1_all.bin)
及其 JSON sidecar `syscall_payload_v1_all.json`。

- **内容：** 47 帧——类型 1–23，按判别值顺序各一对请求 + 成功响应，外加一帧
  `UniError` get 响应。
- **容器格式：** 大端 `u32` 长度前缀 MSSP 帧的顺序拼接。
- **断言方式：** 语料以**语义断言 + roundtrip** 钉住，而非整文件字节锚——
  逐帧 decode → 字段级语义断言 → re-encode → 再 decode → 语义相等；request 帧额外
  保持 re-encode 字节相等（继续钉死 canonical encoder）；`UniError` 帧例外，只做
  字段级断言（解码后的错误带新的 caller location，无法字节还原）。
- **sidecar：** `syscall_payload_v1_all.json` 与 mp_primitives 的 sidecar 同型
  （format/reference/container/regenerate 顶层字段），`frames` 数组按段给出
  `(index, message_kind, message_kind_name, direction, expect)`；`expect` 用
  JSON 描述解码后的关键字段值（u64/i64/u128 为十进制字符串、字节串为小写 hex、
  UniOid 为 `{h, l}`、unit 结果为 `{"unit": true}`），使 C# / AssemblyScript /
  Python 消费方无需 Rust 工具链即可重建每帧期望。帧字节与 sidecar 期望派生自同一份构造
  代码（`golden_syscall_all_cases`），另有交叉验证测试
  `syscall_all_sidecar_matches_bin` 保证两者不漂移。
- **由七个独立编解码器验证：**

  | 实现 | 测试 |
  |------|------|
  | 宿主路由器 | `testing/tests/compat_golden.rs::golden_v1_all_kinds_roundtrip` |
  | 独立 Rust SDK | `mudu_api/rust/tests/golden_frames_test.rs` |
  | C# SDK | `Mudu.Api.Tests/GoldenFrameCorpusTests.cs` |
  | AssemblyScript 生成的编解码器 | `bindings/assemblyscript/run_syscall_corpus_test.mjs`（172 项检查） |
  | Python 生成的编解码器 | `bindings/python/run_syscall_corpus_test.py`（188 项检查） |
  | C 生成的编解码器 | `bindings/c/tests/corpus_driver.c`（原生；三套语料合计 398 项检查） |
  | Go 生成的编解码器 | `bindings/go/corpus/syscall_corpus_test.go`（188 项检查） |

- **宿主编解码器为权威实现**；出现任何分歧以宿主为准。
- 重新生成（宿主侧，同时写出 `.bin` 与 `.json` fixture 文件）：

  ```sh
  cargo test -p testing --test compat_golden -- generate_golden_v1_fixtures --ignored
  ```

七个实现之间未发现任何差异。

### 宽松解码向量

语料目录另有一对 fixture 固定 MSSP v1 解码器的**宽松**一侧：
`lenient_decode_v1.bin` 及其 sidecar `lenient_decode_v1.json`。每个向量是一帧
手工构造的合法但**非 canonical** 的 MSSP 帧（canonical encoder 编不出的字节形态），
附期望解码语义：

| kind | 内容 |
|------|------|
| `int-width-widening` | u64 值 2 用 0xCE（u32 宽度）编码 |
| `int-unsigned-marker` | 非负 i64 用无符号 u8 marker（0xCC）编码 |
| `int-wide-negative` | 小负整数 -2 用 0xD3（i64 宽度）编码 |
| `record-key-order` | record map 键乱序（4,3,2,1） |
| `record-unknown-field` | record 含未知字段号键（跳过） |
| `map-noninteger-key` | map 键为字符串（跳过） |
| `request-missing-param` | request map 缺参数（proto3 默认值） |
| `record-missing-fields` | record 缺字段（默认值，含 variant 字段默认 case） |

sidecar 按段列出 `(index, kind, message_kind, message_kind_name, direction, note,
expect)`；Rust 侧由 `compat_golden.rs::lenient_decode_v1_vectors` 逐向量验证，
C# / AS / Python / C / Go 消费方应按 sidecar 的 `expect` 重放同一组宽松规则
（Python 侧由 `bindings/python/run_lenient_decode_test.py` 实现，16 项检查；C、Go
分别在 `bindings/c/tests/corpus_driver.c` 与 `bindings/go/corpus/lenient_decode_test.go`
中重放）。与上面的 canonical 语料由同一条生成命令一并重新生成。

## MP 原语对齐

上述帧级语料之外，还有一个原语级语料，用于固定 rmp_serde 1.3.1 的编码规则——
每个 guest MessagePack 运行时都必须逐字节复现这些规则：
[`testing/fixtures/golden/v1/mp_primitives_v1.bin`](../../../crates/db-kernel/testing/fixtures/golden/v1/mp_primitives_v1.bin)
及其 JSON sidecar `mp_primitives_v1.json`。

- **内容：** 44 个单值向量——u64 边界（0、1、127、128、255、256、65535、65536、
  2^32-1、2^32、2^64-1）、i64 边界（1、127、128、-1、-32、-33、-128、-129、
  -32768、-32769、-2^31、-2^31-1、i64::MIN）、f32（1.5、0.1）、f64（1.5、-π）、
  nil、两个 bool、字符串（长度 0/31/32/255/256，外加多字节 `héllo世界`）、
  bin（长度 0/255/256）、数组（长度 0/15/16），以及一个嵌套的
  `[u64, str, bin]` 组合。
- **容器格式：** 与 syscall 语料相同的大端 `u32` 长度前缀段；sidecar 按段列出
  `(index, kind, value|len)`，使非 Rust 消费方无需 Rust 工具链即可重建每个期望值。
- **断言方式：** 原语编码规则未变，本语料保持**整文件字节锚**——
  `mp_primitives_v1_roundtrip` 断言提交的 `.bin` 与当前 rmp_serde 编码器逐字节一致
  （sidecar 文本同样逐字节钉死），与 syscall 语料的"语义 + roundtrip"风格不同。
- **sidecar 清单：** 语料目录现有的 JSON sidecar 共三份——`mp_primitives_v1.json`
  （原语向量）、`syscall_payload_v1_all.json`（47 帧语义期望）、
  `lenient_decode_v1.json`（宽松解码向量），均由宿主生成、五语言共享。

### 固定的编码规则

| 值 | 编码 |
|----|------|
| u64 | 按**值**取最小宽度：≤ 127 用 fixint，≤ 255 用 `0xCC`，≤ 65535 用 `0xCD`，≤ 2^32-1 用 `0xCE`，更大用 `0xCF` |
| i64 ≥ 0 | 走**无符号**标记链（`128i64` → `0xCC 0x80`） |
| i64 < 0 | -1..-32 用 negfixint，之后依次 `0xD0` / `0xD1` / `0xD2` / `0xD3` |
| f32 / f64 | `0xCA` / `0xCB`，大端 IEEE-754 |
| str | ≤ 31 用 fixstr，32..=255 用 str8 `0xD9`，256..=65535 用 str16 `0xDA`，更大用 str32 `0xDB`——rmp_serde **确实**会发出 str8 |
| bin | ≤ 255 用 bin8 `0xC4`，之后 bin16 `0xC5`、bin32 `0xC6` |
| array | ≤ 15 用 fixarray，≤ 65535 用 array16 `0xDC`，更大用 array32 `0xDD` |

整数宽度按**值**而非源类型决定：rmp_serde 把 `2u8`、`2u16`、`2u32`、`2u64`
都编码为单个 fixint `0x02`。

### 解码宽严规则

- f32/f64 在 `0xCA`/`0xCB` 两个标记之间**双向宽松**解码
  （`from_slice::<f32>` 接受 f64 编码，反之亦然）。
- 读取 u64 时**拒绝**负值编码（对 negfixint 调用 `from_slice::<u64>` 会报错）。
- 注意：包裹 blob 的 Rust newtype 结构体（`struct W(Vec<u8>)`）在 rmp_serde 中
  被**透明**序列化——不带数组包装。因此原语语料刻意不包含 newtype 包装；
  其线上形态由 syscall 语料固定。

### 原语语料验证

- **由六个独立运行时逐字节验证：**

  | 实现 | 测试 |
  |------|------|
  | 宿主（rmp_serde 1.3.1） | `testing/tests/compat_golden.rs::mp_primitives_v1_roundtrip` |
  | C#（MessagePack-CSharp 3.1.4） | `Mudu.Api.Tests/MpPrimitiveCorpusTests.cs` |
  | AssemblyScript（`mpack.ts`） | `bindings/assemblyscript/run_mp_corpus_test.mjs` |
  | Python（`mududb/codec/mpack.py`） | `bindings/python/run_mp_corpus_test.py` |
  | C（`mududb/codec/mpack.c`） | `bindings/c/tests/corpus_driver.c` |
  | Go（`codec/mpack.go`） | `bindings/go/corpus/mp_corpus_test.go` |

- **rmp_serde 为权威实现**；出现任何分歧以语料为准，修改 guest 运行时，
  绝不修改 fixture。
- 重新生成（宿主侧，写出两个 fixture 文件）：

  ```sh
  cargo test -p testing --test compat_golden -- generate_mp_primitives_v1 --ignored
  ```

- 各语言验证命令：

  ```sh
  cargo test -p testing --test compat_golden   # Rust，在 crates/db-kernel 下
  dotnet test Mudu.Api.Tests                   # C#，在 crates/sdk/mudu_api/csharp 下
  node run_mp_corpus_test.mjs                  # AS，在 crates/sdk/bindings/assemblyscript 下
  python3 run_mp_corpus_test.py                # Python，在 crates/sdk/bindings/python 下
  make check                                   # C，在 crates/sdk/bindings/c 下
  go test ./...                                # Go，在 crates/sdk/bindings/go 下
  ```

六个实现之间未发现任何差异：MessagePack-CSharp 的整数写入按值取最小宽度，
且会发出 str8，与 rmp_serde 在全部 44 个向量上完全一致。

## 版本策略

- 运行时**只解码当前版本**（`1`）。其他版本一律以
  `ErrorCode::UnsupportedFormatVersion` 快速失败。
- 每个 MPK 包都钉入同一版本：`package.manifest.json` 携带 `syscall_abi_version: u32`。
  `mpm_build` 总是写入 `SYSCALL_PAYLOAD_CURRENT_VERSION`（`1`）；加载器
  （`mudu_runtime/src/service/app_package.rs`）对缺失字段默认取 `1`，并在版本不匹配时
  以 `ErrorCode::UnsupportedFormatVersion` 拒绝（"unsupported syscall ABI version
  {found}, expected {expected}"）。`FormatKind::SyscallPayload` 已注册进全局
  `CompatibilityRouter`（`mudu_kernel/src/compat.rs`），v1 恒等迁移处理器位于
  `mudu_binding::codec::syscall_payload::migrate`；迁移单元是完整帧。
- **不提供旧版 MPK 兼容**：包随 ABI 版本重新构建。版本字段的存在是为了对未来升级
  做前向控制，而非跨版本互操作。
- 废弃 v1 的前提是所有 guest 绑定（Rust、C#、AssemblyScript、Python）都能发出并接受后继版本；
  完整的升级与废弃规则见[契约文档](../contract/syscall_payload_v1.md)。

## 参考

- 契约：[doc/cn/contract/syscall_payload_v1.md](../contract/syscall_payload_v1.md)
- WIT 系统调用接口：[uni-syscall.wit](../../../crates/common/mudu_binding/wit/uni-syscall.wit)
- 运行时 world：[mudu_runtime/wit/api.wit](../../../crates/db-kernel/mudu_runtime/wit/api.wit)
- 宿主编解码器：[mudu_binding/src/codec/syscall_payload/](../../../crates/common/mudu_binding/src/codec/syscall_payload/)
  （重新生成见 [`REGENERATE.md`](../../../crates/common/mudu_binding/src/codec/syscall_payload/REGENERATE.md)）
- 兼容性注册表：[mudu/src/compat/mod.rs](../../../crates/common/mudu/src/compat/mod.rs)
- 独立 Rust SDK：[crates/sdk/mudu_api/rust/](../../../crates/sdk/mudu_api/rust/)
  （重新生成见 crate 根目录 `REGENERATE.md`）
- C# SDK：[crates/sdk/mudu_api/csharp/](../../../crates/sdk/mudu_api/csharp/)
  （重新生成见 crate `README.md`）
- AssemblyScript 生成的编解码器：[crates/sdk/bindings/assemblyscript/assembly/generated/](../../../crates/sdk/bindings/assemblyscript/assembly/generated/)
- Python 生成的编解码器（工具/测试验证）：[crates/sdk/bindings/python/](../../../crates/sdk/bindings/python/)
- wallet-as 直连 byte-pipe 构建：[crates/sdk/example/wallet-as/Makefile.toml](../../../crates/sdk/example/wallet-as/Makefile.toml)
- wallet-cs（C# 组件 guest）：[crates/sdk/example/wallet-cs/](../../../crates/sdk/example/wallet-cs/)
- Golden 语料：[testing/fixtures/golden/v1/syscall_payload_v1_all.bin](../../../crates/db-kernel/testing/fixtures/golden/v1/syscall_payload_v1_all.bin)
- MP 原语语料：[testing/fixtures/golden/v1/mp_primitives_v1.bin](../../../crates/db-kernel/testing/fixtures/golden/v1/mp_primitives_v1.bin)（含 `mp_primitives_v1.json` sidecar）
