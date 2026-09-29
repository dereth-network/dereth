//! The char-gen preview is dressed from the appearance state and stands in its background object.
//! Every stage of the dressing update runs (four descriptor merges, the clothing builds, three
//! subpalettes); the dressed body draws different geometry from the naked setup; a clothes arrow
//! re-bakes meshes, a wheel colour and the shade slider move the hair palette range without
//! re-baking; the summary page wears the same; and the heritage's environment setup is a second
//! object that paints inside the viewport. Geometry is read from `PreviewObject::built_from`
//! (what was baked), palettes from the per-part surface overrides, and
//! `dereth_client::preview::ChargenDressStats` counts each stage.
//! Fixture: the retail dats and an offline headless App at 800x600 on the wizard (Aluvian,
//! Holtburg); the list-rule and palette-index tests use synthetic inputs and no device.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use crate::common::client_dir;

use dereth_animation::parts::{AnimPartChange, ObjDesc, PaletteRange, TextureMapChange};
use dereth_client::app::App;
use dereth_client::config::Config;
use dereth_client::gpu::PreviewId;
use dereth_client::preview::pal_set_palette_id;
use dereth_primitives::DataId;
use dereth_ui::framework::mode;
use dereth_ui::ElementId;
use dereth_ui_screens::screens::chargen::{appearance, CharGenScreen, EParts, EcgProgress};

/// Broadcast button-click message 1 with parameters 7,0 on a live element; no pointer is driven.
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

/// Use the chosen part row's next or previous arrow. The next arrow has id 0x1000030B;
/// its parent identifies the part to the appearance-page message consumer.
fn press_part_arrow(app: &mut App, part: EParts, next: bool) {
    let row = appearance_row(part);
    let arrow = if next {
        appearance::ARROW_NEXT
    } else {
        appearance::ARROW_PREV
    };
    let shell = app.ui_mut().expect("the shell is up");
    let root = shell.flow.current().expect("a screen is up").roots()[0];
    let r = shell
        .ui
        .get_child_recursive(root, row)
        .expect("the part row is in the layout");
    let a = shell
        .ui
        .get_child_recursive(r, arrow)
        .expect("the row carries both arrows");
    shell
        .ui
        .broadcast_element_message(a, dereth_ui::msg::element::id::BUTTON_CLICKED, 7, 0);
}

fn appearance_row(part: EParts) -> ElementId {
    dereth_ui_screens::screens::chargen::APPEARANCE_ROWS
        .iter()
        .find(|(_, p)| *p == part)
        .expect("every part has a row")
        .0
}

/// Every fixture path is an `expect`: a missing dat or device fails the test.
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
    // The preview update requires a nonzero heritage and gender. Choose a heritage before
    // asserting clothing; current setup resolution also has an unchosen-state fallback.
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

/// The `GfxObj` id every one of the model's baked meshes was built from -- the **drawn** geometry.
fn baked(app: &mut App) -> Vec<DataId> {
    app.renderer_mut()
        .preview(PreviewId::CharGen)
        .expect("the space exists")
        .object(0)
        .expect("the model is object 0")
        .built_from()
        .to_vec()
}

/// Per-part shift-palette ranges supplied to baking, read from surface overrides.
fn palettes(app: &mut App) -> Vec<Option<(Option<DataId>, Vec<PaletteRange>)>> {
    app.renderer_mut()
        .preview(PreviewId::CharGen)
        .expect("the space exists")
        .object(0)
        .expect("the model is object 0")
        .part_array
        .parts
        .iter()
        .map(|p| {
            p.surface_overrides
                .as_ref()
                .map(|o| (o.shift_palette, o.subpalettes.clone()))
        })
        .collect()
}

fn differing<T: PartialEq>(a: &[T], b: &[T]) -> usize {
    assert_eq!(
        a.len(),
        b.len(),
        "the part array changed length, which no part swap does"
    );
    a.iter().zip(b.iter()).filter(|(x, y)| x != y).count()
}

// =================================================================================================
// 1. The block ran, and all of it ran
// =================================================================================================

