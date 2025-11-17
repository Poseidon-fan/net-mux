pub struct Config {
    pub frame_window_size: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            frame_window_size: 1024,
        }
    }
}
