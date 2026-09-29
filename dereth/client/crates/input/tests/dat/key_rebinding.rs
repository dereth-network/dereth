//! A key can be rebound, the rebinding persists the way the retail client persists it, and the
//! action that fires changes in both directions: the new key fires it and the old key stops.
//!
//! Fixture: the shipped `ActionMap` (`0x26000000`, 12,303 bytes) and both master keymaps
//! (`0x14000000`, 2,391 bytes; `DefaultMap 0x14000002`, 1,005 bytes), read from the
//! retail dats through `shipped`. Nothing here fabricates an `ActionMap`: the conflict table (16
//! entries) and the user-bindable flags are exactly the data the capture policy consults. Offline
//! apart from the dats; absent dats are a failure.
//!
//! Three things a one-sided test would miss:
//!
//! 1. **The old key must stop firing.** Asserted on `InputManager::take_events` after a real
//!    `WM_KEYDOWN`, not on the stored table: a build that bound everything to everything would pass
//!    a "the new key works" test.
//! 2. **It must still not fire after a save and reload.** The merge adds what is *absent*, and
//!    merges the user file first and keymap `0x14000000` second, so a control the user file does
//!    not mention gets its shipped default back. The freed key survives that only because the
//!    rebind stores [`dereth_input::binding::DO_NOTHING`] instead of deleting it.
//! 3. **The action's *other* key must be untouched.** *Move Forward* ships with two controls in
//!    map 4 (`DIK_W` and `DIK_UP`); replacing one row slot must not clear the row.

use std::path::PathBuf;

use dereth_input::binding::{Capture, DO_NOTHING};
use dereth_input::dispatch::priority;
use dereth_input::spec::{activation, ControlCode, SubControlIndex};
use dereth_input::win32::{msg, Win32Message};
use dereth_input::{ActionId, ControlChord, InputManager, InputMapId};

use super::shipped::{shipped, Payloads};

// ---------------------------------------------------------------------------------------------
// The literals, in one place, taken from the shipped keymap rather than from our own symbols
// ---------------------------------------------------------------------------------------------

/// The movement map.
const MOVEMENT: InputMapId = InputMapId(4);
/// The UI command map.
const UI_COMMANDS: InputMapId = InputMapId(0x1000_0009);
/// The item-selection command map.
const ITEM_SELECTION: InputMapId = InputMapId(0x1000_0007);

/// `MovementForward`. The shipped binding: control `0x00110000` (`DIK_W`), meta mode `0`,
/// activation `0x03`, input map `0x00000004`, action `0x00000029`, in keymap `0x14000000`.
const MOVE_FORWARD: ActionId = ActionId(0x29);
/// `USE`, bound to control `0x00130000` (`DIK_R`) in map `0x10000009` of keymap `0x14000000` —
/// offset `0x43F` of that keymap, and absent from keymap `0x14000002`.
const USE: ActionId = ActionId(0x1000_0025);
/// `SelectionPickUp`, control `0x00210000` (`DIK_F`) in map `0x10000007` of keymap `0x14000000` —
/// offset `0x207`.
const SELECTION_PICK_UP: ActionId = ActionId(0x1000_002C);

/// DirectInput scan codes, which are the high half of a keyboard `ControlCode`.
const DIK_ESCAPE: u16 = 0x01;
const DIK_W: u16 = 0x11;
const DIK_R: u16 = 0x13;
const DIK_F: u16 = 0x21;
const DIK_UP: u16 = 0xC8;
/// The one letter-or-function key **no** shipped binding uses, which is why the rebind target is
/// F7 and not something more obvious: every letter A–Z is bound in keymap `0x14000000`.
const DIK_F7: u16 = 0x41;

/// A keyboard control with no modifier and `Click` activation — the shape every keyboard binding
/// in both shipped keymaps has, and the shape forces a capture into.
fn keyboard(offset: u16) -> ControlChord {
    ControlChord::new(
        ControlCode::new(0, SubControlIndex::None, offset),
        0,
        activation::CLICK,
    )
}

