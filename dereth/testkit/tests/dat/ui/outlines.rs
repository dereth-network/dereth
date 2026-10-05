//! UI fixtures and scenarios for outlines.

use super::*;
// =============================================================================================
// ui.text.outline.* -- the dark backing behind drawn text
//
// The refusal strip is drawn with an outline and the chat log is not, which is why the strip can
// look blurry beside the log. The raw count of elements that ask for an outline is the premise of
// the first scenario rather than a row, and the two sizings of the refusal string are legs of the
// third.
// =============================================================================================

/// The attribute a text element asks for an outline with.
const TEXT_OUTLINE: u32 = 0x21;
/// A refusal-strip string the outline scenarios measure.
const OWNER_STRING: &str = "You can't put that item there.";
/// The font the refusal strip is drawn in.
const STRIP_FONT: dereth_primitives::DataId = dereth_primitives::DataId(0x4000_0001);
/// The strip itself, and the layout it lives in.
const STRIP: (dereth_primitives::DataId, u32) =
    (dereth_primitives::DataId(0x2100_0011), 0x1000_004A);
/// The chat log, which asks for none -- which is why it looked fine.
const CHAT_LOG: u32 = 0x1000_0011;

fn the_data_files() -> dereth_dat::RetailDatStore {
    dereth_dat::testing::open_store().expect("the retail data files: set DERETH_TEST_DAT_DIR")
}

fn outline_property_types(s: &dereth_dat::RetailDatStore) -> dereth_assets::ui::PropertyTypes {
    use dereth_assets::Decode as _;
    use dereth_primitives::AssetSource as _;
    let id = dereth_primitives::DataId(0x3900_0001);
    let b = s.read(id).expect("the master property record");
    dereth_assets::MasterProperty::decode_payload(id, &b)
        .expect("it decodes")
        .property_types()
}

/// Every place in the shipped layouts where the attribute is declared, and what it says.
///
/// Every named look is searched as well as the element's own, because the client applies whichever
/// is current and missing one would undercount -- which reads exactly like the census being right.
fn outline_census(s: &dereth_dat::RetailDatStore) -> Vec<(dereth_primitives::DataId, u32, bool)> {
    use dereth_assets::ui::{ElementDesc, LayoutDesc, PropertyValue};
    use dereth_assets::Decode as _;
    use dereth_primitives::AssetSource as _;

    fn walk(e: &ElementDesc, out: &mut Vec<(u32, bool)>) {
        let mut v = None;
        for st in std::iter::once(&e.state).chain(e.states.iter().map(|(_, s)| s)) {
            for (_, p) in &st.properties {
                if p.id == TEXT_OUTLINE {
                    if let PropertyValue::Bool(b) = p.value {
                        v = Some(b);
                    }
                }
            }
        }
        if let Some(b) = v {
            out.push((e.element_id, b));
        }
        for (_, c) in &e.children {
            walk(c, out);
        }
    }

    let types = outline_property_types(s);
    let ids = s.ids_of(LayoutDesc::TYPE);
    assert!(
        ids.len() >= 100,
        "the shipped layout set: {} found",
        ids.len()
    );
    let mut out = Vec::new();
    let mut decoded = 0_usize;
    for id in &ids {
        let bytes = s.read(*id).expect("a layout the directory lists reads");
        let layout = LayoutDesc::decode_payload(*id, &bytes, &types).expect("it decodes");
        decoded += 1;
        for (_, e) in &layout.elements {
            let mut v = Vec::new();
            walk(e, &mut v);
            for (elem, b) in v {
                out.push((*id, elem, b));
            }
        }
    }
    assert_eq!(decoded, ids.len(), "every shipped layout was read");
    out
}

