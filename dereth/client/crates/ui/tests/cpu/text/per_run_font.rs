//! A text element carries a **font and a colour per appended run**: appending a run takes a font
//! index into the font array `0x1A` and a colour index into the colour array `0x1B` (and the tag
//! colour at `0x1D`), and the new glyphs are stamped with the element's current font and colour. An
//! index past the end of the array keeps the element's own font; the two-argument form means font
//! 0; a chat colour table still wins over the authored array; an element with no array answers its
//! own colour for every index.
//!
//! No shipped element can falsify the font half: the `<EXAM>` description pane `0x1000013C` authors
//! three colours and exactly one font, so on the retail data a wrong font index is invisible. The
//! tree is therefore built from a `LayoutDesc` written here, and every assertion reads the composed
//! draw data ([`dereth_ui::text::PlacedGlyph`] out of `TextElement::compose`), whose `font` is a
//! `DataId` resolved through the element's own `0x1A` array. No assets, no network.

use crate::common::NoAssets;
use dereth_assets::ui::BaseProperty;
use dereth_primitives::DataId;
use dereth_ui::desc::{incorporation, ElementDesc, LayoutDesc, StateDesc};
use dereth_ui::factory::ty;
use dereth_ui::props::attr;
use dereth_ui::{Box2D, ElemHandle, ElementId, PropertyValue, UiSystem};

const CONTAINER: u32 = 0x1000_0010;
const LABEL: u32 = 0x1000_0011;

/// The two fonts this file's layout authors at `0x1A`. Any two distinct ids will do; these are the
/// shipped `0x40000001` and its neighbour, so the values look like the data they stand in for.
const FONT_A: DataId = DataId(0x4000_0001);
const FONT_B: DataId = DataId(0x4000_0002);

/// The three colours at `0x1B`, in the `<EXAM>` description pane's own order and values: plain
/// white, `mod_high_font` green, `mod_low_font` red.
const PLAIN: u32 = 0xFFFF_FFFF;
const RAISED: u32 = 0xFF00_FF00;
const LOWERED: u32 = 0xFFFF_0000;

fn desc(id: u32, w: i32, h: i32) -> ElementDesc {
    ElementDesc {
        base: StateDesc {
            incorporation: incorporation::LEGACY_ALL_GEOMETRY,
            x: 0,
            y: 0,
            width: w,
            height: h,
            ..StateDesc::default()
        },
        element_id: ElementId(id),
        ty: if id == CONTAINER { ty::FIELD } else { ty::TEXT },
        ..ElementDesc::default()
    }
}

/// A container with one text element in it, with that element given the two authored arrays
/// through the real `on_set_attribute` path — **not** by writing the fields, which is the whole
/// point: the indices have to travel the same road the layout's bytes do.
fn label() -> (UiSystem, ElemHandle) {
    let mut ui = UiSystem::new((800, 600));
    let mut container = desc(CONTAINER, 400, 200);
    container
        .children
        .insert(ElementId(LABEL), desc(LABEL, 380, 180));
    let l = LayoutDesc {
        did: DataId(0x2100_0001),
        display_width: 800,
        display_height: 600,
        elements: std::iter::once((ElementId(CONTAINER), container)).collect(),
    };
    let d = l
        .access_element(ElementId(CONTAINER))
        .cloned()
        .expect("root");
    let c = ui
        .create_element_recursive_from_full_desc(&NoAssets, &l, &d)
        .expect("no inheritance")
        .expect("registered");
    let root = ui.root();
    ui.set_parent(c, Some(root));
    let h = ui
        .get_child_recursive(c, ElementId(LABEL))
        .expect("the label");

    // `0x1A` — the font array. The font-set helper reads element 0 at reset;
    // `append_text_with_font`'s second argument reads element *n*.
    ui.on_set_attribute(
        h,
        attr::TEXT_FONT_DID,
        Some(&PropertyValue::Array(vec![
            BaseProperty {
                id: attr::TEXT_FONT_DID,
                value: PropertyValue::DataFile(FONT_A),
            },
            BaseProperty {
                id: attr::TEXT_FONT_DID,
                value: PropertyValue::DataFile(FONT_B),
            },
        ])),
    );
    // `0x1B` — the colour array, which the append-with-font-and-colour third argument reads.
    ui.on_set_attribute(
        h,
        attr::TEXT_FONT_COLOR,
        Some(&PropertyValue::Array(vec![
            BaseProperty {
                id: attr::TEXT_FONT_COLOR,
                value: PropertyValue::Color(PLAIN),
            },
            BaseProperty {
                id: attr::TEXT_FONT_COLOR,
                value: PropertyValue::Color(RAISED),
            },
            BaseProperty {
                id: attr::TEXT_FONT_COLOR,
                value: PropertyValue::Color(LOWERED),
            },
        ])),
    );
    (ui, h)
}

