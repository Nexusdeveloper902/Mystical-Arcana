//! Arcane engine math primitives.
//!
//! Built on `nalgebra` for the heavy lifting (matrix decomposition, SIMD
//! intrinsics). This module adds:
//!
//! - Strongly-typed aliases for the dimensions used throughout the engine.
//! - Color space helpers for the stylized-magical visual identity.
//! - AABB and frustum culling primitives.
//! - Small extra utilities that map cleanly to GPU shader uniforms.
//!
//! All types implement `bytemuck::Pod`/`Zeroable` so they can be uploaded
//! to GPU uniform/storage buffers without copy.

#![forbid(unsafe_op_in_unsafe_fn)]
#![warn(missing_docs)]

pub mod aabb;
pub mod color;
pub mod frustum;
pub mod ids;
pub mod vec;

pub use aabb::{Aabb, Aabb2d, Aabb3d};
pub use color::{Color, ColorLinear, Rgba, Rgba8};
pub use frustum::{Frustum, FrustumPlane};
pub use vec::{Vec2, Vec3, Vec4};

/// 4x4 column-major matrix, suitable for direct upload to GPU uniforms.
pub type Mat4 = nalgebra::Matrix4<f32>;
/// 3x3 matrix.
pub type Mat3 = nalgebra::Matrix3<f32>;
/// Quaternion (x, y, z, w).
pub type Quat = nalgebra::Quaternion<f32>;
/// 4x4 view-projection matrix.
pub type ViewProj = nalgebra::Matrix4<f32>;

/// Pi constant.
pub const PI: f32 = std::f32::consts::PI;
/// 2*pi.
pub const TAU: f32 = std::f32::consts::TAU;
