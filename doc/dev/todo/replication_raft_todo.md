# MuduDB 主从流复制 + Raft 哈希共识容错 — 设计与实施 Todo

> 架构：主从走日志流复制；HA 在流复制之上单独开一条 Raft channel，Raft 只存日志段哈希（hash chain/merkle）；恢复 = merkle + 日志 + replay；Raft 为 **scupt-raft 仓内 fork**（`mudu_raft` crate），遵循 mududb 全部约定。
>
> v1 已批准的范围：P1 主从流复制 + P2 Raft 容错最小正确版；**无 `[replication]` 配置时单机行为完全不变**。

## 设计要点（共识约定）

- 数据面：每 worker 一条 XL 流（有序、可批量、背压），异步主从与 HA 共用同一实现。
- 共识面：全节点单 Raft 组；entry 只含哈希——`CommitVec` / `Barrier`（`XidAlloc` 预留）。
- 两条铁律：
  - **I1′ leader 端多数派落盘门控**：replication manager 跟踪各 follower `ReplAck.stored_lsn`，多数派覆盖后才 propose `CommitVec` ⇒ 哈希已提交 ⇒ 数据在多数派 ⇒ RPO=0。（scupt-raft 投票不可插桩的等价实现；fork 后可加 validator 钩子演进为 per-vote 门控。）
  - **I2 apply ≤ 已提交向量点**：staging 与存储分离，重放只到 raft 已提交 `CommitVec`；未提交尾永不入存储，切换时截断零成本。
- 提交向量 `CommitVec{[(worker_id, lsn, seg_hash); W]}`：一次共识推进 W 条流，避免全局写序。
- 新主**先自愈再服务**：按最后 `CommitVec` 逐 worker 比对（hash chain 定位分歧）→ 拉段 → 追平 → `Barrier{term}` → 开写。
- **xid = (raft_term << 32) | worker_counter**，跨主单调，v1 免 XidAlloc entry。
- 粒度四层对齐：组提交批 = 日志段 = 哈希单元 = 共识单元（64 KiB / 32 帧 / 200 µs）。

## 工程约定（mududb 规则）

- `mudu_raft` 单事件循环：一个线程（`spawn_thread`）以 `mudu_sys::sync::sync_channel` + `recv_timeout` 驱动（tick 可注入，测试免睡眠）；副作用仅经两条 channel 出队：`persist_tx` → 持久化线程、`msg_tx` → 发送任务。
- 错误处理一律 `RS<T>` + `mudu_error!` + `EC`；无 scupt `Res/ET`。
- 消息：Raft 族与复制族都进 `mudu_contract` `MessageType`（MessagePack payload + 40B MSFM 头），复制与共识共用 `listen_port`，无独立 raft 线格式/端口。
- 存储：Raft 日志在 `<log_dir>/raft_log/`，**复用现有 LGFM 分块帧格式**存日志条目与硬状态帧；staging WAL 复用同一 chunk/帧格式；merkle 节点作为一种帧 payload kind 落在既有段/帧格式上，零新文件格式。
- 依赖：无外部 git 依赖（fork 入仓），`deny.toml` 不动。

## 配置（`mudud.cfg`）

```toml
# 无 [replication] 段 = 单机，行为与今天完全一致
[replication]
node_id = 1                    # 首启持久化到 <db_path>/node_id；不一致拒绝启动
listen_port = 19527            # 复制 + Raft 共用端口
repl_flow_window_segments = 64
snapshot_wal_threshold_bytes = 1073741824   # 可选：committed LSN 越过上次快照该量即触发快照（提交四）
gc_check_interval_secs = 60                 # 可选：WAL/staging GC 评估间隔（提交四）

# [replication.stream] 与 [replication.raft] 二选一，存在的子段即模式；

[replication.stream]           # 模式一：主从流复制（无 raft）
primary = true                 # 结构性必填：本节点是否主库；全集群恰好一台 true
sync = true                    # 结构性必填：true=同步（等任一从库落盘 ack 再应答）；false=异步
sync_timeout_ms = 10000        # 可选：同步等待超时

[replication.raft]             # 模式二：Raft 共识容错（HA）
raft_tick_ms = 50              # 可选：raft 状态机 tick 间隔（毫秒）
election_timeout_ticks = 500   # 可选：选举超时（tick 数）

[[replication.peers]]          # 静态成员；须含自身 node_id
node_id = 1
host = "127.0.0.1"
port = 19527
```

