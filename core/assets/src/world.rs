//! The cell dat — land blocks, block metadata and environment cells.
//!
//! The reference parser decodes all 805,347 cell records with the cursor landing exactly on each
//! record end.
//!
//! Two traps live here, both of which have cost someone an afternoon:
//!
//! * the object entries are **interleaved** `{DataID, Frame}` 32-byte records, not ids-then-frames;
//! * `num_buildings` is a `ushort` and the upper half of the same dword is a *packMask* whose bit 0
//!   says a restriction table follows.

use std::collections::BTreeMap;

use dereth_dat::{packobj::read_n, Cursor, DatError, DbType};
use dereth_primitives::{DataId, Frame};

use crate::error::AssetError;
use crate::Decode;

/// Nine vertices to a side, `x * 9 + y`, **x major**. Transposing it mirrors the
/// world, and the mirror is symmetric enough to look plausible.
pub const SIDE_VERTEX_COUNT: usize = 9;
/// `9 * 9`.
pub const VERTEX_COUNT: usize = SIDE_VERTEX_COUNT * SIDE_VERTEX_COUNT;
/// Every landblock record is this size, all 65,025 of them.
pub const LANDBLOCK_BYTES: usize = 252;

/// One `{DataID, Frame}` pair. 32 bytes, interleaved.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ObjectEntry {
    pub id: DataId,
    pub frame: Frame,
}

impl ObjectEntry {
    fn decode(c: &mut Cursor<'_>) -> Result<Self, DatError> {
        Ok(Self {
            id: c.data_id()?,
            frame: c.placed_frame()?,
        })
    }
}

// ---------------------------------------------------------------------------------------------
// 0xXXYYFFFF - cell land block
// ---------------------------------------------------------------------------------------------

/// A decoded `0xXXYYFFFF` landblock: the terrain word and height index per vertex. Always 252 bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellLandblock {
    pub id: DataId,
    /// Non-zero when a `0xXXYYFFFE` file exists for this block. ACE calls it `HasObjects`.
    pub lbi_exists: u32,
    /// Terrain word per vertex, indexed `x * 9 + y`.
    ///
    /// Bits `0x0780` are zero in every retail landblock and no reader masks them
    /// off. Preserved verbatim so a future answer is not lost.
    pub terrain: [u16; VERTEX_COUNT],
    /// Height-table index per vertex, same indexing.
    pub height: [u8; VERTEX_COUNT],
}

impl CellLandblock {
    /// Bits 0-1: non-zero means a road passes through this vertex.
    #[must_use]
    pub fn road(&self, x: usize, y: usize) -> u16 {
        self.terrain[x * SIDE_VERTEX_COUNT + y] & 0x0003
    }
    /// Bits 2-6: index into the region's terrain type table.
    #[must_use]
    pub fn terrain_type(&self, x: usize, y: usize) -> u16 {
        (self.terrain[x * SIDE_VERTEX_COUNT + y] & 0x007C) >> 2
    }
    /// Bits 11-15: index into the region's scene-type list.
    #[must_use]
    pub fn scene_type(&self, x: usize, y: usize) -> u16 {
        self.terrain[x * SIDE_VERTEX_COUNT + y] >> 11
    }
}

impl Decode for CellLandblock {
    const TYPE: DbType = DbType::LandBlock;

    fn declared_id(&self) -> Option<DataId> {
        Some(self.id)
    }

    fn decode(c: &mut Cursor<'_>) -> Result<Self, AssetError> {
        let id = c.data_id()?;
        let lbi_exists = c.u32()?;
        let mut terrain = [0u16; VERTEX_COUNT];
        for t in &mut terrain {
            *t = c.u16()?;
        }
        let mut height = [0u8; VERTEX_COUNT];
        for h in &mut height {
            *h = c.u8()?;
        }
        // The one piece of explicit padding in the whole cell dat: the cursor is at 0xFB, so one
        // zero byte rounds it up to 252.
        c.align_ptr();
        Ok(Self {
            id,
            lbi_exists,
            terrain,
            height,
        })
    }
}

