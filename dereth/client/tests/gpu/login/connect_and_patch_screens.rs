//! The connect and data-patch screens and the text that makes them screens rather than pictures.
//! There is no credential-entry screen: account, host, port and credential come from the command
//! line and user preferences, so connecting is decided by `Config` (connecting is the default, and
//! only credentials make it happen). The data-patch screen's two meters and two status lines show
//! the connection and the five patch events with their string-table text; its text rasterises only
//! inside the glyph rectangles the shipped `Font` metrics predict, covering both outline arms; and
//! walking the opening screens reuses descriptor slots rather than accumulating them.
//! Fixture: the retail dats, a headless `UiShell` driven with `HostState`, and an offline headless
//! `App` on a software device compared pixel by pixel between runs.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use crate::common::client_dir;

use dereth_client::app::App;
use dereth_client_runtime::config::Config;
use dereth_primitives::DataId;
use dereth_ui::framework::mode;
use {dereth_client_contract::pregame::PregameView as HostState, dereth_client_shell::ui::UiShell};

/// The retail store; the test fails when it is absent.
fn store() -> std::sync::Arc<dereth_dat::RetailDatStore> {
    crate::common::dats()
}

fn base_config() -> Config {
    Config {
        headless: true,
        sound: false,
        // The data-patch screen covers the whole back buffer, so the world would only make the
        // comparison slower. `tests/gpu/presentation/shell.rs` is where the UI-over-the-world case lives.
        world: false,
        character: false,
        dat_dir: client_dir(),
        ..Config::default()
    }
}

fn at(frame: u32) -> dereth_primitives::LocalTime {
    dereth_primitives::LocalTime(f64::from(frame) / 30.0)
}

// ---------------------------------------------------------------------------------------------
// 1. Connecting is the normal path
// ---------------------------------------------------------------------------------------------

/// Behaviour: login.connect.credentials-decide-whether-the-client-connects
///
/// Oracle: the retail client takes account and host from `-a` and `-h` and refuses to start
/// without them. It therefore always connects, whereas this build connects
/// whenever it has been given something to connect *with*.
///
/// Connecting is the normal path rather than an opt-in flag, asserted where the decision is made
/// rather than inferred from a log line.
#[test]
fn credentials_are_what_decides_whether_the_client_connects() {
    // A bare run: no account, no host, no socket. This is what keeps the offline slices and the
    // headless capture gate runnable, and it is the *only* reason an offline mode exists at all.
    let bare = Config::default();
    assert!(
        bare.connect,
        "connecting is the default, as the retail client's is"
    );
    assert!(bare.ui, "and so is the UI; there is no --ui opt-in");
    assert!(
        !bare.will_connect(),
        "with nothing to connect with, there is nothing to connect to"
    );

    // Credentials on the command line: exactly the launcher's own invocation.
    let live = Config {
        account: "ac01".into(),
        host: "127.0.0.1:19000".into(),
        vg_password: "pass".into(),
        ..Config::default()
    };
    assert!(live.will_connect());
    assert!(live.require_account_and_host().is_ok());

    // `--no-connect` is the explicit opt-out.
    assert!(!Config {
        connect: false,
        ..live.clone()
    }
    .will_connect());

    // Half the pair is still the documented error, not a silent offline start: the client that is
    // given an account and no host has been told to connect and cannot.
    let half = Config {
        host: String::new(),
        ..live
    };
    assert!(half.will_connect());
    assert!(half.require_account_and_host().is_err());
}

// ---------------------------------------------------------------------------------------------
// 2. The patch screen shows the documented states
// ---------------------------------------------------------------------------------------------