/// The same key **as an event**, on its release.
///
/// This distinction is the capture handler's first rule. A *binding* carries `Click` (3); an
/// *event* carries the activation that produced it, which is
/// `Down` (1) on the press and `Up` (2) on the release. `Click` is `Down | Up` as a mask, so
/// offering a binding-shaped control to the capture would trip `activation & 0x81` and be
/// [`Capture::Ignored`] — the client never does that because it hands the handler the event. Step 4
/// is what turns the release back into a `Click` binding.
fn released(offset: u16) -> ControlChord {
    ControlChord::new(
        ControlCode::new(0, SubControlIndex::None, offset),
        0,
        activation::UP,
    )
}

/// Device start-up then the client's keymap init, with the
/// movement, UI-command and item-selection maps registered as
/// `dereth_client::input::BASE_MAP_REGISTRATIONS` registers them.
fn manager(f: &Payloads, user_keymap: Option<&str>) -> InputManager {
    let (am, gm, dm) = (&f.actionmap, &f.keymap_gm, &f.keymap_default);
    let mut m = InputManager::on_startup(am, dm).expect("startup");
    m.init_keymap(user_keymap, gm, dm).expect("keymap init");
    m.keymap.devices = dereth_input::keymap::MasterInputMap::read(gm)
        .expect("gm")
        .devices;
    let cb = m.new_callback();
    for map in [MOVEMENT, UI_COMMANDS, ITEM_SELECTION] {
        m.register_input_map(map, priority::GAMEPLAY, cb);
    }
    m.has_focus = true;
    m
}

/// Press and release one key through the window-message path and answer which
/// actions `fire_input_event` produced.
///
/// **This is the oracle**: the action that fires, out of the real map walk, not the contents of the
/// table the walk reads.
fn press(m: &mut InputManager, scan: u16, t: u32) -> Vec<ActionId> {
    let no_lead = |_: u8| false;
    let down = Win32Message::new(
        msg::WM_KEYDOWN,
        0,
        ((u32::from(scan) << 16) | 1) as isize,
        t,
    );
    let up = Win32Message::new(
        msg::WM_KEYUP,
        0,
        ((u32::from(scan) << 16) | 0xC000_0001) as isize,
        t + 20,
    );
    m.on_window_event(&down, &no_lead);
    m.on_window_event(&up, &no_lead);
    m.take_events().into_iter().map(|e| e.action).collect()
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("dereth-key-rebinding");
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    dir.join(name)
}

// ------------------------------------------------------------------------------------------- 1

/// **The shipped bindings this file reasons about, pinned as literals.**
///
/// A test that reads a constant through the same symbol it writes it through cannot detect a wrong
/// constant. Every other assertion in this file goes through
/// `MOVE_FORWARD` / `USE` / `SELECTION_PICK_UP`; this one goes through the numbers and through the
/// **bytes of the shipped keymap**, at the two offsets the bindings sit at.
#[test]
fn the_shipped_bindings_are_where_the_bytes_say_they_are() {
    let f = shipped();
    let (gm, dm) = (&f.keymap_gm, &f.keymap_default);

    // `DIK_R -> USE 0x10000025` at offset 0x43F and `DIK_F -> SelectionPickUp 0x1000002C` at
    // 0x207 of keymap `0x14000000`, and neither id anywhere in
    // keymap `0x14000002` (`DefaultMap`) -- the shipped bindings live in the *gm* map.
    // `0x43F` and `0x207` are the **control specification** words; the action id is 12 bytes
    // further on in the same record. Both are pinned: the offsets locate the controls and the
    // ids are what the bindings mean.
    let at = |buf: &[u8], off: usize| u32::from_le_bytes(buf[off..off + 4].try_into().unwrap());
    assert_eq!(
        at(gm, 0x43F),
        0x0013_0000,
        "DIK_R as a control binding specification"
    );
    assert_eq!(
        at(gm, 0x44B),
        0x1000_0025,
        "USE, 12 bytes after its control"
    );
    assert_eq!(at(gm, 0x207), 0x0021_0000, "DIK_F");
    assert_eq!(at(gm, 0x213), 0x1000_002C, "SelectionPickUp, likewise");
    for id in [0x1000_0025_u32, 0x1000_002C, 0x29] {
        assert!(
            !dm.windows(4)
                .any(|w| u32::from_le_bytes(w.try_into().unwrap()) == id),
            "{id:#010X} must be absent from DefaultMap"
        );
    }

    // And the decoded map agrees with the bindings quoted at the top of this file.
    let m = manager(f, None);
    assert_eq!(
        m.find_keys_for_action(USE, UI_COMMANDS),
        vec![keyboard(DIK_R)]
    );
    assert_eq!(
        m.find_keys_for_action(SELECTION_PICK_UP, ITEM_SELECTION),
        vec![keyboard(DIK_F)]
    );
    let fwd = m.find_keys_for_action(MOVE_FORWARD, MOVEMENT);
    assert_eq!(
        fwd.len(),
        2,
        "Move Forward ships with two controls: {fwd:?}"
    );
    assert!(fwd.contains(&keyboard(DIK_W)) && fwd.contains(&keyboard(DIK_UP)));

    // `DoNothing` is action 1 and no shipped keymap binds it -- it exists to be written into a
    // *user's* file. Both halves, because "we never see it" and "it does not exist" differ.
    assert_eq!(DO_NOTHING, ActionId(1));
    assert_eq!(
        dereth_input::names::enum_name_for_action(DO_NOTHING),
        "DoNothing"
    );
    for map in [MOVEMENT, UI_COMMANDS, ITEM_SELECTION] {
        assert!(
            m.find_keys_for_action(DO_NOTHING, map).is_empty(),
            "{map:?}"
        );
    }
}

