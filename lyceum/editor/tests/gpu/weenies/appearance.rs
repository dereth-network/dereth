//! Behaviour: none (tooling: the weenie editor's preview of a weenie's look over the retail dats)
//! The weenie editor's head draws its weenie as the game shows it: its setup in the description a
//! server sends for it, applied as the client applies a server's. A changed color scheme, shade or
//! clothing look redraws it; the shade's slider is followed as it is dragged and written when let
//! go; a scheme and a clothing look are chosen from pictures. A clothing look is shown by its name
//! and id. What a creature wields is colored row by row, and the preview follows. A known weenie's
//! preview is the client's own drawing of the description the retail server sent for it.
//! Fixture: a server database of a shirt, a creature and a vendor dressed in its create list (the
//! world database's rows for them), a guard of its own, and the recorded sessions' messages.

use std::sync::Arc;

use dereth_primitives::DataId;
use dereth_rules::appearance::LookFiles;
use lyceum_editor::app::{EditorApp, FormEdit, LookPicker, Page, Section, Tab, TableRow};
use lyceum_editor::look::DatFiles;
use lyceum_foundry::content::{PropertyKind, SqlDoc};
use lyceum_foundry::edit::Weenie;

use crate::{capture, change_in_world, click, drawn, editor_with, gpu_lock, sample, shots};

/// A weenie's rows: its class, name, weenie type, ints, floats and data ids.
fn weenie(
    class: u32,
    name: &str,
    ty: u32,
    ints: &[(u32, i64)],
    floats: &[(u32, f64)],
    dids: &[(u32, u32)],
) -> String {
    let rows = |v: Vec<String>| v.join(", ");
    let mut s = format!(
        "INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES ({class}, 'w{class}', {ty}, '2005-02-09 10:00:00');
INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES ({class}, 1, '{name}');
"
    );
    if !ints.is_empty() {
        s.push_str(&format!(
            "INSERT INTO `weenie_properties_int` (`object_Id`, `type`, `value`)\nVALUES {};\n",
            rows(
                ints.iter()
                    .map(|(t, v)| format!("({class}, {t}, {v})"))
                    .collect()
            )
        ));
    }
    if !floats.is_empty() {
        s.push_str(&format!(
            "INSERT INTO `weenie_properties_float` (`object_Id`, `type`, `value`)\nVALUES {};\n",
            rows(
                floats
                    .iter()
                    .map(|(t, v)| format!("({class}, {t}, {v})"))
                    .collect()
            )
        ));
    }
    s.push_str(&format!(
        "INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`)\nVALUES {};\n",
        rows(
            dids.iter()
                .map(|(t, v)| format!("({class}, {t}, {v})"))
                .collect()
        )
    ));
    s
}

const SHIRT: u32 = 130;
const SHIRT_TABLE: u32 = 0x1000_0001;
const BANDERLING: u32 = 184;
const GROCER: u32 = 714;
const GUARD: u32 = 90_716;

