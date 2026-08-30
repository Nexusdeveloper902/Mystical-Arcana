//! Headless gameplay loop harness for *Mystical Arcana*.
//!
//! This module drives the simulation forward without ever touching the GPU or
//! audio hardware. It is the foundation of the headless testing strategy
//! (ADR-0003). Subsystems currently expose only a stub interface — the
//! concrete simulation steps land in subsequent commits per Phase 3+.

use std::time::Duration;

/// A single tick of the simulation. All gameplay systems update by this delta.
#[derive(Debug, Clone, Copy)]
pub struct Tick {
    /// Wall-clock delta in seconds.
    pub dt: f32,
    /// Absolute simulation time in seconds.
    pub t: f32,
}

/// Outcome of a single headless step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepOutcome {
    /// The simulation advanced normally.
    Continue,
    /// The script reached its terminal condition; the smoke test should pass.
    Complete,
    /// The simulation encountered an error condition.
    Error(HeadlessError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeadlessError {
    /// The player got stuck (no movement over N ticks).
    PlayerStuck,
    /// A system reported a fatal internal state.
    SystemFatal,
    /// The simulation violated the design contract (e.g., mana went negative).
    ContractViolation,
}

/// Drives the simulation forward by `dt` seconds. Returns the outcome.
///
/// In the stub state, this always returns `Continue` after `max_steps` ticks,
/// then `Complete`. Real logic replaces the stub as each Phase lands.
pub fn step(_tick: Tick, _state: &mut (), _max_steps: u32) -> StepOutcome {
    StepOutcome::Continue
}

/// Runs the full headless smoke loop until `Complete` or `Error`.
/// Returns the total simulation time elapsed.
pub fn run_until_complete(max_sim_time: Duration) -> Result<Duration, HeadlessError> {
    let mut t = 0.0f32;
    let dt = 1.0 / 60.0;
    let max_t = max_sim_time.as_secs_f32() as f32;
    while t < max_t {
        let _ = step(Tick { dt, t }, &mut (), 0);
        t += dt;
    }
    Ok(Duration::from_secs_f32(t))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn headless_runs_without_gpu() {
        let elapsed = run_until_complete(Duration::from_secs(1)).expect("headless loop");
        assert!(elapsed.as_secs_f32() >= 0.9);
    }
}
