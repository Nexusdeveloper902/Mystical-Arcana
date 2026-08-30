//! Per-frame metrics, observable through `/metrics` and `/state`.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Metrics {
    /// Active backend name.
    pub backend: String,
    /// Framebuffer width.
    pub width: u32,
    /// Framebuffer height.
    pub height: u32,
    /// Frame time, microseconds.
    pub frame_time_us: u64,
    /// Frames per second, smoothed.
    pub fps: f32,
    /// Number of draw calls submitted this frame.
    pub draw_calls: u32,
    /// Number of triangles submitted this frame.
    pub triangles: u64,
    /// Number of visible (post-cull) objects.
    pub visible_objects: u32,
    /// Loaded mesh count (cached).
    pub loaded_meshes: u32,
    /// Loaded texture count (cached).
    pub loaded_textures: u32,
    /// Active material count.
    pub active_materials: u32,
    /// GPU/Vulkan status (None for CPU backend).
    pub gpu_status: Option<GpuStatus>,
}

/// Vulkan-specific status, when applicable.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct GpuStatus {
    /// Device name.
    pub device_name: String,
    /// Driver version.
    pub driver_version: String,
    /// Api version.
    pub api_version: String,
    /// Validation layers enabled.
    pub validation_enabled: bool,
    /// Memory used (bytes).
    pub memory_used: u64,
    /// Memory budget (bytes).
    pub memory_budget: u64,
    /// Last Vulkan validation error count.
    pub validation_errors: u32,
}

impl Metrics {
    /// Merge another metrics snapshot into this one (used by aggregators).
    pub fn merge(&mut self, other: &Metrics) {
        self.draw_calls += other.draw_calls;
        self.triangles += other.triangles;
        self.visible_objects += other.visible_objects;
        self.loaded_meshes = self.loaded_meshes.max(other.loaded_meshes);
        self.loaded_textures = self.loaded_textures.max(other.loaded_textures);
        self.active_materials = self.active_materials.max(other.active_materials);
        self.frame_time_us = self.frame_time_us.max(other.frame_time_us);
    }
}