/// Count four merges across three feature families (hair, eyes, nose/mouth), the clothing builds,
/// and three final subpalettes. The update permits four clothing builds, but the initial Face tab
/// suppresses headgear, so this fixture expects three.
///
/// Stage counters distinguish an empty description caused by unchosen state from one caused
/// by an update block that never ran; the resulting descriptor alone cannot do so.
#[test]
fn every_stage_of_the_update_tail_runs() {
    let app = wizard_on(EcgProgress::Appearance);
    let s = app.chargen_dress();
    // Four `+=` call sites over three families: the hair style, the eyes strip (bald or not) and
    // the nose and mouth strips.
    assert_eq!(
        s.merges, 4,
        "the four descriptor merges after the gender base description"
    );
    // Page construction selects Face, parks the hat, and stores headgear style -1, so trousers,
    // shirt and footwear run here; the Clothes tab restores the fourth arm.
    // `login::chargen_face_tab` covers both transitions.
    assert_eq!(
        s.clothing_calls, 3,
        "trousers, shirt and footwear -- the Face tab bares the head"
    );
    assert_eq!(
        s.clothing_ok, 3,
        "every clothing-description build found its base and its palette template"
    );
    assert_eq!(
        s.clothing_base_missing, 0,
        "a miss here would take the client's hard-coded per-race/sex fallback chain, which is \
         deliberately not reproduced"
    );
    assert_eq!(
        s.subpalettes, 3,
        "skin at 0/0xC0, hair at 0xC0/0x40, eyes at 0x100/0x40"
    );
    assert_eq!(
        s.subpalette_unresolved, 0,
        "every palette-set lookup resolved: an unrolled shade of -1 would show here"
    );
    app.shutdown();
}

/// Applying the object description succeeds and every part receives a surface override.
/// `PreviewObject::dressed` retains the application result: None means no descriptor was
/// supplied; false means failure.
#[test]
fn the_descriptor_reaches_the_body_and_reports_success() {
    let mut app = wizard_on(EcgProgress::Appearance);
    let space = app
        .renderer_mut()
        .preview(PreviewId::CharGen)
        .expect("the space");
    let o = space.object(0).expect("the model");
    assert_eq!(
        o.dressed,
        Some(true),
        "`None` is an object added with no ObjDesc at all; `Some(false)` is a part index the \
         setup does not have"
    );
    assert_eq!(o.part_array.parts.len(), 34, "the Aluvian male body");
    assert_eq!(
        o.parts_with_surface_overrides(),
        34,
        "part-array palette application applies the shift palette to *every* part"
    );
    assert!(
        o.drawn_parts() > 30,
        "only {} parts contributed geometry",
        o.drawn_parts()
    );
    app.shutdown();
}

/// Behaviour: chargen.appearance.the-preview-wears-the-clothes-the-wizard-chose
///
/// **The drawn geometry, against the naked setup.**
///
/// The same setup is added to the same space a second time with no descriptor, and the two
/// objects' `built_from` lists are compared. A build that applied the `ObjDesc` after the bake --
/// or not at all -- reads 0 here while every model assertion still passes.
#[test]
fn the_dressed_model_draws_different_geometry_from_the_naked_setup() {
    let mut app = wizard_on(EcgProgress::Appearance);
    let dressed = baked(&mut app);
    let setup = app
        .renderer_mut()
        .preview(PreviewId::CharGen)
        .expect("the space")
        .object(0)
        .expect("the model")
        .setup;

    let store =
        dereth_client::assets::open_data_files(&client_dir()).expect("the retail dats open");
    let i = app
        .renderer_mut()
        .add_preview_object(PreviewId::CharGen, &store, setup)
        .expect("the naked object builds")
        .expect("the setup loads");
    let naked = app
        .renderer_mut()
        .preview(PreviewId::CharGen)
        .expect("the space")
        .object(i)
        .expect("the naked object")
        .built_from()
        .to_vec();

    let d = differing(&dressed, &naked);
    eprintln!(
        "{d} of {} baked GfxObj ids differ from the naked setup's",
        naked.len()
    );
    assert!(
        d >= 10,
        "only {d} of {} baked meshes differ from the naked body: the clothing and the face are \
         part swaps, so this is what wearing anything looks like on the GPU",
        naked.len()
    );
    app.shutdown();
}

// =================================================================================================
// 2. The arrows, and the colours
// =================================================================================================

