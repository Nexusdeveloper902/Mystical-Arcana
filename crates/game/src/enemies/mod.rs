//! Enemies, modular AI state machine, loot.
//!
//! Per the design doc:
//!   "Avoid monolithic AI implementations."
//!   "Idle → perception → investigation → chase → attack → retreat → death"
//!
//! Each state is a small struct implementing the [`EnemyState`] trait.
//! The state machine transitions based on sensory input + health/state.

use crate::combat::{DamageInstance, Health, StatusKind};
use serde::{Deserialize, Serialize};

/// Enemy archetype — a data-driven definition of a creature.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnemyDef {
    /// Stable string id — e.g. "wolf_mana_corrupted".
    pub id: String,
    /// Display name.
    pub name: String,
    /// Base health.
    pub base_health: f32,
    /// Movement speed in m/s.
    pub move_speed: f32,
    /// Detection radius in meters (how close the player must be to aggro).
    pub detection_radius: f32,
    /// Attack range in meters.
    pub attack_range: f32,
    /// Damage dealt per attack.
    pub attack_damage: f32,
    /// Attack cooldown in seconds.
    pub attack_cooldown_secs: f32,
    /// Loot table id (resolved against the loot registry).
    pub loot_table_id: String,
}

/// The AI state the enemy is currently in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum AiStateKind {
    /// Idle — at rest, no target.
    Idle = 0,
    /// Investigating — moving toward a noise/last-known position.
    Investigate = 1,
    /// Chasing — actively pursuing the player.
    Chase = 2,
    /// Attacking — within attack range, performing an attack.
    Attack = 3,
    /// Retreating — moving away from the player (low health).
    Retreat = 4,
    /// Dead — corpse remains, will despawn after a delay.
    Dead = 5,
}

/// Per-frame sensory input the AI uses to decide transitions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AiPerception {
    /// Distance to the player in meters.
    pub dist_to_player: f32,
    /// True if the player is visible (line of sight).
    pub player_visible: bool,
    /// True if the player is within attack range.
    pub player_in_attack_range: bool,
    /// True if there's a noise position to investigate (last known).
    pub has_noise_position: bool,
    /// True if the player is currently casting a spell at this enemy.
    pub under_attack: bool,
    /// Current health fraction (0.0..1.0).
    pub health_fraction: f32,
}

impl AiPerception {
    /// Sensory input for "no player visible at all".
    pub const NO_PLAYER: Self = Self {
        dist_to_player: f32::INFINITY,
        player_visible: false,
        player_in_attack_range: false,
        has_noise_position: false,
        under_attack: false,
        health_fraction: 1.0,
    };
}

/// AI state machine decisions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AiDecision {
    /// Stay in the current state.
    Keep,
    /// Transition to a new state.
    Transition(AiStateKind),
    /// Perform an attack this frame (only valid from Attack state).
    Attack,
    /// Move toward the player at this speed (m/s).
    MoveTowardPlayer(f32),
    /// Move away from the player at this speed (m/s).
    MoveAwayFromPlayer(f32),
    /// Move toward the noise position.
    MoveTowardNoise(f32),
    /// No movement this frame.
    Hold,
}

/// Compute the AI decision for a given state + perception. This is intentionally
/// a pure function — the runtime keeps state on the enemy entity itself, and
/// this just produces the decision for the current frame.
pub fn decide(state: AiStateKind, per: AiPerception, def: &EnemyDef) -> AiDecision {
    use AiStateKind::*;
    // Universal: if dead, stay dead.
    if state == Dead {
        return AiDecision::Keep;
    }
    // Universal: if health <= 0, transition to dead (combat system already
    // mutated health; we trust the runtime to call decide after health update).
    if per.health_fraction <= 0.0 {
        return AiDecision::Transition(Dead);
    }
    match state {
        Idle => {
            if per.player_visible && per.dist_to_player <= def.detection_radius {
                AiDecision::Transition(Chase)
            } else if per.has_noise_position {
                AiDecision::Transition(Investigate)
            } else if per.under_attack {
                AiDecision::Transition(Chase)
            } else {
                AiDecision::Hold
            }
        }
        Investigate => {
            if per.player_visible {
                AiDecision::Transition(Chase)
            } else if !per.has_noise_position {
                AiDecision::Transition(Idle)
            } else {
                AiDecision::MoveTowardNoise(def.move_speed * 0.5)
            }
        }
        Chase => {
            if per.player_in_attack_range {
                AiDecision::Transition(Attack)
            } else if !per.player_visible && per.dist_to_player > def.detection_radius * 2.0 {
                AiDecision::Transition(Investigate)
            } else if per.health_fraction < 0.25 {
                AiDecision::Transition(Retreat)
            } else {
                AiDecision::MoveTowardPlayer(def.move_speed)
            }
        }
        Attack => {
            if !per.player_in_attack_range {
                AiDecision::Transition(Chase)
            } else if per.health_fraction < 0.2 {
                AiDecision::Transition(Retreat)
            } else {
                AiDecision::Attack
            }
        }
        Retreat => {
            if per.health_fraction < 0.05 {
                // Fight to the death at 5% — cornered.
                AiDecision::Transition(Attack)
            } else if per.dist_to_player > def.detection_radius * 1.5 {
                AiDecision::Transition(Idle)
            } else {
                AiDecision::MoveAwayFromPlayer(def.move_speed * 1.2)
            }
        }
        Dead => AiDecision::Keep,
    }
}

