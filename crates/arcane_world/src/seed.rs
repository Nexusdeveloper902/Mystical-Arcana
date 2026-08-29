//! World seed — the deterministic root of all procedural generation.

use serde::{Deserialize, Serialize};

/// A 64-bit world seed. Deterministic — same seed → same world, every time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct WorldSeed(pub u64);

impl WorldSeed {
    /// Constructs a seed from a u64.
    pub const fn new(seed: u64) -> Self {
        Self(seed)
    }

    /// Constructs a seed by hashing a string.
    pub fn from_string(s: &str) -> Self {
        // FNV-1a 64-bit
        const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
        const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
        let mut h = FNV_OFFSET;
        for b in s.as_bytes() {
            h ^= *b as u64;
            h = h.wrapping_mul(FNV_PRIME);
        }
        Self(h)
    }

    /// Derives a sub-seed from this seed (for sub-systems that need their
    /// own deterministic seed offset, e.g. "caves", "biomes", "ley_lines").
    pub fn derive(self, label: &str) -> WorldSeed {
        const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
        const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
        let mut h = FNV_OFFSET;
        // Mix in the parent seed first.
        h ^= self.0;
        h = h.wrapping_mul(FNV_PRIME);
        for b in label.as_bytes() {
            h ^= *b as u64;
            h = h.wrapping_mul(FNV_PRIME);
        }
        WorldSeed(h)
    }

    /// Raw value.
    pub fn as_u64(self) -> u64 {
        self.0
    }

    /// Returns a deterministic pseudo-random u32 from this seed.
    pub fn next_u32(self, counter: u32) -> u32 {
        // Splitmix32 — fast, deterministic.
        let mut z = (self.0.wrapping_add(counter as u64)) & 0xFFFF_FFFF;
        z = (z ^ (z >> 16)).wrapping_mul(0x7feb_357d);
        z = (z ^ (z >> 15)).wrapping_mul(0x846c_a49b);
        (z ^ (z >> 16)) as u32
    }
}

impl Default for WorldSeed {
    fn default() -> Self {
        Self(42)
    }
}

impl std::fmt::Display for WorldSeed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "seed:{:016x}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_from_string_is_deterministic() {
        let a = WorldSeed::from_string("mystical arcana");
        let b = WorldSeed::from_string("mystical arcana");
        assert_eq!(a, b);
    }

    #[test]
    fn seed_from_string_distinguishes_inputs() {
        let a = WorldSeed::from_string("alpha");
        let b = WorldSeed::from_string("beta");
        assert_ne!(a, b);
    }

    #[test]
    fn seed_derive_is_deterministic() {
        let s = WorldSeed::new(12345);
        let a = s.derive("caves");
        let b = s.derive("caves");
        assert_eq!(a, b);
        let c = s.derive("biomes");
        assert_ne!(a, c, "different labels produce different sub-seeds");
    }

    #[test]
    fn seed_next_u32_is_deterministic() {
        let s = WorldSeed::new(42);
        let a = s.next_u32(0);
        let b = s.next_u32(0);
        assert_eq!(a, b);
        let c = s.next_u32(1);
        assert_ne!(a, c, "different counters produce different values");
    }

    #[test]
    fn seed_serialization_roundtrip() {
        let s = WorldSeed::new(0xDEAD_BEEF_CAFE_BABE);
        let bytes = postcard::to_allocvec(&s).unwrap();
        let back: WorldSeed = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn seed_default_is_42() {
        assert_eq!(WorldSeed::default().as_u64(), 42);
    }
}
