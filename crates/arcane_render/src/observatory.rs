//! Arcane Render Observatory.
//!
//! A lightweight HTTP server exposing the renderer's current frame and
//! diagnostic state to a browser at `http://127.0.0.1:<port>/`.
//!
//! Exposed endpoints:
//!   GET /                 — Live viewer (auto-refreshing page)
//!   GET /frame.png        — Last rendered PNG (raw bytes)
//!   GET /frame            — JSON metadata about the last frame (size, age)
//!   GET /state            — Aggregated engine state (renderer/camera/player/world)
//!   GET /metrics          — Render metrics (machine-readable JSON)
//!   GET /scene            — Last rendered scene JSON
//!   GET /health           — Health-check JSON
//!
//! Implementation deliberately minimal: `tiny_http`, single worker thread.
//! Multiple concurrent reads are supported by sharing the renderer via
//! `Arc<Renderer>` (which is `Send + Sync` through its `RwLock` fields).

use std::net::TcpListener;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tiny_http::{Header, Method, Response, Server, StatusCode};

use crate::scene::RenderScene;
use crate::Renderer;

/// State snapshot provided by the host application.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ObservatoryState {
    /// Renderer status block.
    pub renderer: RendererState,
    /// Camera state.
    pub camera: CameraState,
    /// Player state (filled by the game session).
    pub player: PlayerState,
    /// World state.
    pub world: WorldState,
    /// Diagnostics (warnings/errors from engine subsystems).
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RendererState {
    pub backend: String,
    pub width: u32,
    pub height: u32,
    pub frame_time_us: u64,
    pub fps: f32,
    pub draw_calls: u32,
    pub triangles: u64,
    pub visible_objects: u32,
    pub loaded_meshes: u32,
    pub loaded_textures: u32,
    pub active_materials: u32,
    pub gpu_status: Option<crate::metrics::GpuStatus>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CameraState {
    pub position: [f32; 3],
    pub target: [f32; 3],
    pub up: [f32; 3],
    pub fov_y: f32,
    pub aspect: f32,
    pub near: f32,
    pub far: f32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PlayerState {
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub health: f32,
    pub mana: f32,
    pub corruption: f32,
    pub selected_spell: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct WorldState {
    pub seed: u64,
    pub current_biome: String,
    pub mana_density: f32,
    pub loaded_chunks: u32,
    pub generated_chunks: u32,
    pub entities: u32,
    pub structures: u32,
    pub nearby_mana_nodes: u32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Diagnostic {
    pub severity: String,
    pub source: String,
    pub message: String,
}

/// Handle to the observatory server. The server runs on a background thread.
pub struct Observatory {
    renderer: Arc<Renderer>,
    state: Arc<RwLock<ObservatoryState>>,
    server: Option<std::thread::JoinHandle<()>>,
    shutdown: Arc<std::sync::atomic::AtomicBool>,
    port: u16,
}

impl Observatory {
    /// Spawn a new observatory. Returns immediately; server runs in background.
    pub fn new(renderer: Arc<Renderer>, port: u16) -> std::io::Result<Self> {
        let state = Arc::new(RwLock::new(ObservatoryState::default()));
        let shutdown = Arc::new(std::sync::atomic::AtomicBool::new(false));

        // Verify port is bindable.
        let listener = TcpListener::bind(("127.0.0.1", port))?;
        let actual_port = listener.local_addr()?.port();
        drop(listener);

        let server_renderer = renderer.clone();
        let server_state = state.clone();
        let server_shutdown = shutdown.clone();
        let server = std::thread::Builder::new()
            .name("arcane-observatory".to_string())
            .spawn(move || {
                run_server(server_renderer, server_state, server_shutdown, actual_port);
            })?;

        Ok(Self {
            renderer,
            state,
            server: Some(server),
            shutdown,
            port: actual_port,
        })
    }

    /// Get a handle to the state for the host application to write to.
    pub fn state(&self) -> Arc<RwLock<ObservatoryState>> {
        self.state.clone()
    }

    /// The port the server is bound to (useful when port 0 was requested).
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Graceful shutdown (best-effort).
    pub fn shutdown(&self) {
        self.shutdown
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

impl Drop for Observatory {
    fn drop(&mut self) {
        self.shutdown
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

fn run_server(
    renderer: Arc<Renderer>,
    state: Arc<RwLock<ObservatoryState>>,
    shutdown: Arc<std::sync::atomic::AtomicBool>,
    port: u16,
) {
    let server = Server::http(("127.0.0.1", port)).unwrap_or_else(|e| {
        tracing::error!("Observatory bind failed: {e:?}");
        panic!("observatory bind failed");
    });
    tracing::info!("Observatory listening at http://127.0.0.1:{port}/");
    let timeout = Some(Duration::from_millis(250));
    for request in server.incoming_requests() {
        if shutdown.load(std::sync::atomic::Ordering::Relaxed) {
            let _ = request
                .respond(Response::from_string("observatory shutting down").with_status_code(503));
            break;
        }
        let method = request.method().clone();
        let url = request.url().to_string();
        let _ = handle_request(request, &method, &url, &renderer, &state, timeout);
    }
}

fn handle_request(
    request: tiny_http::Request,
    method: &Method,
    url: &str,
    renderer: &Renderer,
    state: &RwLock<ObservatoryState>,
    _timeout: Option<Duration>,
) -> std::io::Result<()> {
    if *method != Method::Get {
        let resp = Response::from_string("method not allowed").with_status_code(405);
        request.respond(resp)?;
        return Ok(());
    }

    let path = url.split('?').next().unwrap_or(url);
    match path {
        "/" => {
            let html = HTML_INDEX;
            request.respond(Response::from_string(html).with_header(
                Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap(),
            ))?;
        }
        "/frame.png" => {
            let png = renderer.last_frame_png();
            match png {
                Some(bytes) => {
                    let len = bytes.len();
                    request.respond(
                        Response::from_data(bytes.to_vec())
                            .with_header(
                                Header::from_bytes(&b"Content-Type"[..], &b"image/png"[..])
                                    .unwrap(),
                            )
                            .with_header(
                                Header::from_bytes(
                                    &b"Cache-Control"[..],
                                    &b"no-store, max-age=0"[..],
                                )
                                .unwrap(),
                            )
                            .with_status_code(200),
                    )?;
                    let _ = len;
                }
                None => {
                    request.respond(
                        Response::from_data(PLACEHOLDER_PNG_1x1)
                            .with_header(
                                Header::from_bytes(&b"Content-Type"[..], &b"image/png"[..])
                                    .unwrap(),
                            )
                            .with_status_code(503),
                    )?;
                }
            }
        }
        "/frame" => {
            let png = renderer.last_frame_png();
            let body = json!({
                "frame_available": png.is_some(),
                "size_bytes": png.as_ref().map(|b| b.len()),
                "backend": renderer.backend_kind(),
                "dimensions": {
                    "width": renderer.metrics_snapshot().width,
                    "height": renderer.metrics_snapshot().height,
                }
            });
            request.respond(json_response(&body, StatusCode::from(200)))?;
        }
        "/state" | "/state.json" => {
            let s = state.read().clone();
            request.respond(json_response(&s, StatusCode::from(200)))?;
        }
        "/metrics" | "/metrics.json" => {
            let m = renderer.metrics_snapshot();
            request.respond(json_response(&m, StatusCode::from(200)))?;
        }
        "/scene" | "/scene.json" => {
            let s = renderer.last_scene();
            match s {
                Some(scene) => {
                    request.respond(json_response(scene.as_ref(), StatusCode::from(200)))?
                }
                None => request.respond(json_response(
                    &serde_json::Value::Null,
                    StatusCode::from(200),
                ))?,
            }
        }
        "/health" => {
            let body = json!({ "status": "ok", "backend": renderer.backend_kind() });
            request.respond(json_response(&body, StatusCode::from(200)))?;
        }
        _ => {
            request.respond(Response::from_string("not found").with_status_code(404))?;
        }
    }
    Ok(())
}

fn json_response<T: Serialize>(
    value: &T,
    status: StatusCode,
) -> Response<std::io::Cursor<Vec<u8>>> {
    let body = serde_json::to_vec_pretty(value).unwrap_or_else(|_| b"{}".to_vec());
    Response::from_data(body)
        .with_status_code(status.0)
        .with_header(
            Header::from_bytes(
                &b"Content-Type"[..],
                &b"application/json; charset=utf-8"[..],
            )
            .unwrap(),
        )
        .with_header(
            Header::from_bytes(&b"Cache-Control"[..], &b"no-store, max-age=0"[..]).unwrap(),
        )
}

/// 1x1 transparent PNG (67 bytes), used when no frame has been rendered yet.
const PLACEHOLDER_PNG_1x1: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x01, 0x00, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
    0x42, 0x60, 0x82,
];

const HTML_INDEX: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Arcane Render Observatory</title>
<style>
  body { background: #0b0f17; color: #d6dee8; font-family: ui-sans-serif, system-ui, -apple-system, "Segoe UI", Roboto, Helvetica, Arial, sans-serif; margin: 0; padding: 24px; }
  h1 { margin: 0 0 4px 0; font-size: 24px; letter-spacing: 0.5px; color: #ffcf66; text-shadow: 0 0 16px #ffcf6644; }
  h1 small { color: #8895a7; font-weight: 400; font-size: 14px; }
  .grid { display: grid; grid-template-columns: 2fr 1fr; gap: 16px; margin-top: 16px; }
  .panel { background: #11161f; border: 1px solid #1c2535; border-radius: 8px; padding: 16px; }
  .frame-wrap { display: flex; flex-direction: column; gap: 8px; }
  #frame { width: 100%; height: auto; image-rendering: pixelated; background: #050608; border: 1px solid #2a3b54; border-radius: 6px; }
  .controls { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
  .controls label { color: #8895a7; font-size: 12px; }
  .controls input, .controls select { background: #0a0e16; color: #d6dee8; border: 1px solid #1c2535; border-radius: 4px; padding: 4px 6px; }
  .controls button { background: #2a3b54; color: #d6dee8; border: none; padding: 6px 12px; border-radius: 4px; cursor: pointer; font-size: 13px; }
  .controls button:hover { background: #3a5170; }
  .metrics { font-family: ui-monospace, "JetBrains Mono", monospace; font-size: 12px; color: #8895a7; }
  .metrics .row { display: flex; justify-content: space-between; padding: 4px 0; border-bottom: 1px solid #1c2535; }
  .metrics .row:last-child { border-bottom: none; }
  .metrics .key { color: #6a7787; }
  .metrics .value { color: #d6dee8; text-align: right; }
  .footer { margin-top: 24px; color: #4a5666; font-size: 11px; }
  .footer a { color: #4a8bbf; text-decoration: none; }
  .pulse { display: inline-block; width: 8px; height: 8px; border-radius: 50%; background: #6df08a; box-shadow: 0 0 8px #6df08a88; animation: pulse 1.4s infinite; margin-right: 6px; }
  @keyframes pulse { 0%, 100% { opacity: 1.0; } 50% { opacity: 0.4; } }
</style>
</head>
<body>
<h1>Arcane Render Observatory <small>· Mystical Arcana</small></h1>
<div class="grid">
  <div class="panel frame-wrap">
    <img id="frame" alt="rendered frame" src="/frame.png?initial">
    <div class="controls">
      <span class="pulse" id="pulse"></span>
      <label>Refresh (s):</label>
      <input id="rate" type="number" min="0" max="10" step="0.1" value="0.4">
      <button id="toggle" type="button">Pause</button>
      <button id="once" type="button">Render Once</button>
      <label>Backend:</label>
      <select id="backend"><option>auto</option><option>cpu</option><option>vulkan</option></select>
    </div>
  </div>
  <div class="panel">
    <div class="metrics" id="metrics">Loading...</div>
  </div>
</div>
<div class="footer">
  Endpoints: <a href="/frame.png">/frame.png</a> · <a href="/state">/state</a> · <a href="/metrics">/metrics</a> · <a href="/scene">/scene</a> · <a href="/health">/health</a>
</div>
<script>
  let paused = false;
  let rate = 0.4;
  let timer = null;
  const frame = document.getElementById('frame');
  const pulse = document.getElementById('pulse');
  const metrics = document.getElementById('metrics');
  const rateInput = document.getElementById('rate');
  const toggleBtn = document.getElementById('toggle');
  const onceBtn = document.getElementById('once');
  function scheduleNext() {
    if (paused) return;
    timer = setTimeout(refresh, Math.max(50, rate * 1000));
  }
  function refresh() {
    const tag = Date.now();
    frame.src = '/frame.png?ts=' + tag;
    fetch('/metrics?ts=' + tag).then(r => r.json()).then(m => {
      metrics.innerHTML = '';
      const rows = [
        ['Backend', m.backend],
        ['Resolution', (m.width || 0) + ' x ' + (m.height || 0)],
        ['Frame time', (m.frame_time_us || 0) + ' us'],
        ['FPS', (m.fps || 0).toFixed(1)],
        ['Draw calls', m.draw_calls || 0],
        ['Triangles', m.triangles || 0],
        ['Visible objects', m.visible_objects || 0],
        ['Loaded meshes', m.loaded_meshes || 0],
        ['Loaded textures', m.loaded_textures || 0],
        ['Active materials', m.active_materials || 0],
      ];
      if (m.gpu_status) {
        rows.push(['GPU', m.gpu_status.device_name]);
        rows.push(['API', m.gpu_status.api_version]);
        rows.push(['Validation', m.gpu_status.validation_enabled ? 'ON' : 'OFF']);
        rows.push(['Validation errors', m.gpu_status.validation_errors]);
      } else {
        rows.push(['GPU', '— (CPU backend)']);
      }
      rows.forEach(([k, v]) => {
        const row = document.createElement('div'); row.className = 'row';
        const key = document.createElement('span'); key.className = 'key'; key.textContent = k;
        const val = document.createElement('span'); val.className = 'value'; val.textContent = String(v);
        row.append(key, val);
        metrics.append(row);
      });
    }).catch(() => { pulse.style.background = '#ff4a4a'; });
    pulse.style.background = '#6df08a';
    scheduleNext();
  }
  rateInput.addEventListener('change', () => {
    rate = parseFloat(rateInput.value || '0.4');
    clearTimeout(timer); scheduleNext();
  });
  toggleBtn.addEventListener('click', () => {
    paused = !paused;
    toggleBtn.textContent = paused ? 'Resume' : 'Pause';
    if (paused) { clearTimeout(timer); pulse.style.background = '#ffb946'; }
    else { scheduleNext(); }
  });
  onceBtn.addEventListener('click', () => {
    frame.src = '/frame.png?manual=' + Date.now();
  });
  document.getElementById('backend').addEventListener('change', (e) => {
    // Currently informational only; the host app decides the backend.
  });
  refresh();
</script>
</body>
</html>"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::BackendKind;
    use crate::Renderer;

    #[test]
    fn observatory_serves_endpoints() {
        // Bind on port 0 so the test doesn't collide.
        let port = pick_port();
        let renderer = Arc::new(Renderer::new(BackendKind::Cpu, 16, 16));
        let obs = Observatory::new(renderer, port).unwrap();
        let bound_port = obs.port();
        std::thread::sleep(std::time::Duration::from_millis(100));
        // Health endpoint.
        let resp = std::process::Command::new("curl")
            .args(["-s", &format!("http://127.0.0.1:{bound_port}/health")])
            .output()
            .expect("curl failed");
        assert!(resp.status.success(), "curl must succeed");
        let out = String::from_utf8_lossy(&resp.stdout);
        assert!(out.contains("ok"), "health response: {out}");
        obs.shutdown();
    }

    fn pick_port() -> u16 {
        let l = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        l.local_addr().unwrap().port()
    }
}