模式语义：

- `[replication.raft]`：现状不变——多数派落盘门控 + CommitVec 共识 + 自动选主，RPO=0。peers == 2 仅打警告（它是新节点 bootstrap join 的合法形态——本地只配自身 + 一个现存成员，stored conf wins；作为稳态部署不推荐：无自动容错、写可用性反低于单机）；1（开发调试）或 ≥3 推荐。
- `[replication.stream]`：不启动 raft。主库本地 WAL 提交（`sync=true` 时先等任一从库 `ReplAck.stored_lsn` 覆盖提交点，超时返回错误——数据已本地落盘，从库追平后自愈）再应答；从库即时 apply、只读门控不变。主故障人工切换（从库改 `primary = true` 重启，fencing 为运维责任）：`sync=false` RPO > 0；`sync=true` 对已应答提交 RPO=0（前提：故障时至少一台从库存活）。
- 两个子段二选一、存在即模式：皆缺或并存均为启动期配置错误；`primary`/`sync` 是 stream 子段的结构性必填键（缺键即 TOML 解析失败），raft 子段旋钮全部可选。

## v1 实施（两个可评审提交）

### 提交一（P1 + 地基）

- [x] `ReplicationCfg` 解析/校验（`mudud_cfg.rs`）+ `init-cfg` 样例 + node_id 持久化
- [x] `mudu_contract` 复制族帧：`ReplSubscribe` / `ReplBatch` / `ReplAck` / `ReplHealFetch` / `ReplHealResp`；Error 增 `leader_hint`
- [x] 内核 `server/repl/`：`source.rs`（按 worker+lsn 从 WAL 分块读帧推流、追平挂 live 尾）、`sink.rs`（staging WAL + 即时 apply + 回 ack）、`merkle.rs`（段哈希链）
- [x] 只读门控：replica 角色拒绝写类 MessageType（带 `leader_hint`）
- [x] 运行时透传（tokio / io_uring 两条启动路径；无配置不实例化任何 repl 组件）
- [x] `testing/tests/test_replication.rs`：双节点主写从读一致、只读门控、手动重连

### 提交二（P2 Raft 容错）

- [x] `mudu_contract` Raft 族帧：PreVote/Vote/Append/Apply（快照预留）
- [x] `mudu_raft` crate：状态机移植（pre-vote/选举/append/commit/联合共识版本，对齐 TLA+ 已验证语义）、单事件循环、持久化（raft_log/）、transport、propose/watch_committed/role API
- [x] cluster 提交路径：stage → WAL（不 apply）→ 段广播 → 水位门控 → propose → committed → replay apply → 应答（单机路径不动）
- [x] xid `(term<<32)|counter` 分支
- [x] leader-elect heal 编排（比对/拉段/追平/Barrier）
- [x] `mudu_client`：`leader_hint` 重连重试；topology 增 role/leader/commit_vec
- [x] `testing/tests/test_raft_ha.rs`：三节点杀主 RPO=0、带分叉尾重 join 自愈

### 提交三（P2.5 显式 mode + 主从同步/异步）

- [x] `mudud_cfg.rs`：`[replication.stream]` / `[replication.raft]` 子段二选一、存在即模式（`ReplicationCfg::mode()` 派生 `ReplicationMode`，无 `mode` 键）；`primary` / `sync` 为 stream 子段结构性必填键（缺键即解析失败），`sync_timeout_ms` 默认 10000；raft 子段旋钮 `raft_tick_ms`（默认 50）/ `election_timeout_ticks`（默认 500）+ 模板样例
- [x] 校验：两子段皆缺或并存均报错；raft 拒绝 peers == 2；stream 要求 peers ≥ 2
- [x] 内核：stream 模式不实例化 raft / commit_sequencer；主库走本地提交路径，`sync=true` 挂"任一从库 ack"等待（复用 source 流控的 ack 跟踪），超时返回错误；从库 sink 即时 apply
- [x] `testing/tests/test_replication.rs` 用 stream 子段：异步 + 同步两变体（同步覆盖从库 ack 门控与超时）；`test_raft_ha.rs` 保持 3 节点 raft 子段
- [x] cfg 单测：子段必填、子段互斥、raft 拒绝 2 节点、stream 结构性必填与默认值

