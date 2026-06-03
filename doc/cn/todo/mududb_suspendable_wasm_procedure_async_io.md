# MuduDB 同步过程与 WASM Hostcall 可挂起执行模型需求文档

## 1. 背景

MuduDB 的核心目标之一，是让数据密集型应用逻辑可以直接运行在数据库之上。与传统应用架构相比，MuduDB 希望减少应用服务与数据库之间的频繁远程调用，使事务处理、状态访问、业务过程执行与数据管理在同一运行时中协同完成。

在传统 Rust 异步编程模型中，异步 I/O 通常依赖 `async/await`。这种方式虽然性能较好，但会把业务逻辑显式改写为异步函数，要求调用链上的函数逐层传播 `async`，并在调用点使用 `.await`。对于数据库内执行的业务过程而言，这会带来几个问题：

1. 用户代码需要显式感知异步机制；
2. 同步业务逻辑需要被改写为异步状态机；
3. 数据库运行时难以向用户提供简洁的过程式编程体验；
4. 对复杂业务流程、事务过程、存储过程迁移和多语言 WASM 执行不够友好。

因此，MuduDB 需要一种新的执行模型：用户过程保持同步写法，WASM 函数保持同步调用语义，宿主函数在遇到 I/O 时由运行时挂起当前执行上下文，并在 I/O 完成后恢复执行。

该模型可以理解为：

> 对开发者暴露同步过程式编程模型，对系统内部采用可挂起过程、事件驱动 I/O 和调度器恢复机制。

---

## 2. 目标

本需求旨在设计一种 MuduDB 内部执行模型，使如下调用链成立：

```text
同步过程 P
  -> 调用同步 WASM 函数 WF
      -> WF 调用同步宿主函数 HF
          -> HF 调用 async I/O 能力
          -> HF 发起 I/O
          -> 当前过程挂起
          -> 调度器运行其他过程
          -> I/O 完成
          -> 恢复当前过程
          -> HF 返回结果
      -> WF 继续执行
  -> P 继续执行
```

该模型的目标包括：

1. **保持用户代码同步风格**  
   用户编写的业务过程不需要显式使用 `async/await`。

2. **保持 WASM 函数同步调用语义**  
   WASM 侧函数 WF 可以像普通函数一样调用宿主函数 HF。

3. **宿主函数支持异步 I/O 能力**  
   HF 的接口对 WF 表现为同步函数，但 HF 内部可以调用 async 函数或异步 I/O 后端，并通过运行时挂起当前 Procedure。

4. **数据库运行时支持高并发调度**  
   当某个过程等待 I/O 时，不阻塞 OS 线程，而是让调度器运行其他可执行过程。

5. **为 MuduDB 提供统一的数据库 syscall 模型**  
   所有可能阻塞的数据库操作都通过受控宿主函数边界进入运行时，由运行时负责挂起、恢复、事务上下文维护和资源管理。

---

## 3. 非目标

本设计不追求以下目标：

1. **不要求自动异步化任意 Rust 同步函数**  
   本方案不是把任意 Rust 函数自动改写成 `Future`，也不是通用的 Rust async 替代机制。

2. **不要求兼容所有阻塞系统调用**  
   如果用户代码直接调用 `std::fs::read`、`std::net::TcpStream::read` 等阻塞 API，运行时未必能够拦截。MuduDB 应通过 WASM hostcall 或 SDK API 暴露受控阻塞点。

3. **不要求 HF 返回 Pending 状态给 WF**  
   WF 应看到普通同步返回值，而不是显式处理 `Pending`、`Poll` 或状态机。

4. **不要求跨线程任意恢复执行上下文**  
   初期应采用保守策略：过程在哪个 worker 上挂起，就在哪个 worker 上恢复。

5. **不要求替代 Rust 原生 async/await**  
   该模型主要用于数据库内过程执行、WASM 业务逻辑执行和 MuduDB 内部调度，不是通用 Rust 应用层异步框架。

6. **不在本需求中定义 Procedure 调用 ABI**  
   ABI 由 Procedure 的调用实现负责，与本需求的核心问题无关。本需求只描述同步过程、WASM hostcall、异步 I/O 与可挂起执行上下文之间的关系。

---

## 4. 核心概念

### 4.1 同步过程 P

同步过程 P 是 MuduDB 中的一个业务执行单元，可以代表一次事务过程、一次数据库内应用调用、一次用户请求处理或一次 WASM 执行任务。

