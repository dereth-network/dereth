//! A weenie's look: the object description a server sends for it, worked out from its properties
//! and the data files.
//!
//! The description is built in layers:
//!
//! 1. **The face and skin** a character-like object names: its head model (or the character
//!    creation table's hair style for its heritage and sex), its base palette, and its hair, skin
//!    and eye palettes and face textures. A creature that names a heritage and sex but not all of
//!    its face has the rest rolled when it is made; here each roll is [`Face::FIXED`], so one
//!    weenie always shows one face.
//! 2. **Its clothing look** (data id 7): the clothing table's part swaps and texture swaps for the
//!    object's setup, recolored with the table's color scheme (int 3) at the shade (float 12) it
//!    names. A scheme the table does not have, or none, takes the scheme the table lists first; an
//!    object that names neither a scheme nor a shade keeps the table's own colors.
//! 3. **What a creature wears**, over its face: each worn piece's clothing look for the creature's
//!    setup, in the order the pieces layer, then the setup's own parts wherever nothing covers the
//!    body. A creature that wears nothing and lists its own part, texture or palette changes shows
//!    those instead; one that wears nothing and has a clothing look is dressed in it as any object.
//!
//! This is the working of ACE (and of the servers built on it), so a preview made with it shows
//! what a player of such a server sees. [`object_description`] answers the description as the
//! client receives it: every id whole, as the client reads it off the wire.
//!
//! What a creature wears is chosen from what its create list wields ([`dress`]): its underwear
//! (pants first, then a shirt and a cloak), then its outerwear slot by slot, each piece only where
//! nothing it would cover is worn already, twice over as the server equips a creature twice.
//! Weapons and shields are held, not worn, and add nothing here.
//!
//! Nothing here reads a file: the tables come through [`LookFiles`].

use std::cmp::Ordering;
use std::sync::Arc;

use dereth_assets::motion::{ClothingTable, PaletteTemplate};
use dereth_assets::tables::CharGen;
use dereth_protocol::types::{AnimPartChange, ObjDesc, Subpalette, TextureMapChange};

/// The property ids a look is read from.
pub mod id {
    /// Data ids.
    pub const SETUP: u32 = 1;
    pub const PALETTE_BASE: u32 = 6;
    pub const CLOTHING_BASE: u32 = 7;
    pub const EYES_TEXTURE: u32 = 9;
    pub const NOSE_TEXTURE: u32 = 10;
    pub const MOUTH_TEXTURE: u32 = 11;
    pub const DEFAULT_EYES_TEXTURE: u32 = 12;
    pub const DEFAULT_NOSE_TEXTURE: u32 = 13;
    pub const DEFAULT_MOUTH_TEXTURE: u32 = 14;
    pub const HAIR_PALETTE: u32 = 15;
    pub const EYES_PALETTE: u32 = 16;
    pub const SKIN_PALETTE: u32 = 17;
    pub const HEAD_OBJECT: u32 = 18;
    /// Ints.
    pub const ITEM_TYPE: u32 = 1;
    pub const PALETTE_TEMPLATE: u32 = 3;
    pub const CLOTHING_PRIORITY: u32 = 4;
    pub const VALID_LOCATIONS: u32 = 9;
    pub const ARMOR_LEVEL: u32 = 28;
    pub const GENDER: u32 = 113;
    pub const HERITAGE: u32 = 188;
    pub const HAIR_STYLE: u32 = 9012;
    /// Floats.
    pub const SHADE: u32 = 12;
    pub const DEFAULT_SCALE: u32 = 39;
    /// Bools.
    pub const TOP_LAYER_PRIORITY: u32 = 123;
    /// Strings.
    pub const SEX: u32 = 3;
    pub const HERITAGE_NAME: u32 = 4;
}

/// The weenie types whose objects are creatures, and dress as creatures do: creature, admin,
/// vendor, cow, sentinel, game piece, pet and combat pet.
pub const CREATURE_TYPES: [u32; 8] = [10, 11, 12, 15, 41, 61, 69, 71];

/// The corpse's weenie type: it shows its own changes when it lists any.
const CORPSE_TYPE: u32 = 14;

/// The clothing weenie type, armor among it: the pieces a creature puts on.
const CLOTHING_TYPE: u32 = 2;

/// Gear knights' heritage, whose head model stands in for no head.
const GEAR_KNIGHT: i32 = 6;

/// The head's part.
const HEAD: u8 = 0x10;

/// The setup whose clothing parts say which body parts a worn piece covers as it layers.
const HUMAN_MALE: u32 = 0x0200_0001;

/// Item types.
const ITEM_ARMOR: u32 = 2;
const ITEM_CLOTHING: u32 = 4;

/// Where an item is worn (`EquipMask`).
mod at {
    pub const HEAD_WEAR: u32 = 0x0000_0001;
    pub const CHEST_WEAR: u32 = 0x0000_0002;
    pub const UPPER_ARM_WEAR: u32 = 0x0000_0008;
    pub const LOWER_ARM_WEAR: u32 = 0x0000_0010;
    pub const HAND_WEAR: u32 = 0x0000_0020;
    pub const UPPER_LEG_WEAR: u32 = 0x0000_0040;
    pub const LOWER_LEG_WEAR: u32 = 0x0000_0080;
    pub const FOOT_WEAR: u32 = 0x0000_0100;
    pub const CHEST_ARMOR: u32 = 0x0000_0200;
    pub const ABDOMEN_ARMOR: u32 = 0x0000_0400;
    pub const UPPER_ARM_ARMOR: u32 = 0x0000_0800;
    pub const LOWER_ARM_ARMOR: u32 = 0x0000_1000;
    pub const UPPER_LEG_ARMOR: u32 = 0x0000_2000;
    pub const LOWER_LEG_ARMOR: u32 = 0x0000_4000;
    pub const CLOAK: u32 = 0x0800_0000;
    /// The weapon slots, each by itself.
    pub const WEAPON_SLOTS: [u32; 5] = [
        0x0010_0000,
        0x0100_0000,
        0x0200_0000,
        0x0020_0000,
        0x0040_0000,
    ];
    pub const CLOTHING: u32 = 0x8000_0000 | 0x1FF;
    pub const ARMOR: u32 = CHEST_ARMOR
        | ABDOMEN_ARMOR
        | UPPER_ARM_ARMOR
        | LOWER_ARM_ARMOR
        | UPPER_LEG_ARMOR
        | LOWER_LEG_ARMOR
        | FOOT_WEAR;
    pub const EXTREMITY: u32 = HEAD_WEAR | HAND_WEAR | FOOT_WEAR;
}

