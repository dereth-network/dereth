//! A small set of data files of the later layout or of the layout from before Throne of Destiny,
//! and packs and worlds written over it, in a scratch folder.

use std::path::{Path, PathBuf};

use dereth_assets::material::PFID_CUSTOM_B8G8R8;
use dereth_assets::world::{CellLandblock, LandblockInfo, ObjectEntry};
use dereth_assets::{RenderSurface, Surface, SurfaceTexture};
use dereth_dat::testing::ScratchDir;
use dereth_dat::{iteration, ClassicDat, DatWriter, ModernDat, ITERATION_LIST};
use dereth_primitives::{DataId, EraId, Frame, Quat, Vec3};

/// A model the base holds, a GfxObj and a Setup.
pub const GFXOBJ: u32 = 0x0100_0001;
pub const SETUP: u32 = 0x0200_0001;
/// A picture, its image texture and its surface.
pub const PICTURE: u32 = 0x0600_0010;
pub const TEXTURE: u32 = 0x0500_0010;
pub const SURFACE: u32 = 0x0800_0010;
/// The landblock with a statics record; its three neighbors to the north and east have none.
pub const HOME: u16 = 0x0101;

pub fn frame(x: f32, y: f32, z: f32) -> Frame {
    Frame::new(Vec3::new(x, y, z), Quat::new(1.0, 0.0, 0.0, 0.0))
}

fn container(path: &Path, set: u32, records: &[(u32, Vec<u8>)], iterations: &[u32]) {
    let mut w = DatWriter::create(path, 0x400, set, 0, 0x400 + 0x400 * 64).unwrap();
    for (id, bytes) in records {
        w.save(DataId(*id), bytes, 2, 1, 0).unwrap();
    }
    w.save(ITERATION_LIST, &iteration::encode(iterations), 1, 0, 0)
        .unwrap();
}

/// The terrain of landblock `lb`: every vertex at height index `10 + x + y`, terrain type 1.
pub fn terrain(lb: u16, lbi_exists: bool) -> CellLandblock {
    let mut t = CellLandblock {
        id: DataId((u32::from(lb) << 16) | 0xFFFF),
        lbi_exists: u32::from(lbi_exists),
        terrain: [1 << 2; 81],
        height: [0; 81],
    };
    for x in 0..9u8 {
        for y in 0..9u8 {
            t.height[usize::from(x) * 9 + usize::from(y)] = 10 + x + y;
        }
    }
    t
}

/// The statics record of [`HOME`]: the GfxObj and the Setup.
pub fn home_statics() -> LandblockInfo {
    LandblockInfo {
        id: DataId((u32::from(HOME) << 16) | 0xFFFE),
        num_cells: 0,
        objects: vec![
            ObjectEntry {
                id: DataId(GFXOBJ),
                frame: frame(10.0, 10.0, 5.0),
            },
            ObjectEntry {
                id: DataId(SETUP),
                frame: frame(20.0, 20.0, 5.0),
            },
        ],
        pack_mask: 0,
        buildings: Vec::new(),
        restrictions: None,
    }
}

/// A scratch folder holding a base set of files in `dats/`.
pub struct Fixture {
    pub dir: ScratchDir,
}

impl Fixture {
    pub fn new(tag: &str) -> Self {
        let dir = ScratchDir::new(tag).unwrap();
        let dats = dir.path().join("dats");
        std::fs::create_dir_all(&dats).unwrap();
        let picture = RenderSurface {
            id: DataId(PICTURE),
            data_category: 4,
            width: 2,
            height: 2,
            format: 21,
            image_size: 16,
            data_offset: 0x18,
            default_palette_id: None,
        }
        .encode(&[0x40; 16]);
        let texture = SurfaceTexture {
            id: DataId(TEXTURE),
            data_category: 0,
            unknown_byte: 2,
            source_levels: vec![DataId(PICTURE)],
        }
        .encode();
        let surface = Surface {
            surface_type: 2,
            orig_texture_id: Some(DataId(TEXTURE)),
            orig_palette_id: Some(DataId(0)),
            color_value: None,
            translucency: 0.0,
            luminosity: 0.0,
            diffuse: 1.0,
        }
        .encode();
        container(
            &ModernDat::Portal.in_dir(&dats),
            1,
            &[
                (GFXOBJ, vec![1; 16]),
                (SETUP, vec![2; 16]),
                (TEXTURE, texture),
                (PICTURE, picture),
                (SURFACE, surface),
            ],
            &[1, 2],
        );
        let mut cell = vec![((u32::from(HOME) << 16) | 0xFFFE, home_statics().encode())];
        for lb in [0x0101u16, 0x0102, 0x0201, 0x0202] {
            cell.push((
                (u32::from(lb) << 16) | 0xFFFF,
                terrain(lb, lb == HOME).encode(),
            ));
        }
        cell.sort_by_key(|(id, _)| *id);
        container(&ModernDat::Cell.in_dir(&dats), 2, &cell, &[1, 2, 3]);
        Self { dir }
    }

