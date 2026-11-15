//! A world's tallies, on a clock the test drives: steps that fill one and wait on it, lines that
//! wait on it and say its count, the event it starts as it fills and lines that wait on that event,
//! its resets (as that event ends, by the calendar, and across a restart) and what a reset clears.
//! Fixture: two people beside the start, a quest step on each, and a tally the test gives its
//! rules.

use auberean_protocol::queue;
use auberean_script::args::Arg;
use auberean_script::native_object::VariableValue;
use yalain_content::events::{Event, EventTable, LineGate, ObjectGate, Window};
use yalain_content::quests::{QuestDef, QuestTable, Step, Trigger, Update};
use yalain_content::talk::{How, Line, Role, TalkTable, When, Words};
use yalain_content::tallies::{Adds, Fill, Gate, Holds, Reset, Start, Tally, TallyTable};
use yalain_content::{ContentObject, ContentPack, Description, Facts, Kind, Props};
use yalain_testkit::play::server_call;
use yalain_testkit::Harness;
use yalain_world::calls::Node;
use yalain_world::graph::{Builder, Slot};

const USE: u32 = 0x8216_0016;
const HEAR_TELL: u32 = 0x8205_0017;

/// The two people, and a sign the event shows.
const GATHERER: u64 = 0x8130_0000_0000_0F01;
const GATHERER_T: u32 = 0x4700_32B2;
const HOST: u64 = 0x8130_0000_0000_0F02;
const HOST_T: u32 = 0x4700_32B4;
const SIGN: u64 = 0x8130_0000_0000_0F03;
const SIGN_T: u32 = 0x4700_1154;
const SIGN_AT: [f32; 3] = [63.0, 241.0, -44.6];

/// A quest of each person's, and the lines its step tells.
const GATHER: u32 = 0x4000_0F01;
const CELEBRATE: u32 = 0x4000_0F02;
const TABLE: u32 = 0x2500_2A76;
const THANKS: u32 = 0x0980_FB37;
const ENJOY: u32 = 0x6ED6_48ED;

/// A line saying the count, its variable, and its full line; lines for each stage.
const COUNT: u32 = 0x0378_A89C;
const COUNT_VARIABLE: u32 = 0x5A17_0001;
const COMPLETED: u32 = 0x5B21_F83D;
const STILL_MUCH: u32 = 0x01DF_4E6D;
const ALL_SET: u32 = 0xDAF4_B939;

/// What a reset clears, left on the ground.
const DECORATION: u32 = 0x4700_18CF;

fn object(id: u64, template: u32, origin: [f32; 3]) -> ContentObject {
    ContentObject {
        object: Some(id),
        template,
        description: Description {
            name_text: None,
            package: 0x103E,
            look: 0x1F00_0100,
            enter_fx: 0,
            scale: None,
            name: Some((0xA860_7CEC, 0x2500_1B44)),
            numbers: [0; 6],
            source: "authored for a test".into(),
        },
        cell: 0x0299_0109,
        origin,
        rotation: [1.0, 0.0, 0.0, 0.0],
        kind: Kind::Npc,
        label: String::new(),
        placed: "authored for a test".into(),
        destination: None,
        facts: Facts::default(),
        walks: None,
        gate: None,
    }
}

fn quest(id: u32) -> QuestDef {
    QuestDef {
        duration_by_calendar: false,
        id,
        name: Words::Ref(0xA860_7CEC, TABLE),
        description: (0x6C45_9364, TABLE),
        icon: 0x4100_034B,
        play_fx: true,
        journals: vec![(0x2E5C_1E52, TABLE)],
        challenge: -999,
        duration: 0.0,
        timed_phase: 0,
        retry: 0.0,
        xp: 0,
        cancellable: true,
        placed: "authored for a test".into(),
        hidden: false,
        effects: Vec::new(),
        subquests: Vec::new(),
        min_level: 0,
        fails_on_death: false,
        fails_on_logout: false,
        fails_on_teleport: false,
        once: false,
        fellows: yalain_content::quests::Fellows::default(),
        ranges: Vec::new(),
        hero_only: false,
        pay: yalain_content::quests::CompletionPay::default(),
        required: None,
        cancel_wait: None,
    }
}

