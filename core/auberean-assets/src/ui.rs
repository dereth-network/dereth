//! The user interface: layouts (`.uil`), scenes (`.uiscene`), fonts (`.font`), key maps
//! (`.keymap`) and action maps (`.actionmap`).

use std::collections::BTreeSet;

use auberean_dat::qualified::divine_type;
use auberean_dat::reader::Result;
use auberean_dat::types::RecordType;
use auberean_dat::Reader;
use auberean_primitives::DataId;

use crate::common::Color32;
use crate::entity::Entity;
use crate::{Context, Decode};

/// A layout (`.uil`, local files `0x22000000`+): a screen of the interface as a tree of elements,
/// each with states, each state with attributes, media, transitions and child elements.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct UiLayout {
    /// Records the layout uses, listed with it.
    pub references: Vec<u32>,
    /// The string table its text comes from.
    pub string_table: DataId,
    /// The screen size the layout was made for.
    pub width: u16,
    pub height: u16,
    /// The top-level elements.
    pub elements: Vec<Element>,
}

/// An element of a layout: a rectangle with states. Dropped with its states.
// D2-11078, D2-11077
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Element {
    pub id: u32,
    pub kind: u32,
    /// The state the element starts in.
    pub default_state: u32,
    pub left: i16,
    pub top: i16,
    pub right: i16,
    pub bottom: i16,
    pub z: u16,
    /// How each edge (left, top, right, bottom) follows its parent when the parent is resized.
    pub edges: [u8; 4],
    pub states: Vec<ElementState>,
}

/// One state of an element: its own rectangle, what it shows and does, and its children. Dropped
/// with its attributes, media, transitions and children.
// D2-11087
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ElementState {
    pub id: u32,
    pub flags: u8,
    pub left: i16,
    pub top: i16,
    pub right: i16,
    pub bottom: i16,
    pub z: u16,
    pub attributes: Vec<Attribute>,
    pub media: Vec<Media>,
    pub transitions: Vec<Transition>,
    pub children: Vec<Element>,
}

/// A named value set on a state. Dropped with its text, when it holds text.
// D2-11099
#[derive(Clone, Debug, PartialEq)]
pub struct Attribute {
    pub id: u32,
    pub value: AttributeValue,
}

/// An attribute's value, by its stored kind.
#[derive(Clone, Debug, PartialEq)]
pub enum AttributeValue {
    /// Kind 1.
    Id(u32),
    /// Kind 2.
    Int(i32),
    /// Kind 3.
    Float(f32),
    /// Kind 4.
    Bool(bool),
    /// Kind 5: a narrow string.
    Text(String),
    /// Kind 6: a string-table token.
    StringToken(u32),
    /// Kind 7.
    DataFile(DataId),
    /// Any other kind, which stores nothing.
    Empty(u8),
}

/// What a state shows or does while it is current, in order. Dropped with what it holds (a
/// video's file name).
// D2-11095, D2-11096, D2-11121
#[derive(Clone, Debug, PartialEq)]
pub enum Media {
    /// Kind 1: frames of pictures played over a duration.
    Animation {
        duration: f32,
        draw_mode: u32,
        x: i32,
        y: i32,
        frames: Vec<u32>,
    },
    /// Kind 2: wait a random time in a range.
    Pause { min: f32, max: f32 },
    /// Kind 3: a picture.
    Image {
        picture: DataId,
        draw_mode: u32,
        x: i32,
        y: i32,
    },
    /// Kind 4: a picture used as the element's transparency.
    Alpha { picture: DataId, x: i32, y: i32 },
    /// Kind 5: a sound.
    Sound { sound: DataId },
    /// Kind 6: jump to another media item, with a probability.
    Jump { index: u32, probability: f32 },
    /// Kind 7: send a message, with a probability.
    Message { message: u32, probability: f32 },
    /// Kind 8: go to another state.
    State { state: u32 },
    /// Kind 9: a video file.
    Video { x: i32, y: i32, file: String },
    /// Kind 10: a mouse cursor and its hot spot.
    Cursor { picture: DataId, x: i32, y: i32 },
}

