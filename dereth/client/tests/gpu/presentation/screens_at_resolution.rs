//! Every screen but gameplay is forced to 800x600: the client opens at 800x600 whatever the
//! profile says, takes the chosen resolution when the gameplay screen is built, and returns to
//! 800x600 when it is destroyed, so logging out at an enlarged resolution shows character select
//! at 800x600.
//!
//! Fixture: the shipped layouts of every pre-game screen, loaded from the retail dats and laid
//! out at 800x600 and at 1600x900 (the authored edge modes of every root); and a headless App on
//! a software device driven into gameplay and back out. No datagram leaves the process and
//! nothing opens a socket; a machine without the retail dats fails.
//!
//! ## Forced display-resolution behavior
//!
//! The force operation always stores the supplied width and height, remembers the old
//! force flag, and then writes the new flag. It returns when no renderer exists or the device is
//! not initialized. When enabling the force, it returns if the display already has the requested
//! size; otherwise it reloads presentation preferences and applies the result. When disabling the
//! force, it returns if the old flag was already clear; if the display already has the restored
//! preference it also returns, otherwise it applies that presentation.
//!
//! Preference loading decodes width from the upper 16 bits and height from the lower 16 bits. It
//! rejects widths below 800 and heights below 600, copies an accepted preference to the
//! presentation, then overwrites those dimensions with the stored forced width and height whenever
//! forcing is active. It also disables full screen when full-screen mode is not allowed. Because
//! the override is inside preference loading, initialization, presentation changes, and the force
//! operation itself all honor it.
//!
//! There are exactly three client call sites. Client initialization begins by forcing 800x600,
//! before the shared initializer loads preferences. Gameplay-screen construction disables the
//! force. Gameplay-screen destruction re-enables 800x600 unless the device is already done. The
//! console command is a fourth entry point into the same force operation.
//!
//! So the client **opens its window at 800x600 whatever the profile says**, runs the login,
//! character-select, credits, disconnected, char-gen and epilogue screens at 800x600, takes the
//! player's chosen resolution the moment `GamePlayScreen` is constructed, and puts it back to
//! 800x600 the moment that screen is destroyed. Logging out is exactly that destruction.
//!
//! ## Why the screens do not re-anchor instead
//!
//! The root resize first scales the authored design box onto the display. When width or height is
//! nonzero, or the element is initialized, the per-edge path preserves each mode-0 live coordinate.
//! Four of the eight screens' roots are authored mode 0 on all four edges (the census below), so
//! at any display but 800x600 they would be an 800x600 island in the corner. Global UI message
//! `0x0E` cannot save them: it has one listener, the gameplay screen, and its handler only
//! reloads floaty-window layout positions. The separate UI refresh event resizes the hollow root
//! to the live width and height; device reset invokes that refresh through the render callback.
//!
//! # What this file asserts
//!
//! §1 is the denominator: one test per screen, naming the authored edge modes of every root and
//! the rectangle the transform produces at a non-800x600 display, read off the shipped dats.
//!
//! §2 is the force itself, on a real `App` over a headless software GPU.

#![cfg(gpu)]

use std::rc::Rc;

use dereth_primitives::{AssetSource, DataId};
use dereth_ui::framework::{DidMapperResolver, Screen};
use dereth_ui::layout::EdgeMode;
use dereth_ui::{Box2D, ElementId, UiFlow, UiSystem};
use dereth_ui_screens::screens::SCREENS;

/// The size that client initialization and gameplay-screen destruction both force.
const LOGIN: (i32, i32) = (800, 600);

/// A display nobody has ever viewed these screens at.
const BIG: (i32, i32) = (1600, 900);

fn ui_at(display: (i32, i32)) -> UiSystem {
    let dir = dereth_dat::testing::dat_dir();
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats are this file's oracle: none at {} -- set DERETH_TEST_DAT_DIR",
        dir.display()
    );
    let store = dereth_dat::RetailDatStore::open_dir(&dir).expect("the retail dats open");
    let master_id = DataId(0x3900_0001);
    let bytes = store.read(master_id).expect("MasterProperty");
    let master =
        <dereth_assets::MasterProperty as dereth_assets::Decode>::decode_payload(master_id, &bytes)
            .expect("MasterProperty decodes");
    let mut ui = UiSystem::new(display);
    ui.property_types = master.property_types();
    let mut flow = UiFlow::new();
    dereth_ui_screens::register_all(&mut ui, &mut flow);
    let store = Rc::new(store);
    let resolver =
        Rc::new(DidMapperResolver::load_via_master(store.as_ref()).expect("the DidMapper loads"));
    dereth_ui_screens::env::install(&mut ui, store, resolver);
    ui
}

