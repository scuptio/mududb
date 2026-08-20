# MuduDB WAL 周期 fsync 特性与 1024 连接瓶颈调查报告（2026-07-31，R11 更新）

本文档接续 `perf_stdb_chase_report.md`（R5，2026-07-30），覆盖 R6-R11：
周期 fsync 特性落地、fsync 与写路径解耦、事件循环/栈级 profiling、
四项定向优化 A/B、两个剩余黑盒的测量归因。

环境：AMD Ryzen 9 3900X（24 逻辑/12 物理核），NVMe（nvme1n1p1，XFS）。
服务端 taskset 核 0-7（8 worker）或 0-11（12 worker），客户端核 8-11 或 12-15。
TPC-C（payment 50% / new-order 35%），40wh/100cust/100items（8 核档）与
80wh/400cust/200items（12 核档），32 分区，procedure-iouring 为主。

注意：白天测量受宿主机其他负载（IDE 进程 40GB+ RSS）严重干扰
（数据加载最慢放大 17 倍、TPS 打 4 折）；本文数字均取安静窗口。

## 1. SpacetimeDB 持久化事实核查

"SpacetimeDB 每次 commit 不 fsync" —— **属实**：

- 提交先落内存，经非阻塞队列交后台 durability actor，按批 drain 后才
  `flush_and_sync()`；fsync 异步、按批，不在 commit 路径上
  （`crates/durability/src/imp/local.rs`）。
- 1.x（benchmark 用的 1.12.0）客户端 ack 只代表内存提交；2.0 起默认
  confirmed reads（等 durable 才回客户端），`[commitlog]` 无 fsync 间隔配置。
- 对照：mududb 当时每个 commit 都在 `wait_group_commit_advanced` 等 fsync。

## 2. 落地特性：WAL 周期 fsync（`wal_sync_mode`）

```toml
wal_sync_mode = "periodic"     # commit（默认，行为不变）| periodic
wal_sync_interval_ms = 10      # 后台 fsync 间隔
```

- periodic 语义：commit ack 只等 WAL **写入 page cache**，不等 fsync；
  fsync 由**独立任务槽**（io_uring）/ 独立 tokio 任务按间隔执行；
  正常关闭强制最终 fsync。掉电最多丢失一个 interval 内已 ack 的提交
  （进程崩溃不丢）。等价于 PostgreSQL `synchronous_commit=off`。
- 前置修复：原 WAL 恢复对坏尾（撕裂帧/CRC 错/零填充）启动即死。
  周期 fsync 使未 fsync 尾部成为常态，故恢复改为**按 CRC 容错截断**
  （`scan_valid_frame_prefix`，带 warn 日志），commit 模式的
  write→fsync 崩溃窗口也一并修复。
- periodic 模式 group-commit batching 窗口归零（immediate flush）：
  fsync 已解耦，攒批窗口只增延迟无收益；commit 模式窗口不变。

## 3. 特性效果（8 核，40wh，procedure-iouring）

| 连接 | commit 基线 | periodic(初版,共槽) | periodic(解耦+立即flush) | SpacetimeDB |
|---|---|---|---|---|
| 64 | 3,977 | 4,397 | **10,279**（P99 12ms） | 23,403 |
| 256 | 7,128 | 8,077 | **10,131** | 21,284 |
| 512 | 8,846 | 9,772 | 7,745（±1461 噪音） | 18,554 |
| 1024 | 8,841 | 8,924 | 8,699 | 15,314 |

周期 fsync 本身只值 ~10%；配合写路径解耦后 64/256 连接 +112%/+48%，
但 512/1024 连接不动 → 高连接瓶颈不在 fsync/持久化。

## 4. 1024 连接瓶颈调查（R8-R11）

### 4.1 方法

内置埋点 + 栈采样（perf 被 `perf_event_paranoid=4` 且无 sudo 拒绝）：

