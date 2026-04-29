//! Throughput micro-benchmarks.
//!
//! These run two `Session`s back-to-back over an in-memory `tokio::io::duplex`
//! pipe so we measure the multiplexer overhead in isolation, without any
//! kernel networking.

use std::time::Duration;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use net_mux::{Config, Session};
use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream, duplex};
use tokio::runtime::Builder;

type DuplexSession = Session<DuplexStream>;

fn make_pair(window: u32, frame: u32) -> (DuplexSession, DuplexSession) {
    let (a, b) = duplex(1024 * 1024);
    let cfg = Config::builder()
        .initial_stream_window(window)
        .max_frame_size(frame)
        .keepalive_interval(None)
        .build();
    (Session::client(a, cfg.clone()), Session::server(b, cfg))
}

fn bench_single_stream(c: &mut Criterion) {
    let rt = Builder::new_multi_thread().enable_all().build().unwrap();

    let mut group = c.benchmark_group("single_stream");
    group.measurement_time(Duration::from_secs(5));

    for &payload in &[1024usize, 16 * 1024, 256 * 1024] {
        group.throughput(Throughput::Bytes(payload as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(payload),
            &payload,
            |bencher, &payload| {
                bencher.to_async(&rt).iter(|| async move {
                    let (client, server) = make_pair(512 * 1024, 64 * 1024);

                    let server_task = tokio::spawn({
                        let server = server.clone();
                        async move {
                            let mut s = server.accept().await.unwrap();
                            let mut sink = vec![0u8; payload];
                            s.read_exact(&mut sink).await.unwrap();
                            s.shutdown().await.unwrap();
                            sink
                        }
                    });

                    let mut s = client.open().await.unwrap();
                    let payload_buf = vec![0u8; payload];
                    s.write_all(&payload_buf).await.unwrap();
                    s.shutdown().await.unwrap();

                    let _ = server_task.await.unwrap();
                    client.close().await;
                    server.close().await;
                });
            },
        );
    }
    group.finish();
}

criterion_group!(benches, bench_single_stream);
criterion_main!(benches);