/// The attribute **resolved the way the client resolves it**: one look at a time, and along the
/// chain of things an element is built from. Written from the shipped descriptions alone, so it is
/// an oracle rather than a restatement of the thing under test.
fn resolved_outlines(s: &dereth_dat::RetailDatStore) -> std::collections::BTreeMap<u32, bool> {
    use dereth_assets::ui::{ElementDesc, LayoutDesc, PropertyValue};
    use dereth_assets::Decode as _;
    use dereth_primitives::{AssetSource as _, DataId};

    let types = outline_property_types(s);
    let mut layouts: std::collections::BTreeMap<DataId, LayoutDesc> =
        std::collections::BTreeMap::new();
    for id in s.ids_of(LayoutDesc::TYPE) {
        let bytes = s.read(id).expect("a layout reads");
        layouts.insert(
            id,
            LayoutDesc::decode_payload(id, &bytes, &types).expect("it decodes"),
        );
    }

    fn find(l: &LayoutDesc, id: u32) -> Option<&ElementDesc> {
        fn rec(e: &ElementDesc, id: u32) -> Option<&ElementDesc> {
            if e.element_id == id {
                return Some(e);
            }
            e.children.iter().find_map(|(_, c)| rec(c, id))
        }
        l.elements.iter().find_map(|(_, e)| rec(e, id))
    }

    fn declared(e: &ElementDesc, state: u32) -> Option<bool> {
        let read = |st: &dereth_assets::ui::StateDesc| -> Option<bool> {
            st.properties
                .iter()
                .find_map(|(_, p)| match (p.id, &p.value) {
                    (TEXT_OUTLINE, PropertyValue::Bool(b)) => Some(*b),
                    _ => None,
                })
        };
        let mut v = read(&e.state);
        for (sid, st) in &e.states {
            if *sid == state {
                if let Some(b) = read(st) {
                    v = Some(b);
                }
            }
        }
        v
    }

    fn resolve<'a>(
        layouts: &'a std::collections::BTreeMap<DataId, LayoutDesc>,
        mut layout: DataId,
        mut e: &'a ElementDesc,
        depth: usize,
    ) -> Option<bool> {
        let mut state = e.default_state;
        for _ in 0..depth {
            if let Some(b) = declared(e, state) {
                return Some(b);
            }
            if e.base_element == 0 {
                return None;
            }
            let bl = if e.base_layout.0 == 0 {
                layout
            } else {
                e.base_layout
            };
            let l = layouts.get(&bl)?;
            let next = find(l, e.base_element)?;
            if state == 0 {
                state = next.default_state;
            }
            layout = bl;
            e = next;
        }
        None
    }

    fn walk(
        layouts: &std::collections::BTreeMap<DataId, LayoutDesc>,
        id: DataId,
        e: &ElementDesc,
        out: &mut std::collections::BTreeMap<u32, bool>,
    ) {
        if let Some(b) = resolve(layouts, id, e, 16) {
            let slot = out.entry(e.element_id).or_insert(false);
            *slot = *slot || b;
        }
        for (_, c) in &e.children {
            walk(layouts, id, c, out);
        }
    }

    let mut out = std::collections::BTreeMap::new();
    for (id, l) in &layouts {
        for (_, e) in &l.elements {
            walk(&layouts, *id, e, &mut out);
        }
    }
    out
}

