//! Command-line interface for Mystical Arcana.
//!
//! Usage examples:
//!
//!   mystical_arcana --observatory --port 8765
//!     Run the live observatory server, serving the latest frame at
//!     http://127.0.0.1:8765/
//!
//!   mystical_arcana --headless --visualize --scenario terrain_scene --output /tmp/frame.png
//!     Render a deterministic scenario frame and save PNG to disk.
//!
//!   mystical_arcana --headless --smoke
//!     Run the existing headless gameplay smoke test.
//!
//!   mystical_arcana --backend vulkan --headless --scenario basic_scene --frames 1 --capture /tmp/vulkan.png
//!     Same as --output but with Vulkan backend.

use std::sync::Arc;
use std::time::Duration;

use pico_args::Arguments;

use arcane_render::{BackendKind, Observatory, Renderer};

use crate::scenario::ScenarioKind;
use crate::session::GameSession;

/// Backend selection on the CLI.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CliBackend {
    /// CPU software rasterizer.
    Cpu,
    /// Vulkan backend (real GPU or Lavapipe software fallback).
    Vulkan,
    /// Automatic — prefer Vulkan when not headless; CPU when headless.
    Auto,
}

impl CliBackend {
    /// Convert to a `BackendKind`, choosing headless vs. windowed.
    pub fn to_backend_kind(self, headless: bool) -> BackendKind {
        match self {
            CliBackend::Cpu => BackendKind::Cpu,
            CliBackend::Vulkan => BackendKind::Vulkan { headless },
            CliBackend::Auto => {
                if headless { BackendKind::Cpu } else { BackendKind::Vulkan { headless: false } }
            }
        }
    }
}

/// CLI actions parsed from arguments.
#[derive(Debug, Clone)]
pub struct CliOptions {
    /// Headless mode (no window).
    pub headless: bool,
    /// Render a deterministic scenario frame.
    pub visualize: bool,
    /// Run the smoke test.
    pub smoke: bool,
    /// Scenario to render.
    pub scenario: Option<ScenarioKind>,
    /// Fixed simulation frame index.
    pub frame: Option<u32>,
    /// Number of frames to render (multi-frame simulation).
    pub frames: Option<u32>,
    /// Output PNG file path.
    pub output: Option<String>,
    /// Same as --output (Vulkan convention).
    pub capture: Option<String>,
    /// Backend.
    pub backend: CliBackend,
    /// Observatory HTTP port.
    pub port: u16,
    /// Run the observatory.
    pub observatory: bool,
    /// Framebuffer width.
    pub width: u32,
    /// Framebuffer height.
    pub height: u32,
    /// World seed.
    pub seed: u64,
    /// Pre-set simulation seconds (overrides --frame).
    pub sim_seconds: f32,
}

impl Default for CliOptions {
    fn default() -> Self {
        Self {
            headless: false,
            visualize: false,
            smoke: false,
            scenario: None,
            frame: None,
            frames: None,
            output: None,
            capture: None,
            backend: CliBackend::Auto,
            port: 8765,
            observatory: false,
            width: 1280,
            height: 720,
            seed: 0xC0FFEE,
            sim_seconds: 0.0,
        }
    }
}

