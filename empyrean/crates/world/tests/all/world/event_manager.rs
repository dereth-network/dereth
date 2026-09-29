//! ACE: Source/ACE.Server/Managers/EventManager.cs::StartEvent
//! EventManager init, start/stop state machine, timed window, PK-world event, name comment
//! stripping, case-insensitive duplicate throws.
//! Fixture: synthetic dats, isolated world state.

use std::sync::Arc;
use std::time::Duration;

use empyrean_common::clock::ClockSnapshot;
use empyrean_common::dotnet::datetime::DotNetDateTime;
use empyrean_content::models::world::Event;
use empyrean_content::MemContent;
use empyrean_dat::FakeDats;
use empyrean_entity::enums::GameEventState;
use empyrean_store::MemShard;
use empyrean_world::managers::event_manager as em;
use empyrean_world::managers::property_manager as pm;
use empyrean_world::World;

const NOW: f64 = 1_000_000.0;

fn event(id: u32, name: &str, state: GameEventState, start_time: i32, end_time: i32) -> Event {
    Event {
        id,
        name: name.to_owned(),
        start_time,
        end_time,
        state: state.0,
        ..Default::default()
    }
}

/// A world at unix time `NOW` whose world database holds `events`, after `EventManager.Initialize`.
fn world(events: Vec<Event>) -> World {
    let now = ClockSnapshot {
        portal_year_ticks: 0.0,
        unix_time: NOW,
        utc: DotNetDateTime::new(2026, 1, 1),
        monotonic: Duration::ZERO,
    };
    let mut w = World::new(now, FakeDats::new().build().expect("fake dats"));
    w.content = Arc::new(
        events
            .into_iter()
            .fold(MemContent::new(), MemContent::event),
    );
    pm::install_shard_config(&mut w, pm::shard_config_handle(Box::new(MemShard::new())));
    pm::initialize(&mut w, true);
    em::initialize(&mut w);
    w
}

fn at(w: &mut World, unix_time: f64) {
    w.now.unix_time = unix_time;
}

#[test]
fn initialize_loads_every_event_and_starts_those_stored_on() {
    let w = world(vec![
        event(1, "EventOn", GameEventState::On, -1, -1),
        event(2, "EventOff", GameEventState::Off, -1, -1),
        event(3, "EventEnabled", GameEventState::Enabled, -1, -1),
        event(4, "EventDisabled", GameEventState::Disabled, -1, -1),
    ]);
    assert_eq!(w.event_manager.events.len(), 4);
    assert_eq!(em::get_event_status(&w, "EventOn"), GameEventState::On);
    assert_eq!(em::get_event_status(&w, "eventoff"), GameEventState::Off);
    assert_eq!(
        em::get_event_status(&w, "EVENTENABLED"),
        GameEventState::Enabled
    );
    assert_eq!(
        em::get_event_status(&w, "EventDisabled"),
        GameEventState::Disabled
    );
    assert_eq!(
        em::get_event_status(&w, "NoSuchEvent"),
        GameEventState::Undef
    );
    assert!(em::is_event_available(&w, "eventon@a comment"));
    assert!(!em::is_event_available(&w, "NoSuchEvent"));
    assert!(em::is_event_enabled(&w, "EventOff"));
    assert!(!em::is_event_enabled(&w, "EventDisabled"));
    assert!(!em::is_event_enabled(&w, "NoSuchEvent"));
}

#[test]
fn start_and_stop_follow_the_state_machine() {
    let mut w = world(vec![
        event(1, "A", GameEventState::Enabled, -1, -1),
        event(2, "B", GameEventState::Disabled, -1, -1),
    ]);
    // Enabled -> On; On stays On (still true)
    assert!(em::start_event(&mut w, "A", None, None));
    assert_eq!(em::get_event_status(&w, "A"), GameEventState::On);
    assert!(em::is_event_started(&mut w, "A", None, None));
    assert!(em::start_event(&mut w, "a", None, None));
    assert_eq!(em::get_event_status(&w, "A"), GameEventState::On);
    // On -> Off; Off stays Off (still true)
    assert!(em::stop_event(&mut w, "A@note", None, None));
    assert_eq!(em::get_event_status(&w, "A"), GameEventState::Off);
    assert!(!em::is_event_started(&mut w, "A", None, None));
    assert!(em::stop_event(&mut w, "A", None, None));
    assert_eq!(em::get_event_status(&w, "A"), GameEventState::Off);
    // Disabled refuses both; an unknown name is false
    assert!(!em::start_event(&mut w, "B", None, None));
    assert!(!em::stop_event(&mut w, "B", None, None));
    assert_eq!(em::get_event_status(&w, "B"), GameEventState::Disabled);
    assert!(!em::start_event(&mut w, "C", None, None));
    assert!(!em::is_event_started(&mut w, "C", None, None));
}

