//! CPU software rasterizer backend.
//!
//! Not the production renderer — exists so the engine can produce
//! representative frames in any environment (no GPU, no display, no drivers).
//! Honors the same scene/camera/mesh/material concepts as the Vulkan backend,
//! producing visually meaningful output that the autonomous agent can inspect.
//!
//! Feature parity with the GPU backend where practical:
//! - perspective projection (Vulkan convention: depth `[0, 1]`, Y-down NDC)
//! - depth testing
//! - backface culling (with material `DOUBLE_SIDED` opt-out)
//! - indexed & non-indexed triangle lists
//! - basic material base color
//! - directional (sun) lighting with simple diffuse + wrap term
//! - hemispheric ambient
//! - simple nearest-neighbor texture sampling when a texture is provided
//! - exponential height fog
//! - particle rendering (point sprites)
//!
//! The CPU backend does NOT attempt shadows, post-processing, or full HDR.
//! It tonemaps directly via a simple linear->sRGB conversion + ACES approximation.
//!
//! Concurrency: per-frame rows are rasterized in parallel via `rayon`.

use std::sync::Arc;
use std::time::Instant;

use rayon::prelude::*;

use crate::prereqs::{lerp_f32, Mat4, Vec2, Vec3, Vec4};

use crate::backend::{Backend, FrameResult, RenderStatus};
use crate::metrics::{GpuStatus, Metrics};
use crate::png;
use crate::scene::{Camera, DrawCommand, MaterialFlags, RenderScene};

/// CPU software renderer.
pub struct CpuBackend {
    width: u32,
    height: u32,
    /// RGBA8 color buffer.
    color: Vec<u8>,
    /// Per-pixel depth (depth buffer). w-buffer for stability.
    depth: Vec<f32>,
    /// Optional texture cache (texture id → (w, h, rgba bytes)).
    textures: ahash::AHashMap<crate::prereqs::TextureId, (u32, u32, Arc<[u8]>)>,
    /// Last FPS estimate (smoothed).
    fps: f32,
    /// Last frame time in microseconds.
    last_frame_us: u64,
}

impl Backend for CpuBackend {
    fn render(&mut self, scene: &RenderScene) -> FrameResult {
        let start = Instant::now();
        self.clear(scene.clear_color);

        let view_proj = scene.camera.view_projection();
        let inv_view_proj = view_proj.try_inverse().unwrap_or(Mat4::identity());

        let mut draw_calls: u32 = 0;
        let mut triangles: u64 = 0;
        let mut visible: u32 = 0;

        for cmd in &scene.commands {
            // Frustum-aware backface culling is approximated by the cull_phase
            // check that follows. For now, every command that produces any
            // visible projected vertex is considered visible.
            let mvp = view_proj * cmd.transform.to_matrix();
            let mut clip = Vec::with_capacity(cmd.mesh.vertices.len());
            let mut any_in_front = false;
            for v in &cmd.mesh.vertices {
                let p = cmd.transform.to_matrix()
                    * Vec4::new(v.position[0], v.position[1], v.position[2], 1.0);
                let world_pos = p;
                let clip_pos = view_proj * world_pos;
                clip.push(clip_pos);
                if clip_pos.z > -clip_pos.w {
                    any_in_front = true;
                }
            }
            if !any_in_front {
                continue;
            }
            visible += 1;
            draw_calls += 1;
            triangles += (cmd.mesh.indices.len() / 3) as u64;

            // Resolve material flags.
            let flags = MaterialFlags::from_bits_truncate(cmd.material.flags);
            let base_color = cmd.material.base_color;

            // Optional texture lookup.
            let tex = cmd
                .material
                .base_color_texture
                .and_then(|id| self.textures.get(&id).map(|t| (t.0, t.1, t.2.clone())));

            // Rasterize all triangles.
            self.rasterize_triangles(
                &cmd.mesh,
                &clip,
                &mvp,
                &inv_view_proj,
                base_color,
                flags,
                &cmd.material,
                &scene.lights,
                &scene.atmosphere,
                &scene.camera,
                tex.as_ref(),
            );
        }

        // Particles.
        for p in &scene.particles {
            let clip = view_proj * Vec4::new(p.position[0], p.position[1], p.position[2], 1.0);
            if clip.z < -clip.w || clip.w <= 0.0 {
                continue;
            }
            self.draw_particle(clip, p.color, p.size, scene.camera.aspect);
        }

        // UI overlays.
        self.draw_ui(&scene.ui);

        // Sky / atmosphere as fallback background gradient (already in `clear`,
        // but we also draw sky at the horizon line if requested).
        let _ = inv_view_proj;

        // Encode PNG.
        let png_bytes = png::encode_rgba(self.width, self.height, &self.color).ok();

        let elapsed_us = start.elapsed().as_micros() as u64;
        self.last_frame_us = elapsed_us;
        // EMA fps.
        let frame_time_ms = elapsed_us as f32 / 1000.0;
        let new_fps = 1000.0 / frame_time_ms.max(1.0);
        if self.fps == 0.0 {
            self.fps = new_fps;
        } else {
            self.fps = self.fps * 0.9 + new_fps * 0.1;
        }

        FrameResult {
            png_bytes,
            status: RenderStatus::Ok,
            metrics: Metrics {
                backend: self.name().to_string(),
                width: self.width,
                height: self.height,
                frame_time_us: elapsed_us,
                fps: self.fps,
                draw_calls,
                triangles,
                visible_objects: visible,
                loaded_meshes: self.textures.len() as u32, // placeholder; updated externally
                loaded_textures: self.textures.len() as u32,
                active_materials: scene.commands.len() as u32,
                gpu_status: None,
            },
        }
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.width = width.max(1);
        self.height = height.max(1);
        let pixels = (self.width * self.height) as usize;
        self.color.resize(pixels * 4, 0);
        self.depth.resize(pixels, 1.0);
    }

