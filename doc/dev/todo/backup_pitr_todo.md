# MuduDB 备份 + PITR（Point-In-Time Recovery）— 设计与实施 Todo

> 架构：物理基础备份（base backup）复用复制已有的 per-worker 快照例程；WAL 连续归档（archiving）复用 `.gcfrontier` 的水位推进思想；恢复 = 基础备份 + 归档 WAL 重放到目标点（LSN / 提交时间 / xid）。重放引擎复用 sink/applier。
>
> 核心不变量（PG 同款）：**任何被 GC 删除的 WAL chunk，必须先已被归档**。GC 水位 = `min(在线订阅者 ack, 已提交点, 快照点, 归档水位)`。

## 设计要点（共识约定）

- **三个制品**：基础备份（`backup/<label>/worker_N/v<lsn>/` + 全局 `BACKUP_LABEL`）、WAL 归档（`archive_dir/<timeline>/<short_oid>/<seq>.xl`）、恢复目标（base + replay to target）。
- **一致性点**：mududb 的 MVCC = xid 可见性 + 页 LSN 幂等重放，因此「一致」= 快照点前所有已提交事务已 flush+fsync 到数据文件（`take_worker_snapshot` 已保证）。**无需** PG 式 `pg_start_backup` 停顿窗口——页 LSN 戳让半页写可被 WAL 重放修复（PG `full_page_writes` 的等价物天然具备）。
- **raft 模式**：`CommitVec` 提供全局一致点，跨 worker 原子对齐，RPO=0。
- **单机/stream 模式**：跨 worker 2PC 未实现，PITR 是「各 worker 各自一致」（各自 `base_lsn` 重放到各自目标），文档写明边界。
- **时间线（timeline）**：PITR 后继续运行产生历史分叉，必须区分。一期单时间线（timeline=1，PITR 后旧归档终结、新归档走新目录），二期多时间线分叉管理。

## 工程约定（mududb 规则）

- 归档器独立线程，用 `mudu_sys::fs::async_::*` + `spawn_thread`，错误一律 `RS<T>` + `mudu_error!` + `EC`。
- 归档水位持久化：`.archfrontier` 姊妹文件（`{first_unarchived_seq, lsn}`），镜像 `.gcfrontier` 的实现。
- 归档副本写完后 fsync + 长度/CRC 校验，不匹配重试；源 chunk 与归档副本 CRC64 比对。
- 复用 `WorkerLogLayout` 的 `chunk_paths_sorted_async` / `parse_chunk_sequence` / `scan_valid_frame_prefix`。
- 不破坏 `deny.toml`；无外部 git 依赖。

## v1 实施（分阶段提交）

### 提交一（P1 WAL 归档器 + 归档水位 + GC 门槛）

- [x] `WorkerLogLayout` 增 `archive_dir`（`with_archive_dir` builder）+ `ArchiveFrontier`（`.archfrontier` 持久化：`load_archive_frontier` / `store_archive_frontier`）
- [x] `archive_prefix_chunks(fs)`：把已封口（非 tail）chunk 复制到 `archive_dir/<short_oid>/<seq>.xl`，fsync + 校验，推进归档水位；返回 `(archived_bytes, archived_chunks)`
- [x] GC 门槛收紧：`gc_prefix_chunks` 在配置了 `archive_dir` 时，删除水位封顶为 `min(frontier_lsn, archive_frontier.lsn)`——未归档 chunk 永不删除
- [x] 单测：归档往返（源 == 归档副本）、水位推进、GC 不越过归档水位、归档水位持久化恢复

### 提交二（P2 归档器任务 + 配置）

- [x] `[backup]` 配置段：`archive_dir` / `archive_mode`（`off|on|always`）/ `backup_dir`（runtime `BackupCfg` + kernel `BackupCfg`）
- [x] 每 server 一条归档任务（独立线程，单机与复制模式都跑）：周期性 `archive_prefix_chunks`，把每个 worker 的已封口 chunk 归档，推进 `.archfrontier`
- [x] `archive_dir` 从 `ServerCfg` 经 `WorkerBuildConfig`/`WorkerRuntimeParams` 接入 `WorkerLogLayout.with_archive_dir`
- [x] 归档水位纳入 GC 门槛（`gc_prefix_chunks` 删除水位封顶 `min(frontier_lsn, archive_frontier.lsn)`）
- [ ] 观测：归档字节/chunk 计数器进 `GET /mudu/server/replication` 的 `counters`（或独立 `backup` 段）

