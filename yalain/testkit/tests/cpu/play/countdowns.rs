//! A countdown is a record that puts its result on its holder as it runs out: a quest's next phase
//! closes the window its quest opened, a chain puts its effects on; one taken off sooner puts on
//! nothing, and one that ran out while its player was away runs out as the player comes back. A
//! result holding a change over time with a place of its own comes only to a living holder within
//! its reach, and such a change harms only there. A text effect shows in its player's chat or in a
//! box of its own. Fixture: a player alone in the test world, with effects modeled on the
//! install's Infiltrate the Nursery countdowns, hotspot and popup put in its server's table.

use auberean_script::native_object::{TextInfo, VariableValue};
use yalain_content::quests::{QuestDef, QuestTable, Update};
use yalain_net::ConnId;
use yalain_testkit::Harness;
use yalain_world::calls::{Node, Value};
use yalain_world::craft::function::ADD_RECIPE;
use yalain_world::effect_defs::{text_how, Area, EffectDef, Kind, Scale, Text};
use yalain_world::effects::{self, function::ADD as ADD_EFFECT};
use yalain_world::player::quality;
use yalain_world::popups::DISPLAY_MESSAGE;
use yalain_world::quals;
use yalain_world::session::function::SHOW_TEXT;

/// A countdown of the install's shape (30 s at spellcraft 1 to 900 s at 50, kept from its
/// holder's client), whose result teaches a recipe.
const COUNTDOWN: u32 = 0x6F00_0E1E;
const TAUGHT: u32 = 0x6F00_19EF;
const RECIPE: u32 = 0x6F00_02A6;
/// A countdown whose result is a chain of a hotspot and a popup, as the damage countdown's.
const DAMAGE: u32 = 0x6F00_0E1F;
const DETECTED: u32 = 0x6F00_02C8;
const HOTSPOT: u32 = 0x6F00_0E20;
const POPUP: u32 = 0x6F00_0E21;
const POPUP_WORDS: (u32, u32) = (0xA727_FC98, 0x2500_165E);
/// A text shown in chat (Purple Plague's sneeze).
const SNEEZE: u32 = 0x6F00_0E90;
const SNEEZE_WORDS: (u32, u32) = (0x05E6_5916, 0x2500_16FD);
/// A quest whose advance to its second phase starts a countdown that moves it to its third.
const QUEST: u32 = 0x8000_01DC;
const WINDOW: u32 = 0x6F00_0E4D;
const NEXT_PHASE: u32 = 0x6F00_02C9;

fn effect(state: u32, kind: Kind) -> EffectDef {
    EffectDef {
        state,
        kind,
        duration: Scale(Vec::new()),
        priority: Scale(Vec::new()),
        fx: 0,
        external_flags: 0,
        internal_flags: 0x4140_0512,
        ends_at_logout: false,
        ends_at_teleport: false,
        enum_value: 0,
        eq_class: 0,
        name: None,
    }
}

fn countdown(state: u32, result: u32) -> EffectDef {
    EffectDef {
        duration: Scale(vec![(1.0, 30.0), (50.0, 900.0)]),
        priority: Scale(vec![(0.0, 10.0)]),
        internal_flags: 0x0100_0742,
        eq_class: 127,
        ..effect(
            state,
            Kind::Countdown {
                result: Some(result),
            },
        )
    }
}

/// A player in the world, with the effects in its server's table; the hotspot's place is where
/// the player stands.
fn player() -> (Harness, usize, u64, ConnId) {
    let mut h = Harness::new();
    let (b, me) = h.player_in_world("one", "Elara");
    h.run(10);
    let world = *h.server.players.keys().next().expect("in the world");
    let at = h.server.players[&world].position;
    let t = &mut h.server.effect_table;
    t.effects.insert(COUNTDOWN, countdown(COUNTDOWN, TAUGHT));
    t.effects
        .insert(TAUGHT, effect(TAUGHT, Kind::GrantRecipe(RECIPE)));
    t.effects.insert(DAMAGE, countdown(DAMAGE, DETECTED));
    t.effects.insert(
        DETECTED,
        effect(DETECTED, Kind::Instants(vec![HOTSPOT, POPUP])),
    );
    t.effects.insert(
        HOTSPOT,
        EffectDef {
            external_flags: 7,
            internal_flags: 0x2340_0783,
            eq_class: 128,
            ..effect(
                HOTSPOT,
                Kind::VitalOverTime {
                    flags: 7,
                    per_second: Scale(vec![(0.0, -25.0)]),
                },
            )
        },
    );
    t.areas.insert(HOTSPOT, Area { at, radius: 300.0 });
    for (state, words, how) in [
        (POPUP, POPUP_WORDS, text_how::POPUP),
        (SNEEZE, SNEEZE_WORDS, text_how::CHAT),
    ] {
        t.effects.insert(
            state,
            EffectDef {
                external_flags: how,
                ..effect(
                    state,
                    Kind::Text(Text {
                        words,
                        kind: 8,
                        how,
                    }),
                )
            },
        );
    }
    (h, b, me, world)
}

