//! `GameSession` — live game state container that ties simulation to the
//! Arcane renderer.
//!
//! Owns:
//! - The Arcane renderer (shared, since the observatory needs read access).
//! - A minimal in-session world snapshot (player position + scenario state).
//! - Player state.
//! - Particle system.
//! - UI frame (`UiDraw` list) composited each render.
//!
//! The session is intentionally lightweight at this milestone: it provides
//! enough state to drive the deterministic visual-test scenarios and the
//! observatory. As the engine's `arcane_world` streaming layer integrates,
//! the session will gain a real `World` field.

use std::sync::Arc;
use std::time::Duration;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use arcane_render::prereqs::{Transform, Vec3};
use arcane_render::scene::{RenderScene, UiDraw};
use arcane_render::{BackendKind, Renderer};

use crate::scenario::ScenarioKind;

/// Lightweight player snapshot for the session. Decoupled from the heavy
/// gameplay-logic `Player` struct (which carries mana pool, inventory, etc.)
/// so the session can be used by the renderer/observatory without depending
/// on the full gameplay state.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlayerSnapshot {
    /// World-space position.
    pub position: Vec3,
    /// Velocity (m/s).
    pub velocity: Vec3,
    /// Health (0..=100).
    pub health: f32,
    /// Mana (0..=100).
    pub mana: f32,
    /// Corruption (0..=100).
    pub corruption: f32,
    /// Currently selected spell slot.
    pub selected_spell: u32,
    /// Equipped spells (names).
    pub spells: Vec<String>,
}

impl Default for PlayerSnapshot {
    fn default() -> Self {
        Self {
            position: Vec3::new(0.0, 5.0, 0.0),
            velocity: Vec3::zeros(),
            health: 100.0,
            mana: 100.0,
            corruption: 0.0,
            selected_spell: 0,
            spells: vec![
                "spark".into(),
                "frost_bolt".into(),
                "stone_shape".into(),
                "veil_step".into(),
                "ward".into(),
                "anchor".into(),
                "binding".into(),
                "transmute".into(),
            ],
        }
    }
}

impl PlayerSnapshot {
    /// Try to cast the selected spell. Returns true if mana was spent.
    pub fn try_cast(&mut self, cost: f32) -> bool {
        if self.mana < cost { return false; }
        self.mana -= cost;
        true
    }

    /// Apply damage; respects death (clamped at 0).
    pub fn apply_damage(&mut self, amount: f32) {
        self.health = (self.health - amount).max(0.0);
        self.corruption = (self.corruption + amount * 0.1).min(100.0);
    }

    /// Tick regeneration.
    pub fn tick(&mut self, dt: f32) {
        self.mana = (self.mana + dt * 1.5).min(100.0);
        self.health = (self.health + dt * 0.2).min(100.0);
        self.corruption = (self.corruption - dt * 0.05).max(0.0);
    }
}

/// A minimal world snapshot for the session.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct WorldSnapshot {
    /// Seed used by worldgen.
    pub seed: u64,
    /// Currently loaded chunk count (informational).
    pub loaded_chunks: u32,
    /// Cumulative generated chunk count.
    pub generated_chunks: u32,
    /// Player world position.
    pub player_position: [f32; 3],
}

/// Game session: top-level live game state.
pub struct GameSession {
    /// The Arcane renderer (CPU or Vulkan), wrapped for observatory access.
    pub renderer: Arc<Renderer>,
    /// Player snapshot.
    pub player: PlayerSnapshot,
    /// UI frame (the latest `UiDraw` list).
    pub ui_frame: Vec<UiDraw>,
    /// Renderer aspect ratio (width/height).
    pub renderer_aspect: f32,
    /// Cumulative simulation time (seconds).
    pub sim_time: f32,
    /// Cumulative real time (seconds).
    pub real_time: f32,
    /// Active scenario (if running scenarios mode).
    pub scenario: Option<ScenarioKind>,
    /// World snapshot.
    pub world: WorldSnapshot,
    /// Particle vertex list for the renderer.
    pub particles: Vec<arcane_render::scene::ParticleVertex>,
}