// ------------------------------------------------------------------------------------------- 2

/// Behaviour: ui.key-binding.a-rebound-key-fires-the-action-and-the-old-key-stops
///
/// **In one session, the new key fires the action and the old key no longer does.**
///
/// Both directions, both asserted on `take_events` after a real `WM_KEYDOWN`/`WM_KEYUP` pair
/// through the message handler -> the keyboard event generator -> the fire path ->
/// `walk_input_maps`. A build that bound everything to everything
/// passes the first half and fails the second.
#[test]
fn rebinding_move_forward_moves_which_key_fires_it() {
    let f = shipped();
    let mut m = manager(f, None);

    // Before: W walks, F7 does nothing at all.
    assert!(press(&mut m, DIK_W, 1000).contains(&MOVE_FORWARD));
    assert!(
        press(&mut m, DIK_F7, 1100).is_empty(),
        "F7 is unbound in both shipped keymaps"
    );

    // Capture F7 for Move Forward. It conflicts with nothing, so the page would not have asked.
    let control = match m.capture_key_hit(MOVEMENT, MOVE_FORWARD, released(DIK_F7), false) {
        Capture::Ready { control, conflicts } => {
            assert!(conflicts.is_empty(), "{conflicts:?}");
            control
        }
        other => panic!("F7 is free, so the capture must be Ready: {other:?}"),
    };
    // The row slot the player clicked: the one currently showing `DIK_W`.
    let slot = m
        .find_keys_for_action(MOVE_FORWARD, MOVEMENT)
        .iter()
        .position(|k| *k == keyboard(DIK_W))
        .expect("DIK_W is one of Move Forward's controls");
    assert!(m.set_binding(MOVEMENT, MOVE_FORWARD, Some(slot), control));

    // After: F7 walks ...
    assert!(
        press(&mut m, DIK_F7, 2000).contains(&MOVE_FORWARD),
        "the new key must fire the action"
    );
    // .. W does not ...
    assert!(
        !press(&mut m, DIK_W, 2100).contains(&MOVE_FORWARD),
        "the old key must stop firing it -- the half a stored-table assertion cannot see"
    );
    // .. and the action's *other* shipped control is untouched.
    assert!(
        press(&mut m, DIK_UP, 2200).contains(&MOVE_FORWARD),
        "replacing one row slot must not clear the row"
    );
}

// ------------------------------------------------------------------------------------------- 3