/// The server's database: a shirt, the pieces the grocer wears, the grocer, and a banderling
/// captain, each with its look as the world database has it.
fn server_sql() -> String {
    let piece = |class: u32,
                 name: &str,
                 (item, coverage, at, armor): (i64, i64, i64, i64),
                 (setup, table): (u32, u32),
                 (scheme, shade): (i64, f64)| {
        weenie(
            class,
            name,
            2,
            &[(1, item), (3, scheme), (4, coverage), (9, at), (28, armor)],
            &[(12, shade)],
            &[(1, setup), (6, 0x0400_007E), (7, table)],
        )
    };
    let mut s = String::new();
    s.push_str(&piece(
        SHIRT,
        "Shirt",
        (4, 104, 30, 0),
        (0x0200_00D4, SHIRT_TABLE),
        (8, 0.66),
    ));
    s.push_str(&piece(
        127,
        "Pants",
        (4, 22, 196, 0),
        (0x0200_00DD, 0x1000_0002),
        (14, 0.33),
    ));
    s.push_str(&piece(
        132,
        "Shoes",
        (4, 65536, 256, 20),
        (0x0200_00DE, 0x1000_001F),
        (4, 0.6),
    ));
    s.push_str(&piece(
        118,
        "Cap",
        (4, 16384, 1, 20),
        (0x0200_00D3, 0x1000_0009),
        (4, 0.66),
    ));
    s.push_str(&piece(
        10696,
        "Apron",
        (4, 7168, 3584, 0),
        (0x0200_01A6, 0x1000_0059),
        (4, 0.8),
    ));
    s.push_str(&weenie(
        BANDERLING,
        "Banderling Captain",
        10,
        &[(1, 16), (3, 17)],
        &[(12, 0.5), (39, 1.7)],
        &[(1, 0x0200_0E08), (6, 0x0400_1425), (7, 0x1000_0412)],
    ));
    s.push_str(&weenie(
        GROCER,
        "Fispur Ansel the Grocer",
        12,
        &[(1, 16)],
        &[],
        &[(1, 0x0200_0001)],
    ));
    s.push_str(&format!(
        "INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES ({GROCER}, 3, 'Male'), ({GROCER}, 4, 'Aluvian');
INSERT INTO `weenie_properties_create_list` (`object_Id`, `destination_Type`, `weenie_Class_Id`, `stack_Size`, `palette`, `shade`, `try_To_Bond`)
VALUES ({GROCER}, 2, 130, 0, 8, 0.67, False), ({GROCER}, 2, 127, 0, 7, 0.33, False), ({GROCER}, 2, 132, 0, 18, 1, False), ({GROCER}, 2, 118, 0, 8, 0, False), ({GROCER}, 2, 10696, 0, 9, 0.5, False);
"
    ));
    // A guard of the editor's own, who cannot be attacked: the grocer's shirt and pants always,
    // and his shoes half the time.
    s.push_str(&weenie(
        GUARD,
        "Town Guard",
        10,
        &[(1, 16)],
        &[],
        &[(1, 0x0200_0001)],
    ));
    s.push_str(&format!(
        "INSERT INTO `weenie_properties_bool` (`object_Id`, `type`, `value`)
VALUES ({GUARD}, 19, False);
INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES ({GUARD}, 3, 'Male'), ({GUARD}, 4, 'Aluvian');
INSERT INTO `weenie_properties_create_list` (`object_Id`, `destination_Type`, `weenie_Class_Id`, `stack_Size`, `palette`, `shade`, `try_To_Bond`)
VALUES ({GUARD}, 2, 130, 0, 8, 0.67, False), ({GUARD}, 2, 127, 0, 7, 0.33, False), ({GUARD}, 10, 132, 0, 0, 0.5, False);
"
    ));
    s
}

/// The server's names of the look's properties and of the first color schemes.
const SCHEMA: &str = r#"{"properties":{"int":{"1":"ItemType","3":"PaletteTemplate","4":"ClothingPriority","9":"ValidLocations","28":"ArmorLevel"},"bool":{"19":"Attackable"},"float":{"12":"Shade","39":"DefaultScale"},"string":{"1":"Name","3":"Sex","4":"HeritageGroup"},"did":{"1":"Setup","6":"PaletteBase","7":"ClothingBase"}},"enums":{"PaletteTemplate":{"1":"AquaBlue","2":"Blue","3":"BluePurple","4":"Brown","5":"DarkBlue","6":"DeepBrown","7":"DeepGreen","8":"Green","9":"Grey","10":"LightBlue","11":"Maroon","12":"Navy","13":"Purple","14":"Red","15":"RedPurple","16":"Rose","17":"Yellow","18":"YellowBrown","19":"Copper","20":"Silver","21":"Gold"}}}"#;

/// The editor over the sample world with the server database above, in a 1600 by 1000 window.
fn editor_on_server(tag: &str) -> (dereth_dat::testing::ScratchDir, EditorApp) {
    let (dir, world) = sample(tag);
    let sql = dir.path().join("server.sql");
    std::fs::write(&sql, server_sql()).unwrap();
    let schema = dir.path().join("schema.json");
    std::fs::write(&schema, SCHEMA).unwrap();
    let ed = editor_with(
        &world,
        &[
            "--weenies",
            sql.to_str().unwrap(),
            "--schema",
            schema.to_str().unwrap(),
            "--size",
            "1600x1000",
        ],
    );
    (dir, ed)
}

