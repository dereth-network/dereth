//! A place written as the client writes it for a player: a cell's map coordinates.

use auberean_primitives::land;

/// A land cell's map coordinates as the client writes them (`%.1f%s, %.1f%s`): north or south to
/// a tenth, then east or west, the letter left out on the middle line; `<error>` for a cell that
/// is not a land cell of the world (`land`: its width and length in land cells).
// D2-13206
#[must_use]
pub fn real_map_coords(cell: u32, land: (i32, i32)) -> String {
    let Some((east, north)) = land::map_coords(cell, land) else {
        return "<error>".to_owned();
    };
    let letter = |v: f32, negative: &'static str, positive: &'static str| {
        if v < 0.0 {
            negative
        } else if v > 0.0 {
            positive
        } else {
            ""
        }
    };
    format!(
        "{}{}, {}{}",
        crate::crt::fixed(f64::from(north.abs()), 1),
        letter(north, "S", "N"),
        crate::crt::fixed(f64::from(east.abs()), 1),
        letter(east, "W", "E")
    )
}