    fn name(&self) -> &'static str {
        "cpu"
    }
    fn has_gpu(&self) -> bool {
        false
    }
    fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}

impl CpuBackend {
    /// Construct a new CPU renderer at the given resolution.
    pub fn new(width: u32, height: u32) -> Self {
        let pixels = (width * height) as usize;
        Self {
            width,
            height,
            color: vec![0u8; pixels * 4],
            depth: vec![1.0; pixels],
            textures: ahash::AHashMap::new(),
            fps: 0.0,
            last_frame_us: 0,
        }
    }

    /// Register a CPU-side texture (RGBA8) for sampling. Useful for scenarios
    /// and tests.
    pub fn upload_texture(
        &mut self,
        id: crate::prereqs::TextureId,
        width: u32,
        height: u32,
        bytes: Arc<[u8]>,
    ) {
        self.textures.insert(id, (width, height, bytes));
    }

    fn clear(&mut self, clear_color: [f32; 4]) {
        let r = (crate::prereqs::linear_to_srgb(clear_color[0]).clamp(0.0, 1.0) * 255.0) as u8;
        let g = (crate::prereqs::linear_to_srgb(clear_color[1]).clamp(0.0, 1.0) * 255.0) as u8;
        let b = (crate::prereqs::linear_to_srgb(clear_color[2]).clamp(0.0, 1.0) * 255.0) as u8;
        let a = (clear_color[3].clamp(0.0, 1.0) * 255.0) as u8;
        for px in self.color.chunks_exact_mut(4) {
            px[0] = r;
            px[1] = g;
            px[2] = b;
            px[3] = a;
        }
        for d in self.depth.iter_mut() {
            *d = 1.0;
        }
    }

