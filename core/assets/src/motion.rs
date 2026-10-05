//! `MotionTable`, `PhysicsScript`, `PhysicsScriptTable`, `GfxObjDegradeInfo`,
//! `ClothingTable`.
//!
//! Record layouts are described in `docs/formats/19-motion-table.md`,
//! `docs/formats/18-physics-scripts.md`, `docs/formats/22-degrade-info.md` and
//! `docs/formats/20-clothing-table.md`.

use std::collections::BTreeMap;

use dereth_dat::{packobj::packable_hash_table_header, packobj::read_n, Cursor, DatError, DbType};
use dereth_primitives::{DataId, Vec3};

use crate::error::AssetError;
use crate::hook::AnimHook;
use crate::Decode;

// ---------------------------------------------------------------------------------------------
// 0x09 MotionTable
// ---------------------------------------------------------------------------------------------

/// One animation reference inside a `MotionData`; the record is `dereth_primitives`', shared with the
/// animation crate.
pub use dereth_primitives::records::AnimData;

/// One motion entry: the animation it plays and the frame range and rate it plays it at.
///
/// The header is **three bytes plus one pad**, and the velocity/omega presence mask
/// is the **third** byte. Reading it as two bytes plus a `u16`, or taking the mask from the second
/// byte, desynchronises on the first table that carries a velocity.
#[derive(Debug, Clone, PartialEq)]
pub struct MotionData {
    /// The raw `MotionCommand` key. The top-byte bit layout is inferred from
    /// ACE, so the raw `u32` is kept rather than a decomposition.
    pub key: u32,
    /// Second header byte. Not read by the client's unpack beyond being stored.
    pub bitfield: u8,
    /// Third header byte: bit 0 = a velocity follows, bit 1 = an omega follows.
    pub flags: u8,
    pub anims: Vec<AnimData>,
    pub velocity: Option<Vec3>,
    pub omega: Option<Vec3>,
}

impl MotionData {
    fn decode(c: &mut Cursor<'_>) -> Result<Self, DatError> {
        let key = c.u32()?;
        let num_anims = c.u8()? as usize;
        let bitfield = c.u8()?;
        let flags = c.u8()?;
        c.align_ptr();
        let anims = read_n(c, num_anims, |c| {
            Ok(AnimData {
                anim_id: c.data_id()?,
                low_frame: c.i32()?,
                high_frame: c.i32()?,
                framerate: c.f32()?,
            })
        })?;
        let velocity = if flags & 1 != 0 {
            Some(c.vec3()?)
        } else {
            None
        };
        let omega = if flags & 2 != 0 {
            Some(c.vec3()?)
        } else {
            None
        };
        Ok(Self {
            key,
            bitfield,
            flags,
            anims,
            velocity,
            omega,
        })
    }
}

/// A decoded `0x09` MotionTable: which animation a motion resolves to, per style and per link.
#[derive(Debug, Clone, PartialEq)]
pub struct MotionTable {
    pub id: DataId,
    pub default_style: u32,
    pub style_defaults: BTreeMap<u32, u32>,
    pub cycles: Vec<MotionData>,
    pub modifiers: Vec<MotionData>,
    pub links: BTreeMap<u32, Vec<MotionData>>,
}

impl Decode for MotionTable {
    const TYPE: DbType = DbType::MTable;

    fn declared_id(&self) -> Option<DataId> {
        Some(self.id)
    }

    fn decode(c: &mut Cursor<'_>) -> Result<Self, AssetError> {
        let id = c.data_id()?;
        let default_style = c.u32()?;
        let n = c.u32()? as usize;
        let mut style_defaults = BTreeMap::new();
        for _ in 0..n {
            let k = c.u32()?;
            let v = c.u32()?;
            style_defaults.insert(k, v);
        }
        let n = c.u32()? as usize;
        let cycles = read_n(c, n, MotionData::decode)?;
        let n = c.u32()? as usize;
        let modifiers = read_n(c, n, MotionData::decode)?;
        let n = c.u32()? as usize;
        let mut links = BTreeMap::new();
        for _ in 0..n {
            let key = c.u32()?;
            let m = c.u32()? as usize;
            links.insert(key, read_n(c, m, MotionData::decode)?);
        }
        Ok(Self {
            id,
            default_style,
            style_defaults,
            cycles,
            modifiers,
            links,
        })
    }
}