/// Weenie `w` open in the editor on its Appearance page, its preview still and framed.
fn open_still(ed: &mut EditorApp, w: &Weenie) {
    ed.open_editor(w.clone());
    ed.editor.page = Page::Section(Section::Appearance);
    ed.appearance.turning = false;
    ed.appearance.heading = 0.0;
    ed.appearance.zoom = 1.0;
    for _ in 0..4 {
        ed.frame();
    }
}

/// The preview's pixels, as the frame drawn last shows them: `(side, rgb)`.
fn preview_pixels(ed: &mut EditorApp, dir: &std::path::Path) -> (usize, Vec<[u8; 3]>) {
    let r = ed.appearance.rect.expect("the preview is drawn");
    let (w, _, px) = capture(ed, dir);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (x0, y0, side) = (
        r.min.x.round() as usize,
        r.min.y.round() as usize,
        r.width().round() as usize,
    );
    let mut out = Vec::with_capacity(side * side);
    for y in y0..y0 + side {
        for x in x0..x0 + side {
            let i = (y * w + x) * 4;
            out.push([px[i], px[i + 1], px[i + 2]]);
        }
    }
    (side, out)
}

/// How many of the pixels of `a` and `b` differ by more than a little.
fn differing(a: &[[u8; 3]], b: &[[u8; 3]]) -> usize {
    a.iter()
        .zip(b)
        .filter(|(p, q)| p.iter().zip(q.iter()).any(|(x, y)| x.abs_diff(*y) > 8))
        .count()
}

/// Its own pixels drawn, not black.
fn lit(a: &[[u8; 3]]) -> usize {
    a.iter().filter(|p| p.iter().any(|c| *c > 16)).count()
}

/// Property `id` of `kind` of weenie `w`'s file, as written.
fn written(ed: &mut EditorApp, w: &Weenie, kind: PropertyKind, id: u32) -> Option<String> {
    let (file, _) = ed.session.weenie_file(w).expect("its file");
    SqlDoc::parse(&ed.session.sql_text(&file).unwrap()).property(kind, id)
}

#[test]
fn the_preview_draws_the_weenies_look_and_redraws_it_for_a_new_color_scheme_shade_or_clothing() {
    let _gpu = gpu_lock();
    let (dir, mut ed) = editor_on_server("editor-look-preview");
    ed.server_class_now(SHIRT).unwrap().expect("its rows");
    change_in_world(&mut ed, SHIRT);
    let shirt = Weenie::Class(SHIRT);
    open_still(&mut ed, &shirt);
    let drawn = ed.appearance.drawn().cloned().expect("drawn");
    assert_eq!(drawn.setup, DataId(0x0200_00D4));
    assert!(!drawn.objdesc.subpalettes.is_empty(), "in its colors");
    let (side, before) = preview_pixels(&mut ed, dir.path());
    assert!(lit(&before) > side * side / 20, "the shirt is drawn");
    std::fs::copy(
        dir.path().join("frame.png"),
        shots("weenie-look").join("shirt.png"),
    )
    .unwrap();

    // Another of its schemes, whose colors differ at its shade.
    let files = DatFiles::new(Arc::new(dereth_dat::testing::open_store_or_fail()));
    let table = files.clothing(SHIRT_TABLE).expect("the shirt's table");
    let own = lyceum_editor::look::scheme_colors(&files, SHIRT_TABLE, 8, 0.66);
    let other = *table
        .palette_templates
        .keys()
        .find(|k| lyceum_editor::look::scheme_colors(&files, SHIRT_TABLE, **k, 0.66) != own)
        .expect("a scheme of other colors");
    let builds = ed.appearance.builds;
    ed.form_edit(
        &shirt,
        &FormEdit::Property {
            kind: PropertyKind::Int,
            id: 3,
            value: Some(other.to_string()),
        },
    )
    .unwrap();
    for _ in 0..2 {
        ed.frame();
    }
    assert!(ed.appearance.builds > builds, "built again");
    let (_, recolored) = preview_pixels(&mut ed, dir.path());
    assert!(
        differing(&before, &recolored) > side * side / 50,
        "the new scheme shows: {} pixels",
        differing(&before, &recolored)
    );
    std::fs::copy(
        dir.path().join("frame.png"),
        shots("weenie-look").join("shirt-other-scheme.png"),
    )
    .unwrap();

    // A darker shade.
    ed.form_edit(
        &shirt,
        &FormEdit::Property {
            kind: PropertyKind::Float,
            id: 12,
            value: Some("0".into()),
        },
    )
    .unwrap();
    for _ in 0..2 {
        ed.frame();
    }
    let (_, darker) = preview_pixels(&mut ed, dir.path());
    assert!(
        differing(&recolored, &darker) > side * side / 50,
        "the new shade shows"
    );

    // No clothing look: the bare model, in no colors of the table's.
    ed.form_edit(
        &shirt,
        &FormEdit::Property {
            kind: PropertyKind::DataId,
            id: 7,
            value: None,
        },
    )
    .unwrap();
    for _ in 0..2 {
        ed.frame();
    }
    let bare = ed.appearance.drawn().cloned().expect("drawn");
    assert!(bare.objdesc.subpalettes.is_empty() && bare.objdesc.part_changes.is_empty());
    let (_, plain) = preview_pixels(&mut ed, dir.path());
    assert!(
        differing(&darker, &plain) > side * side / 50,
        "the bare model shows"
    );
}

