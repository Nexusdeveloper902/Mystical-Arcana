//! Mana corruption — destabilization of the world via mana overuse.
//!
//! Per the design doc:
//!   "Increasing magical instability may create: stronger creatures,
//!    altered behavior, visual transformation, environmental anomalies,
//!    area effects, increased danger."
//!
//! Corruption is regional: each chunk has a `CorruptionLevel` (0..1).
//! The player's spellcasting pushes it up; sanctuaries pull it down; ley
//! lines spread it.

use serde::{Deserialize, Serialize};

/// Five bands of corruption intensity.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum CorruptionBand {
    /// Stable, normal environment.
    Stable,
    /// Slight, almost imperceptible changes.
    Mild,
    /// Visible changes, more dangerous creatures.
    Moderate,
    /// Strong corruption, anomalies, unstable physics.
    Severe,
    /// Fully corrupted, extremely dangerous.
    Cataclysmic,
}

impl CorruptionBand {
    /// Classifies a raw 0..1 corruption value into a band.
    pub fn from_value(v: f32) -> Self {
        let v = v.clamp(0.0, 1.0);
        if v < 0.2 { Self::Stable }
        else if v < 0.4 { Self::Mild }
        else if v < 0.6 { Self::Moderate }
        else if v < 0.8 { Self::Severe }
        else { Self::Cataclysmic }
    }

    /// Returns a numeric severity 0..4.
    pub fn severity(self) -> u8 {
        match self {
            Self::Stable => 0,
            Self::Mild => 1,
            Self::Moderate => 2,
            Self::Severe => 3,
            Self::Cataclysmic => 4,
        }
    }
}

/// Regional corruption state for a chunk or area.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct CorruptionState {
    /// Raw 0..1 corruption value.
    pub level: f32,
    /// Rate of natural decay per second (sanctuaries amplify this).
    pub decay_per_sec: f32,
}

impl Default for CorruptionState {
    fn default() -> Self {
        Self { level: 0.0, decay_per_sec: 0.005 }
    }
}

impl CorruptionState {
    /// Constructs a fresh corruption state at the given initial level.
    pub fn new(level: f32) -> Self {
        Self { level: level.clamp(0.0, 1.0), decay_per_sec: 0.005 }
    }

    /// Adds corruption (e.g. from player overcast, enemy death, ley line surge).
    pub fn add(&mut self, delta: f32) {
        self.level = (self.level + delta).clamp(0.0, 1.0);
    }

    /// Reduces corruption (e.g. sanctuary effect).
    pub fn reduce(&mut self, delta: f32) {
        self.level = (self.level - delta).clamp(0.0, 1.0);
    }

    /// Advances by `dt` seconds — natural decay toward 0.
    pub fn tick(&mut self, dt: f32) {
        self.level = (self.level - self.decay_per_sec * dt).clamp(0.0, 1.0);
    }

    /// Current band.
    pub fn band(self) -> CorruptionBand {
        CorruptionBand::from_value(self.level)
    }

    /// Returns a multiplier on enemy damage. Higher corruption → harder enemies.
    pub fn enemy_damage_multiplier(self) -> f32 {
        1.0 + self.level * 1.5 // up to 2.5x at full corruption
    }

    /// Returns a multiplier on enemy health. Higher corruption → tankier enemies.
    pub fn enemy_health_multiplier(self) -> f32 {
        1.0 + self.level * 1.0 // up to 2x
    }

    /// Returns a multiplier on resource yield. Higher corruption → more rare
    /// but more valuable drops.
    pub fn yield_multiplier(self) -> f32 {
        1.0 + self.level * 0.5 // up to 1.5x
    }

    /// Returns true if the corruption has reached a dangerous level.
    pub fn is_dangerous(self) -> bool {
        self.level >= 0.6
    }
}

/// The player's personal corruption. Tracks long-term exposure.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
pub struct PlayerCorruption {
    /// 0..1 — accumulated corruption that does not naturally decay.
    pub permanent: f32,
    /// 0..1 — transient corruption from recent overcast, decays slowly.
    pub transient: f32,
    /// Decay rate for transient (per second).
    pub transient_decay_per_sec: f32,
}

impl PlayerCorruption {
    /// Total corruption (permanent + transient, clamped to 1.0).
    pub fn total(self) -> f32 {
        (self.permanent + self.transient).clamp(0.0, 1.0)
    }

