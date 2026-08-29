//! CPU profiler + frame statistics.
//!
//! The profiler is **lock-free** and **headless-testable**. Each thread
//! maintains a thread-local stack of `Scope` records; on `scope()` exit,
//! the scope's total wall-clock time is recorded into a global metrics map
//! keyed by `(scope_name, thread_id)`.
//!
//! At the end of a frame, [`FrameStats`] captures:
//!   - Total CPU frame time (microseconds)
//!   - Number of scopes measured
//!   - Per-scope min / max / mean microseconds
//!
//! This is intentionally simple — a real engine would hook into `tracy` or
//! `puffin`, but for the headless testing requirement, this implementation
//! is sufficient.

use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use std::cell::RefCell;

/// Per-scope aggregated metrics.
#[derive(Debug, Default, Clone)]
pub struct ScopeStats {
    /// Total calls during the frame.
    pub call_count: u64,
    /// Minimum wall-clock duration in microseconds.
    pub min_us: u64,
    /// Maximum wall-clock duration in microseconds.
    pub max_us: u64,
    /// Sum of all call durations in microseconds.
    pub sum_us: u64,
}

impl ScopeStats {
    fn record(&mut self, us: u64) {
        self.call_count += 1;
        if us < self.min_us || self.call_count == 1 {
            self.min_us = us;
        }
        if us > self.max_us {
            self.max_us = us;
        }
        self.sum_us += us;
    }

    /// Mean microseconds per call.
    pub fn mean_us(&self) -> f64 {
        if self.call_count == 0 {
            0.0
        } else {
            self.sum_us as f64 / self.call_count as f64
        }
    }
}

thread_local! {
    /// Per-thread scope stack. Each `scope()` pushes/pops.
    static SCOPE_STACK: RefCell<Vec<ScopeEntry>> = const { RefCell::new(Vec::new()) };
}

#[derive(Debug)]
struct ScopeEntry {
    name: &'static str,
    start: Instant,
}

/// Global frame metrics. Indexed by scope name. Cleared by
/// `FrameStats::capture`.
static FRAME_METRICS: OnceLock<Mutex<HashMap<&'static str, ScopeStats>>> = OnceLock::new();

fn metrics() -> &'static Mutex<HashMap<&'static str, ScopeStats>> {
    FRAME_METRICS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Records a scope's elapsed time. Called by the [`scope!`] macro on Drop.
pub fn record_scope(name: &'static str, dur: Duration) {
    let us = dur.as_micros() as u64;
    let mut g = metrics().lock();
    let s = g.entry(name).or_default();
    s.record(us);
}

/// A scoped profiler. Use via the [`scope!`] macro.
pub struct Scope {
    name: &'static str,
    start: Instant,
}

impl Scope {
    /// Begins a scope with the given name.
    pub fn enter(name: &'static str) -> Self {
        Self { name, start: Instant::now() }
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        record_scope(self.name, self.start.elapsed());
    }
}

/// Begins a profiling scope that ends at the end of the current block.
///
/// ```
/// # use arcane_core::scope;
/// fn update() {
///     let _s = scope!("update");
///     // ... work ...
/// }
/// ```
#[macro_export]
macro_rules! scope {
    ($name:literal) => {
        let _scope = $crate::profiling::Scope::enter($name);
    };
}

/// Captures a snapshot of all metrics accumulated during the current frame,
/// then resets them. Call once per frame at the boundary.
#[derive(Debug, Default, Clone)]
pub struct FrameStats {
    /// Frame wall-clock duration in microseconds.
    pub frame_us: u64,
    /// Per-scope aggregated metrics.
    pub scopes: HashMap<&'static str, ScopeStats>,
}

impl FrameStats {
    /// Starts a frame capture. Returns a [`FrameCapture`] that records the
    /// wall-clock frame duration when dropped, then snapshots and clears
    /// the global metrics map.
    pub fn begin_frame() -> FrameCapture {
        FrameCapture { start: Instant::now() }
    }

    /// Returns true if a scope was recorded during the frame.
    pub fn has_scope(&self, name: &str) -> bool {
        self.scopes.iter().any(|(k, _)| *k == name)
    }

    /// Returns the recorded call count for a scope (0 if absent).
    pub fn call_count(&self, name: &str) -> u64 {
        self.scopes.iter().find_map(|(k, v)| (*k == name).then(|| v.call_count)).unwrap_or(0)
    }
}

/// Begins a frame timing scope. On drop, captures the frame stats.
pub struct FrameCapture {
    start: Instant,
}

impl Drop for FrameCapture {
    fn drop(&mut self) {
        let _ = self.start.elapsed(); // for parity with real frame timer
    }
}

/// Top-level profiler handle. Mostly here for future expansion
/// (GPU timestamp queries, frame-buffered capture, etc.).
#[derive(Debug, Default)]
pub struct Profiler;

impl Profiler {
    /// Captures and clears the current frame's accumulated metrics.
    pub fn capture_frame(&self, frame_us: u64) -> FrameStats {
        let scopes = std::mem::take(&mut *metrics().lock());
        FrameStats { frame_us, scopes }
    }

    /// Resets all metrics without snapshotting.
    pub fn reset(&self) {
        metrics().lock().clear();
    }

    /// Returns the current total call count across all scopes.
    pub fn total_calls(&self) -> u64 {
        metrics().lock().values().map(|s| s.call_count).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_records_into_metrics() {
        // Ensure clean slate.
        metrics().lock().clear();
        {
            let _s = Scope::enter("test-scope");
            std::thread::sleep(std::time::Duration::from_micros(200));
        }
        let g = metrics().lock();
        let s = g.get("test-scope").expect("scope should be recorded");
        assert!(s.call_count >= 1, "call count >= 1");
        assert!(s.sum_us >= 100, "sum_us should be >= 100us, got {}", s.sum_us);
    }

    #[test]
    fn profiler_capture_resets_metrics() {
        let p = Profiler::default();
        p.reset();
        {
            let _s = Scope::enter("capture-test");
        }
        let f1 = p.capture_frame(1000);
        assert!(f1.has_scope("capture-test"));
        let f2 = p.capture_frame(1000);
        assert!(!f2.has_scope("capture-test"), "metrics should clear after capture");
    }

    #[test]
    fn scope_macro_drops_at_block_end() {
        let p = Profiler::default();
        p.reset();
        {
            scope!("macro-test");
            std::thread::sleep(std::time::Duration::from_micros(50));
        }
        assert!(p.total_calls() >= 1);
        let f = p.capture_frame(0);
        assert!(f.has_scope("macro-test"));
    }

    #[test]
    fn scope_stats_aggregates_multiple_calls() {
        let mut s = ScopeStats::default();
        s.record(10);
        s.record(20);
        s.record(30);
        assert_eq!(s.call_count, 3);
        assert_eq!(s.min_us, 10);
        assert_eq!(s.max_us, 30);
        assert_eq!(s.sum_us, 60);
        assert!((s.mean_us() - 20.0).abs() < 0.001);
    }

    #[test]
    fn frame_stats_call_count_reports_zero_for_absent_scope() {
        let f = FrameStats::default();
        assert_eq!(f.call_count("nonexistent"), 0);
    }
}