// ---------------------------------------------------------------------------------------------
// 0x33 PhysicsScript / 0x34 PhysicsScriptTable
// ---------------------------------------------------------------------------------------------

/// One `{start_time, hook}` pair.
#[derive(Debug, Clone, PartialEq)]
pub struct ScriptStep {
    pub start_time: f64,
    pub hook: AnimHook,
}

/// A decoded `0x33` PhysicsScript: the timed hooks one script fires.
#[derive(Debug, Clone, PartialEq)]
pub struct PhysicsScript {
    pub id: DataId,
    pub script_data: Vec<ScriptStep>,
}

impl Decode for PhysicsScript {
    const TYPE: DbType = DbType::PhysicsScript;

    fn declared_id(&self) -> Option<DataId> {
        Some(self.id)
    }

    fn decode(c: &mut Cursor<'_>) -> Result<Self, AssetError> {
        let id = c.data_id()?;
        let n = c.u32()? as usize;
        let mut script_data = Vec::new();
        for _ in 0..n {
            let start_time = c.f64()?;
            let hook = AnimHook::decode(c)?;
            script_data.push(ScriptStep { start_time, hook });
        }
        c.align_ptr();
        Ok(Self { id, script_data })
    }
}

/// One row of a `PhysicsScriptTable`; the record is `dereth_primitives`', shared with the animation
/// crate.
pub use dereth_primitives::records::ScriptAndMod;

/// A decoded `0x34` PhysicsScriptTable: the scripts a physics state maps to.
#[derive(Debug, Clone, PartialEq)]
pub struct PhysicsScriptTable {
    pub id: DataId,
    pub script_table: BTreeMap<u32, Vec<ScriptAndMod>>,
}

/// A script type as the files from before Throne of Destiny number it, in the later numbering:
/// the types from 30 on are one higher afterwards.
#[must_use]
const fn pre_tod_script_type(key: u32) -> u32 {
    if key >= 30 {
        key + 1
    } else {
        key
    }
}

impl Decode for PhysicsScriptTable {
    const TYPE: DbType = DbType::PhysicsScriptTable;

    fn declared_id(&self) -> Option<DataId> {
        Some(self.id)
    }

    fn decode(c: &mut Cursor<'_>) -> Result<Self, AssetError> {
        let id = c.data_id()?;
        let n = c.u32()? as usize;
        let mut script_table = BTreeMap::new();
        for _ in 0..n {
            let key = c.u32()?;
            let m = c.u32()? as usize;
            let rows = read_n(c, m, |c| {
                Ok(ScriptAndMod {
                    modifier: c.f32()?,
                    script_id: c.data_id()?,
                })
            })?;
            // A repeated key keeps the rows it was first listed with, as the client's table
            // keeps them: the later rows are read and set aside.
            script_table.entry(key).or_insert(rows);
        }
        Ok(Self { id, script_table })
    }

    /// Before Throne of Destiny the script types from 30 on were numbered one lower: a type was
    /// later inserted at 30, and every type from there moved up by one (the February 2005 table's
    /// 117 is the hiding script that is 118 afterwards, its 116 the unhiding one). The table is
    /// keyed by the later numbering, which is what every caller asks with.
    fn decode_classic(c: &mut Cursor<'_>) -> Result<Self, AssetError> {
        let t = Self::decode(c)?;
        let script_table = t
            .script_table
            .into_iter()
            .map(|(key, rows)| (pre_tod_script_type(key), rows))
            .collect();
        Ok(Self {
            id: t.id,
            script_table,
        })
    }
}

// ---------------------------------------------------------------------------------------------
// 0x11 GfxObjDegradeInfo
// ---------------------------------------------------------------------------------------------

/// `GfxObjInfo`: exactly 20 bytes, no alignment and no trailing data. The last level's `max_dist`
/// is `FLT_MAX`, a terminator sentinel, preserved rather than dropped. The record is
/// `dereth_primitives`', shared with the animation crate.
pub use dereth_primitives::records::GfxObjInfo;

/// A decoded `0x11` GfxObjDegradeInfo: the distance bands one object degrades through.
#[derive(Debug, Clone, PartialEq)]
pub struct GfxObjDegradeInfo {
    pub id: DataId,
    pub degrades: Vec<GfxObjInfo>,
}

