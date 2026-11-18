//! How the documents write positions and names.

use crate::content::Spot;

/// `cell`, or for a bare landblock (cell index 0) the land cell that `x`, `y` fall in: cells are
/// 24 units square, eight to a side, numbered from 1 by column then row.
#[must_use]
pub fn land_cell(cell: u32, x: f32, y: f32) -> u32 {
    if cell & 0xFFFF != 0 {
        return cell;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let axis = |v: f32| (f64::from(v) / 24.0).floor().clamp(0.0, 7.0) as u32;
    (cell & 0xFFFF_0000) | (axis(x) * 8 + axis(y) + 1)
}

/// `0x<cell, 8 hex digits> <x> <y> <z>`, the coordinates to three decimals.
#[must_use]
pub fn position(at: &Spot) -> String {
    let [x, y, z] = at.origin;
    format!("0x{:08X} {x:.3} {y:.3} {z:.3}", at.cell)
}

/// A text field: tab, line feed and carriage return become spaces; a backslash is doubled.
#[must_use]
pub fn field(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\t' | '\n' | '\r' => out.push(' '),
            '\\' => out.push_str("\\\\"),
            c => out.push(c),
        }
    }
    out
}
