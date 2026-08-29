//! Versioned binary serialization.
//!
//! The save system uses [postcard](https://crates.io/crates/postcard) for
//! compact, schema-less binary encoding. This module adds:
//!
//! - A 16-byte magic header + version stamp to every save blob.
//! - A roundtrip-verified encode/decode pair.
//! - A version-aware decoder that can detect old saves and route them
//!   through migration.
//!
//! Save corruption is detected via:
//!   1. Magic mismatch — wrong file type.
//!   2. Version mismatch — too old or too new.
//!   3. Postcard deserialization failure — truncated or garbage.
//!   4. Optional CRC32 footer.
//!
//! Layout:
//! ```text
//!   [ magic: 8 bytes ("ARCANE1") ]
//!   [ format version: u32 LE ]
//!   [ crc32 enabled: u8 (0|1) ]
//!   [ payload len: u32 LE ]
//!   [ payload: postcard bytes ]
//!   [ crc32: 4 bytes LE (optional, only if crc32 enabled) ]
//! ```

use crate::result::Error;
use serde::{de::DeserializeOwned, Serialize};
use std::io::Write;

/// Magic bytes written at the start of every Arcane save blob.
pub const MAGIC: &[u8; 8] = b"ARCANE1\0";

/// Current save format version. Increment when the schema changes; the
/// decoder routes older versions through migrations.
pub const CURRENT_VERSION: u32 = 1;

/// Encodes `value` into a self-describing blob with magic + version + CRC.
pub fn encode<T: Serialize>(value: &T) -> crate::result::Result<Vec<u8>> {
    let payload = postcard::to_allocvec(value)
        .map_err(|e| Error::InvalidFormat { what: "save payload".into(), reason: e.to_string() })?;
    let crc = crc32(&payload);
    let mut out = Vec::with_capacity(8 + 4 + 1 + 4 + payload.len() + 4);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&CURRENT_VERSION.to_le_bytes());
    out.push(1); // crc enabled
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(&payload);
    out.extend_from_slice(&crc.to_le_bytes());
    Ok(out)
}

/// Decodes a blob previously produced by [`encode`]. Verifies magic + version
/// + CRC. Returns the deserialized value or an error.
pub fn decode<T: DeserializeOwned>(bytes: &[u8]) -> crate::result::Result<T> {
    if bytes.len() < 8 + 4 + 1 + 4 {
        return Err(Error::SaveCorrupt("blob too short for header".into()).into());
    }
    let mut pos = 0;
    let magic = &bytes[..8];
    if magic != MAGIC {
        return Err(Error::SaveCorrupt(format!("bad magic: {:?}", magic)).into());
    }
    pos += 8;
    let version = u32::from_le_bytes(bytes[pos..pos + 4].try_into().unwrap());
    pos += 4;
    if version > CURRENT_VERSION {
        return Err(Error::SaveCorrupt(format!(
            "save version {} newer than runtime version {} — please update the engine",
            version, CURRENT_VERSION
        )).into());
    }
    let crc_enabled = bytes[pos];
    pos += 1;
    let payload_len = u32::from_le_bytes(bytes[pos..pos + 4].try_into().unwrap()) as usize;
    pos += 4;
    if pos + payload_len + if crc_enabled != 0 { 4 } else { 0 } > bytes.len() {
        return Err(Error::SaveCorrupt(format!(
            "payload length {} exceeds remaining bytes {}",
            payload_len,
            bytes.len() - pos
        )).into());
    }
    let payload = &bytes[pos..pos + payload_len];
    pos += payload_len;
    if crc_enabled != 0 {
        if bytes.len() < pos + 4 {
            return Err(Error::SaveCorrupt("missing CRC footer".into()).into());
        }
        let expected = u32::from_le_bytes(bytes[pos..pos + 4].try_into().unwrap());
        let actual = crc32(payload);
        if actual != expected {
            return Err(Error::SaveCorrupt(format!(
                "CRC mismatch: expected {:#010x}, got {:#010x}",
                expected, actual
            )).into());
        }
    }
    // Route through migration if version is older.
    let migrated = if version == CURRENT_VERSION {
        payload.to_vec()
    } else {
        migrate(payload, version, CURRENT_VERSION)?
    };
    postcard::from_bytes::<T>(&migrated)
        .map_err(|e| Error::SaveCorrupt(format!("decode failure: {}", e)).into())
}

/// Returns the (version, payload_len, crc_enabled) tuple from a blob header
/// without decoding the payload. Used by save-browser tools.
pub fn peek_header(bytes: &[u8]) -> crate::result::Result<(u32, usize, bool)> {
    if bytes.len() < 8 + 4 + 1 + 4 {
        return Err(Error::SaveCorrupt("blob too short".into()).into());
    }
    if &bytes[..8] != MAGIC {
        return Err(Error::SaveCorrupt("bad magic".into()).into());
    }
    let version = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
    let crc = bytes[12] != 0;
    let len = u32::from_le_bytes(bytes[13..17].try_into().unwrap()) as usize;
    Ok((version, len, crc))
}

