//! arcane_world — game world / scene graph / chunk storage.
//!
//! Stub for now; will host the procedural ley-line / mana-zone generator.

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
