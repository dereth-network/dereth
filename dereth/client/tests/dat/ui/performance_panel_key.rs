//! The performance panel's toggle has no key until the player binds one: F7, which no shipped key
//! map binds, does nothing at first, and once bound fires the toggle on every screen, from this
//! client's own map, which is live for the whole run.
//! Fixture: the retail dats' input maps, driven through `InputShell` with synthetic key messages.

use dereth_primitives::LocalTime;

/// `DIK_F7`, a key no shipped key map binds.
const DIK_F7: u16 = 0x41;

/// Behaviour: presentation.performance.a-key-or-the-option-shows-the-panel-over-any-interface
#[test]
fn the_performance_panel_has_no_key_until_one_is_bound_and_then_fires_on_every_screen() {
    use dereth_input::ActionId;
    use winit::keyboard::KeyCode;
    let perf = ActionId(dereth_client_contract::actions::dereth::TOGGLE_PERFORMANCE_PANEL);
    let store = dereth_dat::testing::open_store().expect("required DATs");
    let mut input = dereth_client::input::InputShell::new(&store, None).unwrap();
    assert!(
        input
            .keys_for_action(perf, dereth_input::dereth::INPUT_MAP)
            .is_empty(),
        "no key is bound to the performance panel at first"
    );

    // F7 binds nothing in the shipped maps.
    let shipped = input
        .manager
        .shipped_maps
        .as_ref()
        .expect("the shipped maps");
    for map in [&shipped.0, &shipped.1] {
        for s in &map.sections {
            for (qc, a) in s.bindings() {
                assert_ne!(
                    qc.control.offset(),
                    DIK_F7,
                    "F7 is bound to {a:?} in {:?}",
                    s.input_map_id
                );
            }
        }
    }

    let mut pump = dereth_client::pump::Pump::new();
    let mut press = |input: &mut dereth_client::input::InputShell, t: f64, serial: u32| {
        input.use_time(LocalTime(t));
        input.take_events();
        input.on_message(pump.key_message_for(KeyCode::F7, true, serial).unwrap());
        input.use_time(LocalTime(t + 0.001));
        let started = input
            .take_events()
            .iter()
            .filter(|e| e.action == perf && e.start)
            .count();
        input.on_message(
            pump.key_message_for(KeyCode::F7, false, serial + 1)
                .unwrap(),
        );
        input.use_time(LocalTime(t + 0.002));
        input.take_events();
        started
    };
    input.set_character_session_input_maps(false);
    assert_eq!(press(&mut input, 0.100, 101), 0, "F7 does nothing at first");

    // Bound by the player, it fires off the character session too: the pre-game screens keep it.
    assert!(input.bind_key(DIK_F7, perf, None));
    assert_eq!(press(&mut input, 0.200, 201), 1);
}