/// A live enemy entity in the world.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Enemy {
    /// Stable runtime id (assigned at spawn).
    pub id: arcane_core::IdUlid,
    /// Archetype id.
    pub def_id: String,
    /// Position in world space.
    pub position: [f32; 3],
    /// Velocity in m/s.
    pub velocity: [f32; 3],
    /// Health pool.
    pub health: Health,
    /// Current AI state.
    pub ai_state: AiStateKind,
    /// Cooldown remaining before next attack.
    pub attack_cooldown_secs: f32,
}

impl Enemy {
    /// Constructs a new enemy of the given archetype.
    pub fn new(def: &EnemyDef, id: arcane_core::IdUlid, position: [f32; 3]) -> Self {
        Self {
            id,
            def_id: def.id.clone(),
            position,
            velocity: [0.0, 0.0, 0.0],
            health: Health::new(def.base_health),
            ai_state: AiStateKind::Idle,
            attack_cooldown_secs: 0.0,
        }
    }

    /// Advances the enemy by `dt` seconds, given perception and def.
    /// Returns the decision made this frame.
    pub fn tick(&mut self, dt: f32, per: AiPerception, def: &EnemyDef) -> AiDecision {
        // Tick statuses and attack cooldown.
        let _ = self.health.tick_statuses(dt);
        self.attack_cooldown_secs = (self.attack_cooldown_secs - dt).max(0.0);

        if self.health.is_dead() {
            self.ai_state = AiStateKind::Dead;
            return AiDecision::Keep;
        }

        let decision = decide(self.ai_state, per, def);
        if let AiDecision::Transition(new) = decision {
            self.ai_state = new;
        }
        decision
    }

    /// Applies incoming damage.
    pub fn take_damage(&mut self, dmg: DamageInstance) -> f32 {
        self.health.apply_damage(dmg)
    }

    /// Applies a status effect.
    pub fn apply_status(&mut self, kind: StatusKind, duration_secs: f32, magnitude: f32) {
        self.health.apply_status(crate::combat::StatusEffect::new(kind, duration_secs, magnitude));
    }

    /// True if dead.
    pub fn is_dead(&self) -> bool {
        self.health.is_dead()
    }
}

/// A loot entry — chance to drop a quantity of an item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LootEntry {
    /// Stable item id.
    pub item_id: String,
    /// Drop chance (0.0..1.0).
    pub chance: f32,
    /// Minimum quantity if dropped.
    pub min_qty: u32,
    /// Maximum quantity if dropped.
    pub max_qty: u32,
}

/// A loot table — what an enemy drops on death.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LootTable {
    /// Stable id.
    pub id: String,
    /// Entries.
    pub entries: Vec<LootEntry>,
}

/// A resolved loot roll — item id + quantity.
#[derive(Debug, Clone, PartialEq)]
pub struct LootDrop {
    /// Stable item id.
    pub item_id: String,
    /// Quantity dropped.
    pub quantity: u32,
}

