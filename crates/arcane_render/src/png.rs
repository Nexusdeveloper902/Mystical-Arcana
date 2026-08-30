//! PNG encoding utilities (wraps `image` crate).

use crate::prereqs::{RenderError, RenderResult};

/// Encode RGBA bytes as PNG.
pub fn encode_rgba(width: u32, height: u32, bytes: &[u8]) -> RenderResult<Vec<u8>> {
    let expected = (width as usize) * (height as usize) * 4;
    if bytes.len() < expected {
        return Err(RenderError::Png(format!(
            "png: buffer too small ({} < {})",
            bytes.len(),
            expected
        )));
    }
    let mut out = std::io::Cursor::new(Vec::with_capacity(bytes.len() / 4 + 1024));
    let encoder = image::codecs::png::PngEncoder::new_with_quality(
        &mut out,
        image::codecs::png::CompressionType::Fast,
        image::codecs::png::FilterType::Up,
    );
    image::ImageEncoder::write_image(
        encoder,
        bytes,
        width,
        height,
        image::ExtendedColorType::Rgba8,
    )
    .map_err(|e| RenderError::Png(format!("png encode: {e}")))?;
    Ok(out.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_and_size() {
        let bytes = vec![255u8; 4 * 4 * 4]; // 4x4 white
        let png = encode_rgba(4, 4, &bytes).unwrap();
        assert!(png.len() > 60);
        assert!(&png[..8] == b"\x89PNG\r\n\x1a\n");
    }
}
