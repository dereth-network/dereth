//! Quest steps that run for a player at most once in a while: run, the step waits its time for that
//! player, a use of its person meanwhile answered with the person's too-soon line; the wait is the
//! player's own and kept while the player is away.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_content::quests::{QuestDef, QuestTable, Step, Trigger, Update};
use yalain_content::talk::{How, Line, Role, TalkTable, When};
use yalain_content::{ContentObject, ContentPack, Description, Facts, Kind, Props, Words};
use yalain_testkit::play::server_call;
use yalain_testkit::Harness;
use yalain_world::calls::{self, Node};
use yalain_world::graph::{Builder, Slot};
use yalain_world::quests::status;
use yalain_world::state::QuestState;

const USE: u32 = 0x8216_0016;
const HEAR_TELL: u32 = 0x8205_0017;
const SHOW_TEXT: u32 = 0x8205_0011;

/// The errand under way, two people each handing a scroll to one on it, and their lines.
const ERRAND: u32 = 0x4000_027F;
const TABLE: u32 = 0x2500_2A4C;
const KYRBE: u32 = 0x4700_3220;
const MANAWA: u32 = 0x4700_3241;
const KYRBE_ID: u64 = 0x8130_0000_0000_0801;
const MANAWA_ID: u64 = 0x8130_0000_0000_0802;
const KYRBE_SCROLL: u32 = 0x4700_092D;
const MANAWA_SCROLL: u32 = 0x4700_092A;
const TAKE_THIS: u32 = 0x0FBA_3EAC;
const NOT_YET: u32 = 0xDC2A_290B;
const GREETING: u32 = 0x1516_2DCE;
/// An hour.
const WAIT: u32 = 3600;
/// The two steps' keys.
const ANKORO: &str = "scroll-ankoro";
const SHINOKO: &str = "scroll-shinoko";

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
            name: Some((0x071F_10D1, TABLE)),
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