/// A change animated over time when something happens.
// D2-11106
#[derive(Clone, Debug, PartialEq)]
pub struct Transition {
    /// 1 move, 2 resize, 3 fade, 4 rotate.
    pub kind: u32,
    /// When it runs.
    pub when: u32,
    /// The state it belongs to.
    pub state: u32,
    pub change: TransitionChange,
}

/// What a transition changes, from where to where, and over how long.
#[derive(Clone, Debug, PartialEq)]
pub enum TransitionChange {
    Move {
        end_x: i32,
        end_y: i32,
        start_x: i32,
        start_y: i32,
        duration: f64,
        flags: u32,
    },
    Resize {
        end_height: i32,
        end_width: i32,
        start_height: i32,
        start_width: i32,
        duration: f64,
        flags: u32,
    },
    Fade {
        end_alpha: f32,
        start_alpha: f32,
        duration: f64,
        flags: u32,
    },
    Rotate {
        end_x: i32,
        end_y: i32,
        end_z: i32,
        duration: f64,
        flags: u32,
    },
    /// Any other kind, which stores nothing more.
    None,
}

impl Attribute {
    /// The id, a byte kind, alignment, the value its kind stores, alignment.
    // D2-11103
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let id = r.u32()?;
        let kind = r.u8()?;
        r.align()?;
        let value = match kind {
            1 => AttributeValue::Id(r.u32()?),
            2 => AttributeValue::Int(r.i32()?),
            3 => AttributeValue::Float(r.f32()?),
            4 => AttributeValue::Bool(r.bool32()?),
            5 => AttributeValue::Text(r.narrow_string()?),
            6 => AttributeValue::StringToken(r.u32()?),
            7 => AttributeValue::DataFile(r.data_id()?),
            k => AttributeValue::Empty(k),
        };
        r.align()?;
        Ok(Self { id, value })
    }
}

impl Media {
    /// A kind word, the kind word again (the item's own copy), then the kind's fields.
    // D2-11092, D2-11094, D2-11114, D2-11131, D2-11110, D2-11147, D2-11135, D2-11124, D2-11093
    // D2-11107, D2-11115, D2-11120, D2-11125, D2-11128, D2-11132, D2-11136, D2-11139, D2-11143
    // D2-11148
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let kind = r.u32()?;
        let again = r.u32()?;
        r.ensure(again == kind, || {
            format!("media kind {kind} stored again as {again}")
        })?;
        Ok(match kind {
            1 => Self::Animation {
                duration: r.f32()?,
                draw_mode: r.u32()?,
                x: r.i32()?,
                y: r.i32()?,
                frames: r.u32_list()?,
            },
            2 => Self::Pause {
                min: r.f32()?,
                max: r.f32()?,
            },
            3 => Self::Image {
                picture: r.data_id()?,
                draw_mode: r.u32()?,
                x: r.i32()?,
                y: r.i32()?,
            },
            4 => Self::Alpha {
                picture: r.data_id()?,
                x: r.i32()?,
                y: r.i32()?,
            },
            5 => Self::Sound {
                sound: r.data_id()?,
            },
            6 => Self::Jump {
                index: r.u32()?,
                probability: r.f32()?,
            },
            7 => Self::Message {
                message: r.u32()?,
                probability: r.f32()?,
            },
            8 => Self::State { state: r.u32()? },
            9 => Self::Video {
                x: r.i32()?,
                y: r.i32()?,
                file: r.narrow_string()?,
            },
            10 => Self::Cursor {
                picture: r.data_id()?,
                x: r.i32()?,
                y: r.i32()?,
            },
            k => return r.fail(format!("media kind {k}")),
        })
    }
}

impl Transition {
    /// The kind, when, the state, then the kind's change: two to four words, a duration as a
    /// double, and a flags word.
    // D2-11105
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let kind = r.u32()?;
        let when = r.u32()?;
        let state = r.u32()?;
        let change = match kind {
            1 => TransitionChange::Move {
                end_x: r.i32()?,
                end_y: r.i32()?,
                start_x: r.i32()?,
                start_y: r.i32()?,
                duration: r.f64()?,
                flags: r.u32()?,
            },
            2 => TransitionChange::Resize {
                end_height: r.i32()?,
                end_width: r.i32()?,
                start_height: r.i32()?,
                start_width: r.i32()?,
                duration: r.f64()?,
                flags: r.u32()?,
            },
            3 => TransitionChange::Fade {
                end_alpha: r.f32()?,
                start_alpha: r.f32()?,
                duration: r.f64()?,
                flags: r.u32()?,
            },
            4 => TransitionChange::Rotate {
                end_x: r.i32()?,
                end_y: r.i32()?,
                end_z: r.i32()?,
                duration: r.f64()?,
                flags: r.u32()?,
            },
            _ => TransitionChange::None,
        };
        Ok(Self {
            kind,
            when,
            state,
            change,
        })
    }
}