/// Parse command-line arguments.
pub fn parse_cli(args: Vec<String>) -> Result<CliOptions, String> {
    let mut p = Arguments::from_vec(args.into_iter().map(std::ffi::OsString::from).collect());
    let mut opts = CliOptions::default();
    if p.contains("--headless") { opts.headless = true; }
    if p.contains("--visualize") { opts.visualize = true; }
    if p.contains("--smoke") { opts.smoke = true; }
    if p.contains("--observatory") { opts.observatory = true; }
    if let Some(v) = p.opt_value_from_str::<_, String>("--scenario").ok().flatten() {
        opts.scenario = ScenarioKind::from_str(v.as_str());
        if opts.scenario.is_none() {
            return Err(format!("unknown scenario: {}", v));
        }
    }
    opts.frame = p.opt_value_from_str("--frame").ok().flatten();
    opts.frames = p.opt_value_from_str("--frames").ok().flatten();
    opts.output = p.opt_value_from_str("--output").ok().flatten();
    opts.capture = p.opt_value_from_str("--capture").ok().flatten();
    opts.backend = p.opt_value_from_str::<_, String>("--backend")
        .ok().flatten()
        .map(|s| match s.as_str() {
            "cpu" => CliBackend::Cpu,
            "vulkan" => CliBackend::Vulkan,
            _ => CliBackend::Auto,
        })
        .unwrap_or(opts.backend);
    opts.port = p.opt_value_from_str("--port")
        .ok().flatten()
        .unwrap_or(opts.port);
    opts.width = p.opt_value_from_str("--width")
        .ok().flatten()
        .unwrap_or(opts.width);
    opts.height = p.opt_value_from_str("--height")
        .ok().flatten()
        .unwrap_or(opts.height);
    opts.seed = p.opt_value_from_str("--seed")
        .ok().flatten()
        .unwrap_or(opts.seed);
    opts.sim_seconds = p.opt_value_from_str("--sim")
        .ok().flatten()
        .unwrap_or(opts.sim_seconds);
    Ok(opts)
}

/// Run a CLI invocation.
pub fn run(opts: CliOptions) -> Result<(), String> {
    // Defer to the existing headless smoke harness when requested.
    if opts.smoke {
        match crate::headless::run_until_complete(Duration::from_secs(2)) {
            Ok(elapsed) => {
                println!("[smoke] headless loop completed in {:.3}s", elapsed.as_secs_f32());
                println!("[smoke] OK");
                return Ok(());
            }
            Err(e) => {
                return Err(format!("[smoke] FAILED: {:?}", e));
            }
        }
    }

    let backend_kind = opts.backend.to_backend_kind(opts.headless);

    // Headless render path.
    if opts.headless && (opts.visualize || opts.scenario.is_some()) {
        return run_headless_render(opts, backend_kind);
    }

    // Observatory mode.
    if opts.observatory {
        return run_observatory(opts, backend_kind);
    }

    // Default: single render.
    let session = GameSession::new(backend_kind, opts.width, opts.height, opts.seed);
    let _ = session.render();
    Ok(())
}

fn run_headless_render(opts: CliOptions, backend_kind: BackendKind) -> Result<(), String> {
    let scenario_kind = opts.scenario.unwrap_or(ScenarioKind::Basic);
    let mut session = GameSession::new(backend_kind, opts.width, opts.height, opts.seed);
    session.scenario = Some(scenario_kind);
    let aspect = opts.width as f32 / opts.height.max(1) as f32;
    let sim_time = opts.sim_seconds
        .max(opts.frame.map(|f| f as f32 / 60.0).unwrap_or(0.0));
    let scenario = crate::scenario::Scenario::build(scenario_kind, sim_time, aspect);
    session.renderer_aspect = aspect;

    let result = session.renderer.render(&scenario.scene);

    let path = opts.output.clone().or(opts.capture.clone());
    if let Some(out_path) = path {
        if let Some(png) = &result.png_bytes {
            std::fs::write(&out_path, png)
                .map_err(|e| format!("write {}: {e}", out_path))?;
            println!("Wrote {} bytes to {}", png.len(), out_path);
        } else {
            return Err("no PNG produced".into());
        }
    }

    if let Some(frames) = opts.frames {
        if frames > 1 {
            println!("Simulating {} frames...", frames);
            for i in 1..frames {
                let dt = 1.0 / 60.0;
                session.step(dt);
                let s = crate::scenario::Scenario::build(scenario_kind,
                    sim_time + i as f32 * dt, aspect);
                let r = session.renderer.render(&s.scene);
                if let (Some(png), Some(out_path)) = (r.png_bytes.as_ref(), opts.output.as_ref()) {
                    let _ = std::fs::write(out_path, png);
                }
            }
        }
    }
    Ok(())
}

