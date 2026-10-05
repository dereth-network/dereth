//! A text selection is drawn: the char-gen name box enters with its whole prompt selected and the
//! first key replaces it, and the selection becomes one inverted rectangle per selected glyph, in
//! the draw list and in the pixels, appearing and disappearing with the selection while nothing
//! else on the page draws a highlight. A census counts the shipped edit controls. Fixture: an
//! offline headless `App` driven through character management to the char-gen Summary page (no
//! link, FINISH never pressed), keys typed as `WM_CHAR`, the retail dats, and a device.
//!
//! # How text draws a selection
//!
//! The summary-page update sets the name prompt and selects all of it, with the active-selection
//! bit (0x80) gating its endpoints. Text drawing inverts the pixels of every selected glyph's own
//! cell, one rectangle per glyph, in the foreground glyph pass only:
//!
//! ```text
//! draw foreground glyph -> horizontal advance
//! if selection is active and sel_start <= glyph_index < sel_end:
//!     box = Box2D(pen_x, line_top, advance, line_height)
//!     box = intersection(box, surface_window)
//!     invert_colour_bits(box.x0, box.y0, box.x1 + 1, box.y1 + 1)
//! ```
//!
//! That is text-drawing behaviour, not character-summary-page behaviour: every selectable text
//! element draws its selection through it. The composer asks `TextElement::get_selection` for the
//! range, so a selected box and an unselected one emit different draw commands.
//!
//! # The shipped edit controls
//!
//! Over all 101 `LayoutDesc`s in `client_local_English.dat`, every element and every named
//! state, recursed through children:
//!
//! * 41 declaration sites carry `UICore_Text_editable` (`0x16`) true: the client's edit controls.
//!   That is 39 distinct (layout, element) pairs, 36 distinct element ids, over 19 layouts; all
//!   four are asserted, because "41 edit controls" means four different things.
//! * 2 declare `UICore_Text_select_all_on_gainfocus_mousedown` (`0xD1`), both true:
//!   `0x100001A3` (the toolbar's stack-size box, layout `0x21000016`) and `0x1000046B`
//!   (layout `0x21000033`). Only text mouse-down reads that attribute.
//! * 5 call sites request select-all explicitly outside the text element: the char-gen name box,
//!   the char-gen profession page's attribute box, two in the toolbar, and the chat entry.
//!
//! All 41 draw their selection through the shared renderer path. The client has no login screen
//! (the launcher supplies account, host and credential on the command line). The controls that
//! share this drawing are the chat entry (`0x2100003F`/`0x10000389`), the toolbar's stack-size box
//! (`0x100001A3`), the shared text-input dialog's box in layout `0x2100003C`, the urgent-assistance
//! box (`0x100001BA`), the abuse-report boxes (`0x10000105`/`0x10000107`), and the char-gen
//! profession page's attribute box `0x100002EF`, which is instantiated six times, once per slider.
//!
//! # How the claims are asserted apart
//!
//! 1. [`the_name_box_enters_with_its_whole_prompt_selected_and_the_first_key_replaces_it`] is the
//!    behaviour half and is blind to drawing.
//! 2. [`the_selection_is_carried_into_the_draw_list_as_one_rectangle_per_selected_glyph`] is the
//!    drawing half and is blind to whether typing does anything.
//! 3. [`the_highlight_appears_and_disappears_with_the_selection`] observes two states so the edge
//!    is seen going up and coming down.
//! 4. [`the_highlight_changes_only_the_pixels_under_the_selected_glyphs`] is the picture, on a
//!    real device, with a calibration first and a declared rectangle.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use crate::common::client_dir;
use crate::common::gpu_lock;

use dereth_assets::ui::{ElementDesc, LayoutDesc, PropertyValue};
use dereth_assets::Decode;
use dereth_client::app::App;
use dereth_client_runtime::config::Config;
use dereth_primitives::{AssetSource, DataId, ObjectId};
use dereth_ui::framework::mode;
use dereth_ui::region::Box2D;
use dereth_ui::{ElemHandle, ElementId};
use dereth_ui_screens::screens::chargen::{CharGenScreen, NAME_FIELD, NAME_PROMPT};
use {dereth_desktop::pump::Pump, dereth_input::win32::Win32Message};

