//! The appearance page's three special heritages, the colour wheel's markers, and the shade
//! disk's gradient and tint. Gear Knight (6) and both Olthoi (`0x0C`, `0x0D`) hide Clothes, Nose
//! and Mouth, change the Hair, Eyes and Skin captions, grey the Eyes arrows, move the Skin row up
//! and take their own camera. Row selection lights exactly one pointer and one part row. The shade
//! disk is ColorRing multiplied by the chosen colour, so it is tinted and still graded, dark at the
//! **top** and light at the bottom.
//! Fixture: an offline headless `App` on the retail dats, driven through the char-gen wizard's
//! element messages and read back from its tree and captured frames; no link, FINISH never pressed.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use crate::common::client_dir;
use crate::common::gpu_lock;

use dereth_chargen::{HERITAGE_GEAR_KNIGHT, HERITAGE_OLTHOI, HERITAGE_OLTHOI_ACID};
use dereth_client::app::App;
use dereth_client_runtime::config::Config;
use dereth_ui::framework::mode;
use dereth_ui::{ElemHandle, ElementId};
use dereth_ui_screens::screens::chargen::{self, appearance, CharGenScreen, EcgProgress};

/// Every fixture path is an `expect`: a missing dat or device fails the test.
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

fn visible(app: &App, id: ElementId) -> bool {
    let h = find(app, id);
    app.ui()
        .expect("shell")
        .ui
        .node(h)
        .expect("a live node")
        .region
        .flags
        .visible
}

fn caption(app: &mut App, id: ElementId) -> String {
    let h = find(app, id);
    let shell = app.ui_mut().expect("shell");
    shell.ui.text_element_mut(h).map_or_else(String::new, |t| {
        String::from_utf16_lossy(&t.glyphs.glyphs.iter().map(|g| g.data).collect::<Vec<_>>())
    })
}

/// A row of table enum `0x10000002` as the string table itself gives it.
fn from_the_dat(app: &App, token: &str) -> String {
    app.ui()
        .expect("shell")
        .ui
        .resolve_string(
            chargen::ERROR_STRING_TABLE,
            dereth_primitives::num::hash::str_hash(token.as_bytes()),
        )
        .unwrap_or_else(|| panic!("{token} is in the shipped string table"))
}

/// Attribute `0x0D` (`props::attr::DISABLED`) on one of a row's two arrows.
fn arrow_disabled(app: &App, row: ElementId, arrow: ElementId) -> bool {
    let shell = app.ui().expect("shell");
    let r = shell
        .ui
        .get_child_recursive(root_of(app), row)
        .expect("the row");
    let h = shell.ui.get_child_recursive(r, arrow).expect("the arrow");
    shell
        .ui
        .node(h)
        .expect("a live node")
        .merged_properties()
        .get_bool(dereth_ui::props::attr::DISABLED)
        .unwrap_or(false)
}

/// The shade disk's **vertical luminance profile**: the median luminance of a band near the top
/// of its interior and of one near the bottom.
///
/// The shade knob lies on the disk, far brighter than the interior around it (190 against 13 in a
/// retail frame), so a band mean would be pulled up by it; the median is not.
fn disk_profile(app: &mut App) -> (f64, f64) {
    let (x0, y0, x1, y1) = {
        let h = find(app, appearance::GRAD_CIRCLE);
        let b = app.ui_mut().expect("shell").ui.screen_clip_box(h);
        assert!(b.is_valid(), "the shade disk has a screen box");
        (b.x0, b.y0, b.x1, b.y1)
    };
    app.frame();
    let (w, _h, px) = app
        .renderer_mut()
        .capture_bgra()
        .expect("the frame captures");
    let cx = (x0 + x1) / 2;
    let height = y1 - y0;
    // Inset by a quarter so the gold ring around the disk is outside both bands.
    let inset = height / 4;
    let lum = |y: i32| -> f64 {
        let i = ((y as u32 * w + cx as u32) * 4) as usize;
        // BGRA.
        0.114 * f64::from(px[i]) + 0.587 * f64::from(px[i + 1]) + 0.299 * f64::from(px[i + 2])
    };
    let median = |from: i32, to: i32| -> f64 {
        let mut v: Vec<f64> = (from..to).map(lum).collect();
        v.sort_by(f64::total_cmp);
        v[v.len() / 2]
    };
    let top = median(y0 + inset, y0 + inset + height / 6);
    let bottom = median(y1 - inset - height / 6, y1 - inset);
    (top, bottom)
}

