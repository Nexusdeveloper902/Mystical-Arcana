//! Error and Result types for the Arcane engine.
//!
//! The engine deliberately uses `anyhow::Error` as its top-level error type
//! for ergonomics, but `Result<T>` here provides a stable alias and a
//! typed `Error` for callers that want to inspect failures.

/// The Arcane engine's `Result` alias. Uses `anyhow::Error` internally
/// so that any error implementing `std::error::Error + Send + Sync + 'static`
/// can be converted into it via `?`.
pub type Result<T> = anyhow::Result<T>;

/// A typed error for callers that prefer explicit error matching.
///
/// This is a small, hand-rolled enum that covers common engine-level failure
/// modes. Anything not covered here is bubbled up via `anyhow`'s opaque
/// `Error` type.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A resource was requested but not found.
    #[error("resource not found: {0}")]
    NotFound(String),

    /// A resource was found but in the wrong format / version.
    #[error("invalid format for {what}: {reason}")]
    InvalidFormat { what: String, reason: String },

    /// An asset failed validation.
    #[error("asset validation failed: {0}")]
    AssetValidation(String),

    /// The save file is corrupted or in an unsupported version.
    #[error("save corruption: {0}")]
    SaveCorrupt(String),

    /// A deterministic procedural operation produced a non-deterministic result.
    #[error("procedural determinism violation: {0}")]
    DeterminismViolation(String),

    /// An internal invariant was violated.
    #[error("invariant violated: {0}")]
    Invariant(String),

    /// A subsystem is not yet implemented (used during phased development).
    #[error("not yet implemented: {0}")]
    NotImplemented(&'static str),

    /// A Vulkan operation returned a non-success result code.
    #[error("vulkan error: {0}")]
    Vulkan(String),

    /// A generic I/O error.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_display_nicely() {
        let e = Error::NotFound("chunk(0,0,0)".into());
        assert_eq!(e.to_string(), "resource not found: chunk(0,0,0)");

        let e = Error::InvalidFormat {
            what: "save".into(),
            reason: "bad magic".into(),
        };
        assert!(e.to_string().contains("invalid format for save"));
    }

    #[test]
    fn anyhow_alias_works() {
        fn fallible(x: i32) -> Result<i32> {
            if x < 0 {
                Err(Error::Invariant("negative".into()).into())
            } else {
                Ok(x * 2)
            }
        }

        assert_eq!(fallible(5).unwrap(), 10);
        assert!(fallible(-1).is_err());
    }
}
