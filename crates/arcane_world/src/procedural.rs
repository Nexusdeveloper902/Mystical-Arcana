//! Procedural world generator — biomes, height, ley lines, mana density.
//!
//! Per the design doc:
//!   "Higher mana density can mean: rarer resources, stronger enemies,
//!    more mana nodes, more dangerous phenomena, different environmental
//!    appearance."
//!
//! The world generator is a **pure function** of `(seed, chunk_coord)`.
//! Same inputs → same chunk, every time, on every platform.

use crate::chunk::{Chunk, ChunkCoord, CHUNK_VOXELS};
use crate::noise::{fractal_noise_2d, Perlin2D, NoiseSampler, ValueNoise2D};
use crate::seed::WorldSeed;
use serde::{Deserialize, Serialize};

/// Biome identifier. Values are stable IDs into [`BiomeMap`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum Biome {
    /// Surface base — forests, plains, rivers, mountains.
    Surface = 0,
    /// Subterranean / crystalline — caves, mana deposits.
    Subterranean = 1,
    /// Deep — bioluminescent, highly concentrated mana, corruption.
    Deep = 2,
    /// Ancient sanctuary — warm, stable, controlled.
    Sanctuary = 3,
    /// Corrupted — environment warped by over-saturated mana.
    Corrupted = 4,
}

impl Biome {
    /// Returns the canonical short name.
    pub fn as_str(self) -> &'static str {
        match self {
            Biome::Surface => "surface",
            Biome::Subterranean => "subterranean",
            Biome::Deep => "deep",
            Biome::Sanctuary => "sanctuary",
            Biome::Corrupted => "corrupted",
        }
    }

    /// Parses from canonical name.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "surface" => Some(Self::Surface),
            "subterranean" => Some(Self::Subterranean),
            "deep" => Some(Self::Deep),
            "sanctuary" => Some(Self::Sanctuary),
            "corrupted" => Some(Self::Corrupted),
            _ => None,
        }
    }

    /// Numeric id.
    pub fn id(self) -> u8 {
        self as u8
    }
}

/// A mapping from biome ids to metadata. Stored separately from chunks so
/// biome ids stay compact in the chunk data.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BiomeMap {
    /// Human-readable name per id.
    pub names: Vec<String>,
}

impl BiomeMap {
    /// Default starter biome map covering all 5 known biomes.
    pub fn default_biomes() -> Self {
        Self {
            names: vec![
                Biome::Surface.as_str().into(),
                Biome::Subterranean.as_str().into(),
                Biome::Deep.as_str().into(),
                Biome::Sanctuary.as_str().into(),
                Biome::Corrupted.as_str().into(),
            ],
        }
    }

    /// Looks up a biome name by id.
    pub fn name(&self, id: u8) -> &str {
        self.names.get(id as usize).map(|s| s.as_str()).unwrap_or("unknown")
    }

    /// Looks up a biome by id.
    pub fn biome(&self, id: u8) -> Option<Biome> {
        match id {
            0 => Some(Biome::Surface),
            1 => Some(Biome::Subterranean),
            2 => Some(Biome::Deep),
            3 => Some(Biome::Sanctuary),
            4 => Some(Biome::Corrupted),
            _ => None,
        }
    }
}

/// A ley line — a spline-graph of magical flow between two world points.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct LeyLine {
    /// Stable id.
    pub id: arcane_core::IdUlid,
    /// Start point in world space.
    pub start: [f32; 3],
    /// End point in world space.
    pub end: [f32; 3],
    /// Intensity (0.0..1.0). Higher = more mana flow.
    pub intensity: f32,
}

impl LeyLine {
    /// Constructs a new ley line.
    pub fn new(id: arcane_core::IdUlid, start: [f32; 3], end: [f32; 3], intensity: f32) -> Self {
        Self { id, start, end, intensity: intensity.clamp(0.0, 1.0) }
    }