P 对外表现为同步过程：

```rust
fn procedure_main(ctx: &mut ProcedureContext) -> Result<()> {
    let result = ctx.call_wasm("workflow_main")?;
    ctx.apply_result(result)?;
    Ok(())
}
```

P 不需要是 `async fn`，也不需要返回 `Future`。

在运行时内部，P 被表示为一个可挂起的执行单元：

```text
Procedure {
    id,
    state,
    wasm_instance,
    transaction_context,
    scheduler_context,
    waiting_token,
    local_storage,
}
```

---

### 4.2 WASM 函数 WF

WF 是运行在 WASM 实例中的用户逻辑函数。它可以由 Rust、C、C++、AssemblyScript 或其他语言编译生成。

WF 对宿主函数的调用是同步的：

```rust
#[no_mangle]
pub extern "C" fn workflow_main() -> i32 {
    let rows = mudu_query("select * from user where id = 1001");
    mudu_command("update user set last_seen = now() where id = 1001");
    0
}
```

WF 不感知底层 I/O 是同步还是异步。它只看到普通函数调用和普通返回值。

---

### 4.3 宿主函数 HF

HF 是由 MuduDB 宿主运行时提供给 WASM 调用的函数。它是 WASM 与数据库运行时之间的受控系统调用边界。

本阶段暂时只以两个宿主函数为例：

```text
mudu_query
mudu_command
```

其中：

1. `mudu_query` 用于执行查询类操作，通常返回结果集、单行数据或查询状态；
2. `mudu_command` 用于执行命令类操作，例如写入、更新、删除、DDL、事务控制或状态变更命令。

HF 的接口对 WF 表现为同步调用，但 HF 内部可以：

```text
1. 解析来自 WF 的请求；
2. 构造数据库查询或命令；
3. 调用 async I/O 函数或异步存储后端；
4. 记录当前 Procedure 正在等待的 token；
5. 挂起当前 Procedure；
6. I/O 完成后恢复；
7. 整理查询或命令结果；
8. 返回普通结果给 WF。
```

---

### 4.4 可挂起 Procedure

本设计的核心不是让 HF 本身“变成异步函数”，而是让承载 P、WF、HF 调用链的整个 Procedure 执行上下文可挂起。

也就是说：

```text
HF 只是触发挂起的边界；
真正被挂起的是当前 Procedure。
```

调用栈可以抽象为：

```text
Procedure P
  -> Wasm runtime
      -> WASM function WF
          -> Host function HF
              -> call async I/O
              -> suspend current Procedure
```

当 I/O 完成后，调度器恢复同一个 Procedure，HF 从挂起点继续执行，然后返回给 WF，WF 再继续执行。

---

## 5. Host Function 调用 Async I/O 的需求

### 5.1 基本需求

HF 必须具备调用异步 I/O 或 async 函数的能力。例如，`mudu_query` 和 `mudu_command` 内部可能调用：

```rust
async fn storage_query(req: QueryRequest) -> Result<QueryResult>;

async fn storage_command(req: CommandRequest) -> Result<CommandResult>;
```

但由于 HF 对 WASM 暴露的是同步调用语义，HF 本身不能简单地写成：

```rust
async fn mudu_query(...) -> Result<QueryResult> {
    storage_query(...).await
}
```

否则异步性可能向外传播，使 Procedure 调用框架、WASM 调用入口以及 P 本身都被迫异步化。

因此，本需求要求：

```text
HF 对 WASM 保持同步语义；
HF 内部可以调用 async I/O；
async I/O Pending 时挂起当前 Procedure；
async I/O Ready 后恢复当前 Procedure；
HF 再以普通同步返回值返回给 WF。
```

---

### 5.2 推荐方式一：HF 内部提交 async task，然后挂起 Procedure

推荐实现方式是：HF 内部将 async I/O 封装成一个运行时任务，获得等待 token，然后挂起当前 Procedure。

示意流程：

```text
WF 调用 mudu_query
  -> HF 创建 WaitToken
  -> HF 提交 async query task
  -> HF 将 Procedure 标记为 WaitingIo(token)
  -> HF 挂起当前 Procedure
  -> async query task 执行 .await
  -> async query 完成
  -> completion event 投递回原 worker
  -> Procedure 重新进入 Ready 队列
  -> Scheduler 恢复 Procedure
  -> HF 从挂起点继续执行
  -> HF 取回查询结果
  -> HF 返回给 WF
```

