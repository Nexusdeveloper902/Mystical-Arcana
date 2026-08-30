//! arcane_render — the Arcane engine's real Vulkan renderer.
//!
//! Consolidated into a single file so the ash 0.38 API surface can be
//! audited at a glance and the Vulkan object lifecycle is obvious.
//!
//! ## Object lifecycle
//!
//! Drop order is reverse-construction order:
//!   1. readback buffer + command buffer
//!   2. triangle vertex/index buffers
//!   3. graphics pipeline + shader modules + pipeline layout
//!   4. framebuffers
//!   5. render pass
//!   6. frame (command buffers + sync objects)
//!   7. swapchain + image views + surface
//!   8. logical device
//!   9. debug messenger
//!  10. instance
//!
//! All Vulkan handles are stored as `vk::X` raw handles (not owning wrappers),
//! because the parent `VkContext` is kept alive in an `Arc` until the very end.
//! This is the simplest way to keep the Drop sequence right without fighting
//! the borrow checker for cross-references between wrappers.

use std::borrow::Cow;
use std::ffi::{CStr, CString};
use std::sync::Arc;

use ash::ext::{debug_utils, headless_surface};
use ash::vk;
use parking_lot::Mutex;
use tiny_http::{Header, Response, Server};

// =============================================================================
// Error type
// =============================================================================

