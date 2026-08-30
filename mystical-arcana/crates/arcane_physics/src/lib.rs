//! arcane_physics — rigid body + mana physics.
//!
//! Stub: real-time constraints to be implemented alongside the renderer.

pub struct PhysicsState {
    pub gravity: f32,
}

impl Default for PhysicsState {
    fn default() -> Self { Self { gravity: -9.81 } }
}