/// `IsEventStarted` with a real-time window (unix seconds, `now > start`, `now > end`): an `Off`
/// event starts inside the window; an `On` one stops after it (and the call answers
/// `!StopEvent`, false); before the window nothing changes.
#[test]
fn a_timed_event_starts_inside_its_window_and_stops_after_it() {
    let start = 1_000_100;
    let end = 1_000_200;
    let mut w = world(vec![
        event(1, "T", GameEventState::Off, start, end),
        event(2, "Open", GameEventState::Enabled, start, -1),
    ]);

    assert!(
        !em::is_event_started(&mut w, "T", None, None),
        "before the window"
    );
    assert_eq!(em::get_event_status(&w, "T"), GameEventState::Off);
    at(&mut w, f64::from(start)); // not yet: `now > start` is strict
    assert!(!em::is_event_started(&mut w, "T", None, None));

    at(&mut w, f64::from(start) + 1.5);
    assert!(
        em::is_event_started(&mut w, "T", None, None),
        "inside the window"
    );
    assert_eq!(em::get_event_status(&w, "T"), GameEventState::On);
    assert!(
        em::is_event_started(&mut w, "Open", None, None),
        "no end time"
    );

    at(&mut w, f64::from(end) + 1.0);
    assert!(
        !em::is_event_started(&mut w, "T", None, None),
        "after the window"
    );
    assert_eq!(em::get_event_status(&w, "T"), GameEventState::Off);
    assert!(!em::is_event_started(&mut w, "T", None, None), "stays off");
    assert!(em::is_event_started(&mut w, "Open", None, None));
}

#[test]
fn event_is_pk_world_follows_the_pk_server_property() {
    let mut w = world(vec![]);
    assert!(!em::is_event_started(&mut w, "EventIsPKWorld", None, None));
    assert_eq!(
        em::get_event_status(&w, "eventispkworld"),
        GameEventState::Off
    );
    assert!(
        !em::start_event(&mut w, "EventIsPKWorld", None, None),
        "cannot be started"
    );
    assert!(
        !em::stop_event(&mut w, "EventIsPKWorld", None, None),
        "cannot be stopped"
    );
    assert!(pm::modify_bool(&w, "pk_server", true));
    assert!(em::is_event_started(&mut w, "EventIsPKWorld@x", None, None));
    assert_eq!(
        em::get_event_status(&w, "EventIsPKWorld"),
        GameEventState::On
    );
    assert!(
        !em::is_event_available(&w, "EventIsPKWorld"),
        "not in the table"
    );
}

#[test]
fn get_event_name_strips_the_comment() {
    assert_eq!(em::get_event_name("EventName"), "EventName");
    assert_eq!(em::get_event_name("EventName@comment@more"), "EventName");
    assert_eq!(em::get_event_name("@comment"), "");
}

#[test]
#[should_panic(expected = "An item with the same key has already been added")]
fn two_events_differing_only_in_case_throw_as_dictionary_add_does() {
    let _ = world(vec![
        event(1, "Dup", GameEventState::Off, -1, -1),
        event(2, "DUP", GameEventState::Off, -1, -1),
    ]);
}

mod world_events {
    use crate::support::social_world::*;

    /// `EventManager` (EventManager.cs): Initialize loads every event (a saved On stays On); names
    /// ignore case and drop an `@comment`; Start/Stop refuse a disabled event and `EventIsPKWorld`;
    /// IsEventStarted applies the start / end times.
    #[test]
    fn world_events_start_stop_and_follow_their_times() {
        use empyrean_content::models::world::event::Event;
        use empyrean_entity::enums::GameEventState;
        use empyrean_world::managers::event_manager as em;

        let mut h = H::small();
        #[allow(clippy::cast_possible_truncation)]
        let now: i32 = h.w.now.unix_time as i32;
        let ev = |id: u32, name: &str, state: GameEventState, start: i32, end: i32| Event {
            id,
            name: name.into(),
            state: state.0,
            start_time: start,
            end_time: end,
            ..Event::default()
        };
        h.w.content = std::sync::Arc::new(
            empyrean_content::MemContent::new()
                .event(ev(1, "Festival", GameEventState::On, -1, -1))
                .event(ev(2, "Winter", GameEventState::Off, -1, -1))
                .event(ev(3, "Sealed", GameEventState::Disabled, -1, -1))
                .event(ev(4, "Dawn", GameEventState::Enabled, now - 10, now + 100))
                .event(ev(5, "Dusk", GameEventState::On, now - 100, now - 10)),
        );
        em::initialize(&mut h.w);

        assert_eq!(
            em::get_event_status(&h.w, "festival@note"),
            GameEventState::On
        );
        assert!(em::is_event_available(&h.w, "WINTER") && em::is_event_enabled(&h.w, "Winter"));
        assert!(!em::is_event_enabled(&h.w, "Sealed") && !em::is_event_available(&h.w, "Nothing"));
        assert!(!em::start_event(&mut h.w, "Sealed", None, None));
        assert!(!em::start_event(&mut h.w, "EventIsPKWorld", None, None));
        assert!(em::start_event(&mut h.w, "winter", None, None));
        assert_eq!(em::get_event_status(&h.w, "Winter"), GameEventState::On);
        assert!(em::stop_event(&mut h.w, "Winter", None, None));
        assert!(!em::is_event_started(&mut h.w, "Winter", None, None));

        // past its start: started; past its end: stopped
        assert!(em::is_event_started(&mut h.w, "Dawn", None, None));
        assert_eq!(em::get_event_status(&h.w, "Dawn"), GameEventState::On);
        assert!(!em::is_event_started(&mut h.w, "Dusk", None, None));
        assert_eq!(em::get_event_status(&h.w, "Dusk"), GameEventState::Off);
        assert_eq!(em::get_event_status(&h.w, "Nothing"), GameEventState::Undef);
        assert_eq!(em::get_event_name("a@b@c"), "a");
    }
}

mod initial_state {
    use crate::support::position_and_inventory::*;

    /// The static constructor: an empty, case-insensitive event table; `Debug` off.
    #[test]
    fn the_event_manager_starts_empty() {
        let e = EventManagerState::default();
        assert!(e.events.is_empty() && !e.debug);
    }
}