#[derive(Debug)]
pub enum RenderError {
    LoaderNotFound(String),
    InstanceCreate(String),
    NoPhysicalDevice,
    DeviceCreate(String),
    QueueFamily(String),
    SurfaceCreate(String),
    SwapchainCreate(String),
    AcquireImage(String),
    Submit(String),
    Present(String),
    Shader(String),
    Pipeline(String),
    Allocator(String),
    Validation(String),
    IncompatibleDriver(String),
    Other(String),
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl std::error::Error for RenderError {}

impl From<vk::Result> for RenderError {
    fn from(r: vk::Result) -> Self {
        match r {
            vk::Result::ERROR_INCOMPATIBLE_DRIVER => RenderError::IncompatibleDriver(format!("{:?}", r)),
            vk::Result::ERROR_INITIALIZATION_FAILED => RenderError::InstanceCreate(format!("{:?}", r)),
            vk::Result::ERROR_DEVICE_LOST => RenderError::DeviceCreate(format!("{:?}", r)),
            vk::Result::ERROR_OUT_OF_DATE_KHR => RenderError::SwapchainCreate(format!("{:?}", r)),
            _ => RenderError::Other(format!("vk::Result {:?}", r)),
        }
    }
}

pub type RenderResult<T> = Result<T, RenderError>;

// =============================================================================
// Vulkan context: entry, instance, debug messenger, physical device, device
// =============================================================================

pub struct VkContext {
    pub entry: Arc<ash::Entry>,
    pub instance: ash::Instance,
    pub debug_utils_loader: Option<debug_utils::Instance>,
    pub debug_messenger: Option<vk::DebugUtilsMessengerEXT>,
    pub physical_device: vk::PhysicalDevice,
    pub physical_device_props: vk::PhysicalDeviceProperties,
    pub physical_device_features: vk::PhysicalDeviceFeatures,
    pub device_memory_props: vk::PhysicalDeviceMemoryProperties,
    pub device: ash::Device,
    pub graphics_queue_family: u32,
    pub graphics_queue: vk::Queue,
    pub surface_loader: Option<ash::khr::surface::Instance>,
}

static VALIDATION_FAILED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

pub fn validation_failed() -> bool {
    VALIDATION_FAILED.load(std::sync::atomic::Ordering::SeqCst)
}

unsafe extern "system" fn messenger_callback(
    flags: vk::DebugUtilsMessageSeverityFlagsEXT,
    _types: vk::DebugUtilsMessageTypeFlagsEXT,
    p_callback_data: *const vk::DebugUtilsMessengerCallbackDataEXT,
    _user_data: *mut std::ffi::c_void,
) -> vk::Bool32 {
    // The pfn_user_callback expects Option<PFN_vkDebugUtilsMessengerCallbackEXT>.
    // PFN's signature is exactly the same as ours. The Some(...) wrapping is
    // done by the caller, not here.
    let data = &*p_callback_data;
    let message = if data.p_message.is_null() {
        "(no message)".to_string()
    } else {
        CStr::from_ptr(data.p_message).to_string_lossy().to_string()
    };

    let prefix = if flags.contains(vk::DebugUtilsMessageSeverityFlagsEXT::ERROR) {
        "[VK error]"
    } else if flags.contains(vk::DebugUtilsMessageSeverityFlagsEXT::WARNING) {
        "[VK warn]"
    } else if flags.contains(vk::DebugUtilsMessageSeverityFlagsEXT::INFO) {
        "[VK info]"
    } else {
        "[VK verbose]"
    };

    let full = format!("{} {}", prefix, message);

    if flags.contains(vk::DebugUtilsMessageSeverityFlagsEXT::ERROR) {
        log::error!("{}", full);
        VALIDATION_FAILED.store(true, std::sync::atomic::Ordering::SeqCst);
    } else if flags.contains(vk::DebugUtilsMessageSeverityFlagsEXT::WARNING) {
        log::warn!("{}", full);
    } else if flags.contains(vk::DebugUtilsMessageSeverityFlagsEXT::INFO) {
        log::info!("{}", full);
    } else {
        log::debug!("{}", full);
    }

    vk::FALSE
}

impl VkContext {
    pub fn new(app_name: &str, use_validation: bool) -> RenderResult<Arc<Self>> {
        let entry = unsafe { ash::Entry::load() }
            .map_err(|e| RenderError::LoaderNotFound(e.to_string()))?;
        let entry = Arc::new(entry);

        // enumerate available instance layers
        let available_layers = unsafe {
            entry.enumerate_instance_layer_properties()
                .unwrap_or_else(|e| {
                    log::warn!("enumerate_instance_layer_properties failed: {:?}", e);
                    Vec::new()
                })
        };
        let want_validation = use_validation && available_layers.iter().any(|l| {
            unsafe { CStr::from_ptr(l.layer_name.as_ptr()).to_string_lossy().starts_with("VK_LAYER_KHRONOS") }
        });
        if use_validation && !want_validation {
            log::warn!("VK_LAYER_KHRONOS_validation not available — running without validation");
        }

        let mut layer_names: Vec<CString> = Vec::new();
        if want_validation {
            layer_names.push(CString::new("VK_LAYER_KHRONOS_validation").unwrap());
        }
        let layers_ptrs: Vec<*const i8> = layer_names.iter().map(|n| n.as_ptr()).collect();

        // extensions: debug_utils + headless_surface + KHR_surface
        let mut extension_names: Vec<CString> = Vec::new();
        extension_names.push(CString::new("VK_EXT_debug_utils").unwrap());
        extension_names.push(CString::new("VK_EXT_headless_surface").unwrap());
        extension_names.push(CString::new("VK_KHR_surface").unwrap());

        let available_exts = unsafe {
            entry.enumerate_instance_extension_properties(None)
                .unwrap_or_else(|e| {
                    log::warn!("enumerate_instance_extension_properties failed: {:?}", e);
                    Vec::new()
                })
        };
        for ext in &extension_names {
            let present = available_exts.iter().any(|a| {
                let name = unsafe { CStr::from_ptr(a.extension_name.as_ptr()).to_string_lossy() };
                name == ext.to_string_lossy()
            });
            if !present {
                return Err(RenderError::InstanceCreate(format!(
                    "Required instance extension not available: {}", ext.to_string_lossy()
                )));
            }
        }
        let ext_ptrs: Vec<*const i8> = extension_names.iter().map(|n| n.as_ptr()).collect();

        let app_name_c = CString::new(app_name).unwrap();
        let engine_name_c = CString::new("Arcane").unwrap();
        let app_info = vk::ApplicationInfo::default()
            .application_name(&app_name_c)
            .application_version(vk::make_api_version(0, 0, 1, 0))
            .engine_name(&engine_name_c)
            .engine_version(vk::make_api_version(0, 0, 1, 0))
            .api_version(vk::API_VERSION_1_3);

        let create_info = vk::InstanceCreateInfo::default()
            .application_info(&app_info)
            .enabled_layer_names(&layers_ptrs)
            .enabled_extension_names(&ext_ptrs);

        let mut debug_create = vk::DebugUtilsMessengerCreateInfoEXT::default()
            .flags(vk::DebugUtilsMessengerCreateFlagsEXT::empty())
            .message_severity(
                vk::DebugUtilsMessageSeverityFlagsEXT::ERROR
                    | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
                    | vk::DebugUtilsMessageSeverityFlagsEXT::INFO,
            )
            .message_type(
                vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                    | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                    | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
            )
            .pfn_user_callback(Some(messenger_callback));
            // Some(fn_item) coerces to Option<PFN_...>. If the types differ in
            // mutability, we use as `unsafe extern "system" fn(...)` directly.
        let _ = create_info.push_next(&mut debug_create);

        let instance = unsafe {
            entry.create_instance(&create_info, None)
                .map_err(|e| {
                    log::error!("vkCreateInstance failed: {:?}", e);
                    RenderError::InstanceCreate(format!("{:?}", e))
                })?
        };

        let (debug_utils_loader, debug_messenger) = if want_validation {
            let loader = debug_utils::Instance::new(&entry, &instance);
            let messenger = unsafe {
                loader.create_debug_utils_messenger(&debug_create, None)
                    .map_err(|e| RenderError::Validation(format!(
                        "create_debug_utils_messenger: {:?}", e
                    )))?
            };
            (Some(loader), Some(messenger))
        } else {
            (None, None)
        };

        // physical device selection
        let physicals = unsafe {
            instance.enumerate_physical_devices()
                .map_err(|e| RenderError::Other(format!("enumerate_physical_devices: {:?}", e)))?
        };
        if physicals.is_empty() {
            return Err(RenderError::NoPhysicalDevice);
        }

        let mut best: Option<(vk::PhysicalDevice, vk::PhysicalDeviceProperties, vk::PhysicalDeviceFeatures, vk::PhysicalDeviceMemoryProperties, u32, u64)> = None;
        for pd in physicals {
            let props = unsafe { instance.get_physical_device_properties(pd) };
            let features = unsafe { instance.get_physical_device_features(pd) };
            let mem_props = unsafe { instance.get_physical_device_memory_properties(pd) };
            let qfams = unsafe { instance.get_physical_device_queue_family_properties(pd) };

            let mut graphics_fam = None;
            for (i, q) in qfams.iter().enumerate() {
                if q.queue_flags.contains(vk::QueueFlags::GRAPHICS) {
                    graphics_fam = Some(i as u32);
                    break;
                }
            }
            let graphics_qf = match graphics_fam {
                Some(g) => g,
                None => continue,
            };

            let total_local = (0..mem_props.memory_heap_count as usize)
                .filter(|i| mem_props.memory_heaps[*i].flags.contains(vk::MemoryHeapFlags::DEVICE_LOCAL))
                .map(|i| mem_props.memory_heaps[i].size)
                .sum::<u64>();

            log::info!(
                "physical device: {:?} type={:?} device_local_memory={} MiB graphics_queue={}",
                unsafe { CStr::from_ptr(props.device_name.as_ptr()).to_string_lossy() },
                props.device_type,
                total_local / (1024 * 1024),
                graphics_qf
            );

            let is_better = match &best {
                None => true,
                Some((.., b_total)) => total_local > *b_total,
            };
            if is_better {
                best = Some((pd, props, features, mem_props, graphics_qf, total_local));
            }
        }

        let (physical_device, physical_device_props, physical_device_features, device_memory_props, graphics_qf, _)
            = best.ok_or(RenderError::NoPhysicalDevice)?;

        if physical_device_features.geometry_shader == vk::FALSE {
            log::warn!("Selected physical device has no geometry shader");
        }

        // logical device
        let queue_priorities = [1.0f32];
        let graphics_create = vk::DeviceQueueCreateInfo::default()
            .queue_family_index(graphics_qf)
            .queue_priorities(&queue_priorities);

        let mut desired_features = vk::PhysicalDeviceFeatures::default();
        desired_features.geometry_shader = physical_device_features.geometry_shader;
        desired_features.fill_mode_non_solid = physical_device_features.fill_mode_non_solid;
        desired_features.shader_clip_distance = physical_device_features.shader_clip_distance;
        desired_features.wide_lines = physical_device_features.wide_lines;
        desired_features.depth_clamp = physical_device_features.depth_clamp;
        desired_features.sampler_anisotropy = physical_device_features.sampler_anisotropy;

        // VK_KHR_swapchain is required to create a swapchain (even on a
        // headless surface — it is the only present path).
        // On Vulkan 1.1+ it may be promoted but it's still a device extension.
        let swap_ext = CString::new("VK_KHR_swapchain").unwrap();
        let device_extensions: [*const i8; 1] = [swap_ext.as_ptr()];
        let device_create_info = vk::DeviceCreateInfo::default()
            .queue_create_infos(std::slice::from_ref(&graphics_create))
            .enabled_extension_names(&device_extensions)
            .enabled_features(&desired_features);

        let device = unsafe {
            instance.create_device(physical_device, &device_create_info, None)
                .map_err(|e| {
                    log::error!("vkCreateDevice failed: {:?}", e);
                    RenderError::DeviceCreate(format!("{:?}", e))
                })?
        };

        let graphics_queue = unsafe { device.get_device_queue(graphics_qf, 0) };

        log::info!(
            "Vulkan context ready: device={:?}",
            unsafe { CStr::from_ptr(physical_device_props.device_name.as_ptr()).to_string_lossy() }
        );

        Ok(Arc::new(Self {
            entry,
            instance,
            debug_utils_loader,
            debug_messenger,
            physical_device,
            physical_device_props,
            physical_device_features,
            device_memory_props,
            device,
            graphics_queue_family: graphics_qf,
            graphics_queue,
            surface_loader: None,
        }))
    }

    pub fn device_name(&self) -> String {
        unsafe { CStr::from_ptr(self.physical_device_props.device_name.as_ptr()) }
            .to_string_lossy().to_string()
    }

