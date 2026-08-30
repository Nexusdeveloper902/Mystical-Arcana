//! Render extraction — pulls the live game state into a RenderScene.
//!
//! The session already builds its `RenderScene` via the scenario builder;
//! this module exists as a future integration point for extracting the
//! session's full state (ECS entities, world chunks, etc.) into a renderable
//! representation when the engine's ECS and world crates are fully wired.

use arcane_render::scene::RenderScene;

use crate::session::GameSession;

/// Extract a renderable scene from the session's current state.
pub fn extract_from_session(session: &GameSession) -> RenderScene {
    session.render_scene()
}