    /// True if `point` is within `radius` of any point along this ley line's
    /// straight segment. (Future versions may use splines.)
    pub fn is_within_distance(self, point: [f32; 3], radius: f32) -> bool {
        // Distance from point to line segment start..end.
        let ax = self.start[0]; let ay = self.start[1]; let az = self.start[2];
        let bx = self.end[0];   let by = self.end[1];   let bz = self.end[2];
        let px = point[0]; let py = point[1]; let pz = point[2];

        let abx = bx - ax; let aby = by - ay; let abz = bz - az;
        let apx = px - ax; let apy = py - ay; let apz = pz - az;
        let ab_sq = abx * abx + aby * aby + abz * abz;
        let t = if ab_sq > 1e-9 {
            (apx * abx + apy * aby + apz * abz) / ab_sq
        } else {
            0.0
        };
        let t = t.clamp(0.0, 1.0);
        let cx = ax + abx * t;
        let cy = ay + aby * t;
        let cz = az + abz * t;
        let dx = px - cx; let dy = py - cy; let dz = pz - cz;
        let dist_sq = dx * dx + dy * dy + dz * dz;
        dist_sq <= radius * radius
    }

    /// Bonus mana density contributed by this ley line at `point`.
    pub fn mana_bonus_at(self, point: [f32; 3]) -> f32 {
        // Falloff from the line — simple inverse square.
        let ax = self.start[0]; let ay = self.start[1]; let az = self.start[2];
        let bx = self.end[0];   let by = self.end[1];   let bz = self.end[2];
        let px = point[0]; let py = point[1]; let pz = point[2];

        let abx = bx - ax; let aby = by - ay; let abz = bz - az;
        let apx = px - ax; let apy = py - ay; let apz = pz - az;
        let ab_sq = abx * abx + aby * aby + abz * abz;
        let t = if ab_sq > 1e-9 {
            (apx * abx + apy * aby + apz * abz) / ab_sq
        } else {
            0.0
        };
        let t = t.clamp(0.0, 1.0);
        let cx = ax + abx * t;
        let cy = ay + aby * t;
        let cz = az + abz * t;
        let dx = px - cx; let dy = py - cy; let dz = pz - cz;
        let dist_sq = dx * dx + dy * dy + dz * dz;
        let dist = dist_sq.sqrt();
        // Mana bonus = intensity / (1 + dist^2). Falls off smoothly.
        self.intensity / (1.0 + dist * dist)
    }
}

/// A mana density field — evaluates the ambient mana at a world point.
#[derive(Debug, Clone)]
pub struct ManaField {
    /// World seed.
    pub seed: WorldSeed,
    /// The perlin sampler for low-frequency mana variation.
    pub perlin: Perlin2D,
    /// The value-noise sampler for high-frequency mana flicker.
    pub value: ValueNoise2D,
}

impl ManaField {
    /// Constructs a mana field from a world seed.
    pub fn new(seed: WorldSeed) -> Self {
        Self {
            seed,
            perlin: Perlin2D::new(seed.derive("mana")),
            value: ValueNoise2D::new(seed.derive("mana_flicker")),
        }
    }

    /// Returns the ambient mana density at `point`. Output in [0, 1].
    /// Higher near ley lines and corrupted regions; lower in sanctuaries.
    pub fn density_at(&self, point: [f32; 3], ley_lines: &[LeyLine]) -> f32 {
        // Low-frequency base: perlin noise in [-1,1], normalize to [0,1].
        let base = (fractal_noise_2d(&self.perlin, point[0] * 0.005, point[2] * 0.005, 4, 0.5, 2.0, 1.0) + 1.0) * 0.5;
        // Depth bonus: deeper y = more mana (underground is denser).
        let depth = (-point[1] / 50.0).clamp(0.0, 0.4);
        // High-frequency flicker: small +-0.1 modulation.
        let flicker = (self.value.sample(point[0] * 0.3, point[2] * 0.3) - 0.5) * 0.2;
        // Ley line contributions.
        let ley_bonus: f32 = ley_lines.iter().map(|l| l.mana_bonus_at(point)).sum();

        (base * 0.4 + depth + flicker + ley_bonus * 0.3).clamp(0.0, 1.0)
    }
}

/// The top-level world generator. Holds all samplers + ley lines and
/// produces a `Chunk` from a `ChunkCoord`.
#[derive(Debug, Clone)]
pub struct WorldGenerator {
    /// World seed.
    pub seed: WorldSeed,
    /// Terrain perlin sampler.
    pub terrain: Perlin2D,
    /// Cave perlin sampler.
    pub caves: Perlin2D,
    /// Biome perlin sampler.
    pub biomes: Perlin2D,
    /// Mana field.
    pub mana: ManaField,
    /// Ley lines for this world.
    pub ley_lines: Vec<LeyLine>,
    /// Sea level (meters).
    pub sea_level: f32,
    /// Maximum terrain height (meters).
    pub max_height: f32,
}

