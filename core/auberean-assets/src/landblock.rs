//! The cell files: landblocks (`0xXXYYFFFF`), their placed objects (`0xXXYYFFFE`), their light
//! (`0xXXYYFFFC`) and their interior cells (`0xXXYY0100` and up).

use auberean_dat::reader::Result;
use auberean_dat::{QualifiedIds, Reader, RecordType, Writer};
use auberean_primitives::{CellId, DataId, Frame, Vec3};

use crate::entity::EntityGroup;
use crate::property::PropertyCollection;
use crate::{Context, Decode, Encode};

/// Vertices along a landblock's side.
pub const SIDE: usize = 17;
/// Vertices of a landblock.
pub const VERTICES: usize = SIDE * SIDE;

/// A landblock's terrain: 17 x 17 vertices, each a height byte and a word saying what covers it.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Landblock {
    /// Its placed objects' record (`0xXXYYFFFE`), or 0 when it has none.
    pub info: DataId,
    /// Its path map's record, or 0.
    pub path_map: DataId,
    pub heights: Vec<u8>,
    pub vertices: Vec<u32>,
}

impl Landblock {
    /// The records the landblock names, as the data cache gathers them: its info and its path
    /// map.
    // D2-03700
    pub fn sub_ids(&self, out: &mut QualifiedIds) {
        out.add_typed(self.info.0, RecordType::LANDBLOCK_INFO.0);
        out.add_typed(self.path_map.0, RecordType::PATH_MAP.0);
    }
}

impl Decode for Landblock {
    /// The id, the info id, the path map id, a counted list of height bytes, a counted list of
    /// vertex words, then alignment.
    // D2-03712, D2-03724
    fn decode(r: &mut Reader<'_>, id: u32, _ctx: &Context) -> Result<Self> {
        r.expect_id(id)?;
        let info = r.data_id()?;
        let path_map = r.data_id()?;
        let n = r.count(1)?;
        let heights = r.bytes(n)?.to_vec();
        let vertices = r.u32_list()?;
        r.align()?;
        r.ensure(
            heights.len() == VERTICES && vertices.len() == VERTICES,
            || format!("{} heights, {} vertex words", heights.len(), vertices.len()),
        )?;
        Ok(Self {
            info,
            path_map,
            heights,
            vertices,
        })
    }
}

/// A landblock's placed objects and what its interior holds.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct LandblockInfo {
    pub version: Option<u32>,
    /// How many interior cells the landblock has.
    pub cell_count: u32,
    /// Its light record (`0xXXYYFFFC`), or 0.
    pub light_info: DataId,
    pub entities: EntityGroup,
    pub properties: PropertyCollection,
    /// The interior cells visible from outside, by low word.
    pub outside_visible: Vec<u32>,
}

impl LandblockInfo {
    /// The records the info names, as the data cache gathers them: what its entities and its
    /// properties name, and its light record. (Its interior cells are not among them: the
    /// client never fills in the list they would be named from.)
    // D2-03733
    pub fn sub_ids(&self, out: &mut QualifiedIds) {
        self.entities.sub_ids(out);
        self.properties.sub_ids(out);
        out.add_typed(self.light_info.0, RecordType::LIGHT_INFO.0);
    }
}

impl Decode for LandblockInfo {
    /// A flags word **before** the id (`0x1`: a version follows the id), the id, the version when
    /// flagged, the cell count, the light record id, an entity group (with its own id word), a
    /// property collection, a counted list of cell words, alignment.
    // D2-03739
    fn decode(r: &mut Reader<'_>, id: u32, ctx: &Context) -> Result<Self> {
        let flags = r.u32()?;
        r.expect_id(id)?;
        let version = if flags & 1 != 0 { Some(r.u32()?) } else { None };
        let cell_count = r.u32()?;
        let light_info = r.data_id()?;
        let entities = EntityGroup::read(r, ctx)?;
        let properties = PropertyCollection::read(r, ctx)?;
        let outside_visible = r.u32_list()?;
        r.align()?;
        Ok(Self {
            version,
            cell_count,
            light_info,
            entities,
            properties,
            outside_visible,
        })
    }
}

/// The cells one light reaches: a cell, and its lit vertices as a packed run-length list.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct LitCell {
    pub cell: u32,
    /// How many vertices the packed list expands to; when not positive, no list is stored.
    pub count: i32,
    pub packed: Vec<u8>,
}

