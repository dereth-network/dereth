//! The character-creation tables of the pre-Throne-of-Destiny portal.
//!
//! - The creation table (`0x0E000002`): eight help-string ids, the starting areas and the
//!   heritages, each heritage with its sexes, and each sex with its templates, skill discounts,
//!   palettes, hair, eye, nose and mouth strips, and four clothing lists.
//! - The skill table (`0x0E000004`): every skill's name, description, icon, costs, the
//!   level it is offered at and its attribute formula.
//! - A clothing table (`0x10`) per clothing item: the colours it can be dyed, of which a sex offers
//!   those its colour list names.
//! - The string records (`0x31`): the help text, keyed by the record id in decimal.
//!
//! Strings come in two shapes. A *text* is a 32-bit length and the bytes, unless that length is
//! more than `0xFFFF`, in which case it is the other shape read from the same place: a *legacy
//! text*, a 16-bit length (`0xFFFF` then a 32-bit length for a long one), the bytes, and padding to
//! four. Both are Windows-1252. An object description is kept as the lowercase hex of its bytes,
//! padding included; a face strip also records which texture it swaps in (`%08X`, with `-mirror`
//! for the eye strips, which are drawn mirrored).

use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use dereth_primitives::text::cp1252;
use serde::{Deserialize, Serialize};

use crate::appearance::{self, IndexedTexture};
use crate::reader::Cursor;
use crate::ClassicPortal;

