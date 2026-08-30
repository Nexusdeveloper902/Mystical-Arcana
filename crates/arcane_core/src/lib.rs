//! Arcane engine core — foundational systems shared by every engine crate.
//!
//! Modules:
//! - [`log`]: thin logging façade wired to `log` crate
//! - [`result`]: `Error`/`Result` types for the engine
//! - [`assert`]: debug assertions with structured failure metadata
//! - [`time`]: high-resolution clock + frame timing
//! - [`id`]: stable, hashed identifiers used everywhere in the engine
//! - [`pool`]: object pool + slab allocator (no per-frame allocations)
//! - [`handle`]: generational handle table for resource references
//! - [`thread`]: thread pool + job system
//! - [`profiling`]: scoped CPU profiler (lock-free, headless-testable)
//! - [`serialize`]: versioned binary serializer (postcard + magic header)

#![forbid(unsafe_op_in_unsafe_fn)]
#![warn(missing_docs)]

pub mod assert;
pub mod handle;
pub mod id;
pub mod log;
pub mod pool;
pub mod profiling;
pub mod result;
pub mod serialize;
pub mod thread;
pub mod time;

// Re-export the assertion and profiling macros at crate root for ergonomic
// `use arcane_core::arc_assert;` access. These are exported via
// `#[macro_export]` so they live at crate root — no `pub use` needed.

pub use handle::{Handle, HandleTable};
pub use id::{ident, Id, Id64, IdUlid};
pub use log::{init_logger, LogLevel};
pub use pool::ObjectPool;
pub use profiling::{FrameStats, Profiler, Scope};
pub use result::{Error, Result};
pub use time::{FrameClock, Stopwatch};
pub use thread::{BackgroundWorker, JobHandle, ThreadPool};

/// Re-export of `SmallVec` for ergonomics — most engine hot-path small
/// collections use this to avoid heap allocations.
pub use smallvec::SmallVec;

/// Re-export of `parking_lot` mutex flavours — the engine standardizes on
/// `parking_lot` over `std::sync` for predictable non-poisoning behaviour.
pub use parking_lot::{Mutex, RwLock};

/// Engine-wide version stamp (matches `CARGO_PKG_VERSION`).
pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");