#[test]
fn the_shade_slider_is_followed_as_it_is_dragged_and_written_as_one_step_when_let_go() {
    let _gpu = gpu_lock();
    let (dir, mut ed) = editor_on_server("editor-look-shade");
    ed.server_class_now(SHIRT).unwrap().expect("its rows");
    change_in_world(&mut ed, SHIRT);
    let shirt = Weenie::Class(SHIRT);
    open_still(&mut ed, &shirt);
    let slider = ed.appearance.slider.expect("the shade's slider");
    let (_, at_own) = preview_pixels(&mut ed, dir.path());
    let steps = ed.session.history().0;
    // Pressed at its left end and dragged: the preview follows, nothing is written yet.
    let y = slider.center().y;
    let left = (slider.min.x + 2.0, y);
    let mv = |ed: &mut EditorApp, at: (f32, f32)| {
        ed.queue(dereth_input::host::HostEvent::CursorMoved {
            x: f64::from(at.0),
            y: f64::from(at.1),
        });
    };
    mv(&mut ed, left);
    ed.frame();
    ed.queue(dereth_input::host::HostEvent::MouseInput {
        button: dereth_input::keys::MouseButton::Left,
        pressed: true,
    });
    ed.frame();
    mv(&mut ed, (slider.min.x + 30.0, y));
    ed.frame();
    mv(&mut ed, (slider.min.x + 6.0, y));
    ed.frame();
    ed.frame();
    let held = ed
        .appearance
        .shade_held
        .clone()
        .expect("held while dragged");
    assert!(held.1 < 0.2, "{held:?}");
    assert_eq!(
        ed.session.history().0,
        steps,
        "nothing written while dragged"
    );
    assert_eq!(
        written(&mut ed, &shirt, PropertyKind::Float, 12).as_deref(),
        Some("0.66")
    );
    let (_, dragged) = preview_pixels(&mut ed, dir.path());
    assert!(
        differing(&at_own, &dragged) > 0,
        "the preview follows the slider"
    );
    // Let go: one step.
    ed.queue(dereth_input::host::HostEvent::MouseInput {
        button: dereth_input::keys::MouseButton::Left,
        pressed: false,
    });
    ed.frame();
    ed.frame();
    assert!(ed.appearance.shade_held.is_none());
    assert_eq!(ed.session.history().0, steps + 1, "one step");
    let shade: f64 = written(&mut ed, &shirt, PropertyKind::Float, 12)
        .unwrap()
        .parse()
        .unwrap();
    assert!((shade - held.1).abs() < 1e-9, "{shade} {held:?}");
}

