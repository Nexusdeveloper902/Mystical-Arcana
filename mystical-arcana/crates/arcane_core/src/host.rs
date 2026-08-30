//! The Arcane engine host trait.
//!
//! Each subsystem (renderer, world, audio, input) is owned by the host.
//! The host owns the main loop and dispatches frame events to subsystems.

use crate::time::FrameTime;

pub trait Host: Send + Sync {
    fn tick(&mut self, frame: &FrameTime);
    fn shutdown(&mut self);
}

pub struct DummyHost;
impl Host for DummyHost {
    fn tick(&mut self, _frame: &FrameTime) {}
    fn shutdown(&mut self) {}
}
