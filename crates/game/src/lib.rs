//! Mystical Arcana — game library root.
//!
//! This crate contains all game-side gameplay logic. It depends on the
//! Arcane engine crates (`arcane_*`). All systems are headless-testable
//! by design — pure logic is in this crate; rendering, audio, input are
//! abstracted behind traits implemented in the engine crates.

#![forbid(unsafe_op_in_unsafe_fn)]
#![warn(missing_docs)]

pub const GAME_NAME: &str = "Mystical Arcana";
pub const ENGINE_NAME: &str = "Arcane";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Mana pool, regen, Mana Burn.
pub mod mana;
/// Data-driven rune definitions and combinations.
pub mod runes;
/// Spell composition and casting.
pub mod spells;
/// Learned magical schematics.
pub mod schematics;
/// Items, stacks, pickups.
pub mod inventory;
/// Combat: damage, knockback, status effects.
pub mod combat;
/// Enemies and modular AI.
pub mod enemies;
/// Mana corruption and destabilization.
pub mod corruption;
/// Building system (stabilizing reality).
pub mod building;
/// Sanctuaries.
pub mod sanctuaries;
/// Research tree.
pub mod research;
/// Progression.
pub mod progression;
/// Player controller.
pub mod player;
/// Game-side world definitions.
pub mod world;

/// Headless gameplay loop harness. Drives the simulation forward without a window.
/// Used by `Tests/smoke_headless.rs` and CI.
pub mod headless;

/// Top-level CLI parser + runner. Handles `--observatory`, `--visualize`,
/// `--scenario`, `--output`, `--backend`, etc., alongside the legacy
/// `--smoke` gameplay-loop path.
pub mod cli;
/// Deterministic visual-test scenarios (`empty_scene`, `basic_scene`,
/// `terrain_scene`, `player_scene`, `mana_node_scene`, `combat_scene`,
/// `building_scene`, `corruption_scene`).
pub mod scenario;
/// Live game session: ties the Arcane renderer + player snapshot + particle
/// list + UI frame together. Produces a `RenderScene` each frame.
pub mod session;
/// Render extraction: bridges the session to the renderer's `RenderScene`.
pub mod extract;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn game_constants() {
        assert_eq!(GAME_NAME, "Mystical Arcana");
        assert_eq!(ENGINE_NAME, "Arcane");
    }
}
