//! Minimal math primitives shared across the engine.
//!
//! We keep this thin on purpose: most linear algebra goes through `glam`,
//! and the engine layer adds the few domain-specific helpers it needs.

pub use glam::*;

pub type Vec3f = Vec3;
pub type Vec4f = Vec4;
pub type Mat4f = Mat4;
pub type QuatF = Quat;

/// Color in linear float RGBA, 0..1
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const BLACK: Self = Self { r: 0.0, g: 0.0, b: 0.0, a: 1.0 };
    pub const WHITE: Self = Self { r: 1.0, g: 1.0, b: 1.0, a: 1.0 };
    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b, a: 1.0 }
    }
    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }
    pub fn to_array(self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }
}

/// Axis-aligned bounding box.
#[derive(Clone, Debug, Default)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    pub fn new(min: Vec3, max: Vec3) -> Self { Self { min, max } }
    pub fn empty() -> Self {
        Self {
            min: Vec3::splat(f32::INFINITY),
            max: Vec3::splat(f32::NEG_INFINITY),
        }
    }
    pub fn extend(&mut self, p: Vec3) {
        self.min = self.min.min(p);
        self.max = self.max.max(p);
    }
    pub fn center(&self) -> Vec3 {
        (self.min + self.max) * 0.5
    }
    pub fn size(&self) -> Vec3 {
        self.max - self.min
    }
}

pub fn perspective(fovy: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    Mat4::perspective_rh(fovy, aspect, near, far)
}

pub fn look_at(eye: Vec3, center: Vec3, up: Vec3) -> Mat4 {
    Mat4::look_at_rh(eye, center, up)
}

/// Vulkan-style perspective projection (right-handed view space, depth
/// range [0, 1] in NDC, Y points down on screen).
///
/// This differs from `glam::Mat4::perspective_rh` (which uses the OpenGL
/// depth range [-1, +1]). The matrix maps view-space z = -near to NDC z = 0
/// (the near plane) and view-space z = -far to NDC z = 1 (the far plane).
///
/// Column-major layout matches GLSL `mat4` so the result can be uploaded
/// directly as a 64-byte push constant.
pub fn vulkan_perspective(fovy: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    let f = 1.0 / (fovy / 2.0).tan();
    let m22 = far / (near - far);
    let m32 = -1.0;
    let m23 = -(far * near) / (far - near);
    // Column-major: 4 columns of 4 elements each.
    Mat4::from_cols_array(&[
        f / aspect, 0.0, 0.0, 0.0,
        0.0,        f,   0.0, 0.0,
        0.0,        0.0, m22, m32,
        0.0,        0.0, m23, 0.0,
    ])
}

/// Convert a glam Mat4 to a column-major `[f32; 16]` for upload as a push
/// constant or uniform buffer. This matches GLSL's `mat4` layout so the
/// shader can read the matrix without any transpose flag.
pub fn mat4_to_cols_array(m: Mat4) -> [f32; 16] {
    m.to_cols_array()
}
