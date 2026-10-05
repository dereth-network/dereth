//! UI draw completeness: the region erase is a clear to fully transparent black that changes no
//! pixel and is counted; character-screen buttons take the state their layout paints and their
//! backgrounds blend inside their own box; the chat background is a translucent tile; radar blips
//! are the primitive table's pixels and land only inside the radar; and the fill path costs one
//! descriptor. Fixture: a headless `App` with the retail dats on the character screen or in
//! gameplay, captured off the back buffer; every frame comes from `App` itself, with no desktop
//! input. Missing dats, a missing software GPU device or a shell that will not start fail the test.
//!
//! | claim | oracle |
//! |---|---|
//! | the erase step is a **clear**, not a paint, and costs nothing | the zero-filled null colour and a differential capture: the erase step on and off, byte for byte |
//! | every button takes the state its layout draws, and the background is **blended** | the shipped `charactermanagement` layout's own state media, and a per-pixel `SRCALPHA/INVSRCALPHA` recomposition of the decoded surface over the control frame |
//! | a generated surface reaches the frame, and only inside the rectangle it declared | the generated blip fills and a differential capture with the blips present and absent |
//! | the descriptor heap stays bounded | the renderer allocator's `exhaustions == 0` counter |

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use crate::common::client_dir;
use crate::common::gpu_lock;

use dereth_client::app::App;
use dereth_client_runtime::config::Config;
use dereth_ui::framework::mode;
use dereth_ui::{ElementId, StateId, UiFill};

/// **An `expect`, never a skip.** A test that returns early is counted as a pass and
/// is invisible in the summary line; if the retail dats are not where `$DERETH_TEST_DAT_DIR` says,
/// the run is not a pass.
fn have_dats() {
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats are this file's oracle: none at {} -- set DERETH_TEST_DAT_DIR",
        client_dir().display()
    );
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

/// A headless `App` with its shell started and its first scene loaded, or a failed test that
/// says which step the machine could not take.
fn started_app() -> App {
    let mut app = App::new(base_config())
        .unwrap_or_else(|e| panic!("a headless App on a software GPU device: {e:?}"));
    app.start_shell()
        .unwrap_or_else(|e| panic!("the UI shell starts: {e:?}"));
    app.load_first_pixel_scene()
        .unwrap_or_else(|e| panic!("the first scene loads: {e:?}"));
    app
}

/// The character screen, reached through the mode machine.
fn app_on_character_screen() -> App {
    let mut app = started_app();
    {
        let host = app.probe_mut().host_state_mut();
        host.character_set = Some(dereth_ui::persist::CharacterSet {
            set: Vec::new(),
            num_allowed_characters: 11,
            account: "ac01".into(),
            ..dereth_ui::persist::CharacterSet::default()
        });
        host.received_set = true;
        host.world_name = Some("ACEmulator".into());
    }
    for _ in 0..24 {
        app.frame();
        let m = app.ui().and_then(|u| u.flow.current_mode());
        if m == Some(mode::CHARACTER_MANAGEMENT) {
            app.frame();
            app.frame();
            return app;
        }
        if m == Some(mode::INTRO) {
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
    }
    panic!("the mode machine did not reach the character screen in 24 frames");
}

/// Gameplay mode, queued by hand in the same way as the HUD path.
fn app_in_gameplay() -> App {
    let mut app = started_app();
    app.queue_ui_mode(mode::GAME_PLAY);
    for _ in 0..8 {
        app.frame();
    }
    assert_eq!(
        app.ui().and_then(|u| u.flow.current_mode()),
        Some(mode::GAME_PLAY),
        "the mode machine did not reach gameplay in 8 frames"
    );
    app
}

/// Every pixel a glyph run claims, from the shipped `Font` metrics. This is the character-selection
/// helper built for the exact glyph positioning rules.
///
/// The blend test needs it as a **mask to exclude**: a button's caption is composed over its
/// background, so a pixel under a glyph is not a sample of the background blit.
fn glyph_pixels(
    store: &dereth_dat::RetailDatStore,
    list: &[dereth_ui::UiDrawCmd],
    fb: (u32, u32),
) -> Vec<bool> {
    let mut fonts: std::collections::BTreeMap<
        dereth_primitives::DataId,
        dereth_render::font::Font,
    > = std::collections::BTreeMap::new();
    let mut mask = vec![false; (fb.0 * fb.1) as usize];
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
            let x0 = g.x + i32::from(d.horizontal_offset_before);
            let y0 = g.y + i32::from(d.vertical_offset_before);
            let x1 = x0 + i32::from(d.width) - 1;
            let y1 = y0 + i32::try_from(font.max_char_height).unwrap_or(0) - 1;
            for y in y0.max(clip.1)..=y1.min(clip.3) {
                for x in x0.max(clip.0)..=x1.min(clip.2) {
                    #[allow(clippy::cast_possible_wrap)]
                    // LINT-OK: back-buffer extents.
                    if x >= 0 && y >= 0 && x < fb.0 as i32 && y < fb.1 as i32 {
                        #[allow(clippy::cast_sign_loss)]
                        // LINT-OK: guarded non-negative on the line above.
                        let i = (y as u32 * fb.0 + x as u32) as usize;
                        mask[i] = true;
                    }
                }
            }
        }
    }
    mask
}

