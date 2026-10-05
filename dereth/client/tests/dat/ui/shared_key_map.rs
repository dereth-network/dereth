//! One set of keys, a key map per interface: the modern interface keeps its key map file over the
//! final client's shipped maps, the classic interface its own file over the 2004 default scheme;
//! a key bound in one leaves the other alone; a cleared key stays cleared when the file is read
//! again; and each interface's saved key maps are its own, `<name>-modern.keymap` and
//! `<name>-classic.keymap`, each listed only by its own interface.
//! Fixture: the retail dats' input maps, driven through `InputShell` with key map files in a
//! folder of the test's own.

use dereth_classic_ui::keystore::{KeyStoreRequest, Scheme};
use dereth_input::binding::DO_NOTHING;
use dereth_input::{ActionId, InputMapId};
use {dereth_client_shell::input::InputShell, dereth_client_shell::input::CLASSIC_KEYMAP_FILE};

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
    InputShell::new(
        &store,
        Some(&dir.join(dereth_client_shell::input::DEFAULT_KEYMAP_FILE)),
    )
    .expect("the input tables")
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
fn each_interface_keeps_its_own_key_map_and_its_own_saved_ones() {
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

    // A classic rebind: Q walks forward in place of W. The modern map is unchanged.
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
    assert!(dir.join(CLASSIC_KEYMAP_FILE).is_file(), "written at once");
    assert_eq!(
        retail(&input),
        before,
        "the modern keys are the modern keys"
    );

    // Saved under a name, each interface's map is that interface's file, and each lists only
    // its own.
    input.classic_request(KeyStoreRequest::SaveAs {
        name: "bananas".into(),
        overwrite: false,
    });
    assert_eq!(
        input.save_keymap_as("bananas", false).expect("written"),
        Some(dereth_client_shell::input::SaveKeymapAs::Saved)
    );
    assert!(dir.join("bananas-classic.keymap").is_file());
    assert!(dir.join("bananas-modern.keymap").is_file());
    assert_eq!(input.classic_keys().files, ["bananas"]);
    assert_eq!(
        input
            .scheme_names(dereth_client_shell::input::MODERN_SLUG)
            .expect("listed"),
        ["bananas"]
    );
    assert_eq!(input.modern_scheme_in_use().as_deref(), Some("bananas"));

    // Default, then the saved one back.
    input.classic_request(KeyStoreRequest::Load(Scheme::Default));
    assert!(has(&classic(&mut input), W, FORWARD.0));
    input.classic_request(KeyStoreRequest::Load(Scheme::File("bananas".into())));
    let keys = classic(&mut input);
    assert!(has(&keys, Q, FORWARD.0) && !has(&keys, W, FORWARD.0));
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
