<h1 align="center">net-mux</h1>

<div align="center">
A simple Network Connection Stream Multiplexing async library for rust
</div>

<br>
<div align="center">
    <a href="https://github.com/Poseidon-fan/net-mux/tree/master/examples">Examples</a>
    |
    <a href="https://docs.rs/crate/net-mux/">Released API Docs</a>
</div>

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

![system architecture](https://github.com/Poseidon-fan/net-mux/tree/master/docs/images/architecture.svg)