/// The shade disk's **median colour**, which is the half [`disk_profile`] cannot see.
///
/// A median per channel over the disk's interior, rejecting the shade knob for the same
/// reason the profile does.
fn disk_median_rgb(app: &mut App) -> [f64; 3] {
    let (x0, y0, x1, y1) = {
        let h = find(app, appearance::GRAD_CIRCLE);
        let b = app.ui_mut().expect("shell").ui.screen_clip_box(h);
        assert!(b.is_valid(), "the shade disk has a screen box");
        (b.x0, b.y0, b.x1, b.y1)
    };
    app.frame();
    let (w, _h, px) = app
        .renderer_mut()
        .capture_bgra()
        .expect("the frame captures");
    let inset = (y1 - y0) / 4;
    let mut ch: [Vec<f64>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    for y in (y0 + inset)..(y1 - inset) {
        for x in (x0 + inset)..(x1 - inset) {
            let i = ((y as u32 * w + x as u32) * 4) as usize;
            // BGRA.
            ch[0].push(f64::from(px[i + 2]));
            ch[1].push(f64::from(px[i + 1]));
            ch[2].push(f64::from(px[i]));
        }
    }
    let mut out = [0.0; 3];
    for (o, v) in out.iter_mut().zip(ch.iter_mut()) {
        v.sort_by(f64::total_cmp);
        *o = v[v.len() / 2];
    }
    out
}

fn choose(app: &mut App, heritage: u32) {
    let (id, n) = chargen::HERITAGE_BUTTONS
        .iter()
        .copied()
        .find(|(_, n)| *n == heritage)
        .expect("every heritage has a bullet");
    assert_eq!(n, heritage);
    // Change heritage on its own page, then return so Appearance refreshes from that choice.
    click(
        app,
        EcgProgress::Hertage
            .select_button()
            .expect("the heritage tab"),
    );
    app.frame();
    click(app, id);
    for _ in 0..2 {
        app.frame();
    }
    click(
        app,
        EcgProgress::Appearance
            .select_button()
            .expect("the appearance tab"),
    );
    for _ in 0..4 {
        app.frame();
    }
    assert_eq!(
        wizard(app).state.heritage_group,
        heritage,
        "the heritage was taken"
    );
}

// =================================================================================================
// 1. The three arms
// =================================================================================================

/// The three special heritages against the ten ordinary ones:
///
/// | | ordinary (10) | Gear Knight (6) | Olthoi (`0x0C`, `0x0D`) |
/// |---|---|---|---|
/// | Clothes button, Nose spinner, Mouth spinner | visible | hidden | hidden |
/// | Hair / Eyes / Skin captions | `ID_CharGen_HairStyle` / `_Eyes` / `_Skin` | `ID_CharGen_GearText_*Button` | `ID_CharGen_OlthoiText_*Button` |
/// | Eyes arrows' disabled attribute 0x0D | false | true | true |
/// | Skin row position (x,y) | (0,0xB4) | (0,0x5A) | (0,0x5A) |
/// | camera | `(0, -0.55, 1.65)`, or `-0.85` for heritage 7 | `(0, -0.55, 1.65)` | `(0, -1.85, 1.85)` / `(0, -3.05, 2.75)` |
///
/// Exactly three captions change (Hair, Eyes, Skin); hiding Nose and Mouth moves Skin from y=180
/// to y=90, leaving three rows rather than five with a gap.
///
/// Exercise all three special heritages against an Aluvian ordinary baseline, then return to
/// that baseline. This samples one of the ten ordinary heritages rather than exhaustively
/// testing them all. Removing apply_special_heritage_chrome from appearance_update leaves
/// the Clothes/Nose/Mouth controls visible and the captions ordinary on Gear Knight.
#[test]
fn the_three_special_heritages_get_a_three_row_face_page_and_the_other_ten_do_not() {
    let _gpu = gpu_lock();
    let mut app = wizard_on_appearance();

    // The ordinary page first, so that every claim below is a *difference* and not a constant.
    choose(&mut app, 1);
    assert!(
        visible(&app, appearance::TAB_CLOTHES),
        "an Aluvian has clothes"
    );
    assert!(visible(&app, appearance::NOSE_ROW), "and a nose");
    assert!(visible(&app, appearance::MOUTH_ROW), "and a mouth");
    assert_eq!(
        caption(&mut app, appearance::HAIR_ROW),
        from_the_dat(&app, "ID_CharGen_HairStyle")
    );
    assert_eq!(
        caption(&mut app, appearance::EYES_ROW),
        from_the_dat(&app, "ID_CharGen_Eyes")
    );
    assert_eq!(
        caption(&mut app, appearance::SKIN_ROW),
        from_the_dat(&app, "ID_CharGen_Skin")
    );
    assert!(
        !arrow_disabled(&app, appearance::EYES_ROW, appearance::ARROW_NEXT),
        "an Aluvian can page through eye strips"
    );
    let ordinary_skin_y = {
        let h = find(&app, appearance::SKIN_ROW);
        app.ui()
            .expect("shell")
            .ui
            .node(h)
            .expect("a live node")
            .region
            .box_
            .y0
    };

    for (heritage, captions) in [
        (HERITAGE_GEAR_KNIGHT, chargen::APPEARANCE_CAPTIONS_GEAR),
        (HERITAGE_OLTHOI, chargen::APPEARANCE_CAPTIONS_OLTHOI),
        (HERITAGE_OLTHOI_ACID, chargen::APPEARANCE_CAPTIONS_OLTHOI),
    ] {
        choose(&mut app, heritage);
        assert!(
            !visible(&app, appearance::TAB_CLOTHES),
            "{heritage}: the Clothes button is hidden"
        );
        assert!(
            !visible(&app, appearance::NOSE_ROW),
            "{heritage}: the Nose spinner is hidden"
        );
        assert!(
            !visible(&app, appearance::MOUTH_ROW),
            "{heritage}: the Mouth spinner is hidden"
        );
        for (row, token) in [
            appearance::HAIR_ROW,
            appearance::EYES_ROW,
            appearance::SKIN_ROW,
        ]
        .iter()
        .zip(captions)
        {
            let want = from_the_dat(&app, token);
            assert!(!want.is_empty(), "{token} has a row");
            assert_eq!(caption(&mut app, *row), want, "{heritage}: {token}");
        }
        for arrow in [appearance::ARROW_PREV, appearance::ARROW_NEXT] {
            assert!(
                arrow_disabled(&app, appearance::EYES_ROW, arrow),
                "{heritage}: the Eyes row's arrows are greyed, not hidden"
            );
        }
        let y = {
            let h = find(&app, appearance::SKIN_ROW);
            app.ui()
                .expect("shell")
                .ui
                .node(h)
                .expect("a live node")
                .region
                .box_
                .y0
        };
        assert_ne!(y, ordinary_skin_y, "{heritage}: the Skin row moved");
        assert_eq!(
            y - ordinary_skin_y,
            chargen::SKIN_ROW_Y_SPECIAL - chargen::SKIN_ROW_Y_ORDINARY,
            "{heritage}: Skin row displacement matches 0x5A minus 0xB4"
        );
        // With no Clothes button there is no way back off the Face tab, so the Face tab and the
        // Hair row are selected.
        assert!(
            wizard(&mut app).face_tab,
            "{heritage}: the Face tab is selected"
        );
        assert_eq!(
            wizard(&mut app).current_part,
            dereth_ui_screens::screens::chargen::EParts::Hair,
            "{heritage}: the Hair row is selected"
        );
        assert_eq!(
            wizard(&mut app).view3d.camera_position,
            chargen::zoomed_in_camera(heritage),
            "{heritage}: the arm's own camera"
        );
    }

    // Return to the ordinary heritage: restoration catches an implementation that only hides.
    // The change is detected by remembering the previously selected heritage.
    choose(&mut app, 1);
    assert!(
        visible(&app, appearance::TAB_CLOTHES),
        "the Clothes button comes back"
    );
    assert!(visible(&app, appearance::NOSE_ROW));
    assert!(visible(&app, appearance::MOUTH_ROW));
    assert_eq!(
        caption(&mut app, appearance::HAIR_ROW),
        from_the_dat(&app, "ID_CharGen_HairStyle")
    );
    assert!(!arrow_disabled(
        &app,
        appearance::EYES_ROW,
        appearance::ARROW_NEXT
    ));
    let y = {
        let h = find(&app, appearance::SKIN_ROW);
        app.ui()
            .expect("shell")
            .ui
            .node(h)
            .expect("a live node")
            .region
            .box_
            .y0
    };
    assert_eq!(y, ordinary_skin_y, "and the Skin row comes back down");
    app.shutdown();
}

/// Construction disables both Skin arrows because skin has colour but no style
/// choices; otherwise the arrows would silently reject every click. This test samples Aluvian,
/// Gear Knight and Olthoi. Removing (SKIN_ROW,true) from apply_special_heritage_chrome fails it.
#[test]
fn the_skin_rows_arrows_are_greyed_on_every_heritage() {
    let _gpu = gpu_lock();
    let mut app = wizard_on_appearance();
    for heritage in [1, HERITAGE_GEAR_KNIGHT, HERITAGE_OLTHOI] {
        choose(&mut app, heritage);
        for arrow in [appearance::ARROW_PREV, appearance::ARROW_NEXT] {
            assert!(
                arrow_disabled(&app, appearance::SKIN_ROW, arrow),
                "{heritage}: the Skin row's arrows are greyed"
            );
        }
    }
    app.shutdown();
}

// =================================================================================================
// 2. The wheel's markers and the selected row
// =================================================================================================

/// Behaviour: chargen.appearance.clicking-a-colour-moves-the-marker-and-tints-the-shade-wheel-with-it
///
/// Row selection makes all nine colour spots visible and hides all nine pointers, then colour
/// selection shows only the chosen one.
/// Removing the pointer-hide step leaves the layout's nine markers visible. The test also selects a different colour and requires the marker to move.
#[test]
fn nine_spots_are_shown_and_exactly_one_marker() {
    let _gpu = gpu_lock();
    let mut app = wizard_on_appearance();
    choose(&mut app, 1);
    // A part with several colours, so "exactly one" is a choice and not an accident of there being
    // only one to point at.
    click(&mut app, appearance::HAIR_ROW);
    for _ in 0..2 {
        app.frame();
    }
    let n = wizard(&mut app).choices[0].num_colors;
    assert!(
        n > 1,
        "the hair row offers {n} colours, so a marker is a choice"
    );

    let spots = appearance::COLOR_SPOTS
        .iter()
        .filter(|id| visible(&app, **id))
        .count();
    assert_eq!(
        spots, 9,
        "all nine colour spots are shown, lit or ColorEmpty"
    );

    let lit: Vec<usize> = appearance::COLOR_POINTERS
        .iter()
        .enumerate()
        .filter(|(_, id)| visible(&app, **id))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(lit.len(), 1, "exactly one marker, not nine: {lit:?}");
    assert_eq!(
        i32::try_from(lit[0]).expect("nine fits"),
        wizard(&mut app).current_color,
        "and it is over the current colour"
    );

    // Click a different spot and the marker moves rather than multiplies.
    let other = if lit[0] == 0 { 1 } else { 0 };
    click(&mut app, appearance::COLOR_SPOTS[other]);
    for _ in 0..2 {
        app.frame();
    }
    let lit2: Vec<usize> = appearance::COLOR_POINTERS
        .iter()
        .enumerate()
        .filter(|(_, id)| visible(&app, **id))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(
        lit2,
        vec![other],
        "the marker moved to the spot that was clicked"
    );
    app.shutdown();
}

/// Selecting Eyes after Hair must clear Hair to NORMAL(1) and set Eyes to TOGGLED(6). The test
/// checks that pair in both states; it does not enumerate every row. Removing the old-row
/// reset leaves both lit and fails the negative check.
#[test]
fn exactly_one_part_row_is_latched_lit_and_it_is_the_selected_one() {
    use dereth_ui::widgets::button::state::{NORMAL, TOGGLED};
    let _gpu = gpu_lock();
    let mut app = wizard_on_appearance();
    choose(&mut app, 1);
    click(&mut app, appearance::HAIR_ROW);
    for _ in 0..2 {
        app.frame();
    }
    let state = |app: &App, id: ElementId| {
        let h = find(app, id);
        app.ui()
            .expect("shell")
            .ui
            .node(h)
            .expect("a live node")
            .state
    };
    assert_eq!(state(&app, appearance::HAIR_ROW), TOGGLED);
    assert_eq!(state(&app, appearance::EYES_ROW), NORMAL);

    click(&mut app, appearance::EYES_ROW);
    for _ in 0..2 {
        app.frame();
    }
    assert_eq!(
        state(&app, appearance::EYES_ROW),
        TOGGLED,
        "the new row lights"
    );
    assert_eq!(
        state(&app, appearance::HAIR_ROW),
        NORMAL,
        "and the old one clears"
    );
    app.shutdown();
}

// =================================================================================================
// 3. The shade disk's gradient
// =================================================================================================

/// Check the disk's dark-top/light-bottom shape. A retail frame's centre-column medians are 16.1
/// and 33.5 (ratio 2.08); an untinted disk measures 94.5 and 222.5 (ratio 2.35). These establish
/// direction and relative rise, not exact pixel colours: the chosen colour, shade and heritage
/// affect absolute luminance.
///
/// The test requires bottom>1.5*top and bottom>8, with the Eyes flat-plug case below as the
/// opposite-shape control. Replacing ColorRing with ColorEmpty fails the gradient check; the tint
/// is checked separately below, because an untinted ring still rises.
#[test]
fn the_shade_disk_rises_from_a_dark_top_to_a_light_bottom_as_retails_does() {
    let _gpu = gpu_lock();
    let mut app = wizard_on_appearance();
    choose(&mut app, 1);
    click(&mut app, appearance::HAIR_ROW);
    for _ in 0..4 {
        app.frame();
    }
    let (top, bottom) = disk_profile(&mut app);
    eprintln!(
        "shade disk: our disk's column runs {top:.1} at the top to {bottom:.1} at the bottom, \
         ratio {:.2} (retail's own frame: 16.1 -> 33.5, ratio 2.08)",
        bottom / top.max(0.001)
    );
    assert!(
        bottom > top * 1.5,
        "the disk is flat: {top:.1} at the top against {bottom:.1} at the bottom, so the \
         ColorRing gradient did not survive the multiply"
    );
    // And it is not a black hole either, which is the other way this can fail.
    assert!(
        bottom > 8.0,
        "the whole disk is black: {bottom:.1} at its lightest"
    );
    app.shutdown();
}

/// Behaviour: chargen.appearance.the-eyes-have-one-colour-each-and-no-shade-to-slide
///
/// Eyes use the flat GradientPlug and hide the shade slider because eye colour has no shade.
/// The plug must not rise like ColorRing: drawing the ring for both fails this check, while
/// drawing the plug for both fails the preceding one. Passing false for the eyes argument in
/// set_selection's do_grad_disk call is the targeted mutation.
#[test]
fn the_eyes_row_takes_the_flat_plug_and_hides_the_shade_slider() {
    let _gpu = gpu_lock();
    let mut app = wizard_on_appearance();
    choose(&mut app, 1);
    click(&mut app, appearance::EYES_ROW);
    for _ in 0..4 {
        app.frame();
    }
    assert!(
        !visible(&app, appearance::SHADE_SCROLL),
        "eyes have no shade"
    );
    let (top, bottom) = disk_profile(&mut app);
    eprintln!(
        "shade disk: the plug's column runs {top:.1} at the top to {bottom:.1} at the bottom, \
         ratio {:.2}",
        bottom / top.max(0.001)
    );
    assert!(
        bottom < top * 1.5,
        "the eyes row drew the graded ring, not the flat plug: {top:.1} against {bottom:.1}"
    );
    app.shutdown();
}

/// The tint, which the gradient test cannot see. `SurfaceOp::Multiply` supplies colour; ColorRing already contains a vertical luminance shape,
/// so an untinted ring still rises. The gradient assertion alone could not detect that omission.
///
/// ColorRing is multiplied by the chosen colour over the whole image (no mask), each RGB channel
/// becoming `(dst * (c + (c != 0))) >> 8` with alpha carried through unchanged. Choosing
/// the same spot must preserve the median RGB; a different available colour must change it.
/// Removing Multiply from do_grad_disk or removing that call from set_color fails this test.
#[test]
fn the_shade_disk_takes_the_colour_of_the_spot_that_was_chosen() {
    let _gpu = gpu_lock();
    let mut app = wizard_on_appearance();
    choose(&mut app, 1);
    click(&mut app, appearance::HAIR_ROW);
    for _ in 0..4 {
        app.frame();
    }

    // Re-selecting the same spot must preserve median RGB before testing a different colour.
    // This controls the measured medians, not equality of every disk pixel.
    click(&mut app, appearance::COLOR_SPOTS[0]);
    for _ in 0..4 {
        app.frame();
    }
    let first = disk_median_rgb(&mut app);
    click(&mut app, appearance::COLOR_SPOTS[0]);
    for _ in 0..4 {
        app.frame();
    }
    let again = disk_median_rgb(&mut app);
    assert_eq!(first, again, "re-choosing the same spot moved the disk");

    // Now a spot whose own colour differs, and the disk must follow it.
    let wheel = wizard(&mut app).color_wheel;
    let c0 = wheel[0].expect("spot 0 has a colour");
    let (i, c1) = wheel
        .iter()
        .enumerate()
        .skip(1)
        .find_map(|(i, c)| c.filter(|c| *c != c0).map(|c| (i, c)))
        .expect("the hair wheel holds at least two distinct colours");
    click(&mut app, appearance::COLOR_SPOTS[i]);
    for _ in 0..4 {
        app.frame();
    }
    let second = disk_median_rgb(&mut app);

    let moved: f64 = first
        .iter()
        .zip(second.iter())
        .map(|(a, b)| (a - b).abs())
        .sum();
    eprintln!(
        "shade disk: spot 0 ({c0:#010X}) -> spot {i} ({c1:#010X}) moves the disk's median RGB \
         {first:?} -> {second:?}, total {moved:.1}"
    );
    assert!(
        moved > 6.0,
        "the disk's median colour moved only {moved:.1} across two different spots \
         ({first:?} -> {second:?}): the ring is not being multiplied by the chosen colour"
    );
    app.shutdown();
}