#[test]
fn a_color_scheme_and_a_clothing_look_are_chosen_from_pictures_each_as_one_step() {
    let _gpu = gpu_lock();
    let (dir, mut ed) = editor_on_server("editor-look-pickers");
    ed.server_class_now(SHIRT).unwrap().expect("its rows");
    change_in_world(&mut ed, SHIRT);
    let shirt = Weenie::Class(SHIRT);
    open_still(&mut ed, &shirt);
    // The schemes, each by its number, the weenie's own among them.
    let b = ed
        .appearance
        .button(LookPicker::Scheme)
        .expect("its button");
    click(&mut ed, (b.center().x, b.center().y));
    for _ in 0..2 {
        ed.frame();
    }
    assert_eq!(ed.appearance.open, Some(LookPicker::Scheme));
    let schemes: Vec<u32> = ed.appearance.cells.iter().map(|(k, _)| *k).collect();
    assert!(schemes.contains(&8) && schemes.len() > 4, "{schemes:?}");
    for _ in 0..30 {
        ed.frame();
    }
    capture(&mut ed, dir.path());
    std::fs::copy(
        dir.path().join("frame.png"),
        shots("weenie-look").join("scheme-picker.png"),
    )
    .unwrap();
    let pick = *schemes.iter().find(|k| **k != 8).unwrap();
    let r = ed
        .appearance
        .cells
        .iter()
        .find(|(k, _)| *k == pick)
        .unwrap()
        .1;
    let steps = ed.session.history().0;
    click(&mut ed, (r.center().x, r.center().y));
    ed.frame();
    assert_eq!(
        written(&mut ed, &shirt, PropertyKind::Int, 3),
        Some(pick.to_string())
    );
    assert_eq!(ed.session.history().0, steps + 1, "one step");
    assert_eq!(ed.appearance.open, None, "closed on the choice");

    // The clothing that fits its model, its own marked, and none.
    ed.wait_for_clothing();
    let b = ed
        .appearance
        .button(LookPicker::Clothing)
        .expect("its button");
    click(&mut ed, (b.center().x, b.center().y));
    for _ in 0..3 {
        ed.frame();
    }
    assert_eq!(ed.appearance.open, Some(LookPicker::Clothing));
    let tables: Vec<u32> = ed.appearance.cells.iter().map(|(k, _)| *k).collect();
    assert!(
        tables.contains(&SHIRT_TABLE) && tables.contains(&0),
        "{tables:08X?}"
    );
    // The pictures are drawn on the side, a few a frame.
    for _ in 0..20 {
        ed.frame();
    }
    capture(&mut ed, dir.path());
    std::fs::copy(
        dir.path().join("frame.png"),
        shots("weenie-look").join("clothing-picker.png"),
    )
    .unwrap();
    let r = ed.appearance.cells.iter().find(|(k, _)| *k == 0).unwrap().1;
    click(&mut ed, (r.center().x, r.center().y));
    ed.frame();
    assert_eq!(written(&mut ed, &shirt, PropertyKind::DataId, 7), None);
}

#[test]
fn a_clothing_look_is_shown_by_its_name_and_id_in_the_definition_pane() {
    let _gpu = gpu_lock();
    let (dir, mut ed) = editor_on_server("editor-look-definition");
    ed.server_class_now(SHIRT).unwrap().expect("its rows");
    let shirt = Weenie::Class(SHIRT);
    ed.selection = None;
    ed.panels.weenie = Some(shirt.clone());
    ed.editor.open = false;
    ed.show_tab(Tab::Definition);
    settle(&mut ed);
    let words = ed
        .definition
        .words
        .iter()
        .find(|(k, _)| *k == "clothing_base")
        .map(|(_, v)| v.clone());
    // The one weenie dressed in it names it.
    assert_eq!(words.as_deref(), Some("Shirt (0x10000001)"));
    capture(&mut ed, dir.path());
    std::fs::copy(
        dir.path().join("frame.png"),
        shots("weenie-look").join("definition-clothing.png"),
    )
    .unwrap();
    // The editor's picker says the same.
    change_in_world(&mut ed, SHIRT);
    open_still(&mut ed, &shirt);
    settle(&mut ed);
    capture(&mut ed, dir.path());
    std::fs::copy(
        dir.path().join("frame.png"),
        shots("weenie-look").join("clothing-button.png"),
    )
    .unwrap();
}

/// The frames drawn until every picture they ask for is in, so nothing moves under a click.
fn settle(ed: &mut EditorApp) {
    for _ in 0..4 {
        ed.frame();
        ed.wait_for_pictures();
    }
    for _ in 0..10 {
        ed.frame();
    }
}

/// The rows of weenie `w`'s create list, as its file writes them.
fn create_list(ed: &mut EditorApp, w: &Weenie) -> Vec<TableRow> {
    let (file, _) = ed.session.weenie_file(w).expect("its file");
    let doc = SqlDoc::parse(&ed.session.sql_text(&file).unwrap());
    lyceum_editor::app::table_rows(&doc, "weenie_properties_create_list")
}

