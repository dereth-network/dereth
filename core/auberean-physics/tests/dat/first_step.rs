//! The interior cells a server's first step reads, as physics holds them.

use std::fmt::Write;

use auberean_physics::interior::add_block_cells;
use auberean_physics::land::Land;
use auberean_physics::mesh::Trees;

use crate::common::need_world;

/// The 150 blocks whose interior cells the first step reads to stand the indoor camps of the
/// world, in the order it reads them.
const BLOCKS: [u16; 150] = [
    0x00A7, 0x00DE, 0x00EC, 0x01AC, 0x01AF, 0x01BA, 0x01C5, 0x01C6, 0x01C7, 0x01C9, 0x01CA, 0x01CC,
    0x01CE, 0x01CF, 0x01D0, 0x01D1, 0x01D3, 0x01D4, 0x01D6, 0x01D7, 0x01D9, 0x01DA, 0x01DC, 0x01DD,
    0x01DF, 0x01E1, 0x01E2, 0x01E3, 0x01E5, 0x01E6, 0x01E8, 0x01E9, 0x01EB, 0x01EC, 0x01EE, 0x01EF,
    0x01F1, 0x01F2, 0x01F4, 0x01F5, 0x01F7, 0x01F8, 0x01FA, 0x01FB, 0x01FD, 0x0298, 0x02A0, 0x037E,
    0x03BB, 0x03D7, 0x03E7, 0x03E8, 0x03E9, 0x03FB, 0x0489, 0x048A, 0x048B, 0x048C, 0x0497, 0x0499,
    0x049A, 0x049B, 0x049C, 0x049D, 0x049E, 0x049F, 0x04A0, 0x04A1, 0x04A2, 0x04AD, 0x04AF, 0x04B4,
    0x04B9, 0x05E8, 0x05F9, 0x05FC, 0x06F7, 0x06F8, 0x06F9, 0x06FD, 0x07C3, 0x07F1, 0x07F2, 0x07F3,
    0x07F4, 0x07F5, 0x07F6, 0x07F7, 0x07F9, 0x07FA, 0x07FB, 0x08C3, 0x09F2, 0x09F3, 0x09F4, 0x09F7,
    0x09F8, 0x09F9, 0x09FA, 0x0AFC, 0x0BF1, 0x0BF2, 0x0BF4, 0x0BFB, 0x0BFD, 0x0CF7, 0x0CFA, 0x0CFC,
    0x0DFA, 0x3F9E, 0x3F9D, 0x4592, 0x4491, 0x66A9, 0x67AA, 0x6967, 0x6867, 0x6868, 0x7097, 0x76AE,
    0x7CB9, 0x858D, 0x8B91, 0x98A6, 0x97A5, 0x97A6, 0x97A7, 0x98A5, 0x98A7, 0x99A5, 0x99A6, 0x99A7,
    0x9AB5, 0x99B5, 0x9BB5, 0x9C96, 0x9B95, 0x9B96, 0x9B97, 0x9C95, 0x9C97, 0xB880, 0xB87F, 0xB881,
    0xB97F, 0xB980, 0xB981, 0xDEA8, 0xE1A8, 0xE0A7,
];

/// FNV-1a, 64 bits, over everything written to it, and how many bytes that was.
struct Digest(u64, u64);

impl Write for Digest {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        for b in s.bytes() {
            self.0 ^= u64::from(b);
            self.0 = self.0.wrapping_mul(0x0100_0000_01B3);
        }
        self.1 += s.len() as u64;
        Ok(())
    }
}

/// Every interior cell of the 150 blocks a server's first step reads, read into one land through
/// one set of trees as the server reads them, with the solid parts of what stands in each: their
/// whole collision geometry, every polygon and tree, digests to what it digested when this was
/// recorded. A change to how cells are held that leaves what they hold alone keeps it; a change to
/// what they hold has to say so here.
///
/// Behaviour: motion.interior.the-first-steps-cells-build-to-the-recorded-geometry
#[test]
fn the_first_steps_cells_build_to_the_recorded_geometry() {
    let w = need_world!();
    let land = Land::new();
    let mut trees = Trees::default();
    for b in BLOCKS {
        let [x, y] = b.to_be_bytes();
        add_block_cells(&land, &w.records, &mut trees, x, y);
    }
    let mut d = Digest(0xCBF2_9CE4_8422_2325, 0);
    let mut cells = 0;
    for b in BLOCKS {
        for id in land.interior_cells_of(u32::from(b) << 16) {
            cells += 1;
            let geometry = land.cell(id).expect("the cell");
            write!(d, "{:?}", *geometry).unwrap();
            if let Some(solids) = land.cell_solids(id) {
                write!(d, "{:?}", *solids).unwrap();
            }
        }
    }
    assert_eq!(
        format!("{cells} cells, {} bytes, digest {:016X}", d.1, d.0),
        "17718 cells, 7064085409 bytes, digest 4CB785F203415827"
    );
}
