# TCP Protocol Frame Contract v1

## Scope

This document specifies the length-prefixed binary frame format used on the MuduDB custom TCP client/server connection. It carries request/response messages between `mcli` / client libraries and `mudud`.

## Version history

| Version | Date | Summary |
|---------|------|---------|
| 1 | 2026-6-25 | Initial 40-byte header with magic, version, 32-bit message type, 64-bit flags, request id, trace id, and payload length. |

## Frame layout

```text
+-----------------------------+
| Header (40 bytes)           |
+-----------------------------+
| Payload (variable)          |
+-----------------------------+
```

Each frame is encoded as a single header followed by a payload. The transport layer is responsible for delivering the exact byte range; there is no separate length prefix outside the header.

### Header

All header fields are big-endian.

| Offset | Size | Field | Description |
|--------|------|-------|-------------|
| 0 | 4 | `magic` | Magic `0x4D53_464D` (ASCII `MSFM`). |
| 4 | 4 | `version` | Protocol frame version. Current value: `1`. |
| 8 | 4 | `message_type` | Message type discriminant (see below). |
| 12 | 8 | `flags` | Frame flags. Bit 0 (`0x1`) is the trace sampling flag; all other bits are reserved and must be `0`. |
| 20 | 8 | `request_id` | Client-assigned request identifier. |
| 28 | 8 | `trace_id` | Distributed trace identifier. |
| 36 | 4 | `payload_len` | Length of the payload in bytes. |

### Message types

| Value | Name | Payload encoding |
|-------|------|------------------|
| 1 | Handshake | `rmp_serde` of `HandshakeRequest` / `HandshakeResponse` |
| 2 | Auth | `rmp_serde` of auth payload |
| 3 | Query | `rmp_serde` of `ClientRequest` |
| 4 | Execute | `rmp_serde` of `ClientRequest` |
| 5 | Batch | `rmp_serde` of `ClientRequest` |
| 6 | Response | `rmp_serde` of `ServerResponse` |
| 7 | Error | `rmp_serde` of `ErrorResponse` |
| 8 | Get | `rmp_serde` of `GetRequest` |
| 9 | Put | `rmp_serde` of `PutRequest` |
| 10 | RangeScan | `rmp_serde` of `RangeScanRequest` |
| 11 | ProcedureInvoke | `rmp_serde` of `ProcedureInvokeRequest` |
| 12 | SessionCreate | `rmp_serde` of `SessionCreateRequest` |
| 13 | SessionClose | `rmp_serde` of `SessionCloseRequest` |

## Handshake and version negotiation

The protocol defines a handshake message for version negotiation:

- `HandshakeRequest` contains `supported_versions: Vec<u32>` and optional `capabilities: Vec<String>`.
- `HandshakeResponse` contains `selected_version: u32` and optional server capabilities.

The current server implementation selects version `1` only. A client that does not include `1` in `supported_versions` receives `IncompatibleProtocolVersion`.

## Integrity mechanisms

- **Magic check:** decoders reject frames whose magic does not match `0x4D53_464D`.
- **Version check:** decoders reject any version other than `1`.
- **Length checks:** decoders require at least 40 bytes for the header and `payload_len` additional bytes for the payload.
- **Message type validation:** unknown message type values are rejected.
- **Payload length validation:** decoders require at least `payload_len` bytes after the header and pass exactly that slice to `Frame::from_parts`. Buffers may contain bytes for subsequent frames.
- **Flag validation:** decoders reject frames with unknown flag bits set (only bit 0 is defined).

## Compatibility matrix

| Reader \ Writer | v1 |
|-----------------|----|
| v1 | Compatible |

Only version `1` is supported.

## Upgrade and rollback rules

- **Upgrade:** A future v2 frame may extend the header (e.g., add capability flags) or define new message types. V2 clients and servers must negotiate the version via the handshake before sending v2 frames.
- **Rollback:** A v1 server receiving a v2-only frame returns `UnsupportedFormatVersion`. A v2 server receiving a v1-only client selects version `1` during handshake.
- **Breaking changes:** New message types may be added in minor protocol versions. Removed or renamed message types require a major version bump.

## Deprecation policy

Version `1` may be deprecated only after:
1. All bundled clients and drivers support the new version.
2. The new version has been the default for at least one release cycle.
3. A compatibility test matrix exercises v1 client / v2 server and v2 client / v1 server.

## References

- Frame format: [`mudu_contract/src/protocol/format/latest.rs`](../../../mudu_contract/src/protocol/format/latest.rs)
- Message types and payloads: [`mudu_contract/src/protocol/mod.rs`](../../../mudu_contract/src/protocol/mod.rs)
- Handshake handler: [`mudu_kernel/src/server/handlers/handshake.rs`](../../../mudu_kernel/src/server/handlers/handshake.rs)