伪代码：

```rust
fn mudu_query(ctx: &mut HostContext, req: QueryRequest) -> Result<QueryResult> {
    let token = ctx.new_wait_token();

    ctx.async_runtime.spawn({
        let req = req.into_owned();
        async move {
            let result = storage_query(req).await;
            complete_token(token, result);
        }
    });

    ctx.suspend_current_procedure(token);

    ctx.take_query_result(token)
}
```

`mudu_command` 也采用类似结构：

```rust
fn mudu_command(ctx: &mut HostContext, req: CommandRequest) -> Result<CommandResult> {
    let token = ctx.new_wait_token();

    ctx.async_runtime.spawn({
        let req = req.into_owned();
        async move {
            let result = storage_command(req).await;
            complete_token(token, result);
        }
    });

    ctx.suspend_current_procedure(token);

    ctx.take_command_result(token)
}
```

这种方式保持了以下性质：

```text
P 不需要 async；
WF 不需要 async；
HF 签名不需要 async；
HF 内部可以调用 async I/O；
OS worker 线程不会被 I/O 阻塞；
Procedure 在 I/O 完成后恢复。
```

---

### 5.3 推荐方式二：Fiber-aware block_on

另一种实现方式是在 HF 内部提供一个 `fiber_block_on` 或 `procedure_block_on`。

它的作用是把 `Future` 的 `Pending` 转换成 Procedure 挂起，而不是阻塞 OS 线程。

示意代码：

```rust
fn mudu_query(ctx: &mut HostContext, req: QueryRequest) -> Result<QueryResult> {
    ctx.procedure_block_on(async move {
        storage_query(req).await
    })
}
```

`procedure_block_on` 的核心语义是：

```text
poll future
  -> Poll::Ready(value)：返回结果
  -> Poll::Pending：注册当前 Procedure 的 waker
  -> 挂起当前 Procedure
  -> 被唤醒后恢复 Procedure
  -> 再次 poll future
```

伪代码：

```rust
fn procedure_block_on<F: Future>(ctx: &mut HostContext, future: F) -> F::Output {
    let mut future = pin!(future);

    loop {
        match poll_with_procedure_waker(ctx, &mut future) {
            Poll::Ready(value) => return value,
            Poll::Pending => {
                ctx.suspend_current_procedure_without_result();
            }
        }
    }
}
```

这种方式更接近 Rust Future 模型，优点是可以直接复用 async 函数组合能力；缺点是需要把 Rust Future 的 waker、Procedure scheduler、WASM hostcall continuation 和 fiber resume 机制打通，实现复杂度更高。

---

### 5.4 不推荐方式：普通 block_on

HF 内部不应使用普通 `block_on` 直接等待 async 函数：

```rust
fn mudu_query(ctx: &mut HostContext, req: QueryRequest) -> Result<QueryResult> {
    tokio::runtime::Handle::current().block_on(async {
        storage_query(req).await
    })
}
```

这种方式可能造成：

```text
1. 阻塞当前 worker 线程；
2. 在已有 Tokio runtime 内嵌套 block_on 时 panic；
3. 调度器无法运行其他 Procedure；
4. 导致死锁；
5. 破坏 MuduDB 的高并发执行模型。
```

因此，MuduDB 需要的是：

```text
Procedure-aware / fiber-aware block_on
```

而不是普通 OS 线程阻塞式 `block_on`。

---

### 5.5 Async Runtime 与 Procedure Scheduler 的边界

系统中应明确区分两个层次：

```text
Procedure Scheduler:
    负责 P/WF/HF 的同步执行、挂起、恢复、事务上下文和调度状态。

Async I/O Runtime:
    负责 async Future、I/O driver、网络、磁盘、RPC、存储后端等异步任务。
```

二者通过 token 和 completion event 通信：

```text
HF submit async task
  -> 得到 WaitToken
  -> Procedure 进入 WaitingIo(token)
  -> Async I/O Runtime 完成任务
  -> CompletionEvent(token, result)
  -> 投递回 Procedure 所属 worker
  -> Scheduler 将 Procedure 置为 Ready
```

Async I/O Runtime 不应直接拥有 Procedure 的执行权，也不应直接在任意线程上恢复 Procedure。恢复行为必须由 Procedure Scheduler 完成。