    /// A scratch folder holding the same base in the layout from before Throne of Destiny in
    /// `dats/`: `portal.dat` (its image texture holding its picture's pixels, its picture three
    /// bytes a pixel, its surface carrying its id) and `cell.dat`, at iterations 2 and 3.
    pub fn classic(tag: &str) -> Self {
        let dir = ScratchDir::new(tag).unwrap();
        let dats = dir.path().join("dats");
        std::fs::create_dir_all(&dats).unwrap();
        let era = EraId::Infiltration;
        let picture = RenderSurface {
            id: DataId(PICTURE),
            data_category: 0,
            width: 2,
            height: 2,
            format: PFID_CUSTOM_B8G8R8,
            image_size: 12,
            data_offset: 12,
            default_palette_id: None,
        }
        .encode_for(era, &[0x40; 12])
        .unwrap();
        let texture = RenderSurface {
            id: DataId(TEXTURE),
            data_category: 0,
            width: 2,
            height: 2,
            format: 23,
            image_size: 8,
            data_offset: 16,
            default_palette_id: None,
        }
        .encode_texture_for(era, &[0x40; 8])
        .unwrap();
        let surface = Surface {
            surface_type: 2,
            orig_texture_id: Some(DataId(TEXTURE)),
            orig_palette_id: Some(DataId(0)),
            color_value: None,
            translucency: 0.0,
            luminosity: 0.0,
            diffuse: 1.0,
        }
        .encode_for(era, DataId(SURFACE))
        .unwrap();
        let portal = dereth_dat::testing::classic_container(
            0x400,
            2,
            &[
                (GFXOBJ, vec![1; 16]),
                (SETUP, vec![2; 16]),
                (TEXTURE, texture),
                (PICTURE, picture),
                (SURFACE, surface),
            ],
        );
        std::fs::write(ClassicDat::Portal.in_dir(&dats), portal).unwrap();
        let mut cell = vec![((u32::from(HOME) << 16) | 0xFFFE, home_statics().encode())];
        for lb in [0x0101u16, 0x0102, 0x0201, 0x0202] {
            cell.push((
                (u32::from(lb) << 16) | 0xFFFF,
                terrain(lb, lb == HOME).encode(),
            ));
        }
        let cell = dereth_dat::testing::classic_container(0x100, 3, &cell);
        std::fs::write(ClassicDat::Cell.in_dir(&dats), cell).unwrap();
        Self { dir }
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    pub fn dats(&self) -> PathBuf {
        self.path().join("dats")
    }

    /// Write `text` at `rel` under the scratch folder.
    pub fn write(&self, rel: &str, text: &str) -> PathBuf {
        let p = self.path().join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, text).unwrap();
        p
    }

    /// A world `world.toml` with the one pack `sample` (its `pack.toml` written), its output in
    /// `build/`.
    pub fn world(&self) -> PathBuf {
        self.write(
            "sample/pack.toml",
            "name = \"sample\"\nversion = \"0.1.0\"\nlicense = \"MIT\"\n",
        );
        self.write(
            "world.toml",
            "key = \"test-world\"\npacks = [\"sample\"]\nout = \"build\"\n",
        )
    }

    pub fn out(&self) -> PathBuf {
        self.path().join("build")
    }
}

/// A PNG of `w` x `h` pixels, red, green, blue, alpha.
pub fn png(w: u32, h: u32, rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut e = png::Encoder::new(&mut out, w, h);
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        let mut w = e.write_header().unwrap();
        w.write_image_data(rgba).unwrap();
    }
    out
}

/// The record `id` of the file `t` as the world reads it through the overlay in `out/overlay`.
pub fn read_through(dats: &Path, out: &Path, t: ModernDat, id: u32) -> Option<Vec<u8>> {
    let set = lyceum_foundry::dats::DatSet::open(dats).unwrap();
    let set = set.with_overlay(&out.join("overlay"), None).unwrap();
    set.file(t).unwrap().read(DataId(id)).ok()
}