fn run_observatory(opts: CliOptions, backend_kind: BackendKind) -> Result<(), String> {
    let width = opts.width;
    let height = opts.height;
    let session = GameSession::new(backend_kind, width, height, opts.seed);
    let renderer = session.renderer.clone();
    let port = opts.port;
    let observatory = Observatory::new(renderer.clone(), port)
        .map_err(|e| format!("observatory bind: {e}"))?;
    let actual_port = observatory.port();
    println!("Observatory: http://127.0.0.1:{actual_port}/");

    let state = observatory.state();

    let renderer_for_loop = renderer.clone();
    let scenario = opts.scenario.unwrap_or(ScenarioKind::Basic);
    let aspect = width as f32 / height.max(1) as f32;
    let _loop_handle = std::thread::Builder::new()
        .name("arcane-render-loop".to_string())
        .spawn(move || {
            let mut frame_index: u32 = 0;
            loop {
                let sim = frame_index as f32 / 60.0;
                let sc = crate::scenario::Scenario::build(scenario, sim, aspect);
                let result = renderer_for_loop.render(&sc.scene);

                let mut state = state.write();
                state.renderer = arcane_render::RendererState {
                    backend: result.metrics.backend.clone(),
                    width: result.metrics.width,
                    height: result.metrics.height,
                    frame_time_us: result.metrics.frame_time_us,
                    fps: result.metrics.fps,
                    draw_calls: result.metrics.draw_calls,
                    triangles: result.metrics.triangles,
                    visible_objects: result.metrics.visible_objects,
                    loaded_meshes: result.metrics.loaded_meshes,
                    loaded_textures: result.metrics.loaded_textures,
                    active_materials: result.metrics.active_materials,
                    gpu_status: result.metrics.gpu_status,
                };
                state.camera = arcane_render::CameraState {
                    position: sc.scene.camera.position,
                    target: sc.scene.camera.target,
                    up: sc.scene.camera.up,
                    fov_y: sc.scene.camera.fov_y,
                    aspect: sc.scene.camera.aspect,
                    near: sc.scene.camera.near,
                    far: sc.scene.camera.far,
                };
                state.player = arcane_render::PlayerState {
                    position: [0.0, 5.0, 0.0],
                    velocity: [0.0, 0.0, 0.0],
                    health: 100.0,
                    mana: 100.0,
                    corruption: 0.0,
                    selected_spell: "spark".to_string(),
                };
                state.world = arcane_render::WorldState {
                    seed: opts.seed,
                    current_biome: "Boreal".to_string(),
                    mana_density: 0.4,
                    loaded_chunks: 1,
                    generated_chunks: 1,
                    entities: 0,
                    structures: 0,
                    nearby_mana_nodes: 0,
                };
                state.diagnostics = sc.diagnostics.iter()
                    .map(|d| arcane_render::Diagnostic {
                        severity: "info".to_string(),
                        source: "scenario".to_string(),
                        message: d.clone(),
                    })
                    .collect();
                drop(state);

                frame_index = frame_index.saturating_add(1);
                std::thread::sleep(Duration::from_millis(16));
            }
        });

    // Block forever (the observatory runs in its own background thread).
    loop { std::thread::sleep(Duration::from_secs(60)); }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_scenario_flag() {
        let args = vec!["--headless".into(), "--visualize".into(), "--scenario".into(),
                        "terrain_scene".into(), "--output".into(), "/tmp/x.png".into()];
        let opts = parse_cli(args).unwrap();
        assert!(opts.headless);
        assert!(opts.visualize);
        assert_eq!(opts.scenario, Some(ScenarioKind::Terrain));
        assert_eq!(opts.output.as_deref(), Some("/tmp/x.png"));
    }

    #[test]
    fn rejects_unknown_scenario() {
        let args = vec!["--scenario".into(), "nope".into()];
        assert!(parse_cli(args).is_err());
    }

    #[test]
    fn parses_smoke_flag() {
        let args = vec!["--smoke".into()];
        let opts = parse_cli(args).unwrap();
        assert!(opts.smoke);
    }
}