---

## 6. 典型执行流程

### 6.1 正常同步调用流程

```text
1. Scheduler 选择一个 Ready 状态的 Procedure P；
2. P 开始执行；
3. P 调用 WASM 函数 WF；
4. WF 执行业务逻辑；
5. WF 调用宿主函数 mudu_query 或 mudu_command；
6. HF 不需要 I/O 或数据已经在本地缓存中；
7. HF 立即返回；
8. WF 继续执行；
9. WF 返回；
10. P 完成。
```

---

### 6.2 发生 async I/O 的调用流程

```text
1. Scheduler 选择 Procedure P；
2. P 调用 WASM 函数 WF；
3. WF 调用宿主函数 mudu_query 或 mudu_command；
4. HF 构造 QueryRequest 或 CommandRequest；
5. HF 提交 async I/O 任务；
6. HF 将当前 Procedure 标记为 WaitingIo(token)；
7. HF 调用 suspend_current_procedure；
8. Scheduler 切换到其他 Ready Procedure；
9. Async I/O Runtime 执行 async 函数并等待 I/O；
10. I/O 完成后生成 CompletionEvent；
11. CompletionEvent 投递回 Procedure 所属 worker；
12. Runtime 根据 token 找到等待的 Procedure；
13. 将 Procedure 状态改为 Ready；
14. Scheduler 再次调度该 Procedure；
15. Procedure 从 HF 的挂起点恢复；
16. HF 获取 async I/O 结果；
17. HF 整理结果并返回给 WF；
18. WF 继续执行；
19. P 继续执行。
```

---

## 7. 运行时状态模型

Procedure 可以采用如下状态机：

```text
Created
  -> Ready
  -> Running
  -> WaitingIo(token)
  -> Ready
  -> Running
  -> Finished

Running
  -> Failed

Running
  -> Aborted

WaitingIo(token)
  -> Timeout
  -> Aborted
```

状态说明：

| 状态 | 含义 |
|---|---|
| Created | Procedure 已创建但尚未进入调度队列 |
| Ready | Procedure 可以被调度执行 |
| Running | Procedure 正在某个 worker 上执行 |
| WaitingIo | Procedure 正在等待 async I/O 完成 |
| Finished | Procedure 正常完成 |
| Failed | Procedure 因 trap、panic、运行时错误失败 |
| Aborted | Procedure 被事务回滚、用户取消或系统主动终止 |
| Timeout | Procedure 等待超时 |

---

## 8. 调度器需求

### 8.1 基本能力

调度器需要支持：

1. Ready 队列；
2. I/O 等待表；
3. Completion 队列；
4. 定时器等待表；
5. Procedure 状态切换；
6. 挂起当前 Procedure；
7. 恢复指定 Procedure；
8. 异常终止 Procedure；
9. 事务回滚回调；
10. 资源清理回调。

---

### 8.2 推荐结构

```rust
struct Scheduler {
    ready_queue: VecDeque<ProcedureId>,
    io_waiters: HashMap<WaitToken, ProcedureId>,
    completion_queue: VecDeque<CompletionEvent>,
    timer_waiters: TimerWheel,
}
```

Procedure：

```rust
struct Procedure {
    id: ProcedureId,
    state: ProcedureState,
    wasm_instance: WasmInstanceHandle,
    tx_context: TransactionContext,
    fiber_context: FiberContext,
    waiting_on: Option<WaitReason>,
}
```

WaitReason：

```rust
enum WaitReason {
    Io(WaitToken),
    Timer(TimerId),
    Lock(LockId),
    Rpc(RpcToken),
}
```

---

### 8.3 Worker 绑定策略

初期推荐采用 per-core worker 模型：

```text
Worker-0:
  Scheduler-0
  Async I/O driver-0
  Procedure set-0

Worker-1:
  Scheduler-1
  Async I/O driver-1
  Procedure set-1
```

原则：

```text
在哪个 worker 上挂起，就在哪个 worker 上恢复。
```

这样可以避免：

1. WASM instance 跨线程迁移问题；
2. fiber 栈跨线程恢复问题；
3. TLS 不一致问题；
4. JIT 执行上下文跨线程安全问题；
5. 数据局部性下降问题。

---

## 9. I/O 后端需求

HF 可以将请求提交到不同异步 I/O 后端：

