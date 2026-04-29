//! Half-close semantics: shutting down the write half does not close the
//! read half, and vice versa.

mod common;

use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn write_half_close_keeps_read_open() {
    let (client, server) = common::pair(64 * 1024);

    let server_task = tokio::spawn({
        let server = server.clone();
        async move {
            let mut s = server.accept().await.unwrap();

            // Read until peer FIN
            let mut buf = Vec::new();
            s.read_to_end(&mut buf).await.unwrap();
            assert_eq!(buf, b"hello");

            // Now write back
            s.write_all(b"world").await.unwrap();
            s.shutdown().await.unwrap();
        }
    });

    let mut stream = client.open().await.unwrap();
    stream.write_all(b"hello").await.unwrap();
    stream.shutdown().await.unwrap();

    // After local shutdown, the read half is still open
    let mut got = Vec::new();
    stream.read_to_end(&mut got).await.unwrap();
    assert_eq!(got, b"world");

    server_task.await.unwrap();
    client.close().await;
    server.close().await;
}
