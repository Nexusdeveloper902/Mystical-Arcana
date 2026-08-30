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
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub color: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

impl Vertex {
    pub fn new(pos: [f32; 3], color: [f32; 3], normal: [f32; 3]) -> Self {
        Self { pos, color, normal, uv: [0.0, 0.0] }
    }
    pub fn with_uv(pos: [f32; 3], color: [f32; 3], normal: [f32; 3], uv: [f32; 2]) -> Self {
        Self { pos, color, normal, uv }
    }
}

pub struct TriangleMesh {
    pub vertex: Buffer,
    pub index: Buffer,
    pub index_count: u32,
}

impl TriangleMesh {
    pub fn new(ctx: &VkContext) -> RenderResult<Self> {
        // Flat triangle in the Z=0 plane, CCW winding, facing +Z.
        // UVs map the triangle to a sub-rectangle of the texture.
        let vertices: [Vertex; 3] = [
            Vertex::with_uv([ 0.5, -0.5, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0]),
            Vertex::with_uv([-0.5, -0.5, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0]),
            Vertex::with_uv([ 0.0,  0.5, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0, 1.0], [0.5, 1.0]),
        ];
        let indices: [u16; 3] = [0, 1, 2];
        MeshBuilder::new(ctx)
            .vertices(&vertices)
            .indices_u16(&indices)
            .build()
            .map(|m| TriangleMesh { vertex: m.vertex, index: m.index, index_count: m.index_count })
    }

    pub fn destroy(&mut self, ctx: &VkContext) {
        self.vertex.destroy(ctx);
        self.index.destroy(ctx);
    }
}

/// Generic mesh: vertex + index buffers + index count. Constructed by
/// [`MeshBuilder`] from arbitrary vertex/index data. The caller owns the
/// mesh and is responsible for calling [`Mesh::destroy`] before the
/// underlying `VkContext` is dropped.
pub struct Mesh {
    pub vertex: Buffer,
    pub index: Buffer,
    pub index_count: u32,
}

impl Mesh {
    pub fn destroy(&mut self, ctx: &VkContext) {
        self.vertex.destroy(ctx);
        self.index.destroy(ctx);
    }
}

/// Builder for [`Mesh`]. Streams vertex + index data into HOST_VISIBLE
/// HOST_COHERENT buffers (lavapipe is CPU-backed so this is fine; on a
/// real GPU we'd stage through a transfer queue and a DEVICE_LOCAL buffer).
pub struct MeshBuilder<'a> {
    ctx: &'a VkContext,
    vertices_bytes: Vec<u8>,
    indices_u16: Vec<u16>,
    indices_u32: Vec<u32>,
    index_type: vk::IndexType,
}

impl<'a> MeshBuilder<'a> {
    pub fn new(ctx: &'a VkContext) -> Self {
        Self {
            ctx,
            vertices_bytes: Vec::new(),
            indices_u16: Vec::new(),
            indices_u32: Vec::new(),
            index_type: vk::IndexType::UINT16,
        }
    }

    /// Append a slice of vertices (any `#[repr(C)]` struct). The bytes are
    /// copied as-is; the caller is responsible for matching the vertex
    /// input binding description's stride and attribute offsets to the
    /// struct's layout.
    pub fn vertices<V: bytemuck::Pod>(mut self, vs: &[V]) -> Self {
        let bytes: &[u8] = bytemuck::cast_slice(vs);
        self.vertices_bytes.extend_from_slice(bytes);
        self
    }

    pub fn indices_u16(mut self, idx: &[u16]) -> Self {
        self.indices_u16.extend_from_slice(idx);
        self.index_type = vk::IndexType::UINT16;
        self
    }

    pub fn indices_u32(mut self, idx: &[u32]) -> Self {
        self.indices_u32.extend_from_slice(idx);
        self.index_type = vk::IndexType::UINT32;
        self
    }

    pub fn build(self) -> RenderResult<Mesh> {
        let v_size = self.vertices_bytes.len() as vk::DeviceSize;
        if v_size == 0 {
            return Err(RenderError::Allocator("mesh has no vertices".into()));
        }

        let (index_bytes, index_count, index_type) = if !self.indices_u16.is_empty() {
            let bytes: &[u8] = bytemuck::cast_slice(&self.indices_u16);
            (bytes.to_vec(), self.indices_u16.len() as u32, vk::IndexType::UINT16)
        } else if !self.indices_u32.is_empty() {
            let bytes: &[u8] = bytemuck::cast_slice(&self.indices_u32);
            (bytes.to_vec(), self.indices_u32.len() as u32, vk::IndexType::UINT32)
        } else {
            (Vec::new(), 0, vk::IndexType::NONE_NV)
        };

        let mut vertex = Buffer::new(
            self.ctx,
            v_size.max(16),
            vk::BufferUsageFlags::VERTEX_BUFFER,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )?;
        vertex.write_bytes(&self.vertices_bytes)?;

        let index = if !index_bytes.is_empty() {
            let i_size = index_bytes.len() as vk::DeviceSize;
            let mut buf = Buffer::new(
                self.ctx,
                i_size,
                vk::BufferUsageFlags::INDEX_BUFFER,
                vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            )?;
            buf.write_bytes(&index_bytes)?;
            buf
        } else {
            // No indices — allocate a 16-byte placeholder so the buffer
            // is valid even if the caller never binds it.
            Buffer::new(
                self.ctx,
                16,
                vk::BufferUsageFlags::INDEX_BUFFER,
                vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            )?
        };

        let _ = index_type;
        Ok(Mesh {
            vertex,
            index,
            index_count,
        })
    }
}

/// A unit cube centered at the origin, with extents +/- 1 on each axis.
/// Each face has a distinct per-vertex color (R/G/B/Cyan/Magenta/Yellow)
/// so the cube is easy to visually inspect without textures or lighting.
///
/// Vertex layout per face: 4 vertices (bottom-left, bottom-right, top-right,
/// top-left), wound CCW when viewed from outside the cube. Face order:
///   0: +X (right)  — yellow
///   1: -X (left)   — cyan
///   2: +Y (bottom in Vulkan NDC, since Y points down) — magenta
///   3: -Y (top)    — red
///   4: +Z (front)  — green
///   5: -Z (back)   — blue
///
/// With BACK-face culling + CCW front-face winding, only the outward-
/// facing side of each face is rasterized, so the cube looks correct
/// from any viewing angle.
pub struct CubeMesh {
    pub mesh: Mesh,
}

