//! The char-gen appearance page's sex and zoom controls. Choosing the other sex changes the preview
//! body (a setup computed from the appearance state, never the heritage's shared fallback
//! `0x02000054`) and repaints the viewport, and the summary page previews the same body. Exactly one
//! zoom half is lit: the pressed half becomes TOGGLED(6) and the other NORMAL(1), `+` is lit at rest
//! because selecting the Face tab zooms in, and pressing the active half re-latches it without
//! moving the camera. The zoom cameras are fixed per heritage.
//! Fixture: the retail dats and an offline headless App at 800x600 on the wizard's appearance page;
//! pixel differentials are box-scoped and calibrated by two idle frames agreeing inside the box.

#![cfg(gpu)]

use crate::common::client_dir;
use crate::common::gpu_lock;

use dereth_client::app::App;
use dereth_client_runtime::config::Config;
use dereth_ui::framework::mode;
use dereth_ui::{ElemHandle, ElementId, StateId};
use dereth_ui_screens::screens::chargen::{self, appearance, CharGenScreen, EcgProgress};

/// Every fixture path is an `expect`, never a skip.
fn wizard_on_appearance() -> App {
    let d = client_dir();
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats are required at {} -- set DERETH_TEST_DAT_DIR",
        d.display()
    );
    let cfg = Config {
        headless: true,
        sound: false,
        ui: true,
        width: 800,
        height: 600,
        dat_dir: d,
        ..Config::default()
    };
    let mut app = App::new(cfg).expect("the application comes up");
    app.start_shell().expect("the UI comes up");
    app.queue_ui_mode(mode::CHAR_GEN);
    for _ in 0..8 {
        app.frame();
    }
    // The preview requires a chosen heritage and gender. Select a heritage before measuring
    // appearance; current setup resolution also provides an unchosen-state fallback.
    click(&mut app, chargen::HERITAGE_BUTTONS[0].0);
    for _ in 0..2 {
        app.frame();
    }
    click(
        &mut app,
        EcgProgress::Appearance
            .select_button()
            .expect("the appearance tab"),
    );
    for _ in 0..4 {
        app.frame();
    }
    app
}

fn root_of(app: &App) -> ElemHandle {
    app.ui()
        .expect("shell")
        .flow
        .current()
        .expect("a screen")
        .roots()[0]
}

fn find(app: &App, id: ElementId) -> ElemHandle {
    app.ui()
        .expect("shell")
        .ui
        .get_child_recursive(root_of(app), id)
        .unwrap_or_else(|| panic!("{id:?} is in the shipped char-gen layout"))
}

fn click(app: &mut App, id: ElementId) {
    let h = find(app, id);
    let shell = app.ui_mut().expect("shell");
    shell
        .ui
        .broadcast_element_message(h, dereth_ui::msg::element::id::BUTTON_CLICKED, 7, 0);
    app.frame();
}

fn wizard(app: &mut App) -> &mut CharGenScreen {
    let shell = app.ui_mut().expect("shell");
    let s = shell.flow.current_mut().expect("a screen is up");
    let any: &mut dyn std::any::Any = &mut **s;
    any.downcast_mut::<CharGenScreen>().expect("the wizard")
}

fn state_of(app: &App, id: ElementId) -> StateId {
    let h = find(app, id);
    app.ui()
        .expect("shell")
        .ui
        .node(h)
        .expect("a live node")
        .state
}

/// The screen rectangle an element occupies, for confining a differential to it.
#[derive(Clone, Copy, Debug)]
struct Rect {
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
}

fn rect_of(app: &mut App, id: ElementId) -> Rect {
    let h = find(app, id);
    let b = app.ui_mut().expect("shell").ui.screen_clip_box(h);
    assert!(b.is_valid(), "{id:?} has no screen box");
    Rect {
        x0: b.x0,
        y0: b.y0,
        x1: b.x1,
        y1: b.y1,
    }
}

impl Rect {
    fn union(self, o: Self) -> Self {
        Self {
            x0: self.x0.min(o.x0),
            y0: self.y0.min(o.y0),
            x1: self.x1.max(o.x1),
            y1: self.y1.max(o.y1),
        }
    }

    fn holds(self, x: i32, y: i32) -> bool {
        x >= self.x0 && x <= self.x1 && y >= self.y0 && y <= self.y1
    }
}

fn frame(app: &mut App) -> (u32, u32, Vec<u8>) {
    app.frame();
    app.renderer_mut()
        .capture_bgra()
        .expect("the frame captures")
}

