//! Mystical Arcana — game entry point.
//!
//! Wires the Arcane engine together: Vulkan renderer (`arcane_render`) draws
//! a colored triangle to a headless swapchain, copies each rendered frame
//! back to a host-visible buffer, and publishes that buffer over HTTP via the
//! Render Observatory so a browser can watch the engine run live.
//!
//! ## Run modes
//!
//! The loop runs until one of:
//!   * `MYSTICAL_FRAMES` env var limits reached (default: 60 — keeps CI bounded)
//!   * `MYSTICAL_FOREVER=1` runs indefinitely until SIGINT
//!   * a Vulkan validation ERROR fires (checked via `validation_failed()`)
//!   * a non-validation `render_one` error recurs more than 3 times
//!
//! ## Exit codes
//!   * 0 — clean shutdown
//!   * 1 — renderer initialization failed
//!   * 2 — Vulkan validation layer reported an ERROR during the render loop
//!   * 3 — too many render errors from `render_one`

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use arcane_ecs::World;
use arcane_render::{validation_failed, Backend, MeshKind, SceneInstance, ShaderWatcher};
use arcane_world::{MeshKindComponent, Spin, Transform};

/// Set to true by the SIGINT handler. The render loop polls this between
/// frames so the Vulkan destructor sequence still runs (device_wait_idle →
/// destroy_*) before process exit, rather than being torn down by the default
/// SIGINT disposition (which would skip Drop entirely).
static STOP: AtomicBool = AtomicBool::new(false);