impl CubeMesh {
    pub fn new(ctx: &VkContext) -> RenderResult<Self> {
        // 24 vertices (4 per face x 6 faces) so each face has its own
        // vertex colors and normals (no shared corners). Indexed by 36
        // uint16s (2 triangles per face x 3 indices x 6 faces).
        let s = 1.0f32;

        // Per-face colors (sRGB-ish primaries for visual debugging):
        let c_right  = [1.0, 1.0, 0.0]; // +X = yellow
        let c_left   = [0.0, 1.0, 1.0]; // -X = cyan
        let c_bottom = [1.0, 0.0, 1.0]; // +Y = magenta (Vulkan Y down → bottom)
        let c_top    = [1.0, 0.0, 0.0]; // -Y = red
        let c_front  = [0.0, 1.0, 0.0]; // +Z = green
        let c_back   = [0.0, 0.0, 1.0]; // -Z = blue

        // Per-face outward normals (unit vectors along each face's axis).
        let n_right  = [ 1.0,  0.0,  0.0];
        let n_left   = [-1.0,  0.0,  0.0];
        let n_bottom = [ 0.0,  1.0,  0.0];
        let n_top    = [ 0.0, -1.0,  0.0];
        let n_front  = [ 0.0,  0.0,  1.0];
        let n_back   = [ 0.0,  0.0, -1.0];

        // Vertex winding: each face is 4 verts in BL, BR, TR, TL order
        // (CCW when viewed from outside). The two triangles per face are
        // (BL, BR, TR) and (BL, TR, TL).
        let mut verts: Vec<Vertex> = Vec::with_capacity(24);

        // Per-face UV pattern (BL, BR, TR, TL): each face maps the
        // whole texture onto itself so the checker is visible on every
        // face at the same scale.
        let uv_bl = [0.0, 0.0];
        let uv_br = [1.0, 0.0];
        let uv_tr = [1.0, 1.0];
        let uv_tl = [0.0, 1.0];

        // +X face (right)
        verts.push(Vertex::with_uv([ s, -s, -s], c_right,  n_right,  uv_bl));
        verts.push(Vertex::with_uv([ s, -s,  s], c_right,  n_right,  uv_br));
        verts.push(Vertex::with_uv([ s,  s,  s], c_right,  n_right,  uv_tr));
        verts.push(Vertex::with_uv([ s,  s, -s], c_right,  n_right,  uv_tl));
        // -X face (left)
        verts.push(Vertex::with_uv([-s, -s,  s], c_left,   n_left,   uv_bl));
        verts.push(Vertex::with_uv([-s, -s, -s], c_left,   n_left,   uv_br));
        verts.push(Vertex::with_uv([-s,  s, -s], c_left,   n_left,   uv_tr));
        verts.push(Vertex::with_uv([-s,  s,  s], c_left,   n_left,   uv_tl));
        // +Y face (bottom in Vulkan NDC)
        verts.push(Vertex::with_uv([-s,  s,  s], c_bottom, n_bottom, uv_bl));
        verts.push(Vertex::with_uv([-s,  s, -s], c_bottom, n_bottom, uv_br));
        verts.push(Vertex::with_uv([ s,  s, -s], c_bottom, n_bottom, uv_tr));
        verts.push(Vertex::with_uv([ s,  s,  s], c_bottom, n_bottom, uv_tl));
        // -Y face (top in Vulkan NDC)
        verts.push(Vertex::with_uv([-s, -s, -s], c_top,    n_top,    uv_bl));
        verts.push(Vertex::with_uv([-s, -s,  s], c_top,    n_top,    uv_br));
        verts.push(Vertex::with_uv([ s, -s,  s], c_top,    n_top,    uv_tr));
        verts.push(Vertex::with_uv([ s, -s, -s], c_top,    n_top,    uv_tl));
        // +Z face (front)
        verts.push(Vertex::with_uv([ s, -s,  s], c_front,  n_front,  uv_bl));
        verts.push(Vertex::with_uv([-s, -s,  s], c_front,  n_front,  uv_br));
        verts.push(Vertex::with_uv([-s,  s,  s], c_front,  n_front,  uv_tr));
        verts.push(Vertex::with_uv([ s,  s,  s], c_front,  n_front,  uv_tl));
        // -Z face (back)
        verts.push(Vertex::with_uv([-s, -s, -s], c_back,   n_back,   uv_bl));
        verts.push(Vertex::with_uv([ s, -s, -s], c_back,   n_back,   uv_br));
        verts.push(Vertex::with_uv([ s,  s, -s], c_back,   n_back,   uv_tr));
        verts.push(Vertex::with_uv([-s,  s, -s], c_back,   n_back,   uv_tl));

        // Per-face index pattern: 0,1,2,0,2,3 (relative to face base).
        let face_idx = [0u16, 1, 2, 0, 2, 3];
        let mut indices: Vec<u16> = Vec::with_capacity(36);
        for f in 0..6u16 {
            let base = f * 4;
            for &i in &face_idx {
                indices.push(base + i);
            }
        }

        let mesh = MeshBuilder::new(ctx)
            .vertices(&verts)
            .indices_u16(&indices)
            .build()?;
        Ok(Self { mesh })
    }

    pub fn destroy(&mut self, ctx: &VkContext) {
        self.mesh.destroy(ctx);
    }
}

/// UV sphere with `stacks` latitude bands + `slices` longitude bands.
/// Vertex normals are the unit-position vectors (since the sphere is
/// centered at the origin with radius 1, normal = normalize(pos)). UVs
/// wrap the sphere once around so the procedural checker tiles cleanly.
pub struct SphereMesh {
    pub mesh: Mesh,
}

impl SphereMesh {
    pub fn new(ctx: &VkContext, stacks: u32, slices: u32) -> RenderResult<Self> {
        assert!(stacks >= 2 && slices >= 3);
        let mut verts: Vec<Vertex> = Vec::with_capacity(((stacks + 1) * (slices + 1)) as usize);
        let mut indices: Vec<u16> = Vec::with_capacity((stacks * slices * 6) as usize);
        let color = [0.85, 0.85, 0.92]; // cool off-white so the lighting reads
        // Stacks go from top (pi/2) to bottom (-pi/2). Slices go around Y (0..2pi).
        for i in 0..=stacks {
            let phi = std::f32::consts::PI * 0.5 - (i as f32 / stacks as f32) * std::f32::consts::PI;
            let y = phi.sin();
            let r = phi.cos(); // radius at this latitude
            let v = i as f32 / stacks as f32;
            for j in 0..=slices {
                let theta = (j as f32 / slices as f32) * std::f32::consts::TAU;
                let x = r * theta.cos();
                let z = r * theta.sin();
                let normal = [x, y, z];
                let uv = [j as f32 / slices as f32, v];
                verts.push(Vertex::with_uv([x, y, z], color, normal, uv));
            }
        }
        // Two triangles per quad (per stack x slice).
        for i in 0..stacks {
            for j in 0..slices {
                let a = (i * (slices + 1) + j) as u16;
                let b = (i * (slices + 1) + j + 1) as u16;
                let c = ((i + 1) * (slices + 1) + j + 1) as u16;
                let d = ((i + 1) * (slices + 1) + j) as u16;
                indices.extend_from_slice(&[a, b, d, b, c, d]);
            }
        }
        let mesh = MeshBuilder::new(ctx)
            .vertices(&verts)
            .indices_u16(&indices)
            .build()?;
        Ok(Self { mesh })
    }