fn put_on(h: &mut Harness, world: ConnId, state: u32, spellcraft: f32) -> bool {
    let caster = h.server.players[&world].character;
    let cast = effects::Cast {
        caster,
        spellcraft,
        ..effects::Cast::default()
    };
    let clock = h.clock();
    effects::apply(&mut h.server, &mut h.net, world, state, cast, clock)
}

/// Run the world for so many seconds, a second a step, then the few steps that deliver what it
/// sent last.
fn wait(h: &mut Harness, seconds: f64) {
    let t = h.now.0 + seconds;
    while h.now.0 < t {
        h.step_by(1.0);
    }
    h.run(4);
}

fn taught(h: &Harness, b: usize, from: usize) -> bool {
    h.functions(b, from).contains(&ADD_RECIPE)
}

fn records(h: &Harness, world: ConnId) -> Vec<u32> {
    h.server.players[&world]
        .play
        .effects
        .iter()
        .map(|e| e.state)
        .collect()
}

/// Give the player health enough to stand a hotspot for a while.
fn sturdy(h: &mut Harness, world: ConnId) {
    let p = h.server.players.get_mut(&world).expect("in the world");
    quals::set_int(&mut p.qualities, quality::HEALTH_MAX, 10_000);
    quals::set_int(&mut p.qualities, quality::HEALTH, 10_000);
}

fn health(h: &Harness, world: ConnId) -> i32 {
    quals::int(&h.server.players[&world].qualities, quality::HEALTH).unwrap_or(0)
}

/// The texts a client was shown since `from` by a call: each string, table and the literal of its
/// name variable.
fn shown(h: &Harness, b: usize, from: usize, call: u32) -> Vec<((u32, u32), Option<String>)> {
    h.calls(b, from)
        .into_iter()
        .filter(|(f, _)| *f == call)
        .filter_map(|(_, v)| {
            v.iter().find_map(|x| match x.root() {
                Some(Node::TextInfo(t)) => Some(t.clone()),
                _ => None,
            })
        })
        .map(|t: TextInfo| {
            let named = t.variables.iter().find_map(|v| match &v.value {
                VariableValue::Nested(n) if v.id == effects::TEXT_NAME => {
                    n.literal.as_ref().map(|l| String::from_utf16_lossy(l))
                }
                _ => None,
            });
            ((t.string_id, t.table), named)
        })
        .collect()
}

/// Put at the spellcraft its quest casts it at (50), the countdown runs its 900 s and only then
/// puts its result on: the recipe is taught as it runs out, not a second before. Its holder's
/// client is never told of it, at its start or at its end.
/// Behaviour: server.effects.a-countdown-puts-its-result-on-as-it-runs-out
#[test]
fn a_countdown_puts_its_result_on_as_it_runs_out_and_not_before() {
    let (mut h, b, _, world) = player();
    let from = h.bots[b].inbox.len();
    assert!(put_on(&mut h, world, COUNTDOWN, 50.0));
    assert_eq!(records(&h, world), [COUNTDOWN]);
    wait(&mut h, 898.0);
    assert!(!taught(&h, b, from), "nothing before its end");
    assert_eq!(records(&h, world), [COUNTDOWN], "still running");
    wait(&mut h, 4.0);
    assert!(taught(&h, b, from), "its result as it runs out");
    assert!(records(&h, world).is_empty(), "gone");
    let calls = h.functions(b, from);
    assert!(
        !calls.contains(&ADD_EFFECT) && !calls.contains(&effects::function::REMOVE),
        "never told: {calls:08X?}"
    );
}

/// A countdown taken off before its end puts nothing on.
/// Behaviour: server.effects.a-countdown-puts-its-result-on-as-it-runs-out
#[test]
fn a_countdown_taken_off_before_its_end_puts_on_nothing() {
    let (mut h, b, _, world) = player();
    let from = h.bots[b].inbox.len();
    assert!(put_on(&mut h, world, COUNTDOWN, 1.0));
    let eid = h.server.players[&world].play.effects[0].eid;
    wait(&mut h, 10.0);
    assert!(effects::remove(&mut h.server, &mut h.net, world, eid));
    wait(&mut h, 60.0);
    assert!(!taught(&h, b, from));
}

