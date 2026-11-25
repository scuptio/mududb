# Queue 信封与内部水位 v1

## 范围

队列结果复用 `tuple_binary_v1.md` 与 `protocol_frame_v1.md`，不增加 WIT
接口、syscall 编号或 server-push 帧。本文定义其列布局以及事务内持久水位。

## 信封

`RECEIVE` 按顺序返回 `__seq`、`__lsn`、队列声明的 payload 列。
前两列使用现有 `I64` 非负整数族；payload 保持原来的类型、顺序、NULL
编码。内部 `status` 和 `receive_count` 不出现在结果中。
`__seq` 是队列内序号；`__lsn` 当前仍为入队事务 xid，并非物理 WAL 字节偏移。

## 持久水位

内部 worker KV key 为 ASCII `\0mudu.queue.state.v1/` 加 16 字节大端 queue OID。
该 key 不属于用户 SQL schema。value 固定 33 字节：

| 偏移 | 长度 | 内容 |
|---|---|---|
| 0 | 1 | 版本 `1` |
| 1 | 8 | `next_seq`，大端 u64 |
| 9 | 8 | `frontier`，大端 u64；最小保留序号，空队列取 next_seq |
| 17 | 8 | `depth`，大端 u64；实际保留消息数 |
| 25 | 8 | `bytes`，大端 u64；编码 payload 的字节总数 |

消息和水位写在同一普通 XL 事务中；commit 后才发布内存统计，rollback
同时撤销二者。分配在每队列事务锁内执行，锁到提交/回滚释放；无关队列不共用锁。
ACK 后即使不再保留消息，也不删除水位，保证重启后不会复用已提交序号。

消息数及字节限额在分配时检查。字节数不含系统列、tuple/page 开销，NULL
字段贡献零字节。`overflow_count` 是进程内拒绝次数，不随事务回滚撤销，
也不作为持久恢复承诺。未知版本、截断和超长 value 均解码失败。

## 兼容矩阵

| 数据 | 当前 reader | 旧版 reader |
|---|---|---|
| 既有 tuple/protocol v1 队列结果 | 支持，布局不变 | 支持 |
| 没有独立水位的旧队列 | 首次操作从现存消息迁移 | 支持原有语义 |
| 水位 v1 | 支持 | KV 外层可读，但不维护该水位；不支持降级后继续写队列 |
| 未知水位版本 | 明确 Decode 错误 | 不支持 |

旧数据库若已经清空且丢失历史最高序号，无法从空 relation 反推出历史位点；
本实现不声称恢复这类历史信息。升级后产生的水位保持持久单调。

Golden fixture：`crates/db-kernel/testing/fixtures/golden/v1/queue_state_v1.bin`，
固定值 `(next=9, frontier=4, depth=5, bytes=123)`；单测检查逐字节编码、
解码及未知/截断版本。外层 LogFrame、TupleBinary、ProtocolFrame 版本不变。