/// `0x100003BF` — the Aluvian heritage bullet, so the wizard is on a settled character.
const ALUVIAN: u32 = 0x1000_03BF;
/// `0x100003F4` — the Summary tab.
const SUMMARY_TAB: u32 = 0x1000_03F4;
/// `UICore_Text_editable`.
const TEXT_EDITABLE: u32 = 0x16;
/// `UICore_Text_selectable`.
const TEXT_SELECTABLE: u32 = 0x27;
/// The unnamed `0xD1`: text mouse-down's select-all-on-first-click flag.
const TEXT_SELECT_ALL_ON_FOCUS: u32 = 0xD1;

fn base_config() -> Config {
    Config {
        headless: true,
        sound: false,
        dat_dir: client_dir(),
        ..Config::default()
    }
}

fn require_dats() {
    let d = client_dir();
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats are required at {} -- set DERETH_TEST_DAT_DIR",
        d.display()
    );
}

// ---------------------------------------------------------------------------------------------
// Bring-up: the same route a player takes
// ---------------------------------------------------------------------------------------------

fn character_set() -> dereth_ui::persist::CharacterSet {
    dereth_ui::persist::CharacterSet {
        set: vec![dereth_ui::persist::CharacterIdentity {
            id: ObjectId(0x5000_0001),
            name: "+Alba".into(),
            seconds_grace_period: 0,
        }],
        num_allowed_characters: 11,
        account: "ac01".into(),
        ..dereth_ui::persist::CharacterSet::default()
    }
}

/// Raise a button's own `0x01` on it, for the navigation this file is not testing.
fn click_id(app: &mut App, id: u32) {
    let shell = app.ui_mut().expect("shell");
    let root = shell.flow.current().expect("a screen").roots()[0];
    let h = shell
        .ui
        .get_child_recursive(root, ElementId(id))
        .unwrap_or_else(|| panic!("element {id:#010X} is in the shipped layout"));
    shell
        .ui
        .broadcast_element_message(h, dereth_ui::msg::element::id::BUTTON_CLICKED, 7, 0);
}

fn app_on_wizard() -> App {
    let cfg = Config {
        world: false,
        character: false,
        ..base_config()
    };
    let mut app = App::new(cfg).expect("the application comes up");
    app.start_shell().expect("the UI comes up");
    app.load_first_pixel_scene().expect("the first scene loads");
    let host = app.probe_mut().host_state_mut();
    host.character_set = Some(character_set());
    host.received_set = true;
    host.world_name = Some("ACEmulator".into());
    for _ in 0..12 {
        app.frame();
        match app.ui().and_then(|u| u.flow.current_mode()) {
            Some(m) if m == mode::CHARACTER_MANAGEMENT => break,
            Some(m) if m == mode::INTRO => {
                let root = app
                    .ui()
                    .and_then(|u| u.flow.current())
                    .and_then(|s| s.roots().first().copied());
                if let (Some(root), Some(shell)) = (root, app.ui_mut()) {
                    shell.ui.broadcast_element_message(
                        root,
                        dereth_ui_screens::screens::intro::MSG_SKIP,
                        0,
                        0,
                    );
                }
            }
            _ => {}
        }
    }
    assert_eq!(
        app.ui().and_then(|u| u.flow.current_mode()),
        Some(mode::CHARACTER_MANAGEMENT),
        "the flow reached the character screen"
    );
    click_id(&mut app, 0x1000_03A0); // Create Character
    app.frame();
    app.frame();
    assert_eq!(
        app.ui().and_then(|u| u.flow.current_mode()),
        Some(mode::CHAR_GEN)
    );
    app
}

/// On the Summary page with a settled heritage.
fn app_on_summary() -> App {
    let mut app = app_on_wizard();
    click_id(&mut app, ALUVIAN);
    app.frame();
    click_id(&mut app, SUMMARY_TAB);
    app.frame();
    app.frame();
    app
}

