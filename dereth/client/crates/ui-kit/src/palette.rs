//! How a skin draws where it has no pieces of its own: the plain colours, the window's plain
//! chrome and the text's edge. Every widget falls back to plain drawing piece by piece, so a skin
//! with no pieces at all is drawn entirely from its palette, and a skin with some is drawn from
//! its palette only where they are missing.

use crate::draw::Argb;

/// A skin's plain drawing. The default is the kit's own plain look; a skin of the game's own
/// colours sets its own ([`Palette::retail_plain`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    /// The text's edge (an outline drawn round every letter); a clear colour draws none.
    pub edge: Argb,
    /// A control's caption, and one that cannot be used now, and one lit (chosen of several).
    pub label: Argb,
    pub label_disabled: Argb,
    pub label_lit: Argb,
    /// A tooltip's lines under its title.
    pub tooltip_text: Argb,

    /// A window's or panel's plain ground, and the rule round it (none when clear).
    pub panel: Argb,
    pub panel_border: Argb,
    /// A plain window's title bar: its height and the inset of its body below it (left, top,
    /// right, bottom), in layout units; the strip under its title (none when clear); the title's
    /// size in points, colour and place from the window's top left, or, when `title_centred`,
    /// how far in from the left it starts, its letters centred down the bar.
    pub header: f32,
    pub body_inset: [f32; 4],
    pub title_strip: Argb,
    pub title_points: f32,
    pub title: Argb,
    pub title_at: (f32, f32),
    pub title_centred: bool,
    /// Whether a plain window has a close box of its own in its title bar's right end (with
    /// none, only the pad's cancel or the window's own buttons close it), and its side.
    pub close_box: bool,
    pub close_side: f32,

    /// A plain button's face, its rule (none when clear) and the glow over it under the pointer.
    pub button: Argb,
    pub button_border: Argb,
    pub button_glow: Argb,

    /// A row of tabs: the chosen tab's caption and the others'; and, drawn plainly, their faces
    /// (none when clear).
    pub tab_on: Argb,
    pub tab_off: Argb,
    pub tab_face_on: Argb,
    pub tab_face_off: Argb,

    /// A plain check box: its side in layout units, its well, the well under the pointer, the
    /// rule along its top, the rule round it (none when clear) and its mark.
    pub check_side: f32,
    pub check: Argb,
    pub check_over: Argb,
    pub check_rule: Argb,
    pub check_frame: Argb,
    pub check_mark: Argb,
    /// How far inside the box its mark is drawn, in layout units.
    pub check_inset: f32,

    /// A plain gauge's channel.
    pub gauge_track: Argb,
    /// A plain slider's track, its fill to the value and its knob.
    pub track: Argb,
    pub track_fill: Argb,
    pub knob: Argb,
    /// A plain stepper's middle and its arrows, and its arrows under the pointer.
    pub stepper_well: Argb,
    pub stepper: Argb,
    pub stepper_over: Argb,

    /// A plain field (a dropdown box's, a text box's): its ground, lit (open, or with the
    /// keyboard) and not, and its rule, lit and not.
    pub field: Argb,
    pub field_lit: Argb,
    pub field_rule: Argb,
    pub field_rule_lit: Argb,
    /// A plain text box's ground and the rule along its foot, with the keyboard and without.
    pub text_field: Argb,
    pub text_field_focus: Argb,
    pub text_rule: Argb,
    pub text_rule_focus: Argb,
    /// Whether arrows (a dropdown's, a stepper's) are drawn as shapes rather than as glyphs of
    /// the font, for a font that has no arrows.
    pub drawn_arrows: bool,

    /// An open list's opaque ground, a row under the pointer, and a chosen row drawn plainly.
    pub list_ground: Argb,
    pub row_hover: Argb,
    pub row_lit: Argb,
    /// A plain scrollbar's track and thumb.
    pub scroll_track: Argb,
    pub scroll_thumb: Argb,
}

