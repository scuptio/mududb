# 队列（Queue）机制设计说明

**仅中文版（Chinese only — no English version yet）。**

本文档描述 MuduDB 队列消息传递机制的完整设计与实现完成度。它是设计说明
（design note），不是安装/使用手册；SQL 语法面、线格式契约与配置项分别在
[SQL 子集](../dev/sql_subset.md)、[队列信封与持久化水位 v1](contract/queue_v1.md)、
[服务端配置](contract/mudud_cfg_v1.md) 中。

---

## 1. 概述

队列是 MuduDB 内核提供的一种内置消息传递能力：一张关系表（默认非分区，也可
按 payload 列分区，见“分区队列”一节），其行在 `INSERT`（入队）与
`RECEIVE`/`ACK`（出队/确认）之间按顺序流转。它在普通表之上叠加了：

- **顺序性**：每条消息分配一个每分片单调递增的 `__seq` 序列号（分片队列中
  `__seq` 高位携带分片序号，见下文）；
- **投递语义**：`RECEIVE` 认领（claim）消息，`ACK` 删除已认领前缀；
- **毒消息处理**：反复投递失败的消息自动转入死信队列（DLQ）；
- **扇出（fan-out）**：命名消费者游标让每条消息对每个消费者可见一次；
- **等待唤醒**：`RECEIVE ... WAIT` 在空队列上挂起，入队提交后定向唤醒；
- **队列激活**：注册一个 WASM 过程，队列非空时自动调用。

队列数据以普通 XL 事务写入，因此容量统计、认领、确认与消息提交/回滚完全
一致，崩溃后可恢复。

---

## 2. SQL 语法面

队列语句由自定义解析路径处理（非 tree-sitter 语法），见
`crates/common/sql_parser/src/ast/parser/queue.rs`：

| 语句 | 形式 | 语义 |
|------|------|------|
| `CREATE QUEUE` | `CREATE QUEUE name LIKE tbl` / `CREATE QUEUE name (col type, ...)` / 追加 `PARTITION BY GLOBAL RULE rule REFERENCES (col)` | 声明队列，payload 复制自表或显式列；自动派生 `<name>_dlq`；带分区子句时按 payload 列分片 |
| `DROP QUEUE` | `DROP QUEUE name` | 删除队列及其（若仍存在的）死信队列 |
| `RECEIVE` | `RECEIVE [n] FROM name [FOR consumer] [WAIT ms \| WAIT FOREVER]` | 认领至多 `n` 条就绪消息（默认 1，`n >= 1`），返回信封行 |
| `ACK` | `ACK name [FOR consumer] TO seq` | 删除 `__seq <= seq` 的已认领消息（毒消息转 DLQ），或推进命名游标 frontier |
| `CREATE CONSUMER` | `CREATE CONSUMER consumer ON name` | 在队列上注册命名持久游标（进入扇出模式） |
| `DROP CONSUMER` | `DROP CONSUMER consumer ON name` | 移除命名游标（最后一个游标移除后回到竞争模式） |

- `RECEIVE` 返回信封行：`__seq`、`__lsn`，然后是 payload 列（`status`/`receive_count`
  是内部列，不返回）。
- 队列行只能由 `RECEIVE`/`ACK` 管理：对队列执行 `UPDATE`/`DELETE`/`COPY FROM`
  以及裸 relation-write 系统调用一律失败 `NotImplemented`。入队就是普通 `INSERT`
  （只填 payload 列，系统列由内核分配）。
- 队列语句可以落在任意 worker 上执行：内核按分片路由到分片 owner 所在 worker
  （远程分片走分区 RPC）。无分区队列等价于单分片。

---

## 3. 数据模型

### 3.1 系统列

`crates/db-kernel/catalog/src/contract/queue.rs` 定义固定前导列布局
（`SchemaObjectKind::Queue`）：