- `MUDU_STAGE_STATS=1`：44 个 stage 的 wall/线程 CPU（每 worker 每 10s dump）。
- `MUDU_LOOP_STATS=1`（新增）：事件循环各阶段耗时。
- 栈采样：`LD_PRELOAD` 注入 `prctl(PR_SET_PTRACER)` + gstack 外部 attach（零干扰）。
- wchan/pidstat：内核阻塞函数与线程级 CPU。

### 4.2 稳态事实（1024 连接，对齐窗口）

- 事件循环健康：迭代 ~343µs，`wait_cqe` 占 ~88%（空闲），task_poll ~14-38µs，
  任务预算耗尽率 <1%。
- **无锁争用**：futex 样本 ~0；stmt_lock / stripe / file latch 全 µs 级。
- worker-0 CPU 十倍于其他 worker = page flush 扫描（crc32 校验）与
  meta 路由查询的真实工作，非 bug。
- 两端均不饱和：服务端 12 worker 合计 <1 核，客户端 ~10-28% 单核。
- 客户端埋点：`prep_p50=3µs` —— 单事务延迟 100% 发生在请求离开客户端之后。

### 4.3 四项优化 A/B（12 核，80wh，1024 连接，bench_p5_ab12.yaml，背靠背）

| # | 优化 | A（改前） | B（改后） | 判定 |
|---|---|---|---|---|
| 1 | page_flush 切片让出（每 32 页/每 relation yield） | 10,293 TPS / P99 165ms | 10,290 / 169ms | 中性 |
| 2 | WAL 写轮 4 槽并行 + 乱序水位 | 〃 | 9,907 / 167ms；wait_durable 29→27ms | 中性 |
| 3 | 文件系统（整库 tmpfs 对照） | 9,907 | 9,686 | FS 不是瓶颈 |
| 4 | 客户端 prep/wait 分解埋点 | — | prep 3µs / wait 78.6ms | 归因（客户端无辜） |

中性结果的收敛意义：写完成 ~2.6ms 在 tmpfs 上依旧 → 非磁盘；
4 槽并行无效 → 非槽位串行；page_flush 切片无效 → 本配置下非 P99 驱动。

### 4.4 顺带修复的真实 bug

- io_uring ring loop 关闭 wedge：shutdown 期间 mailbox eventfd 读不再重新
  武装，任务唤醒无法转化为 CQE，`wait_for_cqe` 永久阻塞（loop_mailbox.rs）。
- tokio `yield_now` 在 worker-ring 线程上永久 park（无 tokio 调度器），
  新增 `common/yield_now.rs::cooperative_yield_now()`（自唤醒 poll_fn）。
- periodic fsync 任务插入共槽后未 poll 即阻塞全队（初版 bug，已修）。

## 5. 两个剩余黑盒的测量结论（R11，2026-08-01）

新增 5 个 stage：`conn_read_wait`、`resp_send_wait`、`wal_queue_wait`、
`wal_wake_lag`、`commit_wait_pl_frames`（计数）。12 核 1024 连接稳态对齐窗口：

### 5.1 WAL 等待链 —— 三段全部排除，PL 嫌疑证伪

| 段 | 实测 avg |
|---|---|
| wal_queue_wait（enqueue→被 drain） | 1.5ms |
| wal_write_wait（写完成） | 2.6ms |
| wal_wake_lag（水位推进→waiter 恢复） | 1.4ms |
| **合计** | **~5.5ms ≪ wait_durable 28ms** |

`commit_wait_pl_frames = 0`：commit 等待的 LSN 就是自身 XL 入队末尾
（worker 级提交临界区串行，allocated==own last），PL 帧不扩展等待。
剩余 ~23ms 在"wait 开始 → 其 LSN 被水位覆盖"之间，三段埋点均不覆盖。
**下一个明确动作**：按 commit 采样的时间线 trace
（enqueue/wait-start/每轮 advance 的逐点时间戳）。

