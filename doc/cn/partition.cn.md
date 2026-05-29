# Partition 机制

本文介绍 MuduDB 当前的 partition 机制。

当前实现分为四层：

- 全局 range 分区规则
- 表到分区规则的绑定
- partition 到 worker 的 placement
- 执行时的路由层

目标是让多张表复用同一套 range 切分规则，同时把不同 partition 分布到多个 worker 上。

## 核心概念

### Partition Rule

partition rule 是一个全局元数据对象。

它定义了：

- 分区方法，目前只支持 `RANGE`
- 一个或多个分区键列
- 有序的分区边界
- 一组逻辑 partition

一个 rule 会展开成多个逻辑 partition。后续这些 partition 再通过 placement 元数据映射到具体 worker。

### Table Binding

表本身不会把完整分区布局直接塞进 schema。

相反，表会绑定到一个已经存在的全局 rule，并声明本表哪些列引用这个 rule。

这样可以让多张表共享同一套 partition 布局。

在当前实现里，引用列应当与主键前缀一致。这样 point lookup、range pruning 和底层 key 编码模型可以保持一致。

### Placement

placement 用来描述每个逻辑 partition 落到哪个 worker。

placement 被单独拆成一层元数据，原因是：

- placement 属于部署问题，不属于 schema 本身
- placement 的变化频率可能和表定义不同
- 同一个 rule 可以被多张表复用

### Physical Relation

在 worker 内部，relation storage 不再只按 `table_id` 建索引。

它使用下面这个物理标识：

```text
(table_id, partition_id)
```

这是必须的，因为同一个 worker 上可能同时持有同一张逻辑表的多个 partition。

## 元数据模型

partition 元数据被拆成三类对象。

### `PartitionRuleDesc`

用于定义全局 range rule：

- `rule_id`
- `name`
- `kind`
- `key_types`
- `partitions`
- `version`

每个 partition 定义里包含：

- `partition_id`
- `name`
- `start` 边界，包含
- `end` 边界，不包含

### `TablePartitionBinding`

用于定义一张表如何引用一个全局 rule：

- `table_id`
- `rule_id`
- `ref_attr_indices`

其中 `ref_attr_indices` 表示表内哪些列组成分区键。

### `PartitionPlacement`

用于描述 partition 的放置位置：

- `partition_id`
- `worker_id`

## Catalog

partition 元数据持久化在单独的内部 catalog 中。

- `__meta_partition_rule`
- `__meta_table_partition_binding`
- `__meta_partition_placement`

这些 catalog 由 `MetaMgr` 管理，并在 metadata 层缓存到内存中。

## DDL

当前实现支持以下 DDL。

### 创建全局分区规则

```sql
CREATE PARTITION RULE r_orders
RANGE (region_id, order_id) (
  PARTITION p0 VALUES FROM (MINVALUE, MINVALUE) TO (1000, MINVALUE),
  PARTITION p1 VALUES FROM (1000, MINVALUE) TO (2000, MINVALUE),
  PARTITION p2 VALUES FROM (2000, MINVALUE) TO (MAXVALUE, MAXVALUE)
);
```

### 创建分区表

```sql
CREATE TABLE orders (
  region_id BIGINT,
  order_id BIGINT,
  amount BIGINT,
  PRIMARY KEY(region_id, order_id)
)
PARTITION BY GLOBAL RULE r_orders
REFERENCES (region_id, order_id);
```

### 创建 partition placement

```sql
CREATE PARTITION PLACEMENT FOR RULE r_orders (
  PARTITION p0 ON WORKER 1,
  PARTITION p1 ON WORKER 2,
  PARTITION p2 ON WORKER 3
);
```

## 路由模型

路由由 `PartitionRouter` 实现。

对于分区表，router 会执行以下步骤：

1. 加载表绑定信息。
2. 加载该表引用的分区 rule。
3. 从 SQL key tuple 中抽取分区键。
4. 将分区键与 rule 的边界进行比较。
5. 计算出目标 `partition_id`。
6. 通过 placement 元数据解析目标 `worker_id`。

### Point 操作

point `INSERT`、`READ`、`UPDATE`、`DELETE` 都只命中一个 partition。

路由时会根据绑定列构造分区键，再按 range rule 计算目标 partition。

### Range 操作

range read 会做 partition pruning。

router 会找出所有与目标范围有交集的 partition。执行层随后：

- 本地 partition 直接扫描
- 远端 partition 转发到目标 worker 执行
- 将返回结果合并

## Worker Storage 模型

`WorkerStorage` 的 relation 数据按物理 relation identity 管理，而不是按逻辑表管理。

也就是说，底层模型从：

```text
table_id -> relation
```

变成：

```text
(table_id, partition_id) -> relation
```

worker 会按需懒创建自己需要访问的 partition relation。

## 事务与 WAL 模型

要支持 partition write，事务和 WAL 都必须携带物理 partition 身份。