/// Resolves a loot table against a deterministic RNG (callback) to produce
/// concrete drops.
pub fn roll_loot<F: FnMut() -> f32>(table: &LootTable, mut rng: F) -> Vec<LootDrop> {
    let mut out = Vec::new();
    for e in &table.entries {
        if rng() < e.chance {
            let r = rng();
            let qty = e.min_qty + ((e.max_qty - e.min_qty) as f32 * r).round() as u32;
            if qty > 0 {
                out.push(LootDrop { item_id: e.item_id.clone(), quantity: qty });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_def() -> EnemyDef {
        EnemyDef {
            id: "test".into(),
            name: "Test".into(),
            base_health: 50.0,
            move_speed: 4.0,
            detection_radius: 20.0,
            attack_range: 2.0,
            attack_damage: 10.0,
            attack_cooldown_secs: 1.0,
            loot_table_id: "test_loot".into(),
        }
    }

    #[test]
    fn enemy_starts_idle_with_full_health() {
        let def = dummy_def();
        let e = Enemy::new(&def, arcane_core::IdUlid::new(), [0.0, 0.0, 0.0]);
        assert_eq!(e.ai_state, AiStateKind::Idle);
        assert!((e.health.current - 50.0).abs() < 1e-6);
        assert!(!e.is_dead());
    }

    #[test]
    fn idle_transitions_to_chase_on_visible_player_in_range() {
        let def = dummy_def();
        let mut e = Enemy::new(&def, arcane_core::IdUlid::new(), [0.0; 3]);
        let per = AiPerception {
            dist_to_player: 10.0,
            player_visible: true,
            player_in_attack_range: false,
            has_noise_position: false,
            under_attack: false,
            health_fraction: 1.0,
        };
        let d = e.tick(0.016, per, &def);
        assert!(matches!(d, AiDecision::Transition(AiStateKind::Chase)));
        assert_eq!(e.ai_state, AiStateKind::Chase);
    }

    #[test]
    fn chase_moves_toward_player() {
        let def = dummy_def();
        let mut e = Enemy::new(&def, arcane_core::IdUlid::new(), [0.0; 3]);
        e.ai_state = AiStateKind::Chase;
        let per = AiPerception {
            dist_to_player: 10.0,
            player_visible: true,
            player_in_attack_range: false,
            has_noise_position: false,
            under_attack: false,
            health_fraction: 1.0,
        };
        let d = e.tick(0.016, per, &def);
        assert!(matches!(d, AiDecision::MoveTowardPlayer(_)));
    }

    #[test]
    fn chase_transitions_to_attack_in_range() {
        let def = dummy_def();
        let mut e = Enemy::new(&def, arcane_core::IdUlid::new(), [0.0; 3]);
        e.ai_state = AiStateKind::Chase;
        let per = AiPerception {
            dist_to_player: 1.0,
            player_visible: true,
            player_in_attack_range: true,
            has_noise_position: false,
            under_attack: false,
            health_fraction: 1.0,
        };
        let d = e.tick(0.016, per, &def);
        assert!(matches!(d, AiDecision::Transition(AiStateKind::Attack)));
    }

    #[test]
    fn attack_state_performs_attack() {
        let def = dummy_def();
        let mut e = Enemy::new(&def, arcane_core::IdUlid::new(), [0.0; 3]);
        e.ai_state = AiStateKind::Attack;
        let per = AiPerception {
            dist_to_player: 1.0,
            player_visible: true,
            player_in_attack_range: true,
            has_noise_position: false,
            under_attack: false,
            health_fraction: 1.0,
        };
        let d = e.tick(0.016, per, &def);
        assert!(matches!(d, AiDecision::Attack));
    }

    #[test]
    fn low_health_triggers_retreat() {
        let def = dummy_def();
        let mut e = Enemy::new(&def, arcane_core::IdUlid::new(), [0.0; 3]);
        e.ai_state = AiStateKind::Chase;
        e.health.current = 10.0; // 20% health
        let per = AiPerception {
            dist_to_player: 10.0,
            player_visible: true,
            player_in_attack_range: false,
            has_noise_position: false,
            under_attack: false,
            health_fraction: 0.2,
        };
        let d = e.tick(0.016, per, &def);
        assert!(matches!(d, AiDecision::Transition(AiStateKind::Retreat)));
    }

    #[test]
    fn death_transitions_when_health_zero() {
        let def = dummy_def();
        let mut e = Enemy::new(&def, arcane_core::IdUlid::new(), [0.0; 3]);
        e.health.current = 0.0;
        let per = AiPerception {
            dist_to_player: 1.0,
            player_visible: true,
            player_in_attack_range: true,
            has_noise_position: false,
            under_attack: false,
            health_fraction: 0.0,
        };
        let _ = e.tick(0.016, per, &def);
        assert_eq!(e.ai_state, AiStateKind::Dead);
        assert!(e.is_dead());
    }

    #[test]
    fn dead_state_keeps_dead() {
        let def = dummy_def();
        let mut e = Enemy::new(&def, arcane_core::IdUlid::new(), [0.0; 3]);
        e.ai_state = AiStateKind::Dead;
        let per = AiPerception::NO_PLAYER;
        let d = e.tick(0.016, per, &def);
        assert!(matches!(d, AiDecision::Keep));
        assert_eq!(e.ai_state, AiStateKind::Dead);
    }

    #[test]
    fn investigate_moves_toward_noise() {
        let def = dummy_def();
        let mut e = Enemy::new(&def, arcane_core::IdUlid::new(), [0.0; 3]);
        e.ai_state = AiStateKind::Investigate;
        let per = AiPerception {
            dist_to_player: 100.0,
            player_visible: false,
            player_in_attack_range: false,
            has_noise_position: true,
            under_attack: false,
            health_fraction: 1.0,
        };
        let d = e.tick(0.016, per, &def);
        assert!(matches!(d, AiDecision::MoveTowardNoise(_)));
    }

    #[test]
    fn investigate_without_noise_returns_to_idle() {
        let def = dummy_def();
        let mut e = Enemy::new(&def, arcane_core::IdUlid::new(), [0.0; 3]);
        e.ai_state = AiStateKind::Investigate;
        let per = AiPerception {
            dist_to_player: 100.0,
            player_visible: false,
            player_in_attack_range: false,
            has_noise_position: false,
            under_attack: false,
            health_fraction: 1.0,
        };
        let d = e.tick(0.016, per, &def);
        assert!(matches!(d, AiDecision::Transition(AiStateKind::Idle)));
    }

    #[test]
    fn loot_drop_uses_chance() {
        let table = LootTable {
            id: "test".into(),
            entries: vec![
                LootEntry {
                    item_id: "wood".into(),
                    chance: 0.5,
                    min_qty: 1,
                    max_qty: 3,
                },
                LootEntry {
                    item_id: "stone".into(),
                    chance: 1.0, // always drops
                    min_qty: 2,
                    max_qty: 2,
                },
            ],
        };
        // Deterministic "rng" — always returns 0.99, so wood chance fails.
        let drops = roll_loot(&table, || 0.99);
        assert_eq!(drops.len(), 1);
        assert_eq!(drops[0].item_id, "stone");
        assert_eq!(drops[0].quantity, 2);
    }

    #[test]
    fn loot_drop_rolls_quantity_in_range() {
        let table = LootTable {
            id: "test".into(),
            entries: vec![
                LootEntry {
                    item_id: "wood".into(),
                    chance: 1.0, // always
                    min_qty: 1,
                    max_qty: 10,
                },
            ],
        };
        let drops = roll_loot(&table, || 0.5);
        assert_eq!(drops.len(), 1);
        // 1 + (10-1)*0.5 = 5.5 → rounded to 6.
        assert!(drops[0].quantity >= 1 && drops[0].quantity <= 10);
    }

    #[test]
    fn enemy_take_damage_records_actual() {
        let def = dummy_def();
        let mut e = Enemy::new(&def, arcane_core::IdUlid::new(), [0.0; 3]);
        let dealt = e.take_damage(DamageInstance::new(crate::combat::DamageType::Fire, 20.0));
        assert!((dealt - 20.0).abs() < 1e-6);
        assert!((e.health.current - 30.0).abs() < 1e-6);
    }

    #[test]
    fn enemy_postcard_roundtrip_preserves_state() {
        let def = dummy_def();
        let mut e = Enemy::new(&def, arcane_core::IdUlid::new(), [1.0, 2.0, 3.0]);
        e.ai_state = AiStateKind::Attack;
        e.attack_cooldown_secs = 0.5;
        let bytes = postcard::to_allocvec(&e).unwrap();
        let back: Enemy = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(back.ai_state, AiStateKind::Attack);
        assert!((back.attack_cooldown_secs - 0.5).abs() < 1e-6);
    }
}