    pub fn find_memory_type(
        &self,
        type_bits: u32,
        properties: vk::MemoryPropertyFlags,
    ) -> RenderResult<u32> {
        for i in 0..self.device_memory_props.memory_type_count {
            let bit = (type_bits >> i) & 1;
            if bit == 1 {
                let flags = self.device_memory_props.memory_types[i as usize].property_flags;
                if flags.contains(properties) {
                    return Ok(i);
                }
            }
        }
        Err(RenderError::Allocator(format!(
            "No memory type matching type_bits={:#x} properties={:?}",
            type_bits, properties
        )))
    }
}

impl Drop for VkContext {
    fn drop(&mut self) {
        unsafe {
            if let (Some(loader), Some(messenger)) =
                (self.debug_utils_loader.as_ref(), self.debug_messenger.take())
            {
                let _ = loader.destroy_debug_utils_messenger(messenger, None);
            }
            self.device.destroy_device(None);
            self.instance.destroy_instance(None);
        }
    }
}

// =============================================================================
// Buffer
// =============================================================================

pub struct Buffer {
    pub raw: vk::Buffer,
    pub memory: vk::DeviceMemory,
    pub size: vk::DeviceSize,
    pub mapped: Option<*mut std::ffi::c_void>,
    pub memory_props: vk::MemoryPropertyFlags,
}

impl Buffer {
    pub fn new(
        ctx: &VkContext,
        size: vk::DeviceSize,
        usage: vk::BufferUsageFlags,
        properties: vk::MemoryPropertyFlags,
    ) -> RenderResult<Self> {
        let create = vk::BufferCreateInfo::default()
            .size(size)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let raw = unsafe {
            ctx.device.create_buffer(&create, None)
                .map_err(|e| RenderError::Allocator(format!("create_buffer: {:?}", e)))?
        };
        let req = unsafe { ctx.device.get_buffer_memory_requirements(raw) };
        let mem_type = ctx.find_memory_type(req.memory_type_bits, properties)?;
        let alloc = vk::MemoryAllocateInfo::default()
            .allocation_size(req.size)
            .memory_type_index(mem_type);
        let memory = unsafe {
            ctx.device.allocate_memory(&alloc, None)
                .map_err(|e| RenderError::Allocator(format!("allocate_memory: {:?}", e)))?
        };
        unsafe {
            ctx.device.bind_buffer_memory(raw, memory, 0)
                .map_err(|e| RenderError::Allocator(format!("bind_buffer_memory: {:?}", e)))?;
        }
        let mapped = if properties.contains(vk::MemoryPropertyFlags::HOST_VISIBLE) {
            let ptr = unsafe {
                ctx.device.map_memory(memory, 0, size, vk::MemoryMapFlags::empty())
                    .map_err(|e| RenderError::Allocator(format!("map_memory: {:?}", e)))?
            };
            Some(ptr)
        } else {
            None
        };
        Ok(Self { raw, memory, size, mapped, memory_props: properties })
    }

    pub fn write_bytes(&mut self, bytes: &[u8]) -> RenderResult<()> {
        let p = self.mapped.ok_or_else(|| RenderError::Allocator("buffer not mapped".into()))?;
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), p as *mut u8, bytes.len());
        }
        Ok(())
    }

    pub fn read_bytes(&self) -> &[u8] {
        match self.mapped {
            Some(p) => unsafe { std::slice::from_raw_parts(p as *const u8, self.size as usize) },
            None => &[],
        }
    }

    pub fn destroy(&mut self, ctx: &VkContext) {
        if self.mapped.is_some() {
            unsafe { ctx.device.unmap_memory(self.memory); }
            self.mapped = None;
        }
        unsafe {
            ctx.device.destroy_buffer(self.raw, None);
            ctx.device.free_memory(self.memory, None);
        }
    }
}

// =============================================================================
// Vertex / Mesh
// =============================================================================

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub color: [f32; 3],
}

pub struct TriangleMesh {
    pub vertex: Buffer,
    pub index: Buffer,
    pub index_count: u32,
}

impl TriangleMesh {
    pub fn new(ctx: &VkContext) -> RenderResult<Self> {
        let vertices: [Vertex; 3] = [
            Vertex { pos: [ 0.5, -0.5, 0.0], color: [1.0, 0.0, 0.0] },
            Vertex { pos: [-0.5, -0.5, 0.0], color: [0.0, 1.0, 0.0] },
            Vertex { pos: [ 0.0,  0.5, 0.0], color: [0.0, 0.0, 1.0] },
        ];
        let indices: [u16; 3] = [0, 1, 2];

        let v_size = std::mem::size_of_val(&vertices) as vk::DeviceSize;
        let i_size = std::mem::size_of_val(&indices) as vk::DeviceSize;

        // Use HOST_VISIBLE|HOST_COHERENT for both vertex and index buffers
        // since lavapipe is CPU-backed anyway. For a real GPU we'd stage through
        // a transfer queue.
        let mut vertex = Buffer::new(
            ctx, v_size,
            vk::BufferUsageFlags::VERTEX_BUFFER,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )?;
        vertex.write_bytes(unsafe {
            std::slice::from_raw_parts(
                vertices.as_ptr() as *const u8,
                std::mem::size_of_val(&vertices),
            )
        })?;
        let mut index = Buffer::new(
            ctx, i_size,
            vk::BufferUsageFlags::INDEX_BUFFER,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )?;
        index.write_bytes(unsafe {
            std::slice::from_raw_parts(
                indices.as_ptr() as *const u8,
                std::mem::size_of_val(&indices),
            )
        })?;

        Ok(Self { vertex, index, index_count: indices.len() as u32 })
    }

    pub fn destroy(&mut self, ctx: &VkContext) {
        self.vertex.destroy(ctx);
        self.index.destroy(ctx);
    }
}

// =============================================================================
// Shaders (embedded SPIR-V)
// =============================================================================

#[derive(rust_embed::RustEmbed)]
#[folder = "shaders/spirv"]
pub struct ShaderAssets;

pub fn load_shader(ctx: &VkContext, name: &str) -> RenderResult<vk::ShaderModule> {
    let file = ShaderAssets::get(name).ok_or_else(|| {
        RenderError::Shader(format!("embedded shader not found: {}", name))
    })?;
    let data = Cow::clone(&file.data);
    let code_slice = data.as_ref();
    let code: &[u32] = unsafe {
        std::slice::from_raw_parts(
            code_slice.as_ptr() as *const u32,
            code_slice.len() / 4,
        )
    };
    let create = vk::ShaderModuleCreateInfo::default().code(code);
    unsafe {
        Ok(ctx.device.create_shader_module(&create, None)
            .map_err(|e| RenderError::Shader(format!("create_shader_module: {:?}", e)))?)
    }
}

// =============================================================================
// Depth buffer
// =============================================================================

pub struct DepthBuffer {
    pub image: vk::Image,
    pub memory: vk::DeviceMemory,
    pub view: vk::ImageView,
    pub format: vk::Format,
    pub extent: vk::Extent2D,
}

impl DepthBuffer {
    pub fn new(ctx: &VkContext, extent: vk::Extent2D) -> RenderResult<Self> {
        let format = pick_depth_format(ctx);
        log::info!(
            "depth buffer: {:?} {}x{}",
            format, extent.width, extent.height
        );

        let create_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .extent(vk::Extent3D {
                width: extent.width,
                height: extent.height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .format(format)
            .tiling(vk::ImageTiling::OPTIMAL)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .usage(vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT)
            .samples(vk::SampleCountFlags::TYPE_1)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let image = unsafe {
            ctx.device.create_image(&create_info, None)
                .map_err(|e| RenderError::Allocator(format!("create_image (depth): {:?}", e)))?
        };

        let req = unsafe { ctx.device.get_image_memory_requirements(image) };
        // DEVICE_LOCAL is preferred; on lavapipe this is just host memory
        // anyway but we keep the convention for when we run on a real GPU.
        let mem_type = ctx.find_memory_type(
            req.memory_type_bits,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        )?;
        let alloc = vk::MemoryAllocateInfo::default()
            .allocation_size(req.size)
            .memory_type_index(mem_type);
        let memory = unsafe {
            ctx.device.allocate_memory(&alloc, None)
                .map_err(|e| RenderError::Allocator(format!("allocate_memory (depth): {:?}", e)))?
        };
        unsafe {
            ctx.device.bind_image_memory(image, memory, 0)
                .map_err(|e| RenderError::Allocator(format!("bind_image_memory (depth): {:?}", e)))?;
        }

        let view_create = vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(format)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::DEPTH,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });
        let view = unsafe {
            ctx.device.create_image_view(&view_create, None)
                .map_err(|e| RenderError::Allocator(format!("create_image_view (depth): {:?}", e)))?
        };

        // No explicit layout transition here — the render pass's
        // `initial_layout = UNDEFINED` + `load_op = CLEAR` performs the
        // transition to DEPTH_STENCIL_ATTACHMENT_OPTIMAL on the first
        // render pass instance.

        Ok(Self { image, memory, view, format, extent })
    }

