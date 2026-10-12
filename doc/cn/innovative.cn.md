# MuduDB 的执行模型与工具链

MuduDB 将一组查询、计算和更新封装为过程，在数据库内执行。这样可以减少多步操作中的客户端往返；实际收益取决于数据量、过程开销和事务竞争，应以具体工作负载的测试为准。

## 过程与运行方式

过程编译为 WebAssembly 组件，并通过系统调用访问宿主数据库。应用也可以使用原生 adapter 在数据库进程外运行；两种方式的入口和依赖不同。语言支持及示例见 [SDK 能力表](../dev/sdk_distribution.md#language-and-execution-modes)。

[过程开发指南](procedure.cn.md)以 Rust facade 为例说明查询、参数绑定和事务。可复用的第三方库必须支持所选编译目标及运行环境，不能假设所有原生库都能用于 Wasm guest。

## 类型与 SQL 检查

`Entity` trait 将查询结果映射为 Rust 结构体。`mgen` 根据 DDL 生成实体代码，`mgen check-sql` 在构建时对照 DDL 检查支持范围内的 SQL。

这些检查能发现部分类型和查询错误。业务规则、权限、事务冲突和实际返回结果仍需测试。SQL 支持范围见 [SQL subset](../dev/sql_subset.md)。

## 打包与安装

`.mpk` 包包含 WebAssembly 模块、DDL、描述符及可选初始数据。`mcli app-install` 将包安装到运行中的服务端；安装条件和完整调用示例见[第一个 MPK](your_first_mpk.cn.md)。

## 内核与运行时

内核负责存储、事务和调度，运行时承载 WebAssembly 组件。按核 worker 与 Linux `io_uring` 路径用于减少线程间共享和 I/O 等待；设计及约束见[运行时架构](modern_hardware.cn.md)。

## AI 辅助开发

仓库中的 [AI 工具](../../ai_tool/)用于辅助生成设计和代码。生成结果仍需通过与手写代码相同的编译、SQL 检查和测试。使用这些工具不构成对代码正确性的保证，也不是运行 MuduDB 的前提。
