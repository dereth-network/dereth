//! The shipped action map and both master input maps, decoded straight off the retail dats.
//!
//! The action map (`0x26000000`, `client_portal.dat`) and the two master input maps
//! (`0x14000000`, `DefaultMap 0x14000002`) each decode with the cursor landing exactly
//! on the payload end, and the decoded catalogue has the counts and conflict clusters below.
//! Offline apart from the dats: no window, no renderer, no server.
//!
//! Behaviour: none (decoder conformance of the shipped input maps)

use std::collections::BTreeSet;

use dereth_input::actionmap::ActionMap;
use dereth_input::keymap::MasterInputMap;
use dereth_input::spec::{ControlCode, DeviceType};
use dereth_input::InputMapId;

use super::shipped::shipped;

/// **Parse.** The action map and both master input maps decode with the cursor landing exactly on
/// the payload end — 12303/12303, 2391/2391 and 1005/1005 bytes. Any shortfall or overrun fails,
/// because the reader's end-of-payload check is what a decoder is proved by.
#[test]
fn parse_consumes_every_payload_exactly() {
    let f = shipped();

    let am_bytes = &f.actionmap;
    assert_eq!(am_bytes.len(), 12_303);
    let am = ActionMap::read(am_bytes).expect("the ActionMap must decode");

    let gm_bytes = &f.keymap_gm;
    assert_eq!(gm_bytes.len(), 2_391);
    let gm = MasterInputMap::read(gm_bytes).expect("keymap 0x14000000 must decode");

    let dm_bytes = &f.keymap_default;
    assert_eq!(dm_bytes.len(), 1_005);
    let dm = MasterInputMap::read(dm_bytes).expect("DefaultMap must decode");

    // A truncated payload must be an error, not a panic and not a silent short read.
    assert!(ActionMap::read(&am_bytes[..am_bytes.len() - 1]).is_err());
    assert!(MasterInputMap::read(&gm_bytes[..gm_bytes.len() - 1]).is_err());

    assert_eq!(am.entries().count(), 389, "389 (input map, action) entries");
    assert_eq!(
        am.entries()
            .map(|(_, a, _)| a)
            .collect::<BTreeSet<_>>()
            .len(),
        374,
        "374 distinct action ids -- a few actions appear in two maps"
    );
    assert_eq!(am.input_maps().count(), 27, "27 input maps");
    assert_eq!(
        am.entries()
            .filter(|(m, a, _)| am.is_user_bindable(*m, *a))
            .count(),
        306,
        "306 of the 389 entries are user-bindable"
    );
    assert_eq!(am.string_table, 0x2300_0005);
    assert_eq!(am.conflict_count(), 16, "the conflict table has 16 entries");

    // Startup keeps its current document header while replacing bindings. A loaded scheme
    // constructs a fresh document, including when malformed user text falls back to defaults.
    let mut manager = dereth_input::InputManager::on_startup(am_bytes, dm_bytes).expect("startup");
    manager.keymap.did = 73;
    manager.keymap.name = "retained header".to_owned();
    manager.keymap.guid = [19; 16];
    manager
        .init_keymap(Some("invalid keymap"), gm_bytes, dm_bytes)
        .expect("defaults");
    assert_eq!(
        (
            manager.keymap.did,
            manager.keymap.name.as_str(),
            manager.keymap.guid
        ),
        (73, "retained header", [19; 16])
    );
    let loaded = dereth_input::InputManager::load_over_defaults(
        Some("invalid keymap"),
        &[&gm, &dm],
        Some(&am),
    );
    assert_eq!(
        (loaded.did, loaded.name.as_str(), loaded.guid),
        (0, "User Defined Keymap", [0; 16])
    );
    let mut expected = loaded;
    expected.create_input_map(dereth_input::dereth::INPUT_MAP);
    expected.did = manager.keymap.did;
    expected.name.clone_from(&manager.keymap.name);
    expected.guid = manager.keymap.guid;
    assert_eq!(manager.keymap.to_keymap_text(), expected.to_keymap_text());

    assert_eq!(gm.name, "gmDefaultMap");
    assert_eq!(dm.name, "DefaultMap");
    for (map, want, count) in [(&gm, "gmDefaultMap", 133), (&dm, "DefaultMap", 51)] {
        let n: usize = map.sections.iter().map(|s| s.bindings().len()).sum();
        assert_eq!(n, count, "{want} binding count");
        // Both keymaps declare exactly two devices, keyboard then mouse.
        assert_eq!(map.devices.len(), 2);
        assert_eq!(map.devices[0].device_type, DeviceType::Keyboard);
        assert_eq!(map.devices[1].device_type, DeviceType::Mouse);
    }
    // Nine meta keys in keymap 0x14000000, seven in DefaultMap,
    // and DIK_RSHIFT (0x36) is absent from both because generate_keyboard_event folds it onto 0x2A.
    assert_eq!(gm.meta_keys.len(), 9);
    assert_eq!(dm.meta_keys.len(), 7);
    for map in [&gm, &dm] {
        assert!(!map.meta_keys.iter().any(|(k, _)| k.offset() == 0x36));
        assert_eq!(
            map.meta_mode_from_key(ControlCode(0x002A_0000)),
            0x8000_0000
        );
    }

    // The conflict table's two clusters: the text/edit group lists only itself and its two
    // siblings, and the three combat-mode maps deliberately do not list each other.
    let text_cluster: BTreeSet<u32> = am
        .conflicting_input_maps(InputMapId(7))
        .iter()
        .map(|m| m.0)
        .collect();
    assert_eq!(text_cluster, BTreeSet::from([7, 8, 0xA]));
    let melee: BTreeSet<u32> = am
        .conflicting_input_maps(InputMapId(0x1000_0003))
        .iter()
        .map(|m| m.0)
        .collect();
    assert!(
        !melee.contains(&0x1000_0004),
        "melee must not conflict with missile"
    );
    assert!(
        !melee.contains(&0x1000_0005),
        "melee must not conflict with magic"
    );
}

