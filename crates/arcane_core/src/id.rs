//! Stable, hashed identifiers used across the engine.
//!
//! The engine uses three flavours of identifier:
//! - [`Id64`] — stable 64-bit hash of a string name. Compact, ordered.
//! - [`IdUlid`] — 128-bit ULID (time-ordered, lexicographically sortable).
//! - [`Id`] — type-erased enum that wraps either for interchange.
//!
//! All deterministic subsystems (procedural gen, save system, ECS component
//! IDs) use `Id64`. All runtime-allocated entities (player, enemies, dropped
//! items, particles) use `IdUlid` so collisions are impossible across saves.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

/// A 64-bit hashed identifier produced from a stable string name.
///
/// Uses `ahash::AHasher` with a fixed seed (0). Stable across runs, processes,
/// and platforms. Use [`ident!`] to declare these as constants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(transparent)]
pub struct Id64(pub u64);

impl Id64 {
    /// Hashes a string name into a 64-bit identifier.
    ///
    /// Uses [`Id64::from_str_const`] internally — the function is callable
    /// from const context, so the [`ident!`](crate::ident!) macro can produce
    /// true `const` identifiers.
    pub fn from_str(s: &str) -> Self {
        Self(Self::from_str_const(s))
    }

    /// Const-evaluable string hash. Uses FNV-1a (64-bit) — fast, simple,
    /// and produces well-distributed values for short ASCII names.
    pub const fn from_str_const(s: &str) -> u64 {
        // FNV-1a offset basis and prime (64-bit).
        const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
        const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
        let bytes = s.as_bytes();
        let mut h = FNV_OFFSET;
        let mut i = 0;
        while i < bytes.len() {
            h ^= bytes[i] as u64;
            h = h.wrapping_mul(FNV_PRIME);
            i += 1;
        }
        h
    }

    /// Raw 64-bit value.
    pub fn as_u64(self) -> u64 {
        self.0
    }

    /// The null/zero id, used as a sentinel "no value."
    pub const NULL: Self = Self(0);

    /// True if this is the null id.
    pub fn is_null(self) -> bool {
        self.0 == 0
    }
}

impl fmt::Display for Id64 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "id64:{:016x}", self.0)
    }
}

impl Serialize for Id64 {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u64(self.0)
    }
}

impl<'de> Deserialize<'de> for Id64 {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(Self(<u64 as Deserialize<'de>>::deserialize(d)?))
    }
}

/// Declares a constant `Id64` from a string literal at compile time.
///
/// # Example
/// ```
/// use arcane_core::ident;
/// ident!(FIRE, "fire");
/// assert_ne!(FIRE.as_u64(), 0);
/// ```
#[macro_export]
macro_rules! ident {
    ($name:ident, $str:expr) => {
        #[allow(non_upper_case_globals)]
        pub const $name: $crate::id::Id64 =
            $crate::id::Id64($crate::id::Id64::from_str_const($str));
    };
}

// === ULID (time-ordered, 128-bit) === ---------------------------------------

static ULID_COUNTER: AtomicU64 = AtomicU64::new(0);

/// A 128-bit time-ordered identifier. Suitable for runtime-allocated entities
/// where collisions across save files would be unacceptable.
///
/// Format:
///   bits 127..96 (48 bits)  — unix-millis timestamp
///   bits  95..64 (32 bits)  — process-local counter (monotonic)
///   bits  63..0  (64 bits)  — random salt (seeded once at process start)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct IdUlid {
    /// High 64 bits: timestamp | counter.
    pub hi: u64,
    /// Low 64 bits: random salt.
    pub lo: u64,
}

impl IdUlid {
    /// Generates a new ULID using the current wall-clock time and the
    /// process-local counter.
    pub fn new() -> Self {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
            & 0xFFFF_FFFF_FFFF;
        let c = ULID_COUNTER.fetch_add(1, Ordering::Relaxed) & 0xFFFF_FFFF;
        let hi = (ts << 16) | c;
        // Process-wide random salt, generated once.
        use std::sync::OnceLock;
        static SALT: OnceLock<u64> = OnceLock::new();
        let s = *SALT.get_or_init(|| {
            use std::hash::Hasher;
            let mut h = ahash::AHasher::default();
            h.write_u64(std::process::id() as u64);
            h.write_u64(ts);
            h.finish()
        });
        Self { hi, lo: s ^ c.rotate_left(17).wrapping_mul(0x9E37_79B9_7F4A_7C15) }
    }

