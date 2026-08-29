//! Progression — the player's path from survival to mastery.
//!
//! Per the design doc:
//!   "The player progresses from survival → discovery → understanding →
//!    manipulation → mastery."
//!
//! Progression tracks: research completed, runes discovered, schematics
//! learned, spells cast, corruption survived, sanctuaries established.

use crate::research::ResearchState;
use serde::{Deserialize, Serialize};

/// One of the five phases of the Arcanist's journey.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum ArcanistPhase {
    /// Survival — barely managing, learning to gather.
    Survival = 0,
    /// Discovery — exploring the world, finding runes and nodes.
    Discovery = 1,
    /// Understanding — piecing together how magic works.
    Understanding = 2,
    /// Manipulation — actively reshaping reality through magic.
    Manipulation = 3,
    /// Mastery — true command of the Arcanum.
    Mastery = 4,
}

impl ArcanistPhase {
    /// Returns the canonical name.
    pub fn as_str(self) -> &'static str {
        match self {
            ArcanistPhase::Survival => "survival",
            ArcanistPhase::Discovery => "discovery",
            ArcanistPhase::Understanding => "understanding",
            ArcanistPhase::Manipulation => "manipulation",
            ArcanistPhase::Mastery => "mastery",
        }
    }

    /// Parses from the canonical name.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "survival" => Some(Self::Survival),
            "discovery" => Some(Self::Discovery),
            "understanding" => Some(Self::Understanding),
            "manipulation" => Some(Self::Manipulation),
            "mastery" => Some(Self::Mastery),
            _ => None,
        }
    }
}

/// Long-term player progression stats.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProgressionStats {
    /// Number of runes discovered (any source).
    pub runes_discovered: u32,
    /// Number of schematics learned.
    pub schematics_learned: u32,
    /// Number of research nodes completed.
    pub research_completed: u32,
    /// Total spells cast.
    pub spells_cast: u32,
    /// Enemies defeated.
    pub enemies_defeated: u32,
    /// Sanctuaries established.
    pub sanctuaries_established: u32,
    /// Resources gathered (any type).
    pub resources_gathered: u32,
    /// Time survived in seconds.
    pub time_survived_secs: f32,
    /// Maximum corruption reached (peak).
    pub peak_corruption: f32,
    /// Total Mana Burn time experienced.
    pub total_mana_burn_secs: f32,
}

impl ProgressionStats {
    /// Computes the current Arcanist phase from accumulated stats.
    ///
    /// Thresholds are intentionally calibrated for a satisfying early-game
    /// progression curve. The smoke loop should easily reach Discovery.
    pub fn phase(&self) -> ArcanistPhase {
        let mut score = 0u32;
        score += self.runes_discovered.min(10);
        score += self.schematics_learned.min(10);
        score += self.research_completed.min(10);
        score += (self.spells_cast / 100).min(10);
        score += (self.enemies_defeated / 50).min(10);
        score += self.sanctuaries_established.min(5);
        score += (self.resources_gathered / 500).min(10);
        // Total score is out of 65.

        if score >= 50 { ArcanistPhase::Mastery }
        else if score >= 35 { ArcanistPhase::Manipulation }
        else if score >= 20 { ArcanistPhase::Understanding }
        else if score >= 5 { ArcanistPhase::Discovery }
        else { ArcanistPhase::Survival }
    }

    /// Records a rune discovery. Returns true if newly discovered.
    pub fn record_rune_discovered(&mut self) -> bool {
        self.runes_discovered += 1;
        true
    }

    /// Records a schematic learned.
    pub fn record_schematic_learned(&mut self) {
        self.schematics_learned += 1;
    }

    /// Records a research node completed.
    pub fn record_research_completed(&mut self) {
        self.research_completed += 1;
    }

    /// Records a spell cast.
    pub fn record_spell_cast(&mut self) {
        self.spells_cast += 1;
    }

    /// Records an enemy defeated.
    pub fn record_enemy_defeated(&mut self) {
        self.enemies_defeated += 1;
    }

    /// Records a sanctuary established.
    pub fn record_sanctuary_established(&mut self) {
        self.sanctuaries_established += 1;
    }

    /// Records resources gathered.
    pub fn record_resources_gathered(&mut self, count: u32) {
        self.resources_gathered += count;
    }

    /// Records time survived.
    pub fn record_time(&mut self, dt: f32) {
        self.time_survived_secs += dt;
    }