/// What a piece covers (`CoverageMask`).
mod covers {
    pub const UNDERWEAR_UPPER_LEGS: u32 = 0x0000_0002;
    pub const UNDERWEAR_LOWER_LEGS: u32 = 0x0000_0004;
    pub const UNDERWEAR_CHEST: u32 = 0x0000_0008;
    pub const UNDERWEAR_ABDOMEN: u32 = 0x0000_0010;
    pub const UNDERWEAR_UPPER_ARMS: u32 = 0x0000_0020;
    pub const UNDERWEAR_LOWER_ARMS: u32 = 0x0000_0040;
    pub const OUTERWEAR_UPPER_LEGS: u32 = 0x0000_0100;
    pub const OUTERWEAR_LOWER_LEGS: u32 = 0x0000_0200;
    pub const OUTERWEAR_CHEST: u32 = 0x0000_0400;
    pub const OUTERWEAR_ABDOMEN: u32 = 0x0000_0800;
    pub const OUTERWEAR_UPPER_ARMS: u32 = 0x0000_1000;
    pub const OUTERWEAR_LOWER_ARMS: u32 = 0x0000_2000;
    pub const HEAD: u32 = 0x0000_4000;
    pub const HANDS: u32 = 0x0000_8000;
    pub const FEET: u32 = 0x0001_0000;
    pub const UNDERWEAR: u32 = UNDERWEAR_UPPER_LEGS
        | UNDERWEAR_LOWER_LEGS
        | UNDERWEAR_CHEST
        | UNDERWEAR_ABDOMEN
        | UNDERWEAR_UPPER_ARMS
        | UNDERWEAR_LOWER_ARMS;
    pub const OUTERWEAR: u32 = OUTERWEAR_UPPER_LEGS
        | OUTERWEAR_LOWER_LEGS
        | OUTERWEAR_CHEST
        | OUTERWEAR_ABDOMEN
        | OUTERWEAR_UPPER_ARMS
        | OUTERWEAR_LOWER_ARMS
        | HEAD
        | HANDS
        | FEET;
    pub const SHIRT: u32 = UNDERWEAR_CHEST | UNDERWEAR_UPPER_ARMS | UNDERWEAR_LOWER_ARMS;
    pub const PANTS: u32 = UNDERWEAR_UPPER_LEGS | UNDERWEAR_LOWER_LEGS;
}

/// The bodies whose clothing is looked up on another body of their heritage and sex: the
/// alternate setups the clothing tables do not list. One of the eighteen is written in the
/// server's own table as `0x2001A6EE`, which no setup is, so that body keeps its own.
const CLOTHED_AS: [(u32, u32); 18] = [
    (0x0200_1972, 0x0200_196F),
    (0x0200_1A5F, 0x0200_196F),
    (0x0200_1A6F, 0x0200_196F),
    (0x0200_1A5E, 0x0200_1970),
    (0x2001_A6EE, 0x0200_1970),
    (0x0200_1971, 0x0200_196E),
    (0x0200_1A5D, 0x0200_196E),
    (0x0200_1A70, 0x0200_196E),
    (0x0200_1A5C, 0x0200_196D),
    (0x0200_1A71, 0x0200_196D),
    (0x0200_1A0F, 0x0200_1A0E),
    (0x0200_1A9C, 0x0200_1A0E),
    (0x0200_1A9E, 0x0200_1A0E),
    (0x0200_1A9D, 0x0200_1A0E),
    (0x0200_1A96, 0x0200_1A0E),
    (0x0200_1A0D, 0x0200_1A0C),
    (0x0200_1AA0, 0x0200_1A0C),
    (0x0200_1A9F, 0x0200_1A0C),
];

/// The two more that wear the human bodies' clothing, and the two undead bodies left out of
/// [`CLOTHED_AS`] for length.
const CLOTHED_AS_MORE: [(u32, u32); 4] = [
    (0x0200_1AA1, 0x0200_1A0C),
    (0x0200_1AA2, 0x0200_1A0C),
    (0x0200_1AA3, HUMAN_MALE),
    (0x0200_1AA4, 0x0200_004E),
];

/// The body whose clothing `setup` wears.
fn clothed_as(setup: u32) -> u32 {
    CLOTHED_AS
        .iter()
        .chain(&CLOTHED_AS_MORE)
        .find(|(from, _)| *from == setup)
        .map_or(setup, |(_, to)| *to)
}

/// A weenie's properties, as a look reads them.
pub trait Properties {
    fn int(&self, id: u32) -> Option<i32>;
    fn float(&self, id: u32) -> Option<f64>;
    fn data_id(&self, id: u32) -> Option<u32>;
    fn bool(&self, id: u32) -> Option<bool>;
    fn string(&self, id: u32) -> Option<String>;
}

/// The data files a look is read from.
pub trait LookFiles {
    /// Clothing table `id`; `None` for one the files do not have, which dresses nothing.
    fn clothing(&self, id: u32) -> Option<Arc<ClothingTable>>;
    /// The color scheme clothing table `id` lists first in its file: the one taken for a scheme
    /// it does not have.
    fn first_palette_template(&self, id: u32) -> Option<u32>;
    /// The palettes of palette set `id`, in order; empty for one the files do not have.
    fn palette_set(&self, id: u32) -> Vec<u32>;
    /// The parts of setup `id`, in order; empty for one the files do not have.
    fn setup_parts(&self, id: u32) -> Vec<u32>;
    /// The character creation table.
    fn char_gen(&self) -> Option<&CharGen>;
}

/// A weenie's own part, texture and palette changes (its anim part, texture map and palette rows).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OwnChanges {
    /// `(part, model)`.
    pub parts: Vec<(u8, u32)>,
    /// `(part, old texture, new texture)`.
    pub textures: Vec<(u8, u32, u32)>,
    /// `(palette, offset, length)`, the offset and length in eights of a palette's entries.
    pub palettes: Vec<(u32, u16, u16)>,
}

impl OwnChanges {
    fn is_empty(&self) -> bool {
        self.parts.is_empty() && self.textures.is_empty() && self.palettes.is_empty()
    }
}

/// What a weenie's look is made from.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Look {
    pub weenie_type: u32,
    pub setup: Option<u32>,
    pub clothing_base: Option<u32>,
    pub palette_template: Option<i32>,
    pub shade: Option<f64>,
    pub palette_base: Option<u32>,
    pub head: Option<u32>,
    pub hair_style: Option<i32>,
    pub heritage: Option<i32>,
    pub gender: Option<i32>,
    /// The heritage and sex by name, which a creature's face is rolled from when it names no
    /// number for them.
    pub heritage_name: Option<String>,
    pub sex: Option<String>,
    pub hair_palette: Option<u32>,
    pub skin_palette: Option<u32>,
    pub eyes_palette: Option<u32>,
    /// Each face texture: the setup's own, and the one put over it.
    pub eyes: (Option<u32>, Option<u32>),
    pub nose: (Option<u32>, Option<u32>),
    pub mouth: (Option<u32>, Option<u32>),
    pub own: OwnChanges,
}

