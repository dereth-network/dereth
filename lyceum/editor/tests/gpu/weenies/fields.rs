//! Behaviour: none (tooling: Lyceum's weenie editor pages over the retail dats)
//! A portal's "Usable by" ticks and its Olthoi choice each write their bit of its flags as one
//! step; a door's one Locked tick writes both locks; a switch's actions are ticked together and
//! it says a message; a sword's wield need names a player's property; a vendor buys several kinds;
//! Advanced leaves out what the other pages show until "Show all" is ticked; and a server's
//! weenie opened at a field before its class is read opens there once it is.
//! Fixture: a server database of a portal, a door, a switch, a sword and a vendor; a 1600 by 1000
//! window.

use lyceum_editor::app::{EditorApp, Page, Section};
use lyceum_foundry::content::{bare, PropertyKind, SqlDoc};
use lyceum_foundry::edit::Weenie;
use lyceum_foundry::script::Action as Edit;

use crate::{capture, change_in_world, click, drawn, editor_with, gpu_lock, sample, shots};

/// The server's: a portal anyone may use but neither summon nor recall to, from level 20; a door
/// locked when it appears that never says it locks again; a switch that spawns and says
/// something; a sword needing a society rank; and a vendor of weapons and armor.
const SERVER_PAGES: &str = "\
INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES (90030, 'gateway', 7, '2005-02-09 10:00:00')
     , (90031, 'irondoor', 19, '2005-02-09 10:00:00')
     , (90032, 'lever', 26, '2005-02-09 10:00:00')
     , (90033, 'ranksword', 6, '2005-02-09 10:00:00')
     , (90034, 'trader', 12, '2005-02-09 10:00:00');
INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES (90030, 1, 'Gateway'), (90031, 1, 'Iron Door'), (90032, 1, 'Lever')
     , (90032, 17, 'The gate grinds open.'), (90033, 1, 'Rank Sword'), (90034, 1, 'Trader');
INSERT INTO `weenie_properties_int` (`object_Id`, `type`, `value`)
VALUES (90030, 111, 49), (90030, 86, 20), (90032, 83, 65552), (90033, 1, 1), (90033, 45, 3)
     , (90033, 158, 9), (90033, 159, 287), (90033, 160, 601), (90034, 74, 259);
INSERT INTO `weenie_properties_bool` (`object_Id`, `type`, `value`)
VALUES (90031, 3, True), (90034, 19, False);
INSERT INTO `weenie_properties_position` (`object_Id`, `position_Type`, `obj_Cell_Id`, `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`)
VALUES (90030, 2, 0x019E0100, 10, 20, 0, 1, 0, 0, 0);
";

/// The server's names for the above, as `empyrean-import schema` writes them.
const SCHEMA_PAGES: &str = r#"{"properties":{"int":{"86":"MinLevel","111":"PortalBitmask","287":"SocietyRankCelhan","288":"SocietyRankEldweb"},"bool":{"3":"Locked","35":"DefaultLocked"},"string":{"1":"Name","17":"ActivationTalk"},"position":{"2":"Destination"}}}"#;

