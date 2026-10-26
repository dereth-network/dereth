//! Behaviour: none (tooling: the world editor's Characters tab and its items, over the retail dats)
//! A weenie carried by the playtest character from the Items tab's Add, which lists only what a
//! player can keep in a pack, some of one that stacks, and another dragged from the spawn browser
//! onto the tab; nothing given from a weenie's Definition menu; a chosen weapon its level cannot
//! wield, refused before the server is asked; the kit its archetype carries; where Play As puts
//! it, by the selected object or where it was left. Live, against the editor's own server: the
//! gear the tab shows before anything is made, made exactly by Reset, and one slot rolled again
//! shown and made changing alone; better gear of the tier the tab said, the kit and the items
//! given, the character put at the dungeon's entrance; and the character arriving where it
//! stands, from the camera over an open shaft and at an entrance whose portals name another room.

use dereth_input::host::HostEvent;
use dereth_input::keys::MouseButton;
use lyceum_editor::app::{EditorApp, Mode, Selection, Tab};
use lyceum_editor::characters::{CharacterTab, RecipeItem, RecipeSlot, Start};
use lyceum_foundry::edit::Weenie;

use crate::world::dungeon::keep;
use crate::{click, editor_with, gpu_lock, sample};

/// Server weenies: bread that stacks a hundred and a sword only level 500 could wield, which a
/// player can keep; and a door, a creature, a portal and a sign stuck where it stands, which a
/// player cannot.
const SERVER_ITEMS: &str = "\
INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES (90010, 'testbread', 18, '2005-02-09 10:00:00')
     , (90011, 'testsword', 6, '2005-02-09 10:00:00')
     , (90012, 'testdoor', 19, '2005-02-09 10:00:00')
     , (90013, 'testbeast', 10, '2005-02-09 10:00:00')
     , (90014, 'testgate', 7, '2005-02-09 10:00:00')
     , (90015, 'testsign', 1, '2005-02-09 10:00:00');
INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES (90010, 1, 'Test Bread')
     , (90011, 1, 'Test Sword')
     , (90012, 1, 'Test Door')
     , (90013, 1, 'Test Beast')
     , (90014, 1, 'Test Gate')
     , (90015, 1, 'Test Sign');