impl Look {
    /// The look of a weenie of type `weenie_type` with properties `p` and its own changes `own`.
    #[must_use]
    pub fn of(weenie_type: u32, p: &dyn Properties, own: OwnChanges) -> Self {
        Self {
            weenie_type,
            setup: p.data_id(id::SETUP),
            clothing_base: p.data_id(id::CLOTHING_BASE),
            palette_template: p.int(id::PALETTE_TEMPLATE),
            shade: p.float(id::SHADE),
            palette_base: p.data_id(id::PALETTE_BASE),
            head: p.data_id(id::HEAD_OBJECT),
            hair_style: p.int(id::HAIR_STYLE),
            heritage: p.int(id::HERITAGE),
            gender: p.int(id::GENDER),
            heritage_name: p.string(id::HERITAGE_NAME),
            sex: p.string(id::SEX),
            hair_palette: p.data_id(id::HAIR_PALETTE),
            skin_palette: p.data_id(id::SKIN_PALETTE),
            eyes_palette: p.data_id(id::EYES_PALETTE),
            eyes: (
                p.data_id(id::DEFAULT_EYES_TEXTURE),
                p.data_id(id::EYES_TEXTURE),
            ),
            nose: (
                p.data_id(id::DEFAULT_NOSE_TEXTURE),
                p.data_id(id::NOSE_TEXTURE),
            ),
            mouth: (
                p.data_id(id::DEFAULT_MOUTH_TEXTURE),
                p.data_id(id::MOUTH_TEXTURE),
            ),
            own,
        }
    }

    /// Whether it dresses as a creature.
    #[must_use]
    pub fn is_creature(&self) -> bool {
        CREATURE_TYPES.contains(&self.weenie_type)
    }
}

/// The rolls of a creature's face: each choice's place in the character creation table's list
/// for its heritage and sex, and the two shades.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Face {
    pub hair_style: usize,
    pub hair_color: usize,
    pub hair_shade: f64,
    pub eye_color: usize,
    pub eyes: usize,
    pub nose: usize,
    pub mouth: usize,
    pub skin_shade: f64,
}

impl Face {
    /// The face a preview shows: every choice the first, the shades halfway.
    pub const FIXED: Self = Self {
        hair_style: 0,
        hair_color: 0,
        hair_shade: 0.5,
        eye_color: 0,
        eyes: 0,
        nose: 0,
        mouth: 0,
        skin_shade: 0.5,
    };
}

/// The heritages by the names a weenie gives them, in their numbers' order from 1.
const HERITAGE_NAMES: [&str; 13] = [
    "aluvian",
    "gharundim",
    "sho",
    "viamontian",
    "shadowbound",
    "gearknight",
    "tumerok",
    "lugian",
    "empyrean",
    "penumbraen",
    "undead",
    "olthoi",
    "olthoiacid",
];

/// The number of a heritage or a sex named `name` (any case; a heritage's apostrophes ignored).
fn number_named(name: &str, names: &[&str]) -> Option<i32> {
    let n = name.replace('\'', "").to_ascii_lowercase();
    if let Ok(v) = n.trim().parse::<i32>() {
        return Some(v);
    }
    names
        .iter()
        .position(|x| *x == n.trim())
        .and_then(|i| i32::try_from(i + 1).ok())
}

/// `look` with the face a creature is made with: its heritage and sex taken from their names when
/// it names no number, and every part of its face it does not name rolled as `face` says.
fn rolled(look: &Look, face: &Face, files: &dyn LookFiles) -> Look {
    let mut l = look.clone();
    if l.heritage.is_none() {
        l.heritage = l
            .heritage_name
            .as_deref()
            .and_then(|n| number_named(n, &HERITAGE_NAMES));
    }
    if l.gender.is_none() {
        l.gender = l
            .sex
            .as_deref()
            .and_then(|n| number_named(n, &["male", "female"]));
    }
    let (Some(h), Some(g)) = (l.heritage, l.gender) else {
        return l;
    };
    let Some(sex) = files.char_gen().and_then(|cg| {
        cg.heritage_groups
            .get(&u32::try_from(h).ok()?)?
            .sexes
            .get(&u32::try_from(g).ok()?)
    }) else {
        return l;
    };
    l.palette_base = Some(sex.base_palette.0);
    let pick = |i: usize, n: usize| i.min(n.saturating_sub(1));
    let Some(style) = sex
        .hair_styles
        .get(pick(face.hair_style, sex.hair_styles.len()))
    else {
        return l;
    };
    let first_texture = |od: &dereth_assets::tables::ObjDesc| {
        od.texture_changes
            .first()
            .map(|(_, old, new)| (old.0, new.0))
    };
    if let Some(strip) = sex.eye_strips.get(pick(face.eyes, sex.eye_strips.len())) {
        let od = if style.bald != 0 {
            &strip.objdesc_bald
        } else {
            &strip.objdesc
        };
        if let Some((old, new)) = first_texture(od) {
            l.eyes = (l.eyes.0.or(Some(old)), l.eyes.1.or(Some(new)));
        }
    }
    if let Some((_, od)) = sex.nose_strips.get(pick(face.nose, sex.nose_strips.len())) {
        if let Some((old, new)) = first_texture(od) {
            l.nose = (l.nose.0.or(Some(old)), l.nose.1.or(Some(new)));
        }
    }
    if let Some((_, od)) = sex
        .mouth_strips
        .get(pick(face.mouth, sex.mouth_strips.len()))
    {
        if let Some((old, new)) = first_texture(od) {
            l.mouth = (l.mouth.0.or(Some(old)), l.mouth.1.or(Some(new)));
        }
    }
    // A hair style of one part names the head it puts on; the bodies whose style is several
    // parts name none.
    if l.head.is_none() && style.objdesc.anim_part_changes.len() == 1 {
        l.head = Some(style.objdesc.anim_part_changes[0].1 .0);
    }
    if l.skin_palette.is_none() {
        l.skin_palette = Some(palette_at(
            &files.palette_set(sex.skin_palset.0),
            face.skin_shade,
        ));
    }
    if l.hair_palette.is_none() {
        if let Some(set) = sex
            .hair_colors
            .get(pick(face.hair_color, sex.hair_colors.len()))
        {
            l.hair_palette = Some(palette_at(&files.palette_set(*set), face.hair_shade));
        }
    }
    if l.eyes_palette.is_none() {
        l.eyes_palette = sex
            .eye_colors
            .get(pick(face.eye_color, sex.eye_colors.len()))
            .copied();
    }
    l
}

/// A piece a creature's create list wields, as the server makes it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Wield {
    pub weenie_type: u32,
    pub item_type: u32,
    /// Where it may be worn.
    pub locations: Option<u32>,
    /// What it covers.
    pub coverage: Option<u32>,
    /// Whether it goes over (true) or under (false) the rest.
    pub top_layer: Option<bool>,
    pub armor_level: Option<i32>,
    pub setup: Option<u32>,
    pub clothing_base: Option<u32>,
    pub palette_template: Option<i32>,
    pub shade: Option<f64>,
}