impl GameSession {
    /// Create a new session with a specific backend kind and resolution.
    pub fn new(backend: BackendKind, width: u32, height: u32, seed: u64) -> Self {
        let renderer = Arc::new(Renderer::new(backend, width, height));
        let player = PlayerSnapshot::default();
        let aspect = width as f32 / height.max(1) as f32;
        Self {
            renderer,
            player,
            ui_frame: Vec::new(),
            renderer_aspect: aspect,
            sim_time: 0.0,
            real_time: 0.0,
            scenario: None,
            world: WorldSnapshot { seed, player_position: [0.0, 5.0, 0.0], ..Default::default() },
            particles: Vec::new(),
        }
    }

    /// Advance the simulation by `dt_seconds`.
    pub fn step(&mut self, dt_seconds: f32) {
        self.sim_time += dt_seconds;
        self.real_time += dt_seconds;
        self.player.tick(dt_seconds);
        self.world.player_position = [self.player.position.x, self.player.position.y, self.player.position.z];
    }

    /// Build the current `RenderScene` via the scenario builder.
    pub fn render_scene(&self) -> RenderScene {
        let aspect = self.renderer_aspect;
        let sim_time = self.sim_time;
        match self.scenario {
            Some(kind) => crate::scenario::Scenario::build(kind, sim_time, aspect).scene,
            None => crate::scenario::Scenario::build(ScenarioKind::Basic, sim_time, aspect).scene,
        }
    }

    /// Render the current scene.
    pub fn render(&self) -> arcane_render::FrameResult {
        let scene = self.render_scene();
        self.renderer.render(&scene)
    }
}

/// Settings serialized into a save file.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct SessionSave {
    /// World seed.
    pub seed: u64,
    /// Player position.
    pub player_position: [f32; 3],
    /// Player health.
    pub player_health: f32,
    /// Player mana.
    pub player_mana: f32,
    /// Player corruption.
    pub player_corruption: f32,
    /// Simulation time.
    pub sim_time: f32,
}

impl GameSession {
    /// Snapshot the current session as a save.
    pub fn to_save(&self) -> SessionSave {
        SessionSave {
            seed: self.world.seed,
            player_position: [self.player.position.x, self.player.position.y, self.player.position.z],
            player_health: self.player.health,
            player_mana: self.player.mana,
            player_corruption: self.player.corruption,
            sim_time: self.sim_time,
        }
    }
}

/// Simple postcard-style save serializer (local to game crate, no dependency
/// on `arcane_assets` which is still a stub).
pub fn save_to_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, postcard::Error> {
    postcard::to_stdvec(value)
}

/// Simple postcard-style save deserializer.
pub fn load_from_bytes<'de, T: Deserialize<'de>>(bytes: &'de [u8]) -> Result<T, postcard::Error> {
    postcard::from_bytes(bytes)
}

/// World chunk side length (in cells) used by scenarios.
pub const WORLD_CHUNK_SIZE: i32 = 32;

/// Deterministic height sample at world (x, z). Used by the terrain scenario
/// as a stand-in for the (real, integrated) `arcane_world` heightfield
/// sampler. Layered sine noise — pure function of (x, z).
pub fn sample_height(x: f32, z: f32) -> f32 {
    let h1 = (x * 0.015).sin() * 6.0;
    let h2 = (z * 0.021).sin() * 5.0;
    let h3 = ((x + z) * 0.007).sin() * 9.0;
    let h4 = ((x * 0.003 + z * 0.005).sin() * 0.5 + 0.5) * 14.0;
    h1 + h2 + h3 + h4
}

#[allow(dead_code)]
fn _unused(_: &RwLock<u32>, _: Duration, _: &Transform, _: &Vec3) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_steps_and_renders() {
        let mut session = GameSession::new(BackendKind::Cpu, 32, 32, 42);
        session.step(1.0 / 60.0);
        let result = session.render();
        assert!(result.png_bytes.is_some(), "render should produce a PNG");
    }

    #[test]
    fn session_save_round_trip() {
        let mut session = GameSession::new(BackendKind::Cpu, 32, 32, 42);
        session.player.position = Vec3::new(5.0, 2.0, -3.0);
        session.player.health = 80.0;
        session.player.mana = 60.0;
        let save = session.to_save();
        let bytes = save_to_bytes(&save).unwrap();
        let back: SessionSave = load_from_bytes(&bytes).unwrap();
        assert_eq!(save.player_position, back.player_position);
    }
}
