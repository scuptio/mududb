# MuduDB 术语清单

[English canonical version](../en/terminology.md)

以英文版为术语规范来源。本文每个术语和定义与英文版一对一对应，英文术语链接直接指向对应定义。定义仅适用于 MuduDB，不宣称是行业统一标准。

## 架构关系

**Partition 组织数据，Worker 在节点内执行，Replica Set 管理复制与故障切换。** 一体式、分离式与横向扩容部署都有分区。

```text
数据副本集 G1：一个 Raft 组，四个逻辑分区，每个分区三个副本

Node A（Leader）      Node B（RaftFollower）    Node C（RaftFollower）
  W0 -> P0 副本         W0 -> P0 副本         W0 -> P0 副本
  W1 -> P1 副本         W1 -> P1 副本         W1 -> P1 副本
  W2 -> P2 副本         W2 -> P2 副本         W2 -> P2 副本
  W3 -> P3 副本         W3 -> P3 副本         W3 -> P3 副本
```

例中是四个分区、十二个副本、十二个 Worker 实例，但只有一个 Raft 组。W0 等表示本地槽位。当前主要启动路径为每个 Worker 选择分配列表中的第一个分区；列表形式的身份和 adoption 接口不证明已完整支持多个活跃分区。不要把当前一 Worker 一活跃分区的限制写成永久定义。

部署应按不同维度描述：

| 维度 | 术语 |
|---|---|
| 部署拓扑 | 一体式 / 分离式 / 横向扩容 |
| 副本与故障配置 | voter 数、learner、故障域、HA |
| 复制机制 | 流复制 / Raft 复制 |

部署拓扑结合数据副本集数量与控制面布局：一体式（Standalone）为一个数据副本集加内嵌控制面；分离式（Split）为一个数据副本集加独立控制面；横向扩容（Scale Out）为多个数据副本集加独立控制面。一体式与横向扩容是主要拓扑，分离式是二者之间的中间拓扑（控制面已独立、数据尚未分片）。这些维度不代表任意组合均已支持。当前配置使用顶层 `data_plane` 和可选 `control_plane`：没有 `data_plane` 时运行自包含节点；有 `data_plane` 而无 `control_plane` 时为一体式，要求恰好一个数据组。独立控制面布局的数据节点使用控制面客户端。字段、模式约束与后端限制见[配置契约](contract/mudud_cfg_v1.md)。

编排遵循以下规则：条目按英文规范名称排序，不使用条目编号，以术语名称和稳定锚点引用。
中文版沿用英文顺序；缩写条目以“参见”指向完整定义。

## 术语索引

