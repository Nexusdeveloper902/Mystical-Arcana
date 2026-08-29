//! Spell engine — rune composition, mana cost, cooldown, targeting, effects.
//!
//! Per the design doc:
//!   - Modular spell system. New spells creatable primarily through data.
//!   - Rune composition + mana cost + cooldown + targeting + effects + VFX
//!     + audio + impacts + modifiers + upgrades.
//!   - Improvisation vs. Knowledge: improvised from runes, or known schematic.
//!
//! All spell calculation logic is pure and unit-testable. The visual / audio
//! surface is the responsibility of the `arcane_vfx` / `arcane_audio` crates.

use crate::mana::ManaPool;
use crate::runes::{RuneDef, RunePair, RuneRegistry};
use arcane_core::Id64;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

/// Default base mana cost for any spell, before rune modifiers.
pub const DEFAULT_SPELL_BASE_COST: f32 = 10.0;

/// Default base cooldown in seconds, before rune modifiers.
pub const DEFAULT_SPELL_BASE_COOLDOWN_SECS: f32 = 1.0;

/// A spell targeting mode — where the spell's effect originates and ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum SpellTarget {
    /// Cast from the player at the crosshair (e.g. Fire Bolt).
    SelfToAim = 0,
    /// Cast centered on the player (e.g. Ward).
    SelfCentered = 1,
    /// Cast at the player's feet (e.g. Leap).
    SelfAtFeet = 2,
    /// Cast on a touched world object (e.g. Gather Bolt on a tree).
    TouchedObject = 3,
    /// Cast at a point in the world the player is looking at.
    AimPoint = 4,
}

/// A learned spell recipe. Composed of a primary (verb) + secondary (noun) rune.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchematicSpell {
    /// Stable id — used by save system and progression.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Description.
    pub description: String,
    /// The rune pair that composes this spell.
    pub pair: RunePair,
    /// Base mana cost.
    pub base_mana_cost: f32,
    /// Base cooldown in seconds.
    pub base_cooldown_secs: f32,
    /// Targeting mode.
    pub target: SpellTarget,
    /// Effective power scalar (used by combat/effect dispatch).
    pub power: f32,
    /// Knockback impulse (Newton-seconds).
    pub knockback: f32,
    /// Optional: tag this spell uses to dispatch its effect.
    pub effect_tag: Option<String>,
}

impl SchematicSpell {
    /// Stable ID for this spell.
    pub fn stable_id(&self) -> Id64 {
        Id64::from_str(&self.id)
    }

    /// Computes the effective mana cost given the registry's rune modifiers.
    /// Cost = base * primary.mana_modifier * secondary.mana_modifier.
    pub fn effective_mana_cost(&self, reg: &RuneRegistry) -> f32 {
        let p = reg.get(self.pair.primary).map(|r| r.mana_modifier).unwrap_or(1.0);
        let s = reg.get(self.pair.secondary).map(|r| r.mana_modifier).unwrap_or(1.0);
        self.base_mana_cost * p * s
    }

    /// Computes the effective cooldown in seconds given the registry.
    pub fn effective_cooldown(&self, reg: &RuneRegistry) -> f32 {
        let p = reg.get(self.pair.primary).map(|r| r.cooldown_modifier).unwrap_or(0.0);
        let s = reg.get(self.pair.secondary).map(|r| r.cooldown_modifier).unwrap_or(0.0);
        (self.base_cooldown_secs + p + s).max(0.0)
    }

    /// Computes the effective power given the registry.
    pub fn effective_power(&self, reg: &RuneRegistry) -> f32 {
        let p = reg.get(self.pair.primary).map(|r| r.power_modifier).unwrap_or(1.0);
        let s = reg.get(self.pair.secondary).map(|r| r.power_modifier).unwrap_or(1.0);
        self.power * p * s
    }
}

/// The spell registry — all learned schematics indexed by stable ID.
#[derive(Debug, Default)]
pub struct SpellRegistry {
    spells: HashMap<Id64, SchematicSpell>,
    /// Lookup by rune pair (canonical) → spell id. Lets the player improvise
    /// a known schematic by combining its runes.
    by_pair: HashMap<RunePair, Id64>,
}