/// Behaviour: login.patch.the-data-patch-screen-shows-the-documented-states
///
/// Oracle: the five retail patch events and the string IDs each shows, resolved through the
/// retail string table named by **table enum `0x10000002`**, whose rows are keyed by the hash of the
/// id. The strings are the dat's, not this test's: `ID_DataPatch_Interrogation` is
/// "Looking for data to patch..." because that is what `client_local_English.dat` says.
///
/// Also asserts the connect line's two code states, which are `0x1000003B` "Connecting..." and
/// `0x1000003C` "Connected!" in the shipped `patch` layout.
#[test]
fn the_data_patch_screen_shows_the_documented_states() {
    use dereth_ui_screens::screens::datapatch::DddEvent;

    let store = store();
    let mut shell = UiShell::new(&store, (800, 600))
        .unwrap_or_else(|e| panic!("the ui shell opens over the retail dats: {e}"));
    let mut pump = dereth_ui::NullInputPump;

    // A live connection that has not yet delivered `0xF658`, which is what holds the screen up:
    // exit requires connection >= 1.0 && patch >= 1.0 && (no transport || character set received).
    let mut host = HostState {
        has_packet_controller: true,
        ..HostState::default()
    };

    // Frame 0 performs the switch into the screen; frame 1 is the first one it is up for.
    shell.frame(at(0), &host, &mut pump);
    assert_eq!(
        shell.flow.current_mode(),
        Some(mode::DATA_PATCH),
        "the first mode of the client"
    );

    let text_of = |shell: &mut UiShell, id: u32| -> String {
        let h = shell
            .ui
            .get_element(dereth_ui::ElementId(id))
            .expect("the element is in the layout");
        shell
            .ui
            .text_element_mut(h)
            .map(|t| t.glyphs.inq_text(false))
            .unwrap_or_default()
    };

    // `DDD_PatchtimeInterrogation`: the server has asked what we already have.
    host.ddd = vec![DddEvent::PatchtimeInterrogation];
    shell.frame(at(1), &host, &mut pump);
    assert_eq!(
        text_of(&mut shell, 0x1000_0421),
        "Looking for data to patch..."
    );
    // The connect meter is still empty, so the connect line reads "Connecting...".
    assert_eq!(text_of(&mut shell, 0x1000_0420), "Connecting...");
    assert_eq!(
        shell.flow.current_mode(),
        Some(mode::DATA_PATCH),
        "and the screen has not moved on"
    );

    // `DDD_PatchtimeBegin` records the expected byte count; `DDD_DataDownloaded` accumulates and
    // fills the meter. `ID_DataPatch_PatchProgress` is stored as the three literal pieces
    // `["", "% of ", "K complete..."]`, so the substitution is what makes it a sentence.
    host.ddd = vec![
        DddEvent::PatchtimeBegin {
            expected: 4 * 1024 * 1024,
        },
        DddEvent::DataDownloaded { bytes: 1024 * 1024 },
    ];
    shell.frame(at(2), &host, &mut pump);
    assert_eq!(text_of(&mut shell, 0x1000_0421), "25% of 4096K complete...");

    // A live connection drives the connect meter to 1.0 at the start of the screen tick, and with it the
    // connect line.
    host.ddd.clear();
    host.connected = true;
    shell.frame(at(3), &host, &mut pump);
    assert_eq!(text_of(&mut shell, 0x1000_0420), "Connected!");
    assert_eq!(
        shell.flow.current_mode(),
        Some(mode::DATA_PATCH),
        "still waiting for 0xF658"
    );

    // Patch completion unregisters the patch plugin and forces patch progress to 1.0.
    host.ddd = vec![DddEvent::PatchtimeEnd];
    shell.frame(at(4), &host, &mut pump);
    assert_eq!(text_of(&mut shell, 0x1000_0421), "Patching Done!");

    // Both meters full, and now the character set arrives: the documented DataPatch -> Intro edge.
    assert_eq!(
        shell.flow.current_mode(),
        Some(mode::DATA_PATCH),
        "0xF658 has still not arrived"
    );
    host.ddd.clear();
    // Receiving the character-set notice marks the list as received; its wire source is
    // `0xF658 Login_LoginCharacterSet`.
    host.character_set = Some(dereth_ui::persist::CharacterSet {
        set: vec![dereth_ui::persist::CharacterIdentity {
            id: dereth_primitives::ObjectId(0x5000_0001),
            name: "Frostfell".into(),
            seconds_grace_period: 0,
        }],
        num_allowed_characters: 5,
        account: "ac01".into(),
        ..dereth_ui::persist::CharacterSet::default()
    });
    shell.frame(at(5), &host, &mut pump);
    assert_eq!(shell.flow.current_mode(), Some(mode::INTRO));

    assert_eq!(shell.stats.screen_create_failures, 0);
    assert_eq!(shell.stats.unregistered_mode_requests, 0);
}

