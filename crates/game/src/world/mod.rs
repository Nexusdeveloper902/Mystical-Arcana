//! World-side save/persistence.
//!
//! Saves the world seed + modified chunk overrides + placed structures +
//! sanctuaries. The complete save for a player run combines [`WorldSave`]
//! with [`crate::player::PlayerState`].

use crate::building::Structure;
use crate::sanctuaries::Sanctuary;
use arcane_world::{Chunk, ChunkCoord, WorldSeed};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The persisted world state. World-gen source is `seed`; only chunks that
/// the player has *modified* need to be saved (others regenerate on demand).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldSave {
    /// The world seed — full world regenerates from this.
    pub seed: WorldSeed,
    /// Modified chunks. Each is a complete override of the generated version.
    pub modified_chunks: HashMap<ChunkCoord, Chunk>,
    /// All placed structures.
    pub structures: Vec<Structure>,
    /// All sanctuaries.
    pub sanctuaries: Vec<Sanctuary>,
    /// World-age in seconds (time since world was created).
    pub age_secs: f32,
}

impl WorldSave {
    /// Constructs a fresh world save with the given seed.
    pub fn new(seed: WorldSeed) -> Self {
        Self {
            seed,
            modified_chunks: HashMap::new(),
            structures: Vec::new(),
            sanctuaries: Vec::new(),
            age_secs: 0.0,
        }
    }

    /// Records a modified chunk. Called when a chunk is unloaded and it has
    /// `chunk.modified == true`.
    pub fn save_chunk(&mut self, chunk: Chunk) {
        self.modified_chunks.insert(chunk.coord, chunk);
    }

    /// Returns the saved override for a chunk coord, if any.
    pub fn get_chunk(&self, coord: ChunkCoord) -> Option<&Chunk> {
        self.modified_chunks.get(&coord)
    }

    /// Adds a placed structure to the save.
    pub fn add_structure(&mut self, structure: Structure) {
        self.structures.push(structure);
    }

    /// Adds a sanctuary to the save.
    pub fn add_sanctuary(&mut self, s: Sanctuary) {
        self.sanctuaries.push(s);
    }

    /// Removes a structure by its id. Returns true if removed.
    pub fn remove_structure(&mut self, id: arcane_core::IdUlid) -> bool {
        let before = self.structures.len();
        self.structures.retain(|s| s.id != id);
        self.structures.len() < before
    }

    /// Advances the world age.
    pub fn tick(&mut self, dt: f32) {
        self.age_secs += dt;
    }

    /// Number of modified chunks.
    pub fn modified_chunk_count(&self) -> usize {
        self.modified_chunks.len()
    }

    /// Number of structures.
    pub fn structure_count(&self) -> usize {
        self.structures.len()
    }

    /// Number of sanctuaries.
    pub fn sanctuary_count(&self) -> usize {
        self.sanctuaries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::building::{default_structures, Structure};
    use crate::sanctuaries::Sanctuary;
    use arcane_world::ChunkCoord;

    #[test]
    fn fresh_world_save_has_no_modifications() {
        let ws = WorldSave::new(WorldSeed::new(42));
        assert_eq!(ws.modified_chunk_count(), 0);
        assert_eq!(ws.structure_count(), 0);
        assert_eq!(ws.sanctuary_count(), 0);
    }

    #[test]
    fn save_chunk_round_trip() {
        let mut ws = WorldSave::new(WorldSeed::new(42));
        let mut c = Chunk::empty(ChunkCoord::new(5, -3, 7));
        c.set_density(0, 0, 0, 1.0);
        c.set_mana(0, 0, 0, 0.5);
        let coord = c.coord;
        ws.save_chunk(c);
        assert_eq!(ws.modified_chunk_count(), 1);
        let back = ws.get_chunk(coord).unwrap();
        assert!(back.modified);
        assert_eq!(back.density(0, 0, 0), 1.0);
        assert_eq!(back.mana(0, 0, 0), 0.5);
    }

    #[test]
    fn save_chunk_overrides_existing() {
        let mut ws = WorldSave::new(WorldSeed::new(42));
        let coord = ChunkCoord::new(0, 0, 0);
        let mut c1 = Chunk::empty(coord);
        c1.set_density(0, 0, 0, 0.5);
        ws.save_chunk(c1);
        let mut c2 = Chunk::empty(coord);
        c2.set_density(0, 0, 0, 0.9);
        ws.save_chunk(c2);
        assert_eq!(ws.modified_chunk_count(), 1);
        assert_eq!(ws.get_chunk(coord).unwrap().density(0, 0, 0), 0.9);
    }

    #[test]
    fn add_remove_structure() {
        let mut ws = WorldSave::new(WorldSeed::new(42));
        let def = default_structures().into_iter().find(|d| d.id == "ward_pylon").unwrap();
        let s = Structure::new(&def, [0.0, 0.0, 0.0]);
        let id = s.id;
        ws.add_structure(s);
        assert_eq!(ws.structure_count(), 1);
        assert!(ws.remove_structure(id));
        assert_eq!(ws.structure_count(), 0);
        assert!(!ws.remove_structure(id), "second removal should return false");
    }

    #[test]
    fn add_sanctuary_increments_count() {
        let mut ws = WorldSave::new(WorldSeed::new(42));
        let def = default_structures().into_iter().find(|d| d.id == "sanctuary_core").unwrap();
        let s = Structure::new(&def, [0.0, 0.0, 0.0]);
        let san = Sanctuary::new(&s, [0.0, 0.0, 0.0], 20.0);
        ws.add_sanctuary(san);
        assert_eq!(ws.sanctuary_count(), 1);
    }

    #[test]
    fn tick_advances_age() {
        let mut ws = WorldSave::new(WorldSeed::new(42));
        ws.tick(2.5);
        ws.tick(1.5);
        assert!((ws.age_secs - 4.0).abs() < 1e-6);
    }

    #[test]
    fn world_save_postcard_roundtrip() {
        let mut ws = WorldSave::new(WorldSeed::new(0xCAFEBABE));
        let mut c = Chunk::empty(ChunkCoord::new(1, 2, 3));
        c.set_density(1, 1, 1, 0.7);
        ws.save_chunk(c);
        let def = default_structures().into_iter().find(|d| d.id == "storage_cache").unwrap();
        ws.add_structure(Structure::new(&def, [5.0, 6.0, 7.0]));
        ws.tick(42.0);

        let bytes = postcard::to_allocvec(&ws).unwrap();
        let back: WorldSave = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(back.seed, ws.seed);
        assert_eq!(back.modified_chunk_count(), 1);
        assert_eq!(back.structure_count(), 1);
        assert!((back.age_secs - 42.0).abs() < 1e-6);
    }
}
