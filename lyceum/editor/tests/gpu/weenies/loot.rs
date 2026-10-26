//! Behaviour: none (tooling: Lyceum's Loot tab over the retail dats)
//! The Loot tab lists every loot table of the world, searched by id or name; a table opened lists
//! the weenies that use it, a click on one showing it in the Definition pane and the editor; a
//! server's death treasure is changed in this world field by field and put back, and a new one is
//! made and deleted, each as one step, and copied under the name typed; a wielded treasure's chance is edited in percent and written
//! as the server reads it; a generator's treasure entry shows as the loot table it names, a click
//! opening it in the tab; and a creature's wielded loot field names its table, links to it, and
//! takes a pack's by its symbol.
//! Fixture: a server database of a drudge that drops death treasure 4 and wields 37, a generator
//! that makes a drudge and table 4 as treasure, death treasures 4 and 7 and wielded treasure 37; a
//! 1600 by 1000 window.

use dereth_primitives::Vec3;
use lyceum_editor::app::{Drill, EditorApp, FormEdit, LootAction, Page, Section, Selection, Tab};
use lyceum_foundry::content::PropertyKind;
use lyceum_foundry::edit::{Op, Weenie};
use lyceum_foundry::loot::{LootKind, Whose, NEW_ID};
use lyceum_foundry::script::Action as Edit;
use lyceum_foundry::sql_ids::LootUse;
use lyceum_foundry::wielded;

use crate::world::dungeon::keep;
use crate::{click, editor_with, gpu_lock, sample, type_in, HOLTBURG};

/// The server's: a drudge (dropping death treasure 4, made wielding 37), a generator of drudges
/// that makes table 4 as treasure, death treasures 4 and 7, and wielded treasure 37: a club 60% of
/// the time with ten darts when it is, then darts a quarter of the time.
const SERVER_LOOT: &str = "\
CREATE TABLE `weenie` (`class_Id` int, `class_Name` varchar(100), `type` int, `last_Modified` datetime);
INSERT INTO `weenie` VALUES (9001,'drudge',10,'2005-02-09 10:00:00'),(9002,'drudgegen',1,'2005-02-09 10:00:00'),(309,'club',6,'2005-02-09 10:00:00'),(310,'dart',3,'2005-02-09 10:00:00');
CREATE TABLE `weenie_properties_string` (`id` int, `object_Id` int, `type` smallint, `value` text);
INSERT INTO `weenie_properties_string` VALUES (1,9001,1,'Drudge Skulker'),(2,9002,1,'Drudge Generator'),(4,309,1,'Club'),(5,310,1,'Dart');
CREATE TABLE `weenie_properties_d_i_d` (`id` int, `object_Id` int, `type` smallint, `value` int);
INSERT INTO `weenie_properties_d_i_d` VALUES (1,9001,35,4),(2,9001,32,37),(3,9001,1,33554839),(4,309,1,33554839),(5,310,1,33554839);
CREATE TABLE `weenie_properties_generator` (`id` int, `object_Id` int, `probability` float, `weenie_Class_Id` int, `delay` float, `init_Create` int, `max_Create` int, `when_Create` int, `where_Create` int);
INSERT INTO `weenie_properties_generator` VALUES (1,9002,0.5,9001,180,1,1,1,2),(2,9002,1,4,180,1,1,1,72);
CREATE TABLE `treasure_death` (`id` int, `treasure_Type` int, `tier` int, `loot_Quality_Mod` float, `unknown_Chances` int, `item_Chance` int, `item_Min_Amount` int, `item_Max_Amount` int, `item_Treasure_Type_Selection_Chances` int, `magic_Item_Chance` int, `magic_Item_Min_Amount` int, `magic_Item_Max_Amount` int, `magic_Item_Treasure_Type_Selection_Chances` int, `mundane_Item_Chance` int, `mundane_Item_Min_Amount` int, `mundane_Item_Max_Amount` int, `mundane_Item_Type_Selection_Chances` int, `last_Modified` datetime);
INSERT INTO `treasure_death` VALUES (4,4,2,0,19,100,1,2,10,100,2,3,9,100,0,0,1,'2005-02-09 10:00:00'),(7,7,4,0.25,19,100,0,0,1,100,3,5,9,100,0,0,1,'2005-02-09 10:00:00');
CREATE TABLE `treasure_wielded` (`id` int, `treasure_Type` int, `weenie_Class_Id` int, `palette_Id` int, `unknown_1` int, `shade` float, `stack_Size` int, `stack_Size_Variance` float, `probability` float, `unknown_3` int, `unknown_4` int, `unknown_5` int, `set_Start` bit(1), `has_Sub_Set` bit(1), `continues_Previous_Set` bit(1), `unknown_9` int, `unknown_10` int, `unknown_11` int, `unknown_12` int, `last_Modified` datetime);
INSERT INTO `treasure_wielded` VALUES (1,37,309,0,0,0,0,0,0.6,0,0,0,'\x01','\x01','\\0',0,0,0,0,'2005-02-09 10:00:00'),(2,37,310,0,0,0,10,0.2,1,0,0,0,'\x01','\\0','\\0',0,0,0,0,'2005-02-09 10:00:00'),(3,37,310,0,0,0,5,0,0.25,0,0,0,'\x01','\\0','\x01',0,0,0,0,'2005-02-09 10:00:00');
";

