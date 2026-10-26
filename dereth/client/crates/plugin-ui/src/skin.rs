//! The skins plugin windows are drawn in under an interface with no art of the kit's.
//!
//! - **Retail plain** ([`retail_plain`]): the game's own label font, the game's icons and the
//!   game's windows' colours, with no pieces (every widget drawn plainly from the palette).
//! - **Retail** ([`retail`]): as retail plain, with the game's own window art cut from its data
//!   ([`RETAIL`]): the panels' frame and tab, their close button, the game's buttons, check
//!   boxes, scrollbar, slider, fields, its plain box (as its tooltips are drawn), a list's chosen
//!   row and a panel's divider. A part whose pictures are not all in the data is drawn plainly.
//! - **Classic** ([`classic`]): the classic interface's own 2005 art from the classic portal
//!   ([`CLASSIC`]), its own text ([`atlas_font`]) and its colours ([`classic_palette`]).

use std::sync::Arc;

use dereth_ui_kit::art::{Art, ArtSource, GivenFont, GivenPieces, Image};
use dereth_ui_kit::font::{Font, Glyph};
use dereth_ui_kit::palette::Palette;

/// The game's label font: what plugin windows' text is drawn in.
pub const FONT: u32 = 0x4000_0001;

/// A font of the game's data as the kit draws it: each glyph where its sheet holds it, placed by
/// its bearings, and the sheet (`width` by `height`, four bytes a pixel in blue, green, red,
/// alpha order, the glyphs' coverage in its alpha) made white, so text takes any colour.
#[must_use]
pub fn retail_font(
    font: &dereth_assets::ui::Font,
    width: u32,
    height: u32,
    bgra: &[u8],
) -> GivenFont {
    // The bearings are signed bytes in the font's records.
    #[allow(clippy::cast_possible_wrap)]
    let signed = |v: u8| v as i8;
    let glyphs = font
        .chars
        .iter()
        .filter_map(|c| {
            let ch = char::from_u32(u32::from(c.unicode))?;
            Some((
                ch,
                Glyph {
                    ch,
                    page: 0,
                    x: c.offset_x,
                    y: c.offset_y,
                    w: c.width,
                    h: c.height,
                    advance_adjust: signed(c.h_offset_before)
                        .saturating_add(signed(c.h_offset_after)),
                    y_offset: signed(c.v_offset_before),
                    x_offset: signed(c.h_offset_before),
                },
            ))
        })
        .collect();
    let line = i32::try_from(font.max_char_height).unwrap_or(16);
    let page: Vec<u8> = bgra
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| [255, 255, 255, p[3]])
        .collect();
    #[allow(clippy::cast_precision_loss)]
    let point_size = line as f32;
    GivenFont {
        font: Font {
            point_size,
            pixel_size: point_size,
            line_height: line,
            ascent: i32::try_from(font.baseline_offset).unwrap_or(line),
            glyphs,
            kerning: std::collections::HashMap::new(),
        },
        pages: vec![Arc::new(Image {
            width,
            height,
            bgra: page,
        })],
    }
}

/// The retail plain skin: no pieces, the game's windows' colours, `font` for its text, and the
/// game's icons from `icons`.
#[must_use]
pub fn retail_plain(font: Option<GivenFont>, icons: Option<Arc<dyn ArtSource>>) -> Art {
    let art = Art::plain(font, Palette::retail_plain());
    if let Some(icons) = icons {
        art.set_source(icons);
    }
    art
}

/// One piece of a skin cut from the game's data: the kit's name for it, the picture it comes from
/// (an image of the data), the part of that picture it is, `[x, y, w, h]` (the whole picture
/// when `None`), and whether its black is clear (as the classic interface draws its controls).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cut {
    pub name: &'static str,
    pub image: u32,
    pub part: Option<[u32; 4]>,
    pub keyed: bool,
}

