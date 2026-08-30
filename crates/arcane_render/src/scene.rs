//! Backend-agnostic render-world types.
//!
//! The simulation produces these (via `extract`), and the renderer consumes
//! them. This is the "clean cut" between gameplay state and rendering.

use serde::{Deserialize, Serialize};

use crate::prereqs::{Mat4, Vec3, Vec4};
use crate::prereqs::{MeshId, TextureId, Transform};

/// A complete renderable frame.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RenderScene {
    /// Camera used to render this frame.
    pub camera: Camera,
    /// Mesh draw commands (sorted back-to-front in CPU; front-to-back opaque
    /// in GPU backend with depth test).
    pub commands: Vec<DrawCommand>,
    /// Global lights (sun + sky).
    pub lights: Lights,
    /// Particles.
    pub particles: Vec<ParticleVertex>,
    /// 2D HUD/UI draw commands (in screen-space).
    pub ui: Vec<UiDraw>,
    /// Background color used if sky isn't rendered (e.g. CPU backend default).
    pub clear_color: [f32; 4],
    /// Atmosphere / fog settings.
    pub atmosphere: Atmosphere,
}

/// Atmosphere parameters.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Atmosphere {
    /// Sky zenith color (top of the sky).
    pub sky_zenith: [f32; 4],
    /// Sky horizon color.
    pub sky_horizon: [f32; 4],
    /// Fog color (linear-light, used as fallback when no atmosphere shader).
    pub fog_color: [f32; 4],
    /// Fog density multiplier.
    pub fog_density: f32,
    /// Fog height falloff (for exponential height fog).
    pub fog_height_falloff: f32,
}

impl Default for Atmosphere {
    fn default() -> Self {
        Self {
            sky_zenith: [0.05, 0.07, 0.15, 1.0],
            sky_horizon: [0.30, 0.36, 0.48, 1.0],
            fog_color: [0.25, 0.28, 0.34, 1.0],
            fog_density: 0.0015,
            fog_height_falloff: 0.05,
        }
    }
}

/// Global lighting setup.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Lights {
    /// Directional / sun light (the only mandatory light).
    pub sun: Option<DirectionalLight>,
    /// Simple point lights for mana nodes & spells.
    pub points: Vec<PointLight>,
    /// Ambient hemispheric fill color (upper hemisphere).
    pub ambient_up: [f32; 4],
    /// Ambient hemispheric fill color (lower hemisphere).
    pub ambient_down: [f32; 4],
}

/// Directional / sun light.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DirectionalLight {
    /// World-space direction TO the light (normalized).
    pub direction: [f32; 3],
    /// Linear color * intensity.
    pub color: [f32; 3],
}

/// Point light.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PointLight {
    /// World-space position.
    pub position: [f32; 3],
    /// Linear color * intensity.
    pub color: [f32; 3],
    /// Effective range (meters).
    pub range: f32,
}

/// Camera used by both backends. World-to-clip derived from this.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Camera {
    /// World-space position.
    pub position: [f32; 3],
    /// Look target.
    pub target: [f32; 3],
    /// Up vector.
    pub up: [f32; 3],
    /// Vertical field of view, in radians.
    pub fov_y: f32,
    /// Aspect ratio (width / height).
    pub aspect: f32,
    /// Near plane.
    pub near: f32,
    /// Far plane.
    pub far: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            position: [0.0, 5.0, 10.0],
            target: [0.0, 0.0, 0.0],
            up: [0.0, 1.0, 0.0],
            fov_y: std::f32::consts::FRAC_PI_3, // 60°
            aspect: 16.0 / 9.0,
            near: 0.1,
            far: 1000.0,
        }
    }
}

impl Camera {
    /// View (world → view) matrix.
    pub fn view_matrix(&self) -> Mat4 {
        crate::prereqs::look_at_vk(
            Vec3::from(self.position),
            Vec3::from(self.target),
            Vec3::from(self.up),
        )
    }

    /// Projection (view → clip) matrix.
    pub fn projection_matrix(&self) -> Mat4 {
        crate::prereqs::perspective_vk(self.fov_y, self.aspect, self.near, self.far)
    }

