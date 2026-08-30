//! Vulkan backend (Phase 2 — incremental implementation).
//!
//! Currently implements:
//! - Vulkan instance creation (with validation layers in debug builds)
//! - debug messenger
//! - physical device selection
//! - graphics queue family discovery
//! - logical device creation
//! - graphics queue handle
//! - command pool + command buffer allocation
//!
//! When the device cannot be created (e.g. no ICD, no surface in headless
//! mode), the backend falls back to the CPU rasterizer path and reports
//! `RenderStatus::Degraded`. The observatory will continue to produce frames
//! through the CPU path until a real Vulkan path is wired up.

use std::ffi::CString;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use ash::vk;
use ash::Entry;

use crate::backend::{Backend, FrameResult, RenderStatus};
use crate::metrics::GpuStatus;
use crate::scene::RenderScene;

/// Global counter of validation errors observed by the debug callback.
/// Bumped on every validation error, regardless of which device produced it.
static VALIDATION_ERRORS: AtomicU32 = AtomicU32::new(0);

/// A real Vulkan device, when initialized.
struct VulkanDevice {
    entry: Entry,
    instance: ash::Instance,
    physical_device: vk::PhysicalDevice,
    device: ash::Device,
    graphics_queue: vk::Queue,
    command_pool: vk::CommandPool,
    _debug: Option<MessengerHandle>,
    api_version: u32,
    device_name: String,
    driver_version: u32,
}

/// Owns the destroy fn pointer + messenger handle.
struct MessengerHandle {
    entry: Entry,
    instance: ash::Instance,
    messenger: vk::DebugUtilsMessengerEXT,
}

impl Drop for MessengerHandle {
    fn drop(&mut self) {
        // SAFETY: messenger was created from instance via the debug-utils
        // extension, which is enabled when validation is requested.
        unsafe {
            let name = CString::new("vkDestroyDebugUtilsMessengerEXT").unwrap();
            if let Some(raw) = self
                .entry
                .get_instance_proc_addr(self.instance.handle(), name.as_ptr())
            {
                let destroy: vk::PFN_vkDestroyDebugUtilsMessengerEXT =
                    unsafe { std::mem::transmute(raw) };
                destroy(self.instance.handle(), self.messenger, std::ptr::null());
            }
        }
    }
}