/// A face/clothes arrow changes the drawn geometry. `login::chargen_wizard`'s
/// `the_part_arrows_wrap_the_way_the_client_wraps_them` covers the arrow index changes; here one
/// shirt-next press must change baked mesh sources, after an idle-frame zero control.
#[test]
fn a_clothes_arrow_changes_the_geometry_that_is_drawn() {
    let mut app = wizard_on(EcgProgress::Appearance);
    // The Clothes tab, so the four garment rows are the ones showing.
    click(&mut app, appearance::TAB_CLOTHES.0);
    for _ in 0..3 {
        app.frame();
    }
    let before = baked(&mut app);
    // A control pair first: no input, so nothing may move. Without this the assertion below
    // cannot tell a re-bake from an ordinary frame.
    app.frame();
    assert_eq!(
        differing(&before, &baked(&mut app)),
        0,
        "an idle frame re-baked the body, so the measurement below would mean nothing"
    );

    press_part_arrow(&mut app, EParts::Shirt, true);
    for _ in 0..3 {
        app.frame();
    }
    let after = baked(&mut app);
    let d = differing(&before, &after);
    eprintln!("one shirt arrow re-bakes {d} of {} parts", before.len());
    assert!(
        d > 0,
        "the shirt arrow moved the state and the drawn geometry did not follow"
    );
    app.shutdown();
}

/// The Summary page rebuilds its own preview from the same appearance state. The summary update
/// invokes the same dressing operation; its stage counts and applied result
/// are checked here, separately from the Appearance page's baked-geometry comparison.
#[test]
fn the_summary_page_shows_the_same_dressed_character() {
    let mut app = wizard_on(EcgProgress::Summary);
    let s = app.chargen_dress();
    // Face initialization left headgear style -1, and this fixture never opened Clothes.
    // Summary reads that same state, so the expected stage counts are (4,3,3), not (4,4,3).
    // The selected headgear value also feeds the eventual finish request; no finish is sent here.
    assert_eq!(
        (s.merges, s.clothing_calls, s.subpalettes),
        (4, 3, 3),
        "the same tail runs"
    );
    let space = app
        .renderer_mut()
        .preview(PreviewId::CharGen)
        .expect("the space");
    assert_eq!(space.object(0).expect("the model").dressed, Some(true));
    assert_eq!(
        space
            .object(0)
            .expect("the model")
            .parts_with_surface_overrides(),
        34,
        "the summary model wears what the appearance model wears"
    );
    app.shutdown();
}

/// Colour selection must update the surface palette ranges supplied to the model's bake.
/// This checks the nine wheel spots' appearance-state consumer.
///
/// The hair range starts at 0xC0 and spans 0x40 colours: offset 24/length 8 in eight-entry wire
/// units. Selecting a different palette must change that range on every part while keeping
/// every baked graphics-object id unchanged. This is palette-input evidence, not a pixel test.
#[test]
fn a_colour_chosen_on_the_wheel_reaches_the_model() {
    let mut app = wizard_on(EcgProgress::Appearance);
    // Select Hair so the wheel uses the selected gender's hair-colour list.
    click(&mut app, appearance_row(EParts::Hair).0);
    for _ in 0..2 {
        app.frame();
    }
    let hair_range = |p: &[Option<(Option<DataId>, Vec<PaletteRange>)>]| {
        p.iter()
            .map(|e| {
                e.as_ref()
                    .expect("every part carries the shift palette")
                    .1
                    .iter()
                    .find(|r| r.offset == 0xC0 / 8 && r.length == 0x40 / 8)
                    .copied()
                    .expect("the hair subpalette is in every part's list")
            })
            .collect::<Vec<_>>()
    };
    let before = palettes(&mut app);
    let before_hair = hair_range(&before);
    let before_geo = baked(&mut app);

    // Choose spot index 3 of the nine spots.
    click(&mut app, appearance::COLOR_SPOTS[3].0);
    for _ in 0..3 {
        app.frame();
    }
    let after = palettes(&mut app);
    let after_hair = hair_range(&after);

    assert_ne!(
        before_hair[0].palette_set, after_hair[0].palette_set,
        "the hair colour did not reach the palette the body is drawn through"
    );
    assert_eq!(
        differing(&before_hair, &after_hair),
        before_hair.len(),
        "the shift palette applies to every part, so every part's hair range must have moved"
    );
    assert_eq!(
        differing(&before_geo, &baked(&mut app)),
        0,
        "a colour is a palette, not a mesh: no part may be re-baked from a different GfxObj"
    );
    app.shutdown();
}