/// A step that moves no quest and tells a line, on a use of a person.
fn step(quest: u32, person: u32, tells: u32) -> Step {
    Step {
        out_of_turn: false,
        key: String::new(),
        quest,
        needs: 0,
        trigger: Trigger::Use(person),
        tell: Some(Words::Ref(tells, TABLE)),
        update: Update::Nothing,
        xp: 0,
        gives: Vec::new(),
        counts: Vec::new(),
        effects: Vec::new(),
        text: None,
        recipes: Vec::new(),
        placed: "authored for a test".into(),
        requires: Vec::new(),
        takes: Vec::new(),
        races: Vec::new(),
        effects_last: false,
        effects_quiet: false,
        wants: None,
        fellows: false,
        levels: None,
        xp_share: 0.0,
        to: None,
        transforms: None,
        chance: None,
        to_fellows: false,
    }
}

fn line(who: u32, string: u32) -> Line {
    Line {
        who,
        object: None,
        words: Words::Ref(string, TABLE),
        role: Some(Role::Use),
        how: How::Tell,
        when: When::Use,
        quest: None,
        update: None,
        phase: None,
        item: None,
        order: 0,
        measured: true,
        placed: "authored for a test".into(),
    }
}

fn gate(full: bool) -> Gate {
    Gate {
        tally: "supplies".into(),
        holds: if full { Holds::Full } else { Holds::NotFull },
    }
}

/// A tally of supplies, full at 50, that starts the fair a minute after it fills, for five
/// minutes, resets as the fair ends, and clears the decorations.
fn supplies() -> Tally {
    Tally {
        key: "supplies".into(),
        full: 50,
        starts: Some(Start {
            event: "fair".into(),
            after: 60,
            lasts: 300,
        }),
        resets: Reset::Ends("fair".into()),
        clears: vec![DECORATION],
    }
}

/// The world: the gatherer's step adds 20 to the supplies while they are not full, the host's
/// runs only once they are; the fair, with no windows of its own, shows the sign.
fn world(tally: Tally) -> Harness {
    let mut h = Harness::new();
    h.server.content = ContentPack {
        objects: vec![
            object(GATHERER, GATHERER_T, [61.0, 241.0, -44.6]),
            object(HOST, HOST_T, [62.0, 241.0, -44.6]),
            ContentObject {
                kind: Kind::Static,
                ..object(SIGN, SIGN_T, SIGN_AT)
            },
        ],
        ..ContentPack::default()
    };
    let gathers = step(GATHER, GATHERER_T, THANKS);
    let hosts = step(CELEBRATE, HOST_T, ENJOY);
    h.server.quests = QuestTable {
        quests: vec![quest(GATHER), quest(CELEBRATE)],
        steps: vec![gathers.clone(), hosts.clone()],
        gives: Vec::new(),
        rooms: Vec::new(),
    };
    h.server.content.events = EventTable {
        events: vec![Event {
            ended_by: Vec::new(),
            key: "fair".into(),
            game_event: None,
            windows: Vec::new(),
            on_players: Vec::new(),
            failure: None,
            hides: Vec::new(),
        }],
        objects: vec![ObjectGate {
            object: 2,
            event: "fair".into(),
        }],
        ..EventTable::default()
    };
    h.server.content.tallies = TallyTable {
        tallies: vec![tally],
        steps: vec![(gathers.clone(), gate(false)), (hosts, gate(true))],
        step_adds: vec![(
            gathers,
            Adds {
                tally: "supplies".into(),
                by: 20,
            },
        )],
        ..TallyTable::default()
    };
    h.server.catalog = yalain_testkit::test_catalog();
    h.server.catalog.insert(yalain_testkit::test_template(
        DECORATION,
        0x1067,
        Props::default(),
    ));
    h
}

