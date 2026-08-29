//! Color types for the Arcane renderer.
//!
//! The stylized-magical visual identity needs:
//!   - sRGB-aware conversions (linear work color space).
//!   - 8-bit-per-channel packed colors for textures and assets.
//!   - HDR float colors for emissive magic and bloom.
//!
//! **Conventions** (per the design doc):
//!   - Earth/vegetation/stone colors stay in sRGB display space.
//!   - Magical colors (mana, crystals, emissive runes) work in linear space,
//!     may exceed 1.0, and feed into HDR/bloom pipelines.

use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};

/// A linear-space HDR RGBA color. Each channel may exceed 1.0 for emissive
/// magic. This is the work color space for all shader math.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, Pod, Zeroable)]
#[repr(C)]
pub struct Color {
    /// Red (linear, may exceed 1).
    pub r: f32,
    /// Green (linear, may exceed 1).
    pub g: f32,
    /// Blue (linear, may exceed 1).
    pub b: f32,
    /// Alpha (always 0..1).
    pub a: f32,
}

/// Convenience alias — same as [`Color`].
pub type ColorLinear = Color;

/// 8-bit-per-channel packed sRGB color (RGBA, alpha last).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Pod, Zeroable)]
#[repr(C)]
pub struct Rgba8 {
    /// Red (sRGB, 0..255).
    pub r: u8,
    /// Green (sRGB, 0..255).
    pub g: u8,
    /// Blue (sRGB, 0..255).
    pub b: u8,
    /// Alpha (0..255).
    pub a: u8,
}

/// Convenience alias.
pub type Rgba = Rgba8;

impl Color {
    /// Constructs from linear RGBA components.
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// Linear black, fully opaque.
    pub const BLACK: Self = Self::new(0.0, 0.0, 0.0, 1.0);
    /// Linear white, fully opaque.
    pub const WHITE: Self = Self::new(1.0, 1.0, 1.0, 1.0);
    /// Fully transparent.
    pub const TRANSPARENT: Self = Self::new(0.0, 0.0, 0.0, 0.0);

    /// Constructs from sRGB display-space bytes (converts to linear).
    pub fn from_srgb_u8(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self {
            r: srgb_to_linear_u8(r),
            g: srgb_to_linear_u8(g),
            b: srgb_to_linear_u8(b),
            a: a as f32 / 255.0,
        }
    }

    /// Constructs from an HSV triple (h: 0..360, s: 0..1, v: 0..1).
    pub fn from_hsv(h: f32, s: f32, v: f32) -> Self {
        let c = v * s;
        let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
        let m = v - c;
        let (r1, g1, b1) = if h < 60.0 {
            (c, x, 0.0)
        } else if h < 120.0 {
            (x, c, 0.0)
        } else if h < 180.0 {
            (0.0, c, x)
        } else if h < 240.0 {
            (0.0, x, c)
        } else if h < 300.0 {
            (x, 0.0, c)
        } else {
            (c, 0.0, x)
        };
        Self::new((r1 + m), (g1 + m), (b1 + m), 1.0)
    }

    /// Multiplies (modulates) two colors component-wise — useful for material
    /// blending where a tint multiplies a base color.
    pub fn modulate(self, other: Self) -> Self {
        Self::new(self.r * other.r, self.g * other.g, self.b * other.b, self.a * other.a)
    }

    /// Linear interpolation between two colors.
    pub fn lerp(self, other: Self, t: f32) -> Self {
        Self::new(
            self.r + (other.r - self.r) * t,
            self.g + (other.g - self.g) * t,
            self.b + (other.b - self.b) * t,
            self.a + (other.a - self.a) * t,
        )
    }

    /// Returns the luminance (perceived brightness) of the color.
    pub fn luminance(self) -> f32 {
        0.2126 * self.r + 0.7152 * self.g + 0.0722 * self.b
    }

    /// True if any channel exceeds `1.0` (HDR).
    pub fn is_hdr(self) -> bool {
        self.r > 1.0 || self.g > 1.0 || self.b > 1.0
    }

    /// Converts to sRGB display-space bytes. HDR values are tonemapped by
    /// simple Reinhard-style compression (x / (x + 1)) before encoding.
    pub fn to_srgb_u8(self) -> Rgba8 {
        let r = if self.r > 1.0 { self.r / (self.r + 1.0) } else { self.r };
        let g = if self.g > 1.0 { self.g / (self.g + 1.0) } else { self.g };
        let b = if self.b > 1.0 { self.b / (self.b + 1.0) } else { self.b };
        Rgba8 {
            r: linear_to_srgb_u8(r),
            g: linear_to_srgb_u8(g),
            b: linear_to_srgb_u8(b),
            a: (self.a.clamp(0.0, 1.0) * 255.0).round() as u8,
        }
    }
}

impl Rgba8 {
    /// Constructs from 4 components.
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Opaque black.
    pub const BLACK: Self = Self::new(0, 0, 0, 255);
    /// Opaque white.
    pub const WHITE: Self = Self::new(255, 255, 255, 255);

    /// Converts to a linear HDR [`Color`].
    pub fn to_linear(self) -> Color {
        Color::from_srgb_u8(self.r, self.g, self.b, self.a)
    }