### 提交四（P3 快照/状态传输 + WAL GC）

- [x] 配置：`[replication]` 共有字段 `snapshot_wal_threshold_bytes`（默认 1 GiB，committed LSN 越过上次快照该量即触发下一快照）、`gc_check_interval_secs`（默认 60）；校验（非零）、模板、单测
- [x] 快照例程（两模式共用，per worker）：flush 脏页 + 数据文件补 fsync → 拷 `relation/*.dat`（按 worker 分区过滤；meta 目录随分区 0）+ dump `kv_store`（msgpack 单文件，含墓碑、保留原 xid）→ 写 manifest（version/worker_index/committed_lsn/seg_hash/raft_term/raft_index/taken_at/files+crc64）。版本化目录 `snapshot/worker_N/v<lsn>/`，manifest 最后落盘即发布（无效版本加载时跳过），无需 rename 原语
- [x] 串行切点：raft 模式在 promote applier 内联（滞后一个段——leader 上 sequencer 先 apply 段 k 才 propose 段 k+1，避免快照抢到未 apply 的段）；stream 从库在 sink 内联（resume 点）；stream 主库在维护任务按"applied 低水位"触发（`ReplModeCtl` 提交路径 begin/end 埋点，水位只在没有在途 apply 时推进，保证 ≤ 水位的提交全部 apply 完成）
- [x] 快照传输（复制通道新消息族 `ReplSnapshotOffer`/`ReplSnapshotFetch`/`ReplSnapshotResp`）：source 见 `from_lsn < gc_frontier` 回 offer（携带 manifest）；sink 分页拉文件 → CRC 校验 → 落盘替换 relation 文件 + meta/关系重载 + kv 恢复 → 设 chain cell / resume 点 → raft 模式 `install_snapshot(manifest 前沿)` → 从 snapshot_lsn+1 重订阅。stream 模式同一传输
- [x] WAL GC：`gc_prefix_chunks` 整 chunk 前缀回收 + `<oid>.gcfrontier` 标记文件（含首个存活 lsn 与 seq），`chunk_sequences`/`scan_tail` 从 frontier 起探（修复 seq0 空洞误判）；水位 = leader min(在线订阅者 ack, chain, 快照点) / follower min(chain, 快照点)；订阅者连接注册表（断线不再拖住 leader 的 WAL，重连低于 frontier 走快照）
- [x] staging `gc_prefix` 随 chain 推进（整 chunk 删除 + 索引过滤，重开扫面按 committed.vec 下限过滤防复活）；raft 侧节奏：业务快照点即 raft install 前沿（manifest 记录 CommitVec 的 (term,index)，term 恒正确），各节点 `install_snapshot` 推进，落后从库经 ApplyReq stub 提示 + 下一轮 offer 自愈
- [x] mudu_raft：`step_install_snapshot` 接受越界前沿（擦除路径——全新节点加入已压缩组的唯一通道；原实现越界报错使 wiped follower 永远无法安装前沿）
- [x] 测试：单测（manifest 往返/加载器、GC frontier 恢复、staging gc_prefix、store 物理 GC 既有）；集成 `testing/tests/test_snapshot_gc.rs`：raft 三节点 + stream 双节点各跑"写量触发快照+GC → 清空 follower 数据目录重 join → 断言走快照（restore 计数器）→ 读一致 → WAL chunk 数受控"
- 降级说明（按计划允许）：relation_wal 前缀 GC 本提交不做（其页 LSN 戳使重放幂等，体积增长独立于 worker WAL）；快照文件整读入内存（v1 数据量小）；多 worker 时每 worker 快照只含本分区 relation 文件（测试均为单 worker）

### 提交五（P4 在线成员变更 + learner，raft 模式）