```text
1. async storage engine；
2. io_uring；
3. epoll/kqueue；
4. 网络 RPC；
5. 日志追加系统；
6. 分布式共识模块；
7. 事务锁等待队列；
8. 定时器系统。
```

所有后端都应统一返回一个等待 token：

```rust
struct WaitToken {
    owner_worker: WorkerId,
    sequence: u64,
}
```

I/O 完成后，后端将 completion event 投递回原 worker：

```rust
struct CompletionEvent {
    token: WaitToken,
    result: IoResult,
}
```

---

## 10. 事务语义需求

### 10.1 事务上下文绑定

Procedure 应绑定一个事务上下文：

```rust
struct TransactionContext {
    tx_id: TxId,
    snapshot_ts: Timestamp,
    write_set: WriteSet,
    read_set: ReadSet,
    locks: LockSet,
    status: TxStatus,
}
```

当 Procedure 挂起时，事务上下文不应丢失。

---

### 10.2 挂起期间事务状态

需要明确以下语义：

1. Procedure 挂起期间，事务是否仍然占用锁；
2. MVCC snapshot 是否保持不变；
3. 写集合是否保留；
4. 读集合是否保留；
5. 是否允许事务超时；
6. 是否允许死锁检测；
7. 是否允许事务被主动取消；
8. 如果 async I/O 失败，事务如何回滚。

初期建议：

```text
1. snapshot 保持不变；
2. read_set/write_set 保留；
3. 已获取事务锁默认保留；
4. 支持事务超时；
5. 支持死锁检测；
6. Procedure 失败时事务自动回滚。
```

但需要注意，长时间挂起并持有锁可能降低并发度。因此后续可以优化为：

```text
1. 无锁 I/O 可以自由挂起；
2. 持锁操作尽量短路径完成；
3. 长等待锁操作进入事务等待队列；
4. 对外部 RPC 等长延迟操作禁止持有事务关键锁。
```

---

## 11. 资源生命周期要求

### 11.1 跨挂起点数据

HF 如果要调用 async I/O，必须确保跨挂起点保存的数据是 owned data，或者有明确的生命周期管理。

错误方式：

```rust
fn mudu_query(ctx: &mut HostContext, sql: &str) -> Result<QueryResult> {
    let token = ctx.submit_async(async {
        storage_query(sql).await
    });

    ctx.suspend_current_procedure(token);

    ctx.take_query_result(token)
}
```

正确方式：

```rust
fn mudu_query(ctx: &mut HostContext, sql: &str) -> Result<QueryResult> {
    let sql = sql.to_owned();

    let token = ctx.submit_async(async move {
        storage_query(sql).await
    });

    ctx.suspend_current_procedure(token);

    ctx.take_query_result(token)
}
```

原则：

```text
跨 suspend / await 的数据必须 owned，或者由运行时资源管理器托管。
```

---

### 11.2 WASM memory

HF 不能跨挂起点持有 WASM memory 的直接引用。

原则：

```text
跨 yield 只能保存 offset、len、token、owned buffer；
不能保存 memory view、borrow、引用。
```

如果需要在恢复后写回结果，应在恢复后重新获取 WASM memory view。

---

### 11.3 I/O buffer

提交给异步 I/O 后端的 buffer 不能依赖普通栈变量生命周期。

推荐方式：

```rust
let buf = buffer_pool.alloc();
let token = submit_io(buf);
suspend_current_procedure(token);
let result = take_result(token);
buffer_pool.release(result.buf);
```

原则：

```text
I/O buffer 应由 buffer pool / slab / arena 管理；
token 负责关联 buffer 生命周期；
I/O 完成后再释放或复用。
```

---

### 11.4 锁和临界区

HF 在挂起前不能持有调度器锁、全局存储锁、事务管理器锁或其他不可重入资源。

错误方式：

```rust
let guard = storage.lock();
let token = submit_io(req);
suspend_current_procedure(token);
drop(guard);
```

正确方式：

```rust
let req = {
    let guard = storage.lock();
    guard.prepare_request(key)
};

let token = submit_io(req);
suspend_current_procedure(token);
```

原则：

```text
prepare 阶段可以短暂持锁；
wait 阶段不得持锁；
resume 后重新进入需要的临界区。
```

---

## 12. 错误处理需求

### 12.1 错误类型

运行时需要区分：