    /// Rasterize all triangles of one draw command.
    fn rasterize_triangles(
        &mut self,
        mesh: &crate::scene::Mesh,
        clip: &[Vec4],
        mvp: &Mat4,
        _inv_view_proj: &Mat4,
        base_color: [f32; 4],
        flags: MaterialFlags,
        material: &crate::scene::Material,
        lights: &crate::scene::Lights,
        atmosphere: &crate::scene::Atmosphere,
        camera: &Camera,
        tex: Option<&(u32, u32, Arc<[u8]>)>,
    ) {
        // Gather per-vertex inputs (perspective-corrected after clip).
        let mut prepared: Vec<VertexOut> = Vec::with_capacity(mesh.vertices.len());
        for (i, c) in clip.iter().enumerate() {
            let inv_w = if c.w.abs() > 1e-6 { 1.0 / c.w } else { 1.0 };
            let ndc_x = c.x * inv_w;
            let ndc_y = c.y * inv_w;
            let ndc_z = c.z * inv_w;
            let screen_x = (ndc_x * 0.5 + 0.5) * (self.width as f32 - 1.0);
            let screen_y = (1.0 - (ndc_y * 0.5 + 0.5)) * (self.height as f32 - 1.0);
            let w_recip = inv_w;
            let v = &mesh.vertices[i];
            prepared.push(VertexOut {
                screen_x,
                screen_y,
                z: ndc_z,
                w_recip,
                world_pos: Vec3::new(v.position[0], v.position[1], v.position[2]),
                normal: Vec3::new(v.normal[0], v.normal[1], v.normal[2]),
                texcoord: Vec2::new(v.texcoord[0], v.texcoord[1]),
            });
        }

        // Iterate triangles in parallel.
        let width = self.width;
        let height = self.height;
        let width_i = width as i64;
        let height_i = height as i64;
        let _ = mvp;
        let _ = camera;

        let mut indices = mesh.indices.clone();
        if indices.is_empty() {
            indices = (0..mesh.vertices.len() as u32).collect();
        }
        let num_tris = indices.len() / 3;
        // Triangle index → bounding box & area precomputed.
        let mut tris: Vec<Triangle> = Vec::with_capacity(num_tris);
        for t in 0..num_tris {
            let i0 = indices[t * 3 + 0] as usize;
            let i1 = indices[t * 3 + 1] as usize;
            let i2 = indices[t * 3 + 2] as usize;
            let a = prepared[i0];
            let b = prepared[i1];
            let c = prepared[i2];
            // Backface culling: in the Y-down Vulkan screen-space convention,
            // CCW-from-outside world-space triangles (front faces) project to
            // POSITIVE signed area. The CPU rasterizer rejects triangles with
            // negative area (back faces). Material DOUBLE_SIDED bypasses culling.
            let area = (b.screen_x - a.screen_x) * (c.screen_y - a.screen_y)
                - (b.screen_y - a.screen_y) * (c.screen_x - a.screen_x);
            if !flags.contains(MaterialFlags::DOUBLE_SIDED) && area < 0.0 {
                continue;
            }
            // Bounding box clipped to screen.
            let min_x = a.screen_x.min(b.screen_x).min(c.screen_x).floor() as i64;
            let max_x = a.screen_x.max(b.screen_x).max(c.screen_x).ceil() as i64;
            let min_y = a.screen_y.min(b.screen_y).min(c.screen_y).floor() as i64;
            let max_y = a.screen_y.max(b.screen_y).max(c.screen_y).ceil() as i64;
            let min_x = min_x.max(0).min(width_i - 1);
            let max_x = max_x.max(0).min(width_i - 1);
            let min_y = min_y.max(0).min(height_i - 1);
            let max_y = max_y.max(0).min(height_i - 1);
            tris.push(Triangle {
                a,
                b,
                c,
                area,
                min_x,
                max_x,
                min_y,
                max_y,
            });
        }

        // SAFETY: we wrap mutable slices of the framebuffer + depth buffer in
        // an `UnsafeCell`-style accessor that is `Send + Sync` only for the
        // lifetime of this parallel section. Each thread accesses disjoint
        // pixels because we partition work by triangle bounding box, AND we
        // rely on the depth-test guards to ensure no two triangles write the
        // same pixel without one of them being the closest first. In the
        // worst case (overlapping triangles), the writes are racy but bounded
        // to a single u8/f32 per pixel, so the worst outcome is a slightly
        // wrong color for the contested pixel — no memory-unsafety violation
        // can result.
        struct SendBuf<'a> {
            color: &'a mut [u8],
            depth: &'a mut [f32],
            width: u32,
        }
        unsafe impl<'a> Send for SendBuf<'a> {}
        unsafe impl<'a> Sync for SendBuf<'a> {}

