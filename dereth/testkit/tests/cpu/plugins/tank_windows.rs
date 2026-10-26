//! The Tank sample's windows and profiles against VTank: its meta editor and the meta state set by
//! hand and by key, its hotkeys, its remote control, its Items page (the combat items and the
//! consumables) and Buffs page, and VTank settings profiles' items, consumables and buffs, each
//! run from `dereth/plugins/tank` in a scripted world, as VTank's public documentation and its
//! players' published profiles say VTank has each.

use dereth_plugin_api::Value;
use dereth_plugin_host::PluginFiles;
use dereth_rules::weenie::{bitfield, item_type};
use dereth_testkit::behaviours::note_asserted;
use dereth_testkit::plugins::{PluginRun, ScriptedObject, SCRIPTED_PLAYER, START_EW, START_NS};

use super::samples;
use super::tank::{held, int, logged, met, tank, tank_in, text, NEVER, NONE, TANK};
use super::SharedFiles;

dereth_testkit::scenarios! {
    scenario_the_tanks_meta_page_edits_a_metas_rules_and_the_running_meta_follows => the_tanks_meta_page_edits_a_metas_rules_and_the_running_meta_follows ["plugins.samples.the-tanks-meta-page-edits-a-metas-rules-and-the-running-meta-follows"],
    scenario_the_tanks_hotkeys_run_and_pause_it_set_the_attack_height_loot_the_open_container_and_enter_meta_states => the_tanks_hotkeys_run_and_pause_it_set_the_attack_height_loot_the_open_container_and_enter_meta_states ["plugins.samples.the-tanks-hotkeys-run-and-pause-it-set-the-attack-height-loot-the-open-container-and-enter-meta-states"],
    scenario_the_tanks_remote_control_force_buffs_follows_and_switches_corners_and_target_lock => the_tanks_remote_control_force_buffs_follows_and_switches_corners_and_target_lock ["plugins.samples.the-tanks-remote-control-force-buffs-follows-and-switches-corners-and-target-lock"],
    scenario_the_tanks_items_page_keeps_combat_items_and_consumables_from_the_selection => the_tanks_items_page_keeps_combat_items_and_consumables_from_the_selection ["plugins.samples.the-tanks-items-page-keeps-combat-items-and-consumables-from-the-selection"],
    scenario_the_tanks_buffs_page_lists_its_buffs_and_adds_or_leaves_out_one => the_tanks_buffs_page_lists_its_buffs_and_adds_or_leaves_out_one ["plugins.samples.the-tanks-buffs-page-lists-its-buffs-and-adds-or-leaves-out-one"],
    scenario_the_tank_reads_and_writes_a_vtank_profiles_items_consumables_and_buffs => the_tank_reads_and_writes_a_vtank_profiles_items_consumables_and_buffs ["plugins.samples.the-tank-reads-and-writes-a-vtank-profiles-items-consumables-and-buffs"],
    scenario_the_tanks_settings_page_loads_this_characters_own_profiles_from_a_list => the_tanks_settings_page_loads_this_characters_own_profiles_from_a_list ["plugins.samples.the-tanks-settings-page-loads-this-characters-own-profiles-from-a-list"],
    scenario_the_tanks_settings_page_finds_a_setting_by_part_of_its_name => the_tanks_settings_page_finds_a_setting_by_part_of_its_name ["plugins.samples.the-tanks-settings-page-finds-a-setting-by-part-of-its-name"],
}

/// The meta's set-state action, by the number the file stores.
const SET_STATE: i32 = 1;

/// The Tank's pages, by their place among the window's tabs.
const BUFFS_PAGE: i64 = 9;
const META_PAGE: i64 = 11;

/// The line of a window dump that shows node `id`.
fn node_line(dump: &str, id: &str) -> String {
    let tag = format!("#{id} ");
    dump.lines()
        .find(|l| l.contains(&tag) || l.trim_end().ends_with(&format!("#{id}")))
        .unwrap_or_default()
        .to_owned()
}

/// The player chooses row `row` of list (or page `row` of tabs) `node` in the Tank's window.
fn select(run: &mut PluginRun, node: &str, row: i64) {
    run.host()
        .ui_input(
            TANK,
            "main",
            dereth_plugin_host::UiInput::Select {
                node: node.into(),
                rows: vec![row],
            },
        )
        .unwrap_or_else(|e| panic!("{node}: {e}"));
    run.frame();
}

/// A file of the Tank's data folder, as text.
fn read(files: &SharedFiles, path: &str) -> Option<String> {
    files
        .clone()
        .read(TANK, path)
        .map(|b| String::from_utf8_lossy(&b).into_owned())
}