/// A part of a skin drawn together (a frame, a button in all its states): taken whole or not at
/// all, so a part missing any of its pictures is drawn plainly. `source` says where in the game's
/// layouts or screens its pictures stand; `values` are what the kit lays its pieces out by.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Part {
    pub source: &'static str,
    pub cuts: &'static [Cut],
    pub values: &'static [(&'static str, &'static [f32])],
}

const fn cut(name: &'static str, image: u32) -> Cut {
    Cut {
        name,
        image,
        part: None,
        keyed: false,
    }
}

/// Part `[x, y, w, h]` of picture `image`.
const fn cut_part(name: &'static str, image: u32, part: [u32; 4]) -> Cut {
    Cut {
        name,
        image,
        part: Some(part),
        keyed: false,
    }
}

/// The whole of picture `image`, its black clear.
const fn keyed(name: &'static str, image: u32) -> Cut {
    Cut {
        name,
        image,
        part: None,
        keyed: true,
    }
}

/// Part `[x, y, w, h]` of picture `image`, its black clear.
const fn keyed_part(name: &'static str, image: u32, part: [u32; 4]) -> Cut {
    Cut {
        name,
        image,
        part: Some(part),
        keyed: true,
    }
}

/// The retail skin: the game's own window art, part by part, from the 6096 data.
pub const RETAIL: &[Part] = &[
    Part {
        source:
            "the panels' window, layout 0x2100006E element 0x100005FE: its ground and its eight \
                 fixed edges",
        cuts: &[
            cut("window.fill", 0x0600_4CC2),
            cut("window.tl", 0x0600_74C3),
            cut("window.top", 0x0600_74BF),
            cut("window.tr", 0x0600_74C4),
            cut("window.left", 0x0600_74C0),
            cut("window.right", 0x0600_74C2),
            cut("window.bl", 0x0600_74C5),
            cut("window.bottom", 0x0600_74C1),
            cut("window.br", 0x0600_74C6),
        ],
        values: &[
            ("window.fill_inset", &[0.0, 0.0, 0.0, 0.0]),
            ("window.band", &[5.0, 30.0]),
            ("window.body", &[8.0, 34.0, 8.0, 8.0]),
            ("window.grip", &[2.0, 6.0]),
        ],
    },
    Part {
        source: "the panels' tab, layout 0x2100004D element 0x1000043A, chosen (state 0x0C): the \
                 window's title stands on it",
        cuts: &[
            cut("window.title.left", 0x0600_5D92),
            cut("window.title.mid", 0x0600_5D94),
            cut("window.title.right", 0x0600_5D96),
        ],
        values: &[("window.title", &[5.0, 30.0])],
    },
    Part {
        source: "a panel's close button, layout 0x21000027 element 0x100001F5 (states 1 and 3)",
        cuts: &[
            cut("window.close.normal", 0x0600_1393),
            cut("window.close.hover", 0x0600_1393),
            cut("window.close.pressed", 0x0600_1394),
        ],
        values: &[("window.close", &[29.0, 25.0, 24.0])],
    },
    Part {
        source: "the game's button, layout 0x21000040 element 0x100002CD: its ends and middle at \
                 rest (state 1), held (3) and disabled (0x0D)",
        cuts: &[
            cut("button.normal.left", 0x0600_4C96),
            cut("button.normal.mid", 0x0600_4C99),
            cut("button.normal.right", 0x0600_4C9C),
            cut("button.hover.left", 0x0600_4C96),
            cut("button.hover.mid", 0x0600_4C99),
            cut("button.hover.right", 0x0600_4C9C),
            cut("button.pressed.left", 0x0600_4C95),
            cut("button.pressed.mid", 0x0600_4C98),
            cut("button.pressed.right", 0x0600_4C9B),
            cut("button.disabled.left", 0x0600_4C94),
            cut("button.disabled.mid", 0x0600_4C97),
            cut("button.disabled.right", 0x0600_4C9A),
        ],
        values: &[],
    },
    Part {
        source:
            "the panels' tab, layout 0x2100004D element 0x1000043A, chosen (state 0x0C) and not \
                 (0x0B)",
        cuts: &[
            cut("tab.selected.left", 0x0600_5D92),
            cut("tab.selected.mid", 0x0600_5D94),
            cut("tab.selected.right", 0x0600_5D96),
            cut("tab.unselected.left", 0x0600_5D93),
            cut("tab.unselected.mid", 0x0600_5D95),
            cut("tab.unselected.right", 0x0600_5D97),
            cut("tab.hover.left", 0x0600_5D93),
            cut("tab.hover.mid", 0x0600_5D95),
            cut("tab.hover.right", 0x0600_5D97),
        ],
        values: &[],
    },
    Part {
        source:
            "the options' check box, layout 0x21000044 element 0x1000032B, its box 0x10000328: \
                 off (state 1), off held (3), on (6), on held (8), disabled (0x0D)",
        cuts: &[
            cut("control.box", 0x0600_4D15),
            cut("control.box-pressed", 0x0600_4D16),
            cut("control.box-on", 0x0600_4D17),
            cut("control.box-on-pressed", 0x0600_4D18),
            cut("control.box-disabled", 0x0600_4D19),
        ],
        values: &[],
    },
    Part {
        source: "the scrollbar, layout 0x2100003E element 0x10000367: its track, its arrows \
                 0x10000071 and 0x10000072 at rest, and its thumb's top, middle and foot",
        cuts: &[
            cut("scroll.track-top", 0x0600_4C69),
            cut("scroll.track-mid", 0x0600_4C5F),
            cut("scroll.track-bottom", 0x0600_4C6C),
            cut("scroll.up", 0x0600_4C69),
            cut("scroll.down", 0x0600_4C6C),
            cut("scroll.thumb-top", 0x0600_4C60),
            cut("scroll.thumb-mid", 0x0600_4C63),
            cut("scroll.thumb-grip", 0x0600_4C63),
            cut("scroll.thumb-bottom", 0x0600_4C66),
        ],
        values: &[("scroll.cap", &[16.0]), ("scroll.channel", &[0.0, 0.0])],
    },
    Part {
        source: "the options' slider, layout 0x2100003E element 0x1000036E: its track and its knob",
        cuts: &[
            cut("slider.track", 0x0600_1285),
            cut("slider.knob", 0x0600_1286),
        ],
        values: &[],
    },
    Part {
        source: "the text field, layout 0x21000043 element 0x10000362: its ends and its middle",
        cuts: &[
            cut("field.left", 0x0600_5EAE),
            cut("field.mid", 0x0600_5EAF),
            cut("field.right", 0x0600_5EAE),
        ],
        values: &[],
    },
    Part {
        source:
            "the menu box, layout 0x21000043 element 0x1000035B: the arrow at its field's right \
                 end",
        cuts: &[cut_part("field.arrow", 0x0600_5E92, [132, 0, 18, 18])],
        values: &[],
    },
    Part {
        source: "the game's plain box, as its tooltips are drawn: layout 0x21000042 element \
                 0x100002B6, its ground and its edges",
        cuts: &[
            cut("plate.fill", 0x0600_4CC2),
            cut("plate.rule", 0x0600_4CC8),
        ],
        values: &[],
    },
    Part {
        source: "a list's row, layout 0x2100005D element 0x10000519, chosen (state 6)",
        cuts: &[cut("list.selected", 0x0600_1AAF)],
        values: &[],
    },
    Part {
        source: "a panel's divider, layout 0x2100005D element 0x10000254",
        cuts: &[cut("rule", 0x0600_1420)],
        values: &[],
    },
];

