pub struct Config {
    pub conn_send_window_size: usize,
    pub stream_recv_window_size: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            conn_send_window_size: 10240,
            stream_recv_window_size: 1024,
        }
    }
}