impl SpellRegistry {
    /// Empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a spell. Overwrites any existing spell with the same id.
    pub fn register(&mut self, spell: SchematicSpell) {
        let id = spell.stable_id();
        let pair_canon = spell.pair.canonical();
        self.by_pair.insert(pair_canon, id);
        self.spells.insert(id, spell);
    }

    /// Looks up a spell by stable ID.
    pub fn get(&self, id: Id64) -> Option<&SchematicSpell> {
        self.spells.get(&id)
    }

    /// Looks up a spell by string id.
    pub fn get_by_str(&self, s: &str) -> Option<&SchematicSpell> {
        self.spells.get(&Id64::from_str(s))
    }

    /// Looks up a spell by canonical rune pair.
    pub fn get_by_pair(&self, pair: RunePair) -> Option<&SchematicSpell> {
        self.by_pair.get(&pair.canonical()).and_then(|id| self.spells.get(id))
    }

    /// Number of registered spells.
    pub fn len(&self) -> usize {
        self.spells.len()
    }

    /// True if empty.
    pub fn is_empty(&self) -> bool {
        self.spells.is_empty()
    }

    /// Iterates all registered spells.
    pub fn iter(&self) -> impl Iterator<Item = (&Id64, &SchematicSpell)> {
        self.spells.iter()
    }
}

/// Per-spell cooldown tracker. Lives on the player.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct CooldownState {
    /// Per-spell remaining cooldown in seconds, keyed by spell stable ID.
    pub remaining: HashMap<u64, f32>,
}

impl CooldownState {
    /// Advances all cooldowns by `dt` seconds. Removes expired entries.
    pub fn tick(&mut self, dt: f32) {
        self.remaining.retain(|_, v| {
            *v -= dt;
            *v > 0.0
        });
    }

    /// True if the spell is off cooldown.
    pub fn is_ready(&self, spell_id: Id64) -> bool {
        !self.remaining.contains_key(&spell_id.as_u64())
    }

    /// Returns the remaining cooldown in seconds (0 if ready).
    pub fn remaining_secs(&self, spell_id: Id64) -> f32 {
        self.remaining.get(&spell_id.as_u64()).copied().unwrap_or(0.0)
    }

    /// Sets a cooldown for a spell.
    pub fn set(&mut self, spell_id: Id64, secs: f32) {
        if secs > 0.0 {
            self.remaining.insert(spell_id.as_u64(), secs);
        }
    }
}

/// Outcome of attempting to cast a spell.
#[derive(Debug, Clone, PartialEq)]
pub enum CastResult {
    /// Spell was cast. Mana was spent, cooldown set.
    Success {
        /// Mana spent.
        mana_spent: f32,
        /// Cooldown set, in seconds.
        cooldown_secs: f32,
        /// Effective power of the cast.
        power: f32,
    },
    /// Mana insufficient. Player needs to either find more mana or wait.
    InsufficientMana {
        /// Mana currently available.
        current: f32,
        /// Mana required.
        required: f32,
    },
    /// Spell is on cooldown.
    OnCooldown {
        /// Seconds remaining.
        remaining_secs: f32,
    },
    /// Unknown spell id.
    UnknownSpell(Id64),
}

/// Attempts to cast a spell given the player's mana pool and cooldown state.
/// Returns the outcome; mutates state on success.
///
/// `allow_overcast` controls whether to allow casting when mana is insufficient
/// (per design: overcast risks Mana Burn). When allowed, the spell fires
/// even if mana goes negative.
pub fn try_cast(
    spell_id: Id64,
    reg: &SpellRegistry,
    mana: &mut ManaPool,
    cooldown: &mut CooldownState,
    allow_overcast: bool,
) -> CastResult {
    let spell = match reg.get(spell_id) {
        Some(s) => s,
        None => return CastResult::UnknownSpell(spell_id),
    };

    // Check cooldown.
    if !cooldown.is_ready(spell_id) {
        return CastResult::OnCooldown {
            remaining_secs: cooldown.remaining_secs(spell_id),
        };
    }

    // Check / spend mana.
    let cost = spell.effective_mana_cost(&crate::runes::default_registry());
    if !allow_overcast && cost > mana.current_mana {
        return CastResult::InsufficientMana {
            current: mana.current_mana,
            required: cost,
        };
    }
    let overcast_just_happened = allow_overcast && cost > mana.current_mana;
    mana.try_spend(cost, allow_overcast);
    if overcast_just_happened && mana.is_burning() {
        log::warn!("Spell cast triggered Mana Burn");
    }

    // Set cooldown.
    let cd = spell.effective_cooldown(&crate::runes::default_registry());
    cooldown.set(spell_id, cd);

    // Effective power.
    let power = spell.effective_power(&crate::runes::default_registry());

    CastResult::Success {
        mana_spent: cost,
        cooldown_secs: cd,
        power,
    }
}

