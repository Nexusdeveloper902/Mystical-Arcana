//! Engine clock and frame timing.

use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug)]
pub struct FrameTime {
    pub elapsed: Duration,
    pub delta: Duration,
    pub frame_index: u64,
}

impl FrameTime {
    pub fn new() -> Self {
        Self {
            elapsed: Duration::ZERO,
            delta: Duration::ZERO,
            frame_index: 0,
        }
    }
    pub fn step(&mut self, now: Instant, start: Instant) {
        let new_elapsed = now.duration_since(start);
        self.delta = new_elapsed.saturating_sub(self.elapsed);
        self.elapsed = new_elapsed;
        self.frame_index = self.frame_index.wrapping_add(1);
    }
}

impl Default for FrameTime {
    fn default() -> Self {
        Self::new()
    }
}