fn install_sigint_handler() {
    // signal_hook::low_level::register accepts any `Fn() + Send + Sync +
    // 'static` closure. The closure just flips a static atomic, which is
    // async-signal-safe because atomic stores on x86_64 are atomic
    // instructions and don't allocate or acquire locks.
    //
    // The unsafe block is contained to this single registration call.
    log::debug!("installing SIGINT handler");
    unsafe {
        match signal_hook::low_level::register(signal_hook::consts::SIGINT, || {
            STOP.store(true, Ordering::SeqCst);
        }) {
            Ok(_id) => log::info!("SIGINT handler registered (Ctrl-C will shut down cleanly)"),
            Err(e) => log::error!("SIGINT handler registration failed: {e:?}"),
        }
    }
}

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info,arcane_render=debug"),
    )
    .format_timestamp_millis()
    .init();

    log::info!("Mystical Arcana v0.1.0 — running on the Arcane engine");

    let width: u32 = std::env::var("MYSTICAL_WIDTH")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(800);
    let height: u32 = std::env::var("MYSTICAL_HEIGHT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(600);
    let frame_cap: u64 = std::env::var("MYSTICAL_FRAMES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(60);
    let forever: bool = std::env::var("MYSTICAL_FOREVER")
        .map(|s| s == "1" || s.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let observatory_addr = std::env::var("MYSTICAL_OBSERVATORY")
        .unwrap_or_else(|_| "0.0.0.0:8080".to_string());
    let warmup_ms: u64 = std::env::var("MYSTICAL_WARMUP_MS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(2000);
    let hotreload: bool = std::env::var("MYSTICAL_HOTRELOAD")
        .map(|s| s == "1" || s.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let shader_dir = std::env::var("MYSTICAL_SHADER_DIR")
        .unwrap_or_else(|_| "crates/arcane_render/shaders/spirv".to_string());
    if hotreload {
        log::info!(
            "hot-reload ENABLED: watching {} for *.spv changes",
            shader_dir
        );
    }

    log::info!(
        "config: size={width}x{height} frame_cap={frame_cap} forever={forever} observatory={observatory_addr} warmup_ms={warmup_ms}"
    );
    log::info!("initializing Vulkan renderer (lavapipe CPU driver expected on this host)");

    let mut backend = Backend::with_observatory(width, height, &observatory_addr).map_err(|e| {
        log::error!("backend init failed: {e:?}");
        anyhow::anyhow!(e.to_string())
    })?;
    log::info!("renderer ready; device = {}", backend.ctx.device_name());
    log::info!("observatory: http://{observatory_addr}/");

    install_sigint_handler();

    // Optional shader hot-reload watcher. When MYSTICAL_HOTRELOAD=1, poll
    // the .spv directory each frame and rebuild the pipeline when any
    // shader file's mtime advances.
    let mut shader_watcher = if hotreload {
        Some(ShaderWatcher::new(&shader_dir))
    } else {
        None
    };

    // Render loop.
    let start = Instant::now();
    let mut frame_index: u64 = 0;
    let mut last_log = Instant::now();
    let mut validation_failed_count: u64 = 0;
    let mut render_error_count: u64 = 0;

    // Build the scene as an ECS world. Each entity has a Transform
    // (position + rotation_y + scale) and a MeshKindComponent. Entities
    // that should spin also get a Spin(rate). The render loop iterates
    // the world each frame, advances each spinning entity's rotation,
    // then builds the per-frame scene slice for the renderer.
    //
    // Object layout (camera orbits at radius 8 around Y, looking at origin):
    //   [0] ground  20x20 at y=-2 (no spin)
    //   [1] center cube  (0, 0, 0) — slow spin
    //   [2] left cube   (-3, 0, 0) — fast spin
    //   [3] right cube  (+3, 0, 0) — reverse spin
    //   [4] top cube    (0, 2.5, 0) — medium spin, scaled 0.6
    //   [5] sphere      (0, 0.5, -3) — counter-rotating around Y
    //   [6] pyramid     (-3, 0.5, -3) — slow spin
    //   [7] pyramid     (3, 0.5, -3) — reverse slow spin
    //   [8] octahedron (loaded OBJ asset)  (0, 0.5, 3) — fast spin
    use arcane_math::Vec3;
    let mut world = World::new();
    {
        // Ground plane (no spin).
        let e = world.spawn();
        world.attach(e, Transform::at(Vec3::new(0.0, -2.0, 0.0)));
        world.attach(e, MeshKindComponent(MeshKind::Plane));
        // Center cube.
        let e = world.spawn();
        world.attach(e, Transform::at(Vec3::new(0.0, 0.0, 0.0)));
        world.attach(e, MeshKindComponent(MeshKind::Cube));
        world.attach(e, Spin::new(0.05));
        // Left cube.
        let e = world.spawn();
        world.attach(e, Transform::at(Vec3::new(-3.0, 0.0, 0.0)));
        world.attach(e, MeshKindComponent(MeshKind::Cube));
        world.attach(e, Spin::new(0.08));
        // Right cube.
        let e = world.spawn();
        world.attach(e, Transform::at(Vec3::new(3.0, 0.0, 0.0)));
        world.attach(e, MeshKindComponent(MeshKind::Cube));
        world.attach(e, Spin::new(-0.04));
        // Top cube (smaller).
        let e = world.spawn();
        world.attach(
            e,
            Transform::at(Vec3::new(0.0, 2.5, 0.0)).with_scale(Vec3::new(0.6, 0.6, 0.6)),
        );
        world.attach(e, MeshKindComponent(MeshKind::Cube));
        world.attach(e, Spin::new(0.06));
        // Sphere behind.
        let e = world.spawn();
        world.attach(e, Transform::at(Vec3::new(0.0, 0.5, -3.0)));
        world.attach(e, MeshKindComponent(MeshKind::Sphere));
        world.attach(e, Spin::new(-0.03));
        // Left-back pyramid.
        let e = world.spawn();
        world.attach(e, Transform::at(Vec3::new(-3.0, 0.5, -3.0)));
        world.attach(e, MeshKindComponent(MeshKind::Pyramid));
        world.attach(e, Spin::new(0.04));
        // Right-back pyramid.
        let e = world.spawn();
        world.attach(e, Transform::at(Vec3::new(3.0, 0.5, -3.0)));
        world.attach(e, MeshKindComponent(MeshKind::Pyramid));
        world.attach(e, Spin::new(-0.05));
        // Octahedron (loaded OBJ asset, in front of the camera).
        let e = world.spawn();
        world.attach(
            e,
            Transform::at(Vec3::new(0.0, 0.5, 3.0)).with_scale(Vec3::new(0.8, 0.8, 0.8)),
        );
        world.attach(e, MeshKindComponent(MeshKind::LoadedObj));
        world.attach(e, Spin::new(0.07));
    }
    log::info!("ECS scene: {} entities, {} spinning", world.count(), world.entities_with::<Spin>().len());

    while !STOP.load(Ordering::SeqCst) {
        if !forever && frame_index >= frame_cap {
            break;
        }

        // Optional hot-reload check (no-op when watcher is None).
        if let Some(w) = shader_watcher.as_mut() {
            match backend.hotreload_if_changed(w, "lit_textured.vert.spv", "lit_textured.frag.spv") {
                Ok(true) => log::info!("hot-reload applied at frame {frame_index}"),
                Ok(false) => {},
                Err(e) => log::warn!("hot-reload skipped at frame {frame_index}: {e:?}"),
            }
        }

        // Advance the ECS: each entity with a Spin component gets its
        // Transform.rotation_y incremented by the spin rate.
        let spinning = world.entities_with::<Spin>();
        for e in spinning {
            // Read the spin rate first (immutable borrow), then mutate
            // the transform. The two-step borrow pattern keeps the borrow
            // checker happy without cloning the world.
            let rate = world.get::<Spin>(e).map(|s| s.rate).unwrap_or(0.0);
            if let Some(t) = world.get_mut::<Transform>(e) {
                t.rotation_y += rate;
            }
        }

        // Build the per-frame scene slice from the ECS world.
        let mut scene: Vec<SceneInstance> = Vec::with_capacity(world.count());
        for e in world.entities_with::<Transform>() {
            let (kind, model) = {
                let kind = world.get::<MeshKindComponent>(e).map(|c| c.0);
                let model = world.get::<Transform>(e).map(|t| t.to_model_matrix());
                match (kind, model) {
                    (Some(k), Some(m)) => (k, m),
                    _ => continue,
                }
            };
            scene.push(SceneInstance::new(kind, model));
        }

        // Check for SIGINT once per frame; the cost is one SeqCst atomic
        // load (~1 ns) which is negligible next to a Vulkan frame.
        // We can't check inside render_objects (the GPU work isn't interruptible
        // from the CPU side) but the per-frame cadence is fast enough that
        // Ctrl-C responds within one frame on lavapipe (~1 ms at 1000 fps).
        // On a real GPU at 60 fps this still means ~16 ms response time.

        match backend.render_scene(frame_index, &scene) {
            Ok(()) => {}
            Err(e) => {
                render_error_count += 1;
                log::error!("render_scene failed at frame {frame_index}: {e:?}");
                if render_error_count > 3 {
                    log::error!("too many render errors; aborting loop");
                    std::process::exit(3);
                }
                // Otherwise skip ahead and try the next frame — lavapipe can
                // occasionally surface a transient VK_ERROR_OUT_OF_DATE_KHR on
                // the first frame; the next iteration usually recovers.
            }
        }

        // Check Vulkan validation layer state every frame. The messenger
        // callback sets a global atomic when it sees an ERROR; we then abort
        // immediately so the user sees the validation failure rather than a
        // misleading later crash. We only log when the count actually rises
        // so the validation-counted error path produces exactly one log line.
        if validation_failed() {
            validation_failed_count += 1;
            log::error!(
                "Vulkan validation layer reported an ERROR at frame {frame_index} (total: {validation_failed_count}); tearing down"
            );
            // Drop the backend explicitly so Vulkan destructors run in order.
            drop(backend);
            std::process::exit(2);
        }

        frame_index += 1;

        if last_log.elapsed() > Duration::from_secs(1) {
            let elapsed = start.elapsed().as_secs_f64().max(0.001);
            let fps = frame_index as f64 / elapsed;
            log::info!(
                "rendered {frame_index} frames in {elapsed:.2}s (~{fps:.1} fps); errors={render_error_count} validation_fails={validation_failed_count}"
            );
            last_log = Instant::now();
        }
    }

    if STOP.load(Ordering::SeqCst) {
        log::info!("SIGINT received; finishing frame {frame_index} and tearing down Vulkan");
    }

    log::info!("render loop done; rendered {frame_index} frames total");

    // Keep the process alive briefly so the browser can fetch the last frame
    // from the Observatory before the http thread goes down.
    log::info!("staying alive {warmup_ms}ms for observatory inspection");
    let keep = Instant::now();
    while keep.elapsed() < Duration::from_millis(warmup_ms) {
        if STOP.load(Ordering::SeqCst) {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    // Drop runs the Vulkan destructor sequence: device_wait_idle, then
    // readback → mesh → pipeline → frame → swapchain → ctx, in that order.
    drop(backend);
    log::info!("shutting down Arcane");
    Ok(())
}