| 属性索引 | 列名 | 类型 | 说明 |
|---------|------|------|------|
| 0（键） | `__seq` | I64 | 每分片单调递增序列号，入队时分配；高 11 位为分片序号 |
| 1 | `__lsn` | I64 | 入队事务的提交 LSN（当前是入队事务 xid） |
| 2 | `status` | I32 | `0`=READY，`1`=CLAIMED，`2`=STATE（分片水位哨兵行） |
| 3 | `receive_count` | I32 | 该消息被认领过的次数（毒消息判定依据） |
| 4 | `__state_hi` | U128 | 哨兵行水位高字（`next`/`frontier`）；消息行为 NULL |
| 5 | `__state_lo` | U128 | 哨兵行水位低字（`depth`/`bytes`）；消息行为 NULL |
| 6.. | payload | 用户定义 | 声明顺序 |

payload 列不能与系统列重名、不能是主键（队列键恒为 `__seq`）、不能携带文件系统类型。

### 3.2 持久化状态（水位）

`crates/db-kernel/xcontract/src/queue_state.rs`：每个分片的水位存于该分片分区内的
**哨兵行**：`__seq` 为分片保留的哨兵本地序列（`QUEUE_SEQ_LOCAL_SENTINEL`），
`status = STATE`，四个计数器（`next`、`frontier`、`depth`、`bytes`）打包进隐藏的
`__state_hi`/`__state_lo` 两列。水位是普通的 relation 行——入队/确认因此是纯
relation 写事务，与本地、handoff、跨分区三条提交路径都可组合（包括协调者在非
分片 owner worker 上的事务）。

序列布局：`__seq = shard_index << 52 | local_seq`（分片序号最多 2047 个）。
分片哨兵的本地序列保留在 `2^52 - 1`，可用消息序列严格小于它。

升级路径：哨兵行出现之前的水位记录（worker KV `\0mudu.queue.state.v1/` 前缀的
33 字节 v1 记录，含版本字节与四个大端 u64 字）由 owner 侧的惰性重建一次性迁入
哨兵行；哨兵行一旦存在，KV 记录即被忽略。

### 3.3 `_queue_consumer` 系统表

`crates/db-kernel/meta/src/queue_consumer.rs`：固定表 OID `0x7`，每行一个命名消费者
游标，键 `(queue_oid, shard, consumer)`，值 `frontier`（“该分片上所有本地 `__seq`
小于它的都已确认”的排他水位单位）。行是普通事务 DML 写入，因此游标身份与 frontier 跨重启持久，并随 XL
复制流复制。一个队列恰好在其拥有至少一行时处于扇出模式。

### 3.4 分区队列（分片）

`CREATE QUEUE ... PARTITION BY GLOBAL RULE rule REFERENCES (col)` 把队列按
payload 列分片：规则的第 *i* 个分区承载分片 *i*，分片落在该分区的 owner
worker 上（与表共用 `CREATE PARTITION RULE` / `CREATE PARTITION PLACEMENT`
DDL）。无分区子句的队列等价于单分片，语义与旧版完全一致。

- **序列空间**：`__seq = shard << 52 | local_seq`，分片序号最多 2047 个；
  `ACK` 经 `__seq` 的分片位自动路由，无需新语法；`RECEIVE` 结果同样携带。
- **入队路由**：按 payload 分区列的值经 range 规则路由到分片，每分片独立分配
  本地序列、独立容量记账。
- **出队**：无键 `RECEIVE` 每语句轮询一个分片（重复调用自然排空所有分片）；
  `RECEIVE ... WAIT` 在所选分片的 owner 上挂起，分片提交后定向唤醒。
- **每分片水位**：分片的事务水位（next/frontier/depth/bytes）存于分片分区内
  的哨兵行，入队/认领/确认是纯 relation 写，与本地、handoff、跨分区提交路径
  均可组合；崩溃后由 owner 侧按 relation 惰性重建并重投递。
- **消费者游标**：扇出模式按 `(queue_oid, shard, consumer)` 每分片一条游标行，
  frontier 与 GC 均按分片推进。
- **观测面**：`GET /mudu/server/queues` 聚合各分片计数，并带每分片明细
  （`shards` 数组：分片序号、宿主 worker、depth/bytes/next_seq/frontier/waiters）。
- **限制**：分片队列只有分片内（按分区键）有序，没有全局 FIFO；队列激活
  （activation）暂不支持分区队列（注册返回 `NotImplemented`）。

---

## 4. 核心组件（crate 分布）