    pub fn destroy(&mut self, ctx: &VkContext) {
        unsafe {
            ctx.device.destroy_image_view(self.view, None);
            ctx.device.destroy_image(self.image, None);
            ctx.device.free_memory(self.memory, None);
        }
    }
}

/// Pick a depth format that the physical device supports for optimal tiling
/// depth-stencil attachment usage. Prefer D32_SFLOAT (highest precision,
/// simplest), fall back to D24_UNORM_S8_UINT (24-bit depth + 8-bit stencil,
/// which is what most desktop GPUs natively expose).
pub fn pick_depth_format(ctx: &VkContext) -> vk::Format {
    let candidates = [
        vk::Format::D32_SFLOAT,
        vk::Format::D24_UNORM_S8_UINT,
        vk::Format::D16_UNORM,
    ];
    for &f in &candidates {
        let props = unsafe {
            ctx.instance.get_physical_device_format_properties(
                ctx.physical_device,
                f,
            )
        };
        if props.optimal_tiling_features
            .contains(vk::FormatFeatureFlags::DEPTH_STENCIL_ATTACHMENT)
        {
            return f;
        }
    }
    // Last resort — let the validation layer complain rather than panic.
    vk::Format::D32_SFLOAT
}

// =============================================================================
// Render pass + pipeline
// =============================================================================

pub struct Pipeline {
    pub render_pass: vk::RenderPass,
    pub pipeline: vk::Pipeline,
    pub pipeline_layout: vk::PipelineLayout,
    pub vert_module: vk::ShaderModule,
    pub frag_module: vk::ShaderModule,
}

impl Pipeline {
    pub fn new(ctx: &VkContext, color_format: vk::Format, depth_format: vk::Format, extent: vk::Extent2D) -> RenderResult<Self> {
        // Render pass: color attachment + depth attachment.
        // The depth attachment is cleared at the start of each render pass
        // and discarded at the end (we don't need to read it back; lavapipe
        // discards it as soon as the render pass ends).
        let color_attach = vk::AttachmentDescription::default()
            .format(color_format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .final_layout(vk::ImageLayout::PRESENT_SRC_KHR);

        let depth_attach = vk::AttachmentDescription::default()
            .format(depth_format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::DONT_CARE)
            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .final_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL);

        let attachments = [color_attach, depth_attach];

        let color_ref = vk::AttachmentReference::default()
            .attachment(0)
            .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);
        let depth_ref = vk::AttachmentReference::default()
            .attachment(1)
            .layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL);

        let subpass = vk::SubpassDescription::default()
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
            .color_attachments(std::slice::from_ref(&color_ref))
            .depth_stencil_attachment(&depth_ref);

        let dependency = vk::SubpassDependency::default()
            .src_subpass(vk::SUBPASS_EXTERNAL)
            .dst_subpass(0)
            .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS)
            .src_access_mask(vk::AccessFlags::empty())
            .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS)
            .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_READ | vk::AccessFlags::COLOR_ATTACHMENT_WRITE | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE);

        let rp_create = vk::RenderPassCreateInfo::default()
            .attachments(&attachments)
            .subpasses(std::slice::from_ref(&subpass))
            .dependencies(std::slice::from_ref(&dependency));
        let render_pass = unsafe {
            ctx.device.create_render_pass(&rp_create, None)
                .map_err(|e| RenderError::Pipeline(format!("create_render_pass: {:?}", e)))?
        };

        // Shaders
        let vert_module = load_shader(ctx, "tri.vert.spv")?;
        let frag_module = load_shader(ctx, "tri.frag.spv")?;
        let entry_name = CString::new("main").unwrap();
        let vert_stage = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vert_module)
            .name(&entry_name);
        let frag_stage = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(frag_module)
            .name(&entry_name);
        let stages = [vert_stage, frag_stage];

        let vertex_bindings = [vk::VertexInputBindingDescription::default()
            .binding(0)
            .stride(std::mem::size_of::<Vertex>() as u32)
            .input_rate(vk::VertexInputRate::VERTEX)];
        let vertex_attrs = [
            vk::VertexInputAttributeDescription::default()
                .location(0).binding(0)
                .format(vk::Format::R32G32B32_SFLOAT)
                .offset(0),
            vk::VertexInputAttributeDescription::default()
                .location(1).binding(0)
                .format(vk::Format::R32G32B32_SFLOAT)
                .offset(12),
        ];
        let vertex_input = vk::PipelineVertexInputStateCreateInfo::default()
            .vertex_binding_descriptions(&vertex_bindings)
            .vertex_attribute_descriptions(&vertex_attrs);

        let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
            .primitive_restart_enable(false);

        let viewport = vk::Viewport::default()
            .x(0.0).y(0.0)
            .width(extent.width as f32)
            .height(extent.height as f32)
            .min_depth(0.0).max_depth(1.0);
        let scissor = vk::Rect2D::default()
            .offset(vk::Offset2D { x: 0, y: 0 })
            .extent(extent);
        let viewport_state = vk::PipelineViewportStateCreateInfo::default()
            .viewports(std::slice::from_ref(&viewport))
            .scissors(std::slice::from_ref(&scissor));

        let rasterizer = vk::PipelineRasterizationStateCreateInfo::default()
            .depth_clamp_enable(false)
            .rasterizer_discard_enable(false)
            .polygon_mode(vk::PolygonMode::FILL)
            .line_width(1.0)
            // Cull back faces (the ones facing away from the camera) so a
            // closed mesh like a cube draws correctly without the back faces
            // showing through the front. With CCW front-facing winding and
            // CULL_BACK, only front faces (those whose vertices are CCW
            // when viewed from the camera) are rasterized.
            .cull_mode(vk::CullModeFlags::BACK)
            .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
            .depth_bias_enable(false);

        let multisampling = vk::PipelineMultisampleStateCreateInfo::default()
            .sample_shading_enable(false)
            .rasterization_samples(vk::SampleCountFlags::TYPE_1);

        // Depth/stencil state: enable depth testing with LESS compare (a
        // fragment is drawn if its depth is less than what's already in the
        // depth buffer), and enable depth writes so later fragments see
        // the updated depth. No stencil testing.
        let depth_stencil = vk::PipelineDepthStencilStateCreateInfo::default()
            .depth_test_enable(true)
            .depth_write_enable(true)
            .depth_compare_op(vk::CompareOp::LESS)
            .depth_bounds_test_enable(false)
            .stencil_test_enable(false);

        let color_blend_attachments = [vk::PipelineColorBlendAttachmentState::default()
            .color_write_mask(
                vk::ColorComponentFlags::R
                    | vk::ColorComponentFlags::G
                    | vk::ColorComponentFlags::B
                    | vk::ColorComponentFlags::A)
            .blend_enable(false)];

        let color_blend = vk::PipelineColorBlendStateCreateInfo::default()
            .logic_op_enable(false)
            .attachments(&color_blend_attachments);

        let pc_ranges = [vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::VERTEX)
            .offset(0)
            .size(std::mem::size_of::<[f32; 16]>() as u32)];

        let layout_create = vk::PipelineLayoutCreateInfo::default()
            .push_constant_ranges(&pc_ranges);
        let pipeline_layout = unsafe {
            ctx.device.create_pipeline_layout(&layout_create, None)
                .map_err(|e| RenderError::Pipeline(format!("create_pipeline_layout: {:?}", e)))?
        };

        let create = vk::GraphicsPipelineCreateInfo::default()
            .stages(&stages)
            .vertex_input_state(&vertex_input)
            .input_assembly_state(&input_assembly)
            .viewport_state(&viewport_state)
            .rasterization_state(&rasterizer)
            .multisample_state(&multisampling)
            .depth_stencil_state(&depth_stencil)
            .color_blend_state(&color_blend)
            .layout(pipeline_layout)
            .render_pass(render_pass)
            .subpass(0);

        let pipeline = unsafe {
            ctx.device.create_graphics_pipelines(
                vk::PipelineCache::null(),
                std::slice::from_ref(&create),
                None,
            ).map_err(|(_pipelines, e)| RenderError::Pipeline(format!("create_graphics_pipelines: {:?}", e)))?[0]
        };

        Ok(Self {
            render_pass,
            pipeline,
            pipeline_layout,
            vert_module,
            frag_module,
        })
    }

    pub fn destroy(&mut self, ctx: &VkContext) {
        unsafe {
            ctx.device.destroy_pipeline(self.pipeline, None);
            ctx.device.destroy_pipeline_layout(self.pipeline_layout, None);
            ctx.device.destroy_render_pass(self.render_pass, None);
            ctx.device.destroy_shader_module(self.vert_module, None);
            ctx.device.destroy_shader_module(self.frag_module, None);
        }
    }
}

