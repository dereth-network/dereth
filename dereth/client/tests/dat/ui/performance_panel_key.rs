//! F7 presses the performance panel's toggle, on every screen: no shipped key map binds F7 to
//! anything, and the binding sits in this client's own map, which is live for the whole run.
//! Fixture: the retail dats' input maps, driven through `InputShell` with synthetic key messages.

use dereth_primitives::LocalTime;

/// Behaviour: presentation.performance.a-key-or-the-option-shows-the-panel-over-any-interface
#[test]
fn the_performance_panel_key_is_bound_alone_and_fires_on_every_screen() {
    use dereth_input::ActionId;
    use winit::keyboard::KeyCode;
    let perf = ActionId(dereth_client_contract::actions::dereth::TOGGLE_PERFORMANCE_PANEL);
    let store = dereth_dat::testing::open_store().expect("required DATs");
    let mut input = dereth_client::input::InputShell::new(&store, None).unwrap();
    let keys = input.keys_for_action(perf, dereth_input::dereth::INPUT_MAP);
    assert_eq!(keys.len(), 1);
    assert_eq!(
        keys[0].control.offset(),
        u16::from(dereth_input::dereth::DIK_F7)
    );
    assert_eq!(keys[0].meta_mode, 0);

    // F7 binds nothing in the shipped maps.
    let shipped = input
        .manager
        .shipped_maps
        .as_ref()
        .expect("the shipped maps");
    for map in [&shipped.0, &shipped.1] {
        for s in &map.sections {
            for (qc, a) in s.bindings() {
                if qc.control.offset() == u16::from(dereth_input::dereth::DIK_F7) {
                    assert_eq!(*a, perf, "F7 is bound to {a:?} in {:?}", s.input_map_id);
                }
            }
        }
    }

    // Off the character session too: the pre-game screens keep it.
    input.set_character_session_input_maps(false);
    let mut pump = dereth_client::pump::Pump::new();
    input.use_time(LocalTime(0.100));
    input.take_events();
    input.on_message(pump.key_message_for(KeyCode::F7, true, 101).unwrap());
    input.use_time(LocalTime(0.101));
    let events = input.take_events();
    assert_eq!(
        events
            .iter()
            .filter(|e| e.action == perf && e.start)
            .count(),
        1,
        "{events:?}"
    );
}