/// The composed draw data: one `(char, font, colour)` per drawn glyph, in glyph order.
fn painted(ui: &mut UiSystem, h: ElemHandle) -> Vec<(char, DataId, u32)> {
    let screen = ui.screen_box(h);
    let t = ui.text_element_mut(h).expect("a text element");
    t.compose(screen)
        .into_iter()
        .map(|g| {
            (
                char::from_u32(u32::from(g.ch)).unwrap_or('?'),
                g.font,
                g.color,
            )
        })
        .collect()
}

/// Every distinct `(font, colour)` over the glyphs of the first occurrence of `needle`.
fn run_of(painted: &[(char, DataId, u32)], needle: &str) -> Vec<(DataId, u32)> {
    let text: String = painted.iter().map(|(c, _, _)| *c).collect();
    let at = text
        .find(needle)
        .unwrap_or_else(|| panic!("{needle:?} is not drawn in {text:?}"));
    let mut out = Vec::new();
    for (_, f, c) in &painted[at..at + needle.chars().count()] {
        if !out.contains(&(*f, *c)) {
            out.push((*f, *c));
        }
    }
    out
}

// =============================================================================================
// 1. The two arrays are read whole, not just element 0
// =============================================================================================

/// The font-set and colour-set helpers index the array, so the whole array has to survive the
/// attribute, not only element 0.
#[test]
fn the_element_keeps_both_authored_arrays_and_not_only_their_first_entry() {
    let (mut ui, h) = label();
    let t = ui.text_element_mut(h).expect("a text element");
    assert_eq!(t.fonts, vec![FONT_A, FONT_B], "the whole 0x1A array");
    assert_eq!(
        t.font_colors,
        vec![PLAIN, RAISED, LOWERED],
        "the whole 0x1B array"
    );
    assert_eq!(t.font_color, PLAIN, "the current font colour is element 0");
}

// =============================================================================================
// 2. The font half — per run, and measured at the draw data
// =============================================================================================

/// Behaviour: ui.text.each-appended-run-carries-its-own-font-and-colour
///
/// **Two runs in one text element draw with two different fonts**, and the font is a `DataId` the
/// renderer can load rather than an index only this crate understands.
#[test]
fn two_runs_in_one_element_draw_in_two_different_fonts() {
    let (mut ui, h) = label();
    {
        let t = ui.text_element_mut(h).expect("a text element");
        t.set_text("");
        t.append_text_with_font_and_color("AAA", 0, 0);
        t.append_text_with_font_and_color("BBB", 1, 0);
    }
    let p = painted(&mut ui, h);

    assert_eq!(
        run_of(&p, "AAA"),
        vec![(FONT_A, PLAIN)],
        "the first run is font 0 of the array"
    );
    assert_eq!(
        run_of(&p, "BBB"),
        vec![(FONT_B, PLAIN)],
        "the second run is font 1 of the array"
    );
    let fonts: Vec<DataId> = {
        let mut v: Vec<DataId> = p.iter().map(|(_, f, _)| *f).collect();
        v.dedup();
        v
    };
    assert_eq!(
        fonts,
        vec![FONT_A, FONT_B],
        "`dereth_client::ui_draw` groups its draw calls by exactly this sequence, so two fonts in one \
         element must be two runs and not one"
    );
}

/// An index past the end of the array leaves the run on the element's **current font**, which is
/// the font-set helper's early return on an index past the end, plus the fact that the
/// element's current font was last set to element 0 at reset.
///
/// Passing the index through unclamped gives `DataId(0)` out of `place`, which **rasterises
/// nothing at all**; `dereth_ui_screens::panels::statmgmt::resolve_font_and_color` applies the same
/// clamp. This test is a boundary control: the mutation it catches is removing the clamp,
/// not removing the index.
#[test]
fn a_font_index_past_the_end_of_the_array_keeps_the_elements_own_font() {
    let (mut ui, h) = label();
    {
        let t = ui.text_element_mut(h).expect("a text element");
        t.set_text("");
        t.append_text_with_font_and_color("ZZZ", 7, 0);
    }
    let p = painted(&mut ui, h);
    assert_eq!(
        run_of(&p, "ZZZ"),
        vec![(FONT_A, PLAIN)],
        "the clamp turns the font-set helper's no-op into element 0; without it `place` answers \
         DataId(0) and the run rasterises nothing"
    );
}

// =============================================================================================
// 3. The colour half — the mechanism the enchantment highlighting actually uses
// =============================================================================================

