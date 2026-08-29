//! 2/3/4-component vector types. Thin wrappers around `nalgebra::Vector` for
//! stable naming across the engine and direct `bytemuck::Pod` byte conversion
//! for GPU upload.

use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};

/// A 2D vector.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, Pod, Zeroable)]
#[repr(C)]
pub struct Vec2 {
    /// X component.
    pub x: f32,
    /// Y component.
    pub y: f32,
}

/// A 3D vector.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, Pod, Zeroable)]
#[repr(C)]
pub struct Vec3 {
    /// X component.
    pub x: f32,
    /// Y component.
    pub y: f32,
    /// Z component.
    pub z: f32,
}

/// A 4D vector (homogeneous coordinates, RGBA colors, etc.).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, Pod, Zeroable)]
#[repr(C)]
pub struct Vec4 {
    /// X component.
    pub x: f32,
    /// Y component.
    pub y: f32,
    /// Z component.
    pub z: f32,
    /// W component.
    pub w: f32,
}

impl Vec2 {
    /// Constructs from components.
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    /// The zero vector.
    pub const ZERO: Self = Self::new(0.0, 0.0);

    /// The unit X axis.
    pub const X: Self = Self::new(1.0, 0.0);

    /// The unit Y axis.
    pub const Y: Self = Self::new(0.0, 1.0);

    /// Dot product.
    pub fn dot(self, other: Self) -> f32 {
        self.x * other.x + self.y * other.y
    }

    /// Squared length.
    pub fn length_sq(self) -> f32 {
        self.dot(self)
    }

    /// Length.
    pub fn length(self) -> f32 {
        self.length_sq().sqrt()
    }

    /// Returns the normalized version of this vector.
    pub fn normalized(self) -> Self {
        let l = self.length();
        if l > 0.0 {
            Self::new(self.x / l, self.y / l)
        } else {
            self
        }
    }

    /// Linear interpolation.
    pub fn lerp(self, other: Self, t: f32) -> Self {
        Self::new(
            self.x + (other.x - self.x) * t,
            self.y + (other.y - self.y) * t,
        )
    }
}

impl Vec3 {
    /// Constructs from components.
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    /// The zero vector.
    pub const ZERO: Self = Self::new(0.0, 0.0, 0.0);

    /// Unit X axis.
    pub const X: Self = Self::new(1.0, 0.0, 0.0);
    /// Unit Y axis.
    pub const Y: Self = Self::new(0.0, 1.0, 0.0);
    /// Unit Z axis.
    pub const Z: Self = Self::new(0.0, 0.0, 1.0);

    /// Constructs from a Vec2 + z.
    pub const fn from_vec2_xy(v: Vec2, z: f32) -> Self {
        Self::new(v.x, v.y, z)
    }

    /// Dot product.
    pub fn dot(self, other: Self) -> f32 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    /// Cross product.
    pub fn cross(self, other: Self) -> Self {
        Self::new(
            self.y * other.z - self.z * other.y,
            self.z * other.x - self.x * other.z,
            self.x * other.y - self.y * other.x,
        )
    }

    /// Squared length.
    pub fn length_sq(self) -> f32 {
        self.dot(self)
    }

    /// Length.
    pub fn length(self) -> f32 {
        self.length_sq().sqrt()
    }

    /// Normalized.
    pub fn normalized(self) -> Self {
        let l = self.length();
        if l > 0.0 {
            Self::new(self.x / l, self.y / l, self.z / l)
        } else {
            self
        }
    }

    /// Linear interpolation.
    pub fn lerp(self, other: Self, t: f32) -> Self {
        Self::new(
            self.x + (other.x - self.x) * t,
            self.y + (other.y - self.y) * t,
            self.z + (other.z - self.z) * t,
        )
    }

    /// Component-wise minimum.
    pub fn min(self, other: Self) -> Self {
        Self::new(self.x.min(other.x), self.y.min(other.y), self.z.min(other.z))
    }

    /// Component-wise maximum.
    pub fn max(self, other: Self) -> Self {
        Self::new(self.x.max(other.x), self.y.max(other.y), self.z.max(other.z))
    }
}

impl Vec4 {
    /// Constructs from components.
    pub const fn new(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self { x, y, z, w }
    }

    /// The zero vector.
    pub const ZERO: Self = Self::new(0.0, 0.0, 0.0, 0.0);

    /// From a Vec3 + w.
    pub const fn from_vec3(v: Vec3, w: f32) -> Self {
        Self::new(v.x, v.y, v.z, w)
    }

    /// Dot product.
    pub fn dot(self, other: Self) -> f32 {
        self.x * other.x + self.y * other.y + self.z * other.z + self.w * other.w
    }

    /// Linear interpolation.
    pub fn lerp(self, other: Self, t: f32) -> Self {
        Self::new(
            self.x + (other.x - self.x) * t,
            self.y + (other.y - self.y) * t,
            self.z + (other.z - self.z) * t,
            self.w + (other.w - self.w) * t,
        )
    }
}

// === Operator overloads === ------------------------------------------------

