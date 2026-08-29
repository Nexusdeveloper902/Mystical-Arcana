//! Deterministic noise functions.
//!
//! All noise implementations are pure functions of `(seed, x, y)` and must
//! produce identical output across platforms. The engine uses Perlin-like
//! gradient noise + fractional Brownian motion (fBm) for natural-looking
//! continuous fields, and Worley/cellular noise for discrete cell patterns.

use crate::seed::WorldSeed;

/// A noise sampler — anything that can produce a continuous 0..1 value at
/// a 2D coordinate. All samplers must be deterministic given a fixed seed.
pub trait NoiseSampler {
    /// Returns a noise value at `(x, y)`. Typically in [-1, 1] or [0, 1].
    fn sample(&self, x: f32, y: f32) -> f32;
}

/// A 2D Perlin-like gradient noise implementation. Deterministic.
#[derive(Debug, Clone)]
pub struct Perlin2D {
    /// Permutation table — built from the seed.
    perm: [u8; 512],
}

impl Perlin2D {
    /// Constructs a new Perlin sampler from a seed.
    pub fn new(seed: WorldSeed) -> Self {
        let mut perm = [0u8; 512];
        // Initialize the first 256 with identity.
        for i in 0..256 {
            perm[i] = i as u8;
        }
        // Fisher-Yates shuffle with the seed's splitmix32 PRNG.
        let mut counter = 0u32;
        for i in (1..256).rev() {
            let j = (seed.next_u32(counter) as usize) % (i + 1);
            counter += 1;
            perm.swap(i, j);
        }
        // Duplicate to make 512.
        for i in 0..256 {
            perm[256 + i] = perm[i];
        }
        Self { perm }
    }

    fn grad(&self, hash: u8, x: f32, y: f32) -> f32 {
        // Standard Ken Perlin 8-gradient set (simplified for 2D).
        match hash & 7 {
            0 =>  x + y,
            1 =>  x - y,
            2 => -x + y,
            3 => -x - y,
            4 =>  x,
            5 => -x,
            6 =>  y,
            _ => -y,
        }
    }

    fn fade(t: f32) -> f32 {
        t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
    }

    fn lerp(a: f32, b: f32, t: f32) -> f32 {
        a + (b - a) * t
    }
}

impl NoiseSampler for Perlin2D {
    fn sample(&self, x: f32, y: f32) -> f32 {
        let xi = (x.floor() as i32 & 255) as usize;
        let yi = (y.floor() as i32 & 255) as usize;
        let xf = x - x.floor();
        let yf = y - y.floor();
        let u = Self::fade(xf);
        let v = Self::fade(yf);

        let aa_idx = (self.perm[xi] as usize + yi) & 511;
        let ab_idx = (self.perm[xi] as usize + yi + 1) & 511;
        let ba_idx = (self.perm[xi + 1] as usize + yi) & 511;
        let bb_idx = (self.perm[xi + 1] as usize + yi + 1) & 511;

        let aa = self.perm[aa_idx];
        let ab = self.perm[ab_idx];
        let ba = self.perm[ba_idx];
        let bb = self.perm[bb_idx];

        let x1 = Self::lerp(self.grad(aa, xf, yf), self.grad(ba, xf - 1.0, yf), u);
        let x2 = Self::lerp(self.grad(ab, xf, yf - 1.0), self.grad(bb, xf - 1.0, yf - 1.0), u);
        let result = Self::lerp(x1, x2, v);
        // Perlin noise is in approximately [-1, 1] but typically [-0.707, 0.707].
        // We don't normalize here — consumers can normalize as needed.
        result
    }
}

/// Fractional Brownian motion — sums several octaves of base noise for
/// richer, more natural-looking output. Returns a value approximately in
/// [-1, 1].
pub fn fractal_noise_2d<S: NoiseSampler>(
    sampler: &S,
    x: f32,
    y: f32,
    octaves: u32,
    persistence: f32,
    lacunarity: f32,
    base_scale: f32,
) -> f32 {
    let mut total = 0.0_f32;
    let mut amplitude = 1.0_f32;
    let mut frequency = base_scale;
    let mut max_amplitude = 0.0_f32;
    for _ in 0..octaves {
        total += sampler.sample(x * frequency, y * frequency) * amplitude;
        max_amplitude += amplitude;
        amplitude *= persistence;
        frequency *= lacunarity;
    }
    total / max_amplitude.max(1e-6)
}

/// Cheap value noise — interpolates between random values at integer points.
/// Less smooth than Perlin but fast. Useful for non-critical contexts.
#[derive(Debug, Clone)]
pub struct ValueNoise2D {
    seed: WorldSeed,
}

impl ValueNoise2D {
    /// Constructs a value-noise sampler from a seed.
    pub fn new(seed: WorldSeed) -> Self {
        Self { seed }
    }