/// Require two idle frames to agree inside the measurement rectangle before using the first
/// as a baseline. Scope this to the requested box: zooming out starts model animation, while
/// zooming in stops it, so whole-frame equality is inappropriate for the button-only control.
fn calibrated_frame(app: &mut App, area: Rect) -> (u32, u32, Vec<u8>) {
    let a = frame(app);
    let b = frame(app);
    assert_eq!(
        changed_in(&a, &b, area),
        0,
        "two idle frames differ inside the measured box, so nothing here is attributable"
    );
    a
}

/// Changed pixels between two frames, and how many of them fell outside `area`.
fn differential(a: &(u32, u32, Vec<u8>), b: &(u32, u32, Vec<u8>), area: Rect) -> (u32, u32) {
    assert_eq!((a.0, a.1), (b.0, b.1));
    let (w, h) = (a.0, a.1);
    let (mut changed, mut outside) = (0u32, 0u32);
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            if a.2[i..i + 3] != b.2[i..i + 3] {
                changed += 1;
                if !area.holds(x as i32, y as i32) {
                    outside += 1;
                }
            }
        }
    }
    (changed, outside)
}

/// Changed pixels **inside** `area` only.
fn changed_in(a: &(u32, u32, Vec<u8>), b: &(u32, u32, Vec<u8>), area: Rect) -> u32 {
    assert_eq!((a.0, a.1), (b.0, b.1));
    let (w, h) = (a.0, a.1);
    let mut changed = 0u32;
    for y in 0..h {
        for x in 0..w {
            if !area.holds(x as i32, y as i32) {
                continue;
            }
            let i = ((y * w + x) * 4) as usize;
            if a.2[i..i + 3] != b.2[i..i + 3] {
                changed += 1;
            }
        }
    }
    changed
}

// =================================================================================================
// 1. Sex
// =================================================================================================

/// Behaviour: chargen.appearance.choosing-the-other-sex-changes-the-body
///
/// Changing sex must alter both preview setup and viewport pixels on the Appearance page.
/// A correct state value with a preview that ignores it would satisfy only the first check.
///
/// Using the heritage's shared setup in `refresh_view`, instead of `get_setup_id`, makes both
/// bodies 0x02000054 and removes the differential.
#[test]
fn choosing_the_other_sex_changes_the_body_and_repaints_the_viewport() {
    let _gpu = gpu_lock();
    let mut app = wizard_on_appearance();
    // The declared rectangle is the whole Appearance page. Changing gender constrains the
    // available feature and clothing choices, so rows, nine colour spots and shade disk can
    // change too. The viewport's own contribution is measured separately.
    let page = rect_of(
        &mut app,
        EcgProgress::Appearance.page().expect("the page").0,
    );
    let viewport = rect_of(&mut app, appearance::VIEWPORT);

    // Start from a known sex rather than from the roll, so the click is a real change either way.
    click(&mut app, appearance::GENDER_MALE);
    for _ in 0..3 {
        app.frame();
    }
    assert_eq!(
        wizard(&mut app).state.gender,
        1,
        "0x100003A8 selects gender 1 -- male"
    );
    let male_setup = wizard(&mut app).view3d.setup;
    let male_frame = calibrated_frame(&mut app, page);

    click(&mut app, appearance::GENDER_FEMALE);
    for _ in 0..3 {
        app.frame();
    }
    assert_eq!(
        wizard(&mut app).state.gender,
        2,
        "0x100003A7 selects gender 2 -- female"
    );
    let female_setup = wizard(&mut app).view3d.setup;
    let female_frame = frame(&mut app);

    // Computed Aluvian setup ids are 0x02000001 for male and 0x0200004E for female. These checks
    // require distinct results and reject the shared 0x02000054 fallback; they do not assert
    // the two exact computed ids here.
    assert_ne!(
        male_setup, female_setup,
        "the two sexes preview different setups"
    );
    assert_ne!(
        male_setup,
        chargen::HUMAN_SETUP_ID,
        "and neither is the shared heritage setup, the 0x02000054 every heritage shares"
    );
    assert_ne!(female_setup, chargen::HUMAN_SETUP_ID);

    let (changed, outside) = differential(&male_frame, &female_frame, page);
    let in_viewport = changed_in(&male_frame, &female_frame, viewport);
    eprintln!(
        "appearance: sex moves {changed} px ({in_viewport} of them inside the viewport), \
         {outside} outside the appearance page; setups {male_setup:?} -> {female_setup:?}"
    );
    assert!(
        changed > 100,
        "the sex change repainted only {changed} pixels"
    );
    assert!(
        in_viewport > 100,
        "only {in_viewport} of {changed} pixels were the model -- the body did not follow the sex"
    );
    assert_eq!(
        outside, 0,
        "{outside} of {changed} changed pixels fell outside the appearance page"
    );
    app.shutdown();
}