impl std::ops::Add for Vec2 {
    type Output = Vec2;
    fn add(self, rhs: Self) -> Self {
        Vec2::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl std::ops::Sub for Vec2 {
    type Output = Vec2;
    fn sub(self, rhs: Self) -> Self {
        Vec2::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl std::ops::Mul<f32> for Vec2 {
    type Output = Vec2;
    fn mul(self, k: f32) -> Self {
        Vec2::new(self.x * k, self.y * k)
    }
}

impl std::ops::Add for Vec3 {
    type Output = Vec3;
    fn add(self, rhs: Self) -> Self {
        Vec3::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }
}

impl std::ops::Sub for Vec3 {
    type Output = Vec3;
    fn sub(self, rhs: Self) -> Self {
        Vec3::new(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
    }
}

impl std::ops::Mul<f32> for Vec3 {
    type Output = Vec3;
    fn mul(self, k: f32) -> Self {
        Vec3::new(self.x * k, self.y * k, self.z * k)
    }
}

impl std::ops::Add for Vec4 {
    type Output = Vec4;
    fn add(self, rhs: Self) -> Self {
        Vec4::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z, self.w + rhs.w)
    }
}

impl std::ops::Mul<f32> for Vec4 {
    type Output = Vec4;
    fn mul(self, k: f32) -> Self {
        Vec4::new(self.x * k, self.y * k, self.z * k, self.w * k)
    }
}

// === Conversions to/from nalgebra ===

impl From<Vec2> for nalgebra::Vector2<f32> {
    fn from(v: Vec2) -> Self {
        nalgebra::Vector2::new(v.x, v.y)
    }
}
impl From<nalgebra::Vector2<f32>> for Vec2 {
    fn from(v: nalgebra::Vector2<f32>) -> Self {
        Vec2::new(v.x, v.y)
    }
}
impl From<Vec3> for nalgebra::Vector3<f32> {
    fn from(v: Vec3) -> Self {
        nalgebra::Vector3::new(v.x, v.y, v.z)
    }
}
impl From<nalgebra::Vector3<f32>> for Vec3 {
    fn from(v: nalgebra::Vector3<f32>) -> Self {
        Vec3::new(v.x, v.y, v.z)
    }
}
impl From<Vec4> for nalgebra::Vector4<f32> {
    fn from(v: Vec4) -> Self {
        nalgebra::Vector4::new(v.x, v.y, v.z, v.w)
    }
}
impl From<nalgebra::Vector4<f32>> for Vec4 {
    fn from(v: nalgebra::Vector4<f32>) -> Self {
        Vec4::new(v.x, v.y, v.z, v.w)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vec3_dot_and_cross() {
        let a = Vec3::new(1.0, 0.0, 0.0);
        let b = Vec3::new(0.0, 1.0, 0.0);
        assert!((a.dot(b) - 0.0).abs() < 1e-6);
        let c = a.cross(b);
        assert!((c.x - 0.0).abs() < 1e-6);
        assert!((c.y - 0.0).abs() < 1e-6);
        assert!((c.z - 1.0).abs() < 1e-6);
    }

    #[test]
    fn vec3_length_and_normalize() {
        let v = Vec3::new(0.0, 3.0, 4.0);
        assert!((v.length() - 5.0).abs() < 1e-6);
        let n = v.normalized();
        assert!((n.length() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn vec3_lerp_endpoints() {
        let a = Vec3::new(0.0, 0.0, 0.0);
        let b = Vec3::new(10.0, 20.0, 30.0);
        assert_eq!(a.lerp(b, 0.0), a);
        assert_eq!(a.lerp(b, 1.0), b);
        let mid = a.lerp(b, 0.5);
        assert!((mid.x - 5.0).abs() < 1e-6);
        assert!((mid.y - 10.0).abs() < 1e-6);
        assert!((mid.z - 15.0).abs() < 1e-6);
    }

    #[test]
    fn vec3_min_max() {
        let a = Vec3::new(1.0, 5.0, 3.0);
        let b = Vec3::new(4.0, 2.0, 6.0);
        assert_eq!(a.min(b), Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(a.max(b), Vec3::new(4.0, 5.0, 6.0));
    }

    #[test]
    fn vec3_zero_length_normalization_is_safe() {
        let v = Vec3::ZERO;
        let n = v.normalized();
        assert_eq!(n, Vec3::ZERO, "normalizing zero should return zero, not NaN");
    }

    #[test]
    fn vec2_arithmetic_and_dot() {
        let a = Vec2::new(1.0, 2.0);
        let b = Vec2::new(3.0, 4.0);
        assert_eq!(a + b, Vec2::new(4.0, 6.0));
        assert_eq!(b - a, Vec2::new(2.0, 2.0));
        assert!((a.dot(b) - 11.0).abs() < 1e-6);
    }

    #[test]
    fn vec4_dot() {
        let a = Vec4::new(1.0, 2.0, 3.0, 4.0);
        let b = Vec4::new(1.0, 1.0, 1.0, 1.0);
        assert!((a.dot(b) - 10.0).abs() < 1e-6);
    }

    #[test]
    fn vec3_pod_for_gpu_upload() {
        // Confirm Vec3 is Pod (16 bytes? No, 12 bytes — f32 * 3).
        // Confirm we can reinterpret bytes safely.
        let v = Vec3::new(1.0, 2.0, 3.0);
        let bytes: &[u8] = bytemuck::cast_slice(std::slice::from_ref(&v));
        assert_eq!(bytes.len(), 12);
        // First 4 bytes = 1.0f32 in little-endian.
        let mut arr = [0u8; 4];
        arr.copy_from_slice(&bytes[0..4]);
        assert_eq!(f32::from_le_bytes(arr), 1.0);
    }

    #[test]
    fn vec3_serialization_roundtrip() {
        let v = Vec3::new(1.5, -2.0, 3.14);
        let json = serde_json::to_string(&v).unwrap();
        let back: Vec3 = serde_json::from_str(&json).unwrap();
        assert_eq!(v, back);
    }

    #[test]
    fn nalgebra_conversion_roundtrip() {
        let v = Vec3::new(1.0, 2.0, 3.0);
        let n: nalgebra::Vector3<f32> = v.into();
        let back: Vec3 = n.into();
        assert_eq!(v, back);
    }
}
