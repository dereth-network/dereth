//! The shipped Jump binding: Space, with no modifier, raising a start on press and a release on
//! release, and Windows key repeat never restarting the charge, with or without Shift held.
//! Fixture: the retail dats' input maps, driven through `InputShell` with synthetic key messages.

use dereth_primitives::LocalTime;

/// Behaviour: movement.jump.the-shipped-key-starts-and-releases-once-and-key-repeat-does-not-restart-the-charge
#[test]
fn shipped_jump_binding_preserves_start_release_repeat_and_modifier_semantics() {
    use dereth_input::{ActionId, InputMapId};
    use winit::keyboard::KeyCode;
    let store = dereth_dat::testing::open_store().expect("required DATs");
    let mut input = dereth_client_shell::input::InputShell::new(&store, None).unwrap();
    let keys = input.keys_for_action(ActionId(0x31), InputMapId(4));
    eprintln!("Jump keys={keys:?}");
    assert_eq!(keys.len(), 1);
    assert_eq!(keys[0].control.offset(), 0x39, "shipped Space scan code");
    assert_eq!(keys[0].meta_mode, 0, "binding requires no modifier bits");
    assert_eq!(
        keys[0].activation, 3,
        "press and release, no held/repeat binding"
    );
    let mut pump = dereth_desktop::pump::Pump::new();
    for modified in [false, true] {
        if modified {
            input.on_message(pump.key_message_for(KeyCode::ShiftLeft, true, 100).unwrap());
        }
        input.use_time(LocalTime(0.100));
        input.take_events();
        let down = pump.key_message_for(KeyCode::Space, true, 101).unwrap();
        input.on_message(down);
        input.use_time(LocalTime(0.101));
        let events = input.take_events();
        assert_eq!(
            events
                .iter()
                .filter(|e| e.action == ActionId(0x31) && e.start)
                .count(),
            1
        );
        input.on_message(dereth_input::win32::Win32Message {
            lparam: down.lparam | (1 << 30),
            ..down
        });
        input.use_time(LocalTime(0.1015));
        assert!(
            input
                .take_events()
                .iter()
                .all(|e| e.action != ActionId(0x31)),
            "retail Win32 keyboard repeat must not restart charge"
        );
        input.on_message(pump.key_message_for(KeyCode::Space, false, 102).unwrap());
        input.use_time(LocalTime(0.102));
        let events = input.take_events();
        assert_eq!(
            events
                .iter()
                .filter(|e| e.action == ActionId(0x31) && !e.start)
                .count(),
            1
        );
        if modified {
            input.on_message(
                pump.key_message_for(KeyCode::ShiftLeft, false, 103)
                    .unwrap(),
            );
        }
    }
}
