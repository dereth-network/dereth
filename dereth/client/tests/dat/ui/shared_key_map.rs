//! One set of keys, a key map per interface: the retail interface keeps its key map file over the
//! final client's shipped maps, the classic interface its own file over the 2004 default scheme;
//! a key bound in one leaves the other alone; a cleared key stays cleared when the file is read
//! again; the classic map starts from the player's keys of the one map both interfaces kept; and
//! either interface can take the other's keys, or either default scheme, whole.
//! Fixture: the retail dats' input maps, driven through `InputShell` with key map files in a
//! folder of the test's own.

use dereth_classic_ui::keystore::{KeyStoreRequest, Scheme};
use dereth_client::input::{InputShell, CLASSIC_KEYMAP_FILE};
use dereth_input::binding::DO_NOTHING;
use dereth_input::{ActionId, InputMapId};

/// The scan codes of Q, W and F7.
const Q: u16 = 0x10;
const W: u16 = 0x11;
const F7: u16 = 0x41;
const MOVEMENT: InputMapId = InputMapId(4);
const FORWARD: ActionId = ActionId(0x29);

/// A folder of the test's own, empty.
fn folder(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "dereth-keys-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a folder");
    dir
}

fn shell(dir: &std::path::Path) -> InputShell {
    let store = dereth_dat::testing::open_store().expect("required DATs");
    InputShell::new(&store, Some(&dir.join("dereth.keymap"))).expect("the input tables")
}

/// The classic keys `(scan, modifiers, action, map)`, sorted.
fn classic(input: &mut InputShell) -> Vec<(u16, u8, u32, u32)> {
    let mut v: Vec<_> = input
        .classic_keys()
        .bindings
        .iter()
        .map(|b| (b.scan, b.modifiers, b.action, b.map))
        .collect();
    v.sort_unstable();
    v
}

/// The classic keys as the retail map writes them, `(scan, meta, action, map)`, sorted.
fn classic_as_retail(input: &mut InputShell) -> Vec<(u16, u32, u32, u32)> {
    let mut v: Vec<_> = classic(input)
        .into_iter()
        .map(|(s, m, a, map)| (s, dereth_classic_ui::keystore::meta_of_modifiers(m), a, map))
        .collect();
    v.sort_unstable();
    v
}

/// The retail map's keyboard keys `(scan, meta, action, map)` of the key pages' rows, the
/// cleared ones left out, sorted.
fn retail(input: &InputShell) -> Vec<(u16, u32, u32, u32)> {
    let mut v: Vec<_> = dereth_input::scheme::keyboard_bindings(&input.manager.keymap)
        .filter(|(m, _, a)| dereth_input::presentation::find(*m, *a).is_some())
        .map(|(m, k, a)| (k.control.offset(), k.meta_mode, a.0, m.0))
        .collect();
    v.sort_unstable();
    v
}

fn has(keys: &[(u16, u8, u32, u32)], scan: u16, action: u32) -> bool {
    keys.iter()
        .any(|k| k.0 == scan && k.1 == 0 && k.2 == action)
}

/// Behaviour: keys.shared.each-interface-keeps-its-own-key-map
#[test]
fn each_interface_keeps_its_own_key_map_and_takes_the_others_whole() {
    let dir = folder("own");
    let mut input = shell(&dir);
    let research = dereth_client_contract::actions::dereth::TOGGLE_SPELL_RESEARCH_PANEL;

    // The classic default scheme is the 2004 map: W walks, F7 the research page, Escape cancels.
    let keys = classic(&mut input);
    assert!(has(&keys, W, FORWARD.0));
    assert!(has(&keys, F7, research));
    assert!(has(
        &keys,
        0x01,
        dereth_client_contract::actions::dereth::CANCEL
    ));
    assert!(
        dir.join(CLASSIC_KEYMAP_FILE).is_file(),
        "written the first time it is wanted"
    );

    // A classic rebind: Q walks forward in place of W. The retail map is unchanged.
    let before = retail(&input);
    input.classic_request(KeyStoreRequest::Bind {
        scan: Q,
        modifiers: 0,
        action: FORWARD.0,
        map: MOVEMENT.0,
        replaced: Some((W, 0)),
    });
    let keys = classic(&mut input);
    assert!(has(&keys, Q, FORWARD.0) && !has(&keys, W, FORWARD.0));
    assert_eq!(
        retail(&input),
        before,
        "the retail keys are the retail keys"
    );

    // The classic interface takes the retail defaults whole, and the retail one the classic map.
    input.classic_request(KeyStoreRequest::Load(Scheme::RetailDefaults));
    assert_eq!(
        classic_as_retail(&mut input),
        retail(&input)
            .into_iter()
            .filter(|k| dereth_classic_ui::keystore::modifiers_of_meta(k.1).is_some())
            .collect::<Vec<_>>()
    );
    input.classic_request(KeyStoreRequest::Load(Scheme::ClassicDefaults));
    assert!(input.load_retail_scheme(&Scheme::File("dereth-classic".into())));
    assert_eq!(
        retail(&input),
        classic_as_retail(&mut input),
        "the retail map is the classic map, key for key"
    );
    // What no key page lists stays the retail interface's: Enter still sends in a text box.
    assert!(input
        .manager
        .keymap
        .section(InputMapId(7))
        .is_some_and(|s| s
            .bindings()
            .iter()
            .any(|(k, a)| k.control.offset() == 0x1C && a.0 == 0x25)));
    assert!(input.restore_shipped_keys());
    assert_eq!(retail(&input), before);
    let _ = std::fs::remove_dir_all(dir);
}