#[test]
fn an_equipped_rows_color_and_shade_are_each_chosen_as_one_step_and_the_preview_follows() {
    let _gpu = gpu_lock();
    let (dir, mut ed) = editor_on_server("editor-look-equipped");
    for c in [GUARD, SHIRT, 127, 132] {
        ed.server_class_now(c).unwrap().expect("its rows");
    }
    change_in_world(&mut ed, GUARD);
    let guard = Weenie::Class(GUARD);
    open_still(&mut ed, &guard);
    ed.editor.page = Page::Section(Section::Creature);
    settle(&mut ed);
    let shoot = |ed: &mut EditorApp, name: &str| {
        capture(ed, dir.path());
        std::fs::copy(
            dir.path().join("frame.png"),
            shots("weenie-look").join(name),
        )
        .unwrap();
    };
    shoot(&mut ed, "equipped.png");
    // The shirt's row has its color and shade; the shoes' treasure row its chance and color, and
    // no shade, as its shade is its chance.
    let has = |ed: &EditorApp, k: &str| ed.form_fields.iter().any(|(f, _)| f == k);
    assert!(has(&ed, "list:wields:0:palette") && has(&ed, "list:wields:0:shade"));
    assert!(has(&ed, "list:wields:2:chance") && has(&ed, "list:wields:2:palette"));
    assert!(
        !has(&ed, "list:wields:2:shade"),
        "no shade on a treasure row"
    );
    let (side, before) = preview_pixels(&mut ed, dir.path());
    assert!(lit(&before) > side * side / 20, "the guard is drawn");
    let colors = |ed: &EditorApp| {
        ed.appearance
            .drawn()
            .expect("drawn")
            .objdesc
            .subpalettes
            .clone()
    };
    let dressed = colors(&ed);

    // Another of the shirt's schemes, whose colors differ at the row's shade.
    let files = DatFiles::new(Arc::new(dereth_dat::testing::open_store_or_fail()));
    let table = files.clothing(SHIRT_TABLE).expect("the shirt's table");
    let own = lyceum_editor::look::scheme_colors(&files, SHIRT_TABLE, 8, 0.67);
    let other = *table
        .palette_templates
        .keys()
        .find(|k| lyceum_editor::look::scheme_colors(&files, SHIRT_TABLE, **k, 0.67) != own)
        .expect("a scheme of other colors");
    let combo = drawn(&ed.form_fields, "list:wields:0:palette");
    click(&mut ed, (combo.center().x, combo.center().y));
    for _ in 0..30 {
        ed.frame();
    }
    shoot(&mut ed, "equipped-colors.png");
    let entry = drawn(
        &ed.editor.picker_rows,
        &format!("list:wields:0:palette:{other}"),
    );
    let steps = ed.session.history().0;
    let builds = ed.appearance.builds;
    click(&mut ed, (entry.center().x, entry.center().y));
    ed.frame();
    let rows = create_list(&mut ed, &guard);
    assert_eq!(rows[0].number("palette"), Some(f64::from(other)));
    assert_eq!(rows[0].number("shade"), Some(0.67), "its shade kept");
    assert_eq!(ed.session.history().0, steps + 1, "one step");
    for _ in 0..2 {
        ed.frame();
    }
    assert!(ed.appearance.builds > builds, "built again");
    let recolor = colors(&ed);
    assert_ne!(recolor, dressed, "dressed in the new colors");
    let (_, recolored) = preview_pixels(&mut ed, dir.path());
    assert!(
        differing(&before, &recolored) > side * side / 200,
        "the new colors show: {} pixels",
        differing(&before, &recolored)
    );
    shoot(&mut ed, "equipped-recolored.png");

    // Its shade dragged toward dark: the preview follows, and letting go writes one step.
    let slider = drawn(&ed.form_fields, "list:wields:0:shade");
    let steps = ed.session.history().0;
    let y = slider.center().y;
    let mv = |ed: &mut EditorApp, x: f32| {
        ed.queue(dereth_input::host::HostEvent::CursorMoved {
            x: f64::from(x),
            y: f64::from(y),
        });
    };
    mv(&mut ed, slider.min.x + 2.0);
    ed.frame();
    ed.queue(dereth_input::host::HostEvent::MouseInput {
        button: dereth_input::keys::MouseButton::Left,
        pressed: true,
    });
    ed.frame();
    mv(&mut ed, slider.min.x + 30.0);
    ed.frame();
    mv(&mut ed, slider.min.x + 4.0);
    ed.frame();
    ed.frame();
    let held = ed
        .appearance
        .row_shade_held
        .clone()
        .expect("held while dragged");
    assert!(held.0 == guard && held.2 < 0.2, "{held:?}");
    assert_eq!(
        ed.session.history().0,
        steps,
        "nothing written while dragged"
    );
    assert_ne!(colors(&ed), recolor, "dressed at the shade held");
    let (_, dragged) = preview_pixels(&mut ed, dir.path());
    assert!(
        differing(&recolored, &dragged) > 0,
        "the preview follows the slider"
    );
    ed.queue(dereth_input::host::HostEvent::MouseInput {
        button: dereth_input::keys::MouseButton::Left,
        pressed: false,
    });
    ed.frame();
    ed.frame();
    assert!(ed.appearance.row_shade_held.is_none());
    assert_eq!(ed.session.history().0, steps + 1, "one step");
    let rows = create_list(&mut ed, &guard);
    let shade = rows[0].number("shade").unwrap();
    assert!((shade - held.2).abs() < 1e-9, "{shade} {held:?}");
    assert_eq!(rows[0].number("palette"), Some(f64::from(other)));
    // The shoes' chance is as it was.
    assert_eq!(rows[2].number("shade"), Some(0.5));
    for _ in 0..2 {
        ed.frame();
    }
    let (_, darker) = preview_pixels(&mut ed, dir.path());
    assert!(
        differing(&recolored, &darker) > side * side / 200,
        "the new shade shows: {} pixels",
        differing(&recolored, &darker)
    );
    shoot(&mut ed, "equipped-darker.png");
}

