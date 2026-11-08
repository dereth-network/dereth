//! A world's scheduled events, on a clock the test drives: a camp, an object and a quest step that
//! wait on one stand or run only while it is on, and its effects are on every player in the world
//! while it is, running out as it ends. Fixture: the test world's camp of beetles, one static
//! object beside the player, a quest under way at its first phase, and an event the test gives its
//! windows.

use auberean_protocol::ServerMessage;
use yalain_content::events::{Event, EventTable, ObjectGate, Window};
use yalain_content::quests::{Step, Trigger, Update};
use yalain_content::{ContentObject, Description, Facts, Kind};
use yalain_testkit::Harness;
use yalain_world::effect_defs::{EffectDef, Kind as EffectKind, Scale};
use yalain_world::state::QuestState;

const OBJECT: u64 = 0x4002_9900_0000_0E01;
const TEMPLATE: u32 = 0x4700_1154;
const CELL: u32 = 0x0299_0109;
const ORIGIN: [f32; 3] = [61.0, 241.0, -44.6];
const QUEST: u32 = 0x4000_0E01;
const BOOST: u32 = 0x6F00_0E01;

fn object() -> ContentObject {
    ContentObject {
        object: Some(OBJECT),
        template: TEMPLATE,
        description: Description {
            name_text: None,
            package: 0x13C1,
            look: 0x1F00_0ED5,
            enter_fx: 0,
            scale: None,
            name: None,
            numbers: [0; 6],
            source: "authored for a test".into(),
        },
        cell: CELL,
        origin: ORIGIN,
        rotation: [1.0, 0.0, 0.0, 0.0],
        kind: Kind::Static,
        label: String::new(),
        placed: "authored for a test".into(),
        destination: None,
        facts: Facts::default(),
        walks: None,
        gate: None,
    }
}

fn step() -> Step {
    Step {
        quest: QUEST,
        needs: 1,
        trigger: Trigger::Use(TEMPLATE),
        tell: None,
        update: Update::Nothing,
        xp: 0,
        gives: Vec::new(),
        counts: Vec::new(),
        effects: Vec::new(),
        text: None,
        placed: "authored for a test".into(),
        requires: Vec::new(),
        takes: Vec::new(),
        races: Vec::new(),
        effects_last: false,
        wants: None,
        fellows: false,
        levels: None,
        xp_share: 0.0,
        to: None,
    }
}

/// An effect that lasts an hour of its own, changing nothing.
fn boost() -> EffectDef {
    EffectDef {
        state: BOOST,
        kind: EffectKind::Numeric {
            flags: 0x9,
            stats: vec![yalain_world::effects::KILL_EXPERIENCE],
            magnitude: Scale(vec![(0.0, 1.0)]),
        },
        duration: Scale(vec![(0.0, 3600.0)]),
        priority: Scale(vec![(0.0, 1.0)]),
        fx: 0,
        external_flags: 0x9,
        internal_flags: 0x0100_0713,
        ends_at_logout: false,
        enum_value: 0,
        eq_class: 4101,
        name: None,
    }
}

/// The world with an event `fair`, on in these windows, gating the test camp, the object and the
/// step and putting the effect on players; a player in it, under way at the step's phase.
fn world(windows: Vec<Window>) -> (Harness, usize, u64) {
    let mut h = Harness::new();
    h.add_creatures(2);
    h.server.content.objects = vec![object()];
    h.server.content.events = EventTable {
        events: vec![Event {
            key: "fair".into(),
            game_event: None,
            windows,
            on_players: vec![BOOST],
        }],
        camps: vec![("test".into(), "fair".into())],
        objects: vec![ObjectGate {
            template: TEMPLATE,
            cell: CELL,
            origin: ORIGIN,
            event: "fair".into(),
        }],
        steps: vec![(step(), "fair".into())],
    };
    h.server.effect_table.effects.insert(BOOST, boost());
    let (b, me) = h.player_in_world("one", "Elara");
    let world = world_of(&h, me);
    if let Some(p) = h.server.players.get_mut(&world) {
        p.play.quests.push(QuestState {
            quest: QUEST,
            status: yalain_world::quests::status::UNDERWAY,
            phase: 1,
            started: 0,
            updated: 0,
        });
    }
    h.run(10);
    (h, b, me)
}

fn world_of(h: &Harness, me: u64) -> yalain_net::ConnId {
    *h.server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .expect("in the world")
        .0
}

fn beetles(h: &Harness) -> usize {
    h.server
        .things
        .by_id
        .values()
        .filter(|t| t.creature.is_some())
        .count()
}

fn made(h: &Harness, b: usize, from: usize) -> bool {
    h.world_messages(b, from)
        .iter()
        .any(|m| matches!(m, ServerMessage::CreateObject(o) if o.id == OBJECT))
}

fn forgotten(h: &Harness, b: usize, from: usize) -> bool {
    h.world_messages(b, from)
        .iter()
        .any(|m| matches!(m, ServerMessage::ForgetObject(o) if o.0.id == OBJECT))
}

fn holds_boost(h: &Harness, me: u64) -> bool {
    h.server.players[&world_of(h, me)]
        .play
        .effects
        .iter()
        .any(|e| e.state == BOOST)
}

fn step_runs(h: &Harness, me: u64) -> bool {
    let st = step();
    yalain_world::quests::may_run_now(&h.server, world_of(h, me), &st, h.unix)
}