### 5.2 传输/投递段 —— 黑盒打开，单连接全周期分解成立

单连接完整周期 ~122ms（与客户端 wait_p50 78.6ms 及 TPS 口径互洽）：

| 段 | 实测 avg | 占周期 | 归属 |
|---|---|---|---|
| conn_read_wait（响应送达+客户端周转+请求传输） | 60.8ms | ~50% | 客户端/传输 |
| frame_handle（服务端 dispatch，含 commit） | 31.9ms | ~26% | 服务端 |
| resp_send_wait（服务端发响应） | 29.5ms | ~24% | 响应投递 |

结论：**1024 连接下没有资源饱和型瓶颈**（CPU<10%、无锁、非磁盘），
是逐连接延迟链：~50% 客户端周转与传输（1024 线程/4 核的调度与协议往返）、
~26% 服务端 dispatch（commit 等待仍是最大单项）、~24% 响应投递
（疑似客户端排干慢/TCP 窗口；ss Send-Q 探测受环境干扰未取得干净样本，
待安静窗口复核）。

## 6. 差距汇总与下一步（按 ROI）

当前：12 核 80wh@1024 mududb ~10-12k TPS vs SpacetimeDB ~15k（8 核口径）。

1. **commit 等待未闭合的 ~23ms**（靶：per-commit 时间线 trace）——服务端最大单项。
2. **响应投递 29.5ms**（靶：SO_SNDBUF/TCP 窗口、响应批量、客户端排干）。
3. **客户端周转 ~30-60ms**（1024 线程模型；客户端改协程化复用连接或减线程）。
4. 次级：冷启动首轮慢 2-4 倍（benchmark 纪律：warmup>0 或弃首轮）；
   interactive 模式高连接比 procedure 慢 ~1.8 倍。

## 7. 测量/配置资产

- 配置：`wal_sync_mode` / `wal_sync_interval_ms`（mudud.cfg，含模板注释）。
- 环境变量：`MUDU_STAGE_STATS=1`（44 stage）、`MUDU_LOOP_STATS=1`、
  `MUDU_WAL_FORCE_FLUSH=1`。
- 客户端结果行新增字段：`prep_p50_us/prep_p99_us/wait_p50_us/wait_p99_us/wait_max_us`
  （追加在行尾，bench_cross_db.py 兼容）。
- bench yaml（未入库，被清理后可按 §4.3 重建）：bench_p4_* / bench_p5_* 系列。
- 结果存档：`bench_cross_db_results/20260730_*`、`20260731_*`、`20260801_*`。

## 8. Write-heavy 秒杀负载（R12，2026-08-01）—— mududb 9-10× 胜 SpacetimeDB

新增 `--workload tpcc|seckill`（原 TPC-C 保留不变）：每事务 1 次热行 UPDATE
（扣库存）+ 1 次 INSERT（订单行，2KB payload），320 个促销 item 按 si_id
散到 32 分区。改动：`src/rust/seckill.rs`（mududb 过程）、`tpcc_benchmark.rs`
与 `spacetimedb/{module,client}`（同构实现）、`bench_cross_db.py` 透传
（workload/seckill_items/seckill_payload_bytes）、`bench_seckill.yaml`。

| 连接 | mududb proc+iouring | SpacetimeDB | 倍数 |
|---|---|---|---|
| 64 | ~12-15k | 2,811 | ~5× |
| 256 | **24,227** | 2,525 | **9.6×** |
| 512 | **25,463** | 2,443 | **10.4×** |
| 1024 | **22,211** | 2,379 | **9.3×** |

（12 核，80wh 规模参数，全部真实写入：sold_out=0、abort=0、
库存扣减数==操作数。stdb P99 在 1024 连接达 1.6s。）

胜负机理：stdb 每库单线程 reducer，2KB 订单序列化+commitlog 追加全部压
在单线程上（~2.4-2.8k TPS 封顶）；mududb 12 worker 并行 + 分区本地化
（item 按 worker 亲和选择）+ periodic fsync，写路径横向扩展。