/// The creation table's id.
pub const CREATION_TABLE: u32 = 0x0E00_0002;
/// The skill table's id.
pub const SKILL_TABLE: u32 = 0x0E00_0004;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Named {
    pub name: String,
    pub icon: u32,
    pub resource: u32,
    #[serde(default)]
    pub colors: Vec<ClothingColor>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClothingColor {
    pub key: u32,
    pub icon: u32,
    /// The palette set the colour's first sub-palette is drawn from, or zero when it has none.
    /// Its palettes, darkest to lightest, are the colour's shades.
    #[serde(default)]
    pub palette_set: u32,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Profile {
    pub attributes: [i32; 6],
    pub trained: Vec<u32>,
    pub specialized: Vec<u32>,
    pub legacy_third: Vec<u32>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Template {
    pub name: String,
    pub icon: u32,
    pub resource: u32,
    pub profiles: Vec<Profile>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Strip {
    pub icon: u32,
    pub appearance: String,
    #[serde(default)]
    pub bald: u32,
    #[serde(default)]
    pub bald_appearance: String,
    #[serde(default)]
    pub texture: String,
    #[serde(default)]
    pub bald_texture: String,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Discount {
    pub skill: u32,
    pub trained: i32,
    pub specialized: i32,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Sex {
    #[serde(default)]
    pub legacy_18: u32,
    #[serde(default)]
    pub legacy_1c: u32,
    pub name: String,
    pub setup: u32,
    pub sound: u32,
    pub icon: u32,
    pub naming_help: u32,
    pub appearance: String,
    pub attribute_credits: i32,
    pub skill_credits: i32,
    pub legacy_60: i32,
    pub legacy_68: Vec<Named>,
    pub legacy_80: Vec<Vec<u32>>,
    pub templates: Vec<Template>,
    pub skill_discounts: Vec<Discount>,
    pub base_palette: u32,
    pub skin_palette: u32,
    pub hair_colors: Vec<u32>,
    pub eye_colors: Vec<u32>,
    pub hair_styles: Vec<Strip>,
    pub eyes: Vec<Strip>,
    pub noses: Vec<Strip>,
    pub mouths: Vec<Strip>,
    pub headgear: Vec<Named>,
    pub shirts: Vec<Named>,
    pub trousers: Vec<Named>,
    pub footwear: Vec<Named>,
    pub clothing_colors: Vec<u32>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Heritage {
    pub name: String,
    pub icon: u32,
    pub setup: u32,
    pub description: u32,
    pub environment: u32,
    pub primary_areas: Vec<usize>,
    pub secondary_areas: Vec<usize>,
    pub sexes: Vec<Sex>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Area {
    pub name: String,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Skill {
    pub id: u32,
    pub name: String,
    pub description: String,
    pub icon: u32,
    pub trained: i32,
    pub specialized: i32,
    pub chargen: u32,
    pub min_level: i32,
    pub formula: [u32; 6],
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppearanceData {
    pub palettes: BTreeMap<String, Vec<[u8; 4]>>,
    pub palette_sets: BTreeMap<String, Vec<u32>>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CreationData {
    #[serde(default)]
    pub appearance: AppearanceData,
    pub heritages: Vec<Heritage>,
    pub areas: Vec<Area>,
    #[serde(default)]
    pub help_ids: Vec<u32>,
    #[serde(default)]
    pub skills: Vec<Skill>,
    #[serde(default)]
    pub help_text: BTreeMap<String, String>,
    #[serde(default)]
    pub text_heights: BTreeMap<String, i32>,
}

/// The indexed face textures the creation screens draw, keyed `%08X`, or `%08X-mirror` for the
/// mirrored eye strips.
#[derive(Debug, Clone, Default)]
pub struct IndexedAssets {
    pub textures: BTreeMap<String, IndexedTexture>,
}

/// A 32-bit field that the tables treat as signed.
#[allow(clippy::cast_possible_wrap)]
fn signed(v: u32) -> i32 {
    v as i32
}

/// The table cursor: the strings and lists the creation tables are made of.
struct Tables<'a>(Cursor<'a>);

impl Tables<'_> {
    fn u32(&mut self) -> Result<u32, String> {
        self.0.u32()
    }

    fn i32(&mut self) -> Result<i32, String> {
        self.0.u32().map(signed)
    }

    fn words(&mut self, count: usize) -> Result<Vec<u32>, String> {
        (0..count).map(|_| self.u32()).collect()
    }

    /// A 32-bit count, then that many items. A count larger than the bytes left is refused
    /// before anything is allocated for it.
    fn items<T>(
        &mut self,
        mut read: impl FnMut(&mut Self) -> Result<T, String>,
    ) -> Result<Vec<T>, String> {
        let count = self.u32()? as usize;
        if count > self.0.remaining() {
            return Err(format!(
                "unbounded creation array at offset {}",
                self.0.pos()
            ));
        }
        (0..count).map(|_| read(self)).collect()
    }

    fn legacy_text(&mut self) -> Result<String, String> {
        let mut size = u32::from(self.0.u16()?);
        if size == 0xFFFF {
            size = self.u32()?;
        }
        let bytes = self.0.take_n(size, 1)?;
        self.0.align()?;
        Ok(cp1252::decode(bytes))
    }

    fn text(&mut self) -> Result<String, String> {
        let start = self.0.pos();
        let size = self.u32()?;
        if size <= 0xFFFF {
            return Ok(cp1252::decode(self.0.take_n(size, 1)?));
        }
        self.0.rewind(start);
        self.legacy_text()
    }

    fn appearance(&mut self) -> Result<String, String> {
        self.0.align()?;
        let start = self.0.pos();
        self.0.objdesc()?;
        Ok(hex(self.0.since(start)))
    }

    fn named(&mut self) -> Result<Named, String> {
        Ok(Named {
            name: self.text()?,
            icon: self.u32()?,
            resource: self.u32()?,
            colors: Vec::new(),
        })
    }

    fn profile(&mut self) -> Result<Profile, String> {
        let mut attributes = [0i32; 6];
        for a in &mut attributes {
            *a = self.i32()?;
        }
        Ok(Profile {
            attributes,
            trained: self.items(Self::u32)?,
            specialized: self.items(Self::u32)?,
            legacy_third: self.items(Self::u32)?,
        })
    }

    fn template(&mut self) -> Result<Template, String> {
        let Named {
            name,
            icon,
            resource,
            ..
        } = self.named()?;
        Ok(Template {
            name,
            icon,
            resource,
            profiles: self.items(Self::profile)?,
        })
    }

    fn face(&mut self) -> Result<Strip, String> {
        Ok(Strip {
            icon: self.u32()?,
            appearance: self.appearance()?,
            ..Strip::default()
        })
    }

    fn sex(&mut self) -> Result<Sex, String> {
        let mut s = Sex {
            name: self.text()?,
            setup: self.u32()?,
            sound: self.u32()?,
            icon: self.u32()?,
            naming_help: self.u32()?,
            appearance: self.appearance()?,
            legacy_18: self.u32()?,
            legacy_1c: self.u32()?,
            ..Sex::default()
        };
        // A list of names, then a list of name pairs with a value: read and not kept.
        self.items(Self::text)?;
        self.items(|r| {
            r.text()?;
            r.text()?;
            r.u32()
        })?;
        s.attribute_credits = self.i32()?;
        s.legacy_60 = self.i32()?;
        s.skill_credits = self.i32()?;
        s.legacy_68 = self.items(Self::named)?;
        s.skill_discounts = self.items(|r| {
            Ok(Discount {
                skill: r.u32()?,
                trained: r.i32()?,
                specialized: r.i32()?,
            })
        })?;
        s.templates = self.items(Self::template)?;
        s.legacy_80 = self.items(|r| r.words(17))?;
        s.base_palette = self.u32()?;
        s.skin_palette = self.u32()?;
        s.hair_colors = self.items(Self::u32)?;
        s.hair_styles = self.items(|r| {
            Ok(Strip {
                icon: r.u32()?,
                bald: r.u32()?,
                appearance: r.appearance()?,
                ..Strip::default()
            })
        })?;
        s.eye_colors = self.items(Self::u32)?;
        s.eyes = self.items(|r| {
            let icon = r.u32()?;
            let _bald_icon = r.u32()?;
            Ok(Strip {
                icon,
                appearance: r.appearance()?,
                bald_appearance: r.appearance()?,
                ..Strip::default()
            })
        })?;
        s.noses = self.items(Self::face)?;
        s.mouths = self.items(Self::face)?;
        s.headgear = self.items(Self::named)?;
        s.shirts = self.items(Self::named)?;
        s.trousers = self.items(Self::named)?;
        s.footwear = self.items(Self::named)?;
        let count = self.u32()? as usize;
        let _maximum = self.u32()?;
        s.clothing_colors = self.words(count)?;
        Ok(s)
    }

    fn heritage(&mut self) -> Result<Heritage, String> {
        let index = |r: &mut Self| r.u32().map(|v| v as usize);
        Ok(Heritage {
            name: self.text()?,
            icon: self.u32()?,
            setup: self.u32()?,
            description: self.u32()?,
            environment: self.u32()?,
            primary_areas: self.items(index)?,
            secondary_areas: self.items(index)?,
            sexes: self.items(Self::sex)?,
        })
    }

    fn area(&mut self) -> Result<Area, String> {
        let name = self.text()?;
        // Each starting position: a cell and a seven-float frame, read and not kept.
        self.items(|r| {
            r.u32()?;
            r.0.take(28).map(|_| ())
        })?;
        Ok(Area { name })
    }
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

fn unhex(s: &str) -> Result<Vec<u8>, String> {
    if !s.len().is_multiple_of(2) {
        return Err("odd-length hex".into());
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|e| e.to_string()))
        .collect()
}

/// Decode the creation table (`0x0E000002`) alone: help ids, areas and heritages.
///
/// # Errors
///
/// Not the creation table, or a payload the fields do not consume exactly.
pub fn decode_table(data: &[u8]) -> Result<CreationData, String> {
    let mut r = Tables(Cursor::new(data));
    if r.u32()? != CREATION_TABLE {
        return Err("not the creation table".into());
    }
    let help_ids = r.words(8)?;
    let areas = r.items(Tables::area)?;
    let heritages = r.items(Tables::heritage)?;
    r.0.end()?;
    Ok(CreationData {
        help_ids,
        areas,
        heritages,
        ..CreationData::default()
    })
}

/// Decode the skill table (`0x0E000004`).
///
/// # Errors
///
/// Not the skill table, or a payload the fields do not consume exactly.
pub fn decode_skills(data: &[u8]) -> Result<Vec<Skill>, String> {
    let mut r = Tables(Cursor::new(data));
    if r.u32()? != SKILL_TABLE {
        return Err("not the skill table".into());
    }
    let count = r.u32()? & 0xFFFF;
    let mut out = Vec::new();
    for _ in 0..count {
        let id = r.u32()?;
        let description = r.legacy_text()?;
        let name = r.legacy_text()?;
        let icon = r.u32()?;
        let trained = r.i32()?;
        let specialized = r.i32()?;
        let _category = r.u32()?;
        let chargen = r.u32()?;
        let min_level = r.i32()?;
        let mut formula = [0u32; 6];
        for f in &mut formula {
            *f = r.u32()?;
        }
        // Upper bound, lower bound and learning modifier: read and not kept.
        for _ in 0..3 {
            r.0.f64()?;
        }
        out.push(Skill {
            id,
            name,
            description,
            icon,
            trained,
            specialized,
            chargen,
            min_level,
            formula,
        });
    }
    r.0.end()?;
    Ok(out)
}

/// The colours of one clothing table (`0x10`): each colour's key, swatch icon and the palette set of
/// its first sub-palette, in table order.
///
/// # Errors
///
/// Not a clothing table, or a payload the fields do not consume exactly.
pub fn clothing_colors(data: &[u8]) -> Result<Vec<ClothingColor>, String> {
    let mut r = Tables(Cursor::new(data));
    if r.u32()? >> 24 != 0x10 {
        return Err("not a clothing table".into());
    }
    for _ in 0..r.u32()? & 0xFFFF {
        r.u32()?;
        for _ in 0..r.u32()? {
            r.0.take(8)?;
            let n = r.u32()?;
            r.0.take_n(n, 8)?;
        }
    }
    let mut colors = Vec::new();
    for _ in 0..r.u32()? & 0xFFFF {
        let mut color = ClothingColor {
            key: r.u32()?,
            icon: r.u32()?,
            palette_set: 0,
        };
        // Each sub-palette: the palette ranges it recolours, then the palette set it draws from.
        for i in 0..r.u32()? {
            let n = r.u32()?;
            r.0.take_n(n, 8)?;
            let set = r.u32()?;
            if i == 0 {
                color.palette_set = set;
            }
        }
        colors.push(color);
    }
    r.0.end()?;
    Ok(colors)
}

/// One string record (`0x31`): its id echoed, then one legacy text.
///
/// # Errors
///
/// The id does not echo, or the payload is not exactly one legacy text.
pub fn decode_string(id: u32, data: &[u8]) -> Result<String, String> {
    let mut r = Tables(Cursor::new(data));
    if r.u32()? != id {
        return Err(format!("string record {id:08X} does not echo its id"));
    }
    let text = r.legacy_text()?;
    r.0.end()?;
    Ok(text)
}

/// The texture key a strip's description names, or empty when it names none.
fn texture_key(objdesc_hex: &str, mirror: bool) -> Result<String, String> {
    let id = appearance::first_texture(&unhex(objdesc_hex)?)?;
    Ok(match id {
        0 => String::new(),
        _ if mirror => format!("{id:08X}-mirror"),
        _ => format!("{id:08X}"),
    })
}

/// The id a texture key names, and whether it is the mirrored form.
fn parse_key(key: &str) -> Result<(u32, bool), String> {
    let (id, mirror) = match key.strip_suffix("-mirror") {
        Some(id) => (id, true),
        None => (key, false),
    };
    u32::from_str_radix(id, 16)
        .map(|id| (id, mirror))
        .map_err(|e| format!("texture key {key}: {e}"))
}

/// Every face texture the strips name, with the mirrored form of each eye texture beside the plain
/// one.
fn wanted_textures(data: &CreationData) -> Result<BTreeSet<(u32, bool)>, String> {
    let mut out = BTreeSet::new();
    for sex in data.heritages.iter().flat_map(|h| &h.sexes) {
        for strip in sex.eyes.iter().chain(&sex.noses).chain(&sex.mouths) {
            for key in [&strip.texture, &strip.bald_texture] {
                if key.is_empty() {
                    continue;
                }
                let (id, mirror) = parse_key(key)?;
                out.insert((id, false));
                if mirror {
                    out.insert((id, true));
                }
            }
        }
    }
    Ok(out)
}

/// Read every creation table from the portal: help ids, areas, heritages with their clothing
/// colours and face textures, skills, help text, and the palettes and palette sets the creation
/// screens colour with.
///
/// # Errors
///
/// A table missing from the portal or failing to decode.
pub fn read(portal: &ClassicPortal) -> Result<CreationData, String> {
    let mut data = decode_table(&portal.require(CREATION_TABLE)?)?;
    data.skills = decode_skills(&portal.require(SKILL_TABLE)?)?;

    // A clothing item's colours: those of its clothing table the sex offers, by key.
    let mut tables: BTreeMap<u32, Vec<ClothingColor>> = BTreeMap::new();
    for sex in data.heritages.iter_mut().flat_map(|h| &mut h.sexes) {
        let offered: BTreeSet<u32> = sex.clothing_colors.iter().copied().collect();
        for item in sex
            .headgear
            .iter_mut()
            .chain(&mut sex.shirts)
            .chain(&mut sex.trousers)
            .chain(&mut sex.footwear)
        {
            let table = match tables.entry(item.icon) {
                Entry::Occupied(o) => o.into_mut(),
                Entry::Vacant(v) => v.insert(
                    clothing_colors(&portal.require(item.icon)?)
                        .map_err(|e| format!("clothing table {:08X}: {e}", item.icon))?,
                ),
            };
            let mut colors: Vec<ClothingColor> = table
                .iter()
                .filter(|c| offered.contains(&c.key))
                .cloned()
                .collect();
            colors.sort_by_key(|c| c.key);
            item.colors = colors;
        }
    }

    for id in portal.ids().into_iter().filter(|id| id >> 24 == 0x31) {
        let text = decode_string(id, &portal.require(id)?)?;
        data.help_text.insert(id.to_string(), text);
    }

    // Face strips name their textures; eyes are drawn mirrored.
    let mut palettes = BTreeSet::new();
    let mut sets = BTreeSet::new();
    for sex in data.heritages.iter_mut().flat_map(|h| &mut h.sexes) {
        palettes.insert(sex.base_palette);
        palettes.extend(sex.eye_colors.iter().copied());
        sets.insert(sex.skin_palette);
        sets.extend(sex.hair_colors.iter().copied());
        for item in sex
            .headgear
            .iter()
            .chain(&sex.shirts)
            .chain(&sex.trousers)
            .chain(&sex.footwear)
        {
            sets.extend(item.colors.iter().map(|c| c.palette_set));
        }
        for strip in &mut sex.eyes {
            strip.texture = texture_key(&strip.appearance, true)?;
            strip.bald_texture = texture_key(&strip.bald_appearance, true)?;
        }
        for strip in sex.noses.iter_mut().chain(&mut sex.mouths) {
            strip.texture = texture_key(&strip.appearance, false)?;
        }
    }
    for (id, _) in wanted_textures(&data)? {
        palettes.insert(appearance::indexed(&portal.require(id)?)?.palette);
    }
    sets.remove(&0);
    for id in sets {
        let values = appearance::palette_set(&portal.require(id)?)?;
        palettes.extend(values.iter().copied());
        data.appearance
            .palette_sets
            .insert(format!("{id:08X}"), values);
    }
    palettes.remove(&0);
    for id in palettes {
        let colors = appearance::palette(&portal.require(id)?)
            .map_err(|e| format!("palette {id:08X}: {e}"))?;
        data.appearance.palettes.insert(format!("{id:08X}"), colors);
    }
    Ok(data)
}

/// The indexed face textures `data`'s strips name, each eye texture also mirrored.
///
/// # Errors
///
/// A strip names a texture key that does not parse, or a texture missing or failing to decode.
pub fn indexed_assets(
    portal: &ClassicPortal,
    data: &CreationData,
) -> Result<IndexedAssets, String> {
    let mut textures = BTreeMap::new();
    for (id, mirror) in wanted_textures(data)? {
        let t = appearance::indexed(&portal.require(id)?)
            .map_err(|e| format!("texture {id:08X}: {e}"))?;
        if mirror {
            textures.insert(format!("{id:08X}-mirror"), appearance::mirrored(&t));
        } else {
            textures.insert(format!("{id:08X}"), t);
        }
    }
    Ok(IndexedAssets { textures })
}