// ---------------------------------------------------------------------------------------------
// 0xXXYYFFFE - land-block metadata
// ---------------------------------------------------------------------------------------------

/// One building portal. 8 bytes plus the stab list, padded to 4.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildingPortal {
    /// bit 0 -> `exact_match`; bit 1 -> `portal_side = ((~flags) >> 1) & 1`.
    pub flags: u16,
    /// Low 16 bits; the client ORs in the landblock base.
    pub other_cell_id: u16,
    /// Signed polygon index in the other cell; -1 = none.
    pub other_portal_id: i16,
    /// Cell indices visible through this portal, again low 16 bits only.
    pub stab_list: Vec<u16>,
}

impl BuildingPortal {
    /// bit 0.
    #[must_use]
    pub fn exact_match(&self) -> bool {
        self.flags & 1 != 0
    }
    /// `portal_side` is 1 when bit 1 is **clear**.
    #[must_use]
    pub fn portal_side(&self) -> u16 {
        (!self.flags >> 1) & 1
    }
}

/// `BuildInfo`: 40 bytes plus portals.
#[derive(Debug, Clone, PartialEq)]
pub struct BuildInfo {
    pub id: DataId,
    pub frame: Frame,
    /// Unpacked but never read by the client. Decoded; the renderer may want it.
    pub num_leaves: u32,
    pub portals: Vec<BuildingPortal>,
}

/// A decoded `0xXXYYFFFE` landblock info: the static objects and buildings standing on a block.
#[derive(Debug, Clone, PartialEq)]
pub struct LandblockInfo {
    pub id: DataId,
    /// The number of [`EnvCell`] records belonging to this block. **Not** a count of the records that
    /// follow — the client uses it only to bound cell prefetch.
    pub num_cells: u32,
    pub objects: Vec<ObjectEntry>,
    /// The high half of the `num_buildings` dword. Only bit 0 is read: a restriction table follows.
    pub pack_mask: u16,
    pub buildings: Vec<BuildInfo>,
    /// Packed map from cell id to owning object id. Present iff `pack_mask & 1`.
    pub restrictions: Option<RestrictionTable>,
}

/// The restriction table, kept with its bucket count so a round trip can reproduce bucket order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestrictionTable {
    pub buckets: u32,
    /// In file (bucket) order. A `BTreeMap` would lose that order, and bucket order is
    /// load-bearing for a byte-exact round trip.
    pub entries: Vec<(u32, u32)>,
}

impl Decode for LandblockInfo {
    const TYPE: DbType = DbType::Lbi;

    fn declared_id(&self) -> Option<DataId> {
        Some(self.id)
    }

    fn decode(c: &mut Cursor<'_>) -> Result<Self, AssetError> {
        let id = c.data_id()?;
        let num_cells = c.u32()?;
        let n = c.u32()? as usize;
        let objects = read_n(c, n, ObjectEntry::decode)?;
        // num_buildings is a ushort; the upper half of the same dword is the packMask.
        let num_buildings = c.u16()? as usize;
        let pack_mask = c.u16()?;
        let mut buildings = Vec::new();
        for _ in 0..num_buildings {
            let bid = c.data_id()?;
            let frame = c.placed_frame()?;
            let num_leaves = c.u32()?;
            let np = c.u32()? as usize;
            let mut portals = Vec::new();
            for _ in 0..np {
                let flags = c.u16()?;
                let other_cell_id = c.u16()?;
                let other_portal_id = c.i16()?;
                let ns = c.u16()? as usize;
                let stab_list = read_n(c, ns, Cursor::u16)?;
                // Two zero bytes when num_stabs is odd, to re-align to 4.
                c.align_ptr();
                portals.push(BuildingPortal {
                    flags,
                    other_cell_id,
                    other_portal_id,
                    stab_list,
                });
            }
            buildings.push(BuildInfo {
                id: bid,
                frame,
                num_leaves,
                portals,
            });
        }
        let restrictions = if pack_mask & 1 != 0 {
            let h = dereth_dat::packobj::packable_hash_table_header(c)?;
            let entries = read_n(c, h.count as usize, |c| Ok((c.u32()?, c.u32()?)))?;
            Some(RestrictionTable {
                buckets: h.buckets,
                entries,
            })
        } else {
            None
        };
        Ok(Self {
            id,
            num_cells,
            objects,
            pack_mask,
            buildings,
            restrictions,
        })
    }
}

