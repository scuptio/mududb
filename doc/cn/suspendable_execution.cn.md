# 可挂起的过程执行：并发模型

MuduDB 以 WebAssembly 组件的形式执行存储过程。过程在遇到异步 I/O 时如何挂起、
以及一个 OS 线程上可以同时有多少个过程在飞，取决于包构建时所用的组件 world。
本页描述两种模型及其约束。

## 同步 world（旧模型）：每调用一线程，阻塞执行

针对同步 world（`wit/api.wit`）构建的包调用**同步** host 函数。该路径上没有
挂起机制：`call` 为每次调用启动一个专用 OS 线程，线程内运行一个 current-thread
运行时，租出池化实例后用 `block_on` 把调用执行到完成，调用方线程 join 等待结果。

因此同步路径上的并发只能来自在多个 OS 线程上同时运行调用——每个在飞调用
占一个阻塞线程。单个线程内部没有交错：遇到异步 I/O 的 host 调用只是阻塞该
线程直到 I/O 完成。

## 异步 world（当前默认）：wasmtime 管理的 guest 纤程

针对异步 world（`wit/async-api.wit`，当前所有 `.mpk` fixture）构建的包导入
**async-lowered** 的 host 函数。引擎配置 `wasm_component_model_async` +
`more_async_builtins`（`enable_async`），调用走 `call_async`：

- wasmtime 运行每个 `Store` 的组件事件循环。当 guest await 一个异步 host 调用
  且没有其他可推进的工作时，`call_async` future 向嵌入方返回 `Poll::Pending`，
  被挂起的调用状态从线程上 detach——线程随即可以运行其他 store。
- **N 个组件实例可以在一个 OS 线程上挂起于 host 调用之中**，真正交错执行。
  并发单位是租出的 store：每个 worker 线程上在飞的过程数等于租给它们的池化
  实例数。
- guest 源码在任何 guest 语言中都保持**同步写法**——过程代码里不需要
  async/await。挂起机制完全位于异步 world 生成的 canonical-ABI 胶水代码中；
  原生驱动就是一个普通的 async task。
- server 模式下这已是既有的执行形态：`WorkerTaskRegistry` 上的每个连接任务
  驱动自己的 `call_async` future，因此并发连接在 io_uring/tokio worker 上
  各自独立地挂起与恢复。

## host 侧交错安全性

并发调用共享 host 侧设施；保证彼此隔离的不变量：

- **worker_local 重绑定。** 池化实例在 worker 之间共享。
  `LeasedInstance::set_worker_local` 在*每次*调用前把 store 的 WASI 上下文
  重新绑定到调用方的 worker，因此 host 调用总是看到它所属调用的 worker。
- **事务。** `invoke_procedure_async` 为每次调用开启独立事务（`pre_invoke`），
  作用于该调用自己的 session 上下文；调用返回时提交或回滚的正是该上下文——
  失败的调用只回滚自己的事务。
- **实例池排他性。** 租约把实例从池中移出：每个 store 最多**一个在飞调用**，
  由所有权机制保证。调用 trap 的实例会被丢弃而不是归还池中；干净返回
  （包括结果字节中编码的 domain error）的实例可以复用。

## 要求与限制

- 每个 `Store` 一个在飞调用（wasmtime 规则；由池租约强制）。
- host 上下文数据必须是 `Send`（`call_async` future 可能在线程间迁移）。
- 同一 app 上的并发调用必须使用**不同的 task session**：SQL session 注册表
  和每 task 连接以 task id 为键，共享同一 task id 的两个调用会混用同一个
  session 上下文。
- 异步 world 的组件需要异步引擎配置（`enable_async`）并走 `call_async`
  路径；同步 world 的组件走每调用一线程的 `call` 路径。
- 池化实例在调用之间保留 guest 线性内存：过程必须是参数的纯函数，不得依赖
  全新实例的状态。

## 验证

该模型由 `mudu_runtime` 中的测试固定：

- `service/concurrent_wasm_suspension_test.rs`——两个实例挂起于 host 调用之中，
  在一个 OS 线程上交错执行（有序的 host 调用日志），并有顺序执行的对照运行
  产生完全相同的结果。
- `service/procedure_instance_pool.rs` 测试——池排他性、每个租约各自的
  worker-local 绑定、归还实例的复用与重绑定。
- `service/app_inst_impl_test.rs` 测试——并发调用各自独立提交；失败的调用
  只回滚自己的事务。