### 提交三（P3 基础备份 + `mcli backup`）

- [x] `BACKUP_LABEL`（msgpack）：label / timeline / `workers[{worker, base_lsn, seg_hash}]` / taken_at
- [x] `run_base_backup`：遍历 worker 调 `take_worker_snapshot` → 拷贝快照到 `backup/<label>/` → 写 `BACKUP_LABEL` 最后落盘（原子发布）
- [x] `load_backup_label` / 备份标签序列化往返 + 损坏忽略单测
- [x] `mcli backup` 子命令 + HTTP 端点 `POST /mudu/backup`（调用 `run_base_backup`，消除 `#[allow(dead_code)]`）
- [x] `mcli backup --list` / `--verify` / `--delete`（verify 校验 base backup 快照文件 CRC；归档链完整性校验未做）

### 提交四（P4 恢复 + PITR）

- [x] `mcli restore --label --target-dir`：落基础备份（`install_snapshot_relation_files` + `read_kv_dump`）→ 从归档按序拉 WAL → 重放到 EOF
- [x] `mcli pitr --label --target-dir (--time|--lsn|--xid)`：重放引擎 `stop-at-target`
- [x] 提交时间戳：`TxOp::Commit` 增 `commit_ts`（混合逻辑时钟）；时间→LSN 翻译改为恢复时扫描归档 WAL（未落 `commit_ts_index` 持久化索引）
- [x] 重放引擎复用 applier，包一层 stop 判定（`Commit.commit_ts > target` 即停；无目标跑到 EOF）；页 LSN 戳保证精确帧边界
- [x] 收尾：重建 KV 索引（写回目标 WAL）、写 `.gcfrontier`（`.archfrontier` 未写），输出恢复报告（LSN/时间/xid）

### 提交五（P5 时间线 + 云归档 + 保留策略）

- [ ] timeline 并入归档路径（`archive_dir/<timeline>/...`）与 `BACKUP_LABEL`；PITR 后生成新 timeline
- [ ] `archive_command`（外部脚本/S3/NFS）可插拔
- [ ] 备份保留策略（`backup_retention` / `--keep N`）与自动清理
- [ ] raft 模式：timeline 并入 CommitVec/term 语义（或独立 history 文件），防陈旧 leader 复活

## 延后（v1 明确不做）

- [ ] 多时间线分叉的完整管理（一期单时间线）
- [ ] 跨 worker 全局原子 PITR（依赖跨分区 2PC）
- [x] 逻辑导出/恢复（`mcli dump` / `mcli restore-logical`，DDL + CSV 打包为 ZIP；分区表数据暂只导元数据）
- [ ] 备份加密 / 云 KMS 集成

## 风险登记

- 提交时间戳需动 WAL 格式——唯一兼容性影响点，按「新增 payload kind」设计并配 golden 测试。
- KV store 在内存：需确认 KV 写已入 WAL 流，否则 KV 的 PITR 会缺一段。
- 异步归档下最后 chunk 可能未归档即宕机（RPO 上限），`archive_mode=always` + 同步落盘才趋近 0。
- 多 worker 单机/stream 一致性边界需在文档写清。

## 参考

- 复制 + Raft 设计 Todo：`doc/dev/todo/replication_raft_todo.md`
- 快照例程：`crates/db-kernel/mudu_kernel/src/server/repl/snapshot.rs`
- WAL 布局/GC：`crates/db-kernel/mudu_kernel/src/wal/worker_wal_backend/layout.rs`
- 复制维护循环：`crates/db-kernel/mudu_kernel/src/server/repl/maintain.rs`
- WAL 条目：`crates/db-kernel/mudu_kernel/src/wal/xl_entry.rs`
