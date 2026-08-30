//! arcane_core — engine-wide traits and shared types.
//!
//! Currently this is just a placeholder module so the workspace compiles.
//! Real engine systems will live here: timeline, frame context, host trait,
//! engine config, etc.

pub mod host;
pub mod time;

pub use host::*;
pub use time::*;
