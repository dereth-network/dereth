//! Synthetic layouts and pictures held in memory, built element by element.

use std::collections::HashMap;
use std::sync::Arc;

use auberean_assets::ui::{
    Attribute, AttributeValue, CharacterMap, Element, ElementState, Font, Glyph, Media, Transition,
    TransitionChange, UiLayout,
};
use auberean_primitives::DataId;
use auberean_ui::{Assets, Image, Token, Ui};

/// Records in memory.
#[derive(Default)]
pub struct Records {
    pub layouts: HashMap<u32, Arc<UiLayout>>,
    pub images: HashMap<u32, Arc<Image>>,
    pub fonts: HashMap<u32, Arc<Font>>,
    /// Record ids by enumeration value and mapping.
    pub enums: HashMap<(u32, u32), u32>,
}

impl Assets for Records {
    fn layout(&self, id: u32) -> Option<Arc<UiLayout>> {
        self.layouts.get(&id).cloned()
    }
    fn image(&self, id: u32) -> Option<Arc<Image>> {
        self.images.get(&id).cloned()
    }
    fn font(&self, id: u32) -> Option<Arc<Font>> {
        self.fonts.get(&id).cloned()
    }
    fn string(&self, _table: u32, _token: u32) -> Option<String> {
        None
    }
    fn data_id_by_enum(&self, value: u32, mapping: u32) -> u32 {
        self.enums.get(&(value, mapping)).copied().unwrap_or(0)
    }
}

/// The layout every test builds into.
pub const LAYOUT: u32 = 0x2200_0001;

/// An element `name` of `kind` at `(l, t)-(r, b)` with depth `z`, edges `[1, 1, 1, 1]` and the
/// given states.
pub fn element(
    name: &str,
    kind: &str,
    rect: [i16; 4],
    z: u16,
    states: Vec<ElementState>,
) -> Element {
    Element {
        id: Token::of(name).0,
        kind: Token::of(kind).0,
        default_state: states.first().map_or(Token::of("stateless").0, |s| s.id),
        left: rect[0],
        top: rect[1],
        right: rect[2],
        bottom: rect[3],
        z,
        edges: [1, 1, 1, 1],
        states,
    }
}

/// A state `name` with attributes, media and children; no rectangle of its own.
pub fn state(
    name: &str,
    flags: u8,
    attributes: Vec<(&str, AttributeValue)>,
    media: Vec<Media>,
    children: Vec<Element>,
) -> ElementState {
    ElementState {
        id: Token::of(name).0,
        flags,
        left: -1,
        top: -1,
        right: -1,
        bottom: -1,
        z: 0xFFFF,
        attributes: attributes
            .into_iter()
            .map(|(n, value)| Attribute {
                id: Token::of(n).0,
                value,
            })
            .collect(),
        media,
        transitions: Vec::new(),
        children,
    }
}

/// A state with transitions besides.
pub fn state_with(
    name: &str,
    attributes: Vec<(&str, AttributeValue)>,
    media: Vec<Media>,
    transitions: Vec<Transition>,
) -> ElementState {
    let mut s = state(name, 0, attributes, media, vec![]);
    s.transitions = transitions;
    s
}

/// A fade from `from` to `to` over `seconds`, run as the element enters (`when` 1) or leaves
/// (2) a state, matching `state`; it starts from `from` itself.
pub fn fade(when: u32, state: &str, from: f32, to: f32, seconds: f64) -> Transition {
    Transition {
        kind: 3,
        when,
        state: Token::of(state).0,
        change: TransitionChange::Fade {
            end_alpha: to,
            start_alpha: from,
            duration: seconds,
            flags: 1,
        },
    }
}

/// An animation of `frames` over `seconds`.
pub fn animation(frames: Vec<u32>, seconds: f32) -> Media {
    Media::Animation {
        duration: seconds,
        draw_mode: 0,
        x: 0,
        y: 0,
        frames,
    }
}

/// A picture media item.
pub fn image(id: u32, mode: u32) -> Media {
    Media::Image {
        picture: DataId(id),
        draw_mode: mode,
        x: 0,
        y: 0,
    }
}

/// A solid picture `w` by `h` of one blue, green, red, alpha texel.
pub fn solid(w: u32, h: u32, bgra: [u8; 4]) -> Arc<Image> {
    Arc::new(Image {
        width: w,
        height: h,
        bgra: bgra.repeat((w * h) as usize),
        alpha_only: false,
    })
}

/// The font every text test writes in.
pub const FONT: u32 = 0x4200_0001;

/// A font of the printable ASCII characters, each 6 pixels wide and 10 high, with blank glyphs and
/// the map of its characters, as a font read from the files has.
pub fn font() -> Arc<Font> {
    let glyphs: Vec<Glyph> = (0x20u16..0x7F)
        .enumerate()
        .map(|(i, c)| Glyph {
            width: 6,
            code_point: c,
            unused: 0,
            offset: u32::try_from(i).unwrap_or(0) * 6,
        })
        .collect();
    let total_width = u32::try_from(glyphs.len()).unwrap_or(0) * 6;
    Arc::new(Font {
        max_height: 10,
        max_width: 6,
        average_width: 6,
        total_width,
        antialiased: true,
        pixels: vec![0; (total_width * 10) as usize],
        character_map: CharacterMap::of(&glyphs),
        glyphs,
    })
}

/// An interface on a 1024 by 768 screen over one layout of these top-level elements.
pub fn ui(elements: Vec<Element>, images: Vec<(u32, Arc<Image>)>) -> Ui {
    ui_on(elements, images, (1024, 768))
}

/// The same on a screen of another size.
pub fn ui_on(elements: Vec<Element>, images: Vec<(u32, Arc<Image>)>, screen: (i32, i32)) -> Ui {
    Ui::new(Arc::new(records(elements, images)), screen.0, screen.1)
}

/// The records [`ui`] builds over: the layout of these elements, the pictures and the test font.
pub fn records(elements: Vec<Element>, images: Vec<(u32, Arc<Image>)>) -> Records {
    let mut r = Records::default();
    r.layouts.insert(
        LAYOUT,
        Arc::new(UiLayout {
            references: Vec::new(),
            string_table: DataId(0),
            width: 1024,
            height: 768,
            elements,
        }),
    );
    r.images = images.into_iter().collect();
    r.fonts.insert(FONT, font());
    r
}