    pub fn destroy(&mut self, ctx: &VkContext) {
        self.mesh.destroy(ctx);
    }
}

/// Square-base pyramid: 4 triangular side faces + 1 square base.
/// 16 vertices (4 per side face + 4 for the base) for distinct normals
/// + UVs per face. 18 indices (6 per side + 6 for the base = 24? no,
/// 4 sides * 3 + 2 base triangles * 3 = 12+6 = 18). Wound CCW from
/// outside so BACK-face culling keeps only outward-facing triangles.
pub struct PyramidMesh {
    pub mesh: Mesh,
}

impl PyramidMesh {
    pub fn new(ctx: &VkContext) -> RenderResult<Self> {
        let s = 1.0f32;
        let h = 1.5f32; // height above base
        let color_side = [0.95, 0.55, 0.25]; // warm orange
        let color_base = [0.35, 0.35, 0.40]; // dark grey
        // Apex (shared by all 4 side faces, but each side face keeps its
        // own copy so normals are distinct per face).
        let apex = [0.0, h, 0.0];
        let bl = [-s, 0.0, -s];
        let br = [ s, 0.0, -s];
        let tr = [ s, 0.0,  s];
        let tl = [-s, 0.0,  s];

        let mut verts: Vec<Vertex> = Vec::with_capacity(16);
        // Side face normals — point outward + up. Approximate by taking
        // the average of the two base edge directions cross the up axis.
        let n_front = [0.0,  0.5,  1.0];
        let n_right = [1.0,  0.5,  0.0];
        let n_back  = [0.0,  0.5, -1.0];
        let n_left  = [-1.0, 0.5,  0.0];
        let n_base  = [0.0, -1.0, 0.0];

        // +Z (front) face — apex + bl + tl (CCW from outside which is +Z).
        verts.push(Vertex::with_uv(apex, color_side, n_front, [0.5, 1.0]));
        verts.push(Vertex::with_uv(bl,  color_side, n_front, [0.0, 0.0]));
        verts.push(Vertex::with_uv(tl,  color_side, n_front, [1.0, 0.0]));
        verts.push(Vertex::with_uv(apex, color_side, n_front, [0.5, 1.0])); // pad
        // +X (right) face — apex + br + tr.
        verts.push(Vertex::with_uv(apex, color_side, n_right, [0.5, 1.0]));
        verts.push(Vertex::with_uv(br,  color_side, n_right, [0.0, 0.0]));
        verts.push(Vertex::with_uv(tr,  color_side, n_right, [1.0, 0.0]));
        verts.push(Vertex::with_uv(apex, color_side, n_right, [0.5, 1.0])); // pad
        // -Z (back) face — apex + br + tr (CCW from outside = -Z).
        verts.push(Vertex::with_uv(apex, color_side, n_back, [0.5, 1.0]));
        verts.push(Vertex::with_uv(br,  color_side, n_back, [0.0, 0.0]));
        verts.push(Vertex::with_uv(bl,  color_side, n_back, [1.0, 0.0]));
        verts.push(Vertex::with_uv(apex, color_side, n_back, [0.5, 1.0])); // pad
        // -X (left) face — apex + tl + bl.
        verts.push(Vertex::with_uv(apex, color_side, n_left, [0.5, 1.0]));
        verts.push(Vertex::with_uv(tl,  color_side, n_left, [0.0, 0.0]));
        verts.push(Vertex::with_uv(bl,  color_side, n_left, [1.0, 0.0]));
        verts.push(Vertex::with_uv(apex, color_side, n_left, [0.5, 1.0])); // pad

        // Base (looking up from below, CCW so culling keeps it visible
        // from below).
        let base_base = verts.len() as u16;
        verts.push(Vertex::with_uv(bl, color_base, n_base, [0.0, 0.0]));
        verts.push(Vertex::with_uv(br, color_base, n_base, [1.0, 0.0]));
        verts.push(Vertex::with_uv(tr, color_base, n_base, [1.0, 1.0]));
        verts.push(Vertex::with_uv(tl, color_base, n_base, [0.0, 1.0]));

        let mut indices: Vec<u16> = Vec::with_capacity(18);
        // 4 side triangles (3 verts each): apex, bl, tl etc — CCW.
        // Side 0 (+Z): apex, bl, tl
        indices.extend_from_slice(&[0, 1, 2]);
        // Side 1 (+X): apex, br, tr — but br and tr indices depend on
        // the order we pushed them above.
        indices.extend_from_slice(&[4, 5, 6]);
        // Side 2 (-Z): apex, tr, bl
        indices.extend_from_slice(&[8, 9, 10]);
        // Side 3 (-X): apex, tl, bl
        indices.extend_from_slice(&[12, 13, 14]);
        // Base: bl, tr, br (CCW from below)
        indices.extend_from_slice(&[base_base, base_base + 2, base_base + 1]);
        indices.extend_from_slice(&[base_base, base_base + 3, base_base + 2]);

        let mesh = MeshBuilder::new(ctx)
            .vertices(&verts)
            .indices_u16(&indices)
            .build()?;
        Ok(Self { mesh })
    }

    pub fn destroy(&mut self, ctx: &VkContext) {
        self.mesh.destroy(ctx);
    }
}

/// A flat horizontal plane of the given world-size, centered at the origin,
/// facing +Y (normal up). Useful as a ground plane under the scene.
/// Vertex winding is CCW when viewed from above (+Y side) so BACK-face
/// culling keeps it visible from above and discards it from below.
pub struct PlaneMesh {
    pub mesh: Mesh,
}

impl PlaneMesh {
    pub fn new(ctx: &VkContext, size_x: f32, size_z: f32) -> RenderResult<Self> {
        let hx = size_x * 0.5;
        let hz = size_z * 0.5;
        // 4 verts in BL, BR, TR, TL order (CCW from +Y looking down).
        // UVs repeat the texture 8x across the plane so the checker pattern
        // is small enough to read as a ground texture rather than one tile.
        let color = [0.7, 0.7, 0.7]; // neutral grey — modulated by texel
        let normal = [0.0, 1.0, 0.0]; // up
        let tile = 8.0;
        let verts: [Vertex; 4] = [
            Vertex::with_uv([-hx, 0.0,  hz], color, normal, [0.0,    0.0]),
            Vertex::with_uv([ hx, 0.0,  hz], color, normal, [tile,   0.0]),
            Vertex::with_uv([ hx, 0.0, -hz], color, normal, [tile,   tile]),
            Vertex::with_uv([-hx, 0.0, -hz], color, normal, [0.0,    tile]),
        ];
        let indices: [u16; 6] = [0, 1, 2, 0, 2, 3];
        let mesh = MeshBuilder::new(ctx)
            .vertices(&verts)
            .indices_u16(&indices)
            .build()?;
        Ok(Self { mesh })
    }