| Crate / 模块 | 职责 |
|--------------|------|
| `common/sql_parser/src/ast/parser/queue.rs` | 队列/消费者语句的自定义解析 |
| `db-kernel/sql/src/binder/queue.rs`、`planner/queue.rs` | 绑定与计划生成 |
| `db-kernel/catalog/src/contract/queue.rs` | 系统列、DLQ 派生、信封描述、编解码 |
| `db-kernel/command/src/{create_queue,drop_queue,receive_queue,ack_queue,create_consumer,drop_consumer,queue_dead_letter}.rs` | 各语句执行器 |
| `db-kernel/xengine/src/api.rs`、`x_param.rs` | `XContract` 队列方法 + 参数结构 |
| `db-kernel/xcontract/src/queue.rs` | `QueueRegistry`：每队列内存计数器与容量限制 |
| `db-kernel/xcontract/src/queue_state.rs` | 持久化水位 |
| `db-kernel/xcontract/src/queue_wait_registry.rs` | `RECEIVE ... WAIT` 等待注册表 |
| `db-kernel/xcontract/src/queue_consumer.rs` | `ConsumerRegistry` + 扇出模式操作 |
| `db-kernel/xcontract/src/queue_activation.rs` | 队列激活注册表与驱动任务 |
| `db-kernel/meta/src/queue_consumer.rs` | `_queue_consumer` 系统表 |
| `db-kernel/worker` | 提交 apply 路径的定向唤醒（`queue_wait_wake_tables`） |
| `db-kernel/mudu_runtime` | HTTP 管理端点（`queues.rs`、`queue_activation.rs`） |

---

## 5. 消费模式

### 5.1 竞争模式（默认游标）

无命名消费者时，`RECEIVE`（无 `FOR`）在语句事务内就地更新每行
（`status: ready -> claimed`，`receive_count + 1` 作为锁定的增量赋值），随提交生效、
回滚则回到 `ready`。并发在途事务已认领的行暴露为语句锁冲突并跳过；已提交的
`claimed` 行直接跳过、不触碰锁管理器。`ACK`（无 `FOR`）物理删除 `__seq <= seq` 且
`status == claimed` 的前缀，仍 `ready` 的行保留。

### 5.2 扇出模式（命名消费者）

`RECEIVE ... FOR consumer` 是从游标 frontier 起、按 `__seq` 序的**只读**扫描，不认领、
不更新，因此每个游标都看到每条消息。`ACK ... FOR consumer TO seq` 将游标 frontier 推进
到 `max(frontier, seq+1)`（幂等、不回退），并在同一事务中对“所有消费者都已确认”的前缀
（`__seq` 小于所有游标 frontier 的最小值）做 GC（min-cursor GC）。扇出行从不认领/毒化，
GC 直接物理删除。

### 5.3 模式互斥

每个队列两种模式互斥：带命名消费者的队列拒绝默认竞争 `RECEIVE`/`ACK`（`InvalidArgument`）；
命名接收/确认指向未知游标时失败 `EntityNotFound`。`CREATE CONSUMER` 在同一事务内把队列中
仍 `claimed` 的行重置为 `ready` 并清零 `receive_count`，使竞争模式遗留的半认领行对每个
游标可见。

---

## 6. 死信队列与毒消息

每个 `CREATE QUEUE` 都通过存储链直接派生同 payload 布局的 `<name>_dlq`（DLQ 自身不再派生
DLQ）。毒消息判定阈值 `QUEUE_POISON_RECEIVE_COUNT = 5`：

- **RECEIVE 路径**：认领时 `receive_count >= 5` 的消息在同一事务内原子转入 DLQ（DLQ 分配
  自己的 `__seq`，`__lsn` 为本事务 xid，`status`/`receive_count` 重置），不再投递给接收者
  （`command/src/queue_dead_letter.rs`）。
- **ACK 路径**：删除时 `receive_count > 5` 的消息转入 DLQ 而非删除（`command/src/ack_queue.rs`）。

DLQ 满时事务失败，保留原消息。无 DLQ 时毒消息直接删除并记录警告。

---

## 7. RECEIVE WAIT 与等待注册表

`RECEIVE ... WAIT n | WAIT FOREVER` 仅支持 autocommit（显式事务内失败 `InvalidState`）。
执行器在**注册等待者之后**才做决定是否挂起的扫描，并在每轮扫描前清除 sticky 信号；提交
通知要么落在本轮扫描之前（扫描能看见新消息），要么落在 clear 之后（sticky 信号重新武装、
park 立即返回）——不会丢失唤醒。