```text
1. WASM trap；
2. Hostcall 参数错误；
3. async I/O 错误；
4. Future panic；
5. 事务冲突；
6. 事务超时；
7. 死锁中止；
8. Procedure panic；
9. 调度器内部错误；
10. WASM instance 状态损坏；
11. 用户主动取消。
```

---

### 12.2 Panic 与 unwind

不建议允许 Rust panic 跨越 WASM/host/fiber 边界传播。

推荐策略：

```text
1. hostcall 边界 catch_unwind；
2. async task 内部 catch_unwind 或统一转换错误；
3. 将 panic 转换为 ProcedureFailure；
4. 当前事务回滚；
5. 当前 WASM instance 标记为 poisoned 或销毁；
6. 释放等待 token、I/O buffer、锁、事务资源。
```

---

### 12.3 失败恢复

当 Procedure 失败时，应执行：

```text
1. 从 ready queue 或 wait table 移除；
2. 取消尚未完成的 I/O 请求，或标记 completion 忽略；
3. 回滚事务；
4. 释放锁；
5. 回收 buffer；
6. 销毁或重置 WASM instance；
7. 返回明确错误给调用者。
```

---

## 13. WASM Runtime 适配需求

### 13.1 关键问题

不是所有 WASM runtime 都支持在 hostcall 中直接挂起当前执行上下文。

需要重点验证：

```text
1. hostcall 内是否允许 fiber yield；
2. JIT/解释器栈帧是否能跨 yield 保留；
3. WASM instance 是否必须固定在线程上；
4. Caller、Store、Memory view 是否允许跨挂起点使用；
5. 是否支持 async host functions；
6. 是否支持 continuation 或 stack switching；
7. 是否允许同一个 instance 重入；
8. panic/trap 如何传播。
```

---

### 13.2 三种可选实现路线

#### 路线 A：Procedure 整体运行在 native fiber 上

```text
Procedure Fiber
  -> 调用 WASM runtime
      -> WF
          -> mudu_query / mudu_command
              -> submit async I/O
              -> yield 当前 fiber
```

优点：

```text
1. P/WF/HF 都可以保持同步外观；
2. 编程模型清晰；
3. 对用户最透明。
```

缺点：

```text
1. 要求 WASM runtime 能承受 hostcall 中 fiber yield；
2. 栈切换和 ABI 风险较高；
3. 调试复杂。
```

---

#### 路线 B：使用 WASM runtime 原生 async/fiber 支持

部分 WASM runtime 可能提供 async hostcall、fuel、epoch interruption、continuation 或类似机制。

优点：

```text
1. 与 WASM runtime 兼容性更好；
2. 不需要强行在 hostcall 中切换 native 栈；
3. 安全边界更清晰。
```

缺点：

```text
1. 可能重新引入 async runtime；
2. 实现受具体 WASM runtime 约束；
3. 对同步外观的支持程度取决于引擎能力。
```

---

#### 路线 C：自研解释型 WASM runtime 或定制 continuation

自研 WASM 解释器可以显式保存 VM 调用帧和操作数栈，从而在 hostcall 处挂起。

优点：

```text
1. 可控性最高；
2. 容易实现 syscall-style suspend；
3. 适合数据库内受控执行环境。
```

缺点：

```text
1. 实现成本高；
2. 性能可能低于 JIT；
3. 需要完整处理 WASM 语义、安全沙箱和调试工具。
```

---

## 14. 重入与并发限制

### 14.1 禁止同一 WASM instance 重入

同一个 WASM instance 在任意时刻只能被一个 Procedure activation 使用。

需要维护状态：

```rust
enum InstanceState {
    Idle,
    Running,
    Suspended,
    Poisoned,
}
```

禁止以下情况：

```text
WF -> mudu_query -> Scheduler -> another WF on same instance
```

否则会造成 WASM stack、linear memory、host state 不一致。

---

### 14.2 Procedure 恢复限制

Procedure 恢复时必须满足：

```text
1. I/O token 已完成；
2. Procedure 状态为 Ready；
3. 原 WASM instance 未被销毁；
4. 原事务上下文仍然有效；
5. 原 worker 或兼容 worker 可恢复；
6. 当前 Procedure 不处于 Running 状态；
7. 没有重复 resume。
```

---

## 15. 安全性与隔离需求

### 15.1 WASM 沙箱

WASM 代码不能直接访问宿主内存，只能通过 hostcall 访问数据库资源。

