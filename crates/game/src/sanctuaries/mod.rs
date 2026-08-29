//! Sanctuaries — warm, stable, controlled refuges in the wilderness.
//!
//! Per the design doc:
//!   "Sanctuaries: storage, crafting, research, spell work, regen,
//!    magical stability, protection. Should visually and mechanically
//!    contrast with unstable wilderness."
//!
//! A sanctuary is a region (anchored at a [`Structure`] with
//! `provides_sanctuary`) that grants passive bonuses to the player inside.

use crate::building::Structure;
use crate::corruption::CorruptionState;
use crate::mana::ManaPool;
use serde::{Deserialize, Serialize};

/// A sanctuary's effects on the player. Computed by the sanctuary system each
/// tick based on the player's position and active sanctuaries.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub struct SanctuaryEffects {
    /// Bonus mana regen per second while inside.
    pub mana_regen_bonus: f32,
    /// True if the player benefits from sanctuary protection.
    pub protection_active: bool,
    /// Corruption reduction per second while inside.
    pub corruption_reduction_per_sec: f32,
    /// True if research can be performed (multiplies research speed).
    pub research_active: bool,
    /// True if crafting is enabled here.
    pub crafting_enabled: bool,
}

/// A defined sanctuary region. Created by placing a `sanctuary_core` structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sanctuary {
    /// The structure that anchors this sanctuary.
    pub anchor_structure_id: arcane_core::IdUlid,
    /// Center of the sanctuary's area of effect.
    pub center: [f32; 3],
    /// Influence radius in meters.
    pub radius: f32,
    /// Current stability of the sanctuary (mirrors the structure's stability).
    pub stability: f32,
}

impl Sanctuary {
    /// Constructs a new sanctuary around the given structure + radius.
    pub fn new(structure: &Structure, center: [f32; 3], radius: f32) -> Self {
        Self {
            anchor_structure_id: structure.id,
            center,
            radius,
            stability: structure.stability,
        }
    }

    /// True if `player_pos` is inside the sanctuary's influence radius.
    pub fn contains(&self, player_pos: [f32; 3]) -> bool {
        let dx = player_pos[0] - self.center[0];
        let dy = player_pos[1] - self.center[1];
        let dz = player_pos[2] - self.center[2];
        let dist_sq = dx * dx + dy * dy + dz * dz;
        dist_sq <= self.radius * self.radius
    }

    /// Returns the per-tick effects the sanctuary applies to the player.
    /// Falloff is linear from center (full) to edge (zero).
    pub fn effects_at(&self, player_pos: [f32; 3]) -> SanctuaryEffects {
        if !self.contains(player_pos) {
            return SanctuaryEffects::default();
        }
        let dx = player_pos[0] - self.center[0];
        let dy = player_pos[1] - self.center[1];
        let dz = player_pos[2] - self.center[2];
        let dist = (dx * dx + dy * dy + dz * dz).sqrt();
        let falloff = (1.0 - dist / self.radius).max(0.0) * self.stability;
        SanctuaryEffects {
            mana_regen_bonus: 10.0 * falloff,
            protection_active: falloff > 0.3,
            corruption_reduction_per_sec: 0.05 * falloff,
            research_active: falloff > 0.5,
            crafting_enabled: falloff > 0.2,
        }
    }

    /// Updates the sanctuary's stability from the anchoring structure.
    pub fn update_stability(&mut self, structure: &Structure) {
        self.stability = structure.stability;
    }
}

/// Aggregates effects from all active sanctuaries at `player_pos`.
pub fn aggregate_effects(sanctuaries: &[Sanctuary], player_pos: [f32; 3]) -> SanctuaryEffects {
    let mut out = SanctuaryEffects::default();
    for s in sanctuaries {
        let e = s.effects_at(player_pos);
        out.mana_regen_bonus += e.mana_regen_bonus;
        out.protection_active |= e.protection_active;
        out.corruption_reduction_per_sec += e.corruption_reduction_per_sec;
        out.research_active |= e.research_active;
        out.crafting_enabled |= e.crafting_enabled;
    }
    out
}