/// The **summary** page and the **appearance** page agree about the body.
///
/// Falsified by giving the summary page its own setup source.
#[test]
fn the_summary_page_previews_the_same_body_the_appearance_page_does() {
    let _gpu = gpu_lock();
    let mut app = wizard_on_appearance();
    click(&mut app, appearance::GENDER_FEMALE);
    for _ in 0..3 {
        app.frame();
    }
    let on_appearance = wizard(&mut app).view3d.setup;

    click(
        &mut app,
        EcgProgress::Summary
            .select_button()
            .expect("the summary tab"),
    );
    for _ in 0..4 {
        app.frame();
    }
    assert_eq!(wizard(&mut app).progress, EcgProgress::Summary);
    assert_eq!(
        wizard(&mut app).view3d.setup,
        on_appearance,
        "the two pages are views over one CharGenState"
    );
    app.shutdown();
}

// =================================================================================================
// 2. Zoom
// =================================================================================================

/// Behaviour: chargen.appearance.the-zoom-halves-move-the-camera-and-exactly-one-is-lit
///
/// One zoom half is lit, initially '+' on Face. Selecting the other half must clear the first;
/// relighting only the pressed half would leave both latched after a transition. Removing the
/// button-state update, lighting only the pressed half, or swapping TOGGLED/NORMAL each fails
/// this test.
#[test]
fn exactly_one_zoom_half_is_lit_and_at_rest_on_the_face_tab_it_is_the_plus() {
    use dereth_ui::widgets::button::state::{NORMAL, TOGGLED};
    let _gpu = gpu_lock();
    let mut app = wizard_on_appearance();

    // Face-tab initialization zooms in.
    assert!(
        wizard(&mut app).face_tab,
        "the appearance page opens on the Face tab"
    );
    assert!(
        wizard(&mut app).view3d.zoomed_in,
        "selecting the Face tab zooms in"
    );
    assert_eq!(
        state_of(&app, appearance::ZOOM_IN),
        TOGGLED,
        "+ is lit at rest"
    );
    assert_eq!(state_of(&app, appearance::ZOOM_OUT), NORMAL, "and - dark");

    // `-`: the pair swaps.
    click(&mut app, appearance::ZOOM_OUT);
    app.frame();
    assert!(!wizard(&mut app).view3d.zoomed_in);
    assert_eq!(
        state_of(&app, appearance::ZOOM_IN),
        NORMAL,
        "zooming out puts + back to NORMAL"
    );
    assert_eq!(state_of(&app, appearance::ZOOM_OUT), TOGGLED);

    // `+`: and back, which is the second half of "never clears".
    click(&mut app, appearance::ZOOM_IN);
    app.frame();
    assert!(wizard(&mut app).view3d.zoomed_in);
    assert_eq!(state_of(&app, appearance::ZOOM_IN), TOGGLED);
    assert_eq!(
        state_of(&app, appearance::ZOOM_OUT),
        NORMAL,
        "the other half is dark"
    );

    // Pressing the active half re-latches the active half without moving the camera.
    let camera = wizard(&mut app).view3d.camera_position;
    click(&mut app, appearance::ZOOM_IN);
    app.frame();
    assert_eq!(
        state_of(&app, appearance::ZOOM_IN),
        TOGGLED,
        "still latched"
    );
    assert_eq!(
        state_of(&app, appearance::ZOOM_OUT),
        NORMAL,
        "and the other half still dark"
    );
    assert_eq!(
        wizard(&mut app).view3d.camera_position,
        camera,
        "and the camera did not move"
    );
    app.shutdown();
}

/// Compare camera values and viewport pixels for the fixture's selected heritage: the preview
/// uses the camera values. Button pixels cannot satisfy the separate viewport-positive check.
///
/// This path uses the camera functions as expectations. The fixed literals below independently
/// catch a shared-function mutation that makes zoomed-out coordinates equal zoomed-in ones.
#[test]
fn the_two_halves_move_the_camera_to_their_own_targets_and_repaint_the_model() {
    let _gpu = gpu_lock();
    let mut app = wizard_on_appearance();
    // The viewport **and** the zoom control: the press repaints the model and re-latches the
    // button, and both are the press's own doing.
    let viewport = rect_of(&mut app, appearance::VIEWPORT);
    let area = viewport
        .union(rect_of(&mut app, appearance::ZOOM_IN))
        .union(rect_of(&mut app, appearance::ZOOM_OUT));
    let heritage = wizard(&mut app).state.heritage_group;

    assert_eq!(
        wizard(&mut app).view3d.camera_position,
        chargen::zoomed_in_camera(heritage),
        "at rest the Face tab holds the zoomed-in target"
    );
    let zoomed_in = calibrated_frame(&mut app, area);

    click(&mut app, appearance::ZOOM_OUT);
    for _ in 0..3 {
        app.frame();
    }
    assert_eq!(
        wizard(&mut app).view3d.camera_position,
        chargen::zoomed_out_camera(heritage),
        "the zoomed-out target, which differs from the zoomed-in target on every heritage"
    );
    let zoomed_out = frame(&mut app);

    let (changed, outside) = differential(&zoomed_in, &zoomed_out, area);
    let in_viewport = changed_in(&zoomed_in, &zoomed_out, viewport);
    eprintln!(
        "appearance: the zoom moves {changed} px ({in_viewport} inside the viewport), \
         {outside} outside the viewport and the zoom control together"
    );
    assert!(
        in_viewport > 100,
        "the zoom repainted only {in_viewport} pixels of the model"
    );
    assert_eq!(
        outside, 0,
        "{outside} of {changed} changed pixels fell outside the declared box"
    );
    app.shutdown();
}