    /// Returns a deterministic pseudo-random 0..1 value at the integer
    /// lattice point (ix, iy).
    fn lattice(&self, ix: i32, iy: i32) -> f32 {
        let h = self.seed.0
            ^ (ix as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
            ^ (iy as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
        let z = (h ^ (h >> 17)).wrapping_mul(0xBF58_4276_D8E5_7792u64);
        ((z ^ (z >> 17)) as u32 as f32) / (u32::MAX as f32)
    }
}

impl NoiseSampler for ValueNoise2D {
    fn sample(&self, x: f32, y: f32) -> f32 {
        let xi = x.floor() as i32;
        let yi = y.floor() as i32;
        let xf = x - x.floor();
        let yf = y - y.floor();
        let u = xf * xf * (3.0 - 2.0 * xf);
        let v = yf * yf * (3.0 - 2.0 * yf);
        let aa = self.lattice(xi, yi);
        let ba = self.lattice(xi + 1, yi);
        let ab = self.lattice(xi, yi + 1);
        let bb = self.lattice(xi + 1, yi + 1);
        let x1 = aa + (ba - aa) * u;
        let x2 = ab + (bb - ab) * u;
        x1 + (x2 - x1) * v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perlin_is_deterministic_across_instances() {
        let s = WorldSeed::new(123);
        let a = Perlin2D::new(s);
        let b = Perlin2D::new(s);
        for (x, y) in [(0.0, 0.0), (1.5, 2.3), (-3.4, 7.8), (100.0, -50.0)] {
            assert!((a.sample(x, y) - b.sample(x, y)).abs() < 1e-7, "Perlin must be deterministic");
        }
    }

    #[test]
    fn perlin_seed_affects_output() {
        let a = Perlin2D::new(WorldSeed::new(1));
        let b = Perlin2D::new(WorldSeed::new(2));
        let mut different = 0;
        for i in 0..20 {
            let x = i as f32 * 0.7;
            let y = i as f32 * 1.3;
            if (a.sample(x, y) - b.sample(x, y)).abs() > 1e-3 {
                different += 1;
            }
        }
        assert!(different >= 10, "different seeds should produce different noise, got {} differences", different);
    }

    #[test]
    fn perlin_output_in_reasonable_range() {
        let p = Perlin2D::new(WorldSeed::new(42));
        let mut min = f32::INFINITY;
        let mut max = f32::NEG_INFINITY;
        for i in 0..1000 {
            let v = p.sample(i as f32 * 0.13, i as f32 * 0.27);
            if v < min { min = v; }
            if v > max { max = v; }
        }
        // Perlin output should land roughly in [-1, 1] for the 8-gradient set.
        assert!(min >= -2.0 && max <= 2.0, "Perlin out of range: [{}, {}]", min, max);
        assert!(max > min, "noise must produce variation");
    }

    #[test]
    fn fractal_noise_smooth_at_integer_lattice() {
        let p = Perlin2D::new(WorldSeed::new(99));
        // Two close samples should produce close values.
        let a = fractal_noise_2d(&p, 10.0, 10.0, 4, 0.5, 2.0, 0.05);
        let b = fractal_noise_2d(&p, 10.001, 10.0, 4, 0.5, 2.0, 0.05);
        assert!((a - b).abs() < 0.01, "close samples should produce close values: {} vs {}", a, b);
    }

    #[test]
    fn fractal_noise_deterministic() {
        let p1 = Perlin2D::new(WorldSeed::new(7));
        let p2 = Perlin2D::new(WorldSeed::new(7));
        for (x, y) in [(0.0, 0.0), (3.0, 5.0), (-1.0, -2.0)] {
            let a = fractal_noise_2d(&p1, x, y, 4, 0.5, 2.0, 0.05);
            let b = fractal_noise_2d(&p2, x, y, 4, 0.5, 2.0, 0.05);
            assert!((a - b).abs() < 1e-7);
        }
    }

    #[test]
    fn value_noise_is_deterministic() {
        let a = ValueNoise2D::new(WorldSeed::new(5));
        let b = ValueNoise2D::new(WorldSeed::new(5));
        for (x, y) in [(0.0, 0.0), (1.5, 2.3), (-3.4, 7.8)] {
            assert!((a.sample(x, y) - b.sample(x, y)).abs() < 1e-7);
        }
    }

    #[test]
    fn value_noise_output_in_0_to_1() {
        let n = ValueNoise2D::new(WorldSeed::new(11));
        for i in 0..1000 {
            let v = n.sample(i as f32 * 0.31, i as f32 * 0.71);
            assert!(v >= 0.0 && v <= 1.0, "value noise out of [0,1]: {}", v);
        }
    }
}