// =============================================================================
// Swapchain (headless surface)
// =============================================================================

pub struct HeadlessSwapchain {
    pub surface: vk::SurfaceKHR,
    pub raw: vk::SwapchainKHR,
    pub images: Vec<vk::Image>,
    pub image_views: Vec<vk::ImageView>,
    pub format: vk::Format,
    pub extent: vk::Extent2D,
    pub headless_loader: headless_surface::Instance,
    pub surface_loader: ash::khr::surface::Instance,
    pub swapchain_loader: ash::khr::swapchain::Device,
}

impl HeadlessSwapchain {
    pub fn new(ctx: Arc<VkContext>, width: u32, height: u32) -> RenderResult<Self> {
        let headless_loader = headless_surface::Instance::new(&ctx.entry, &ctx.instance);
        let create = vk::HeadlessSurfaceCreateInfoEXT::default();
        let surface = unsafe {
            headless_loader.create_headless_surface(&create, None)
                .map_err(|e| RenderError::SurfaceCreate(format!("{:?}", e)))?
        };

        let surface_loader = ash::khr::surface::Instance::new(&ctx.entry, &ctx.instance);

        // query support
        let capabilities = unsafe {
            surface_loader.get_physical_device_surface_capabilities(ctx.physical_device, surface)
                .map_err(|e| RenderError::SurfaceCreate(format!("get_surface_capabilities: {:?}", e)))?
        };
        let formats = unsafe {
            surface_loader.get_physical_device_surface_formats(ctx.physical_device, surface)
                .map_err(|e| RenderError::SurfaceCreate(format!("get_surface_formats: {:?}", e)))?
        };
        let present_modes = unsafe {
            surface_loader.get_physical_device_surface_present_modes(ctx.physical_device, surface)
                .map_err(|e| RenderError::SurfaceCreate(format!("get_surface_present_modes: {:?}", e)))?
        };

        log::info!(
            "headless surface: formats={} present_modes={} caps={:?}",
            formats.len(), present_modes.len(), capabilities
        );

        let format = formats.iter()
            .find(|f| f.format == vk::Format::B8G8R8A8_UNORM)
            .or_else(|| formats.first())
            .copied()
            .ok_or_else(|| RenderError::SwapchainCreate("no surface formats".into()))?;

        let extent = vk::Extent2D {
            width: width.clamp(capabilities.min_image_extent.width,
                               capabilities.max_image_extent.width),
            height: height.clamp(capabilities.min_image_extent.height,
                                 capabilities.max_image_extent.height),
        };
        if extent.width == 0 || extent.height == 0 {
            return Err(RenderError::SwapchainCreate(format!(
                "Computed swapchain extent is zero: {:?}", extent)));
        }

        let mut image_count = capabilities.min_image_count + 1;
        if capabilities.max_image_count > 0
            && image_count > capabilities.max_image_count
        {
            image_count = capabilities.max_image_count;
        }

        let present_mode = if present_modes.contains(&vk::PresentModeKHR::MAILBOX) {
            vk::PresentModeKHR::MAILBOX
        } else if present_modes.contains(&vk::PresentModeKHR::IMMEDIATE) {
            vk::PresentModeKHR::IMMEDIATE
        } else {
            vk::PresentModeKHR::FIFO
        };

        log::info!(
            "headless swapchain: {}x{} {:?} images={} present_mode={:?}",
            extent.width, extent.height, format.format, image_count, present_mode
        );

        // Create swapchain (need to enable VK_KHR_swapchain on the device too)
        let swap_ext = CString::new("VK_KHR_swapchain").unwrap();
        let ext_names: [*const i8; 1] = [swap_ext.as_ptr()];

        // Re-create device with the swapchain extension enabled if not already.
        // We need to do this before creating the swapchain.
        // Check if already enabled — but we know we didn't enable it earlier.
        // For now, we recreate the device with the extension added.
        // SAFETY: the existing device is not used afterwards, so we leak it.
        // Actually a better approach is to enable the extension up-front in
        // VkContext::new. We'll do that.

        // For now we hack: directly create a swapchain via the KHR device extension.
        // If the device wasn't created with VK_KHR_swapchain, this will fail
        // with VK_ERROR_INITIALIZATION_FAILED. We'll handle that.

        let create_swap = vk::SwapchainCreateInfoKHR::default()
            .surface(surface)
            .min_image_count(image_count)
            .image_format(format.format)
            .image_color_space(format.color_space)
            .image_extent(extent)
            .image_array_layers(1)
            .image_usage(
                vk::ImageUsageFlags::COLOR_ATTACHMENT
                    | vk::ImageUsageFlags::TRANSFER_SRC
                    | vk::ImageUsageFlags::TRANSFER_DST,
            )
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .pre_transform(capabilities.current_transform)
            .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
            .present_mode(present_mode)
            .clipped(true);

        let swapchain_loader = ash::khr::swapchain::Device::new(&ctx.instance, &ctx.device);
        let raw = unsafe {
            swapchain_loader.create_swapchain(&create_swap, None)
                .map_err(|e| {
                    log::error!("create_swapchain failed: {:?} — likely VK_KHR_swapchain not enabled on device", e);
                    RenderError::SwapchainCreate(format!("{:?}", e))
                })?
        };

        let images = unsafe {
            swapchain_loader.get_swapchain_images(raw)
                .map_err(|e| RenderError::SwapchainCreate(format!("get_swapchain_images: {:?}", e)))?
        };

        let mut image_views = Vec::with_capacity(images.len());
        for img in &images {
            let view_create = vk::ImageViewCreateInfo::default()
                .image(*img)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(format.format)
                .components(vk::ComponentMapping::default())
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                });
            let view = unsafe {
                ctx.device.create_image_view(&view_create, None)
                    .map_err(|e| RenderError::SwapchainCreate(format!("create_image_view: {:?}", e)))?
            };
            image_views.push(view);
        }

        let _ = ext_names;
        Ok(Self {
            surface,
            raw,
            images,
            image_views,
            format: format.format,
            extent,
            headless_loader,
            surface_loader,
            swapchain_loader,
        })
    }

    pub fn acquire_next_image(
        &self,
        _ctx: &VkContext,
        semaphore: vk::Semaphore,
    ) -> RenderResult<u32> {
        // The Vulkan acquire call lives on `swapchain_loader`, which already
        // holds a reference to the logical device. The `ctx` parameter is kept
        // in the signature so future swapchain-recreation paths can consult the
        // physical-device capabilities when the surface is invalidated.
        unsafe {
            match self.swapchain_loader.acquire_next_image(self.raw, u64::MAX, semaphore, vk::Fence::null()) {
                Ok((idx, _suboptimal)) => Ok(idx),
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => Ok(0),
                Err(e) => Err(RenderError::AcquireImage(format!("{:?}", e))),
            }
        }
    }

    pub fn present(&self, ctx: &VkContext, image_index: u32, wait_semaphores: &[vk::Semaphore]) -> RenderResult<()> {
        let present_info = vk::PresentInfoKHR::default()
            .wait_semaphores(wait_semaphores)
            .swapchains(std::slice::from_ref(&self.raw))
            .image_indices(std::slice::from_ref(&image_index));
        unsafe {
            self.swapchain_loader.queue_present(ctx.graphics_queue, &present_info)
                .map_err(|e| RenderError::Present(format!("{:?}", e)))?;
        }
        // `ctx` is required by the API surface even though we only need the
        // graphics queue (which lives on `ctx`). Keep it explicit so future
        // multi-queue variants can plug in here without rewriting the
        // signature.
        let _ = ctx;
        Ok(())
    }
}

