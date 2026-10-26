//! Behaviour: none (tooling: Lyceum's Quests tab over the retail dats)
//! The Quests tab lists every quest of the world, searched by name; a quest opened lists the
//! weenies that use it, a click on one showing it in the Definition pane and opening it in the
//! editor, and one named in the weenie form's Quests fold opening in the tab; the list narrowed
//! to the pack's own; a
//! server's quest is changed in this world and put back; and a pack's quest is registered,
//! renamed and deleted, each as one step, undone.
//! Fixture: the world database's rows of Holtburg's Sir Rylanan, Sir Tenshin and Dame Tsaya and
//! of Dwennon that name their two quests, and those quests; a 1600 by 1000 window.

use lyceum_editor::app::{EditorApp, FormEdit, Page, QuestAction, Section, Tab};
use lyceum_foundry::edit::Weenie;
use lyceum_foundry::quests::Whose;
use lyceum_foundry::script::Action as Edit;
use lyceum_foundry::sql_ids::QuestDoes;

use crate::world::dungeon::keep;
use crate::{click, editor_with, gpu_lock, sample, type_in};

/// Sir Rylanan, Sir Tenshin, Dame Tsaya and Dwennon: their rows that name their quests, and
/// those quests, as the world database writes them.
const HOLTBURG_QUESTS: &str = "\
CREATE TABLE `weenie` (`class_Id` int, `class_Name` varchar(100), `type` int, `last_Modified` datetime);
INSERT INTO `weenie` VALUES (14413,'guardrylanan',10,'2005-02-09 10:00:00'),(14414,'guardtenshin',10,'2005-02-09 10:00:00'),(14415,'guardtsaya',10,'2005-02-09 10:00:00'),(33970,'ace33970-dwennon',10,'2022-08-22 03:09:27');
CREATE TABLE `weenie_properties_string` (`id` int, `object_Id` int, `type` smallint, `value` text);
INSERT INTO `weenie_properties_string` VALUES (12316,14413,1,'Sir Rylanan'),(12320,14414,1,'Sir Tenshin'),(12324,14415,1,'Dame Tsaya'),(64918,33970,1,'Dwennon');
CREATE TABLE `weenie_properties_emote` (`id` int, `object_Id` int, `category` int, `probability` float, `weenie_Class_Id` int, `style` int, `substyle` int, `quest` text, `vendor_Type` int, `min_Health` float, `max_Health` float);
INSERT INTO `weenie_properties_emote` VALUES (11054,14413,6,1,14443,NULL,NULL,NULL,NULL,NULL,NULL),(11056,14413,6,0.25,14438,NULL,NULL,NULL,NULL,NULL,NULL),(11057,14413,6,0.5,14438,NULL,NULL,NULL,NULL,NULL,NULL),(11058,14413,6,0.75,14438,NULL,NULL,NULL,NULL,NULL,NULL),(11059,14413,6,1,14438,NULL,NULL,NULL,NULL,NULL,NULL),(11064,14413,12,1,NULL,NULL,NULL,'HoltburgGreetingGiven',NULL,NULL,NULL),(11079,14413,13,1,NULL,NULL,NULL,'PortalRegicideWitshirePermissionGiven',NULL,NULL,NULL),(11083,14413,13,1,NULL,NULL,NULL,'HoltburgGreetingGiven',NULL,NULL,NULL),(11092,14413,13,1,NULL,NULL,NULL,'LetterTsaya2Given',NULL,NULL,NULL),(11106,14414,6,0.25,14438,NULL,NULL,NULL,NULL,NULL,NULL),(11107,14414,6,0.5,14438,NULL,NULL,NULL,NULL,NULL,NULL),(11108,14414,6,0.75,14438,NULL,NULL,NULL,NULL,NULL,NULL),(11109,14414,6,1,14438,NULL,NULL,NULL,NULL,NULL,NULL),(11152,14415,6,0.25,14438,NULL,NULL,NULL,NULL,NULL,NULL),(11153,14415,6,0.5,14438,NULL,NULL,NULL,NULL,NULL,NULL),(11154,14415,6,0.75,14438,NULL,NULL,NULL,NULL,NULL,NULL),(11155,14415,6,1,14438,NULL,NULL,NULL,NULL,NULL,NULL),(54013,33970,12,1,NULL,NULL,NULL,'OnCowTipQuest',NULL,NULL,NULL),(54014,33970,12,1,NULL,NULL,NULL,'CowTipCounter',NULL,NULL,NULL),(54015,33970,12,1,NULL,NULL,NULL,'CowTipCounter@250',NULL,NULL,NULL),(54016,33970,12,1,NULL,NULL,NULL,'CowTipCounter@200',NULL,NULL,NULL),(54017,33970,12,1,NULL,NULL,NULL,'CowTipCounter@150',NULL,NULL,NULL),(54018,33970,12,1,NULL,NULL,NULL,'CowTipCounter@150_2',NULL,NULL,NULL),(54019,33970,12,1,NULL,NULL,NULL,'CowTipCounter@100',NULL,NULL,NULL),(54020,33970,12,1,NULL,NULL,NULL,'CowTipCounter@50',NULL,NULL,NULL),(54021,33970,12,1,NULL,NULL,NULL,'CowTipCounter@1',NULL,NULL,NULL),(54023,33970,13,1,NULL,NULL,NULL,'CowTipCounter',NULL,NULL,NULL),(54024,33970,13,1,NULL,NULL,NULL,'CowTipCounter@250',NULL,NULL,NULL),(54025,33970,13,1,NULL,NULL,NULL,'CowTipCounter@200',NULL,NULL,NULL),(54026,33970,13,1,NULL,NULL,NULL,'CowTipCounter@150',NULL,NULL,NULL),(54027,33970,13,1,NULL,NULL,NULL,'CowTipCounter@150_2',NULL,NULL,NULL),(54028,33970,13,1,NULL,NULL,NULL,'CowTipCounter@100',NULL,NULL,NULL),(54029,33970,13,1,NULL,NULL,NULL,'CowTipCounter@50',NULL,NULL,NULL),(54030,33970,13,1,NULL,NULL,NULL,'CowTipCounter@1',NULL,NULL,NULL),(54031,33970,13,1,NULL,NULL,NULL,'OnCowTipQuest',NULL,NULL,NULL);
CREATE TABLE `weenie_properties_emote_action` (`id` int, `emote_Id` int, `order` int, `type` int, `delay` float, `extent` float, `motion` int, `message` text);
INSERT INTO `weenie_properties_emote_action` VALUES (27198,11054,5,22,0,1,NULL,'HoltburgGreetingGiven'),(27221,11056,19,31,0,1,NULL,'HoltburgGreetingGiven'),(27241,11057,16,31,0,1,NULL,'HoltburgGreetingGiven'),(27260,11058,15,31,0,1,NULL,'HoltburgGreetingGiven'),(27280,11059,16,31,0,1,NULL,'HoltburgGreetingGiven'),(27329,11079,0,21,0,1,NULL,'HoltburgGreetingGiven'),(27375,11092,3,22,0,1,NULL,'HoltburgGreetingGiven'),(27443,11106,16,31,0,1,NULL,'HoltburgGreetingGiven'),(27463,11107,16,31,0,1,NULL,'HoltburgGreetingGiven'),(27483,11108,16,31,0,1,NULL,'HoltburgGreetingGiven'),(27502,11109,15,31,0,1,NULL,'HoltburgGreetingGiven'),(27651,11152,16,31,0,1,NULL,'HoltburgGreetingGiven'),(27671,11153,16,31,0,1,NULL,'HoltburgGreetingGiven'),(27690,11154,15,31,0,1,NULL,'HoltburgGreetingGiven'),(27710,11155,16,31,0,1,NULL,'HoltburgGreetingGiven'),(125284,54013,0,21,0,1,NULL,'CowTipCounter'),(125291,54014,6,31,0,1,NULL,'CowTipCounter'),(125308,54023,0,30,0,1,NULL,'CowTipCounter@250'),(125309,54024,0,30,0,1,NULL,'CowTipCounter@200'),(125310,54025,0,30,0,1,NULL,'CowTipCounter@150'),(125311,54026,0,30,0,1,NULL,'CowTipCounter@150_2'),(125312,54027,0,30,0,1,NULL,'CowTipCounter@100'),(125313,54028,0,30,0,1,NULL,'CowTipCounter@50'),(125314,54029,0,30,0,1,NULL,'CowTipCounter@1'),(125318,54031,2,70,0,1,NULL,'CowTipCounter');
CREATE TABLE `quest` (`id` int, `name` varchar(255), `min_Delta` int, `max_Solves` int, `message` text, `last_Modified` datetime);
INSERT INTO `quest` VALUES (767,'HoltburgGreetingGiven',0,1,'Rylanan has said hello to player','2005-02-09 10:00:00'),(2397,'CowTipCounter',0,300,'Counter for how many cows Player has tipped','2021-11-01 00:00:00');
";

