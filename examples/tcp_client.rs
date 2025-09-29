use net_mux::{Config, Session};
use std::time::Duration;
use tokio::io::{self, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let conn = TcpStream::connect("127.0.0.1:7777").await?;
    conn.set_nodelay(true)?;
    println!("Connected to server!");

    let session = Session::client(conn, Config::default());
    println!("session starting");

    let mut interval = time::interval(Duration::from_secs(2));
    interval.tick().await;
    let mut counter = 0;

    loop {
        interval.tick().await;
        counter += 1;
        let message = format!("Hello, server! This is message #{}", counter);

        let stream = session.open().await?;

        let (_, mut writer) = io::split(stream);
        writer
            .write_all(format!("{}\n", message).as_bytes())
            .await?;
    }
}
