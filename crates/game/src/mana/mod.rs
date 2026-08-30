//! Mana pool, regeneration, modifiers, Mana Burn.
//!
//! Per the design doc:
//!   "Cast too little: you're weak. Cast too much: you risk Mana Burn."
//!
//! Mana Burn state:
//!   - Reduces regeneration.
//!   - Visually affects the player.
//!   - Destabilizes the environment.
//!   - Eventually triggers magical storms.
//!
//! All mana state is pure-logic and fully unit-testable headless. The visual
//! and audio surface is feature-gated behind the renderer/audio crates.

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Default maximum mana the player starts with. Modifiable by progression.
pub const DEFAULT_MAX_MANA: f32 = 100.0;

/// Default mana regeneration per second (when not under Mana Burn).
pub const DEFAULT_REGEN_PER_SEC: f32 = 5.0;

/// Default burn threshold — casting spells that push current mana negative
/// by more than this value triggers Mana Burn.
pub const DEFAULT_BURN_THRESHOLD: f32 = 10.0;

/// How long Mana Burn lasts by default, in seconds.
pub const DEFAULT_BURN_DURATION_SECS: f32 = 8.0;

/// Multiplier applied to regeneration while in Mana Burn (should be < 1.0).
pub const DEFAULT_BURN_REGEN_MULTIPLIER: f32 = 0.25;

/// A single transient modifier on mana regen, e.g. "near Mana Node: +3/s".
/// Multiple modifiers stack additively.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct ManaRegenModifier {
    /// Stable identifier for the source (so duplicates can be deduped).
    pub source: arcane_core::Id64,
    /// Additive regen change per second (may be negative for drain).
    pub delta_per_sec: f32,
    /// Optional duration. None = permanent until removed.
    pub duration_secs: Option<f32>,
    /// Elapsed time since the modifier was applied.
    pub elapsed_secs: f32,
}

impl ManaRegenModifier {
    /// Returns the current delta (or 0 if expired).
    pub fn current_delta(&self) -> f32 {
        match self.duration_secs {
            Some(d) if self.elapsed_secs < d => self.delta_per_sec,
            None => self.delta_per_sec,
            _ => 0.0,
        }
    }

    /// Advances the modifier by `dt` seconds. Returns true if still active.
    pub fn tick(&mut self, dt: f32) -> bool {
        self.elapsed_secs += dt;
        match self.duration_secs {
            Some(d) => self.elapsed_secs < d,
            None => true,
        }
    }
}

/// The player's full mana state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManaPool {
    /// Maximum mana capacity (without modifiers; modifiers adjust max too).
    pub max_mana: f32,
    /// Current mana. May be negative during the Burn transient.
    pub current_mana: f32,
    /// Base regeneration per second.
    pub base_regen_per_sec: f32,
    /// Burn threshold: if mana is depleted past `-burn_threshold`, Burn starts.
    pub burn_threshold: f32,
    /// How long Mana Burn lasts once triggered.
    pub burn_duration_secs: f32,
    /// Regen multiplier applied during Burn.
    pub burn_regen_multiplier: f32,
    /// Active regen modifiers.
    pub modifiers: Vec<ManaRegenModifier>,
    /// Remaining Burn time, 0.0 if not burning.
    pub burn_remaining: f32,
    /// Total time spent in Burn across this save (for stats / progression).
    pub total_burn_time_secs: f32,
    /// Total mana spent across this save (for stats / progression).
    pub total_mana_spent: f32,
}

impl Default for ManaPool {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_MANA, DEFAULT_REGEN_PER_SEC)
    }
}

impl ManaPool {
    /// Creates a new mana pool with the given max and base regen.
    pub fn new(max_mana: f32, base_regen_per_sec: f32) -> Self {
        Self {
            max_mana,
            current_mana: max_mana,
            base_regen_per_sec,
            burn_threshold: DEFAULT_BURN_THRESHOLD,
            burn_duration_secs: DEFAULT_BURN_DURATION_SECS,
            burn_regen_multiplier: DEFAULT_BURN_REGEN_MULTIPLIER,
            modifiers: Vec::new(),
            burn_remaining: 0.0,
            total_burn_time_secs: 0.0,
            total_mana_spent: 0.0,
        }
    }

    /// Adds a modifier to the pool.
    pub fn add_modifier(&mut self, m: ManaRegenModifier) {
        self.modifiers.push(m);
    }

    /// Removes all modifiers from the given source.
    pub fn remove_modifiers_from(&mut self, source: arcane_core::Id64) {
        self.modifiers.retain(|m| m.source != source);
    }

