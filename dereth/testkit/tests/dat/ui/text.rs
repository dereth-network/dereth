//! UI fixtures and scenarios for text.

use super::*;
// ---------------------------------------------------------------------------------------------
// ui.text.*
//
// A caption with a trailing space ("Environment Texture " with a gap after it), and the four
// panes that measure themselves without being told to.
// ---------------------------------------------------------------------------------------------

/// A UI over the shipped data files, with the two resolvers that make a shipped caption real text
/// measured in the shipped font. Without them every caption reads empty and a scenario about the
/// width of a line would be measuring its own gap.
fn a_shipped_ui() -> (
    dereth_ui::UiSystem,
    std::sync::Arc<dereth_dat::RetailDatStore>,
) {
    use std::rc::Rc;
    use std::sync::Arc;

    use dereth_primitives::AssetSource as _;

    let dir = dereth_dat::testing::dat_dir();
    let store = Arc::new(
        dereth_dat::RetailDatStore::open_dir(&dir).expect("the retail data files are the oracle"),
    );
    let id = dereth_primitives::DataId(0x3900_0001);
    let master = <dereth_assets::MasterProperty as dereth_assets::Decode>::decode_payload(
        id,
        &store.read(id).expect("the master property table"),
    )
    .expect("it decodes");
    let mut ui = dereth_ui::UiSystem::new((800, 600));
    ui.install_master(&master);
    ui.strings = Some(Rc::new(dereth_client::ui_draw::DatStringResolver::new(
        Arc::clone(&store),
    )));
    ui.fonts = Some(Rc::new(dereth_client::ui_draw::DatFontProvider::new(
        Arc::clone(&store),
    )));
    let mut flow = dereth_ui::UiFlow::new();
    dereth_ui_screens::register_all(&mut ui, &mut flow);
    let s = Rc::new(Arc::clone(&store));
    let resolver = Rc::new(
        dereth_ui::framework::DidMapperResolver::load_via_master(s.as_ref()).expect("the mapper"),
    );
    dereth_ui_screens::env::install(&mut ui, s, resolver);
    (ui, store)
}

fn walk_tree(
    ui: &dereth_ui::UiSystem,
    h: dereth_ui::ElemHandle,
    out: &mut Vec<dereth_ui::ElemHandle>,
) {
    out.push(h);
    for c in ui.children(h) {
        walk_tree(ui, c, out);
    }
}

/// A right-justified caption that wraps sits flush against its box.
pub(super) fn a_wrapped_right_justified_caption_is_flush_with_its_box() {
    use dereth_ui::framework::Screen as _;

    /// The caption inside the shipped option-row template.
    const MENU_LABEL: dereth_ui::ElementId = dereth_ui::ElementId(0x1000_0223);
    /// The caption with the trailing space, found by its own words rather than by a handle.
    const CAPTION: &str = "Environment Texture Detail";

    let (mut ui, _store) = a_shipped_ui();
    let mut screen = dereth_ui_screens::screens::gameplay::GamePlayScreen::default();
    screen
        .create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the gameplay screen builds from the shipped layout");
    ui.requests.clear();

    let label = {
        let root = screen.root().expect("a root");
        let mut all = Vec::new();
        walk_tree(&ui, root, &mut all);
        let candidates: Vec<dereth_ui::ElemHandle> = all
            .into_iter()
            .filter(|h| ui.node(*h).is_some_and(|n| n.element_id() == MENU_LABEL))
            .collect();
        candidates
            .into_iter()
            .find(|h| {
                ui.text_element_mut(*h)
                    .is_some_and(|t| t.glyphs.inq_text(false) == CAPTION)
            })
            .expect("one option row's caption reads its own words")
    };

    let screen_box = ui.screen_box(label);
    let t = ui.text_element_mut(label).expect("a text element");
    // The calibration: this caption is right-justified, which is the only reason the space it
    // breaks at is visible at all. A left-justified one would hide the defect completely.
    let right_justified = t.h_justify == 3 && t.margins == (0, 0, 0, 0);
    let the_whole_caption = t.glyphs.inq_text(false) == CAPTION;

    let lines = dereth_ui::text::glyph::wrap(&t.glyphs.glyphs, screen_box.width(), false);
    let with_space: i32 = t.glyphs.glyphs[0..lines[0].end]
        .iter()
        .map(|g| g.width)
        .sum();
    let wrapped_once = lines.len() == 2;
    // The break character stays on the line it broke; only its width is dropped.
    let dropped_the_space = lines[0].width < with_space;

    let glyphs = t.compose(screen_box);
    let first_line: Vec<&dereth_ui::text::PlacedGlyph> =
        glyphs.iter().filter(|g| g.y == screen_box.y0).collect();
    let drawn: String = first_line
        .iter()
        .filter(|g| g.x <= screen_box.x1)
        .map(|g| char::from_u32(u32::from(g.ch)).unwrap_or('?'))
        .collect();
    let last = first_line
        .iter()
        .filter(|g| g.ch != u16::from(b' '))
        .next_back()
        .expect("the line has ink");
    let advance = t
        .glyphs
        .glyphs
        .iter()
        .find(|g| g.data == last.ch)
        .map_or(0, |g| g.width);
    let flush = last.x + advance == screen_box.x1 + 1;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "ui.text.a-right-justified-caption-that-wraps-sits-flush-against-its-box",
        move |_| {
            right_justified
                && the_whole_caption
                && wrapped_once
                && dropped_the_space
                && drawn == "Environment Texture"
                && flush
        },
    );
}