const DRUDGE: Weenie = Weenie::Class(9001);

/// The editor over the sample world with the server's rows above, the Loot tab in front.
fn editor_on_loot(tag: &str) -> (dereth_dat::testing::ScratchDir, EditorApp) {
    let (dir, world) = sample(tag);
    let sql = dir.path().join("server.sql");
    std::fs::write(&sql, SERVER_LOOT).unwrap();
    let mut ed = editor_with(
        &world,
        &["--weenies", sql.to_str().unwrap(), "--size", "1600x1000"],
    );
    ed.show_tab(Tab::Loot);
    for _ in 0..3 {
        ed.frame();
    }
    (dir, ed)
}

fn centre(r: egui::Rect) -> (f32, f32) {
    (r.center().x, r.center().y)
}

/// A click on the tab's button `what`, and a frame for what it did.
fn press(ed: &mut EditorApp, what: &str) {
    let r = ed
        .loot
        .button(what)
        .unwrap_or_else(|| panic!("{what} drawn"));
    click(ed, centre(r));
    ed.frame();
}

/// Table `id` of `kind` found by the search and opened by a click on its row.
fn open_table(ed: &mut EditorApp, kind: LootKind, id: u32) {
    if ed.loot.open.is_some() {
        press(ed, "back");
    }
    let search = ed.loot.search_field.expect("the search");
    type_in(ed, search, &id.to_string(), false);
    let r = ed
        .loot
        .row(kind, id)
        .unwrap_or_else(|| panic!("{kind:?} {id} listed"));
    click(ed, centre(r));
    ed.frame();
    assert_eq!(ed.loot.open, Some((kind, id)));
}

fn whose(ed: &EditorApp, kind: LootKind, id: u32) -> Option<Whose> {
    ed.session.loot_table(kind, id).map(|t| t.whose)
}