impl ElementState {
    /// The id, a flags byte, the rectangle and depth (16 bits each), alignment, then counted lists
    /// of attributes, media, transitions and child elements, then alignment.
    // D2-11089
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let id = r.u32()?;
        let flags = r.u8()?;
        let left = r.i16()?;
        let top = r.i16()?;
        let right = r.i16()?;
        let bottom = r.i16()?;
        let z = r.u16()?;
        r.align()?;
        let attributes = r.list(8, Attribute::read)?;
        let media = r.list(8, Media::read)?;
        let transitions = r.list(12, Transition::read)?;
        let children = r.list(28, Element::read)?;
        r.align()?;
        Ok(Self {
            id,
            flags,
            left,
            top,
            right,
            bottom,
            z,
            attributes,
            media,
            transitions,
            children,
        })
    }
}

impl Element {
    /// The id, the kind, the default state, the rectangle and depth (16 bits each), the four edge
    /// bytes, a 16-bit count of states, alignment, the states, alignment.
    // D2-11080
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let id = r.u32()?;
        let kind = r.u32()?;
        let default_state = r.u32()?;
        let left = r.i16()?;
        let top = r.i16()?;
        let right = r.i16()?;
        let bottom = r.i16()?;
        let z = r.u16()?;
        let edges = [r.u8()?, r.u8()?, r.u8()?, r.u8()?];
        let n = r.count16(16)?;
        r.align()?;
        let mut states = Vec::with_capacity(n);
        for _ in 0..n {
            states.push(ElementState::read(r)?);
        }
        r.align()?;
        Ok(Self {
            id,
            kind,
            default_state,
            left,
            top,
            right,
            bottom,
            z,
            edges,
            states,
        })
    }
}

// D2-11070, D2-05044, D2-11071, D2-11064, D2-11063, D2-11062
impl Decode for UiLayout {
    /// The id, a counted list of referenced record ids, the string table id, the width and height
    /// (16 bits each), a 16-bit count of elements, alignment, the elements.
    // D2-11068
    fn decode(r: &mut Reader<'_>, id: u32, _ctx: &Context) -> Result<Self> {
        r.expect_id(id)?;
        let references = r.u32_list()?;
        let string_table = r.data_id()?;
        let width = r.u16()?;
        let height = r.u16()?;
        let n = r.count16(28)?;
        r.align()?;
        let mut elements = Vec::with_capacity(n);
        for _ in 0..n {
            elements.push(Element::read(r)?);
        }
        Ok(Self {
            references,
            string_table,
            width,
            height,
            elements,
        })
    }
}

/// A scene shown inside the interface (`.uiscene`, `0x21000000`+): placed entities, and fog.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct UiScene {
    pub entities: Vec<Entity>,
    pub fog_color: Color32,
    pub fog: bool,
    pub fog_near: f32,
    pub fog_far: f32,
}

impl Decode for UiScene {
    /// The id, a counted list of entities, the fog color, whether there is fog, the fog's near and
    /// far distances.
    // D2-10459, D2-13245
    fn decode(r: &mut Reader<'_>, id: u32, ctx: &Context) -> Result<Self> {
        r.expect_id(id)?;
        let entities = r.list(4, |r| Entity::read(r, ctx))?;
        let fog_color = Color32::read(r)?;
        let fog = r.bool32()?;
        let fog_near = r.f32()?;
        let fog_far = r.f32()?;
        Ok(Self {
            entities,
            fog_color,
            fog,
            fog_near,
            fog_far,
        })
    }
}

/// One character of a font: its width, its code point, and where its pixels start.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Glyph {
    pub width: u32,
    pub code_point: u16,
    /// The stored glyph's two unused bytes.
    pub unused: u16,
    /// Its first column in the font's strip of pixels.
    pub offset: u32,
}

