//! End-to-end smoke test: open / accept / echo.

mod common;

use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn echo_round_trip() {
    let (client, server) = common::pair(64 * 1024);

    let server_task = tokio::spawn({
        let server = server.clone();
        async move {
            let mut s = server.accept().await.expect("accept");
            let mut buf = [0u8; 64];
            let n = s.read(&mut buf).await.expect("read");
            s.write_all(&buf[..n]).await.expect("write");
            tokio::io::AsyncWriteExt::shutdown(&mut s)
                .await
                .expect("shutdown");
        }
    });

    let mut stream = client.open().await.expect("open");
    stream.write_all(b"hello world").await.expect("write");
    stream.flush().await.unwrap();

    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .await
        .expect("read_to_end");
    assert_eq!(response, b"hello world");

    server_task.await.unwrap();
    client.close().await;
    server.close().await;
}

#[tokio::test]
async fn many_concurrent_streams() {
    const N: usize = 32;
    let (client, server) = common::pair(64 * 1024);

    let server_task = tokio::spawn({
        let server = server.clone();
        async move {
            let mut handles = Vec::new();
            for _ in 0..N {
                let s = server.accept().await.expect("accept");
                handles.push(tokio::spawn(async move {
                    let mut s = s;
                    let mut buf = vec![0u8; 1024];
                    let n = s.read(&mut buf).await.unwrap();
                    s.write_all(&buf[..n]).await.unwrap();
                    tokio::io::AsyncWriteExt::shutdown(&mut s).await.unwrap();
                }));
            }
            for h in handles {
                h.await.unwrap();
            }
        }
    });

    let mut tasks = Vec::new();
    for i in 0..N {
        let c = client.clone();
        tasks.push(tokio::spawn(async move {
            let mut s = c.open().await.unwrap();
            let msg = format!("payload-{i}");
            s.write_all(msg.as_bytes()).await.unwrap();
            s.flush().await.unwrap();
            let mut got = Vec::new();
            s.read_to_end(&mut got).await.unwrap();
            assert_eq!(got, msg.as_bytes());
        }));
    }
    for t in tasks {
        t.await.unwrap();
    }
    server_task.await.unwrap();
    client.close().await;
    server.close().await;
}