/// The shade consumer reselects a palette from the chosen hair palette set and refreshes the
/// preview. Calling it directly checks the model-side effect, not pointer dragging of a slider.
#[test]
fn the_shade_slider_reaches_the_model() {
    let mut app = wizard_on(EcgProgress::Appearance);
    click(&mut app, appearance_row(EParts::Hair).0);
    for _ in 0..2 {
        app.frame();
    }
    let hair_of = |app: &mut App| {
        palettes(app)[0]
            .as_ref()
            .expect("the shift palette")
            .1
            .iter()
            .find(|r| r.offset == 0xC0 / 8 && r.length == 0x40 / 8)
            .copied()
            .expect("the hair subpalette")
            .palette_set
    };
    let before = hair_of(&mut app);
    let mut seen = std::collections::BTreeSet::new();
    seen.insert(before);
    // Sample eleven shades from 0 through 1. Truncating palette selection must yield more than
    // one id for this fixture's set; the arithmetic unit below checks exact boundaries.
    for step in 0..=10 {
        {
            let shell = app.ui_mut().expect("the shell is up");
            let screen = shell.flow.current_mut().expect("a screen is up");
            let any: &mut dyn std::any::Any = &mut **screen;
            let w = any
                .downcast_mut::<CharGenScreen>()
                .expect("the wizard is up");
            let shade = f64::from(step) / 10.0;
            w.set_shade(&mut shell.ui, shade);
        }
        for _ in 0..2 {
            app.frame();
        }
        seen.insert(hair_of(&mut app));
    }
    assert!(
        seen.len() > 1,
        "the shade slider swept 0 -> 1 and the palette-set lookup answered one palette throughout"
    );
    app.shutdown();
}

// =================================================================================================
// 3. The background object
// =================================================================================================

/// The model stands in something rather than a void.
///
/// The preview keeps a second object built from the heritage's environment setup id in the
/// character-generation table: the space holds the model plus its environment.
#[test]
fn the_background_object_stands_behind_the_model() {
    let mut app = wizard_on(EcgProgress::Appearance);
    let bg = {
        let shell = app.ui_mut().expect("the shell is up");
        let screen = shell.flow.current().expect("a screen is up");
        let any: &dyn std::any::Any = screen;
        any.downcast_ref::<CharGenScreen>()
            .expect("the wizard is up")
            .view3d
            .bg_setup
    };
    assert_ne!(
        bg.0, 0,
        "the Aluvian heritage names an environment setup ID"
    );
    let space = app
        .renderer_mut()
        .preview(PreviewId::CharGen)
        .expect("the space");
    assert_eq!(space.object_count(), 2, "the model and its background");
    let o = space.object(1).expect("the background object");
    assert_eq!(
        o.setup, bg,
        "built from the selected heritage's environment setup"
    );
    assert!(
        o.drawn_parts() > 0,
        "the background contributed no geometry at all"
    );
    assert!(o.triangles() > 0, "the background baked no triangles");
    app.shutdown();
}

/// Compare frames with and without the background object in one process. Face-tab animation
/// is held at 0 fps and the turntable is stationary; an idle-frame byte-equality control verifies
/// stability before removal, so pose changes cannot hide the difference.
#[test]
fn the_background_object_changes_the_frame_inside_the_viewport() {
    let mut app = wizard_on(EcgProgress::Appearance);
    let area = {
        let shell = app.ui_mut().expect("the shell is up");
        let root = shell.flow.current().expect("a screen is up").roots()[0];
        let h = shell
            .ui
            .get_child_recursive(root, appearance::VIEWPORT)
            .expect("the viewport is in the layout");
        shell.ui.screen_clip_box(h)
    };
    assert!(area.is_valid());

    app.frame();
    let (w, h, a) = app
        .renderer_mut()
        .capture_bgra()
        .expect("the frame captures");
    // The control: two frames with nothing changed must be byte-identical, or the difference
    // below is not attributable to the background object.
    app.frame();
    let (_, _, a2) = app
        .renderer_mut()
        .capture_bgra()
        .expect("the frame captures");
    assert_eq!(
        a, a2,
        "two idle frames differ, so this instrument cannot attribute anything"
    );

    // Take the background away and nothing else. `preview_use_time` rebuilds on the key, which
    // has not moved, so the space stays as this leaves it.
    {
        let s = app
            .renderer_mut()
            .preview_mut(PreviewId::CharGen)
            .expect("the space");
        assert_eq!(s.object_count(), 2);
        s.remove_object(1);
        assert_eq!(s.object_count(), 1, "only the background went away");
    }
    app.frame();
    let (w2, h2, b) = app
        .renderer_mut()
        .capture_bgra()
        .expect("the frame captures");
    assert_eq!((w, h), (w2, h2));

    let mut changed = 0u32;
    let mut outside = 0u32;
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            if a[i..i + 3] != b[i..i + 3] {
                changed += 1;
                let (px, py) = (x as i32, y as i32);
                if px < area.x0 || px > area.x1 || py < area.y0 || py > area.y1 {
                    outside += 1;
                }
            }
        }
    }
    eprintln!("the background object paints {changed} pixels");
    assert_eq!(
        outside, 0,
        "{outside} of {changed} changed pixels fell outside the viewport"
    );
    assert!(
        changed > 100,
        "the background object painted only {changed} pixels"
    );
    app.shutdown();
}

