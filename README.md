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

net-mux is an asynchronous connection multiplexing library built on tokio. It multiplexes ordered, connection-oriented transports such as TCP, KCP, and TLS-over-TCP into multiple logical concurrent, ordered, bidirectional streams.

![system architecture](https://github.com/Poseidon-fan/net-mux/raw/master/docs/images/architecture.svg)

## Getting Started

**Examples**

### Echo Service

```sh
$ cargo run --example tcp_server
$ cargo run --example tcp_client
```

This launches a TCP listener on the local loopback address, waiting for client connections. Each connection is wrapped as a mux session. The server and client interact over this single connection through multiple streams. The server receives messages from the client and writes them back unchanged, while the client reads strings from the standard input, sends them to the server, and prints the received messages.

### Forward Service

```sh
$ cargo run --example forward_server
$ cargo run --example forward_client
```

This launches a forward server and client. The server listens on port 7777 for client connections and listens on port 8001 to forward its data to the client. The client connects to the server's port 7777 and forwards data from the local port 8000. If you start an HTTP service on port 8000 (e.g., `python -m http.server 8000`), you can access it via `http://127.0.0.1:8001`.

**Links**

- Usage [examples][examples]
- Released API [Docs][documentation]

[examples]: https://github.com/Poseidon-fan/net-mux/tree/master/examples
[documentation]: https://docs.rs/crate/net-mux/
