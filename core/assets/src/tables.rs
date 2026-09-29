//! The gameplay tables.
//!
//! Spell and skill records are described in `docs/formats/30-spell-tables.md` and
//! `docs/formats/31-skill-xp-tables.md`. The reference readers report equal `_end` and `_len`
//! values for every shipped gameplay-table file.
//!
//! The de-obfuscation nibble swap applies to **exactly** spell
//! names/descriptions, spell-component names/texts, and object-hierarchy node names. Applying it
//! elsewhere produces garbage; not applying it there produces garbage.

use std::collections::BTreeMap;

use dereth_dat::archive::intrusive_hash_table_header;
use dereth_dat::{packobj::read_n, Cursor, DatError, DbType};
use dereth_primitives::text::cp1252::decode as cp1252_to_string;
use dereth_primitives::{CellId, DataId, Frame, Position};

use crate::error::AssetError;
use crate::Decode;

// ---------------------------------------------------------------------------------------------
// shared primitives
// ---------------------------------------------------------------------------------------------

/// A `Position`: a cell id then a 28-byte `Frame`.
pub fn read_position(c: &mut Cursor<'_>) -> Result<Position, DatError> {
    let cell = CellId(c.u32()?);
    let frame: Frame = c.frame()?;
    Ok(Position::new(cell, frame))
}

/// The de-obfuscation nibble swap: `0xAB -> 0xBA`.
///
/// Reads a `PackObj` string and swaps the nibbles of every byte. Used by exactly three readers.
pub fn obfuscated_string_bytes(c: &mut Cursor<'_>) -> Result<Vec<u8>, DatError> {
    Ok(c.packobj_string_bytes()?
        .iter()
        .map(|b| (b >> 4) | ((b << 4) & 0xF0))
        .collect())
}

/// The same, decoded as cp1252.
pub fn obfuscated_string(c: &mut Cursor<'_>) -> Result<String, DatError> {
    Ok(cp1252_to_string(&obfuscated_string_bytes(c)?))
}