fn find(app: &App, id: ElementId) -> ElemHandle {
    let shell = app.ui().expect("shell");
    let root = shell.flow.current().expect("a screen").roots()[0];
    shell
        .ui
        .get_child_recursive(root, id)
        .unwrap_or_else(|| panic!("{id:?} is in the shipped layout"))
}

fn text_of(app: &mut App, h: ElemHandle) -> String {
    app.ui_mut()
        .expect("shell")
        .ui
        .text_element_mut(h)
        .map_or_else(String::new, |t| t.glyphs.inq_text(false))
}

fn selection_of(app: &mut App, h: ElemHandle) -> Option<(usize, usize)> {
    app.ui_mut()
        .expect("shell")
        .ui
        .text_element_mut(h)
        .and_then(|t| t.get_selection())
}

fn wizard(app: &mut App) -> &mut CharGenScreen {
    let shell = app.ui_mut().expect("shell");
    let s = shell.flow.current_mut().expect("a screen");
    let any: &mut dyn std::any::Any = &mut **s;
    any.downcast_mut::<CharGenScreen>().expect("the wizard")
}

/// The shipped `ID_CharGen_NamePrompt`, read out of the string table rather than written here.
fn prompt(app: &App) -> String {
    app.ui()
        .expect("shell")
        .ui
        .resolve_string(
            dereth_ui_screens::screens::chargen::ERROR_STRING_TABLE,
            dereth_primitives::num::hash::str_hash(NAME_PROMPT.as_bytes()),
        )
        .expect("ID_CharGen_NamePrompt is in the shipped string table")
}

/// Every `UiDrawCmd` the current tree emits, recorded rather than drawn.
fn draw_list(app: &mut App) -> Vec<dereth_ui::UiDrawCmd> {
    let mut back = dereth_ui::RecordingDrawBackend::default();
    app.ui_mut().expect("shell").ui.draw(&mut back);
    back.calls
}

/// The invert rectangles one element's command carries, and how many commands carried any.
fn inverts(app: &mut App, who: ElemHandle) -> Vec<Box2D> {
    draw_list(app)
        .into_iter()
        .filter(|c| c.who == who)
        .flat_map(|c| c.invert)
        .collect()
}

// ---------------------------------------------------------------------------------------------
// The keyboard, driven as Windows drives it; no injected desktop input anywhere in this file
// ---------------------------------------------------------------------------------------------

struct Hand {
    pump: Pump,
    time_ms: u32,
}

impl Hand {
    fn new() -> Self {
        let mut pump = Pump::new();
        pump.state.is_ready = true;
        pump.state.is_active_app = true;
        Self {
            pump,
            time_ms: 100_000,
        }
    }

    /// `WM_CHAR`, which is what `TranslateMessage` produces for a printable key press.
    fn type_text(&mut self, app: &mut App, s: &str) {
        for b in s.bytes() {
            self.time_ms += 10;
            let m = Win32Message::new(
                dereth_input::win32::msg::WM_CHAR,
                b as usize,
                0,
                self.time_ms,
            );
            self.pump.dispatch(m);
            if let Some(input) = app.input_manager_mut() {
                input.on_message(m);
            }
            app.frame();
        }
    }
}

// =================================================================================================
// 1. The behaviour half — blind to drawing on purpose
// =================================================================================================

/// Behaviour: chargen.summary.the-name-box-prompts-takes-the-keyboard-and-the-first-character-replaces-the-prompt
///
/// **The behaviour half.** The box enters with the whole default string selected, so the first
/// key replaces it rather than appending.
///
/// It is deliberately **blind to the drawing**: every assertion below passes on a build whose
/// renderer cannot draw a selection at all, so the drawing half is asserted separately.
#[test]
fn the_name_box_enters_with_its_whole_prompt_selected_and_the_first_key_replaces_it() {
    let _gpu = gpu_lock();
    require_dats();
    let mut app = app_on_summary();
    let name = find(&app, NAME_FIELD);
    let p = prompt(&app);

    assert_eq!(text_of(&mut app, name), p, "the name prompt was installed");
    assert_eq!(
        selection_of(&mut app, name),
        Some((0, p.chars().count())),
        "the whole prompt was selected, and selection lookup answers only while 0x80 is set"
    );
    assert!(
        !wizard(&mut app).name_entered,
        "the name-entered flag is still false"
    );

    let mut hand = Hand::new();
    hand.type_text(&mut app, "Z");
    assert_eq!(
        text_of(&mut app, name),
        "Z",
        "the first key replaced the whole prompt: text insertion deletes the selection"
    );
    assert_eq!(
        selection_of(&mut app, name),
        None,
        "selection deletion clears 0x80 and zeroes both endpoints"
    );
    hand.type_text(&mut app, "z");
    assert_eq!(
        text_of(&mut app, name),
        "Zz",
        "and the second key appends — which is what separates one-time whole-prompt selection from selection every frame"
    );
    app.shutdown();
}