### 8.1 排查中发现并修掉的三个问题

1. **benchmark 公平性 bug（已修）**：客户端 item 选择未按 worker 亲和，
   跨分区点读返回空 → ~75% 事务退化为 sold_out 空操作（虚高 TPS）。
   修复：终端按 warehouse→worker 亲和只买本 worker 分区的 item。
2. **mududb 真 bug（已修复）**：单条多行 INSERT 的行跨**同一
   worker 的多个分区**时，只有一行落库（非确定哪行），却报告全部
   affected_rows。根因：`worker_storage.rs::apply_cross_partition_tx_async`
   的幂等去重按 `tx_id` 单键，coordinator 按分区逐次调用时同 worker 的
   第二个分区被误判为已应用而跳过；修复为按 `(tx_id, partition_id)` 去重。
   回归：一条 32 行跨 32 分区 INSERT 全部落库（修复前仅 8 行）。
3. **跨分区点读返回空/50023（已修复）**：根因是
   `x_contract/ops.rs` 五处（insert/read_key/read_range/delete/update）
   卫语句 `self.worker_id != 0 &&`——worker 0 永远走不到远程分支，
   对外来分区落到本地分支：点读创建空影子 relation 返回空；
   影子文件损坏时错误向上传播致连接被重置（客户端 50023）。
   交互 SQL 同样受影响（同一路由）。修复：删除该错误子句。
   回归：worker 0 对其他分区的点读正常返回行。
   修复后复测（全部真实写入）：seckill 64/256/512/1024 =
   **13,548 / 24,365 / 25,452 / 24,832 TPS**（stdb 2,816/2,542/2,474/2,198，
   **4.8-11.3×**）；TPC-C 回归 256 连接 11,837 TPS（无退化）。

## 9. TPC-C 冲突/写量扫描与交叉曲线（R13，2026-08-01）

旋钮（原 TPC-C 行为默认不变）：`--hot-rows-per-warehouse K`（每仓 K 个热点行，
每 op 追加一次热点行更新小事务）；`--order-lines L`（new-order 固定行数）。
双侧同构实现（mududb 过程 `tpcc_hotspot_hit`、stdb reducer + 表；
两客户端与 harness 透传）。12 核 / 256 连接 / 80wh / periodic 10ms。

### 9.1 冲突扫描（K：0/1/8/64）

| K | mududb | stdb | mududb/stdb |
|---|---|---|---|
| 0（关闭） | 12,212 | 12,145 | 1.01×（平） |
| 64 | 8,531 | 6,357 | **1.34×** |
| 8 | 8,895 | 6,488 | **1.37×** |
| 1（最强冲突） | 9,538 | 6,448 | **1.48×** |

结论：mududb 全面 ≥ stdb。热点行对 mududb 几乎无感（8.5-9.5k，K=1 反而
最高——分区锁管线本来就按分区串行）；stdb 降 ~47%，主因是热点更新使它
每 op 多一次串行事务（单线程两跳/op）。注意当日 stdb 基线（12.1k）低于
历史值（~20k，8 核 40wh 口径），跨日绝对值不可比；同口径对比有效。
图：`bench_cross_db_results/sweep_20260801_124152/tps_vs_hot_rows.png`。

### 9.2 写量扫描（L：3/8/16/32）—— 真实交叉点 ~L=5.5

| L | mududb | stdb | 胜者 |
|---|---|---|---|
| 3 | **14,148** | 12,131 | mududb 1.17× |
| 8 | 9,225 | 11,887 | stdb 1.29× |
| 16 | 5,877 | 11,685 | stdb 1.99× |
| 32 | 3,364 | 11,569 | stdb 3.44× |

