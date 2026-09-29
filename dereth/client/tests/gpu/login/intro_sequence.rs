//! The intro phase: the logo movie plays, then the splash states, then character management. The
//! splash sequence is the four states the shipped layout authors (`0x21000001`, attribute
//! `0x10000047`); the logo movie is 219 frames of 640x480 at 29.97 fps and does not loop; its
//! frames change pixels only inside the layout's (80, 60) 640x480 field; the empty state list (or
//! the skip message `0x10000001`) queues character-management mode `0x1000000A`; and the
//! descriptor heap stays bounded (at most 64 live, no exhaustion, no unknown release).
//! Fixture: an offline headless `App` on the retail dats, stepped through `App::frame` with element
//! messages broadcast on its own tree, plus `turbine_logo_ac.avi` from the retail install decoded directly.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use crate::common::client_dir;
use crate::common::gpu_lock;

use dereth_client::app::App;
use dereth_client::config::Config;
use dereth_ui::framework::mode;
use dereth_ui_screens::screens::intro::{self, IntroScreen};

/// Fails the test when the retail dats are not where `$DERETH_TEST_DAT_DIR` says.
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

/// Bring an offline `App` up and step it until the intro screen is the current mode.
///
/// Without an active network link, the data-patch screen has no connection to wait for. The expected flow is DataPatch → Intro in two frames; this helper tries up to three.
/// A missing device or shell, or a flow that never reaches the intro, fails the test.
fn app_on_intro() -> Option<App> {
    let mut app = App::new(base_config())
        .unwrap_or_else(|e| panic!("a headless App on the software device: {e}"));
    app.start_shell()
        .unwrap_or_else(|e| panic!("the shell starts over the retail dats: {e}"));
    app.load_first_pixel_scene()
        .unwrap_or_else(|e| panic!("the first-pixel scene loads: {e}"));
    for _ in 0..3 {
        app.frame();
        if app.ui().and_then(|u| u.flow.current_mode()) == Some(mode::INTRO) {
            return Some(app);
        }
    }
    panic!(
        "the flow did not reach the intro ({:?})",
        app.ui().and_then(|u| u.flow.current_mode())
    );
}

/// The concrete screen, through the same downcast the shell uses.
fn screen(app: &mut App) -> &mut IntroScreen {
    let shell = app.ui_mut().expect("the shell is up");
    let s = shell.flow.current_mut().expect("a screen");
    let any: &mut dyn std::any::Any = &mut **s;
    any.downcast_mut::<IntroScreen>().expect("the intro screen")
}

// ---------------------------------------------------------------------------------------------
// 1. The state list
// ---------------------------------------------------------------------------------------------

/// Oracle: the shipped `intro` layout, `client_local_English.dat` object `0x21000001`, read through
/// the live element tree whose intro-mode root is element `0x10000419` (mode `0x10000002`).
///
/// The intro screen reads attribute `0x10000047` into its state list:
/// the attribute is an `Array` of `0x10000048`
/// `Enum`s and it holds four state ids, and every one of those four is a state the same element
/// declares.
#[test]
fn the_intro_state_list_is_the_four_states_the_shipped_layout_authors() {
    let _gpu = gpu_lock();
    have_dats();
    let mut app = app_on_intro().expect("an app on the intro screen");

    let root = screen(&mut app).root().expect("the intro root exists");
    let states = {
        let ui = &app.ui().expect("shell").ui;
        assert_eq!(
            ui.node(root)
                .map(dereth_ui::element::ElementNode::element_id),
            Some(dereth_ui::ElementId(0x1000_0419)),
            "the intro field is element 0x10000419"
        );
        IntroScreen::read_state_list(ui, root)
    };
    assert_eq!(
        states,
        intro::SHIPPED_STATES.to_vec(),
        "attribute 0x10000047 is the intro state list"
    );

    // The constructor pops the head immediately, so by now the screen is showing state[0] and the
    // rest are still queued.
    let s = screen(&mut app);
    assert!(
        s.had_state_list,
        "the list was found, so the intro is not one frame long"
    );
    assert_eq!(s.current_state, Some(intro::SHIPPED_STATES[0]));
    assert_eq!(
        s.states.iter().copied().collect::<Vec<u32>>(),
        intro::SHIPPED_STATES[1..].to_vec()
    );
}

// ---------------------------------------------------------------------------------------------
// 2. The Turbine logo movie
// ---------------------------------------------------------------------------------------------