/// A countdown that ran out while its player was away runs out as the player comes back: its
/// result comes after the session start.
/// Behaviour: server.effects.a-countdown-run-out-away-runs-out-on-return
#[test]
fn a_countdown_that_ran_out_while_away_runs_out_as_its_player_comes_back() {
    let (mut h, b, me, world) = player();
    assert!(put_on(&mut h, world, COUNTDOWN, 1.0));
    h.run(2);
    log_off(&mut h, b, me);
    wait(&mut h, 120.0);
    let from = h.bots[b].inbox.len();
    assert!(!taught(&h, b, from - 1), "not while away");
    h.enter_world(b, "one", me);
    wait(&mut h, 5.0);
    let calls = h.functions(b, from);
    let start = calls.iter().position(|f| *f == 0x8212_0027);
    let learned = calls.iter().position(|f| *f == ADD_RECIPE);
    assert!(
        matches!((start, learned), (Some(s), Some(l)) if s < l),
        "{calls:08X?}"
    );
    let world = *h.server.players.keys().next().expect("back in the world");
    assert!(records(&h, world).is_empty());
}

fn log_off(h: &mut Harness, b: usize, id: u64) {
    use auberean_protocol::login::CharacterRef;
    use auberean_protocol::{queue, ClientMessage, ServerMessage};
    let before = h.bots[b].inbox.len();
    h.bots[b].send(
        false,
        queue::LOGON,
        &ClientMessage::LogOff(CharacterRef { character: id }),
    );
    assert!(h.run_until(400, |h| h.bots[b].inbox[before..]
        .iter()
        .any(|r| matches!(r.message(), Some(ServerMessage::CharacterList(_))))));
}

