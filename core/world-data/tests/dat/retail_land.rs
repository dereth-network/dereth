//! Asheron's Call's own land, as its 9 × 9 landblock records give it, read exactly as it always has
//! been: every record's heights, the terrain mesh at each level of detail, and the collision the
//! client stands on.
//!
//! Behaviour: none (a guarantee of this codebase's own land format: a finer grid's additions never
//! reach a 9 × 9 record)

use std::fmt::Write;

use dereth_assets::world::CellLandblock;
use dereth_assets::Decode;
use dereth_dat::{DbType, RetailDatStore};
use dereth_primitives::land_grid::LandGrid;
use dereth_primitives::{DataId, LandblockId};
use dereth_terrain::land::mesh::{generate_landblock_with_table, height_table, Direction};

/// FNV-1a, 64 bits, over everything written to it.
struct Digest(u64);

impl Write for Digest {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        for b in s.bytes() {
            self.0 ^= u64::from(b);
            self.0 = self.0.wrapping_mul(0x0100_0000_01B3);
        }
        Ok(())
    }
}

/// What a cell file's land read to: how many landblocks, vertices, mesh vertices (over the four
/// levels of detail) and collision polygons, and the digest of every height, mesh vertex and plane
/// and collision vertex and plane, in that order.
#[derive(Debug, PartialEq, Eq)]
struct Read {
    blocks: usize,
    vertices: usize,
    mesh_vertices: usize,
    polygons: usize,
    digest: u64,
}

/// Read every landblock of `store`'s cell file. Each is a 9 × 9 record with no finer grid; a
/// record the files' era has an encoder for writes back to its own bytes (`encodes`); every
/// height, every mesh vertex at each level of detail and every collision vertex stands at
/// exactly its height-table entry.
fn read_all(store: &RetailDatStore, encodes: bool) -> Read {
    let region_id = DataId(0x1300_0000);
    let rb = store
        .read_typed(DbType::Region, region_id)
        .expect("the region");
    let region = dereth_assets::Region::decode_payload_in(store.era_of(region_id), region_id, &rb)
        .expect("the region reads");
    let table = height_table(&region);
    let mut d = Digest(0xCBF2_9CE4_8422_2325);
    let mut read = Read {
        blocks: 0,
        vertices: 0,
        mesh_vertices: 0,
        polygons: 0,
        digest: 0,
    };
    for bx in 0..255u16 {
        for by in 0..255u16 {
            let id = DataId((u32::from(bx) << 24) | (u32::from(by) << 16) | 0xFFFF);
            let Ok(bytes) = store.read_typed(DbType::LandBlock, id) else {
                continue;
            };
            let lb = CellLandblock::decode_payload_in(store.era_of(id), id, &bytes)
                .unwrap_or_else(|e| panic!("{id:?}: {e:?}"));
            assert!(lb.dense.is_none(), "{id:?} reads as a finer grid");
            if encodes {
                assert_eq!(lb.encode(), bytes, "{id:?} writes back otherwise");
            }
            read.blocks += 1;
            for &h in &lb.height {
                let _ = write!(d, "{:08x}", table[usize::from(h)].to_bits());
                read.vertices += 1;
            }
            let (x, y) = (i32::from(bx), i32::from(by));
            for lod in [1u8, 2, 4, 8] {
                let m = generate_landblock_with_table(
                    &lb,
                    &region,
                    &table,
                    x,
                    y,
                    lod,
                    Direction::InViewerBlock,
                );
                let svc = usize::from(m.side_vertex_count);
                let step = 8 / (svc - 1);
                for (k, v) in m.vertices.iter().enumerate() {
                    let (i, j) = (k / svc, k % svc);
                    let want = table[usize::from(lb.height[(i * 9 + j) * step])];
                    assert_eq!(v.z.to_bits(), want.to_bits(), "{id:?} lod {lod} vertex {k}");
                    let _ = write!(
                        d,
                        "{:08x}{:08x}{:08x}",
                        v.x.to_bits(),
                        v.y.to_bits(),
                        v.z.to_bits()
                    );
                    read.mesh_vertices += 1;
                }
                for p in &m.polygons {
                    let n = p.plane.normal;
                    let _ = write!(
                        d,
                        "{:08x}{:08x}{:08x}{:08x}",
                        n.x.to_bits(),
                        n.y.to_bits(),
                        n.z.to_bits(),
                        p.plane.d.to_bits()
                    );
                }
            }
            let lid = LandblockId((bx << 8) | by);
            let c = dereth_world_data::land_source::land_collision(
                &LandGrid::DEFAULT,
                lid,
                &lb,
                &table,
            )
            .unwrap_or_else(|| panic!("{id:?} has no collision"));
            for (k, v) in c.vertices.iter().enumerate() {
                assert_eq!(
                    v.z.to_bits(),
                    table[usize::from(lb.height[k])].to_bits(),
                    "{id:?} collision vertex {k}"
                );
                let _ = write!(d, "{:08x}", v.z.to_bits());
            }
            for p in &c.polygons {
                let n = p.plane.normal;
                let _ = write!(
                    d,
                    "{:08x}{:08x}{:08x}{:08x}",
                    n.x.to_bits(),
                    n.y.to_bits(),
                    n.z.to_bits(),
                    p.plane.d.to_bits()
                );
                read.polygons += 1;
            }
            let _ = write!(
                d,
                "{:08x}{:08x}",
                c.max_zval.to_bits(),
                c.min_zval.to_bits()
            );
        }
    }
    read.digest = d.0;
    read
}

/// **Every landblock of acclient 00.00.11.6096's cell file reads as it always has**: a 9 × 9
/// record writing back to its own bytes, each mesh and collision vertex at its height-table entry
/// to the bit, and the whole digesting to what it digested before the finer grid's height bytes.
#[test]
fn every_landblock_of_the_end_of_retail_files_reads_as_it_always_has() {
    let store = dereth_dat::testing::open_store().expect("the retail dats");
    let read = read_all(&store, true);
    eprintln!("6096: {read:?}");
    assert_eq!(read, RECORDED_6096);
}

/// The end of retail's land as read before the finer grid's height bytes.
const RECORDED_6096: Read = Read {
    blocks: 65_025,
    vertices: 5_267_025,
    mesh_vertices: 7_737_975,
    polygons: 8_323_200,
    digest: 2_808_736_190_102_583_829,
};

/// **Every landblock of the October 1999 cell file reads as it always has**, by the same rule:
/// each mesh and collision vertex at its height-table entry to the bit, none a finer grid. The
/// launch files have no encoder, so their records are read only. Where the capture isn't present,
/// says so and checks nothing.
#[test]
fn every_landblock_of_the_october_1999_files_reads_as_it_always_has() {
    let Some(cell) = std::env::var_os(dereth_dat::testing::DAT_CAPTURES_DIR_VAR)
        .filter(|v| !v.is_empty())
        .map(std::path::PathBuf::from)
        .map(|d| d.join("1999-10-09").join("cell.dat"))
        .filter(|p| p.exists())
    else {
        eprintln!("the 1999-10-09 capture's cell.dat is not here: nothing checked");
        return;
    };
    let dir = cell.parent().expect("the capture's folder").to_path_buf();
    let store = RetailDatStore::open_classic_with_modern(&dir, &dereth_dat::testing::dat_dir())
        .expect("the October 1999 files open beside the retail ones");
    let read = read_all(&store, false);
    eprintln!("1999-10-09: {read:?}");
    assert!(read.blocks > 0);
}