/// Behaviour: intro.movie.the-logo-movie-is-two-hundred-and-nineteen-frames-and-stops
///
/// Oracle: `turbine_logo_ac.avi` from the retail install's own `avih` and `strf` headers, and the decoder walked to
/// the end.
///
/// The AVI headers declare 640 x 480, `dwMicroSecPerFrame` 33367 (29.970 fps), and
/// `dwTotalFrames` 219, i.e. 7.31 s. In movie playback, flag bit 0 selects looping;
/// movie-record updates pass **flags = 0**, so this path does not loop.
#[test]
fn the_logo_movie_is_two_hundred_and_nineteen_distinct_frames_and_stops() {
    let path = client_dir().join("turbine_logo_ac.avi");
    let mut movie = dereth_audio::video::Movie::open(&path).unwrap_or_else(|| {
        panic!(
            "the retail install's logo movie is this test's oracle and does not open at {}",
            path.display()
        )
    });
    let info = movie.info();
    assert_eq!((info.width, info.height), (640, 480));
    assert_eq!(info.total_frames, 219);
    assert_eq!(info.micro_sec_per_frame, 33367);
    assert!((info.fps() - 29.970).abs() < 0.001, "{}", info.fps());
    assert_eq!(
        movie.frame_count(),
        219,
        "the movi list holds what the header claims"
    );

    // Every frame must decode, and more than 150 of 218 adjacent transitions must differ.
    // A decoder returning the same buffer 219 times would satisfy every count above.
    let mut previous: Option<Vec<u8>> = None;
    let mut changed = 0usize;
    for i in 0..219 {
        let frame = movie.decode_frame(i).expect("frame decodes").to_vec();
        assert_eq!(frame.len(), 640 * 480 * 4);
        // The A8R8G8B8 sample path forces alpha opaque.
        assert!(
            frame.iter().skip(3).step_by(4).all(|a| *a == 0xFF),
            "frame {i} has a transparent pixel"
        );
        if let Some(p) = &previous {
            if *p != frame {
                changed += 1;
            }
        }
        previous = Some(frame);
    }
    assert!(
        changed > 150,
        "only {changed} of 218 frame transitions changed a pixel"
    );

    // …and it stops rather than looping: 219 frames is the end, and nothing comes after it.
    assert!(movie.is_finished());
    assert!(movie.decode_frame(219).is_none(), "there is no frame 219");
    assert_eq!(movie.frame_count(), 219, "and no wrap back to the start");
}

// ---------------------------------------------------------------------------------------------
// 3. The movie's pixels reach the screen
// ---------------------------------------------------------------------------------------------

/// Oracle: early and later captures of the same screen in one run, compared pixel by pixel. The
/// intended changing input is **which movie frame is current**.
///
/// The shipped `intro` layout puts the movie's element, `0x10000434`, at (80, 60) 640 x 480 inside
/// the 800 x 600 root. So every pixel that a later movie frame changes must lie inside that
/// rectangle, and none outside it.
#[test]
fn the_movie_frames_change_pixels_only_inside_the_field_the_layout_authors() {
    let _gpu = gpu_lock();
    have_dats();
    let movie = client_dir().join("turbine_logo_ac.avi");
    assert!(
        movie.exists(),
        "the retail install's logo movie is this test's subject and is absent at {}",
        movie.display()
    );
    let mut app = app_on_intro().expect("an app on the intro screen");

    // Frame 1 of the movie: the app has stepped once into the intro, so the first movie frame is
    // already uploaded. Draw it.
    app.frame();
    let (w, h, early) = app.renderer_mut().capture_bgra().expect("a capture");
    assert_eq!((w, h), (800, 600));

    // Run on until the movie is well into its middle, then capture again. Nothing else on the
    // screen can have changed: the intro's other two children are still in the same state, and this
    // build draws no world.
    let mut frames = 0u64;
    for _ in 0..90 {
        app.frame();
        frames += 1;
        if app.ui().is_some_and(|u| u.movie_stats.frames >= 60) {
            break;
        }
    }
    let stats = app.ui().expect("shell").movie_stats;
    assert!(stats.started >= 1, "no movie record step was reached");
    assert!(
        stats.frames >= 60,
        "only {} movie frames decoded in {frames}",
        stats.frames
    );
    let (_, _, later) = app.renderer_mut().capture_bgra().expect("a capture");

    // The layout's own rectangle, in the 800 x 600 back buffer.
    const FIELD: (i32, i32, i32, i32) = (80, 60, 80 + 640 - 1, 60 + 480 - 1);
    let mut inside = 0usize;
    let mut outside = 0usize;
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let i = (y as usize * w as usize + x as usize) * 4;
            if early[i..i + 4] != later[i..i + 4] {
                if x >= FIELD.0 && x <= FIELD.2 && y >= FIELD.1 && y <= FIELD.3 {
                    inside += 1;
                } else {
                    outside += 1;
                }
            }
        }
    }
    assert_eq!(
        outside, 0,
        "{outside} pixels changed outside the movie's own 640x480 field"
    );
    assert!(
        inside > 10_000,
        "only {inside} pixels changed inside it -- is the movie drawing?"
    );

    // Bound the aggregate descriptor usage; this does not isolate an exact per-movie count.
    let usage = app.renderer().descriptor_usage();
    let d = app.renderer().descriptor_stats();
    eprintln!("intro descriptors: {usage:?} {d:?}");
    assert_eq!(d.exhaustions, 0, "the movie exhausted the descriptor heap");
    assert!(
        usage.live <= 64,
        "a 219-frame movie should hold one slot, not {} ({usage:?})",
        usage.live
    );
    assert_eq!(
        app.ui_release_report().unknown,
        0,
        "a slot was released twice"
    );
}