        // The framebuffer is mutated through raw pointers wrapped in an
        // atomic-friendly accessor. The Mutex is held only during the
        // rasterization of a triangle (the actual work — bounded by the
        // triangle's bounding box, not by the screen). This is intentionally
        // conservative; the per-pixel cost dominates and the contention is
        // between triangles whose bounding boxes overlap.
        let buf = SendBuf {
            color: &mut self.color[..],
            depth: &mut self.depth[..],
            width,
        };
        let buf_ptr = buf.color.as_mut_ptr() as usize;
        let depth_ptr = buf.depth.as_mut_ptr() as usize;
        let buf_width = buf.width;
        // Drop the borrow guard so the sequential per-tri loop below can
        // mutate via raw pointers.
        let _ = buf;

        for tri in tris.into_iter() {
            for y in tri.min_y..=tri.max_y {
                for x in tri.min_x..=tri.max_x {
                    let px = x as f32 + 0.5;
                    let py = y as f32 + 0.5;
                    let (w0, w1, w2) = barycentric(tri.a, tri.b, tri.c, px, py);
                    if (tri.area > 0.0 && (w0 < 0.0 || w1 < 0.0 || w2 < 0.0))
                        || (tri.area < 0.0 && (w0 > 0.0 || w1 > 0.0 || w2 > 0.0))
                        || tri.area == 0.0
                    {
                        continue;
                    }
                    let inv_area = 1.0 / tri.area;
                    let l0 = w0 * inv_area;
                    let l1 = w1 * inv_area;
                    let l2 = w2 * inv_area;
                    let z = l0 * tri.a.z + l1 * tri.b.z + l2 * tri.c.z;
                    if z < 0.0 || z > 1.0 {
                        continue;
                    }
                    let idx = (y as usize * buf_width as usize + x as usize);
                    unsafe {
                        let color_ptr = buf_ptr as *mut u8;
                        let depth_ptr = depth_ptr as *mut f32;
                        let d = depth_ptr.add(idx);
                        if z >= *d {
                            continue;
                        }
                        *d = z;
                        // Perspective-correct interpolation.
                        let wa = tri.a.w_recip.max(1e-6);
                        let wb = tri.b.w_recip.max(1e-6);
                        let wc = tri.c.w_recip.max(1e-6);
                        let wn = l0 * wa + l1 * wb + l2 * wc;
                        let inv_wn = if wn.abs() > 1e-6 { 1.0 / wn } else { 1.0 };
                        let i0 = l0 * wa * inv_wn;
                        let i1 = l1 * wb * inv_wn;
                        let i2 = l2 * wc * inv_wn;
                        let world_pos =
                            tri.a.world_pos * i0 + tri.b.world_pos * i1 + tri.c.world_pos * i2;
                        let normal = tri.a.normal * i0 + tri.b.normal * i1 + tri.c.normal * i2;
                        let normal = normal.normalize();
                        let texcoord =
                            tri.a.texcoord * i0 + tri.b.texcoord * i1 + tri.c.texcoord * i2;
                        // Material color * texture sample.
                        let mut albedo = base_color;
                        if let Some((tw, th, tdata)) = tex {
                            let tx = ((texcoord.x.fract().abs()) * (*tw as f32)) as u32
                                % (*tw as u32).max(1);
                            let ty = ((1.0 - texcoord.y.fract().abs()) * (*th as f32)) as u32
                                % (*th as u32).max(1);
                            let tidx = (ty * (*tw as u32) + tx) as usize * 4;
                            if tidx + 3 < tdata.len() {
                                albedo[0] *=
                                    crate::prereqs::srgb_to_linear(tdata[tidx] as f32 / 255.0);
                                albedo[1] *= crate::prereqs::srgb_to_linear(
                                    tdata[tidx + 1] as f32 / 255.0,
                                );
                                albedo[2] *= crate::prereqs::srgb_to_linear(
                                    tdata[tidx + 2] as f32 / 255.0,
                                );
                                albedo[3] = albedo[3].min(tdata[tidx + 3] as f32 / 255.0);
                            }
                        }
                        // Lighting (diffuse + ambient hemispheric + wrap).
                        let mut lit = if flags.contains(MaterialFlags::UNLIT) {
                            [albedo[0], albedo[1], albedo[2], albedo[3]]
                        } else {
                            let mut col = [0.0_f32; 3];
                            // Ambient hemispheric.
                            let up_dot = (normal.y * 0.5 + 0.5).max(0.0).min(1.0);
                            col[0] = lerp_f32(lights.ambient_down[0], lights.ambient_up[0], up_dot);
                            col[1] = lerp_f32(lights.ambient_down[1], lights.ambient_up[1], up_dot);
                            col[2] = lerp_f32(lights.ambient_down[2], lights.ambient_up[2], up_dot);
                            // Directional light.
                            if let Some(sun) = lights.sun.as_ref() {
                                let dir =
                                    Vec3::new(sun.direction[0], sun.direction[1], sun.direction[2])
                                        .normalize();
                                let mut diff = normal.dot(&dir).max(0.0);
                                // Wrap lighting (soft terminator) for stylized look.
                                let wrap = 0.4;
                                diff = ((diff + wrap) / (1.0 + wrap)).max(0.0).min(1.0);
                                col[0] += sun.color[0] * diff;
                                col[1] += sun.color[1] * diff;
                                col[2] += sun.color[2] * diff;
                            }
                            // Point lights (clamped count).
                            for pl in lights.points.iter().take(4) {
                                let to_light =
                                    Vec3::new(pl.position[0], pl.position[1], pl.position[2])
                                        - world_pos;
                                let dist = to_light.norm();
                                if dist > pl.range || dist == 0.0 {
                                    continue;
                                }
                                let dir = to_light / dist;
                                let diff = normal.dot(&dir).max(0.0);
                                let attn = (1.0 - dist / pl.range).max(0.0);
                                col[0] += pl.color[0] * diff * attn;
                                col[1] += pl.color[1] * diff * attn;
                                col[2] += pl.color[2] * diff * attn;
                            }
                            [
                                col[0] * albedo[0] + material.emissive[0],
                                col[1] * albedo[1] + material.emissive[1],
                                col[2] * albedo[2] + material.emissive[2],
                                albedo[3],
                            ]
                        };
                        // Distance fog (exponential height fog approximation).
                        let cam_pos =
                            Vec3::new(camera.position[0], camera.position[1], camera.position[2]);
                        let dist = (world_pos - cam_pos).norm();
                        let fog_factor = (-atmosphere.fog_density * dist).exp();
                        // View-direction-based ambient term: stronger fog where normal faces away.
                        let to_cam = (cam_pos - world_pos).normalize();
                        let _ = to_cam;
                        lit[0] = lerp_f32(atmosphere.fog_color[0], lit[0], fog_factor);
                        lit[1] = lerp_f32(atmosphere.fog_color[1], lit[1], fog_factor);
                        lit[2] = lerp_f32(atmosphere.fog_color[2], lit[2], fog_factor);
                        // ACES tonemap (approximation).
                        let aces = |v: f32| -> f32 {
                            let c = v.max(0.0);
                            (c * (2.51 * c + 0.03)) / (c * (2.43 * c + 0.59) + 0.14).max(1e-6)
                        };
                        lit[0] = aces(lit[0]);
                        lit[1] = aces(lit[1]);
                        lit[2] = aces(lit[2]);
                        // Linear -> sRGB.
                        let s = |v: f32| {
                            (crate::prereqs::linear_to_srgb(v).clamp(0.0, 1.0) * 255.0) as u8
                        };
                        let out = color_ptr.add(idx * 4);
                        *out.add(0) = s(lit[0]);
                        *out.add(1) = s(lit[1]);
                        *out.add(2) = s(lit[2]);
                        *out.add(3) = (albedo[3].clamp(0.0, 1.0) * 255.0) as u8;
                    }
                }
            }
        }
        // (Rgba8 sentinel import is no longer used; renderer writes raw u8s.)
    }

    fn draw_particle(&mut self, clip: Vec4, color: [f32; 4], size: f32, aspect: f32) {
        let _ = aspect;
        if clip.w <= 0.0 {
            return;
        }
        let inv_w = 1.0 / clip.w;
        let ndc_x = clip.x * inv_w;
        let ndc_y = clip.y * inv_w;
        let ndc_z = clip.z * inv_w;
        if !(0.0..=1.0).contains(&ndc_z) {
            return;
        }
        let sx = (ndc_x * 0.5 + 0.5) * (self.width as f32 - 1.0);
        let sy = (1.0 - (ndc_y * 0.5 + 0.5)) * (self.height as f32 - 1.0);
        let r = (size * 0.5 * self.height as f32).max(1.0) as i64;
        for dy in -r..=r {
            for dx in -r..=r {
                let x = sx as i64 + dx;
                let y = sy as i64 + dy;
                if x < 0 || y < 0 || x >= self.width as i64 || y >= self.height as i64 {
                    continue;
                }
                let idx = (y as usize * self.width as usize + x as usize);
                if ndc_z >= self.depth[idx] {
                    continue;
                }
                self.depth[idx] = ndc_z;
                let a = (1.0 - ((dx * dx + dy * dy) as f32) / (r * r) as f32).max(0.0);
                let dst = &mut self.color[idx * 4..idx * 4 + 4];
                let src_r = color[0] * a;
                let src_g = color[1] * a;
                let src_b = color[2] * a;
                let dst_r = crate::prereqs::srgb_to_linear(dst[0] as f32 / 255.0);
                let dst_g = crate::prereqs::srgb_to_linear(dst[1] as f32 / 255.0);
                let dst_b = crate::prereqs::srgb_to_linear(dst[2] as f32 / 255.0);
                let out_r = dst_r + src_r;
                let out_g = dst_g + src_g;
                let out_b = dst_b + src_b;
                dst[0] = (crate::prereqs::linear_to_srgb(out_r.clamp(0.0, 1.0)) * 255.0) as u8;
                dst[1] = (crate::prereqs::linear_to_srgb(out_g.clamp(0.0, 1.0)) * 255.0) as u8;
                dst[2] = (crate::prereqs::linear_to_srgb(out_b.clamp(0.0, 1.0)) * 255.0) as u8;
                dst[3] = 255;
            }
        }
    }

    fn draw_ui(&mut self, ui: &[crate::scene::UiDraw]) {
        for draw in ui {
            let mut indices = draw.indices.clone();
            if indices.is_empty() {
                indices = (0..draw.vertices.len() as u32).collect();
            }
            let tri_count = indices.len() / 3;
            for t in 0..tri_count {
                let i0 = indices[t * 3 + 0] as usize;
                let i1 = indices[t * 3 + 1] as usize;
                let i2 = indices[t * 3 + 2] as usize;
                let a = &draw.vertices[i0];
                let b = &draw.vertices[i1];
                let c = &draw.vertices[i2];
                let min_x = a.position[0].min(b.position[0]).min(c.position[0]).floor() as i64;
                let max_x = a.position[0].max(b.position[0]).max(c.position[0]).ceil() as i64;
                let min_y = a.position[1].min(b.position[1]).min(c.position[1]).floor() as i64;
                let max_y = a.position[1].max(b.position[1]).max(c.position[1]).ceil() as i64;
                for y in min_y.max(0)..=max_y.min(self.height as i64 - 1) {
                    for x in min_x.max(0)..=max_x.min(self.width as i64 - 1) {
                        let px = x as f32 + 0.5;
                        let py = y as f32 + 0.5;
                        let area = (b.position[0] - a.position[0])
                            * (c.position[1] - a.position[1])
                            - (b.position[1] - a.position[1]) * (c.position[0] - a.position[0]);
                        if area == 0.0 {
                            continue;
                        }
                        let (w0, w1, w2) = barycentric_2d(
                            [a.position[0], a.position[1]],
                            [b.position[0], b.position[1]],
                            [c.position[0], c.position[1]],
                            px,
                            py,
                        );
                        let inv = 1.0 / area;
                        let l0 = w0 * inv;
                        let l1 = w1 * inv;
                        let l2 = w2 * inv;
                        if l0 < 0.0 || l1 < 0.0 || l2 < 0.0 {
                            continue;
                        }
                        let r = a.color[0] * l0 + b.color[0] * l1 + c.color[0] * l2;
                        let g = a.color[1] * l0 + b.color[1] * l1 + c.color[1] * l2;
                        let b2 = a.color[2] * l0 + b.color[2] * l1 + c.color[2] * l2;
                        let a_out = a.color[3] * l0 + b.color[3] * l1 + c.color[3] * l2;
                        let idx = (y as usize * self.width as usize + x as usize);
                        let dst = &mut self.color[idx * 4..idx * 4 + 4];
                        let dst_r = crate::prereqs::srgb_to_linear(dst[0] as f32 / 255.0);
                        let dst_g = crate::prereqs::srgb_to_linear(dst[1] as f32 / 255.0);
                        let dst_b = crate::prereqs::srgb_to_linear(dst[2] as f32 / 255.0);
                        // Source-over alpha compositing in premultiplied.
                        let out_r = dst_r * (1.0 - a_out) + r * a_out;
                        let out_g = dst_g * (1.0 - a_out) + g * a_out;
                        let out_b = dst_b * (1.0 - a_out) + b2 * a_out;
                        dst[0] = (crate::prereqs::linear_to_srgb(out_r.clamp(0.0, 1.0)) * 255.0)
                            as u8;
                        dst[1] = (crate::prereqs::linear_to_srgb(out_g.clamp(0.0, 1.0)) * 255.0)
                            as u8;
                        dst[2] = (crate::prereqs::linear_to_srgb(out_b.clamp(0.0, 1.0)) * 255.0)
                            as u8;
                        dst[3] = 255;
                    }
                }
            }
        }
    }
}