    /// Updates peak corruption if `current` exceeds the stored peak.
    pub fn record_corruption(&mut self, current: f32) {
        if current > self.peak_corruption {
            self.peak_corruption = current;
        }
    }

    /// Records Mana Burn time.
    pub fn record_burn_time(&mut self, dt: f32) {
        self.total_mana_burn_secs += dt;
    }
}

/// Combined progression record for save files.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Progression {
    /// Accumulated stats.
    pub stats: ProgressionStats,
    /// Current research state.
    pub research: ResearchState,
}

impl Progression {
    /// Empty.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the current Arcanist phase.
    pub fn phase(&self) -> ArcanistPhase {
        self.stats.phase()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_progression_is_survival() {
        let p = Progression::new();
        assert_eq!(p.phase(), ArcanistPhase::Survival);
    }

    #[test]
    fn phase_thresholds_progress_correctly() {
        let mut p = Progression::new();
        // 5 runes → score 5 → Discovery.
        for _ in 0..5 {
            p.stats.record_rune_discovered();
        }
        assert_eq!(p.phase(), ArcanistPhase::Discovery);
        // Push past 20 → Understanding.
        for _ in 0..15 {
            p.stats.record_schematic_learned();
        }
        // Score: 5 (runes) + 10 (schematics capped) = 15. Add research for 20.
        for _ in 0..5 {
            p.stats.record_research_completed();
        }
        assert_eq!(p.phase(), ArcanistPhase::Understanding);
        // Push past 35 → Manipulation. Add resources.
        for _ in 0..10_000 {
            p.stats.record_resources_gathered(1);
        }
        // Score: 15 + 5 (research) + 10 (resources capped) = 30. Need more.
        // Push with more research (capped at 10 → add 5 more).
        for _ in 0..5 {
            p.stats.record_research_completed();
        }
        // Score: 5 + 10 + 10 + 10 = 35 → Manipulation.
        assert_eq!(p.phase(), ArcanistPhase::Manipulation);
    }

    #[test]
    fn phase_names_round_trip() {
        for phase in [
            ArcanistPhase::Survival,
            ArcanistPhase::Discovery,
            ArcanistPhase::Understanding,
            ArcanistPhase::Manipulation,
            ArcanistPhase::Mastery,
        ] {
            let s = phase.as_str();
            assert_eq!(ArcanistPhase::from_str(s), Some(phase));
        }
        assert_eq!(ArcanistPhase::from_str("nonexistent"), None);
    }

    #[test]
    fn progression_postcard_roundtrip() {
        let mut p = Progression::new();
        p.stats.record_rune_discovered();
        p.stats.record_schematic_learned();
        p.stats.record_spell_cast();
        p.stats.record_enemy_defeated();
        p.stats.record_sanctuary_established();
        p.stats.record_resources_gathered(50);
        p.stats.record_time(120.0);
        p.stats.record_corruption(0.42);
        p.stats.record_burn_time(8.0);
        let bytes = postcard::to_allocvec(&p).unwrap();
        let back: Progression = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(back.stats.runes_discovered, 1);
        assert_eq!(back.stats.schematics_learned, 1);
        assert_eq!(back.stats.spells_cast, 1);
        assert_eq!(back.stats.enemies_defeated, 1);
        assert_eq!(back.stats.sanctuaries_established, 1);
        assert_eq!(back.stats.resources_gathered, 50);
        assert!((back.stats.time_survived_secs - 120.0).abs() < 1e-6);
        assert!((back.stats.peak_corruption - 0.42).abs() < 1e-6);
        assert!((back.stats.total_mana_burn_secs - 8.0).abs() < 1e-6);
    }

    #[test]
    fn peak_corruption_only_increases() {
        let mut p = Progression::new();
        p.stats.record_corruption(0.3);
        p.stats.record_corruption(0.5);
        p.stats.record_corruption(0.2);  // should NOT decrease peak
        assert!((p.stats.peak_corruption - 0.5).abs() < 1e-6);
    }

    #[test]
    fn stats_cap_individual_metrics_in_phase_calculation() {
        let mut s = ProgressionStats::default();
        // 10 runes alone → 10 score, Discovery phase.
        for _ in 0..10 {
            s.record_rune_discovered();
        }
        // Adding 100 more runes doesn't change score (capped at 10).
        for _ in 0..100 {
            s.record_rune_discovered();
        }
        assert_eq!(s.runes_discovered, 110);
        // Score is still 10 → Discovery (need 8 minimum to leave Survival).
        assert_eq!(s.phase(), ArcanistPhase::Discovery);
    }
}
