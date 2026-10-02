//! Indexed textures, palettes and palette sets: what the character-creation face strips are drawn
//! from.
//!
//! - An indexed texture (`0x05`, pixel type 2) is its id, the type, a width and a height, one
//!   palette index per pixel, then the id of its default palette, padded to four bytes.
//! - A palette (`0x04`) is its id, a count of 256 and 256 words of `0x00RRGGBB`.
//! - A palette set (`0x0F`) is its id, a count and that many palette ids.
//!
//! The eye strip is stored as one half of a face and drawn mirrored: each row followed by the same
//! row reversed, which doubles its width.

use crate::reader::Cursor;

/// One indexed texture: row-major palette indices and the id of its default palette.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IndexedTexture {
    pub width: u32,
    pub height: u32,
    pub indices: Vec<u8>,
    pub palette: u32,
}

fn words<const N: usize>(payload: &[u8], what: &str) -> Result<[u32; N], String> {
    if payload.len() < N * 4 {
        return Err(format!("{what} header is truncated"));
    }
    let mut out = [0u32; N];
    for (i, w) in out.iter_mut().enumerate() {
        let p = i * 4;
        *w = u32::from_le_bytes([payload[p], payload[p + 1], payload[p + 2], payload[p + 3]]);
    }
    Ok(out)
}

/// Decode an indexed texture.
///
/// # Errors
///
/// Not an indexed (type 2) texture record, a zero dimension, or a payload the pixels, the palette
/// id and the padding do not consume exactly.
pub fn indexed(payload: &[u8]) -> Result<IndexedTexture, String> {
    let [id, kind, width, height] = words::<4>(payload, "indexed texture")?;
    if id >> 24 != 5 || kind != 2 || width == 0 || height == 0 {
        return Err("face is not an indexed texture".into());
    }
    let end = 16 + u64::from(width) * u64::from(height);
    if payload.len() as u64 != (end + 4 + 3) & !3 {
        return Err("indexed texture does not consume payload".into());
    }
    let end = usize::try_from(end).map_err(|e| e.to_string())?;
    let palette = u32::from_le_bytes([
        payload[end],
        payload[end + 1],
        payload[end + 2],
        payload[end + 3],
    ]);
    Ok(IndexedTexture {
        width,
        height,
        indices: payload[16..end].to_vec(),
        palette,
    })
}

/// Decode a palette to 256 opaque RGBA colours.
///
/// # Errors
///
/// Not a palette record, not 256 entries, or entries that do not fill the payload exactly.
pub fn palette(payload: &[u8]) -> Result<Vec<[u8; 4]>, String> {
    let [id, count] = words::<2>(payload, "palette")?;
    if id >> 24 != 4 || count != 256 || payload.len() != 8 + 256 * 4 {
        return Err("palette dimensions do not consume payload".into());
    }
    Ok(payload[8..]
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| [c[2], c[1], c[0], 0xFF])
        .collect())
}

/// Decode a palette set to its palette ids.
///
/// # Errors
///
/// Not a palette-set record, or ids that do not fill the payload exactly.
pub fn palette_set(payload: &[u8]) -> Result<Vec<u32>, String> {
    let [id, count] = words::<2>(payload, "palette set")?;
    if id >> 24 != 0x0F || payload.len() as u64 != 8 + u64::from(count) * 4 {
        return Err("palette set does not consume payload".into());
    }
    Ok(payload[8..]
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| u32::from_le_bytes(*c))
        .collect())
}

/// The texture an object description swaps in first: the new texture of its first texture swap,
/// as a `0x05` id, or 0 when it swaps no texture.
///
/// # Errors
///
/// Not a version `0x11` description, or one that ends inside the fields read.
pub fn first_texture(objdesc: &[u8]) -> Result<u32, String> {
    let mut r = Cursor::new(objdesc);
    let head = r.take(4)?;
    let (version, palettes, textures) = (head[0], head[1], head[2]);
    if version != 0x11 {
        return Err("ObjDesc version".into());
    }
    if palettes != 0 {
        r.packed_id()?;
    }
    for _ in 0..palettes {
        r.packed_id()?;
        r.take(2)?;
    }
    if textures == 0 {
        return Ok(0);
    }
    r.take(1)?;
    r.packed_id()?;
    Ok(0x0500_0000 | r.packed_id()?)
}

/// The texture drawn mirrored: each row followed by itself reversed, so twice as wide.
#[must_use]
pub fn mirrored(t: &IndexedTexture) -> IndexedTexture {
    let width = t.width as usize;
    let mut indices = Vec::with_capacity(t.indices.len() * 2);
    if width > 0 {
        for row in t.indices.chunks(width) {
            indices.extend_from_slice(row);
            indices.extend(row.iter().rev());
        }
    }
    IndexedTexture {
        width: t.width * 2,
        height: t.height,
        indices,
        palette: t.palette,
    }
}
