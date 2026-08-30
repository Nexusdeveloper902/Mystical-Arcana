//! arcane_world — game world / scene graph / chunk storage.
//!
//! For now this crate hosts domain-level scene-graph types that the game
//! composes via arcane_ecs: Transform, MeshKindComponent, Spin (a per-frame
//! Y-rotation rate). A future phase will host the procedural ley-line /
//! mana-zone generator here.

use arcane_math::{Mat4, Vec3};
use arcane_render::MeshKind;

/// Local-to-world transform of an entity. Composed each frame as
/// `translation * rotation_y * scale` to drive the renderer's per-
/// instance model matrix.
#[derive(Clone, Debug)]
pub struct Transform {
    pub position: Vec3,
    pub rotation_y: f32, // radians, accumulated
    pub scale: Vec3,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            rotation_y: 0.0,
            scale: Vec3::ONE,
        }
    }
}

impl Transform {
    pub fn at(position: Vec3) -> Self {
        Self { position, ..Default::default() }
    }

    pub fn with_rotation(mut self, rot_y: f32) -> Self {
        self.rotation_y = rot_y;
        self
    }

    pub fn with_scale(mut self, scale: Vec3) -> Self {
        self.scale = scale;
        self
    }

    /// Build the local-to-world Mat4 the renderer's push constant expects.
    pub fn to_model_matrix(&self) -> Mat4 {
        Mat4::from_translation(self.position)
            * Mat4::from_rotation_y(self.rotation_y)
            * Mat4::from_scale(self.scale)
    }
}

/// Which built-in mesh this entity renders as. Wraps arcane_render::MeshKind
/// so arcane_ecs can store it as a component (MeshKind itself is Copy but
/// we wrap it for clarity inside the world module).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeshKindComponent(pub MeshKind);

impl From<MeshKind> for MeshKindComponent {
    fn from(k: MeshKind) -> Self { Self(k) }
}

/// Per-frame Y rotation rate in radians. The render system advances each
/// entity's Transform.rotation_y by `rate * dt` per frame.
#[derive(Clone, Copy, Debug)]
pub struct Spin {
    pub rate: f32,
}

impl Spin {
    pub fn new(rate_per_frame: f32) -> Self { Self { rate: rate_per_frame } }
}

#[derive(Default)]
pub struct WorldState {
    pub name: String,
    pub time_seconds: f32,
    pub entities: u32,
}

impl WorldState {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into(), time_seconds: 0.0, entities: 0 }
    }
}