- 每轮扫描/认领跑一个短事务，提交后再 park；park 本身不持有事务、快照或锁。
- 提交写出队列会唤醒 FIFO 队首等待者（P0 语义：每队列单一活跃消费者，无惊群）。
- `WAIT FOREVER` 只去掉超时，不改变“部分批次立即返回”的语义；超时返回空批次。
- 等待注册表（`queue_wait_registry.rs`）仿照锁管理器等待者设计，`QueueWaiterGuard` 是
  RAII：超时/结束/取消的语句 drop 后自动注销。`DROP QUEUE` 唤醒并移除该队列全部等待者；
  `DROP CONSUMER` 唤醒该队列全部等待者（让其重新扫描并快速失败）。

---

## 8. 队列激活（Queue Activation）

通过 HTTP 管理 API 注册（`POST /mudu/server/queue_activation`、
`DELETE /mudu/server/queue_activation/{queue}`、`GET /mudu/server/queue_activations`），
**非 SQL**。语义为至少一次（at-least-once）：

- 注册路由到拥有队列（非分区 partition）的 worker；该 worker 提交写入队列后，apply 路径
  调用 `ActivationRegistry::notify`。
- 驱动任务在队列非空时每次唤醒调用一次已注册 WASM 过程，以队列名为唯一 `String` 参数；
  过程应自行通过 `RECEIVE`/`ACK` 排空。
- 成功调用后队列仍非空则延迟 `50ms` 重新武装；失败以 `100ms` 起指数退避（封顶 `30s`）。
- 连续失败达 `max_retry`（默认 5）后激活自禁用并记录告警，只能通过重新注册恢复。

注册是**易失的**（仅内存），重启后丢失，客户端需在重启后重新注册；RFC 中 catalog 注册的
`ALTER QUEUE ... SET ACTIVATION` 表单在 P1 范围之外。

---

## 9. 容量与限流

每队列两个正数上限，由 `mudud.cfg` 配置：

| 配置 | 默认 | 含义 |
|------|------|------|
| `queue_max_msgs` | 65536 | 每队列最大保留消息数 |
| `queue_max_bytes` | 67108864 | 每队列编码 payload 字节上限（不含系统列与存储开销） |

入队会预分配序列号并累加 payload 字节，任一上限被突破即失败事务 `QUEUE_FULL`（50040）。
NULL payload 字段计 0 字节。序列分配与活跃深度是事务性的；`overflow_count` 是进程内的
被拒次数，不受回滚影响。

> 注：`catalog/contract/queue.rs` 中的 `QUEUE_MAX_DEPTH_DEFAULT` 常量旁仍留有一条
> “wire to mudud.cfg” 的 `TODO(queue)` 注释，但实际运行时限制已通过
> `with_queue_limits` → `QueueRegistry::configure_limits` 从 `mudud.cfg` 接通；该注释是
> 关于常量默认值的历史遗留，不影响行为。

---

## 10. 可观测性

- `GET /mudu/server/queues`：每队列 `depth`、`bytes`、`overflow_count`、`next_seq`、
  `acked_frontier`、`dlq_depth`、`waiters`、消费者游标列表（`name`/`frontier`/`lag`）、
  激活注册。
- `GET /mudu/server/queue_activations`：激活注册状态（`enabled`、`consecutive_failures` 等）。
- 派生 DLQ 折叠进其所属队列的 `dlq_depth`，不单独列出。

---

## 11. 事务与崩溃恢复语义

- 认领/确认/容量/水位都在语句事务内生效，随提交/回滚一致。
- 空队列重启后由 relation 行重建计数器；SIGKILL 恢复会把 `claimed` 行（认领事务在 commit
  与 ACK 之间崩溃）重置为 `ready` 重投，保留 `receive_count` 以维持毒消息检测。
- 旧队列（无水位记录）首次使用时从活行迁移水位；历史上已排空的旧队列无法恢复其已丢失的高
  水位。当前读取器拒绝未知版本与非 33 字节值。