impl Default for Palette {
    fn default() -> Self {
        Self {
            edge: 0xFF00_0000,
            label: 0xFFEE_E1C5,
            label_disabled: 0xFF80_8080,
            label_lit: 0xFFFF_D878,
            tooltip_text: 0xFFFF_FFFF,
            panel: 0xFF20_2020,
            panel_border: 0,
            header: crate::widgets::TITLE_BAR,
            body_inset: [12.0, crate::widgets::BODY_TOP, 12.0, 10.0],
            title_strip: 0,
            title_points: crate::widgets::TITLE_POINTS,
            title: 0xFFEE_E1C5,
            title_at: (14.0, 0.0),
            title_centred: true,
            close_box: false,
            close_side: 24.0,
            button: 0xFF30_3030,
            button_border: 0,
            button_glow: 0x20FF_C060,
            tab_on: 0xFFFF_FFFF,
            tab_off: 0xFFA0_A0A0,
            tab_face_on: 0,
            tab_face_off: 0,
            check_side: 20.0,
            check: 0xFF20_1C18,
            check_over: 0xFF40_3A30,
            check_rule: 0xFFC8_B27A,
            check_frame: 0,
            check_mark: 0xFFEE_E1C5,
            check_inset: 5.0,
            gauge_track: 0xFF20_1810,
            track: 0xFF20_1C18,
            track_fill: 0xFFC8_B27A,
            knob: 0xFFEE_E1C5,
            stepper_well: 0x6020_1C18,
            stepper: 0xFF30_2A22,
            stepper_over: 0xFF50_4638,
            field: 0xC020_1C18,
            field_lit: 0xE030_2A22,
            field_rule: 0xA0C8_B27A,
            field_rule_lit: 0xA0C8_B27A,
            text_field: 0x9010_0C08,
            text_field_focus: 0xE020_1C14,
            text_rule: 0x60C8_B27A,
            text_rule_focus: 0xFFC8_B27A,
            drawn_arrows: false,
            list_ground: 0xFF12_161C,
            row_hover: 0x30C8_9646,
            row_lit: 0x6040_A0C0,
            scroll_track: 0x4000_0000,
            scroll_thumb: 0xC0C8_B27A,
        }
    }
}

impl Palette {
    /// The game's own windows' colours, for a skin with no pieces drawn in the game's font: a
    /// near-black ground in a fine gold rule, a dark title strip, parchment text with no edge,
    /// dark brown controls ruled in gold, and a close box of its own on every window.
    #[must_use]
    pub fn retail_plain() -> Self {
        const BODY: Argb = 0xE618_120C;
        const BORDER: Argb = 0xFF8C_7A4E;
        const TITLE: Argb = 0xFF2E_2418;
        const TEXT: Argb = 0xFFE8_D8A8;
        const DIM: Argb = 0xFF8A_806A;
        const BUTTON: Argb = 0xFF3A_2E1E;
        const TRACK: Argb = 0xFF20_1A12;
        const GOLD: Argb = 0xFFD8_B860;
        Self {
            edge: 0,
            label: TEXT,
            label_disabled: DIM,
            label_lit: GOLD,
            tooltip_text: TEXT,
            panel: BODY,
            panel_border: BORDER,
            header: 20.0,
            body_inset: [6.0, 26.0, 6.0, 6.0],
            title_strip: TITLE,
            title_points: 13.0,
            title: TEXT,
            title_at: (6.0, 2.0),
            title_centred: false,
            close_box: true,
            close_side: 16.0,
            button: BUTTON,
            button_border: BORDER,
            button_glow: 0x30D8_B860,
            tab_on: TEXT,
            tab_off: DIM,
            tab_face_on: BUTTON,
            tab_face_off: TRACK,
            check_side: 11.0,
            check: TRACK,
            check_over: 0xFF2E_2418,
            check_rule: BORDER,
            check_frame: BORDER,
            check_mark: GOLD,
            check_inset: 3.0,
            gauge_track: TRACK,
            track: TRACK,
            track_fill: BORDER,
            knob: GOLD,
            stepper_well: TRACK,
            stepper: BUTTON,
            stepper_over: 0xFF4E_3E28,
            field: TRACK,
            field_lit: 0xFF2E_2418,
            field_rule: 0xFF5A_4E32,
            field_rule_lit: BORDER,
            text_field: TRACK,
            text_field_focus: 0xFF2A_2216,
            text_rule: 0xFF5A_4E32,
            text_rule_focus: BORDER,
            drawn_arrows: true,
            list_ground: 0xFF18_120C,
            row_hover: 0x30D8_B860,
            row_lit: 0x60D8_B860,
            scroll_track: 0xFF20_1A12,
            scroll_thumb: BORDER,
        }
    }
}
