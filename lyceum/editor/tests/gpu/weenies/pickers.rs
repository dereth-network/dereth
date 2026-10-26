//! Behaviour: none (tooling: the world editor's Model and Icon pickers over the retail dats)
//! The weenie editor's Model and Icon pickers: a button opens a popout of pictures, its search
//! narrows them, and a picture clicked, the model chosen in Place or an id typed is the weenie's,
//! as one step, and the popout closes.
//! Fixture: the sample world's guide, an NPC of the pack's own, in a 1600 by 1000 window.

use dereth_primitives::DataId;
use lyceum_editor::app::{EditorApp, Page, Picker, Section};
use lyceum_foundry::content::{PropertyKind, SqlDoc};
use lyceum_foundry::edit::Weenie;
use lyceum_foundry::script::Action as Edit;

use crate::{capture, click, editor_with, gpu_lock, sample, shots, type_in};

/// The guide's model and icon, as the sample pack writes them.
const GRIBBLE: u32 = 0x0200_07DD;
const GRIBBLE_ICON: u32 = 0x0600_1035;

/// The sample's guide, open in the editor on its Basic page.
fn guide_open(ed: &mut EditorApp) -> Weenie {
    let guide = Weenie::Symbol("holtburg:guide".into());
    ed.open_editor(guide.clone());
    ed.editor.page = Page::Section(Section::Basic);
    for _ in 0..3 {
        ed.frame();
    }
    guide
}

/// Data id `id` of weenie `w`, as its file writes it now.
fn data_id(ed: &mut EditorApp, w: &Weenie, id: u32) -> Option<u32> {
    let (file, _) = ed.session.weenie_file(w).expect("its file");
    let doc = SqlDoc::parse(&ed.session.sql_text(&file).unwrap());
    doc.property(PropertyKind::DataId, id)
        .and_then(|v| lyceum_foundry::sql_ids::number(&lyceum_foundry::content::bare(&v)))
}

/// Picker `p`'s popout opened by a click on its button, drawn once it is laid out.
fn open(ed: &mut EditorApp, p: Picker) {
    let b = ed.editor.pickers.button(p).expect("its button");
    click(ed, (b.center().x, b.center().y));
    for _ in 0..2 {
        ed.frame();
    }
    assert_eq!(ed.editor.pickers.open, Some(p), "open");
}

fn cells(ed: &EditorApp) -> Vec<u32> {
    ed.editor.pickers.cells.iter().map(|(id, _)| *id).collect()
}

/// A click on the picture of `id` in the open popout.
fn choose(ed: &mut EditorApp, id: u32) {
    let r = ed
        .editor
        .pickers
        .cells
        .iter()
        .find(|(c, _)| *c == id)
        .map(|(_, r)| *r)
        .unwrap_or_else(|| panic!("{id:08X} shown"));
    click(ed, (r.center().x, r.center().y));
    ed.frame();
}

fn shot(ed: &mut EditorApp, dir: &std::path::Path, name: &str) {
    capture(ed, dir);
    std::fs::copy(dir.join("frame.png"), shots("pickers").join(name)).unwrap();
}

#[test]
fn a_weenies_model_is_chosen_from_pictures_found_by_search_from_place_or_by_id_each_as_one_step() {
    let _gpu = gpu_lock();
    let (dir, world) = sample("editor-model-picker");
    let mut ed = editor_with(&world, &["--size", "1600x1000"]);
    ed.wait_for_names().unwrap();
    let guide = guide_open(&mut ed);
    assert_eq!(data_id(&mut ed, &guide, 1), Some(GRIBBLE));
    // Opened on the guide's own, marked among every setup.
    open(&mut ed, Picker::Model);
    ed.wait_for_pictures();
    ed.frame();
    let shown = cells(&ed);
    assert!(shown.contains(&GRIBBLE), "its own shown: {}", shown.len());
    assert!(shown.iter().all(|id| id >> 24 == 0x02), "setups only");
    shot(&mut ed, dir.path(), "model.png");
    // Its search takes the keys: an id narrows it to that setup, and a click chooses it.
    let stone = 0x0200_040B;
    let search = ed.editor.pickers.search_field.expect("its search");
    type_in(&mut ed, search, "0200040b", false);
    assert_eq!(ed.editor.pickers.open, Some(Picker::Model), "still open");
    assert_eq!(cells(&ed), [stone]);
    shot(&mut ed, dir.path(), "model-search.png");
    choose(&mut ed, stone);
    assert_eq!(ed.editor.pickers.open, None, "closed");
    assert_eq!(data_id(&mut ed, &guide, 1), Some(stone));
    assert_eq!(ed.session.history(), (1, 0), "one step");
    ed.edit(&Edit::Undo).unwrap();
    assert_eq!(data_id(&mut ed, &guide, 1), Some(GRIBBLE), "undone as one");
    ed.frame();
    // The model chosen in Place, in one click.
    ed.model = Some(DataId(stone));
    open(&mut ed, Picker::Model);
    let place = ed
        .editor
        .pickers
        .place_button
        .expect("Place's model offered");
    click(&mut ed, (place.center().x, place.center().y));
    ed.frame();
    assert_eq!(ed.editor.pickers.open, None);
    assert_eq!(data_id(&mut ed, &guide, 1), Some(stone));
    // An id typed, for one no search shows.
    open(&mut ed, Picker::Model);
    let field = ed.editor.pickers.id_field.expect("its id field");
    type_in(&mut ed, field, "0x02000181", true);
    assert_eq!(ed.editor.pickers.open, None);
    assert_eq!(data_id(&mut ed, &guide, 1), Some(0x0200_0181));
    assert_eq!(ed.session.history(), (2, 0), "a step each");
    // The button shows the weenie's own.
    ed.frame();
    shot(&mut ed, dir.path(), "model-chosen.png");
}