/// A wrapped line does not carry the width of the space it broke at; the last line does.
pub(super) fn a_wrapped_line_drops_the_width_of_its_break() {
    use dereth_ui::text::glyph::{wrap, Glyph};

    // Ten-pixel words and a four-pixel space, so the arithmetic is readable.
    let g = |ch: char| Glyph {
        data: ch as u16,
        width: if ch == ' ' { 4 } else { 10 },
        height: 14,
        color: 0xFFFF_FFFF,
        font: 0,
        tag: None,
    };
    let text: Vec<Glyph> = "aa bb cc".chars().map(g).collect();
    let lines = wrap(&text, 24, false);
    let three_lines = lines.len() == 3
        && (lines[0].start, lines[0].end) == (0, 3)
        && (lines[1].start, lines[1].end) == (3, 6)
        && (lines[2].start, lines[2].end) == (6, 8);
    // Each wrapped line is its words, not its words plus the space it broke at; the last line has
    // no break to drop and is its words too.
    let widths = lines.iter().all(|l| l.width == 20);

    // A space at the very end of the text is not a break, so it is still measured.
    let trailing = wrap(&"aa ".chars().map(g).collect::<Vec<_>>(), 100, false);
    let last_line_keeps_it = trailing.len() == 1 && trailing[0].width == 24;

    // ...and a space is not measured against the box, so a line whose words exactly fill it keeps
    // the space rather than wrapping it on to the next line on its own.
    let exact = wrap(&"aa bb".chars().map(g).collect::<Vec<_>>(), 20, false);
    let no_lonely_space =
        exact.len() == 2 && (exact[0].start, exact[0].end) == (0, 3) && exact[0].width == 20;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "ui.text.a-wrapped-line-does-not-carry-the-width-of-the-space-it-broke-at",
        move |_| three_lines && widths && last_line_keeps_it && no_lonely_space,
    );
}

