# 跨分区事务 Coordinator 单日志提交 TODO 需求

## 背景

当前系统中的 partition 是数据局部性单元。一次事务如果写入多个 partition，参与写入的 worker 可能位于同一节点，也可能位于不同节点。

传统跨分区事务可以使用 2PC：

```text
coordinator prepare log
participant prepare log
coordinator commit log
participant commit/apply log
```

这种方式运行时需要多个参与者持久化 prepare/commit 状态，写路径成本较高。

本 TODO 采用另一种策略：只在 coordinator 所在 worker 写一次权威事务日志。参与者运行时不写 prepare log，不把 commit 决策本地持久化为权威状态。故障恢复时，依赖 coordinator 的事务日志进行协作恢复。

该策略本质是用恢复复杂度换运行时效率。

## 核心目标

实现跨分区写事务的原子提交，并满足：

- 运行时只有 coordinator 写权威 Tx Log。
- participant 不写跨分区 prepare log。
- participant 的写入必须可幂等重放。
- commit 决策由 coordinator Tx Log 决定。
- 恢复时由 coordinator log 驱动，将所有 participant 恢复到一致状态。
- 对外可见性必须避免部分 partition 已 apply、部分 partition 未 apply 的中间状态暴露为已提交。

## 非目标

本设计不要求实现完整分布式 2PC。

不要求 participant 保存独立的跨分区事务决策日志。

不要求 participant 在 coordinator durable 前暴露跨分区写入结果。

不把 partition 当作可用性局部单元。HA 仍以节点或 node-level HA group 为可用性单元。

## 基本模型

一次跨分区事务由一个 coordinator worker 负责。

事务包含：

```text
tx_id
coordinator_worker_id
participant_partition_ids
participant_worker_ids
write_set
commit_epoch / visibility_epoch
decision
```

coordinator worker 的 Tx Log 是该事务的唯一权威日志。

participant 只接收 coordinator 发来的 apply 请求，并在本地执行可幂等写入。

## 日志要求

需要扩展 XL Log，使它能完整描述跨分区事务。

建议新增跨分区事务记录：

```rust
struct CrossPartitionTxRecord {
    tx_id: OID,
    coordinator_worker_id: OID,
    participants: Vec<CrossPartitionParticipant>,
    write_set: Vec<CrossPartitionWrite>,
    visibility_epoch: u64,
    decision: CrossPartitionDecision,
}

struct CrossPartitionParticipant {
    partition_id: OID,
    worker_id: OID,
}

enum CrossPartitionDecision {
    Commit,
    Abort,
}
```

要求：

- `Commit` 记录必须包含完整 write set，不能只包含 commit marker。
- 每个 participant 的写入必须能从该记录单独提取。
- 记录必须包含足够信息，让恢复流程在没有原始客户端请求的情况下重放事务。
- `tx_id` 必须全局唯一，participant 用它做幂等判断。

## 提交流程

### 1. 路由和参与者确定

coordinator 根据分区规则将 write set 拆分为多个 participant：

```text
table/key -> partition_id -> worker_id
```

worker id 是 RPC 的一级寻址对象。

### 2. Coordinator 写权威日志

coordinator 先写入一条 durable cross-partition XL record：

```text
CrossPartitionTxRecord { decision: Commit, full write_set, participants, visibility_epoch }
```

只有该日志持久化成功后，事务才允许进入 participant apply 阶段。

如果 coordinator 在日志持久化前失败，则事务视为未提交。

### 3. Participant Apply

coordinator 向每个 participant worker 发送 apply RPC：

```text
ApplyCrossPartitionTx {
    tx_id,
    coordinator_worker_id,
    partition_id,
    visibility_epoch,
    partition_write_set,
}
```

participant apply 要求：

- 按 `tx_id` 幂等。
- 同一个 `tx_id` 重复 apply 必须返回成功。
- apply 写入不能在全局可见性发布前被普通读看到。
- participant 可以维护本地 volatile applied set；如果本地重启丢失，由 coordinator replay 补齐。

### 4. 可见性发布

所有 participant apply 成功后，coordinator 发布事务可见性：

```text
visibility_epoch committed
```

读路径必须只读取已发布可见 epoch 的数据。

初版可以采用 coordinator/metadata 管理的 committed visibility watermark。

要求：

- 单个 partition 已 apply 但 global visibility 未发布时，普通读不可见。
- global visibility 发布后，所有 participant 的写入必须最终可读。
- 如果 publish 后某 participant 之后故障恢复，它必须通过 coordinator log replay 补齐。

### 5. 返回客户端

事务成功返回客户端的条件建议为：

```text
coordinator log durable
all participant apply ack
visibility published
```

不建议在 participant apply ack 前返回成功，除非读路径能严格通过 visibility epoch 屏蔽未完成写入，并且后台恢复保证最终完成。

## 恢复流程

恢复时扫描 coordinator 的 Tx Log。

对于每条 `CrossPartitionTxRecord { decision: Commit }`：

1. 检查 visibility epoch 是否已发布。
2. 对每个 participant 检查该 partition 是否已包含 `tx_id` 对应写入。
3. 对缺失 participant 重新发送 apply RPC，或在本地恢复路径中直接 apply。
4. 所有 participant apply 完成后，发布或确认 visibility epoch。

