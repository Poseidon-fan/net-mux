//! Keepalive frames flow without false-positive timeouts.

mod common;

use std::time::Duration;

use net_mux::Config;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn keepalive_does_not_trip_for_idle_session() {
    let (client, server) = common::pair_with(64 * 1024, |_| {
        Config::builder()
            .keepalive_interval(Some(Duration::from_millis(100)))
            .keepalive_timeout(Duration::from_millis(500))
            .build()
    });

    let server_task = tokio::spawn({
        let server = server.clone();
        async move {
            let mut s = server.accept().await.unwrap();
            // Mostly-idle: read until peer FIN, then echo nothing.
            let mut buf = Vec::new();
            s.read_to_end(&mut buf).await.unwrap();
            assert_eq!(buf, b"ping");
            s.shutdown().await.unwrap();
        }
    });

    let mut s = client.open().await.unwrap();

    // Sleep well over keepalive interval; if keepalive misbehaved the
    // session would have already torn itself down.
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert!(!client.is_closed());
    assert!(!server.is_closed());

    s.write_all(b"ping").await.unwrap();
    s.shutdown().await.unwrap();
    let mut empty = Vec::new();
    s.read_to_end(&mut empty).await.unwrap();
    assert!(empty.is_empty());

    server_task.await.unwrap();
    client.close().await;
    server.close().await;
}