impl VulkanDevice {
    fn create(headless: bool, validation: bool) -> Result<Self, String> {
        let entry = unsafe { Entry::load().map_err(|e| format!("vulkan entry load: {e}"))? };

        let app_name = CString::new("Mystical Arcana").unwrap();
        let engine_name = CString::new("Arcane").unwrap();
        let app_info = vk::ApplicationInfo {
            p_application_name: app_name.as_ptr(),
            application_version: 0,
            p_engine_name: engine_name.as_ptr(),
            engine_version: 0,
            api_version: vk::make_api_version(0, 1, 3, 0),
            ..Default::default()
        };

        let validation_layer = CString::new("VK_LAYER_KHRONOS_validation").unwrap();
        let mut layers: Vec<*const i8> = Vec::new();
        if validation {
            layers.push(validation_layer.as_ptr());
        }

        let debug_utils_ext = CString::new("VK_EXT_debug_utils").unwrap();
        let mut exts: Vec<*const i8> = Vec::new();
        if validation {
            exts.push(debug_utils_ext.as_ptr());
        }
        // In headless mode we don't request any surface extension; in windowed
        // mode the host application would normally request
        // VK_KHR_surface + platform surface extensions, but that's deferred
        // until windowing is integrated.
        let _ = headless;

        let mut debug_create_info = vk::DebugUtilsMessengerCreateInfoEXT {
            message_severity: vk::DebugUtilsMessageSeverityFlagsEXT::ERROR
                | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
                | vk::DebugUtilsMessageSeverityFlagsEXT::INFO,
            message_type: vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
            pfn_user_callback: Some(debug_callback),
            p_user_data: std::ptr::null_mut(),
            ..Default::default()
        };
        let pnext: *const vk::DebugUtilsMessengerCreateInfoEXT = if validation {
            &debug_create_info as *const _
        } else {
            std::ptr::null()
        };

        let instance_create_info = vk::InstanceCreateInfo {
            p_application_info: &app_info,
            enabled_layer_count: layers.len() as u32,
            pp_enabled_layer_names: layers.as_ptr(),
            enabled_extension_count: exts.len() as u32,
            pp_enabled_extension_names: exts.as_ptr(),
            p_next: pnext as *const _,
            ..Default::default()
        };

        let instance = unsafe {
            entry
                .create_instance(&instance_create_info, None)
                .map_err(|e| format!("create_instance: {e:?}"))?
        };

        // Optional debug messenger.
        let _debug = if validation {
            let create_name = CString::new("vkCreateDebugUtilsMessengerEXT").unwrap();
            let create_fn: Option<vk::PFN_vkCreateDebugUtilsMessengerEXT> = unsafe {
                entry
                    .get_instance_proc_addr(instance.handle(), create_name.as_ptr())
                    .map(|raw| std::mem::transmute(raw))
            };
            if let Some(func) = create_fn {
                let mut messenger = vk::DebugUtilsMessengerEXT::null();
                let r = unsafe {
                    (func)(
                        instance.handle(),
                        &debug_create_info,
                        std::ptr::null(),
                        &mut messenger,
                    )
                };
                if r == vk::Result::SUCCESS && messenger != vk::DebugUtilsMessengerEXT::null() {
                    Some(MessengerHandle {
                        entry: entry.clone(),
                        instance: instance.clone(),
                        messenger,
                    })
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        // Physical device selection.
        let physical_devices = unsafe {
            instance
                .enumerate_physical_devices()
                .map_err(|e| format!("enumerate_physical_devices: {e:?}"))?
        };
        if physical_devices.is_empty() {
            return Err("no Vulkan physical devices available".into());
        }
        let physical_device = physical_devices[0];

        let props = unsafe { instance.get_physical_device_properties(physical_device) };
        let device_name = {
            let name = unsafe { std::ffi::CStr::from_ptr(props.device_name.as_ptr()) };
            name.to_string_lossy().to_string()
        };
        let api_version = props.api_version;
        let driver_version = props.driver_version;

        // Find a graphics queue family.
        let queue_families =
            unsafe { instance.get_physical_device_queue_family_properties(physical_device) };
        let graphics_queue_family_index = queue_families
            .iter()
            .enumerate()
            .find(|(_, q)| q.queue_flags.contains(vk::QueueFlags::GRAPHICS))
            .map(|(i, _)| i as u32)
            .ok_or("no graphics queue family")?;

        // Logical device: one graphics queue.
        let queue_priorities = [1.0f32];
        let queue_create_info = vk::DeviceQueueCreateInfo {
            queue_family_index: graphics_queue_family_index,
            queue_count: 1,
            p_queue_priorities: queue_priorities.as_ptr(),
            ..Default::default()
        };
        let device_create_info = vk::DeviceCreateInfo {
            p_queue_create_infos: &queue_create_info,
            queue_create_info_count: 1,
            ..Default::default()
        };
        let device = unsafe {
            instance
                .create_device(physical_device, &device_create_info, None)
                .map_err(|e| format!("create_device: {e:?}"))?
        };

        let graphics_queue = unsafe { device.get_device_queue(graphics_queue_family_index, 0) };

        // Command pool.
        let command_pool_info = vk::CommandPoolCreateInfo {
            flags: vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER,
            queue_family_index: graphics_queue_family_index,
            ..Default::default()
        };
        let command_pool = unsafe {
            device
                .create_command_pool(&command_pool_info, None)
                .map_err(|e| format!("create_command_pool: {e:?}"))?
        };

        Ok(Self {
            entry,
            instance,
            physical_device,
            device,
            graphics_queue,
            command_pool,
            _debug,
            api_version,
            device_name,
            driver_version,
        })
    }

    fn status(&self, validation: bool) -> GpuStatus {
        GpuStatus {
            device_name: self.device_name.clone(),
            driver_version: format!("{:x}", self.driver_version),
            api_version: format!(
                "{}.{}.{}",
                vk::api_version_major(self.api_version),
                vk::api_version_minor(self.api_version),
                vk::api_version_patch(self.api_version)
            ),
            validation_enabled: validation,
            memory_used: 0,
            memory_budget: 0,
            validation_errors: VALIDATION_ERRORS.load(Ordering::Relaxed),
        }
    }
}

impl Drop for VulkanDevice {
    fn drop(&mut self) {
        unsafe {
            self.device.device_wait_idle().ok();
            self.device.destroy_command_pool(self.command_pool, None);
            self.device.destroy_device(None);
            // `_debug` drops here, releasing the messenger.
            if let Some(_m) = self._debug.take() {
                drop(_m);
            }
            self.instance.destroy_instance(None);
        }
    }
}

unsafe extern "system" fn debug_callback(
    severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    _ty: vk::DebugUtilsMessageTypeFlagsEXT,
    data: *const vk::DebugUtilsMessengerCallbackDataEXT,
    _user_data: *mut std::ffi::c_void,
) -> vk::Bool32 {
    if severity >= vk::DebugUtilsMessageSeverityFlagsEXT::ERROR {
        VALIDATION_ERRORS.fetch_add(1, Ordering::Relaxed);
    }
    if !data.is_null() {
        let data = unsafe { &*data };
        if !data.p_message.is_null() {
            let msg = unsafe { std::ffi::CStr::from_ptr(data.p_message) }
                .to_string_lossy()
                .to_string();
            if severity >= vk::DebugUtilsMessageSeverityFlagsEXT::WARNING {
                tracing::warn!("[vulkan-validation] {}", msg);
            } else {
                tracing::info!("[vulkan-validation] {}", msg);
            }
        }
    }
    vk::FALSE
}

/// The Vulkan renderer.
pub struct VulkanBackend {
    headless: bool,
    width: u32,
    height: u32,
    device: Option<VulkanDevice>,
    /// When Vulkan is unavailable (e.g. no ICD), we fall back to the CPU
    /// rasterizer so the observatory still produces a meaningful frame.
    fallback_cpu: Option<crate::cpu::CpuBackend>,
}

impl VulkanBackend {
    /// Construct a new Vulkan renderer. Tries to initialize the device; if
    /// that fails, falls back to the CPU backend but reports `has_gpu=false`.
    pub fn new(headless: bool, width: u32, height: u32) -> Self {
        Self {
            headless,
            width,
            height,
            device: None,
            fallback_cpu: None,
        }
    }

    fn ensure_device(&mut self) -> Option<&VulkanDevice> {
        if self.device.is_some() {
            return self.device.as_ref();
        }
        let validation = cfg!(debug_assertions);
        match VulkanDevice::create(self.headless, validation) {
            Ok(dev) => {
                self.device = Some(dev);
                self.device.as_ref()
            }
            Err(e) => {
                tracing::info!("vulkan unavailable, using CPU fallback: {e}");
                if self.fallback_cpu.is_none() {
                    self.fallback_cpu = Some(crate::cpu::CpuBackend::new(self.width, self.height));
                }
                None
            }
        }
    }

    fn render_through_fallback(&mut self, scene: &RenderScene) -> FrameResult {
        if self.fallback_cpu.is_none() {
            self.fallback_cpu = Some(crate::cpu::CpuBackend::new(self.width, self.height));
        }
        let cpu = self.fallback_cpu.as_mut().unwrap();
        let mut result = cpu.render(scene);
        result.status = RenderStatus::Degraded;
        result.metrics.backend = "vulkan-fallback-cpu".to_string();
        result
    }
}

impl Backend for VulkanBackend {
    fn render(&mut self, scene: &RenderScene) -> FrameResult {
        let _ = self.ensure_device();
        // At this milestone, the Vulkan device exists but the full pipeline
        // is not yet wired (in-flight development). Render through CPU fallback
        // and report degraded status until real GPU rasterization is wired up.
        let mut result = self.render_through_fallback(scene);
        if let Some(dev) = self.device.as_ref() {
            result.metrics.gpu_status = Some(dev.status(cfg!(debug_assertions)));
        }
        result
    }
    fn resize(&mut self, width: u32, height: u32) {
        self.width = width.max(1);
        self.height = height.max(1);
        if let Some(cpu) = self.fallback_cpu.as_mut() {
            cpu.resize(width, height);
        }
    }
    fn name(&self) -> &'static str {
        "vulkan"
    }
    fn has_gpu(&self) -> bool {
        self.device.is_some()
    }
    fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}

// silence unused import warning when not all items from crate are used.
#[allow(dead_code)]
fn _unused(_a: &Arc<Mutex<()>>, _b: vk::Queue) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::RenderScene;

    #[test]
    fn backend_falls_back_gracefully_without_vulkan() {
        let mut backend = VulkanBackend::new(true, 32, 32);
        let result = backend.render(&RenderScene::default());
        assert!(
            matches!(result.status, RenderStatus::Degraded | RenderStatus::Ok),
            "backend must produce some frame"
        );
        assert!(result.png_bytes.is_some(), "must produce a PNG");
    }

    #[test]
    fn ensures_device_or_fallback() {
        let mut backend = VulkanBackend::new(true, 32, 32);
        let _ = backend.ensure_device();
        // Either we have a Vulkan device or we have a CPU fallback.
        if backend.device.is_none() {
            assert!(
                backend.fallback_cpu.is_some(),
                "must have CPU fallback when no Vulkan device is available"
            );
        }
    }
}