/// The editor over the sample world with the server's rows above.
fn editor_on_pages(tag: &str) -> (dereth_dat::testing::ScratchDir, EditorApp) {
    let (dir, world) = sample(tag);
    let sql = dir.path().join("server.sql");
    std::fs::write(&sql, SERVER_PAGES).unwrap();
    let schema = dir.path().join("schema.json");
    std::fs::write(&schema, SCHEMA_PAGES).unwrap();
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

/// Server class `class` written again by the pack and the editor opened on its page `page`.
fn open_on(ed: &mut EditorApp, class: u32, page: Page) {
    ed.server_class_now(class).unwrap().expect("its rows");
    change_in_world(ed, class);
    ed.editor.page = page;
    for _ in 0..3 {
        ed.frame();
    }
}

/// Property `id` of type `kind` of the pack's class `class`, as written.
fn property(ed: &mut EditorApp, class: u32, kind: PropertyKind, id: u32) -> Option<String> {
    let (file, _) = ed.session.weenie_file(&Weenie::Class(class)).unwrap();
    SqlDoc::parse(&ed.session.sql_text(&file).unwrap())
        .property(kind, id)
        .map(|v| bare(&v))
}

/// The frame drawn last, kept as `name` among the editor's pictures.
fn shoot(ed: &mut EditorApp, dir: &std::path::Path, name: &str) {
    capture(ed, dir);
    std::fs::copy(dir.join("frame.png"), shots("weenie-pages").join(name)).unwrap();
}

/// What the editor drew last: each open dropdown's rows, each dropdown, each row's buttons.
fn picker_rows(ed: &EditorApp) -> &[(String, egui::Rect)] {
    &ed.editor.picker_rows
}

fn add_buttons(ed: &EditorApp) -> &[(String, egui::Rect)] {
    &ed.editor.add_buttons
}

fn row_buttons(ed: &EditorApp) -> &[(String, egui::Rect)] {
    &ed.editor.row_buttons
}

/// A click on the thing drawn as `key` among `list`, and two frames.
fn press(ed: &mut EditorApp, list: fn(&EditorApp) -> &[(String, egui::Rect)], key: &str) {
    let r = drawn(list(ed), key);
    click(ed, (r.center().x, r.center().y));
    for _ in 0..2 {
        ed.frame();
    }
}

#[test]
fn a_portals_usable_by_ticks_and_olthoi_choice_each_write_their_bit_as_one_step() {
    let _gpu = gpu_lock();
    let (dir, mut ed) = editor_on_pages("editor-portal-usable-by");
    open_on(&mut ed, 90030, Page::Section(Section::Portal));
    let flags = |ed: &mut EditorApp| property(ed, 90030, PropertyKind::Int, 111);
    let (rows, combos) = (picker_rows, add_buttons);
    shoot(&mut ed, dir.path(), "portal-usable-by.png");
    assert_eq!(flags(&mut ed).as_deref(), Some("49"));
    let steps = ed.session.history().0;
    // Allow PK off, then can be summoned on: a step each.
    press(&mut ed, rows, "int:111:00E:002");
    assert_eq!(flags(&mut ed).as_deref(), Some("51"));
    press(&mut ed, rows, "int:111:030:010");
    assert_eq!(flags(&mut ed).as_deref(), Some("35"));
    // Only Olthoi, from its dropdown.
    press(&mut ed, combos, "int:111:0C0");
    shoot(&mut ed, dir.path(), "portal-olthoi.png");
    press(&mut ed, rows, "int:111:0C0:040");
    assert_eq!(flags(&mut ed).as_deref(), Some("99"));
    assert_eq!(ed.session.history().0, steps + 3, "a step each");
    shoot(&mut ed, dir.path(), "portal-usable-by-changed.png");
    // Each undone alone.
    ed.edit(&Edit::Undo).unwrap();
    assert_eq!(flags(&mut ed).as_deref(), Some("35"));
    ed.edit(&Edit::Undo).unwrap();
    ed.edit(&Edit::Undo).unwrap();
    assert_eq!(flags(&mut ed).as_deref(), Some("49"));
    // Its flags are on its page: left out of Advanced until "Show all" is ticked.
    ed.editor.page = Page::Advanced;
    for _ in 0..2 {
        ed.frame();
    }
    let has = |ed: &EditorApp, key: &str| ed.form_fields.iter().any(|(k, _)| k == key);
    assert!(!has(&ed, "int:111") && !has(&ed, "int:86") && !has(&ed, "string:1"));
    shoot(&mut ed, dir.path(), "advanced-left-out.png");
    let switch = ed.editor.show_all_switch.expect("its Show all");
    click(&mut ed, (switch.center().x, switch.center().y));
    for _ in 0..2 {
        ed.frame();
    }
    assert!(ed.editor.advanced_all);
    assert!(has(&ed, "int:111") && has(&ed, "int:86") && has(&ed, "string:1"));
    shoot(&mut ed, dir.path(), "advanced-show-all.png");
}

#[test]
fn a_doors_locked_tick_writes_both_locks_a_switch_ticks_its_actions_and_says_its_message() {
    let _gpu = gpu_lock();
    let (dir, mut ed) = editor_on_pages("editor-lock-and-switch");
    // Locked when it appears, so locked: unticked, both locks written as one step.
    open_on(&mut ed, 90031, Page::Section(Section::Lock));
    shoot(&mut ed, dir.path(), "door-locked.png");
    let steps = ed.session.history().0;
    press(&mut ed, row_buttons, "flow:locked:no");
    for id in [3, 35] {
        assert_eq!(
            property(&mut ed, 90031, PropertyKind::Bool, id).as_deref(),
            Some("False"),
            "{id}"
        );
    }
    assert_eq!(ed.session.history().0, steps + 1);
    // A switch that spawns and says something: both ticked; a third ticked on.
    open_on(&mut ed, 90032, Page::Section(Section::Activation));
    assert!(ed.form_fields.iter().any(|(k, _)| k == "string:17"));
    press(&mut ed, add_buttons, "int:83");
    shoot(&mut ed, dir.path(), "switch-actions.png");
    press(&mut ed, picker_rows, "int:83:00000002");
    assert_eq!(
        property(&mut ed, 90032, PropertyKind::Int, 83).as_deref(),
        Some("65554")
    );
}

#[test]
fn a_swords_wield_need_names_a_players_property_and_its_damage_and_a_vendors_wares_are_ticked() {
    let _gpu = gpu_lock();
    let (dir, mut ed) = editor_on_pages("editor-wield-property");
    open_on(&mut ed, 90033, Page::Section(Section::Wearing));
    shoot(&mut ed, dir.path(), "sword-wield-property.png");
    // Its damage: slashing and piercing, a dropdown of check boxes.
    open_on(&mut ed, 90033, Page::Section(Section::Weapon));
    press(&mut ed, add_buttons, "int:45");
    shoot(&mut ed, dir.path(), "sword-damage-type.png");
    press(&mut ed, picker_rows, "int:45:00000010");
    assert_eq!(
        property(&mut ed, 90033, PropertyKind::Int, 45).as_deref(),
        Some("19")
    );
    // A vendor of weapons and armor.
    open_on(&mut ed, 90034, Page::Section(Section::Trade));
    press(&mut ed, add_buttons, "int:74");
    shoot(&mut ed, dir.path(), "vendor-buys.png");
}

/// A server's weenie opened at a field before its class is read from the server's database, as a
/// usage or a finding opens it, opens at that field's page once its class is read: the gateway at
/// its least level, on its portal page. The read is held until the editor has drawn it twice.
#[test]
fn a_server_weenie_opened_at_a_field_before_its_class_is_read_opens_there_once_it_is() {
    let _gpu = gpu_lock();
    let (_dir, mut ed) = editor_on_pages("editor-field-before-read");
    // Last on Advanced, which every weenie has.
    ed.editor.page = Page::Advanced;
    ed.hold_server_reads(true);
    let gateway = Weenie::Class(90030);
    ed.edit_weenie(&gateway, Some("int:86".into()));
    // Drawn, its class being read: no form yet.
    let mut drawn = 0;
    for _ in 0..10 {
        ed.frame();
        if ed.editor.head_words.is_some() {
            drawn += 1;
            if drawn == 2 {
                break;
            }
        }
    }
    assert_eq!(drawn, 2, "the editor drawn");
    assert!(ed.form_fields.is_empty(), "still being read");
    ed.hold_server_reads(false);
    ed.server_class_now(90030).unwrap().expect("its rows");
    for _ in 0..2 {
        ed.frame();
    }
    assert_eq!(ed.editor.page, Page::Section(Section::Portal));
    assert!(ed.form_fields.iter().any(|(k, _)| k == "int:86"));
}
