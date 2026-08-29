//! World chunks — the spatial unit of the procedural + streaming system.

use serde::{Deserialize, Serialize};

/// Chunk edge length in meters. Chunks are cubic.
pub const CHUNK_SIZE: u32 = 32;

/// Number of voxels along each chunk edge (chunk_size / voxel_size).
pub const CHUNK_VOXELS: u32 = 32;

/// Chunk coordinates in chunk-space (integer indices).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ChunkCoord {
    /// X axis.
    pub x: i32,
    /// Y axis (vertical).
    pub y: i32,
    /// Z axis.
    pub z: i32,
}

impl ChunkCoord {
    /// Constructs from components.
    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    /// Converts from world-space meters to chunk coordinates.
    pub fn from_world(world_pos: [f32; 3]) -> Self {
        let s = CHUNK_SIZE as f32;
        Self {
            x: (world_pos[0] / s).floor() as i32,
            y: (world_pos[1] / s).floor() as i32,
            z: (world_pos[2] / s).floor() as i32,
        }
    }

    /// Converts to world-space meters (the chunk's minimum corner).
    pub fn to_world_min(self) -> [f32; 3] {
        let s = CHUNK_SIZE as f32;
        [self.x as f32 * s, self.y as f32 * s, self.z as f32 * s]
    }

    /// Converts to world-space meters (the chunk's center).
    pub fn to_world_center(self) -> [f32; 3] {
        let s = CHUNK_SIZE as f32;
        [
            (self.x as f32 + 0.5) * s,
            (self.y as f32 + 0.5) * s,
            (self.z as f32 + 0.5) * s,
        ]
    }

    /// Returns the 6 axial-neighbour coordinates (+X, -X, +Y, -Y, +Z, -Z).
    pub fn neighbors(self) -> [Self; 6] {
        [
            Self::new(self.x + 1, self.y, self.z),
            Self::new(self.x - 1, self.y, self.z),
            Self::new(self.x, self.y + 1, self.z),
            Self::new(self.x, self.y - 1, self.z),
            Self::new(self.x, self.y, self.z + 1),
            Self::new(self.x, self.y, self.z - 1),
        ]
    }

    /// Squared Chebyshev distance to `other` (in chunk units).
    pub fn chebyshev_distance(self, other: Self) -> i32 {
        ((self.x - other.x).abs())
            .max((self.y - other.y).abs())
            .max((self.z - other.z).abs())
    }
}

/// A stable 1D index for a chunk, used as a hashmap key.
pub type ChunkIndex = (i32, i32, i32);

impl From<ChunkCoord> for ChunkIndex {
    fn from(c: ChunkCoord) -> Self {
        (c.x, c.y, c.z)
    }
}

/// A voxel-density chunk. Each cell stores a `density` f32 (0.0 = empty,
/// 1.0 = solid). The renderer meshes this; the procedural generator fills it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Chunk {
    /// This chunk's coordinate.
    pub coord: ChunkCoord,
    /// Voxel densities, indexed as `[z * CHUNK_VOXELS * CHUNK_VOXELS + y * CHUNK_VOXELS + x]`.
    pub densities: Vec<f32>,
    /// Per-voxel biome id (0 = surface base, others defined by [`BiomeMap`]).
    pub biome_ids: Vec<u8>,
    /// Per-voxel mana density (0.0..1.0).
    pub mana_density: Vec<f32>,
    /// Corruption level for this whole chunk (0.0..1.0).
    pub corruption: f32,
    /// Modified flag — true if the player has altered any voxel. Causes
    /// the chunk to be persisted to disk on unload.
    pub modified: bool,
}

impl Chunk {
    /// Number of voxels in the chunk.
    pub fn voxel_count() -> usize {
        (CHUNK_VOXELS as usize).pow(3)
    }

    /// Constructs an empty chunk at the given coord.
    pub fn empty(coord: ChunkCoord) -> Self {
        let n = Self::voxel_count();
        Self {
            coord,
            densities: vec![0.0; n],
            biome_ids: vec![0; n],
            mana_density: vec![0.0; n],
            corruption: 0.0,
            modified: false,
        }
    }

    /// Returns the linear index for a (x, y, z) voxel within this chunk.
    pub fn voxel_index(x: u32, y: u32, z: u32) -> usize {
        debug_assert!(x < CHUNK_VOXELS && y < CHUNK_VOXELS && z < CHUNK_VOXELS);
        (z as usize * (CHUNK_VOXELS as usize) * (CHUNK_VOXELS as usize))
            + (y as usize * (CHUNK_VOXELS as usize))
            + x as usize
    }

    /// Gets the density at the given voxel.
    pub fn density(&self, x: u32, y: u32, z: u32) -> f32 {
        self.densities[Self::voxel_index(x, y, z)]
    }

    /// Sets the density at the given voxel. Marks the chunk as modified.
    pub fn set_density(&mut self, x: u32, y: u32, z: u32, v: f32) {
        let i = Self::voxel_index(x, y, z);
        self.densities[i] = v;
        self.modified = true;
    }