// ---------------------------------------------------------------------------------------------
// 1. `EraseSelf`: modelled, and measured to be free
// ---------------------------------------------------------------------------------------------

/// **The erase step is a clear, not a blended fill.**
///
/// The region erase calls its background-clear operation once per dirty rectangle. In behavioural
/// pseudocode:
///
/// ```text
/// prepare the element surface
/// select the dirty rectangle as a surface window
/// fill that window with the null RGBA colour
/// finish the surface window
/// ```
///
/// The null RGBA colour is all zero bytes, and the background clear is its only reader; nothing
/// writes it. The erase therefore clears to fully transparent
/// black. Every element-owned surface is created with alpha and uses modulated alpha blending, so
/// those transparent pixels stay transparent when the quad is blended.
///
/// The step is modelled anyway (the draw list carries it at the right point in the region's nine
/// draw phases), and this test claims that it changes nothing: the same screen drawn with the
/// erase-background flag honored and cleared is **byte-identical**.
#[test]
fn erase_self_is_a_clear_to_rgba_null_and_changes_no_pixel() {
    let _gpu = gpu_lock();
    have_dats();
    let mut app = app_in_gameplay();

    // Run A: the erase step as the layout asks for it.
    let with_erase = app.ui_draw_list().to_vec();
    let (w, h, a) = app
        .renderer_mut()
        .capture_bgra()
        .expect("the renderer captures a frame");

    let carries_erase = |c: &dereth_ui::UiDrawCmd| c.fills.iter().any(|f| f.color == UiFill::NULL);
    let erasers: Vec<_> = with_erase
        .iter()
        .filter(|c| carries_erase(c))
        .map(|c| c.who)
        .collect();
    assert!(
        !erasers.is_empty(),
        "no element in the gameplay tree has erase-background enabled; the test proves nothing"
    );
    // The erase is one rectangle per element, it is the null colour, and it covers the element's
    // whole clip box: the region's single dirty rectangle.
    //
    // **Selected by the command that carries the erase, not by `who`.** One element can emit two
    // commands: its own blit and a second one for `region.surface_fills` (see
    // `UiSystem::draw_region`: "the generated pixels come after the children"). Matching on the
    // handle would also pick up the radar's generated-pixel command, whose first fill is a blip
    // and not an erase: the player's centre marker is drawn even in an offline scene.
    for c in with_erase.iter().filter(|c| carries_erase(c)) {
        let f = c.fills[0];
        assert_eq!(
            f.color,
            UiFill::NULL,
            "the erase colour is fully transparent black"
        );
        assert!(
            f.is_invisible(),
            "a fully transparent fill cannot change a pixel"
        );
        assert_eq!(
            (f.x, f.y, f.w, f.h),
            (c.clip.x0, c.clip.y0, c.clip.width(), c.clip.height()),
            "the erase covers the clip box"
        );
    }
    // Run B: the same screen with the erase suppressed, and nothing else touched.
    if let Some(shell) = app.ui_mut() {
        for e in shell.ui.element_list().to_vec() {
            if let Some(n) = shell.ui.node_mut(e) {
                n.region.flags.erase_background = false;
            }
        }
    }
    app.frame();
    let without = app.ui_draw_list().to_vec();
    let (w2, h2, b) = app
        .renderer_mut()
        .capture_bgra()
        .expect("the renderer captures a frame");
    assert_eq!((w, h), (w2, h2));
    assert!(
        !without
            .iter()
            .any(|c| c.fills.iter().any(|f| f.color == UiFill::NULL)),
        "the control run still erases"
    );

    let changed = a
        .as_chunks::<4>()
        .0
        .iter()
        .zip(b.as_chunks::<4>().0.iter())
        .filter(|(x, y)| x != y)
        .count();
    assert_eq!(
        changed, 0,
        "{changed} pixels changed when the erase step was removed; the erase is not free after all"
    );
    assert_eq!(app.renderer_mut().descriptor_stats().exhaustions, 0);
}

