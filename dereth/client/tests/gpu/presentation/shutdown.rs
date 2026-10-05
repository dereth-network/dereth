//! The disconnected, credits and epilogue screens and the documented shutdown order: a clean
//! logout and a server-side disconnect both land on the right screen, the disconnect reason
//! reaches the player, the epilogue logs off and ends the main loop, and teardown runs in order.
//!
//! Fixture: the retail dats (string tables, layouts, fonts); offline headless Apps on a software
//! device brought up on each screen, with clicks delivered as element-message broadcasts on the
//! application's own tree, not desktop input.
//!
//! | claim | oracle |
//! |---|---|
//! | the disconnect reason reaches the player | two runs of the same screen differing only in the token written to `host_state.error`, with changed pixels bounded by the shipped `Font` metrics |
//! | the character-error → string-token table | the 25-code mapping in `character_error_string_id`, resolved against shipped string table `0x23000002` |
//! | the credits are the credits | table `0x23000008`: count 2,345, the first and last rows, and the missing next row; scrolling duration and seven picture IDs are separate controls |
//! | the epilogue logs off and quits | the OK-button mode edge and the shipped epilogue layout's own message media entry `0x10000002` |
//! | the teardown order | [`dereth_client_runtime::shutdown::Step::ORDER`], the emitted cleanup log, and three explicitly load-bearing orderings |
//!
//! **One exception.** The credits test injects one `InputEvent` for action `0x27` on input map 9 into the real `InputShell`.
//! It then follows `UiShell::mode_on_action` and the production first-refusal path. The injection
//! bypasses only the key-to-action conversion, which the dialog and pregame action tests cover with
//! `WM_KEYDOWN`/`WM_CHAR`/`WM_KEYUP` triples on this screen.

#![cfg(gpu)]

use crate::common::client_dir;
use crate::common::gpu_lock;

use dereth_client::app::App;
use dereth_client_runtime::config::Config;
use dereth_primitives::DataId;
use dereth_ui::framework::{mode, Screen as _};
use dereth_ui_screens::screens::credits::{self, CreditsScreen};
use dereth_ui_screens::screens::disconnected::{self, DisconnectedScreen};
use dereth_ui_screens::screens::epilogue::EpilogueScreen;
use {dereth_client_runtime::shutdown::Outcome, dereth_client_runtime::shutdown::Step};

/// Missing dats fail the test; they never skip it. A test that returns early is counted as a pass
/// and is invisible in the summary line; if the retail dats are not where `$DERETH_TEST_DAT_DIR`
/// says, the run is not a pass.
fn have_dats() {
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats are this file's oracle: none at {} -- set DERETH_TEST_DAT_DIR",
        client_dir().display()
    );
}

/// The retail store, or a failed test.
fn store() -> std::sync::Arc<dereth_dat::RetailDatStore> {
    crate::common::dats()
}

fn base_config() -> Config {
    Config {
        headless: true,
        sound: false,
        world: false,
        character: false,
        dat_dir: client_dir(),
        ..Config::default()
    }
}

/// Bring an offline `App` up on `mode`, broadcasting the intro screen's own skip message.
///
/// A missing device or missing dats fail the test rather than skip it.
fn app_on(want: dereth_ui::UiMode) -> App {
    let mut app = App::new(base_config())
        .unwrap_or_else(|e| panic!("the gpu tier needs a headless App on a software device: {e}"));
    app.start_shell()
        .unwrap_or_else(|e| panic!("the UI shell must come up over the retail dats: {e}"));
    app.load_first_pixel_scene()
        .unwrap_or_else(|e| panic!("the first-pixel scene must load: {e}"));
    step_to(&mut app, want, 20);
    app
}

/// Frame the app until `want` is up, using the intro screen's own skip message when necessary.
fn step_to(app: &mut App, want: dereth_ui::UiMode, limit: u32) {
    for _ in 0..limit {
        if app.ui().and_then(|u| u.flow.current_mode()) == Some(want) {
            return;
        }
        if app.ui().and_then(|u| u.flow.current_mode()) == Some(mode::INTRO) {
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
        app.frame();
    }
    panic!(
        "the flow did not reach {want:?}: {:?}",
        app.ui().and_then(|u| u.flow.current_mode())
    );
}

/// Resolve a character-error code, write its token directly to `host_state.error`, and settle on the
/// disconnected screen. This helper does not drive a server event or character-error notice.
fn app_disconnected_with(code: u32) -> App {
    let mut app = app_on(mode::CHARACTER_MANAGEMENT);
    let token = disconnected::character_error_string_id(code).expect("a code with a token");
    app.probe_mut().host_state_mut().error = Some(token.to_string());
    // One frame queues the mode, one performs the switch, and one applies the error text and draws.
    for _ in 0..3 {
        app.frame();
    }
    assert_eq!(
        app.ui().and_then(|u| u.flow.current_mode()),
        Some(mode::DISCONNECTED)
    );
    app
}

fn screen<T: 'static>(app: &mut App) -> &mut T {
    let shell = app.ui_mut().expect("the shell is up");
    let s = shell.flow.current_mut().expect("a screen");
    let any: &mut dyn std::any::Any = &mut **s;
    any.downcast_mut::<T>().expect("the expected screen")
}

