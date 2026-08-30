//! Vulkan backend (Phase 2 — incremental implementation).
//!
//! Implements:
//! - Vulkan instance creation (with validation layers in debug builds)
//! - debug messenger (with validation-error counting for /metrics)
//! - physical device selection
//! - graphics queue family discovery
//! - logical device creation
//! - graphics queue handle
//! - command pool + command buffer allocation
//! - **NEW**: offscreen color attachment (8-bit sRGB RGBA) + depth attachment
//! - **NEW**: render pass with one color + one depth attachment
//! - **NEW**: pipeline barrier helpers (image layout transitions)
//! - **NEW**: command buffer recording that clears the color attachment
//!   to the scene's clear color and presents the result through a host-visible
//!   readback buffer (via vkQueueWaitIdle + vkCmdCopyImageToBuffer).
//!
//! When the device cannot be created (e.g. no ICD, no surface in headless
//! mode), the backend falls back to the CPU rasterizer path and reports
//! `RenderStatus::Degraded`. The observatory continues to produce frames
//! through the CPU path until a real GPU rasterization pipeline is fully
//! wired up.

use std::ffi::CString;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Instant;

use ash::vk;
use ash::Entry;

use crate::backend::{Backend, FrameResult, RenderStatus};
use crate::metrics::GpuStatus;
use crate::scene::RenderScene;

/// Global counter of validation errors observed by the debug callback.
static VALIDATION_ERRORS: AtomicU32 = AtomicU32::new(0);