- [x] mudu_raft（agent-5）：`RaftHandle::update_conf`（leader-only，立即拒绝非 leader/进行中/非法成员，waiter 在 joint 提交时 resolve、易主/停止明确报错）、`RaftHandle::conf()`、`NodeStatus` 携带 conf 版本；`ConfNodeValue::derive_add_voter/add_learner/remove_node/promote_voter`。本提交补充：选举超时改为**纯本地**（`override_timeout_max_tick` 落本地字段不落 conf；新增 `RaftCore::set_election_timeout_offset`/`RaftCfg::with_election_timeout_offset` 本地相位）——conf 值随复制传播会把全节点超时压平、重新造成同相位分票（锁步活锁），选举相位必须永远本地
- [x] 内核动态化：transport 地址簿改 `SMutex<HashMap>`；conf watcher（watch_status 观察 committed conf 版本变化）→ `run_conf_sync` 增删 peer writer；`RaftShare.peers/voters` 随 committed conf 更新（`quorum_size`、heal voter 列表、sequencer 多数派门随之动态）；地址来源 = 内核侧地址注册表（conf 只有 node_id，membership 请求携带 host/port，conf 变更时对齐清理已移除成员）；source 的订阅者校验从静态 peers 改为当前 committed conf
- [x] HTTP `POST /mudu/repl/membership`（body `{action, node_id, host?, port?}`）：仅 raft 模式；非 leader 错误信封带 `leader_hint{node_id, host, http_port}`（对称部署：HTTP 端口各节点一致）；leader 侧 derive → update_conf → waiter 等 joint 提交（30s 预算，actix 线程经独立线程桥接）→ 返回新 conf 视图；挂新能力位 `enable_repl_admin`（Tokio 默认开，Legacy 关）
- [x] mcli `mcli repl`：`add-node --node-id N --host H --port P [--learner]`、`remove-node --node-id N`、`promote-learner --node-id N`；`management::repl_membership` 客户端遇 leader_hint 自动重投（≤4 跳）；mock 测试三命令 + hint 重投
- [x] 新节点入网（learner 推荐）：本地 cfg 只需自身 + 任一现存成员即可 bootstrap join（stored conf wins）；提交四的快照/流追平后 `promote-learner`。两 peers 的 raft 配置从拒绝降级为启动警告（它是入网节点的合法 bootstrap 形态；稳态双节点仍不推荐）
- [x] 集成 `testing/tests/test_membership.rs`：3 节点 → learner 加第 4 节点（最小 bootstrap 配置）→ 追平 → promote → 杀旧 leader → 第 4 节点参与并胜选、RPO=0 → remove 死成员缩回 3 节点正常写；另有 reconf 中杀主用例（请求明确失败或已提交，新主重投完成）

### 提交六（P5 stream promote 工具化）

- [x] epoch：`<db_path>/stream_epoch`（load-or-create + 持久化，复用 node_id 模式）；promote 时 `epoch = max(本地持久, 已知)+1`，**先原子落盘再翻转 ctl 开写门**；`xid = (epoch<<32)|counter`（复用 raft term 编码分支，`set_repl_ctl` 时把 ctl 的 epoch 源装进 snapshot_mgr）
- [x] 运行时角色切换：`ReplModeCtl` 改共享可变（SRwLock inner + AtomicU64 epoch，每 server 一份全 worker 共享）：`is_primary`/`primary_addr`/`leader_hint` 动态化；被提升节点的 sink 逐批检查到 `is_primary` 或目标变化即断开重估（自然转 idle/重连新主）；follow 后的重叠区 CRC64 字节比对截断分歧（`probe_from` + 本地 CRC 映射，复用 raft sink 同款模式）
- [x] epoch 数据面兜底：协议增 `ReplSubscribeAccept`（源端身份 + epoch 首帧）+ `ReplSubscribeReq.src_epoch` + `ReplBatchFrame.epoch`；source 拒绝知道更高 epoch 的订阅者（僵尸源闭嘴），sink 校验 accept/批 epoch 低于已知即拒并把更高 epoch 持久化
- [x] `POST /mudu/repl/promote {fencing_ok}`（守卫=stream 模式+当前为从库+fencing_ok==true，否则 400）：epoch 落盘 → ctl 置 primary → 持久化 `<db_path>/stream_role`（role/epoch/primary_node_id/host/port，stored wins 同 raft conf 哲学）→ 返回角色视图；`POST /mudu/repl/follow {primary_node_id, primary_host, primary_port}`：先探活校验对端 node_id（订阅首帧）再翻转 ctl + 落 stream_role；启动时 stream_role 优先于 cfg 的 `primary` 键，首启用 cfg 并立刻写 stream_role
- [x] mcli `repl promote [--fencing-cmd <脚本>|--yes]`（fencing-cmd 本地执行非零即中止不发请求；无钩子需 --yes 显式确认）、`repl follow --primary <host:port> --primary-node-id <N>`；README + mock 测试（--yes/fencing-cmd 成败/拒绝/follow/地址解析）
- [x] 集成 `testing/tests/test_stream_promote.rs`：杀主 → promote 从库（epoch 断言）→ 新主写读 → 另一从库 follow 追平 → fencing 缺失 400 且角色不变 → 僵尸旧主复活（stored role 仍自认 primary、低 epoch）→ 其写不混入、follow 指它被"stale primary"拒绝 → 重启 stored wins（角色与 epoch 不变）

