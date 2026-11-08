//! Where a cell lies on the land: its land coordinates and its map coordinates.
//!
//! The land is a grid of land cells, eight across and eight up in each landblock: a world `width`
//! land cells across (east) and `length` up (north), as the region gives its size in landblocks.
//! A land cell's coordinates count land cells from the world's south-west corner. Its map
//! coordinates say how far east and north of the world's middle it lies, as a hundredth of half
//! the world's width or length: -100 at the west or south edge, 100 at the east or north edge.
//!
//! The land's size is given as `(width, length)` in land cells; a world whose size is not known
//! yet is `(0, 0)`, where no cell is valid.

/// Land cells along each side of a landblock.
pub const CELLS_PER_BLOCK_SIDE: u32 = 8;

/// The low words a cell id's kind is told by: the land cells, the interior cells, and the cell
/// that stands for the whole landblock.
const FIRST_LAND_CELL: u32 = 1;
const LAST_LAND_CELL: u32 = CELLS_PER_BLOCK_SIDE * CELLS_PER_BLOCK_SIDE;
pub const FIRST_INTERIOR_CELL: u32 = 0x0100;
const LAST_INTERIOR_CELL: u32 = 0xFFFD;
const BLOCK_CELL: u32 = 0xFFFF;

/// The column and row of a cell's landblock, in land cells.
#[allow(clippy::cast_possible_wrap)]
const fn block_corner(cell: u32) -> (i32, i32) {
    (
        ((cell >> 24) * CELLS_PER_BLOCK_SIDE) as i32,
        (((cell >> 16) & 0xFF) * CELLS_PER_BLOCK_SIDE) as i32,
    )
}

/// Whether a cell id names a cell of the world: a land cell, an interior cell or a whole
/// landblock, in a landblock on the land.
// D2-02541
#[must_use]
pub fn is_valid_cell(cell: u32, (width, length): (i32, i32)) -> bool {
    let low = cell & 0xFFFF;
    let kind = (FIRST_LAND_CELL..=LAST_LAND_CELL).contains(&low)
        || (FIRST_INTERIOR_CELL..=LAST_INTERIOR_CELL).contains(&low)
        || low == BLOCK_CELL;
    let (x, y) = block_corner(cell);
    kind && x < width && y < length
}

/// A land cell's land coordinates (across, up); `None` for a cell that is not a land cell of
/// the world.
// D2-02556
#[must_use]
pub fn land_coords(cell: u32, land: (i32, i32)) -> Option<(i32, i32)> {
    let low = cell & 0xFFFF;
    if !is_valid_cell(cell, land) || low >= FIRST_INTERIOR_CELL {
        return None;
    }
    let (bx, by) = block_corner(cell);
    #[allow(clippy::cast_possible_wrap)]
    let c = (low - FIRST_LAND_CELL) as i32;
    let x = bx + (c >> 3);
    let y = by + (c & 7);
    (x < land.0 && y < land.1).then_some((x, y))
}

/// The land cell at land coordinates (across, up); 0 off the land.
// D2-02555
#[must_use]
pub fn cell_at(x: i32, y: i32, (width, length): (i32, i32)) -> u32 {
    if x < 0 || y < 0 || x >= width || y >= length {
        return 0;
    }
    #[allow(clippy::cast_sign_loss)]
    let (x, y) = (x as u32, y as u32);
    let low = (y & 7) + FIRST_LAND_CELL + ((x & 7) << 3);
    low | ((x >> 3) << 8 | y >> 3) << 16
}

/// A land cell's map coordinates (east, north); `None` for a cell that is not a land cell of the
/// world. Worked out in double precision, each kept as a float.
// D2-13191
#[must_use]
pub fn map_coords(cell: u32, land: (i32, i32)) -> Option<(f32, f32)> {
    let (x, y) = land_coords(cell, land)?;
    let along = |at: i32, size: i32| {
        let half = f64::from(size) * 0.5;
        #[allow(clippy::cast_possible_truncation)]
        let v = ((f64::from(at) - half) / half * 100.0) as f32;
        v
    };
    Some((along(x, land.0), along(y, land.1)))
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (plain conversions, held to hand-worked values)
    use super::*;

    const LAND: (i32, i32) = (255 * 8, 255 * 8);

    #[test]
    fn a_land_cell_has_coordinates_and_the_cell_at_them_is_itself() {
        assert_eq!(land_coords(0x0000_0001, LAND), Some((0, 0)));
        assert_eq!(land_coords(0x0102_000A, LAND), Some((9, 17)));
        assert_eq!(cell_at(9, 17, LAND), 0x0102_000A);
        assert_eq!(land_coords(0xFEFE_0040, LAND), Some((2039, 2039)));
        // An interior cell, a cell past a landblock's 64, a block beyond the world.
        assert_eq!(land_coords(0x0102_0100, LAND), None);
        assert_eq!(land_coords(0x0102_0041, LAND), None);
        assert_eq!(land_coords(0xFF00_0001, LAND), None);
        assert_eq!(cell_at(2040, 0, LAND), 0);
        assert!(is_valid_cell(0x0102_0100, LAND) && is_valid_cell(0x0102_FFFF, LAND));
        assert!(!is_valid_cell(0x0102_0000, LAND) && !is_valid_cell(0x0102_FFFE, LAND));
        assert!(!is_valid_cell(0x0102_0001, (0, 0)));
    }

    #[test]
    fn map_coordinates_run_from_minus_a_hundred_to_a_hundred() {
        assert_eq!(map_coords(0x0000_0001, LAND), Some((-100.0, -100.0)));
        let (east, north) = map_coords(0x7F7F_0025, LAND).unwrap_or_default();
        assert_eq!((east, north), (0.0, 0.0));
        assert_eq!(map_coords(0x0102_0100, LAND), None);
    }
}