impl Drop for HeadlessSwapchain {
    fn drop(&mut self) {
        // SAFETY: only call if device is alive. We assume VkContext outlives
        // the swapchain (it does — VkContext is held in Arc inside Backend).
        // But we don't have the device here. So callers must use destroy().
    }
}

impl HeadlessSwapchain {
    pub fn destroy(&mut self, ctx: &VkContext) {
        unsafe {
            for v in self.image_views.drain(..) {
                ctx.device.destroy_image_view(v, None);
            }
            self.swapchain_loader.destroy_swapchain(self.raw, None);
            self.surface_loader.destroy_surface(self.surface, None);
        }
    }
}

// =============================================================================
// Command pool + sync
// =============================================================================

pub struct Frame {
    pub command_pool: vk::CommandPool,
    pub command_buffers: Vec<vk::CommandBuffer>,
    pub image_available: Vec<vk::Semaphore>,
    pub render_finished: Vec<vk::Semaphore>,
    pub in_flight: Vec<vk::Fence>,
    pub framebuffers: Vec<vk::Framebuffer>,
    pub current: usize,
    pub frames_in_flight: usize,
}

impl Frame {
    pub fn new(
        ctx: &VkContext,
        extent: vk::Extent2D,
        render_pass: vk::RenderPass,
        image_views: &[vk::ImageView],
        depth_view: vk::ImageView,
        frames_in_flight: usize,
    ) -> RenderResult<Self> {
        let pool_create = vk::CommandPoolCreateInfo::default()
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
            .queue_family_index(ctx.graphics_queue_family);
        let command_pool = unsafe {
            ctx.device.create_command_pool(&pool_create, None)
                .map_err(|e| RenderError::Other(format!("create_command_pool: {:?}", e)))?
        };

        let mut command_buffers = Vec::with_capacity(frames_in_flight);
        for _ in 0..frames_in_flight {
            let alloc = vk::CommandBufferAllocateInfo::default()
                .command_pool(command_pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_buffer_count(1);
            let cmd = unsafe {
                ctx.device.allocate_command_buffers(&alloc)
                    .map_err(|e| RenderError::Other(format!("allocate_command_buffers: {:?}", e)))?[0]
            };
            command_buffers.push(cmd);
        }

        let mut image_available = Vec::with_capacity(frames_in_flight);
        let mut render_finished = Vec::with_capacity(frames_in_flight);
        let mut in_flight = Vec::with_capacity(frames_in_flight);
        let sem_create = vk::SemaphoreCreateInfo::default();
        let fence_create = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);
        for _ in 0..frames_in_flight {
            let s1 = unsafe { ctx.device.create_semaphore(&sem_create, None)
                .map_err(|e| RenderError::Other(format!("create_semaphore: {:?}", e)))? };
            let s2 = unsafe { ctx.device.create_semaphore(&sem_create, None)
                .map_err(|e| RenderError::Other(format!("create_semaphore: {:?}", e)))? };
            let f = unsafe { ctx.device.create_fence(&fence_create, None)
                .map_err(|e| RenderError::Other(format!("create_fence: {:?}", e)))? };
            image_available.push(s1);
            render_finished.push(s2);
            in_flight.push(f);
        }

        // Each framebuffer gets BOTH the color image view (per swapchain image)
        // AND the shared depth image view. The render pass references them by
        // index: 0 = color, 1 = depth. The depth image is shared across all
        // framebuffers because we serialize GPU work with fences and never
        // access the depth attachment from two frames concurrently.
        let mut framebuffers = Vec::with_capacity(image_views.len());
        for view in image_views {
            let attachments = [*view, depth_view];
            let fb_create = vk::FramebufferCreateInfo::default()
                .render_pass(render_pass)
                .attachments(&attachments)
                .width(extent.width)
                .height(extent.height)
                .layers(1);
            let fb = unsafe {
                ctx.device.create_framebuffer(&fb_create, None)
                    .map_err(|e| RenderError::Pipeline(format!("create_framebuffer: {:?}", e)))?
            };
            framebuffers.push(fb);
        }

        Ok(Self {
            command_pool,
            command_buffers,
            image_available,
            render_finished,
            in_flight,
            framebuffers,
            current: 0,
            frames_in_flight,
        })
    }

    pub fn destroy(&mut self, ctx: &VkContext) {
        unsafe {
            for fb in self.framebuffers.drain(..) {
                ctx.device.destroy_framebuffer(fb, None);
            }
            for s in self.image_available.drain(..) {
                ctx.device.destroy_semaphore(s, None);
            }
            for s in self.render_finished.drain(..) {
                ctx.device.destroy_semaphore(s, None);
            }
            for f in self.in_flight.drain(..) {
                ctx.device.destroy_fence(f, None);
            }
            ctx.device.destroy_command_pool(self.command_pool, None);
        }
    }
}

// =============================================================================
// Browser Render Observatory
// =============================================================================

pub struct FrameSnapshot {
    pub bytes: Vec<u8>,    // BGRA8
    pub width: u32,
    pub height: u32,
    pub frame_index: u64,
}

impl Default for FrameSnapshot {
    fn default() -> Self {
        Self { bytes: Vec::new(), width: 0, height: 0, frame_index: 0 }
    }
}

pub struct Observatory {
    pub latest: Arc<Mutex<FrameSnapshot>>,
    pub _server: Option<std::thread::JoinHandle<()>>,
}