/// The character a font draws in place of one it lacks.
pub const MISSING_CHARACTER: u16 = 0x003F;

/// Where each character's glyph stands among a font's glyphs: the least and greatest code points
/// it holds, and for every code point from the one to the other the glyph's place, or none.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct CharacterMap {
    pub first: u16,
    pub last: u16,
    pub places: Vec<Option<u32>>,
}

impl CharacterMap {
    /// The map of `glyphs`, or none for a font without glyphs. A code point held twice maps to
    /// its last glyph.
    // D2-10045, D2-10047
    #[must_use]
    pub fn of(glyphs: &[Glyph]) -> Option<Self> {
        let first = glyphs.iter().map(|g| g.code_point).min()?;
        let last = glyphs.iter().map(|g| g.code_point).max()?;
        let mut places = vec![None; usize::from(last - first) + 1];
        for (i, g) in glyphs.iter().enumerate() {
            places[usize::from(g.code_point - first)] = u32::try_from(i).ok();
        }
        Some(Self {
            first,
            last,
            places,
        })
    }

    /// The place of `ch`'s glyph, if the map holds one.
    #[must_use]
    pub fn place(&self, ch: u16) -> Option<usize> {
        if ch < self.first || ch > self.last {
            return None;
        }
        self.places[usize::from(ch - self.first)].map(|p| p as usize)
    }
}

/// A bitmap font (`.font`, `0x42000000`-`0x42000FFF`; local `0x42001000`+): its glyphs and one
/// strip of pixels `total_width` wide and `max_height` high, a byte a pixel when antialiased,
/// else a bit a pixel. A font read from the files carries the map of its characters; one made
/// otherwise has none, and its glyphs are searched in order. A new font is empty, and dropping
/// it lets go of its glyphs, its pixels and its map.
// D2-10039, D2-10040, D2-10041, D2-05064, D2-05065, D2-05066, D2-05068, D2-05104, D2-05067
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Font {
    pub max_height: u32,
    pub max_width: u32,
    pub average_width: u32,
    pub total_width: u32,
    pub antialiased: bool,
    pub glyphs: Vec<Glyph>,
    pub pixels: Vec<u8>,
    pub character_map: Option<CharacterMap>,
}

impl Font {
    /// The glyph `ch` is drawn with: its own, else the font's question mark's, else none.
    // D2-10042
    #[must_use]
    pub fn char_desc(&self, ch: u16) -> Option<&Glyph> {
        self.own_glyph(ch).or_else(|| {
            (ch != MISSING_CHARACTER)
                .then(|| self.own_glyph(MISSING_CHARACTER))
                .flatten()
        })
    }

    fn own_glyph(&self, ch: u16) -> Option<&Glyph> {
        match &self.character_map {
            Some(m) => m.place(ch).and_then(|i| self.glyphs.get(i)),
            None => self.glyphs.iter().find(|g| g.code_point == ch),
        }
    }

    /// How wide `ch` is drawn, in pixels (0 when the font has neither it nor a question mark).
    // D2-10043
    #[must_use]
    pub fn char_width(&self, ch: u16) -> u32 {
        self.char_desc(ch).map_or(0, |g| g.width)
    }

    /// Whether the font has every character of `s` but new lines. A font without a map of its
    /// characters is taken to have them all.
    // D2-10044
    #[must_use]
    pub fn contains_string(&self, s: &[u16]) -> bool {
        let Some(m) = &self.character_map else {
            return true;
        };
        s.iter().all(|&c| c == 0x000A || m.place(c).is_some())
    }

    /// The width of `s`'s widest line, its lines broken at new lines.
    // D2-10049, D2-10046
    #[must_use]
    pub fn string_width(&self, s: &[u16]) -> u32 {
        let (mut widest, mut line) = (0u32, 0u32);
        for &c in s {
            if c == 0x000A {
                widest = widest.max(line);
                line = 0;
            } else {
                line = line.wrapping_add(self.char_width(c));
            }
        }
        widest.max(line)
    }
}

