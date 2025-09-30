use net_mux::{Config, Session};
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let conn = TcpStream::connect("127.0.0.1:7777").await?;
    conn.set_nodelay(true)?;
    println!("Connected to server!");

    let session = Session::client(conn, Config::default());
    println!("session starting");

    loop {
        let stream = session.open().await?;
        let (read_half, mut write_half) = io::split(stream);

        let mut reader = BufReader::new(read_half);
        let mut stdin = io::BufReader::new(io::stdin());

        let mut line_to_send = String::new();
        let mut server_response = String::new();

        let _ = stdin.read_line(&mut line_to_send).await?;
        write_half.write_all(line_to_send.as_bytes()).await?;
        line_to_send.clear();

        let _ = reader.read_line(&mut server_response).await?;
        println!("Server response: {}", server_response);
        server_response.clear();
    }
}