#[test]
fn a_weenies_icon_is_chosen_from_those_weenies_use_or_from_every_icon_by_id_each_as_one_step() {
    let _gpu = gpu_lock();
    let (dir, world) = sample("editor-icon-picker");
    let mut ed = editor_with(&world, &["--size", "1600x1000"]);
    let guide = guide_open(&mut ed);
    assert_eq!(data_id(&mut ed, &guide, 8), Some(GRIBBLE_ICON));
    // The icons weenies use: the guide's alone in the sample, read off the frame.
    open(&mut ed, Picker::Icon);
    assert_eq!(cells(&ed), [GRIBBLE_ICON]);
    ed.wait_for_icons();
    ed.frame();
    assert!(ed.icon_shown(GRIBBLE_ICON), "read and held");
    let img = capture(&mut ed, dir.path());
    let r = ed.editor.pickers.cells[0].1;
    let colors: std::collections::BTreeSet<[u8; 3]> = (0..24)
        .flat_map(|y| (0..24).map(move |x| (x, y)))
        .map(|(x, y)| crate::pixel(&img, r.min.x + 7.0 + x as f32, r.min.y + 7.0 + y as f32))
        .collect();
    assert!(colors.len() > 20, "{} colors in the icon", colors.len());
    std::fs::copy(
        dir.path().join("frame.png"),
        shots("pickers").join("icon.png"),
    )
    .unwrap();
    // Found by the name of a weenie that uses it.
    let search = ed.editor.pickers.search_field.expect("its search");
    type_in(&mut ed, search, "gribble", false);
    assert_eq!(cells(&ed), [GRIBBLE_ICON]);
    // Show all: every other icon after it, by id, listed off the frame.
    ed.editor.pickers.search.clear();
    ed.frame();
    let all = ed.editor.pickers.show_all_button.expect("Show all");
    click(&mut ed, (all.center().x, all.center().y));
    assert_eq!(ed.editor.pickers.open, Some(Picker::Icon), "still open");
    ed.wait_for_icons();
    ed.frame();
    ed.wait_for_icons();
    ed.frame();
    let listed = ed.icons_listed().expect("listed");
    assert!(listed > 1000, "{listed} icons");
    let shown = cells(&ed);
    assert_eq!(shown[0], GRIBBLE_ICON, "the used first");
    assert!(shown[1..].windows(2).all(|w| w[0] < w[1]), "then by id");
    assert!(
        shown.len() >= 50,
        "the grid stands its full height: {}",
        shown.len()
    );
    shot(&mut ed, dir.path(), "icon-show-all.png");
    // One of them by its id, chosen with a click.
    let other = shown[1];
    let search = ed.editor.pickers.search_field.expect("its search");
    type_in(&mut ed, search, &format!("{other:08x}"), false);
    assert_eq!(cells(&ed), [other]);
    choose(&mut ed, other);
    assert_eq!(ed.editor.pickers.open, None, "closed");
    assert_eq!(data_id(&mut ed, &guide, 8), Some(other));
    assert_eq!(ed.session.history(), (1, 0), "one step");
    ed.wait_for_icons();
    ed.frame();
    shot(&mut ed, dir.path(), "icon-chosen.png");
    ed.edit(&Edit::Undo).unwrap();
    assert_eq!(
        data_id(&mut ed, &guide, 8),
        Some(GRIBBLE_ICON),
        "undone as one"
    );
}