当前实现使用统一的 `PhysicalRelationId`：

```text
{ table_id, partition_id }
```

这个标识被用于：

- 事务暂存
- 写冲突检查
- commit 阶段的写锁
- relation insert/delete 的 WAL 记录
- WAL replay

这样可以避免同一张表的不同 partition 在同一 worker 上相互污染。

## 远端 Partition 访问

当前执行层通过 worker message bus 提供 partition RPC。

当前支持的远端操作包括：

- point read
- range read
- insert
- update
- delete

当路由命中远端 worker 时，请求会按 partition placement 转发到该 worker 处理。

## Port Sharding 策略

partition placement 负责决定一个逻辑 partition 归属哪个 worker。Port sharding 则把这些 worker 暴露为稳定的
TCP 端口，让客户端和工具可以直接连接到目标数据所在的 worker。

当前 IOUring 和 Tokio 后端使用以下端口策略：

- `tcp_listen_port` 是基础 TCP 端口
- worker `0` 监听 `tcp_listen_port`
- worker `1` 监听 `tcp_listen_port + 1`
- worker `N` 监听 `tcp_listen_port + N`

当后端 worker 数量大于 1 时，启动路径会自动开启 `tcp_multi_port`。例如 `tcp_listen_port = 9527` 且有 4 个
worker 时，端口分布如下：

```text
worker 0 -> 9527
worker 1 -> 9528
worker 2 -> 9529
worker 3 -> 9530
```

这是传输层的分片机制，不替代 partition placement。placement 仍然负责把 `partition_id` 映射到 `worker_id`；
port sharding 负责把目标 worker 映射到可连接的 TCP endpoint。

### 发现 Worker 端口

可以通过 HTTP 管理接口查看当前 topology：

```bash
mcli --http-addr 127.0.0.1:8300 server-topology
```

返回结果包含：

- `worker_count`
- `tcp_multi_port`
- `tcp_base_listen_port`
- 每个 worker 的 `worker_index`
- 每个 worker 的 `worker_id`
- 每个 worker 的 `tcp_listen_port`
- 当前与该 worker 关联的 partition id 列表

可以用 `partition-route` 查询某个 rule key 或 range 会路由到哪些 partition 和 worker：

```bash
mcli --http-addr 127.0.0.1:8300 partition-route --rule-name r_orders --key 1001,50001
```

客户端可以组合使用这两个接口：

1. `partition-route` 解析目标 `partition_id` 和 `worker_id`。
2. `server-topology` 把该 `worker_id` 解析为 `tcp_listen_port`。
3. 客户端连接 `listen_ip:tcp_listen_port`。

### 作用

当调用方能在打开或重新绑定 session 前确定目标 partition 时，port sharding 可以减少不必要的 worker 间转发。

它提供的能力包括：

- 让客户端直接亲和到 partition 所在 worker
- 减少 partition-local 流量的跨 worker 转发
- 为 benchmark 和运维工具提供稳定的 per-worker endpoint
- 从外部观察 placement 如何映射到网络端口

### 限制

port sharding 目前绑定在多 worker 的 IOUring 和 Tokio 后端路径上。legacy 后端仍保持较早的单 TCP/PG serving
模型，不应当把它当作 per-worker 端口分片部署来使用。

客户端仍然必须尊重 partition placement。连接某个 worker 端口只是在选择执行 worker；它不会移动数据，也不会改变某个
partition 的归属关系。

## 当前语义与限制

当前实现有明确边界。

- 只支持 `RANGE`
- partition binding 预期与主键前缀一致
- partition pruning 目前只围绕 key 列进行
- placement 是显式元数据，不是自动调度
- 远端 partition 访问通过 worker-to-worker RPC 完成
- per-worker port sharding 可用于多 worker 的 IOUring 和 Tokio 后端路径

目前还存在一个重要限制：

- 跨 worker 写请求虽然已经可以远程转发执行，但还没有实现分布式两阶段提交

这意味着“跨多个 worker 的完整原子提交”目前还不成立。当前模型适合：

- 单 partition 路由写
- 跨 partition 读

但它还不是完整的分布式事务协议。

## 适用场景

以下场景适合使用当前 partition 机制：

- 表天然按有序 key 分布
- point lookup 和 range scan 都围绕同一组 key 前缀
- 数据需要分散到多个 worker
- 多张表需要共享同一套逻辑分区布局

不要把当前实现直接当成一个已经完整支持跨 worker 原子事务的分布式数据库事务层。

## 总结

当前 partition 子系统将以下部分解耦：

- 逻辑分区定义
- 表绑定
- worker 放置
- 物理存储
- 执行期路由
- 可选的 per-worker TCP port sharding

这样的拆分可以保持 schema 模型干净，也为后续扩展打基础，例如：

- partition rebalance
- partition split / merge
- 分布式 commit 协议