/// Behaviour: keys.shared.a-cleared-key-stays-cleared-when-the-game-starts-again
#[test]
fn a_cleared_key_stays_cleared_in_both_interfaces_when_the_game_starts_again() {
    let dir = folder("clear");
    let mut input = shell(&dir);
    // The retail page clears W from walking forward, as its clear button does.
    let w = dereth_input::scheme::keyboard_key(&input.manager.keymap, W, 0).expect("a key");
    let held = input
        .keys_for_action(FORWARD, MOVEMENT)
        .into_iter()
        .find(|k| k.control == w.control)
        .expect("W walks forward");
    input.manager.unbind_by_key(&held, MOVEMENT);
    input.manager.bind_action(held, DO_NOTHING, MOVEMENT);
    // The classic page clears X from backing up.
    input.classic_request(KeyStoreRequest::Clear {
        scan: 0x2D,
        modifiers: 0,
        action: 0x2A,
        map: MOVEMENT.0,
    });
    assert!(input.save_keymap().expect("written"));
    drop(input);

    let mut again = shell(&dir);
    assert!(
        again
            .keys_for_action(FORWARD, MOVEMENT)
            .iter()
            .all(|k| k.control != w.control),
        "W does not walk again"
    );
    assert!(
        !has(&classic(&mut again), 0x2D, 0x2A),
        "X does not back up again"
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// Behaviour: keys.shared.each-interface-keeps-its-own-key-map
#[test]
fn the_classic_map_starts_from_the_players_keys_of_the_one_map_both_kept() {
    let dir = folder("migrate");
    let mut input = shell(&dir);
    // The player's keys in the one map: Q walks forward in place of W, and Ctrl+F7 opens the
    // research page.
    let key = |input: &InputShell, scan, meta| {
        dereth_input::scheme::keyboard_key(&input.manager.keymap, scan, meta).expect("a key")
    };
    let q = key(&input, Q, 0);
    let w = input
        .keys_for_action(FORWARD, MOVEMENT)
        .iter()
        .position(|k| k.control.offset() == W)
        .expect("W walks forward");
    assert!(input.set_binding(MOVEMENT, FORWARD, Some(w), q));
    let research = ActionId(dereth_client_contract::actions::dereth::TOGGLE_SPELL_RESEARCH_PANEL);
    let ctrl_f7 = key(&input, F7, 0x4000_0000);
    assert!(input.set_binding(dereth_input::dereth::INPUT_MAP, research, None, ctrl_f7));
    assert!(input.save_keymap().expect("written"));
    assert!(
        !dir.join(CLASSIC_KEYMAP_FILE).exists(),
        "no classic map yet"
    );

    let keys = classic(&mut input);
    assert!(has(&keys, Q, FORWARD.0), "the player's Q");
    assert!(!has(&keys, W, FORWARD.0), "the W the player took away");
    assert!(keys.iter().any(|k| *k
        == (
            F7,
            dereth_classic_ui::keystore::CTRL,
            research.0,
            dereth_input::dereth::INPUT_MAP.0
        )));
    assert!(has(&keys, F7, research.0), "and the 2004 map's own F7");
    let _ = std::fs::remove_dir_all(dir);
}
