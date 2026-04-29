//! Session configuration.

use std::time::Duration;

/// Tunable parameters for a [`Session`](crate::Session).
///
/// Build via [`Config::builder`] or take [`Config::default`] for sensible
/// defaults. Once a `Session` is constructed the configuration is immutable.
#[derive(Debug, Clone)]
pub struct Config {
    /// Initial credit (in bytes) granted to each newly-opened receive stream.
    /// Doubles as the maximum amount of pending unread data the session will
    /// buffer per stream before exerting back-pressure on the peer.
    pub initial_stream_window: u32,

    /// Hard upper bound on the payload of a single `Data` frame. Larger user
    /// writes are transparently fragmented.
    pub max_frame_size: u32,

    /// Hard cap on concurrently open streams. Attempts to exceed this raise
    /// [`Error::TooManyStreams`](crate::Error::TooManyStreams).
    pub max_streams: usize,

    /// Interval between automatic `Ping` frames. `None` disables keepalive.
    pub keepalive_interval: Option<Duration>,

    /// How long to wait for a `Ping` reply before declaring the session dead.
    pub keepalive_timeout: Duration,

    /// Maximum time `Session::open` will wait for the peer's `ACK`.
    pub open_timeout: Duration,
}

impl Config {
    /// Default initial per-stream window: 256 KiB.
    pub const DEFAULT_INITIAL_STREAM_WINDOW: u32 = 256 * 1024;
    /// Default maximum single-frame payload: 64 KiB.
    pub const DEFAULT_MAX_FRAME_SIZE: u32 = 64 * 1024;
    /// Default maximum concurrent streams: 1024.
    pub const DEFAULT_MAX_STREAMS: usize = 1024;
    /// Default keepalive interval: 30 seconds.
    pub const DEFAULT_KEEPALIVE_INTERVAL: Duration = Duration::from_secs(30);
    /// Default keepalive timeout: 30 seconds.
    pub const DEFAULT_KEEPALIVE_TIMEOUT: Duration = Duration::from_secs(30);
    /// Default open timeout: 10 seconds.
    pub const DEFAULT_OPEN_TIMEOUT: Duration = Duration::from_secs(10);

    /// Start a new builder using the default configuration.
    pub fn builder() -> ConfigBuilder {
        ConfigBuilder::default()
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            initial_stream_window: Self::DEFAULT_INITIAL_STREAM_WINDOW,
            max_frame_size: Self::DEFAULT_MAX_FRAME_SIZE,
            max_streams: Self::DEFAULT_MAX_STREAMS,
            keepalive_interval: Some(Self::DEFAULT_KEEPALIVE_INTERVAL),
            keepalive_timeout: Self::DEFAULT_KEEPALIVE_TIMEOUT,
            open_timeout: Self::DEFAULT_OPEN_TIMEOUT,
        }
    }
}

/// Fluent builder for [`Config`].
#[derive(Debug, Clone, Default)]
pub struct ConfigBuilder {
    inner: Config,
}

impl ConfigBuilder {
    /// Override the initial per-stream window (bytes).
    #[must_use]
    pub fn initial_stream_window(mut self, bytes: u32) -> Self {
        self.inner.initial_stream_window = bytes.max(1);
        self
    }

    /// Override the maximum single-frame payload (bytes).
    #[must_use]
    pub fn max_frame_size(mut self, bytes: u32) -> Self {
        self.inner.max_frame_size = bytes.max(1);
        self
    }

    /// Override the maximum number of concurrent streams.
    #[must_use]
    pub fn max_streams(mut self, n: usize) -> Self {
        self.inner.max_streams = n.max(1);
        self
    }

    /// Override the keepalive interval. `None` disables keepalive.
    #[must_use]
    pub fn keepalive_interval(mut self, every: Option<Duration>) -> Self {
        self.inner.keepalive_interval = every;
        self
    }

    /// Override the keepalive timeout.
    #[must_use]
    pub fn keepalive_timeout(mut self, dur: Duration) -> Self {
        self.inner.keepalive_timeout = dur;
        self
    }

    /// Override the open timeout.
    #[must_use]
    pub fn open_timeout(mut self, dur: Duration) -> Self {
        self.inner.open_timeout = dur;
        self
    }

    /// Finalize and return the constructed [`Config`].
    pub fn build(self) -> Config {
        self.inner
    }
}