// =================================================================================================
// 2. The drawing half — blind to typing on purpose
// =================================================================================================

/// Behaviour: ui.edit.selected-text-draws-one-highlight-per-selected-glyph
///
/// **The drawing half.**
///
/// Text drawing builds `Box2D(pen_x, line_top, advance, line_height)` for each selected
/// glyph and inverts it. The assertions are the three independent properties of that rectangle —
/// **how many**, **where**, and **how big** — rather than one equality against a recorded blob,
/// because a single golden rectangle cannot say which of the three went wrong.
///
/// The pen and the line top are cross-checked against the element's **own** composed glyphs, which
/// is the independent oracle: the selection and the text must be laid out by one walk
/// (`dereth_ui::text::compose`'s `walk_cells`), or the highlight sits beside the letters instead
/// of on them.
///
/// It is blind to the behaviour half: it never types, and it would pass on a build where the first
/// keypress appended.
#[test]
fn the_selection_is_carried_into_the_draw_list_as_one_rectangle_per_selected_glyph() {
    let _gpu = gpu_lock();
    require_dats();
    let mut app = app_on_summary();
    let name = find(&app, NAME_FIELD);
    let p = prompt(&app);
    let n = p.chars().count();
    assert!(
        n >= 4,
        "the shipped prompt {p:?} is long enough to be a meaningful denominator"
    );

    // The element's own composed glyphs, which are what is on screen.
    let (screen, glyphs) = {
        let shell = app.ui_mut().expect("shell");
        let screen = shell.ui.screen_box(name);
        let g = shell
            .ui
            .text_element_mut(name)
            .expect("a text element")
            .compose(screen);
        (screen, g)
    };
    assert_eq!(glyphs.len(), n, "every character of the prompt is composed");

    let rects = inverts(&mut app, name);
    assert_eq!(
        rects.len(),
        n,
        "one inverted-colour rectangle per selected glyph — `{p}` is {n} glyphs"
    );
    for (i, (r, g)) in rects.iter().zip(&glyphs).enumerate() {
        assert_eq!(
            (r.x0, r.y0),
            (g.x, g.y),
            "rect {i} starts at the pen the glyph was drawn at"
        );
        assert!(
            r.x1 >= r.x0 && r.y1 >= r.y0,
            "rect {i} is a real rectangle: {r:?}"
        );
        assert!(
            r.y1 - r.y0 + 1 >= 8,
            "rect {i} is a whole line cell tall, not a glyph bounding box: {r:?}"
        );
    }
    // The cells tile the run: each starts where the last ended, because the pen advances by
    // exactly the width the rectangle uses.
    for w in rects.windows(2) {
        assert_eq!(
            w[1].x0,
            w[0].x1 + 1,
            "the cells are contiguous: {:?} then {:?}",
            w[0],
            w[1]
        );
        assert_eq!(
            w[1].y0, w[0].y0,
            "and on one line, because the box is `one_line`"
        );
    }
    // And every one of them is inside the element.
    for r in &rects {
        assert!(
            r.x0 >= screen.x0 && r.x1 <= screen.x1 && r.y0 >= screen.y0 && r.y1 <= screen.y1,
            "{r:?} escaped the element {screen:?} — the element's own draw intersects with its surface window"
        );
    }
    app.shutdown();
}