fn quest_def() -> QuestDef {
    QuestDef {
        duration_by_calendar: false,
        id: QUEST,
        name: yalain_content::Words::Ref(0, 0),
        description: (0, 0),
        icon: 0,
        play_fx: false,
        journals: vec![(1, 1), (2, 2), (3, 3)],
        challenge: 0,
        duration: 0.0,
        timed_phase: 0,
        retry: 0.0,
        xp: 0,
        cancellable: false,
        placed: "authored for a test".into(),
        hidden: false,
        effects: vec![(0x2000_0002, WINDOW, 50.0)],
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

fn phase(h: &Harness, world: ConnId) -> u32 {
    yalain_world::quests::phase_of(&h.server.players[&world].play.quests, QUEST)
}

/// A quest whose advance to its second phase starts a countdown, as the tent quests' do: the phase
/// is a window that closes 900 s later, the countdown moving the quest to its third phase; a quest
/// that moved on before then is left where it is.
/// Behaviour: server.effects.a-countdown-puts-its-result-on-as-it-runs-out
#[test]
fn a_quests_countdown_closes_its_window_by_moving_it_to_its_next_phase() {
    let (mut h, _, _, world) = player();
    h.server.quests = QuestTable {
        quests: vec![quest_def()],
        ..QuestTable::default()
    };
    let t = &mut h.server.effect_table.effects;
    t.insert(WINDOW, countdown(WINDOW, NEXT_PHASE));
    t.insert(
        NEXT_PHASE,
        effect(
            NEXT_PHASE,
            Kind::Quest {
                quest: QUEST,
                update: 0x2000_0003,
            },
        ),
    );
    let update = |h: &mut Harness, up| {
        let clock = h.clock();
        yalain_world::quests::apply(&mut h.server, &mut h.net, world, QUEST, up, 0, clock);
    };
    update(&mut h, Update::Bestow);
    update(&mut h, Update::Advance(2));
    assert_eq!(records(&h, world), [WINDOW], "the window's countdown");
    wait(&mut h, 890.0);
    assert_eq!(phase(&h, world), 2, "the window still open");
    wait(&mut h, 15.0);
    assert_eq!(phase(&h, world), 3, "the window closed");
    // Moved on by the player before the window closes: the countdown leaves it there.
    update(&mut h, Update::Complete);
    update(&mut h, Update::Bestow);
    update(&mut h, Update::Advance(2));
    update(&mut h, Update::Advance(3));
    update(&mut h, Update::Complete);
    wait(&mut h, 905.0);
    assert!(yalain_world::quests::done(
        &h.server.players[&world].play.quests,
        QUEST
    ));
    assert_eq!(phase(&h, world), 0);
}

/// The damage countdown's result, a hotspot and its popup, comes only to a living player within the
/// hotspot's reach: one who stands there is shown the popup and holds the hotspot; one gone
/// elsewhere, or dead, gets neither.
/// Behaviour: server.effects.a-countdowns-hotspot-only-within-its-reach
#[test]
fn a_countdowns_hotspot_comes_only_to_a_living_player_within_its_reach() {
    let (mut h, b, _, world) = player();
    sturdy(&mut h, world);
    let name = |h: &Harness| {
        let p = &h.server.players[&world];
        yalain_world::player::world_name(&p.name, p.sex)
    };
    let from = h.bots[b].inbox.len();
    assert!(put_on(&mut h, world, DAMAGE, 1.0));
    wait(&mut h, 31.0);
    assert_eq!(
        shown(&h, b, from, DISPLAY_MESSAGE),
        [(POPUP_WORDS, Some(name(&h)))],
        "the popup, naming its player"
    );
    assert_eq!(records(&h, world), [HOTSPOT], "the hotspot held");
    let eid = h.server.players[&world].play.effects[0].eid;
    effects::remove(&mut h.server, &mut h.net, world, eid);
    // Elsewhere: another landblock.
    let home = h.server.players[&world].position;
    let from = h.bots[b].inbox.len();
    assert!(put_on(&mut h, world, DAMAGE, 1.0));
    h.server.players.get_mut(&world).expect("in").position.cell = home.cell ^ 0x0100_0000;
    wait(&mut h, 31.0);
    assert!(shown(&h, b, from, DISPLAY_MESSAGE).is_empty(), "elsewhere");
    assert!(records(&h, world).is_empty());
    // Dead where it stands.
    h.server.players.get_mut(&world).expect("in").position = home;
    assert!(put_on(&mut h, world, DAMAGE, 1.0));
    let p = h.server.players.get_mut(&world).expect("in");
    quals::set_int(&mut p.qualities, quality::HEALTH, 0);
    wait(&mut h, 31.0);
    assert!(shown(&h, b, from, DISPLAY_MESSAGE).is_empty(), "dead");
    assert!(records(&h, world).is_empty());
}

/// A hotspot harms its player while the player stands within its reach, and ends at its first
/// change that finds the player outside, harming nothing more.
/// Behaviour: server.effects.a-hotspot-harms-only-within-its-reach
#[test]
fn a_hotspot_harms_its_player_inside_and_ends_once_it_is_outside() {
    let (mut h, _, _, world) = player();
    sturdy(&mut h, world);
    let full = health(&h, world);
    assert!(put_on(&mut h, world, HOTSPOT, 1.0));
    wait(&mut h, 8.0);
    let hurt = health(&h, world);
    assert!(hurt < full, "harmed inside: {full} to {hurt}");
    assert_eq!(records(&h, world), [HOTSPOT]);
    let at = h.server.players[&world].position;
    let p = h.server.players.get_mut(&world).expect("in");
    p.position.origin[0] = at.origin[0] + 301.0;
    p.position.cell = at.cell;
    wait(&mut h, 8.0);
    assert!(records(&h, world).is_empty(), "ended outside");
    assert!(health(&h, world) >= hurt, "nothing more");
}

/// A text effect shows its text in its player's chat as its kind, or in a box of its own, naming
/// its player in a variable, as 2017's did.
/// Behaviour: server.effects.a-text-effect-shows-its-text
#[test]
fn a_text_effect_shows_in_chat_or_in_a_box_of_its_own_naming_its_player() {
    let (mut h, b, _, world) = player();
    let name = {
        let p = &h.server.players[&world];
        yalain_world::player::world_name(&p.name, p.sex)
    };
    let from = h.bots[b].inbox.len();
    assert!(put_on(&mut h, world, SNEEZE, 1.0));
    assert!(put_on(&mut h, world, POPUP, 1.0));
    h.run(2);
    assert_eq!(h.texts(b, from), [(8, SNEEZE_WORDS.0)], "in chat, its kind");
    assert_eq!(
        shown(&h, b, from, SHOW_TEXT),
        [(SNEEZE_WORDS, Some(name.clone()))]
    );
    assert_eq!(
        shown(&h, b, from, DISPLAY_MESSAGE),
        [(POPUP_WORDS, Some(name))],
        "in a box of its own"
    );
    assert!(records(&h, world).is_empty(), "no record");
    let shown_value = h
        .calls(b, from)
        .into_iter()
        .filter(|(f, _)| *f == DISPLAY_MESSAGE)
        .flat_map(|(_, v)| v)
        .any(|v| matches!(v, Value::Int(0)));
    assert!(shown_value, "the box's flag");
}