/// `part` of `image`, or the whole of it; `None` when the part falls outside it.
fn cut_out(image: &Image, part: Option<[u32; 4]>) -> Option<Image> {
    let Some([x, y, w, h]) = part else {
        return Some(image.clone());
    };
    if w == 0 || h == 0 || x + w > image.width || y + h > image.height {
        return None;
    }
    let mut bgra = Vec::with_capacity((w * h * 4) as usize);
    for row in y..y + h {
        let at = ((row * image.width + x) * 4) as usize;
        bgra.extend_from_slice(&image.bgra[at..at + (w * 4) as usize]);
    }
    Some(Image {
        width: w,
        height: h,
        bgra,
    })
}

/// Make `image`'s black clear, as the classic interface draws its controls: a pixel darker than
/// the screen's sixteen-bit colours can tell from black.
fn clear_black(image: &mut Image) {
    for px in image.bgra.as_chunks_mut::<4>().0 {
        if px[2] < 8 && px[1] < 4 && px[0] < 8 {
            *px = [0; 4];
        }
    }
}

/// The pieces of `parts`, each part's pictures read by `load` (an image of the game's data
/// decoded, `None` where the data has no such picture): only the parts whose pictures are all
/// there.
#[must_use]
pub fn pieces(parts: &[Part], load: &dyn Fn(u32) -> Option<Image>) -> GivenPieces {
    let mut out = GivenPieces::default();
    let mut read: std::collections::BTreeMap<u32, Option<Arc<Image>>> =
        std::collections::BTreeMap::new();
    for part in parts {
        let cut: Option<Vec<(&str, Image)>> = part
            .cuts
            .iter()
            .map(|c| {
                let image = read
                    .entry(c.image)
                    .or_insert_with(|| load(c.image).map(Arc::new))
                    .clone()?;
                Some((c.name, cut_out(&image, c.part)?))
            })
            .collect();
        let Some(mut cut) = cut else {
            continue;
        };
        for ((_, image), c) in cut.iter_mut().zip(part.cuts) {
            if c.keyed {
                clear_black(image);
            }
        }
        for (name, image) in cut {
            out.pieces.insert(name.to_owned(), Arc::new(image));
        }
        for (name, v) in part.values {
            out.values.insert((*name).to_owned(), v.to_vec());
        }
    }
    out
}