    pub fn destroy(&mut self, ctx: &VkContext) {
        self.mesh.destroy(ctx);
    }
}

/// Which built-in mesh to draw for a scene instance. The Backend owns
/// one of each primitive; the scene descriptor references them by kind so
/// the caller never needs to borrow mesh data out of the Backend (which
/// would conflict with the `&mut self` borrow that render_scene needs
/// for the frame-state mutation).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeshKind {
    Cube,
    Plane,
    Sphere,
    Pyramid,
    /// The OBJ-loaded mesh (octahedron by default). Lets the renderer
    /// exercise the arcane_assets OBJ parser without needing a real disk
    /// asset pipeline.
    LoadedObj,
}

/// One scene entry: which mesh + which model matrix to draw it with.
#[derive(Clone, Copy)]
pub struct SceneInstance {
    pub mesh: MeshKind,
    pub model: arcane_math::Mat4,
}

impl SceneInstance {
    pub fn new(mesh: MeshKind, model: arcane_math::Mat4) -> Self {
        Self { mesh, model }
    }
}

// =============================================================================
// Texture (procedural checker for now; will be loadable from disk later)
// =============================================================================

/// A 2D texture image + image view + sampler, all owned by the renderer.
///
/// For now this is constructed from a procedural checkerboard pattern
/// generated in Rust. A future phase will let the caller supply image
/// bytes loaded from disk via arcane_assets.
pub struct Texture {
    pub image: vk::Image,
    pub memory: vk::DeviceMemory,
    pub view: vk::ImageView,
    pub sampler: vk::Sampler,
    pub format: vk::Format,
    pub extent: vk::Extent2D,
}

