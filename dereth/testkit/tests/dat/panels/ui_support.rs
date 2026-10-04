use super::*;

// =============================================================================================
// Support
//
// The client, the login and the pointer are `dereth_testkit`'s; what stays here is the walk down
// the shipped character page and the two glyph readers the snapshot has no answer for.
// =============================================================================================

/// The shell's element system and the gameplay screen behind it.
pub(super) fn gameplay_screen(app: &mut App) -> (&mut UiSystem, &mut GamePlayScreen) {
    let shell = app.ui_mut().expect("the shell is up");
    let ui = &mut shell.ui;
    let screen = shell.flow.current_mut().expect("a screen is current");
    let any: &mut dyn std::any::Any = &mut **screen;
    let screen = any
        .downcast_mut::<GamePlayScreen>()
        .expect("the gameplay screen is current");
    (ui, screen)
}

/// One message, delivered the way the session layer delivers it.
pub(super) fn deliver<M: dereth_protocol::Message>(c: &mut HeadlessClient, m: &M) {
    c.when(Inbound::message(m));
    c.tick(1);
}

/// The glyphs `h` composed, as text and as one `(font index, colour)` pair per UTF-16 unit.
///
/// **The gap `UiSnapshot` has.** A snapshot answers what an element says; what colour it says it
/// in is a property of the composed glyph, and font-aware text composition is how a
/// panel writes two colours into one element. Several claims here are exactly about that
/// second colour.
pub(super) fn glyph_runs(app: &mut App, h: ElemHandle) -> (String, Vec<(u32, u32)>) {
    let (ui, _) = gameplay_screen(app);
    ui.text_element_mut(h).map_or_else(
        || (String::new(), Vec::new()),
        |t| {
            (
                t.glyphs.inq_text(false),
                t.glyphs.glyphs.iter().map(|g| (g.font, g.color)).collect(),
            )
        },
    )
}

/// The three colours an element's own state array declares, read back off the live element and
/// checked against the words a reader would use for them -- a symmetric read cannot see a wrong
/// constant.
pub(super) fn state_colours(app: &mut App, h: ElemHandle) -> (u32, u32, u32) {
    let (ui, _) = gameplay_screen(app);
    let white = statmgmt::font_color_at(ui, h, 0).expect("the plain colour");
    let green = statmgmt::font_color_at(ui, h, 1).expect("the raised colour");
    let red = statmgmt::font_color_at(ui, h, 2).expect("the lowered colour");
    assert_eq!(white, 0xFFFF_FFFF, "unmodified: white");
    assert_eq!(green, 0xFF00_FF00, "raised: green");
    assert_eq!(red, 0xFFFF_0000, "lowered: red");
    (white, green, red)
}

/// Every glyph of `h` is one run: font index 0 throughout, and exactly `colour`.
pub(super) fn one_run(g: &[(u32, u32)], text: &str, colour: u32) -> bool {
    g.len() == text.encode_utf16().count()
        && g.iter().all(|(f, _)| *f == 0)
        && g.iter().all(|(_, c)| *c == colour)
}

/// Whether an element is drawn at all.
pub(super) fn element_visible(c: &mut HeadlessClient, h: ElemHandle) -> bool {
    c.app_mut()
        .ui()
        .expect("the shell is up")
        .ui
        .node(h)
        .is_some_and(|n| n.region.flags.visible)
}

/// The centre of an element's box, which is where a hand puts the pointer.
pub(super) fn centre_of(c: &mut HeadlessClient, h: ElemHandle) -> ScreenPoint {
    let b = c.app_mut().ui().expect("the shell is up").ui.screen_box(h);
    ScreenPoint::new((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2)
}