// =================================================================================================
// 4. The arithmetic, without a device
// =================================================================================================

/// Palette selection truncates `(palette_count - 0.000001) * shade` after rejecting
/// empty sets and out-of-range shades. The subtraction keeps shade 1.0 within the palette list.
#[test]
fn pal_set_get_palette_id_is_the_clients_truncating_index() {
    let set: Vec<DataId> = (1..=4).map(DataId).collect();
    assert_eq!(pal_set_palette_id(&set, 0.0), DataId(1));
    // At this boundary, (4 - 0.000001) * 0.25 is 0.99999975 and truncates to 0, not 1.
    // Plain palette_count * shade would answer DataId(2); the same subtraction also bounds 1.0.
    assert_eq!(pal_set_palette_id(&set, 0.25), DataId(1));
    assert_eq!(pal_set_palette_id(&set, 0.5), DataId(2));
    assert_eq!(pal_set_palette_id(&set, 0.75), DataId(3));
    assert_eq!(
        pal_set_palette_id(&set, 1.0),
        DataId(4),
        "the -0.000001 keeps 1.0 in range"
    );
    // Invalid shades or an empty set return the invalid id 0; composition can retain it.
    assert_eq!(
        pal_set_palette_id(&set, -1.0),
        DataId(0),
        "reset character state leaves shade -1"
    );
    assert_eq!(pal_set_palette_id(&set, 1.5), DataId(0));
    assert_eq!(pal_set_palette_id(&[], 0.5), DataId(0));
}

/// Build a clothing description from a synthetic table, checking part, texture and palette
/// effects plus shade selection without archive inputs.
#[test]
fn build_obj_desc_emits_the_part_texture_and_palette_changes() {
    use dereth_assets::motion::{
        ClothingTable, ObjectEffect, PaletteEffect, PaletteRange as CloRange, PaletteTemplate,
        TextureEffect,
    };
    let table = ClothingTable {
        id: DataId(0x1000_0001),
        clothing_base_buckets: 8,
        clothing_bases: [(
            DataId(0x0200_0001),
            vec![ObjectEffect {
                part_num: 9,
                object_id: DataId(0x0100_0AAA),
                texture_effects: vec![TextureEffect {
                    old_texture: DataId(0x0500_0001),
                    new_texture: DataId(0x0500_0002),
                }],
            }],
        )]
        .into_iter()
        .collect(),
        palette_template_buckets: 8,
        palette_templates: [(
            7,
            PaletteTemplate {
                icon: DataId(0),
                subpalette_effects: vec![PaletteEffect {
                    // 0x400 entries in, 0x40 long -- 128 and 8 in wire units.
                    ranges: vec![CloRange {
                        offset: 0x400,
                        length: 0x40,
                    }],
                    palette_set: DataId(0x0F00_0001),
                }],
            },
        )]
        .into_iter()
        .collect(),
    };
    let mut sets = |_: DataId| vec![DataId(0x0400_0010), DataId(0x0400_0011)];
    let mut od = ObjDesc::default();
    assert!(dereth_client::preview::build_obj_desc(
        &table,
        DataId(0x0200_0001),
        7,
        1.0,
        &mut sets,
        &mut od
    ));
    assert_eq!(
        od.part_changes,
        vec![AnimPartChange {
            part_index: 9,
            part_id: DataId(0x0100_0AAA)
        }]
    );
    assert_eq!(
        od.texture_changes,
        vec![TextureMapChange {
            part_index: 9,
            old_texture: DataId(0x0500_0001),
            new_texture: DataId(0x0500_0002),
        }]
    );
    assert_eq!(
        od.subpalettes,
        vec![PaletteRange {
            palette_set: DataId(0x0400_0011),
            offset: 0x400 / 8,
            length: 0x40 / 8,
        }],
        "the range is divided into the wire's 8-entry units and the shade picks the last palette"
    );

    // `template_key == 0` is "no dye": the object and texture changes only.
    let mut plain = ObjDesc::default();
    assert!(dereth_client::preview::build_obj_desc(
        &table,
        DataId(0x0200_0001),
        0,
        1.0,
        &mut sets,
        &mut plain
    ));
    assert!(plain.subpalettes.is_empty(), "no template key means no dye");
    assert_eq!(plain.part_changes.len(), 1);

    // This table lacks the requested setup. The client retries per-race/sex defaults; that
    // fallback is not implemented here, so this build reports failure without applying data.
    let mut miss = ObjDesc::default();
    assert!(!dereth_client::preview::build_obj_desc(
        &table,
        DataId(0x0200_9999),
        7,
        1.0,
        &mut sets,
        &mut miss
    ));
    assert_eq!(
        miss,
        ObjDesc::default(),
        "nothing is applied for a body the garment does not fit"
    );
}