/// **The freed key is bound to `DoNothing`, and that is what survives the merge.**
///
/// The mechanism, asserted directly, because the session test above passes either way and only the
/// reload test below can tell them apart.
#[test]
fn the_freed_key_is_bound_to_do_nothing_rather_than_deleted() {
    let f = shipped();
    let mut m = manager(f, None);
    let slot = m
        .find_keys_for_action(MOVE_FORWARD, MOVEMENT)
        .iter()
        .position(|k| *k == keyboard(DIK_W))
        .expect("DIK_W");
    m.set_binding(MOVEMENT, MOVE_FORWARD, Some(slot), keyboard(DIK_F7));

    assert_eq!(
        m.find_keys_for_action(DO_NOTHING, MOVEMENT),
        vec![keyboard(DIK_W)],
        ": the old key is unbound and, unless the new key conflicts with it, \
         rebound to DoNothing in the same map"
    );
    // ..and it is genuinely inert: `DoNothing` reaches the queue as an action nobody handles,
    // never as Move Forward.
    let fired = press(&mut m, DIK_W, 3000);
    assert!(!fired.contains(&MOVE_FORWARD), "{fired:?}");
}

// ------------------------------------------------------------------------------------------- 4

/// Behaviour: ui.key-binding.a-rebound-key-fires-the-action-and-the-old-key-stops
///
/// **It persists the way retail persists it: the whole merged map, in the `.keymap` text format,
/// re-merged first on the next run — and the old key is still dead.**
///
/// Client cleanup saves the full master map through its file-node writer. On the next run, keymap
/// initialization merges the user path first, then map `0x10000001`, then map `1`, all three with
/// `overwrite = true`.
///
/// **This is the test that has teeth.** Deleting the line that rebinds the old key to `DoNothing`
/// from `set_binding` leaves everything above green and makes this fail: keymap `0x14000000` is
/// merged after the user file, `DIK_W` is then a control the user file never mentioned, and Move
/// Forward comes back on it — so a rebind would last exactly one session and both keys would walk
/// afterwards.
#[test]
fn a_rebound_key_survives_a_save_and_a_reload_in_both_directions() {
    let f = shipped();
    let mut m = manager(f, None);
    let slot = m
        .find_keys_for_action(MOVE_FORWARD, MOVEMENT)
        .iter()
        .position(|k| *k == keyboard(DIK_W))
        .expect("DIK_W");
    assert!(m.set_binding(MOVEMENT, MOVE_FORWARD, Some(slot), keyboard(DIK_F7)));

    let path = scratch("reload.keymap");
    m.save_keymap(&path).expect("the keymap is written");
    let text = std::fs::read_to_string(&path).expect("and read back");
    assert!(
        text.len() > 4_000,
        "the FULL merged map, not just the overrides: {} bytes",
        text.len()
    );
    assert!(text.contains("DIK_F7"), "the new control is in the file");
    assert!(
        text.contains("DoNothing"),
        "and so is the freed one, which is the point"
    );

    // The next run: the user file first, then keymap 0x14000000, then DefaultMap.
    let mut next = manager(f, Some(&text));
    assert!(
        press(&mut next, DIK_F7, 1000).contains(&MOVE_FORWARD),
        "the rebound key survived the round trip"
    );
    assert!(
        !press(&mut next, DIK_W, 1100).contains(&MOVE_FORWARD),
        "and keymap 0x14000000 did NOT put Move Forward back on DIK_W: Merge adds what is \
         ABSENT, and DoNothing is what stops DIK_W being absent"
    );
    assert_eq!(
        next.find_keys_for_action(DO_NOTHING, MOVEMENT),
        vec![keyboard(DIK_W)]
    );
    // The unrelated shipped bindings came through the round trip unharmed -- the denominator
    // without which "nothing fires any more" would also pass the assertion above.
    assert!(press(&mut next, DIK_UP, 1200).contains(&MOVE_FORWARD));
    assert!(press(&mut next, DIK_R, 1300).contains(&USE));
    assert!(press(&mut next, DIK_F, 1400).contains(&SELECTION_PICK_UP));

    let _ = std::fs::remove_file(&path);
}

// ------------------------------------------------------------------------------------------- 5