// ---------------------------------------------------------------------------------------------
// 1. The disconnect reason reaches the player
// ---------------------------------------------------------------------------------------------

/// Oracle: the 25-code character-error mapping against shipped string table `0x23000002`.
///
/// Four codes (0 `UNDEF`, 2 `LOGGED_ON`, 7 `NO_PREMADE`, 22 `CHARACTER_IS_BOOTED`) "fall into the
/// `default:` arm of the switch, which leaves the `StringInfo` untouched and **never queues a
/// mode**", and codes 4 and 8 share one token. Every other token must be a real row.
#[test]
fn every_documented_character_error_token_is_a_row_of_the_shipped_string_table() {
    let store = store();
    let shell = dereth_client_shell::ui::UiShell::new(&store, (800, 600))
        .unwrap_or_else(|e| panic!("the UI shell must come up over the retail dats: {e}"));
    // Resolve through the same string-table path as the shell. This test deliberately pins the
    // shipped table's concrete DataId `0x23000002`; it does not independently test enum indirection.
    let table = DataId(0x2300_0002);
    let resolve = |name: &str| {
        shell.ui.resolve_string(
            table,
            dereth_primitives::num::hash::str_hash(name.as_bytes()),
        )
    };
    assert_eq!(
        resolve(disconnected::SERVER_DIED_STRING_ID).as_deref(),
        Some("The connection to the server has been lost!"),
        "the server-died notice handler's own string"
    );
    let mut named = 0;
    for code in 0..=24u32 {
        let Some(token) = disconnected::character_error_string_id(code) else {
            continue;
        };
        named += 1;
        let text = resolve(token);
        // **A shipped data bug.** All twenty tokens
        // exist in the original client's string inventory, but English table `0x23000002` has
        // **no row** for `ID_CHAR_ERROR_ENTER_GAME_OLD_CHARACTER` — and the row for
        // `ID_CHAR_ERROR_ENTER_GAME_GENERIC` holds, as its *text*, the literal string
        // `"ID_CHAR_ERROR_ENTER_GAME_OLD_CHARACTER"`. The localiser pasted the key where the value
        // belonged and lost the row. So error 17 shows nothing and error 11 shows a
        // symbol name. Reproduced rather than repaired: it is what the retail client displays.
        if code == 17 {
            assert!(
                text.is_none(),
                "character error 17's row is missing from the shipped table"
            );
            continue;
        }
        assert!(
            text.is_some(),
            "character error {code} names {token}, which is not in the table"
        );
        let text = text.unwrap();
        assert!(!text.is_empty(), "character error {code}: {token} is blank");
        if code == 11 {
            assert_eq!(
                text, "ID_CHAR_ERROR_ENTER_GAME_OLD_CHARACTER",
                "the shipped paste error"
            );
        }
    }
    assert_eq!(named, 21, "21 of the 25 codes carry a token");
    for silent in [0u32, 2, 7, 22] {
        assert_eq!(disconnected::character_error_string_id(silent), None);
    }
}

