# 系统调用语义

## Worker 标识

在当前 runtime 路径里，session 路由使用 `worker_id` 表示。

- `worker_id` 是会话本地执行的路由目标
- 一旦某个 session 绑定到某个 worker，其请求就必须由该 worker 处理

## Session Open

`open` 接受一个可选的 JSON 字符串参数。

该 JSON 负载用于描述 session 路由以及 session 配置变更。负载中至少包含：

- `session_id`
- `worker_id`

示例：

```json
{
  "session_id": 0,
  "worker_id": 3
}
```

## `session_id` 的含义

`session_id` 用于控制本次调用是创建新 session，还是更新已有 session。

- 如果 `session_id == 0`，kernel 会创建一个新 session
- 如果 `session_id != 0`，则表示该调用引用的是一个已有 session，并修改该 session 的配置

这里提到的配置变更，指的是同一个 JSON 负载中携带的目标 worker 绑定。

## `worker_id` 的含义

`worker_id` 用于告诉 kernel，哪个 worker 应该拥有该 session。

- 如果当前连接已经附着在目标 worker 上，则 session 会在该 worker 上创建或更新
- 如果当前连接不在该 worker 上，kernel 会将该连接转移到该 worker

在这次转移之后，目标 worker 就成为该 session 的拥有者。

## 连接的默认路由

当某个 session 导致连接迁移到另一个 worker 时，该 worker 也会成为当前连接的默认 worker。

这意味着：

- 之后该 session 的请求会继续发送到该 worker
- 同一连接上的后续请求，默认也会发送到该 worker

除非同一连接上的其他 session 之后再次通过 `open` 显式修改该设置，否则这个默认值会一直生效。

## 路由规则

实际行为如下：

1. 解析传递给 `open` 的可选 JSON 参数。
2. 读取 `session_id` 和 `worker_id`。
3. 如果 `session_id == 0`，则创建一个新 session。
4. 如果 `session_id != 0`，则更新已有 session 的配置。
5. 确保该 session 由 `worker_id` 指定的 worker 持有。
6. 如有必要，将当前连接转移到该 worker。
7. 在下一次显式修改 session 路由之前，将该 worker 作为当前连接的默认目标 worker。

## 说明

- session 路由是显式指定的
- 连接路由可能会作为打开或重新配置 session 的副作用而发生变化
- 在这类变化之后，session 所属 worker 与连接默认 worker 应保持一致
- 后续如果对另一个 session 调用 `open`，同一连接仍可能再次迁移