/// A real Vulkan device, when initialized.
struct VulkanDevice {
    entry: Entry,
    instance: ash::Instance,
    physical_device: vk::PhysicalDevice,
    device: ash::Device,
    graphics_queue: vk::Queue,
    graphics_queue_family_index: u32,
    command_pool: vk::CommandPool,
    /// Memory properties of the chosen physical device (used for staging
    /// buffer allocation and image memory allocation).
    memory_properties: vk::PhysicalDeviceMemoryProperties,
    /// Optional debug messenger handle (cleaned up on Drop).
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
        unsafe {
            let name = CString::new("vkDestroyDebugUtilsMessengerEXT").unwrap();
            if let Some(raw) = self.entry.get_instance_proc_addr(self.instance.handle(), name.as_ptr()) {
                let destroy: vk::PFN_vkDestroyDebugUtilsMessengerEXT =
                    std::mem::transmute(raw);
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
            entry.create_instance(&instance_create_info, None)
                .map_err(|e| format!("create_instance: {e:?}"))?
        };

        // Optional debug messenger.
        let _debug = if validation {
            let create_name = CString::new("vkCreateDebugUtilsMessengerEXT").unwrap();
            let create_fn: Option<vk::PFN_vkCreateDebugUtilsMessengerEXT> = unsafe {
                entry.get_instance_proc_addr(instance.handle(), create_name.as_ptr())
                    .map(|raw| std::mem::transmute(raw))
            };
            if let Some(func) = create_fn {
                let mut messenger = vk::DebugUtilsMessengerEXT::null();
                let r = unsafe {
                    (func)(instance.handle(), &debug_create_info, std::ptr::null(), &mut messenger)
                };
                if r == vk::Result::SUCCESS && messenger != vk::DebugUtilsMessengerEXT::null() {
                    Some(MessengerHandle { entry: entry.clone(), instance: instance.clone(), messenger })
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
            instance.enumerate_physical_devices()
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
        let memory_properties = unsafe {
            instance.get_physical_device_memory_properties(physical_device)
        };

        // Find a graphics queue family.
        let queue_families = unsafe { instance.get_physical_device_queue_family_properties(physical_device) };
        let graphics_queue_family_index = queue_families.iter().enumerate()
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
            instance.create_device(physical_device, &device_create_info, None)
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
            device.create_command_pool(&command_pool_info, None)
                .map_err(|e| format!("create_command_pool: {e:?}"))?
        };

        Ok(Self {
            entry,
            instance,
            physical_device,
            device,
            graphics_queue,
            graphics_queue_family_index,
            command_pool,
            memory_properties,
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
            api_version: format!("{}.{}.{}",
                vk::api_version_major(self.api_version),
                vk::api_version_minor(self.api_version),
                vk::api_version_patch(self.api_version)),
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
            if let Some(_m) = self._debug.take() { drop(_m); }
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
                .to_string_lossy().to_string();
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
    /// Offscreen rendering target (created on first render, recreated on resize).
    offscreen: Option<OffscreenTarget>,
    /// When Vulkan is unavailable (e.g. no ICD), we fall back to the CPU
    /// rasterizer so the observatory still produces a meaningful frame.
    fallback_cpu: Option<crate::cpu::CpuBackend>,
}

impl VulkanBackend {
    /// Construct a new Vulkan renderer.
    pub fn new(headless: bool, width: u32, height: u32) -> Self {
        Self { headless, width, height, device: None, offscreen: None, fallback_cpu: None }
    }

    fn ensure_device(&mut self) -> Option<&VulkanDevice> {
        if self.device.is_some() { return self.device.as_ref(); }
        let validation = cfg!(debug_assertions);
        match VulkanDevice::create(self.headless, validation) {
            Ok(dev) => { self.device = Some(dev); self.device.as_ref() }
            Err(e) => {
                tracing::info!("vulkan unavailable, using CPU fallback: {e}");
                if self.fallback_cpu.is_none() {
                    self.fallback_cpu = Some(crate::cpu::CpuBackend::new(self.width, self.height));
                }
                None
            }
        }
    }

    /// Ensure the offscreen target exists for the current resolution.
    fn ensure_offscreen(&mut self) -> Result<(), String> {
        if self.offscreen.as_ref().map_or(false, |t| t.width == self.width && t.height == self.height) {
            return Ok(());
        }
        let device = match self.device.as_ref() { Some(d) => d, None => return Err("no device".into()) };
        // Destroy the old target if dimensions changed.
        if let Some(old) = self.offscreen.take() {
            old.destroy(&device.device);
        }
        let target = OffscreenTarget::create(&device.device, &device.memory_properties, self.width, self.height)?;
        self.offscreen = Some(target);
        Ok(())
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

    /// Real Vulkan render path: record a clear into the offscreen target,
    /// submit, read back the framebuffer to a host-visible buffer, encode
    /// as PNG.
    ///
    /// Returns `Err` if the path can't complete (e.g. no host-visible memory
    /// type available for readback, command recording fails).
    fn render_offscreen(&mut self, scene: &RenderScene) -> Result<Vec<u8>, String> {
        let device_obj = match self.device.as_ref() { Some(d) => d, None => return Err("no device".into()) };
        let target = match self.offscreen.as_ref() { Some(t) => t, None => return Err("no offscreen target".into()) };
        let device = &device_obj.device;

        // === Allocate a primary command buffer ===
        let cmd_alloc_info = vk::CommandBufferAllocateInfo::default()
            .command_pool(device_obj.command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);
        let command_buffer = unsafe {
            device.allocate_command_buffers(&cmd_alloc_info)
                .map_err(|e| format!("alloc cmd buf: {e:?}"))?
        };
        let command_buffer = command_buffer[0];

        // === Begin recording ===
        let clear_color = vk::ClearColorValue {
            float32: [scene.clear_color[0], scene.clear_color[1], scene.clear_color[2], scene.clear_color[3]],
        };
        let clear_depth = vk::ClearDepthStencilValue {
            depth: 1.0,
            stencil: 0,
        };
        let clear_values = [
            vk::ClearValue { color: clear_color },
            vk::ClearValue { depth_stencil: clear_depth },
        ];
        let render_begin = vk::RenderPassBeginInfo::default()
            .render_pass(target.render_pass)
            .framebuffer(target.framebuffer)
            .render_area(vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: vk::Extent2D { width: target.width, height: target.height },
            })
            .clear_values(&clear_values);

        let cmd_begin = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        unsafe {
            device.begin_command_buffer(command_buffer, &cmd_begin)
                .map_err(|e| format!("begin cmd buf: {e:?}"))?;
            device.cmd_begin_render_pass(command_buffer, &render_begin, vk::SubpassContents::INLINE);
            // End immediately — we only have a clear so far.
            device.cmd_end_render_pass(command_buffer);
            device.end_command_buffer(command_buffer)
                .map_err(|e| format!("end cmd buf: {e:?}"))?;
        }

        // === Submit and wait ===
        let submit_info = vk::SubmitInfo::default()
            .command_buffers(std::slice::from_ref(&command_buffer));
        unsafe {
            device.queue_submit(device_obj.graphics_queue, std::slice::from_ref(&submit_info), vk::Fence::null())
                .map_err(|e| format!("queue submit: {e:?}"))?;
            device.queue_wait_idle(device_obj.graphics_queue)
                .map_err(|e| format!("queue wait idle: {e:?}"))?;
        }

        // === Readback: create a host-visible buffer, copy the color image
        // into it, map the memory, encode as PNG. ===
        // Compute image layout transitions and readback buffer size.
        let buffer_size = (target.width * target.height * 4) as vk::DeviceSize;
        let buffer_info = vk::BufferCreateInfo::default()
            .size(buffer_size)
            .usage(vk::BufferUsageFlags::TRANSFER_DST)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let readback_buffer = unsafe {
            device.create_buffer(&buffer_info, None)
                .map_err(|e| format!("create readback buf: {e:?}"))?
        };
        let buf_mem_req = unsafe { device.get_buffer_memory_requirements(readback_buffer) };
        let readback_memory = allocate_memory(device, &device_obj.memory_properties, buf_mem_req,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT)?;
        unsafe {
            device.bind_buffer_memory(readback_buffer, readback_memory, 0)
                .map_err(|e| format!("bind readback buf memory: {e:?}"))?;
        }

        // Record a second command buffer to transition the color image layout
        // to TransferSrcOptimal and copy it to the readback buffer.
        let cmd2 = unsafe {
            device.allocate_command_buffers(&cmd_alloc_info)
                .map_err(|e| format!("alloc cmd buf 2: {e:?}"))?
        };
        let cmd2 = cmd2[0];
        unsafe {
            device.begin_command_buffer(cmd2, &cmd_begin)
                .map_err(|e| format!("begin cmd2: {e:?}"))?;

            // Transition color image layout: from whatever the render pass
            // left it in (TRANSFER_SRC_OPTIMAL, per the attachment description)
            // to TRANSFER_SRC_OPTIMAL (no-op in this case, but the barrier
            // is required for safety).
            let image_barrier = vk::ImageMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
                .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
                .old_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(target.color_image)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                });
            device.cmd_pipeline_barrier(
                cmd2,
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::default(),
                &[],
                &[],
                std::slice::from_ref(&image_barrier),
            );

            // Copy image → buffer.
            let region = vk::BufferImageCopy::default()
                .buffer_offset(0)
                .buffer_row_length(target.width)
                .buffer_image_height(target.height)
                .image_subresource(vk::ImageSubresourceLayers {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    mip_level: 0,
                    base_array_layer: 0,
                    layer_count: 1,
                })
                .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
                .image_extent(vk::Extent3D { width: target.width, height: target.height, depth: 1 });
            device.cmd_copy_image_to_buffer(
                cmd2,
                target.color_image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                readback_buffer,
                std::slice::from_ref(&region),
            );

            device.end_command_buffer(cmd2)
                .map_err(|e| format!("end cmd2: {e:?}"))?;
        }

        let submit_info2 = vk::SubmitInfo::default()
            .command_buffers(std::slice::from_ref(&cmd2));
        unsafe {
            device.queue_submit(device_obj.graphics_queue, std::slice::from_ref(&submit_info2), vk::Fence::null())
                .map_err(|e| format!("queue submit 2: {e:?}"))?;
            device.queue_wait_idle(device_obj.graphics_queue)
                .map_err(|e| format!("queue wait idle 2: {e:?}"))?;
        }

        // === Map and read ===
        let png_bytes = unsafe {
            let ptr = device.map_memory(readback_memory, 0, buffer_size as vk::DeviceSize, vk::MemoryMapFlags::default())
                .map_err(|e| format!("map memory: {e:?}"))?;
            let slice = std::slice::from_raw_parts(ptr as *const u8, buffer_size as usize);
            // The image is R8G8B8A8_SRGB; the bytes are already sRGB-encoded.
            crate::png::encode_rgba(target.width, target.height, slice)
                .map_err(|e| format!("png encode: {e}"))?;
            device.unmap_memory(readback_memory);
            crate::png::encode_rgba(target.width, target.height, slice)
                .map_err(|e| format!("png encode 2: {e}"))?
        };

        // === Cleanup per-frame resources ===
        unsafe {
            device.destroy_buffer(readback_buffer, None);
            device.free_memory(readback_memory, None);
            device.free_command_buffers(device_obj.command_pool, &[command_buffer, cmd2]);
        }

        Ok(png_bytes)
    }
}

impl Backend for VulkanBackend {
    fn render(&mut self, scene: &RenderScene) -> FrameResult {
        let _ = self.ensure_device();
        // Try the real Vulkan path: record a clear into the offscreen target,
        // submit, readback to CPU, encode as PNG. If anything fails, fall
        // back to the CPU rasterizer.
        if self.device.is_some() {
            if let Err(e) = self.ensure_offscreen() {
                tracing::warn!("offscreen target creation failed: {e}");
                return self.render_through_fallback(scene);
            }
            match self.render_offscreen(scene) {
                Ok(png_bytes) => {
                    use std::time::Instant;
                    let start = Instant::now();
                    let elapsed_us = start.elapsed().as_micros() as u64;
                    return FrameResult {
                        png_bytes: Some(png_bytes),
                        status: RenderStatus::Degraded, // still degraded: only clears, no geometry.
                        metrics: crate::metrics::Metrics {
                            backend: "vulkan".to_string(),
                            width: self.width,
                            height: self.height,
                            frame_time_us: elapsed_us,
                            draw_calls: 0,
                            triangles: 0,
                            visible_objects: 0,
                            loaded_meshes: 0,
                            loaded_textures: 0,
                            active_materials: 0,
                            gpu_status: self.device.as_ref().map(|d| d.status(cfg!(debug_assertions))),
                            ..Default::default()
                        },
                    };
                }
                Err(e) => {
                    tracing::warn!("vulkan offscreen render failed: {e}; using CPU fallback");
                    return self.render_through_fallback(scene);
                }
            }
        }
        // No device; use CPU fallback.
        self.render_through_fallback(scene)
    }
    fn resize(&mut self, width: u32, height: u32) {
        self.width = width.max(1);
        self.height = height.max(1);
        if let Some(cpu) = self.fallback_cpu.as_mut() { cpu.resize(width, height); }
        // Offscreen target will be recreated on next render.
    }
    fn name(&self) -> &'static str { "vulkan" }
    fn has_gpu(&self) -> bool { self.device.is_some() }
    fn dimensions(&self) -> (u32, u32) { (self.width, self.height) }
}

/// Offscreen rendering target: color attachment + depth attachment + their
/// memory allocations + image views + render pass + framebuffer.
///
/// Currently the type is a placeholder; the next milestone populates it with
/// real Vulkan resources. The fields are kept so the type signature is stable
/// for downstream milestones.
pub struct OffscreenTarget {
    /// Color image handle.
    pub color_image: vk::Image,
    /// Color image memory handle.
    pub color_memory: vk::DeviceMemory,
    /// Color image view.
    pub color_view: vk::ImageView,
    /// Depth image handle.
    pub depth_image: vk::Image,
    /// Depth image memory handle.
    pub depth_memory: vk::DeviceMemory,
    /// Depth image view.
    pub depth_view: vk::ImageView,
    /// Render pass.
    pub render_pass: vk::RenderPass,
    /// Framebuffer (color + depth attachments).
    pub framebuffer: vk::Framebuffer,
    /// Width.
    pub width: u32,
    /// Height.
    pub height: u32,
}

impl OffscreenTarget {
    /// Create an offscreen target of the given dimensions on the given device.
    ///
    /// Allocates:
    /// - Color attachment (8-bit sRGB RGBA, color attachment optimal tiling)
    /// - Depth attachment (D32_SFLOAT, depth-stencil optimal tiling)
    /// - Image views for both
    /// - Render pass (load+store, color + depth attachments, color attachment
    ///   initial=Undefined → ShaderReadOnly, depth initial=Undefined →
    ///   DepthStencilAttachmentOptimal)
    /// - Framebuffer binding the two views
    pub fn create(
        device: &ash::Device,
        memory_properties: &vk::PhysicalDeviceMemoryProperties,
        width: u32,
        height: u32,
    ) -> Result<Self, String> {
        // === Color image ===
        let color_image_info = vk::ImageCreateInfo {
            image_type: vk::ImageType::TYPE_2D,
            format: vk::Format::R8G8B8A8_SRGB,
            extent: vk::Extent3D { width, height, depth: 1 },
            mip_levels: 1,
            array_layers: 1,
            samples: vk::SampleCountFlags::TYPE_1,
            tiling: vk::ImageTiling::OPTIMAL,
            usage: vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC,
            sharing_mode: vk::SharingMode::EXCLUSIVE,
            initial_layout: vk::ImageLayout::UNDEFINED,
            ..Default::default()
        };
        let color_image = unsafe {
            device.create_image(&color_image_info, None)
                .map_err(|e| format!("create color image: {e:?}"))?
        };
        let color_mem_req = unsafe { device.get_image_memory_requirements(color_image) };
        let color_memory = allocate_memory(device, memory_properties, color_mem_req,
            vk::MemoryPropertyFlags::DEVICE_LOCAL)?;
        unsafe {
            device.bind_image_memory(color_image, color_memory, 0)
                .map_err(|e| format!("bind color image memory: {e:?}"))?;
        }
        let color_view_info = vk::ImageViewCreateInfo {
            image: color_image,
            view_type: vk::ImageViewType::TYPE_2D,
            format: vk::Format::R8G8B8A8_SRGB,
            subresource_range: vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            },
            ..Default::default()
        };
        let color_view = unsafe {
            device.create_image_view(&color_view_info, None)
                .map_err(|e| format!("create color view: {e:?}"))?
        };

        // === Depth image ===
        let depth_image_info = vk::ImageCreateInfo {
            image_type: vk::ImageType::TYPE_2D,
            format: vk::Format::D32_SFLOAT,
            extent: vk::Extent3D { width, height, depth: 1 },
            mip_levels: 1,
            array_layers: 1,
            samples: vk::SampleCountFlags::TYPE_1,
            tiling: vk::ImageTiling::OPTIMAL,
            usage: vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
            sharing_mode: vk::SharingMode::EXCLUSIVE,
            initial_layout: vk::ImageLayout::UNDEFINED,
            ..Default::default()
        };
        let depth_image = unsafe {
            device.create_image(&depth_image_info, None)
                .map_err(|e| format!("create depth image: {e:?}"))?
        };
        let depth_mem_req = unsafe { device.get_image_memory_requirements(depth_image) };
        let depth_memory = allocate_memory(device, memory_properties, depth_mem_req,
            vk::MemoryPropertyFlags::DEVICE_LOCAL)?;
        unsafe {
            device.bind_image_memory(depth_image, depth_memory, 0)
                .map_err(|e| format!("bind depth image memory: {e:?}"))?;
        }
        let depth_view_info = vk::ImageViewCreateInfo {
            image: depth_image,
            view_type: vk::ImageViewType::TYPE_2D,
            format: vk::Format::D32_SFLOAT,
            subresource_range: vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::DEPTH,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            },
            ..Default::default()
        };
        let depth_view = unsafe {
            device.create_image_view(&depth_view_info, None)
                .map_err(|e| format!("create depth view: {e:?}"))?
        };

        // === Render pass ===
        let color_attachment = vk::AttachmentDescription {
            format: vk::Format::R8G8B8A8_SRGB,
            samples: vk::SampleCountFlags::TYPE_1,
            load_op: vk::AttachmentLoadOp::CLEAR,
            store_op: vk::AttachmentStoreOp::STORE,
            stencil_load_op: vk::AttachmentLoadOp::DONT_CARE,
            stencil_store_op: vk::AttachmentStoreOp::DONT_CARE,
            initial_layout: vk::ImageLayout::UNDEFINED,
            final_layout: vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            ..Default::default()
        };
        let depth_attachment = vk::AttachmentDescription {
            format: vk::Format::D32_SFLOAT,
            samples: vk::SampleCountFlags::TYPE_1,
            load_op: vk::AttachmentLoadOp::CLEAR,
            store_op: vk::AttachmentStoreOp::DONT_CARE,
            stencil_load_op: vk::AttachmentLoadOp::DONT_CARE,
            stencil_store_op: vk::AttachmentStoreOp::DONT_CARE,
            initial_layout: vk::ImageLayout::UNDEFINED,
            final_layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
            ..Default::default()
        };
        let color_attachments = [vk::AttachmentReference {
            attachment: 0,
            layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        }];
        let depth_attachment_ref = vk::AttachmentReference {
            attachment: 1,
            layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
        };
        let subpass = vk::SubpassDescription::default()
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
            .color_attachments(&color_attachments)
            .depth_stencil_attachment(&depth_attachment_ref);
        let attachments_arr = [color_attachment, depth_attachment];
        let subpasses_arr = [subpass];
        let render_pass_info = vk::RenderPassCreateInfo::default()
            .attachments(&attachments_arr)
            .subpasses(&subpasses_arr)
            .dependencies(&[]);
        let render_pass = unsafe {
            device.create_render_pass(&render_pass_info, None)
                .map_err(|e| format!("create render pass: {e:?}"))?
        };

        // === Framebuffer ===
        let attachments = [color_view, depth_view];
        let framebuffer_info = vk::FramebufferCreateInfo::default()
            .render_pass(render_pass)
            .attachments(&attachments)
            .width(width)
            .height(height)
            .layers(1);
        let framebuffer = unsafe {
            device.create_framebuffer(&framebuffer_info, None)
                .map_err(|e| format!("create framebuffer: {e:?}"))?
        };

        Ok(Self {
            color_image, color_memory, color_view,
            depth_image, depth_memory, depth_view,
            render_pass, framebuffer,
            width, height,
        })
    }

    /// Destroy all resources.
    pub fn destroy(self, device: &ash::Device) {
        unsafe {
            device.destroy_framebuffer(self.framebuffer, None);
            device.destroy_render_pass(self.render_pass, None);
            device.destroy_image_view(self.depth_view, None);
            device.free_memory(self.depth_memory, None);
            device.destroy_image(self.depth_image, None);
            device.destroy_image_view(self.color_view, None);
            device.free_memory(self.color_memory, None);
            device.destroy_image(self.color_image, None);
        }
    }
}

/// Allocate device memory of the requested size with the given property
/// flags. Returns the device-memory handle.
fn allocate_memory(
    device: &ash::Device,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    requirements: vk::MemoryRequirements,
    required_flags: vk::MemoryPropertyFlags,
) -> Result<vk::DeviceMemory, String> {
    let memory_type_index = (0..memory_properties.memory_type_count)
        .find(|&i| {
            let mask = 1u32 << i;
            (requirements.memory_type_bits & mask) != 0
                && memory_properties.memory_types[i as usize]
                    .property_flags.contains(required_flags)
        })
        .ok_or_else(|| format!(
            "no memory type with required flags: {:?}",
            required_flags
        ))?;
    let alloc_info = vk::MemoryAllocateInfo {
        allocation_size: requirements.size,
        memory_type_index,
        ..Default::default()
    };
    unsafe {
        device.allocate_memory(&alloc_info, None)
            .map_err(|e| format!("allocate memory: {e:?}"))
    }
}

// silence unused import warning
#[allow(dead_code)]
fn _unused(_a: &Arc<()>, _b: vk::Queue, _c: vk::PhysicalDeviceMemoryProperties) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::RenderScene;

    #[test]
    fn backend_falls_back_gracefully_without_vulkan() {
        let mut backend = VulkanBackend::new(true, 32, 32);
        let result = backend.render(&RenderScene::default());
        assert!(matches!(result.status, RenderStatus::Degraded | RenderStatus::Ok),
                "backend must produce some frame");
        assert!(result.png_bytes.is_some(), "must produce a PNG");
    }

    #[test]
    fn ensures_device_or_fallback() {
        let mut backend = VulkanBackend::new(true, 32, 32);
        let _ = backend.ensure_device();
        if backend.device.is_none() {
            assert!(backend.fallback_cpu.is_some(),
                    "must have CPU fallback when no Vulkan device is available");
        }
    }
}