#[test]
fn the_tab_lists_every_table_and_a_servers_table_opened_lists_who_uses_it() {
    let _gpu = gpu_lock();
    let (_dir, mut ed) = editor_on_loot("loot-list");
    assert!(ed.tab_in_front(Tab::Loot));
    assert_eq!(ed.loot.rows.len(), 3, "death 4 and 7, wielded 37");
    keep(&mut ed, "loot", "01-list.png");
    // Found by the name of what uses it.
    let search = ed.loot.search_field.expect("the search");
    type_in(&mut ed, search, "generator", false);
    let rows: Vec<(LootKind, u32)> = ed.loot.rows.iter().map(|(k, _)| *k).collect();
    assert_eq!(rows, [(LootKind::Death, 4)]);
    let r = ed.loot.row(LootKind::Death, 4).unwrap();
    click(&mut ed, centre(r));
    ed.frame();
    assert_eq!(ed.loot.open, Some((LootKind::Death, 4)));
    // The drudge drops it, and the generator makes it as treasure.
    let used: Vec<Weenie> = ed.loot.use_rows.iter().map(|(w, _)| w.clone()).collect();
    assert_eq!(used, [Weenie::Class(9002), DRUDGE], "by name");
    // The server's: shown, not edited.
    assert!(ed.loot.field("item_Chance").is_none());
    assert!(ed.loot.button("change_here").is_some());
    keep(&mut ed, "loot", "02-death-uses.png");
    // A click on the drudge shows it in the Definition pane, asking how a server's weenie
    // changes; nothing changes yet.
    let r = ed
        .loot
        .use_rows
        .iter()
        .find(|(w, _)| *w == DRUDGE)
        .unwrap()
        .1;
    click(&mut ed, centre(r));
    for _ in 0..3 {
        ed.frame();
    }
    assert_eq!(ed.form_weenie(), Some(DRUDGE));
    assert!(ed.tab_in_front(Tab::Definition));
    assert_eq!(ed.session.history(), (0, 0));
    // Back to the list, the search kept.
    ed.show_tab(Tab::Loot);
    ed.frame();
    press(&mut ed, "back");
    assert!(ed.loot.open.is_none());
    assert_eq!(ed.loot.rows.len(), 1);
}

#[test]
fn a_servers_death_table_is_changed_in_this_world_and_put_back_and_a_new_one_made_and_deleted() {
    let _gpu = gpu_lock();
    let (_dir, mut ed) = editor_on_loot("loot-change");
    open_table(&mut ed, LootKind::Death, 4);
    press(&mut ed, "change_here");
    assert_eq!(
        whose(&ed, LootKind::Death, 4),
        Some(Whose::Changed { mine: true })
    );
    assert_eq!(ed.session.history(), (1, 0), "one step");
    // Its fields edited: the magic items' chance, typed.
    let field = ed
        .loot
        .field("magic_Item_Chance")
        .expect("the pack's to edit");
    type_in(&mut ed, field, "40", true);
    assert_eq!(ed.session.treasure(4).unwrap().magic.0, 40);
    assert_eq!(ed.session.history(), (2, 0));
    // A least past its most is refused, and said.
    let field = ed.loot.field("item_Min_Amount").unwrap();
    type_in(&mut ed, field, "5", true);
    assert!(ed.loot.note.is_some());
    assert_eq!(ed.session.history(), (2, 0), "nothing written");
    ed.frame();
    keep(&mut ed, "loot", "03-death-changed.png");
    ed.edit(&Edit::Undo).unwrap();
    assert_eq!(ed.session.treasure(4).unwrap().magic.0, 100);
    ed.frame();
    press(&mut ed, "put_back");
    assert_eq!(whose(&ed, LootKind::Death, 4), Some(Whose::Server));
    assert_eq!(ed.session.history(), (2, 0), "one step");
    ed.edit(&Edit::Undo).unwrap();
    assert_eq!(
        whose(&ed, LootKind::Death, 4),
        Some(Whose::Changed { mine: true })
    );
    ed.edit(&Edit::Undo).unwrap();
    assert_eq!(whose(&ed, LootKind::Death, 4), Some(Whose::Server));
    // A new one, plain: the pack's own, opened; deleted, as nothing uses it.
    ed.frame();
    press(&mut ed, "back");
    press(&mut ed, "new_death");
    assert_eq!(ed.loot.open, Some((LootKind::Death, NEW_ID)));
    assert_eq!(
        whose(&ed, LootKind::Death, NEW_ID),
        Some(Whose::Pack { mine: true })
    );
    press(&mut ed, "delete");
    assert!(ed.session.loot_table(LootKind::Death, NEW_ID).is_none());
    assert!(ed.loot.open.is_none());
    ed.edit(&Edit::Undo).unwrap();
    assert!(ed.session.loot_table(LootKind::Death, NEW_ID).is_some());
    // A copy, named as typed: the pack names it by that symbol, the build numbering it next.
    open_table(&mut ed, LootKind::Death, NEW_ID);
    let field = ed
        .loot
        .field("new_name")
        .expect("a name box beside the copy");
    type_in(&mut ed, field, "drudge-hoard", false);
    press(&mut ed, "copy");
    assert_eq!(ed.loot.open, Some((LootKind::Death, NEW_ID + 1)));
    let t = ed.session.loot_table(LootKind::Death, NEW_ID + 1).unwrap();
    assert_eq!(t.symbol.as_deref(), Some("{{treasure:drudge-hoard}}"));
    assert_eq!(ed.session.treasure(NEW_ID + 1), ed.session.treasure(NEW_ID));
    assert!(ed.loot.new_name.is_empty(), "the box emptied for the next");
    ed.frame();
    keep(&mut ed, "loot", "10-named-copy.png");
}