/// The kind a font's id is read as: a font of the shared files or of the language's own file;
/// none for an id of any other kind.
// D2-10050, D2-10053
#[must_use]
pub fn font_kind(id: u32) -> Option<RecordType> {
    let kind = RecordType(divine_type(id));
    (kind == RecordType::FONT || kind == RecordType::FONT_LOCAL).then_some(kind)
}

impl Decode for Font {
    /// The id, the greatest height and width, the glyph count, the average width, the strip's
    /// width, the antialiased flag, the glyphs (12 bytes each), then the strip's bytes; the map
    /// of its characters made once it is read.
    // D2-10048
    fn decode(r: &mut Reader<'_>, id: u32, _ctx: &Context) -> Result<Self> {
        r.expect_id(id)?;
        let max_height = r.u32()?;
        let max_width = r.u32()?;
        let count = r.u32()? as usize;
        let average_width = r.u32()?;
        let total_width = r.u32()?;
        let antialiased = r.bool32()?;
        r.ensure(count.saturating_mul(12) <= r.remaining(), || {
            format!("{count} glyphs")
        })?;
        let mut glyphs = Vec::with_capacity(count);
        for _ in 0..count {
            glyphs.push(Glyph {
                width: r.u32()?,
                code_point: r.u16()?,
                unused: r.u16()?,
                offset: r.u32()?,
            });
        }
        let mut n = (total_width as usize).saturating_mul(max_height as usize);
        if !antialiased {
            n >>= 3;
        }
        let pixels = r.bytes(n)?.to_vec();
        let character_map = CharacterMap::of(&glyphs);
        Ok(Self {
            max_height,
            max_width,
            average_width,
            total_width,
            antialiased,
            glyphs,
            pixels,
            character_map,
        })
    }
}

/// One binding of a key map: an input to an action, with a name.
// D2-04879
#[derive(Clone, Debug, PartialEq, Default)]
pub struct KeyBinding {
    pub action: u32,
    pub input: u32,
    pub name: String,
}

/// A key map (`.keymap`, `0x1D000000`+): a named set of bindings from inputs to actions. A new
/// one is empty (the client leaves a new map's identity as the memory held it; this one is zero).
// D2-04904, D2-04905, D2-04906, D2-04911, D2-04910
#[derive(Clone, Debug, PartialEq, Default)]
pub struct KeyMap {
    pub name: String,
    /// The map's identity, a GUID as stored.
    pub guid: [u8; 16],
    pub bindings: Vec<KeyBinding>,
}

impl Decode for KeyMap {
    /// The id, the name (a narrow string), the 16-byte GUID, then a counted list of bindings (the
    /// action, the input, a narrow-string name).
    // D2-04889, D2-04900
    fn decode(r: &mut Reader<'_>, id: u32, _ctx: &Context) -> Result<Self> {
        r.expect_id(id)?;
        let name = r.narrow_string()?;
        let mut guid = [0u8; 16];
        guid.copy_from_slice(r.bytes(16)?);
        let bindings = r.list(12, |r| {
            Ok(KeyBinding {
                action: r.u32()?,
                input: r.u32()?,
                name: r.narrow_string()?,
            })
        })?;
        Ok(Self {
            name,
            guid,
            bindings,
        })
    }
}

/// What an action with modifiers does: its class, its value, and how far.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct ActionEntry {
    pub action: u32,
    pub modifiers: u32,
    pub class: u32,
    pub value: u32,
    pub extent: f32,
}

/// An action's name and description, as string-table tokens.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct ActionText {
    pub action: u32,
    pub name: u32,
    pub description: u32,
}

/// The action map (`.actionmap`, `0x51000000`+): which actions toggle which, what each action does
/// with each set of modifiers, and the actions' names.
// D2-03421, D2-03422, D2-03427, D2-03428, D2-03431, D2-03430
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ActionBindings {
    pub string_table: DataId,
    pub toggles: Vec<(u32, u32)>,
    pub actions: Vec<ActionEntry>,
    pub texts: Vec<ActionText>,
}

/// What an action does with a set of modifiers held: its class, its value and how far.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ActionValues {
    pub class: u32,
    pub value: u32,
    pub extent: f32,
}

impl ActionBindings {
    /// How an action toggles (held, latched, a single press); 0 for an action the map does not
    /// list.
    // D2-03406
    #[must_use]
    pub fn toggle_type(&self, action: u32) -> u32 {
        self.toggles
            .iter()
            .find(|(a, _)| *a == action)
            .map_or(0, |(_, t)| *t)
    }