/// Behaviour: ui.text.each-appended-run-carries-its-own-font-and-colour
///
/// `mod_high_font` (1) and `mod_low_font` (2) against the plain 0, out of the element's own `0x1B`
/// array. This is what the item-info append's second argument selects.
#[test]
fn three_runs_draw_in_the_three_authored_colours() {
    let (mut ui, h) = label();
    {
        let t = ui.text_element_mut(h).expect("a text element");
        t.set_text("");
        t.append_text_with_font_and_color("PPP", 0, 0);
        t.append_text_with_font_and_color("HHH", 0, 1);
        t.append_text_with_font_and_color("LLL", 0, 2);
    }
    let p = painted(&mut ui, h);
    assert_eq!(run_of(&p, "PPP"), vec![(FONT_A, PLAIN)]);
    assert_eq!(
        run_of(&p, "HHH"),
        vec![(FONT_A, RAISED)],
        "mod_high_font, 0x1B[1]"
    );
    assert_eq!(
        run_of(&p, "LLL"),
        vec![(FONT_A, LOWERED)],
        "mod_low_font, 0x1B[2]"
    );
}

// =============================================================================================
// 4. The shorter forms and the chat colour table keep their meaning
// =============================================================================================

/// The two-argument `append_text_with_font` means "font 0", which is what every caller in the
/// workspace already meant: each stands in for a call site that pushes a literal `0` for the font
/// (the item-info append pushes a literal `0` for the font, and so do the chat interface's three
/// calls on the final-string notice).
#[test]
fn the_two_argument_form_still_means_font_zero() {
    let (mut ui, h) = label();
    {
        let t = ui.text_element_mut(h).expect("a text element");
        t.set_text("");
        t.append_text_with_font("OLD", 1);
    }
    let p = painted(&mut ui, h);
    assert_eq!(
        run_of(&p, "OLD"),
        vec![(FONT_A, RAISED)],
        "font 0 and the colour index it was given, which is exactly the old behaviour with a \
         three-entry array and the new behaviour with one"
    );
}

/// **The chat log is untouched.** Setting the default fills every `chat_colors` slot, and setting a
/// channel colour overwrites that slot. This table still wins over the authored `0x1B` array, so
/// per-type colours are unchanged.
#[test]
fn a_chat_colour_table_still_wins_over_the_authored_array() {
    let (mut ui, h) = label();
    const GREY: u32 = 0xFF80_8080;
    {
        let t = ui.text_element_mut(h).expect("a text element");
        t.set_default_chat_color(GREY, 0x22);
        t.set_chat_color(1, 0xFF00_00FF);
        t.set_text("");
        t.append_text_with_font("aaa", 1);
        t.append_text_with_font("bbb", 2);
    }
    let p = painted(&mut ui, h);
    assert_eq!(
        run_of(&p, "aaa"),
        vec![(FONT_A, 0xFF00_00FF)],
        "set_chat_color(1, blue), not the layout's 0x1B[1]"
    );
    assert_eq!(
        run_of(&p, "bbb"),
        vec![(FONT_A, GREY)],
        "set_default_chat_color's fill, not the layout's 0x1B[2]"
    );
}

/// An element with **no** colour array and no chat table — which is every text element the shipped
/// layouts give a single colour — answers with its own `font_color` for every index, as the
/// colour-set helper's bounds check does.
#[test]
fn an_element_with_no_array_answers_with_its_own_colour_for_every_index() {
    let mut ui = UiSystem::new((800, 600));
    let mut container = desc(CONTAINER, 400, 200);
    container
        .children
        .insert(ElementId(LABEL), desc(LABEL, 380, 180));
    let l = LayoutDesc {
        did: DataId(0x2100_0001),
        display_width: 800,
        display_height: 600,
        elements: std::iter::once((ElementId(CONTAINER), container)).collect(),
    };
    let d = l
        .access_element(ElementId(CONTAINER))
        .cloned()
        .expect("root");
    let c = ui
        .create_element_recursive_from_full_desc(&NoAssets, &l, &d)
        .expect("no inheritance")
        .expect("registered");
    let root = ui.root();
    ui.set_parent(c, Some(root));
    let h = ui
        .get_child_recursive(c, ElementId(LABEL))
        .expect("the label");
    let own = {
        let t = ui.text_element_mut(h).expect("a text element");
        assert!(t.font_colors.is_empty(), "nothing authored 0x1B here");
        t.set_text("");
        t.append_text_with_font_and_color("xxx", 0, 2);
        t.font_color
    };
    let p = painted(&mut ui, h);
    assert_eq!(run_of(&p, "xxx"), vec![(DataId(0), own)]);
    let _: Box2D = ui.screen_box(h);
}
