//! Player state — the Arcanist.
//!
//! Holds the player's mana pool, health, inventory, schematics, research,
//! progression, and world transform. This is the canonical "player save
//! state" used by the save system.

use crate::combat::Health;
use crate::inventory::Inventory;
use crate::mana::ManaPool;
use crate::progression::Progression;
use crate::schematics::SchematicCollection;
use crate::spells::CooldownState;
use crate::corruption::PlayerCorruption;
use serde::{Deserialize, Serialize};

/// The player's position + orientation in the world.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct PlayerTransform {
    /// Position in world space (meters).
    pub position: [f32; 3],
    /// Yaw angle in radians (rotation around Y axis).
    pub yaw: f32,
    /// Pitch angle in radians (rotation around X axis).
    pub pitch: f32,
    /// Velocity in m/s.
    pub velocity: [f32; 3],
    /// True if currently on the ground (touching terrain).
    pub grounded: bool,
}

impl Default for PlayerTransform {
    fn default() -> Self {
        Self {
            position: [0.0, 32.0, 0.0],
            yaw: 0.0,
            pitch: 0.0,
            velocity: [0.0, 0.0, 0.0],
            grounded: false,
        }
    }
}

/// The full player state. Serialized to disk on save.
///
/// Note: `PlayerState` does not derive `Clone` because [`Inventory`] uses
/// a `HandleTable` (generational index table) that is not cheaply cloneable.
/// For save/load, use `postcard` roundtrip via [`encode`](arcane_core::serialize::encode)
/// / [`decode`](arcane_core::serialize::decode).
#[derive(Debug, Serialize, Deserialize)]
pub struct PlayerState {
    /// World transform.
    pub transform: PlayerTransform,
    /// Mana pool.
    pub mana: ManaPool,
    /// Player health.
    pub health: Health,
    /// Player corruption.
    pub corruption: PlayerCorruption,
    /// Inventory.
    pub inventory: Inventory,
    /// Learned schematics.
    pub schematics: SchematicCollection,
    /// Spell cooldowns.
    pub cooldowns: CooldownState,
    /// Progression (research + stats).
    pub progression: Progression,
}

impl Default for PlayerState {
    fn default() -> Self {
        Self {
            transform: PlayerTransform::default(),
            mana: ManaPool::default(),
            health: Health::new(100.0),
            corruption: PlayerCorruption::default(),
            inventory: Inventory::new(),
            schematics: SchematicCollection::new(),
            cooldowns: CooldownState::default(),
            progression: Progression::new(),
        }
    }
}

impl PlayerState {
    /// Constructs a fresh player state (new game).
    pub fn new() -> Self {
        Self::default()
    }

    /// Advances the player by `dt` seconds. Updates mana pool, corruption,
    /// spell cooldowns, and progression time.
    pub fn tick(&mut self, dt: f32) {
        self.mana.tick(dt);
        self.corruption.tick(dt);
        self.cooldowns.tick(dt);
        self.progression.stats.record_time(dt);
        if self.mana.is_burning() {
            self.progression.stats.record_burn_time(dt);
        }
        self.progression.stats.record_corruption(self.corruption.total());
        // Status effects from health tick.
        let _ = self.health.tick_statuses(dt);
    }

    /// Returns true if the player is dead.
    pub fn is_dead(&self) -> bool {
        self.health.is_dead()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_player_starts_at_default_spawn() {
        let p = PlayerState::new();
        assert_eq!(p.transform.position, [0.0, 32.0, 0.0]);
        assert!((p.mana.current_mana - 100.0).abs() < 1e-6);
        assert!((p.health.current - 100.0).abs() < 1e-6);
        assert!(!p.is_dead());
    }

    #[test]
    fn tick_advances_mana_regen_and_progression_time() {
        let mut p = PlayerState::new();
        p.mana.current_mana = 50.0;
        let t0 = p.progression.stats.time_survived_secs;
        p.tick(2.0);
        // Mana regen: 5/sec * 2 = 10
        assert!((p.mana.current_mana - 60.0).abs() < 1e-6);
        assert!((p.progression.stats.time_survived_secs - (t0 + 2.0)).abs() < 1e-6);
    }

    #[test]
    fn tick_with_burn_records_burn_time() {
        let mut p = PlayerState::new();
        p.mana.trigger_burn();
        p.tick(2.0);
        assert!((p.progression.stats.total_mana_burn_secs - 2.0).abs() < 1e-6);
    }

    #[test]
    fn tick_advances_cooldowns() {
        let mut p = PlayerState::new();
        use arcane_core::Id64;
        let id = Id64::from_str("test");
        p.cooldowns.set(id, 5.0);
        p.tick(2.0);
        assert!(!p.cooldowns.is_ready(id));
        assert!((p.cooldowns.remaining_secs(id) - 3.0).abs() < 1e-6);
    }

    #[test]
    fn tick_records_peak_corruption() {
        let mut p = PlayerState::new();
        p.corruption.add_transient(0.5);
        p.tick(0.0);
        assert!((p.progression.stats.peak_corruption - 0.5).abs() < 1e-6);
        // Add less, peak should not decrease.
        p.corruption.transient = 0.3;
        p.tick(0.0);
        assert!((p.progression.stats.peak_corruption - 0.5).abs() < 1e-6);
    }

    #[test]
    fn player_postcard_roundtrip_preserves_state() {
        let mut p = PlayerState::new();
        p.transform.position = [10.0, 5.0, -20.0];
        p.transform.yaw = 1.5;
        p.mana.current_mana = 75.0;
        p.health.apply_damage(crate::combat::DamageInstance::new(crate::combat::DamageType::Fire, 10.0));
        let bytes = postcard::to_allocvec(&p).unwrap();
        let back: PlayerState = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(back.transform.position, [10.0, 5.0, -20.0]);
        assert!((back.mana.current_mana - 75.0).abs() < 1e-6);
        assert!((back.health.current - 90.0).abs() < 1e-6);
    }
}