impl Observatory {
    pub fn start(addr: &str) -> Self {
        let latest: Arc<Mutex<FrameSnapshot>> = Arc::new(Mutex::new(FrameSnapshot::default()));
        let latest_inner = latest.clone();
        let addr_owned: String = addr.to_string();

        let handle = std::thread::spawn(move || {
            let server = match Server::http(addr_owned.as_str()) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Observatory server bind failed: {e:?}");
                    return;
                }
            };
            for request in server.incoming_requests() {
                let url = request.url().to_string();
                let (resp, ct): (Vec<u8>, &'static str) = if url == "/" {
                    (HTML_VIEWER.as_bytes().to_vec(), "text/html; charset=utf-8")
                } else if url == "/frame.png" {
                    let snap = latest_inner.lock();
                    if snap.bytes.is_empty() {
                        (b"not yet rendered".to_vec(), "text/plain")
                    } else {
                        let png = bgra_to_png(&snap.bytes, snap.width, snap.height);
                        (png, "image/png")
                    }
                } else if url == "/frame.raw" {
                    let snap = latest_inner.lock();
                    (snap.bytes.clone(), "application/octet-stream")
                } else if url == "/debug/state" {
                    let snap = latest_inner.lock();
                    let json = format!(
                        r#"{{"width":{},"height":{},"frame_index":{},"bytes":{}}}"#,
                        snap.width, snap.height, snap.frame_index, snap.bytes.len()
                    );
                    (json.into_bytes(), "application/json")
                } else {
                    (b"not found".to_vec(), "text/plain")
                };
                let mut response = Response::from_data(resp);
                let _ = response.add_header(Header::from_bytes(
                    b"Content-Type", ct.as_bytes()
                ).unwrap());
                let _ = request.respond(response);
            }
        });

        Self { latest, _server: Some(handle) }
    }

    pub fn publish(&self, bytes: Vec<u8>, width: u32, height: u32, frame_index: u64) {
        let mut snap = self.latest.lock();
        snap.bytes = bytes;
        snap.width = width;
        snap.height = height;
        snap.frame_index = frame_index;
    }
}

fn bgra_to_png(bgra: &[u8], width: u32, height: u32) -> Vec<u8> {
    use image::ImageEncoder;
    let mut rgba = Vec::with_capacity(bgra.len());
    for chunk in bgra.chunks_exact(4) {
        rgba.push(chunk[2]); // R
        rgba.push(chunk[1]); // G
        rgba.push(chunk[0]); // B
        rgba.push(chunk[3]); // A
    }
    let mut out = Vec::new();
    let enc = image::codecs::png::PngEncoder::new(&mut out);
    enc.write_image(&rgba, width, height, image::ExtendedColorType::Rgba8)
        .expect("png encode");
    out
}