/// The retail skin: the game's window art from its data where `load` finds it ([`RETAIL`]), the
/// retail plain skin's drawing where it does not; `font` for its text and the game's icons from
/// `icons`.
#[must_use]
pub fn retail(
    font: Option<GivenFont>,
    icons: Option<Arc<dyn ArtSource>>,
    load: &dyn Fn(u32) -> Option<Image>,
) -> Art {
    let pieces = pieces(RETAIL, load);
    let mut palette = Palette::retail_plain();
    if pieces.has("window.tl") {
        // The frame's top edge and the tab under it.
        palette.header = 30.0;
    }
    if pieces.has("window.close.normal") {
        palette.close_side = 25.0;
    }
    if pieces.has("control.box") {
        palette.check_side = 13.0;
    }
    let art = Art::plain(font, palette).with_pieces(pieces);
    if let Some(icons) = icons {
        art.set_source(icons);
    }
    art
}

/// The classic skin: the classic interface's own 2005 art, part by part, from the classic portal.
pub const CLASSIC: &[Part] = &[
    Part {
        source: "the side pages' ground, edged with the inventory's pack area's rules (3 pixels): \
                 their corners cut from the rule's ends",
        cuts: &[
            cut("window.fill", 0x0600_1398),
            cut_part("window.tl", 0x0600_11B9, [0, 0, 3, 3]),
            cut("window.top", 0x0600_11B9),
            cut_part("window.tr", 0x0600_11B9, [29, 0, 3, 3]),
            cut("window.left", 0x0600_11BA),
            cut("window.right", 0x0600_11BA),
            cut_part("window.bl", 0x0600_11B9, [0, 0, 3, 3]),
            cut("window.bottom", 0x0600_11B9),
            cut_part("window.br", 0x0600_11B9, [29, 0, 3, 3]),
        ],
        values: &[
            ("window.fill_inset", &[0.0, 0.0, 0.0, 0.0]),
            ("window.band", &[3.0, 33.0]),
            ("window.body", &[8.0, 37.0, 8.0, 8.0]),
            ("window.grip", &[3.0, 5.0]),
        ],
    },
    Part {
        source: "a side page's title strip (276 by 30): its ends and its middle",
        cuts: &[
            cut_part("window.title.left", 0x0600_127B, [0, 0, 16, 30]),
            cut_part("window.title.mid", 0x0600_127B, [16, 0, 244, 30]),
            cut_part("window.title.right", 0x0600_127B, [260, 0, 16, 30]),
        ],
        values: &[("window.title", &[3.0, 27.0])],
    },
    Part {
        source: "a side page's close button (24 by 30), at rest and pressed",
        cuts: &[
            keyed("window.close.normal", 0x0600_1393),
            keyed("window.close.hover", 0x0600_1393),
            keyed("window.close.pressed", 0x0600_1394),
        ],
        values: &[("window.close", &[27.0, 30.0, 24.0])],
    },
    Part {
        source: "the classic button: its body (300 by 36) cut to its left end and middle, and its \
                 right end (32 by 36), at rest, pressed and disabled",
        cuts: &[
            keyed_part("button.normal.left", 0x0600_1207, [0, 0, 150, 36]),
            keyed_part("button.normal.mid", 0x0600_1207, [140, 0, 20, 36]),
            keyed("button.normal.right", 0x0600_1206),
            keyed_part("button.hover.left", 0x0600_1207, [0, 0, 150, 36]),
            keyed_part("button.hover.mid", 0x0600_1207, [140, 0, 20, 36]),
            keyed("button.hover.right", 0x0600_1206),
            keyed_part("button.pressed.left", 0x0600_1208, [0, 0, 150, 36]),
            keyed_part("button.pressed.mid", 0x0600_1208, [140, 0, 20, 36]),
            keyed("button.pressed.right", 0x0600_1209),
            keyed_part("button.disabled.left", 0x0600_120A, [0, 0, 150, 36]),
            keyed_part("button.disabled.mid", 0x0600_120A, [140, 0, 20, 36]),
            keyed("button.disabled.right", 0x0600_1205),
        ],
        values: &[],
    },
    Part {
        source:
            "the options page's tabs: a body (138 by 25) and its end (32 by 25), chosen and not",
        cuts: &[
            keyed_part("tab.selected.left", 0x0600_128F, [0, 0, 20, 25]),
            keyed_part("tab.selected.mid", 0x0600_128F, [20, 0, 98, 25]),
            keyed("tab.selected.right", 0x0600_1290),
            keyed_part("tab.unselected.left", 0x0600_1291, [0, 0, 20, 25]),
            keyed_part("tab.unselected.mid", 0x0600_1291, [20, 0, 98, 25]),
            keyed("tab.unselected.right", 0x0600_1292),
            keyed_part("tab.hover.left", 0x0600_1291, [0, 0, 20, 25]),
            keyed_part("tab.hover.mid", 0x0600_1291, [20, 0, 98, 25]),
            keyed("tab.hover.right", 0x0600_1292),
        ],
        values: &[],
    },
    Part {
        source: "the classic check box (13 by 13): ticked, and not; held, the other",
        cuts: &[
            keyed("control.box", 0x0600_128D),
            keyed("control.box-pressed", 0x0600_128B),
            keyed("control.box-on", 0x0600_128B),
            keyed("control.box-on-pressed", 0x0600_128D),
        ],
        values: &[],
    },
    Part {
        source:
            "the narrow scrollbar: its arrows, its track, and its thumb (16 by 16) cut into its \
                 top, middle and foot",
        cuts: &[
            keyed("scroll.track-top", 0x0600_1261),
            keyed("scroll.track-mid", 0x0600_1260),
            keyed("scroll.track-bottom", 0x0600_125E),
            keyed("scroll.up", 0x0600_1261),
            keyed("scroll.down", 0x0600_125E),
            keyed_part("scroll.thumb-top", 0x0600_1263, [0, 0, 16, 3]),
            keyed_part("scroll.thumb-mid", 0x0600_1263, [0, 3, 16, 10]),
            keyed_part("scroll.thumb-grip", 0x0600_1263, [0, 3, 16, 10]),
            keyed_part("scroll.thumb-bottom", 0x0600_1263, [0, 13, 16, 3]),
        ],
        values: &[("scroll.cap", &[16.0]), ("scroll.channel", &[0.0, 0.0])],
    },
    Part {
        source: "the settings pages' slider: its track and its thumb",
        cuts: &[
            keyed("slider.track", 0x0600_1285),
            keyed("slider.knob", 0x0600_1286),
        ],
        values: &[],
    },
    Part {
        source: "the options page's choice face (120 by 18): its ends and its middle",
        cuts: &[
            cut_part("field.left", 0x0600_1287, [0, 0, 4, 18]),
            cut_part("field.mid", 0x0600_1287, [4, 0, 112, 18]),
            cut_part("field.right", 0x0600_1287, [116, 0, 4, 18]),
        ],
        values: &[],
    },
    Part {
        source: "the options page's choice arrow (20 by 19)",
        cuts: &[keyed("field.arrow", 0x0600_1274)],
        values: &[],
    },
    Part {
        source: "the side pages' ground, ruled with the pack area's rule",
        cuts: &[
            cut("plate.fill", 0x0600_1398),
            cut_part("plate.rule", 0x0600_11B9, [0, 1, 32, 1]),
        ],
        values: &[],
    },
    Part {
        source: "the social page's chosen member's row (281 by 16)",
        cuts: &[cut("list.selected", 0x0600_1451)],
        values: &[],
    },
    Part {
        source: "the examine and settings pages' divider (292 by 8)",
        cuts: &[cut("rule", 0x0600_12C5)],
        values: &[],
    },
];