    /// True if currently in Mana Burn.
    pub fn is_burning(&self) -> bool {
        self.burn_remaining > 0.0
    }

    /// Current effective regen per second (including modifiers and burn
    /// multiplier).
    pub fn effective_regen_per_sec(&self) -> f32 {
        let base = if self.is_burning() {
            self.base_regen_per_sec * self.burn_regen_multiplier
        } else {
            self.base_regen_per_sec
        };
        let mods: f32 = self.modifiers.iter().map(|m| m.current_delta()).sum();
        base + mods
    }

    /// Advances the mana pool by `dt` seconds.
    pub fn tick(&mut self, dt: f32) {
        // 1. Calculate regen using current (pre-tick) modifier state.
        // This ensures a modifier that expires this tick still contributed
        // to this frame's regen.
        let regen = self.effective_regen_per_sec();
        self.current_mana = (self.current_mana + regen * dt).clamp(0.0, self.max_mana);

        // 2. Tick modifiers' elapsed time, then remove expired ones.
        for m in &mut self.modifiers {
            m.tick(dt);
        }
        self.modifiers.retain(|m| match m.duration_secs {
            Some(d) => m.elapsed_secs < d,
            None => true,
        });

        // 3. Tick burn.
        if self.burn_remaining > 0.0 {
            self.burn_remaining = (self.burn_remaining - dt).max(0.0);
            self.total_burn_time_secs += dt;
        }
    }

    /// Attempts to spend `amount` mana. Returns true if successful.
    ///
    /// If `allow_overcast` is true and the player has less mana than `amount`,
    /// the spell still casts, but `current_mana` may go negative and Mana Burn
    /// may trigger.
    pub fn try_spend(&mut self, amount: f32, allow_overcast: bool) -> bool {
        if !allow_overcast && amount > self.current_mana {
            return false;
        }
        self.current_mana -= amount;
        self.total_mana_spent += amount;
        if self.current_mana <= -self.burn_threshold {
            self.trigger_burn();
        }
        true
    }

    /// Forcibly restores `amount` mana (clamped to max). Does not affect burn.
    pub fn restore(&mut self, amount: f32) {
        self.current_mana = (self.current_mana + amount).clamp(0.0, self.max_mana);
    }

    /// Triggers Mana Burn now. Resets burn_remaining to the full duration.
    pub fn trigger_burn(&mut self) {
        self.burn_remaining = self.burn_duration_secs;
    }

    /// Clears Mana Burn immediately (e.g. sanctuary effect).
    pub fn clear_burn(&mut self) {
        self.burn_remaining = 0.0;
    }

    /// Sets the maximum mana (clamping current as needed).
    pub fn set_max_mana(&mut self, max: f32) {
        self.max_mana = max.max(0.0);
        if self.current_mana > self.max_mana {
            self.current_mana = self.max_mana;
        }
    }

    /// Proximity to Mana Burn: 0.0 = safe (mana near full), 1.0 = burning.
    /// Useful for visual feedback (camera desaturation, screen tint, etc.).
    pub fn burn_proximity(&self) -> f32 {
        if self.is_burning() {
            return 1.0;
        }
        // Negative mana indicates approaching burn.
        if self.current_mana < 0.0 {
            let ratio = (-self.current_mana) / self.burn_threshold;
            ratio.clamp(0.0, 0.9)
        } else {
            0.0
        }
    }
}

/// A Mana Node — a physical conduit in the world. Proximity boosts regen.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct ManaNode {
    /// Stable ID for this node (used for save persistence).
    pub id: arcane_core::IdUlid,
    /// Position in world space.
    pub position: [f32; 3],
    /// Bonus regen granted per second when player is within `radius`.
    pub bonus_regen_per_sec: f32,
    /// Influence radius in meters.
    pub radius: f32,
    /// Whether the node has been "attuned" by the player (unlocks full bonus).
    pub attuned: bool,
}

impl ManaNode {
    /// Creates a new Mana Node.
    pub fn new(id: arcane_core::IdUlid, position: [f32; 3], bonus: f32, radius: f32) -> Self {
        Self { id, position, bonus_regen_per_sec: bonus, radius, attuned: false }
    }

    /// True if `player_pos` is inside this node's influence radius.
    pub fn is_player_in_radius(&self, player_pos: [f32; 3]) -> bool {
        let dx = player_pos[0] - self.position[0];
        let dy = player_pos[1] - self.position[1];
        let dz = player_pos[2] - self.position[2];
        let dist_sq = dx * dx + dy * dy + dz * dz;
        dist_sq <= self.radius * self.radius
    }