impl Decode for GfxObjDegradeInfo {
    const TYPE: DbType = DbType::DegradeInfo;

    fn declared_id(&self) -> Option<DataId> {
        Some(self.id)
    }

    fn decode(c: &mut Cursor<'_>) -> Result<Self, AssetError> {
        let id = c.data_id()?;
        let n = c.u32()? as usize;
        let degrades = read_n(c, n, |c| {
            Ok(GfxObjInfo {
                gfxobj_id: c.data_id()?,
                degrade_mode: c.i32()?,
                min_dist: c.f32()?,
                ideal_dist: c.f32()?,
                max_dist: c.f32()?,
            })
        })?;
        Ok(Self { id, degrades })
    }
}

// ---------------------------------------------------------------------------------------------
// 0x10 ClothingTable
// ---------------------------------------------------------------------------------------------

/// One texture substitution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextureEffect {
    pub old_texture: DataId,
    pub new_texture: DataId,
}

/// One `{part_num, object_id, texture effects}` triple.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectEffect {
    pub part_num: u32,
    pub object_id: DataId,
    pub texture_effects: Vec<TextureEffect>,
}

/// One clothing-table palette range, in **palette entries** (0 to 2,048; every shipped value is a
/// multiple of 8). The appearance record on the wire carries it divided by 8.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaletteRange {
    pub offset: u32,
    pub length: u32,
}

/// One sub-palette effect: a set of ranges plus the palette set to draw from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaletteEffect {
    pub ranges: Vec<PaletteRange>,
    pub palette_set: DataId,
}

/// One palette-template row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaletteTemplate {
    pub icon: DataId,
    pub subpalette_effects: Vec<PaletteEffect>,
}

/// A decoded `0x10` ClothingTable, with two `PackableHashTable` headers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClothingTable {
    pub id: DataId,
    /// The bucket count of the clothing-base table, kept for round-trip fidelity.
    pub clothing_base_buckets: u32,
    pub clothing_bases: BTreeMap<DataId, Vec<ObjectEffect>>,
    pub palette_template_buckets: u32,
    pub palette_templates: BTreeMap<u32, PaletteTemplate>,
}

impl Decode for ClothingTable {
    const TYPE: DbType = DbType::Clothing;

    fn declared_id(&self) -> Option<DataId> {
        Some(self.id)
    }

    fn decode(c: &mut Cursor<'_>) -> Result<Self, AssetError> {
        let id = c.data_id()?;
        let h = packable_hash_table_header(c)?;
        let mut clothing_bases = BTreeMap::new();
        for _ in 0..h.count {
            let setup = c.data_id()?;
            let ne = c.u32()? as usize;
            let effects = read_n(c, ne, |c| {
                let part_num = c.u32()?;
                let object_id = c.data_id()?;
                let nt = c.u32()? as usize;
                let texture_effects = read_n(c, nt, |c| {
                    Ok(TextureEffect {
                        old_texture: c.data_id()?,
                        new_texture: c.data_id()?,
                    })
                })?;
                Ok(ObjectEffect {
                    part_num,
                    object_id,
                    texture_effects,
                })
            })?;
            clothing_bases.insert(setup, effects);
        }
        let h2 = packable_hash_table_header(c)?;
        let mut palette_templates = BTreeMap::new();
        for _ in 0..h2.count {
            let key = c.u32()?;
            let icon = c.data_id()?;
            let ns = c.u32()? as usize;
            let subpalette_effects = read_n(c, ns, |c| {
                let nr = c.u32()? as usize;
                let ranges = read_n(c, nr, |c| {
                    Ok(PaletteRange {
                        offset: c.u32()?,
                        length: c.u32()?,
                    })
                })?;
                Ok(PaletteEffect {
                    ranges,
                    palette_set: c.data_id()?,
                })
            })?;
            palette_templates.insert(
                key,
                PaletteTemplate {
                    icon,
                    subpalette_effects,
                },
            );
        }
        Ok(Self {
            id,
            clothing_base_buckets: h.buckets,
            clothing_bases,
            palette_template_buckets: h2.buckets,
            palette_templates,
        })
    }