impl WorldGenerator {
    /// Constructs a new generator from a seed. Ley lines are auto-generated.
    pub fn new(seed: WorldSeed) -> Self {
        let terrain = Perlin2D::new(seed.derive("terrain"));
        let caves = Perlin2D::new(seed.derive("caves"));
        let biomes = Perlin2D::new(seed.derive("biomes"));
        let mana = ManaField::new(seed);
        let ley_lines = Self::generate_ley_lines(seed);
        Self {
            seed,
            terrain,
            caves,
            biomes,
            mana,
            ley_lines,
            sea_level: 0.0,
            max_height: 64.0,
        }
    }

    /// Generates a set of ley lines for this world. Deterministic per seed.
    pub fn generate_ley_lines(seed: WorldSeed) -> Vec<LeyLine> {
        let mut out = Vec::new();
        let sub = seed.derive("ley_lines");
        // 8 ley lines, each running roughly along a major axis.
        for i in 0..8u32 {
            let angle = sub.next_u32(i) as f32 / (u32::MAX as f32) * std::f32::consts::TAU;
            let radius = 200.0 + (sub.next_u32(i + 100) as f32 / (u32::MAX as f32)) * 400.0;
            let start = [
                (angle.sin() * radius),
                -20.0 - (sub.next_u32(i + 200) as f32 / (u32::MAX as f32)) * 80.0,
                (angle.cos() * radius),
            ];
            let end = [
                -start[0] + (sub.next_u32(i + 300) as f32 / (u32::MAX as f32) - 0.5) * 100.0,
                -50.0 - (sub.next_u32(i + 400) as f32 / (u32::MAX as f32)) * 100.0,
                -start[2] + (sub.next_u32(i + 500) as f32 / (u32::MAX as f32) - 0.5) * 100.0,
            ];
            let intensity = 0.5 + (sub.next_u32(i + 600) as f32 / (u32::MAX as f32)) * 0.5;
            out.push(LeyLine::new(arcane_core::IdUlid::new(), start, end, intensity));
        }
        out
    }

    /// Generates a complete chunk at the given coordinate. Pure function.
    pub fn generate_chunk(&self, coord: ChunkCoord) -> Chunk {
        let mut chunk = Chunk::empty(coord);
        let base = coord.to_world_min();
        let ley = &self.ley_lines;

        // Determine biome for this chunk based on average height + corruption.
        // For now we use a coarse biome mask derived from the chunk's center.
        let center = coord.to_world_center();
        let biome_mask = (fractal_noise_2d(&self.biomes, center[0] * 0.002, center[2] * 0.002, 4, 0.5, 2.0, 1.0) + 1.0) * 0.5;
        // Map biome mask to a biome id (0..2 by default; 3 for sanctuary, 4 for corrupted).
        // Sanctuary is a future explicit-placement system; for now sanctuaries are not
        // auto-generated. Corrupted regions emerge where mana density > 0.7.
        let chunk_mana_avg = self.mana.density_at(center, ley);
        let biome_id = if chunk_mana_avg > 0.75 {
            Biome::Corrupted.id()
        } else if coord.y < -3 {
            Biome::Deep.id()
        } else if coord.y < 0 {
            Biome::Subterranean.id()
        } else {
            Biome::Surface.id()
        };
        let _ = biome_mask; // Future: use to vary density within biome.

        for z in 0..CHUNK_VOXELS {
            for y in 0..CHUNK_VOXELS {
                for x in 0..CHUNK_VOXELS {
                    let i = Chunk::voxel_index(x, y, z);
                    let world_x = base[0] + x as f32;
                    let world_y = base[1] + y as f32;
                    let world_z = base[2] + z as f32;

                    // Height from terrain noise at this (x, z).
                    let h = self.terrain_height(world_x, world_z);
                    // Solid if voxel y is below terrain height.
                    let density = if world_y < h { 1.0 } else { 0.0 };
                    chunk.densities[i] = density;
                    chunk.biome_ids[i] = biome_id;

                    // Mana density at this voxel.
                    chunk.mana_density[i] = self.mana.density_at([world_x, world_y, world_z], ley);

                    // Caves: carved out by high-frequency noise inside solid regions.
                    if density > 0.5 && coord.y < 1 {
                        let cave = fractal_noise_2d(&self.caves, world_x * 0.05, world_z * 0.05, 3, 0.5, 2.0, 1.0);
                        if cave.abs() < 0.08 {
                            chunk.densities[i] = 0.0;
                        }
                    }
                }
            }
        }

        // Chunk-level corruption = average mana density.
        let total_mana: f32 = chunk.mana_density.iter().sum();
        chunk.corruption = total_mana / (chunk.mana_density.len() as f32);

        chunk
    }