/// Run the clock on to `unix`, a second at a time.
fn run_to_unix(h: &mut Harness, unix: i64) {
    while h.unix < unix {
        h.step_by(1.0);
    }
    h.run(4);
}

/// Before its dated window an event's camp keeps no creatures, its object is made for nobody, its
/// step does not run and its effect is on no player; as the window starts the camp fills, the
/// object is made for the player that sees it, the step runs and the player holds the effect; as it
/// ends the creatures leave the world, the object is forgotten, the step stops and the effect runs
/// out with it (within the second the host's clock counts in).
/// Behaviour: server.events.gate-camps-objects-steps-and-players
#[test]
fn an_events_camp_object_step_and_effect_are_there_only_while_it_runs() {
    let start = 1_700_000_000 + 120;
    let (mut h, b, me) = world(vec![Window::Dated {
        from: start,
        until: start + 300,
    }]);
    assert_eq!(beetles(&h), 0, "no creatures before it");
    assert!(!made(&h, b, 0), "the object made for nobody");
    assert!(!step_runs(&h, me), "the step waits");
    assert!(!holds_boost(&h, me));
    let from = h.bots[b].inbox.len();
    run_to_unix(&mut h, start);
    assert_eq!(beetles(&h), 2, "the camp fills as it starts");
    assert!(
        made(&h, b, from),
        "the object is made for the player that sees it"
    );
    assert!(step_runs(&h, me), "the step runs");
    assert!(holds_boost(&h, me), "the player holds its effect");
    let from = h.bots[b].inbox.len();
    run_to_unix(&mut h, start + 300);
    assert_eq!(beetles(&h), 0, "its creatures leave the world as it ends");
    assert!(forgotten(&h, b, from), "the object is forgotten");
    assert!(!step_runs(&h, me), "the step stops");
    // The host's clock counts whole seconds: the effect runs out within one of the end.
    h.step_by(1.0);
    h.run(4);
    assert!(!holds_boost(&h, me), "the effect ran out with it");
}

/// A player who comes into the world while an event is on is given its effect, lasting to the
/// event's end and no longer, though the effect alone would last an hour.
/// Behaviour: server.events.gate-camps-objects-steps-and-players
#[test]
fn a_player_entering_during_an_event_is_given_its_effect_to_the_events_end() {
    let (mut h, _, _) = world(vec![Window::Dated {
        from: 1_700_000_000 - 60,
        until: 1_700_000_000 + 600,
    }]);
    let (_, late) = h.player_in_world("two", "Babyfare");
    h.run(4);
    let e = h.server.players[&world_of(&h, late)]
        .play
        .effects
        .iter()
        .find(|e| e.state == BOOST)
        .copied()
        .expect("given as it came in");
    assert!(
        e.timeout <= 600.0 && e.timeout > 590.0,
        "to the window's end: {}",
        e.timeout
    );
}

/// A weekly window runs on its days from its time to its end, into the next day when the end is
/// not after the start; a dated one from its start to its end, the end not included. Times are
/// UTC.
/// Behaviour: server.events.windows-in-utc
#[test]
fn an_events_windows_hold_their_days_and_times_in_utc() {
    use yalain_content::events::{time_of_day, utc};
    let fri = utc("2026-11-06T00:00Z").expect("a date");
    let sat_night = Window::Weekly {
        days: 1 << 5,
        from: time_of_day("22:00").expect("a time"),
        until: time_of_day("02:00").expect("a time"),
    };
    let sat = fri + 86_400;
    assert_eq!(sat_night.holding(sat + 21 * 3600), None, "not before 22:00");
    assert_eq!(
        sat_night.holding(sat + 23 * 3600),
        Some((sat + 22 * 3600, sat + 26 * 3600))
    );
    assert!(
        sat_night.holding(sat + 25 * 3600).is_some(),
        "on past midnight"
    );
    assert_eq!(sat_night.holding(sat + 26 * 3600), None, "over at 02:00");
    assert_eq!(sat_night.holding(fri + 23 * 3600), None, "not on a Friday");
    let whole_day = Window::Weekly {
        days: 1 << 6,
        from: 0,
        until: 0,
    };
    assert!(
        whole_day.holding(sat + 86_400 + 86_399).is_some(),
        "all of Sunday"
    );
    assert!(whole_day.holding(sat + 2 * 86_400).is_none(), "not Monday");
    let dated = Window::Dated {
        from: utc("2026-12-24T09:00Z").expect("a date"),
        until: utc("2026-12-25T09:00Z").expect("a date"),
    };
    assert_eq!(utc("2026-12-24T09:00Z"), Some(1_798_102_800));
    assert!(dated.holding(1_798_102_800).is_some());
    assert!(dated.holding(1_798_102_800 + 86_400).is_none());
    for bad in [
        "2026-12-24T09:00",
        "2026-02-30T09:00Z",
        "2026-12-24 09:00Z",
        "26-12-24T09:00Z",
    ] {
        assert_eq!(utc(bad), None, "{bad}");
    }
    assert_eq!(time_of_day("24:00"), None);
}

/// A world with no events gates nothing: the camp fills, the object is made and the step runs.
/// Behaviour: server.events.gate-camps-objects-steps-and-players
#[test]
fn a_world_with_no_events_gates_nothing() {
    let mut h = Harness::new();
    h.add_creatures(2);
    h.server.content.objects = vec![object()];
    let (b, _) = h.player_in_world("one", "Elara");
    h.run(10);
    assert_eq!(beetles(&h), 2);
    assert!(made(&h, b, 0));
}