// ---------------------------------------------------------------------------------------------
// 3. Text lands on the right pixels
// ---------------------------------------------------------------------------------------------

/// Every pixel a glyph run claims, from the shipped `Font` metrics.
///
/// The foreground glyph rectangle starts at the pen plus the font's horizontal and vertical
/// offsets. It uses the glyph width and the font's maximum character height: the atlas cell's
/// height, not the glyph's own height.
///
/// # The outline pass
///
/// The text loop starts at pass 0 when outline flag 0x10 is set, otherwise at pass 1,
/// and runs until pass 2. An element carrying outline attribute `0x21` therefore draws an
/// outline pass over the same pens *before* the foreground pass, so the mask must contain the
/// dilated halo as well as the foreground rectangle.
///
/// Glyph drawing has two outline arms and the mask has to cover both, because they are two
/// populations of the shipped font set:
///
/// * **`0x7000`, 37 of the 49 shipped fonts** — one draw out of the font's own *background* sheet
///   through a rectangle **dilated by the horizontal and vertical border-pixel counts**.
///   With border counts `hb`/`vb`, the destination is
///   `(penX - hb, penY - vb) .. (penX + glyph_width + hb, penY + glyph_height + hb)`.
///   The bottom edge uses the **horizontal** count, a retail asymmetry that is
///   unobservable in shipped data because all 49 fonts have
///   `hb == vb`.
/// * **`0x9000`, the other 12** — eight draws of the *foreground* sheet over
///   [`dereth_render::font::OUTLINE_NEIGHBOURHOOD`], i.e. a one-pixel dilation on every side. Those
///   twelve name no background sheet and declare **zero** borders, so keying the dilation off the
///   border counts alone would leave this arm at the foreground rectangle.
///
/// The arm is selected by the font's background-surface ID, exactly as
/// `FontAtlas::has_outline_sheet` selects it in the renderer. Both arms are asserted, on a real
/// font from each population, by [`assert_two_outline_arms`].
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
                    // `0x7000`: union the dilated outline rectangle with the foreground cell
                    // (maximum character height versus glyph height + horizontal border).
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

