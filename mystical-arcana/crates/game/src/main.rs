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

use arcane_render::{validation_failed, Backend};

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

    // Render loop.
    let start = Instant::now();
    let mut frame_index: u64 = 0;
    let mut last_log = Instant::now();
    let mut validation_failed_count: u64 = 0;
    let mut render_error_count: u64 = 0;

    // Build the scene: a row of three cubes plus one floating above.
    // Each cube has a fixed world-space offset and rotates around Y at
    // its own rate so the scene has visible motion between frames.
    //
    // Cube layout (camera at (0,0,+5) looking down -Z):
    //   [0] center  (0, 0,  0) — slow spin
    //   [1] left    (-3, 0, 0) — fast spin
    //   [2] right   (+3, 0, 0) — reverse slow spin
    //   [3] top      (0, 2, 0) — medium spin (above the center cube)
    use arcane_math::{Mat4, Vec3};

    while !STOP.load(Ordering::SeqCst) {
        if !forever && frame_index >= frame_cap {
            break;
        }

        // Per-frame model matrices. Each cube has a fixed translation
        // and a Y rotation that increments with frame_index (so the scene
        // animates without input).
        let t = frame_index as f32;
        let models: [Mat4; 4] = [
            // [0] center: 0.05 rad/frame ~ 50 deg/sec at 1000 fps
            Mat4::from_translation(Vec3::new(0.0, 0.0, 0.0))
                * Mat4::from_rotation_y(t * 0.05),
            // [1] left: 0.08 rad/frame
            Mat4::from_translation(Vec3::new(-3.0, 0.0, 0.0))
                * Mat4::from_rotation_y(t * 0.08),
            // [2] right: -0.04 rad/frame (reverse)
            Mat4::from_translation(Vec3::new(3.0, 0.0, 0.0))
                * Mat4::from_rotation_y(-t * 0.04),
            // [3] top: 0.06 rad/frame, also smaller (scale 0.6) so it
            // doesn't visually merge with the center cube.
            Mat4::from_translation(Vec3::new(0.0, 2.5, 0.0))
                * Mat4::from_scale(Vec3::new(0.6, 0.6, 0.6))
                * Mat4::from_rotation_y(t * 0.06),
        ];

        // Check for SIGINT once per frame; the cost is one SeqCst atomic
        // load (~1 ns) which is negligible next to a Vulkan frame.
        // We can't check inside render_objects (the GPU work isn't interruptible
        // from the CPU side) but the per-frame cadence is fast enough that
        // Ctrl-C responds within one frame on lavapipe (~1 ms at 1000 fps).
        // On a real GPU at 60 fps this still means ~16 ms response time.

        match backend.render_objects(frame_index, &models) {
            Ok(()) => {}
            Err(e) => {
                render_error_count += 1;
                log::error!("render_objects failed at frame {frame_index}: {e:?}");
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
