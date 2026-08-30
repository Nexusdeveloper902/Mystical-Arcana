//! `arcane_render` — the Arcane renderer.
//!
//! Backend-agnostic scene model + two render backends:
//! - [`cpu`] — software rasterizer (always available; works in any headless /
//!   GPU-less environment).
//! - [`vulkan`] — real Vulkan backend (built incrementally; first milestone:
//!   instance + layers + debug messenger + physical device + logical device +
//!   graphics queue + command pool).
//!
//! The simulation never reaches past the [`scene`] boundary. The renderer
//! never reaches past the [`scene`] boundary. This is the clean cut that lets
//! the game run with either backend.
//!
//! The [`observatory`] module exposes the renderer's current frame and
//! diagnostic state to a browser at `http://127.0.0.1:<port>/` for autonomous
//! visual feedback during development.

#![forbid(unsafe_op_in_unsafe_fn)]
#![warn(missing_docs)]

pub mod backend;
pub mod cpu;
pub mod extract;
pub mod metrics;
pub mod observatory;
pub mod png;
pub mod prereqs;
pub mod scene;
pub mod vulkan;

pub use backend::{Backend, BackendKind, FrameResult, RenderStatus};
pub use observatory::{CameraState, Diagnostic, Observatory, ObservatoryState,
                      PlayerState, RendererState, WorldState};
pub use prereqs::{look_at_vk, lerp_f32, linear_to_srgb, Mat4, MeshId, perspective_vk,
                  Quat, RenderError, RenderResult, srgb_to_linear, TextureId,
                  Transform, Vec2, Vec3, Vec4};
pub use scene::{Atmosphere, Camera, DirectionalLight, DrawCommand, Lights, Material,
                MaterialFlags, Mesh, MeshVertex, ParticleVertex, PointLight,
                RenderScene, UiDraw, UiVertex};

use parking_lot::RwLock;
use std::sync::Arc;

/// Top-level renderer owning the active backend and the latest frame.
pub struct Renderer {
    backend: RwLock<Box<dyn Backend>>,
    /// Most recent successfully rendered frame (PNG bytes for the observatory).
    last_frame: RwLock<Option<Arc<[u8]>>>,
    /// Most recent scene (for the /scene endpoint).
    last_scene: RwLock<Option<Arc<RenderScene>>>,
    /// Aggregated metrics.
    metrics: RwLock<metrics::Metrics>,
    /// Active backend kind.
    kind: RwLock<BackendKind>,
}

impl Renderer {
    /// Construct a renderer with a specific backend kind.
    pub fn new(kind: BackendKind, width: u32, height: u32) -> Self {
        let backend: Box<dyn Backend> = match kind {
            BackendKind::Cpu => Box::new(cpu::CpuBackend::new(width, height)),
            BackendKind::Vulkan { headless } => {
                Box::new(vulkan::VulkanBackend::new(headless, width, height))
            }
        };
        Self {
            backend: RwLock::new(backend),
            last_frame: RwLock::new(None),
            last_scene: RwLock::new(None),
            metrics: RwLock::new(metrics::Metrics::default()),
            kind: RwLock::new(kind),
        }
    }

    /// Render a scene, capture the framebuffer as PNG bytes.
    pub fn render(&self, scene: &RenderScene) -> FrameResult {
        let result = self.backend.write().render(scene);
        if let Some(png) = result.png_bytes.as_ref() {
            *self.last_frame.write() = Some(Arc::from(png.as_slice()));
        }
        *self.last_scene.write() = Some(Arc::new(scene.clone()));
        *self.metrics.write() = result.metrics.clone();
        result
    }

    /// Snapshot the last rendered PNG bytes (zero-copy).
    pub fn last_frame_png(&self) -> Option<Arc<[u8]>> {
        self.last_frame.read().clone()
    }

    /// Snapshot the last rendered scene.
    pub fn last_scene(&self) -> Option<Arc<RenderScene>> {
        self.last_scene.read().clone()
    }

    /// Current metrics snapshot.
    pub fn metrics_snapshot(&self) -> metrics::Metrics {
        self.metrics.read().clone()
    }

    /// Current backend kind.
    pub fn backend_kind(&self) -> BackendKind {
        *self.kind.read()
    }

    /// Resize the framebuffer.
    pub fn resize(&self, width: u32, height: u32) {
        self.backend.write().resize(width, height);
    }
}