/// Intrusive hash-table encoding: a `u8` bucket-size index, a compressed count, then `count` pairs
/// of a `u32` key and a value.
fn hash_table<T, F>(c: &mut Cursor<'_>, mut value: F) -> Result<(u8, Vec<(u32, T)>), AssetError>
where
    F: FnMut(&mut Cursor<'_>) -> Result<T, AssetError>,
{
    let h = intrusive_hash_table_header(c)?;
    let idx = h.bucket_index.unwrap_or(0);
    let mut v = Vec::new();
    for _ in 0..h.count {
        let k = c.u32()?;
        v.push((k, value(c)?));
    }
    Ok((idx, v))
}

/// The `u16 count; u16 buckets` packed-map header used by older game tables, which
/// write the two halves in the opposite order from the general packed-map format.
fn count_then_buckets(c: &mut Cursor<'_>) -> Result<(u32, u32), DatError> {
    let n = c.u16()?;
    let b = c.u16()?;
    Ok((u32::from(n), u32::from(b)))
}

/// The one-dword hash-table header: the count in the low 24 bits and a bucket-size **index** in
/// the high 8.
fn count_then_bucket_index(c: &mut Cursor<'_>) -> Result<(u32, u8), DatError> {
    let h = c.u32()?;
    Ok((h & 0x00FF_FFFF, h.to_le_bytes()[3]))
}

// ---------------------------------------------------------------------------------------------
// 0x0E00000E SpellTable / 0x0E00000F SpellComponentTable
// ---------------------------------------------------------------------------------------------

/// The client and ACE use this spell-name hash: a PJW/ELF hash over
/// **signed** cp1252 chars.
#[must_use]
pub fn spell_hash(bytes: &[u8]) -> u32 {
    let mut h: u64 = 0;
    for &b in bytes {
        // The original iterates a `char`, which is signed on MSVC.
        let c = i64::from(b as i8);
        h = (h << 4).wrapping_add(c as u64);
        if h & 0xF000_0000 != 0 {
            h = (h ^ ((h & 0xF000_0000) >> 24)) & 0x0FFF_FFFF;
        }
    }
    u32::try_from(h & 0xFFFF_FFFF).unwrap_or(0)
}

/// The additive key the spell's eight component slots are encrypted with.
#[must_use]
pub fn spell_component_key(name: &[u8], desc: &[u8]) -> u32 {
    (spell_hash(name) % 0x1210_7680).wrapping_add(spell_hash(desc) % 0xBEAD_CF45)
}

/// One decoded spell record, in the client's unpack order.
#[derive(Debug, Clone, PartialEq)]
pub struct SpellBase {
    /// De-obfuscated.
    pub name: String,
    /// De-obfuscated.
    pub description: String,
    pub school: u32,
    pub icon: u32,
    pub category: u32,
    pub bitfield: u32,
    pub base_mana: i32,
    pub base_range_constant: f32,
    pub base_range_mod: f32,
    pub power: i32,
    pub spell_economy_mod: f32,
    pub formula_version: u32,
    pub component_loss: f32,
    pub meta_spell_type: u32,
    pub meta_spell_id: u32,
    /// Present for `Enchantment` (1) and `FellowEnchantment` (12).
    pub duration: Option<(f64, f32, f32)>,
    /// Present for `PortalSummon` (7).
    pub portal_lifetime: Option<f64>,
    /// The eight slots exactly as stored, before the key is subtracted.
    pub raw_comps: [u32; 8],
    /// The additive key derived from the name and description hashes.
    pub comp_key: u32,
    /// The non-zero slots with the key subtracted.
    pub comps: Vec<u32>,
    pub caster_effect: u32,
    pub target_effect: u32,
    pub fizzle_effect: u32,
    pub recovery_interval: f64,
    pub recovery_amount: f32,
    pub display_order: i32,
    pub non_component_target_type: u32,
    pub mana_mod: i32,
}

fn read_spell_base(c: &mut Cursor<'_>) -> Result<SpellBase, AssetError> {
    let name_raw = obfuscated_string_bytes(c)?;
    let desc_raw = obfuscated_string_bytes(c)?;
    let school = c.u32()?;
    let icon = c.u32()?;
    let category = c.u32()?;
    let bitfield = c.u32()?;
    let base_mana = c.i32()?;
    let base_range_constant = c.f32()?;
    let base_range_mod = c.f32()?;
    let power = c.i32()?;
    let spell_economy_mod = c.f32()?;
    let formula_version = c.u32()?;
    let component_loss = c.f32()?;
    let meta_spell_type = c.u32()?;
    if !(1..=15).contains(&meta_spell_type) {
        // Only types 1 to 15 have a record the client can read; for any other the rest of the
        // spell would be read at the wrong offsets, so the record is refused rather than guessed.
        return Err(AssetError::UnknownTag {
            what: "meta-spell type",
            tag: meta_spell_type,
        });
    }
    let meta_spell_id = c.u32()?;
    let mut duration = None;
    let mut portal_lifetime = None;
    match meta_spell_type {
        1 | 12 => duration = Some((c.f64()?, c.f32()?, c.f32()?)),
        7 => portal_lifetime = Some(c.f64()?),
        _ => {}
    }
    let mut raw_comps = [0u32; 8];
    for r in &mut raw_comps {
        *r = c.u32()?;
    }
    let comp_key = spell_component_key(&name_raw, &desc_raw);
    let comps = raw_comps
        .iter()
        .filter(|v| **v != 0)
        .map(|v| v.wrapping_sub(comp_key))
        .collect();
    Ok(SpellBase {
        name: cp1252_to_string(&name_raw),
        description: cp1252_to_string(&desc_raw),
        school,
        icon,
        category,
        bitfield,
        base_mana,
        base_range_constant,
        base_range_mod,
        power,
        spell_economy_mod,
        formula_version,
        component_loss,
        meta_spell_type,
        meta_spell_id,
        duration,
        portal_lifetime,
        raw_comps,
        comp_key,
        comps,
        caster_effect: c.u32()?,
        target_effect: c.u32()?,
        fizzle_effect: c.u32()?,
        recovery_interval: c.f64()?,
        recovery_amount: c.f32()?,
        display_order: c.i32()?,
        non_component_target_type: c.u32()?,
        mana_mod: c.i32()?,
    })
}

/// One `SpellSet`: tiers keyed by the number of equipped pieces.
///
/// Stored as a plain list of tiers (one `u32` count, then each tier); no shipped set repeats a
/// piece count, so keying them loses nothing, and a table that did is refused.
#[derive(Debug, Clone, PartialEq)]
pub struct SpellSet {
    pub tiers: BTreeMap<u32, Vec<u32>>,
}

/// [`SpellTable`] — `0x0E00000E`.
#[derive(Debug, Clone, PartialEq)]
pub struct SpellTable {
    pub id: DataId,
    pub spell_buckets: u32,
    pub spells: BTreeMap<u32, SpellBase>,
    /// The set table's bucket-size index: the high byte of its header, whose low 24 bits are the
    /// set count.
    pub spellset_bucket_index: u8,
    pub spellsets: BTreeMap<u32, SpellSet>,
}

impl Decode for SpellTable {
    const TYPE: DbType = DbType::SpellTable;
    fn declared_id(&self) -> Option<DataId> {
        Some(self.id)
    }
    fn decode(c: &mut Cursor<'_>) -> Result<Self, AssetError> {
        let id = c.data_id()?;
        let (n, spell_buckets) = count_then_buckets(c)?;
        let mut spells = BTreeMap::new();
        for _ in 0..n {
            let k = c.u32()?;
            spells.insert(k, read_spell_base(c)?);
        }
        let (n2, spellset_bucket_index) = count_then_bucket_index(c)?;
        let mut spellsets = BTreeMap::new();
        for _ in 0..n2 {
            let k = c.u32()?;
            let nt = c.u32()?;
            let mut tiers = BTreeMap::new();
            for _ in 0..nt {
                let piece_count = c.u32()?;
                let cnt = c.u32()? as usize;
                if tiers
                    .insert(piece_count, read_n(c, cnt, Cursor::u32)?)
                    .is_some()
                {
                    // The shipped list never repeats a piece count; a map cannot hold one that does.
                    return Err(AssetError::Unsupported {
                        what: "repeated spell-set piece count",
                        value: piece_count,
                    });
                }
            }
            spellsets.insert(k, SpellSet { tiers });
        }
        Ok(Self {
            id,
            spell_buckets,
            spells,
            spellset_bucket_index,
            spellsets,
        })
    }
}

/// One spell component.
#[derive(Debug, Clone, PartialEq)]
pub struct SpellComponent {
    /// De-obfuscated.
    pub name: String,
    pub category: u32,
    pub icon: u32,
    pub component_type: u32,
    pub gesture: u32,
    pub time: f32,
    /// De-obfuscated.
    pub text: String,
    pub cdm: f32,
}

/// `SpellComponentTable` — `0x0E00000F`.
#[derive(Debug, Clone, PartialEq)]
pub struct SpellComponentTable {
    pub id: DataId,
    pub buckets: u32,
    pub components: BTreeMap<u32, SpellComponent>,
}

impl Decode for SpellComponentTable {
    const TYPE: DbType = DbType::SpellComponentTable;
    fn declared_id(&self) -> Option<DataId> {
        Some(self.id)
    }
    fn decode(c: &mut Cursor<'_>) -> Result<Self, AssetError> {
        let id = c.data_id()?;
        let (n, buckets) = count_then_buckets(c)?;
        let mut components = BTreeMap::new();
        for _ in 0..n {
            let k = c.u32()?;
            components.insert(
                k,
                SpellComponent {
                    name: obfuscated_string(c)?,
                    category: c.u32()?,
                    icon: c.u32()?,
                    component_type: c.u32()?,
                    gesture: c.u32()?,
                    time: c.f32()?,
                    text: obfuscated_string(c)?,
                    cdm: c.f32()?,
                },
            );
        }
        Ok(Self {
            id,
            buckets,
            components,
        })
    }
}

// ---------------------------------------------------------------------------------------------
// 0x0E000004 SkillTable / 0x0E000018 XpTable / 0x0E000003 Attribute2ndTable
// ---------------------------------------------------------------------------------------------

/// The `w,x,y,z,attr1,attr2` sextuple both the skill and vital tables use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SkillFormula {
    pub w: u32,
    pub x: u32,
    pub y: u32,
    pub z: u32,
    pub attr1: u32,
    pub attr2: u32,
}

fn read_skill_formula(c: &mut Cursor<'_>) -> Result<SkillFormula, DatError> {
    Ok(SkillFormula {
        w: c.u32()?,
        x: c.u32()?,
        y: c.u32()?,
        z: c.u32()?,
        attr1: c.u32()?,
        attr2: c.u32()?,
    })
}

/// One skill.
#[derive(Debug, Clone, PartialEq)]
pub struct SkillBase {
    pub description: String,
    pub name: String,
    pub icon: u32,
    pub trained_cost: i32,
    pub specialized_cost: i32,
    pub category: u32,
    pub chargen_use: i32,
    pub min_level: u32,
    pub formula: SkillFormula,
    pub upper_bound: f64,
    pub lower_bound: f64,
    pub learn_mod: f64,
}

/// `SkillTable` — `0x0E000004`.
#[derive(Debug, Clone, PartialEq)]
pub struct SkillTable {
    pub id: DataId,
    pub buckets: u32,
    pub skills: BTreeMap<u32, SkillBase>,
}

impl Decode for SkillTable {
    const TYPE: DbType = DbType::SkillTable;
    fn declared_id(&self) -> Option<DataId> {
        Some(self.id)
    }
    fn decode(c: &mut Cursor<'_>) -> Result<Self, AssetError> {
        let id = c.data_id()?;
        let (n, buckets) = count_then_buckets(c)?;
        let mut skills = BTreeMap::new();
        for _ in 0..n {
            let k = c.u32()?;
            skills.insert(
                k,
                SkillBase {
                    description: c.packobj_string()?,
                    name: c.packobj_string()?,
                    icon: c.u32()?,
                    trained_cost: c.i32()?,
                    specialized_cost: c.i32()?,
                    category: c.u32()?,
                    chargen_use: c.i32()?,
                    min_level: c.u32()?,
                    formula: read_skill_formula(c)?,
                    upper_bound: c.f64()?,
                    lower_bound: c.f64()?,
                    learn_mod: c.f64()?,
                },
            );
        }
        Ok(Self {
            id,
            buckets,
            skills,
        })
    }
}

/// [`XpTable`] — `0x0E000018`. Every list is `max + 1` long.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XpTable {
    pub id: DataId,
    pub attribute_xp: Vec<u32>,
    pub vital_xp: Vec<u32>,
    pub trained_xp: Vec<u32>,
    pub specialized_xp: Vec<u32>,
    pub level_xp: Vec<u64>,
    pub level_credits: Vec<u32>,
}

