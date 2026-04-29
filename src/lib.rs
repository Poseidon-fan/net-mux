//! `net-mux` is an asynchronous connection multiplexing library built on
//! Tokio.
//!
//! It multiplexes a single ordered, connection-oriented byte stream
//! (TCP, KCP, TLS-over-TCP, ...) into many bidirectional logical streams
//! with credit-based flow control, ping keepalive, and graceful shutdown.
//!
//! # Quick start
//!
//! ```no_run
//! use net_mux::{Config, Session};
//! use tokio::net::TcpStream;
//!
//! # async fn run() -> Result<(), Box<dyn std::error::Error>> {
//! let conn = TcpStream::connect("127.0.0.1:7777").await?;
//! let session = Session::client(conn, Config::default());
//!
//! let mut stream = session.open().await?;
//! tokio::io::AsyncWriteExt::write_all(&mut stream, b"hello").await?;
//! # Ok(()) }
//! ```
//!
//! See the `examples/` directory for end-to-end demos.

#![doc(html_root_url = "https://docs.rs/net-mux/1.0.0")]
#![cfg_attr(docsrs, feature(doc_cfg))]

mod error;
mod flow;
mod protocol;
mod session;
mod stream;
mod util;

pub mod config;

pub use crate::config::{Config, ConfigBuilder};
pub use crate::error::{Error, ErrorCode, Result};
pub use crate::session::Session;
pub use crate::stream::Stream;
pub use crate::util::id::StreamId;