与直觉相反但机理清晰：**stdb 每行成本 ~µs（内存 find+update+insert），
L 从 3→32 只降 5%；mududb 每行走完整 MVCC/语句锁/页操作/WAL 路径
（~2ms/行），L 越大线性坍塌**。结论：大量小事务的写负载 mududb 并行
管线胜（L≤5），大事务（多行写）stdb 内存模型胜——mududb 的每语句固定
开销是大事务场景的优化靶点。
图：`bench_cross_db_results/sweep_20260801_124152/tps_vs_order_lines.png`。

### 9.3 用户预期 vs 实测

- "高冲突 mududb 超过 stdb" —— **成立**（K 任意值 mududb 不输，强冲突 +48%）。
- "高写入 mududb 超过 stdb" —— **不成立**：L≥8 后 stdb 反而拉开（最高 3.4×）。
  写量维度 mududb 的优势在 L≤5 的小区段（+17%）。

## 10. 大事务瓶颈定位与优化（R14，2026-08-01）

L=32 stage profile 定位每行成本（全部 44 stage，安静窗口对齐）：
**stmt_lock 626µs×26（2PL 热行排队，负载固有）、wr_stripe_wait 3.5ms×4.4
（write_rows 条纹锁）、wr_publish 614µs×8.8（阈值 flush 内联在 write_latch
里拖住全部写者）、storage_apply 23.8ms/commit**。

修复（`time_series_file/plan.rs`+`write.rs`、`relation/relation.rs`）：
1. 256 页阈值 flush 移出 write_latch（改由调用方在 latch 释放后执行）——
   wr_publish 614µs → **12µs**，wr_file_latch 归零。
2. 条纹锁 64→512（降 ~8× 假共享；Box 化避免 future 栈溢出）。
3. stmt_lock 经审查无固定成本缺陷（2PL 语义需要，保留）。

安静窗口复测（L=32）：**TPS 2094→2318（+11%）**，wr_stripe_wait -14%，
storage_apply -9%，wait_durable -11%；TPC-C 回归 10,851 ✓；
635 测试全绿 + clippy 干净。
写量扫描（部分时段负载 5-7，有噪音）：交叉点未实质右移——L≥8 后
stmt_lock（26 次/事务的 2PL 排队）与 storage_apply 的每页应用成本仍是
大事务主成本，属于下一轮结构性优化（每语句路径精简/批量应用）。

### 10.1 page_size 扫描（4K/16K/64K × L=8/32）—— 不是杠杆

| page_size | L=8 TPS | L=32 TPS |
|---|---|---|
| 4096（现值） | 9,548 | 3,324 |
| 16384 | 9,952 | 3,276 |
| 65536 | 9,078 | 3,358 |

±4% 噪音内，无显著差异。L=32 stage 分解佐证：wr_locate 次数两档同为 52/事务
（行按键散布，页变大并不减少定位次数）、storage_apply 21.7ms vs 21.0ms、
stmt_lock/wr_stripe_wait 不变。**每行成本的主体是行级工作（2PL 锁、版本链、
行级 WAL 帧、语句固定开销），不是页级工作**——page_size 不是大事务瓶颈的杠杆。
`bench_cross_db.py` 已加 `page_size` 透传（mudud yaml 段），便于后续复测。

### 10.2 条纹锁范围修复（R15）—— storage_apply 再降 42%

空间分析的关键发现：storage_apply 的 ~340µs/行中约 85% 是**条纹锁持有过长**
（`write_rows` 把条纹持有贯穿整个按关系应用：解析+两批写+索引插入 ~1-2ms），
并发大批次在条纹上 convoy；真正的行级工作很小（追加写定位 O(1)、页级 WAL
已是每页一帧）。

修复（relation.rs）：条纹锁只在 tuple-id 预留（µs 级）持有，文件写由
write_latch 串行、版本由 DataRow 自同步。语义不变（635 测试全绿）。

累计效果（L=32，安静窗口）：

