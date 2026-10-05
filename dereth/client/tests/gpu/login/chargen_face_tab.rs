//! Entering the char-gen appearance page selects Face and leaves the head without headgear. Page
//! initialization chooses Face once, after camera setup and before its first update; choosing Face
//! parks the selected headgear style and stores -1, and preview dressing skips the headgear
//! clothing call at -1 (hair is applied independently, so the head shows its hair). Clothes restores
//! the parked style, Face parks it again, and a character created without visiting Clothes carries
//! headgear style -1 through the wire-structure adapter.
//! Fixture: the retail dats and an offline headless App at 800x600 on the wizard; tab clicks are
//! element messages delivered by `App::frame`, and creation calls `do_finish` and reads its queue.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use crate::common::client_dir;

use dereth_client::app::App;
use dereth_client_runtime::config::Config;
use dereth_client_shell::gpu::PreviewId;
use dereth_primitives::DataId;
use dereth_ui::framework::mode;
use dereth_ui::ElementId;
use dereth_ui_screens::screens::chargen::{appearance, CharGenAction, CharGenScreen, EcgProgress};

/// Inject a BUTTON_CLICKED element message with parameters (7, 0); this is not a raw mouse event.
fn click(app: &mut App, id: u32) {
    let shell = app.ui_mut().expect("the shell is up");
    let root = shell.flow.current().expect("a screen is up").roots()[0];
    let h = shell
        .ui
        .get_child_recursive(root, ElementId(id))
        .unwrap_or_else(|| panic!("element {id:#010X} is in the shipped layout"));
    shell
        .ui
        .broadcast_element_message(h, dereth_ui::msg::element::id::BUTTON_CLICKED, 7, 0);
}

fn wizard(app: &mut App) -> &mut CharGenScreen {
    let shell = app.ui_mut().expect("the shell is up");
    let screen = shell.flow.current_mut().expect("a screen is up");
    let any: &mut dyn std::any::Any = &mut **screen;
    any.downcast_mut::<CharGenScreen>()
        .expect("the wizard is current")
}

/// The same harness `login/chargen_dressing.rs` uses: a headless app driven to one wizard page.
fn wizard_on(page: EcgProgress) -> App {
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
    click(
        &mut app,
        dereth_ui_screens::screens::chargen::HERITAGE_BUTTONS[0]
            .0
             .0,
    );
    for _ in 0..2 {
        app.frame();
    }
    click(
        &mut app,
        dereth_ui_screens::screens::chargen::TOWN_BUTTONS[0].0 .0,
    );
    for _ in 0..2 {
        app.frame();
    }
    click(
        &mut app,
        page.select_button().expect("a real page has a tab").0,
    );
    for _ in 0..4 {
        app.frame();
    }
    app
}

/// Graphics-object IDs recorded when each model mesh was baked. Pixel coverage is checked separately.
fn baked(app: &mut App) -> Vec<DataId> {
    app.renderer_mut()
        .preview(PreviewId::CharGen)
        .expect("the preview space exists")
        .object(0)
        .expect("the model is object 0")
        .built_from()
        .to_vec()
}

// =================================================================================================
// 1. Arrival
// =================================================================================================

/// On initial Face selection, three of four clothing-description calls succeed and headgear is
/// omitted. Four descriptor merges and three subpalettes remain required: absence of a hat alone
/// could also result from skipping the whole dressing tail. These counters distinguish that
/// failure from declining only the headgear arm.
#[test]
fn the_wizard_arrives_on_the_face_tab_with_the_head_bare() {
    let mut app = wizard_on(EcgProgress::Appearance);

    let w = wizard(&mut app);
    assert!(w.face_tab, "the page opens on Face");
    assert_eq!(w.state.headgear_style, -1, "and the Face arm stored -1");
    let parked = w.hold_headgear;
    assert!(
        parked >= 0,
        "hold_headgear holds the randomized hat, not another -1: {parked}"
    );

    let s = app.chargen_dress();
    assert_eq!(
        s.merges, 4,
        "the four ObjDesc merges still run -- the tail was not skipped"
    );
    assert_eq!(s.subpalettes, 3, "and all three subpalettes");
    assert_eq!(
        s.clothing_calls, 3,
        "trousers, shirt and footwear; the headgear arm is declined"
    );
    assert_eq!(s.clothing_ok, 3);
    assert_eq!(s.clothing_base_missing, 0);
    app.shutdown();
}

// =================================================================================================
// 2. Switching tabs restores and removes headgear in the baked model.
// =================================================================================================