    /// Gets the mana density at the given voxel.
    pub fn mana(&self, x: u32, y: u32, z: u32) -> f32 {
        self.mana_density[Self::voxel_index(x, y, z)]
    }

    /// Sets the mana density at the given voxel.
    pub fn set_mana(&mut self, x: u32, y: u32, z: u32, v: f32) {
        let i = Self::voxel_index(x, y, z);
        self.mana_density[i] = v;
        self.modified = true;
    }

    /// Gets the biome id at the given voxel.
    pub fn biome(&self, x: u32, y: u32, z: u32) -> u8 {
        self.biome_ids[Self::voxel_index(x, y, z)]
    }

    /// Sets the biome id at the given voxel.
    pub fn set_biome(&mut self, x: u32, y: u32, z: u32, b: u8) {
        let i = Self::voxel_index(x, y, z);
        self.biome_ids[i] = b;
        self.modified = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_coord_from_world_round_trip() {
        let c = ChunkCoord::new(2, -1, 3);
        let world_min = c.to_world_min();
        // 2 * 32 = 64, -1 * 32 = -32, 3 * 32 = 96.
        assert!((world_min[0] - 64.0).abs() < 1e-6);
        assert!((world_min[1] - (-32.0)).abs() < 1e-6);
        assert!((world_min[2] - 96.0).abs() < 1e-6);

        // Round-trip: same chunk from min corner.
        let c2 = ChunkCoord::from_world(world_min);
        assert_eq!(c, c2);

        // Same chunk from any point inside.
        let inside = [world_min[0] + 31.5, world_min[1] + 5.0, world_min[2] + 12.0];
        let c3 = ChunkCoord::from_world(inside);
        assert_eq!(c, c3);
    }

    #[test]
    fn chunk_coord_to_world_center() {
        let c = ChunkCoord::new(0, 0, 0);
        let center = c.to_world_center();
        // Center of chunk (0,0,0) is (16, 16, 16).
        assert!((center[0] - 16.0).abs() < 1e-6);
        assert!((center[1] - 16.0).abs() < 1e-6);
        assert!((center[2] - 16.0).abs() < 1e-6);
    }

    #[test]
    fn chunk_coord_neighbors_returns_six_axial() {
        let c = ChunkCoord::new(0, 0, 0);
        let n = c.neighbors();
        assert_eq!(n.len(), 6);
        // No duplicates.
        let set: std::collections::HashSet<_> = n.iter().cloned().collect();
        assert_eq!(set.len(), 6);
        // All neighbors differ from c by exactly 1 on one axis.
        for nb in &n {
            let diff = (nb.x - c.x).abs() + (nb.y - c.y).abs() + (nb.z - c.z).abs();
            assert_eq!(diff, 1);
        }
    }

    #[test]
    fn chunk_coord_chebyshev_distance() {
        let a = ChunkCoord::new(0, 0, 0);
        let b = ChunkCoord::new(3, 1, -2);
        assert_eq!(a.chebyshev_distance(b), 3);
    }

    #[test]
    fn chunk_empty_has_zero_density() {
        let c = Chunk::empty(ChunkCoord::new(0, 0, 0));
        for v in &c.densities {
            assert_eq!(*v, 0.0);
        }
        assert!(!c.modified);
    }

    #[test]
    fn chunk_set_density_marks_modified() {
        let mut c = Chunk::empty(ChunkCoord::new(0, 0, 0));
        assert!(!c.modified);
        c.set_density(0, 0, 0, 1.0);
        assert!(c.modified, "set_density should mark chunk as modified");
        assert_eq!(c.density(0, 0, 0), 1.0);
    }

    #[test]
    fn chunk_voxel_index_round_trip() {
        let mut c = Chunk::empty(ChunkCoord::new(0, 0, 0));
        for x in 0..CHUNK_VOXELS {
            for y in 0..CHUNK_VOXELS {
                for z in 0..CHUNK_VOXELS {
                    let v = (x + y * CHUNK_VOXELS + z * CHUNK_VOXELS * CHUNK_VOXELS) as f32 * 0.1;
                    c.set_density(x, y, z, v);
                }
            }
        }
        for x in 0..CHUNK_VOXELS {
            for y in 0..CHUNK_VOXELS {
                for z in 0..CHUNK_VOXELS {
                    let expected = (x + y * CHUNK_VOXELS + z * CHUNK_VOXELS * CHUNK_VOXELS) as f32 * 0.1;
                    assert!((c.density(x, y, z) - expected).abs() < 1e-6, "({},{},{})", x, y, z);
                }
            }
        }
    }

    #[test]
    fn chunk_postcard_roundtrip() {
        let mut c = Chunk::empty(ChunkCoord::new(5, -3, 7));
        c.set_density(1, 2, 3, 0.5);
        c.set_mana(1, 2, 3, 0.7);
        c.set_biome(1, 2, 3, 2);
        c.corruption = 0.42;
        let bytes = postcard::to_allocvec(&c).unwrap();
        let back: Chunk = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn chunk_coord_negative_world_pos_rounds_correctly() {
        let c = ChunkCoord::from_world([-1.0, -1.0, -1.0]);
        assert_eq!(c, ChunkCoord::new(-1, -1, -1));
    }
}