    /// Advances transient decay.
    pub fn tick(&mut self, dt: f32) {
        self.transient = (self.transient - self.transient_decay_per_sec * dt).max(0.0);
    }

    /// Adds transient corruption (e.g. from Mana Burn).
    pub fn add_transient(&mut self, amount: f32) {
        self.transient = (self.transient + amount).clamp(0.0, 1.0);
    }

    /// Adds permanent corruption (e.g. from prolonged exposure or story events).
    /// Permanent corruption is recoverable only through special means.
    pub fn add_permanent(&mut self, amount: f32) {
        self.permanent = (self.permanent + amount).clamp(0.0, 1.0);
    }

    /// True if the player is in danger of being consumed by corruption.
    pub fn is_critical(self) -> bool {
        self.total() >= 0.8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corruption_band_classification() {
        assert_eq!(CorruptionBand::from_value(0.0), CorruptionBand::Stable);
        assert_eq!(CorruptionBand::from_value(0.15), CorruptionBand::Stable);
        assert_eq!(CorruptionBand::from_value(0.2), CorruptionBand::Mild);
        assert_eq!(CorruptionBand::from_value(0.35), CorruptionBand::Mild);
        assert_eq!(CorruptionBand::from_value(0.4), CorruptionBand::Moderate);
        assert_eq!(CorruptionBand::from_value(0.55), CorruptionBand::Moderate);
        assert_eq!(CorruptionBand::from_value(0.6), CorruptionBand::Severe);
        assert_eq!(CorruptionBand::from_value(0.75), CorruptionBand::Severe);
        assert_eq!(CorruptionBand::from_value(0.8), CorruptionBand::Cataclysmic);
        assert_eq!(CorruptionBand::from_value(1.0), CorruptionBand::Cataclysmic);
    }

    #[test]
    fn corruption_band_severity_is_monotonic() {
        let bands = [
            CorruptionBand::Stable,
            CorruptionBand::Mild,
            CorruptionBand::Moderate,
            CorruptionBand::Severe,
            CorruptionBand::Cataclysmic,
        ];
        for w in bands.windows(2) {
            assert!(w[0].severity() < w[1].severity());
        }
    }

    #[test]
    fn corruption_state_clamps_add() {
        let mut c = CorruptionState::new(0.9);
        c.add(0.5);
        assert_eq!(c.level, 1.0);
    }

    #[test]
    fn corruption_state_decays_over_time() {
        let mut c = CorruptionState::new(0.5);
        c.decay_per_sec = 0.05;
        c.tick(1.0);
        assert!((c.level - 0.45).abs() < 1e-6);
        c.tick(100.0); // way past zero
        assert_eq!(c.level, 0.0);
    }

    #[test]
    fn corruption_enemy_multiplier_scales() {
        let stable = CorruptionState::new(0.0);
        let cat = CorruptionState::new(1.0);
        assert!((stable.enemy_damage_multiplier() - 1.0).abs() < 1e-6);
        assert!((cat.enemy_damage_multiplier() - 2.5).abs() < 1e-6);
        assert!((cat.enemy_health_multiplier() - 2.0).abs() < 1e-6);
    }

    #[test]
    fn corruption_is_dangerous_at_severe_threshold() {
        assert!(!CorruptionState::new(0.5).is_dangerous());
        assert!(CorruptionState::new(0.6).is_dangerous());
        assert!(CorruptionState::new(0.9).is_dangerous());
    }

    #[test]
    fn player_corruption_total_combines_permanent_and_transient() {
        let mut p = PlayerCorruption::default();
        p.add_permanent(0.3);
        p.add_transient(0.2);
        assert!((p.total() - 0.5).abs() < 1e-6);
    }

    #[test]
    fn player_corruption_transient_decays() {
        let mut p = PlayerCorruption { transient_decay_per_sec: 0.05, ..Default::default() };
        p.add_transient(0.5);
        p.tick(10.0);
        assert!((p.transient - 0.0).abs() < 1e-6, "transient should decay to zero");
        // Permanent stays.
        assert_eq!(p.permanent, 0.0);
    }

    #[test]
    fn player_corruption_critical_threshold() {
        let mut p = PlayerCorruption::default();
        p.add_permanent(0.8);
        assert!(p.is_critical());
    }

    #[test]
    fn corruption_state_postcard_roundtrip() {
        let c = CorruptionState::new(0.42);
        let bytes = postcard::to_allocvec(&c).unwrap();
        let back: CorruptionState = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(c, back);
    }
}