fn use_of(item: u64, player: u64) -> Arg {
    let mut b = Builder::new();
    b.object(
        0x245,
        &[
            Slot::Ref(None),
            Slot::Ref(None),
            Slot::Long(item),
            Slot::Ref(None),
            Slot::Int(0),
            Slot::Long(player),
            Slot::Int((-1.0f32).to_bits()),
            Slot::Long(0),
            Slot::Int(0),
            Slot::Long(0),
            Slot::Int(0),
            Slot::Ref(None),
            Slot::Int(0),
            Slot::Int(316),
            Slot::Int(0),
            Slot::Int(0),
        ],
    );
    Arg::Object(b.graph())
}

/// Use a person; returns what it told the player: each tell's string and the count its variable
/// holds, if any.
fn use_it(h: &mut Harness, b: usize, me: u64, id: u64) -> Vec<(u32, Option<u32>)> {
    let from = h.bots[b].inbox.len();
    h.bots[b].send(true, queue::WEENIE, &server_call(USE, &[use_of(id, me)]));
    h.run(20);
    h.calls(b, from)
        .into_iter()
        .filter(|(f, _)| *f == HEAR_TELL)
        .filter_map(|(_, v)| {
            v.iter().find_map(|x| match x.root() {
                Some(Node::TextInfo(t)) => Some((
                    t.string_id,
                    t.variables.iter().find_map(|v| match v.value {
                        VariableValue::Word(n) if v.id == COUNT_VARIABLE => Some(n),
                        _ => None,
                    }),
                )),
                _ => None,
            })
        })
        .collect()
}

fn told(said: &[(u32, Option<u32>)]) -> Vec<u32> {
    said.iter().map(|(s, _)| *s).collect()
}

fn count(h: &Harness) -> u32 {
    yalain_world::tallies::count(&h.server, "supplies")
}

/// Run the clock on to `unix`, a second at a time.
fn run_to_unix(h: &mut Harness, unix: i64) {
    while h.unix < unix {
        h.step_by(1.0);
    }
    h.run(4);
}

/// A decoration left on the ground beside the start; its id.
fn decoration(h: &mut Harness) -> u64 {
    let id = yalain_world::inventory::make_item(&mut h.server, DECORATION, 1, 0).expect("made");
    if let Some(t) = h.server.things.get_mut(id) {
        t.at = Some(yalain_store::StoredPosition {
            cell: 0x0299_0109,
            origin: [60.0, 242.0, -44.6],
            rotation: [1.0, 0.0, 0.0, 0.0],
        });
    }
    id
}

/// Fill the supplies by the gatherer's step, three uses; returns when they filled.
fn fill(h: &mut Harness, b: usize, me: u64) -> i64 {
    for _ in 0..3 {
        use_it(h, b, me, GATHERER);
    }
    assert_eq!(count(h), 50);
    h.server.tallies.counts["supplies"]
        .filled
        .expect("the moment it filled")
}

/// Each use of the gatherer adds 20 to the supplies, never past their 50; once they are full the
/// gatherer's step no longer runs, and the host's, which waited on them full, does.
/// Behaviour: server.tallies.steps-and-gives-fill-a-shared-count
#[test]
fn a_step_adds_to_a_tally_never_past_full_and_what_waits_on_it_runs_once_it_is_full() {
    let mut h = world(supplies());
    let (b, me) = h.player_in_world("one", "Elara");
    h.run(4);
    assert_eq!(
        told(&use_it(&mut h, b, me, HOST)),
        Vec::<u32>::new(),
        "not yet"
    );
    assert_eq!(told(&use_it(&mut h, b, me, GATHERER)), vec![THANKS]);
    assert_eq!(count(&h), 20);
    use_it(&mut h, b, me, GATHERER);
    assert_eq!(count(&h), 40);
    assert_eq!(told(&use_it(&mut h, b, me, GATHERER)), vec![THANKS]);
    assert_eq!(count(&h), 50, "never past full");
    assert_eq!(
        told(&use_it(&mut h, b, me, GATHERER)),
        Vec::<u32>::new(),
        "it waits on them not full"
    );
    assert_eq!(count(&h), 50);
    assert_eq!(told(&use_it(&mut h, b, me, HOST)), vec![ENJOY], "full now");
}