fn make(class: &str) -> Box<dyn Screen> {
    match class {
        "DataPatchScreen" => {
            dereth_ui_screens::screens::datapatch::DataPatchScreen::create_screen()
        }
        "IntroScreen" => dereth_ui_screens::screens::intro::IntroScreen::create_screen(),
        "CharacterManagementScreen" => {
            dereth_ui_screens::screens::charmgmt::CharacterManagementScreen::create_screen()
        }
        "GamePlayScreen" => dereth_ui_screens::screens::gameplay::GamePlayScreen::create_screen(),
        "EpilogueScreen" => dereth_ui_screens::screens::epilogue::EpilogueScreen::create_screen(),
        "DisconnectedScreen" => {
            dereth_ui_screens::screens::disconnected::DisconnectedScreen::create_screen()
        }
        "CharGenScreen" => dereth_ui_screens::screens::chargen::CharGenScreen::create_screen(),
        "CreditsScreen" => dereth_ui_screens::screens::credits::CreditsScreen::create_screen(),
        other => panic!("no factory for {other}"),
    }
}

/// One screen's roots, built fresh at `display`, reported as `(element id, edges, box)`.
fn roots_at(class: &str, display: (i32, i32)) -> Vec<(ElementId, [EdgeMode; 4], Box2D)> {
    let mut ui = ui_at(display);
    let mut s = make(class);
    s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .unwrap_or_else(|e| panic!("{class}: {e}"));
    let out = s
        .roots()
        .iter()
        .filter_map(|h| {
            ui.node(*h).map(|n| {
                let e = n.desc.edges;
                (
                    n.element_id(),
                    [e.left, e.top, e.right, e.bottom],
                    n.region.box_,
                )
            })
        })
        .collect();
    for h in s.roots().to_vec() {
        ui.remove_and_delete_root(h);
    }
    s.destroy(&mut dereth_ui::framework::ScreenCx::new(&mut ui));
    ui.clean_delete_queue();
    ui.requests.clear();
    out
}

/// The same screen built at 800x600 and then handed a larger display under it —
/// the UI refresh path. The answer must match [`roots_at`]'s fresh build,
/// or "created at the new size" and "resized to it" would be two different clients.
fn roots_resized(class: &str, display: (i32, i32)) -> Vec<(ElementId, [EdgeMode; 4], Box2D)> {
    let mut ui = ui_at(LOGIN);
    let mut s = make(class);
    s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .unwrap_or_else(|e| panic!("{class}: {e}"));
    ui.refresh_event(display);
    let out = s
        .roots()
        .iter()
        .filter_map(|h| {
            ui.node(*h).map(|n| {
                let e = n.desc.edges;
                (
                    n.element_id(),
                    [e.left, e.top, e.right, e.bottom],
                    n.region.box_,
                )
            })
        })
        .collect();
    for h in s.roots().to_vec() {
        ui.remove_and_delete_root(h);
    }
    s.destroy(&mut dereth_ui::framework::ScreenCx::new(&mut ui));
    ui.clean_delete_queue();
    ui.requests.clear();
    out
}

fn check(class: &str, want: &[(u32, [EdgeMode; 4], Box2D)]) {
    let fresh = roots_at(class, BIG);
    let resized = roots_resized(class, BIG);
    let expect: Vec<(ElementId, [EdgeMode; 4], Box2D)> = want
        .iter()
        .map(|(id, e, b)| (ElementId(*id), *e, *b))
        .collect();
    assert_eq!(fresh, expect, "{class}: built fresh at {BIG:?}");
    assert_eq!(
        resized, expect,
        "{class}: built at 800x600 and then resized to {BIG:?} by the resize refresh"
    );
}