### 15.2 Capability 权限

每个 Procedure 应绑定 capability：

```text
1. 可访问哪些表；
2. 可执行哪些 hostcall；
3. 可使用多少 CPU；
4. 可使用多少内存；
5. 可占用多少 I/O；
6. 可运行多久；
7. 是否允许网络访问；
8. 是否允许调用外部服务。
```

### 15.3 资源限制

需要支持：

```text
1. CPU instruction budget；
2. memory limit；
3. I/O quota；
4. transaction timeout；
5. max suspension count；
6. max stack size；
7. max result size；
8. max hostcall nesting depth。
```

---

## 16. 性能需求

该模型的性能目标包括：

1. 避免每个 Procedure 占用一个 OS 线程；
2. I/O 等待期间释放 worker 执行其他 Procedure；
3. hostcall 挂起/恢复开销低于线程切换；
4. 支持大规模并发 Procedure；
5. 支持 per-core 数据局部性；
6. 支持批量 completion；
7. 减少跨进程、跨网络、跨服务调用；
8. 支持复用 Rust async 生态中的存储、网络和 RPC 后端。

需要重点度量：

```text
1. Procedure 创建开销；
2. Fiber stack 内存占用；
3. hostcall 调用开销；
4. suspend/resume 开销；
5. async task 创建与唤醒开销；
6. Future poll 开销；
7. io_uring submit/completion 开销；
8. WASM 调用开销；
9. 事务持锁等待时间；
10. tail latency；
11. worker 利用率；
12. throughput。
```

---

## 17. 观测与调试需求

运行时需要提供观测能力：

```text
1. 当前 Procedure 状态；
2. 当前等待的 token；
3. hostcall 调用链；
4. 当前 async task 状态；
5. 事务 id；
6. worker id；
7. 挂起次数；
8. 总运行时间；
9. 总等待时间；
10. 最后一次 hostcall；
11. 失败原因。
```

推荐提供命令或内部接口：

```text
SHOW PROCEDURES;
SHOW PROCEDURE <id>;
SHOW WAITERS;
SHOW COMPLETIONS;
SHOW WASM_INSTANCES;
SHOW TX_LOCKS;
```

示例输出：

```text
procedure_id | state      | worker | tx_id | waiting_on | last_hostcall | wait_ms
-------------|------------|--------|-------|------------|---------------|--------
1001         | WaitingIo  | 0      | 8812  | io:9921    | mudu_query    | 3.2
1002         | Ready      | 1      | 8813  | null       | mudu_command  | 0
```

---

## 18. 最小可行版本设计

### 18.1 MVP 范围

第一阶段可以实现：

```text
1. 单 worker scheduler；
2. Procedure 状态机；
3. WASM 函数调用；
4. 两个 hostcall：mudu_query、mudu_command；
5. HF 内部提交 async task；
6. 模拟 async I/O；
7. hostcall 内挂起 Procedure；
8. completion 后恢复 Procedure；
9. 简单事务上下文；
10. 错误回滚。
```

---

### 18.2 MVP 调用链

```text
procedure_main
  -> wasm workflow_main
      -> mudu_query
          -> submit async query task
          -> suspend
      -> continue
      -> mudu_command
          -> submit async command task
          -> suspend
      -> continue
  -> finish
```

---

### 18.3 MVP 验证点

需要验证：

```text
1. P 不使用 async/await；
2. WF 不使用 async/await；
3. HF 签名保持同步；
4. HF 内部可以调用 async I/O；
5. HF 可以挂起当前 Procedure；
6. async I/O 完成后可以恢复 HF；
7. HF 返回后 WF 可以继续；
8. WF 返回后 P 可以继续；
9. 多个 Procedure 可以交错执行；
10. 单个 OS 线程可以承载多个等待 I/O 的 Procedure；
11. 错误时资源能够正确回收。
```

---

## 19. 后续演进方向

### 19.1 支持真实 io_uring

将 fake async I/O 替换为真实 io_uring：

```text
mudu_query / mudu_command
  -> submit SQE
  -> Procedure WaitingIo
  -> CQE
  -> resume
```

### 19.2 支持多 worker

引入 per-core worker：

```text
worker-local ready queue
worker-local async I/O driver
worker-local procedure table
cross-worker wakeup
```

### 19.3 支持事务调度