impl Wield {
    /// A piece of type `weenie_type` with properties `p`, as its weenie has it.
    #[must_use]
    pub fn of(weenie_type: u32, p: &dyn Properties) -> Self {
        Self {
            weenie_type,
            item_type: p
                .int(id::ITEM_TYPE)
                .map_or(0, |n| u32::from_ne_bytes(n.to_ne_bytes())),
            locations: p
                .int(id::VALID_LOCATIONS)
                .map(|n| u32::from_ne_bytes(n.to_ne_bytes())),
            coverage: p
                .int(id::CLOTHING_PRIORITY)
                .map(|n| u32::from_ne_bytes(n.to_ne_bytes())),
            top_layer: p.bool(id::TOP_LAYER_PRIORITY),
            armor_level: p.int(id::ARMOR_LEVEL),
            setup: p.data_id(id::SETUP),
            clothing_base: p.data_id(id::CLOTHING_BASE),
            palette_template: p.int(id::PALETTE_TEMPLATE),
            shade: p.float(id::SHADE),
        }
    }

    /// The piece as a create list's row makes it: the row's color scheme over the weenie's when
    /// it names one (above 0), and for a row that is no treasure, the row's shade always (for a
    /// treasure row the column is the row's chance).
    #[must_use]
    pub fn made(mut self, palette: i32, shade: f32, treasure: bool) -> Self {
        if palette > 0 {
            self.palette_template = Some(palette);
        }
        if !treasure {
            self.shade = Some(f64::from(shade));
        }
        self
    }
}

/// A piece a creature wears, and where.
#[derive(Debug, Clone, PartialEq)]
pub struct Worn {
    pub piece: Wield,
    pub location: u32,
}

/// The palette a palette set gives at `shade`: the set's palettes spread evenly from 0 to 1, a
/// shade outside that giving none (0).
#[must_use]
pub fn palette_at(palettes: &[u32], shade: f64) -> u32 {
    if palettes.is_empty() || !(0.0..=1.0).contains(&shade) {
        return 0;
    }
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    // The product is in [0, len) for a shade in [0, 1]; the index is clamped all the same.
    let i = ((palettes.len() as f64 - 0.000_001) * shade) as usize;
    palettes[i.min(palettes.len() - 1)]
}

/// The color scheme clothing table `table` (whose id is `id`) dresses in for scheme `key`: that
/// one when it has it, else the one it lists first. `None` when it has none.
#[must_use]
pub fn scheme_of<'a>(
    table: &'a ClothingTable,
    id: u32,
    key: Option<i32>,
    files: &dyn LookFiles,
) -> Option<(u32, &'a PaletteTemplate)> {
    let asked = u32::from_ne_bytes(key.unwrap_or(0).to_ne_bytes());
    if let Some(t) = table.palette_templates.get(&asked) {
        return Some((asked, t));
    }
    let first = files
        .first_palette_template(id)
        .filter(|k| table.palette_templates.contains_key(k))
        .or_else(|| table.palette_templates.keys().next().copied())?;
    table.palette_templates.get(&first).map(|t| (first, t))
}

/// The object description being built, its ids as the server holds them before they are sent.
#[derive(Debug, Default)]
struct Built {
    palette: u32,
    /// `(palette, offset, length)`.
    subpalettes: Vec<(u32, u16, u16)>,
    /// `(part, old, new)`.
    textures: Vec<(u8, u32, u32)>,
    /// `(part, model)`.
    parts: Vec<(u8, u32)>,
}

impl Built {
    /// A part swap, moved to the end if the same swap is there already.
    fn add_part(&mut self, part: u8, model: u32) {
        if let Some(i) = self.parts.iter().position(|p| *p == (part, model)) {
            self.parts.remove(i);
        }
        self.parts.push((part, model));
    }

    /// A texture swap, unless the same one is there already.
    fn add_texture(&mut self, t: (u8, u32, u32)) {
        if !self.textures.contains(&t) {
            self.textures.push(t);
        }
    }

    /// Clothing table `id`'s palette changes for scheme `key` at `shade`.
    fn recolor(
        &mut self,
        table: &ClothingTable,
        id: u32,
        key: Option<i32>,
        shade: Option<f64>,
        files: &dyn LookFiles,
    ) {
        let Some((_, scheme)) = scheme_of(table, id, key, files) else {
            return;
        };
        // The shade is kept in single precision on the way.
        #[allow(clippy::cast_possible_truncation)]
        let shade = f64::from(shade.map_or(0.0, |s| s as f32));
        for effect in &scheme.subpalette_effects {
            // The palette's id travels in sixteen bits.
            let palette = palette_at(&files.palette_set(effect.palette_set.0), shade) & 0xFFFF;
            for r in &effect.ranges {
                self.subpalettes.push((
                    palette,
                    u16::try_from(r.offset / 8).unwrap_or(u16::MAX),
                    u16::try_from(r.length / 8).unwrap_or(u16::MAX),
                ));
            }
        }
    }

    /// The description as the client reads it off the wire: each id whole, each count and
    /// palette range in a byte.
    fn received(self) -> ObjDesc {
        /// An id as it arrives: sent less its kind when it carries it, and read back with it.
        fn whole(v: u32, kind: u32) -> u32 {
            let sent = if v & kind != 0 {
                v.wrapping_sub(kind)
            } else {
                v
            };
            kind.wrapping_add(sent & 0x3FFF_FFFF)
        }
        const PALETTE: u32 = 0x0400_0000;
        const TEXTURE: u32 = 0x0500_0000;
        const MODEL: u32 = 0x0100_0000;
        #[allow(clippy::cast_possible_truncation)]
        let byte = |v: u16| v as u8;
        ObjDesc {
            palette_id: if self.subpalettes.is_empty() {
                0
            } else {
                whole(self.palette, PALETTE)
            },
            subpalettes: self
                .subpalettes
                .iter()
                .map(|(p, offset, length)| Subpalette {
                    sub_id: whole(*p, PALETTE),
                    offset: byte(*offset),
                    num_colors: byte(*length),
                })
                .collect(),
            texture_changes: self
                .textures
                .iter()
                .map(|(part, old, new)| TextureMapChange {
                    part_index: *part,
                    old_tex_id: whole(*old, TEXTURE),
                    new_tex_id: whole(*new, TEXTURE),
                })
                .collect(),
            anim_part_changes: self
                .parts
                .iter()
                .map(|(part, model)| AnimPartChange {
                    part_index: *part,
                    part_id: whole(*model, MODEL),
                })
                .collect(),
        }
    }
}