const HTML_VIEWER: &str = r#"<!doctype html>
<html>
<head>
<meta charset="utf-8">
<title>Arcane — Render Observatory</title>
<style>
  body { background:#08080c; color:#e6e6f0; font-family:system-ui,sans-serif; margin:0; padding:16px; }
  h1 { font-size:14pt; margin:0 0 8pt; }
  .meta { font-size:10pt; color:#999; margin-bottom:12pt; }
  canvas { image-rendering: pixelated; width:800px; height:600px; border:1px solid #333; }
  .controls { margin:8pt 0; }
  button { background:#222; color:#eee; border:1px solid #444; padding:4pt 8pt; cursor:pointer; }
</style>
</head>
<body>
<h1>Mystical Arcana &mdash; Render Observatory</h1>
<div class="meta" id="meta">Loading...</div>
<div class="controls">
  <button id="toggle">Pause</button>
  <input type="range" id="fps" min="1" max="60" value="10"> <span id="fpsLabel">10 FPS</span>
</div>
<canvas id="c" width="800" height="600"></canvas>

<script>
let paused = false;
let lastFrameIdx = -1;
const canvas = document.getElementById('c');
const ctx = canvas.getContext('2d');
const meta = document.getElementById('meta');
const fpsSlider = document.getElementById('fps');
const fpsLabel = document.getElementById('fpsLabel');

function delay(ms) { return new Promise(r => setTimeout(r, ms)); }

async function loadFrame() {
  try {
    const resp = await fetch('/frame.raw', { cache: 'no-store' });
    if (!resp.ok) { meta.textContent = 'frame fetch failed: ' + resp.status; return; }
    const buf = await resp.arrayBuffer();
    const state = await (await fetch('/debug/state', { cache: 'no-store' })).json();
    if (state.frame_index !== lastFrameIdx) {
      lastFrameIdx = state.frame_index;
      meta.textContent = `frame ${state.frame_index} (${state.width}x${state.height})`;
    }
    if (state.width === 0 || state.height === 0) return;
    const u8 = new Uint8Array(buf);
    const rgba = new Uint8ClampedArray(u8.length);
    for (let i = 0; i < u8.length; i += 4) {
      rgba[i] = u8[i+2];
      rgba[i+1] = u8[i+1];
      rgba[i+2] = u8[i+0];
      rgba[i+3] = u8[i+3];
    }
    const img = new ImageData(rgba, state.width, state.height);
    canvas.width = state.width;
    canvas.height = state.height;
    ctx.putImageData(img, 0, 0);
  } catch (e) {
    meta.textContent = 'error: ' + e;
  }
}

async function loop() {
  while (true) {
    if (!paused) await loadFrame();
    const ms = 1000 / parseInt(fpsSlider.value, 10);
    fpsLabel.textContent = fpsSlider.value + ' FPS';
    await delay(ms);
  }
}

document.getElementById('toggle').addEventListener('click', () => {
  paused = !paused;
  document.getElementById('toggle').textContent = paused ? 'Resume' : 'Pause';
});

loop();
</script>
</body>
</html>
"#;

// =============================================================================
// Backend
// =============================================================================

pub struct Backend {
    pub ctx: Arc<VkContext>,
    pub swapchain: HeadlessSwapchain,
    pub depth: DepthBuffer,
    pub pipeline: Pipeline,
    pub frame: Frame,
    pub mesh: TriangleMesh,
    pub observatory: Observatory,
    pub readback: Buffer,
    pub width: u32,
    pub height: u32,
}

impl Backend {
    pub fn new(width: u32, height: u32) -> RenderResult<Self> {
        Self::with_observatory(width, height, "0.0.0.0:8080")
    }

    /// Same as [`Backend::new`] but lets the caller pick the Observatory bind
    /// address. Pass e.g. `"127.0.0.1:9999"` to scope to localhost and a
    /// non-default port, or `"0.0.0.0:8080"` (the default) to expose to the
    /// host network.
    pub fn with_observatory(
        width: u32,
        height: u32,
        observatory_addr: &str,
    ) -> RenderResult<Self> {
        let ctx = VkContext::new("Mystical Arcana", cfg!(debug_assertions))?;
        log::info!("Vulkan context: device = {}", ctx.device_name());

        let swapchain = HeadlessSwapchain::new(ctx.clone(), width, height)?;
        let depth = DepthBuffer::new(&ctx, swapchain.extent)?;
        let pipeline = Pipeline::new(&ctx, swapchain.format, depth.format, swapchain.extent)?;
        let frame = Frame::new(
            &ctx,
            swapchain.extent,
            pipeline.render_pass,
            &swapchain.image_views,
            depth.view,
            2,
        )?;
        let mesh = TriangleMesh::new(&ctx)?;
        let observatory = Observatory::start(observatory_addr);

        let bytes_needed = (width as u64) * (height as u64) * 4;
        let readback = Buffer::new(
            &ctx,
            bytes_needed,
            vk::BufferUsageFlags::TRANSFER_DST,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )?;

        Ok(Self {
            ctx,
            swapchain,
            depth,
            pipeline,
            frame,
            mesh,
            observatory,
            readback,
            width,
            height,
        })
    }

    pub fn render_one(&mut self, frame_index: u64) -> RenderResult<()> {
        let mvp: [f32; 16] = [
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ];

        let current = self.frame.current;
        let in_flight = self.frame.in_flight[current];
        let image_available = self.frame.image_available[current];
        let render_finished = self.frame.render_finished[current];
        let cmd = self.frame.command_buffers[current];

        unsafe {
            self.ctx.device.wait_for_fences(std::slice::from_ref(&in_flight), true, u64::MAX)
                .map_err(|e| RenderError::Other(format!("wait_for_fences: {:?}", e)))?;
            self.ctx.device.reset_fences(std::slice::from_ref(&in_flight))
                .map_err(|e| RenderError::Other(format!("reset_fences: {:?}", e)))?;
        }

        let image_index = self.swapchain.acquire_next_image(&self.ctx, image_available)?;

        unsafe {
            self.ctx.device.reset_command_buffer(cmd, vk::CommandBufferResetFlags::empty())
                .map_err(|e| RenderError::Other(format!("reset_command_buffer: {:?}", e)))?;
        }

        self.record_draw(cmd, image_index, &mvp)?;

        let wait_stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
        let wait_semaphores = [image_available];
        let signal_semaphores = [render_finished];
        let cmd_buffers = [cmd];
        let submit_info = vk::SubmitInfo::default()
            .wait_semaphores(&wait_semaphores)
            .wait_dst_stage_mask(&wait_stages)
            .command_buffers(&cmd_buffers)
            .signal_semaphores(&signal_semaphores);
        unsafe {
            self.ctx.device.queue_submit(
                self.ctx.graphics_queue,
                std::slice::from_ref(&submit_info),
                in_flight,
            ).map_err(|e| RenderError::Submit(format!("{:?}", e)))?;
        }

        self.swapchain.present(&self.ctx, image_index, &[render_finished])?;

        // Readback: copy the swapchain image back to host-visible buffer.
        self.readback_image(image_index)?;

        // Publish to observatory.
        let bytes = self.readback.read_bytes().to_vec();
        self.observatory.publish(bytes, self.width, self.height, frame_index);

        self.frame.current = (self.frame.current + 1) % self.frame.frames_in_flight;
        Ok(())
    }

    fn record_draw(
        &self,
        cmd: vk::CommandBuffer,
        image_index: u32,
        mvp: &[f32; 16],
    ) -> RenderResult<()> {
        let extent = self.swapchain.extent;
        let framebuffer = self.frame.framebuffers[image_index as usize];

        let clear_values = [
            vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: [0.05, 0.05, 0.08, 1.0],
                },
            },
            // Depth clear = 1.0 (far plane). LESS compare means a fragment
            // passes only if its depth is < the buffer value, so starting
            // from 1.0 means everything in front of the far plane passes
            // on the first write.
            vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: 1.0,
                    stencil: 0,
                },
            },
        ];

        let render_begin = vk::RenderPassBeginInfo::default()
            .render_pass(self.pipeline.render_pass)
            .framebuffer(framebuffer)
            .render_area(vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent,
            })
            .clear_values(&clear_values);

        let begin_info = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);

        unsafe {
            self.ctx.device.begin_command_buffer(cmd, &begin_info)
                .map_err(|e| RenderError::Other(format!("begin_command_buffer: {:?}", e)))?;
            self.ctx.device.cmd_begin_render_pass(cmd, &render_begin, vk::SubpassContents::INLINE);
            self.ctx.device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.pipeline.pipeline);
            self.ctx.device.cmd_bind_vertex_buffers(cmd, 0, std::slice::from_ref(&self.mesh.vertex.raw), &[0]);
            self.ctx.device.cmd_bind_index_buffer(cmd, self.mesh.index.raw, 0, vk::IndexType::UINT16);
            let mvp_bytes: &[u8] = std::slice::from_raw_parts(
                mvp.as_ptr() as *const u8,
                std::mem::size_of::<[f32; 16]>(),
            );
            self.ctx.device.cmd_push_constants(
                cmd,
                self.pipeline.pipeline_layout,
                vk::ShaderStageFlags::VERTEX,
                0,
                mvp_bytes,
            );
            self.ctx.device.cmd_draw_indexed(cmd, self.mesh.index_count, 1, 0, 0, 0);
            self.ctx.device.cmd_end_render_pass(cmd);
            self.ctx.device.end_command_buffer(cmd)
                .map_err(|e| RenderError::Other(format!("end_command_buffer: {:?}", e)))?;
        }
        Ok(())
    }

    fn readback_image(&self, image_index: u32) -> RenderResult<()> {
        let extent = self.swapchain.extent;
        let image = self.swapchain.images[image_index as usize];

        // Allocate a one-shot command buffer from the existing pool.
        let alloc = vk::CommandBufferAllocateInfo::default()
            .command_pool(self.frame.command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);
        let cmd = unsafe {
            self.ctx.device.allocate_command_buffers(&alloc)
                .map_err(|e| RenderError::Other(format!("allocate_command_buffers (readback): {:?}", e)))?[0]
        };

        unsafe {
            let begin = vk::CommandBufferBeginInfo::default()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
            self.ctx.device.begin_command_buffer(cmd, &begin)
                .map_err(|e| RenderError::Other(format!("begin_command_buffer (readback): {:?}", e)))?;

            // Image layout: PRESENT_SRC_KHR -> TRANSFER_SRC_OPTIMAL
            let barrier = vk::ImageMemoryBarrier::default()
                .image(image)
                .old_layout(vk::ImageLayout::PRESENT_SRC_KHR)
                .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .src_access_mask(vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE)
                .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                });
            self.ctx.device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::ALL_COMMANDS,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                std::slice::from_ref(&barrier),
            );

            let region = vk::BufferImageCopy::default()
                .image_subresource(vk::ImageSubresourceLayers {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    mip_level: 0,
                    base_array_layer: 0,
                    layer_count: 1,
                })
                .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
                .image_extent(vk::Extent3D {
                    width: extent.width,
                    height: extent.height,
                    depth: 1,
                });
            self.ctx.device.cmd_copy_image_to_buffer(
                cmd,
                image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                self.readback.raw,
                std::slice::from_ref(&region),
            );

            // Image back to PRESENT_SRC_KHR
            let barrier2 = vk::ImageMemoryBarrier::default()
                .image(image)
                .old_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .src_access_mask(vk::AccessFlags::TRANSFER_READ)
                .dst_access_mask(vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                });
            self.ctx.device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::ALL_COMMANDS,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                std::slice::from_ref(&barrier2),
            );

            self.ctx.device.end_command_buffer(cmd)
                .map_err(|e| RenderError::Other(format!("end_command_buffer (readback): {:?}", e)))?;
        }

        let submit_info = vk::SubmitInfo::default()
            .command_buffers(std::slice::from_ref(&cmd));
        unsafe {
            self.ctx.device.queue_submit(
                self.ctx.graphics_queue,
                std::slice::from_ref(&submit_info),
                vk::Fence::null(),
            ).map_err(|e| RenderError::Submit(format!("readback submit: {:?}", e)))?;
            self.ctx.device.queue_wait_idle(self.ctx.graphics_queue)
                .map_err(|e| RenderError::Other(format!("readback wait: {:?}", e)))?;
            self.ctx.device.free_command_buffers(self.frame.command_pool, std::slice::from_ref(&cmd));
        }
        Ok(())
    }
}

impl Drop for Backend {
    fn drop(&mut self) {
        // Wait for device idle before any destruction
        unsafe { let _ = self.ctx.device.device_wait_idle(); }
        // Drop order is the reverse of construction:
        //   readback (host buffer)
        //   mesh (vertex/index buffers)
        //   pipeline (graphics pipeline + render pass + shaders + pipeline layout)
        //   frame (framebuffers + sync objects + command pool)
        //   depth (depth image + view + memory)
        //   swapchain (surface + image views + raw swapchain)
        // The VkContext (instance + device + physical device) outlives all
        // of these because it's held in an Arc and dropped last when the
        // Backend is consumed.
        self.readback.destroy(&self.ctx);
        self.mesh.destroy(&self.ctx);
        self.pipeline.destroy(&self.ctx);
        self.frame.destroy(&self.ctx);
        self.depth.destroy(&self.ctx);
        self.swapchain.destroy(&self.ctx);
    }
}
