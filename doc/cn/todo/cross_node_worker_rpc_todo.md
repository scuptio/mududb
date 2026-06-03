# 跨节点 Worker RPC TODO 需求

## 背景

当前 worker-to-worker RPC 已经有统一的 `MessageBus` 抽象，并且 partition RPC 已经基于该抽象实现 request/response：

- `EndpointId` 当前等同于 `worker_id`
- `Envelope.src` 和 `Envelope.dst` 都表示 worker id
- `MessageBus::send(dst_worker_id, message)` 由当前 worker bus 自动填充 source worker id
- `MessageBus::recv(filter)` 按 worker id、message kind 和 correlation id 过滤消息

现阶段的限制是：`MessageBus` 只能把 `worker_id` 解析成本进程内 worker index，再投递到本地 mailbox。跨节点 worker 还没有网络传输实现。

本 TODO 的目标是实现跨节点 worker RPC，同时保持 worker 作为第一级抽象。

## 核心要求

### Worker 是一级寻址抽象

RPC 调用方只面向 worker id：

```text
from_worker_id -> to_worker_id
```

调用方不应直接传入远端地址、端口或 node 地址。

发送接口语义保持：

```rust
bus.send(to_worker_id, message)
```

其中：

- `from_worker_id` 来自当前 bus 的 `local_endpoint()`
- `to_worker_id` 是目标 worker id
- 网络地址只由拓扑路由层解析

### EndpointId 语义

`EndpointId` 保持为 worker id 类型别名：

```rust
pub type EndpointId = OID;
```

当 endpoint 来源于 worker 时，它就是 `worker_id`。

`Envelope` 中必须保留 worker 级 src/dst：

```rust
Envelope {
    src: from_worker_id,
    dst: to_worker_id,
    ...
}
```

网络传输层只能承载该 envelope，不能把 RPC API 改成 address-first。

### Port Sharding 是拓扑属性

一个节点可以有多个 worker 端口。端口分片不改变 RPC API。

拓扑应维护：

```text
worker_id -> node_id + rpc_addr
```

示例：

```text
worker_0_on_node_a -> 10.0.0.1:9527
worker_1_on_node_a -> 10.0.0.1:9528
worker_0_on_node_b -> 10.0.0.2:9527
worker_1_on_node_b -> 10.0.0.2:9528
```

调用方仍然只传 `worker_id`。

## 拓扑需求

启动恢复时需要得到完整 worker 拓扑。

建议新增：

```rust
pub struct ClusterTopology {
    pub local_node_id: OID,
    pub workers: HashMap<OID, WorkerEndpoint>,
}

pub enum WorkerEndpoint {
    Local {
        worker_id: OID,
        worker_index: usize,
        rpc_addr: SocketAddr,
    },
    Remote {
        worker_id: OID,
        node_id: OID,
        rpc_addr: SocketAddr,
    },
}
```

路由接口：

```rust
fn route_worker(worker_id: OID) -> RS<WorkerEndpoint>;
```

路由必须是 worker-first：

```text
worker_id -> Local(worker_index) | Remote(node_id, rpc_addr)
```

不能设计成：

```text
addr -> worker
```

## MessageBus 改造

当前 IOUring 和 Tokio 后端都已有自己的本地投递实现。

需要把本地-only 路由：

```text
worker_id -> local worker_index
```

升级为：

```text
worker_id -> Local(worker_index) | Remote(rpc_addr)
```

### 本地路径

本地 worker 保持现有路径：

- IOUring：`SegQueue<WorkerMailboxMsg>` + eventfd 唤醒
- Tokio：`SegQueue<Envelope>` + `Notify` 唤醒

### 远端路径

远端 worker 走网络传输：

```text
MessageBus::send(to_worker_id, msg)
  -> build Envelope { src: local_worker_id, dst: to_worker_id, ... }
  -> ClusterTopology::route_worker(to_worker_id)
  -> Remote { rpc_addr, ... }
  -> NetworkTransport::send(envelope)
```

## 网络传输需求

新增网络 envelope 编码。

建议 wire format：

```rust
struct WireEnvelope {
    version: u16,
    src_worker_id: OID,
    dst_worker_id: OID,
    msg_id: u64,
    correlation_id: Option<u64>,
    kind: MessageKind,
    delivery: DeliveryMode,
    payload: Vec<u8>,
}
```

TCP frame 建议：