/// A scrolling pane measures what it holds from the draw itself.
pub(super) fn a_scrolling_pane_measures_itself_from_the_draw() {
    use std::rc::Rc;
    use std::sync::Arc;

    /// The four panes that must measure themselves without being told to.
    const PANES: [u32; 4] = [0x1000_03C4, 0x1000_013C, 0x1000_011D, 0x1000_0126];

    /// Build every shipped layout's roots and answer the first live element carrying `id` whose
    /// box is real -- so the scenario names a **shipped** element and cannot be satisfied by one
    /// it made up.
    fn a_shipped_pane(
        ui: &mut dereth_ui::UiSystem,
        store: &Arc<dereth_dat::RetailDatStore>,
        id: u32,
    ) -> dereth_ui::ElemHandle {
        use dereth_primitives::AssetSource as _;

        let types = ui.property_types.clone();
        for did in store.ids_of(dereth_dat::DbType::UiLayout) {
            let Ok(bytes) = store.read(did) else { continue };
            let Ok(desc) = dereth_ui::desc::LayoutDesc::read(did, &bytes, &types) else {
                continue;
            };
            let roots: Vec<u32> = desc.elements.values().map(|e| e.element_id.0).collect();
            for eid in roots {
                let s = Rc::new(Arc::clone(store));
                let Ok(h) = dereth_ui::framework::create_and_add_root_element_by_data_id(
                    ui,
                    s.as_ref(),
                    did,
                    dereth_ui::ElementId(eid),
                ) else {
                    continue;
                };
                let mut all = Vec::new();
                walk_tree(ui, h, &mut all);
                for candidate in all {
                    let Some(n) = ui.node(candidate) else {
                        continue;
                    };
                    if n.element_id().0 == id && n.region.box_.is_valid() {
                        return candidate;
                    }
                }
            }
        }
        panic!("{id:#010X} is in none of the shipped roots");
    }

    /// Twenty-four lines of prose -- more than any of these panes is tall.
    fn a_long_description() -> String {
        let mut s = String::new();
        for i in 0..24 {
            s.push_str(&format!(
                "Increases the target's Strength by a considerable amount, line {i} of the wordy \
                 description the shipped tables hand the pane. "
            ));
        }
        s
    }

    let mut overflowed = true;
    let mut measured_after_the_write = true;
    for id in PANES {
        let (mut ui, store) = a_shipped_ui();
        let h = a_shipped_pane(&mut ui, &store, id);
        ui.text_element_mut(h)
            .expect("a text element")
            .set_text(&a_long_description());
        let before = ui.text_element_mut(h).expect("alive").scroll.height;
        let view = ui.screen_box(h).height();
        // The precondition: writing the text alone does not measure it, so what the draw does is
        // separable from what the write does.
        measured_after_the_write &= before <= view;
        let mut back = dereth_ui::RecordingDrawBackend::default();
        ui.draw(&mut back);
        let content = ui.text_element_mut(h).expect("alive").scroll.height;
        overflowed &= content > view;
    }

    // The calibration: a pane whose text fits reports an extent that fits, so the readings above
    // are not of a rule that always says yes.
    let (mut ui, store) = a_shipped_ui();
    let h = a_shipped_pane(&mut ui, &store, PANES[3]);
    ui.text_element_mut(h)
        .expect("a text element")
        .set_text("Strength Self VI");
    let mut back = dereth_ui::RecordingDrawBackend::default();
    ui.draw(&mut back);
    let view = ui.screen_box(h).height();
    let content = ui.text_element_mut(h).expect("alive").scroll.height;
    let fits = content > 0 && content <= view;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "ui.text.a-scrolling-pane-measures-what-it-holds-from-the-draw-itself",
        move |_| measured_after_the_write && overflowed && fits,
    );
}

// ---------------------------------------------------------------------------------------------
// ui.text, the overflow rules
//
// Two captions in retail: the squelch panel's draws "Character" and the wrapped remainder is
// simply not shown, and the rendering-quality row's draws "Environment Texture" with "Detail" not
// shown. Centring the whole two-line block would land neither line where it should.
//
// **Nothing here drops a glyph.** What a player sees is what the element's own box lets through,
// which is why the paragraph scenario below asserts that every line is still there to scroll to.
// ---------------------------------------------------------------------------------------------

/// Which of a composed run's glyphs put ink inside `clip` -- a glyph straddling the edge is partly
/// drawn and counts, which is what the client's own per-pixel clipping does.
fn what_is_visible(
    glyphs: &[dereth_ui::text::PlacedGlyph],
    clip: dereth_ui::Box2D,
    line_height: i32,
) -> String {
    glyphs
        .iter()
        .filter(|g| {
            g.y <= clip.y1
                && g.y + line_height - 1 >= clip.y0
                && g.x <= clip.x1
                && g.x >= clip.x0 - 64
        })
        .map(|g| char::from_u32(u32::from(g.ch)).unwrap_or('?'))
        .collect()
}

/// Deliver whatever the tree raised, until it settles.
fn settle(
    ui: &mut dereth_ui::UiSystem,
    s: &mut dereth_ui_screens::screens::gameplay::GamePlayScreen,
) {
    use dereth_ui::framework::Screen as _;
    for _ in 0..16 {
        let batch = ui.drain_outbox();
        if batch.is_empty() {
            return;
        }
        for d in batch {
            if let dereth_ui::Delivery::Element { msg, .. } = d {
                s.on_element_message(&mut dereth_ui::framework::ScreenCx::new(ui), &msg);
            }
        }
    }
    panic!("the element outbox never settled");
}

/// A real click on an element of a shipped tree that has no client around it.
fn click_in_tree(
    ui: &mut dereth_ui::UiSystem,
    s: &mut dereth_ui_screens::screens::gameplay::GamePlayScreen,
    h: dereth_ui::ElemHandle,
) {
    let (ox, oy) = ui.screen_origin(h);
    let r = ui.node(h).expect("alive").region.box_;
    let (x, y) = (ox + r.width() / 2, oy + r.height() / 2);
    ui.mouse_move(dereth_primitives::LocalTime(1.0), x, y);
    settle(ui, s);
    ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, x, y);
    settle(ui, s);
    ui.mouse_up(dereth_ui::focus::action::PRIMARY_CLICK, x, y, false);
    settle(ui, s);
}

