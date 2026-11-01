//! Text elements and tooltips: typing and editing at the caret, the caret's blink, strings in the
//! element's numbered fonts and colors, the view scrolling, and a tooltip showing and going.

use auberean_assets::ui::AttributeValue as V;
use auberean_primitives::DataId;
use auberean_ui::text::Color;
use auberean_ui::types::text::key;
use auberean_ui::{Token, Ui};

use crate::fixture::{element, state, ui, FONT, LAYOUT};

/// A text element `name` at `rect` with these attributes and the test font.
fn text(name: &str, rect: [i16; 4], attrs: Vec<(&str, V)>) -> auberean_assets::ui::Element {
    let mut a = vec![("font", V::DataFile(DataId(FONT)))];
    a.extend(attrs);
    element(
        name,
        "text",
        rect,
        0,
        vec![state("stateless", 0, a, vec![], vec![])],
    )
}

fn entry(u: &mut Ui) -> auberean_ui::Handle {
    let l = u.load_layout(LAYOUT).unwrap();
    u.create_root(l, Token::of("Entry")).unwrap()
}

fn typed(u: &Ui, h: auberean_ui::Handle) -> String {
    u.text(h).unwrap_or_default()
}

/// An editable entry pressed takes the keyboard; typing goes in at the caret, no further than its
/// greatest length; Left and Home move the caret, Backspace drops the character before it and
/// Delete the one after; Shift with an arrow selects, and typing replaces the selection; Ctrl+C
/// copies it and Ctrl+V pastes over it.
///
/// Behaviour: ui.text.typing-and-editing-keys-work-at-the-caret
#[test]
fn typing_and_editing_keys_work_at_the_caret() {
    let e = text(
        "Entry",
        [0, 0, 199, 19],
        vec![
            ("editable", V::Bool(true)),
            ("selectable", V::Bool(true)),
            ("one_line", V::Bool(true)),
            ("max_chars", V::Int(8)),
        ],
    );
    let mut u = ui(vec![e], vec![]);
    let h = entry(&mut u);
    u.pointer_move(10, 10);
    u.pointer_press();
    u.pointer_release();
    assert_eq!(u.focus(), Some(h), "a press gives the entry the keyboard");
    assert!(u.type_text("abc"));
    u.character_input(0, key::LEFT);
    u.type_text("X");
    assert_eq!(typed(&u, h), "abXc", "typing goes in at the caret");
    u.backspace();
    assert_eq!(
        typed(&u, h),
        "abc",
        "Backspace drops the character before the caret"
    );
    u.character_input(0, key::HOME);
    u.character_input(0, key::DELETE);
    assert_eq!(typed(&u, h), "bc", "Delete drops the character after it");
    u.type_text("123456789");
    assert_eq!(
        typed(&u, h),
        "123456bc",
        "no further than its greatest length"
    );
    u.character_input(0, key::END);
    u.set_modifiers(true, false);
    u.character_input(0, key::LEFT);
    u.character_input(0, key::LEFT);
    u.set_modifiers(false, false);
    assert_eq!(u.selected_text(h), "bc");
    u.character_input(u16::from(b'z'), 0);
    assert_eq!(typed(&u, h), "123456z", "typing replaces the selection");
    u.select_all(h);
    u.set_modifiers(false, true);
    u.character_input(3, 0);
    assert_eq!(u.take_copied().as_deref(), Some("123456z"), "Ctrl+C copies");
    u.set_clipboard(Some("pq".into()));
    u.character_input(0x16, 0);
    u.set_modifiers(false, false);
    assert_eq!(typed(&u, h), "pq", "Ctrl+V pastes over the selection");
}

/// An editable entry's caret shows steadily for a second and a half after it moves, then blinks,
/// a second on and a second off.
///
/// Behaviour: ui.text.the-caret-shows-steadily-then-blinks
#[test]
fn the_caret_shows_steadily_then_blinks() {
    let e = text("Entry", [0, 0, 199, 19], vec![("editable", V::Bool(true))]);
    let mut u = ui(vec![e], vec![]);
    let h = entry(&mut u);
    u.use_time(0.0);
    u.pointer_move(10, 10);
    u.pointer_press();
    u.pointer_release();
    u.type_text("a");
    for (t, shown) in [
        (1.0, true),
        (1.6, false),
        (2.0, false),
        (2.7, true),
        (3.8, false),
    ] {
        u.use_time(t);
        assert_eq!(u.caret_shown(h), shown, "at {t} s");
    }
}