    /// Combined view-projection matrix.
    pub fn view_projection(&self) -> Mat4 {
        self.projection_matrix() * self.view_matrix()
    }
}

/// A textured mesh vertex.
#[repr(C)]
#[derive(
    Copy, Clone, Debug, Default, bytemuck::Zeroable, bytemuck::Pod, Serialize, Deserialize,
)]
pub struct MeshVertex {
    /// Position (object space).
    pub position: [f32; 3],
    /// Normal (object space, normalized).
    pub normal: [f32; 3],
    /// Texture coordinate (0..1, Y-down convention).
    pub texcoord: [f32; 2],
}

/// A particle vertex.
#[repr(C)]
#[derive(
    Copy, Clone, Debug, Default, bytemuck::Zeroable, bytemuck::Pod, Serialize, Deserialize,
)]
pub struct ParticleVertex {
    /// World-space position.
    pub position: [f32; 3],
    /// Color (linear).
    pub color: [f32; 4],
    /// World-space size (meters).
    pub size: f32,
}

/// A renderable mesh: vertices + indices + material + transform.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Mesh {
    /// Optional stable id (for diagnostics / GPU-resource caching).
    pub id: MeshId,
    /// Vertices.
    pub vertices: Vec<MeshVertex>,
    /// Indices (u32).
    pub indices: Vec<u32>,
    /// Optional texture id.
    pub texture: Option<TextureId>,
}

impl Default for Mesh {
    fn default() -> Self {
        Self {
            id: MeshId::NULL,
            vertices: Vec::new(),
            indices: Vec::new(),
            texture: None,
        }
    }
}

impl Mesh {
    /// Build a unit cube centered at the origin. Useful for tests.
    ///
    /// Vertex winding is set so that triangles appear counter-clockwise when
    /// viewed from outside the cube — this corresponds to negative screen-space
    /// area in the Y-down Vulkan convention, so backface culling (which rejects
    /// positive screen-space area when not double-sided) keeps them visible.
    pub fn unit_cube() -> Self {
        let positions = [
            // +Z face (front)
            [-1.0, -1.0, 1.0],
            [1.0, -1.0, 1.0],
            [1.0, 1.0, 1.0],
            [-1.0, 1.0, 1.0],
            // -Z face (back, winding reversed for outward normal)
            [1.0, -1.0, -1.0],
            [-1.0, -1.0, -1.0],
            [-1.0, 1.0, -1.0],
            [1.0, 1.0, -1.0],
            // -Y face (bottom, winding reversed)
            [-1.0, -1.0, -1.0],
            [1.0, -1.0, -1.0],
            [1.0, -1.0, 1.0],
            [-1.0, -1.0, 1.0],
            // +Y face (top, winding reversed)
            [1.0, 1.0, -1.0],
            [-1.0, 1.0, -1.0],
            [-1.0, 1.0, 1.0],
            [1.0, 1.0, 1.0],
            // +X face (right, winding reversed)
            [1.0, -1.0, 1.0],
            [1.0, -1.0, -1.0],
            [1.0, 1.0, -1.0],
            [1.0, 1.0, 1.0],
            // -X face (left, winding reversed)
            [-1.0, -1.0, -1.0],
            [-1.0, -1.0, 1.0],
            [-1.0, 1.0, 1.0],
            [-1.0, 1.0, -1.0],
        ];
        let normals_per_face = [
            [0.0, 0.0, 1.0],
            [0.0, 0.0, -1.0],
            [0.0, -1.0, 0.0],
            [0.0, 1.0, 0.0],
            [1.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0],
        ];
        let uvs = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        let mut vertices = Vec::with_capacity(24);
        for face in 0..6 {
            for v in 0..4 {
                let p = positions[face * 4 + v];
                vertices.push(MeshVertex {
                    position: p,
                    normal: normals_per_face[face],
                    texcoord: uvs[v],
                });
            }
        }
        // Triangle winding: for a CCW-from-outside face, screen-space area is
        // negative in Y-down (Vulkan) — so we generate [0, 1, 2] triangles that
        // preserve CCW-from-outside winding in world space.
        let indices: Vec<u32> = (0..6)
            .flat_map(|f| {
                let base = (f * 4) as u32;
                [base, base + 1, base + 2, base, base + 2, base + 3]
            })
            .collect();
        Self {
            id: MeshId::from_str("builtin/cube"),
            vertices,
            indices,
            texture: None,
        }
    }

