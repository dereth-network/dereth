//! Logging off: at the request a trade under way is closed for both, as closed by the leaver
//! because it logs out; an attack under way ends at its next swing, its attacks failed as its
//! actions are locked, the attack state off and peace; in the leaving's last tick the fellowship
//! is left after the records that end at logout are taken off and before the leaver's own look.
//! Fixture: players of the test world beside one another.

use auberean_protocol::login::CharacterRef;
use auberean_protocol::{queue, ClientMessage, ServerMessage};
use auberean_script::args::Arg;
use auberean_script::native_object::{native_type, NativeObject};
use yalain_testkit::Harness;
use yalain_world::calls::Value;
use yalain_world::combat::{self, mode};
use yalain_world::world::log_off;
use yalain_world::{effects, fellowship, trade};

const ENTER_PORTAL_SPACE: u32 = 0x8212_0020;

fn beside(h: &mut Harness, who: u64, of: u64, dx: f32) {
    let at = h
        .server
        .players
        .values()
        .find(|p| p.character == of)
        .expect("in the world")
        .position;
    let p = h
        .server
        .players
        .values_mut()
        .find(|p| p.character == who)
        .expect("in the world");
    p.position = at;
    p.position.origin[0] += dx;
}

fn string_arg(s: &str) -> Arg {
    let mut b = yalain_world::graph::Builder::new();
    #[allow(clippy::cast_possible_truncation)]
    b.native(
        native_type::WP_STRING as u16,
        NativeObject::Text(s.encode_utf16().collect()),
    );
    Arg::Object(b.graph())
}

/// Ask to log off; returns the clock's time of the request.
fn ask_to_log_off(h: &mut Harness, b: usize, id: u64) -> f64 {
    h.bots[b].send(
        false,
        queue::LOGON,
        &ClientMessage::LogOff(CharacterRef { character: id }),
    );
    h.now.0
}

/// Step the clock until `seconds` after `since`.
fn until(h: &mut Harness, since: f64, seconds: f64) {
    while h.now.0 < since + seconds {
        h.step();
    }
}

/// What a client was sent since `from`, in the order sent.
fn sent(h: &Harness, b: usize, from: usize) -> Vec<ServerMessage> {
    let mut v: Vec<(u64, ServerMessage)> = h.bots[b].inbox[from..]
        .iter()
        .filter(|r| r.world)
        .filter_map(|r| Some((r.blob_id & 0xFFFF_FFFF, r.message()?)))
        .collect();
    v.sort_by_key(|(n, _)| *n);
    v.into_iter().map(|(_, m)| m).collect()
}

/// The trade closes a client was told since `from`: (code, by whom).
fn closes(h: &Harness, b: usize, from: usize) -> Vec<(u32, u64)> {
    h.calls(b, from)
        .into_iter()
        .filter(|(f, _)| *f == trade::function::CLOSED)
        .map(|(_, v)| {
            let code = v.iter().find_map(|x| match x {
                Value::Int(i) => Some(*i),
                _ => None,
            });
            let by = v.iter().find_map(|x| match x {
                Value::Long(l) => Some(*l),
                _ => None,
            });
            (code.unwrap_or(0), by.unwrap_or(0))
        })
        .collect()
}

/// A trade under way when one of its two asks to log off is closed for both at once, as closed by
/// the leaver because it logs out (2017: 12 of 12 leavers told so 0.14 to 0.59 s after the
/// request, the partner as well).
/// Behaviour: server.logging-off.closes-a-trade-at-the-request
#[test]
fn a_trade_under_way_is_closed_for_both_when_one_asks_to_log_off() {
    let mut h = Harness::new();
    let (a, aid) = h.player_in_world("one", "Trader");
    let (b, bid) = h.player_in_world("two", "Buyer");
    beside(&mut h, bid, aid, 1.0);
    h.ask(a, trade::function::OPEN, &[Arg::LongInt(bid)], 10);
    h.ask(b, trade::function::OPEN, &[Arg::LongInt(aid)], 10);
    assert!(!h.server.trades.list.is_empty(), "opened");
    let (fa, fb) = (h.bots[a].inbox.len(), h.bots[b].inbox.len());
    let asked = ask_to_log_off(&mut h, a, aid);
    until(&mut h, asked, 1.0);
    assert!(h.server.trades.list.is_empty());
    assert_eq!(closes(&h, a, fa), [(trade::error::LOGGED_OUT, aid)]);
    assert_eq!(closes(&h, b, fb), [(trade::error::LOGGED_OUT, aid)]);
    until(&mut h, asked, log_off::OUT_AFTER + 1.0);
    assert_eq!(closes(&h, b, fb).len(), 1, "told once");
}