    /// Packed as a single little-endian u32 (0xAABBGGRR).
    pub fn to_u32_le(self) -> u32 {
        ((self.a as u32) << 24) | ((self.b as u32) << 16) | ((self.g as u32) << 8) | (self.r as u32)
    }
}

/// sRGB → linear conversion for an 8-bit channel.
fn srgb_to_linear_u8(c: u8) -> f32 {
    let cs = c as f32 / 255.0;
    if cs <= 0.04045 {
        cs / 12.92
    } else {
        ((cs + 0.055) / 1.055).powf(2.4)
    }
}

/// Linear → sRGB conversion for an 8-bit channel.
fn linear_to_srgb_u8(c: f32) -> u8 {
    let cs = if c <= 0.0031308 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    };
    (cs.clamp(0.0, 1.0) * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srgb_roundtrip_black_white() {
        let black = Color::from_srgb_u8(0, 0, 0, 255);
        assert!((black.r - 0.0).abs() < 1e-6);
        assert!((black.g - 0.0).abs() < 1e-6);
        assert!((black.b - 0.0).abs() < 1e-6);

        let white = Color::from_srgb_u8(255, 255, 255, 255);
        assert!((white.r - 1.0).abs() < 1e-3);
        assert!((white.g - 1.0).abs() < 1e-3);
        assert!((white.b - 1.0).abs() < 1e-3);
    }

    #[test]
    fn srgb_to_linear_midgray_matches_canonical_formula() {
        // sRGB byte 128 → linear ~0.215.
        let c = Color::from_srgb_u8(128, 128, 128, 255);
        assert!((c.r - 0.2158).abs() < 0.005, "got {}", c.r);
    }

    #[test]
    fn rgba8_to_u32_packs_correctly() {
        let c = Rgba8::new(0x11, 0x22, 0x33, 0x44);
        assert_eq!(c.to_u32_le(), 0x44_33_22_11);
    }

    #[test]
    fn color_modulate_works() {
        let a = Color::new(0.5, 0.5, 0.5, 1.0);
        let b = Color::new(0.4, 0.4, 0.4, 1.0);
        let m = a.modulate(b);
        assert!((m.r - 0.2).abs() < 1e-6);
    }

    #[test]
    fn color_lerp_endpoints() {
        let a = Color::BLACK;
        let b = Color::WHITE;
        assert_eq!(a.lerp(b, 0.0), a);
        assert_eq!(a.lerp(b, 1.0), b);
        let mid = a.lerp(b, 0.5);
        assert!((mid.r - 0.5).abs() < 1e-6);
    }

    #[test]
    fn color_luminance_weights_match_rec709() {
        // Pure red should give ~0.2126.
        assert!((Color::new(1.0, 0.0, 0.0, 1.0).luminance() - 0.2126).abs() < 1e-6);
        assert!((Color::new(0.0, 1.0, 0.0, 1.0).luminance() - 0.7152).abs() < 1e-6);
        assert!((Color::new(0.0, 0.0, 1.0, 1.0).luminance() - 0.0722).abs() < 1e-6);
    }

    #[test]
    fn color_hdr_detection() {
        assert!(!Color::WHITE.is_hdr());
        assert!(Color::new(2.0, 0.5, 0.5, 1.0).is_hdr());
        assert!(Color::new(0.0, 0.0, 5.0, 1.0).is_hdr());
    }

    #[test]
    fn hdr_color_tonemaps_on_to_srgb_u8() {
        let hdr = Color::new(10.0, 0.5, 0.5, 1.0);
        let bytes = hdr.to_srgb_u8();
        // Reinhard: 10 / 11 = ~0.909 linear → ~0.97 sRGB byte = ~247
        assert!(bytes.r > 200 && bytes.r < 255, "got {}", bytes.r);
    }

    #[test]
    fn hsv_to_rgb_primary_hues() {
        let red = Color::from_hsv(0.0, 1.0, 1.0);
        assert!((red.r - 1.0).abs() < 1e-6);
        assert!((red.g - 0.0).abs() < 1e-6);
        assert!((red.b - 0.0).abs() < 1e-6);

        let green = Color::from_hsv(120.0, 1.0, 1.0);
        assert!((green.r - 0.0).abs() < 1e-6);
        assert!((green.g - 1.0).abs() < 1e-6);
        assert!((green.b - 0.0).abs() < 1e-6);

        let blue = Color::from_hsv(240.0, 1.0, 1.0);
        assert!((blue.r - 0.0).abs() < 1e-6);
        assert!((blue.g - 0.0).abs() < 1e-6);
        assert!((blue.b - 1.0).abs() < 1e-6);
    }

    #[test]
    fn rgba8_to_linear_roundtrip() {
        let orig = Rgba8::new(123, 200, 50, 200);
        let lin = orig.to_linear();
        let back = lin.to_srgb_u8();
        // Round-trip should be close (sRGB is not exactly reversible for 8-bit).
        assert!((back.r as i32 - orig.r as i32).abs() <= 1, "r: {} vs {}", back.r, orig.r);
        assert!((back.g as i32 - orig.g as i32).abs() <= 1, "g: {} vs {}", back.g, orig.g);
        assert!((back.b as i32 - orig.b as i32).abs() <= 1, "b: {} vs {}", back.b, orig.b);
        assert_eq!(back.a, orig.a);
    }
}