fn errand() -> QuestDef {
    QuestDef {
        duration_by_calendar: false,
        id: ERRAND,
        name: Words::Ref(0xA860_7CEC, TABLE),
        description: (0x6C45_9364, TABLE),
        icon: 0x4100_034B,
        play_fx: true,
        journals: vec![(0x2E5C_1E52, TABLE), (0x2E5F_1E52, TABLE)],
        challenge: 15,
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

/// A step of the errand under way that hands over a scroll at a use of its person, moving nothing.
fn scroll_step(key: &str, person: u32, scroll: u32) -> Step {
    Step {
        out_of_turn: false,
        skills: Vec::new(),
        key: key.into(),
        quest: ERRAND,
        needs: 1,
        trigger: Trigger::Use(person),
        tell: Some(Words::Ref(TAKE_THIS, TABLE)),
        update: Update::Nothing,
        xp: 0,
        gives: vec![scroll],
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

fn line(who: u32, string: u32, role: Option<Role>) -> Line {
    Line {
        who,
        object: None,
        words: Words::Ref(string, TABLE),
        role,
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

/// The two people, their scroll steps (those of the keys in `waits` waiting an hour between runs),
/// each person's greeting at a plain use, and their too-soon line when `too_soon`.
fn world(waits: &[&str], too_soon: bool) -> Harness {
    let mut h = Harness::new();
    let mut content = ContentPack {
        objects: vec![
            object(KYRBE_ID, KYRBE, [61.0, 241.0, -44.6]),
            object(MANAWA_ID, MANAWA, [62.0, 241.0, -44.6]),
        ],
        ..ContentPack::default()
    };
    for key in waits {
        content.step_waits.insert((ERRAND, (*key).into()), WAIT);
    }
    h.server.content = content;
    h.server.quests = QuestTable {
        quests: vec![errand()],
        steps: vec![
            scroll_step(ANKORO, KYRBE, KYRBE_SCROLL),
            scroll_step(SHINOKO, MANAWA, MANAWA_SCROLL),
        ],
        gives: Vec::new(),
        rooms: Vec::new(),
    };
    let mut lines = vec![line(KYRBE, GREETING, None), line(MANAWA, GREETING, None)];
    if too_soon {
        lines.push(line(KYRBE, NOT_YET, Some(Role::TooSoon)));
        lines.push(line(MANAWA, NOT_YET, Some(Role::TooSoon)));
    }
    h.server.talk = TalkTable { lines };
    for scroll in [KYRBE_SCROLL, MANAWA_SCROLL] {
        let made = yalain_testkit::test_template(scroll, 0x1067, Props::default());
        h.server.catalog.insert(made);
    }
    h
}

/// Every player in the world on the errand, at its first phase.
fn on_the_errand(h: &mut Harness) {
    let at = h.unix;
    for p in h.server.players.values_mut() {
        p.play.quests = vec![QuestState {
            quest: ERRAND,
            status: status::UNDERWAY,
            phase: 1,
            started: at,
            updated: at,
        }];
    }
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

/// Use a person: the string ids of the texts the player was told or shown at the use.
fn use_it(h: &mut Harness, b: usize, me: u64, id: u64) -> Vec<u32> {
    let from = h.bots[b].inbox.len();
    h.bots[b].send(true, queue::WEENIE, &server_call(USE, &[use_of(id, me)]));
    h.run(20);
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c)
                if c.function.0 == HEAR_TELL || c.function.0 == SHOW_TEXT =>
            {
                let v = c
                    .args
                    .as_ref()
                    .and_then(|a| calls::values(&a.0).ok())
                    .unwrap_or_default();
                v.iter().find_map(|x| match x.root() {
                    Some(Node::TextInfo(t)) => Some(t.string_id),
                    _ => None,
                })
            }
            _ => None,
        })
        .collect()
}

/// How many of an item the character holds.
fn held(h: &Harness, character: u64, template: u32) -> u32 {
    h.server
        .players
        .values()
        .find(|p| p.character == character)
        .map_or(0, |p| {
            p.play
                .items
                .iter()
                .filter(|i| i.template == template)
                .map(|i| i.quantity)
                .sum()
        })
}

/// A step that waits an hour between runs hands its scroll over, then for the rest of the hour its
/// person says its too-soon line and gives nothing, a minute short of the hour too, and the step
/// may not run until the second the hour is over; past the hour it hands another over. Another
/// person's step with no wait gives at every use meanwhile.
/// Behaviour: server.quests.a-step-waits-between-its-runs-for-a-player
#[test]
fn a_step_with_a_wait_runs_then_its_person_says_its_too_soon_line_until_the_wait_is_over() {
    let mut h = world(&[ANKORO], true);
    let (b, me) = h.player_in_world("one", "Elara");
    on_the_errand(&mut h);
    assert_eq!(use_it(&mut h, b, me, KYRBE_ID), [TAKE_THIS], "handed over");
    assert_eq!(held(&h, me, KYRBE_SCROLL), 1);
    h.unix += 1_800;
    assert_eq!(
        use_it(&mut h, b, me, KYRBE_ID),
        [NOT_YET],
        "half an hour on"
    );
    assert_eq!(held(&h, me, KYRBE_SCROLL), 1, "nothing more");
    for _ in 0..2 {
        assert_eq!(use_it(&mut h, b, me, MANAWA_ID), [TAKE_THIS]);
    }
    assert_eq!(held(&h, me, MANAWA_SCROLL), 2, "no wait: at every use");
    let ran = h
        .server
        .players
        .values()
        .find(|p| p.character == me)
        .and_then(|p| p.play.step_waits.first().map(|w| w.ran))
        .expect("its wait");
    let world = *h.server.players.keys().next().expect("in the world");
    let step = scroll_step(ANKORO, KYRBE, KYRBE_SCROLL);
    let at = |h: &Harness, t: i64| yalain_world::quests::may_run_now(&h.server, world, &step, t);
    let hour = i64::from(WAIT);
    assert!(!at(&h, ran + hour - 1), "a second short");
    assert!(at(&h, ran + hour), "the hour over");
    h.unix = ran + hour - 60;
    assert_eq!(use_it(&mut h, b, me, KYRBE_ID), [NOT_YET], "a minute short");
    assert_eq!(held(&h, me, KYRBE_SCROLL), 1);
    h.unix = ran + hour;
    assert_eq!(
        use_it(&mut h, b, me, KYRBE_ID),
        [TAKE_THIS],
        "the hour over"
    );
    assert_eq!(held(&h, me, KYRBE_SCROLL), 2);
    assert_eq!(
        use_it(&mut h, b, me, KYRBE_ID),
        [NOT_YET],
        "and it waits again"
    );
}

/// A step's wait is its own player's and its own step's: another player is handed the scroll at
/// once, and another step's run leaves it waiting; the first player, leaving the world and coming
/// back within the hour, still waits on both, kept with its play state; once over, both scrolls
/// are handed over again.
/// Behaviour: server.quests.a-step-waits-between-its-runs-for-a-player
#[test]
fn a_steps_wait_is_its_own_players_and_is_kept_while_the_player_is_away() {
    let mut h = world(&[ANKORO, SHINOKO], true);
    let (b, me) = h.player_in_world("one", "Elara");
    let (other, them) = h.player_in_world("two", "Rowan");
    on_the_errand(&mut h);
    assert_eq!(use_it(&mut h, b, me, KYRBE_ID), [TAKE_THIS]);
    assert_eq!(
        use_it(&mut h, other, them, KYRBE_ID),
        [TAKE_THIS],
        "not mine"
    );
    h.unix += 60;
    assert_eq!(
        use_it(&mut h, b, me, MANAWA_ID),
        [TAKE_THIS],
        "another step"
    );
    assert_eq!(
        use_it(&mut h, b, me, KYRBE_ID),
        [NOT_YET],
        "the first still waits"
    );
    h.unix += 540;
    h.bots[b].leave_world();
    assert!(h.run_until(400, |h| !h
        .server
        .players
        .values()
        .any(|p| p.character == me)));
    h.run(20);
    h.unix += 600;
    h.enter_world(b, "one", me);
    let play = h
        .server
        .players
        .values()
        .find(|p| p.character == me)
        .map(|p| p.play.clone())
        .expect("back");
    assert_eq!(
        yalain_world::state::PlayState::from_bytes(&play.to_bytes()).expect("reads"),
        play,
        "kept whole"
    );
    assert_eq!(play.step_waits.len(), 2, "both waits kept");
    assert_eq!(
        use_it(&mut h, b, me, KYRBE_ID),
        [NOT_YET],
        "back, still waiting"
    );
    assert_eq!(use_it(&mut h, b, me, MANAWA_ID), [NOT_YET]);
    assert_eq!(held(&h, me, KYRBE_SCROLL), 1);
    h.unix += i64::from(WAIT);
    assert_eq!(use_it(&mut h, b, me, KYRBE_ID), [TAKE_THIS], "over");
    assert_eq!(use_it(&mut h, b, me, MANAWA_ID), [TAKE_THIS]);
    assert_eq!(held(&h, me, KYRBE_SCROLL), 2);
    assert_eq!(held(&h, me, MANAWA_SCROLL), 2);
}

/// Within a step's wait a person with no too-soon line says what it says at a plain use and gives
/// nothing; one with a too-soon line says what it says at a plain use to a player the step would
/// not run for anyway (its quest done); one whose step names no wait gives at every use.
/// Behaviour: server.quests.a-step-waits-between-its-runs-for-a-player
#[test]
fn within_a_steps_wait_a_person_with_no_too_soon_line_greets_and_gives_nothing() {
    let mut h = world(&[ANKORO], false);
    let (b, me) = h.player_in_world("one", "Elara");
    on_the_errand(&mut h);
    assert_eq!(use_it(&mut h, b, me, KYRBE_ID), [TAKE_THIS]);
    assert_eq!(
        use_it(&mut h, b, me, KYRBE_ID),
        [GREETING],
        "its plain line"
    );
    assert_eq!(held(&h, me, KYRBE_SCROLL), 1);

    let mut h = world(&[ANKORO], true);
    let (b, me) = h.player_in_world("one", "Elara");
    on_the_errand(&mut h);
    assert_eq!(use_it(&mut h, b, me, KYRBE_ID), [TAKE_THIS]);
    for p in h.server.players.values_mut() {
        p.play.quests[0].status = status::COMPLETED;
        p.play.quests[0].phase = 0;
    }
    assert_eq!(
        use_it(&mut h, b, me, KYRBE_ID),
        [GREETING],
        "nothing to wait for"
    );

    let mut h = world(&[], true);
    let (b, me) = h.player_in_world("one", "Elara");
    on_the_errand(&mut h);
    for _ in 0..3 {
        assert_eq!(use_it(&mut h, b, me, KYRBE_ID), [TAKE_THIS]);
    }
    assert_eq!(held(&h, me, KYRBE_SCROLL), 3, "no wait named");
}