/// **The key pages' one set of rows against the shipped action map.** Every user-bindable entry
/// is a row, apart from the quickslots 10 to 18 and the quest detail panel's toggle; the one other
/// row is Disable Most Weather Effects, which the action map has in the character options map but
/// gives no tab.
#[test]
fn the_key_pages_rows_are_the_bindable_entries_and_the_clients_own() {
    use dereth_input::presentation::{find, ROWS};
    let f = shipped();
    let mut am = ActionMap::read(&f.actionmap).expect("the ActionMap must decode");
    am.add_dereth_actions();
    let left_out: BTreeSet<String> = (10..=18)
        .map(|n| format!("UseQuickSlot_{n}"))
        .chain(["ToggleQuestManagementPanel".to_owned()])
        .collect();
    let mut listed = 0;
    for (map, action, _) in am.entries() {
        if !am.is_user_bindable(map, action) {
            continue;
        }
        let name = dereth_input::names::enum_name_for_action(action);
        if left_out.contains(&name) {
            assert!(find(map, action).is_none(), "{name} is not a row");
        } else {
            assert!(find(map, action).is_some(), "{name} in {map:?} is a row");
            listed += 1;
        }
    }
    let unbindable: Vec<&str> = ROWS
        .iter()
        .filter(|r| !am.is_user_bindable(r.input_map(), r.action()))
        .map(|r| r.action_name)
        .collect();
    assert_eq!(unbindable, ["PlayerOption_DisableMostWeatherEffects"]);
    let weather = ROWS
        .iter()
        .find(|r| r.action_name == unbindable[0])
        .expect("the weather row");
    assert!(am.is_action_allowed_in_input_map(weather.input_map(), weather.action()));
    assert_eq!(listed + 1, ROWS.len());
}

/// **A row the modern interface does nothing with has no default key in it**: neither shipped
/// map, nor this client's own defaults, binds any of the five.
///
/// Behaviour: keys.retail.a-row-this-interface-does-not-use-has-no-default-key
#[test]
fn every_row_the_retail_interface_does_not_use_has_no_default_key() {
    use dereth_input::presentation::{Interface, ROWS};
    let f = shipped();
    let gm = MasterInputMap::read(&f.keymap_gm).expect("keymap 0x14000000 must decode");
    let mut dm = MasterInputMap::read(&f.keymap_default).expect("DefaultMap must decode");
    dm.create_input_map(dereth_input::dereth::INPUT_MAP);
    let unused: Vec<_> = ROWS
        .iter()
        .filter(|r| r.not_used(Interface::Modern).is_some())
        .collect();
    assert_eq!(unused.len(), 5);
    for r in unused {
        for m in [&gm, &dm] {
            assert!(
                m.section(r.input_map())
                    .is_none_or(|s| s.keys_for_action(r.action()).is_empty()),
                "{} has a default key",
                r.action_name
            );
        }
    }
}