/// Builds the default spell registry: spells the player can learn from the
/// starter runes. These are intentionally discoverable through rune
/// combinations, not pre-granted.
pub fn default_spells() -> Vec<SchematicSpell> {
    use SpellTarget::*;
    vec![
        SchematicSpell {
            id: "fire_bolt".into(),
            name: "Fire Bolt".into(),
            description: "A piercing ray of fire.".into(),
            pair: RunePair::new(
                Id64::from_str("fire"),
                Id64::from_str("pierce"),
            ),
            base_mana_cost: 15.0,
            base_cooldown_secs: 1.0,
            target: SelfToAim,
            power: 25.0,
            knockback: 5.0,
            effect_tag: Some("fire_bolt".into()),
        },
        SchematicSpell {
            id: "gather_bolt".into(),
            name: "Gather Bolt".into(),
            description: "Pulls loose resources from a target node.".into(),
            pair: RunePair::new(
                Id64::from_str("gather"),
                Id64::from_str("pierce"),
            ),
            base_mana_cost: 5.0,
            base_cooldown_secs: 0.5,
            target: TouchedObject,
            power: 1.0,
            knockback: 0.0,
            effect_tag: Some("gather_bolt".into()),
        },
        SchematicSpell {
            id: "ice_ward".into(),
            name: "Ice Ward".into(),
            description: "A defensive crystalline barrier.".into(),
            pair: RunePair::new(
                Id64::from_str("ice"),
                Id64::from_str("ward"),
            ),
            base_mana_cost: 20.0,
            base_cooldown_secs: 4.0,
            target: SelfCentered,
            power: 50.0,
            knockback: 0.0,
            effect_tag: Some("ice_ward".into()),
        },
        SchematicSpell {
            id: "leap".into(),
            name: "Leap".into(),
            description: "Displaces the caster over a short distance.".into(),
            pair: RunePair::new(
                Id64::from_str("leap"),
                Id64::from_str("flow"),
            ),
            base_mana_cost: 12.0,
            base_cooldown_secs: 3.0,
            target: SelfAtFeet,
            power: 8.0,
            knockback: 0.0,
            effect_tag: Some("leap".into()),
        },
        SchematicSpell {
            id: "bind_field".into(),
            name: "Bind Field".into(),
            description: "Holds matter in a small region, slowing creatures.".into(),
            pair: RunePair::new(
                Id64::from_str("bind"),
                Id64::from_str("flow"),
            ),
            base_mana_cost: 18.0,
            base_cooldown_secs: 5.0,
            target: AimPoint,
            power: 30.0,
            knockback: 0.0,
            effect_tag: Some("bind_field".into()),
        },
        SchematicSpell {
            id: "shatter_burst".into(),
            name: "Shatter Burst".into(),
            description: "A short-range destruction burst that severs matter.".into(),
            pair: RunePair::new(
                Id64::from_str("shatter"),
                Id64::from_str("pierce"),
            ),
            base_mana_cost: 35.0,
            base_cooldown_secs: 6.0,
            target: SelfToAim,
            power: 60.0,
            knockback: 15.0,
            effect_tag: Some("shatter_burst".into()),
        },
        SchematicSpell {
            id: "mana_resonance".into(),
            name: "Mana Resonance".into(),
            description: "Tunes the caster to nearby mana, boosting regen.".into(),
            pair: RunePair::new(
                Id64::from_str("resonance"),
                Id64::from_str("flow"),
            ),
            base_mana_cost: 8.0,
            base_cooldown_secs: 10.0,
            target: SelfCentered,
            power: 0.0,
            knockback: 0.0,
            effect_tag: Some("mana_resonance".into()),
        },
    ]
}