/// The face and skin `look` names.
fn face(look: &Look, files: &dyn LookFiles, od: &mut Built) {
    match (look.head, look.hair_style, look.heritage, look.gender) {
        (Some(head), None, Some(h), _) if h != GEAR_KNIGHT => od.parts.push((HEAD, head)),
        (_, Some(style), Some(h), Some(g)) => {
            let sex = files.char_gen().and_then(|cg| {
                cg.heritage_groups
                    .get(&u32::try_from(h).ok()?)?
                    .sexes
                    .get(&u32::try_from(g).ok()?)
            });
            if let Some(s) = sex.and_then(|s| s.hair_styles.get(usize::try_from(style).ok()?)) {
                for (part, old, new) in &s.objdesc.texture_changes {
                    od.textures.push((*part, old.0, new.0));
                }
                for (part, model) in &s.objdesc.anim_part_changes {
                    od.parts.push((*part, model.0));
                }
            }
        }
        _ => {}
    }
    if let Some(p) = look.hair_palette {
        od.subpalettes.push((p, 0x18, 0x8));
    }
    if let Some(p) = look.palette_base {
        od.palette = p;
    }
    if let Some(p) = look.skin_palette {
        od.subpalettes.push((p, 0, 0x18));
    }
    if let (Some(from), Some(to)) = look.eyes {
        od.textures.push((HEAD, from, to));
    }
    if let Some(p) = look.eyes_palette {
        od.subpalettes.push((p, 0x20, 0x8));
    }
    for (from, to) in [look.nose, look.mouth] {
        if let (Some(from), Some(to)) = (from, to) {
            od.textures.push((HEAD, from, to));
        }
    }
}

/// The description of an object that is no creature: its face, and its clothing look for its
/// setup.
fn object(look: &Look, files: &dyn LookFiles) -> Built {
    let mut od = Built::default();
    face(look, files, &mut od);
    let Some(base) = look.clothing_base else {
        return od;
    };
    let Some(table) = files.clothing(base) else {
        return od;
    };
    let Some(effects) = table
        .clothing_bases
        .get(&dereth_primitives::DataId(look.setup.unwrap_or(0)))
    else {
        return od;
    };
    for e in effects {
        let part = byte_of(e.part_num);
        od.parts.push((part, e.object_id.0));
        for t in &e.texture_effects {
            od.textures.push((part, t.old_texture.0, t.new_texture.0));
        }
    }
    if !table.palette_templates.is_empty()
        && (look.shade.is_some() || look.palette_template.is_some())
    {
        od.recolor(&table, base, look.palette_template, look.shade, files);
    }
    od
}

/// A part number in its byte.
fn byte_of(part: u32) -> u8 {
    #[allow(clippy::cast_possible_truncation)]
    let b = part as u8;
    b
}

/// What clothing table `table` covers of a person as the pieces layer: each body part its clothing
/// for the first human body swaps; `None` when it has no clothing for that body.
fn layer_of(table: &ClothingTable) -> Option<u32> {
    let effects = table
        .clothing_bases
        .get(&dereth_primitives::DataId(HUMAN_MALE))?;
    let mut mask = 0;
    for e in effects {
        mask |= match e.part_num {
            0 => covers::OUTERWEAR_ABDOMEN,
            1 | 5 => covers::OUTERWEAR_UPPER_LEGS,
            2 | 6 => covers::OUTERWEAR_LOWER_LEGS,
            3 | 4 | 7 | 8 => covers::FEET,
            9 => covers::OUTERWEAR_CHEST,
            10 | 13 => covers::OUTERWEAR_UPPER_ARMS,
            11 | 14 => covers::OUTERWEAR_LOWER_ARMS,
            12 | 15 => covers::HANDS,
            16 => covers::HEAD,
            _ => 0,
        };
    }
    Some(mask)
}

/// The description of a creature wearing `worn`, in the order it put them on.
fn creature(look: &Look, worn: &[Worn], files: &dyn LookFiles) -> Built {
    let mut od = Built::default();
    face(look, files, &mut od);
    let setup = look.setup.unwrap_or(0);
    let clothed = clothed_as(setup);
    let armor_like =
        |w: &Worn| w.piece.item_type == ITEM_ARMOR || w.location & (at::ARMOR | at::EXTREMITY) != 0;
    // The armor and the pieces worn on the head, hands and feet layer by their top-layer mark
    // (under, unmarked, over) and then by what their clothing covers of a person, else by what
    // they say they cover; the rest go under them by what they say they cover.
    let layer = |w: &Worn| {
        w.piece
            .clothing_base
            .filter(|_| w.location & (at::ARMOR | at::EXTREMITY) != 0)
            .and_then(|b| files.clothing(b))
            .and_then(|t| layer_of(&t))
            .or(w.piece.coverage)
    };
    let armor: Vec<(&Worn, Option<u32>)> = worn
        .iter()
        .filter(|w| armor_like(w))
        .map(|w| (w, layer(w)))
        .collect();
    let mut sorted: Vec<&Worn> = Vec::new();
    for mark in [Some(false), None, Some(true)] {
        let mut these: Vec<&(&Worn, Option<u32>)> = armor
            .iter()
            .filter(|(w, _)| w.piece.top_layer == mark)
            .collect();
        these.sort_by_key(|(_, l)| *l);
        sorted.extend(these.into_iter().map(|(w, _)| *w));
    }
    let mut clothes: Vec<&Worn> = worn
        .iter()
        .filter(|w| {
            w.piece.item_type == ITEM_CLOTHING && w.location & (at::ARMOR | at::EXTREMITY) == 0
        })
        .collect();
    clothes.sort_by_key(|w| w.piece.coverage);
    let pieces: Vec<&Worn> = clothes.into_iter().chain(sorted).collect();

    if pieces.is_empty() && !look.own.is_empty() {
        od.parts.extend(look.own.parts.iter().copied());
        od.subpalettes.extend(look.own.palettes.iter().copied());
        od.textures.extend(look.own.textures.iter().copied());
        return od;
    }

    let mut covered: Vec<u8> = Vec::new();
    for w in pieces {
        // Only what covers the body changes it.
        if w.location & (at::CLOTHING | at::ARMOR | at::CLOAK) == 0 {
            continue;
        }
        let Some(base) = w.piece.clothing_base else {
            // A piece with no clothing look wears its own setup's parts.
            for (i, part) in files
                .setup_parts(w.piece.setup.unwrap_or(0))
                .into_iter()
                .enumerate()
            {
                // The last head part is no part at all.
                if part != 0x0100_01EC || i != 16 {
                    od.parts
                        .push((byte_of(u32::try_from(i).unwrap_or(u32::MAX)), part));
                }
            }
            for (part, _) in &od.parts {
                if !covered.contains(part) {
                    covered.push(*part);
                }
            }
            continue;
        };
        let Some(table) = files.clothing(base) else {
            continue;
        };
        let effects = table
            .clothing_bases
            .get(&dereth_primitives::DataId(setup))
            .or_else(|| {
                table
                    .clothing_bases
                    .get(&dereth_primitives::DataId(clothed))
            });
        let Some(effects) = effects else { continue };
        for e in effects {
            let part = byte_of(e.part_num);
            covered.push(part);
            od.add_part(part, e.object_id.0);
            for t in &e.texture_effects {
                od.add_texture((part, t.old_texture.0, t.new_texture.0));
            }
        }
        if !table.palette_templates.is_empty() {
            od.recolor(&table, base, w.piece.palette_template, w.piece.shade, files);
        }
    }
    // The body's own parts wherever nothing covers it, the head left as the face made it.
    if setup > 0 {
        for (i, part) in files.setup_parts(setup).into_iter().enumerate() {
            let i = byte_of(u32::try_from(i).unwrap_or(u32::MAX));
            if !covered.contains(&i) && i != HEAD {
                od.parts.push((i, part));
            }
        }
    }
    if covered.is_empty() && look.clothing_base.is_some() {
        return object(look, files);
    }
    od
}

