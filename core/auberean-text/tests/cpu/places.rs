//! A place written as the client writes it: map coordinates, and the C library's rounding.

use auberean_text::crt::fixed;
use auberean_text::place::real_map_coords;

/// A land cell's map coordinates are written north or south first, then east or west, each to a
/// tenth with the C library's rounding (a half rounds up: 6.25 is `6.3`), the letter left out on
/// the middle line; a cell that is not a land cell of the world is `<error>`. Each case is the
/// September 2013 client's own answer, run in an emulator, on a world 255 landblocks a side
/// (its every column and row agree) and one of 128, whose halves land exactly on a twentieth; the
/// C library's own `%.1f`, `%.0f` and `%.6f`, run likewise.
///
/// Behaviour: text.place.map-coordinates-are-written-as-the-client-writes-them
#[test]
fn map_coordinates_are_written_as_the_client_writes_them() {
    let world = |blocks: i32| (blocks * 8, blocks * 8);
    for (blocks, cell, want) in [
        (255, 0x0000_0001, "100.0S, 100.0W"),
        (255, 0x0000_0009, "100.0S, 99.9W"),
        (255, 0x7F00_0021, "100.0S, 0.0"),
        (255, 0x7F00_0029, "100.0S, 0.1E"),
        (255, 0x7F7F_0025, "0.0, 0.0"),
        (255, 0xFEFE_0040, "99.9N, 99.9E"),
        (255, 0x0157_002E, "31.3S, 98.7W"),
        (255, 0x00FE_0008, "99.9N, 100.0W"),
        (128, 0x3C00_0001, "100.0S, 6.3W"),
        (128, 0x4400_0001, "100.0S, 6.3E"),
        (128, 0x4C00_0001, "100.0S, 18.8E"),
        (128, 0x5400_0001, "100.0S, 31.3E"),
        (255, 0x1234_0041, "<error>"),
        (255, 0x1234_0100, "<error>"),
        (255, 0x1234_FFFF, "<error>"),
        (255, 0xFF00_0001, "<error>"),
        (255, 0x00FF_0001, "<error>"),
    ] {
        assert_eq!(real_map_coords(cell, world(blocks)), want, "{cell:08X}");
    }
    // A world whose size is not known yet has no land cells.
    assert_eq!(real_map_coords(0x0000_0001, (0, 0)), "<error>");
    assert_eq!(fixed(0.05, 1), "0.1");
    assert_eq!(fixed(0.25, 1), "0.3");
    assert_eq!(fixed(99.95, 1), "100.0");
    assert_eq!(fixed(0.004, 1), "0.0");
    assert_eq!(fixed(-1.5, 0), "-2");
    assert_eq!(fixed(-0.004, 1), "-0.0");
    assert_eq!(fixed(-0.0, 6), "0.000000");
    assert_eq!(fixed(1e20, 6), "100000000000000000000.000000");
    // Its seventeen digits end in a 5, though it lies below the half.
    assert_eq!(fixed(1_234.567_890_5, 6), "1234.567891");
}