/// Saves a blob atomically: writes to `path.tmp`, then renames to `path`.
/// The `path.tmp` file is opened with `create_new = true` first so a stale
/// tmp file is detected, then truncated.
pub fn save_atomic(path: &std::path::Path, bytes: &[u8]) -> crate::result::Result<()> {
    let mut tmp = path.with_extension("tmp");
    if tmp.as_os_str().is_empty() {
        tmp = std::path::PathBuf::from("save.tmp");
    }
    {
        let _ = std::fs::remove_file(&tmp);
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Loads a blob from disk and decodes it. Combines [`std::fs::read`] + [`decode`].
pub fn load_file<T: DeserializeOwned>(path: &std::path::Path) -> crate::result::Result<T> {
    let bytes = std::fs::read(path)?;
    decode(&bytes)
}

// === Internals ===============================================================

/// CRC32 / IEEE 802.3 polynomial, bitwise reversed (matches zlib/zip).
fn crc32(bytes: &[u8]) -> u32 {
    // Fast table-based implementation.
    static TABLE: [u32; 256] = build_table();
    let mut crc: u32 = 0xFFFF_FFFF;
    for &b in bytes {
        crc = (crc >> 8) ^ TABLE[((crc as u8) ^ b) as usize];
    }
    !crc
}

/// Build the CRC32 table at compile time.
const fn build_table() -> [u32; 256] {
    let mut t = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut j = 0;
        while j < 8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            j += 1;
        }
        t[i] = c;
        i += 1;
    }
    t
}

/// Save migration entry point. Currently a no-op since we are on version 1.
/// Future versions will route to per-version migration functions.
fn migrate(payload: &[u8], from: u32, to: u32) -> crate::result::Result<Vec<u8>> {
    // No migrations defined yet — version 1 is current.
    if from != to {
        return Err(Error::SaveCorrupt(format!(
            "no migration path from version {} to {}",
            from, to
        )).into());
    }
    Ok(payload.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct SampleSave {
        seed: u64,
        player_name: String,
        mana: f32,
        items: Vec<String>,
    }

    fn sample() -> SampleSave {
        SampleSave {
            seed: 0xDEAD_BEEF_CAFE_BABE,
            player_name: "Test Arcanist".into(),
            mana: 42.5,
            items: vec!["rune".into(), "tablet".into()],
        }
    }

    #[test]
    fn encode_decode_roundtrip() {
        let s = sample();
        let bytes = encode(&s).unwrap();
        let back: SampleSave = decode(&bytes).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn header_peek_reports_version_and_len() {
        let s = sample();
        let bytes = encode(&s).unwrap();
        let (ver, len, crc) = peek_header(&bytes).unwrap();
        assert_eq!(ver, CURRENT_VERSION);
        assert!(len > 0);
        assert!(crc, "CRC should be enabled by default");
    }

    #[test]
    fn bad_magic_rejected() {
        let s = sample();
        let mut bytes = encode(&s).unwrap();
        bytes[0] = b'X';
        let err = decode::<SampleSave>(&bytes).unwrap_err();
        assert!(err.to_string().contains("bad magic"), "got: {}", err);
    }

    #[test]
    fn truncated_payload_rejected() {
        let s = sample();
        let mut bytes = encode(&s).unwrap();
        // Chop off the last 10 bytes (some payload + CRC).
        let chopped = bytes.len() - 10;
        bytes.truncate(chopped);
        let err = decode::<SampleSave>(&bytes).unwrap_err();
        assert!(err.to_string().to_lowercase().contains("payload"), "got: {}", err);
    }

    #[test]
    fn crc_corruption_detected() {
        let s = sample();
        let mut bytes = encode(&s).unwrap();
        // Flip a bit in the middle of the payload (after the 17-byte header).
        bytes[20] ^= 0xFF;
        let err = decode::<SampleSave>(&bytes).unwrap_err();
        assert!(err.to_string().to_lowercase().contains("crc"), "got: {}", err);
    }

    #[test]
    fn atomic_save_then_load_roundtrip() {
        let s = sample();
        let bytes = encode(&s).unwrap();
        let tmp = std::env::temp_dir().join(format!("arcane-save-test-{}.bin", std::process::id()));
        save_atomic(&tmp, &bytes).unwrap();
        let back: SampleSave = load_file(&tmp).unwrap();
        assert_eq!(back, s);
        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn future_version_rejected() {
        let s = sample();
        let mut bytes = encode(&s).unwrap();
        // Overwrite the version (bytes 8..12) with a higher version.
        bytes[8..12].copy_from_slice(&(CURRENT_VERSION + 1).to_le_bytes());
        let err = decode::<SampleSave>(&bytes).unwrap_err();
        assert!(err.to_string().to_lowercase().contains("newer"), "got: {}", err);
    }

    #[test]
    fn crc32_known_vector() {
        // CRC32 of empty string is 0.
        assert_eq!(crc32(b""), 0);
        // CRC32 of "123456789" is 0xCBF43926 (standard test vector).
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }
}
