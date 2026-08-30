use std::time::{Duration, Instant};

use arcane_render::Backend;

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info,arcane_render=debug"))
        .format_timestamp_millis()
        .init();
    log::info!("Mystical Arcana v0.1.0 — running on the Arcane engine");

    log::info!("initializing Vulkan renderer (lavapipe CPU driver expected on this host)");
    let mut backend = Backend::new(800, 600).map_err(|e| {
        log::error!("backend init failed: {e:?}");
        anyhow::anyhow!(e.to_string())
    })?;
    log::info!("renderer ready; device = {}", backend.ctx.device_name());
    log::info!("observatory: http://localhost:8080/");

    // Render 60 frames then quit (or stay alive for the browser to inspect).
    let start = Instant::now();
    let mut frame_index = 0u64;
    let mut last_log = Instant::now();
    while frame_index < 60 {
        if let Err(e) = backend.render_one(frame_index) {
            log::error!("render_one failed at frame {frame_index}: {e:?}");
            break;
        }
        frame_index += 1;
        if last_log.elapsed() > Duration::from_secs(1) {
            let fps = frame_index as f64 / start.elapsed().as_secs_f64().max(0.001);
            log::info!("rendered {frame_index} frames, ~{fps:.1} fps");
            last_log = Instant::now();
        }
    }

    // Keep the process alive for a few seconds so the browser can fetch the last frame.
    log::info!("render loop done; staying alive 5 s for observatory inspection");
    std::thread::sleep(Duration::from_secs(5));

    log::info!("shutting down Arcane");
    Ok(())
}