    /// Returns the regen bonus for a player at `player_pos`. Scales linearly
    /// from `bonus_regen_per_sec` at the center to 0 at the radius boundary.
    /// If not attuned, bonus is halved.
    pub fn current_bonus(&self, player_pos: [f32; 3]) -> f32 {
        if !self.is_player_in_radius(player_pos) {
            return 0.0;
        }
        let dx = player_pos[0] - self.position[0];
        let dy = player_pos[1] - self.position[1];
        let dz = player_pos[2] - self.position[2];
        let dist = (dx * dx + dy * dy + dz * dz).sqrt();
        let falloff = (1.0 - dist / self.radius).max(0.0);
        let bonus = self.bonus_regen_per_sec * falloff;
        if self.attuned { bonus } else { bonus * 0.5 }
    }
}

/// Aggregates the regen contribution from a list of nearby Mana Nodes.
pub fn total_mana_node_bonus(nodes: &[ManaNode], player_pos: [f32; 3]) -> f32 {
    nodes.iter().map(|n| n.current_bonus(player_pos)).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mana_pool_default_starts_full() {
        let p = ManaPool::default();
        assert!((p.current_mana - p.max_mana).abs() < 1e-6);
        assert!(!p.is_burning());
        assert_eq!(p.burn_proximity(), 0.0);
    }

    #[test]
    fn mana_regen_fills_over_time() {
        let mut p = ManaPool::new(100.0, 10.0);
        p.current_mana = 50.0;
        p.tick(1.0);
        assert!((p.current_mana - 60.0).abs() < 1e-6, "got {}", p.current_mana);
        p.tick(5.0);
        assert!((p.current_mana - 100.0).abs() < 1e-6, "mana should be capped at max");
    }

    #[test]
    fn mana_spend_basic() {
        let mut p = ManaPool::new(100.0, 0.0);
        assert!(p.try_spend(30.0, false));
        assert!((p.current_mana - 70.0).abs() < 1e-6);
        // Spending more than current without overcast fails.
        assert!(!p.try_spend(100.0, false));
        // State unchanged.
        assert!((p.current_mana - 70.0).abs() < 1e-6);
    }

    #[test]
    fn mana_overcast_triggers_burn() {
        let mut p = ManaPool::new(20.0, 0.0);
        // Spending 30 with overcast drops current to -10, exceeds burn_threshold
        // (10 by default), should trigger burn.
        assert!(p.try_spend(30.0, true));
        assert!(p.is_burning(), "should be burning after overcast past threshold");
        assert!(p.burn_remaining > 0.0);
    }

    #[test]
    fn mana_overcast_below_threshold_does_not_burn() {
        let mut p = ManaPool::new(20.0, 0.0);
        p.burn_threshold = 5.0;
        // Spending 23 with overcast: current = -3, which is within threshold.
        assert!(p.try_spend(23.0, true));
        assert!(!p.is_burning(), "should not burn within threshold");
        assert!((p.current_mana - (-3.0)).abs() < 1e-6);
    }

    #[test]
    fn mana_burn_reduces_regen() {
        let mut p = ManaPool::new(100.0, 10.0);
        p.current_mana = 50.0;
        p.trigger_burn();
        assert!(p.is_burning());
        // Regen should be 10 * 0.25 = 2.5 per sec during burn.
        let regen = p.effective_regen_per_sec();
        assert!((regen - 2.5).abs() < 1e-6, "got {}", regen);
    }

    #[test]
    fn mana_burn_expires() {
        let mut p = ManaPool::default();
        p.trigger_burn();
        let burn_dur = p.burn_duration_secs;
        p.tick(burn_dur + 0.01);
        assert!(!p.is_burning(), "burn should expire after duration");
        assert_eq!(p.burn_remaining, 0.0);
    }

    #[test]
    fn mana_modifiers_apply_and_expire() {
        let mut p = ManaPool::new(100.0, 5.0);
        p.current_mana = 50.0;
        p.add_modifier(ManaRegenModifier {
            source: arcane_core::Id64(1),
            delta_per_sec: 20.0,
            duration_secs: Some(2.0),
            elapsed_secs: 0.0,
        });
        // Total regen: 5 + 20 = 25 per sec.
        assert!((p.effective_regen_per_sec() - 25.0).abs() < 1e-6);
        p.tick(2.0);
        // Modifier should be expired. Mana gained: 25*2 = 50, capped at 100.
        assert!((p.current_mana - 100.0).abs() < 1e-6);
        assert!(p.modifiers.is_empty(), "expired modifier should be removed");
    }

    #[test]
    fn mana_modifier_persists_with_no_duration() {
        let mut p = ManaPool::new(100.0, 0.0);
        // Start at zero mana so the +5 modifier has room to accumulate.
        p.current_mana = 0.0;
        p.add_modifier(ManaRegenModifier {
            source: arcane_core::Id64(2),
            delta_per_sec: 5.0,
            duration_secs: None,
            elapsed_secs: 0.0,
        });
        p.tick(10.0);
        assert_eq!(p.modifiers.len(), 1, "permanent modifier should persist");
        assert!((p.current_mana - 50.0).abs() < 1e-6, "got {}", p.current_mana);
    }

    #[test]
    fn mana_node_bonus_attuned_doubles() {
        let node = ManaNode {
            id: arcane_core::IdUlid::new(),
            position: [0.0, 0.0, 0.0],
            bonus_regen_per_sec: 10.0,
            radius: 10.0,
            attuned: false,
        };
        // Player at center: full bonus = 10, halved = 5.
        let unattuned_bonus = node.current_bonus([0.0, 0.0, 0.0]);
        assert!((unattuned_bonus - 5.0).abs() < 1e-6, "got {}", unattuned_bonus);

        // Attuned node: full 10.
        let mut attuned = node;
        attuned.attuned = true;
        let attuned_bonus = attuned.current_bonus([0.0, 0.0, 0.0]);
        assert!((attuned_bonus - 10.0).abs() < 1e-6);
    }

    #[test]
    fn mana_node_bonus_falloff_at_radius() {
        let node = ManaNode::new(arcane_core::IdUlid::new(), [0.0, 0.0, 0.0], 10.0, 10.0);
        // At center: full bonus.
        assert!((node.current_bonus([0.0, 0.0, 0.0]) - 5.0).abs() < 1e-6); // unattuned = halved
        // At radius edge: bonus ~ 0.
        let edge_bonus = node.current_bonus([10.0, 0.0, 0.0]);
        assert!(edge_bonus < 0.5, "edge bonus should be near zero, got {}", edge_bonus);
        // Outside radius: no bonus.
        assert_eq!(node.current_bonus([11.0, 0.0, 0.0]), 0.0);
    }

    #[test]
    fn total_mana_node_bonus_sums_multiple() {
        let n1 = ManaNode::new(arcane_core::IdUlid::new(), [0.0, 0.0, 0.0], 10.0, 5.0);
        let n2 = ManaNode::new(arcane_core::IdUlid::new(), [3.0, 0.0, 0.0], 5.0, 5.0);
        let nodes = vec![n1, n2];
        let total = total_mana_node_bonus(&nodes, [0.0, 0.0, 0.0]);
        // n1 at center: 5 (unattuned halved). n2 at distance 3: falloff 0.4, bonus 5*0.4*0.5 = 1.
        // Total ~ 6.
        assert!(total > 5.0 && total < 7.0, "got {}", total);
    }

    #[test]
    fn burn_proximity_is_zero_when_safe() {
        let mut p = ManaPool::new(100.0, 0.0);
        p.current_mana = 50.0;
        assert_eq!(p.burn_proximity(), 0.0);
    }

    #[test]
    fn burn_proximity_grows_when_negative_mana() {
        let mut p = ManaPool::default();
        p.burn_threshold = 10.0;
        p.current_mana = -5.0;
        let prox = p.burn_proximity();
        assert!(prox > 0.0 && prox < 1.0, "got {}", prox);
        assert!((prox - 0.5).abs() < 1e-6, "should be 0.5 (halfway to threshold)");
    }

    #[test]
    fn clear_burn_resets_state() {
        let mut p = ManaPool::default();
        p.trigger_burn();
        assert!(p.is_burning());
        p.clear_burn();
        assert!(!p.is_burning());
        assert_eq!(p.burn_remaining, 0.0);
    }

    #[test]
    fn restore_clamps_to_max() {
        let mut p = ManaPool::new(100.0, 0.0);
        p.current_mana = 80.0;
        p.restore(50.0);
        assert!((p.current_mana - 100.0).abs() < 1e-6);
    }

    #[test]
    fn set_max_clamps_current() {
        let mut p = ManaPool::new(100.0, 0.0);
        p.current_mana = 80.0;
        p.set_max_mana(50.0);
        assert_eq!(p.max_mana, 50.0);
        assert_eq!(p.current_mana, 50.0);
    }

    #[test]
    fn total_mana_spent_accumulates() {
        let mut p = ManaPool::new(100.0, 0.0);
        p.try_spend(20.0, false);
        p.try_spend(30.0, false);
        assert!((p.total_mana_spent - 50.0).abs() < 1e-6);
    }
}