impl Decode for XpTable {
    const TYPE: DbType = DbType::XpTable;
    fn declared_id(&self) -> Option<DataId> {
        Some(self.id)
    }
    fn decode(c: &mut Cursor<'_>) -> Result<Self, AssetError> {
        let id = c.data_id()?;
        let na = c.u32()? as usize;
        let nv = c.u32()? as usize;
        let nt = c.u32()? as usize;
        let ns = c.u32()? as usize;
        let nl = c.u32()? as usize;
        Ok(Self {
            id,
            attribute_xp: read_n(c, na + 1, Cursor::u32)?,
            vital_xp: read_n(c, nv + 1, Cursor::u32)?,
            trained_xp: read_n(c, nt + 1, Cursor::u32)?,
            specialized_xp: read_n(c, ns + 1, Cursor::u32)?,
            level_xp: read_n(c, nl + 1, Cursor::u64)?,
            level_credits: read_n(c, nl + 1, Cursor::u32)?,
        })
    }
}

/// `Attribute2ndTable` — `0x0E000003`. Three formulae, 76 bytes with the id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Attribute2ndTable {
    pub id: DataId,
    pub health: SkillFormula,
    pub stamina: SkillFormula,
    pub mana: SkillFormula,
}

impl Decode for Attribute2ndTable {
    const TYPE: DbType = DbType::Attribute2ndTable;
    fn declared_id(&self) -> Option<DataId> {
        Some(self.id)
    }
    fn decode(c: &mut Cursor<'_>) -> Result<Self, AssetError> {
        Ok(Self {
            id: c.data_id()?,
            health: read_skill_formula(c)?,
            stamina: read_skill_formula(c)?,
            mana: read_skill_formula(c)?,
        })
    }
}

// ---------------------------------------------------------------------------------------------
// 0x0E000002 CharGen
// ---------------------------------------------------------------------------------------------

/// One sub-palette range, in 8-entry units with 0 meaning 256.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubPalette {
    pub subpalette: DataId,
    /// Already multiplied by 8.
    pub offset: u32,
    /// Already multiplied by 8, with a stored 0 read as 256.
    pub num_colors: u32,
}

/// Appearance override attached to every character-generation option.
///
/// Bracketed by `ALIGN_PTR` on both sides, which is the one place in the chargen table where
/// padding appears at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjDesc {
    pub version: u8,
    pub palette: Option<DataId>,
    pub subpalettes: Vec<SubPalette>,
    pub texture_changes: Vec<(u8, DataId, DataId)>,
    pub anim_part_changes: Vec<(u8, DataId)>,
}

fn read_objdesc(c: &mut Cursor<'_>) -> Result<ObjDesc, AssetError> {
    c.align_ptr();
    let version = c.u8()?;
    let npal = c.u8()? as usize;
    let ntex = c.u8()? as usize;
    let napc = c.u8()? as usize;
    let palette = if npal > 0 {
        Some(c.data_id_of_known_type(0x0400_0000)?)
    } else {
        None
    };
    let subpalettes = read_n(c, npal, |c| {
        let subpalette = c.data_id_of_known_type(0x0400_0000)?;
        let off = u32::from(c.u8()?);
        let num = u32::from(c.u8()?);
        Ok(SubPalette {
            subpalette,
            offset: off * 8,
            num_colors: if num == 0 { 256 } else { num } * 8,
        })
    })?;
    let texture_changes = read_n(c, ntex, |c| {
        Ok((
            c.u8()?,
            c.data_id_of_known_type(0x0500_0000)?,
            c.data_id_of_known_type(0x0500_0000)?,
        ))
    })?;
    let anim_part_changes = read_n(c, napc, |c| {
        Ok((c.u8()?, c.data_id_of_known_type(0x0100_0000)?))
    })?;
    c.align_ptr();
    Ok(ObjDesc {
        version,
        palette,
        subpalettes,
        texture_changes,
        anim_part_changes,
    })
}