const RYLANAN: Weenie = Weenie::Class(14413);

/// The editor over the sample world with the server's rows above, the Quests tab in front.
fn editor_on_quests(tag: &str) -> (dereth_dat::testing::ScratchDir, EditorApp) {
    let (dir, world) = sample(tag);
    let sql = dir.path().join("server.sql");
    std::fs::write(&sql, HOLTBURG_QUESTS).unwrap();
    let mut ed = editor_with(
        &world,
        &["--weenies", sql.to_str().unwrap(), "--size", "1600x1000"],
    );
    ed.show_tab(Tab::Quests);
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
        .quests
        .button(what)
        .unwrap_or_else(|| panic!("{what} drawn"));
    click(ed, centre(r));
    ed.frame();
}

/// Quest `name` found by the search and opened by a click on its row.
fn open_quest(ed: &mut EditorApp, name: &str) {
    if ed.quests.open.is_some() {
        press(ed, "back");
    }
    let search = ed.quests.search_field.expect("the search");
    type_in(ed, search, name, false);
    let r = ed
        .quests
        .row(name)
        .unwrap_or_else(|| panic!("{name} listed"));
    click(ed, centre(r));
    ed.frame();
    assert_eq!(ed.quests.open.as_deref(), Some(name));
}

fn whose(ed: &EditorApp, name: &str) -> Option<Whose> {
    ed.session.world_quest(name).map(|q| q.whose)
}

