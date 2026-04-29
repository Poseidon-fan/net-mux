//! `Session::close` propagates a graceful shutdown.

mod common;

use net_mux::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn close_propagates_to_peer() {
    let (client, server) = common::pair(64 * 1024);

    let server_task = tokio::spawn({
        let server = server.clone();
        async move {
            // First accept succeeds
            let mut s = server.accept().await.unwrap();
            s.write_all(b"hi").await.unwrap();
            s.shutdown().await.unwrap();

            // After peer GoAway the next accept must terminate.
            let res = server.accept().await;
            assert!(matches!(res, Err(Error::SessionClosed)));
        }
    });

    let mut s = client.open().await.unwrap();
    let mut buf = [0u8; 8];
    let n = s.read(&mut buf).await.unwrap();
    assert_eq!(&buf[..n], b"hi");

    client.close().await;
    server_task.await.unwrap();
    server.close().await;

    // Open after close fails
    let res = client.open().await;
    assert!(matches!(res, Err(Error::SessionClosed)));
}

#[tokio::test]
async fn open_after_close_fails() {
    let (client, server) = common::pair(64 * 1024);
    client.close().await;
    let res = client.open().await;
    assert!(matches!(res, Err(Error::SessionClosed)));
    server.close().await;
}