#[test]
fn a_vendor_is_drawn_in_what_its_create_list_wields() {
    let _gpu = gpu_lock();
    let (dir, mut ed) = editor_on_server("editor-look-vendor");
    for c in [GROCER, SHIRT, 127, 132, 118, 10696] {
        ed.server_class_now(c).unwrap().expect("its rows");
    }
    let grocer = Weenie::Class(GROCER);
    open_still(&mut ed, &grocer);
    let drawn = ed.appearance.drawn().cloned().expect("drawn");
    assert_eq!(drawn.setup, DataId(0x0200_0001));
    // The shirt's colors from the grocer's row (scheme 8, shade 0.67), among the pieces'.
    let files = DatFiles::new(Arc::new(dereth_dat::testing::open_store_or_fail()));
    let shirt = rules_shirt_palette(&files);
    assert!(
        drawn
            .objdesc
            .subpalettes
            .iter()
            .any(|s| s.palette_set == DataId(shirt)),
        "{:?}",
        drawn.objdesc.subpalettes
    );
    // Every body part has a model: the pieces' or its own.
    let parts: std::collections::BTreeSet<u32> = drawn
        .objdesc
        .part_changes
        .iter()
        .map(|p| p.part_index)
        .collect();
    assert!((0..16).all(|p| parts.contains(&p)), "{parts:?}");
    for _ in 0..10 {
        ed.frame();
    }
    let (side, px) = preview_pixels(&mut ed, dir.path());
    assert!(lit(&px) > side * side / 20, "the grocer is drawn");
    std::fs::copy(
        dir.path().join("frame.png"),
        shots("weenie-look").join("grocer.png"),
    )
    .unwrap();
}

/// The palette the grocer's shirt row (scheme 8 of the shirt's table, shade 0.67) dresses it in.
fn rules_shirt_palette(files: &DatFiles) -> u32 {
    let table = files.clothing(SHIRT_TABLE).unwrap();
    let scheme = &table.palette_templates[&8];
    let set = files.palette_set(scheme.subpalette_effects[0].palette_set.0);
    0x0400_0000 | (dereth_rules::appearance::palette_at(&set, f64::from(0.67f32)) & 0xFFFF)
}