| 指标 | 优化前 | flush-latch 修复 | +条纹修复 | 累计 |
|---|---|---|---|---|
| TPS | 2,094 | 2,318 | **2,812** | **+34%** |
| storage_apply | 23.8ms | 21.7ms | **12.5ms** | -47% |
| wr_stripe_wait | 3,517µs | 3,017µs | **28µs** | **-99%** |
| wr_publish | 614µs | 12µs | 5.5µs | -99% |
| proc_invoke | ~58ms | 52.5ms | **40.7ms** | -30% |

TPC-C 回归 11,270 ✓（较修复前 10,851 亦有提升）。
剩余大事务主项：stmt_lock 588µs×26 ≈ 15.3ms/事务（2PL 热行排队，需
"可交换条件 delta"把 stock 更新改免锁，已论证可交换性 `((x-10-q) mod 91)+10`，
涉及 delta 契约扩展，列为独立任务）；wait_durable ~12.5ms。

### 10.3 wait_durable 两项实验（均否决，R16）

同窗对照（L=32，各自安静门槛后启动）：

| 配置 | wal_write_wait | wait_durable | 判定 |
|---|---|---|---|
| 基线 interval=10, 4 槽 | 5.5ms | 12.6ms | — |
| exp1: interval=100 | 5.0ms | 12.2ms | **无差异** |
| exp2: 8 写槽 | 6.2ms | 13.7ms | **无提升** |

- "周期 fsync 在 inode 级阻塞并发写"假设**否决**：10ms→100ms 无变化。
- "写槽饥饿"假设**否决**：4→8 槽无提升（且并发合批测试假定单轮语义被破坏，已回退为 4）。
- 当前账目（L=32）：wal_queue_wait ~3.7ms + 写轮完成 ~5ms + wal_wake_lag
  ~2.6-3.3ms ≈ wait_durable ~12.5ms，大头是**写轮完成延迟本身**（page-cache
  写完成要 ~5ms，tmpfs 同速，非设备）——下一步的测量靶子改为
  "写轮提交→完成拾取"链路的采样 trace（io_uring submit/CQE/任务恢复逐点）。

### 10.4 写轮延迟 trace（R17）—— 6ms 在写完成等待内，不在提交侧

`wal_prep_submit`（execute_flush_batch 开始→写 SQE 全部入环）实测：

| 段 | L=32 avg |
|---|---|
| wal_prep_submit（checkout+提交） | 0.67ms |
| **wal_write_wait（写 handle 完成等待）** | **6.3ms** |
| flush_round 合计 | 7.0ms |
| wal_queue_wait / wal_wake_lag | 4.0 / 3.5ms |

写轮 7ms 里提交侧只占 0.67ms，**~90% 在"SQE 已提交、等内核完成"段**。
设备侧已排除（tmpfs 同速）；fsync 间隔/写槽数均否决（§10.3）。
剩余嫌疑：io_uring 缓冲写经 io_wq 线程执行，在内存回收压力
（kswapd/direct-reclaim，宿主机 IDE 占 40GB+）下被 stall；
或 CQE 拾取粒度。下一步验证：空闲内存重测 + /proc/pressure/memory 相关性。

### 10.5 写完成延迟根因：io_uring 缓冲写 vs 直接 pwrite（R18，重大突破）

| 配置 | flush_round | wal_write_wait | wait_durable | TPS |
|---|---|---|---|---|
| io_uring 写 + 同迭代重poll（实验B） | 6.9ms | 6.1ms | 14.7ms | 2,427 |
| **pwrite 直写（实验A, MUDU_WAL_PWRITE=1）** | **0.63ms** | **0.62ms** | **9.5ms** | **2,707** |

- **根因坐实**：WAL 写轮的 ~6ms 不在提交、不在设备、不在 fsync、不在槽数，
  而在 **io_uring 缓冲写的内核 io_wq 线程调度路径**——SQE 由 io_wq 线程
  执行，其唤醒/调度延迟达毫秒级。改为 flush 驱动线程直接 `pwrite(2)` 后
  写轮降到 ~0.6ms（其中大半是 ~200KB 批次的 page-cache 拷贝本身）。