/// Every pixel a glyph run claims, from the shipped `Font` metrics.
///
/// A wider mask makes an `outside == 0` check easier, so matching this helper is not itself an
/// ink-shape oracle.
///
/// # The outline pass
///
/// An element carrying text attribute `0x21` draws an outline pass over the same glyph pens before
/// the foreground pass, so the mask covers the dilated halo as well as the ordinary glyph cell.
///
/// The character draw has two outline arms, and the mask covers both shipped font
/// populations:
///
/// * **`0x7000`, 37 of the 49 shipped fonts** — one draw from the font's background sheet, with
///   the rectangle dilated by its horizontal and vertical border counts. The destination is
///   `(penX - hb, penY - vb) .. (penX + width + hb, penY + height + hb)`: the bottom edge uses
///   the **horizontal** count. That original asymmetry is unobservable in shipped data because all
///   49 fonts have `hb == vb`.
/// * **`0x9000`, the other 12** — eight draws of the *foreground* sheet over
///   [`dereth_render::font::OUTLINE_NEIGHBOURHOOD`], i.e. a one-pixel dilation on every side. Those
///   twelve name no background sheet and declare **zero** borders, so keying the dilation off the
///   border counts alone would leave this arm at rule 2's rectangle and back where we started.
///
/// A nonzero background-surface DataId selects the first arm, exactly as
/// `FontAtlas::has_outline_sheet` does in the renderer. [`assert_two_outline_arms`] checks both
/// arms on a real font from each population.
fn glyph_pixels(
    store: &dereth_dat::RetailDatStore,
    list: &[dereth_ui::UiDrawCmd],
    fb: (u32, u32),
) -> (Vec<bool>, usize) {
    let mut fonts: std::collections::BTreeMap<DataId, dereth_render::font::Font> =
        std::collections::BTreeMap::new();
    let mut mask = vec![false; (fb.0 * fb.1) as usize];
    let mut inked = 0usize;
    for cmd in list {
        let Some(clip) = dereth_client_shell::ui_draw::visible_box(cmd, fb) else {
            continue;
        };
        for g in &cmd.glyphs {
            let font = fonts.entry(g.font).or_insert_with(|| {
                dereth_client_shell::ui_draw::load_font(store, g.font).expect("a font")
            });
            let Some(d) = font.get_char_desc(g.ch) else {
                continue;
            };
            if d.width == 0 {
                continue;
            }
            inked += 1;
            let pen_x = g.x + i32::from(d.horizontal_offset_before);
            let pen_y = g.y + i32::from(d.vertical_offset_before);
            let cell_h = i32::try_from(font.max_char_height).unwrap_or(0);
            let (mut x0, mut y0) = (pen_x, pen_y);
            let mut x1 = pen_x + i32::from(d.width) - 1;
            let mut y1 = pen_y + cell_h - 1;
            if cmd.text_outline.is_some() {
                if font.background_surface_data_id == 0 {
                    // `0x9000`: eight draws over `OUTLINE_NEIGHBOURHOOD`, i.e. +/-1 px.
                    x0 -= 1;
                    y0 -= 1;
                    x1 += 1;
                    y1 += 1;
                } else {
                    // `0x7000`: the original outline-sheet rectangle, unioned with rule 2's
                    // (the cell uses max_char_height; the outline uses decoded glyph height + hb).
                    let hb = i32::try_from(font.num_horizontal_border_pixels).unwrap_or(0);
                    let vb = i32::try_from(font.num_vertical_border_pixels).unwrap_or(0);
                    x0 -= hb;
                    y0 -= vb;
                    x1 += hb;
                    y1 = y1.max(pen_y + i32::from(d.height) + hb - 1);
                }
            }
            for y in y0.max(clip.1)..=y1.min(clip.3) {
                for x in x0.max(clip.0)..=x1.min(clip.2) {
                    if x >= 0 && y >= 0 && x < fb.0 as i32 && y < fb.1 as i32 {
                        mask[(y as u32 * fb.0 + x as u32) as usize] = true;
                    }
                }
            }
        }
    }
    (mask, inked)
}