/// The classic interface's text: cream, grey where disabled, no edge round the letters.
const CLASSIC_INK: u32 = 0xFFD2_D2C8;
const CLASSIC_GREY: u32 = 0xFF64_6464;
/// The classic interface's chosen row and text selection.
const CLASSIC_CHOSEN: u32 = 0xFF27_4657;

/// The classic interface's colours, where the classic skin draws plainly: its cream text, its
/// black fields and its blue choice.
#[must_use]
pub fn classic_palette() -> Palette {
    let mut p = Palette::retail_plain();
    p.label = CLASSIC_INK;
    p.label_disabled = CLASSIC_GREY;
    p.tooltip_text = CLASSIC_INK;
    p.title = CLASSIC_INK;
    p.tab_on = CLASSIC_INK;
    p.tab_off = 0xFF96_9696;
    p.field = 0xFF00_0000;
    p.field_lit = 0xFF15_1515;
    p.text_field = 0xFF00_0000;
    p.text_field_focus = 0xFF00_0000;
    p.track = 0xFF15_1515;
    p.list_ground = 0xFF00_0000;
    p.row_lit = CLASSIC_CHOSEN;
    p.row_hover = 0x6027_4657;
    p
}

/// One glyph of a font drawn into an atlas: its inked box there, the pen's distance to the box's
/// left, the baseline's to the box's top (negative above it), and the pen's advance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AtlasGlyph {
    pub ch: char,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub bearing_x: i32,
    pub bearing_y: i32,
    pub advance: i32,
}

