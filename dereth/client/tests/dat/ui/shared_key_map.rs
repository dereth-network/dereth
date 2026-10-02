//! One key map for every interface: a key the classic page binds goes into the same map the retail
//! key page edits, in the map its action belongs to, in place of whatever the key did there; the
//! player's own keys are what differs from the shipped defaults, and they survive the key map
//! file; this client's own actions are written by name; the shipped defaults drop them.
//! Fixture: the retail dats' input maps, driven through `InputShell`.

use dereth_input::{ActionId, InputMapId};

/// The scan codes of Q, W and F8.
const Q: u16 = 0x10;
const W: u16 = 0x11;
const F8: u16 = 0x42;

/// Behaviour: keys.shared.one-key-map-for-every-interface
#[test]
fn a_key_bound_for_one_interface_is_the_shared_maps_and_survives_its_file() {
    let forward = ActionId(0x29);
    let store = dereth_dat::testing::open_store().expect("required DATs");
    let mut input = dereth_client::input::InputShell::new(&store, None).unwrap();
    assert!(
        input.player_bindings().is_empty(),
        "nothing of the player's yet"
    );
    assert!(input.removed_bindings().is_empty());

    // The classic page moves walking forward from W to Q.
    assert_eq!(input.home_map(forward), InputMapId(4), "the movement map");
    assert!(input.bind_key(Q, forward, Some(W)));
    let offsets = |input: &dereth_client::input::InputShell| -> Vec<u16> {
        input
            .keys_for_action(forward, InputMapId(4))
            .iter()
            .map(|k| k.control.offset())
            .collect()
    };
    assert!(offsets(&input).contains(&Q));
    assert!(!offsets(&input).contains(&W));
    assert!(input.player_bindings().contains(&(Q, forward)));
    assert!(input.removed_bindings().contains(&(W, forward)));

    // One of this client's own actions, on F8, which the attributes window had.
    let stretch = ActionId(dereth_client_contract::actions::dereth::TOGGLE_STRETCH_UI);
    assert_eq!(input.home_map(stretch), dereth_input::dereth::INPUT_MAP);
    input.bind_key(F8, stretch, None);
    assert!(
        input
            .player_bindings()
            .iter()
            .all(|(s, a)| *s != F8 || *a == stretch),
        "F8 does one thing"
    );

    // The key map file holds both, the own action by its name.
    let text = input.manager.keymap.to_keymap_text();
    assert!(text.contains("ToggleStretchUI"));
    let back = dereth_input::MasterInputMap::from_keymap_text(&text).expect("it reads back");
    let has = |map: InputMapId, scan: u16, a: ActionId| {
        back.section(map).is_some_and(|s| {
            s.bindings()
                .iter()
                .any(|(k, x)| k.control.offset() == scan && *x == a)
        })
    };
    assert!(has(InputMapId(4), Q, forward));
    assert!(has(dereth_input::dereth::INPUT_MAP, F8, stretch));

    // The shipped defaults: nothing of the player's.
    assert!(input.restore_shipped_keys());
    assert!(input.player_bindings().is_empty());
    assert!(offsets(&input).contains(&W));
}