```text
magic u32
version u16
flags u16
frame_len u32
payload bytes
```

初版要求：

- 有协议版本
- 有最大 frame size 限制
- 支持 request/response correlation
- 断线时 pending request 由调用方 timeout
- 不做自动重试，避免非幂等写请求重复执行

## 入站分发

网络 reader 收到 `WireEnvelope` 后，应按 `dst_worker_id` 投递到本节点对应 worker bus。

建议把 `handle_incoming` 提升到 `MessageBus` trait：

```rust
trait MessageBus {
    fn local_endpoint(&self) -> EndpointId;
    async fn send(&self, dst: EndpointId, message: OutgoingMessage) -> RS<MessageId>;
    async fn recv(&self, filter: RecvFilter) -> RS<Envelope>;
    fn on_recv_callback(&self, filter: RecvFilter, callback: OnRecvCallback) -> RS<SubscriptionId>;
    fn cancel_callback(&self, id: SubscriptionId) -> RS<bool>;
    fn handle_incoming(&self, envelope: Envelope) -> RS<()>;
}
```

节点级 dispatcher：

```text
network reader
  -> decode WireEnvelope
  -> dst_worker_id
  -> local bus registry
  -> target_bus.handle_incoming(envelope)
```

网络 reader 不能依赖 `current_message_bus()`，因为它不一定运行在目标 worker 线程中。

## IOUring 与 Tokio 支持

### Tokio 后端

Tokio 后端实现原生 TCP transport：

- 每个 remote worker 或 remote node 维护连接
- writer task 从队列取 envelope 并写 socket
- reader task 读 frame 并分发到目标 worker bus

### IOUring 后端

第一阶段建议复用同一套 Tokio network transport：

- IOUring worker 的本地消息继续走 eventfd mailbox
- IOUring worker 的远端消息投递给共享 network transport
- network transport 收到远端消息后，通过目标 worker 的现有 mailbox/eventfd 路径注入

第二阶段如有性能需求，再实现纯 io_uring socket transport。

## Partition RPC 适配

partition RPC 的业务层不应改变。

现有逻辑应继续成立：

```rust
bus.send(target_worker_id, request)
bus.recv(RecvFilter {
    src: Some(target_worker_id),
    dst: Some(self.worker_id),
    correlation_id: Some(msg_id),
    ...
})
```

响应端继续使用请求 envelope 的 source worker：

```rust
bus.send(*request_envelope.src(), response)
```

这里发送目标仍然是 worker id，不是地址。

## 非目标

本 TODO 不要求实现跨 worker 原子事务。

跨节点 worker RPC 可用后，远端写请求仍然只是远端 worker 上的独立事务提交。完整跨 worker 原子提交仍需要：

- 分布式事务协调器
- prepare/commit/abort participant 状态
- participant WAL
- coordinator recovery
- timeout 和不确定状态处理

## 验收标准

1. IOUring 后端本地 worker RPC 行为不回退。
2. Tokio 后端本地 worker RPC 行为不回退。
3. IOUring 后端可以向远端 worker id 发送 request 并收到 response。
4. Tokio 后端可以向远端 worker id 发送 request 并收到 response。
5. 调用方 API 只使用 worker id，不直接暴露远端地址。
6. port sharding 通过 topology 解析，不进入 RPC 调用参数。
7. partition RPC 可跨节点发送 `ReadKey` / `ReadRange` request。
8. 跨节点写 RPC 可以转发执行，但文档明确说明它不是跨 worker 原子事务。
9. 网络断开时 request 能通过 timeout 退出，不永久挂起。
10. `Envelope.src` / `Envelope.dst` 在跨节点后仍保持 worker id 语义。

## 建议实施顺序

1. 增加 `ClusterTopology` / `WorkerEndpoint`。
2. 启动恢复阶段构建本地 worker endpoint，并预留加载远端 worker endpoint 的配置入口。
3. 把 `MessageBus::handle_incoming` 提升到 trait。
4. 将 IOUring/Tokio 的 route 逻辑改为使用 `ClusterTopology::route_worker`。
5. 保持本地路径使用现有 mailbox。
6. 实现 Tokio TCP network transport。
7. 让 IOUring 后端复用 Tokio network transport。
8. 增加跨节点 envelope frame 编解码。
9. 增加节点级 inbound dispatcher。
10. 增加跨节点 partition RPC 集成测试。
