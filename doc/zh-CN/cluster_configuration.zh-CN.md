# 集群配置结构规范

状态：当前已实现的配置结构。`MuduDBCfg` 解析器、`validate_topology`、默认配置模板和 Tokio 启动装配均已使用该结构。完整字段与默认值见[配置契约](contract/mudud_cfg_v1.md)。配置可解析不等于所有后端均支持或端到端恢复已验收；复制数据组使用 Tokio。[English specification](../en/cluster_configuration.md)。

去掉 `[cluster]` 包装层，`control_plane`、`data_plane` 及节点／集群身份字段直接位于顶层；顶层携带显式 `version = 1`（配置格式版本）。节点、数据组与集群用**唯一名字**标识，配置里不出现数字 id；内部 id 由稳定哈希推导。`topology_version` 是运行时拓扑修订／隔离值，不是配置格式版本：实际成员或 placement 变更遵守现有拓扑版本与在线成员变更协议。

## 层级

```text
version, name, cluster, channel_listen_port, topology_version
control_plane
  replica
    raft
    member[]
data_plane
  group[]
    name, worker_count
    replica
      raft | stream
      member[]
  placements[]
  rebalance_interval_ms
```

## 身份与哈希

节点与数据组用唯一 `name`（字符串）标识，运维只写名字，不手填数字 id。内部数字身份由稳定哈希确定性推导，无需协调：

| 身份 | 来源 | 宽度 |
|---|---|---|
| `node_id` | `fnv1a64(name)`（本节点或成员 `name` 的 UTF-8 字节） | u64 |
| `group_id` | `fnv1a64(group.name)`（组 `name` 的 UTF-8 字节） | u64 |
| `cluster_id` | `fnv1a64(cluster)`（集群名字的 UTF-8 字节） | u64 |

- 哈希函数为 **FNV-1a 64 位**（offset basis `0xcbf29ce484222325`、prime `0x100000001b3`，无 salt／seed），作用于名字的 UTF-8 字节。哈希函数固化进契约：更换哈希 = 破坏性变更。
- 每个节点独立对同一 `name` 算出同一 id，无需协调、无 bootstrap 循环依赖；raft／stream／持久化仍使用 u64 id，不变。
- `name` 集群内唯一（跨控制成员与数据成员），每节点不同；`cluster` 全集群一致（所有节点填同一个集群名）。重复 `name` 或哈希碰撞在配置加载时报错。u64 碰撞可忽略（birthday bound 约 2^32 个名字才 50% 碰撞）。
- 本节点角色由本节点 `name` 在成员列表中的位置推导（不再由数字 `node_id` 推导）。

## 单数据组与多数据组使用同一语法

统一使用顶层 `control_plane.replica.member` 和 `data_plane.group[].replica.member`，不为单数据组另设扁平配置。`member` 是数组，即使只有一个成员也使用双中括号：

```toml
# 成员字段见下方完整示例
[control_plane.replica.raft]

[[control_plane.replica.member]]

[[data_plane.group]]
name = "grp-1"
worker_count = 4

[data_plane.group.replica.raft]

[[data_plane.group.replica.member]]
```

上面仅展示表路径，不是完整可启动配置。单数据组只保留一个 `[[data_plane.group]]`；多数据组重复这个条目。组内副本数由该组 `[[data_plane.group.replica.member]]` 的条目数决定。单数据组不等于单副本；省略 `control_plane` 段即一体式（Standalone，控制与数据角色位于相同成员节点），提供即独立控制面（单数据组为分离式 Split，多数据组为横向扩容 Scale Out）。

## 字段约定