// D2-03910
fn lit_cells(r: &mut Reader<'_>) -> Result<Vec<LitCell>> {
    r.list(8, |r| {
        let cell = r.u32()?;
        let count = r.i32()?;
        let mut packed = Vec::new();
        if count > 0 {
            let n = r.count(1)?;
            packed = r.bytes(n)?.to_vec();
            r.align()?;
        }
        Ok(LitCell {
            cell,
            count,
            packed,
        })
    })
}

/// One light of a landblock: where it is, its properties and the cells it reaches.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct LightSource {
    pub cell: CellId,
    pub properties: PropertyCollection,
    pub origin: Vec3,
    pub lit: Vec<LitCell>,
}

/// A landblock's lights, which cells each reaches, and the sunlight.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct LightInfo {
    pub lights: Vec<LightSource>,
    /// The size place the lookup table was stored with.
    pub lookup_buckets: u8,
    /// Per cell, the lights that reach it.
    pub lookup: Vec<(u32, Vec<u32>)>,
    /// The nine sun directions' lit cells, when sunlight is on.
    pub sunlight: Option<Vec<Vec<LitCell>>>,
}

impl Decode for LightInfo {
    /// The id, a counted list of lights (a cell id, a property collection, the origin, the lit
    /// cells), a hash table from cell to a counted list of light words, a sunlight word, and when
    /// it is set nine lists of lit cells.
    // D2-03917, D2-03924
    fn decode(r: &mut Reader<'_>, id: u32, ctx: &Context) -> Result<Self> {
        r.expect_id(id)?;
        let lights = r.list(24, |r| {
            Ok(LightSource {
                cell: CellId(r.u32()?),
                properties: PropertyCollection::read(r, ctx)?,
                origin: r.vec3()?,
                lit: lit_cells(r)?,
            })
        })?;
        let (lookup_buckets, lookup) =
            r.hash_table_with_buckets(8, |r| Ok((r.u32()?, r.u32_list()?)))?;
        let sunlight = if r.bool32()? {
            let mut v = Vec::with_capacity(9);
            for _ in 0..9 {
                v.push(lit_cells(r)?);
            }
            Some(v)
        } else {
            None
        };
        Ok(Self {
            lights,
            lookup_buckets,
            lookup,
            sunlight,
        })
    }
}

/// A doorway from one interior cell to another (or to the outside).
// D2-04237, D2-04238
#[derive(Clone, Debug, PartialEq, Default)]
pub struct CellPortal {
    pub flags: u16,
    /// Which of the cell mesh's portals this is.
    pub portal: u16,
    /// The cell on the other side, by low word.
    pub other_cell: u16,
    /// The portal's index in the other cell.
    pub other_portal: u16,
    /// With flag `0x4`: the cells (by low word) this portal is known to see.
    pub sees: Vec<u16>,
}

/// An interior cell: where it is, its geometry, its doorways, what it sees, and what is in it.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Cell {
    pub frame: Frame,
    /// `0x1` seen from outside, `0x2` holds entities, `0x4` holds properties, `0x8` never
    /// culled, `0x10` draws the sky.
    pub flags: u32,
    /// The cell's geometry (a `.cellmesh`, `0x01...`).
    pub mesh: DataId,
    pub portals: Vec<CellPortal>,
    /// The cells visible from this one, by low word.
    pub visible: Vec<u16>,
    /// The cells drawn together with this one, by low word.
    pub shared: Vec<u16>,
    pub entities: Option<EntityGroup>,
    pub properties: Option<PropertyCollection>,
}

fn u16_list(r: &mut Reader<'_>) -> Result<Vec<u16>> {
    let v = r.list(2, Reader::u16)?;
    r.align()?;
    Ok(v)
}

impl Cell {
    /// The records the cell names, as the data cache gathers them: its geometry, and what its
    /// entities and its properties name.
    // D2-03493
    pub fn sub_ids(&self, out: &mut QualifiedIds) {
        out.add(self.mesh.0);
        if let Some(g) = &self.entities {
            g.sub_ids(out);
        }
        if let Some(p) = &self.properties {
            p.sub_ids(out);
        }
    }
}