/// The shipped gameplay screen over the shipped data files, with no client around it.
fn a_shipped_gameplay_tree() -> (
    dereth_ui::UiSystem,
    dereth_ui_screens::screens::gameplay::GamePlayScreen,
) {
    use dereth_ui::framework::Screen as _;
    let (mut ui, _store) = a_shipped_ui();
    let mut screen = dereth_ui_screens::screens::gameplay::GamePlayScreen::default();
    screen
        .create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the gameplay screen builds from the shipped layout");
    ui.requests.clear();
    (ui, screen)
}

/// A caption too tall for its box shows its first line at the top of it.
pub(super) fn a_caption_too_tall_for_its_box_shows_its_first_line_at_the_top() {
    use dereth_ui::framework::Screen as _;

    /// The squelch panel's caption, and the social panel it is on.
    const SQUELCH_CAPTION: dereth_ui::ElementId = dereth_ui::ElementId(0x1000_053F);
    const SOCIAL_PANEL: u32 = 0x0C;
    const SOCIAL_PAGE: dereth_ui::ElementId = dereth_ui::ElementId(0x1000_018F);
    /// The caption inside the shipped option-row template.
    const MENU_LABEL: dereth_ui::ElementId = dereth_ui::ElementId(0x1000_0223);

    // ---- the first caption, through the whole draw, after real clicks ----------------------
    let (mut ui, mut screen) = a_shipped_gameplay_tree();
    let social = screen
        .toolbar
        .buttons
        .iter()
        .find(|b| b.panel_id == SOCIAL_PANEL)
        .expect("the toolbar has a social button")
        .handle;
    click_in_tree(&mut ui, &mut screen, social);
    let root = screen.root().expect("the gameplay root");
    let page = ui
        .get_child_recursive(root, SOCIAL_PAGE)
        .expect("the social page");
    let pairs: Vec<(dereth_ui::ElementId, dereth_ui::ElementId)> = ui
        .node(page)
        .and_then(|n| {
            n.behaviour
                .as_ref()?
                .as_any()?
                .downcast_ref::<dereth_ui::widgets::panel::Panel>()
        })
        .expect("the social page is a panel")
        .page_to_tab
        .iter()
        .map(|(p, t)| (*p, *t))
        .collect();
    let tab = pairs
        .into_iter()
        .find_map(|(page_id, tab_id)| {
            let pe = ui.get_child_recursive(page, page_id)?;
            ui.get_child_recursive(pe, SQUELCH_CAPTION)?;
            ui.get_child_recursive(page, tab_id)
        })
        .expect("one page of the social panel carries that caption");
    click_in_tree(&mut ui, &mut screen, tab);

    let label = ui
        .get_child_recursive(root, SQUELCH_CAPTION)
        .expect("the caption");
    let open = ui.is_visible(label);
    let mut rec = dereth_ui::RecordingDrawBackend::default();
    ui.draw(&mut rec);
    let cmd = rec
        .calls
        .iter()
        .find(|c| c.who == label)
        .expect("the caption put a draw of its own on the frame")
        .clone();
    let composed: String = cmd
        .glyphs
        .iter()
        .map(|g| char::from_u32(u32::from(g.ch)).unwrap_or('?'))
        .collect();
    let seen = what_is_visible(&cmd.glyphs, cmd.screen.intersect(&cmd.clip), 18);
    let first_squelch = composed == "Character Name:"
        && seen == "Character "
        // ...and the first line sits at the top of the box, not centred as a two-line block.
        && cmd.glyphs[0].y == cmd.screen.y0;

    // ---- the second caption, found by its own words ----------------------------------------
    let (mut ui, screen) = a_shipped_gameplay_tree();
    let root = screen.root().expect("a root");
    let mut all = Vec::new();
    walk_tree(&ui, root, &mut all);
    let candidates: Vec<dereth_ui::ElemHandle> = all
        .into_iter()
        .filter(|h| ui.node(*h).is_some_and(|n| n.element_id() == MENU_LABEL))
        .collect();
    let label = candidates
        .into_iter()
        .find(|h| {
            ui.text_element_mut(*h)
                .is_some_and(|t| t.glyphs.inq_text(false) == "Environment Texture Detail")
        })
        .expect("one option row's caption reads its own words");
    let screen_box = ui.screen_box(label);
    let t = ui.text_element_mut(label).expect("a text element");
    let glyphs = t.compose(screen_box);
    let composed: String = glyphs
        .iter()
        .map(|g| char::from_u32(u32::from(g.ch)).unwrap_or('?'))
        .collect();
    let second = composed == "Environment Texture Detail"
        && what_is_visible(&glyphs, screen_box, 14) == "Environment Texture"
        && glyphs[0].y == screen_box.y0;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "ui.text.a-caption-too-tall-for-its-box-shows-its-first-line-at-the-top",
        move |_| open && first_squelch && second,
    );
}