/// Oracle: the frame itself, so that a pass that draws nothing, draws everywhere, or draws in the
/// wrong place all fail. Two runs of the **same screen** from the same
/// dats: one with the string tables installed and one without, so the only difference between them
/// is the characters. Every changed pixel must lie inside a rectangle the shipped `Font` metrics
/// predict for a glyph, and the glyphs must actually have changed some.
///
/// Suppressing the text by dropping `UiSystem::strings` is exact rather than convenient: a
/// `StringInfo` that names a table and an id resolves to nothing without one, so the layout, the
/// images, the element boxes and the draw order are all identical between the two runs.
#[test]
fn the_text_rasterises_only_inside_the_glyph_rectangles_the_font_metrics_predict() {
    let store = store();

    type Shot = (
        u32,
        u32,
        Vec<u8>,
        Vec<dereth_ui::UiDrawCmd>,
        dereth_client_shell::ui_draw::UiTextureStats,
    );
    let run = |with_text: bool| -> Option<Shot> {
        let mut app = App::new(base_config())
            .unwrap_or_else(|e| panic!("a headless App on the software device: {e}"));
        app.start_shell()
            .unwrap_or_else(|e| panic!("the shell starts over the retail dats: {e}"));
        if !with_text {
            // String-table lookup answers nothing, so every `StringInfo` that names
            // an id resolves to nothing and no glyph is created. Everything else is unchanged.
            app.ui_mut().expect("the shell is up").ui.strings = None;
        }
        app.load_first_pixel_scene().ok()?;
        // One frame switches into the data-patch screen and *then* takes the blit list.
        // Presentation draws that list later in the same frame — so one
        // frame is the data-patch screen, drawn. A second frame would already be the intro, because
        // with no transport the screen's transition condition is satisfied at once.
        app.frame();
        let list = app.ui_draw_list().to_vec();
        let stats = app.renderer_mut().ui_stats;
        let (w, h, bgra) = app.renderer_mut().capture_bgra().ok()?;
        Some((w, h, bgra, list, stats))
    };

    let (w, h, without, plain, _) =
        run(false).expect("a rendered frame: retail dats and a WARP device");
    let (w2, h2, with, list, stats) =
        run(true).expect("a rendered frame: retail dats and a WARP device");
    assert_eq!((w, h), (w2, h2));

    // The two runs really are the same screen: same commands, same boxes, same images.
    assert_eq!(
        plain.len(),
        list.len(),
        "the two runs drew different element counts"
    );
    for (a, b) in plain.iter().zip(&list) {
        assert_eq!((a.screen, a.clip, a.image), (b.screen, b.clip, b.image));
    }
    assert!(
        plain.iter().all(|c| c.glyphs.is_empty()),
        "the no-strings run still produced glyphs"
    );

    // The text pass ran and nothing was silently dropped.
    assert!(
        stats.glyphs_drawn > 100,
        "only {} glyph quads were drawn",
        stats.glyphs_drawn
    );
    assert_eq!(stats.glyphs_skipped, 0, "a glyph had no atlas entry");
    assert_eq!(stats.font_failures, 0, "a font would not bake");
    assert!(stats.fonts_baked > 0, "no font atlas was built");

    let (mask, inked) = glyph_pixels(&store, &list, (w, h));
    assert!(inked > 100, "only {inked} glyphs claimed any pixels");

    let mut inside = 0usize;
    let mut outside = 0usize;
    for (i, claimed) in mask.iter().enumerate() {
        if without[i * 4..i * 4 + 4] != with[i * 4..i * 4 + 4] {
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
        inside > 500,
        "only {inside} pixels changed inside {} claimed; the text drew nothing visible",
        mask.iter().filter(|c| **c).count()
    );

    // ...and the ink is where a *particular* string is, not merely somewhere legal. The Cancel
    // button's caption is element `0x1000041C` of the `patch` layout, whose text is string
    // `134262076` of table `0x23000002` -- "Cancel".
    let cancel = list
        .iter()
        .find(|c| !c.glyphs.is_empty() && c.screen.x0 == 696 && c.screen.y0 == 30)
        .expect("the Cancel button's caption");
    let text: String = cancel
        .glyphs
        .iter()
        .filter_map(|g| char::from_u32(u32::from(g.ch)))
        .collect();
    assert_eq!(text, "Cancel");
    let (cancel_mask, _) = glyph_pixels(&store, std::slice::from_ref(cancel), (w, h));
    let changed = cancel_mask
        .iter()
        .enumerate()
        .filter(|(i, c)| **c && without[i * 4..i * 4 + 4] != with[i * 4..i * 4 + 4])
        .count();
    assert!(
        changed > 30,
        "the Cancel caption changed only {changed} pixels"
    );

    // ---- both outline arms, on a real font from each population ------------------------------
    //
    // The frame above proves the mask on the **`0x7000`** arm end to end: the Cancel caption is
    // an element carrying attribute `0x21`, its font names a background sheet, and the
    // `outside == 0` above is the evidence. The **`0x9000`** arm draws in no screen this
    // test can reach, and 37 fonts take one arm while 12 take the other -- two populations, and a
    // mask proved on one is not proved on the other. So the second arm is proved on the mask's own
    // geometry, against literals taken from the shipped fonts.
    assert_two_outline_arms(&store, cancel, (w, h));
}

/// The mask's two outline arms, each on a shipped font that really takes it.
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
        "37 shipped fonts name a background sheet and take the background-sheet 0x7000 arm; 12 name \
         none and take the 0x9000 arm"
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
            "outline arm {arm}: font {fid:?} borders {borders:?}, mask {plain:?} -> {outlined:?}, \
             grew (l,t,r,b) by {measured:?}"
        );
        assert_eq!(
            measured, growth,
            "{arm}: font {fid:?}'s outlined mask grew by {measured:?} on (left, top, right, \
             bottom) where {growth:?} is what the glyph {arm} arm dilates by"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// 4. The descriptor heap stays bounded
// ---------------------------------------------------------------------------------------------

/// Oracle: the renderer's descriptor allocator and its texture reference/release accounting.
///
/// A screen's images are released when a mode change destroys it, so walking the opening sequence — data patch, intro,
/// character management — must **reuse** slots rather than accumulate them.
#[test]
fn walking_the_opening_screens_reuses_descriptor_slots_rather_than_accumulating_them() {
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats are this test's oracle and there are none at {} -- \
         set DERETH_TEST_DAT_DIR",
        client_dir().display()
    );
    let mut app = App::new(base_config())
        .unwrap_or_else(|e| panic!("a headless App on the software device: {e}"));
    app.start_shell()
        .unwrap_or_else(|e| panic!("the shell starts over the retail dats: {e}"));
    app.load_first_pixel_scene()
        .expect("the first-pixel surface decodes");

    // Three screens, then round and round: the flow ends on character management and stays there
    // with no server, so drive it back through the disconnect edge to make it switch repeatedly.
    //
    // Every frame's live count, mode, movie state and texture counters are kept, so a failure
    // names the frame the count moved on and what moved with it.
    let mut history = Vec::new();
    let step = |app: &mut App, history: &mut Vec<String>| {
        app.frame();
        let r = app.renderer_mut();
        let live = r.descriptor_usage().live;
        let (keys, uploaded, failures) = (
            r.texture_keys().len(),
            r.ui_stats.uploaded,
            r.ui_stats.decode_failures,
        );
        let ui = app.ui().expect("the shell is up");
        history.push(format!(
            "frame {}: live {live}, mode {:?}, movie playing {}, {:?}, keyed {keys}, uploaded \
             {uploaded}, upload failures {failures}",
            history.len(),
            ui.flow.current_mode(),
            ui.movie_playing(),
            ui.movie_stats,
        ));
        live
    };
    for _ in 0..3 {
        step(&mut app, &mut history);
    }
    let after_three = app.renderer().descriptor_usage();
    for _ in 0..30 {
        step(&mut app, &mut history);
    }
    let steady = app.renderer().descriptor_usage();
    assert_eq!(
        steady.live,
        after_three.live,
        "a screen that has not changed must not keep taking slots:\n{}",
        history.join("\n")
    );

    let stats = app.renderer().descriptor_stats();
    assert_eq!(stats.exhaustions, 0, "the descriptor heap was exhausted");
    let release = app.ui_release_report();
    assert_eq!(release.unknown, 0, "a texture was released twice");
    assert!(release.freed > 0, "no screen ever released its images");
    assert!(
        steady.high_water < steady.capacity,
        "high water {} of {}",
        steady.high_water,
        steady.capacity
    );
    eprintln!(
        "descriptors: live {} / high water {} / frontier {} of {} (free {}), releases {release:?}",
        steady.live, steady.high_water, steady.frontier, steady.capacity, steady.free
    );
    app.shutdown();
}