将事务锁等待、MVCC snapshot、commit log append 纳入统一等待机制。

### 19.4 支持分区局部性

Procedure 可以根据访问表、key range、partition id 被调度到数据所在 worker，从而减少跨核访问和锁竞争。

### 19.5 支持 WASM continuation

长期可以探索定制 WASM runtime，在 VM 层保存 continuation，而不是依赖 native fiber 栈切换。

### 19.6 支持 procedure_block_on

中期可以实现 `procedure_block_on`，让 HF 能够直接等待 Rust Future，但 Pending 时挂起 Procedure，而不是阻塞 OS 线程。

---

## 20. 风险清单

| 风险 | 描述 | 缓解方式 |
|---|---|---|
| WASM runtime 不支持 hostcall 中 yield | hostcall 中切换 fiber 可能破坏 runtime 假设 | 选择支持 fiber/async 的 runtime，或自研 continuation |
| async runtime 与 Procedure scheduler 混用 | 调度权混乱，可能导致重入或跨线程恢复 | 通过 token/completion 解耦 |
| 普通 block_on 阻塞 worker | 造成死锁、panic 或并发能力下降 | 使用 Procedure-aware block_on |
| Future 借用 HF 栈上数据 | Future 跨 await 后引用失效 | async task 使用 owned data |
| 跨线程恢复失败 | fiber/WASM instance/TLS 不可跨线程恢复 | 初期固定 worker 恢复 |
| 挂起时持锁 | 可能造成死锁 | hostcall 规范禁止 yield 前持有锁 |
| WASM memory borrow 跨 yield | 可能造成悬垂引用或违反 Rust 借用规则 | yield 前只保存 offset/owned data |
| I/O buffer 生命周期错误 | 栈上 buffer 被异步 I/O 使用 | 使用 buffer pool/slab |
| panic 跨边界传播 | 可能破坏 runtime 状态 | hostcall 边界 catch_unwind |
| 事务长时间持锁 | 影响并发和尾延迟 | 超时、死锁检测、锁等待队列 |
| instance 重入 | 破坏 WASM 执行状态 | InstanceState 控制 |
| 调试困难 | 栈切换、Future、WASM 调用链难观察 | 增强 tracing 和 procedure introspection |
| 内存占用过高 | stackful procedure 需要栈空间 | 小栈、按需增长、WASM continuation |

---

## 21. 推荐技术路线

建议 MuduDB 按三阶段实现：

### 阶段一：同步 API + async task + 模拟挂起

目标是验证编程模型。

```text
1. P/WF/HF 全部保持同步外观；
2. HF 内部提交 async task；
3. fake async I/O；
4. Procedure 状态机；
5. 手动 suspend/resume；
6. 单 worker。
```

### 阶段二：native fiber + 真实 async I/O

目标是验证系统性能。

```text
1. Procedure 运行在 fiber 上；
2. HF 内部可以提交 async I/O；
3. HF 可以 yield 当前 fiber；
4. async I/O completion 恢复 fiber；
5. 多 Procedure 并发执行；
6. 固定 worker 恢复。
```

### 阶段三：WASM continuation / 定制 runtime

目标是降低 native stack 切换风险，提高可控性。

```text
1. 在 WASM VM 层保存调用帧；
2. hostcall 触发 Suspend；
3. VM 保存 continuation；
4. async I/O 完成后恢复 VM；
5. 更精确地控制资源、安全和调试信息。
```

---

## 22. 总体结论

本需求提出的模型是可行的：

```text
P 保持同步；
WF 保持同步；
HF 保持同步签名；
HF 内部具备调用 async I/O 的能力；
HF 调用 async query/command；
运行时挂起当前 Procedure；
async I/O 完成后恢复 Procedure；
HF 返回结果；
WF 和 P 继续同步执行。
```

其核心不是把普通函数自动改造成 Rust `Future`，也不是让用户显式使用 `async/await`，而是把 MuduDB 内部的执行单元设计成可挂起的 Procedure continuation。

该模型可以为 MuduDB 形成一个重要系统特征：

> 以同步过程式编程模型暴露数据库内应用执行能力，以可挂起 Procedure、同步 hostcall 语义和 async I/O 后端实现高并发运行时。

它既保留了同步代码的可读性和工程友好性，又允许数据库运行时在 I/O 等待期间调度其他过程，从而兼顾易用性、性能和系统可控性。
