//! Local prerequisites for the renderer.
//!
//! The Arcane renderer is intentionally self-contained: it depends on
//! `nalgebra` for linear algebra and `serde` for serialization, but does NOT
//! depend on the engine's `arcane_core` / `arcane_math` crates (which are
//! still evolving their APIs). This module defines the small subset of
//! primitives the renderer needs.

use serde::{Deserialize, Serialize};

/// Renderer-specific error type. Strings only — no complex error chaining
/// (the renderer doesn't depend on `arcane_core`'s error type).
#[derive(thiserror::Error, Debug)]
pub enum RenderError {
    /// Asset / I/O failure.
    #[error("render asset error: {0}")]
    Asset(String),
    /// PNG encoding failure.
    #[error("png encode error: {0}")]
    Png(String),
    /// Vulkan failure.
    #[error("vulkan error: {0}")]
    Vulkan(String),
    /// Generic.
    #[error("{0}")]
    Msg(String),
}

impl From<std::io::Error> for RenderError {
    fn from(value: std::io::Error) -> Self {
        Self::Asset(value.to_string())
    }
}

/// Renderer result.
pub type RenderResult<T> = std::result::Result<T, RenderError>;

/// 4×4 column-major matrix suitable for direct upload to GPU uniforms.
pub type Mat4 = nalgebra::Matrix4<f32>;
/// 3-component vector.
pub type Vec3 = nalgebra::Vector3<f32>;
/// 4-component vector.
pub type Vec4 = nalgebra::Vector4<f32>;
/// 2-component vector.
pub type Vec2 = nalgebra::Vector2<f32>;
/// Quaternion.
pub type Quat = nalgebra::UnitQuaternion<f32>;

/// Build a right-handed perspective projection matrix suitable for Vulkan
/// (depth range `[0, 1]`, Y-down NDC).
///
/// Maps view-space `z ∈ [-near, -far]` to NDC `z ∈ [0, 1]` (and w = -z_view).
pub fn perspective_vk(fov_y_radians: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    let f = 1.0 / (fov_y_radians * 0.5).tan();
    let range = far - near;
    Mat4::new(
        f / aspect, 0.0, 0.0,           0.0,
        0.0,        -f,  0.0,           0.0, // negative Y for Vulkan NDC
        0.0,        0.0, -far / range,  -far * near / range,
        0.0,        0.0, -1.0,          0.0,
    )
}

/// Build an orthonormal view matrix ("look-at") in Vulkan's right-handed
/// convention. `eye` is the camera position, `target` the focus point, `up`
/// the up direction.
pub fn look_at_vk(eye: Vec3, target: Vec3, up: Vec3) -> Mat4 {
    nalgebra::Matrix4::look_at_rh(
        &nalgebra::Point3::from(eye),
        &nalgebra::Point3::from(target),
        &up)
}

/// sRGB -> linear-light conversion.
pub fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

/// linear-light -> sRGB conversion.
pub fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.0031308 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
}

/// Linear lerp.
pub fn lerp_f32(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Compact transform: position + rotation (quaternion) + scale.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Transform {
    /// Translation.
    pub position: Vec3,
    /// Rotation as unit quaternion.
    pub rotation: Quat,
    /// Non-uniform scale.
    pub scale: Vec3,
}

impl Default for Transform {
    fn default() -> Self { Self::identity() }
}

impl Transform {
    /// Identity transform.
    pub fn identity() -> Self {
        Self {
            position: Vec3::zeros(),
            rotation: Quat::identity(),
            scale: Vec3::new(1.0, 1.0, 1.0),
        }
    }

    /// Construct from position only.
    pub fn from_position(position: Vec3) -> Self {
        Self { position, ..Self::identity() }
    }

    /// Build the local-to-world matrix.
    pub fn to_matrix(&self) -> Mat4 {
        let t = Mat4::new_translation(&self.position);
        let r = self.rotation.to_homogeneous();
        let s = Mat4::new_nonuniform_scaling(&self.scale);
        t * r * s
    }
}

/// FNV-1a 64-bit hash of a byte slice with an additional salt.
fn fnv1a_salted(bytes: &[u8], salt: u64) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325 ^ salt;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// Stable hashed texture identifier.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TextureId(pub u64);

impl TextureId {
    /// Sentinel "null" id.
    pub const NULL: Self = Self(0);
    /// Construct from a stable hash of a string key.
    pub fn from_str(s: &str) -> Self {
        Self(fnv1a_salted(s.as_bytes(), 0x7e5_7475))
    }
    /// Is null?
    pub fn is_null(self) -> bool { self.0 == 0 }
}

impl Default for TextureId { fn default() -> Self { Self::NULL } }

/// Stable hashed mesh identifier.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MeshId(pub u64);

impl MeshId {
    /// Sentinel "null" id.
    pub const NULL: Self = Self(0);
    /// Construct from a stable hash of a string key.
    pub fn from_str(s: &str) -> Self {
        Self(fnv1a_salted(s.as_bytes(), 0x6d6f_7468))
    }
}

impl Default for MeshId { fn default() -> Self { Self::NULL } }