// ---------------------------------------------------------------------------------------------
// 0xXXYY0100+ - environment cell
// ---------------------------------------------------------------------------------------------

/// One cell portal. 8 bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellPortal {
    /// bit 0 `exact_match`, bit 1 `portal_side`, bit 2 "leads outside".
    pub flags: u16,
    /// Id of the portal polygon inside the cell struct.
    pub polygon_id: u16,
    /// `0xFFFFFFFF` when the portal leads outside (flags bit 2), otherwise **the stored low 16
    /// bits, unwidened**. The environment-cell loader ORs this with the landblock base.
    /// A decoder receives one record and does not know its landblock, so the widening is
    /// the caller's: a caller that assumes the widening has happened addresses cell `0x0110`
    /// instead of `0xA9B40110`.
    pub other_cell_id: u32,
    pub other_portal_id: i16,
}

/// A decoded `0xXXYY0100+` indoor cell.
#[derive(Debug, Clone, PartialEq)]
pub struct EnvCell {
    pub id: DataId,
    /// bit 0 `seen_outside`, bit 1 static objects follow, bit 3 a `restriction_obj` follows.
    pub flags: u32,
    /// The cell id again; identical to `id` in every retail file.
    pub cell_id_repeat: u32,
    /// Surface ids, already widened from the stored `u16` with `0x08000000`.
    pub surfaces: Vec<DataId>,
    /// Environment id, widened with `0x0D000000`.
    pub environment: DataId,
    /// Index into the environment's cell array.
    pub cell_struct: u16,
    pub frame: Frame,
    pub portals: Vec<CellPortal>,
    /// Visible-cell indices, low 16 bits only.
    pub visible_cells: Vec<u16>,
    pub static_objects: Vec<ObjectEntry>,
    pub restriction_obj: Option<u32>,
}

impl Decode for EnvCell {
    const TYPE: DbType = DbType::Cell;

    fn declared_id(&self) -> Option<DataId> {
        Some(self.id)
    }

    fn decode(c: &mut Cursor<'_>) -> Result<Self, AssetError> {
        let id = c.data_id()?;
        let flags = c.u32()?;
        let cell_id_repeat = c.u32()?;
        let num_surfaces = c.u8()? as usize;
        let num_portals = c.u8()? as usize;
        let num_stabs = c.u16()? as usize;
        let surfaces = read_n(c, num_surfaces, |c| {
            Ok(DataId(0x0800_0000 | u32::from(c.u16()?)))
        })?;
        let environment = DataId(0x0D00_0000 | u32::from(c.u16()?));
        let cell_struct = c.u16()?;
        // Note the offset: with an odd surface count the Frame starts 2-aligned but not 4-aligned,
        // and the client reads these floats unaligned. Nothing pads here.
        let frame = c.placed_frame()?;
        let portals = read_n(c, num_portals, |c| {
            let flags = c.u16()?;
            let polygon_id = c.u16()?;
            let other = c.u16()?;
            let other_portal_id = c.i16()?;
            Ok(CellPortal {
                flags,
                polygon_id,
                other_cell_id: if flags & 4 != 0 {
                    0xFFFF_FFFF
                } else {
                    u32::from(other)
                },
                other_portal_id,
            })
        })?;
        let visible_cells = read_n(c, num_stabs, Cursor::u16)?;
        let static_objects = if flags & 2 != 0 {
            let n = c.u32()? as usize;
            read_n(c, n, ObjectEntry::decode)?
        } else {
            Vec::new()
        };
        let restriction_obj = if flags & 8 != 0 { Some(c.u32()?) } else { None };
        Ok(Self {
            id,
            flags,
            cell_id_repeat,
            surfaces,
            environment,
            cell_struct,
            frame,
            portals,
            visible_cells,
            static_objects,
            restriction_obj,
        })
    }
}