    /// Build a ground quad on the XZ plane (size × size), centered at origin.
    pub fn ground_quad(size: f32) -> Self {
        let h = size * 0.5;
        let vertices = vec![
            MeshVertex {
                position: [-h, 0.0, -h],
                normal: [0.0, 1.0, 0.0],
                texcoord: [0.0, 0.0],
            },
            MeshVertex {
                position: [h, 0.0, -h],
                normal: [0.0, 1.0, 0.0],
                texcoord: [1.0, 0.0],
            },
            MeshVertex {
                position: [h, 0.0, h],
                normal: [0.0, 1.0, 0.0],
                texcoord: [1.0, 1.0],
            },
            MeshVertex {
                position: [-h, 0.0, h],
                normal: [0.0, 1.0, 0.0],
                texcoord: [0.0, 1.0],
            },
        ];
        let indices = vec![0, 1, 2, 0, 2, 3];
        Self {
            id: MeshId::from_str("builtin/ground_quad"),
            vertices,
            indices,
            texture: None,
        }
    }
}

/// GPU-friendly material. Subset of the full material spec; the CPU backend
/// honors the same fields where practical.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Material {
    /// Linear base color (multiplied with texture if any).
    pub base_color: [f32; 4],
    /// Roughness 0 (mirror) .. 1 (diffuse).
    pub roughness: f32,
    /// Metallic 0..1.
    pub metallic: f32,
    /// Emissive color * intensity (linear).
    pub emissive: [f32; 3],
    /// Texture handle for the diffuse albedo.
    pub base_color_texture: Option<TextureId>,
    /// Bitfield: transparent | double_sided | unlit | cast_shadow
    pub flags: u32,
}

bitflags::bitflags! {
    /// Material flags consumed by both backends.
    #[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
    pub struct MaterialFlags: u32 {
        /// Blend transparent.
        const TRANSPARENT   = 0x01;
        /// Don't cull back-faces.
        const DOUBLE_SIDED  = 0x02;
        /// Skip lighting.
        const UNLIT         = 0x04;
        /// Don't cast shadows.
        const NO_SHADOW     = 0x08;
    }
}

/// One draw call: mesh + material + transform.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DrawCommand {
    /// Mesh (embedded; the GPU backend caches by `MeshId`).
    pub mesh: Mesh,
    /// Material.
    pub material: Material,
    /// World transform.
    pub transform: Transform,
}

/// 2D UI draw. Vertices are in screen-space pixels (top-left origin).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UiDraw {
    /// 2D triangles (positions + colors + UVs interleaved).
    pub vertices: Vec<UiVertex>,
    /// Triangle indices into `vertices`.
    pub indices: Vec<u32>,
    /// Optional texture (rectangular). If none, treat as colored quads.
    pub texture: Option<TextureId>,
}

/// UI vertex.
#[repr(C)]
#[derive(
    Copy, Clone, Debug, Default, bytemuck::Zeroable, bytemuck::Pod, Serialize, Deserialize,
)]
pub struct UiVertex {
    /// Position in pixel space (top-left origin).
    pub position: [f32; 2],
    /// Color (linear, premultiplied).
    pub color: [f32; 4],
    /// Texture coordinate.
    pub texcoord: [f32; 2],
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cube_has_indices() {
        let c = Mesh::unit_cube();
        assert_eq!(c.vertices.len(), 24);
        assert_eq!(c.indices.len(), 36);
    }

    #[test]
    fn camera_view_origin() {
        let c = Camera {
            position: [0., 0., 5.],
            target: [0., 0., 0.],
            up: [0., 1., 0.],
            ..Default::default()
        };
        let v = c.view_matrix();
        let p = v * Vec4::new(0.0, 0.0, 5.0, 1.0);
        assert!(p.xyz().norm() < 1e-3);
    }
}
