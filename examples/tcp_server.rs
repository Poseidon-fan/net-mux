use anyhow::{Ok, Result};
use net_mux::{Config, Session};
use tokio::io::{self, AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[tokio::main]
async fn main() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:7777").await?;
    println!("Server listening on 127.0.0.1:7777");

    loop {
        let (conn, addr) = listener.accept().await?;
        conn.set_nodelay(true)?;
        println!("New client: {:?}", addr);
        tokio::spawn(handle_conn(conn));
    }
}

async fn handle_conn(conn: TcpStream) -> Result<()> {
    let mut session = Session::server(conn, Config::default());
    println!("session starting");

    loop {
        let stream = session.accept().await?;
        let (mut reader, mut writer) = io::split(stream);
        let mut buf = [0u8; 1024];

        let n = reader.read(&mut buf).await?;
        if n == 0 {
            println!("remote closed");
            break;
        }
        println!("Received: {:?}", String::from_utf8_lossy(&buf[..n]));
        writer.write_all(&buf[..n]).await?;
    }
    Ok(())
}
