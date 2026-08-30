//! Time, frame clock, and stopwatch.
//!
//! [`FrameClock`] tracks simulation wall-clock delta and accumulated sim time.
//! It is the canonical source of `dt` for every gameplay system. The clock
//! is **single-threaded by design**: gameplay systems read from the main
//! thread's clock; background threads (streaming, audio) get their own.

use std::time::{Duration, Instant};
use parking_lot::Mutex;

/// A monotonic frame clock. Use [`FrameClock::tick`] once per frame.
#[derive(Debug)]
pub struct FrameClock {
    inner: Mutex<Inner>,
    max_dt: f32,
}

#[derive(Debug)]
struct Inner {
    start: Instant,
    last: Instant,
    sim_t: f32,
    frame: u64,
}

impl FrameClock {
    /// Creates a new frame clock with the given maximum dt (in seconds).
    /// Any tick larger than `max_dt` is clamped — prevents the "spiral of
    /// death" when the simulation is paused or hitched.
    pub fn new(max_dt: f32) -> Self {
        let now = Instant::now();
        Self {
            inner: Mutex::new(Inner {
                start: now,
                last: now,
                sim_t: 0.0,
                frame: 0,
            }),
            max_dt,
        }
    }

    /// Advances the clock. Returns the (clamped) delta in seconds and the
    /// new simulation time.
    pub fn tick(&self) -> (f32, f32) {
        let mut g = self.inner.lock();
        let now = Instant::now();
        let dt = (now - g.last).as_secs_f32().min(self.max_dt);
        g.last = now;
        g.sim_t += dt;
        g.frame += 1;
        (dt, g.sim_t)
    }

    /// Returns the elapsed wall-clock time since the clock was created.
    pub fn elapsed(&self) -> Duration {
        let g = self.inner.lock();
        g.last - g.start
    }

    /// Returns the current simulation time in seconds.
    pub fn sim_t(&self) -> f32 {
        self.inner.lock().sim_t
    }

    /// Returns the current frame counter.
    pub fn frame(&self) -> u64 {
        self.inner.lock().frame
    }
}

impl Default for FrameClock {
    fn default() -> Self {
        // 0.25s — generous enough for hitches, tight enough to avoid
        // the spiral of death when paused.
        Self::new(0.25)
    }
}

/// A simple stopwatch for measuring elapsed wall-clock time.
pub struct Stopwatch {
    start: Instant,
}

impl Stopwatch {
    /// Starts a new stopwatch.
    pub fn start() -> Self {
        Self { start: Instant::now() }
    }

    /// Elapsed time in seconds (f32).
    pub fn elapsed_secs(&self) -> f32 {
        self.start.elapsed().as_secs_f32()
    }

    /// Elapsed time in microseconds (u128).
    pub fn elapsed_us(&self) -> u128 {
        self.start.elapsed().as_micros()
    }

    /// Resets the stopwatch.
    pub fn reset(&mut self) {
        self.start = Instant::now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn frame_clock_advances_sim_t() {
        let clock = FrameClock::new(0.25);
        let (dt1, t1) = clock.tick();
        thread::sleep(Duration::from_millis(10));
        let (dt2, t2) = clock.tick();
        assert!(dt2 > 0.0, "second tick must produce positive dt");
        assert!(dt2 <= 0.25, "dt must be clamped to max");
        assert!(t2 > t1, "sim_t must advance");
        assert!(t2 - t1 - dt2 < 0.0001, "sim delta must equal dt");
        let _ = dt1;
    }

    #[test]
    fn frame_clock_clamps_huge_dt() {
        let clock = FrameClock::new(0.05);
        let _ = clock.tick();
        thread::sleep(Duration::from_millis(200));
        let (dt, _) = clock.tick();
        assert!(dt <= 0.05 + 1e-6, "dt must be clamped to 0.05, got {}", dt);
    }

    #[test]
    fn stopwatch_measures_time() {
        let mut sw = Stopwatch::start();
        thread::sleep(Duration::from_millis(5));
        let e1 = sw.elapsed_secs();
        assert!(e1 > 0.0, "stopwatch should report elapsed time");
        sw.reset();
        let e2 = sw.elapsed_secs();
        assert!(e2 < e1, "reset should reset the stopwatch");
    }
}