/// A line that fills with a tally says its count as a percent of full, rounded down, as its one
/// variable, and its full line once the tally is full; a line waiting on the tally full, or not
/// yet full, is said only then.
/// Behaviour: server.tallies.lines-wait-on-and-say-a-count
#[test]
fn a_line_says_a_tallys_percent_until_it_is_full_and_lines_wait_on_its_stage() {
    let mut h = world(supplies());
    // The host's step aside: its use says one of its lines.
    h.server.quests.steps.retain(|s| s.quest == GATHER);
    h.server.talk = TalkTable {
        lines: vec![line(HOST_T, COUNT)],
    };
    h.server.content.tallies.fills = vec![(
        Words::Ref(COUNT, TABLE),
        Fill {
            tally: "supplies".into(),
            variable: COUNT_VARIABLE,
            full: Some(Words::Ref(COMPLETED, TABLE)),
        },
    )];
    let (b, me) = h.player_in_world("one", "Elara");
    h.run(4);
    assert_eq!(use_it(&mut h, b, me, HOST), vec![(COUNT, Some(0))]);
    use_it(&mut h, b, me, GATHERER);
    assert_eq!(
        use_it(&mut h, b, me, HOST),
        vec![(COUNT, Some(40))],
        "20 of 50"
    );
    use_it(&mut h, b, me, GATHERER);
    use_it(&mut h, b, me, GATHERER);
    assert_eq!(
        use_it(&mut h, b, me, HOST),
        vec![(COMPLETED, None)],
        "its full line once full"
    );

    // Lines for each stage.
    let still = line(HOST_T, STILL_MUCH);
    let set = line(HOST_T, ALL_SET);
    h.server.talk = TalkTable {
        lines: vec![still.clone(), set.clone()],
    };
    h.server.content.tallies.lines = vec![(still, gate(false)), (set, gate(true))];
    assert_eq!(told(&use_it(&mut h, b, me, HOST)), vec![ALL_SET], "full");
    h.server.tallies.counts.get_mut("supplies").expect("kept").n = 10;
    assert_eq!(
        told(&use_it(&mut h, b, me, HOST)),
        vec![STILL_MUCH],
        "not yet full"
    );
}

/// Lines may wait on a tally's count at least, and below, so many: each stage of the count says
/// its own line, one at a time, as the count rises through them.
/// Behaviour: server.tallies.lines-wait-on-and-say-a-count
#[test]
fn lines_wait_on_a_tallys_count_at_least_and_below_so_many() {
    let mut h = world(supplies());
    h.server.quests.steps.retain(|s| s.quest == GATHER);
    let stage = |string: u32| line(HOST_T, string);
    let (first, second, last) = (stage(STILL_MUCH), stage(COUNT), stage(ALL_SET));
    let at = |tally: &str, holds| Gate {
        tally: tally.into(),
        holds,
    };
    h.server.talk = TalkTable {
        lines: vec![first.clone(), second.clone(), last.clone()],
    };
    h.server.content.tallies.lines = vec![
        (first, at("supplies", Holds::Below(20))),
        (second.clone(), at("supplies", Holds::AtLeast(20))),
        (second, at("supplies", Holds::Below(40))),
        (last, at("supplies", Holds::AtLeast(40))),
    ];
    let (b, me) = h.player_in_world("one", "Elara");
    h.run(4);
    assert_eq!(
        told(&use_it(&mut h, b, me, HOST)),
        vec![STILL_MUCH],
        "below 20"
    );
    use_it(&mut h, b, me, GATHERER);
    assert_eq!(
        told(&use_it(&mut h, b, me, HOST)),
        vec![COUNT],
        "20, below 40"
    );
    use_it(&mut h, b, me, GATHERER);
    assert_eq!(told(&use_it(&mut h, b, me, HOST)), vec![ALL_SET], "40");
}