    /// Before Throne of Destiny a palette held 256 colours and a sub-palette range counted them;
    /// the later files count the entries of a palette eight times as long. The ranges are read
    /// into the later count, so a range means the same colours whichever file it came from.
    fn decode_classic(c: &mut Cursor<'_>) -> Result<Self, AssetError> {
        let mut t = Self::decode(c)?;
        for template in t.palette_templates.values_mut() {
            for effect in &mut template.subpalette_effects {
                for range in &mut effect.ranges {
                    range.offset *= CLASSIC_PALETTE_SCALE;
                    range.length *= CLASSIC_PALETTE_SCALE;
                }
            }
        }
        Ok(t)
    }
}

/// How many entries of a later palette one colour of a palette from before Throne of Destiny is.
const CLASSIC_PALETTE_SCALE: u32 = 8;

#[cfg(test)]
mod tests {
    use super::*;

    /// A physics-script table that lists a key twice keeps the first rows listed for it.
    #[test]
    fn a_repeated_physics_script_table_key_keeps_the_first_rows() {
        let mut b = Vec::new();
        b.extend_from_slice(&0x3400_0001u32.to_le_bytes()); // id
        b.extend_from_slice(&2u32.to_le_bytes()); // two keys
        for script in [0x3300_0001u32, 0x3300_0002] {
            b.extend_from_slice(&7u32.to_le_bytes()); // the same key both times
            b.extend_from_slice(&1u32.to_le_bytes()); // one row
            b.extend_from_slice(&1.0f32.to_le_bytes()); // modifier
            b.extend_from_slice(&script.to_le_bytes());
        }
        let t = PhysicsScriptTable::decode(&mut Cursor::new(&b)).expect("decodes");
        assert_eq!(t.script_table.len(), 1);
        assert_eq!(t.script_table[&7][0].script_id, DataId(0x3300_0001));
    }

    /// As bytes: three header bytes, one pad, and the mask in the third.
    ///
    /// Oracle: an independent reader of the shipped dats, which decodes all 436 motion tables with zero
    /// trailing bytes.
    #[test]
    fn motion_data_header_is_three_bytes_plus_one_pad() {
        let mut b = Vec::new();
        b.extend_from_slice(&0x4100_0001u32.to_le_bytes()); // key
        b.push(1); // num_anims
        b.push(0x0A); // bitfield
        b.push(0x03); // flags: velocity and omega both present
        b.push(0xCD); // the pad byte the align skips
        b.extend_from_slice(&0x0300_0001u32.to_le_bytes());
        b.extend_from_slice(&0i32.to_le_bytes());
        b.extend_from_slice(&10i32.to_le_bytes());
        b.extend_from_slice(&30.0f32.to_le_bytes());
        for v in [1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        let mut c = Cursor::new(&b);
        let m = MotionData::decode(&mut c).unwrap();
        c.expect_end().unwrap();
        assert_eq!(m.bitfield, 0x0A);
        assert_eq!(m.flags, 0x03);
        assert_eq!(m.anims.len(), 1);
        assert_eq!(m.velocity, Some(Vec3::new(1.0, 2.0, 3.0)));
        assert_eq!(m.omega, Some(Vec3::new(4.0, 5.0, 6.0)));
    }

    /// A `GfxObjInfo` is exactly 20 bytes with no alignment and no trailing data.
    #[test]
    fn gfxobj_info_is_twenty_bytes_and_keeps_the_flt_max_terminator() {
        let mut b = Vec::new();
        b.extend_from_slice(&0x1100_0000u32.to_le_bytes()); // id
        b.extend_from_slice(&1u32.to_le_bytes()); // one level
        b.extend_from_slice(&0x0100_0001u32.to_le_bytes());
        b.extend_from_slice(&0i32.to_le_bytes());
        b.extend_from_slice(&0.0f32.to_le_bytes());
        b.extend_from_slice(&10.0f32.to_le_bytes());
        b.extend_from_slice(&f32::MAX.to_le_bytes());
        assert_eq!(b.len(), 8 + 20);
        let d = GfxObjDegradeInfo::decode_payload(DataId(0x1100_0000), &b).unwrap();
        assert_eq!(d.degrades.len(), 1);
        assert_eq!(d.degrades[0].max_dist, f32::MAX);
    }
}