/// Check the buttons' own pixel rectangle separately from the moving model. A '+'-to-'-'
/// transition must change pixels in that rectangle; pressing '-' again must change none.
/// Both initial states receive idle-pair calibration. The model may animate outside the box.
///
/// Removing `set_zoom_button_states` makes the positive differential zero. The positive
/// transition and unchanged re-press control distinguish the two button behaviors.
#[test]
fn the_zoom_buttons_own_rectangle_repaints_when_the_state_changes() {
    let _gpu = gpu_lock();
    let mut app = wizard_on_appearance();
    let both =
        rect_of(&mut app, appearance::ZOOM_IN).union(rect_of(&mut app, appearance::ZOOM_OUT));

    let lit_plus = calibrated_frame(&mut app, both);
    click(&mut app, appearance::ZOOM_OUT);
    for _ in 0..6 {
        app.frame();
    }
    let lit_minus = calibrated_frame(&mut app, both);

    let changed = changed_in(&lit_plus, &lit_minus, both);
    eprintln!("appearance: the zoom control's own box moves {changed} px when the lit half moves");
    assert!(
        changed > 20,
        "the zoom control's own rectangle changed only {changed} pixels when the lit half moved"
    );

    // **The other direction of the same instrument.** Press the half that is already active: the
    // client re-latches it and nothing else happens, so this box must now move **0** pixels — and
    // the {changed} above is what makes that zero worth reading.
    click(&mut app, appearance::ZOOM_OUT);
    for _ in 0..6 {
        app.frame();
    }
    let still = frame(&mut app);
    assert_eq!(
        changed_in(&lit_minus, &still, both),
        0,
        "a re-press of the already-active half repainted the control"
    );
    app.shutdown();
}

/// Fixed camera literals provide an independent expectation for the sampled heritage branches.
/// The integration above compares against the same camera functions used by the preview, so
/// changing a shared function can move its expected result too; the explicit arrays below do not.
///
/// The camera constant 1.65 is the float with bits `0x3FD33333`; the acid-Olthoi zoom-out case
/// uses the same value.
#[test]
fn the_two_zoom_cameras_are_the_literals_the_two_functions_push() {
    // Zoomed-in branches: 0x0C, 0x0D and 7 differ; the other ten heritages share the default.
    assert_eq!(chargen::zoomed_in_camera(1), [0.0, -0.55, 1.65]);
    assert_eq!(
        chargen::zoomed_in_camera(6),
        [0.0, -0.55, 1.65],
        "a Gear Knight is not special here"
    );
    assert_eq!(
        chargen::zoomed_in_camera(7),
        [0.0, -0.85, 1.65],
        "Tumerok, the one exception"
    );
    assert_eq!(chargen::zoomed_in_camera(0x0C), [0.0, -1.85, 1.85]);
    assert_eq!(chargen::zoomed_in_camera(0x0D), [0.0, -3.05, 2.75]);
    // Only the two Olthoi branches differ when zoomed out; heritage 7 uses the default.
    assert_eq!(chargen::zoomed_out_camera(1), [0.0, -2.5, 0.95]);
    assert_eq!(
        chargen::zoomed_out_camera(7),
        [0.0, -2.5, 0.95],
        "7 has no zoom-out case"
    );
    assert_eq!(chargen::zoomed_out_camera(0x0C), [0.0, -3.8, 1.15]);
    assert_eq!(chargen::zoomed_out_camera(0x0D), [0.0, -5.7, 1.65]);
    // The five sampled heritages have distinct zoomed-in and zoomed-out targets.
    for h in [1_u32, 6, 7, 0x0C, 0x0D] {
        assert_ne!(
            chargen::zoomed_in_camera(h),
            chargen::zoomed_out_camera(h),
            "heritage {h}"
        );
    }
}