/// Build every shipped layout and record, per element, whether what it drew carried an outline.
fn drawn_outlines(s: &dereth_dat::RetailDatStore) -> std::collections::BTreeMap<u32, Option<u32>> {
    use dereth_assets::ui::LayoutDesc;
    use dereth_assets::Decode as _;
    use dereth_primitives::AssetSource as _;

    let types = outline_property_types(s);
    let mut out = std::collections::BTreeMap::new();
    for id in s.ids_of(LayoutDesc::TYPE) {
        let bytes = s.read(id).expect("a layout reads");
        let layout = LayoutDesc::decode_payload(id, &bytes, &types).expect("it decodes");
        let mut ui = UiSystem::new((800, 600));
        ui.property_types = types.clone();
        for (elem, _) in &layout.elements {
            // A layout whose root will not build is a standing condition of this client and not
            // this claim's subject; it is skipped, and the counts below say how many were seen.
            let _ = dereth_ui::framework::create_and_add_root_element_by_data_id(
                &mut ui,
                s,
                id,
                ElementId(*elem),
            );
        }
        let mut back = dereth_ui::RecordingDrawBackend::default();
        ui.draw(&mut back);
        for cmd in &back.calls {
            let Some(n) = ui.node(cmd.who) else { continue };
            let key = n.desc.element_id.0;
            // One element can draw twice; the drawing that carries the letters is the one with
            // the outline, so a colour wins over none.
            let e = out.entry(key).or_insert(None);
            if cmd.text_outline.is_some() {
                *e = cmd.text_outline;
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------
// ui.text.outline.the-refusal-strip-is-drawn-with-one-and-the-chat-log-is-not
// ---------------------------------------------------------------------------------------------

/// The refusal strip and the chat log, side by side.
pub(super) fn the_refusal_strip_is_drawn_with_an_outline_and_the_chat_log_is_not() {
    let s = the_data_files();
    let all = outline_census(&s);
    let asked: Vec<_> = all.iter().filter(|(_, _, b)| *b).collect();
    // The premise, and it is two numbers rather than one: declaring the attribute and asking for
    // an outline are different things, because two of them declare it false.
    let counted = all.len() == 71 && asked.len() == 69;
    let the_strip_asks = asked.iter().any(|(l, e, _)| (*l, *e) == STRIP);
    let the_log_does_not = !all.iter().any(|(_, e, _)| *e == CHAT_LOG);

    let drawn = drawn_outlines(&s);
    let the_strip_draws_one = drawn.get(&STRIP.1).copied().flatten().is_some();
    // The log draws nothing at all in this reading -- with no words in it there is nothing to
    // draw -- so the other half is read off the resolved data, which does not depend on it
    // drawing.
    let resolved = resolved_outlines(&s);
    let the_log_resolves_to_none = resolved.get(&CHAT_LOG) != Some(&true)
        && drawn
            .iter()
            .filter(|(e, _)| **e == CHAT_LOG)
            .all(|(_, v)| v.is_none());

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "ui.text.outline.the-refusal-strip-is-drawn-with-one-and-the-chat-log-is-not",
        move |_| {
            counted
                && the_strip_asks
                && the_log_does_not
                && the_strip_draws_one
                && the_log_resolves_to_none
        },
    );
}

// ---------------------------------------------------------------------------------------------
// ui.text.outline.every-element-that-asks-for-one-is-drawn-with-one-and-no-other-is
// ---------------------------------------------------------------------------------------------

/// The set, both ways, against an oracle written from the shipped data alone.
pub(super) fn every_element_that_asks_for_an_outline_is_drawn_with_one() {
    let s = the_data_files();
    let resolved = resolved_outlines(&s);
    let want: std::collections::BTreeSet<u32> = resolved
        .iter()
        .filter(|(_, v)| **v)
        .map(|(k, _)| *k)
        .collect();
    let refused: std::collections::BTreeSet<u32> = resolved
        .iter()
        .filter(|(_, v)| !**v)
        .map(|(k, _)| *k)
        .collect();
    let both_arms = !want.is_empty() && !refused.is_empty();

    let drawn = drawn_outlines(&s);
    let with: std::collections::BTreeSet<u32> = drawn
        .iter()
        .filter(|(_, v)| v.is_some())
        .map(|(k, _)| *k)
        .collect();
    let observable: std::collections::BTreeSet<u32> = want
        .iter()
        .filter(|k| drawn.contains_key(k))
        .copied()
        .collect();

    let none_unasked = with.difference(&want).count() == 0;
    let none_silent = observable.difference(&with).count() == 0;
    // Equality, not containment: a claim about the whole set rather than a sample of it.
    let the_same_set = with == observable;
    // ...and the ones the data says must **not** be outlined are not, which is what stops a
    // reader that outlines every piece of text from passing.
    let none_refused_drew_one = refused.iter().all(|r| !with.contains(r));
    let the_strip_is_among_them = with.contains(&STRIP.1);
    // The measured shape of the shipped set, so none of it can drift in silence.
    let the_shape =
        want.len() == 106 && refused.len() == 14 && observable.len() == 30 && with.len() == 30;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "ui.text.outline.every-element-that-asks-for-one-is-drawn-with-one-and-no-other-is",
        move |_| {
            both_arms
                && none_unasked
                && none_silent
                && the_same_set
                && none_refused_drew_one
                && the_strip_is_among_them
                && the_shape
        },
    );
}

// ---------------------------------------------------------------------------------------------
// ui.text.outline.it-comes-from-the-fonts-own-heavier-sheet-and-fits-inside-it
// ---------------------------------------------------------------------------------------------

/// Where the dark backing comes from, and how much of it there is.
pub(super) fn the_outline_comes_from_the_fonts_own_heavier_sheet() {
    use dereth_primitives::DataId;

    let s = the_data_files();
    let textures = dereth_scene::textures::TextureStore::new(&s);
    let fonts = s.ids_of(dereth_dat::DbType::Font);
    let the_shipped_font_set = fonts.len() == 49;

    let (mut with_bg, mut without_bg, mut checked) = (0_usize, 0_usize, 0_u64);
    let mut all_fit = true;
    for f in &fonts {
        let font = dereth_client_shell::ui_draw::load_font(&s, *f).expect("a shipped font loads");
        let atlas = dereth_client_shell::ui_draw::build_font_atlas(&s, *f).expect("and rasterises");
        // The two spreads survive into what the client draws from.
        all_fit &= atlas.num_horizontal_border_pixels == font.num_horizontal_border_pixels
            && atlas.num_vertical_border_pixels == font.num_vertical_border_pixels;
        if font.background_surface_data_id == 0 {
            without_bg += 1;
            // A font with no heavier sheet declares no spread either, which is consistent: the
            // spread exists to widen a sheet that is not there.
            all_fit &= !atlas.has_outline_sheet()
                && (
                    font.num_horizontal_border_pixels,
                    font.num_vertical_border_pixels,
                ) == (0, 0);
            continue;
        }
        with_bg += 1;
        all_fit &= atlas.has_outline_sheet();
        let fg = textures
            .bgra8(DataId(font.foreground_surface_data_id))
            .expect("the letters' sheet");
        // The heavier sheet shares the letters' own size, because both are addressed by the same
        // place in it.
        all_fit &= atlas.texture_size == (fg.width, fg.height);
        let hb = i64::from(font.num_horizontal_border_pixels);
        let vb = i64::from(font.num_vertical_border_pixels);
        for d in &font.char_descs {
            let (l, t) = (i64::from(d.offset_x) - hb, i64::from(d.offset_y) - vb);
            let r = i64::from(d.offset_x) + i64::from(d.width) + hb;
            let b = i64::from(d.offset_y) + i64::from(d.height) + vb;
            // The client clamps a widened rectangle against the window it draws into and never
            // against the sheet, so a wrong spread would read outside the sheet on some letter of
            // some font. It reads outside on none of them.
            all_fit &= l >= 0 && t >= 0 && r <= i64::from(fg.width) && b <= i64::from(fg.height);
            checked += 1;
        }
    }
    let the_split = (with_bg, without_bg) == (37, 12) && checked > 0;

    // And how much backing there is, on a refusal the strip really draws.
    let font = dereth_client_shell::ui_draw::load_font(&s, STRIP_FONT).expect("the strip's font");
    let fg = textures
        .bgra8(DataId(font.foreground_surface_data_id))
        .expect("the letters");
    let bg = textures
        .bgra8(DataId(font.background_surface_data_id))
        .expect("the backing");
    let hb = i32::try_from(font.num_horizontal_border_pixels).expect("a small number");
    let vb = i32::try_from(font.num_vertical_border_pixels).expect("a small number");
    let the_spread = (hb, vb) == (4, 4);

    // The sheet is taken apart into its three plain pieces, because the type it comes in belongs
    // to a crate this one does not name.
    let measure = |sw: u32, sh: u32, pixels: &[[u8; 4]], bx: i32, by: i32, bb: i32| {
        let mut map = std::collections::BTreeMap::<(i32, i32), u8>::new();
        let (mut sum_g, mut n_g) = (0_u64, 0_u64);
        let mut pen = 0_i32;
        for ch in OWNER_STRING.encode_utf16() {
            let Some(d) = font.get_char_desc(ch) else {
                continue;
            };
            let left = pen + i32::from(d.horizontal_offset_before);
            let top = i32::from(d.vertical_offset_before);
            let (w, h) = (i32::from(d.width), i32::from(d.height));
            for r in -by..(h + bb) {
                for col in -bx..(w + bx) {
                    let (sx, sy) = (i32::from(d.offset_x) + col, i32::from(d.offset_y) + r);
                    if sx < 0
                        || sy < 0
                        || sx >= i32::try_from(sw).expect("a small number")
                        || sy >= i32::try_from(sh).expect("a small number")
                    {
                        continue;
                    }
                    let a = pixels[(sy as usize) * (sw as usize) + (sx as usize)][3];
                    if a != 0 {
                        n_g += 1;
                        sum_g += u64::from(a);
                    }
                    let e = map.entry((left + col, top + r)).or_insert(0);
                    *e = (*e).max(a);
                }
            }
            pen += d.advance();
        }
        let inked: Vec<u8> = map.values().copied().filter(|a| *a != 0).collect();
        let n = inked.len() as u64;
        let opaque = inked.iter().filter(|a| **a == 255).count() as u64;
        let sum: u64 = inked.iter().map(|a| u64::from(*a)).sum();
        #[allow(clippy::cast_precision_loss)]
        let out = (
            n,
            opaque,
            sum as f64 / n.max(1) as f64,
            n_g,
            sum_g as f64 / n_g.max(1) as f64,
        );
        out
    };

    let f = measure(fg.width, fg.height, &fg.pixels, 0, 0, 0);
    let o = measure(bg.width, bg.height, &bg.pixels, hb, vb, hb);
    // The letters themselves. Counted two ways, which agree, because the letters' own rectangles
    // do not overlap.
    let the_letters = (f.0, f.1) == (543, 206) && f.0 == f.3 && (f.2 - 172.6).abs() < 0.1;
    // The backing, counted both ways: once letter by letter, which counts the overlaps the widened
    // rectangles make, and once as distinct places on the screen, which is the number comparable
    // with the letters' own.
    #[allow(clippy::cast_precision_loss)]
    let ratio = o.0 as f64 / f.0 as f64;
    let the_backing = o.3 == 2282
        && (o.4 - 128.4).abs() < 0.1
        && o.0 == 1881
        && (o.2 - 146.8).abs() < 0.1
        && (ratio - 3.46).abs() < 0.01;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "ui.text.outline.it-comes-from-the-fonts-own-heavier-sheet-and-fits-inside-it",
        move |_| {
            the_shipped_font_set && all_fit && the_split && the_spread && the_letters && the_backing
        },
    );
}