/// The sample's guide, an NPC of the pack's own.
fn guide() -> Weenie {
    Weenie::Symbol("holtburg:guide".into())
}

/// The guide's use made to stamp `GribbleGreeted`, which nothing registers: one step.
fn stamp_gribble(ed: &mut EditorApp) {
    ed.form_edits(
        &guide(),
        &[
            FormEdit::AddAction {
                set: 0,
                kind: "22".into(),
            },
            FormEdit::Action {
                set: 0,
                action: 1,
                column: "message".into(),
                value: "GribbleGreeted".into(),
            },
        ],
    )
    .unwrap();
    ed.frame();
}

#[test]
fn the_tab_lists_every_quest_searched_by_name_and_a_quest_opened_lists_who_uses_it() {
    let _gpu = gpu_lock();
    let (_dir, mut ed) = editor_on_quests("quests-list");
    assert!(ed.tab_in_front(Tab::Quests));
    let all: Vec<String> = ed.world_quests().iter().map(|q| q.name.clone()).collect();
    for q in ["CowTipCounter", "HoltburgGreetingGiven"] {
        assert!(all.iter().any(|n| n == q), "{q} in {all:?}");
    }
    // A set answering a test of a quest the database here does not register: named, not
    // registered.
    assert_eq!(whose(&ed, "OnCowTipQuest"), Some(Whose::Nobody));
    assert_eq!(ed.quests.rows.len(), all.len(), "every one listed");
    keep(&mut ed, "quests", "01-list.png");
    let search = ed.quests.search_field.expect("the search");
    type_in(&mut ed, search, "greeting", false);
    let rows: Vec<&str> = ed.quests.rows.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(rows, ["HoltburgGreetingGiven"]);
    let r = ed.quests.row("HoltburgGreetingGiven").unwrap();
    click(&mut ed, centre(r));
    ed.frame();
    assert_eq!(ed.quests.open.as_deref(), Some("HoltburgGreetingGiven"));
    // Each knight who uses it, by name.
    let used: Vec<Weenie> = ed.quests.use_rows.iter().map(|(w, _)| w.clone()).collect();
    assert_eq!(
        used,
        [Weenie::Class(14415), RYLANAN, Weenie::Class(14414)],
        "Dame Tsaya, Sir Rylanan, Sir Tenshin"
    );
    // Sir Rylanan stamps it, asks after it, answers that, and erases it.
    let q = ed.session.world_quest("HoltburgGreetingGiven").unwrap();
    let mut does: Vec<QuestDoes> = q
        .uses
        .iter()
        .filter(|u| u.by == RYLANAN)
        .map(|u| u.how.does())
        .collect();
    does.sort();
    does.dedup();
    assert_eq!(
        does,
        [
            QuestDoes::Stamps,
            QuestDoes::Tests,
            QuestDoes::Erases,
            QuestDoes::Answers
        ]
    );
    keep(&mut ed, "quests", "02-uses.png");
    // A click on him opens him in the editor at once, as Edit opens a server's weenie, and shows
    // him in the Definition pane; nothing changes yet.
    let r = ed
        .quests
        .use_rows
        .iter()
        .find(|(w, _)| *w == RYLANAN)
        .unwrap()
        .1;
    click(&mut ed, centre(r));
    for _ in 0..3 {
        ed.frame();
    }
    assert_eq!(ed.form_weenie(), Some(RYLANAN));
    assert!(ed.editor.open && ed.editor.weenie() == Some(&RYLANAN));
    assert!(ed.tab_in_front(Tab::Editor));
    assert!(ed.session.weenie_file(&RYLANAN).is_none());
    assert_eq!(ed.session.history(), (0, 0));
    keep(&mut ed, "quests", "03-opened.png");
    // Back to the list, the search kept.
    press(&mut ed, "back");
    assert!(ed.quests.open.is_none());
    assert_eq!(ed.quests.rows.len(), 1);
}