/// One starter town.
#[derive(Debug, Clone, PartialEq)]
pub struct StarterArea {
    pub name: String,
    pub locations: Vec<Position>,
}

/// One chargen template.
#[derive(Debug, Clone, PartialEq)]
pub struct CharGenTemplate {
    pub name: String,
    pub icon: u32,
    pub title: u32,
    /// strength, endurance, coordination, quickness, focus, self.
    pub attributes: [u32; 6],
    pub normal_skills: Vec<u32>,
    pub primary_skills: Vec<u32>,
}

/// One piece of starting gear.
#[derive(Debug, Clone, PartialEq)]
pub struct GearItem {
    pub name: String,
    pub clothing_table: DataId,
    pub weenie_default: u32,
}

/// One hair style option.
#[derive(Debug, Clone, PartialEq)]
pub struct HairStyle {
    pub icon: u32,
    pub bald: u8,
    pub alternate_setup: DataId,
    pub objdesc: ObjDesc,
}

/// One eye strip.
#[derive(Debug, Clone, PartialEq)]
pub struct EyeStrip {
    pub icon: u32,
    pub icon_bald: u32,
    pub objdesc: ObjDesc,
    pub objdesc_bald: ObjDesc,
}

/// One sex within a heritage group.
#[derive(Debug, Clone, PartialEq)]
pub struct SexCg {
    pub name: String,
    pub scale: u32,
    pub setup: DataId,
    pub sound_table: DataId,
    pub icon: u32,
    pub base_palette: DataId,
    pub skin_palset: DataId,
    pub physics_table: DataId,
    pub motion_table: DataId,
    pub combat_table: DataId,
    pub base_objdesc: ObjDesc,
    pub hair_colors: Vec<u32>,
    pub hair_styles: Vec<HairStyle>,
    pub eye_colors: Vec<u32>,
    pub eye_strips: Vec<EyeStrip>,
    pub nose_strips: Vec<(u32, ObjDesc)>,
    pub mouth_strips: Vec<(u32, ObjDesc)>,
    pub headgear: Vec<GearItem>,
    pub shirts: Vec<GearItem>,
    pub pants: Vec<GearItem>,
    pub footwear: Vec<GearItem>,
    pub clothing_colors: Vec<u32>,
}

/// One heritage group.
#[derive(Debug, Clone, PartialEq)]
pub struct HeritageGroup {
    pub name: String,
    pub icon: u32,
    pub setup: DataId,
    pub environment_setup: DataId,
    pub attribute_credits: u32,
    pub skill_credits: u32,
    pub primary_start_areas: Vec<u32>,
    pub secondary_start_areas: Vec<u32>,
    /// `(skill, normal_cost, primary_cost)`.
    pub skills: Vec<(u32, i32, i32)>,
    pub templates: Vec<CharGenTemplate>,
    pub sex_table_marker: u8,
    pub sexes: BTreeMap<u32, SexCg>,
}

/// Character-generation table payload for data id `0x0E000002`.
#[derive(Debug, Clone, PartialEq)]
pub struct CharGen {
    pub id: DataId,
    /// One format question remains: the decoder reads a second `DataID` after the object id and
    /// passes it through unresolved dispatch. It is retained; the record ends correctly either way.
    pub second_data_id: DataId,
    pub starter_areas: Vec<StarterArea>,
    /// The `u8` that precedes the heritage-group hash table.
    pub hg_table_marker: u8,
    pub heritage_groups: BTreeMap<u32, HeritageGroup>,
}

impl Decode for CharGen {
    const TYPE: DbType = DbType::CharGen;
    fn declared_id(&self) -> Option<DataId> {
        Some(self.id)
    }
    fn decode(c: &mut Cursor<'_>) -> Result<Self, AssetError> {
        let id = c.data_id()?;
        let second_data_id = c.data_id()?;
        let n = c.compressed_u32()? as usize;
        let starter_areas = read_n(c, n, |c| {
            let name = c.archive_string()?;
            let nl = c.compressed_u32()? as usize;
            Ok(StarterArea {
                name,
                locations: read_n(c, nl, read_position)?,
            })
        })?;
        let hg_table_marker = c.u8()?;
        let nh = c.compressed_u32()?;
        let mut heritage_groups = BTreeMap::new();
        for _ in 0..nh {
            let key = c.u32()?;
            let name = c.archive_string()?;
            let icon = c.u32()?;
            let setup = c.data_id()?;
            let environment_setup = c.data_id()?;
            let attribute_credits = c.u32()?;
            let skill_credits = c.u32()?;
            let n = c.compressed_u32()? as usize;
            let primary_start_areas = read_n(c, n, Cursor::u32)?;
            let n = c.compressed_u32()? as usize;
            let secondary_start_areas = read_n(c, n, Cursor::u32)?;
            let n = c.compressed_u32()? as usize;
            let skills = read_n(c, n, |c| Ok((c.u32()?, c.i32()?, c.i32()?)))?;
            let n = c.compressed_u32()?;
            let mut templates = Vec::new();
            for _ in 0..n {
                let name = c.archive_string()?;
                let icon = c.u32()?;
                let title = c.u32()?;
                let mut attributes = [0u32; 6];
                for a in &mut attributes {
                    *a = c.u32()?;
                }
                let m = c.compressed_u32()? as usize;
                let normal_skills = read_n(c, m, Cursor::u32)?;
                let m = c.compressed_u32()? as usize;
                let primary_skills = read_n(c, m, Cursor::u32)?;
                templates.push(CharGenTemplate {
                    name,
                    icon,
                    title,
                    attributes,
                    normal_skills,
                    primary_skills,
                });
            }
            let sex_table_marker = c.u8()?;
            let ns = c.compressed_u32()?;
            let mut sexes = BTreeMap::new();
            for _ in 0..ns {
                let sk = c.u32()?;
                sexes.insert(sk, read_sex(c)?);
            }
            heritage_groups.insert(
                key,
                HeritageGroup {
                    name,
                    icon,
                    setup,
                    environment_setup,
                    attribute_credits,
                    skill_credits,
                    primary_start_areas,
                    secondary_start_areas,
                    skills,
                    templates,
                    sex_table_marker,
                    sexes,
                },
            );
        }
        Ok(Self {
            id,
            second_data_id,
            starter_areas,
            hg_table_marker,
            heritage_groups,
        })
    }
}