/// Behaviour: chargen.appearance.the-face-tab-takes-the-hat-off-and-the-clothes-tab-puts-it-back
///
/// Clothes restores the fourth clothing call and changes the baked graphics-object IDs; Face
/// restores the original bare IDs exactly. This checks model assembly beyond wizard state.
/// The separate pixel test checks rendering impact.
#[test]
fn the_clothes_tab_puts_the_headgear_call_back_and_the_face_tab_takes_it_away() {
    let mut app = wizard_on(EcgProgress::Appearance);
    let parked = wizard(&mut app).hold_headgear;

    let bare_calls = app.chargen_dress().clothing_calls;
    let bare_geometry = baked(&mut app);
    assert_eq!(bare_calls, 3);

    // Restore the parked style only when -2 < parked style < headgear choice count.
    // The lower bound deliberately permits -1, the no-headgear style.
    click(&mut app, appearance::TAB_CLOTHES.0);
    for _ in 0..4 {
        app.frame();
    }
    assert!(!wizard(&mut app).face_tab, "the Clothes tab is selected");
    assert_eq!(
        wizard(&mut app).state.headgear_style,
        parked,
        "the parked hat came back"
    );
    let dressed = app.chargen_dress();
    assert_eq!(
        dressed.clothing_calls, 4,
        "all four clothing-description calls now run"
    );
    assert_eq!(dressed.clothing_ok, 4);
    let dressed_geometry = baked(&mut app);
    assert_ne!(
        dressed_geometry, bare_geometry,
        "the hat is a real part swap: the drawn GfxObj list must move, or the descriptor never \
         reached the body"
    );

    // Back to Face: parked again, and the geometry returns to exactly what it was.
    click(&mut app, appearance::TAB_FACE.0);
    for _ in 0..4 {
        app.frame();
    }
    assert_eq!(wizard(&mut app).state.headgear_style, -1, "bare again");
    assert_eq!(
        wizard(&mut app).hold_headgear,
        parked,
        "and re-parked at the same style"
    );
    assert_eq!(app.chargen_dress().clothing_calls, 3);
    assert_eq!(
        baked(&mut app),
        bare_geometry,
        "the same bare head as on arrival"
    );
    app.shutdown();
}

// =================================================================================================
// 3. The wire
// =================================================================================================

/// Behaviour: chargen.finish.a-character-never-shown-the-clothes-tab-goes-out-bare-headed
///
/// Without a Clothes visit, creation queues headgear style -1; visiting Clothes restores the
/// parked style. This inspects the queued result and its wire-structure adapter, not an observed
/// network transmission.
///
/// The public ACE server treats HeadgearStyle == uint.MaxValue as no headgear. Signed -1
/// carries that value on the wire, matching a deliberate no-hat choice on Clothes. This is
/// why restoring a parked style permits -1 with a strict -2 lower bound.
#[test]
fn a_character_created_without_opening_the_clothes_tab_goes_out_bare_headed() {
    // Use independent wizard runs. After one accepted finish, verification is pending and
    // a second finish on the same wizard is refused; it would not create an independent
    // result and could leave the test reading an earlier queued action.
    let (bare, parked_bare) = create_character(false);
    let (dressed, parked_dressed) = create_character(true);

    assert!(
        parked_bare >= 0 && parked_dressed >= 0,
        "a hat was rolled and parked in both runs"
    );
    assert_eq!(
        parked_bare, parked_dressed,
        "the two runs parked the same seeded headgear style"
    );

    assert_eq!(
        bare.headgear_style, -1,
        "never opened Clothes: the create request carries the -1"
    );
    assert_eq!(
        dressed.headgear_style, parked_bare,
        "opened Clothes: it carries the hat"
    );

    // Check the other four clothing/hair style fields and positive clothing values, not every result field.
    assert_eq!(bare.shirt_style, dressed.shirt_style);
    assert_eq!(bare.trousers_style, dressed.trousers_style);
    assert_eq!(bare.footwear_style, dressed.footwear_style);
    assert_eq!(bare.hair_style, dressed.hair_style);
    assert!(bare.shirt_style >= 0 && bare.trousers_style >= 0 && bare.footwear_style >= 0);

    // Preserve the style across the UI-result to wire-result adapter.
    assert_eq!(
        dereth_client_runtime::app::chargen_result_to_wire(&bare).headgear_style,
        -1
    );
    assert_eq!(
        dereth_client_runtime::app::chargen_result_to_wire(&dressed).headgear_style,
        parked_bare
    );
}

