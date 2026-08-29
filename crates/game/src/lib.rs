//! Mystical Arcana — game library root. Implementation lands in subsequent commits.
#![warn(missing_docs)]

pub const GAME_NAME: &str = "Mystical Arcana";
pub const ENGINE_NAME: &str = "Arcane";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Headless gameplay loop harness. Drives the simulation forward without a window.
/// Used by `Tests/smoke_headless.rs` and CI.
pub mod headless;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn game_constants() {
        assert_eq!(GAME_NAME, "Mystical Arcana");
        assert_eq!(ENGINE_NAME, "Arcane");
    }
}