/// A line may wait on an event being on, or off, as well as on a tally: the host says its gathering
/// line while the supplies are wanted, its all-set line once they are full and before the fair
/// they start comes on, its fair line only while the fair is on, and its gathering line again as
/// the fair ends and the supplies go back to nothing.
/// Behaviour: server.events.lines-wait-on-an-event
#[test]
fn a_line_waiting_on_an_event_is_said_only_while_it_is_on_and_one_waiting_on_it_off_only_while_it_is_off(
) {
    let mut h = world(supplies());
    h.server.quests.steps.retain(|s| s.quest == GATHER);
    let (still, set, enjoy) = (
        line(HOST_T, STILL_MUCH),
        line(HOST_T, ALL_SET),
        line(HOST_T, ENJOY),
    );
    h.server.talk = TalkTable {
        lines: vec![still.clone(), set.clone(), enjoy.clone()],
    };
    h.server.content.tallies.lines = vec![(still, gate(false)), (set.clone(), gate(true))];
    h.server.content.events.lines = vec![
        (
            set,
            LineGate {
                event: "fair".into(),
                on: false,
            },
        ),
        (
            enjoy,
            LineGate {
                event: "fair".into(),
                on: true,
            },
        ),
    ];
    let (b, me) = h.player_in_world("one", "Elara");
    h.run(4);
    assert_eq!(
        told(&use_it(&mut h, b, me, HOST)),
        vec![STILL_MUCH],
        "gathering"
    );
    let filled = fill(&mut h, b, me);
    for _ in 0..4 {
        assert_eq!(
            told(&use_it(&mut h, b, me, HOST)),
            vec![ALL_SET],
            "full, the fair not yet on"
        );
    }
    run_to_unix(&mut h, filled + 60);
    assert!(h.server.events.on.contains("fair"));
    for _ in 0..4 {
        assert_eq!(
            told(&use_it(&mut h, b, me, HOST)),
            vec![ENJOY],
            "the fair on: its line alone, though the supplies are still full"
        );
    }
    run_to_unix(&mut h, filled + 60 + 300);
    assert!(!h.server.events.on.contains("fair"));
    assert_eq!(
        told(&use_it(&mut h, b, me, HOST)),
        vec![STILL_MUCH],
        "the fair over and the supplies back to nothing"
    );
}

/// An event on by its own window gates lines as one a tally starts does: before the window the
/// line waiting on it off is said, during it the line waiting on it on, after it the first again.
/// Behaviour: server.events.lines-wait-on-an-event
#[test]
fn a_line_waits_on_an_event_on_by_its_own_window() {
    let start = 1_700_000_000 + 120;
    let mut h = world(supplies());
    h.server.quests.steps.clear();
    h.server.content.events.events[0].windows = vec![Window::Dated {
        from: start,
        until: start + 300,
    }];
    let (off, on) = (line(HOST_T, ALL_SET), line(HOST_T, ENJOY));
    h.server.talk = TalkTable {
        lines: vec![off.clone(), on.clone()],
    };
    h.server.content.events.lines = vec![
        (
            off,
            LineGate {
                event: "fair".into(),
                on: false,
            },
        ),
        (
            on,
            LineGate {
                event: "fair".into(),
                on: true,
            },
        ),
    ];
    let (b, me) = h.player_in_world("one", "Elara");
    h.run(4);
    assert_eq!(told(&use_it(&mut h, b, me, HOST)), vec![ALL_SET], "before");
    run_to_unix(&mut h, start);
    assert_eq!(told(&use_it(&mut h, b, me, HOST)), vec![ENJOY], "during");
    run_to_unix(&mut h, start + 300);
    assert_eq!(told(&use_it(&mut h, b, me, HOST)), vec![ALL_SET], "after");
}