/// **The capture policy, against the shipped `ActionMap`'s own conflict table and bindable flags.**
///
/// `DIK_R` is `USE` in `0x10000009`; the conflict table lists `0x10000007`
/// alongside it (the gameplay cluster), so offering `DIK_R` to `SelectionPickUp` finds the real
/// conflict and asks. Confirmed, `SetBinding` takes `DIK_R` away from `USE` — and `USE` is then
/// unreachable from the keyboard, which is exactly what the overwrite dialog warns about.
#[test]
fn a_real_conflict_is_found_asked_about_and_then_honoured() {
    let f = shipped();
    let mut m = manager(f, None);
    assert!(press(&mut m, DIK_R, 1000).contains(&USE));

    let conflicts =
        match m.capture_key_hit(ITEM_SELECTION, SELECTION_PICK_UP, released(DIK_R), false) {
            Capture::NeedsConfirmation { conflicts, .. } => conflicts,
            other => panic!("DIK_R is USE's key and the two maps conflict: {other:?}"),
        };
    assert!(
        conflicts
            .iter()
            .any(|c| c.action == USE && c.input_map == UI_COMMANDS),
        "{conflicts:?}"
    );

    let control = match m.capture_key_hit(ITEM_SELECTION, SELECTION_PICK_UP, released(DIK_R), true)
    {
        Capture::Ready { control, .. } => control,
        other => panic!("confirmed, it must be Ready: {other:?}"),
    };
    let slot = m
        .find_keys_for_action(SELECTION_PICK_UP, ITEM_SELECTION)
        .iter()
        .position(|k| *k == keyboard(DIK_F))
        .expect("DIK_F");
    assert!(m.set_binding(ITEM_SELECTION, SELECTION_PICK_UP, Some(slot), control));

    let fired = press(&mut m, DIK_R, 2000);
    assert!(fired.contains(&SELECTION_PICK_UP), "{fired:?}");
    assert!(
        !fired.contains(&USE),
        "USE lost its only key to the overwrite: {fired:?}"
    );
    assert!(m.find_keys_for_action(USE, UI_COMMANDS).is_empty());
    assert!(
        !press(&mut m, DIK_F, 2100).contains(&SELECTION_PICK_UP),
        "and DIK_F is freed"
    );
}

/// **The capture handler's three early returns, against the real manager.**
///
/// `Ignored` and `Rejected` leave the handler registered — they are *not* refusals — and
/// `Cancelled` is `DIK_ESCAPE`. Asserted here rather than only in a unit test because the mouse
/// arm needs the real device table: the rule is "`DIMOFS_X`/`Y` on the **mouse** device", and the
/// device index comes from the keymap's own `Devices` section.
#[test]
fn the_capture_early_returns_hold_against_the_shipped_device_table() {
    let f = shipped();
    let m = manager(f, None);
    let down = ControlChord::new(
        ControlCode::new(0, SubControlIndex::None, DIK_F7),
        0,
        activation::DOWN,
    );
    assert_eq!(
        m.capture_key_hit(MOVEMENT, MOVE_FORWARD, down, false),
        Capture::Ignored
    );
    assert_eq!(
        m.capture_key_hit(MOVEMENT, MOVE_FORWARD, released(DIK_ESCAPE), false),
        Capture::Cancelled
    );

    let mouse_index = m
        .keymap
        .devices
        .iter()
        .position(|d| d.device_type == dereth_input::DeviceType::Mouse)
        .expect("the shipped keymap has a mouse");
    let mouse = |offset, meta| {
        ControlChord::new(
            ControlCode::new(
                u8::try_from(mouse_index).expect("a small index"),
                SubControlIndex::None,
                offset,
            ),
            meta,
            activation::UP,
        )
    };
    for offset in [
        dereth_input::binding::DIMOFS_X,
        dereth_input::binding::DIMOFS_Y,
    ] {
        assert_eq!(
            m.capture_key_hit(MOVEMENT, MOVE_FORWARD, mouse(offset, 0), false),
            Capture::Rejected
        );
    }
    assert_eq!(
        m.capture_key_hit(
            MOVEMENT,
            MOVE_FORWARD,
            mouse(dereth_input::binding::DIMOFS_BUTTON0, 0),
            false
        ),
        Capture::Rejected
    );
    // Both directions of the same rule: *with* a modifier the left button is bindable.
    assert!(matches!(
        m.capture_key_hit(
            MOVEMENT,
            MOVE_FORWARD,
            mouse(dereth_input::binding::DIMOFS_BUTTON0, 0x8000_0000),
            false
        ),
        Capture::Ready { .. } | Capture::NeedsConfirmation { .. }
    ));

    // And a control already bound to the action is a no-op rather than a rebind.
    assert_eq!(
        m.capture_key_hit(MOVEMENT, MOVE_FORWARD, released(DIK_W), false),
        Capture::Unchanged
    );
}