### 提交七（P6 复制可观测性）

- [x] `GET /mudu/server/replication`（只读端点，无能力位门控）：mode（raft/stream/none——单机返回显式 `mode="none"` 而非 404，文档化）、node_id、role、serving；raft 段（term/leader_node_id/commit_index/commit_vec/voters/learners/conf 版本对）；stream 段（epoch/primary_node_id/is_primary/sync/sync_timeout_ms）；per-worker（wal_next_lsn、raft chain 点 (lsn, seg_hash)、staging 高水位、最近快照点 (committed_lsn/taken_at_ms/目录字节)）；leader/primary 视角每订阅者 `(node_id, worker, stored_lsn, source_lsn, lag_lsn)`（lag 以 LSN/帧计，文档注明）；计数器（sync 写超时、快照次数、WAL/staging GC 回收 chunk 数、快照 offer/恢复次数——在 ctl waiter / take_worker_snapshot / maintain GC 路径埋点）
- [x] 数据源全部复用既有注册表（ack 水位、RaftShare、stream_admin、chain cell、manifest 加载器）；存储体积走**周期缓存**（每 server 一条 10s 刷新线程，端点读缓存，不在请求路径 walk dir；首次刷新在启动时）
- [x] mcli `repl status`（JSON 默认输出，`--table` 分节文本渲染：节点角色 / raft|stream 段 / per-worker 水位 / 订阅者 lag / 存储占用 / 计数器）；`management.rs` 的 `ServerTopology` 客户端结构补 `replication` 字段（修服务端返回被静默丢弃的缺口），`mcli server-topology` 随 serde 输出 role/leader/commit_vec
- [x] 集成 `testing/tests/test_repl_status.rs`：stream 双节点（sync=true）断言 mode/epoch/primary/订阅者 lag/快照计数/存储字节 + 杀从库触发 sync 超时计数；raft 三节点断言 term/leader/commit_vec/voters/订阅者水位 + follower 视角 + 单机 `mode="none"`；mcli 渲染 mock 测试

## 延后（P4+，v1 明确不做）

- [ ] relation_wal 前缀 GC（独立水位：per-file 页 LSN 戳 + flush；重放幂等，仅体积增长）
- [ ] 跨分区事务 2PC（选主窗口原子性缺口文档化）
- [ ] ds 后端接入（raft 线程时钟特判）
- [ ] PG wire / Legacy 模式覆盖

## 风险登记

- fork 移植量大（~3k 行核心）：逐文件移植、对齐 TLA+ 已验证不变量，不引入新语义。
- 单事件循环吞吐靠 fx 批配聚合；raft 只过哈希，负载天然小。
- cluster 提交路径"先共识后落存储"，写延迟 +一次多数派往返，预期内，文档注明。
- io_uring 与 tokio 两条启动路径都接 repl listener，尽量共用实现，差异只在 listener。
- scupt-raft 算法出处：scuptio/scupt-raft（Apache-2.0，TLA+ 验证），fork 文件注明。