/// The object description a server sends for an object of look `look`, wearing `worn` (a
/// creature's, from [`dress`]; nothing for any other), as the client receives it.
#[must_use]
pub fn object_description(look: &Look, worn: &[Worn], files: &dyn LookFiles) -> ObjDesc {
    let built = if look.is_creature() {
        creature(&rolled(look, &Face::FIXED, files), worn, files)
    } else if look.weenie_type == CORPSE_TYPE && !look.own.is_empty() {
        let mut od = Built::default();
        face(look, files, &mut od);
        od.parts.extend(look.own.parts.iter().copied());
        od.subpalettes.extend(look.own.palettes.iter().copied());
        od.textures.extend(look.own.textures.iter().copied());
        od
    } else {
        object(look, files)
    };
    built.received()
}

/// A list sorted as the server's runtime sorts one of up to sixteen: two by one comparison, three
/// by three, more by insertion. (Its longer lists are sorted otherwise; a creature wears fewer.)
fn sort_small<T>(v: &mut [T], cmp: impl Fn(&T, &T) -> Ordering) {
    let swap_if_greater = |v: &mut [T], i: usize, j: usize| {
        if cmp(&v[i], &v[j]) == Ordering::Greater {
            v.swap(i, j);
        }
    };
    match v.len() {
        0 | 1 => {}
        2 => swap_if_greater(v, 0, 1),
        3 => {
            swap_if_greater(v, 0, 1);
            swap_if_greater(v, 0, 2);
            swap_if_greater(v, 1, 2);
        }
        n => {
            for i in 0..n - 1 {
                let mut j = i + 1;
                while j > 0 && cmp(&v[j], &v[j - 1]) == Ordering::Less {
                    v.swap(j, j - 1);
                    j -= 1;
                }
            }
        }
    }
}

