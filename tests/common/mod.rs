//! Shared helpers for integration tests.

#![allow(dead_code)]

use net_mux::{Config, Session};
use tokio::io::{DuplexStream, duplex};

pub type DuplexSession = Session<DuplexStream>;

/// Build a paired (client, server) session over an in-memory `duplex` pipe.
pub fn pair(buf_size: usize) -> (DuplexSession, DuplexSession) {
    pair_with(buf_size, |c| c)
}

/// Build a paired session with caller-supplied configuration.
pub fn pair_with<F>(buf_size: usize, customize: F) -> (DuplexSession, DuplexSession)
where
    F: Fn(Config) -> Config,
{
    let (a, b) = duplex(buf_size);
    let cfg_a = customize(Config::default());
    let cfg_b = customize(Config::default());
    let client = Session::client(a, cfg_a);
    let server = Session::server(b, cfg_b);
    (client, server)
}