| 路径 | 含义 |
|---|---|
| `version` | 配置格式版本（u32，当前为 1） |
| `name` | 本节点唯一名字（字符串，集群内唯一） |
| `cluster` | 集群唯一名字（字符串，全集群一致） |
| `channel_listen_port` | 本地节点间消息监听端口 |
| `topology_version` | 运行时拓扑修订／隔离值 |
| `control_plane.replica.member[].name` | 控制组启动成员唯一名字 |
| `data_plane.group[].name` | 数据组唯一名字 |
| `data_plane.group[].worker_count` | 该组每个成员的业务 Worker 数 |
| `data_plane.group[].replica.member[].name` | 该数据组启动成员唯一名字 |
| `control_plane.replica.raft` | 独立控制组的复制模式（仅 Raft）及其参数 |
| `data_plane.group[].replica.raft` | 数据组 Raft 模式参数，与 stream 互斥 |
| `data_plane.group[].replica.stream` | 数据组流复制模式参数，与 raft 互斥 |
| `data_plane.placements[].group` | 放置引用的数据组名字 |
| `data_plane.rebalance_interval_ms` | 现有重平衡周期；0 表示关闭调度 |

控制成员使用 `name`、`host`、`channel_port`、现有 `client_port` 字段。保留 `client_port` 不表示控制节点提供业务 SQL。数据成员额外使用 `replication_port`，承接旧 `replication.peers[].port`。`channel_port` 用于集群 RPC／控制通信，`replication_port` 用于数据复制；`client_port` 保持现有 pgwire／Leader hint 含义，不是自定义 TCP Worker 端口。

新集群结构以嵌套的 `replica.member` 为启动成员及 peer 地址的唯一配置来源，不再重复声明 `replication.peers`。复制模式和模式专属参数收拢到各组的 `replica.raft` 或 `replica.stream`；本地监听及其他服务参数仍在拓扑之外。Tokio 启动从本节点所属数据组派生成员列表与复制模式；部署时须使本地监听与成员地址一致。控制节点走控制面启动流程，不创建业务复制组或业务 Worker。`worker_threads = 0` 不是控制节点角色开关。

## 复制模式与专属参数

`replica` 只容纳公共 `member` 数组及模式子表，不设置重复的 `mode` 字段。每个数据组至多出现 `raft`、`stream` 之一；两者都不声明（且只有一个成员）即单副本形态（单节点数据副本集、无复制机制），两者都不声明却有多于一个成员则报错。空的 `[data_plane.group.replica.raft]` 也明确选择 Raft。控制组必须声明 `[control_plane.replica.raft]`，禁止 `control_plane.replica.stream`。配置中的「控制组必须 Raft」约束针对独立 `control_plane` 段；运行时控制元数据仍使用 Raft。一体式（Standalone）且已有数据 Raft 实例时，控制与数据挂到同一个共享组；stream 或无数据复制模式下没有共享数据 Raft，`ControlPlaneNode::start` 会在集群通道上创建独立控制 Raft 实例。因此数据组使用 stream 不表示控制面也使用 stream，一体式不等于共享共识。

| 子表 | 字段 | 约定 |
|---|---|---|
| `replica.raft` | `tick_ms` | 可选，正整数毫秒；默认沿用现有 Raft tick 默认值 |
| `replica.raft` | `election_timeout_ticks` | 可选，正整数；默认沿用现有 Raft 选举超时默认值 |
| `replica.stream` | `bootstrap_primary` | 必填，必须引用同组 member 的 `name`；仅用于首次启动 |
| `replica.stream` | `sync` | 必填布尔值；true 等待至少一个 StreamFollower 持久化确认，false 本地持久化后即可应答 |
| `replica.stream` | `sync_timeout_ms` | 可选，正整数毫秒，默认 10000；只在 sync=true 时生效 |

Raft 不配置固定 Primary/StreamFollower 分配，不允许 `bootstrap_primary` 或 `sync`；其提交遵守 voter quorum，联合共识须满足两个 voter 集合。Stream 的同步 ACK 不是 Raft 投票。同步提交还须满足 Primary 本地持久化，超时仅表示复制未确认，不能推断本地事务已回滚。配置 `sync=true` 时必须至少有一个 Primary 之外的成员。

Stream 提升后的持久化角色与代际优先于 `bootstrap_primary`，重启不能回退为旧的 Primary/StreamFollower 关系。各节点使用相同组配置，不再用每节点不同的 `primary=true/false` 表达 Primary/StreamFollower。

### Raft