/// What a creature whose create list wields `wields` (in the list's order, the rows chosen by
/// chance left out) wears, in the order it puts them on: the clothing pieces only, as only they
/// change its look.
#[must_use]
pub fn dress(wields: &[Wield]) -> Vec<Worn> {
    let mut held: Vec<Wield> = wields
        .iter()
        .filter(|w| w.weenie_type == CLOTHING_TYPE)
        .cloned()
        .collect();
    let mut worn: Vec<Worn> = Vec::new();
    // Put on, when nothing worn covers what it would.
    let put_on = |worn: &mut Vec<Worn>, w: &Wield| {
        let location = w.locations.unwrap_or(0);
        let free = if at::WEAPON_SLOTS.contains(&location) {
            !worn.iter().any(|x| x.location == location)
        } else {
            let mask = w.coverage.unwrap_or(0);
            !worn
                .iter()
                .any(|x| x.piece.coverage.is_some_and(|c| c & mask != 0))
        };
        if free {
            worn.push(Worn {
                piece: w.clone(),
                location,
            });
        }
    };
    // Taken from what it holds, whether or not it then goes on.
    let take = |held: &mut Vec<Wield>, w: &Wield| {
        if let Some(i) = held.iter().position(|x| x == w) {
            held.remove(i);
        }
    };
    let flags = |w: &Wield| w.locations.unwrap_or(0).count_ones();
    // The server equips a creature twice: once for its create list, once more after any treasure
    // it wields (none here), over what it still holds.
    for _ in 0..2 {
        // Its underwear and cloak: pants, a shirt and a cloak, each the one covering most.
        let mut under: Vec<Wield> = held
            .iter()
            .filter(|w| {
                w.coverage.unwrap_or(0) & covers::UNDERWEAR != 0
                    || w.locations.unwrap_or(0) & at::CLOAK != 0
            })
            .cloned()
            .collect();
        if !under.is_empty() {
            sort_small(&mut under, |a, b| flags(a).cmp(&flags(b)));
            under.reverse();
            let pick = |mask: u32, by_location: bool| {
                under
                    .iter()
                    .find(|w| {
                        if by_location {
                            w.locations.unwrap_or(0) & mask != 0
                        } else {
                            w.coverage.unwrap_or(0) & mask != 0
                        }
                    })
                    .cloned()
            };
            for w in [
                pick(covers::PANTS, false),
                pick(covers::SHIRT, false),
                pick(at::CLOAK, true),
            ]
            .into_iter()
            .flatten()
            {
                take(&mut held, &w);
                put_on(&mut worn, &w);
            }
        }
        // Its outerwear, the best armor first, slot by slot.
        let mut outer: Vec<Wield> = held
            .iter()
            .filter(|w| w.coverage.unwrap_or(0) & covers::OUTERWEAR != 0)
            .cloned()
            .collect();
        if outer.is_empty() {
            continue;
        }
        sort_small(&mut outer, |a, b| flags(a).cmp(&flags(b)));
        outer.reverse();
        sort_small(&mut outer, |a, b| {
            a.armor_level.unwrap_or(0).cmp(&b.armor_level.unwrap_or(0))
        });
        outer.reverse();
        let slots = [
            at::HEAD_WEAR,
            at::CHEST_ARMOR | at::CHEST_WEAR,
            at::UPPER_ARM_ARMOR | at::UPPER_ARM_WEAR,
            at::LOWER_ARM_ARMOR | at::LOWER_ARM_WEAR,
            at::HAND_WEAR,
            at::UPPER_LEG_ARMOR | at::UPPER_LEG_WEAR,
            at::ABDOMEN_ARMOR,
            at::LOWER_LEG_ARMOR | at::LOWER_LEG_WEAR,
            at::FOOT_WEAR,
        ];
        for slot in slots {
            for w in outer
                .iter()
                .filter(|w| w.locations.unwrap_or(0) & slot != 0)
            {
                take(&mut held, w);
                // A piece already worn is not put on twice.
                if !worn.iter().any(|x| x.piece == *w) {
                    put_on(&mut worn, w);
                }
            }
        }
    }
    worn
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (a server's object description, as ACE works it out from a weenie's
    //! properties; the client registry holds the client's own behavior)
    use std::collections::BTreeMap;

    use dereth_assets::motion::{
        ObjectEffect, PaletteEffect, PaletteRange, PaletteTemplate, TextureEffect,
    };
    use dereth_primitives::DataId;

    use super::*;

    #[derive(Default)]
    struct Files {
        tables: BTreeMap<u32, Arc<ClothingTable>>,
        first: BTreeMap<u32, u32>,
        sets: BTreeMap<u32, Vec<u32>>,
        setups: BTreeMap<u32, Vec<u32>>,
    }

    impl LookFiles for Files {
        fn clothing(&self, id: u32) -> Option<Arc<ClothingTable>> {
            self.tables.get(&id).cloned()
        }
        fn first_palette_template(&self, id: u32) -> Option<u32> {
            self.first.get(&id).copied()
        }
        fn palette_set(&self, id: u32) -> Vec<u32> {
            self.sets.get(&id).cloned().unwrap_or_default()
        }
        fn setup_parts(&self, id: u32) -> Vec<u32> {
            self.setups.get(&id).cloned().unwrap_or_default()
        }
        fn char_gen(&self) -> Option<&CharGen> {
            None
        }
    }

    const BODY: u32 = 0x0200_0001;
    const ROBE: u32 = 0x1000_0010;
    const HAT: u32 = 0x1000_0020;

    /// A table dressing `setup` with `parts` (each swapped to a model of its own number), whose
    /// schemes are `schemes`, each one range of palette set `0x0F00_0000 + scheme`.
    fn table(id: u32, setup: u32, parts: &[u32], schemes: &[u32]) -> ClothingTable {
        let effects = parts
            .iter()
            .map(|p| ObjectEffect {
                part_num: *p,
                object_id: DataId(0x0100_1000 + p),
                texture_effects: vec![TextureEffect {
                    old_texture: DataId(0x0500_0001),
                    new_texture: DataId(0x0500_1000 + p),
                }],
            })
            .collect();
        ClothingTable {
            id: DataId(id),
            clothing_base_buckets: 1,
            clothing_bases: [(DataId(setup), effects)].into_iter().collect(),
            palette_template_buckets: 1,
            palette_templates: schemes
                .iter()
                .map(|s| {
                    (
                        *s,
                        PaletteTemplate {
                            icon: DataId(0),
                            subpalette_effects: vec![PaletteEffect {
                                ranges: vec![PaletteRange {
                                    offset: 0x600,
                                    length: 0x100,
                                }],
                                palette_set: DataId(0x0F00_0000 + s),
                            }],
                        },
                    )
                })
                .collect(),
        }
    }

    /// Files with one robe of schemes 2, 5 and 9 (9 first in its file), each set four palettes.
    fn files() -> Files {
        let mut f = Files::default();
        f.tables
            .insert(ROBE, Arc::new(table(ROBE, BODY, &[0, 9], &[2, 5, 9])));
        f.tables
            .insert(HAT, Arc::new(table(HAT, BODY, &[16], &[4])));
        f.first.insert(ROBE, 9);
        for s in [2, 4, 5, 9] {
            f.sets.insert(
                0x0F00_0000 + s,
                (0..4).map(|i| 0x0400_0000 + s * 0x100 + i).collect(),
            );
        }
        f.setups
            .insert(BODY, (0..17).map(|p| 0x0100_0000 + p).collect());
        f
    }

    fn robe_on_the_ground(scheme: Option<i32>, shade: Option<f64>) -> Look {
        Look {
            weenie_type: CLOTHING_TYPE,
            setup: Some(BODY),
            clothing_base: Some(ROBE),
            palette_template: scheme,
            shade,
            ..Look::default()
        }
    }

    fn palettes(od: &ObjDesc) -> Vec<u32> {
        od.subpalettes.iter().map(|s| s.sub_id).collect()
    }

    #[test]
    fn a_clothing_look_swaps_the_tables_parts_and_textures_for_the_objects_setup() {
        let od = object_description(&robe_on_the_ground(Some(2), Some(0.0)), &[], &files());
        let parts: Vec<(u8, u32)> = od
            .anim_part_changes
            .iter()
            .map(|p| (p.part_index, p.part_id))
            .collect();
        assert_eq!(parts, [(0, 0x0100_1000), (9, 0x0100_1009)]);
        assert_eq!(od.texture_changes.len(), 2);
        assert_eq!(od.texture_changes[1].new_tex_id, 0x0500_1009);
        // A setup the table does not dress keeps everything.
        let mut other = robe_on_the_ground(Some(2), Some(0.0));
        other.setup = Some(0x0200_0002);
        assert_eq!(
            object_description(&other, &[], &files()),
            ObjDesc::default()
        );
    }

    #[test]
    fn the_shade_picks_a_palette_of_the_schemes_set_from_dark_to_light() {
        let f = files();
        let at = |shade| {
            palettes(&object_description(
                &robe_on_the_ground(Some(5), Some(shade)),
                &[],
                &f,
            ))
        };
        assert_eq!(at(0.0), [0x0400_0500]);
        assert_eq!(at(0.3), [0x0400_0501]);
        assert_eq!(at(0.74), [0x0400_0502]);
        assert_eq!(at(1.0), [0x0400_0503], "the last palette, not past it");
        // A range in the clothing table's entries goes out in eights of them.
        let od = object_description(&robe_on_the_ground(Some(5), Some(0.0)), &[], &f);
        assert_eq!(
            (od.subpalettes[0].offset, od.subpalettes[0].num_colors),
            (0xC0, 0x20)
        );
        // Out of range is no palette; the client reads it as the palette kind's own id.
        assert_eq!(at(1.5), [0x0400_0000]);
    }

    #[test]
    fn a_scheme_the_table_lacks_takes_the_one_its_file_lists_first_and_none_keeps_its_own_colors() {
        let f = files();
        let set_of = |od: &ObjDesc| od.subpalettes.first().map(|s| s.sub_id & 0xFF00);
        // Scheme 7 is not the robe's: the first in its file, 9, not the lowest, 2.
        let od = object_description(&robe_on_the_ground(Some(7), Some(0.0)), &[], &f);
        assert_eq!(set_of(&od), Some(0x0900));
        // A shade alone takes it too.
        let od = object_description(&robe_on_the_ground(None, Some(0.0)), &[], &f);
        assert_eq!(set_of(&od), Some(0x0900));
        // Neither: the parts and textures, no palettes.
        let od = object_description(&robe_on_the_ground(None, None), &[], &f);
        assert!(od.subpalettes.is_empty() && od.palette_id == 0);
        assert_eq!(od.anim_part_changes.len(), 2);
    }

    #[test]
    fn a_palette_goes_out_in_sixteen_bits_and_comes_back_with_its_kind() {
        let mut f = files();
        f.sets.insert(0x0F00_0005, vec![0x0401_2345]);
        let od = object_description(&robe_on_the_ground(Some(5), Some(0.5)), &[], &f);
        assert_eq!(palettes(&od), [0x0400_2345]);
        // The base palette is sent whole.
        let mut look = robe_on_the_ground(Some(5), Some(0.5));
        look.palette_base = Some(0x0400_007E);
        let od = object_description(&look, &[], &f);
        assert_eq!(od.palette_id, 0x0400_007E);
    }

    fn piece(base: u32, locations: u32, coverage: u32, item_type: u32) -> Wield {
        Wield {
            weenie_type: CLOTHING_TYPE,
            item_type,
            locations: Some(locations),
            coverage: Some(coverage),
            clothing_base: Some(base),
            ..Wield::default()
        }
    }

    fn person(own: OwnChanges) -> Look {
        Look {
            weenie_type: 10,
            setup: Some(BODY),
            own,
            ..Look::default()
        }
    }

    #[test]
    fn a_creature_wears_its_pieces_and_its_own_parts_show_where_nothing_covers_it() {
        let f = files();
        let robe =
            piece(ROBE, at::CHEST_WEAR, covers::UNDERWEAR_CHEST, ITEM_CLOTHING).made(5, 0.0, false);
        let hat = piece(HAT, at::HEAD_WEAR, covers::HEAD, ITEM_ARMOR);
        let worn = dress(&[hat.clone(), robe.clone()]);
        assert_eq!(worn.len(), 2);
        let od = object_description(&person(OwnChanges::default()), &worn, &f);
        let parts: Vec<u8> = od.anim_part_changes.iter().map(|p| p.part_index).collect();
        // The robe (clothing) first, then the hat (armor), then the body's own parts but the
        // covered ones and the head.
        let mut want = vec![0, 9, 16];
        want.extend((1..17).filter(|p| *p != 9 && *p != 16));
        assert_eq!(parts, want);
        assert_eq!(od.anim_part_changes[3].part_id, 0x0100_0001);
        // The robe's scheme from its row; the hat's table lists only one.
        assert_eq!(palettes(&od), [0x0400_0500, 0x0400_0400]);
    }

    #[test]
    fn a_creature_wearing_nothing_shows_its_own_changes_else_its_clothing_look() {
        let f = files();
        let own = OwnChanges {
            parts: vec![(3, 0x0100_0333)],
            textures: vec![(3, 0x0500_0001, 0x0500_0002)],
            palettes: vec![(0x0400_0042, 0, 0x18)],
        };
        let od = object_description(&person(own), &[], &f);
        assert_eq!(od.anim_part_changes.len(), 1);
        assert_eq!(od.anim_part_changes[0].part_id, 0x0100_0333);
        assert_eq!(palettes(&od), [0x0400_0042]);
        // Nothing worn, nothing of its own, a clothing look: dressed in it as any object is.
        let mut look = person(OwnChanges::default());
        look.clothing_base = Some(ROBE);
        look.palette_template = Some(2);
        let od = object_description(&look, &[], &f);
        assert_eq!(od.anim_part_changes.len(), 2);
        assert_eq!(palettes(&od), [0x0400_0200]);
    }

    #[test]
    fn a_creature_puts_on_one_piece_for_what_it_covers_and_the_one_covering_most_first() {
        let shirt = |base, locations| {
            piece(
                base,
                locations,
                covers::UNDERWEAR_CHEST | covers::UNDERWEAR_UPPER_ARMS,
                ITEM_CLOTHING,
            )
        };
        let short = shirt(0x1000_0001, at::CHEST_WEAR);
        let long = shirt(0x1000_0002, at::CHEST_WEAR | at::UPPER_ARM_WEAR);
        let worn = dress(&[short, long.clone()]);
        assert_eq!(worn.len(), 1, "the two cover the same and only one goes on");
        assert_eq!(worn[0].piece, long);
        // A weapon is held, not worn.
        let mut sword = piece(0x1000_0003, 0x0010_0000, 0, 1);
        sword.weenie_type = 6;
        assert!(dress(&[sword]).is_empty());
    }

    #[test]
    fn outerwear_goes_on_slot_by_slot_the_best_armor_first() {
        let mut light = piece(
            0x1000_0001,
            at::CHEST_ARMOR,
            covers::OUTERWEAR_CHEST,
            ITEM_ARMOR,
        );
        light.armor_level = Some(10);
        let mut heavy = piece(
            0x1000_0002,
            at::CHEST_ARMOR,
            covers::OUTERWEAR_CHEST,
            ITEM_ARMOR,
        );
        heavy.armor_level = Some(90);
        let boots = piece(0x1000_0003, at::FOOT_WEAR, covers::FEET, ITEM_ARMOR);
        let worn = dress(&[boots.clone(), light, heavy.clone()]);
        let on: Vec<&Wield> = worn.iter().map(|w| &w.piece).collect();
        assert_eq!(on, [&heavy, &boots]);
    }

    #[test]
    fn a_create_lists_row_colors_its_piece_and_a_treasure_rows_shade_is_its_chance() {
        let p = piece(ROBE, at::CHEST_WEAR, covers::UNDERWEAR_CHEST, ITEM_CLOTHING);
        let mut own = p.clone();
        own.palette_template = Some(9);
        own.shade = Some(0.9);
        let made = own.clone().made(5, 0.25, false);
        assert_eq!((made.palette_template, made.shade), (Some(5), Some(0.25)));
        let kept = own.clone().made(0, 0.0, false);
        assert_eq!((kept.palette_template, kept.shade), (Some(9), Some(0.0)));
        let treasure = own.made(0, 0.4, true);
        assert_eq!(treasure.shade, Some(0.9));
    }

    #[test]
    fn a_short_list_sorts_as_the_servers_runtime_sorts_it() {
        // Three: two equal keys after a greater one come out the other way round.
        let mut three = [(1, 'a'), (1, 'b'), (0, 'c')];
        sort_small(&mut three, |a, b| a.0.cmp(&b.0));
        assert_eq!(three, [(0, 'c'), (1, 'b'), (1, 'a')]);
        // Four or more keep equal keys in their order.
        let mut four = [(1, 'a'), (1, 'b'), (0, 'c'), (0, 'd')];
        sort_small(&mut four, |a, b| a.0.cmp(&b.0));
        assert_eq!(four, [(0, 'c'), (0, 'd'), (1, 'a'), (1, 'b')]);
    }

    #[test]
    fn an_alternate_body_wears_its_heritages_clothing() {
        assert_eq!(clothed_as(0x0200_1972), 0x0200_196F);
        assert_eq!(clothed_as(0x0200_1AA3), HUMAN_MALE);
        assert_eq!(clothed_as(BODY), BODY);
    }
}