impl Texture {
    /// Build a 64x64 RGBA checkerboard texture. The checker alternates
    /// between two warm colors so it's visible against the cool background
    /// clear. The texture is uploaded via a staging buffer + queue submit
    /// + layout transitions (the standard Vulkan upload path).
    pub fn new_checker(ctx: &VkContext) -> RenderResult<Self> {
        let extent = vk::Extent2D { width: 64, height: 64 };
        let format = vk::Format::R8G8B8A8_UNORM;
        let bytes_per_pixel = 4;
        let total_bytes = extent.width as usize * extent.height as usize * bytes_per_pixel;

        // Generate the checker pattern: 8x8 squares of 8x8 px each.
        let mut pixels: Vec<u8> = Vec::with_capacity(total_bytes);
        let color_a: [u8; 4] = [220, 200, 160, 255]; // warm light tan
        let color_b: [u8; 4] = [120, 80, 60, 255];   // warm dark brown
        let square = 8usize;
        for y in 0..extent.height as usize {
            for x in 0..extent.width as usize {
                let cell_x = x / square;
                let cell_y = y / square;
                let c = if (cell_x + cell_y) % 2 == 0 { color_a } else { color_b };
                pixels.extend_from_slice(&c);
            }
        }

        // Staging buffer (HOST_VISIBLE) -> texture image (DEVICE_LOCAL).
        let staging_size = total_bytes as vk::DeviceSize;
        let mut staging = Buffer::new(
            ctx,
            staging_size,
            vk::BufferUsageFlags::TRANSFER_SRC,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )?;
        staging.write_bytes(&pixels)?;

        // Image creation. initial_layout = UNDEFINED so the first barrier
        // transitions it into TRANSFER_DST_OPTIMAL for the copy.
        let image_create = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .extent(vk::Extent3D { width: extent.width, height: extent.height, depth: 1 })
            .mip_levels(1)
            .array_layers(1)
            .format(format)
            .tiling(vk::ImageTiling::OPTIMAL)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .usage(vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED)
            .samples(vk::SampleCountFlags::TYPE_1)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let image = unsafe {
            ctx.device.create_image(&image_create, None)
                .map_err(|e| RenderError::Allocator(format!("create_image (texture): {:?}", e)))?
        };
        let req = unsafe { ctx.device.get_image_memory_requirements(image) };
        let mem_type = ctx.find_memory_type(req.memory_type_bits, vk::MemoryPropertyFlags::DEVICE_LOCAL)?;
        let alloc = vk::MemoryAllocateInfo::default()
            .allocation_size(req.size)
            .memory_type_index(mem_type);
        let memory = unsafe {
            ctx.device.allocate_memory(&alloc, None)
                .map_err(|e| RenderError::Allocator(format!("allocate_memory (texture): {:?}", e)))?
        };
        unsafe {
            ctx.device.bind_image_memory(image, memory, 0)
                .map_err(|e| RenderError::Allocator(format!("bind_image_memory (texture): {:?}", e)))?;
        }

        // One-shot upload command buffer: UNDEFINED -> TRANSFER_DST_OPTIMAL,
        // copy buffer-to-image, then TRANSFER_DST_OPTIMAL -> SHADER_READ_ONLY_OPTIMAL.
        let pool_create = vk::CommandPoolCreateInfo::default()
            .flags(vk::CommandPoolCreateFlags::empty())
            .queue_family_index(ctx.graphics_queue_family);
        let upload_pool = unsafe {
            ctx.device.create_command_pool(&pool_create, None)
                .map_err(|e| RenderError::Other(format!("create_command_pool (texture): {:?}", e)))?
        };
        let alloc = vk::CommandBufferAllocateInfo::default()
            .command_pool(upload_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);
        let cmd = unsafe {
            ctx.device.allocate_command_buffers(&alloc)
                .map_err(|e| RenderError::Other(format!("alloc cmd (texture): {:?}", e)))?[0]
        };
        unsafe {
            let begin = vk::CommandBufferBeginInfo::default()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
            ctx.device.begin_command_buffer(cmd, &begin)
                .map_err(|e| RenderError::Other(format!("begin cmd (texture): {:?}", e)))?;

            let to_dst = vk::ImageMemoryBarrier::default()
                .image(image)
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .src_access_mask(vk::AccessFlags::empty())
                .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0, level_count: 1,
                    base_array_layer: 0, layer_count: 1,
                });
            ctx.device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[], &[], std::slice::from_ref(&to_dst),
            );

            let region = vk::BufferImageCopy::default()
                .buffer_offset(0)
                .buffer_row_length(0)
                .buffer_image_height(0)
                .image_subresource(vk::ImageSubresourceLayers {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    mip_level: 0,
                    base_array_layer: 0,
                    layer_count: 1,
                })
                .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
                .image_extent(vk::Extent3D { width: extent.width, height: extent.height, depth: 1 });
            ctx.device.cmd_copy_buffer_to_image(
                cmd,
                staging.raw,
                image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                std::slice::from_ref(&region),
            );

            let to_read = vk::ImageMemoryBarrier::default()
                .image(image)
                .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0, level_count: 1,
                    base_array_layer: 0, layer_count: 1,
                });
            ctx.device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::DependencyFlags::empty(),
                &[], &[], std::slice::from_ref(&to_read),
            );

            ctx.device.end_command_buffer(cmd)
                .map_err(|e| RenderError::Other(format!("end cmd (texture): {:?}", e)))?;
        }
        let submit_info = vk::SubmitInfo::default()
            .command_buffers(std::slice::from_ref(&cmd));
        unsafe {
            ctx.device.queue_submit(ctx.graphics_queue, std::slice::from_ref(&submit_info), vk::Fence::null())
                .map_err(|e| RenderError::Submit(format!("texture upload submit: {:?}", e)))?;
            ctx.device.queue_wait_idle(ctx.graphics_queue)
                .map_err(|e| RenderError::Other(format!("texture upload wait: {:?}", e)))?;
            ctx.device.free_command_buffers(upload_pool, std::slice::from_ref(&cmd));
            ctx.device.destroy_command_pool(upload_pool, None);
        }

        // Drop the staging buffer now (frees its memory).
        staging.destroy(ctx);

        // Image view + sampler.
        let view_create = vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(format)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0, level_count: 1,
                base_array_layer: 0, layer_count: 1,
            });
        let view = unsafe {
            ctx.device.create_image_view(&view_create, None)
                .map_err(|e| RenderError::Allocator(format!("create_image_view (texture): {:?}", e)))?
        };

        let sampler_create = vk::SamplerCreateInfo::default()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
            .address_mode_u(vk::SamplerAddressMode::REPEAT)
            .address_mode_v(vk::SamplerAddressMode::REPEAT)
            .address_mode_w(vk::SamplerAddressMode::REPEAT)
            .mip_lod_bias(0.0)
            .anisotropy_enable(false)
            .compare_op(vk::CompareOp::NEVER)
            .min_lod(0.0)
            .max_lod(0.0)
            .border_color(vk::BorderColor::INT_OPAQUE_BLACK);
        let sampler = unsafe {
            ctx.device.create_sampler(&sampler_create, None)
                .map_err(|e| RenderError::Allocator(format!("create_sampler: {:?}", e)))?
        };

        log::info!(
            "texture: checkerboard {}x{} {:?} (uploaded via staging buffer)",
            extent.width, extent.height, format
        );

        Ok(Self { image, memory, view, sampler, format, extent })
    }

    pub fn destroy(&mut self, ctx: &VkContext) {
        unsafe {
            ctx.device.destroy_sampler(self.sampler, None);
            ctx.device.destroy_image_view(self.view, None);
            ctx.device.destroy_image(self.image, None);
            ctx.device.free_memory(self.memory, None);
        }
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

/// Load a SPIR-V shader module directly from disk (used by the hot-
/// reload path — embedded shaders are a build-time snapshot; disk
/// shaders are whatever the user just recompiled with `glslc`).
pub fn load_shader_from_disk(ctx: &VkContext, path: &str) -> RenderResult<vk::ShaderModule> {
    let bytes = std::fs::read(path)
        .map_err(|e| RenderError::Shader(format!("read {}: {}", path, e)))?;
    if bytes.len() % 4 != 0 || bytes.len() < 8 {
        return Err(RenderError::Shader(format!(
            "SPIR-V file {} has bad size {} (must be multiple of 4, >= 8 bytes)",
            path, bytes.len()
        )));
    }
    // Sanity-check the SPIR-V magic number (0x07230203 in native byte order).
    let magic = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    if magic != 0x07230203 {
        return Err(RenderError::Shader(format!(
            "{}: not a SPIR-V binary (magic was {:#010x}, expected 0x07230203)",
            path, magic
        )));
    }
    let code: &[u32] = unsafe {
        std::slice::from_raw_parts(
            bytes.as_ptr() as *const u32,
            bytes.len() / 4,
        )
    };
    let create = vk::ShaderModuleCreateInfo::default().code(code);
    unsafe {
        Ok(ctx.device.create_shader_module(&create, None)
            .map_err(|e| RenderError::Shader(format!("create_shader_module (disk): {:?}", e)))?)
    }
}

// =============================================================================
// Shader hot-reload watcher
// =============================================================================

/// Polling-based file watcher for SPIR-V shader files. Captures the
/// mtimes of all `*.spv` files in `dir` at construction time, then
/// `changed()` returns true if any of them has a newer mtime than the
/// snapshot.
///
/// We poll rather than use the `notify` crate to avoid pulling another
/// dependency for a feature that is gated behind `MYSTICAL_HOTRELOAD=1`.
/// The poll is a stat() per file, ~10 µs per file — negligible next to
/// a single Vulkan frame.
pub struct ShaderWatcher {
    dir: std::path::PathBuf,
    snapshots: Vec<(std::path::PathBuf, std::time::SystemTime)>,
}

impl ShaderWatcher {
    /// Create a watcher for `dir/*.spv`. Snapshots mtimes at construction
    /// time so the first `changed()` call returns false unless the files
    /// change after this point.
    pub fn new(dir: impl AsRef<std::path::Path>) -> Self {
        let dir = dir.as_ref().to_path_buf();
        let snapshots = Self::snapshot_dir(&dir);
        Self { dir, snapshots }
    }

    /// Read-only access to the watched directory. Used by callers
    /// (e.g. Backend::hotreload_if_changed) to construct full paths to
    /// individual SPIR-V files.
    pub fn dir(&self) -> &std::path::Path {
        &self.dir
    }

    /// Re-snapshot the directory and return true if any file's mtime
    /// changed since the last snapshot. Also picks up newly created files.
    pub fn changed(&mut self) -> bool {
        let new = Self::snapshot_dir(&self.dir);
        let mut changed = false;
        for (path, mtime) in &new {
            let prev = self.snapshots.iter()
                .find(|(p, _)| p == path)
                .map(|(_, m)| *m);
            match prev {
                Some(prev_mtime) if *mtime != prev_mtime => {
                    changed = true;
                }
                None => {
                    // New file.
                    changed = true;
                }
                _ => {}
            }
        }
        // Detect deletions too — if a snapshot file is gone, that's a change.
        if self.snapshots.len() != new.len() {
            changed = true;
        }
        if changed {
            self.snapshots = new;
        }
        changed
    }

    fn snapshot_dir(dir: &std::path::Path) -> Vec<(std::path::PathBuf, std::time::SystemTime)> {
        let mut out = Vec::new();
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("spv") {
                    if let Ok(meta) = std::fs::metadata(&path) {
                        if let Ok(mtime) = meta.modified() {
                            out.push((path, mtime));
                        }
                    }
                }
            }
        }
        out
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
    pub descriptor_set_layout: vk::DescriptorSetLayout,
    pub vert_module: vk::ShaderModule,
    pub frag_module: vk::ShaderModule,
}

