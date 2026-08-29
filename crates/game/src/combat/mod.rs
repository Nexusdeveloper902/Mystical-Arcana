//! Combat: damage, health, knockback, status effects, death.
//!
//! Per the design doc:
//!   "Combat should feel like magical manipulation rather than conventional
//!    firearms with fantasy artwork."
//!
//! Damage is typed — different damage types interact with different defenses
//! and produce different VFX. Status effects (burn, freeze, bind) are
//! time-limited conditions applied to a target.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Magical damage type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum DamageType {
    /// Pure kinetic force — kinetic bolts, knockback, falls.
    Kinetic = 0,
    /// Heat / fire — applied Burn status.
    Fire = 1,
    /// Cold / ice — applies Freeze status (movement slow).
    Ice = 2,
    /// Arcane / pure mana — bypasses most defenses.
    Arcane = 3,
    /// Severing / structural — strong vs crystalline enemies.
    Severing = 4,
    /// Corruption damage — over time, mutates enemies.
    Corruption = 5,
}

/// A damage instance applied to a target.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct DamageInstance {
    /// Damage type.
    pub kind: DamageType,
    /// Raw damage amount (before mitigation).
    pub amount: f32,
    /// Knockback impulse in Newton-seconds.
    pub knockback: f32,
    /// Source position (for knockback direction). Optional.
    pub source_pos: Option<[f32; 3]>,
}

impl DamageInstance {
    /// Constructs a simple damage instance with no knockback.
    pub fn new(kind: DamageType, amount: f32) -> Self {
        Self { kind, amount, knockback: 0.0, source_pos: None }
    }

    /// Adds knockback from a source position.
    pub fn with_knockback(mut self, source: [f32; 3], impulse: f32) -> Self {
        self.source_pos = Some(source);
        self.knockback = impulse;
        self
    }
}

/// A defense/resistance profile for a target.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
pub struct Defenses {
    /// Fraction of kinetic damage reduced (0.0..1.0).
    pub kinetic_resist: f32,
    /// Fraction of fire damage reduced.
    pub fire_resist: f32,
    /// Fraction of ice damage reduced.
    pub ice_resist: f32,
    /// Fraction of arcane damage reduced.
    pub arcane_resist: f32,
    /// Fraction of severing damage reduced.
    pub severing_resist: f32,
    /// Fraction of corruption damage reduced.
    pub corruption_resist: f32,
}

impl Defenses {
    /// All-zero defenses (full damage taken).
    pub const BARE: Self = Self {
        kinetic_resist: 0.0,
        fire_resist: 0.0,
        ice_resist: 0.0,
        arcane_resist: 0.0,
        severing_resist: 0.0,
        corruption_resist: 0.0,
    };

    /// Returns the resistance fraction for a damage type.
    pub fn for_type(self, t: DamageType) -> f32 {
        match t {
            DamageType::Kinetic => self.kinetic_resist,
            DamageType::Fire => self.fire_resist,
            DamageType::Ice => self.ice_resist,
            DamageType::Arcane => self.arcane_resist,
            DamageType::Severing => self.severing_resist,
            DamageType::Corruption => self.corruption_resist,
        }
    }

    /// Applies the damage instance to this defenses profile, returning the
    /// effective (post-mitigation) damage amount.
    pub fn mitigate(self, dmg: DamageInstance) -> f32 {
        let resist = self.for_type(dmg.kind).clamp(0.0, 0.95);
        dmg.amount * (1.0 - resist)
    }
}

/// A status effect applied to a target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum StatusKind {
    /// Burning — takes fire damage over time.
    Burn = 0,
    /// Frozen — movement slowed.
    Freeze = 1,
    /// Bound — cannot move.
    Bind = 2,
    /// Mana drain — mana pool drains over time.
    Drain = 3,
    /// Stunned — cannot act.
    Stun = 4,
    /// Corrupted — taking corruption damage over time, may mutate.
    Corrupt = 5,
}

/// An active status effect instance.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct StatusEffect {
    /// Kind.
    pub kind: StatusKind,
    /// Remaining duration in seconds.
    pub remaining_secs: f32,
    /// Magnitude (per-stack strength, type-dependent).
    pub magnitude: f32,
}

impl StatusEffect {
    /// Constructs a new status effect.
    pub fn new(kind: StatusKind, duration_secs: f32, magnitude: f32) -> Self {
        Self { kind, remaining_secs: duration_secs, magnitude }
    }

    /// Advances the effect by `dt` seconds. Returns true if still active.
    pub fn tick(&mut self, dt: f32) -> bool {
        self.remaining_secs -= dt;
        self.remaining_secs > 0.0
    }
}