// ---------------------------------------------------------------------------------------------
// 4. The sequence ends on character management
// ---------------------------------------------------------------------------------------------

/// The skip trigger is element message `0x10000001`, which queues character-management
/// mode `0x1000000A`. Broadcast it through the real tree, as the layout's skip hotspot does.
#[test]
fn the_layouts_skip_message_leaves_the_intro_for_character_management() {
    let _gpu = gpu_lock();
    have_dats();
    let mut app = app_on_intro().expect("an app on the intro screen");
    let root = screen(&mut app).root().expect("root");
    {
        let shell = app.ui_mut().expect("shell");
        shell
            .ui
            .broadcast_element_message(root, intro::MSG_SKIP, 0, 0);
    }
    // One frame to deliver the message, one for the mode transition to build the next screen.
    for _ in 0..2 {
        app.frame();
    }
    assert_eq!(
        app.ui().and_then(|u| u.flow.current_mode()),
        Some(mode::CHARACTER_MANAGEMENT),
        "the skip message queues 0x1000000A"
    );
    // Destroying the movie record releases its playback and stops it; leaving the
    // screen must therefore leave no movie playing here either.
    assert!(!app.ui().expect("shell").movie_playing());
}

/// Behaviour: intro.click.clicking-through-the-sequence-ends-at-character-select-and-not-before
///
/// State advancement takes one state at a time, and the empty list queues character
/// management — run through the **whole** shipped sequence with no skip at all.
///
/// This is the acceptance line: "the logo plays, then the intro screen". The four states are the
/// movie, two splash images and `AC-ThroneOfDestiny.avi`, which is not in this install and is
/// passed over silently, as movie initialization treats an absent file (`VFW_E_NOT_FOUND`).
#[test]
fn the_whole_shipped_sequence_plays_and_lands_on_character_management() {
    let _gpu = gpu_lock();
    have_dats();
    let mut app = app_on_intro().expect("an app on the intro screen");

    // 1/30 s per headless frame, and the sequence is the movie's 7.31 s plus two 4.1 s splashes:
    // about 460 frames. 900 is a generous bound that still fails rather than hangs.
    let mut seen: Vec<u32> = Vec::new();
    let mut frames = 0u64;
    for _ in 0..900 {
        if app.ui().and_then(|u| u.flow.current_mode()) != Some(mode::INTRO) {
            break;
        }
        if let Some(s) = app
            .ui_mut()
            .and_then(|sh| sh.flow.current_mut())
            .and_then(|s| {
                let any: &mut dyn std::any::Any = &mut **s;
                any.downcast_mut::<IntroScreen>()
            })
            .and_then(|s| s.current_state)
        {
            if seen.last() != Some(&s) {
                seen.push(s);
            }
        }
        app.frame();
        frames += 1;
    }
    eprintln!("intro: {frames} frames, states {seen:02X?}");
    assert_eq!(
        seen,
        intro::SHIPPED_STATES.to_vec(),
        "every authored state was shown, in order"
    );
    assert_eq!(
        app.ui().and_then(|u| u.flow.current_mode()),
        Some(mode::CHARACTER_MANAGEMENT),
        "the intro ends on 0x1000000A"
    );

    let stats = app.ui().expect("shell").movie_stats;
    eprintln!("movie: {stats:?}");
    assert_eq!(
        stats.started, 2,
        "two movie record steps: the logo and Throne of Destiny"
    );
    assert_eq!(stats.completed, 1, "the logo played to EC_COMPLETE");
    assert_eq!(
        stats.skipped, 1,
        "AC-ThroneOfDestiny.avi is not in this install"
    );
    assert_eq!(
        stats.frames, 219,
        "every one of the movie's frames was decoded and uploaded"
    );

    // The movie ran for its own duration and nothing shortened it: 219 frames at 33367 us is
    // 7.3074 s, which is 220 headless frames of 1/30 s. The two splashes are 0.1 + 0.75 + 2.75 +
    // 0.5 = 4.1 s each, i.e. 123 frames each.
    assert!(
        frames > 400,
        "the sequence was only {frames} frames -- did the pauses run?"
    );

    let d = app.renderer().descriptor_stats();
    eprintln!("intro descriptors after the whole sequence: {d:?}");
    assert_eq!(d.exhaustions, 0);
    assert_eq!(
        app.ui_release_report().unknown,
        0,
        "a slot was released twice"
    );
}