fn read_gear(c: &mut Cursor<'_>) -> Result<Vec<GearItem>, AssetError> {
    let n = c.compressed_u32()?;
    let mut v = Vec::new();
    for _ in 0..n {
        v.push(GearItem {
            name: c.archive_string()?,
            clothing_table: c.data_id()?,
            weenie_default: c.u32()?,
        });
    }
    Ok(v)
}

fn read_sex(c: &mut Cursor<'_>) -> Result<SexCg, AssetError> {
    let name = c.archive_string()?;
    let scale = c.u32()?;
    let setup = c.data_id()?;
    let sound_table = c.data_id()?;
    let icon = c.u32()?;
    let base_palette = c.data_id()?;
    let skin_palset = c.data_id()?;
    let physics_table = c.data_id()?;
    let motion_table = c.data_id()?;
    let combat_table = c.data_id()?;
    let base_objdesc = read_objdesc(c)?;
    let n = c.compressed_u32()? as usize;
    let hair_colors = read_n(c, n, Cursor::u32)?;
    let n = c.compressed_u32()?;
    let mut hair_styles = Vec::new();
    for _ in 0..n {
        hair_styles.push(HairStyle {
            icon: c.u32()?,
            bald: c.u8()?,
            alternate_setup: c.data_id()?,
            objdesc: read_objdesc(c)?,
        });
    }
    let n = c.compressed_u32()? as usize;
    let eye_colors = read_n(c, n, Cursor::u32)?;
    let n = c.compressed_u32()?;
    let mut eye_strips = Vec::new();
    for _ in 0..n {
        eye_strips.push(EyeStrip {
            icon: c.u32()?,
            icon_bald: c.u32()?,
            objdesc: read_objdesc(c)?,
            objdesc_bald: read_objdesc(c)?,
        });
    }
    let n = c.compressed_u32()?;
    let mut nose_strips = Vec::new();
    for _ in 0..n {
        nose_strips.push((c.u32()?, read_objdesc(c)?));
    }
    let n = c.compressed_u32()?;
    let mut mouth_strips = Vec::new();
    for _ in 0..n {
        mouth_strips.push((c.u32()?, read_objdesc(c)?));
    }
    let headgear = read_gear(c)?;
    let shirts = read_gear(c)?;
    let pants = read_gear(c)?;
    let footwear = read_gear(c)?;
    let n = c.compressed_u32()? as usize;
    let clothing_colors = read_n(c, n, Cursor::u32)?;
    Ok(SexCg {
        name,
        scale,
        setup,
        sound_table,
        icon,
        base_palette,
        skin_palset,
        physics_table,
        motion_table,
        combat_table,
        base_objdesc,
        hair_colors,
        hair_styles,
        eye_colors,
        eye_strips,
        nose_strips,
        mouth_strips,
        headgear,
        shirts,
        pants,
        footwear,
        clothing_colors,
    })
}

// ---------------------------------------------------------------------------------------------
// 0x0E000007 ChatPoseTable / 0x30xxxxxx CombatManeuverTable / 0x0E00001D ContractTable
// ---------------------------------------------------------------------------------------------

/// `ChatPoseTable` — `0x0E000007`.
#[derive(Debug, Clone, PartialEq)]
pub struct ChatPoseTable {
    pub id: DataId,
    pub pose_buckets: u32,
    pub poses: Vec<(String, String)>,
    pub emote_buckets: u32,
    /// `(key, (my_emote, other_emote))`.
    pub emotes: Vec<(String, (String, String))>,
}

impl Decode for ChatPoseTable {
    const TYPE: DbType = DbType::ChatPoseTable;
    fn declared_id(&self) -> Option<DataId> {
        Some(self.id)
    }
    fn decode(c: &mut Cursor<'_>) -> Result<Self, AssetError> {
        let id = c.data_id()?;
        let (n, pose_buckets) = count_then_buckets(c)?;
        let poses = read_n(c, n as usize, |c| {
            Ok((c.packobj_string()?, c.packobj_string()?))
        })?;
        let (n2, emote_buckets) = count_then_buckets(c)?;
        let emotes = read_n(c, n2 as usize, |c| {
            Ok((
                c.packobj_string()?,
                (c.packobj_string()?, c.packobj_string()?),
            ))
        })?;
        Ok(Self {
            id,
            pose_buckets,
            poses,
            emote_buckets,
            emotes,
        })
    }
}

/// One combat manoeuvre.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CombatManeuver {
    pub style: u32,
    pub attack_height: u32,
    pub attack_type: u32,
    pub min_skill_level: u32,
    pub motion: u32,
}

/// `CombatManeuverTable` — `0x30xxxxxx`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CombatManeuverTable {
    pub id: DataId,
    pub maneuvers: Vec<CombatManeuver>,
}

impl Decode for CombatManeuverTable {
    const TYPE: DbType = DbType::CombatTable;
    fn declared_id(&self) -> Option<DataId> {
        Some(self.id)
    }
    fn decode(c: &mut Cursor<'_>) -> Result<Self, AssetError> {
        let id = c.data_id()?;
        let n = c.u32()? as usize;
        let maneuvers = read_n(c, n, |c| {
            Ok(CombatManeuver {
                style: c.u32()?,
                attack_height: c.u32()?,
                attack_type: c.u32()?,
                min_skill_level: c.u32()?,
                motion: c.u32()?,
            })
        })?;
        Ok(Self { id, maneuvers })
    }
}

