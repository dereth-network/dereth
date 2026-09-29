// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Managers/EventManager.cs
//! Port of `Source/ACE.Server/Managers/EventManager.cs`.
//!
//! The world events (the `event` table of the world database): each has a `GameEventState`
//! (`Enabled`, `Disabled`, `Off`, `On`) and an optional real-time window. Generators whose
//! `GeneratorTimeType` is `Event` consult it (`WorldObject.CheckEventStatus`), as do the emotes
//! and the `@event` admin command.

use empyrean_common::dotnet::{CsCast, DotNetDict};
use empyrean_content::models::world::Event;
use empyrean_entity::enums::{GameEventState, PropertyString};
use empyrean_entity::ObjectGuid;

use crate::managers::property_manager;
use crate::World;

/// The mutable static state of ACE's `EventManager`, held as a field of `World`.
#[derive(Debug)]
pub struct EventManagerState {
    /// `Events`: `new Dictionary<string, Event>(StringComparer.OrdinalIgnoreCase)`. The key is the
    /// name folded by [`ordinal_ignore_case_key`]; the value keeps the name as the table spells it.
    // ACE: EventManager.Events
    pub events: DotNetDict<String, Event>,
    // ACE: EventManager.Debug
    pub debug: bool,
}

impl Default for EventManagerState {
    /// The static constructor: an empty event table (`Debug` starts false).
    // ACE: EventManager.EventManager
    fn default() -> Self {
        Self {
            events: DotNetDict::new(),
            debug: false,
        }
    }
}

impl EventManagerState {
    /// `Events.TryGetValue(name, out evnt)`, ignoring case.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Event> {
        self.events.get(&ordinal_ignore_case_key(name))
    }

    fn get_mut(&mut self, name: &str) -> Option<&mut Event> {
        self.events.get_mut(&ordinal_ignore_case_key(name))
    }

    /// `Events.Add(evnt.Name, evnt)`.
    ///
    /// # Panics
    /// When an event of the same name (ignoring case) is already present, as .NET throws.
    pub fn add(&mut self, evnt: Event) {
        self.events.add(ordinal_ignore_case_key(&evnt.name), evnt);
    }

    /// `Events.TryAdd(evnt.Name, evnt)`.
    pub fn try_add(&mut self, evnt: Event) -> bool {
        self.events
            .try_add(ordinal_ignore_case_key(&evnt.name), evnt)
    }

    /// `Events.Remove(name)`.
    pub fn remove(&mut self, name: &str) -> bool {
        self.events.remove(&ordinal_ignore_case_key(name)).is_some()
    }
}

/// `StringComparer.OrdinalIgnoreCase`'s key: the simple upper-case mapping of every character.
fn ordinal_ignore_case_key(name: &str) -> String {
    name.chars()
        .map(|c| c.to_uppercase().next().unwrap_or(c))
        .collect()
}

/// `a.Equals(b, StringComparison.OrdinalIgnoreCase)`.
fn equals_ordinal_ignore_case(a: &str, b: &str) -> bool {
    ordinal_ignore_case_key(a) == ordinal_ignore_case_key(b)
}

// ACE: EventManager.Initialize
/// Loads every event of the world database, and starts those whose stored state is `On`.
pub fn initialize(w: &mut World) {
    let events = w.content.get_all_events();

    for evnt in events {
        let evnt = Event::clone(&evnt);
        let (name, state) = (evnt.name.clone(), evnt.state);
        w.event_manager.add(evnt);

        if state == GameEventState::On.0 {
            start_event(w, &name, None, None);
        }
    }

    log::debug!("EventManager Initalized.");
}

/// `{Name} (0x{Guid}|{WeenieClassId})` of a source or target.
fn describe(w: &World, g: ObjectGuid) -> Option<String> {
    let o = w.objects.get(g)?;
    let name = o.get_property(PropertyString::Name).unwrap_or_default();
    Some(format!("{name} (0x{}|{})", o.guid, o.biota.weenie_class_id))
}

/// The `[EVENT]` debug line of `StartEvent`/`StopEvent`.
fn log_event(
    w: &World,
    source: Option<ObjectGuid>,
    target: Option<ObjectGuid>,
    verb: &str,
    name: &str,
    unchanged: bool,
) {
    let source_text = source
        .and_then(|g| describe(w, g))
        .unwrap_or_else(|| "SYSTEM".to_owned());
    let target_text = target
        .and_then(|g| describe(w, g))
        .map_or_else(String::new, |t| format!(", triggered by {t},"));
    let suffix = if !unchanged {
        String::new()
    } else if source.is_none() {
        ", which is the default state for this event.".to_owned()
    } else {
        format!(", which had already been {verb}.")
    };
    log::debug!("[EVENT] {source_text}{target_text} {verb} an event: {name}{suffix}");
}

