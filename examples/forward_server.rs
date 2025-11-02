use anyhow::Result;
use net_mux::{Config, Session};
use tokio::{io, net::TcpListener};

#[tokio::main]
async fn main() -> Result<()> {
    let trasnport_listener = TcpListener::bind("127.0.0.1:7777").await?;
    let proxy_listener = TcpListener::bind("127.0.0.1:8001").await?;
    println!("Trasnport listening on 127.0.0.1:7777, Proxy listening on 127.0.0.1:8001");

    let (raw_stream, _) = trasnport_listener.accept().await?;
    let session = Session::server(raw_stream, Config::default());
    println!("Session started");

    loop {
        let (mut proxy_stream, _) = proxy_listener.accept().await?;
        println!("Got new proxy connection");
        let mut trasnport_stream = session.open().await.unwrap();
        tokio::spawn(async move {
            println!("Start forwarding");
            let _ = io::copy_bidirectional(&mut proxy_stream, &mut trasnport_stream).await;
            println!("Forwarding finished");
        });
    }
}