/// A tally that starts an event turns it on its `after` seconds past the moment it filled, for its
/// `lasts` seconds, an event with no windows of its own showing its object only then; as the event
/// ends the tally goes back to nothing and what it clears leaves the world.
/// Behaviour: server.tallies.a-full-count-starts-an-event-and-resets
#[test]
fn a_full_tally_starts_its_event_and_resets_as_the_event_ends_clearing_what_it_clears() {
    let mut h = world(supplies());
    let (b, me) = h.player_in_world("one", "Elara");
    h.run(4);
    assert!(!h.server.events.on.contains("fair"), "not on its own");
    let filled = fill(&mut h, b, me);
    let left = decoration(&mut h);
    h.run(4);
    assert!(!h.server.events.on.contains("fair"), "not yet");
    run_to_unix(&mut h, filled + 60);
    assert!(
        h.server.events.on.contains("fair"),
        "a minute after it filled"
    );
    assert!(
        h.server
            .players
            .values()
            .all(|p| p.known.contains_key(&SIGN)),
        "its sign stands"
    );
    assert_eq!(count(&h), 50, "still full while it runs");
    assert!(h.server.things.get(left).is_some());
    run_to_unix(&mut h, filled + 60 + 300);
    assert!(!h.server.events.on.contains("fair"), "over");
    assert_eq!(count(&h), 0, "back to nothing as it ends");
    assert!(
        h.server.things.get(left).is_none(),
        "the decoration cleared"
    );
    assert_eq!(
        told(&use_it(&mut h, b, me, GATHERER)),
        vec![THANKS],
        "filling again"
    );
    assert_eq!(count(&h), 20);
}

/// A restart keeps a tally's count and the moment it filled, so the event it started is on again
/// for what is left of its time; and a tally whose event ended while the world was down resets
/// at the first step.
/// Behaviour: server.tallies.a-count-outlives-a-restart
#[test]
fn a_tally_keeps_its_count_and_its_events_time_across_a_restart() {
    let mut h = world(supplies());
    let (b, me) = h.player_in_world("one", "Elara");
    h.run(4);
    use_it(&mut h, b, me, GATHERER);
    h.run(1);
    h.server.tallies = yalain_world::tallies::Tallies::default();
    h.run(4);
    assert_eq!(count(&h), 20, "read back from the world's record");
    let filled = fill(&mut h, b, me);
    run_to_unix(&mut h, filled + 90);
    assert!(h.server.events.on.contains("fair"));
    h.server.tallies = yalain_world::tallies::Tallies::default();
    h.server.events = yalain_world::events::Events::default();
    h.run(4);
    assert_eq!(count(&h), 50);
    assert!(h.server.events.on.contains("fair"), "on again for its rest");

    // Down from before the fair's end to after it: the start it made is over, so it resets.
    let record = yalain_world::tallies::Tallies {
        loaded: true,
        counts: [(
            "supplies".to_string(),
            yalain_world::tallies::Count {
                n: 50,
                filled: Some(h.unix - 1000),
                period: None,
            },
        )]
        .into(),
        on: ["fair".to_string()].into(),
    }
    .to_bytes();
    yalain_store::Store::set_world_record(
        &mut h.server.store,
        yalain_world::tallies::RECORD,
        &record,
    )
    .expect("kept");
    h.server.tallies = yalain_world::tallies::Tallies::default();
    h.server.events = yalain_world::events::Events::default();
    h.run(4);
    assert_eq!(count(&h), 0, "its fair ended while the world was down");
    assert!(!h.server.events.on.contains("fair"));
}