#[test]
fn a_servers_quest_is_changed_in_this_world_and_put_back_each_as_one_step() {
    let _gpu = gpu_lock();
    let (_dir, mut ed) = editor_on_quests("quests-change");
    open_quest(&mut ed, "CowTipCounter");
    assert_eq!(whose(&ed, "CowTipCounter"), Some(Whose::Server));
    assert!(
        ed.quests.button("rename").is_none(),
        "the server's is not renamed"
    );
    assert!(ed.quests.button("copy").is_some(), "but copied");
    assert!(ed.quests.field("max_Solves").is_none(), "nor edited");
    press(&mut ed, "change_here");
    assert_eq!(
        whose(&ed, "CowTipCounter"),
        Some(Whose::Changed { mine: true })
    );
    assert_eq!(ed.session.history(), (1, 0), "one step");
    let q = ed.session.world_quest("CowTipCounter").unwrap();
    assert!(q
        .file
        .as_ref()
        .unwrap()
        .ends_with("server/sql/quests/CowTipCounter.sql"));
    assert_eq!(q.quest, q.server, "as the server has it");
    // The pack's row now, its fields edited as the weenie form's Quests fold edits them.
    let field = ed.quests.field("max_Solves").expect("the pack's to edit");
    type_in(&mut ed, field, "500", true);
    let max = |ed: &EditorApp| {
        ed.session
            .world_quest("CowTipCounter")
            .and_then(|q| q.quest)
            .map(|q| q.max_solves)
    };
    assert_eq!(max(&ed), Some(500));
    assert_eq!(ed.session.history(), (2, 0));
    keep(&mut ed, "quests", "04-changed.png");
    ed.edit(&Edit::Undo).unwrap();
    assert_eq!(max(&ed), Some(300));
    ed.frame();
    press(&mut ed, "put_back");
    assert_eq!(whose(&ed, "CowTipCounter"), Some(Whose::Server));
    assert_eq!(ed.session.history(), (2, 0), "one step");
    ed.edit(&Edit::Undo).unwrap();
    assert_eq!(
        whose(&ed, "CowTipCounter"),
        Some(Whose::Changed { mine: true })
    );
    ed.edit(&Edit::Undo).unwrap();
    assert_eq!(whose(&ed, "CowTipCounter"), Some(Whose::Server));
    assert_eq!(ed.session.history(), (0, 2));
}

