# WebSocket Gateway

The `ws_gateway` crate exposes a long-lived WebSocket endpoint (`/ws`) for
clients that invoke MuduDB procedures and receive server-pushed events without
reconnecting per call. It is a thin frontend over a backend that invokes
procedures and long-polls queue receives.

## Endpoint

Connect a WebSocket to the server's HTTP management port (default `8300`):

```text
ws://127.0.0.1:8300/ws
ws://127.0.0.1:8300/ws?queue=<queue-name>
```

- Without a `queue` query parameter, the connection only invokes procedures.
- With `queue=<queue-name>`, the connection joins that queue's **zone**: one
  receive loop per zone long-polls `RECEIVE ... WAIT FOREVER` on the queue and
  fans every received row out to every session subscribed to that zone.

## Frames

All frames are JSON text messages.

### Client → server: procedure invocation

```json
{ "app": "wallet", "module": "wallet", "proc": "transfer", "body": { "from": 1, "to": 2, "amount": 100 } }
```

`app` / `module` / `proc` identify the installed procedure; `body` is the
procedure argument object.

### Server → client: invocation result

```json
{ "op": "result", "ok": true, "data": { "..": ".." }, "error": null }
```

`ok` is `false` and `error` carries the message when the call fails (including
an unparseable request frame).

### Server → client: queue event (zone fan-out)

```json
{ "op": "event", "zone": "orders", "data": { "..": ".." } }
```

Each `event` is one payload row received by the zone's receive loop, broadcast
to every session in that zone.

## Lifecycle

- The server answers WebSocket `Ping` with `Pong`.
- A connection's zone receive loop starts on the first session that joins the
  zone and exits when the zone's last session disconnects or the queue is
  dropped (`ObjectNotFound`); a transient failure (worker restart, network)
  retries after a short backoff.
- Procedure invocation and queue receive go through the same
  `enable_invoke` gate as the HTTP invoke endpoint.