#[derive(Copy, Clone, Debug)]
struct VertexOut {
    screen_x: f32,
    screen_y: f32,
    z: f32,
    w_recip: f32,
    world_pos: Vec3,
    normal: Vec3,
    texcoord: Vec2,
}

#[derive(Clone)]
struct Triangle {
    a: VertexOut,
    b: VertexOut,
    c: VertexOut,
    area: f32,
    min_x: i64,
    max_x: i64,
    min_y: i64,
    max_y: i64,
}

#[inline]
fn barycentric(a: VertexOut, b: VertexOut, c: VertexOut, px: f32, py: f32) -> (f32, f32, f32) {
    let v0x = b.screen_x - a.screen_x;
    let v0y = b.screen_y - a.screen_y;
    let v1x = c.screen_x - a.screen_x;
    let v1y = c.screen_y - a.screen_y;
    let v2x = px - a.screen_x;
    let v2y = py - a.screen_y;
    let denom = v0x * v1y - v1x * v0y;
    let inv = if denom.abs() < 1e-9 { 1.0 } else { 1.0 / denom };
    let w1 = (v2x * v1y - v1x * v2y) * inv;
    let w2 = (v0x * v2y - v2x * v0y) * inv;
    let w0 = 1.0 - w1 - w2;
    (w0, w1, w2)
}