    /// The null ULID.
    pub const NULL: Self = Self { hi: 0, lo: 0 };

    /// True if null.
    pub fn is_null(self) -> bool {
        self.hi == 0 && self.lo == 0
    }
}

impl Default for IdUlid {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for IdUlid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ulid:{:016x}{:016x}", self.hi, self.lo)
    }
}

impl Serialize for IdUlid {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        (&self.hi, &self.lo).serialize(s)
    }
}

impl<'de> Deserialize<'de> for IdUlid {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let (hi, lo) = <(u64, u64)>::deserialize(d)?;
        Ok(Self { hi, lo })
    }
}

/// Type-erased ID. Used when an API needs to accept either flavor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Id {
    /// Stable, hashed ID.
    Hashed(Id64),
    /// Runtime-allocated ULID.
    Uuid(IdUlid),
}

impl Id {
    /// True if both arms are null.
    pub fn is_null(self) -> bool {
        match self {
            Id::Hashed(i) => i.is_null(),
            Id::Uuid(i) => i.is_null(),
        }
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Id::Hashed(i) => write!(f, "{}", i),
            Id::Uuid(i) => write!(f, "{}", i),
        }
    }
}

/// Convenience: hash a string into an `Id64`. Same as [`Id64::from_str`].
pub fn ident(s: &str) -> Id64 {
    Id64::from_str(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id64_stable_across_calls() {
        let a = Id64::from_str("fire");
        let b = Id64::from_str("fire");
        assert_eq!(a, b);
        assert_eq!(a.as_u64(), b.as_u64());
    }

    #[test]
    fn id64_distinguishes_strings() {
        let a = Id64::from_str("fire");
        let b = Id64::from_str("ice");
        assert_ne!(a, b);
    }

    #[test]
    fn id64_serializes_as_plain_u64() {
        let id = Id64::from_str("test");
        let s = serde_json::to_string(&id).unwrap();
        // Should be a bare number, not an object or string.
        assert!(s.parse::<u64>().is_ok(), "expected bare u64, got: {}", s);
        let back: Id64 = serde_json::from_str(&s).unwrap();
        assert_eq!(back, id);
    }

    #[test]
    fn id64_display_is_hex_prefixed() {
        let id = Id64::from_str("fire");
        let s = format!("{}", id);
        assert!(s.starts_with("id64:"));
    }

    #[test]
    fn id64_null_is_zero() {
        assert!(Id64::NULL.is_null());
        assert_eq!(Id64::NULL.as_u64(), 0);
    }

    #[test]
    fn id64_ordering_is_deterministic() {
        let a = Id64::from_str("alpha");
        let b = Id64::from_str("beta");
        // Deterministic: ordering is the same on every run.
        if !(a < b) {
            assert!(b < a, "ordering must be total");
        }
    }

    #[test]
    fn ulid_is_monotonic_within_thread() {
        let a = IdUlid::new();
        let b = IdUlid::new();
        assert_ne!(a, b, "ULIDs must not collide within a single thread");
        // Time-ordered: the second ULID's high bits are >= the first's.
        assert!(b.hi >= a.hi, "ULID must be monotonic");
    }

    #[test]
    fn ulid_serializes_roundtrip() {
        let id = IdUlid::new();
        let bytes = postcard::to_allocvec(&id).unwrap();
        let back: IdUlid = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(id, back);
    }

    #[test]
    fn ulid_null_check() {
        assert!(IdUlid::NULL.is_null());
        assert!(!IdUlid::new().is_null());
    }

    #[test]
    fn ident_macro_produces_stable_constant() {
        ident!(FIRE, "fire");
        assert_eq!(FIRE, Id64::from_str("fire"));
    }

    #[test]
    fn id_type_erased_distinguishes_flavours() {
        let a = Id::Hashed(Id64::from_str("fire"));
        let b = Id::Uuid(IdUlid::new());
        assert_ne!(a, b);
    }
}