/// The inclusive bounding box of a mask, or `None` if nothing is claimed.
fn mask_bbox(mask: &[bool], fb: (u32, u32)) -> Option<(i32, i32, i32, i32)> {
    let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
    let stride = usize::try_from(fb.0).expect("a back-buffer width");
    for (i, m) in mask.iter().enumerate() {
        if *m {
            let x = i32::try_from(i % stride).expect("a back-buffer column");
            let y = i32::try_from(i / stride).expect("a back-buffer row");
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
    }
    (x1 >= x0).then_some((x0, y0, x1, y1))
}

/// The mask's two outline arms, each on a shipped font that really takes it.
///
/// The frame this file renders proves the mask on the **`0x7000`** arm end to end — the disconnect
/// message is drawn with attribute `0x21` out of a font that carries a background sheet, and
/// `outside == 0` is the pixel evidence. The **`0x9000`** arm draws in no screen this test can
/// reach, and 37 fonts take one arm while 12 take the other: two populations, and a mask proved on
/// one is not proved on the other. So the second arm is proved on the mask's own geometry, against
/// literals taken from the shipped fonts.
///
/// `cmd` is any real command from the frame, cloned so that every field this helper does not care
/// about is a value the client actually produced rather than one invented here.
fn assert_two_outline_arms(
    store: &dereth_dat::RetailDatStore,
    cmd: &dereth_ui::UiDrawCmd,
    fb: (u32, u32),
) {
    let mut fids = store.ids_of(dereth_dat::DbType::Font);
    fids.sort_unstable();
    assert_eq!(fids.len(), 49, "the shipped font set");
    let (mut sheet, mut neighbourhood) = (Vec::new(), Vec::new());
    for f in &fids {
        let font = dereth_client_shell::ui_draw::load_font(store, *f).expect("a font");
        if font.background_surface_data_id == 0 {
            neighbourhood.push(*f);
        } else {
            sheet.push(*f);
        }
    }
    assert_eq!(
        (sheet.len(), neighbourhood.len()),
        (37, 12),
        "37 shipped fonts name a background sheet and take the original 0x7000 outline arm; 12 \
         name none and take the 0x9000 arm"
    );

    // The bounding box of one glyph's claim, with and without the element's `0x21`.
    let probe = |fid: DataId, outline: bool| -> (i32, i32, i32, i32) {
        let mut c = cmd.clone();
        c.screen = dereth_ui::Box2D {
            x0: 0,
            y0: 0,
            x1: fb.0 as i32 - 1,
            y1: fb.1 as i32 - 1,
        };
        c.clip = c.screen;
        c.fills.clear();
        c.invert.clear();
        c.text_outline = outline.then_some(0xFF00_0000u32);
        c.glyphs = vec![dereth_ui::text::PlacedGlyph {
            x: 300,
            y: 300,
            ch: u16::from(b'A'),
            color: 0xFFFF_FFFF,
            font: fid,
        }];
        let (mask, inked) = glyph_pixels(store, std::slice::from_ref(&c), fb);
        assert_eq!(
            inked, 1,
            "the probe glyph claimed no pixels in font {fid:?}"
        );
        mask_bbox(&mask, fb).expect("a non-empty mask")
    };

    // Lowest DataID in each population, so the choice cannot drift with the font set's order.
    for (arm, fid, borders, growth) in [
        (
            "0x7000 background sheet",
            sheet[0],
            (4u32, 4u32),
            (4, 4, 4, 0),
        ),
        (
            "0x9000 neighbourhood",
            neighbourhood[0],
            (0u32, 0u32),
            (1, 1, 1, 1),
        ),
    ] {
        let font = dereth_client_shell::ui_draw::load_font(store, fid).expect("a font");
        assert_eq!(
            (
                font.num_horizontal_border_pixels,
                font.num_vertical_border_pixels
            ),
            borders,
            "{arm}: font {fid:?}'s declared border-pixel counts"
        );
        let plain = probe(fid, false);
        let outlined = probe(fid, true);
        let measured = (
            plain.0 - outlined.0,
            plain.1 - outlined.1,
            outlined.2 - plain.2,
            outlined.3 - plain.3,
        );
        eprintln!(
            "outline {arm}: font {fid:?} borders {borders:?}, mask {plain:?} -> {outlined:?}, \
             grew (l,t,r,b) by {measured:?}"
        );
        assert_eq!(
            measured, growth,
            "{arm}: font {fid:?}'s outlined mask grew by {measured:?} on (left, top, right, \
             bottom) where {growth:?} is what the original {arm} outline arm dilates by"
        );
    }
}

/// Oracle: two runs of the same screen differing only in which resolved error token is written to
/// `host_state.error`. This is a direct host-state setup, not a server-delivered error.
///
/// The disconnected screen writes the message into text element `0x10000417`, which the shipped
/// layout puts at (15, 15) inside the 290x205 panel at (430, 115): (445, 130), 260 x 126. The two
/// draw lists must have equal lengths and equal screen, clip, and image fields, with exactly one
/// differing glyph run. The union of all glyph rectangles from both draw lists then bounds every
/// changed pixel; this does not prove the exact ink shape of the error text.
#[test]
fn the_disconnect_reason_rasterises_only_inside_the_glyph_rectangles_the_font_metrics_predict() {
    let _gpu = gpu_lock();
    let store = store();

    type Shot = (u32, u32, Vec<u8>, Vec<dereth_ui::UiDrawCmd>, String);
    let run = |code: u32| -> Option<Shot> {
        let mut app = app_disconnected_with(code);
        let shown = screen::<DisconnectedScreen>(&mut app)
            .shown_text
            .clone()
            .unwrap_or_default();
        let list = app.ui_draw_list().to_vec();
        let (w, h, bgra) = app.renderer_mut().capture_bgra().ok()?;
        Some((w, h, bgra, list, shown))
    };

    // Two mapped reasons with different text: reason 1 ("Cannot have two accounts logged on
    // at the same time.") and reason 4 ("The server has disconnected…").
    let (w, h, a_px, a_list, a_text) =
        run(1).expect("a rendered frame: retail dats and a software GPU device");
    let (w2, h2, b_px, b_list, b_text) =
        run(4).expect("a rendered frame: retail dats and a software GPU device");
    assert_eq!((w, h), (w2, h2));
    assert_ne!(
        a_text, b_text,
        "the two codes must resolve to different text"
    );
    assert!(a_text.starts_with("Cannot have two accounts"), "{a_text:?}");
    assert!(
        b_text.starts_with("The server has disconnected"),
        "{b_text:?}"
    );

    // The two runs have the same draw-command count and the same screen, clip, and image fields.
    // Other command fields are not compared here.
    assert_eq!(
        a_list.len(),
        b_list.len(),
        "the two runs drew different element counts"
    );
    for (x, y) in a_list.iter().zip(&b_list) {
        assert_eq!((x.screen, x.clip, x.image), (y.screen, y.clip, y.image));
    }

    // Exactly one glyph run differs, at the error text element's shipped screen position.
    let differing: Vec<&dereth_ui::UiDrawCmd> = a_list
        .iter()
        .zip(&b_list)
        .filter(|(x, y)| x.glyphs != y.glyphs)
        .map(|(x, _)| x)
        .collect();
    assert_eq!(differing.len(), 1, "more than the error text changed");
    assert_eq!(
        differing[0].screen.x0, 445,
        "the error-text element is at (430 + 15, 115 + 15)"
    );
    assert_eq!(differing[0].screen.y0, 130);

    // The mask unions predicted rectangles for every glyph command in both lists. The messages
    // differ in length and wrapping, so a pixel claimed by either run is accounted for. Combined
    // with the one-differing-glyph-run check, this bounds the observed differential without
    // claiming an independently exact raster shape.
    let (mask_a, inked_a) = glyph_pixels(&store, &a_list, (w, h));
    let (mask_b, inked_b) = glyph_pixels(&store, &b_list, (w, h));
    let mask: Vec<bool> = mask_a.iter().zip(&mask_b).map(|(x, y)| *x || *y).collect();
    let inked = inked_a.max(inked_b);
    assert!(inked > 30, "only {inked} glyphs claimed any pixels");
    let mut inside = 0usize;
    let mut outside = 0usize;
    for (i, claimed) in mask.iter().enumerate() {
        if a_px[i * 4..i * 4 + 4] != b_px[i * 4..i * 4 + 4] {
            if *claimed {
                inside += 1;
            } else {
                outside += 1;
            }
        }
    }
    assert_eq!(
        outside, 0,
        "{outside} pixels changed outside every glyph rectangle"
    );
    assert!(
        inside > 200,
        "only {inside} pixels changed inside a glyph rectangle"
    );

    // Both outline arms, on a real font from each population.
    assert_two_outline_arms(&store, differing[0], (w, h));
}

// ---------------------------------------------------------------------------------------------
// 2. Disconnected --> Epilogue --> device completion
// ---------------------------------------------------------------------------------------------

/// Behaviour: login.disconnect.ok-reaches-the-epilogue-and-ends-the-loop
///
/// The shipped OK button is element `0x10000418`. Broadcasting its button-click message queues
/// epilogue mode `0x10000009`; the auto-repeat hot-click message is a distinct value and is not the
/// producer used here.
///
/// **Timing.** Completion is not deferred to the frame after the one that enters the epilogue:
/// the layout's own message media entry `0x10000002` is handled inside the switch. The ordering
/// is:
///
/// ```text
/// register root 0x10000399 for message 0x10000002
/// create root 0x10000399 from layout enum 0x10000037
///   initialize its media list
///     reset the media index and immediately update it
///       broadcast the layout's 0x10000002 data message
///         handle completion and request device shutdown
/// ```
///
/// Thus the shutdown request is emitted **inside epilogue creation**, inside the mode switch, and
/// inside the frame that delivered the OK-button message. A `UiRequest::DeviceDone` queued in
/// `dereth_ui_screens::requests` is this crate's own seam (a `Screen` is handed only a
/// `&mut UiSystem` and cannot call the host directly), so the claim asserted is the edge itself:
/// **false before that frame, true after it**. That edge depends on the media message firing during
/// root initialization, registration preceding root creation, and the epilogue retaining its
/// `0x10000002` listener.
#[test]
fn the_ok_button_reaches_the_epilogue_which_logs_off_and_ends_the_main_loop() {
    let _gpu = gpu_lock();
    have_dats();
    let mut app = app_disconnected_with(1);

    let ok = {
        let shell = app.ui_mut().expect("shell");
        shell
            .ui
            .get_element(disconnected::OK_BUTTON)
            .expect("the OK button is in the layout")
    };
    app.ui_mut().expect("shell").ui.broadcast_element_message(
        ok,
        dereth_ui::msg::element::id::BUTTON_CLICKED,
        0,
        0,
    );
    // The epilogue does not exist yet, so no device-shutdown request has been emitted. This makes
    // the assertion after the frame an edge rather than a reading of an already-set flag.
    assert!(
        !app.ui().expect("shell").stats.device_done,
        "not yet — the epilogue has not been constructed"
    );

    // One frame delivers the message and queues the mode; the switch runs at the end of it.
    app.frame();
    assert_eq!(
        app.ui().and_then(|u| u.flow.current_mode()),
        Some(mode::EPILOGUE)
    );

    // Epilogue creation requests a clean end to the character session.
    assert!(screen::<EpilogueScreen>(&mut app).log_off_requested);

    // The shipped epilogue layout's own message media entry `0x10000002` has already set the device-
    // completion flag, with no key press and **in this same frame**. Its media list broadcasts
    // during root initialization, inside the mode switch. This test does not inspect `App::frame`'s
    // return value or exercise a live-link shutdown.
    // That is the whole of the screen: layout `0x21000036` is one element and one media entry.
    assert!(
        app.ui().expect("shell").stats.device_done,
        "the epilogue's own media message must set the device-completion flag in the frame that built it"
    );

    // And it stays done: the completion handler is idempotent, which lets the two registrations
    // (`0x10000399` by id and the root by handle) both deliver harmlessly.
    app.frame();
    assert!(
        app.ui().expect("shell").stats.device_done,
        "and it does not un-end the loop"
    );
}

// ---------------------------------------------------------------------------------------------
// 3. The credits
// ---------------------------------------------------------------------------------------------

/// Oracle: `client_local_English.dat` string table `0x23000008`, walked by
/// the string hash of `"ID_Credits" + n` from one, matching the original sequential-key format.
///
/// This test checks the resulting count, first row, last row, and missing next row. It does not
/// independently compare every one of the 2,345 intermediate keys.
#[test]
fn the_credits_are_the_shipped_credit_roll_and_it_scrolls_to_character_management() {
    let _gpu = gpu_lock();
    have_dats();
    let mut app = app_on(mode::CHARACTER_MANAGEMENT);

    // The shipped Credits button, element `0x100003A3`.
    let credits_button = {
        let shell = app.ui_mut().expect("shell");
        shell
            .ui
            .get_element(dereth_ui::ElementId(0x1000_03A3))
            .expect("the Credits button")
    };
    app.ui_mut().expect("shell").ui.broadcast_element_message(
        credits_button,
        dereth_ui::msg::element::id::BUTTON_CLICKED,
        0,
        0,
    );
    app.frame();
    assert_eq!(
        app.ui().and_then(|u| u.flow.current_mode()),
        Some(mode::CREDITS)
    );
    // A second frame runs screen initialization, which needs a clock the constructor does not have.
    app.frame();

    let (lines, duration, pictures, roots) = {
        let s = screen::<CreditsScreen>(&mut app);
        (
            s.line_count,
            s.duration,
            s.pictures.clone(),
            s.roots().len(),
        )
    };
    eprintln!(
        "credits: {lines} lines, {duration} s, {} pictures",
        pictures.len()
    );
    assert_eq!(roots, 2, "the picture and text roots");
    assert_eq!(lines, 2345, "the shipped English credit roll");
    assert_eq!(
        pictures,
        (0x0600_5F14..=0x0600_5F1A)
            .map(DataId)
            .collect::<Vec<DataId>>(),
        "the seven pictures the layout's attribute 0x10000005 names"
    );
    // Speed 20.0 on a 600-high field with 2,345 lines: minutes, not seconds. The exact number is
    // whatever the composed text height makes it; what is asserted is that it is a real roll.
    assert!(duration > 60.0, "the roll is {duration} s");

    // The first and last line, straight out of the table.
    let table = DataId(0x2300_0008);
    let shell = app.ui().expect("shell");
    assert_eq!(
        shell
            .ui
            .resolve_string(table, credits::credit_line_string_id(1))
            .as_deref(),
        Some("ASHERON'S CALL: LIVE TEAM 2012\n")
    );
    assert_eq!(
        shell
            .ui
            .resolve_string(table, credits::credit_line_string_id(2345))
            .as_deref(),
        Some("(Ibn out.)\n")
    );
    assert!(
        shell
            .ui
            .resolve_string(table, credits::credit_line_string_id(2346))
            .is_none(),
        "the walk stops where the table does"
    );

    // The credits have no backdrop element: the shipped layout contains two text
    // fields, two picture fields, and the picture template. The original transition hides the
    // world scene, leaving the frame-start clear behind the credits UI.
    //
    // Compare the actual frame reached through the shipped Credits button handler with the *same
    // already composed UI list* drawn over only the frame-start clear. The full-screen first-pixel
    // scene makes a leaked world unambiguous without guessing which transparent pixel in the authored
    // artwork ought to be black. This is a software-device pixel oracle, not a model assertion.
    let actual = app
        .renderer_mut()
        .capture_bgra()
        .expect("the actual Credits frame");
    let credits_ui = app.ui_draw_list().to_vec();
    app.renderer_mut()
        .start_frame()
        .expect("the frame-start black clear");
    app.renderer_mut()
        .draw_ui(&credits_ui)
        .expect("the same Credits UI");
    app.renderer_mut()
        .end_frame()
        .expect("the UI-only Credits control");
    let ui_over_black = app
        .renderer_mut()
        .capture_bgra()
        .expect("the UI-only Credits frame");
    assert_eq!(
        actual, ui_over_black,
        "Credits is the two shipped roots over black, never over the retained world-controller scene"
    );

    // This test injects action `0x27` and checks that it takes the credits action path to the
    // please-wait transition and character-management mode `0x1000000A`. It does not wait for the
    // credits roll to end naturally.
    //
    // The action is driven through the client's own route rather than by calling `on_action`
    // directly: input map 9 is registered and its callback is the first-refusal action step,
    // `UiShell::mode_on_action`. A direct call could not see that nothing in the client reached
    // the handler or that the mode never changed.
    //
    // What runs is one action **injected into the real `InputShell`**, so it travels
    // `InputManager::take_events` -> `UiShell::route_input` -> `mode_on_action` -> this screen —
    // the production path, with only the key-to-action walk stubbed here. The dat tier's dialog
    // and pregame action tests drive real `WM_KEYDOWN`/`WM_CHAR`/`WM_KEYUP` triples through it and
    // assert that `DIK_ESCAPE` resolves to `(map 9, 0x27)` on this very screen. The injection is
    // declared in this file's header, and it is the only one in the file.
    //
    // `mode_on_action` queues the mode from inside `route_input`, and the mode switch runs at the end
    // of the *same* `App::frame` — so the credits screen has already been destroyed by the time this
    // test can look at it, and `s.please_wait` is no longer reachable from here at all. The
    // injected action's result is asserted by the mode switch, **counted**: two deliveries of the
    // same `0x27` would be two switches, and `mode_switches` is the one observable that can see a
    // double where `current_mode` cannot.
    //
    // The resulting wait dialog's element, `Wait` kind,
    // the `ID_Wait_PleaseWait` prompt out of the dat, and the teardown that closes the factory's
    // context — is pinned in the ui-screens crate's dat tier, which owns the screen and where the
    // screen outlives the call; this file keeps the half it alone can assert: an injected input action reaches the credits
    // action handler through the running application exactly once.
    let switches_before = app.ui().expect("shell").stats.mode_switches;
    let credits_actions_before = app.ui().expect("shell").stats.credits_actions;
    app.input_manager_mut()
        .expect("an input manager")
        .inject_action(dereth_input::InputEvent {
            action: dereth_input::ActionId(0x27),
            input_map: dereth_input::MAP_DIALOG_BOXES,
            toggle: dereth_input::ToggleType::OneShot,
            extent: 1.0,
            start: true,
            repeat_delta: 1,
            repeat_total: 0,
            from_key_down: true,
        });
    app.frame();
    assert_eq!(
        app.ui().expect("shell").stats.credits_actions,
        credits_actions_before + 1,
        "the action reached the credits screen's action handler exactly once"
    );
    assert_eq!(
        app.ui().expect("shell").stats.mode_switches,
        switches_before + 1,
        "and queued exactly one mode -- not double-handled"
    );
    assert_eq!(
        app.ui().expect("shell").flow.current_mode(),
        Some(mode::CHARACTER_MANAGEMENT),
        "the UI framework queued mode 0x1000000A"
    );
}

// ---------------------------------------------------------------------------------------------
// 4. The shutdown order
// ---------------------------------------------------------------------------------------------

/// Behaviour: presentation.shutdown.the-teardown-runs-the-documented-order
///
/// Oracle: [`Step::ORDER`] names 13 recorded units: two gameplay-client steps, nine enum entries
/// covering the original eleven shared cleanup roles, client release, and the final explicit
/// device-idle wait. This test compares a real teardown log to that sequence and
/// separately asserts three load-bearing relationships: network before UI, UI before database, and
/// database before the idle wait.
///
/// The outcome checks below are deliberately limited to this offline setup: UI, database, and idle
/// work run, while no live link exists for disconnect or network cleanup.
#[test]
fn the_teardown_runs_the_documented_order() {
    let _gpu = gpu_lock();
    have_dats();
    let app = app_on(mode::CHARACTER_MANAGEMENT);
    let log = app.shutdown();
    for (s, o) in &log.0 {
        eprintln!("cleanup {s:?}: {o:?}");
    }
    assert_eq!(
        log.order(),
        Step::ORDER.to_vec(),
        "the teardown ran the documented order"
    );

    let at = |s: Step| {
        log.order()
            .iter()
            .position(|x| *x == s)
            .expect("in the log")
    };
    assert!(
        at(Step::CleanupNet) < at(Step::CleanupUi),
        "net before UI: the world controller holds network references"
    );
    assert!(
        at(Step::CleanupUi) < at(Step::CleanupDatabase),
        "UI elements hold dat objects"
    );
    assert!(at(Step::CleanupDatabase) < at(Step::WaitIdle));

    // What this build actually has a counterpart for. A step named `NotInThisBuild` is a gap that
    // is *reported*; a step that ran must have had something to do.
    assert_eq!(
        log.outcome(Step::CleanupUi),
        Some(Outcome::Ran),
        "the UI was up, so it came down"
    );
    assert_eq!(log.outcome(Step::CleanupDatabase), Some(Outcome::Ran));
    assert_eq!(log.outcome(Step::WaitIdle), Some(Outcome::Ran));
    // Offline: there is no live link, so these two network steps are `Nothing` rather than `Ran`.
    assert_eq!(log.outcome(Step::Disconnect), Some(Outcome::Nothing));
    assert_eq!(log.outcome(Step::CleanupNet), Some(Outcome::Nothing));
}

/// **A setting the player changed survives the quit.**
///
/// Normal-exit preference cleanup ends by saving the registered preferences.
///
/// The observable is the **file on disk after a real `App::shutdown`**, not a call to `save_into`:
/// a test that calls the writer itself cannot see a missing caller.
///
/// No datagram leaves this process. The profile is written into a temporary directory of this
/// test's own making and removed again; the installation's `UserPreferences.ini` is never opened.
#[test]
fn a_changed_preference_is_written_back_on_shutdown() {
    let _gpu = gpu_lock();
    have_dats();

    // A path of this test's own, beside the target directory rather than in the install.
    let dir = std::env::temp_dir().join(format!("dere-g19-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let path = dir.join("UserPreferences.ini");
    let _ = std::fs::remove_file(&path);

    let cfg = Config {
        preferences_file: path.clone(),
        ..base_config()
    };
    // `app_on`'s body with this test's own `Config`: it hard-codes `base_config()`.
    let mut app = match App::new(cfg) {
        Ok(a) => a,
        Err(e) => panic!("the app must come up to have preferences to save: {e}"),
    };
    app.start_shell()
        .expect("the UI shell, which registers UI preferences");
    app.load_first_pixel_scene().expect("the first-pixel scene");
    step_to(&mut app, mode::CHARACTER_MANAGEMENT, 20);

    // `UiShell::new` registered all 34 supported preferences at their defaults.
    assert_eq!(
        dereth_ui_screens::options::store::inq_value("Sound.SoundFeatures"),
        Some(dereth_ui_screens::PrefValue::Int(0)),
        "the registered default is Stereo"
    );
    // Set three representative values directly in the registered store. The option-menu test
    // covers the UI gesture; this test isolates the shutdown flush and does not claim to click
    // those controls.
    dereth_ui_screens::options::store::set_value(
        "Sound.SoundFeatures",
        dereth_ui_screens::PrefValue::Int(1),
    );
    dereth_ui_screens::options::store::set_value(
        "Render.LandscapeDrawDistance",
        dereth_ui_screens::PrefValue::Int(25),
    );
    dereth_ui_screens::options::store::set_value(
        "Misc.TooltipDelay",
        dereth_ui_screens::PrefValue::Float(0.75),
    );

    assert!(!path.exists(), "nothing has written it yet");
    let log = app.shutdown();
    assert_eq!(
        log.outcome(Step::CleanupPreferences),
        Some(Outcome::Ran),
        "`CleanupPreferences` must actually save"
    );

    let text = std::fs::read_to_string(&path).expect("the profile was written");
    eprintln!("saved profile:\n{text}");
    // The persisted shape uses one section per category, removes the category prefix from each
    // key, writes enumeration labels, formats floats to two decimal places, and uses `True`/`False`
    // for booleans. The substring checks below cover those representative encodings.
    assert!(text.contains("[Sound]\r\n"), "a section per category");
    assert!(
        text.contains("SoundFeatures=Mono\r\n"),
        "the label, not the number 1"
    );
    assert!(
        text.contains("LandscapeDrawDistance=Extreme\r\n"),
        "25 is `Extreme`, not index 25"
    );
    assert!(text.contains("TooltipDelay=0.75\r\n"), "%.2f");
    assert!(
        text.contains("SoundDisabled=True\r\n"),
        "True/False, and uninverted"
    );
    assert!(
        !text.contains("[Default]"),
        "the saved profile contains no fallback section"
    );

    // Parse and reload the same text into the existing process. Every registered value is
    // accepted: retail's 34 and this client's own beside them; two of the three values set above
    // are then checked explicitly. This is not a fresh `App` restart and does not independently
    // assert every reloaded value.
    assert_eq!(
        dereth_ui_screens::options::store::init(),
        34,
        "every retail variable back to its default"
    );
    let registered = dereth_ui_screens::options::store::len();
    let ini = dereth_ui::persist::preferences::UserPreferences::parse(&text).expect("it parses");
    let (applied, _ignored) = dereth_ui_screens::options::store::load(&ini);
    assert_eq!(
        applied, registered,
        "every registered option came back: {applied} applied of {registered}"
    );
    assert_eq!(
        dereth_ui_screens::options::store::inq_value("Sound.SoundFeatures"),
        Some(dereth_ui_screens::PrefValue::Int(1)),
        "the setting the player changed survived the quit"
    );
    assert_eq!(
        dereth_ui_screens::options::store::inq_value("Render.LandscapeDrawDistance"),
        Some(dereth_ui_screens::PrefValue::Int(25))
    );

    // The whole directory: `Step::SaveKeyMap` writes `<stem>.keymap` beside the profile, so
    // removing the `.ini` alone leaves the directory behind.
    let _ = std::fs::remove_dir_all(&dir);
}