/// Health pool for any damageable entity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Health {
    /// Maximum health.
    pub max: f32,
    /// Current health.
    pub current: f32,
    /// Defenses applied to incoming damage.
    pub defenses: Defenses,
    /// Active status effects.
    pub statuses: Vec<StatusEffect>,
    /// True if dead (current <= 0 and death has been finalized).
    pub dead: bool,
}

impl Health {
    /// Constructs a new health pool with `max` hp and bare defenses.
    pub fn new(max: f32) -> Self {
        Self {
            max,
            current: max,
            defenses: Defenses::BARE,
            statuses: Vec::new(),
            dead: false,
        }
    }

    /// Constructs with explicit defenses.
    pub fn with_defenses(max: f32, defenses: Defenses) -> Self {
        Self {
            max,
            current: max,
            defenses,
            statuses: Vec::new(),
            dead: false,
        }
    }

    /// Applies a damage instance. Returns the actual damage subtracted
    /// from health (clamped so we never report more damage than health
    /// remaining). Useful for VFX/UI feedback ("you dealt N damage").
    pub fn apply_damage(&mut self, dmg: DamageInstance) -> f32 {
        if self.dead {
            return 0.0;
        }
        let mitigated = self.defenses.mitigate(dmg);
        let dealt = mitigated.min(self.current);
        self.current = (self.current - mitigated).max(0.0);
        if self.current <= 0.0 {
            self.dead = true;
        }
        dealt
    }

    /// Heals by `amount`, clamped to max.
    pub fn heal(&mut self, amount: f32) {
        if self.dead {
            return;
        }
        self.current = (self.current + amount).min(self.max);
    }

    /// Applies a status effect (stacking extends duration if same kind).
    pub fn apply_status(&mut self, effect: StatusEffect) {
        if let Some(existing) = self.statuses.iter_mut().find(|s| s.kind == effect.kind) {
            // Refresh duration, take the larger magnitude.
            existing.remaining_secs = existing.remaining_secs.max(effect.remaining_secs);
            existing.magnitude = existing.magnitude.max(effect.magnitude);
        } else {
            self.statuses.push(effect);
        }
    }

    /// Advances all status effects by `dt` seconds. Returns total damage-over-time
    /// (per type) accumulated this tick.
    pub fn tick_statuses(&mut self, dt: f32) -> HashMap<DamageType, f32> {
        let mut dots = HashMap::new();
        self.statuses.retain_mut(|s| {
            let alive = s.tick(dt);
            if alive {
                match s.kind {
                    StatusKind::Burn => {
                        let dmg = s.magnitude * dt;
                        *dots.entry(DamageType::Fire).or_insert(0.0) += dmg;
                    }
                    StatusKind::Corrupt => {
                        let dmg = s.magnitude * dt;
                        *dots.entry(DamageType::Corruption).or_insert(0.0) += dmg;
                    }
                    StatusKind::Drain => {
                        let dmg = s.magnitude * dt;
                        *dots.entry(DamageType::Arcane).or_insert(0.0) += dmg;
                    }
                    _ => {}
                }
            }
            alive
        });
        // Apply accumulated DoTs.
        for (kind, amount) in &dots {
            self.apply_damage(DamageInstance::new(*kind, *amount));
        }
        dots
    }

    /// True if the entity has the given status.
    pub fn has_status(&self, kind: StatusKind) -> bool {
        self.statuses.iter().any(|s| s.kind == kind)
    }

    /// True if dead. Checks both the explicit `dead` flag and `current <= 0`
    /// so that direct mutations to `current` (e.g. from DoT or save-load)
    /// still report death correctly.
    pub fn is_dead(&self) -> bool {
        self.dead || self.current <= 0.0
    }