    /// What an action does with exactly these modifiers, if the map says.
    // D2-03407
    #[must_use]
    pub fn action_values(&self, action: u32, modifiers: u32) -> Option<ActionValues> {
        self.actions
            .iter()
            .find(|e| e.action == action && e.modifiers == modifiers)
            .map(|e| ActionValues {
                class: e.class,
                value: e.value,
                extent: e.extent,
            })
    }

    /// An action's name and description as tokens of the map's string table, if it has a name
    /// (a description token of 0 is none).
    // D2-03408, D2-03436
    #[must_use]
    pub fn descrip_values(&self, action: u32) -> Option<(u32, u32)> {
        self.texts
            .iter()
            .find(|t| t.action == action)
            .map(|t| (t.name, t.description))
    }

    /// The map made empty.
    // D2-03423
    pub fn destroy(&mut self) {
        *self = Self::default();
    }

    /// Write the record as [`ActionBindings`] decodes it, each list in its key's order (the
    /// client keeps them in ordered maps): the toggles by action, the actions by action then
    /// modifiers, the texts by action.
    // D2-03402
    pub fn write(&self, id: u32, w: &mut auberean_dat::Writer) {
        w.u32(id);
        w.data_id(self.string_table);
        let mut toggles = self.toggles.clone();
        toggles.sort_unstable();
        w.list(&toggles, |w, (a, t)| {
            w.u32(*a);
            w.u32(*t);
        });
        let mut actions = self.actions.clone();
        actions.sort_by_key(|e| (e.action, e.modifiers));
        w.list(&actions, |w, e| {
            w.u32(e.action);
            w.u32(e.modifiers);
            w.u32(e.class);
            w.u32(e.value);
            w.f32(e.extent);
        });
        let mut texts = self.texts.clone();
        texts.sort_by_key(|t| t.action);
        w.list(&texts, |w, t| {
            w.u32(t.action);
            w.u32(t.name);
            w.u32(t.description);
        });
    }
}

impl KeyMap {
    /// Write the record as [`KeyMap`] decodes it.
    // D2-04889, D2-04899, D2-15350
    pub fn write(&self, id: u32, w: &mut auberean_dat::Writer) {
        w.u32(id);
        w.narrow_string(&self.name);
        w.bytes(&self.guid);
        w.list(&self.bindings, |w, b| {
            w.u32(b.action);
            w.u32(b.input);
            w.narrow_string(&b.name);
        });
    }
}

fn unique<T: Ord>(r: &Reader<'_>, keys: impl Iterator<Item = T>, what: &str) -> Result<()> {
    let mut seen = BTreeSet::new();
    for k in keys {
        if !seen.insert(k) {
            return r.fail(format!("a repeated {what}"));
        }
    }
    Ok(())
}

impl Decode for ActionBindings {
    /// The id, the string table, a counted list of toggle pairs, a counted list of actions (action,
    /// modifiers, class, value, extent), a counted list of texts (action, name, description). Each
    /// list's keys are unique: the client refuses a map that repeats one.
    // D2-03429
    fn decode(r: &mut Reader<'_>, id: u32, _ctx: &Context) -> Result<Self> {
        r.expect_id(id)?;
        let string_table = r.data_id()?;
        let toggles = r.list(8, |r| Ok((r.u32()?, r.u32()?)))?;
        unique(r, toggles.iter().map(|t| t.0), "toggle")?;
        let actions = r.list(20, |r| {
            Ok(ActionEntry {
                action: r.u32()?,
                modifiers: r.u32()?,
                class: r.u32()?,
                value: r.u32()?,
                extent: r.f32()?,
            })
        })?;
        unique(r, actions.iter().map(|a| (a.action, a.modifiers)), "action")?;
        let texts = r.list(12, |r| {
            Ok(ActionText {
                action: r.u32()?,
                name: r.u32()?,
                description: r.u32()?,
            })
        })?;
        unique(r, texts.iter().map(|t| t.action), "action text")?;
        Ok(Self {
            string_table,
            toggles,
            actions,
            texts,
        })
    }
}