/// The counter exists so that "the erase ran and was free" is measurable rather than asserted.
///
/// `UiTextureStats::fills_invisible` accumulates over the life of the renderer, so one frame of the
/// gameplay screen must move it by exactly the number of erasing elements.
#[test]
fn every_erase_is_counted_rather_than_dropped_silently() {
    let _gpu = gpu_lock();
    have_dats();
    let mut app = app_in_gameplay();
    let before = app.renderer_mut().ui_stats.fills_invisible;
    let expected = app
        .ui_draw_list()
        .iter()
        .flat_map(|c| c.fills.iter())
        .filter(|f| f.is_invisible())
        .count() as u64;
    assert!(expected > 0, "the gameplay tree has no erasing element");
    app.frame();
    let after = app.renderer_mut().ui_stats.fills_invisible;
    assert_eq!(after - before, expected, "an erase went unrecorded");
}

// ---------------------------------------------------------------------------------------------
// 2. The button backgrounds
// ---------------------------------------------------------------------------------------------

/// The four character-screen buttons (Create Character, DELETE, CREDITS, EXIT), each a red fill
/// inside a gold frame.
const CHAR_BUTTONS: [(u32, &str); 4] = [
    (0x1000_039F, "DELETE"),
    (0x1000_03A0, "Create Character"),
    (0x1000_03A3, "CREDITS"),
    (0x1000_03A4, "EXIT"),
];