// =============================================================================================
// 1. The denominator — all eight screens, both roots of the credits, at 1600x900
//
// Every rectangle here applies the size-and-position update to the shipped layout's authored
// design box with `dw = 800`, `dh = 300`; the layouts are the oracle.
// =============================================================================================

use EdgeMode::{AnchorEnd as End, AnchorStart as Start, Centre, Fixed};

/// **Mode 0 on all four edges: the root does not move and does not grow.** The datapatch screen
/// is an 800x600 island at the origin of a 1600x900 window.
#[test]
fn datapatch_does_not_re_anchor() {
    check(
        "DataPatchScreen",
        &[(0x1000_041A, [Fixed; 4], Box2D::new(0, 0, 799, 599))],
    );
}

/// **The login screen, and it is the same story as character select.** Mode 0 on all four edges.
#[test]
fn the_login_screen_does_not_re_anchor() {
    check(
        "IntroScreen",
        &[(0x1000_0419, [Fixed; 4], Box2D::new(0, 0, 799, 599))],
    );
}

/// **Character select.** `CharacterManagementScreen`'s root `0x1000039A` is mode 0 on all four
/// edges, so at 1600x900 the whole character-select screen sits in the top-left 800x600 of the
/// window with the other three quarters empty.
#[test]
fn character_select_does_not_re_anchor() {
    check(
        "CharacterManagementScreen",
        &[(0x1000_039A, [Fixed; 4], Box2D::new(0, 0, 799, 599))],
    );
}

/// The epilogue — the screen a logout actually passes *through* — is mode 0 as well.
#[test]
fn the_epilogue_does_not_re_anchor() {
    check(
        "EpilogueScreen",
        &[(0x1000_0399, [Fixed; 4], Box2D::new(0, 0, 799, 599))],
    );
}

/// **Mode 1 on all four edges: left/top pinned, right/bottom follow the delta.** The gameplay
/// screen is the one screen authored to fill whatever it is given, which is exactly the screen
/// retail lets the player resize.
#[test]
fn gameplay_fills_the_display() {
    check(
        "GamePlayScreen",
        &[(0x1000_0495, [Start; 4], Box2D::new(0, 0, 1599, 899))],
    );
}

/// **Mode 3 on all four edges: centred.** `new_w/2 - own_w/2 = 800 - 400 = 400`,
/// `new_h/2 - own_h/2 = 450 - 300 = 150` — an 800x600 screen centred in the window rather than
/// stretched. Not wrong, but not filled either.
#[test]
fn the_disconnected_screen_centres_itself() {
    check(
        "DisconnectedScreen",
        &[(0x1000_0416, [Centre; 4], Box2D::new(400, 150, 1199, 749))],
    );
}

/// Char-gen centres itself too, by the same arithmetic.
#[test]
fn char_gen_centres_itself() {
    check(
        "CharGenScreen",
        &[(0x1000_03CC, [Centre; 4], Box2D::new(400, 150, 1199, 749))],
    );
}

/// The credits screen builds **two** roots from one layout enum, both mode 1, and both grow by
/// the full `dw`. The right-hand pane keeps its authored `x0 = 400` while the left-hand pane's
/// right edge travels to 1199, so at 1600x900 they overlap across 800 pixels. That is the
/// shipped layout's own arithmetic and it is recorded here rather than treated as a defect —
/// nothing in retail ever renders it, because the credits screen is forced to 800x600 too.
#[test]
fn the_credits_two_roots_both_grow_and_overlap() {
    check(
        "CreditsScreen",
        &[
            (0x1000_0413, [Start; 4], Box2D::new(0, 0, 1199, 899)),
            (0x1000_0410, [Start; 4], Box2D::new(400, 0, 1599, 899)),
        ],
    );
}

