//! Reverse-tunnel client. Connects the tunnel to `127.0.0.1:7777` and
//! forwards each incoming multiplexed stream to a local TCP service at
//! `127.0.0.1:8000`.

use anyhow::Result;
use net_mux::{Config, Session};
use tokio::io;
use tokio::net::TcpStream;

#[tokio::main]
async fn main() -> Result<()> {
    let raw = TcpStream::connect("127.0.0.1:7777").await?;
    let session = Session::client(raw, Config::default());
    println!("session started");

    loop {
        let mut tunnel = match session.accept().await {
            Ok(s) => s,
            Err(e) => {
                eprintln!("session is gone: {e}");
                break;
            }
        };
        tokio::spawn(async move {
            let mut local = match TcpStream::connect("127.0.0.1:8000").await {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("upstream dial failed: {e}");
                    return;
                }
            };
            if let Err(e) = io::copy_bidirectional(&mut tunnel, &mut local).await {
                eprintln!("forwarding ended: {e}");
            }
        });
    }
    session.close().await;
    Ok(())
}