/// A font drawn into an atlas as the kit draws it: each glyph on the atlas's one page, placed by
/// its bearings from a line `line_height` tall whose baseline is `baseline` below its top; the
/// atlas (`width` by `height`) white, the glyphs' coverage in its alpha.
#[must_use]
pub fn atlas_font(
    glyphs: &[AtlasGlyph],
    line_height: i32,
    baseline: i32,
    width: u32,
    height: u32,
    white: Vec<u8>,
) -> GivenFont {
    let narrow = |v: i32| i8::try_from(v.clamp(-128, 127)).unwrap_or(0);
    let glyphs = glyphs
        .iter()
        .map(|g| {
            (
                g.ch,
                Glyph {
                    ch: g.ch,
                    page: 0,
                    x: u16::try_from(g.x.max(0)).unwrap_or(0),
                    y: u16::try_from(g.y.max(0)).unwrap_or(0),
                    w: u8::try_from(g.w.clamp(0, 255)).unwrap_or(0),
                    h: u8::try_from(g.h.clamp(0, 255)).unwrap_or(0),
                    advance_adjust: narrow(g.advance - g.w),
                    y_offset: narrow(baseline + g.bearing_y),
                    x_offset: narrow(g.bearing_x),
                },
            )
        })
        .collect();
    #[allow(clippy::cast_precision_loss)]
    let point_size = line_height as f32;
    GivenFont {
        font: Font {
            point_size,
            pixel_size: point_size,
            line_height,
            ascent: baseline,
            glyphs,
            kerning: std::collections::HashMap::new(),
        },
        pages: vec![Arc::new(Image {
            width,
            height,
            bgra: white,
        })],
    }
}

