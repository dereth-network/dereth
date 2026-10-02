//! The 24-bit RGB records (`0x06`): the interface art of the pre-Throne-of-Destiny portal.
//!
//! A record is its own id, a width and a height, then `width * height` pixels of three bytes in
//! red, green, blue order, top row first, and nothing after them. There is no alpha: a colour key,
//! where one applies, is a property of the draw, not of the record.

/// Decoded pixels, top row first, four bytes per pixel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Decode one RGB record to opaque RGBA. `expected` is the id the record must echo.
///
/// # Errors
///
/// A short header, an id that is not an RGB record's (or not `expected`), a zero dimension, or
/// pixels that do not fill the payload exactly.
pub fn decode_rgb(payload: &[u8], expected: Option<u32>) -> Result<Image, String> {
    if payload.len() < 12 {
        return Err("short RGB header".into());
    }
    let word =
        |i: usize| u32::from_le_bytes([payload[i], payload[i + 1], payload[i + 2], payload[i + 3]]);
    let (id, width, height) = (word(0), word(4), word(8));
    if id >> 24 != 6 || expected.is_some_and(|e| e != id) {
        return Err("RGB record id mismatch".into());
    }
    let pixels = u64::from(width) * u64::from(height);
    if width == 0 || height == 0 || payload.len() as u64 != 12 + pixels * 3 {
        return Err("RGB dimensions do not consume the complete payload".into());
    }
    let mut rgba = Vec::with_capacity(payload.len() / 3 * 4);
    for px in payload[12..].as_chunks::<3>().0 {
        rgba.extend_from_slice(&[px[0], px[1], px[2], 0xFF]);
    }
    Ok(Image {
        width,
        height,
        rgba,
    })
}