| 英文规范定义 | 中文 |
|---|---|
| [Checkpoint](../en/terminology.md#checkpoint) | [检查点](#checkpoint) |
| [Class](../en/terminology.md#class) | [类](#class) |
| [Cluster](../en/terminology.md#cluster) | [集群](#cluster) |
| [Combined Deployment](../en/terminology.md#combined-deployment) | [控制数据合设部署](#combined-deployment) |
| [Commit Vector](../en/terminology.md#commit-vector) | [提交向量](#commit-vector) |
| [Control Plane](../en/terminology.md#control-plane) | [控制面](#control-plane) |
| [Control Plane Client (CPC)](../en/terminology.md#control-plane-client) | [控制面客户端](#control-plane-client) |
| [Control Plane Node (CPN)](../en/terminology.md#control-plane-node) | [控制面节点](#control-plane-node) |
| [Control Replica Set (CRS)](../en/terminology.md#control-replica-set) | [控制副本集](#control-replica-set) |
| [Coordinator](../en/terminology.md#coordinator) | [事务协调者](#coordinator) |
| [CPC](../en/terminology.md#cpc) | [CPC](#cpc) |
| [CPN](../en/terminology.md#cpn) | [CPN](#cpn) |
| [Cross Partition Transaction](../en/terminology.md#cross-partition-transaction) | [跨分区事务](#cross-partition-transaction) |
| [Cross Replica Set Transaction](../en/terminology.md#cross-replica-set-transaction) | [跨副本集事务](#cross-replica-set-transaction) |
| [CRS](../en/terminology.md#crs) | [CRS](#crs) |
| [Data Plane](../en/terminology.md#data-plane) | [数据面](#data-plane) |
| [Data Plane Group (DPG)](../en/terminology.md#data-plane-group) | [数据面组](#data-plane-group) |
| [Data Plane Node (DPN)](../en/terminology.md#data-plane-node) | [数据面节点](#data-plane-node) |
| [Data Replica Set (DRS)](../en/terminology.md#data-replica-set) | [数据副本集](#data-replica-set) |
| [Dedicated Control Plane](../en/terminology.md#dedicated-control-plane) | [独立控制面](#dedicated-control-plane) |
| [DPG](../en/terminology.md#dpg) | [DPG](#dpg) |
| [DPN](../en/terminology.md#dpn) | [DPN](#dpn) |
| [DRS](../en/terminology.md#drs) | [DRS](#drs) |
| [Fencing](../en/terminology.md#fencing) | [旧权威隔离](#fencing) |
| [Generation](../en/terminology.md#generation) | [代际](#generation) |
| [HA](../en/terminology.md#ha) | [HA](#ha) |
| [High Availability (HA)](../en/terminology.md#high-availability) | [高可用](#high-availability) |
| [Host](../en/terminology.md#host) | [主机](#host) |
| [Intra Replica Set Cross Partition Transaction](../en/terminology.md#intra-replica-set-cross-partition-transaction) | [同副本集跨分区事务](#intra-replica-set-cross-partition-transaction) |
| [Leader](../en/terminology.md#leader) | [领导者](#leader) |
| [Learner](../en/terminology.md#learner) | [学习成员](#learner) |
| [Log Sequence Number (LSN)](../en/terminology.md#lsn) | [日志序列号](#lsn) |
| [Log Stream](../en/terminology.md#log-stream) | [日志流](#log-stream) |
| [LSN](../en/terminology.md#lsn-abbreviation) | [LSN](#lsn-abbreviation) |
| [Member](../en/terminology.md#member) | [成员](#member) |
| [Multi Replica Set Deployment](../en/terminology.md#multi-replica-set-deployment) | [多副本集部署](#multi-replica-set-deployment) |
| [Node](../en/terminology.md#node) | [节点](#node) |
| [Node Group](../en/terminology.md#node-group) | [节点组](#node-group) |
| [Not Replicated](../en/terminology.md#not-replicated) | [不复制](#not-replicated) |
| [Object](../en/terminology.md#object) | [对象](#object) |
| [Participant](../en/terminology.md#participant) | [事务参与者](#participant) |
| [Partition](../en/terminology.md#partition) | [分区](#partition) |
| [Partition Key](../en/terminology.md#partition-key) | [分区键](#partition-key) |
| [Partition Merge](../en/terminology.md#partition-merge) | [分区合并](#partition-merge) |
| [Partition Move](../en/terminology.md#partition-move) | [分区迁移](#partition-move) |
| [Partition Rule](../en/terminology.md#partition-rule) | [分区规则](#partition-rule) |
| [Partition Split](../en/terminology.md#partition-split) | [分区分裂](#partition-split) |
| [Partitioning](../en/terminology.md#partitioning) | [分区划分](#partitioning) |
| [Physical Relation](../en/terminology.md#physical-relation) | [物理关系](#physical-relation) |
| [Placement](../en/terminology.md#placement) | [分区放置](#placement) |
| [Primary](../en/terminology.md#primary) | [主节点](#primary) |
| [Quorum](../en/terminology.md#quorum) | [法定人数](#quorum) |
| [Raft Group](../en/terminology.md#raft-group) | [Raft 共识组](#raft-group) |
| [RaftCandidate](../en/terminology.md#raft-candidate) | [Raft 候选者](#raft-candidate) |
| [RaftFollower](../en/terminology.md#raft-follower) | [Raft 跟随者](#raft-follower) |
| [Rebalance](../en/terminology.md#rebalance) | [负载重平衡](#rebalance) |
| [Replica](../en/terminology.md#replica) | [副本](#replica) |
| [Replica Set](../en/terminology.md#replica-set) | [副本集](#replica-set) |
| [Scale Out](../en/terminology.md#scale-out) | [横向扩容](#scale-out) |
| [Single Node Deployment](../en/terminology.md#single-node-deployment) | [单节点部署](#single-node-deployment) |
| [Single Partition Transaction](../en/terminology.md#single-partition-transaction) | [单分区事务](#single-partition-transaction) |
| [Single Replica Set Deployment](../en/terminology.md#single-replica-set-deployment) | [单副本集部署](#single-replica-set-deployment) |
| [Single Replica Set HA](../en/terminology.md#single-replica-set-ha) | [单副本集高可用](#single-replica-set-ha) |
| [Snapshot](../en/terminology.md#snapshot) | [快照](#snapshot) |
| [Split](../en/terminology.md#split) | [分离式](#split) |
| [Stable Epoch](../en/terminology.md#stable-epoch) | [稳定纪元](#stable-epoch) |
| [Standalone](../en/terminology.md#standalone) | [一体式](#standalone) |
| [Stream Epoch](../en/terminology.md#stream-epoch) | [流复制纪元](#stream-epoch) |
| [StreamFollower](../en/terminology.md#stream-follower) | [流复制跟随者](#stream-follower) |
| [Struct](../en/terminology.md#struct) | [结构体](#struct) |
| [Table Binding](../en/terminology.md#table-binding) | [表分区绑定](#table-binding) |
| [Term](../en/terminology.md#term) | [共识任期](#term) |
| [Transaction Epoch](../en/terminology.md#epoch) | [事务纪元](#epoch) |
| [Voter](../en/terminology.md#voter) | [投票成员](#voter) |
| [Worker](../en/terminology.md#worker) | [执行器](#worker) |
| [Worker Slot](../en/terminology.md#worker-slot) | [执行器槽位](#worker-slot) |

## 术语定义

<a id="checkpoint"></a>

### 检查点（Checkpoint）

英文规范：[Checkpoint](../en/terminology.md#checkpoint)。

持久化的恢复状态或进度。需注明作用域与持久权威；本地文件不自动等于 quorum 提交的控制状态。

<a id="class"></a>

### 类（Class）

英文规范：[Class](../en/terminology.md#class)。

DDL 声明的对象的定义侧类型：持久表、消息队列、易失缓存或临时类型声明。类定义形状（字段、类型、可空性、键）；[对象](#object)是类的实例。共享的 `ClassKind` 标签命名承载者。

<a id="cluster"></a>

### 集群（Cluster）

英文规范：[Cluster](../en/terminology.md#cluster)。

具有共同身份和路由域的 MuduDB 部署，可包含一个或多个数据副本集。

<a id="combined-deployment"></a>

### 控制数据合设部署（Combined Deployment）

参见[一体式](#standalone)。

<a id="commit-vector"></a>

### 提交向量（Commit Vector）

英文规范：[Commit Vector](../en/terminology.md#commit-vector)。

各流已提交前沿的权威集合。GroupCommit 承载进度引用，流数据体位于外部日志；向量提交不自动等于跨流业务事务。

<a id="control-plane"></a>

### 控制面（Control Plane）

英文规范：[Control Plane](../en/terminology.md#control-plane)。

负责权威 catalog、目录、placement 和 epoch 管理的服务。完整持久 split/merge/rebalance 作业调度迁入控制面仍未完成。

<a id="control-plane-client"></a>

### 控制面客户端（Control Plane Client (CPC)）

英文规范：[Control Plane Client (CPC)](../en/terminology.md#control-plane-client)。

数据节点订阅控制副本集的客户端，用于获取权威的 catalog、placement 与 epoch 元数据。它不是共识成员：与学习成员（Learner）不同，它不复制 Raft 日志，不能投票或参与竞选，也不作为故障域副本。

<a id="control-plane-node"></a>

### 控制面节点（Control Plane Node (CPN)）

英文规范：[Control Plane Node (CPN)](../en/terminology.md#control-plane-node)。

被分配控制面职责的节点。专用 CPN 不承载业务分区；当前角色标签本身不证明部署隔离已实现。

<a id="control-replica-set"></a>

### 控制副本集（Control Replica Set (CRS)）

英文规范：[Control Replica Set](../en/terminology.md#control-replica-set)。

承载权威控制元数据与 epoch 记录的副本集。当前一体式（Standalone）Raft 路径中，控制与数据职责共享底层复制组；数据使用 stream 或无复制不表示控制 Raft 被取消。

<a id="coordinator"></a>

### 事务协调者（Coordinator）

英文规范：[Coordinator](../en/terminology.md#coordinator)。

驱动事务决策协议的角色。持久权威由 CoordinatorRef 与协调者数据副本集确定，不绑定临时 Leader 地址。

<a id="cpc"></a>

### CPC

参见[控制面客户端](#control-plane-client)。

<a id="cpn"></a>

### CPN

参见[控制面节点](#control-plane-node)。

<a id="cross-partition-transaction"></a>

### 跨分区事务（Cross Partition Transaction）

英文规范：[Cross Partition Transaction](../en/terminology.md#cross-partition-transaction)。

访问多个逻辑分区的事务；这些分区可以位于同一节点/副本集的不同 Worker，跨分区不必然跨副本集。

<a id="cross-replica-set-transaction"></a>

### 跨副本集事务（Cross Replica Set Transaction）

英文规范：[Cross Replica Set Transaction](../en/terminology.md#cross-replica-set-transaction)。

访问多个数据副本集所拥有分区的事务；决策归其协调者所属数据副本集。

<a id="crs"></a>

### CRS

参见[控制副本集](#control-replica-set)。

<a id="data-plane"></a>

### 数据面（Data Plane）

英文规范：[Data Plane](../en/terminology.md#data-plane)。

执行业务事务、存储分区数据，并复制数据与协调者决策的服务。

<a id="data-plane-group"></a>

### 数据面组（Data Plane Group (DPG)）

英文规范：[Data Plane Group (DPG)](../en/terminology.md#data-plane-group)。

数据面中承载业务数据的数据副本集（DRS）序列，对应配置 `data_plane.group` 列表，由多个数据副本集组成。每个数据副本集可以是单副本（一个成员、无 `replica.raft`/`replica.stream`）或多副本（多个成员与复制机制）。数据面组指整个序列；描述其中单个单元时用数据副本集（DRS）。

<a id="data-plane-node"></a>

### 数据面节点（Data Plane Node (DPN)）

英文规范：[Data Plane Node (DPN)](../en/terminology.md#data-plane-node)。

被分配数据面职责的节点。Scale Out 路径中，DPN 承载业务分区并通过控制客户端（CPC）订阅控制副本集，不是 Raft learner。

<a id="data-replica-set"></a>

### 数据副本集（Data Replica Set (DRS)）

英文规范：[Data Replica Set](../en/terminology.md#data-replica-set)。

承载一组业务分区及其事务决策的副本集。副本集内分区共享副本集级复制与故障切换（Raft 模式包含选举），不为每个分区各建一个 Raft 组。

<a id="dedicated-control-plane"></a>

### 独立控制面（Dedicated Control Plane）

英文规范：[Dedicated Control Plane](../en/terminology.md#dedicated-control-plane)。

由专用节点承载控制副本集、与数据副本集分离的控制面布局。它是分离式与横向扩容的控制面一侧，与数据副本集数是不同维度；当前 Scale Out 路径尚未完全实现这种分离。

<a id="dpg"></a>

### DPG

参见[数据面组](#data-plane-group)。

<a id="dpn"></a>

### DPN

参见[数据面节点](#data-plane-node)。

<a id="drs"></a>

### DRS

参见[数据副本集](#data-replica-set)。

<a id="fencing"></a>

### 旧权威隔离（Fencing）

英文规范：[Fencing](../en/terminology.md#fencing)。

通过经过验证的令牌或屏障拒绝旧 owner/旧权威操作。仅修改路由或发现超时不足以构成隔离。

<a id="generation"></a>

### 代际（Generation）

英文规范：[Generation](../en/terminology.md#generation)。

有明确作用域的实例代际或权威版本，例如副本集代际、流代际。必须注明作用域；完整 epoch 授权与 fencing 代际协议尚未实现。

<a id="ha"></a>

### HA

参见[高可用](#high-availability)。

<a id="high-availability"></a>

### 高可用（High Availability (HA)）

英文规范：[High Availability (HA)](../en/terminology.md#high-availability)。

在指定故障模型下维持或恢复服务的能力。需说明 quorum、故障域、恢复行为与验证边界，部署名称本身不构成 HA 保证。

<a id="host"></a>

### 主机（Host）

英文规范：[Host](../en/terminology.md#host)。

运行服务进程的物理机或虚拟机。主机属于故障域概念，不是分区身份。

<a id="intra-replica-set-cross-partition-transaction"></a>

### 同副本集跨分区事务（Intra Replica Set Cross Partition Transaction）

英文规范：[Intra Replica Set Cross Partition Transaction](../en/terminology.md#intra-replica-set-cross-partition-transaction)。

所有参与分区属于同一数据副本集的跨分区事务。共享 Raft 组不等于无需事务协调。

<a id="leader"></a>

### 领导者（Leader）

英文规范：[Leader](../en/terminology.md#leader)。

经选举协调 Raft 组协议与写入的角色。领导权属于副本集，不同数据副本集可有不同 Leader；当选本身不证明业务恢复已完成。

<a id="learner"></a>

### 学习成员（Learner）

英文规范：[Learner](../en/terminology.md#learner)。

接收复制状态但不投票的成员。它仍是 Raft 副本，不是轻量元数据订阅客户端。

<a id="lsn"></a>

### 日志序列号（Log Sequence Number (LSN)）

英文规范：[Log Sequence Number (LSN)](../en/terminology.md#lsn)。

指定日志流中的位置。只在相同流身份和代际内比较位置；不同流没有共同的标量 LSN。

<a id="log-stream"></a>

### 日志流（Log Stream）

英文规范：[Log Stream](../en/terminology.md#log-stream)。

副本集内独立有序的日志。当前包括 meta、epoch、每 Worker 的 data 以及 tx_decision；流不等于分区或 Raft 组。

<a id="lsn-abbreviation"></a>

### LSN

参见[日志序列号](#lsn)。

<a id="member"></a>

### 成员（Member）

英文规范：[Member](../en/terminology.md#member)。

属于某个副本集的节点。成员按成员类别划分（Voter、Learner），并承担角色（Raft 模式的 Leader / RaftFollower / RaftCandidate；流复制模式的 Primary / StreamFollower）。不在副本集内的节点不是成员。

<a id="multi-replica-set-deployment"></a>

### 多副本集部署（Multi Replica Set Deployment）

参见[横向扩容](#scale-out)。

<a id="node"></a>

### 节点（Node）

英文规范：[Node](../en/terminology.md#node)。

参与部署的服务实例，在集群拓扑中由 node_id 标识。节点不等于主机、分区或 Worker。

<a id="node-group"></a>

### 节点组（Node Group）

英文规范：[Node Group](../en/terminology.md#node-group)。

现有拓扑/配置中对承载数据副本集的成员节点集合的称呼。描述数据归属优先使用 Data Replica Set，描述共识优先使用 Replica Set；本清单不直接重命名 ClusterGroup。

<a id="not-replicated"></a>

### 不复制（Not Replicated）

英文规范：[Not Replicated](../en/terminology.md#not-replicated)。

指不复制到其他成员的数据或状态。既未声明 `replica.raft` 也未声明 `replica.stream` 的数据组仅含一个成员，其数据不复制；控制面客户端（CPC）同样不复制 Raft 日志。

<a id="object"></a>

### 对象（Object）

英文规范：[Object](../en/terminology.md#object)。

实例侧的类型化行：一个[类](#class)的值——表行、队列载荷、缓存值或线上消息。对象承载类形状及其自身的值；其身份在声明时为对象标识符（OID）。

<a id="participant"></a>

### 事务参与者（Participant）

英文规范：[Participant](../en/terminology.md#participant)。

参与事务 prepare 与决定应用的角色。epoch participant 应另写“epoch 参与者”，其表示稳定水位证据集合成员，不是该事务角色。

<a id="partition"></a>

### 分区（Partition）

英文规范：[Partition](../en/terminology.md#partition)。

具有 partition_id 的逻辑数据路由与放置单元。显式分区由分区规则定义，可在保持身份的前提下迁移；多张表可绑定同一规则。未绑定规则的表或队列使用默认分区（现有存储表示为 `partition_id = 0`）。默认值 0 不表示集群中所有未分区对象共享同一个全局放置单元；具体存储仍由对象身份和所在节点上下文确定。

<a id="partition-key"></a>

### 分区键（Partition Key）

英文规范：[Partition Key](../en/terminology.md#partition-key)。

分区规则用于确定分区的值或元组；表绑定指定提供该值的表列。

<a id="partition-merge"></a>

### 分区合并（Partition Merge）

英文规范：[Partition Merge](../en/terminology.md#partition-merge)。

在受控转换下把符合条件的分区组合为更大的分区布局，不是合并副本集。

<a id="partition-move"></a>

### 分区迁移（Partition Move）

英文规范：[Partition Move](../en/terminology.md#partition-move)。

保留现有分区身份并改变其放置。更换 Worker 或数据副本集本身不创建新逻辑分区。

<a id="partition-rule"></a>

### 分区规则（Partition Rule）

英文规范：[Partition Rule](../en/terminology.md#partition-rule)。

定义键类型、分区边界和逻辑分区的元数据对象。当前规则类型为 Range，可被多个表绑定复用。

<a id="partition-split"></a>

### 分区分裂（Partition Split）

英文规范：[Partition Split](../en/terminology.md#partition-split)。

在受控元数据转换下，把分区范围布局变为更小的分区。它改变分区边界，不是增加副本。

<a id="partitioning"></a>

### 分区划分（Partitioning）

英文规范：[Partitioning](../en/terminology.md#partitioning)。

按照分区规则划分数据。单副本集和多副本集部署都存在分区划分，本身不表示跨节点分布。

<a id="physical-relation"></a>

### 物理关系（Physical Relation）

英文规范：[Physical Relation](../en/terminology.md#physical-relation)。

表在某个分区中的存储，在存储模型中由 (table_id, partition_id) 标识；不同于通过规则共享的逻辑分区。

<a id="placement"></a>

### 分区放置（Placement）

英文规范：[Placement](../en/terminology.md#placement)。

把分区分配给数据副本集与 Worker 槽位。集群路由概念上是 partition_id → (group_id, worker_index)，本地元数据也使用 worker_id。

<a id="primary"></a>

### 主节点（Primary）

英文规范：[Primary](../en/terminology.md#primary)。

流复制中指定的写入源。Raft 协议使用 Leader；不要把两种模式的故障切换保证相互套用。

<a id="quorum"></a>

### 法定人数（Quorum）

英文规范：[Quorum](../en/terminology.md#quorum)。

决策或持久性证明所需的配置成员数量条件，通常为 voter 多数派。联合共识期间不能只按全部节点简单计数。

<a id="raft-group"></a>

### Raft 共识组（Raft Group）

英文规范：[Raft Group](../en/terminology.md#raft-group)。

分布在成员上的一个独立 Raft 共识实例。在通用 Raft 路径中支撑副本集；日志流和 Worker 都不是独立 Raft 组。

<a id="raft-candidate"></a>

### Raft 候选者（RaftCandidate）

英文规范：[RaftCandidate](../en/terminology.md#raft-candidate)。

正在竞选 Leader 的 Raft 协议角色，对应 `RaftRole::Candidate`。只有具备相应 voter 成员资格的节点才能竞选；learner 不能竞选。Candidate 是协议状态，不是独立成员类别，也不等于已经具备写入权限。

<a id="raft-follower"></a>

### Raft 跟随者（RaftFollower）

英文规范：[RaftFollower](../en/terminology.md#raft-follower)。

未领导、也未竞选的 Raft 协议角色，对应 `RaftRole::Follower`。已知 Leader 时跟随其复制；启动或选举过渡期间也可处于此角色且尚无已知 Leader。角色与成员资格是两个维度：voter 处于此角色时有投票资格，learner 也可处于此角色，但不能投票或竞选。接收 Raft 模式下的外部日志流不会使其成为 StreamFollower。

<a id="rebalance"></a>

### 负载重平衡（Rebalance）

英文规范：[Rebalance](../en/terminology.md#rebalance)。

按照容量或负载策略重新分布分区放置。可以发生在横向扩容后，也可以不增加节点而执行。

<a id="replica"></a>

### 副本（Replica）

英文规范：[Replica](../en/terminology.md#replica)。

逻辑分区在成员节点上的本地副本。副本集成员是持有副本的节点，本身不是副本；计数时明确说成员还是副本。描述 Raft 角色使用 RaftFollower，因为 Leader 也持有副本。

<a id="replica-set"></a>

### 副本集（Replica Set）

英文规范：[Replica Set](../en/terminology.md#replica-set)。

共享复制成员配置与故障切换的一组成员节点。在通用 Raft 路径中，一个副本集具有一套共识状态和一个 Leader，并承载多条日志流。

<a id="scale-out"></a>

### 横向扩容（Scale Out）

英文规范：[Scale Out](../en/terminology.md#scale-out)。

具有多个数据副本集与独立控制面的部署拓扑。横向扩容通过增加数据副本集并重新分布分区扩展数据容量与处理能力，独立控制面是其前提。向现有副本集增加副本属于副本扩展，不是数据副本集横向扩容。另见[分离式](#split)与[一体式](#standalone)。

<a id="single-node-deployment"></a>

### 单节点部署（Single Node Deployment）

英文规范：[Single Node Deployment](../en/terminology.md#single-node-deployment)。

只有一个服务节点的部署。可以包含多个 Worker 和分区，但不能容忍该节点丢失。

<a id="single-partition-transaction"></a>

### 单分区事务（Single Partition Transaction）

英文规范：[Single Partition Transaction](../en/terminology.md#single-partition-transaction)。

访问一个逻辑分区的事务，与该分区有多少副本无关。

<a id="single-replica-set-deployment"></a>

### 单副本集部署（Single Replica Set Deployment）

参见[一体式](#standalone)。

<a id="single-replica-set-ha"></a>

### 单副本集高可用（Single Replica Set HA）

英文规范：[Single Replica Set HA](../en/terminology.md#single-replica-set-ha)。

一个数据副本集通过副本与故障切换满足声明的故障模型。Raft 路径应明确 voter 数量与故障域放置。

<a id="snapshot"></a>

### 快照（Snapshot）

英文规范：[Snapshot](../en/terminology.md#snapshot)。

在指定一致切点捕获的状态。明确读快照、Worker 快照或副本集快照；本地检查点不自动等于完整副本集恢复镜像。

<a id="split"></a>

### 分离式（Split）

英文规范：[Split](../en/terminology.md#split)。

介于一体式与横向扩容之间的中间部署拓扑：一个数据副本集加独立控制面。分离式在不分片业务数据的前提下把控制副本集与数据副本集隔离；它是通往横向扩容的受支持的过渡步骤，而非仅限临时的状态。另见[独立控制面](#dedicated-control-plane)。

<a id="stable-epoch"></a>

### 稳定纪元（Stable Epoch）

英文规范：[Stable Epoch](../en/terminology.md#stable-epoch)。

epoch oracle 在所需事务排空后发布的可见性水位。不能等同于 current epoch，也不自动授权回收所有更早数据。

<a id="standalone"></a>

### 一体式（Standalone）

英文规范：[Standalone](../en/terminology.md#standalone)。

具有一个数据副本集与内嵌控制面的部署拓扑：控制副本集与数据副本集共享。副本集内可以有一个或多个副本；一体式不表示单节点或单分区。另见[分离式](#split)与[横向扩容](#scale-out)。

<a id="stream-epoch"></a>

### 流复制纪元（Stream Epoch）

英文规范：[Stream Epoch](../en/terminology.md#stream-epoch)。

流复制模式中 Primary 的权威代际，提升时递增，用于拒绝旧 Primary 的过期复制流。对应现有 `stream_epoch` 持久化值及流复制状态中的 `epoch` 字段。它不是事务可见性时间戳，也不是 Raft term；代际检查不替代提升前对旧 Primary 的隔离。

<a id="stream-follower"></a>

### 流复制跟随者（StreamFollower）

英文规范：[StreamFollower](../en/terminology.md#stream-follower)。

流复制模式中接收并应用 Primary 日志、提供只读服务的角色，对应现有 `StreamRoleKind::Replica`。此角色本身不参加 Raft 选举或共识 quorum，也不等于 Raft learner；同步复制 ACK 不等于 Raft 投票。提升为 Primary 需要显式管理操作，并隔离旧 Primary；不能据此宣称具备 Raft 自动选主保证。

<a id="struct"></a>

### 结构体（Struct）

英文规范：[Struct](../en/terminology.md#struct)。

值类型复合：类型系统中具名、按字段有序的复合（原“record”类型族）。结构体是[类](#class)的值层，或独立的复合值；它本身不携带存储承载者。

<a id="table-binding"></a>

### 表分区绑定（Table Binding）

英文规范：[Table Binding](../en/terminology.md#table-binding)。

表、分区规则与路由列之间的关联，不是把表分配给某台主机。

<a id="term"></a>

### 共识任期（Term）

英文规范：[Term](../en/terminology.md#term)。

一个 Raft 组的选举任期，与 epoch 编号及副本集/流 generation 分离。

<a id="epoch"></a>

### 事务纪元（Transaction Epoch）

英文规范：[Transaction Epoch](../en/terminology.md#epoch)。

用于事务可见性与静止屏障的逻辑时间戳。事务语境中未加限定的 epoch 指此概念；它不是 Stream Epoch、Raft term、物理时间或授权 generation。

<a id="voter"></a>

### 投票成员（Voter）

英文规范：[Voter](../en/terminology.md#voter)。

有资格投票并计入配置共识 quorum 的成员。联合共识期间，两个 voter 配置都必须满足各自 quorum 条件。

<a id="worker"></a>

### 执行器（Worker）

英文规范：[Worker](../en/terminology.md#worker)。

按执行资源组织的节点内执行与存储 owner。当前主要启动路径为每个 Worker 选择一个活跃分区；这是实现限制，不是身份等同关系。

<a id="worker-slot"></a>

### 执行器槽位（Worker Slot）

英文规范：[Worker Slot](../en/terminology.md#worker-slot)。

节点/副本集配置中的 worker_index，用于对应的 Worker 和每 Worker 数据流；不是全局稳定的分区身份，也不是全局唯一的 worker_id。

## 写作约定

| 避免使用 | 建议写法 |
|---|---|
| 用“单点 / 单机”表示非 Scale Out HA | 单副本集高可用 |
| 用“非分片”暗示节点内没有分区 | 一体式 |
| 一个 Node 就是一个分片 | Node 承载 Worker 和副本 |
| 将一 Worker 一分区作为永久身份关系 | 当前主要路径为一 Worker 一活跃分区 |
| 一个分区一个 Raft 组 | 多个分区共享一个数据副本集 |
| shard = 分区 + 副本集 | 分区与副本分别表述 |
| 节点组 / 数据副本集 / Raft 组无差别混用 | 明确是在说成员集合、数据归属还是共识实例 |
| 跨分区 = 跨节点 = 跨副本集 | 明确事务实际范围 |
| epoch = term = generation | 明确逻辑时间戳、选举任期或有作用域的代际 |
| 未限定的 epoch | 明确是事务纪元、稳定纪元还是流纪元 |
| 不说明范围的“快照完成” | 说明捕获状态、切点与恢复保证 |

数据分区统一使用 **Partition（分区）**，取消 Shard 作为概念或别名；分区划分使用 **Partitioning**，跨副本集分布使用 **Cross Replica Set Partition Distribution**。

角色明确写为 **Leader / RaftFollower / RaftCandidate**（Raft 协议角色）和 **Primary / StreamFollower**（流复制模式）。跨模式描述不使用无修饰的 Follower；角色按副本集或复制模式说明，不把它当作节点的全局身份。现有类型名、配置键与磁盘字段在明确迁移前保持不变；旧代码中的 `shard` 名称不构成新的架构概念。队列的分区序号与稳定 `partition_id` 仍需区分，不能只改名就混用。

务必把 **epoch** 限定为事务纪元、稳定纪元或流纪元；裸用 epoch 会在时间戳、水位线与权威代际三种含义间产生歧义。

## 实现依据

- [Worker 归属与启动](../../crates/db-kernel/worker/src/worker/mod.rs)
- [分区规则](../../crates/db-kernel/catalog/src/contract/partition_rule.rs)和[表绑定](../../crates/db-kernel/catalog/src/contract/partition_rule_binding.rs)
- [拓扑与角色](../../crates/db-kernel/catalog/src/contract/cluster_topology.rs)
- [一体式副本集装配](../../crates/db-kernel/repl/src/combined.rs)和[服务启动](../../crates/db-kernel/server/src/server/serve/mod.rs)
- [通用流契约](../../crates/db-kernel/repl/src/log_stream/adapter.rs)和[数据流映射](../../crates/db-kernel/repl/src/data_plane/mod.rs)

另见[分区机制](partition.zh-CN.md)和[复制机制](replication.zh-CN.md)。旧设计文档可能使用历史名称或描述目标行为；本清单统一名称，不替代实现状态审计。
