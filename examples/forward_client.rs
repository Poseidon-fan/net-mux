use anyhow::Result;
use net_mux::{Config, Session};
use tokio::{io, net::TcpStream};

#[tokio::main]
async fn main() -> Result<()> {
    let raw_stream = TcpStream::connect("127.0.0.1:7777").await?;
    let session = Session::client(raw_stream, Config::default());
    println!("Session started");

    loop {
        let mut transport_stream = session.accept().await?;
        println!("Got new transport connection");
        tokio::spawn(async move {
            let mut local_stream = TcpStream::connect("127.0.0.1:8000").await.unwrap();
            println!("Start forwarding");
            let _ = io::copy_bidirectional(&mut transport_stream, &mut local_stream).await;
            println!("Forwarding finished");
        });
    }
}