/// A tally on a clock event resets as that event ends, and also when it ended while the world was
/// down, by the events the record kept as on; one that resets every so many seconds does at each
/// new period of the calendar.
/// Behaviour: server.tallies.a-full-count-starts-an-event-and-resets
#[test]
fn a_tally_resets_as_a_clock_event_ends_and_every_period_of_the_calendar() {
    let on_the_clock = Tally {
        starts: None,
        resets: Reset::Ends("fair".into()),
        clears: Vec::new(),
        ..supplies()
    };
    let mut h = world(on_the_clock.clone());
    let start = h.unix + 30;
    h.server.content.events.events[0].windows = vec![Window::Dated {
        from: start,
        until: start + 120,
    }];
    let (b, me) = h.player_in_world("one", "Elara");
    use_it(&mut h, b, me, GATHERER);
    assert_eq!(count(&h), 20);
    run_to_unix(&mut h, start + 60);
    assert!(h.server.events.on.contains("fair"));
    assert_eq!(count(&h), 20, "it keeps its count while the fair runs");
    run_to_unix(&mut h, start + 120);
    assert_eq!(count(&h), 0, "as the fair ends");

    // The world down from the fair's time to after it: the record says whether it was on.
    for (was_on, left) in [(true, 0), (false, 30)] {
        let mut h = world(on_the_clock.clone());
        h.server.content.events.events[0].windows = vec![Window::Dated {
            from: h.unix - 1000,
            until: h.unix - 500,
        }];
        let record = yalain_world::tallies::Tallies {
            loaded: true,
            counts: [(
                "supplies".to_string(),
                yalain_world::tallies::Count {
                    n: 30,
                    filled: None,
                    period: None,
                },
            )]
            .into(),
            on: if was_on {
                ["fair".to_string()].into()
            } else {
                std::collections::BTreeSet::new()
            },
        }
        .to_bytes();
        yalain_store::Store::set_world_record(
            &mut h.server.store,
            yalain_world::tallies::RECORD,
            &record,
        )
        .expect("kept");
        h.run(4);
        assert_eq!(
            count(&h),
            left,
            "the fair was on when the world went down: {was_on}"
        );
    }

    let mut h = world(Tally {
        starts: None,
        resets: Reset::Every(600),
        clears: Vec::new(),
        ..supplies()
    });
    let (b, me) = h.player_in_world("one", "Elara");
    h.run(4);
    use_it(&mut h, b, me, GATHERER);
    assert_eq!(count(&h), 20);
    let next = (h.unix.div_euclid(600) + 1) * 600;
    run_to_unix(&mut h, next - 2);
    assert_eq!(count(&h), 20, "within its period");
    run_to_unix(&mut h, next + 1);
    assert_eq!(count(&h), 0, "at the next");
}

/// A world with no tallies keeps no record of them.
/// Behaviour: server.tallies.steps-and-gives-fill-a-shared-count
#[test]
fn a_world_with_no_tallies_keeps_no_record_of_them() {
    let mut h = world(supplies());
    h.server.content.tallies = TallyTable::default();
    let (b, me) = h.player_in_world("one", "Elara");
    h.run(4);
    assert_eq!(told(&use_it(&mut h, b, me, GATHERER)), vec![THANKS]);
    h.run(2);
    let kept =
        yalain_store::Store::world_record(&mut h.server.store, yalain_world::tallies::RECORD)
            .expect("reads");
    assert_eq!(kept, None);
}

/// Two steps of the gatherer's, alike but for their keys, their tally gates and their adds, keep
/// their own: the one waiting on the supplies not full runs and adds 20 to them alone; once they are
/// full the other, waiting on them full, runs and adds 1 to the thanks alone.
/// Behaviour: server.tallies.steps-and-gives-fill-a-shared-count
#[test]
fn steps_alike_but_for_their_tallies_wait_on_and_fill_only_their_own() {
    let mut h = world(supplies());
    let short = Step {
        key: "short".into(),
        ..step(GATHER, GATHERER_T, THANKS)
    };
    let full = Step {
        key: "full".into(),
        ..step(GATHER, GATHERER_T, THANKS)
    };
    h.server.quests.steps = vec![short.clone(), full.clone()];
    let adds = |tally: &str, by: u32| Adds {
        tally: tally.into(),
        by,
    };
    h.server.content.tallies = TallyTable {
        tallies: vec![
            supplies(),
            Tally {
                key: "thanks".into(),
                full: 10,
                starts: None,
                resets: Reset::Never,
                clears: Vec::new(),
            },
        ],
        steps: vec![(short.clone(), gate(false)), (full.clone(), gate(true))],
        step_adds: vec![(short, adds("supplies", 20)), (full, adds("thanks", 1))],
        ..TallyTable::default()
    };
    let (b, me) = h.player_in_world("one", "Elara");
    h.run(4);
    let thanks = |h: &Harness| yalain_world::tallies::count(&h.server, "thanks");
    for at in [20, 40, 50] {
        assert_eq!(told(&use_it(&mut h, b, me, GATHERER)), vec![THANKS]);
        assert_eq!((count(&h), thanks(&h)), (at, 0), "the short one's");
    }
    assert_eq!(told(&use_it(&mut h, b, me, GATHERER)), vec![THANKS]);
    assert_eq!((count(&h), thanks(&h)), (50, 1), "the full one's");
}