/// **The calibration for every zero above**: an element with no selection emits no rectangles, and
/// the frame as a whole carries exactly the name box's.
///
/// Without this, "the highlight is drawn" and "every text element is drawing a highlight" read
/// alike, and only the first is the claim.
#[test]
fn nothing_else_on_the_page_draws_a_highlight() {
    let _gpu = gpu_lock();
    require_dats();
    let mut app = app_on_summary();
    let name = find(&app, NAME_FIELD);
    let n = prompt(&app).chars().count();

    let cmds = draw_list(&mut app);
    let total: usize = cmds.iter().map(|c| c.invert.len()).sum();
    let mine: usize = cmds
        .iter()
        .filter(|c| c.who == name)
        .map(|c| c.invert.len())
        .sum();
    assert!(
        cmds.len() > 20,
        "the summary page draws a real tree: {} commands",
        cmds.len()
    );
    assert_eq!(mine, n, "the name box carries all {n} of them");
    assert_eq!(total, n, "and nothing else on the page carries any");
    app.shutdown();
}

// =================================================================================================
// 3. The transition — neither half above can see it
// =================================================================================================

/// **Two states, so the edge is seen going up and coming down.**
///
/// Looking only at the opening frame cannot tell a renderer that draws the selection from one that
/// draws a highlight over the name box unconditionally; the second looks right on that frame.
///
/// Station 1 is entry: selected, `n` rectangles. Station 2 is after one keystroke:
/// deselection has cleared the active-selection bit, so `get_selection` answers false and there are
/// **zero**. Both are asserted; the pair is the assertion.
#[test]
fn the_highlight_appears_and_disappears_with_the_selection() {
    let _gpu = gpu_lock();
    require_dats();
    let mut app = app_on_summary();
    let name = find(&app, NAME_FIELD);
    let n = prompt(&app).chars().count();

    assert_eq!(
        inverts(&mut app, name).len(),
        n,
        "station 1: entry, the whole prompt highlighted"
    );
    assert!(
        selection_of(&mut app, name).is_some(),
        "station 1: and the range is live"
    );

    Hand::new().type_text(&mut app, "Z");

    assert_eq!(
        selection_of(&mut app, name),
        None,
        "station 2: the range is gone"
    );
    assert_eq!(
        inverts(&mut app, name).len(),
        0,
        "station 2: and so is the highlight"
    );
    assert_eq!(
        text_of(&mut app, name),
        "Z",
        "station 2: with one glyph still being drawn"
    );
    app.shutdown();
}

/// **The active-selection bit (0x80) is what the drawing hangs on, and only a test that leaves the
/// endpoints behind can see it.**
///
/// Deselection clears the bit *and* zeroes both endpoints; `set_selecting(false)` clears the bit
/// and leaves nothing to find. This drives the third case, a live pair of endpoints with the bit
/// put down by hand, because the client does not treat that state as a live selection.
/// `get_selection` must answer nothing, and therefore so must the draw list.
#[test]
fn endpoints_without_the_selecting_bit_draw_no_highlight() {
    let _gpu = gpu_lock();
    require_dats();
    let mut app = app_on_summary();
    let name = find(&app, NAME_FIELD);
    let n = prompt(&app).chars().count();
    assert_eq!(
        inverts(&mut app, name).len(),
        n,
        "the control case: it draws while 0x80 is up"
    );

    {
        let shell = app.ui_mut().expect("shell");
        let t = shell.ui.text_element_mut(name).expect("a text element");
        t.set_selecting(false);
        // **And then put the endpoints back behind the bit's back**, which is the whole point.
        // `set_selecting(false)` zeroes them, so clearing the bit alone leaves `(0, 0)` and an
        // empty range would answer "nothing to invert" for the wrong reason: without this line, a
        // draw path that read the raw endpoints instead of `get_selection` would still pass here.
        // There is no way
        // to reach this state through the client's own API, which is exactly the invariant
        // `set_selecting` and deselection maintain; it is constructed here because the
        // gate is what stops a stale pair from drawing if anything ever does reach it.
        t.selection = dereth_ui::text::Selection { start: 0, end: n };
        assert_eq!(
            t.get_selection(),
            None,
            "selection lookup refuses without 0x80"
        );
    }
    assert_eq!(
        inverts(&mut app, name).len(),
        0,
        "and the draw path asks the same question, so a live pair with 0x80 down draws nothing"
    );
    app.shutdown();
}