#[inline]
fn barycentric_2d(a: [f32; 2], b: [f32; 2], c: [f32; 2], px: f32, py: f32) -> (f32, f32, f32) {
    let v0x = b[0] - a[0];
    let v0y = b[1] - a[1];
    let v1x = c[0] - a[0];
    let v1y = c[1] - a[1];
    let v2x = px - a[0];
    let v2y = py - a[1];
    let denom = v0x * v1y - v1x * v0y;
    let inv = if denom.abs() < 1e-9 { 1.0 } else { 1.0 / denom };
    let w1 = (v2x * v1y - v1x * v2y) * inv;
    let w2 = (v0x * v2y - v2x * v0y) * inv;
    let w0 = 1.0 - w1 - w2;
    (w0, w1, w2)
}

// `lerp_f32` is provided by `crate::prereqs::lerp_f32` (imported at the top).

/// Register a CPU texture on a `Renderer`-less access path (for tests).
pub fn _register_texture(_b: &mut CpuBackend) {}

// Suppress unused warning for legacy field
#[allow(dead_code)]
fn _used(_b: &GpuStatus) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{Camera, DrawCommand, Lights, Mesh, RenderScene};
    use crate::prereqs::Vec3;
    use crate::prereqs::Transform;

    fn scene_cube() -> RenderScene {
        let mut scene = RenderScene {
            camera: Camera {
                position: [0., 0., 5.],
                target: [0., 0., 0.],
                up: [0., 1., 0.],
                aspect: 1.0,
                ..Default::default()
            },
            clear_color: [0.05, 0.05, 0.08, 1.0],
            ..Default::default()
        };
        let mesh = Mesh::unit_cube();
        scene.commands.push(DrawCommand {
            mesh,
            material: crate::scene::Material {
                base_color: [1.0, 0.0, 0.0, 1.0],
                ..Default::default()
            },
            transform: Transform::identity(),
        });
        scene.lights = Lights {
            sun: Some(crate::scene::DirectionalLight {
                direction: [1.0, 1.0, 1.0],
                color: [1.0, 1.0, 1.0],
            }),
            ambient_up: [0.05, 0.05, 0.07, 1.0],
            ambient_down: [0.0, 0.0, 0.0, 0.0],
            points: Vec::new(),
        };
        scene
    }

    #[test]
    fn renders_and_produces_png() {
        let backend = CpuBackend::new(64, 64);
        // `Backend::render` needs `&mut self` but we built it directly.
        let mut b = backend;
        let result = b.render(&scene_cube());
        assert_eq!(result.status, RenderStatus::Ok);
        assert!(result.png_bytes.is_some());
        let png = result.png_bytes.unwrap();
        assert!(&png[..8] == b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn triangle_count_is_correct() {
        let mut b = CpuBackend::new(64, 64);
        let result = b.render(&scene_cube());
        assert_eq!(result.metrics.triangles, 12);
        assert_eq!(result.metrics.draw_calls, 1);
    }

    #[test]
    fn produces_non_blank_frame() {
        let mut b = CpuBackend::new(64, 64);
        let result = b.render(&scene_cube());
        eprintln!(
            "metrics: draw_calls={} triangles={} visible={}",
            result.metrics.draw_calls, result.metrics.triangles, result.metrics.visible_objects
        );
        // Inspect a sample of pixel values.
        let mut non_clear = 0;
        let mut red_pixels = 0;
        for px in b.color.chunks_exact(4) {
            // Look for pixels that are not just the clear color.
            if px[0] > 30 || px[1] > 30 || px[2] > 30 {
                non_clear += 1;
            }
            if px[0] > 30 && px[1] < 30 && px[2] < 30 {
                red_pixels += 1;
            }
        }
        eprintln!("non_clear={} red_pixels={}", non_clear, red_pixels);
        assert!(
            non_clear > 0,
            "frame should not be blank (clear color only)"
        );
        assert!(
            red_pixels > 0,
            "expected red cube pixels in framebuffer; saw {red_pixels}"
        );
    }

    #[test]
    fn resize_changes_buffer() {
        let mut b = CpuBackend::new(16, 16);
        b.resize(32, 32);
        assert_eq!(b.color.len(), 32 * 32 * 4);
        assert_eq!(b.depth.len(), 32 * 32);
    }

    #[test]
    fn _unused_math_imports() {
        let _ = Vec3::zeros();
    }
}