/// Three description-list rules shared by merges and clothing builds: replace matching ranges,
/// let a full palette supersede narrower ranges, and key geometry/texture replacements correctly.
///
/// The shipped Aluvian-male subpalettes have distinct offset/length pairs and none covers the
/// full palette, so synthetic inputs below exercise the replacement and full-palette cases.
#[test]
fn the_three_objdesc_list_rules_are_the_clients() {
    use dereth_client::preview::{add_anim_part_change, add_subpalette, add_texture_map_change};
    let r = |offset, length| PaletteRange {
        palette_set: DataId(offset + 1),
        offset,
        length,
    };

    // Equal offset/length replaces the old range and appends the newcomer at the end.
    let mut od = ObjDesc::default();
    add_subpalette(&mut od, r(0x18, 8));
    add_subpalette(&mut od, r(0x20, 8));
    add_subpalette(
        &mut od,
        PaletteRange {
            palette_set: DataId(0xBEEF),
            offset: 0x18,
            length: 8,
        },
    );
    assert_eq!(
        od.subpalettes.len(),
        2,
        "the first entry was replaced, not appended to"
    );
    assert_eq!(
        od.subpalettes[1].palette_set,
        DataId(0xBEEF),
        "remove the matching subpalette then append: the replacement is last"
    );

    // A whole-palette range removes narrow ranges; once present, it blocks a later narrow range.
    let whole = PaletteRange {
        palette_set: DataId(0x77),
        offset: 0,
        length: 2048 / 8,
    };
    add_subpalette(&mut od, whole);
    assert_eq!(
        od.subpalettes,
        vec![whole],
        "the full-palette range replaced both"
    );
    add_subpalette(&mut od, r(0x28, 8));
    assert_eq!(
        od.subpalettes,
        vec![whole],
        "a narrower range cannot displace it"
    );

    // Geometry replacement keys only on part index; the latest change moves to the end.
    let mut od = ObjDesc::default();
    add_anim_part_change(
        &mut od,
        AnimPartChange {
            part_index: 9,
            part_id: DataId(1),
        },
    );
    add_anim_part_change(
        &mut od,
        AnimPartChange {
            part_index: 16,
            part_id: DataId(2),
        },
    );
    add_anim_part_change(
        &mut od,
        AnimPartChange {
            part_index: 9,
            part_id: DataId(3),
        },
    );
    assert_eq!(
        od.part_changes,
        vec![
            AnimPartChange {
                part_index: 16,
                part_id: DataId(2)
            },
            AnimPartChange {
                part_index: 9,
                part_id: DataId(3)
            },
        ]
    );

    // Texture replacement keys on both part index and old texture. Different original
    // textures on the same part survive independently.
    let mut od = ObjDesc::default();
    let t = |part, old, new| TextureMapChange {
        part_index: part,
        old_texture: DataId(old),
        new_texture: DataId(new),
    };
    add_texture_map_change(&mut od, t(9, 0x10, 0x20));
    add_texture_map_change(&mut od, t(9, 0x11, 0x21));
    add_texture_map_change(&mut od, t(9, 0x10, 0x99));
    assert_eq!(
        od.texture_changes,
        vec![t(9, 0x11, 0x21), t(9, 0x10, 0x99)],
        "only the entry with the same (part, old_tex_id) was replaced"
    );
}