/// An attack run under way when its player asks to log off ends at its next swing: its attacks
/// failed as its actions are locked (1029), the attack state off, peace (2017: 2 of 2 runs under
/// way at a request ended 0.5 and 1.8 s after it, the attack state off and peace).
/// Behaviour: server.logging-off.ends-an-attack
#[test]
fn an_attack_under_way_ends_when_its_player_asks_to_log_off() {
    let mut h = Harness::new();
    let (b, me) = h.player_in_world("one", "Fighter");
    let world = yalain_world::social::world_of(&h.server, me).expect("in the world");
    let asked = ask_to_log_off(&mut h, b, me);
    if let Some(p) = h.server.players.get_mut(&world) {
        p.attacking = Some(combat::Run {
            target: 1,
            next_swing: asked + 0.5,
            context: 0,
            mode_sent: true,
            walking: false,
            next_target: None,
        });
    }
    let from = h.bots[b].inbox.len();
    until(&mut h, asked, 1.0);
    assert!(h.server.players[&world].attacking.is_none());
    let told: Vec<String> = sent(&h, b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c)
                if c.function.0 == combat::function::BASIC_ATTACKS_FAILED =>
            {
                Some("failed".to_string())
            }
            ServerMessage::PlayerCall(c) if c.function.0 == combat::function::ATTACK_STATE => {
                Some("attack state".to_string())
            }
            ServerMessage::DoMode(e) if e.sender.id == me => Some(format!("mode {:X}", e.action.0)),
            _ => None,
        })
        .collect();
    assert_eq!(
        told,
        ["failed", "attack state", &format!("mode {:X}", mode::PEACE)]
    );
    assert!(h
        .calls(b, from)
        .iter()
        .any(|(f, v)| *f == combat::function::BASIC_ATTACKS_FAILED
            && v.contains(&Value::Int(combat::error::ACTIONS_LOCKED))));
}

/// The leaving's last tick: into portal space, the records that end at logout taken off, the
/// fellowship left (cleared and told), then the leaver's own look (2017: 30 of 30 leavers'
/// fellowships cleared after the records' removals and before the look).
/// Behaviour: server.logging-off.leaves-its-fellowship-before-its-look
#[test]
fn a_leaver_leaves_its_fellowship_after_its_records_and_before_its_look() {
    let mut h = Harness::new();
    let (a, aid) = h.player_in_world("one", "Leader");
    let (c, cid) = h.player_in_world("two", "Second");
    beside(&mut h, cid, aid, 0.5);
    h.ask(
        a,
        fellowship::function::CREATE,
        &[Arg::Int(1), Arg::Int(0), string_arg("Band")],
        10,
    );
    h.ask(a, fellowship::function::RECRUIT, &[Arg::LongInt(cid)], 10);
    h.ask(c, fellowship::function::ACCEPT, &[Arg::LongInt(aid)], 10);
    assert!(h.server.fellowships.of(aid).is_some(), "a fellowship");
    let world = yalain_world::social::world_of(&h.server, aid).expect("in the world");
    let clock = h.clock();
    let cast = effects::Cast {
        caster: aid,
        spellcraft: 1.0,
        ..effects::Cast::default()
    };
    effects::apply(
        &mut h.server,
        &mut h.net,
        world,
        yalain_testkit::COURAGE_EFFECT,
        cast,
        clock,
    );
    h.run(2);
    if let Some(e) = h
        .server
        .players
        .get_mut(&world)
        .and_then(|p| p.play.effects.first_mut())
    {
        e.flags |= effects::record_flag::LOGOUT;
    }
    let asked = ask_to_log_off(&mut h, a, aid);
    until(&mut h, asked, log_off::LEAVES_AFTER - 0.5);
    let from = h.bots[a].inbox.len();
    until(&mut h, asked, log_off::OUT_AFTER + 1.0);
    let order: Vec<&str> = sent(&h, a, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(x) => match x.function.0 {
                ENTER_PORTAL_SPACE => Some("portal"),
                effects::function::REMOVE => Some("remove"),
                fellowship::function::CLEAR_FELLOWSHIP => Some("fellowship"),
                _ => None,
            },
            ServerMessage::UpdateVisualDesc(e) if e.sender.id == aid => Some("look"),
            _ => None,
        })
        .collect();
    assert_eq!(order, ["portal", "remove", "fellowship", "look"]);
    assert!(h.server.fellowships.of(aid).is_none(), "left");
}
