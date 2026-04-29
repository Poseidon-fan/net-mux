//! Echo server. For each accepted TCP connection a `Session::server` is
//! created; every multiplexed stream the client opens is echoed back.

use anyhow::Result;
use net_mux::{Config, Session};
use tokio::io::{self, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,net_mux=debug".into()),
        )
        .init();

    let listener = TcpListener::bind("127.0.0.1:7777").await?;
    println!("listening on 127.0.0.1:7777");

    loop {
        let (conn, addr) = listener.accept().await?;
        conn.set_nodelay(true)?;
        println!("connected: {addr}");
        tokio::spawn(handle_conn(conn));
    }
}

async fn handle_conn(conn: TcpStream) -> Result<()> {
    let session = Session::server(conn, Config::default());
    loop {
        let mut stream = match session.accept().await {
            Ok(s) => s,
            Err(e) => {
                println!("session ended: {e}");
                break;
            }
        };

        tokio::spawn(async move {
            let (mut r, mut w) = io::split(&mut stream);
            if let Err(e) = io::copy(&mut r, &mut w).await {
                eprintln!("echo stream errored: {e}");
            }
            let _ = w.shutdown().await;
        });
    }
    session.close().await;
    Ok(())
}
