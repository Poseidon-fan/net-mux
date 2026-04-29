//! Reverse-tunnel server.
//!
//! 1. Listens on `127.0.0.1:7777` for a single transport TCP connection
//!    (the "tunnel"), wraps it in a `Session::server`.
//! 2. Listens on `127.0.0.1:8001` for client requests and opens one new
//!    multiplexed stream per accepted request, copying bytes
//!    bidirectionally.

use anyhow::Result;
use net_mux::{Config, Session};
use tokio::io;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<()> {
    let transport_listener = TcpListener::bind("127.0.0.1:7777").await?;
    let proxy_listener = TcpListener::bind("127.0.0.1:8001").await?;
    println!("transport: 127.0.0.1:7777, proxy: 127.0.0.1:8001");

    let (raw, _) = transport_listener.accept().await?;
    let session = Session::server(raw, Config::default());

    loop {
        let (mut proxy_stream, _) = proxy_listener.accept().await?;
        let mut tunnel = match session.open().await {
            Ok(s) => s,
            Err(e) => {
                eprintln!("session is gone: {e}");
                break;
            }
        };
        tokio::spawn(async move {
            if let Err(e) = io::copy_bidirectional(&mut proxy_stream, &mut tunnel).await {
                eprintln!("forwarding ended: {e}");
            }
        });
    }
    session.close().await;
    Ok(())
}