/// The count itself, so the denominator is asserted and not just described: **eight
/// screens, nine roots; the gameplay root and both credits roots use Start, two roots use Centre,
/// and four roots use Fixed.**
#[test]
fn the_denominator_is_eight_screens_and_one_of_them_follows_the_display() {
    let mut fills = 0;
    let mut centres = 0;
    let mut frozen = 0;
    let mut roots = 0;
    for spec in SCREENS {
        for (_, edges, _) in roots_at(spec.class, BIG) {
            roots += 1;
            match edges[0] {
                Start => fills += 1,
                Centre => centres += 1,
                Fixed => frozen += 1,
                End | EdgeMode::Proportional => panic!("{}: unexpected edge mode", spec.class),
            }
        }
    }
    assert_eq!(SCREENS.len(), 8, "UiFlow registers eight screens");
    assert_eq!(roots, 9, "the credits screen builds two");
    assert_eq!((fills, centres, frozen), (3, 2, 4));
}

// =============================================================================================
// 2. The force — a real `App` on a headless software device
// =============================================================================================

mod wired {
    use std::path::PathBuf;

    use dereth_client::app::App;
    use dereth_ui::framework::mode;
    use dereth_ui_screens::options::store;
    use dereth_ui_screens::{PrefValue, UiRequest};
    use {dereth_client_runtime::config::Config, dereth_client_runtime::config::Preferences};

    fn client_dir() -> PathBuf {
        dereth_dat::testing::dat_dir()
    }

    /// An `App` whose profile names 1600x900, as a profile does after the resolution drop-down.
    fn app_with(resolution: Option<&str>) -> App {
        assert!(
            dereth_dat::testing::have_dats(),
            "the retail dats are this file's oracle: none at {} -- set DERETH_TEST_DAT_DIR",
            client_dir().display()
        );
        let mut cfg = Config {
            ui: true,
            headless: true,
            sound: false,
            connect: false,
            dat_dir: client_dir(),
            preferences_file: std::env::temp_dir()
                .join("dereth-screens-at-resolution-not-created/prefs.ini"),
            ..Config::default()
        };
        if let Some(r) = resolution {
            cfg.apply_preferences(&Preferences::parse(&format!(
                "[Display]\r\nFullScreen=False\r\nResolution={r}\r\n"
            )));
        }
        let mut a = App::new(cfg).expect("an application");
        a.start_shell().expect("the shell starts");
        a.ui_mut().expect("the UI shell is up").ui.requests.clear();
        a
    }

    /// The one-frame lag: the device event loop runs before the UI
    /// request drain, so the frame that changes something and the frame that applies it differ.
    fn settle(a: &mut App) {
        a.frame();
        a.frame();
    }

    fn enter(a: &mut App, m: dereth_ui::UiMode) {
        a.queue_ui_mode(m);
        settle(a);
        assert_eq!(a.ui().expect("a shell").flow.current_mode(), Some(m));
    }

    /// Behaviour: presentation.resolution.every-screen-but-gameplay-is-forced-to-eight-hundred-by-six-hundred
    ///
    /// Enter the world at 1600x900, log out, and the presentation is 800x600 again —
    /// gameplay-screen destruction re-enables the forced 800x600 presentation.
    #[test]
    fn logging_out_at_an_enlarged_resolution_returns_to_800x600() {
        let mut a = app_with(Some("1600x900"));
        assert_eq!(
            a.renderer().size(),
            (800, 600),
            "client initialization forces 800x600 before loading a preference, so \
             preference loading can only return the forced size"
        );

        enter(&mut a, mode::GAME_PLAY);
        assert_eq!(
            a.renderer().size(),
            (1600, 900),
            "gameplay-screen construction removes the forced size and restores the preference"
        );
        assert!(!a.uses_forced_resolution());

        enter(&mut a, mode::CHARACTER_MANAGEMENT);
        assert_eq!(
            a.renderer().size(),
            (800, 600),
            "gameplay-screen destruction restores the forced login size"
        );
        assert!(a.uses_forced_resolution());
        assert_eq!(
            a.config().display.resolution,
            u32::from_ne_bytes(store::mode_desc(1600, 900).to_ne_bytes()),
            "the chosen resolution remains stored while the force overrides presentation size"
        );
    }

