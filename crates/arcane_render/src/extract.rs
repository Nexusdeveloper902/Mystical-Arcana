//! Render extraction — bridges gameplay/simulation state to the renderer's
//! `RenderScene`.
//!
//! The simulation owns its own state; this module performs a pure copy of
//! relevant view into the render-scene format. Two flavors are provided:
//!
//! 1. **Direct builder**: the caller assembles a `RenderScene` by pushing
//!    `Renderable`, `SunLightSpec`, `PointLightSpec`, `ActiveCamera` values
//!    into a `SceneBuilder`. This is the canonical path the game uses.
//!
//! 2. **Trait-based extraction**: for engines that already have an ECS, the
//!    `RenderExtract` trait is implemented on the world type. This is left
//!    as a stub today; the engine's ECS is still being integrated.

use crate::prereqs::{Mat4, Vec3, Vec4};
use crate::prereqs::Transform;
use crate::scene::{Atmosphere, Camera, DirectionalLight, DrawCommand, Lights, Material,
                   Mesh, PointLight, RenderScene};

/// A renderable: mesh + material + transform.
#[derive(Clone, Debug)]
pub struct Renderable {
    /// Mesh data (vertices + indices + texture).
    pub mesh: Mesh,
    /// Material.
    pub material: Material,
    /// World transform.
    pub transform: Transform,
}

impl Default for Renderable {
    fn default() -> Self {
        Self {
            mesh: Mesh::default(),
            material: Material::default(),
            transform: Transform::identity(),
        }
    }
}

/// Sun / directional light spec used for extraction.
#[derive(Clone, Debug, Default)]
pub struct SunLightSpec {
    /// Direction TO the sun (normalized).
    pub direction: [f32; 3],
    /// Color * intensity (linear).
    pub color: [f32; 3],
    /// Ambient upper hemisphere (linear).
    pub ambient_up: [f32; 4],
    /// Ambient lower hemisphere (linear).
    pub ambient_down: [f32; 4],
}

/// Active camera spec.
#[derive(Clone, Debug)]
pub struct ActiveCameraSpec {
    /// Position.
    pub position: [f32; 3],
    /// Look target.
    pub target: [f32; 3],
    /// Up vector.
    pub up: [f32; 3],
    /// Vertical FOV (radians).
    pub fov_y: f32,
    /// Aspect ratio (width/height).
    pub aspect: f32,
    /// Near plane.
    pub near: f32,
    /// Far plane.
    pub far: f32,
}

impl Default for ActiveCameraSpec {
    fn default() -> Self {
        Self {
            position: [0.0, 5.0, 10.0],
            target: [0.0, 0.0, 0.0],
            up: [0.0, 1.0, 0.0],
            fov_y: std::f32::consts::FRAC_PI_3,
            aspect: 16.0 / 9.0,
            near: 0.1,
            far: 1000.0,
        }
    }
}

/// Point light spec.
#[derive(Clone, Debug, Default)]
pub struct PointLightSpec {
    /// Position.
    pub position: [f32; 3],
    /// Color * intensity.
    pub color: [f32; 3],
    /// Effective range (meters).
    pub range: f32,
}

/// Builder for assembling a `RenderScene` from gameplay state.
pub struct SceneBuilder {
    scene: RenderScene,
}

impl Default for SceneBuilder { fn default() -> Self { Self::new() } }

impl SceneBuilder {
    /// New empty builder.
    pub fn new() -> Self {
        Self { scene: RenderScene::default() }
    }

    /// Set the clear color.
    pub fn clear_color(mut self, c: [f32; 4]) -> Self {
        self.scene.clear_color = c;
        self
    }

    /// Set the atmosphere.
    pub fn atmosphere(mut self, a: Atmosphere) -> Self {
        self.scene.atmosphere = a;
        self
    }

    /// Set the active camera.
    pub fn camera(mut self, cam: ActiveCameraSpec) -> Self {
        self.scene.camera = Camera {
            position: cam.position,
            target: cam.target,
            up: cam.up,
            fov_y: cam.fov_y,
            aspect: cam.aspect,
            near: cam.near,
            far: cam.far,
        };
        self
    }

    /// Set the sun.
    pub fn sun(mut self, sun: SunLightSpec) -> Self {
        self.scene.lights.sun = Some(DirectionalLight {
            direction: sun.direction,
            color: sun.color,
        });
        self.scene.lights.ambient_up = sun.ambient_up;
        self.scene.lights.ambient_down = sun.ambient_down;
        self
    }

    /// Add a renderable.
    pub fn push(mut self, r: Renderable) -> Self {
        self.scene.commands.push(DrawCommand {
            mesh: r.mesh,
            material: r.material,
            transform: r.transform,
        });
        self
    }

    /// Add a point light.
    pub fn point_light(mut self, p: PointLightSpec) -> Self {
        self.scene.lights.points.push(PointLight {
            position: p.position,
            color: p.color,
            range: p.range,
        });
        self
    }

    /// Add particles.
    pub fn particles(mut self, p: Vec<crate::scene::ParticleVertex>) -> Self {
        self.scene.particles = p;
        self
    }

    /// Add UI draws.
    pub fn ui(mut self, ui: Vec<crate::scene::UiDraw>) -> Self {
        self.scene.ui = ui;
        self
    }

    /// Build the final scene.
    pub fn build(self) -> RenderScene {
        self.scene
    }
}

/// Trait: extract a `RenderScene` from any simulation-side world type.
///
/// The engine's `arcane_ecs` crate is still being implemented; in the meantime
/// the renderer accepts `RenderScene` directly. Implementations of this trait
/// are optional and live in the engine crate that owns the world type.
pub trait RenderExtract {
    /// Extract a renderable scene.
    fn extract_scene(&self) -> RenderScene;
}

// Suppress unused-import warning for types used by docs / future code.
#[allow(dead_code)]
fn _unused(_: &Mat4, _: &Vec3, _: &Vec4, _: &Mesh, _: &Lights, _: &Material) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prereqs::Transform;
    use crate::scene::{Mesh, Material};

    #[test]
    fn builder_assembles_scene() {
        let scene = SceneBuilder::new()
            .clear_color([0.10, 0.12, 0.18, 1.0])
            .camera(ActiveCameraSpec {
                position: [1.0, 2.0, 3.0],
                ..Default::default()
            })
            .sun(SunLightSpec {
                direction: [0.0, -1.0, 0.0],
                color: [1.0, 1.0, 0.9],
                ambient_up: [0.05, 0.05, 0.07, 1.0],
                ambient_down: [0.0, 0.0, 0.0, 0.0],
            })
            .push(Renderable {
                mesh: Mesh::unit_cube(),
                material: Material { base_color: [0.5, 0.3, 0.2, 1.0], ..Default::default() },
                transform: Transform::identity(),
            })
            .build();
        assert_eq!(scene.camera.position, [1.0, 2.0, 3.0]);
        assert!(scene.lights.sun.is_some());
        assert_eq!(scene.commands.len(), 1);
    }
}