// ---------------------------------------------------------------------------------------------
// Scene (ids 0x12xxxxxx) / ParticleEmitterInfo (ids 0x32xxxxxx)
// ---------------------------------------------------------------------------------------------

/// One object placed inside a scene.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ObjectDesc {
    pub obj_id: DataId,
    pub base_loc: Frame,
    pub freq: f32,
    pub displace_x: f32,
    pub displace_y: f32,
    pub min_scale: f32,
    pub max_scale: f32,
    pub max_rot: f32,
    pub min_slope: f32,
    pub max_slope: f32,
    pub align: i32,
    /// Serialised (1 on 40 of 1,167 records) and read by nothing. Decoded.
    pub orient: i32,
    pub weenie_obj: i32,
}

/// A decoded Scene (ids `0x12xxxxxx`, record type `0x1B`): the objects a terrain scene scatters.
#[derive(Debug, Clone, PartialEq)]
pub struct Scene {
    pub id: DataId,
    pub objects: Vec<ObjectDesc>,
}

impl Decode for Scene {
    const TYPE: DbType = DbType::Scene;

    fn declared_id(&self) -> Option<DataId> {
        Some(self.id)
    }

    fn decode(c: &mut Cursor<'_>) -> Result<Self, AssetError> {
        let id = c.data_id()?;
        let n = c.u32()? as usize;
        let objects = read_n(c, n, |c| {
            Ok(ObjectDesc {
                obj_id: c.data_id()?,
                base_loc: c.placed_frame()?,
                freq: c.f32()?,
                displace_x: c.f32()?,
                displace_y: c.f32()?,
                min_scale: c.f32()?,
                max_scale: c.f32()?,
                max_rot: c.f32()?,
                min_slope: c.f32()?,
                max_slope: c.f32()?,
                align: c.i32()?,
                orient: c.i32()?,
                weenie_obj: c.i32()?,
            })
        })?;
        Ok(Self { id, objects })
    }
}

/// A decoded `0x32` ParticleEmitterInfo.
///
/// Two format questions remain: the dword at file offset 4 is skipped by the decoder and is 0 in all
/// 2,051 emitters, and the `a`/`b`/`c` vectors' meanings depend on `ParticleType`. Both are decoded
/// and retain neutral names until their meaning is established.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParticleEmitterInfo {
    pub id: DataId,
    /// The dword `UnPack` skips. 0 in every shipped emitter.
    pub unknown_skipped: u32,
    pub emitter_type: i32,
    pub particle_type: i32,
    pub gfxobj_id: DataId,
    pub hw_gfxobj_id: DataId,
    pub birthrate: f64,
    pub max_particles: i32,
    pub initial_particles: i32,
    pub total_particles: i32,
    pub total_seconds: f64,
    pub lifespan: f64,
    pub lifespan_rand: f64,
    pub offset_dir: dereth_primitives::Vec3,
    pub min_offset: f32,
    pub max_offset: f32,
    pub a: dereth_primitives::Vec3,
    pub min_a: f32,
    pub max_a: f32,
    pub b: dereth_primitives::Vec3,
    pub min_b: f32,
    pub max_b: f32,
    pub c: dereth_primitives::Vec3,
    pub min_c: f32,
    pub max_c: f32,
    pub start_scale: f32,
    pub final_scale: f32,
    pub scale_rand: f32,
    pub start_trans: f32,
    pub final_trans: f32,
    pub trans_rand: f32,
    pub is_parent_local: i32,
}

impl Decode for ParticleEmitterInfo {
    const TYPE: DbType = DbType::ParticleEmitter;

    fn declared_id(&self) -> Option<DataId> {
        Some(self.id)
    }