    /// Fraction of health remaining (0.0..1.0).
    pub fn health_fraction(&self) -> f32 {
        if self.max <= 0.0 { 0.0 } else { self.current / self.max }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn damage_mitigation_uses_resistance() {
        let d = Defenses { kinetic_resist: 0.5, ..Defenses::BARE };
        let dmg = DamageInstance::new(DamageType::Kinetic, 100.0);
        let actual = d.mitigate(dmg);
        assert!((actual - 50.0).abs() < 1e-6);
    }

    #[test]
    fn resistance_clamped_to_95_percent() {
        let d = Defenses { fire_resist: 1.5, ..Defenses::BARE };
        let dmg = DamageInstance::new(DamageType::Fire, 100.0);
        let actual = d.mitigate(dmg);
        assert!((actual - 5.0).abs() < 1e-6, "95% cap on resist, got {}", actual);
    }

    #[test]
    fn health_take_damage_and_die() {
        let mut h = Health::new(100.0);
        let dealt = h.apply_damage(DamageInstance::new(DamageType::Kinetic, 30.0));
        assert!((dealt - 30.0).abs() < 1e-6);
        assert!((h.current - 70.0).abs() < 1e-6);
        assert!(!h.is_dead());

        let dealt = h.apply_damage(DamageInstance::new(DamageType::Kinetic, 80.0));
        assert!((dealt - 70.0).abs() < 1e-6, "only actual damage dealt counts");
        assert_eq!(h.current, 0.0);
        assert!(h.is_dead());
    }

    #[test]
    fn dead_target_takes_no_more_damage() {
        let mut h = Health::new(10.0);
        h.apply_damage(DamageInstance::new(DamageType::Kinetic, 20.0));
        let dealt = h.apply_damage(DamageInstance::new(DamageType::Kinetic, 50.0));
        assert_eq!(dealt, 0.0, "no damage after death");
    }

    #[test]
    fn heal_clamps_to_max() {
        let mut h = Health::new(100.0);
        h.apply_damage(DamageInstance::new(DamageType::Kinetic, 30.0));
        h.heal(50.0);
        assert!((h.current - 100.0).abs() < 1e-6);
    }

    #[test]
    fn heal_does_not_revive_dead() {
        let mut h = Health::new(10.0);
        h.apply_damage(DamageInstance::new(DamageType::Kinetic, 20.0));
        assert!(h.is_dead());
        h.heal(100.0);
        assert!(h.is_dead(), "dead stays dead without explicit revive");
    }

    #[test]
    fn status_effect_burn_deals_dot() {
        let mut h = Health::new(100.0);
        h.apply_status(StatusEffect::new(StatusKind::Burn, 5.0, 10.0));
        let dots = h.tick_statuses(1.0);
        let fire_dot = *dots.get(&DamageType::Fire).unwrap();
        assert!((fire_dot - 10.0).abs() < 1e-6);
        assert!((h.current - 90.0).abs() < 1e-6, "burn should reduce health");
    }

    #[test]
    fn status_effect_expires() {
        let mut h = Health::new(100.0);
        h.apply_status(StatusEffect::new(StatusKind::Burn, 2.0, 10.0));
        for _ in 0..3 {
            h.tick_statuses(1.0);
        }
        assert!(!h.has_status(StatusKind::Burn), "burn should expire");
    }

    #[test]
    fn status_refresh_extends_duration() {
        let mut h = Health::new(100.0);
        h.apply_status(StatusEffect::new(StatusKind::Burn, 2.0, 5.0));
        h.apply_status(StatusEffect::new(StatusKind::Burn, 1.0, 8.0));
        // Should now have duration 2.0 (max of 2, 1), magnitude 8.0 (max of 5, 8).
        let s = h.statuses.iter().find(|s| s.kind == StatusKind::Burn).unwrap();
        assert!((s.remaining_secs - 2.0).abs() < 1e-6);
        assert!((s.magnitude - 8.0).abs() < 1e-6);
    }

    #[test]
    fn defense_resist_per_damage_type() {
        let d = Defenses {
            kinetic_resist: 0.1,
            fire_resist: 0.5,
            ice_resist: 0.2,
            arcane_resist: 0.0,
            severing_resist: 0.3,
            corruption_resist: 0.9,
        };
        for (kind, expected) in [
            (DamageType::Kinetic, 0.1),
            (DamageType::Fire, 0.5),
            (DamageType::Ice, 0.2),
            (DamageType::Arcane, 0.0),
            (DamageType::Severing, 0.3),
            (DamageType::Corruption, 0.9),
        ] {
            assert!((d.for_type(kind) - expected).abs() < 1e-6);
        }
    }

    #[test]
    fn health_fraction_reports_correct_ratio() {
        let mut h = Health::new(100.0);
        assert!((h.health_fraction() - 1.0).abs() < 1e-6);
        h.apply_damage(DamageInstance::new(DamageType::Kinetic, 30.0));
        assert!((h.health_fraction() - 0.7).abs() < 1e-6);
    }

    #[test]
    fn damage_instance_postcard_roundtrip() {
        let d = DamageInstance::new(DamageType::Fire, 25.0)
            .with_knockback([1.0, 2.0, 3.0], 5.0);
        let bytes = postcard::to_allocvec(&d).unwrap();
        let back: DamageInstance = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(d, back);
    }

    #[test]
    fn defenses_postcard_roundtrip() {
        let d = Defenses {
            kinetic_resist: 0.1,
            fire_resist: 0.2,
            ice_resist: 0.3,
            arcane_resist: 0.4,
            severing_resist: 0.5,
            corruption_resist: 0.6,
        };
        let bytes = postcard::to_allocvec(&d).unwrap();
        let back: Defenses = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(d, back);
    }
}
