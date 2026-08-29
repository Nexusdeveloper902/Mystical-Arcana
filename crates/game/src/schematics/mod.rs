//! Learned magical schematics.
//!
//! Per the design doc:
//!   "Schematics are the player's learned magical recipes. They represent
//!    'I understand this magical construction.'"
//!
//! The player distinguishes between:
//!   - Improvisation — trying a rune combination they haven't learned yet.
//!   - Knowledge — using a known schematic reliably.
//!
//! When the player experiments with an unknown rune combination and it
//! produces a valid spell, they may learn it as a schematic. Once learned,
//! the schematic is permanently available and reproducible.

use crate::runes::RunePair;
use crate::spells::SchematicSpell;
use arcane_core::Id64;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// A learned schematic — owned by the player.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearnedSchematic {
    /// Stable id of the spell this schematic unlocks.
    pub spell_id: Id64,
    /// The rune pair that composes this schematic.
    pub pair: RunePair,
    /// Number of times the player has cast this schematic.
    pub casts: u32,
    /// True if the schematic has been "mastered" — typically achieved by
    /// casting it N times. Mastered schematics have reduced cooldown.
    pub mastered: bool,
    /// Time the schematic was first learned (sim time in seconds).
    pub learned_at_secs: f32,
}

/// The player's known schematics. Indexed by spell stable id.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct SchematicCollection {
    schematics: HashSet<Id64>,
    /// Per-spell learned detail. Reconstructed from spell registry on load.
    details: Vec<LearnedSchematic>,
}

impl SchematicCollection {
    /// Empty.
    pub fn new() -> Self {
        Self::default()
    }

    /// True if the player has learned the schematic for `spell_id`.
    pub fn knows(&self, spell_id: Id64) -> bool {
        self.schematics.contains(&spell_id)
    }

    /// Learns a new schematic. Returns true if it was newly learned.
    pub fn learn(&mut self, spell: &SchematicSpell, learned_at_secs: f32) -> bool {
        if self.schematics.contains(&spell.stable_id()) {
            return false;
        }
        self.schematics.insert(spell.stable_id());
        self.details.push(LearnedSchematic {
            spell_id: spell.stable_id(),
            pair: spell.pair,
            casts: 0,
            mastered: false,
            learned_at_secs,
        });
        true
    }

    /// Forgets a schematic (used by debug tools / future "memory limit" system).
    pub fn forget(&mut self, spell_id: Id64) {
        self.schematics.remove(&spell_id);
        self.details.retain(|d| d.spell_id != spell_id);
    }

    /// Records a cast of the given spell. Returns the new cast count.
    /// If the cast count crosses the mastery threshold, sets `mastered = true`.
    pub fn record_cast(&mut self, spell_id: Id64, mastery_threshold: u32) -> u32 {
        if let Some(d) = self.details.iter_mut().find(|d| d.spell_id == spell_id) {
            d.casts = d.casts.saturating_add(1);
            if d.casts >= mastery_threshold {
                d.mastered = true;
            }
            d.casts
        } else {
            0
        }
    }

    /// True if the schematic for `spell_id` is mastered.
    pub fn is_mastered(&self, spell_id: Id64) -> bool {
        self.details.iter().find(|d| d.spell_id == spell_id).map(|d| d.mastered).unwrap_or(false)
    }

    /// Returns the LearnedSchematic for the given spell, if known.
    pub fn get(&self, spell_id: Id64) -> Option<&LearnedSchematic> {
        self.details.iter().find(|d| d.spell_id == spell_id)
    }

    /// Number of learned schematics.
    pub fn count(&self) -> usize {
        self.schematics.len()
    }

    /// Iterates all learned schematics.
    pub fn iter(&self) -> impl Iterator<Item = &LearnedSchematic> {
        self.details.iter()
    }

    /// True if a rune pair matches a known schematic (lets the player
    /// distinguish "improvisation" from "knowledge").
    pub fn knows_pair(&self, pair: RunePair) -> bool {
        let canon = pair.canonical();
        self.details.iter().any(|d| d.pair.canonical() == canon)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spells::{default_spell_registry, default_spells};
    use crate::runes::RunePair;

    #[test]
    fn empty_collection_knows_nothing() {
        let c = SchematicCollection::new();
        assert_eq!(c.count(), 0);
        assert!(!c.knows(Id64::from_str("fire_bolt")));
    }

    #[test]
    fn learn_adds_schematic() {
        let mut c = SchematicCollection::new();
        let fire_bolt = default_spells().into_iter().find(|s| s.id == "fire_bolt").unwrap();
        assert!(c.learn(&fire_bolt, 0.0));
        assert!(c.knows(fire_bolt.stable_id()));
        assert_eq!(c.count(), 1);
    }

    #[test]
    fn learn_is_idempotent() {
        let mut c = SchematicCollection::new();
        let fire_bolt = default_spells().into_iter().find(|s| s.id == "fire_bolt").unwrap();
        assert!(c.learn(&fire_bolt, 0.0));
        assert!(!c.learn(&fire_bolt, 0.0), "second learn should return false");
    }

    #[test]
    fn record_cast_tracks_count_and_mastery() {
        let mut c = SchematicCollection::new();
        let fire_bolt = default_spells().into_iter().find(|s| s.id == "fire_bolt").unwrap();
        c.learn(&fire_bolt, 0.0);
        let id = fire_bolt.stable_id();
        for _ in 0..9 {
            let n = c.record_cast(id, 10);
            assert!(!c.is_mastered(id), "should not be mastered at {} casts", n);
        }
        let _ = c.record_cast(id, 10);
        assert!(c.is_mastered(id), "should be mastered at 10 casts");
    }

    #[test]
    fn forget_removes_schematic() {
        let mut c = SchematicCollection::new();
        let fire_bolt = default_spells().into_iter().find(|s| s.id == "fire_bolt").unwrap();
        let id = fire_bolt.stable_id();
        c.learn(&fire_bolt, 0.0);
        assert!(c.knows(id));
        c.forget(id);
        assert!(!c.knows(id));
        assert_eq!(c.count(), 0);
    }

    #[test]
    fn knows_pair_distinguishes_improvisation_from_knowledge() {
        let mut c = SchematicCollection::new();
        let fire_bolt = default_spells().into_iter().find(|s| s.id == "fire_bolt").unwrap();
        c.learn(&fire_bolt, 0.0);
        let fire = arcane_core::Id64::from_str("fire");
        let pierce = arcane_core::Id64::from_str("pierce");
        // Known pair should be recognized.
        assert!(c.knows_pair(RunePair::new(fire, pierce)));
        // Order-independent.
        assert!(c.knows_pair(RunePair::new(pierce, fire)));
        // Unknown pair should not be recognized.
        let ice = arcane_core::Id64::from_str("ice");
        assert!(!c.knows_pair(RunePair::new(ice, pierce)));
    }

    #[test]
    fn default_spell_registry_supports_learning() {
        let spell_reg = default_spell_registry();
        let mut c = SchematicCollection::new();
        for (_, s) in spell_reg.iter() {
            c.learn(s, 0.0);
        }
        assert_eq!(c.count(), spell_reg.len());
    }

    #[test]
    fn collection_serializes_roundtrip() {
        let mut c = SchematicCollection::new();
        for s in default_spells().iter().take(3) {
            c.learn(s, 0.0);
        }
        let bytes = postcard::to_allocvec(&c).unwrap();
        let back: SchematicCollection = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(back.count(), c.count());
    }
}