```toml
[[data_plane.group]]
name = "grp-1"
worker_count = 4

[data_plane.group.replica.raft]
tick_ms = 50
election_timeout_ticks = 10

[[data_plane.group.replica.member]]
name = "g1-node-1"
host = "10.0.0.1"
channel_port = 19600
client_port = 5432
replication_port = 19527
```

### Stream

```toml
[[data_plane.group]]
name = "grp-1"
worker_count = 4

[data_plane.group.replica.stream]
bootstrap_primary = "g1-node-1"
sync = true
sync_timeout_ms = 10000

[[data_plane.group.replica.member]]
name = "g1-node-1"
host = "10.0.0.1"
channel_port = 19600
client_port = 5432
replication_port = 19527

[[data_plane.group.replica.member]]
name = "g1-node-2"
host = "10.0.0.2"
channel_port = 19600
client_port = 5432
replication_port = 19527
```

### 单副本

不声明 `replica.raft`/`replica.stream` 子表、且只有一个 member 的数据组即单副本数据副本集：一个节点承载该组分区，无复制机制。

```toml
[[data_plane.group]]
name = "grp-1"
worker_count = 4

[[data_plane.group.replica.member]]
name = "g1-node-1"
host = "10.0.0.1"
channel_port = 19600
client_port = 5432
replication_port = 19527
```

模式字段应放在对应子表；`mode` 和 `member.role` 不是支持的字段。当前校验拒绝双模式、非法成员数及不在组内的 bootstrap Primary；serde 未统一拒绝未知或错层字段，部分字段可能被忽略。无模式的组必须恰好一个 member。声明 Stream 语法不宣称与所有控制面／跨组功能组合均已实现。

## 示例：单数据组一体式（Standalone，省略 control_plane）

单数据组且省略 `control_plane` 段即一体式（Standalone）：本 Raft 示例中控制与数据共享一个 Raft 组。三个成员即三副本、三个控制 voter。把 `[data_plane.group.replica.raft]` 换成 `[data_plane.group.replica.stream]`（配 `bootstrap_primary`、`sync`）仅改变数据复制模式，此时控制面在相同成员上另外运行 Raft。

```toml
version = 1
name = "g1-node-1"
cluster = "prod"
channel_listen_port = 19600
topology_version = 1

[data_plane]
rebalance_interval_ms = 0
placements = []

[[data_plane.group]]
name = "grp-1"
worker_count = 4

[data_plane.group.replica.raft]

[[data_plane.group.replica.member]]
name = "g1-node-1"
host = "10.0.0.1"
channel_port = 19600
client_port = 5432
replication_port = 19527

[[data_plane.group.replica.member]]
name = "g1-node-2"
host = "10.0.0.2"
channel_port = 19600
client_port = 5432
replication_port = 19527

[[data_plane.group.replica.member]]
name = "g1-node-3"
host = "10.0.0.3"
channel_port = 19600
client_port = 5432
replication_port = 19527
```

## 示例：横向扩容（Scale Out）—— 三个数据组各三副本，一个控制组三副本

下面是完整的拓扑片段，不是完整服务配置。各节点共享此拓扑，修改 `name` 及各自本地服务参数。示例每节点一台机器，因此端口可以相同。未创建分区规则前，不声明对应 placement。

