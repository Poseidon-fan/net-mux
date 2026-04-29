# Wire protocol

`net-mux` v1 frames are 12 bytes of header followed by an optional payload.
All multi-byte integers are big-endian.

## Header

```
0       1       2               4               8               12
+-------+-------+---------------+---------------+---------------+
|  Ver  | Type  |     Flags     |   StreamId    |    Length     |
+-------+-------+---------------+---------------+---------------+
  u8      u8         u16              u32             u32
```

| Field | Width | Description |
| ----- | ----- | ----------- |
| Ver | 1 byte | Protocol version. Currently `0x01`. Receivers MUST reject any other value with `ErrorCode::InvalidVersion`. |
| Type | 1 byte | Frame type. `0x00 Data`, `0x01 WindowUpdate`, `0x02 Ping`, `0x03 GoAway`. |
| Flags | 2 bytes | Bitmask of `SYN(0x1) ACK(0x2) FIN(0x4) RST(0x8)`. Other bits MUST be zero. |
| StreamId | 4 bytes | Logical stream id. `0` is reserved for session-scoped frames (`Ping`, `GoAway`). Client-initiated streams use odd ids; server-initiated streams use even ids. |
| Length | 4 bytes | Type-specific. See below. |

## Frame types

### `Data` (`Type = 0x00`)

`Length` is the payload length in bytes. Carries up to
`Config::max_frame_size` bytes (default 64 KiB).

The `Flags` field controls stream lifecycle:

| Flag | Meaning |
| ---- | ------- |
| `SYN` | Stream is being opened. The first frame on a stream MUST set this. |
| `ACK` | Confirms a previously received `SYN`. The corresponding peer's first frame on the stream MUST set this. |
| `FIN` | Sender will not transmit any further `Data` on this stream. May be combined with payload (data + FIN). |
| `RST` | Stream is being abruptly torn down. Implies both `FIN` halves; receiver MUST drop any buffered inbound data and surface `ConnectionReset`. |

### `WindowUpdate` (`Type = 0x01`)

Carries no payload. `Length` is the credit increment, in bytes, granted to
the peer for the indicated `StreamId`. Receivers add the value to their
send window saturated at `u32::MAX`.

### `Ping` (`Type = 0x02`)

Carries no payload. `StreamId` is `0`. `Length` is an opaque value chosen
by the requester. The peer MUST reply with another `Ping` carrying the
same `Length` and `Flags = ACK`.

### `GoAway` (`Type = 0x03`)

Carries no payload. `StreamId` is `0`. `Length` is interpreted as an
[`ErrorCode`](../src/error.rs):

| Code | Name | Meaning |
| ---- | ---- | ------- |
| 0 | Normal | Graceful shutdown. |
| 1 | ProtocolError | Wire-format violation. |
| 2 | InternalError | Implementation bug. |
| 3 | FlowControlError | Peer overran an advertised window. |
| 4 | StreamLimit | Too many concurrent streams. |
| 5 | InvalidVersion | Unsupported `Ver` byte. |
| 6 | Timeout | Keepalive timeout. |
| `>= 7` | Unknown | Forwarded as `ErrorCode::Unknown(u32)` for forward compatibility. |

After sending or receiving a `GoAway`, neither side opens new streams,
but already-established streams continue to operate until their natural
termination.

## Stream id allocation

* Client allocates `1, 3, 5, ...`
* Server allocates `2, 4, 6, ...`
* `0` is reserved for session-scoped frames.

A peer MUST refuse a frame whose `StreamId` parity does not match the
opener's role with `ErrorCode::ProtocolError`.

## Limits

| Constant | Default | Purpose |
| -------- | ------- | ------- |
| `Config::initial_stream_window` | 256 KiB | Bytes a fresh stream may receive before requiring `WindowUpdate`. |
| `Config::max_frame_size` | 64 KiB | Maximum payload per `Data` frame. Larger user writes are fragmented. |
| `Config::max_streams` | 1024 | Hard cap on concurrent streams per session. |