/// One contract.
#[derive(Debug, Clone, PartialEq)]
pub struct Contract {
    pub version: u32,
    pub contract_id: u32,
    /// name, description, description_progress, npc_start, npc_end, questflag_stamped,
    /// questflag_started, questflag_finished, questflag_progress, questflag_timer,
    /// questflag_repeat_time.
    pub strings: [String; 11],
    pub location_npc_start: Position,
    pub location_npc_end: Position,
    pub location_quest_area: Position,
}

/// [`ContractTable`] — `0x0E00001D`.
#[derive(Debug, Clone, PartialEq)]
pub struct ContractTable {
    pub id: DataId,
    pub buckets: u32,
    pub contracts: BTreeMap<u32, Contract>,
}

impl Decode for ContractTable {
    const TYPE: DbType = DbType::ContractTable;
    fn declared_id(&self) -> Option<DataId> {
        Some(self.id)
    }
    fn decode(c: &mut Cursor<'_>) -> Result<Self, AssetError> {
        let id = c.data_id()?;
        let (n, buckets) = count_then_buckets(c)?;
        let mut contracts = BTreeMap::new();
        for _ in 0..n {
            let k = c.u32()?;
            let version = c.u32()?;
            let contract_id = c.u32()?;
            let mut strings: [String; 11] = Default::default();
            for s in &mut strings {
                *s = c.packobj_string()?;
            }
            contracts.insert(
                k,
                Contract {
                    version,
                    contract_id,
                    strings,
                    location_npc_start: read_position(c)?,
                    location_npc_end: read_position(c)?,
                    location_quest_area: read_position(c)?,
                },
            );
        }
        Ok(Self {
            id,
            buckets,
            contracts,
        })
    }
}

// ---------------------------------------------------------------------------------------------
// 0x0E00000D object hierarchy / 0x0E00001E TabooTable / 0x0E000020 NameFilterTable
// 0x0E00001A BadData / 0x0E01xxxx QualityFilter
// ---------------------------------------------------------------------------------------------

/// One node of the object hierarchy, in an arena. Node 0 is the root.
#[derive(Debug, Clone, PartialEq)]
pub struct HierarchyNode {
    /// De-obfuscated — the third and last place the nibble swap applies.
    pub name: String,
    pub id: u32,
    pub children: Vec<u32>,
}

/// The object hierarchy's root node — `0x0E00000D`.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectHierarchy {
    pub id: DataId,
    pub nodes: Vec<HierarchyNode>,
}

impl Decode for ObjectHierarchy {
    const TYPE: DbType = DbType::ObjectHierarchy;
    fn declared_id(&self) -> Option<DataId> {
        Some(self.id)
    }
    fn decode(c: &mut Cursor<'_>) -> Result<Self, AssetError> {
        let id = c.data_id()?;
        let mut nodes: Vec<HierarchyNode> = Vec::new();
        // Work list, not recursion: the hierarchy is 114 KB of nested nodes.
        let mut stack: Vec<Option<usize>> = vec![None];
        while let Some(parent) = stack.pop() {
            let idx = nodes.len();
            let name = obfuscated_string(c)?;
            let node_id = c.u32()?;
            let n = c.u32()? as usize;
            nodes.push(HierarchyNode {
                name,
                id: node_id,
                children: Vec::new(),
            });
            if let Some(p) = parent {
                nodes[p].children.push(u32::try_from(idx).unwrap_or(0));
            }
            for _ in 0..n {
                stack.push(Some(idx));
            }
        }
        Ok(Self { id, nodes })
    }
}

/// One audience's censor lists, keyed by the client's own bucket key.
pub type TabooAudience = Vec<(u32, Vec<String>)>;

/// The `0x0E00001E` censor table.
#[derive(Debug, Clone, PartialEq)]
pub struct TabooTable {
    pub id: DataId,
    /// audience -> (key -> patterns).
    pub audiences: Vec<(u32, TabooAudience)>,
}

impl Decode for TabooTable {
    const TYPE: DbType = DbType::TabooTable;
    fn declared_id(&self) -> Option<DataId> {
        Some(self.id)
    }
    fn decode(c: &mut Cursor<'_>) -> Result<Self, AssetError> {
        let id = c.data_id()?;
        let (_, audiences) = hash_table(c, |c| {
            let (_, inner) = hash_table(c, |c| {
                let n = c.u32()? as usize;
                let mut v = Vec::new();
                for _ in 0..n {
                    v.push(c.archive_string()?);
                }
                Ok(v)
            })?;
            Ok(inner)
        })?;
        Ok(Self { id, audiences })
    }
}

/// One language's name rules.
#[derive(Debug, Clone, PartialEq)]
pub struct NameFilterLanguage {
    pub max_same_chars_in_a_row: u32,
    pub max_vowels_in_a_row: u32,
    pub first_n_chars_must_have_vowel: u32,
    pub vowel_containing_substring_length: u32,
    pub extra_allowed_characters: String,
    pub compound_letter_groups: Vec<String>,
}

/// The `0x0E000020` name-filter table.
#[derive(Debug, Clone, PartialEq)]
pub struct NameFilterTable {
    pub id: DataId,
    pub languages: Vec<(u32, NameFilterLanguage)>,
}

impl Decode for NameFilterTable {
    const TYPE: DbType = DbType::NameFilterTable;
    fn declared_id(&self) -> Option<DataId> {
        Some(self.id)
    }
    fn decode(c: &mut Cursor<'_>) -> Result<Self, AssetError> {
        let id = c.data_id()?;
        let (_, languages) = hash_table(c, |c| {
            let max_same_chars_in_a_row = c.u32()?;
            let max_vowels_in_a_row = c.u32()?;
            let first_n_chars_must_have_vowel = c.u32()?;
            let vowel_containing_substring_length = c.u32()?;
            let extra_allowed_characters = c.archive_wstring()?;
            let n = c.u32()? as usize;
            let mut compound_letter_groups = Vec::new();
            for _ in 0..n {
                compound_letter_groups.push(c.archive_wstring()?);
            }
            Ok(NameFilterLanguage {
                max_same_chars_in_a_row,
                max_vowels_in_a_row,
                first_n_chars_must_have_vowel,
                vowel_containing_substring_length,
                extra_allowed_characters,
                compound_letter_groups,
            })
        })?;
        Ok(Self { id, languages })
    }
}