/// A string added in one of the element's numbered colors takes that color's red, green and blue
/// attributes (`font_color<n>_red` ...), one in the element's own color its `font_color_red` ...;
/// a channel the element lacks reads as full.
///
/// Behaviour: ui.text.strings-take-the-elements-numbered-font-colors
#[test]
fn strings_take_the_elements_numbered_font_colors() {
    let e = text(
        "Entry",
        [0, 0, 199, 19],
        vec![
            ("font_color_red", V::Float(1.0)),
            ("font_color_green", V::Float(0.0)),
            ("font_color3_red", V::Float(0.5)),
            ("font_color3_green", V::Float(0.25)),
            ("font_color3_blue", V::Float(0.75)),
        ],
    );
    let mut u = ui(vec![e], vec![]);
    let h = entry(&mut u);
    u.set_text(h, "");
    u.add_string_with_font(h, "x", 0, 3);
    u.add_string(h, "y");
    let g = &u.text_state(h).unwrap().glyphs.glyphs;
    assert_eq!(typed(&u, h), "xy");
    assert_eq!(
        g[0].color,
        Color {
            r: 0.5,
            g: 0.25,
            b: 0.75,
            a: 1.0
        }
    );
    assert_eq!(
        g[1].color,
        Color {
            r: 1.0,
            g: 0.0,
            b: 1.0,
            a: 1.0
        },
        "the missing blue reads as full"
    );
    assert_eq!(g[0].width, 6, "each glyph its width in the font");
}

/// A text taller than its element scrolls: to its end (the last lines at the foot), a page down
/// at a time within the text, and back home.
///
/// Behaviour: ui.text.the-view-scrolls-by-page-and-to-either-end
#[test]
fn the_view_scrolls_by_page_and_to_either_end() {
    let e = text("Entry", [0, 0, 99, 19], vec![]);
    let mut u = ui(vec![e], vec![]);
    let h = entry(&mut u);
    u.set_text(h, "a\nb\nc\nd\ne");
    u.recalculate_glyph_list(h);
    assert_eq!(u.total_lines(h), 5);
    u.scroll_to_end(h);
    assert_eq!(
        u.text_state(h).unwrap().view_y,
        30,
        "the last two lines show"
    );
    u.scroll_to_home(h);
    assert_eq!(u.text_state(h).unwrap().view_y, 0);
    u.vertical_page_increment(h);
    u.vertical_page_increment(h);
    assert_eq!(
        u.text_state(h).unwrap().view_y,
        30,
        "a page at a time, never past the end"
    );
}

/// Once the pointer has rested a quarter of a second on an element that shows tooltips, its
/// tooltip template comes up in front with the element's tip text, below and right of the pointer;
/// it goes when the pointer moves to another element.
///
/// Behaviour: ui.tooltip.a-tooltip-shows-once-the-pointer-rests-and-goes-when-it-leaves
#[test]
fn a_tooltip_shows_once_the_pointer_rests_and_goes_when_it_leaves() {
    let owner = element(
        "Owner",
        "field",
        [100, 100, 199, 199],
        5,
        vec![state(
            "stateless",
            0,
            vec![
                ("tooltips_on", V::Bool(true)),
                ("tooltip_ID", V::Id(Token::of("Tip").0)),
            ],
            vec![],
            vec![],
        )],
    );
    let tip_text = text("TipText", [4, 4, 53, 23], vec![]);
    let tip = element(
        "Tip",
        "field",
        [0, 0, 57, 27],
        0,
        vec![state(
            "stateless",
            0,
            vec![("tooltip_text_ID", V::Id(Token::of("TipText").0))],
            vec![],
            vec![tip_text],
        )],
    );
    let other = element("Other", "field", [300, 300, 399, 399], 5, vec![]);
    let mut u = ui(vec![owner, tip, other], vec![]);
    let l = u.load_layout(LAYOUT).unwrap();
    let o = u.create_root(l, Token::of("Owner")).unwrap();
    u.create_root(l, Token::of("Other")).unwrap();
    u.set_tooltip(o, "Hello");
    u.use_time(0.0);
    u.pointer_move(150, 150);
    u.use_time(0.1);
    assert!(u.tooltip().is_none(), "not before the delay");
    u.use_time(0.3);
    let t = u.tooltip().expect("the tooltip shows");
    assert_eq!(u.roots()[0], t, "in front of everything");
    let field = u.child_recursive(t, Token::of("TipText")).unwrap();
    assert_eq!(u.text(field).as_deref(), Some("Hello"));
    let r = u.screen_rect(t).unwrap();
    assert_eq!(
        (r.x0, r.y0),
        (150, 150),
        "at the pointer (no cursor to step past)"
    );
    assert_eq!(
        (r.width(), r.height()),
        (30 + 8, 10 + 8),
        "sized so its text element holds the text"
    );
    u.pointer_move(350, 350);
    assert!(u.tooltip().is_none(), "gone once the pointer leaves");
}