恢复状态机：

```text
NoLog
  -> Abort

CommitLogDurable + SomeParticipantMissing
  -> replay apply
  -> PublishVisible

CommitLogDurable + AllParticipantsApplied + NotVisible
  -> PublishVisible

CommitLogDurable + Visible
  -> Done
```

## Participant 幂等要求

participant 必须能判断一个 `tx_id` 是否已经 apply。

可选实现：

- 在目标 partition 数据页中保存事务 apply marker。
- 在 partition 内部维护 `applied_cross_tx` 系统表。
- 在 PL 物理日志中包含 apply marker 的页变更。

要求 marker 随 participant 的本地数据一起恢复。

如果 participant 不持久化任何 marker，则恢复后必须能通过数据内容或版本号判断是否已 apply，否则重复 replay 可能造成重复写入。

## 可见性要求

跨分区事务不能依赖“所有 partition 物理写入同时完成”来实现原子性。

必须引入提交可见性层：

```text
write apply != visible commit
```

建议：

- 每个跨分区写入携带 `visibility_epoch`。
- 普通读绑定当前已提交 epoch。
- 只有 `visibility_epoch <= committed_visibility_epoch` 的版本可见。
- coordinator 恢复补齐 apply 后再推进 committed visibility。

如果已有快照机制能表达类似 epoch/watermark，可以复用其发布机制；不要让读路径直接看到未发布版本。

## 与 HA 的关系

该策略依赖 coordinator Tx Log 的高可靠性。

如果 coordinator worker 或所在节点可能永久丢失，则 coordinator Tx Log 必须由节点级 HA/Raft 复制保护。

推荐关系：

```text
Raft / HA log
  -> 复制并提交 coordinator XL record
  -> 本地 XL replay
  -> participant apply / recovery replay
```

也就是说，运行时仍然只写一条权威事务记录，但这条记录在 HA 模式下必须经过 quorum commit。

不能只依赖 coordinator 本地磁盘，否则 coordinator 节点永久丢失会导致已提交事务丢失或无法恢复。

## 与 System Partition 的关系

跨分区事务所需的拓扑和 placement 信息来自 system partition 的系统表快照：

```text
partition_id -> partition_group_id
partition_group_id -> worker_id / ha_group_id
worker_id -> node_id + rpc endpoint
```

事务开始时 coordinator 绑定一个 metadata snapshot。

要求：

- 路由和 participant 列表基于同一个 metadata snapshot。
- 事务执行期间 placement 变化不能改变该事务的 participant 集合。
- placement 变更必须等待相关 in-flight 跨分区事务结束，或通过 epoch fencing 隔离。

## RPC 要求

需要 worker-to-worker RPC 支持：

```text
source_worker_id -> target_worker_id
```

跨分区事务使用 worker id 寻址 participant。

RPC 消息至少包括：

- `ApplyCrossPartitionTx`
- `ApplyCrossPartitionTxAck`
- `QueryCrossPartitionTxApplied`
- `RecoverCrossPartitionTx`

RPC 必须支持跨节点 worker。

IOUring 和 Tokio 后端都应走同一个 `MessageBus` 语义接口。

## 实施阶段

### Phase 1：日志和数据结构

- 定义 `CrossPartitionTxRecord`。
- 扩展 XL encode/decode。
- 增加 `tx_id` 生成规则。
- 增加 participant write set 拆分逻辑。

### Phase 2：单进程多 worker 原型

- 基于本地 `MessageBus` 实现 apply RPC。
- participant 支持幂等 apply。
- 读路径屏蔽未发布 visibility epoch。
- 增加 coordinator 恢复 replay。

### Phase 3：跨节点 RPC

- 接入跨节点 worker RPC。
- apply/recover 消息支持远端 worker。
- 处理 timeout、重复 apply、worker 重启后的 replay。

### Phase 4：HA 集成

- coordinator XL record 进入节点级 HA/Raft 复制。
- quorum commit 后再进入 participant apply。
- leader 切换后，新 coordinator 根据已提交 XL record 继续恢复。

### Phase 5：测试

- coordinator 写 log 前故障：事务不可见。
- coordinator 写 log 后、participant apply 前故障：恢复后补齐。
- 部分 participant apply 后故障：恢复后补齐，读不可见中间状态。
- apply 全部成功但 visibility publish 前故障：恢复后发布。
- visibility publish 后 participant 重启：通过 log replay 恢复可见数据。
- RPC timeout 后重复 apply：participant 幂等成功。
- 跨节点 worker 故障和 leader 切换：新 coordinator 继续 replay。

## 风险

该策略降低运行时写日志次数，但明显增加恢复和可见性控制复杂度。

最大风险是：

- participant apply 非幂等导致重复写。
- 可见性发布早于所有 participant apply。
- coordinator Tx Log 没有 HA 保护。
- placement/topology 变化导致恢复时找不到原 participant。

这些风险必须通过 `tx_id` 幂等、visibility epoch、system partition metadata snapshot 和 HA 复制共同约束。