#[test]
fn a_wielded_sets_chance_is_edited_in_percent_and_written_as_the_server_reads_it() {
    let _gpu = gpu_lock();
    let (_dir, mut ed) = editor_on_loot("loot-wielded");
    open_table(&mut ed, LootKind::Wielded, 37);
    keep(&mut ed, "loot", "04-wielded-server.png");
    press(&mut ed, "change_here");
    let steps = ed.session.history().0;
    // The club, 60% of its set: made 80%.
    let field = ed.loot.field("0.0:chance").expect("the club's chance");
    type_in(&mut ed, field, "80", true);
    assert_eq!(ed.session.history().0, steps + 1, "one step");
    let rows = ed
        .session
        .loot_table(LootKind::Wielded, 37)
        .unwrap()
        .wielded()
        .unwrap()
        .to_vec();
    assert!((rows[0].probability - 0.8).abs() < 1e-6, "{rows:?}");
    let w = wielded::read(&rows);
    assert_eq!(
        w.sets[0].entries[0].then.len(),
        1,
        "the darts under it kept"
    );
    assert!(
        (w.sets[1].nothing() - 0.75).abs() < 1e-6,
        "the last set as it was"
    );
    // More than its set has room for: refused, and said.
    let field = ed.loot.field("1.0:chance").unwrap();
    type_in(&mut ed, field, "120", true);
    assert!(ed.loot.note.is_some());
    assert_eq!(ed.session.history().0, steps + 1);
    ed.frame();
    keep(&mut ed, "loot", "05-wielded-changed.png");
}

/// A generator of the pack's: it makes a guide, and death treasure 4 as treasure.
const GENERATOR: &str = "\
DELETE FROM `weenie` WHERE `class_Id` = {{wcid:gen}};

INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES ({{wcid:gen}}, 'gen', 1, '2026-10-25 00:00:00');

INSERT INTO `weenie_properties_float` (`object_Id`, `type`, `value`)
VALUES ({{wcid:gen}},  43,      10) /* GeneratorRadius */;

INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES ({{wcid:gen}},   1, 'Chest Generator') /* Name */;

INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`)
VALUES ({{wcid:gen}},   1, 0x020007DD) /* Setup */;

INSERT INTO `weenie_properties_generator` (`object_Id`, `probability`, `weenie_Class_Id`, `delay`, `init_Create`, `max_Create`, `when_Create`, `where_Create`, `stack_Size`, `palette_Id`, `shade`, `obj_Cell_Id`, `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`)
VALUES ({{wcid:gen}}, 0.5, {{wcid:guide}}, 180, 1, 1, 2, 2, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL)
     , ({{wcid:gen}}, 1, 4, 180, 1, 1, 1, 72, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL);
";

#[test]
fn a_generators_treasure_entry_shows_as_the_loot_table_it_names_and_opens_it_in_the_tab() {
    let _gpu = gpu_lock();
    let (dir, mut ed) = editor_on_loot("loot-generator");
    let file = dir.path().join("holtburg/server/sql/weenies/gen.sql");
    ed.session
        .apply(&Op::WriteSql {
            file,
            text: Some(GENERATOR.to_owned()),
        })
        .unwrap();
    let z = ed.session.ground(HOLTBURG, 60.0, 60.0).unwrap();
    ed.session
        .apply(&Op::PlaceSpawn {
            landblock: HOLTBURG,
            weenie: Weenie::Symbol("holtburg:gen".into()),
            origin: Vec3::new(60.0, 60.0, z),
            heading: 0.0,
        })
        .unwrap();
    ed.reload(&lyceum_foundry::edit::Touched {
        landblocks: [HOLTBURG].into_iter().collect(),
        sql: true,
    })
    .unwrap();
    let s = ed
        .session
        .spawns()
        .into_iter()
        .find(|s| s.weenie == "{{wcid:gen}}")
        .unwrap();
    ed.selection = Some(Selection::Spawn {
        file: s.file.clone(),
        guid: s.guid.clone(),
    });
    // The Definition pane opened into what it spawns.
    let gen = Weenie::Symbol("holtburg:gen".into());
    ed.panels.weenie = Some(gen.clone());
    ed.editor.open = false;
    ed.show_tab(Tab::Definition);
    ed.definition.drill_into(&gen, Drill::Spawns);
    for _ in 0..3 {
        ed.frame();
    }
    // Two entries: the guide, and the loot table, which no weenie stands for: not a class the
    // checks look for, nor a use of the weenie whose class id it is.
    assert_eq!(ed.panels.spawn_rows.len(), 2);
    assert!(ed.session.uses_of(&Weenie::Class(4)).is_empty());
    assert!(
        !ed.faults.iter().any(|f| f.text.contains("class 4")),
        "{:?}",
        ed.faults
    );
    keep(&mut ed, "loot", "06-generator-entry.png");
    let r = ed.panels.spawn_rows[1];
    click(&mut ed, centre(r));
    ed.frame();
    assert!(ed.tab_in_front(Tab::Loot));
    assert_eq!(ed.loot.open, Some((LootKind::Death, 4)));
    // The pack's generator counts among the table's uses.
    let t = ed.session.loot_table(LootKind::Death, 4).unwrap();
    assert!(t
        .uses
        .iter()
        .any(|u| u.by == Weenie::Symbol("holtburg:gen".into())));
    // The tab answers the same asked by name.
    ed.loot_action(LootAction::Open(None)).unwrap();
    assert!(ed.loot.open.is_none());
}

#[test]
fn a_creatures_wielded_loot_field_names_its_table_links_to_it_and_takes_a_packs_by_its_symbol() {
    let _gpu = gpu_lock();
    let (_dir, mut ed) = editor_on_loot("loot-wielded-field");
    let guide = Weenie::Symbol("holtburg:guide".into());
    let set = |ed: &mut EditorApp, value: &str| {
        ed.form_edit(
            &guide,
            &FormEdit::Property {
                kind: PropertyKind::DataId,
                id: 32,
                value: Some(value.to_owned()),
            },
        )
        .unwrap();
    };
    let link = |ed: &mut EditorApp| {
        ed.edit_weenie(&guide, Some("did:32".into()));
        for _ in 0..3 {
            ed.frame();
        }
        assert!(ed.editor.open);
        assert_eq!(ed.editor.page, Page::Section(Section::Creature));
        ed.editor
            .links
            .iter()
            .find(|(k, _)| k == "did:32")
            .expect("the field's link to its table")
            .1
    };
    // Made wearing the server's 37: in Creature, over what it has equipped, and opened by its
    // link; the guide among the table's uses.
    set(&mut ed, "37");
    let r = link(&mut ed);
    keep(&mut ed, "loot", "11-wielded-field.png");
    click(&mut ed, centre(r));
    ed.frame();
    assert!(ed.tab_in_front(Tab::Loot));
    assert_eq!(ed.loot.open, Some((LootKind::Wielded, 37)));
    let table = ed.session.loot_table(LootKind::Wielded, 37).unwrap();
    assert!(table
        .uses
        .iter()
        .any(|u| u.by == guide && u.how == LootUse::Wields));
    // A copy of 37 of the pack's, named, and the guide set to it: written by its token.
    ed.loot_action(LootAction::New {
        kind: LootKind::Wielded,
        from: Some(37),
        first: None,
        name: Some("guide-gear".into()),
    })
    .unwrap();
    let id = ed.session.treasure_id("holtburg:guide-gear").unwrap();
    assert_eq!(ed.loot.open, Some((LootKind::Wielded, id)));
    let table = ed.session.loot_table(LootKind::Wielded, id).unwrap();
    let written = ed.session.loot_written(&table);
    assert_eq!(written, "{{treasure:guide-gear}}");
    set(&mut ed, &written);
    let (file, _) = ed.session.weenie_file(&guide).unwrap();
    let text = ed.session.sql_text(&file).unwrap();
    assert!(text.contains("{{treasure:guide-gear}}"), "{text}");
    let table = ed.session.loot_table(LootKind::Wielded, id).unwrap();
    assert!(table
        .uses
        .iter()
        .any(|u| u.by == guide && u.how == LootUse::Wields));
    // The field shows the pack's table, and its link opens it.
    let r = link(&mut ed);
    keep(&mut ed, "loot", "12-wielded-field-pack.png");
    click(&mut ed, centre(r));
    ed.frame();
    assert_eq!(ed.loot.open, Some((LootKind::Wielded, id)));
}

/// The editor's world draws a table, making nothing: the server's death treasure 4, changed in
/// this world to drop nothing and put back again, each reaching the world as it is made; and the
/// server's wielded treasure 37, a stack of pyreals each time.
#[cfg(feature = "live-server-tests")]
#[test]
fn roll_it_draws_the_table_as_the_editors_world_holds_it_after_each_change() {
    use std::path::PathBuf;
    let _g = gpu_lock();
    let (_dir, world) = sample("live-loot-roll");
    let server =
        PathBuf::from(std::env::var_os("EMPYREAN_SERVER_BIN").expect("EMPYREAN_SERVER_BIN"));
    let sql = PathBuf::from(std::env::var_os("EMPYREAN_WORLD_SQL").expect("EMPYREAN_WORLD_SQL"));
    let mut ed = editor_with(
        &world,
        &[
            "--empyrean",
            server.parent().unwrap().to_str().unwrap(),
            "--weenies",
            sql.to_str().unwrap(),
            "--size",
            "1600x1000",
        ],
    );
    ed.wait_for_names().unwrap();
    // Without Live, Roll it says what it needs.
    assert!(ed
        .loot_action(LootAction::Roll { id: 4, times: 5 })
        .is_err());
    ed.set_live(true).unwrap();
    ed.wait_live().unwrap();
    let roll = |ed: &mut EditorApp, id: u32, times: u32| {
        ed.loot_action(LootAction::Roll { id, times }).unwrap();
        let draws = ed.wait_loot_roll().unwrap();
        assert_eq!(draws.len(), times as usize);
        draws
    };
    // The server's own: items at 100%, at least one a draw.
    let draws = roll(&mut ed, 4, 20);
    assert!(draws.iter().all(|d| !d.is_empty()), "{draws:?}");
    // Changed in this world to drop nothing: pushed, imported, drawn so.
    ed.loot_action(LootAction::ChangeHere(LootKind::Death, 4))
        .unwrap();
    for column in ["item_Chance", "magic_Item_Chance", "mundane_Item_Chance"] {
        ed.loot_action(LootAction::SetDeath {
            id: 4,
            column: column.into(),
            value: "0".into(),
        })
        .unwrap();
    }
    let draws = roll(&mut ed, 4, 20);
    assert!(draws.iter().all(Vec::is_empty), "{draws:?}");
    ed.show_tab(Tab::Loot);
    ed.loot_action(LootAction::Open(Some((LootKind::Death, 4))))
        .unwrap();
    for _ in 0..3 {
        ed.frame();
    }
    keep(&mut ed, "loot", "07-live-roll-nothing.png");
    // Put back: the server's own again.
    ed.loot_action(LootAction::PutBack(LootKind::Death, 4))
        .unwrap();
    let draws = roll(&mut ed, 4, 20);
    assert!(draws.iter().all(|d| !d.is_empty()), "{draws:?}");
    for _ in 0..3 {
        ed.frame();
    }
    keep(&mut ed, "loot", "08-live-roll.png");
    // A wielded table: its pyreals each time.
    let draws = roll(&mut ed, 37, 5);
    assert!(
        draws
            .iter()
            .all(|d| d.iter().any(|(class, _, _)| *class == 273)),
        "{draws:?}"
    );
    ed.loot_action(LootAction::Open(Some((LootKind::Wielded, 37))))
        .unwrap();
    for _ in 0..3 {
        ed.frame();
    }
    keep(&mut ed, "loot", "09-live-roll-wielded.png");
    ed.set_live(false).unwrap();
}

/// A server's weenie changed in this world and put back is the server's own again in the editor's
/// world at once, not only once it starts again: wielded treasure 37's pyreals, rolled, take the
/// name the pack gives them, then their own again.
#[cfg(feature = "live-server-tests")]
#[test]
fn a_servers_weenie_put_back_is_the_servers_own_again_in_the_editors_world() {
    use std::path::PathBuf;
    let _g = gpu_lock();
    let (_dir, world) = sample("live-loot-put-back");
    let server =
        PathBuf::from(std::env::var_os("EMPYREAN_SERVER_BIN").expect("EMPYREAN_SERVER_BIN"));
    let sql = PathBuf::from(std::env::var_os("EMPYREAN_WORLD_SQL").expect("EMPYREAN_WORLD_SQL"));
    let mut ed = editor_with(
        &world,
        &[
            "--empyrean",
            server.parent().unwrap().to_str().unwrap(),
            "--weenies",
            sql.to_str().unwrap(),
            "--size",
            "1600x1000",
        ],
    );
    ed.wait_for_names().unwrap();
    ed.set_live(true).unwrap();
    ed.wait_live().unwrap();
    // The names each roll gives the pyreals it makes, every roll making some.
    let pyreals = |ed: &mut EditorApp| -> Vec<String> {
        ed.loot_action(LootAction::Roll { id: 37, times: 5 })
            .unwrap();
        let draws = ed.wait_loot_roll().unwrap();
        assert_eq!(draws.len(), 5);
        draws
            .iter()
            .map(|d| {
                d.iter()
                    .find(|(class, _, _)| *class == 273)
                    .map(|(_, _, name)| name.clone())
                    .unwrap_or_else(|| panic!("no pyreals: {draws:?}"))
            })
            .collect()
    };
    assert!(pyreals(&mut ed).iter().all(|n| n == "Pyreal"));
    // Changed in this world: the pack's name.
    let own = ed.server_class_now(273).unwrap().expect("the server's 273");
    let renamed = own.replacen("'Pyreal')", "'Pyreal Hoard')", 1);
    assert_ne!(renamed, own);
    let file = ed.session.class_file(273, "Pyreal");
    ed.edit(&Edit::Op(Op::WriteSql {
        file,
        text: Some(renamed),
    }))
    .unwrap();
    assert!(pyreals(&mut ed).iter().all(|n| n == "Pyreal Hoard"));
    // Put back: the server's own, made again.
    ed.put_back_server(273).unwrap();
    assert!(ed.session.weenie_file(&Weenie::Class(273)).is_none());
    assert!(pyreals(&mut ed).iter().all(|n| n == "Pyreal"));
    ed.set_live(false).unwrap();
}