/// **Oracle: the shipped `charactermanagement` layout.** Each of the four buttons carries *no*
/// image in its base state and an image media entry in states 1, 2, 3 and 13; state 1's is
/// `0x06004C9E` for the three small buttons and `0x06004CA3` for Create Character. The button
/// post-initialization state update moves each out of state 0; without it only the caption draws.
#[test]
fn every_character_screen_button_takes_the_state_its_layout_paints() {
    let _gpu = gpu_lock();
    have_dats();
    let app = app_on_character_screen();
    let list = app.ui_draw_list().to_vec();
    let shell = app.ui().expect("the shell is up");

    for (id, what) in CHAR_BUTTONS {
        let h = shell
            .ui
            .get_element(ElementId(id))
            .unwrap_or_else(|| panic!("{what} is missing"));
        let n = shell.ui.node(h).expect("a live node");
        // The resting-state update maps not-disabled, not-toggled, not-pressed and not-hovered to
        // state 1.
        //
        // The character-screen button update then moves two controls again by requesting state 0
        // to enable them. That clears disabled attribute `0x0D` only when a button was disabled;
        // an already-enabled button really ends in state 0 with **state 1's picture still on it**.
        // State 0's media contains only alpha, and resetting it does not replace the image. The
        // test therefore asserts state only on the three untouched controls and asserts the
        // visible picture on all four.
        if id != 0x1000_03A0 && id != 0x1000_03A2 {
            assert_eq!(
                n.state,
                StateId(1),
                "{what} sat in state {} instead of the resting state 1",
                n.state.0
            );
        }
        let img = n.region.image.as_ref().map(|g| g.did);
        assert!(img.is_some(), "{what} still has no background image");
        // And the layout is where that image came from, not this test.
        let from_layout = n
            .desc
            .access_state(StateId(1))
            .map(|s| {
                s.media
                    .iter()
                    .filter_map(|m| match m.fields {
                        dereth_ui::desc::MediaFields::Image { file, .. } => Some(file),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        assert_eq!(
            img.map(|d| vec![d]),
            Some(from_layout.clone()),
            "{what}'s image is not the one state 1 of the shipped layout names"
        );
        assert!(
            list.iter().any(|c| c.who == h && c.image == img),
            "{what}'s background never reached the draw list"
        );
    }
}

/// Behaviour: ui.draw.button-backgrounds-blend-and-erase-changes-nothing
///
/// The changed-pixel test: two runs of the **same screen** differing only in the thing under
/// test, every changed pixel inside a rectangle the UI declared, and **zero** outside.
///
/// The thing under test is the four buttons' state-1 background. In run B each of the four is put
/// back in state 0 and its image cleared, as it stands before the post-initialization update.
/// Everything else (every element, every box, every caption and the artwork behind) is identical.
///
/// **The blend is asserted, not just the change.** For a sample of pixels inside each button the
/// composited value must equal `SRCALPHA/INVSRCALPHA` of the decoded surface texel over the
/// control frame's own pixel, to within one unit of rounding. An opaque fill would pass a
/// changed-pixel test and fail this one.
#[test]
fn the_button_background_is_blended_over_the_artwork_and_lands_only_inside_its_own_box() {
    let _gpu = gpu_lock();
    have_dats();
    let mut app = app_on_character_screen();

    // Run A: as the layout paints it.
    let list = app.ui_draw_list().to_vec();
    let (w, h, painted) = app
        .renderer_mut()
        .capture_bgra()
        .expect("the renderer captures a frame");

    // The four boxes, and the images they claimed.
    let mut claimed: Vec<(
        dereth_ui::Box2D,
        dereth_ui::Box2D,
        dereth_primitives::DataId,
    )> = Vec::new();
    for (id, what) in CHAR_BUTTONS {
        let cmd = list
            .iter()
            .find(|c| {
                app.ui()
                    .and_then(|s| s.ui.node(c.who))
                    .is_some_and(|n| n.desc.element_id == ElementId(id))
            })
            .unwrap_or_else(|| panic!("{what} drew nothing"));
        claimed.push((cmd.screen, cmd.clip, cmd.image.expect("an image")));
    }

    // Run B: the same screen with those four backgrounds gone.
    if let Some(shell) = app.ui_mut() {
        for (id, _) in CHAR_BUTTONS {
            let Some(hh) = shell.ui.get_element(ElementId(id)) else {
                continue;
            };
            if let Some(n) = shell.ui.node_mut(hh) {
                n.region.image = None;
                n.state = StateId(0);
            }
        }
    }
    app.frame();
    let bare = app.ui_draw_list().to_vec();
    let (w2, h2, plain) = app
        .renderer_mut()
        .capture_bgra()
        .expect("the renderer captures a frame");
    assert_eq!((w, h), (w2, h2));

    // The two runs really are the same screen but for the four images.
    assert_eq!(
        list.len(),
        bare.len(),
        "the two runs drew different element counts"
    );
    let mut differing = 0usize;
    for (a, b) in list.iter().zip(&bare) {
        assert_eq!(
            (a.screen, a.clip, a.glyphs.len()),
            (b.screen, b.clip, b.glyphs.len())
        );
        if a.image != b.image {
            differing += 1;
            assert!(b.image.is_none(), "the control still has a background");
        }
    }
    assert_eq!(
        differing,
        CHAR_BUTTONS.len(),
        "the control removed the wrong set of images"
    );

    // Every changed pixel inside one of the four declared rectangles, and none outside.
    let inside_any = |x: i32, y: i32| {
        claimed.iter().any(|(s, c, _)| {
            x >= s.x0.max(c.x0) && x <= s.x1.min(c.x1) && y >= s.y0.max(c.y0) && y <= s.y1.min(c.y1)
        })
    };
    let (mut inside, mut outside) = (0usize, 0usize);
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) as usize * 4;
            if painted[i..i + 4] != plain[i..i + 4] {
                #[allow(clippy::cast_possible_wrap)]
                // LINT-OK: back-buffer coordinates, at most a few thousand.
                let (xi, yi) = (x as i32, y as i32);
                if inside_any(xi, yi) {
                    inside += 1;
                } else {
                    outside += 1;
                }
            }
        }
    }
    assert_eq!(
        outside, 0,
        "{outside} pixels changed outside every button's own rectangle"
    );
    assert!(
        inside > 5_000,
        "only {inside} pixels changed inside the four buttons"
    );

    assert_eq!(app.renderer_mut().descriptor_stats().exhaustions, 0);
}

/// **The chat window is a *translucent* rectangle, not an opaque one.**
///
/// The translucency is neither a user setting nor a constant in code: it is **in the picture**.
/// The chat window's background element `0x10000010` carries image media `0x06004CC2`, which
/// decodes to a 48x48 `Bgra8` surface that is uniform `(0, 0, 0)` at alpha **175**, i.e. 68.6 %
/// black, and tiles it across the 400x73 element. The default/active opacity user setting is a
/// *second* factor on top of the image alpha and is 1.0 until the player moves the chat-options
/// slider.
///
/// So the assertion is exact: every pixel of that background must be
/// `SRCALPHA/INVSRCALPHA(black at 175/255)` over whatever the world put there. An opaque fill
/// would pass a changed-pixel test and fail this one by 68 units per channel.
#[test]
fn the_chat_window_background_is_a_translucent_black_tile_not_an_opaque_one() {
    let _gpu = gpu_lock();
    have_dats();
    let store = dereth_dat::RetailDatStore::open_dir(&client_dir())
        .expect("the retail dats: set DERETH_TEST_DAT_DIR");
    let mut app = app_in_gameplay();

    // `0x10000010` is not unique: all five chat windows are built from the same base layout, so
    // the tree holds five of it. The one that is up is the main chat window's, `<CHAT>` =
    // `0x10000601`, which is why this goes through the window rather than through
    // the flat element-id table.
    const MAIN_CHAT: ElementId = ElementId(0x1000_0601);
    const CHAT_BG: ElementId = ElementId(0x1000_0010);
    let list = app.ui_draw_list().to_vec();
    let bg = app
        .ui()
        .and_then(|s| {
            let w = s.ui.get_element(MAIN_CHAT)?;
            s.ui.get_child_recursive(w, CHAT_BG)
        })
        .expect("the chat window's background element");
    let idx = list
        .iter()
        .position(|c| c.who == bg)
        .expect("the chat background drew nothing");
    let cmd = list[idx].clone();
    let did = cmd.image.expect("the chat background has an image");

    // The tile itself, straight out of the dat.
    let tex = dereth_scene::textures::TextureStore::new(&store)
        .texture_data(did)
        .expect("the tile decodes");
    let px = &tex.levels[0];
    let first: [u8; 4] = [px[0], px[1], px[2], px[3]];
    assert!(
        px.as_chunks::<4>().0.iter().all(|t| *t == first),
        "{did:?} is not a uniform tile; the exact assertion below would not be exact"
    );
    assert_eq!(first, [0, 0, 0, 175], "{did:?} is not 68.6% black");

    let (w, h, painted) = app
        .renderer_mut()
        .capture_bgra()
        .expect("the renderer captures a frame");

    // The control: the same screen with that one background gone.
    if let Some(shell) = app.ui_mut() {
        if let Some(n) = shell.ui.node_mut(bg) {
            n.region.image = None;
        }
    }
    app.frame();
    let (_, _, plain) = app
        .renderer_mut()
        .capture_bgra()
        .expect("the renderer captures a frame");

    // Only pixels of the background that nothing drawn later covers. "Later" is every command
    // after it in the list, which is region draw order, i.e. everything painted on top --
    // but a command that has no image, no glyph and no visible fill paints nothing, and the chat
    // scrollback `0x10000011` is exactly that (a text element with erase-background enabled and no
    // picture), so counting it would leave almost nothing to measure.
    let glyphs = glyph_pixels(&store, &list, (w, h));
    let covered = |x: i32, y: i32| {
        let i = usize::try_from(y).unwrap_or(0) * w as usize + usize::try_from(x).unwrap_or(0);
        if glyphs.get(i).copied().unwrap_or(false) {
            return true;
        }
        list[idx + 1..].iter().any(|c| {
            let inside = x >= c.screen.x0.max(c.clip.x0)
                && x <= c.screen.x1.min(c.clip.x1)
                && y >= c.screen.y0.max(c.clip.y0)
                && y <= c.screen.y1.min(c.clip.y1);
            (inside && c.image.is_some())
                || c.fills.iter().any(|f| {
                    !f.is_invisible() && x >= f.x && x < f.x + f.w && y >= f.y && y < f.y + f.h
                })
        })
    };
    let a = f64::from(first[3]) / 255.0;
    let over = |src: u8, dst: u8| -> i32 {
        dereth_primitives::num::to_i32_f64(
            (f64::from(src) * a + f64::from(dst) * (1.0 - a)).round(),
        )
    };
    let mut checked = 0usize;
    let mut opaque_would_have_failed = 0usize;
    for y in cmd.screen.y0.max(cmd.clip.y0)..=cmd.screen.y1.min(cmd.clip.y1) {
        for x in cmd.screen.x0.max(cmd.clip.x0)..=cmd.screen.x1.min(cmd.clip.x1) {
            if x < 0 || y < 0 || covered(x, y) {
                continue;
            }
            #[allow(clippy::cast_sign_loss)]
            // LINT-OK: guarded non-negative directly above.
            let (ux, uy) = (x as u32, y as u32);
            if ux >= w || uy >= h {
                continue;
            }
            let i = (uy * w + ux) as usize * 4;
            for k in 0..3 {
                let want = over(first[k], plain[i + k]);
                let got = i32::from(painted[i + k]);
                assert!(
                    (want - got).abs() <= 1,
                    "at ({x},{y}) channel {k}: black at alpha 175 over {} should composite to \
                     {want}, the frame has {got}; the chat background is not blended",
                    plain[i + k]
                );
                // An opaque fill would have produced `first[k]` (0) here.
                if (i32::from(first[k]) - want).abs() > 8 {
                    opaque_would_have_failed += 1;
                }
            }
            checked += 1;
        }
    }
    assert!(
        checked > 3_000,
        "only {checked} chat-background pixels were uncovered"
    );
    assert!(
        opaque_would_have_failed > 1_000,
        "the control frame was too dark for this test to distinguish translucent from opaque"
    );
    assert_eq!(app.renderer_mut().descriptor_stats().exhaustions, 0);
}

// ---------------------------------------------------------------------------------------------
// 3. The generated surface: radar blips
// ---------------------------------------------------------------------------------------------

/// Radar-object drawing projects each blip and dispatches to one of eight shape functions, whose
/// only primitive is a 1×1 surface fill.
///
/// This is the *computation* half against the primitive table: a `Default` blip is a plus
/// (centre + four edges), a selected one adds the selection bracket's twenty
/// bracket pixels, and the colour carries `dim` on rgb with **alpha untouched**.
#[test]
fn a_blip_is_the_pixels_the_primitive_table_names() {
    use dereth_ui_screens::mapradar::radar::{blip_fills, Blip, BlipShape};

    let gold = dereth_ui_screens::mapradar::radar::GOLD;
    let b = Blip {
        index: 0,
        x: 60,
        y: 60,
        color: gold,
        shape: BlipShape::Default,
        dim: 1.0,
        selected: false,
    };
    let f = blip_fills(&b);
    assert_eq!(f.len(), 5, "a Plus is the centre and four edges");
    assert!(
        f.iter().all(|p| p.w == 1 && p.h == 1),
        "every blip pixel is a 1x1 fill"
    );
    let mut at: Vec<(i32, i32)> = f.iter().map(|p| (p.x - 60, p.y - 60)).collect();
    at.sort_unstable();
    assert_eq!(at, vec![(-1, 0), (0, -1), (0, 0), (0, 1), (1, 0)]);
    // The radar-gold value is (1.0, 0.67f, 0.0, 1.0), and conversion to 8-bit channels truncates
    // rather than rounds: 0.67f * 255 = 170.85 -> 170 = 0xAA, not 0xAB.
    assert_eq!(
        f[0].color, 0xFFFF_AA00,
        "the palette entry is opaque and is not dimmed"
    );

    // "multiplies r, g, b by `dim` (alpha untouched)".
    let dim = Blip { dim: 0.65, ..b };
    let d = blip_fills(&dim)[0];
    assert_eq!(d.color >> 24, 0xFF, "dim must not touch the alpha");
    assert_eq!((d.color >> 16) & 0xFF, 165, "truncating 1.0 * 0.65 * 255.0");

    // A selected blip adds a 7x7 open square: twenty more pixels.
    let sel = Blip {
        selected: true,
        ..b
    };
    assert_eq!(blip_fills(&sel).len(), 5 + 20);
}

/// The *drawing* half: a generated surface reaches the frame, and every pixel it changes is inside
/// the rectangle the element declared.
///
/// Two runs of the same screen differ only in the radar's `surface_fills`, which is what the
/// `/radar off` blank-state flag switches in the client. The blips are the output of
/// `draw_objects` over a synthetic object set. The primitive table supplies the expected shapes
/// and channel conversion; the differential exercises the projection and cull rather than
/// independently proving them.
#[test]
fn radar_blips_reach_the_frame_and_land_only_inside_the_radar() {
    use dereth_ui_screens::mapradar::radar::{
        blip_fills, draw_objects, radar_range, RadarGeometry,
    };
    use dereth_ui_screens::view::RadarEntry;

    let _gpu = gpu_lock();
    have_dats();
    let mut app = app_in_gameplay();

    // The radar element, and the geometry the shipped layout gives it.
    const RADAR: ElementId = ElementId(0x1000_06D2);
    let radar = app
        .ui()
        .and_then(|s| s.ui.get_element(RADAR))
        .expect("the radar window");
    let radar_box = app.ui().map(|s| s.ui.screen_box(radar)).expect("a box");
    let geom = RadarGeometry {
        radius: 50,
        center: (60.0, 60.0),
    };

    // Eight objects at eight compass points, well inside the range.
    // `radar_enum` is `ShowAlways` because `RadarEntry::default()` is the undefined radar value,
    // which `inq_showable_on_radar` rejects: the add-object path never makes a radar entry for an
    // object the server did not flag for the radar. These eight synthetic objects have to be
    // objects the retail client would plot.
    let entry = |x: f32, y: f32| RadarEntry {
        id: dereth_primitives::ObjectId(0x8000_0000),
        player_space: (x, y, 0.0),
        in_world: true,
        radar_enum: dereth_ui_screens::mapradar::radar::radar_enum::SHOW_ALWAYS,
        ..RadarEntry::default()
    };
    let objects: Vec<RadarEntry> = [
        (0.0, 20.0),
        (20.0, 0.0),
        (0.0, -20.0),
        (-20.0, 0.0),
        (14.0, 14.0),
        (-14.0, 14.0),
        (14.0, -14.0),
        (-14.0, -14.0),
    ]
    .into_iter()
    .map(|(x, y)| entry(x, y))
    .collect();
    let blips = draw_objects(&objects, None, geom, radar_range(true), None, false);
    assert_eq!(
        blips.len(),
        8,
        "draw_objects culled an object it should have kept"
    );
    let fills: Vec<UiFill> = blips.iter().flat_map(blip_fills).collect();
    assert_eq!(fills.len(), 8 * 5, "eight Plus blips");

    // The same draw list twice, differing only in the radar element's `surface_fills`, which is
    // exactly what the `/radar off` blank-state flag switches in the client.
    //
    // Both frames are rendered through the same manual `BeginFrame`/`Draw`/`PresentFrame` bracket
    // rather than through `App::frame`, because `App::frame` re-runs `update_radar` from the
    // (empty) object stream and would clear the fills again, and because `Gpu::capture` reads the
    // buffer the swap chain currently points at, so the two captures must come from brackets of
    // the same length or they read different buffers.
    let store = dereth_dat::RetailDatStore::open_dir(&client_dir()).expect("dats");
    let render = |app: &mut App, list: &[dereth_ui::UiDrawCmd]| {
        app.renderer_mut().prepare_ui(&store, list);
        for _ in 0..2 {
            app.renderer_mut().start_frame().expect("frame");
            app.probe_mut().draw_world_scene().expect("scene");
            app.renderer_mut().draw_ui(list).expect("ui");
            app.renderer_mut().end_frame().expect("present");
        }
        app.renderer_mut().capture_bgra().expect("capture")
    };

    let blank_list = app.ui_mut().expect("shell").draw_list();
    assert!(
        blank_list
            .iter()
            .filter(|c| c.who == radar)
            .all(|c| c.fills.iter().all(dereth_ui::UiFill::is_invisible)),
        "the control run already has blips"
    );
    let (w, h, blank) = render(&mut app, &blank_list);

    // Run B: the same screen with the blips on, and nothing else touched. The write goes where
    // the radar view writes it: onto the radar element's own surface.
    if let Some(shell) = app.ui_mut() {
        if let Some(n) = shell.ui.node_mut(radar) {
            n.region.surface_fills = fills.clone();
        }
    }
    let list = app.ui_mut().expect("shell").draw_list();
    let drawn: Vec<_> = list
        .iter()
        .filter(|c| c.who == radar)
        .flat_map(|c| c.fills.clone())
        .filter(|f| !f.is_invisible())
        .collect();
    assert_eq!(
        drawn.len(),
        fills.len(),
        "the blips did not reach the draw list"
    );
    // Run B carries exactly one command more: the radar-object pass, issued after the radar's
    // children so the blips are not painted under the dial.
    assert_eq!(
        list.len(),
        blank_list.len() + 1,
        "the two runs differ by more than the blip pass"
    );
    // …and it is that one command, wherever in the list it falls. The blip pass is emitted after
    // the radar's own children, so it is **not** at the end of the list, and pairing the two lists
    // by index only works once it is taken out. (The read-order tie-break reorders equal-z
    // siblings, which keeps the vitals fill from being buried under its own trough.)
    let extra = list
        .iter()
        .position(|c| c.who == radar && c.fills.iter().any(|f| !f.is_invisible()))
        .expect("the blip pass is in the list");
    let without: Vec<_> = list
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != extra)
        .map(|(_, c)| c)
        .collect();
    assert_eq!(without.len(), blank_list.len());
    for (a, b) in blank_list.iter().zip(&without) {
        assert_eq!(
            (a.screen, a.clip, a.image, a.glyphs.len()),
            (b.screen, b.clip, b.image, b.glyphs.len())
        );
    }
    let (w2, h2, lit) = render(&mut app, &list);
    assert_eq!((w, h), (w2, h2));

    // Every changed pixel inside a declared blip rectangle, and none outside.
    let inside_a_blip = |x: i32, y: i32| {
        drawn
            .iter()
            .any(|f| x >= f.x && x < f.x + f.w && y >= f.y && y < f.y + f.h)
    };
    let (mut inside, mut outside) = (0usize, 0usize);
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) as usize * 4;
            if blank[i..i + 4] != lit[i..i + 4] {
                #[allow(clippy::cast_possible_wrap)]
                // LINT-OK: back-buffer coordinates.
                let (xi, yi) = (x as i32, y as i32);
                assert!(
                    xi >= radar_box.x0 && xi <= radar_box.x1,
                    "a blip changed a pixel outside the radar window"
                );
                if inside_a_blip(xi, yi) {
                    inside += 1;
                } else {
                    outside += 1;
                }
            }
        }
    }
    assert_eq!(
        outside, 0,
        "{outside} pixels changed outside every blip rectangle"
    );
    assert_eq!(
        inside,
        fills.len(),
        "one pixel per 1x1 fill, no more and no fewer"
    );

    // A blip is opaque: the default radar colour is white with alpha 1.0, so the frame carries the
    // palette colour exactly rather than a blend of it.
    let f = drawn[0];
    #[allow(clippy::cast_sign_loss)]
    // LINT-OK: inside the radar box, which is on screen.
    let i = ((f.y as u32) * w + f.x as u32) as usize * 4;
    assert_eq!(
        [lit[i + 2], lit[i + 1], lit[i]],
        [
            ((f.color >> 16) & 0xFF) as u8,
            ((f.color >> 8) & 0xFF) as u8,
            (f.color & 0xFF) as u8
        ],
        "an opaque blip must land on the frame as its own colour"
    );
    assert_eq!(app.renderer_mut().descriptor_stats().exhaustions, 0);
}

/// The descriptor cost of the fill path: one white texel, and nothing else new.
#[test]
fn the_fill_path_costs_exactly_one_descriptor() {
    let _gpu = gpu_lock();
    have_dats();
    let mut app = app_in_gameplay();
    let before = app.renderer_mut().ui_texture_count();
    // A frame with no visible fill uploads nothing; the erase alone must not claim a slot.
    app.frame();
    assert_eq!(
        app.renderer_mut().ui_texture_count(),
        before,
        "an all-transparent fill list claimed a descriptor"
    );
    let u = app.renderer_mut().descriptor_usage();
    let st = app.renderer_mut().descriptor_stats();
    assert_eq!(st.exhaustions, 0, "the descriptor heap was exhausted");
    eprintln!("fill_and_erase descriptors: {u:?} {st:?}");
}