    fn decode(c: &mut Cursor<'_>) -> Result<Self, AssetError> {
        Ok(Self {
            id: c.data_id()?,
            unknown_skipped: c.u32()?,
            emitter_type: c.i32()?,
            particle_type: c.i32()?,
            gfxobj_id: c.data_id()?,
            hw_gfxobj_id: c.data_id()?,
            birthrate: c.f64()?,
            max_particles: c.i32()?,
            initial_particles: c.i32()?,
            total_particles: c.i32()?,
            total_seconds: c.f64()?,
            lifespan: c.f64()?,
            lifespan_rand: c.f64()?,
            offset_dir: c.vec3()?,
            min_offset: c.f32()?,
            max_offset: c.f32()?,
            a: c.vec3()?,
            min_a: c.f32()?,
            max_a: c.f32()?,
            b: c.vec3()?,
            min_b: c.f32()?,
            max_b: c.f32()?,
            c: c.vec3()?,
            min_c: c.f32()?,
            max_c: c.f32()?,
            start_scale: c.f32()?,
            final_scale: c.f32()?,
            scale_rand: c.f32()?,
            start_trans: c.f32()?,
            final_trans: c.f32()?,
            trans_rand: c.f32()?,
            is_parent_local: c.i32()?,
        })
    }
}

/// The restriction table's entries as a map, for callers that do not care about bucket order.
#[must_use]
pub fn restrictions_as_map(t: &RestrictionTable) -> BTreeMap<u32, u32> {
    t.entries.iter().copied().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the complete 252-byte payload of the representative `0xA9B4FFFF` cell record.
    #[test]
    fn holtburg_landblock_decodes_to_the_documented_values() {
        let mut b = Vec::new();
        b.extend_from_slice(&0xA9B4_FFFFu32.to_le_bytes());
        b.extend_from_slice(&1u32.to_le_bytes());
        // terrain[0..6] from the document's hex dump.
        for w in [0x0004u16, 0x7804, 0x000C, 0x000C, 0x780C, 0x000D] {
            b.extend_from_slice(&w.to_le_bytes());
        }
        b.resize(4 + 4 + 162, 0);
        b.push(45); // height[0][0]
        b.push(42); // height[0][1]
        b.resize(4 + 4 + 162 + 81, 0);
        b.push(0); // the single pad byte at 0xFB
        assert_eq!(b.len(), LANDBLOCK_BYTES);

        let lb = CellLandblock::decode_payload(DataId(0xA9B4_FFFF), &b).unwrap();
        assert_eq!(lb.id, DataId(0xA9B4_FFFF));
        assert_eq!(lb.lbi_exists, 1);
        assert_eq!(lb.terrain[0], 0x0004);
        assert_eq!(lb.terrain[1], 0x7804);
        assert_eq!(lb.height[0], 45);
        assert_eq!(lb.height[1], 42);
        // Section 2.2's reading of those two words.
        assert_eq!(
            (lb.road(0, 0), lb.terrain_type(0, 0), lb.scene_type(0, 0)),
            (0, 1, 0)
        );
        assert_eq!(
            (lb.road(0, 1), lb.terrain_type(0, 1), lb.scene_type(0, 1)),
            (0, 1, 15)
        );
        // terrain[0][5] = 0x000D: road 1, type 3, scene 0.
        assert_eq!(
            (lb.road(0, 5), lb.terrain_type(0, 5), lb.scene_type(0, 5)),
            (1, 3, 0)
        );
    }

    /// Contract 9.10: `x * 9 + y`, x major. If this were transposed the world would be mirrored.
    #[test]
    fn the_terrain_array_is_x_major() {
        let mut b = vec![0u8; LANDBLOCK_BYTES];
        b[0..4].copy_from_slice(&0u32.to_le_bytes());
        // Put a marker at flat index 9, which is (x=1, y=0) under x-major indexing.
        b[8 + 9 * 2..8 + 9 * 2 + 2].copy_from_slice(&0xBEEFu16.to_le_bytes());
        let lb = CellLandblock::decode_payload(DataId(0), &b).unwrap();
        assert_eq!(lb.terrain[SIDE_VERTEX_COUNT], 0xBEEF);
        assert_eq!(lb.terrain[9], 0xBEEF);
    }
}