```toml
version = 1
name = "g1-node-1"
cluster = "prod"
channel_listen_port = 19600
topology_version = 1

[control_plane.replica.raft]

[[control_plane.replica.member]]
name = "ctl-1"
host = "10.0.0.101"
channel_port = 19600
client_port = 5432

[[control_plane.replica.member]]
name = "ctl-2"
host = "10.0.0.102"
channel_port = 19600
client_port = 5432

[[control_plane.replica.member]]
name = "ctl-3"
host = "10.0.0.103"
channel_port = 19600
client_port = 5432

[data_plane]
rebalance_interval_ms = 0
placements = []

[[data_plane.group]]
name = "grp-1"
worker_count = 4

[data_plane.group.replica.raft]
[[data_plane.group.replica.member]]
name = "g1-node-1"
host = "10.0.0.1"
channel_port = 19600
client_port = 5432
replication_port = 19527

[[data_plane.group.replica.member]]
name = "g1-node-2"
host = "10.0.0.2"
channel_port = 19600
client_port = 5432
replication_port = 19527

[[data_plane.group.replica.member]]
name = "g1-node-3"
host = "10.0.0.3"
channel_port = 19600
client_port = 5432
replication_port = 19527

[[data_plane.group]]
name = "grp-2"
worker_count = 4

[data_plane.group.replica.raft]
[[data_plane.group.replica.member]]
name = "g2-node-1"
host = "10.0.0.4"
channel_port = 19600
client_port = 5432
replication_port = 19527

[[data_plane.group.replica.member]]
name = "g2-node-2"
host = "10.0.0.5"
channel_port = 19600
client_port = 5432
replication_port = 19527

[[data_plane.group.replica.member]]
name = "g2-node-3"
host = "10.0.0.6"
channel_port = 19600
client_port = 5432
replication_port = 19527

[[data_plane.group]]
name = "grp-3"
worker_count = 4

[data_plane.group.replica.raft]
[[data_plane.group.replica.member]]
name = "g3-node-1"
host = "10.0.0.7"
channel_port = 19600
client_port = 5432
replication_port = 19527

[[data_plane.group.replica.member]]
name = "g3-node-2"
host = "10.0.0.8"
channel_port = 19600
client_port = 5432
replication_port = 19527

[[data_plane.group.replica.member]]
name = "g3-node-3"
host = "10.0.0.9"
channel_port = 19600
client_port = 5432
replication_port = 19527
```

共四个独立 Raft 组、十二个配置的投票副本。

## 当前装配边界

- 数据组的 `replica.raft.tick_ms` / `election_timeout_ticks` 已传入数据 Raft；独立控制组的同名字段可解析，但 `ControlPlaneNode::start` 当前构造 `RaftCfg::new(..., 25, 4, ...)`，尚未使用配置中的控制组旋钮。不能据配置存在认定已生效。
- 数据节点的实际 Worker 数来自顶层 `worker_threads`，0 表示机器可用并行度，不自动取组的 `worker_count`。部署时显式设为相同数值；控制节点启动则强制为 0。
- IOUring 启动明确拒绝配置 `data_plane`，Legacy 也不支持该集群路径；当前集群使用 Tokio。

## 校验与实现依据

- `version` 必须为 1；未知版本报错，不静默降级。
- 顶层 `version`、`name`、`cluster`、`channel_listen_port`、`topology_version` 的 TOML 赋值必须写在第一个表头之前。
- `name` 集群内唯一（跨控制成员与数据成员）；组 `name` 唯一。重复 `name` 或哈希碰撞报错。
- 省略 `control_plane` 段表示一体式（Standalone）拓扑，此时 `data_plane.group` 必须恰好一个；提供 `control_plane` 段表示独立控制面（单数据组即分离式 Split，多数据组即横向扩容 Scale Out），控制成员非空且与数据成员不交叉（名字集合不交叉）。
- 本节点 `name` 恰好出现一次（在某数据组或控制组中）；各组成员非空，worker_count 为正。部署时使数据节点本地 Worker 数一致，不能仅由拓扑校验推断运行时数量已一致。
- 校验地址和端口范围；placement 必须引用存在的数据组（按名字）及有效 Worker 槽位，控制节点不能承载 placement。
- `bootstrap_primary` 必须引用同组 member 的 `name`。
- 既无 `replica.raft` 也无 `replica.stream` 的数据组为单副本，必须恰好一个 member。
- 启动配置不能覆盖持久化 Raft 成员配置；修改列表不等于完成在线 add/remove/promote。
- 解析、拓扑校验及默认模板已在 [`mudud_cfg`](../../crates/db-kernel/mudu_runtime/src/backend/mudud_cfg/mod.rs) 实现；启动字段映射见 [`tokio_backend`](../../crates/db-kernel/mudu_runtime/src/backend/tokio_backend.rs)。后续字段变更须同步契约、模板、fixture 和示例。
- 以上规则描述支持的配置接口；serde 结构体并未统一拒绝未知字段，旧字段或写错层级的字段可能被忽略而非报错。TOML 可解析不证明字段生效。复制数据组当前使用 Tokio 启动路径。