/// Applies sanctuary effects to the player's mana pool and corruption state
/// for the duration of one tick (`dt` seconds).
pub fn apply_effects(
    mana: &mut ManaPool,
    corruption: &mut CorruptionState,
    effects: &SanctuaryEffects,
    dt: f32,
) {
    // Apply regen bonus to mana pool.
    if effects.mana_regen_bonus > 0.0 {
        mana.restore(effects.mana_regen_bonus * dt);
    }
    // Apply corruption reduction.
    if effects.corruption_reduction_per_sec > 0.0 {
        corruption.reduce(effects.corruption_reduction_per_sec * dt);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::building::default_structures;

    fn make_structure() -> Structure {
        let def = default_structures().into_iter().find(|d| d.id == "sanctuary_core").unwrap();
        Structure::new(&def, [0.0, 0.0, 0.0])
    }

    #[test]
    fn sanctuary_at_center_provides_full_effects() {
        let s = Sanctuary::new(&make_structure(), [0.0, 0.0, 0.0], 20.0);
        let e = s.effects_at([0.0, 0.0, 0.0]);
        assert!((e.mana_regen_bonus - 10.0).abs() < 1e-6);
        assert!(e.protection_active);
        assert!(e.research_active);
        assert!(e.crafting_enabled);
        assert!((e.corruption_reduction_per_sec - 0.05).abs() < 1e-6);
    }

    #[test]
    fn sanctuary_outside_radius_provides_nothing() {
        let s = Sanctuary::new(&make_structure(), [0.0, 0.0, 0.0], 20.0);
        let e = s.effects_at([30.0, 0.0, 0.0]);
        assert_eq!(e, SanctuaryEffects::default());
        assert!(!s.contains([30.0, 0.0, 0.0]));
    }

    #[test]
    fn sanctuary_falloff_at_edge() {
        let s = Sanctuary::new(&make_structure(), [0.0, 0.0, 0.0], 10.0);
        let e = s.effects_at([9.9, 0.0, 0.0]);
        // At 99% of radius, falloff = 0.01, mana_regen_bonus = 0.1
        assert!(e.mana_regen_bonus < 0.5, "near edge, bonus should be tiny: {}", e.mana_regen_bonus);
    }

    #[test]
    fn sanctuary_low_stability_reduces_effects() {
        let mut s = Sanctuary::new(&make_structure(), [0.0, 0.0, 0.0], 20.0);
        s.stability = 0.5;
        let e = s.effects_at([0.0, 0.0, 0.0]);
        assert!((e.mana_regen_bonus - 5.0).abs() < 1e-6, "half stability → half bonus");
    }

    #[test]
    fn aggregate_effects_sums_multiple_sanctuaries() {
        let s1 = Sanctuary::new(&make_structure(), [0.0, 0.0, 0.0], 20.0);
        let s2 = Sanctuary::new(&make_structure(), [3.0, 0.0, 0.0], 20.0);
        let total = aggregate_effects(&[s1, s2], [0.0, 0.0, 0.0]);
        // s1 at center: 10.0. s2 at distance 3, falloff = (1 - 3/20) = 0.85, bonus = 8.5.
        assert!((total.mana_regen_bonus - 18.5).abs() < 0.1, "got {}", total.mana_regen_bonus);
    }

    #[test]
    fn apply_effects_increases_mana_and_reduces_corruption() {
        let mut mana = ManaPool::new(100.0, 0.0);
        mana.current_mana = 50.0;
        let mut c = CorruptionState::new(0.5);
        let e = SanctuaryEffects {
            mana_regen_bonus: 20.0,
            protection_active: true,
            corruption_reduction_per_sec: 0.1,
            research_active: false,
            crafting_enabled: false,
        };
        apply_effects(&mut mana, &mut c, &e, 1.0);
        assert!((mana.current_mana - 70.0).abs() < 1e-6, "mana should restore by 20");
        assert!((c.level - 0.4).abs() < 1e-6, "corruption should reduce by 0.1");
    }

    #[test]
    fn sanctuary_postcard_roundtrip() {
        let s = Sanctuary::new(&make_structure(), [5.0, 0.0, 7.0], 15.0);
        let bytes = postcard::to_allocvec(&s).unwrap();
        let back: Sanctuary = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(s.center, back.center);
        assert_eq!(s.radius, back.radius);
    }

    #[test]
    fn sanctuary_update_stability_from_structure() {
        let mut struct_ = make_structure();
        let mut s = Sanctuary::new(&struct_, [0.0, 0.0, 0.0], 10.0);
        assert!((s.stability - 1.0).abs() < 1e-6);
        struct_.stability = 0.7;
        s.update_stability(&struct_);
        assert!((s.stability - 0.7).abs() < 1e-6);
    }
}