/// The description the retail server sent for class `class` in the recorded sessions.
fn retail_description(class: u32) -> dereth_protocol::types::ObjDesc {
    use dereth_protocol::Message;
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/message-corpus");
    for e in std::fs::read_dir(&root).expect("the recorded sessions' messages") {
        let Ok(text) = std::fs::read_to_string(e.unwrap().path().join("blobs.jsonl")) else {
            continue;
        };
        for line in text.lines() {
            let v: serde_json::Value = serde_json::from_str(line).unwrap();
            if v["dir"] != "s2c" || v["opcode"].as_u64() != Some(0xF745) {
                continue;
            }
            let hex = v["payload_hex"].as_str().unwrap();
            let bytes: Vec<u8> = (0..hex.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
                .collect();
            let mut r = dereth_protocol::archive::Reader::new(&bytes[4..]);
            if let Ok(c) = dereth_protocol::objects::ItemCreateObject::read(&mut r) {
                if c.0.wdesc.wcid == class {
                    return c.0.objdesc;
                }
            }
        }
    }
    panic!("class {class} is in the recorded sessions");
}

#[test]
fn a_known_weenies_preview_is_the_clients_drawing_of_what_the_retail_server_sent_for_it() {
    use dereth_render::camera::Viewport;
    use dereth_render::device::{Backend, DeviceConfig, Gpu};
    let _gpu = gpu_lock();
    let (dir, mut ed) = editor_on_server("editor-look-retail");
    ed.server_class_now(BANDERLING).unwrap().expect("its rows");
    let captain = Weenie::Class(BANDERLING);
    open_still(&mut ed, &captain);
    let drawn = ed.appearance.drawn().cloned().expect("drawn");
    let sent = retail_description(BANDERLING);
    let theirs = dereth_client_runtime::movement::to_anim_objdesc(&sent);
    assert_eq!(drawn.objdesc, theirs, "the retail server's description");
    let (side, ours) = preview_pixels(&mut ed, dir.path());
    std::fs::copy(
        dir.path().join("frame.png"),
        shots("weenie-look").join("banderling.png"),
    )
    .unwrap();

    // The client's preview of the same setup in that description, at the weenie's size, lit and
    // seen as the editor's is, on a device of its own.
    let store = Arc::new(dereth_dat::testing::open_store_or_fail());
    let backend = Backend::default_backend().expect("a backend");
    let size = u32::try_from(side).unwrap();
    let mut gpu = Gpu::new_on(
        backend,
        None,
        &DeviceConfig {
            width: size,
            height: size,
            force_software: false,
            ..DeviceConfig::default()
        },
    )
    .expect("a device");
    let assets = Arc::new(dereth_world_data::anim_assets::DatAnimAssets::new(
        Arc::clone(&store),
    ));
    let mut space = dereth_scene::preview::PreviewSpace::new(assets);
    space.set_light(
        dereth_world_render::lighting::LightType::Directional,
        lyceum_editor::thumbs::LIGHT,
        lyceum_editor::thumbs::LIGHT_DIRECTION,
    );
    space.mode.set_fov(lyceum_editor::thumbs::FOV);
    let i = space
        .add_object_dressed(&store, &mut gpu, DataId(0x0200_0E08), Some(&theirs))
        .unwrap()
        .expect("its setup");
    space.set_scale(i, 1.7);
    let bb = space.object(i).unwrap().bounding_box(&store);
    let (center, reach) = lyceum_editor::app::framing(bb.min, bb.max);
    space.set_heading(i, 0.0);
    space.use_time(0.0);
    space.mode.view_frame =
        lyceum_editor::thumbs::camera(center, reach, lyceum_editor::thumbs::FOV, 1.0);
    gpu.begin_frame().unwrap();
    space
        .draw(
            &mut gpu,
            Viewport {
                x: 0,
                y: 0,
                width: size,
                height: size,
            },
            None,
            lyceum_editor::thumbs::FOV,
        )
        .unwrap();
    gpu.end_frame().unwrap();
    let image = gpu.capture().unwrap();
    let client: Vec<[u8; 3]> = image
        .bgra
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| [p[2], p[1], p[0]])
        .collect();
    assert_eq!(client.len(), ours.len());
    assert!(lit(&client) > side * side / 20, "the client drew it");
    let off = differing(&ours, &client);
    assert!(
        off <= side * side / 200,
        "{off} of {} pixels differ from the client's drawing",
        side * side
    );
}