/// Convenience: builds a default spell registry with all starter spells.
pub fn default_spell_registry() -> SpellRegistry {
    let mut reg = SpellRegistry::new();
    for s in default_spells() {
        reg.register(s);
    }
    reg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_spell_registry_has_starter_spells() {
        let reg = default_spell_registry();
        assert!(reg.len() >= 5);
        assert!(reg.get_by_str("fire_bolt").is_some());
        assert!(reg.get_by_str("gather_bolt").is_some());
        assert!(reg.get_by_str("leap").is_some());
    }

    #[test]
    fn effective_mana_cost_uses_rune_modifiers() {
        let rune_reg = crate::runes::default_registry();
        let spell_reg = default_spell_registry();
        let fire_bolt = spell_reg.get_by_str("fire_bolt").unwrap();
        // fire mana_mod=1.0, pierce mana_mod=1.2 → cost = 15 * 1.0 * 1.2 = 18
        let cost = fire_bolt.effective_mana_cost(&rune_reg);
        assert!((cost - 18.0).abs() < 1e-4, "got {}", cost);
    }

    #[test]
    fn effective_cooldown_uses_rune_modifiers() {
        let rune_reg = crate::runes::default_registry();
        let spell_reg = default_spell_registry();
        let fire_bolt = spell_reg.get_by_str("fire_bolt").unwrap();
        // fire cd_mod=0.0, pierce cd_mod=0.5 → cooldown = 1.0 + 0.0 + 0.5 = 1.5
        let cd = fire_bolt.effective_cooldown(&rune_reg);
        assert!((cd - 1.5).abs() < 1e-4, "got {}", cd);
    }

    #[test]
    fn effective_power_uses_rune_modifiers() {
        let rune_reg = crate::runes::default_registry();
        let spell_reg = default_spell_registry();
        let fire_bolt = spell_reg.get_by_str("fire_bolt").unwrap();
        // fire power_mod=1.0, pierce power_mod=1.5 → power = 25 * 1.0 * 1.5 = 37.5
        let p = fire_bolt.effective_power(&rune_reg);
        assert!((p - 37.5).abs() < 1e-4, "got {}", p);
    }

    #[test]
    fn spell_lookup_by_rune_pair_finds_spell() {
        let spell_reg = default_spell_registry();
        let rune_reg = crate::runes::default_registry();
        let fire = rune_reg.get_by_str("fire").unwrap().stable_id();
        let pierce = rune_reg.get_by_str("pierce").unwrap().stable_id();
        let pair = RunePair::new(fire, pierce);
        let spell = spell_reg.get_by_pair(pair);
        assert!(spell.is_some(), "should find fire_bolt by fire+pierce pair");
        assert_eq!(spell.unwrap().id, "fire_bolt");
    }

    #[test]
    fn spell_lookup_by_pair_is_order_independent() {
        let spell_reg = default_spell_registry();
        let rune_reg = crate::runes::default_registry();
        let fire = rune_reg.get_by_str("fire").unwrap().stable_id();
        let pierce = rune_reg.get_by_str("pierce").unwrap().stable_id();
        let p1 = RunePair::new(fire, pierce);
        let p2 = RunePair::new(pierce, fire);
        assert_eq!(
            spell_reg.get_by_pair(p1).map(|s| s.id.as_str()),
            spell_reg.get_by_pair(p2).map(|s| s.id.as_str()),
        );
    }

    #[test]
    fn cooldown_state_blocks_spell_until_expires() {
        let mut cd = CooldownState::default();
        let id = Id64::from_str("test");
        assert!(cd.is_ready(id));
        cd.set(id, 2.0);
        assert!(!cd.is_ready(id));
        assert!((cd.remaining_secs(id) - 2.0).abs() < 1e-6);
        cd.tick(1.0);
        assert!(!cd.is_ready(id));
        assert!((cd.remaining_secs(id) - 1.0).abs() < 1e-6);
        cd.tick(1.0);
        assert!(cd.is_ready(id), "spell should be ready after cooldown expires");
    }

    #[test]
    fn try_cast_succeeds_when_mana_available() {
        let rune_reg = crate::runes::default_registry();
        let spell_reg = default_spell_registry();
        let mut mana = ManaPool::new(100.0, 0.0);
        let mut cd = CooldownState::default();
        let id = spell_reg.get_by_str("fire_bolt").unwrap().stable_id();

        let result = try_cast(id, &spell_reg, &mut mana, &mut cd, false);
        assert!(matches!(result, CastResult::Success { .. }));
        let cost = spell_reg.get_by_str("fire_bolt").unwrap().effective_mana_cost(&rune_reg);
        assert!((mana.current_mana - (100.0 - cost)).abs() < 1e-4);
        assert!(!cd.is_ready(id));
    }

    #[test]
    fn try_cast_blocks_when_on_cooldown() {
        let spell_reg = default_spell_registry();
        let mut mana = ManaPool::new(100.0, 0.0);
        let mut cd = CooldownState::default();
        let id = spell_reg.get_by_str("fire_bolt").unwrap().stable_id();

        let _ = try_cast(id, &spell_reg, &mut mana, &mut cd, false);
        let result = try_cast(id, &spell_reg, &mut mana, &mut cd, false);
        assert!(matches!(result, CastResult::OnCooldown { .. }));
    }

    #[test]
    fn try_cast_insufficient_mana_blocks_without_overcast() {
        let spell_reg = default_spell_registry();
        let mut mana = ManaPool::new(10.0, 0.0);  // low mana
        let mut cd = CooldownState::default();
        let id = spell_reg.get_by_str("fire_bolt").unwrap().stable_id();
        // fire_bolt cost = 18, mana = 10, no overcast → blocked.
        let result = try_cast(id, &spell_reg, &mut mana, &mut cd, false);
        match result {
            CastResult::InsufficientMana { current, required } => {
                assert!((current - 10.0).abs() < 1e-6);
                assert!((required - 18.0).abs() < 1e-4);
            }
            _ => panic!("expected InsufficientMana, got {:?}", result),
        }
    }

    #[test]
    fn try_cast_overcast_allows_casting_with_little_mana() {
        let spell_reg = default_spell_registry();
        let mut mana = ManaPool::new(10.0, 0.0);
        let mut cd = CooldownState::default();
        let id = spell_reg.get_by_str("fire_bolt").unwrap().stable_id();
        let result = try_cast(id, &spell_reg, &mut mana, &mut cd, true);
        assert!(matches!(result, CastResult::Success { .. }));
        // Mana should be negative.
        assert!(mana.current_mana < 0.0);
    }

    #[test]
    fn try_cast_unknown_spell_returns_error() {
        let spell_reg = default_spell_registry();
        let mut mana = ManaPool::new(100.0, 0.0);
        let mut cd = CooldownState::default();
        let result = try_cast(Id64::from_str("nonexistent"), &spell_reg, &mut mana, &mut cd, false);
        assert!(matches!(result, CastResult::UnknownSpell(_)));
    }

    #[test]
    fn gather_bolt_targets_touched_object() {
        let reg = default_spell_registry();
        let s = reg.get_by_str("gather_bolt").unwrap();
        assert_eq!(s.target, SpellTarget::TouchedObject);
    }

    #[test]
    fn spell_postcard_roundtrip() {
        let s = default_spells().into_iter().find(|s| s.id == "leap").unwrap();
        let bytes = postcard::to_allocvec(&s).unwrap();
        let back: SchematicSpell = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(back.id, "leap");
        assert_eq!(back.pair, s.pair);
    }

    #[test]
    fn spell_ron_roundtrip() {
        let s = default_spells().into_iter().find(|s| s.id == "ice_ward").unwrap();
        let ron_str = ron::to_string(&s).unwrap();
        let back: SchematicSpell = ron::from_str(&ron_str).unwrap();
        assert_eq!(back.id, "ice_ward");
        assert_eq!(back.target, SpellTarget::SelfCentered);
    }

    #[test]
    fn cooldown_tick_removes_expired_entries() {
        let mut cd = CooldownState::default();
        cd.set(Id64::from_str("a"), 1.0);
        cd.set(Id64::from_str("b"), 5.0);
        cd.tick(2.0);  // a expires (1 < 2), b remains (5-2=3)
        assert!(cd.is_ready(Id64::from_str("a")));
        assert!(!cd.is_ready(Id64::from_str("b")));
        assert!((cd.remaining_secs(Id64::from_str("b")) - 3.0).abs() < 1e-6);
    }
}
