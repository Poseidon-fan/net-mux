//! Echo client. Each line typed on stdin is sent over a fresh multiplexed
//! stream and the response is printed.

use anyhow::Result;
use net_mux::{Config, Session};
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

#[tokio::main]
async fn main() -> Result<()> {
    let conn = TcpStream::connect("127.0.0.1:7777").await?;
    conn.set_nodelay(true)?;
    let session = Session::client(conn, Config::default());

    let mut stdin = BufReader::new(io::stdin()).lines();
    while let Some(line) = stdin.next_line().await? {
        let mut stream = session.open().await?;
        stream.write_all(line.as_bytes()).await?;
        stream.write_all(b"\n").await?;
        stream.shutdown().await?;

        let mut response = Vec::new();
        tokio::io::AsyncReadExt::read_to_end(&mut stream, &mut response).await?;
        print!("{}", String::from_utf8_lossy(&response));
    }

    session.close().await;
    Ok(())
}