#[test]
fn a_packs_quest_is_registered_renamed_and_deleted_each_as_one_step_undone() {
    let _gpu = gpu_lock();
    let (_dir, mut ed) = editor_on_quests("quests-pack");
    let guide = guide();
    let guide_text = |ed: &mut EditorApp| {
        let (file, _) = ed.session.weenie_file(&guide).unwrap();
        ed.session.sql_text(&file).unwrap()
    };
    // The guide stamps GribbleGreeted, which nothing registers.
    stamp_gribble(&mut ed);
    let steps = ed.session.history().0;
    open_quest(&mut ed, "GribbleGreeted");
    assert_eq!(whose(&ed, "GribbleGreeted"), Some(Whose::Nobody));
    // The pack's guide, clicked, opens in the Definition pane and the editor.
    let r = ed.quests.use_rows.first().expect("the guide uses it").1;
    click(&mut ed, centre(r));
    for _ in 0..2 {
        ed.frame();
    }
    assert_eq!(ed.editor.weenie(), Some(&guide));
    assert_eq!(ed.form_weenie(), Some(guide.clone()));
    assert!(ed.tab_in_front(Tab::Definition) && ed.tab_in_front(Tab::Editor));
    ed.editor.open = false;
    ed.frame();
    press(&mut ed, "register");
    assert_eq!(
        whose(&ed, "GribbleGreeted"),
        Some(Whose::Pack { mine: true })
    );
    assert_eq!(ed.session.history().0, steps + 1, "one step");
    ed.edit(&Edit::Undo).unwrap();
    assert_eq!(whose(&ed, "GribbleGreeted"), Some(Whose::Nobody));
    ed.edit(&Edit::Redo).unwrap();
    ed.frame();
    // Renamed: what it changes is shown first.
    let field = ed.quests.name_field.expect("the new name's field");
    type_in(&mut ed, field, "GribbleMet", false);
    press(&mut ed, "rename");
    let asked = ed.quests.renaming.clone().expect("shown first");
    let mut files: Vec<(String, bool)> = asked
        .renamed
        .files
        .iter()
        .map(|(p, t)| {
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            (name, t.is_some())
        })
        .collect();
    files.sort();
    assert_eq!(
        files,
        [
            ("GribbleGreeted.sql".to_owned(), false),
            ("GribbleMet.sql".to_owned(), true),
            ("guide.sql".to_owned(), true),
        ]
    );
    assert!(
        asked.renamed.left.is_empty(),
        "nothing of the server's names it"
    );
    assert_eq!(ed.session.history().0, steps + 1, "nothing changed yet");
    ed.frame();
    keep(&mut ed, "quests", "05-rename.png");
    press(&mut ed, "rename_do");
    assert_eq!(ed.quests.open.as_deref(), Some("GribbleMet"));
    assert_eq!(whose(&ed, "GribbleMet"), Some(Whose::Pack { mine: true }));
    assert_eq!(whose(&ed, "GribbleGreeted"), None);
    let text = guide_text(&mut ed);
    assert!(text.contains("'GribbleMet'") && !text.contains("GribbleGreeted"));
    assert_eq!(ed.session.history().0, steps + 2, "one step");
    ed.edit(&Edit::Undo).unwrap();
    assert_eq!(
        whose(&ed, "GribbleGreeted"),
        Some(Whose::Pack { mine: true })
    );
    assert!(guide_text(&mut ed).contains("'GribbleGreeted'"));
    ed.edit(&Edit::Redo).unwrap();
    ed.frame();
    // Not deleted while the guide uses it: the tab says so.
    press(&mut ed, "delete");
    assert!(ed.quests.note.is_some(), "refused, and said");
    assert_eq!(whose(&ed, "GribbleMet"), Some(Whose::Pack { mine: true }));
    assert_eq!(ed.session.history().0, steps + 2);
    ed.frame();
    keep(&mut ed, "quests", "06-still-used.png");
    // Once nothing uses it, it is deleted, and an undo brings it back.
    ed.form_edit(&guide, &FormEdit::RemoveAction { set: 0, action: 1 })
        .unwrap();
    ed.frame();
    press(&mut ed, "delete");
    assert_eq!(whose(&ed, "GribbleMet"), None);
    assert!(ed.quests.open.is_none());
    ed.edit(&Edit::Undo).unwrap();
    assert_eq!(whose(&ed, "GribbleMet"), Some(Whose::Pack { mine: true }));
}

