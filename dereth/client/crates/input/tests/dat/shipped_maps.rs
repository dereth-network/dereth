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
    // the recovered binding behavior §2: nine meta keys in keymap 0x14000000, seven in DefaultMap,
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