// =================================================================================================
// 4. The picture, on a real device
// =================================================================================================

/// A frame, captured off the back buffer.
fn frame(app: &mut App) -> (u32, u32, Vec<u8>) {
    app.frame();
    app.renderer_mut()
        .capture_bgra()
        .expect("the frame captures")
}

fn holds(b: Box2D, x: i32, y: i32) -> bool {
    x >= b.x0 && x <= b.x1 && y >= b.y0 && y <= b.y1
}

/// Changed pixels between two frames **inside `window`**, split by whether they also fall inside
/// `area`.
///
/// The window exists because the char-gen summary page carries an animating 3D preview: two idle
/// frames legitimately differ by about ten thousand pixels inside that viewport, and a whole-frame
/// zero is therefore not available on this page at all. It is measured and declared rather than
/// assumed — [`the_highlight_changes_only_the_pixels_under_the_selected_glyphs`] takes the
/// calibration inside the same window first, so a window that turned out to be noisy fails loudly
/// instead of laundering the result.
fn differential(
    a: &(u32, u32, Vec<u8>),
    b: &(u32, u32, Vec<u8>),
    window: Box2D,
    area: Box2D,
) -> (u32, u32) {
    assert_eq!((a.0, a.1), (b.0, b.1));
    let (w, h) = (a.0, a.1);
    let (mut changed, mut outside) = (0u32, 0u32);
    for y in 0..h {
        for x in 0..w {
            let (px, py) = (x as i32, y as i32);
            if !holds(window, px, py) {
                continue;
            }
            let i = ((y * w + x) * 4) as usize;
            if a.2[i..i + 3] != b.2[i..i + 3] {
                changed += 1;
                if !holds(area, px, py) {
                    outside += 1;
                }
            }
        }
    }
    (changed, outside)
}

