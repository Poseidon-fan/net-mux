<h1 align="center">net-mux</h1>

<div align="center">

[![GitHub][github-badge]][github-url]
[![Crates.io][crates-badge]][crates-url]
[![MIT licensed][mit-badge]][mit-url]
[![Build Status][actions-badge]][actions-url]

</div>

[crates-badge]: https://img.shields.io/crates/v/net-mux.svg
[crates-url]: https://crates.io/crates/net-mux
[mit-badge]: https://img.shields.io/badge/license-MIT-blue.svg
[mit-url]: https://github.com/Poseidon-fan/net-mux/blob/master/LICENSE
[actions-badge]: https://github.com/Poseidon-fan/net-mux/actions/workflows/rust.yaml/badge.svg
[actions-url]: https://github.com/Poseidon-fan/net-mux/actions?branch=master
[github-badge]: https://img.shields.io/badge/github-repo-black?logo=github
[github-url]: https://github.com/Poseidon-fan/net-mux

`net-mux` is an asynchronous connection multiplexing library built on
Tokio. It turns a single ordered, connection-oriented byte stream
(TCP, TLS-over-TCP, KCP, ...) into many concurrent bidirectional logical
streams.

## Features

- **Credit-based per-stream flow control.** Slow consumers do not stall
  other streams; total memory is bounded by `max_streams *
  initial_stream_window`.
- **Correct `AsyncRead` / `AsyncWrite`.** Large user writes are
  transparently fragmented; back-pressure surfaces as `Poll::Pending`.
- **Half-close.** `shutdown()` closes only the write half, mirroring TCP
  semantics.
- **Keepalive.** Periodic `Ping` frames with timeout-driven session
  shutdown.
- **Graceful close.** `Session::close().await` flushes outstanding frames,
  emits a `GoAway`, and joins all background tasks.
- **Lock-free hot paths.** Atomic windows, atomic stream state, single
  outbound queue.

See [`docs/architecture.md`](docs/architecture.md) and
[`docs/protocol.md`](docs/protocol.md) for the deeper design notes.

## Quick start

```rust,no_run
use net_mux::{Config, Session};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let conn = TcpStream::connect("127.0.0.1:7777").await?;
    let session = Session::client(conn, Config::default());

    let mut stream = session.open().await?;
    stream.write_all(b"hello\n").await?;
    stream.shutdown().await?;

    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await?;
    println!("{}", String::from_utf8_lossy(&buf));

    session.close().await;
    Ok(())
}
```

## Examples

### Echo

```sh
$ cargo run --example tcp_server
$ cargo run --example tcp_client
```

The server echoes each line received over a fresh multiplexed stream;
the client reads stdin, opens one stream per line, and prints the reply.

### Reverse tunnel

```sh
$ cargo run --example forward_server   # listens on :7777 (transport) and :8001 (proxy)
$ cargo run --example forward_client   # connects to :7777, forwards to :8000
```

If you start an HTTP service on `127.0.0.1:8000`
(e.g. `python -m http.server 8000`), it becomes reachable on
`http://127.0.0.1:8001` via the tunnel.

## Configuration

```rust
use net_mux::Config;
use std::time::Duration;

let cfg = Config::builder()
    .initial_stream_window(512 * 1024)
    .max_frame_size(64 * 1024)
    .max_streams(2048)
    .keepalive_interval(Some(Duration::from_secs(30)))
    .build();
```

## Links

- API [docs][documentation]
- Source [examples][examples]
- Wire-protocol [spec](docs/protocol.md)

[examples]: https://github.com/Poseidon-fan/net-mux/tree/master/examples
[documentation]: https://docs.rs/crate/net-mux/