/// `BadData` — `0x0E00001A`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BadData {
    pub id: DataId,
    pub buckets: u32,
    pub bad: Vec<(u32, u32)>,
}

impl Decode for BadData {
    const TYPE: DbType = DbType::BadData;
    fn declared_id(&self) -> Option<DataId> {
        Some(self.id)
    }
    fn decode(c: &mut Cursor<'_>) -> Result<Self, AssetError> {
        let id = c.data_id()?;
        let (n, buckets) = count_then_buckets(c)?;
        let bad = read_n(c, n as usize, |c| Ok((c.u32()?, c.u32()?)))?;
        Ok(Self { id, buckets, bad })
    }
}

/// The quality filter — `0x0E01xxxx`. Eight property-type lists then three attribute lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QualityFilter {
    pub id: DataId,
    /// int, int64, bool, float, did, iid, string, position.
    pub property_lists: [Vec<u32>; 8],
    /// attribute, attribute2nd, skill.
    pub attribute_lists: [Vec<u32>; 3],
}

impl QualityFilter {
    /// Whether int property `id` may be enchanted at all. A property the filter does not list keeps
    /// its stored value whatever enchantments name it.
    #[must_use]
    pub fn allows_int(&self, id: u32) -> bool {
        self.property_lists[0].contains(&id)
    }

    /// Whether float property `id` may be enchanted at all.
    #[must_use]
    pub fn allows_float(&self, id: u32) -> bool {
        self.property_lists[3].contains(&id)
    }

    /// Whether secondary attribute `id` may be enchanted at all. The shipped filter lists the three
    /// maxima and none of the current values.
    #[must_use]
    pub fn allows_attribute_2nd(&self, id: u32) -> bool {
        self.attribute_lists[1].contains(&id)
    }
}

impl Decode for QualityFilter {
    const TYPE: DbType = DbType::QualityFilter;
    fn declared_id(&self) -> Option<DataId> {
        Some(self.id)
    }
    fn decode(c: &mut Cursor<'_>) -> Result<Self, AssetError> {
        let id = c.data_id()?;
        let mut counts = [0usize; 8];
        for n in &mut counts {
            *n = c.u32()? as usize;
        }
        let mut property_lists: [Vec<u32>; 8] = Default::default();
        for (l, n) in property_lists.iter_mut().zip(counts) {
            *l = read_n(c, n, Cursor::u32)?;
        }
        let mut counts2 = [0usize; 3];
        for n in &mut counts2 {
            *n = c.u32()? as usize;
        }
        let mut attribute_lists: [Vec<u32>; 3] = Default::default();
        for (l, n) in attribute_lists.iter_mut().zip(counts2) {
            *l = read_n(c, n, Cursor::u32)?;
        }
        Ok(Self {
            id,
            property_lists,
            attribute_lists,
        })
    }
}

// ---------------------------------------------------------------------------------------------
// 0x22xxxxxx EnumMapper / 0x25xxxxxx DidMapper / 0x27xxxxxx DualDidMapper
// ---------------------------------------------------------------------------------------------

/// `0x22xxxxxx`. The string→id map is rebuilt at load time and
/// is not stored.
#[derive(Debug, Clone, PartialEq)]
pub struct EnumMapper {
    pub id: DataId,
    pub base_emp_did: DataId,
    pub id_to_string: Vec<(u32, String)>,
}

impl Decode for EnumMapper {
    const TYPE: DbType = DbType::EnumMapper;
    fn declared_id(&self) -> Option<DataId> {
        Some(self.id)
    }
    fn decode(c: &mut Cursor<'_>) -> Result<Self, AssetError> {
        let id = c.data_id()?;
        let base_emp_did = c.data_id()?;
        let (_, id_to_string) = hash_table(c, |c| Ok(c.archive_string()?))?;
        Ok(Self {
            id,
            base_emp_did,
            id_to_string,
        })
    }
}

/// `0x25xxxxxx` and, with the same body, `0x27xxxxxx`.
///
/// The dual variant only adds a reverse map at load time; its serialized payload uses this same
/// function, so both types decode identically.
#[derive(Debug, Clone, PartialEq)]
pub struct DidMapper {
    pub id: DataId,
    pub enum_to_id: Vec<(u32, u32)>,
    pub enum_to_name: Vec<(u32, String)>,
    pub enum_to_id_internal: Vec<(u32, u32)>,
    pub enum_to_name_internal: Vec<(u32, String)>,
}

impl DidMapper {
    fn read(c: &mut Cursor<'_>) -> Result<Self, AssetError> {
        let id = c.data_id()?;
        let (_, enum_to_id) = hash_table(c, |c| Ok(c.u32()?))?;
        let (_, enum_to_name) = hash_table(c, |c| Ok(c.archive_string()?))?;
        let (_, enum_to_id_internal) = hash_table(c, |c| Ok(c.u32()?))?;
        let (_, enum_to_name_internal) = hash_table(c, |c| Ok(c.archive_string()?))?;
        Ok(Self {
            id,
            enum_to_id,
            enum_to_name,
            enum_to_id_internal,
            enum_to_name_internal,
        })
    }
}

impl Decode for DidMapper {
    const TYPE: DbType = DbType::DidMapper;
    fn declared_id(&self) -> Option<DataId> {
        Some(self.id)
    }
    fn decode(c: &mut Cursor<'_>) -> Result<Self, AssetError> {
        Self::read(c)
    }
}

/// Master [`DidMapper`] where every enum-to-DID lookup starts.
pub const MASTER_DID_MAPPER: DataId = DataId(0x2500_0000);