/// **The selection is visible in the pixels.**
///
/// Two frames differing only in the active-selection bit, on the real D3D12 device: every changed
/// pixel falls inside the union of the rectangles text drawing inverts, and none
/// outside it.
///
/// **Three things make the zero mean something:**
///
/// 1. a **calibration** first — two idle frames must agree inside the measured box, or nothing
///    measured there is attributable to anything;
/// 2. the **premise** is asserted, not just the difference — the box really holds the prompt and
///    really is selected, and the changed count must be **large**, because a highlight that draws
///    nowhere would give a perfect zero outside the rectangle;
/// 3. the rectangle is **declared** and comes from the draw list, not from a hand-typed constant.
///
/// The declared name-box window is measured rather than a hand-selected subregion, so "0 outside"
/// covers the whole box. The char-gen summary page has a 3D preview that animates outside that
/// window, and the calibration establishes that the box itself is quiet in these frames; it fires
/// if it is not.
#[test]
fn the_highlight_changes_only_the_pixels_under_the_selected_glyphs() {
    let _gpu = gpu_lock();
    require_dats();
    let mut app = app_on_summary();
    let name = find(&app, NAME_FIELD);
    let p = prompt(&app);

    // The premise: the box holds the prompt and the whole of it is selected.
    assert_eq!(text_of(&mut app, name), p);
    assert_eq!(selection_of(&mut app, name), Some((0, p.chars().count())));

    // The declared rectangle, taken from the draw list the renderer was handed.
    let rects = inverts(&mut app, name);
    assert_eq!(
        rects.len(),
        p.chars().count(),
        "one rectangle per selected glyph"
    );
    let area = rects.iter().fold(rects[0], |a, r| {
        Box2D::new(
            a.x0.min(r.x0),
            a.y0.min(r.y0),
            a.x1.max(r.x1),
            a.y1.max(r.y1),
        )
    });
    // The window is the name box itself, declared: the page's 3D preview animates and no
    // whole-frame zero exists here (measured: two idle frames differ by ~10,400 px across the
    // frame and by **0** inside this box).
    let window = app.ui().expect("shell").ui.screen_box(name);
    assert!(
        holds(window, area.x0, area.y0) && holds(window, area.x1, area.y1),
        "the highlight {area:?} is inside the measured window {window:?}"
    );

    // The calibration: two idle frames agree inside the window, so a later difference there is
    // the change and not the page.
    let a = frame(&mut app);
    let b = frame(&mut app);
    let (idle, _) = differential(&a, &b, window, area);
    assert_eq!(
        idle, 0,
        "two idle frames differ inside {window:?}; nothing here is attributable"
    );

    // The one change: clear the active-selection bit. Nothing else about the tree moves.
    {
        let shell = app.ui_mut().expect("shell");
        shell
            .ui
            .text_element_mut(name)
            .expect("a text element")
            .set_selecting(false);
    }
    let c = frame(&mut app);
    let (changed, outside) = differential(&b, &c, window, area);

    assert_eq!(
        outside, 0,
        "every changed pixel in {window:?} is inside {area:?}"
    );
    let box_area = u32::try_from((area.x1 - area.x0 + 1) * (area.y1 - area.y0 + 1)).unwrap_or(0);
    assert!(
        changed > box_area / 4,
        "the highlight covers the run: {changed} of {box_area} px in {area:?} changed"
    );
    // And the highlight is an *inversion*, not a paint: each channel becomes `~c`, so
    // every changed pixel's before and after must sum to 0xFF in each of B, G and R. This is the
    // one assertion that would still fail if the rectangle were filled with a flat colour, which
    // is the obvious wrong way to draw a selection.
    let (w, mut checked) = (b.0, 0u32);
    for y in area.y0..=area.y1 {
        for x in area.x0..=area.x1 {
            let i = ((y as u32 * w + x as u32) * 4) as usize;
            if b.2[i..i + 3] == c.2[i..i + 3] {
                continue;
            }
            for ch in 0..3 {
                assert_eq!(
                    u32::from(b.2[i + ch]) + u32::from(c.2[i + ch]),
                    0xFF,
                    "({x},{y}) channel {ch}: {} then {} is not a complement",
                    b.2[i + ch],
                    c.2[i + ch]
                );
            }
            checked += 1;
        }
    }
    assert_eq!(
        checked, changed,
        "every changed pixel in the window was inside the highlight"
    );
    eprintln!(
        "edit_field_selection_highlight: highlight differential {changed} px changed, {outside} outside {area:?} \
         (box {box_area} px) within {window:?}; all {checked} are exact complements",
    );
    app.shutdown();
}

// =================================================================================================
// 5. The census of shipped edit controls
// =================================================================================================

fn store() -> dereth_dat::RetailDatStore {
    crate::common::dat_store()
}

fn property_types(s: &dereth_dat::RetailDatStore) -> dereth_assets::ui::PropertyTypes {
    let id = DataId(0x3900_0001);
    let b = s.read(id).expect("MasterProperty 0x39000001");
    dereth_assets::MasterProperty::decode_payload(id, &b)
        .expect("decode")
        .property_types()
}

/// The last value an element's own state or any of its named states gives `want`.
///
/// Every named state is searched, not just the default one, because these are `StateDesc`
/// properties and the client applies whichever state is current — searching one would undercount,
/// and an undercount here reads exactly like the census being right.
fn declared(e: &ElementDesc, want: u32) -> Option<bool> {
    let mut v = None;
    for st in std::iter::once(&e.state).chain(e.states.iter().map(|(_, s)| s)) {
        for (_, p) in &st.properties {
            if p.id == want {
                if let PropertyValue::Bool(b) = p.value {
                    v = Some(b);
                }
            }
        }
    }
    v
}