    /// Returns the terrain height (world Y) at (x, z).
    pub fn terrain_height(&self, x: f32, z: f32) -> f32 {
        let base = fractal_noise_2d(&self.terrain, x * 0.005, z * 0.005, 5, 0.5, 2.0, 1.0);
        // Normalize from [-1, 1] to [0, 1] then scale to max_height.
        let normalized = (base + 1.0) * 0.5;
        self.sea_level + normalized * self.max_height
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn biome_names_round_trip() {
        for b in [
            Biome::Surface,
            Biome::Subterranean,
            Biome::Deep,
            Biome::Sanctuary,
            Biome::Corrupted,
        ] {
            let s = b.as_str();
            assert_eq!(Biome::from_str(s), Some(b));
        }
    }

    #[test]
    fn biome_map_default_has_all_5_biomes() {
        let m = BiomeMap::default_biomes();
        for i in 0..5 {
            assert_eq!(m.biome(i).map(|b| b.id()), Some(i as u8));
        }
        assert_eq!(m.biome(99), None);
    }

    #[test]
    fn ley_line_distance_check() {
        let l = LeyLine::new(arcane_core::IdUlid::new(), [0.0, 0.0, 0.0], [10.0, 0.0, 0.0], 1.0);
        // Point at midpoint, 1m off the line.
        assert!(l.is_within_distance([5.0, 1.0, 0.0], 2.0));
        // Point far off the line.
        assert!(!l.is_within_distance([5.0, 10.0, 0.0], 2.0));
        // Point near endpoint.
        assert!(l.is_within_distance([10.0, 0.0, 0.0], 1.0));
        // Point beyond endpoint (should clamp to endpoint).
        assert!(l.is_within_distance([15.0, 0.0, 0.0], 5.0));
        assert!(!l.is_within_distance([20.0, 0.0, 0.0], 5.0));
    }

    #[test]
    fn ley_line_mana_bonus_falls_off_with_distance() {
        let l = LeyLine::new(arcane_core::IdUlid::new(), [0.0, 0.0, 0.0], [10.0, 0.0, 0.0], 1.0);
        let on_line = l.mana_bonus_at([5.0, 0.0, 0.0]);
        let near = l.mana_bonus_at([5.0, 1.0, 0.0]);
        let far = l.mana_bonus_at([5.0, 10.0, 0.0]);
        assert!(on_line > near);
        assert!(near > far);
        assert!((on_line - 1.0).abs() < 1e-6, "on-line bonus should equal intensity");
    }

    #[test]
    fn mana_field_density_in_zero_to_one_range() {
        let seed = WorldSeed::new(42);
        let f = ManaField::new(seed);
        let ley = vec![];
        for i in 0..1000 {
            let x = (i as f32) * 0.7 - 350.0;
            let y = (i as f32) * 0.3 - 100.0;
            let z = (i as f32) * 0.5 - 250.0;
            let d = f.density_at([x, y, z], &ley);
            assert!(d >= 0.0 && d <= 1.0, "density out of range: {}", d);
        }
    }

    #[test]
    fn mana_field_is_deterministic_for_same_seed() {
        let seed = WorldSeed::new(42);
        let f1 = ManaField::new(seed);
        let f2 = ManaField::new(seed);
        let ley = WorldGenerator::new(seed).ley_lines.clone();
        for i in 0..100 {
            let x = i as f32 * 7.0;
            let y = i as f32 * 3.0 - 50.0;
            let z = i as f32 * 5.0;
            let a = f1.density_at([x, y, z], &ley);
            let b = f2.density_at([x, y, z], &ley);
            assert!((a - b).abs() < 1e-6, "mana field must be deterministic: {} vs {}", a, b);
        }
    }

    #[test]
    fn world_generator_is_deterministic() {
        let seed = WorldSeed::new(123);
        let g1 = WorldGenerator::new(seed);
        let g2 = WorldGenerator::new(seed);
        let coord = ChunkCoord::new(0, 0, 0);
        let c1 = g1.generate_chunk(coord);
        let c2 = g2.generate_chunk(coord);
        assert_eq!(c1, c2, "world generation must be deterministic for same seed");
    }

    #[test]
    fn world_generator_different_seeds_produce_different_chunks() {
        let g1 = WorldGenerator::new(WorldSeed::new(1));
        let g2 = WorldGenerator::new(WorldSeed::new(2));
        let coord = ChunkCoord::new(0, 0, 0);
        let c1 = g1.generate_chunk(coord);
        let c2 = g2.generate_chunk(coord);
        // At least the densities should differ.
        assert_ne!(c1.densities, c2.densities, "different seeds should produce different terrain");
    }

    #[test]
    fn surface_chunk_has_some_solid_density() {
        let g = WorldGenerator::new(WorldSeed::new(42));
        let c = g.generate_chunk(ChunkCoord::new(0, 0, 0));
        // At least one voxel should be solid at y=0..5 (close to terrain base).
        let mut solid_count = 0;
        for x in 0..CHUNK_VOXELS {
            for z in 0..CHUNK_VOXELS {
                for y in 0..CHUNK_VOXELS.min(5) {
                    if c.density(x, y, z) > 0.5 {
                        solid_count += 1;
                    }
                }
            }
        }
        assert!(solid_count > 0, "surface chunk should have some solid voxels");
    }

    #[test]
    fn deep_chunks_have_higher_mana_than_surface() {
        let g = WorldGenerator::new(WorldSeed::new(42));
        let surface = g.generate_chunk(ChunkCoord::new(0, 0, 0));
        let deep = g.generate_chunk(ChunkCoord::new(0, -10, 0));
        let avg_surface: f32 = surface.mana_density.iter().sum::<f32>() / surface.mana_density.len() as f32;
        let avg_deep: f32 = deep.mana_density.iter().sum::<f32>() / deep.mana_density.len() as f32;
        assert!(avg_deep > avg_surface, "deep should have more mana ({} vs {})", avg_deep, avg_surface);
    }

    #[test]
    fn ley_lines_generated_deterministically() {
        let s = WorldSeed::new(7);
        let a = WorldGenerator::generate_ley_lines(s);
        let b = WorldGenerator::generate_ley_lines(s);
        assert_eq!(a.len(), b.len());
        for (la, lb) in a.iter().zip(b.iter()) {
            assert_eq!(la.start, lb.start);
            assert_eq!(la.end, lb.end);
            assert_eq!(la.intensity, lb.intensity);
        }
    }

    #[test]
    fn chunk_corruption_is_average_mana() {
        let g = WorldGenerator::new(WorldSeed::new(42));
        let c = g.generate_chunk(ChunkCoord::new(0, 0, 0));
        let avg: f32 = c.mana_density.iter().sum::<f32>() / c.mana_density.len() as f32;
        assert!((c.corruption - avg).abs() < 1e-3, "corruption should equal avg mana");
    }

    #[test]
    fn world_generator_terrain_height_in_range() {
        let g = WorldGenerator::new(WorldSeed::new(42));
        for i in 0..100 {
            let x = i as f32 * 7.0;
            let z = i as f32 * 5.0;
            let h = g.terrain_height(x, z);
            assert!(h >= g.sea_level, "height below sea level: {}", h);
            assert!(h <= g.sea_level + g.max_height + 1e-3, "height above max: {}", h);
        }
    }

    #[test]
    fn ley_line_postcard_roundtrip() {
        let l = LeyLine::new(arcane_core::IdUlid::new(), [1.0, 2.0, 3.0], [4.0, 5.0, 6.0], 0.7);
        let bytes = postcard::to_allocvec(&l).unwrap();
        let back: LeyLine = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(l, back);
    }
}