- 物理 base-backup 恢复保留队列 schema、行、游标 frontier 与空队列水位（快照含 partition 0
  数据与 `meta/relation` 目录文件）。

---

## 12. 错误码

| 错误码 | 值 | 触发 |
|--------|----|------|
| `QueueFull` | 50040 | 入队突破 `queue_max_msgs` 或 `queue_max_bytes` |
| `NotImplemented` | — | 对队列做 `UPDATE`/`DELETE`/`COPY FROM`、裸 relation 写、非 owner worker 上的队列运行时操作 |
| `InvalidState` | — | 显式事务内 `RECEIVE ... WAIT` |
| `InvalidArgument` | — | 有命名消费者时使用竞争模式 `RECEIVE`/`ACK` |
| `EntityNotFound` | — | 命名接收/确认指向未知游标、`DROP QUEUE`/`DROP CONSUMER` 目标不存在 |
| `EntityAlreadyExists` | — | `CREATE QUEUE`/`CREATE CONSUMER` 重复 |

---

## 13. 完成度评估

### 13.1 已实现

- 完整 SQL 面：`CREATE/DROP QUEUE`、`CREATE/DROP CONSUMER`、`RECEIVE`（竞争 + 扇出 + WAIT）、
  `ACK`（竞争 + 扇出）、`INSERT` 入队。
- 系统列 `__seq`/`__lsn`/`status`/`receive_count` 与信封返回。
- 死信队列派生与毒消息原子转移（RECEIVE 与 ACK 两路径）。
- 事务性容量/ACK 回滚、空队列重启与 SIGKILL 恢复、claimed 行重投。
- 扇出命名游标、frontier 持久化（`_queue_consumer`）、min-cursor GC、重启存活。
- 持久化水位 v1（含 golden fixture 与版本校验）。
- `RECEIVE ... WAIT` 与 FIFO 定向唤醒（P0 语义、无惊群、无丢唤醒）。
- 队列激活（真实 WASM guest）、指数退避与自禁用。
- `queue_max_msgs`/`queue_max_bytes` 已接 `mudud.cfg`。
- 观察端点 `/mudu/server/queues`、`/mudu/server/queue_activations`。
- 物理 base-backup 恢复保留队列 schema、行、游标 frontier、水位。

### 13.2 部分实现 / 已知限制

- **队列激活注册易失**：仅内存，重启丢失（P1 范围；`ALTER QUEUE ... SET ACTIVATION` 未做）。
- **`__lsn` 语义**：是入队事务 xid，非物理 WAL 字节偏移。
- **非 owner worker**：队列运行时操作在非 owner worker 上失败 `NotImplemented`，不支持跨
  worker 路由。

### 13.3 尚未实现 / 待办

- **有界磁盘使用 / 空间回收**：ACK 删除活行但不回收历史 relation 数据文件；churn 测试显示
  relation 数据随吞吐持续增长（relation WAL 保持平稳）。这是当前最明确的未完成项。
- **队列 PITR**（point-in-time recovery）。
- **逻辑 dump/restore**。
- **Raft 故障切换支持**。

---

## 14. 测试覆盖

- 单元测试：`command/src/*_queue_test.rs`、`*_consumer_test.rs`、`queue_dead_letter` 路径，
  以及 `xcontract` 的 `queue.rs`/`queue_state.rs`/`queue_wait_registry.rs`/
  `queue_consumer.rs`/`queue_activation.rs` 内联测试。
- 端到端（`crates/db-kernel/testing/tests/`）：`test_queue.rs`、`test_queue_consumer.rs`、
  `test_queue_activation.rs`，覆盖事务性容量/ACK 回滚、空队列重启、SIGKILL 恢复、自动毒消息
  转移、扇出游标、真实 WASM 激活 guest、物理 base-backup 恢复。
- 可选基线：`queue_churn_baseline` 跑 2000 轮 enqueue/receive/ACK 并打印延迟与存储采样
  （2026-10-22 debug 运行入队 p50/p99 为 906/1381 微秒；见 `sql_subset.md`）。
- Golden fixture：`crates/db-kernel/testing/fixtures/golden/v1/queue_state_v1.bin`（编码
  `(next=9, frontier=4, depth=5, bytes=123)`），测试钉死字节并拒绝截断/未知版本。
