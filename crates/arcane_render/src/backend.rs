//! Backend trait shared by CPU & Vulkan renderers.

use crate::metrics::Metrics;
use crate::scene::RenderScene;

/// Identifies the backend implementation.
#[derive(Copy, Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum BackendKind {
    /// CPU software rasterizer.
    Cpu,
    /// Vulkan renderer. `headless=true` selects the offscreen path.
    Vulkan { headless: bool },
}

/// Result of a single render call.
#[derive(Clone, Debug, Default)]
pub struct FrameResult {
    /// PNG bytes of the final composited frame (RGBA, 8-bit sRGB).
    pub png_bytes: Option<Vec<u8>>,
    /// Metrics gathered for this frame.
    pub metrics: Metrics,
    /// Whether rendering succeeded or degraded gracefully.
    pub status: RenderStatus,
}

/// Status of a frame.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum RenderStatus {
    /// Default (unset).
    #[default]
    Pending,
    /// Successful render.
    Ok,
    /// Rendered, but using a degraded path (e.g. CPU backend fell back for a
    /// feature it cannot represent).
    Degraded,
    /// Failed entirely; the framebuffer is invalid.
    Failed,
}

/// A renderer backend.
pub trait Backend: Send + Sync {
    /// Render a frame. Returns PNG bytes when available.
    fn render(&mut self, scene: &RenderScene) -> FrameResult;
    /// Resize the framebuffer.
    fn resize(&mut self, width: u32, height: u32);
    /// Backend name (for `/state`).
    fn name(&self) -> &'static str;
    /// Whether the backend has a usable GPU surface (true for Vulkan, false
    /// for CPU).
    fn has_gpu(&self) -> bool {
        false
    }
    /// Current framebuffer dimensions.
    fn dimensions(&self) -> (u32, u32);
}