// ------------------------------------------------------------------------------------------- 6

/// **Unbinding every key of an action — the row's *Clear all* button — and the guards on
/// `SetBinding`.**
#[test]
fn clear_all_empties_one_row_and_an_invalid_control_binds_nothing() {
    let f = shipped();
    let mut m = manager(f, None);
    assert!(m.unbind_all_by_action(MOVE_FORWARD, MOVEMENT));
    assert!(m.find_keys_for_action(MOVE_FORWARD, MOVEMENT).is_empty());
    assert!(
        press(&mut m, DIK_W, 1000).is_empty(),
        "both of its keys are gone"
    );
    assert!(press(&mut m, DIK_UP, 1100).is_empty());
    // ..and only that row: Move Backward still answers to DIK_X.
    assert!(!m.find_keys_for_action(ActionId(0x2A), MOVEMENT).is_empty());

    let invalid = ControlChord::new(ControlCode::INVALID, 0, activation::CLICK);
    assert!(!m.set_binding(MOVEMENT, MOVE_FORWARD, None, invalid));
    let no_activation = ControlChord::new(ControlCode::new(0, SubControlIndex::None, DIK_F7), 0, 0);
    assert!(!m.set_binding(MOVEMENT, MOVE_FORWARD, None, no_activation));
    assert!(m.find_keys_for_action(MOVE_FORWARD, MOVEMENT).is_empty());

    // Adding (no slot) rather than replacing gives the action a second key.
    assert!(m.set_binding(MOVEMENT, MOVE_FORWARD, None, keyboard(DIK_F7)));
    assert!(m.set_binding(MOVEMENT, MOVE_FORWARD, None, keyboard(DIK_W)));
    assert_eq!(m.find_keys_for_action(MOVE_FORWARD, MOVEMENT).len(), 2);
    assert!(press(&mut m, DIK_F7, 2000).contains(&MOVE_FORWARD));
    assert!(press(&mut m, DIK_W, 2100).contains(&MOVE_FORWARD));
}

// ------------------------------------------------------------------------------------------- 7

/// **The cheap regression on the file format: how many shipped bindings round-trip unchanged.**
///
/// The count is reported rather than a boolean, because "it round-trips" with no
/// denominator is the same sentence whether 184 bindings survive or 3 do. Save the untouched merged
/// map, read it back, and compare **every section, every `(ControlChord, ActionId)` pair, and
/// the device and meta-key tables** against the originals, which is also the pair used when the
/// keymap is initialized and saved at shutdown.
#[test]
fn every_shipped_binding_survives_a_save_and_a_load() {
    let f = shipped();
    let m = manager(f, None);

    let path = scratch("roundtrip.keymap");
    m.save_keymap(&path).expect("written");
    let text = std::fs::read_to_string(&path).expect("read back");
    let back = dereth_input::keymap::MasterInputMap::from_keymap_text(&text).expect("re-parsed");

    let mut total = 0usize;
    let mut same = 0usize;
    for section in &m.keymap.sections {
        let other = back.section(section.input_map_id);
        for (control, action) in section.bindings() {
            total += 1;
            if other.is_some_and(|o| {
                o.bindings()
                    .iter()
                    .any(|(c, a)| c == control && a == action)
            }) {
                same += 1;
            }
        }
    }
    assert!(
        total >= 184,
        "the merged map carries every shipped binding: {total}"
    );
    assert_eq!(
        same, total,
        "{same} of {total} bindings survived the round trip"
    );
    eprintln!("key rebinding: {same} of {total} merged bindings round-tripped unchanged");

    assert_eq!(
        back.devices.len(),
        m.keymap.devices.len(),
        "the Devices section"
    );
    assert_eq!(
        back.meta_keys.len(),
        m.keymap.meta_keys.len(),
        "the MetaKeys section"
    );
    assert_eq!(
        back.sections.len(),
        m.keymap.sections.len(),
        "and every input map came back, not just the ones with bindings"
    );

    let _ = std::fs::remove_file(&path);
}