#[test]
fn a_quest_named_in_the_weenie_forms_quests_fold_opens_in_the_quests_tab_brought_forward() {
    let _gpu = gpu_lock();
    let (_dir, mut ed) = editor_on_quests("quests-fold");
    stamp_gribble(&mut ed);
    // The Quests tab is behind the world view it stands beside.
    ed.show_tab(Tab::World);
    ed.open_editor(guide());
    ed.editor.page = Page::Section(Section::Emotes);
    for _ in 0..3 {
        ed.frame();
    }
    assert!(!ed.tab_in_front(Tab::Quests));
    let r = ed
        .quests
        .fold_links
        .iter()
        .find(|(n, _)| n == "GribbleGreeted")
        .expect("the fold names it")
        .1;
    click(&mut ed, centre(r));
    ed.frame();
    assert_eq!(ed.quests.open.as_deref(), Some("GribbleGreeted"));
    assert!(ed.tab_in_front(Tab::Quests));
    assert_eq!(
        ed.editor.weenie(),
        Some(&guide()),
        "the editor stays on its weenie"
    );
    keep(&mut ed, "quests", "07-from-the-fold.png");
}

#[test]
fn this_packs_only_lists_the_packs_own_quests_its_changes_and_those_nothing_registers() {
    let _gpu = gpu_lock();
    let (_dir, mut ed) = editor_on_quests("quests-filter");
    stamp_gribble(&mut ed);
    ed.quest_action(QuestAction::ChangeHere("CowTipCounter".into()))
        .unwrap();
    ed.frame();
    assert!(!ed.quests.packs_only, "off to start with");
    let every = ed.quests.rows.len();
    press(&mut ed, "this_pack_only");
    assert!(ed.quests.packs_only);
    let rows: Vec<&str> = ed.quests.rows.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(
        rows,
        [
            "CowTipCounter",
            "GribbleGreeted",
            "LetterTsaya2Given",
            "OnCowTipQuest",
            "PortalRegicideWitshirePermissionGiven",
        ],
        "not the server's HoltburgGreetingGiven"
    );
    keep(&mut ed, "quests", "08-this-packs-only.png");
    press(&mut ed, "this_pack_only");
    assert_eq!(ed.quests.rows.len(), every, "every one again");
}