/// The classic skin: the classic interface's own art where `load` finds it in the classic portal
/// ([`CLASSIC`]), its colours drawn plainly where it does not; `font` (its own text) and the
/// game's icons from `icons`.
#[must_use]
pub fn classic(
    font: Option<GivenFont>,
    icons: Option<Arc<dyn ArtSource>>,
    load: &dyn Fn(u32) -> Option<Image>,
) -> Art {
    let pieces = pieces(CLASSIC, load);
    let mut palette = classic_palette();
    if pieces.has("window.tl") {
        // The frame's top edge and the title strip under it.
        palette.header = 33.0;
    }
    if pieces.has("window.close.normal") {
        palette.close_side = 24.0;
    }
    if pieces.has("control.box") {
        palette.check_side = 13.0;
    }
    let art = Art::plain(font, palette).with_pieces(pieces);
    if let Some(icons) = icons {
        art.set_source(icons);
    }
    art
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (this client's own skin for plugin windows)
    use super::*;

    /// A glyph of the game's font keeps its place on the sheet and its bearings, and the sheet
    /// becomes white, keeping its coverage, the alpha, whatever its colour.
    #[test]
    fn a_game_font_s_glyphs_keep_their_bearings_and_its_sheet_becomes_coverage() {
        let font = dereth_assets::ui::Font {
            id: dereth_primitives::DataId(FONT),
            max_char_height: 14,
            max_char_width: 9,
            chars: vec![dereth_assets::ui::FontChar {
                unicode: u16::from(b'A'),
                offset_x: 3,
                offset_y: 4,
                width: 7,
                height: 10,
                h_offset_before: 1,
                h_offset_after: 2,
                v_offset_before: 3,
            }],
            num_horizontal_border_pixels: 0,
            num_vertical_border_pixels: 0,
            baseline_offset: 11,
            foreground_surface: dereth_primitives::DataId(0),
            background_surface: dereth_primitives::DataId(0),
        };
        let sheet = [
            0, 0, 0, 255, 200, 200, 200, 255, 255, 255, 255, 40, 0, 0, 0, 0,
        ];
        let given = retail_font(&font, 2, 2, &sheet);
        let a = given.font.glyph('A').expect("A");
        assert_eq!((a.x, a.y, a.w, a.h), (3, 4, 7, 10));
        assert_eq!(a.advance(), 7 + 1 + 2, "its bearings either side");
        assert_eq!((a.x_offset, a.y_offset), (1, 3));
        assert_eq!(given.font.line_height, 14);
        let alpha: Vec<u8> = given.pages[0].bgra.chunks(4).map(|p| p[3]).collect();
        assert_eq!(alpha, [255, 255, 40, 0]);
        assert!(given.pages[0]
            .bgra
            .chunks(4)
            .all(|p| p[..3] == [255, 255, 255]));
    }
}