- 实验B（CQE 后同迭代重 poll flush 任务）：无显著效果（拾取链路本来就紧）。
- 实验A 现为环境开关 `MUDU_WAL_PWRITE=1`（flush.rs 内 env-gated 分支）。
  代价：写期间阻塞本 worker 循环 ~0.6ms/轮（WAL 批写是上界明确的小写，
  可接受）；建议后续提为正式配置项或默认。
- 累计：L=32 TPS 2,094（优化前）→ **2,707（+29%）**；wait_durable
  12.5ms → 9.5ms。剩余：wal_queue_wait ~3.7ms、wal_wake_lag ~2.7ms、
  stmt_lock（§10.2 独立任务）。

### 10.6 io_wq 亲和实验（MUDU_IOWQ_AFF=1）—— 边际无效

实现：`IoUring::register_iowq_affinity`（worker N 绑 core N）+
`register_iowq_max_workers(128,512)`（raw opcode 17/19 syscall，env 门控）。

| 配置 | wal_write_wait | flush_round | TPS |
|---|---|---|---|
| io_uring 写基线 | 6.1ms | 6.9ms | 2,427 |
| + io_wq 亲和/上限 | 5.6ms（-9%） | 6.2ms | 2,651 |
| pwrite 直写（参照） | **0.62ms（-90%）** | **0.63ms** | 2,707 |

结论：io_wq 延迟的主成分**不是"被排到忙核"**（亲和只值 9%），而是
io_wq 跨线程"提交-执行-唤醒-恢复"路径本身（或其内核内部阻塞）。
对 WAL 这种上界明确的小批写，io_uring 缓冲写在结构上就无法逼近
pwrite——工程结论维持 §10.5：WAL 写路径用 pwrite（MUDU_WAL_PWRITE=1），
io_uring 保留给网络收发与 fsync。TPS 三者接近（2427/2651/2707）说明
wal_write_wait 已不是当前 TPS 的第一主项（wal_queue_wait ~3.9ms +
wal_wake_lag ~2.9ms 与 stmt_lock 更受关注）。

### 10.7 stmt_lock 可交换 delta 免锁（R19）+ queue/wake 归因

**delta 免锁落地**：新增延迟求值 delta 契约（ADD_DEFERRED/SUB_DEFERRED/
SUB_WRAP_DEFERRED，op 2/3/4；`SubWrap = ((x-floor-q) mod wrap)+floor`，
已证可交换）。stock 更新改走该契约：免读旧值、免语句锁、apply 时行锁内
对最新值求值。两处 fallback 修复：tpcc 包 SQL fallback 改过程内锁内 RMW
（mududb SQL 无 CASE/非键谓词）；sqlite adapter 用 CASE 单语句。
纯插入路径仍走原 2PL；district 计数器刻意保留锁（分配语义不可交换）。

| 指标（L=32） | 修复前 | 修复后 |
|---|---|---|
| stmt_lock 次数/事务 | 26.1 | **14.9** |
| L=32 profile TPS | 2,281 | **2,576（+13%）** |
| 写量扫描 L=32 | 3,301 | **3,827（+16%）** |
| TPC-C 回归（256 连接） | 11,270 | **11,501** |

640 测试全绿（新增 delta 交换性/免锁/回放/混合拒绝 5 个测试）。

**queue/wake 归因**：`MUDU_LOOP_STATS` 显示循环迭代 261µs、82% 空闲、
预算耗尽率 0.6%——不是节奏/饥饿问题；ANotify 唤醒路径（sticky signaled
flag + waker）本身即时。两项 lag（~3.7/2.8ms）落在任务注册表调度域，
TASK_POLL_BUDGET 8→64 无改善（已回退 8）。维持现状，留作后续观察。
