//! With a small per-stream window, the writer is forced to wait for the
//! reader to consume bytes before transmitting more — i.e. credit-based
//! flow control is actually enforced.

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use net_mux::Config;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::sleep;

#[tokio::test]
async fn writer_blocks_until_reader_consumes() {
    // Tiny window so the writer must wait for WindowUpdates.
    let (client, server) = common::pair_with(64 * 1024, |_| {
        Config::builder()
            .initial_stream_window(4 * 1024)
            .max_frame_size(1024)
            .keepalive_interval(None)
            .build()
    });

    let consumed = Arc::new(AtomicUsize::new(0));
    let consumed_c = consumed.clone();

    let server_task = tokio::spawn({
        let server = server.clone();
        async move {
            let mut s = server.accept().await.unwrap();
            let mut buf = vec![0u8; 4096];
            let mut total = 0usize;
            // Read slowly to ensure flow control kicks in.
            loop {
                let n = s.read(&mut buf).await.unwrap();
                if n == 0 {
                    break;
                }
                total += n;
                consumed_c.store(total, Ordering::Release);
                sleep(Duration::from_millis(5)).await;
            }
            total
        }
    });

    let payload = vec![0xABu8; 64 * 1024];
    let mut stream = client.open().await.unwrap();
    stream.write_all(&payload).await.unwrap();
    stream.shutdown().await.unwrap();

    let total = server_task.await.unwrap();
    assert_eq!(total, payload.len());
    assert_eq!(consumed.load(Ordering::Acquire), payload.len());

    client.close().await;
    server.close().await;
}