INSERT INTO `weenie_properties_bool` (`object_Id`, `type`, `value`)
VALUES (90015, 1, True);
INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`)
VALUES (90010, 1, 0x02000179)
     , (90011, 1, 0x02000179);
INSERT INTO `weenie_properties_int` (`object_Id`, `type`, `value`)
VALUES (90010, 1, 32), (90010, 5, 35), (90010, 11, 100), (90010, 13, 35)
     , (90011, 1, 1), (90011, 5, 450), (90011, 9, 1048576)
     , (90011, 158, 7), (90011, 159, 0), (90011, 160, 500);
";

/// A drag with the pointer from `from` to `to`, in steps, let go there.
fn drag(ed: &mut EditorApp, from: (f32, f32), to: (f32, f32)) {
    let at = |x: f32, y: f32| HostEvent::CursorMoved {
        x: f64::from(x),
        y: f64::from(y),
    };
    ed.queue(at(from.0, from.1));
    ed.frame();
    ed.queue(HostEvent::MouseInput {
        button: MouseButton::Left,
        pressed: true,
    });
    ed.frame();
    for i in 1..=10 {
        #[allow(clippy::cast_precision_loss)] // ten steps
        let k = i as f32 / 10.0;
        ed.queue(at(
            from.0 + (to.0 - from.0) * k,
            from.1 + (to.1 - from.1) * k,
        ));
        ed.frame();
    }
    ed.queue(HostEvent::MouseInput {
        button: MouseButton::Left,
        pressed: false,
    });
    ed.frame();
    ed.frame();
}

/// The tile of weenie `w` in the spawn browser, as drawn last.
fn tile(ed: &EditorApp, w: &Weenie) -> (f32, f32) {
    let r = ed
        .weenie_rects
        .iter()
        .find(|(x, _)| x == w)
        .map(|(_, r)| *r)
        .unwrap_or_else(|| panic!("{w:?} in the spawn browser"));
    (r.center().x, r.center().y)
}

#[test]
fn a_weenie_given_from_its_definition_or_dragged_from_the_spawn_browser_is_carried_by_the_character(
) {
    let _gpu = gpu_lock();
    let (dir, world) = sample("editor-characters-items");
    let sql = dir.path().join("server.sql");
    std::fs::write(&sql, SERVER_ITEMS).unwrap();
    let mut ed = editor_with(
        &world,
        &["--weenies", sql.to_str().unwrap(), "--size", "1600x1000"],
    );
    ed.wait_for_names().unwrap();
    ed.mode = Mode::Spawn;
    // The world view laid out first: the tab floats over its right, clear of the spawn browser.
    for _ in 0..3 {
        assert!(ed.frame());
    }
    ed.open_characters();
    ed.new_character("Gift Taker").unwrap();
    ed.characters.recipe.level = 20;
    for _ in 0..3 {
        assert!(ed.frame());
    }
    let (bread, sword) = (Weenie::Class(90010), Weenie::Class(90011));

    // The bread chosen, shown in Definition: its menu gives nothing to the character.
    let at = tile(&ed, &bread);
    click(&mut ed, at);
    ed.show_tab(Tab::Definition);
    for _ in 0..3 {
        assert!(ed.frame());
    }
    let menu = ed.definition.menu_button.expect("Definition's menu");
    click(&mut ed, (menu.center().x, menu.center().y));
    ed.frame();
    let offered = ed.definition.menu_items.clone();
    assert!(!offered.is_empty(), "the menu is open");
    assert!(
        offered
            .iter()
            .all(|i| !i.contains("Give") && !i.contains("test character")),
        "{offered:?}"
    );
    // Escape closes it.
    crate::hold_key(&mut ed, 0x1B, 1);

    // Add, under Items: only what a player can keep in a pack; the bread, some of it as it
    // stacks.
    ed.show_tab(Tab::Characters);
    ed.characters.tab = CharacterTab::Items;
    for _ in 0..3 {
        assert!(ed.frame());
    }
    let add = ed.characters.add_button.expect("Carried's Add");
    click(&mut ed, (add.center().x, add.center().y));
    ed.characters.picker_search = "test".into();
    for _ in 0..2 {
        assert!(ed.frame());
    }
    let listed: Vec<Weenie> = ed
        .characters
        .picker_rows
        .iter()
        .map(|(w, _)| w.clone())
        .collect();
    assert!(
        listed.contains(&bread) && listed.contains(&sword),
        "{listed:?}"
    );
    for kept_out in [90012, 90013, 90014, 90015] {
        assert!(!listed.contains(&Weenie::Class(kept_out)), "{listed:?}");
    }
    keep(&mut ed, "characters-items", "01-add.png");
    let row = ed
        .characters
        .picker_rows
        .iter()
        .find(|(w, _)| *w == bread)
        .map(|(_, r)| *r)
        .expect("the bread's row");
    click(&mut ed, (row.center().x, row.center().y));
    ed.frame();
    assert_eq!(
        ed.characters.recipe.items,
        [RecipeItem {
            weenie: "90010".into(),
            count: 20
        }],
        "{}",
        ed.panels.message
    );
    let saved = std::fs::read_to_string(world.parent().unwrap().join("characters/Gift Taker.toml"))
        .unwrap();
    assert!(saved.contains("weenie = \"90010\""), "{saved}");

    // The sword dragged from the spawn browser onto the Items tab's button.
    ed.characters.tab = CharacterTab::Build;
    for _ in 0..3 {
        assert!(ed.frame());
    }
    let items = ed
        .characters
        .tab_rects
        .iter()
        .find(|(t, _)| *t == CharacterTab::Items)
        .map(|(_, r)| *r)
        .expect("the Items tab");
    let from = tile(&ed, &sword);
    drag(&mut ed, from, (items.center().x, items.center().y));
    assert_eq!(
        ed.characters
            .recipe
            .items
            .iter()
            .map(|i| (i.weenie.as_str(), i.count))
            .collect::<Vec<_>>(),
        [("90010", 20), ("90011", 1)],
        "one sword: it does not stack"
    );
    // Dropped on the carried items, more bread: one more stack's worth.
    ed.characters.tab = CharacterTab::Items;
    for _ in 0..3 {
        assert!(ed.frame());
    }
    let carried = ed.characters.carried_rect.expect("the carried items");
    let from = tile(&ed, &bread);
    drag(&mut ed, from, (carried.center().x, carried.center().y));
    assert_eq!(ed.characters.recipe.items[0].count, 40);

    // The sword chosen as its weapon: its level cannot wield it, said before the server is asked.
    ed.characters.recipe.slots.insert(
        dereth_progression::gear::Slot::Weapon,
        RecipeSlot {
            roll: 0,
            weenie: Some("90011".into()),
        },
    );
    let found = ed.gear_findings();
    assert!(
        found
            .iter()
            .any(|f| !f.warning && f.text.contains("Test Sword") && f.text.contains("level 500")),
        "{found:?}"
    );
    // The archetype's kit comes with it: the world here has none of its weenies, so it carries
    // only what it was given, and nothing it names is missing.
    assert_eq!(ed.characters.recipe.kit_id(), "adventurer");
    assert!(
        !found.iter().any(|f| f.text.contains("has no weenie")),
        "{found:?}"
    );
    for _ in 0..3 {
        assert!(ed.frame());
    }
    keep(&mut ed, "characters-items", "02-items.png");

    // Where Play As puts it: in front of the selected spawn, on the ground there; where it was
    // left; the dungeon's entrance only in a dungeon.
    let guide = ed
        .session
        .spawns()
        .into_iter()
        .find(|s| !s.server)
        .expect("the pack's guide");
    ed.selection = Some(Selection::Spawn {
        file: guide.file.clone(),
        guid: guide.guid.clone(),
    });
    ed.characters.recipe.start = Start::Selected;
    let (cell, at) = ed.start_point().unwrap().expect("a place");
    assert_eq!(cell >> 16, u32::from(guide.landblock()));
    let o = guide.frame.origin;
    let away = ((at[0] - o.x).powi(2) + (at[1] - o.y).powi(2)).sqrt();
    assert!(
        (away - 2.0).abs() < 0.01,
        "{away} m in front: {at:?} from {o:?}"
    );
    let ground = ed.session.ground(guide.landblock(), at[0], at[1]).unwrap();
    assert!(
        (at[2] - (ground + 0.5)).abs() < 0.01,
        "on the ground: {at:?}"
    );
    ed.characters.recipe.start = Start::Left;
    assert_eq!(ed.start_point().unwrap(), None);
    ed.characters.recipe.start = Start::Entrance;
    assert!(ed.start_point().is_err(), "not in a dungeon");
}

/// A retail dungeon whose creatures are all level 80.
#[cfg(feature = "live-server-tests")]
const LEVEL_80: u16 = 0x0035;

#[cfg(feature = "live-server-tests")]
#[test]
fn in_a_level_80_dungeon_better_gear_and_a_given_item_arrive_at_its_entrance_and_one_slot_rolled_again_changes_alone(
) {
    use dereth_progression::gear::Slot;
    use dereth_progression::order::Made;
    use lyceum_editor::characters::GearChoice;
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    let _g = gpu_lock();
    let (_dir, world) = sample("live-characters-gear");
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
    ed.enter_dungeon(LEVEL_80).unwrap();
    ed.open_characters();
    for _ in 0..5 {
        assert!(ed.frame());
    }
    ed.new_character("Better Geared").unwrap();
    assert_eq!(ed.characters.recipe.level, 80, "the place's level");
    ed.characters.recipe.gear = GearChoice::Better;
    ed.characters.recipe.start = Start::Entrance;
    ed.characters.save().unwrap();
    // Twenty apples, as Add gives them.
    const APPLE: u32 = 258;
    ed.give_to_character(&Weenie::Class(APPLE), 20).unwrap();
    let found = ed.gear_findings();
    assert!(found.iter().all(|f| f.warning), "{found:?}");
    let tier = ed
        .gear_tier()
        .expect("the tier, from the server's creatures");
    for _ in 0..3 {
        assert!(ed.frame());
    }
    keep(&mut ed, "live-characters-gear", "01-better-gear.png");

    ed.set_live(true).unwrap();
    ed.wait_live().unwrap();
    // Play As: it arrives where the dungeon's portals drop it.
    ed.play_until_entered(&[]).unwrap();
    let made = ed.characters.made.clone().expect("what the server made");
    println!("made {made:#?}");
    assert_eq!(made.tier, tier, "the tier the tab said");
    let slots = |m: &Made| -> BTreeMap<Slot, (u32, String)> {
        m.items
            .iter()
            .filter_map(|i| Some((i.gear?, (i.class, i.name.clone()))))
            .collect()
    };
    let gear = slots(&made);
    assert!(
        gear.contains_key(&Slot::Weapon) && gear.contains_key(&Slot::Shield),
        "{gear:?}"
    );
    let jewels = Slot::JEWELRY
        .iter()
        .filter(|s| gear.contains_key(s))
        .count();
    assert!(jewels >= 3, "jewelry for better gear: {gear:?}");
    let count = |class: u32| -> u32 {
        made.items
            .iter()
            .filter(|i| i.class == class)
            .map(|i| i.count)
            .sum()
    };
    assert_eq!(count(APPLE), 20, "the apples given");
    assert_eq!(count(632), 5, "the kit's Peerless Healing Kits");
    for _ in 0..60 {
        assert!(ed.frame());
    }
    keep(&mut ed, "live-characters-gear", "02-at-the-entrance.png");

    // Its weapon rolled again: made again, with only the weapon changed.
    ed.stop_play();
    assert!(ed.wait_world_empty(), "the character logged off");
    ed.characters
        .recipe
        .slots
        .entry(Slot::Weapon)
        .or_default()
        .roll += 1;
    ed.characters.save().unwrap();
    ed.play_until_entered(&[]).unwrap();
    let again = slots(ed.characters.made.as_ref().expect("made again"));
    assert_ne!(gear[&Slot::Weapon], again[&Slot::Weapon]);
    for (slot, item) in gear.iter().filter(|(s, _)| **s != Slot::Weapon) {
        assert_eq!(again.get(slot), Some(item), "{slot:?}");
    }
    ed.stop_play();
    assert!(ed.wait_world_empty(), "the character logged off");
    ed.set_live(false).unwrap();
}

/// The gear's slots and items as the server says them: each slot's class and name, then what is
/// carried.
#[cfg(feature = "live-server-tests")]
fn things(
    m: &dereth_progression::order::Made,
) -> Vec<(Option<dereth_progression::gear::Slot>, u32, String, u32)> {
    m.items
        .iter()
        .map(|i| (i.gear, i.class, i.name.clone(), i.count))
        .collect()
}

#[cfg(feature = "live-server-tests")]
#[test]
fn the_items_tab_shows_the_gear_before_it_is_made_and_reset_makes_exactly_it_and_a_slot_rolled_again(
) {
    use dereth_progression::gear::Slot;
    use std::path::PathBuf;
    let _g = gpu_lock();
    let (dir, world) = sample("live-characters-preview");
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
    ed.open_characters();
    ed.new_character("Shown Gear").unwrap();
    ed.characters.recipe.level = 80;
    ed.characters.save().unwrap();
    ed.characters.tab = CharacterTab::Items;
    ed.set_live(true).unwrap();
    ed.wait_live().unwrap();
    // Before anything is made, the tab shows the gear the server rolls for the recipe.
    let shown = ed
        .wait_preview()
        .unwrap()
        .expect("what the recipe would make");
    println!("shown {shown:#?}");
    assert!(ed.characters.made.is_none(), "nothing made yet");
    assert!(
        shown.items.iter().any(|i| i.gear == Some(Slot::Weapon)),
        "{shown:?}"
    );
    for _ in 0..3 {
        assert!(ed.frame());
    }
    keep(&mut ed, "live-characters-preview", "01-shown.png");
    // Reset makes exactly that.
    ed.make_character().unwrap();
    let made = ed.characters.made.clone().expect("made");
    assert_eq!(things(&made), things(&shown));
    assert_eq!(made.tier, shown.tier);
    // Its weapon rolled again: shown before it is made, the rest as it was; Reset makes it.
    ed.characters
        .recipe
        .slots
        .entry(Slot::Weapon)
        .or_default()
        .roll += 1;
    ed.characters.save().unwrap();
    let again = ed
        .wait_preview()
        .unwrap()
        .expect("what the recipe would make now");
    let weapon = |m: &dereth_progression::order::Made| {
        m.items
            .iter()
            .find(|i| i.gear == Some(Slot::Weapon))
            .map(|i| (i.class, i.name.clone()))
    };
    assert_ne!(weapon(&again), weapon(&shown));
    for i in shown
        .items
        .iter()
        .filter(|i| i.gear.is_some_and(|g| g != Slot::Weapon))
    {
        assert!(
            again
                .items
                .iter()
                .any(|j| j.gear == i.gear && j.class == i.class),
            "{:?} stays: {again:?}",
            i.gear
        );
    }
    let mark = ed.live_log(0).len();
    ed.make_character().unwrap();
    assert!(ed
        .live_log(mark)
        .iter()
        .any(|l| l.contains("Playtest character updated.")));
    assert_eq!(
        things(ed.characters.made.as_ref().expect("made again")),
        things(&again)
    );
    // Reset with the recipe as it is makes it again, from its recipe; no backup is kept.
    let mark = ed.live_log(0).len();
    ed.make_character().unwrap();
    assert!(ed
        .live_log(mark)
        .iter()
        .any(|l| l.contains("Playtest character updated.")));
    assert!(!dir
        .path()
        .join(".playtest")
        .join("character-backups")
        .exists());
    for _ in 0..3 {
        assert!(ed.frame());
    }
    keep(&mut ed, "live-characters-preview", "02-made.png");
    ed.set_live(false).unwrap();
}

/// Sand Caves: its portals drop players over an open shaft, twelve meters above its floor.
#[cfg(feature = "live-server-tests")]
const SAND_CAVES: u16 = 0x5760;

/// Tenkarrdun Foundry: its portals name a room beside the one their place is in.
#[cfg(feature = "live-server-tests")]
const TENKARRDUN_FOUNDRY: u16 = 0x0116;

#[cfg(feature = "live-server-tests")]
#[test]
fn play_as_from_the_camera_over_an_open_shaft_and_at_an_entrance_naming_another_room_arrives_where_it_stands(
) {
    use std::path::PathBuf;
    let _g = gpu_lock();
    let (_dir, world) = sample("live-characters-landings");
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
    // Gone into Sand Caves, the camera is at its drop point, in the shaft: the character starts
    // on the floor at the shaft's bottom, not on the ground of the landblock.
    ed.enter_dungeon(SAND_CAVES).unwrap();
    ed.open_characters();
    ed.new_character("Sand Diver").unwrap();
    assert_eq!(ed.characters.recipe.start, Start::Camera);
    let (cell, at) = ed.start_point().unwrap().expect("a place");
    assert_eq!(cell, 0x5760_0133, "{at:?}");
    assert!(
        (at[2] + 17.5).abs() < 0.01,
        "half a meter over the shaft's floor: {at:?}"
    );
    ed.set_live(true).unwrap();
    ed.wait_live().unwrap();
    ed.play_until_entered(&[]).unwrap();
    assert_eq!(ed.play_body().map(|b| b.cell.0), Some(0x5760_0133));
    ed.stop_play();
    assert!(ed.wait_world_empty(), "the character logged off");

    // At the entrance of a dungeon whose portals name a room beside the one their place is in:
    // the character starts in the room the game puts it in there.
    ed.enter_dungeon(TENKARRDUN_FOUNDRY).unwrap();
    assert_eq!(
        ed.drop_point(TENKARRDUN_FOUNDRY).unwrap().0,
        0x0116_042F,
        "the room its portals name"
    );
    ed.characters.recipe.start = Start::Entrance;
    ed.characters.save().unwrap();
    let (cell, at) = ed.start_point().unwrap().expect("a place");
    assert_eq!(cell, 0x0116_0433, "{at:?}");
    ed.play_until_entered(&[]).unwrap();
    assert_eq!(ed.play_body().map(|b| b.cell.0), Some(0x0116_0433));
    ed.stop_play();
    assert!(ed.wait_world_empty(), "the character logged off");
    ed.set_live(false).unwrap();
}