/// A caption that fits and asks to be centred is still centred.
pub(super) fn a_caption_that_fits_is_still_centred() {
    use dereth_ui::framework::Screen as _;

    let (mut ui, screen) = a_shipped_gameplay_tree();
    let root = screen.root().expect("a root");
    let mut all = Vec::new();
    walk_tree(&ui, root, &mut all);

    let mut looked = 0usize;
    let mut flattened = 0usize;
    for h in all {
        let Some(_) = ui.node(h) else { continue };
        let screen_box = ui.screen_box(h);
        if !screen_box.is_valid() {
            continue;
        }
        let Some(t) = ui.text_element_mut(h) else {
            continue;
        };
        if t.v_justify != 1 || t.glyphs.is_empty() {
            continue;
        }
        let content = t.content_box(screen_box);
        if !content.is_valid() {
            continue;
        }
        let lines =
            dereth_ui::text::glyph::wrap(&t.glyphs.glyphs, content.width(), t.glyphs.one_line);
        let text_h: i32 = lines.iter().map(|l| l.height).sum();
        // Only captions with real room to spare, so "centred" and "at the top" are two different
        // readings.
        if text_h == 0 || content.height() - text_h < 8 {
            continue;
        }
        looked += 1;
        let glyphs = t.compose(screen_box);
        let Some(first) = glyphs.first() else {
            continue;
        };
        if first.y <= content.y0 {
            flattened += 1;
        }
    }

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "ui.text.a-caption-that-fits-and-asks-to-be-centred-is-still-centred",
        move |_| {
            // The denominator first: "none failed" and "none was looked at" are the same reading
            // without it.
            looked >= 20 && flattened == 0
        },
    );
}

/// A paragraph too tall for its box keeps every line and scrolls to them.
pub(super) fn a_paragraph_too_tall_for_its_box_keeps_every_line() {
    use dereth_ui::framework::Screen as _;

    /// The wizard's appearance help -- the shipped paragraph that overflows its pane.
    const HELP: dereth_ui::ElementId = dereth_ui::ElementId(0x1000_03AB);

    let (mut ui, _store) = a_shipped_ui();
    let mut screen = dereth_ui_screens::screens::chargen::CharGenScreen::default();
    screen
        .create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the wizard builds from the shipped layout");
    let root = *screen.roots().first().expect("the wizard has a root");
    let help = ui
        .get_child_recursive(root, HELP)
        .expect("the appearance help");
    let screen_box = ui.screen_box(help);
    let t = ui.text_element_mut(help).expect("a text element");
    let content = t.content_box(screen_box);
    let lines = dereth_ui::text::glyph::wrap(&t.glyphs.glyphs, content.width(), t.glyphs.one_line);
    let text_h: i32 = lines.iter().map(|l| l.height).sum();
    let really_overflows = lines.len() > 25 && text_h > content.height();

    // How many wrapped lines carry a glyph that draws: a blank line occupies no row of its own.
    let n = t.glyphs.glyphs.len();
    let drawable = lines
        .iter()
        .filter(|l| {
            t.glyphs.glyphs[l.start.min(n)..l.end.min(n)]
                .iter()
                .any(|g| !g.is_new_line())
        })
        .count();
    let glyphs = t.compose(screen_box);
    let rows: std::collections::BTreeSet<i32> = glyphs.iter().map(|g| g.y).collect();
    let every_line_is_placed =
        rows.len() == drawable && *rows.iter().next().expect("a first row") == content.y0;

    // Scrolling is what shows the rest, and it can only do that because every line is there.
    let before = glyphs[0].y;
    t.scroll.y = 16;
    let scrolled = t.compose(screen_box);
    let follows_the_scroll = scrolled[0].y == before - 16;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "ui.text.a-paragraph-too-tall-for-its-box-keeps-every-line-and-scrolls",
        move |_| really_overflows && every_line_is_placed && follows_the_scroll,
    );
}