impl Decode for Cell {
    /// Its position (the cell id, which is the record id, then a frame), the flags, the mesh id,
    /// a counted list of portals (16-bit flags, portal, other cell, other portal, and with flag
    /// `0x4` a 16-bit-counted list of 16-bit cells and alignment), a counted list of visible cells
    /// and alignment, a counted list of shared cells and alignment, then an entity group (flag
    /// `0x2`) and a property collection (flag `0x4`).
    // D2-03569, D2-04243
    fn decode(r: &mut Reader<'_>, id: u32, ctx: &Context) -> Result<Self> {
        r.expect_id(id)?;
        let frame = r.frame()?;
        let flags = r.u32()?;
        r.ensure(flags & !0x1F == 0, || format!("cell flags {flags:#x}"))?;
        let mesh = r.data_id()?;
        let portals = r.list(8, |r| {
            let flags = r.u16()?;
            let portal = r.u16()?;
            let other_cell = r.u16()?;
            let other_portal = r.u16()?;
            let mut sees = Vec::new();
            if flags & 4 != 0 {
                let n = r.count16(2)?;
                for _ in 0..n {
                    sees.push(r.u16()?);
                }
                r.align()?;
            }
            Ok(CellPortal {
                flags,
                portal,
                other_cell,
                other_portal,
                sees,
            })
        })?;
        let visible = u16_list(r)?;
        let shared = u16_list(r)?;
        let entities = if flags & 2 != 0 {
            Some(EntityGroup::read(r, ctx)?)
        } else {
            None
        };
        let properties = if flags & 4 != 0 {
            Some(PropertyCollection::read(r, ctx)?)
        } else {
            None
        };
        Ok(Self {
            frame,
            flags,
            mesh,
            portals,
            visible,
            shared,
            entities,
            properties,
        })
    }
}

// D2-03910
fn write_lit_cells(w: &mut Writer, cells: &[LitCell]) {
    w.list(cells, |w, c| {
        w.u32(c.cell);
        w.i32(c.count);
        if c.count > 0 {
            w.count(c.packed.len());
            w.bytes(&c.packed);
            w.align();
        }
    });
}

fn write_u16_list(w: &mut Writer, v: &[u16]) {
    w.list(v, |w, x| w.u16(*x));
    w.align();
}

impl Encode for Landblock {
    /// As [`Landblock::decode`] reads it.
    // D2-03712, D2-03724
    fn encode(&self, id: u32, w: &mut Writer) {
        w.u32(id);
        w.data_id(self.info);
        w.data_id(self.path_map);
        w.count(self.heights.len());
        w.bytes(&self.heights);
        w.list(&self.vertices, |w, v| w.u32(*v));
        w.align();
    }
}

impl Encode for LandblockInfo {
    // D2-03739
    fn encode(&self, id: u32, w: &mut Writer) {
        w.u32(u32::from(self.version.is_some()));
        w.u32(id);
        if let Some(v) = self.version {
            w.u32(v);
        }
        w.u32(self.cell_count);
        w.data_id(self.light_info);
        self.entities.write(w);
        self.properties.write(w);
        w.list(&self.outside_visible, |w, x| w.u32(*x));
        w.align();
    }
}

impl Encode for LightInfo {
    // D2-03917, D2-03924
    fn encode(&self, id: u32, w: &mut Writer) {
        w.u32(id);
        w.list(&self.lights, |w, l| {
            w.u32(l.cell.0);
            l.properties.write(w);
            w.vec3(l.origin);
            write_lit_cells(w, &l.lit);
        });
        w.hash_table(self.lookup_buckets, &self.lookup, |w, (k, v)| {
            w.u32(*k);
            w.list(v, |w, x| w.u32(*x));
        });
        w.bool32(self.sunlight.is_some());
        if let Some(s) = &self.sunlight {
            for cells in s {
                write_lit_cells(w, cells);
            }
        }
    }
}

impl Encode for Cell {
    // D2-03569, D2-04243, D2-04241, D2-04242
    fn encode(&self, id: u32, w: &mut Writer) {
        w.u32(id);
        w.frame(self.frame);
        w.u32(self.flags);
        w.data_id(self.mesh);
        w.list(&self.portals, |w, p| {
            w.u16(p.flags);
            w.u16(p.portal);
            w.u16(p.other_cell);
            w.u16(p.other_portal);
            if p.flags & 4 != 0 {
                w.u16(u16::try_from(p.sees.len()).unwrap_or(u16::MAX));
                for x in &p.sees {
                    w.u16(*x);
                }
                w.align();
            }
        });
        write_u16_list(w, &self.visible);
        write_u16_list(w, &self.shared);
        if self.flags & 2 != 0 {
            self.entities.clone().unwrap_or_default().write(w);
        }
        if self.flags & 4 != 0 {
            self.properties.clone().unwrap_or_default().write(w);
        }
    }
}