/// The macro's value of setting `name`, as `/vt opt get` answers it.
fn setting(run: &mut PluginRun, name: &str) -> String {
    let from = run.printed().len();
    run.type_line(&format!("/tank vt opt get {name}"));
    let prefix = format!("[Tank] {name} = ");
    run.printed()[from..]
        .iter()
        .find_map(|l| l.strip_prefix(&prefix).map(str::to_owned))
        .unwrap_or_else(|| panic!("no {name}: {:?}", &run.printed()[from..]))
}

/// The meta editor: a rule chosen fills the editor; changed and saved, it is the running meta's
/// at once (its state moves on by it) and its file's, which reads back as a VTank meta; a new rule
/// with an All of two conditions, its parts edited a part at a time, is added; one that embeds a
/// route file is added and runs (red in the list), and stays run when another rule changes; rules
/// move and are removed; and a new meta is made.
fn the_tanks_meta_page_edits_a_metas_rules_and_the_running_meta_follows() {
    let (mut run, files) = tank(Vec::new());
    let mut put = files.clone();
    let meta = met(&[
        (NEVER, int(0), SET_STATE, text("Hunt"), "Default"),
        (NEVER, int(0), NONE, int(0), "Hunt"),
    ]);
    put.write(TANK, "metas/m.met", meta.as_bytes())
        .expect("in memory");
    let route = std::fs::read(samples().join("tank/routes/holtburg-loop.nav")).expect("a route");
    put.write(TANK, "routes/r.nav", &route).expect("in memory");
    for line in [
        "/tank set EnableNav false",
        "/tank set EnableCombat false",
        "/tank set EnableBuffing false",
        "/tank set EnableLooting false",
    ] {
        run.type_line(line);
    }
    run.set(TANK, "main.s:Meta", Value::from("m.met"));
    run.type_line("/tank start");
    run.advance(1.0);
    // Rule 1 chosen: the editor holds it.
    select(&mut run, "meta_rules", 1);
    let dump = run.ui_dump();
    assert!(
        node_line(&dump, "mr_ckind").contains(r#"value="Never""#)
            && node_line(&dump, "mr_akind").contains(r#"value="Set Meta State""#)
            && node_line(&dump, "mr_a1").contains(r#"value="Hunt""#),
        "{dump}"
    );
    // Its condition made Always and saved: the running meta moves on by it.
    run.set(TANK, "main.mr_ckind", Value::from("Always"));
    run.click(TANK, "main.mr_save");
    run.advance(1.0);
    assert!(
        logged(&run, "rule 1 saved; saved metas/m.met") && logged(&run, "meta: Default -> Hunt"),
        "{:#?}",
        run.console(TANK)
    );
    let saved = dereth_plugin_host::metfile::parse(&read(&files, "metas/m.met").expect("saved"))
        .expect("it reads as a VTank meta");
    let rules = saved.get("rules").as_list().expect("rules").to_vec();
    assert_eq!(
        rules[0].get("condition").get("kind").as_str(),
        Some("always")
    );
    assert_eq!(rules[0].get("action").get("data").as_str(), Some("Hunt"));
    // A new rule in Hunt: All of Character Death and an item count, each part edited in turn.
    run.click(TANK, "main.mr_new");
    run.set(TANK, "main.mr_state", Value::from("Hunt"));
    run.set(TANK, "main.mr_ckind", Value::from("All"));
    run.click(TANK, "main.mr_cpart_add");
    run.click(TANK, "main.mr_cpart_open");
    run.set(TANK, "main.mr_ckind", Value::from("Character Death"));
    run.click(TANK, "main.mr_cback");
    run.click(TANK, "main.mr_cpart_add");
    run.click(TANK, "main.mr_cpart_open");
    run.set(
        TANK,
        "main.mr_ckind",
        Value::from("Inventory Item Count <="),
    );
    run.set(TANK, "main.mr_c1", Value::from("Mana Scarab"));
    run.set(TANK, "main.mr_c2", Value::from("5"));
    run.click(TANK, "main.mr_cback");
    run.set(TANK, "main.mr_akind", Value::from("Chat Command"));
    run.set(TANK, "main.mr_a1", Value::from("/vt echo low on scarabs"));
    run.click(TANK, "main.mr_add");
    let rows = node_line(&run.ui_dump(), "meta_rules");
    assert!(
        rows.contains(r#"[3,"Hunt","All { Character Death; Inventory Item Count <=: \"Mana Scarab\", 5 }","Chat Command: \"/vt echo low on scarabs\""]"#),
        "{rows}"
    );
    let saved = dereth_plugin_host::metfile::parse(&read(&files, "metas/m.met").expect("saved"))
        .expect("it reads as a VTank meta");
    let all = saved.get("rules").as_list().expect("rules")[2]
        .get("condition")
        .clone();
    assert_eq!(all.get("kind").as_str(), Some("all"));
    let parts = all.get("data").as_list().expect("parts").to_vec();
    assert_eq!(parts[0].get("kind").as_str(), Some("died"));
    assert_eq!(parts[1].get("data").get("n").as_str(), Some("Mana Scarab"));
    assert_eq!(parts[1].get("data").get("c").as_i64(), Some(5));
    // A rule in Hunt that embeds a route file: added, it runs, red in the list.
    run.click(TANK, "main.mr_new");
    run.set(TANK, "main.mr_state", Value::from("Hunt"));
    run.set(TANK, "main.mr_ckind", Value::from("Always"));
    run.set(TANK, "main.mr_akind", Value::from("Load Embedded Navroute"));
    run.set(TANK, "main.mr_a1", Value::from("r.nav"));
    run.click(TANK, "main.mr_add");
    run.advance(1.0);
    let set_route =
        |run: &PluginRun| logged_count(run, "route from the meta: circular, 4 waypoints");
    assert_eq!(set_route(&run), 1, "{:#?}", run.console(TANK));
    select(&mut run, "pages", META_PAGE);
    let rows = node_line(&run.ui_dump(), "meta_rules");
    assert!(
        rows.contains(r##"[{"color":"#FF6060","text":"4"}"##),
        "the embedding rule ran: {rows}"
    );
    // Another rule changed: the rule that ran stays run, and does not run again.
    select(&mut run, "meta_rules", 2);
    run.set(TANK, "main.mr_akind", Value::from("Chat Command"));
    run.set(TANK, "main.mr_a1", Value::from("/vt echo hunting"));
    run.click(TANK, "main.mr_save");
    run.advance(1.0);
    assert_eq!(set_route(&run), 1, "{:#?}", run.console(TANK));
    let saved = dereth_plugin_host::metfile::parse(&read(&files, "metas/m.met").expect("saved"))
        .expect("it reads as a VTank meta");
    let walk = saved.get("rules").as_list().expect("rules")[3]
        .get("action")
        .clone();
    assert_eq!(walk.get("kind").as_str(), Some("walk"));
    assert_eq!(
        walk.get("data").get("route").get("kind").as_str(),
        Some("circular")
    );
    // Moved up, then removed.
    select(&mut run, "meta_rules", 3);
    run.click(TANK, "main.mr_up");
    let rows = node_line(&run.ui_dump(), "meta_rules");
    assert!(rows.contains(r#"[2,"Hunt","All { "#), "{rows}");
    run.click(TANK, "main.mr_remove");
    let rows = node_line(&run.ui_dump(), "meta_rules");
    assert!(!rows.contains("All { "), "{rows}");
    // A new meta, with no rules, run from now.
    run.set(TANK, "main.meta_save_name", Value::from("fresh"));
    run.click(TANK, "main.meta_new");
    let dump = run.ui_dump();
    assert!(
        logged(&run, "a new meta, metas/fresh.met")
            && node_line(&dump, "s:Meta").contains(r#"value="fresh.met""#)
            && node_line(&dump, "meta_rules").contains("rows=[]"),
        "{dump}"
    );
    assert!(read(&files, "metas/fresh.met").is_some());
    run.type_line("/tank stop");
    note_asserted(
        "plugins.samples.the-tanks-meta-page-edits-a-metas-rules-and-the-running-meta-follows",
    );
}

/// How many console lines contain `s`.
fn logged_count(run: &PluginRun, s: &str) -> usize {
    run.console(TANK).iter().filter(|l| l.contains(s)).count()
}

/// VTank's hotkeys, each a key set in the Settings page: the run hotkey starts the macro and
/// pauses it; an attack height's sets the height; "Loot this Container" takes from the open chest
/// what the loot profile keeps (as the Loot page's button does); a key that does not read is not
/// bound; and a meta state bound to a key on the Meta page is entered at it, until unbound.
fn the_tanks_hotkeys_run_and_pause_it_set_the_attack_height_loot_the_open_container_and_enter_meta_states(
) {
    const CHEST: u32 = 0x8000_0400;
    const GEM: u32 = 0x8000_0401;
    let (mut run, files) = tank(vec![
        ScriptedObject {
            id: CHEST,
            name: "Chest".into(),
            obj_type: item_type::CONTAINER,
            bitfield: bitfield::OPENABLE | bitfield::STUCK,
            place: Some((START_NS, START_EW + 1.0 / 240.0)),
            ..ScriptedObject::default()
        },
        ScriptedObject {
            id: GEM,
            name: "Black Garnet".into(),
            obj_type: item_type::GEM,
            container: Some(CHEST),
            value: 3000,
            ..ScriptedObject::default()
        },
    ]);
    let mut put = files.clone();
    let meta = met(&[
        (NEVER, int(0), NONE, int(0), "Default"),
        (NEVER, int(0), NONE, int(0), "Hunt"),
    ]);
    put.write(TANK, "metas/m.met", meta.as_bytes())
        .expect("in memory");
    for line in [
        "/tank set EnableNav false",
        "/tank set EnableCombat false",
        "/tank set EnableBuffing false",
        "/tank set EnableLooting false",
    ] {
        run.type_line(line);
    }
    run.set(TANK, "main.s:HotkeyRun", Value::from("ctrl+shift+r"));
    run.set(TANK, "main.s:HotkeyLow", Value::from("Alt+3"));
    run.set(TANK, "main.s:HotkeyLootContainer", Value::from("Ctrl+L"));
    run.set(TANK, "main.s:HotkeyHigh", Value::from("Shift+Q"));
    run.click(TANK, "main.hk_apply");
    let dump = run.ui_dump();
    assert!(
        node_line(&dump, "hk:HotkeyRun").contains(r#"key="Ctrl+Shift+R""#)
            && node_line(&dump, "hk:HotkeyRun").contains(r#"command="tank toggle""#)
            && node_line(&dump, "hk:HotkeyHigh").is_empty(),
        "{dump}"
    );
    assert!(
        logged(&run, "not keys a hotkey takes: Attack high (Shift+Q)"),
        "{:#?}",
        run.console(TANK)
    );
    // Run, then pause, at the one key.
    run.press_hotkey(TANK, "hk:HotkeyRun");
    run.advance(0.5);
    assert_eq!(run.status(TANK), "running");
    assert!(logged(&run, "started"), "{:#?}", run.console(TANK));
    run.press_hotkey(TANK, "hk:HotkeyRun");
    assert!(logged(&run, "paused: by you"), "{:#?}", run.console(TANK));
    run.press_hotkey(TANK, "hk:HotkeyLow");
    assert_eq!(setting(&mut run, "DefaultMeleeAttackHeight"), "3");
    run.type_line("/tank stop");
    // Loot this Container: the chest the player opened, by the loot profile.
    run.type_line("/tank profile basic.utl");
    run.player_uses(CHEST);
    run.advance(0.5);
    run.press_hotkey(TANK, "hk:HotkeyLootContainer");
    run.advance(3.0);
    assert_eq!(
        run.container_of(GEM),
        Some(SCRIPTED_PLAYER),
        "the gem taken: {:?} {:#?}",
        run.sent(),
        run.console(TANK)
    );
    // A meta state bound to a key, entered at it, and unbound.
    run.set(TANK, "main.s:Meta", Value::from("m.met"));
    run.set(TANK, "main.meta_state", Value::from("Hunt"));
    run.set(TANK, "main.meta_key", Value::from("Ctrl+1"));
    run.click(TANK, "main.meta_bind");
    assert!(
        node_line(&run.ui_dump(), "hk:state:1").contains(r#"command="tank vt setmetastate Hunt""#),
        "{}",
        run.ui_dump()
    );
    run.press_hotkey(TANK, "hk:state:1");
    assert!(
        run.printed().iter().any(|l| l == "[Tank] meta state Hunt"),
        "{:#?}",
        run.printed()
    );
    run.click(TANK, "main.meta_unbind:1");
    assert!(node_line(&run.ui_dump(), "hk:state:1").is_empty());
    note_asserted("plugins.samples.the-tanks-hotkeys-run-and-pause-it-set-the-attack-height-loot-the-open-container-and-enter-meta-states");
}

/// VTank's remote control, a bar on the screen while MiniRemote is on: F casts every buff again,
/// CF cancels that, FC follows the selected character, C switches following around corners and
/// the lock target lock, each switch showing its setting however it was set; off, the bar goes.
fn the_tanks_remote_control_force_buffs_follows_and_switches_corners_and_target_lock() {
    const BOB: u32 = 0x5000_0002;
    let (mut run, _) = tank(vec![ScriptedObject {
        id: BOB,
        name: "Bob".into(),
        obj_type: item_type::CREATURE,
        bitfield: bitfield::PLAYER,
        place: Some((START_NS + 5.0 / 240.0, START_EW)),
        ..ScriptedObject::default()
    }]);
    run.type_line("/tank set EnableNav false");
    run.set(TANK, "main.s:MiniRemote", Value::Bool(true));
    assert!(
        run.ui_dump()
            .contains("window network.dereth.tank remote open=true"),
        "{}",
        run.ui_dump()
    );
    run.click(TANK, "remote.F");
    run.click(TANK, "remote.CF");
    assert!(
        logged(&run, "force buff: every buff is cast again")
            && logged(&run, "force buff cancelled"),
        "{:#?}",
        run.console(TANK)
    );
    run.select(BOB);
    run.click(TANK, "remote.FC");
    assert!(
        node_line(&run.ui_dump(), "r_title").contains("following Bob"),
        "{}",
        run.ui_dump()
    );
    run.set(TANK, "remote.C", Value::Bool(true));
    run.set(TANK, "remote.lock", Value::Bool(true));
    assert_eq!(setting(&mut run, "FollowAroundCorners"), "true");
    assert_eq!(setting(&mut run, "TargetLock"), "true");
    assert!(
        node_line(&run.ui_dump(), "s:FollowAroundCorners").contains("value=true"),
        "the Nav page shows it: {}",
        run.ui_dump()
    );
    run.type_line("/tank set TargetLock false");
    assert!(
        node_line(&run.ui_dump(), "lock").contains("value=false"),
        "the lock shows the setting: {}",
        run.ui_dump()
    );
    run.set(TANK, "main.s:MiniRemote", Value::Bool(false));
    assert!(
        run.ui_dump()
            .contains("window network.dereth.tank remote open=false"),
        "{}",
        run.ui_dump()
    );
    note_asserted("plugins.samples.the-tanks-remote-control-force-buffs-follows-and-switches-corners-and-target-lock");
}

/// VTank's Items and Consumables tabs, on the Items page: the selected weapon and caster are
/// added as combat items (the caster never buffed), the selected pet as a pet, a kit refused; a
/// combat item chosen is switched to never buffed, another removed; the selected kit and an item
/// named become consumables, each with its kind and how many the pack holds, and one is removed.
/// The monster list takes the selected monster's name, and offers the combat items as a row's
/// weapon; the Vitals page switches refilling while stopped.
fn the_tanks_items_page_keeps_combat_items_and_consumables_from_the_selection() {
    const SWORD: u32 = 0x8000_0500;
    const WAND: u32 = 0x8000_0501;
    const ESSENCE: u32 = 0x8000_0502;
    const KIT: u32 = 0x8000_0503;
    const MONSTER: u32 = 0x8000_0504;
    let (mut run, _) = tank(vec![
        held(SWORD, "Sword", item_type::MELEE_WEAPON),
        held(WAND, "Wand", item_type::CASTER),
        held(ESSENCE, "Drudge Essence", item_type::MISC),
        ScriptedObject {
            bitfield: bitfield::HEALER,
            stack: Some(3),
            ..held(KIT, "Handy Healing Kit", item_type::MISC)
        },
        super::tank::monster(MONSTER, "Drudge Skulker", START_NS + 10.0 / 240.0, START_EW),
    ]);
    run.select(SWORD);
    run.click(TANK, "main.item_add");
    run.select(WAND);
    run.click(TANK, "main.item_add_nobuff");
    run.select(ESSENCE);
    run.click(TANK, "main.item_add");
    run.select(KIT);
    run.click(TANK, "main.item_add");
    let items = node_line(&run.ui_dump(), "items");
    assert!(
        items.contains(r#"rows=[["Sword","melee weapon","yes"],["Wand","caster","no"],["Drudge Essence","pet","yes"]]"#),
        "{items}"
    );
    assert!(
        logged(
            &run,
            "Handy Healing Kit is not a weapon, caster, shield or pet"
        ),
        "{:#?}",
        run.console(TANK)
    );
    assert_eq!(setting(&mut run, "Weapons"), "Sword, Wand");
    assert_eq!(setting(&mut run, "Pets"), "Drudge Essence");
    select(&mut run, "items", 1);
    run.click(TANK, "main.item_buff");
    select(&mut run, "items", 2);
    run.click(TANK, "main.item_remove");
    assert_eq!(setting(&mut run, "Weapons"), "Sword");
    assert_eq!(setting(&mut run, "NoBuffItems"), "Sword");
    // Consumables: the selected kit, and an item named.
    run.click(TANK, "main.cons_add");
    run.set(TANK, "main.cons_name", Value::from("Mana Scarab"));
    run.click(TANK, "main.cons_add");
    let cons = node_line(&run.ui_dump(), "consumables");
    assert!(
        cons.contains(
            r#"rows=[["Handy Healing Kit","kit",3],["Mana Scarab","not in the pack",0]]"#
        ),
        "{cons}"
    );
    select(&mut run, "consumables", 2);
    run.click(TANK, "main.cons_remove");
    assert_eq!(setting(&mut run, "Consumables"), "Handy Healing Kit");
    // The monster list: the selected monster's name, and the combat items as its weapon.
    run.select(MONSTER);
    run.click(TANK, "main.monster_selected");
    let dump = run.ui_dump();
    assert!(
        node_line(&dump, "monster_name").contains(r#"value="Drudge Skulker""#)
            && node_line(&dump, "monster_weapon").contains(r#"options=["","auto","Sword","Wand"]"#),
        "{dump}"
    );
    run.set(TANK, "main.s:ManaChargesWhenOff", Value::Bool(true));
    assert_eq!(setting(&mut run, "ManaChargesWhenOff"), "true");
    note_asserted("plugins.samples.the-tanks-items-page-keeps-combat-items-and-consumables-from-the-selection");
}

/// VTank's Buffs tab, on the Buffs page: opened, it lists the buffs the Tank keeps up with the spell
/// each is cast with (the spell book read for it); a buff named is added (as VTank's tab adds a
/// skill not trained), a buff chosen left out and cast again, and the one added taken off.
fn the_tanks_buffs_page_lists_its_buffs_and_adds_or_leaves_out_one() {
    let files = SharedFiles::default();
    let mut run = tank_in(
        PluginRun::new().with_spells(&[(256, "Impregnability Self I", 4)], true),
        &files,
        Vec::new(),
    );
    select(&mut run, "pages", BUFFS_PAGE);
    run.advance(1.0);
    let rows = node_line(&run.ui_dump(), "buffs");
    assert!(
        rows.contains(r#"["Strength Self","Might of the Lugians","default"]"#)
            && rows.contains(r#"["Armor Self","Armor Self I","default"]"#),
        "{rows}"
    );
    run.set(TANK, "main.buff_name", Value::from("Impregnability Self"));
    run.click(TANK, "main.buff_add");
    let rows = node_line(&run.ui_dump(), "buffs");
    assert!(
        rows.contains(r#"["Impregnability Self","Impregnability Self I","added"]"#),
        "{rows}"
    );
    assert_eq!(setting(&mut run, "AddedBuffs"), "Impregnability Self");
    let names: Vec<String> = rows
        .split_once("rows=")
        .map_or("", |(_, r)| r)
        .split("[\"")
        .skip(1)
        .filter_map(|s| s.split('"').next().map(str::to_owned))
        .collect();
    let row_of = |name: &str| {
        i64::try_from(names.iter().position(|n| n == name).expect("listed") + 1).expect("small")
    };
    select(&mut run, "buffs", row_of("Armor Self"));
    run.click(TANK, "main.buff_exclude");
    assert!(
        node_line(&run.ui_dump(), "buffs").contains(r#"["Armor Self","Armor Self I","excluded"]"#),
        "{}",
        run.ui_dump()
    );
    assert_eq!(setting(&mut run, "ExcludedBuffs"), "Armor Self");
    run.click(TANK, "main.buff_include");
    assert_eq!(setting(&mut run, "ExcludedBuffs"), "");
    select(&mut run, "buffs", row_of("Impregnability Self"));
    run.click(TANK, "main.buff_remove");
    assert!(
        !node_line(&run.ui_dump(), "buffs").contains("Impregnability Self"),
        "{}",
        run.ui_dump()
    );
    note_asserted(
        "plugins.samples.the-tanks-buffs-page-lists-its-buffs-and-adds-or-leaves-out-one",
    );
}

/// A table of a VTank settings profile, as the published profiles write it: its name, its
/// columns, a flag a column, its rows' count and each row's typed cells.
fn usd_table(name: &str, columns: &[&str], rows: &[Vec<(&str, String)>]) -> Vec<String> {
    let mut out = vec![name.to_owned(), columns.len().to_string()];
    out.extend(columns.iter().map(|c| (*c).to_owned()));
    out.extend(
        columns
            .iter()
            .enumerate()
            .map(|(i, _)| if i == 0 { "y" } else { "n" }.to_owned()),
    );
    out.push(rows.len().to_string());
    for row in rows {
        for (ty, v) in row {
            out.push((*ty).to_owned());
            out.push(v.clone());
        }
    }
    out
}

fn s(v: &str) -> (&'static str, String) {
    ("s", v.to_owned())
}
fn i(v: i64) -> (&'static str, String) {
    ("i", v.to_string())
}
fn b(v: bool) -> (&'static str, String) {
    ("b", if v { "True" } else { "False" }.to_owned())
}

/// An item's id as a profile writes it.
fn signed(id: u32) -> i64 {
    i64::from(i32::from_ne_bytes(id.to_ne_bytes()))
}

/// A VTank settings profile's other tables, as the profiles its players published hold them: its
/// kits, stones and buff items become the consumables; its buffs added and left out (by their
/// families' first spells) the buffs added and left out; its combat items (by the item's id, with
/// their item buffs, or none) the combat items, one of another character's counted; a monster
/// row's rings, streaks, void curses and weapon (by the item's id) the row's, its damage type at
/// VTank's default read as auto. Saved again, the Tank writes each table as the published ones
/// are written, its items by their ids and its buffs by their families' first spells.
fn the_tank_reads_and_writes_a_vtank_profiles_items_consumables_and_buffs() {
    const SWORD: u32 = 0x8000_0600;
    const WAND: u32 = 0x8000_0601;
    const KIT: u32 = 0x8000_0602;
    const STONE: u32 = 0x8000_0603;
    const GEM: u32 = 0x8000_0604;
    let files = SharedFiles::default();
    let mut run = tank_in(
        PluginRun::new().with_spells(
            &[
                (256, "Impregnability Self I", 4),
                (467, "Missile Weapon Mastery Self I", 4),
                (35, "Blood Drinker I", 3),
                (49, "Swift Killer I", 3),
                (1587, "Heart Seeker I", 3),
                (1599, "Defender I", 3),
                (1475, "Hermetic Link I", 3),
                (3253, "Spirit Drinker I", 3),
                (3810, "Asheron's Benediction", 2),
            ],
            true,
        ),
        &files,
        vec![
            held(SWORD, "Sword", item_type::MELEE_WEAPON),
            held(WAND, "Wand", item_type::CASTER),
            ScriptedObject {
                bitfield: bitfield::HEALER,
                ..held(KIT, "Handy Healing Kit", item_type::MISC)
            },
            held(STONE, "Major Mana Stone", item_type::MANASTONE),
            ScriptedObject {
                spell_id: Some(3810),
                ..held(GEM, "Asheron's Benediction", item_type::GEM)
            },
        ],
    );
    let mut usd: Vec<String> = vec!["8".into()];
    usd.extend(usd_table(
        "AntiExtraBuffSpells",
        &["ExemplarId"],
        &[vec![i(467)]],
    ));
    usd.extend(usd_table(
        "AssistItems",
        &["Object", "Type"],
        &[
            vec![s("Handy Healing Kit"), i(0)],
            vec![s("Major Mana Stone"), i(8)],
        ],
    ));
    usd.extend(usd_table(
        "BuffedItems",
        &["Object", "Spell"],
        &[
            vec![i(signed(SWORD)), i(35)],
            vec![i(signed(SWORD)), i(49)],
            vec![i(signed(WAND)), i(-1)],
            vec![i(-1_903_554_490), i(1599)],
        ],
    ));
    usd.extend(usd_table(
        "ExtraBuffSpells",
        &["ExemplarId"],
        &[vec![i(256)]],
    ));
    usd.extend(usd_table(
        "GemFoodItems",
        &["Name", "Spell"],
        &[vec![s("Asheron's Benediction"), i(3810)]],
    ));
    usd.extend(usd_table("ItemUseSpecifiers", &["Object", "Uses"], &[]));
    let mut monster = vec![s("Drudge Skulker"), i(3), i(8), i(signed(SWORD))];
    // Imperil, Vuln, Yield, GravityW, Attack, Ring, Broadside, Fester, WeakeningCurse,
    // FesteringCurse, Corruption, DestructiveCurse, Corrosion, Streak.
    for on in [
        true, false, false, false, true, true, false, false, false, true, true, false, false, true,
    ] {
        monster.push(b(on));
    }
    monster.extend([i(98), i(0), i(101)]);
    usd.extend(usd_table(
        "MyMonsters",
        &[
            "MonsterName",
            "AttackPriority",
            "DamageType",
            "WeaponToUse",
            "Imperil",
            "Vuln",
            "Yield",
            "GravityW",
            "Attack",
            "Ring",
            "Broadside",
            "Fester",
            "WeakeningCurse",
            "FesteringCurse",
            "Corruption",
            "DestructiveCurse",
            "Corrosion",
            "Streak",
            "SecondaryVuln",
            "SecondaryEquip",
            "PetDamageType",
        ],
        &[monster],
    ));
    usd.extend(usd_table(
        "Settings",
        &["Setting", "Value"],
        &[vec![s("EnableNav"), b(false)]],
    ));
    let mut put = files.clone();
    put.write(TANK, "vt.usd", (usd.join("\r\n") + "\r\n").as_bytes())
        .expect("in memory");
    run.type_line("/tank import vt.usd");
    run.type_line("/tank monsters");
    run.advance(0.5);
    let printed = run.printed();
    let said = |s: &str| printed.iter().any(|l| l.contains(s));
    assert!(
        said("1 settings and 1 monster rows read; 2 combat items (1 of another character's), 3 consumables, 1 buffs added, 1 left out; 0 settings this macro does not have"),
        "{printed:#?}"
    );
    assert!(
        said("Drudge Skulker: priority 3, attack, rings, streaks, damage auto, weapon Sword, debuffs corruption festering imperil"),
        "{printed:#?}"
    );
    assert_eq!(
        setting(&mut run, "Consumables"),
        "Handy Healing Kit, Major Mana Stone, Asheron's Benediction"
    );
    assert_eq!(setting(&mut run, "AddedBuffs"), "Impregnability Self");
    assert_eq!(
        setting(&mut run, "ExcludedBuffs"),
        "Missile Weapon Mastery Self"
    );
    assert_eq!(setting(&mut run, "Weapons"), "Sword, Wand");
    assert_eq!(setting(&mut run, "NoBuffItems"), "Wand");
    // Saved again: each table as the published profiles write it.
    run.type_line("/tank export again.usd");
    run.advance(1.0);
    let text = read(&files, "again.usd").expect("saved");
    let saved = dereth_plugin_host::usdfile::parse(&text).expect("it reads as a VTank profile");
    let column = |table: &str, col: &str| -> Vec<Value> {
        saved
            .get(table)
            .as_list()
            .unwrap_or_else(|| panic!("no {table}: {text}"))
            .iter()
            .map(|r| r.get(col).clone())
            .collect()
    };
    assert_eq!(
        column("AssistItems", "Type"),
        vec![Value::Int(0), Value::Int(8)],
        "{text}"
    );
    assert_eq!(
        column("GemFoodItems", "Spell"),
        vec![Value::Int(3810)],
        "{text}"
    );
    assert_eq!(
        column("ExtraBuffSpells", "ExemplarId"),
        vec![Value::Int(256)]
    );
    assert_eq!(
        column("AntiExtraBuffSpells", "ExemplarId"),
        vec![Value::Int(467)]
    );
    let sword: Vec<Value> = saved
        .get("BuffedItems")
        .as_list()
        .expect("BuffedItems")
        .iter()
        .filter(|r| r.get("Object").as_i64() == Some(signed(SWORD)))
        .map(|r| r.get("Spell").clone())
        .collect();
    assert_eq!(
        sword,
        [35, 49, 1587, 1599].map(Value::Int).to_vec(),
        "the sword's weapon buffs: {text}"
    );
    let wand: Vec<Value> = saved
        .get("BuffedItems")
        .as_list()
        .expect("BuffedItems")
        .iter()
        .filter(|r| r.get("Object").as_i64() == Some(signed(WAND)))
        .map(|r| r.get("Spell").clone())
        .collect();
    assert_eq!(wand, vec![Value::Int(-1)], "the wand never buffed: {text}");
    let row = &saved.get("MyMonsters").as_list().expect("MyMonsters")[0];
    assert_eq!(row.get("DamageType").as_i64(), Some(8));
    assert_eq!(row.get("WeaponToUse").as_i64(), Some(signed(SWORD)));
    assert_eq!(row.get("Ring").as_bool(), Some(true));
    assert_eq!(row.get("FesteringCurse").as_bool(), Some(true));
    note_asserted(
        "plugins.samples.the-tank-reads-and-writes-a-vtank-profiles-items-consumables-and-buffs",
    );
}

/// VTank's [Char] profiles: a profile this character saves is listed on the Settings page among its
/// own, and chosen there it loads.
fn the_tanks_settings_page_loads_this_characters_own_profiles_from_a_list() {
    let (mut run, _) = tank(Vec::new());
    run.type_line("/tank set AttackPower 70");
    run.set(TANK, "main.char_profile", Value::from("hunter"));
    run.click(TANK, "main.savechar");
    run.advance(1.0);
    assert!(
        node_line(&run.ui_dump(), "char_pick").contains(r#"options=["","hunter"]"#),
        "{}",
        run.ui_dump()
    );
    run.type_line("/tank set AttackPower 40");
    run.set(TANK, "main.char_pick", Value::from("hunter"));
    run.click(TANK, "main.loadchar");
    assert_eq!(setting(&mut run, "AttackPower"), "70");
    note_asserted(
        "plugins.samples.the-tanks-settings-page-loads-this-characters-own-profiles-from-a-list",
    );
}

/// VTank's advanced options panel is filterable: the Settings page's list of every setting narrows
/// to those whose name holds what is typed, without case, and a setting picked from the narrowed
/// list is the one set.
fn the_tanks_settings_page_finds_a_setting_by_part_of_its_name() {
    let (mut run, _) = tank(Vec::new());
    run.set(TANK, "main.all_filter", Value::from("corpseapproach"));
    let rows = node_line(&run.ui_dump(), "all");
    let names: Vec<String> = rows
        .split_once("rows=")
        .map_or("", |(_, r)| r)
        .split("[\"")
        .skip(1)
        .filter_map(|s| s.split('"').next().map(str::to_owned))
        .collect();
    assert_eq!(
        names,
        ["CorpseApproachRange-Max", "CorpseApproachRange-Min"],
        "{rows}"
    );
    select(&mut run, "all", 2);
    run.set(TANK, "main.all_value", Value::from("0.01"));
    run.click(TANK, "main.all_set");
    assert_eq!(setting(&mut run, "CorpseApproachRange-Min"), "0.01");
    assert_eq!(
        setting(&mut run, "CorpseApproachRange-Max"),
        "0.08333333333333333"
    );
    note_asserted("plugins.samples.the-tanks-settings-page-finds-a-setting-by-part-of-its-name");
}