impl Pipeline {
    pub fn new(
        ctx: &VkContext,
        color_format: vk::Format,
        depth_format: vk::Format,
        extent: vk::Extent2D,
        descriptor_set_layout: vk::DescriptorSetLayout,
    ) -> RenderResult<Self> {
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

        // Shaders — Phase F switched to the lit_textured.{vert,frag} pair
        // which extends the lit pair with a UV attribute and a bound
        // combined image sampler at set 0 binding 0. The fragment shader
        // multiplies the per-vertex color by the sampled texel before
        // applying ambient + diffuse + rim lighting.
        let vert_module = load_shader(ctx, "lit_textured.vert.spv")?;
        let frag_module = load_shader(ctx, "lit_textured.frag.spv")?;
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

        // Vertex layout: pos (12B) + color (12B) + normal (12B) = 36B.
        // All three attributes are 4-byte aligned (vec3 of f32), so no
        // padding is needed and the struct is bytemuck::Pod-compatible.
        let vertex_bindings = [vk::VertexInputBindingDescription::default()
            .binding(0)
            .stride(std::mem::size_of::<Vertex>() as u32)
            .input_rate(vk::VertexInputRate::VERTEX)];
        // Vertex layout: pos (12B) + color (12B) + normal (12B) + uv (8B)
        // = 44 bytes. uv is a vec2 of f32 (8 bytes, 4-byte aligned) and
        // starts at offset 24 + 12 = 36. Struct is Pod-compatible (no
        // internal padding) but its size 44 isn't a multiple of 4
        // (44 % 4 == 0 actually — 44/4 == 11, so it IS 4-aligned; no
        // trailing pad needed either).
        let vertex_attrs = [
            vk::VertexInputAttributeDescription::default()
                .location(0).binding(0)
                .format(vk::Format::R32G32B32_SFLOAT)
                .offset(0),
            vk::VertexInputAttributeDescription::default()
                .location(1).binding(0)
                .format(vk::Format::R32G32B32_SFLOAT)
                .offset(12),
            vk::VertexInputAttributeDescription::default()
                .location(2).binding(0)
                .format(vk::Format::R32G32B32_SFLOAT)
                .offset(24),
            vk::VertexInputAttributeDescription::default()
                .location(3).binding(0)
                .format(vk::Format::R32G32_SFLOAT)
                .offset(36),
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

        // Push constants: { view_proj: mat4, model: mat4 } = 128 bytes.
        // This is the Vulkan-guaranteed minimum push-constant size, so the
        // pipeline works on every conformant driver. view_proj is constant
        // across all instances in a frame; model is per-instance. We push
        // both together per draw call (the simpler API wins over a split
        // push that would only save 64 bytes of bandwidth per draw).
        let pc_ranges = [vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::VERTEX)
            .offset(0)
            .size(std::mem::size_of::<[f32; 32]>() as u32)];

        // Caller-supplied descriptor set layout (the Backend pre-creates
        // it so it can also pre-allocate + update the descriptor set with
        // the texture before this pipeline is built — lavapipe wants the
        // sampler to exist when create_graphics_pipelines lowers the
        // texture-sample op to LLVM IR).
        let dsl_create_unused = vk::DescriptorSetLayoutCreateInfo::default();
        let _ = dsl_create_unused;

        let layout_create = vk::PipelineLayoutCreateInfo::default()
            .push_constant_ranges(&pc_ranges)
            .set_layouts(std::slice::from_ref(&descriptor_set_layout));
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
            descriptor_set_layout,
            vert_module,
            frag_module,
        })
    }

    pub fn destroy(&mut self, ctx: &VkContext) {
        unsafe {
            ctx.device.destroy_pipeline(self.pipeline, None);
            ctx.device.destroy_pipeline_layout(self.pipeline_layout, None);
            ctx.device.destroy_descriptor_set_layout(self.descriptor_set_layout, None);
            ctx.device.destroy_render_pass(self.render_pass, None);
            ctx.device.destroy_shader_module(self.vert_module, None);
            ctx.device.destroy_shader_module(self.frag_module, None);
        }
    }

    /// Hot-reload: re-read the SPIR-V files at `vert_path` / `frag_path`
    /// from disk, then rebuild ONLY the graphics pipeline object (the
    /// pipeline_layout + render_pass + descriptor_set_layout stay alive
    /// because they don't depend on shader code, and the framebuffers
    /// reference the render_pass so we can't drop it without also
    /// recreating them).
    ///
    /// Caller must ensure the GPU is idle (calls device_wait_idle first).
    /// On failure the old pipeline + shader modules are gone — we
    /// surface the error and let the caller decide whether to rebuild
    /// from scratch or exit.
    pub fn reload_shaders_from_disk(
        &mut self,
        ctx: &VkContext,
        vert_path: &str,
        frag_path: &str,
        extent: vk::Extent2D,
    ) -> RenderResult<()> {
        log::info!(
            "hot-reload: vert={} frag={}",
            vert_path, frag_path
        );
        // Build the new shader modules first; if either fails we abort
        // without touching the existing pipeline.
        let new_vert = load_shader_from_disk(ctx, vert_path)?;
        let new_frag = load_shader_from_disk(ctx, frag_path)?;

        // Reuse the existing pipeline_layout + render_pass + descriptor_set_layout.
        // The new pipeline object will reference them.
        let entry_name = CString::new("main").unwrap();
        let vert_stage = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(new_vert)
            .name(&entry_name);
        let frag_stage = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(new_frag)
            .name(&entry_name);
        let stages = [vert_stage, frag_stage];

        // Vertex input state must match what the new shaders expect.
        // We rebuild from the same Vertex struct definition so the
        // stride + attribute offsets are unchanged.
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
            vk::VertexInputAttributeDescription::default()
                .location(2).binding(0)
                .format(vk::Format::R32G32B32_SFLOAT)
                .offset(24),
            vk::VertexInputAttributeDescription::default()
                .location(3).binding(0)
                .format(vk::Format::R32G32_SFLOAT)
                .offset(36),
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
            .cull_mode(vk::CullModeFlags::BACK)
            .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
            .depth_bias_enable(false);

        let multisampling = vk::PipelineMultisampleStateCreateInfo::default()
            .sample_shading_enable(false)
            .rasterization_samples(vk::SampleCountFlags::TYPE_1);

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

        let create = vk::GraphicsPipelineCreateInfo::default()
            .stages(&stages)
            .vertex_input_state(&vertex_input)
            .input_assembly_state(&input_assembly)
            .viewport_state(&viewport_state)
            .rasterization_state(&rasterizer)
            .multisample_state(&multisampling)
            .depth_stencil_state(&depth_stencil)
            .color_blend_state(&color_blend)
            .layout(self.pipeline_layout)
            .render_pass(self.render_pass)
            .subpass(0);

        // Destroy the old pipeline + modules first, then create the new one.
        // If creation fails, we surface the error — the caller is in an
        // inconsistent state but at least the device is idle.
        unsafe {
            let _ = ctx.device.device_wait_idle();
            ctx.device.destroy_pipeline(self.pipeline, None);
            ctx.device.destroy_shader_module(self.vert_module, None);
            ctx.device.destroy_shader_module(self.frag_module, None);
        }
        let new_pipeline = unsafe {
            ctx.device.create_graphics_pipelines(
                vk::PipelineCache::null(),
                std::slice::from_ref(&create),
                None,
            ).map_err(|(_pipelines, e)| RenderError::Pipeline(format!("hot-reload create_graphics_pipelines: {:?}", e)))?[0]
        };
        self.pipeline = new_pipeline;
        self.vert_module = new_vert;
        self.frag_module = new_frag;
        log::info!("hot-reload: pipeline rebuilt successfully");
        Ok(())
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
    pub mesh: CubeMesh,
    pub plane_mesh: Mesh,
    pub sphere_mesh: Mesh,
    pub pyramid_mesh: Mesh,
    pub obj_mesh: Mesh,
    pub texture: Texture,
    pub descriptor_pool: vk::DescriptorPool,
    pub descriptor_set: vk::DescriptorSet,
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
        // Order matters here: the lit_textured pipeline samples a bound
        // texture in the fragment shader, and lavapipe (CPU Vulkan) wants
        // the descriptor set + texture bound BEFORE create_graphics_pipelines
        // is called — it lowers the texture sample to LLVM IR at pipeline
        // build time, and that lowering can segfault if no VkSampler exists
        // yet. So we create the texture + descriptor pool + descriptor set
        // FIRST, then build the pipeline that references the descriptor
        // set layout. The descriptor_set_layout itself is created inside
        // Pipeline::new, but the layout object only describes the shape;
        // the actual binding data is filled by update_descriptor_sets here
        // before pipeline creation runs.
        //
        // Workaround: pre-create the descriptor_set_layout inline so we
        // can allocate + update the set before the pipeline references it.
        let binding = vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT)
            .immutable_samplers(&[]);
        let dsl_create = vk::DescriptorSetLayoutCreateInfo::default()
            .bindings(std::slice::from_ref(&binding));
        let pre_dsl = unsafe {
            ctx.device.create_descriptor_set_layout(&dsl_create, None)
                .map_err(|e| RenderError::Pipeline(format!("pre-create_descriptor_set_layout: {:?}", e)))?
        };
        let texture = Texture::new_checker(&ctx)?;
        let pool_sizes = [vk::DescriptorPoolSize::default()
            .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)];
        let pool_create = vk::DescriptorPoolCreateInfo::default()
            .flags(vk::DescriptorPoolCreateFlags::empty())
            .max_sets(1)
            .pool_sizes(&pool_sizes);
        let descriptor_pool = unsafe {
            ctx.device.create_descriptor_pool(&pool_create, None)
                .map_err(|e| RenderError::Pipeline(format!("create_descriptor_pool: {:?}", e)))?
        };
        let set_alloc = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(descriptor_pool)
            .set_layouts(std::slice::from_ref(&pre_dsl));
        let descriptor_set = unsafe {
            ctx.device.allocate_descriptor_sets(&set_alloc)
                .map_err(|e| RenderError::Pipeline(format!("allocate_descriptor_sets: {:?}", e)))?[0]
        };
        let image_info = vk::DescriptorImageInfo::default()
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image_view(texture.view)
            .sampler(texture.sampler);
        let writes = [vk::WriteDescriptorSet::default()
            .dst_set(descriptor_set)
            .dst_binding(0)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .image_info(std::slice::from_ref(&image_info))];
        unsafe { ctx.device.update_descriptor_sets(&writes, &[]); }
        log::info!(
            "texture + descriptor set pre-created before pipeline (lavapipe lowering requirement)"
        );
        let pipeline = Pipeline::new(
            &ctx, swapchain.format, depth.format, swapchain.extent, pre_dsl,
        )?;
        let frame = Frame::new(
            &ctx,
            swapchain.extent,
            pipeline.render_pass,
            &swapchain.image_views,
            depth.view,
            2,
        )?;
        let mesh = CubeMesh::new(&ctx)?;
        let plane_mesh = PlaneMesh::new(&ctx, 20.0, 20.0)?.mesh;
        let sphere_mesh = SphereMesh::new(&ctx, 12, 16)?.mesh;
        let pyramid_mesh = PyramidMesh::new(&ctx)?.mesh;
        // OBJ-loaded asset: parse the embedded OCTAHEDRON_OBJ at runtime
        // via arcane_assets::parse_obj, then build a renderer Mesh from
        // the parsed positions + synthesized face normals + zero UVs.
        // Phase J demonstrates the asset-loading path without a disk
        // pipeline; a future host can swap in a real .obj file.
        let obj_model = arcane_assets::parse_obj(arcane_assets::OCTAHEDRON_OBJ)
            .map_err(|e| RenderError::Other(format!("parse octahedron OBJ: {}", e)))?;
        log::info!(
            "loaded OBJ asset: octahedron, {} verts, {} tris",
            obj_model.vertex_count(),
            obj_model.triangle_count()
        );
        let mut obj_verts: Vec<Vertex> = Vec::with_capacity(obj_model.positions.len());
        for (p, n) in obj_model.positions.iter().zip(obj_model.normals.iter()) {
            // Octahedron color — warm gold to visually distinguish it
            // from the cube (yellow on +X) and the sphere (cool grey).
            obj_verts.push(Vertex::with_uv(*p, [0.95, 0.80, 0.20], *n, [0.0, 0.0]));
        }
        let obj_mesh = MeshBuilder::new(&ctx)
            .vertices(&obj_verts)
            .indices_u16(&obj_model.indices)
            .build()?;

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
            plane_mesh,
            sphere_mesh,
            pyramid_mesh,
            obj_mesh,
            texture,
            descriptor_pool,
            descriptor_set,
            observatory,
            readback,
            width,
            height,
        })
    }

    pub fn render_one(&mut self, frame_index: u64) -> RenderResult<()> {
        // Default single-cube scene for backwards compatibility: a single
        // cube at the origin rotating around Y.
        use arcane_math::Mat4;
        let angle = (frame_index as f32) * 0.05;
        let model = Mat4::from_rotation_y(angle);
        self.render_scene(frame_index, &[SceneInstance::new(MeshKind::Cube, model)])
    }

    /// Check the shader directory for changes and reload the pipeline's
    /// shaders from disk if any .spv file's mtime advanced. Returns
    /// `true` if a reload happened (caller can log), `false` if no
    /// changes detected. The vert + frag paths are derived from the
    /// watcher's directory + fixed shader names.
    pub fn hotreload_if_changed(
        &mut self,
        watcher: &mut ShaderWatcher,
        vert_name: &str,
        frag_name: &str,
    ) -> RenderResult<bool> {
        if !watcher.changed() {
            return Ok(false);
        }
        // Build full paths to the vert + frag SPIR-V files.
        let vert_path = format!(
            "{}/{}",
            watcher.dir().display(),
            vert_name
        );
        let frag_path = format!(
            "{}/{}",
            watcher.dir().display(),
            frag_name
        );
        // Reload. Failure here is recoverable in the sense that the
        // pipeline_layout + render_pass + descriptor set + framebuffers
        // are still alive; only the pipeline object + shader modules
        // got destroyed. The caller would have to rebuild from scratch
        // if the new pipeline creation failed.
        match self.pipeline.reload_shaders_from_disk(
            &self.ctx, &vert_path, &frag_path, self.swapchain.extent,
        ) {
            Ok(()) => Ok(true),
            Err(e) => {
                log::error!("hot-reload failed: {:?}", e);
                Err(e)
            }
        }
    }

    /// Render a scene containing many cube instances, each transformed by
    /// its own model matrix. All instances share the same vertex/index
    /// buffers and the same graphics pipeline; the per-instance MVP is
    /// pushed as a 64-byte push constant before each draw.
    ///
    /// The view and projection matrices are computed internally from the
    /// framebuffer dimensions (so the aspect ratio is always right). The
    /// camera sits at (0, 0, +5) in world space looking at the origin
    /// with a 60 degree vertical FOV.
    /// Render a scene of mixed primitives. Each entry picks one of the
    /// Backend-owned meshes (cube, plane) and a model matrix. The push
    /// constant per draw is view_proj + model (128 bytes).
    pub fn render_scene(
        &mut self,
        frame_index: u64,
        scene: &[SceneInstance],
    ) -> RenderResult<()> {
        use arcane_math::{Mat4, Vec3, look_at, mat4_to_cols_array, vulkan_perspective};

        let aspect = self.width as f32 / self.height as f32;
        let proj: Mat4 = vulkan_perspective(
            60.0f32.to_radians(),
            aspect,
            0.1,
            100.0,
        );
        // Orbit camera: eye position rotates around the Y axis at a
        // rate of 0.01 rad/frame (~6 deg/sec at 1000 fps, ~0.6 deg/sec
        // at 60 fps). Looking at the origin, up = +Y. The orbit radius
        // is 8 units so the cubes at +/- 3 still fit in view.
        let angle = (frame_index as f32) * 0.01;
        let eye = Vec3::new(
            angle.cos() * 8.0,
            2.5,
            angle.sin() * 8.0,
        );
        let target = Vec3::new(0.0, 0.0, 0.0);
        let up = Vec3::new(0.0, 1.0, 0.0);
        let view: Mat4 = look_at(eye, target, up);
        let view_proj: Mat4 = proj * view;
        let view_proj_cols = mat4_to_cols_array(view_proj);

        // Precompute per-instance push-constant payload + resolve the
        // mesh each instance draws. Storing the mesh kind here keeps the
        // record_draw loop pure w.r.t. self (only touches GPU state).
        let per_instance: Vec<([f32; 32], MeshKind)> = scene
            .iter()
            .map(|inst| {
                let model_cols = mat4_to_cols_array(inst.model);
                let mut buf = [0.0f32; 32];
                buf[..16].copy_from_slice(&view_proj_cols);
                buf[16..32].copy_from_slice(&model_cols);
                (buf, inst.mesh)
            })
            .collect();

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

        self.record_draw(cmd, image_index, &per_instance)?;

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

    /// Legacy entry kept for source-level backwards compatibility. Maps a
    /// list of (MeshKind, Mat4) tuples to a SceneInstance slice and
    /// delegates to render_scene.
    pub fn render_objects(
        &mut self,
        frame_index: u64,
        scene: &[(MeshKind, arcane_math::Mat4)],
    ) -> RenderResult<()> {
        let instances: Vec<SceneInstance> = scene
            .iter()
            .map(|(kind, m)| SceneInstance::new(*kind, *m))
            .collect();
        self.render_scene(frame_index, &instances)
    }

    fn record_draw(
        &self,
        cmd: vk::CommandBuffer,
        image_index: u32,
        per_instance: &[([f32; 32], MeshKind)],
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
            // Bind the descriptor set (texture) once for the whole render
            // pass. All draws use the same texture at set 0 binding 0.
            self.ctx.device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipeline.pipeline_layout,
                0,
                std::slice::from_ref(&self.descriptor_set),
                &[],
            );

            // Per-instance loop: bind that instance's vertex/index buffers,
            // push view_proj + model (128 bytes), then draw. The pipeline,
            // descriptor set, and texture are bound once outside the loop.
            for (pc, kind) in per_instance {
                let mesh = match kind {
                    MeshKind::Cube => &self.mesh.mesh,
                    MeshKind::Plane => &self.plane_mesh,
                    MeshKind::Sphere => &self.sphere_mesh,
                    MeshKind::Pyramid => &self.pyramid_mesh,
                    MeshKind::LoadedObj => &self.obj_mesh,
                };
                self.ctx.device.cmd_bind_vertex_buffers(
                    cmd, 0, std::slice::from_ref(&mesh.vertex.raw), &[0],
                );
                self.ctx.device.cmd_bind_index_buffer(
                    cmd, mesh.index.raw, 0, vk::IndexType::UINT16,
                );
                let pc_bytes: &[u8] = std::slice::from_raw_parts(
                    pc.as_ptr() as *const u8,
                    std::mem::size_of::<[f32; 32]>(),
                );
                self.ctx.device.cmd_push_constants(
                    cmd,
                    self.pipeline.pipeline_layout,
                    vk::ShaderStageFlags::VERTEX,
                    0,
                    pc_bytes,
                );
                self.ctx.device.cmd_draw_indexed(cmd, mesh.index_count, 1, 0, 0, 0);
            }
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
        //   plane_mesh (vertex/index buffers)
        //   texture (image + view + sampler + memory)
        //   descriptor_pool (frees all descriptor sets)
        //   pipeline (graphics pipeline + render pass + shaders + pipeline layout + dsl)
        //   frame (framebuffers + sync objects + command pool)
        //   depth (depth image + view + memory)
        //   swapchain (surface + image views + raw swapchain)
        // The VkContext (instance + device + physical device) outlives all
        // of these because it's held in an Arc and dropped last when the
        // Backend is consumed.
        self.readback.destroy(&self.ctx);
        self.mesh.destroy(&self.ctx);
        self.plane_mesh.destroy(&self.ctx);
        self.sphere_mesh.destroy(&self.ctx);
        self.pyramid_mesh.destroy(&self.ctx);
        self.obj_mesh.destroy(&self.ctx);
        self.texture.destroy(&self.ctx);
        unsafe { self.ctx.device.destroy_descriptor_pool(self.descriptor_pool, None); }
        self.pipeline.destroy(&self.ctx);
        self.frame.destroy(&self.ctx);
        self.depth.destroy(&self.ctx);
        self.swapchain.destroy(&self.ctx);
    }
}
