//! ACE: Source/ACE.Server/Managers/EventManager.cs::StartEvent
//! Event generators through game messages.
//! Fixture: shared virtual-time server state and recorded content where enabled.

mod events {
    //! ACE: Source/ACE.Server/Managers/EventManager.cs::StartEvent
    use crate::support::event_world::*;

    /// The Fall Festival case with synthetic content: the event is stored `Off`, so the generator
    /// placed near the admin spawns nothing. `@event status` reports it; `@event start` turns it `On`
    /// and within two generator heartbeats the spawn is created (and sent); `@event stop` turns it
    /// `Off` and the spawn is destroyed (GeneratorEndDestructionType Destroy), a DeleteObject.
    #[test]
    fn the_event_command_gates_an_event_generator() {
        let mut ts = server();
        assert_eq!(
            event_manager::get_event_status(&ts.world, EVENT),
            GameEventState::Off,
            "loaded at start-up"
        );
        let alpha = join(
            &mut ts,
            "i15alpha",
            ALPHA,
            "Alpha Admin",
            at(20.0, 20.0),
            true,
        );
        on_ground(&mut ts.world, GENERATOR, at(24.0, 20.0));
        ts.advance(30.0);
        assert!(
            live(&ts, SPAWN).is_empty(),
            "nothing spawns while the event is off"
        );

        assert_eq!(
            say(&mut ts, alpha, &format!("@event status {EVENT}")),
            [broadcast(&format!("Event {EVENT} - GameEventState.Off"))]
        );
        assert_eq!(
            say(&mut ts, alpha, "@event status NoSuchEvent"),
            [broadcast("Event NoSuchEvent - GameEventState.Undef")]
        );

        let reply = say(&mut ts, alpha, &format!("@event start {EVENT}"));
        assert_eq!(
            reply.first(),
            Some(&broadcast(&format!("Event {EVENT} started successfully."))),
            "{reply:?}"
        );
        assert_eq!(
            event_manager::get_event_status(&ts.world, EVENT),
            GameEventState::On
        );
        assert!(
            ts.run_until(30.0, |ts| !live(ts, SPAWN).is_empty()),
            "the generator spawns once its event is on"
        );
        let spawn = live(&ts, SPAWN)[0];
        assert!(
            ts.run_until(2.0, |ts| ts
                .received::<ItemCreateObject>(alpha)
                .iter()
                .any(|c| c.0.id.0 == spawn.full())),
            "the admin sees it"
        );

        let reply = say(&mut ts, alpha, &format!("@event stop {EVENT}"));
        assert_eq!(
            reply.first(),
            Some(&broadcast(&format!("Event {EVENT} stopped successfully."))),
            "{reply:?}"
        );
        assert!(
            ts.run_until(30.0, |ts| live(ts, SPAWN).is_empty()),
            "the spawn is destroyed once its event is off"
        );
        assert!(
            ts.run_until(2.0, |ts| ts
                .received::<ItemDeleteObject>(alpha)
                .iter()
                .any(|d| d.id.0 == spawn.full())),
            "and deleted"
        );
        assert_eq!(
            event_manager::get_event_status(&ts.world, EVENT),
            GameEventState::Off
        );
    }
}

#[cfg(feature = "real-content")]
mod events_real {
    //! ACE: Source/ACE.Server/Managers/EventManager.cs::StartEvent
    use crate::support::event_world::real::*;

    #[test]
    fn holtburg_spawns_the_pumpkin_buffer_only_while_the_fall_festival_is_on() {
        let mut ts = server();
        assert_eq!(
            event_manager::get_event_status(&ts.world, FALL_FESTIVAL),
            GameEventState::Off,
            "the world DB's default state"
        );
        let alpha = join(
            &mut ts,
            "i15alpha",
            ALPHA,
            "Alpha Admin",
            holtburg(84.0, 7.1, 94.0),
            true,
        );
        assert!(
            ts.world
                .landblock_manager
                .landblocks
                .get(landblock_id())
                .is_some(),
            "Holtburg is loaded"
        );
        ts.advance(30.0);
        assert_eq!(
            live(&ts, PUMPKIN_BUFFER_GENERATOR).len(),
            1,
            "the generator stands in Holtburg"
        );
        assert!(
            live(&ts, PUMPKIN_BUFFER).is_empty(),
            "no Pumpkin Buffer while the Fall Festival is off"
        );

        let reply = say(&mut ts, alpha, &format!("@event start {FALL_FESTIVAL}"));
        assert_eq!(
            reply.first(),
            Some(&broadcast(&format!(
                "Event {FALL_FESTIVAL} started successfully."
            ))),
            "{reply:?}"
        );
        assert!(
            ts.run_until(30.0, |ts| !live(ts, PUMPKIN_BUFFER).is_empty()),
            "the Pumpkin Buffer spawns once the event is on"
        );
        let buffer = live(&ts, PUMPKIN_BUFFER)[0];

        // Its appearance: the Pumpkin Kin setup, which has exactly one part (a jack-o'-lantern
        // head). The weenie has no clothing, anim parts, palettes or texture maps, so
        // `Creature.CalculateObjDesc` sends only the setup's "naked" parts: that one part at
        // index 0. A floating pumpkin head is its real appearance, as for the Pumpkin Kin and the
        // Pet Pumpkin, which share the setup.
        assert!(ts.run_until(2.0, |ts| ts
            .received::<ItemCreateObject>(alpha)
            .iter()
            .any(|c| c.0.id.0 == buffer.full())));
        let creates = ts.received::<ItemCreateObject>(alpha);
        let c = creates.iter().find(|c| c.0.id.0 == buffer.full()).unwrap();
        assert_eq!(c.0.physicsdesc.setup_id, Some(PUMPKIN_SETUP));
        assert_eq!(
            ts.world
                .objects
                .get(buffer)
                .unwrap()
                .get_property(PropertyDataId::Setup),
            Some(PUMPKIN_SETUP)
        );
        let setup = ts
            .world
            .dats
            .portal_dat()
            .read_from_dat::<empyrean_dat::file_types::SetupModel>(PUMPKIN_SETUP)
            .expect("the setup");
        assert_eq!(setup.parts.len(), 1, "a one-part setup");
        let od = &c.0.objdesc;
        assert!(
            od.subpalettes.is_empty() && od.texture_changes.is_empty(),
            "no palettes or textures: {od:?}"
        );
        let parts: Vec<(u8, u32)> = od
            .anim_part_changes
            .iter()
            .map(|a| (a.part_index, a.part_id))
            .collect();
        assert_eq!(parts, [(0, setup.parts[0].0)], "only the setup's own part");

        let reply = say(&mut ts, alpha, &format!("@event stop {FALL_FESTIVAL}"));
        assert_eq!(
            reply.first(),
            Some(&broadcast(&format!(
                "Event {FALL_FESTIVAL} stopped successfully."
            ))),
            "{reply:?}"
        );
        assert!(
            ts.run_until(30.0, |ts| live(ts, PUMPKIN_BUFFER).is_empty()),
            "the Pumpkin Buffer leaves once the event is off"
        );
    }
}
