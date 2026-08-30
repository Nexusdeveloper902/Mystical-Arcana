//! Arcane World — chunks, streaming, procedural generation, save system.
//!
//! Deterministic procedural generation is the foundation. Given the same seed,
//! the same world is produced bit-for-bit across platforms and runs.
//! See `Docs/ADR/0003-headless-testing-strategy.md` for the contract.

#![forbid(unsafe_op_in_unsafe_fn)]
#![warn(missing_docs)]

pub mod chunk;
pub mod noise;
pub mod procedural;
pub mod seed;
pub mod streaming;

pub use chunk::{Chunk, ChunkCoord, ChunkIndex, CHUNK_SIZE};
pub use noise::{fractal_noise_2d, NoiseSampler, Perlin2D, ValueNoise2D};
pub use procedural::{Biome, BiomeMap, LeyLine, ManaField, WorldGenerator};
pub use seed::WorldSeed;
pub use streaming::{StreamRequest, StreamResult, Streamer};
