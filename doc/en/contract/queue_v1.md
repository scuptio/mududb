# Queue envelope and internal watermark v1

RECEIVE reuses TupleBinary v1 and ProtocolFrame v1. Its columns are `__seq`,
`__lsn`, then the declared payload columns, preserving payload types and NULLs.
The leading columns are nonnegative I64 values. Internal claim status and
receive count are omitted. Currently `__lsn` is the enqueue transaction xid,
not a physical WAL byte offset. No WIT or syscall changes are introduced.

The private worker KV key is the ASCII prefix `\0mudu.queue.state.v1/` followed
by the queue OID as 16 big-endian bytes. Its value is exactly 33 bytes:

| Offset | Size | Value |
|---|---|---|
| 0 | 1 | Version 1 |
| 1 | 8 | Next sequence, big-endian u64 |
| 9 | 8 | First retained sequence, or next sequence when empty |
| 17 | 8 | Number of retained messages, big-endian u64 |
| 25 | 8 | Encoded payload bytes, big-endian u64 |

Watermarks and messages commit in the same ordinary XL transaction. Queue
state locks serialize allocation until commit or rollback; unrelated queues
use different locks. Memory statistics publish only committed state. Draining
a queue retains its durable watermark. NULL payloads contribute zero bytes;
system columns, tuple and page overhead are excluded. Overflow counts are
process-local rejected attempts, unaffected by transaction rollback.

Compatibility: existing tuple/protocol results are unchanged. Old queues
without a watermark migrate from live rows on first use. A historically empty
old queue cannot recover its already-lost high watermark. Current readers reject
unknown versions and non-33-byte values. Old binaries can decode the enclosing
KV WAL but do not maintain these watermarks: downgrading and continuing queue
writes is unsupported. Outer LogFrame, TupleBinary and ProtocolFrame versions
do not change.

The golden fixture `crates/db-kernel/testing/fixtures/golden/v1/queue_state_v1.bin`
encodes `(9, 4, 5, 123)`. Tests pin bytes and reject truncated/unknown versions.
