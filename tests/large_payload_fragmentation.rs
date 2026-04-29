//! User writes that exceed `Config::max_frame_size` are fragmented across
//! multiple `Data` frames transparently.

mod common;

use net_mux::Config;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn write_larger_than_frame_size() {
    // Tiny frames force many fragments.
    let (client, server) = common::pair_with(64 * 1024, |_| {
        Config::builder()
            .max_frame_size(1024)
            .initial_stream_window(64 * 1024)
            .keepalive_interval(None)
            .build()
    });

    let payload: Vec<u8> = (0..100_000_u32).flat_map(u32::to_be_bytes).collect();
    let payload_clone = payload.clone();

    let server_task = tokio::spawn({
        let server = server.clone();
        async move {
            let mut s = server.accept().await.unwrap();
            let mut received = Vec::new();
            s.read_to_end(&mut received).await.unwrap();
            assert_eq!(received, payload_clone);
            s.shutdown().await.unwrap();
        }
    });

    let mut stream = client.open().await.unwrap();
    stream.write_all(&payload).await.unwrap();
    stream.shutdown().await.unwrap();

    // Drain any reply (none in this test)
    let mut empty = Vec::new();
    stream.read_to_end(&mut empty).await.unwrap();
    assert!(empty.is_empty());

    server_task.await.unwrap();
    client.close().await;
    server.close().await;
}