/// Drive heritage/town/appearance tabs, type a summary name, then invoke do_finish directly.
/// Return its queued CharGenResultData and the style parked on initial Face selection.
/// open_clothes controls whether the tab-restoration path runs before creating the result.
///
/// Typing uses the actual text widget and a text-changed message, rather than assigning the
/// name-entered guard. The assertion below confirms delivery; `login::chargen_wizard` covers
/// wizard typing separately. This helper tests the accepted finish method and queue, not a Finish-button click.
fn create_character(open_clothes: bool) -> (dereth_chargen::CharGenResultData, i32) {
    let mut app = wizard_on(EcgProgress::Appearance);
    let parked = wizard(&mut app).hold_headgear;
    if open_clothes {
        click(&mut app, appearance::TAB_CLOTHES.0);
        for _ in 0..4 {
            app.frame();
        }
    }
    click(
        &mut app,
        EcgProgress::Summary
            .select_button()
            .expect("the summary tab")
            .0,
    );
    for _ in 0..4 {
        app.frame();
    }

    // Summary setup has focused the name box and selected its prompt. Type through the widget,
    // then broadcast its text-changed message 0x44 before checking wizard delivery.
    let name = {
        let shell = app.ui_mut().expect("shell");
        let root = shell.flow.current().expect("a screen is up").roots()[0];
        shell
            .ui
            .get_child_recursive(root, dereth_ui_screens::screens::chargen::NAME_FIELD)
            .expect("the name box is in the shipped layout")
    };
    {
        let shell = app.ui_mut().expect("shell");
        for ch in "Tarinell".encode_utf16() {
            shell.ui.character(ch);
        }
        shell
            .ui
            .broadcast_element_message(name, dereth_ui::MessageId(0x44), 0, 0);
    }
    app.frame();
    assert!(
        wizard(&mut app).name_entered,
        "the typed name reached the wizard"
    );

    let w = wizard(&mut app);
    w.take_actions();
    assert!(w.do_finish(false), "do_finish accepted the character");
    let result = w
        .take_actions()
        .into_iter()
        .find_map(|a| match a {
            CharGenAction::SendCharGenResult(r) => Some(*r),
            _ => None,
        })
        .expect("do_finish queued a CharGenResult");
    app.shutdown();
    (result, parked)
}

// =================================================================================================
// 4. Frames
// =================================================================================================

/// A captured frame: width, height and BGRA bytes, read back in memory.
type Frame = (u32, u32, Vec<u8>);

/// Differing pixels (colour only; alpha is not part of this comparison) and the rectangle they
/// fall in.
fn diff(a: &Frame, b: &Frame) -> (u64, Option<(u32, u32, u32, u32)>) {
    assert_eq!((a.0, a.1), (b.0, b.1), "the two frames are the same size");
    let (w, h) = (a.0, a.1);
    let mut n = 0u64;
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0u32, 0u32);
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            if a.2[i..i + 3] != b.2[i..i + 3] {
                n += 1;
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    (n, if n == 0 { None } else { Some((x0, y0, x1, y1)) })
}

/// Compare bare Face against Face with the parked hat restored directly. Switching to Clothes
/// would also zoom out and enable animation, confounding the headgear comparison. Staying on
/// Face retains its camera/zoom/rest-pose configuration; only the headgear control is changed.
/// The tab-switch test above establishes the normal restoration path, while this deliberate
/// control isolates rendering impact without switching tabs.
///
/// First require identical captures of the unchanged bare state, separated by four frames.
/// Then require a nonzero hatted difference and a bounding rectangle narrower than 400 and
/// shorter than 500 pixels. This bounds the change's extent, not its absolute position inside
/// the viewport or a separately defined head region. No explicit full-pose comparison is made.
#[test]
fn the_face_tab_frame_differs_from_the_hatted_one_only_where_the_head_is() {
    let mut app = wizard_on(EcgProgress::Appearance);
    for _ in 0..4 {
        app.frame();
    }
    let bare = app
        .renderer_mut()
        .capture_bgra()
        .expect("the frame is captured");
    for _ in 0..4 {
        app.frame();
    }
    let control = app
        .renderer_mut()
        .capture_bgra()
        .expect("the control frame is captured");
    let (noise, _) = diff(&bare, &control);
    assert_eq!(
        noise, 0,
        "the noise floor is not zero, so no differential below is evidence"
    );

    // Restore headgear while retaining Face's camera and rest-pose configuration.
    let parked = wizard(&mut app).hold_headgear;
    {
        let w = wizard(&mut app);
        let t = w.tables.clone().expect("the wizard has its tables");
        w.state.set_headgear_style(&t.chargen, parked);
    }
    for _ in 0..4 {
        app.frame();
    }
    let hatted = app
        .renderer_mut()
        .capture_bgra()
        .expect("the hatted frame is captured");

    let (n, rect) = diff(&bare, &hatted);
    eprintln!("face tab: bare vs hatted = {n} px differ, rect {rect:?}");
    assert!(
        n > 0,
        "putting the hat back changed no pixel at all: the ObjDesc never reached the body"
    );
    let (x0, y0, x1, y1) = rect.expect("a rectangle");
    // Layout viewport ID 0x100003BB identifies the appearance preview. These predicates only
    // bound the difference rectangle's width/height; they do not compare its origin or edges
    // against the viewport rectangle. They reject a broad repaint without proving head localization.
    assert!(
        x1 - x0 < 400 && y1 - y0 < 500,
        "the change exceeded the allowed model-sized extent: {rect:?}"
    );
    app.shutdown();
}