    /// What the player sees: character select fills the window, because the
    /// window is the size the screen was authored for. Asserted on the UI element manager's display
    /// **and** on the screen's own root rectangle, which would otherwise sit in the corner.
    #[test]
    fn character_select_fills_the_window_after_the_logout() {
        let mut a = app_with(Some("1600x900"));
        enter(&mut a, mode::GAME_PLAY);
        enter(&mut a, mode::CHARACTER_MANAGEMENT);

        let shell = a.ui().expect("a shell");
        assert_eq!(
            shell.ui.display(),
            (800, 600),
            "the UI refresh sets the display size to 800x600"
        );
        let root = shell
            .flow
            .current()
            .and_then(|s| s.roots().first().copied())
            .expect("CharacterManagementScreen built a root");
        let b = shell.ui.node(root).expect("live").region.box_;
        assert_eq!(
            (b.x0, b.y0, b.width(), b.height()),
            (0, 0, 800, 600),
            "0x1000039A is mode 0 on all four edges: it fills the window only when the window is \
             800x600"
        );
    }

    /// The other direction, and the negative control for the test above: a profile that names no
    /// resolution is not dragged to the registered 1024x768 default when the gameplay screen
    /// un-forces. The original client would take it; this build keeps the device-initialization
    /// argument, the same rule as the start-up read, so tests may set `Config::width` directly.
    #[test]
    fn a_profile_that_names_no_resolution_is_not_moved_by_entering_the_world() {
        let mut a = app_with(None);
        assert_eq!(a.renderer().size(), (800, 600));
        enter(&mut a, mode::GAME_PLAY);
        assert_eq!(a.renderer().size(), (800, 600));
        enter(&mut a, mode::CHARACTER_MANAGEMENT);
        assert_eq!(a.renderer().size(), (800, 600));
    }

    /// While forced, a resolution write reaches the stored preference and **not** the presentation:
    /// preference loading overwrites presentation width and height with the stored forced dimensions
    /// after decoding the new preference. In retail the resolution drop-down is unreachable outside
    /// gameplay for exactly this reason.
    #[test]
    fn a_resolution_chosen_while_forced_is_stored_and_not_applied() {
        let mut a = app_with(None);
        a.ui_mut()
            .expect("the UI shell is up")
            .ui
            .requests
            .emit(UiRequest::SetPreference(
                "Display.Resolution",
                PrefValue::Int(store::mode_desc(1280, 720)),
            ));
        settle(&mut a);
        assert_eq!(
            a.config().display.resolution,
            u32::from_ne_bytes(store::mode_desc(1280, 720).to_ne_bytes()),
            "the resolution preference is stored even while the presentation is forced"
        );
        assert_eq!(
            a.renderer().size(),
            (800, 600),
            "the active force overrides the newly stored preference"
        );

        // …and it is waiting there for the gameplay screen, which is what makes the store write
        // worth keeping.
        enter(&mut a, mode::GAME_PLAY);
        assert_eq!(a.renderer().size(), (1280, 720));
    }

    /// Forcing a size the device already wears returns early. The test checks that repeated calls
    /// leave the presentation size unchanged.
    #[test]
    fn forcing_a_size_the_device_already_wears_does_nothing() {
        let mut a = app_with(Some("1600x900"));
        enter(&mut a, mode::CHARACTER_MANAGEMENT);
        let before = a.renderer().size();
        a.force_display_resolution(true, 800, 600);
        a.force_display_resolution(true, 800, 600);
        assert_eq!(a.renderer().size(), before);
        // Un-forcing something that was never forced also returns early.
        a.force_display_resolution(false, 800, 600);
        a.force_display_resolution(false, 800, 600);
        assert_eq!(
            a.renderer().size(),
            (1600, 900),
            "the first un-force restored the preference"
        );
    }

    /// The preference loader's own floor remains in force while the override is off:
    /// below 800x600 it answers `false` and the presentation change does nothing.
    #[test]
    fn a_resolution_below_the_floor_still_leaves_the_presentation_alone() {
        let mut a = app_with(Some("1600x900"));
        enter(&mut a, mode::GAME_PLAY);
        a.ui_mut()
            .expect("the UI shell is up")
            .ui
            .requests
            .emit(UiRequest::SetPreference(
                "Display.Resolution",
                PrefValue::Int(store::mode_desc(640, 480)),
            ));
        settle(&mut a);
        assert_eq!(
            a.renderer().size(),
            (1600, 900),
            "a preference below the 800x600 floor leaves the active presentation unchanged"
        );
    }
}