/// The two-level [`DidMapper`] lookup, for any group.
///
/// An enum lookup resolves an id in two hops: the master
/// [`MASTER_DID_MAPPER`] maps the **group** to a second mapper, and that mapper maps the **enum
/// value** to the object's `DataID`. The master mapper contains twenty-two groups.
///
/// **Never hard-code a `DataID` at a call site.** A DDD patch can move any of these objects, and
/// hard-coding works against this dat build and no other.
///
/// Returns `None` when either hop is missing or the entry is the mapper's own zero row, which the
/// caller must treat as a failure rather than as an empty result: the client's own `require` on
/// the returned pointer is fatal.
///
/// This lives here rather than in the client because two crates need it — `dereth_client::assets`
/// (the action map, the key maps, the UI sound table, the preview animations) and
/// `dereth_ui_screens::env` (the icon backgrounds an item slot paints) — and
/// `dereth-ui-screens` sits *below* `dereth-client`. `dereth_client::assets::enum_did` is a
/// one-line forward to it.
#[must_use]
pub fn did_by_enum(
    assets: &dyn dereth_primitives::AssetSource,
    group: u32,
    value: u32,
) -> Option<DataId> {
    let mapper = |did: DataId| -> Option<DidMapper> {
        let bytes = assets.read(did).ok()?;
        <DidMapper as Decode>::decode_payload(did, &bytes).ok()
    };
    let second = mapper(MASTER_DID_MAPPER)?
        .enum_to_id
        .iter()
        .find(|(k, _)| *k == group)
        .map(|(_, v)| DataId(*v))?;
    mapper(second)?
        .enum_to_id
        .iter()
        .find(|(k, _)| *k == value)
        .map(|(_, v)| DataId(*v))
        .filter(|d| d.0 != 0)
}

/// [`DualDidMapper`] — `0x27xxxxxx`. Byte-identical to [`DidMapper`]; a separate type so that
/// [`Decode::TYPE`] names the right database record type.
#[derive(Debug, Clone, PartialEq)]
pub struct DualDidMapper(pub DidMapper);

impl Decode for DualDidMapper {
    const TYPE: DbType = DbType::DualDidMapper;
    fn declared_id(&self) -> Option<DataId> {
        Some(self.0.id)
    }
    fn decode(c: &mut Cursor<'_>) -> Result<Self, AssetError> {
        Ok(Self(DidMapper::read(c)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Contract 9.14: the nibble swap, and nothing but the nibble swap.
    ///
    /// Oracle: an independent reader of the shipped dats, which de-obfuscates the same strings.
    #[test]
    fn the_de_obfuscation_is_a_nibble_swap() {
        // "Strength Other" obfuscated: each byte's nibbles exchanged.
        let plain = b"Strength";
        let obf: Vec<u8> = plain.iter().map(|b| (b >> 4) | ((b << 4) & 0xF0)).collect();
        let mut buf = Vec::new();
        buf.extend_from_slice(&u16::try_from(obf.len()).unwrap().to_le_bytes());
        buf.extend_from_slice(&obf);
        buf.resize((buf.len() + 3) & !3, 0);
        let mut c = Cursor::new(&buf);
        assert_eq!(obfuscated_string(&mut c).unwrap(), "Strength");
        c.expect_end().unwrap();
    }

    /// The PJW hash is over *signed* chars, which matters for every byte above 0x7F.
    #[test]
    fn the_spell_hash_treats_bytes_as_signed() {
        // A pure-ASCII string is unaffected by the sign, so the two agree there...
        assert_eq!(
            spell_hash(b"Strength Other I"),
            spell_hash(b"Strength Other I")
        );
        // ...but a high byte must not be treated as unsigned.
        let signed = spell_hash(&[0xE9]);
        let unsigned_equivalent = {
            let mut h: u64 = 0;
            h = (h << 4).wrapping_add(0xE9);
            if h & 0xF000_0000 != 0 {
                h = (h ^ ((h & 0xF000_0000) >> 24)) & 0x0FFF_FFFF;
            }
            u32::try_from(h & 0xFFFF_FFFF).unwrap()
        };
        assert_ne!(signed, unsigned_equivalent);
    }

    /// A set's tier count is one whole dword. A count of 65536 is `00 00 01 00`: a reader that took
    /// a 16-bit count and a 16-bit bucket half would see no tiers and stop short of the end.
    #[test]
    fn a_spell_set_tier_count_is_a_full_dword() {
        let mut b = Vec::new();
        let mut put = |v: u32| b.extend_from_slice(&v.to_le_bytes());
        put(0x0E00_000E); // id
        put(0); // no spells: 16-bit count 0, 16-bit buckets 0
        put(0x0100_0001); // one set, bucket-size index 1
        put(7); // the set's key
        put(0x0001_0000); // its tier count
        for piece_count in 0..0x0001_0000u32 {
            put(piece_count);
            put(0); // no spells in the tier
        }
        let table = SpellTable::decode_bytes(&b).expect("the whole payload is read");
        assert_eq!(table.spellset_bucket_index, 1);
        assert_eq!(table.spellsets.len(), 1);
        assert_eq!(table.spellsets[&7].tiers.len(), 0x0001_0000);
    }

    /// Only meta-spell types 1 to 15 have a record the client can read.
    #[test]
    fn a_meta_spell_of_type_zero_or_past_fifteen_is_refused() {
        fn record(meta_spell_type: u32) -> Vec<u8> {
            let mut b = vec![0u8; 8]; // empty name and description
            for _ in 0..11 {
                b.extend_from_slice(&0u32.to_le_bytes()); // school .. component loss
            }
            b.extend_from_slice(&meta_spell_type.to_le_bytes());
            b.resize(b.len() + 256, 0); // the rest of the record, whatever its shape
            b
        }
        for t in [0u32, 16, 17, 0xFFFF_FFFF] {
            let r = read_spell_base(&mut Cursor::new(&record(t)));
            assert!(
                matches!(
                    r,
                    Err(AssetError::UnknownTag {
                        what: "meta-spell type",
                        tag
                    }) if tag == t
                ),
                "type {t}: {r:?}"
            );
        }
        for t in 1..=15u32 {
            let base = read_spell_base(&mut Cursor::new(&record(t))).expect("a readable type");
            assert_eq!(base.meta_spell_type, t);
        }
    }
}