// ACE: EventManager.StartEvent
pub fn start_event(
    w: &mut World,
    e: &str,
    source: Option<ObjectGuid>,
    target: Option<ObjectGuid>,
) -> bool {
    let event_name = get_event_name(e);

    if equals_ordinal_ignore_case(event_name, "EventIsPKWorld") {
        // special event
        return false;
    }

    let debug = w.event_manager.debug;
    let Some(evnt) = w.event_manager.get_mut(event_name) else {
        return false;
    };

    let state = GameEventState(evnt.state);

    if state == GameEventState::Disabled {
        return false;
    }

    if state == GameEventState::Enabled || state == GameEventState::Off {
        evnt.state = GameEventState::On.0;

        if debug {
            log::info!("Starting event {}", evnt.name);
        }
    }

    if log::log_enabled!(log::Level::Debug) {
        let (name, unchanged) = (evnt.name.clone(), state.0 == evnt.state);
        log_event(w, source, target, "started", &name, unchanged);
    }

    true
}

// ACE: EventManager.StopEvent
pub fn stop_event(
    w: &mut World,
    e: &str,
    source: Option<ObjectGuid>,
    target: Option<ObjectGuid>,
) -> bool {
    let event_name = get_event_name(e);

    if equals_ordinal_ignore_case(event_name, "EventIsPKWorld") {
        // special event
        return false;
    }

    let debug = w.event_manager.debug;
    let Some(evnt) = w.event_manager.get_mut(event_name) else {
        return false;
    };

    let state = GameEventState(evnt.state);

    if state == GameEventState::Disabled {
        return false;
    }

    if state == GameEventState::Enabled || state == GameEventState::On {
        evnt.state = GameEventState::Off.0;

        if debug {
            log::info!("Stopping event {}", evnt.name);
        }
    }

    if log::log_enabled!(log::Level::Debug) {
        let (name, unchanged) = (evnt.name.clone(), state.0 == evnt.state);
        log_event(w, source, target, "stopped", &name, unchanged);
    }

    true
}

// ACE: EventManager.IsEventStarted
pub fn is_event_started(
    w: &mut World,
    e: &str,
    source: Option<ObjectGuid>,
    target: Option<ObjectGuid>,
) -> bool {
    let event_name = get_event_name(e);

    if equals_ordinal_ignore_case(event_name, "EventIsPKWorld") {
        // special event
        let server_pk_state = property_manager::get_bool(w, "pk_server", false, true).item;

        return server_pk_state;
    }

    let Some(evnt) = w.event_manager.get(event_name) else {
        return false;
    };

    if evnt.state != GameEventState::Disabled.0 && (evnt.start_time != -1 || evnt.end_time != -1) {
        let prev_state = GameEventState(evnt.state);

        let now: i32 = w.now.unix_time.cs_cast();

        let start = (now > evnt.start_time) && (evnt.start_time > -1);
        let end = (now > evnt.end_time) && (evnt.end_time > -1);

        let name = evnt.name.clone();
        if prev_state == GameEventState::On && end {
            return !stop_event(w, &name, source, target);
        } else if (prev_state == GameEventState::Off || prev_state == GameEventState::Enabled)
            && start
            && !end
        {
            return start_event(w, &name, source, target);
        }
    }

    w.event_manager
        .get(event_name)
        .is_some_and(|evnt| evnt.state == GameEventState::On.0)
}

// ACE: EventManager.IsEventEnabled
#[must_use]
pub fn is_event_enabled(w: &World, e: &str) -> bool {
    let event_name = get_event_name(e);

    let Some(evnt) = w.event_manager.get(event_name) else {
        return false;
    };

    evnt.state != GameEventState::Disabled.0
}

// ACE: EventManager.IsEventAvailable
#[must_use]
pub fn is_event_available(w: &World, e: &str) -> bool {
    let event_name = get_event_name(e);

    w.event_manager.get(event_name).is_some()
}

// ACE: EventManager.GetEventStatus
#[must_use]
pub fn get_event_status(w: &World, e: &str) -> GameEventState {
    let event_name = get_event_name(e);

    if equals_ordinal_ignore_case(event_name, "EventIsPKWorld") {
        // special event
        if property_manager::get_bool(w, "pk_server", false, true).item {
            return GameEventState::On;
        }
        return GameEventState::Off;
    }

    let Some(evnt) = w.event_manager.get(event_name) else {
        return GameEventState::Undef;
    };

    GameEventState(evnt.state)
}

// ACE: EventManager.GetEventName
/// Returns the event name without the @ comment.
///
/// `event_format`: a event name with an optional @comment on the end.
#[must_use]
pub fn get_event_name(event_format: &str) -> &str {
    // strip comment
    match event_format.find('@') {
        None => event_format,
        Some(idx) => &event_format[..idx],
    }
}