/// **A count over a stated space.**
///
/// All 101 `LayoutDesc`s in `client_local_English.dat`, every top-level element and every
/// descendant, every named state. The denominator is asserted, and the decode count is asserted
/// against it, so a census that silently read half the layouts cannot pass.
///
/// The two `0xD1` elements are named rather than only counted, because text mouse-down is the
/// attribute's **only** reader and a census that cannot name its members is not checkable.
#[test]
fn the_census_of_shipped_edit_controls() {
    let s = store();
    let types = property_types(&s);
    let ids = s.ids_of(dereth_assets::ui::LayoutDesc::TYPE);
    assert_eq!(ids.len(), 101, "the shipped layout set");

    fn walk(
        e: &ElementDesc,
        lid: DataId,
        out: &mut Vec<(DataId, u32, Option<bool>, Option<bool>)>,
    ) {
        let ed = declared(e, TEXT_EDITABLE);
        let sa = declared(e, TEXT_SELECT_ALL_ON_FOCUS);
        if ed.is_some() || sa.is_some() || declared(e, TEXT_SELECTABLE).is_some() {
            out.push((lid, e.element_id, ed, sa));
        }
        for (_, c) in &e.children {
            walk(c, lid, out);
        }
    }

    let mut rows = Vec::new();
    let mut decoded = 0usize;
    for id in &ids {
        let bytes = s.read(*id).unwrap_or_else(|e| panic!("{id:?}: {e}"));
        let layout = LayoutDesc::decode_payload(*id, &bytes, &types)
            .unwrap_or_else(|e| panic!("{id:?}: {e}"));
        decoded += 1;
        for (_, e) in &layout.elements {
            walk(e, *id, &mut rows);
        }
    }
    assert_eq!(
        decoded,
        ids.len(),
        "every shipped layout was decoded; the census saw all of them"
    );

    let editable: Vec<_> = rows.iter().filter(|r| r.2 == Some(true)).collect();
    let select_all: Vec<_> = rows.iter().filter(|r| r.3.is_some()).collect();

    // **Four numbers, not one**, because "41 edit controls" means four different things and only
    // the first is what the walk above actually counts. Layout `0x21000059` declares
    // `0x100004CF` three times, once per named state block, so the site count exceeds the pair
    // count; and three ids are shared between layouts (`0x10000016` by three, `0x1000013E` by
    // two), so the pair count exceeds the id count.
    assert_eq!(
        editable.len(),
        41,
        "DECLARATION SITES with `UICore_Text_editable` (0x16) true"
    );
    let pairs: std::collections::BTreeSet<(DataId, u32)> =
        editable.iter().map(|r| (r.0, r.1)).collect();
    let elem_ids: std::collections::BTreeSet<u32> = editable.iter().map(|r| r.1).collect();
    let layouts: std::collections::BTreeSet<DataId> = editable.iter().map(|r| r.0).collect();
    assert_eq!(pairs.len(), 39, "distinct (layout, element) pairs");
    assert_eq!(elem_ids.len(), 36, "distinct element ids");
    assert_eq!(layouts.len(), 19, "layouts that carry at least one");
    assert_eq!(
        select_all.len(),
        2,
        "elements declaring the select-all-on-focus attribute 0xD1"
    );
    assert!(
        select_all.iter().all(|r| r.3 == Some(true)),
        "both of them ask for it; neither declares it false"
    );
    let named: Vec<u32> = select_all.iter().map(|r| r.1).collect();
    assert_eq!(
        named,
        vec![0x1000_01A3, 0x1000_046B],
        "the toolbar's stack-size box and layout 0x21000033's box"
    );
    // The char-gen name box is one of the 41 and is **not** one of the two: its select-all comes
    // from the character-summary update, not from the attribute: for this box the mechanism is
    // not a flag on the element.
    assert!(
        editable.iter().any(|r| r.1 == NAME_FIELD.0),
        "the char-gen name box {:#010X} declares itself editable",
        NAME_FIELD.0
    );
    assert!(
        !named.contains(&NAME_FIELD.0),
        "and it does not carry 0xD1: its selection is the per-frame update's, not the mouse-down path's"
    );
    eprintln!(
        "edit_field_selection_highlight census over {} layouts: {} editable declaration sites ({} pairs, {} ids, {} layouts), {} declaring 0xD1 ({:#010X}, {:#010X})",
        ids.len(),
        editable.len(),
        pairs.len(),
        elem_ids.len(),
        layouts.len(),
        select_all.len(),
        named[0],
        named[1]
    );
}
