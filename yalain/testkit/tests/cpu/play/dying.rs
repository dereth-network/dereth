//! A player's death: its tick, its corpse ten seconds later, the resurrection at its lifestone;
//! vitae.

use auberean_protocol::{queue, ServerMessage};
use yalain_store::Store;
use yalain_testkit::play::server_call;
use yalain_testkit::Harness;
use yalain_world::calls::{self, Value};
use yalain_world::death::{min_vitae, xp_to_raise};

const DEATH_MESSAGE: u32 = 0x8207_002B;
const WHO_KILLED: u32 = 0x8207_0030;
const DEATH_STATE: u32 = 0x8207_0033;
const RESURRECT: u32 = 0x8207_000E;
const CORPSE: u32 = 0x4700_0539;
const DEAD: u32 = 279;
const SELECTABLE: u32 = 624;
const FOCUS: u32 = 266;

/// Let `seconds` pass, the connections talking.
fn wait(h: &mut Harness, seconds: f64) {
    let t = h.now.0 + seconds;
    while h.now.0 < t {
        h.step();
    }
}

fn scene_drawn(h: &mut Harness, b: usize) {
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
}

fn calls_of(h: &Harness, b: usize, from: usize) -> Vec<(u32, Vec<Value>)> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c) => Some((
                c.function.0,
                c.args
                    .as_ref()
                    .and_then(|a| calls::values(&a.0).ok())
                    .unwrap_or_default(),
            )),
            _ => None,
        })
        .collect()
}

/// A player one hit from death beside a creature that attacks it.
fn about_to_die(h: &mut Harness) -> (usize, u64) {
    h.add_creatures(1);
    let (b, me) = h.player_in_world("one", "Elara");
    scene_drawn(h, b);
    for p in h.server.players.values_mut() {
        yalain_world::quals::set_int(&mut p.qualities, 258, 1);
    }
    (b, me)
}

/// Run until the player's death message arrives; the index of the first message after it.
fn run_to_death(h: &mut Harness, b: usize, from: usize) -> usize {
    let died = h.run_until(4000, |h| {
        calls_of(h, b, from)
            .iter()
            .any(|(f, _)| *f == DEATH_MESSAGE)
    });
    assert!(died, "the creature kills the player");
    h.bots[b].inbox.len()
}

/// A player killed by a creature: its death message (the killer's name), and for all who see it:
/// dead, its killer announced on it, unselectable, posed, playing its death; the creature gives
/// up. Nothing more until its corpse ten seconds later: its client told it is dead, and a corpse
/// of the corpse template made where it fell, its originator the player.
/// Behaviour: server.death.player-dies
#[test]
fn a_player_killed_by_a_creature_dies_and_its_corpse_is_made_ten_seconds_later() {
    let mut h = Harness::new();
    let (b, me) = about_to_die(&mut h);
    let from = h.bots[b].inbox.len();
    let after = run_to_death(&mut h, b, from);
    let msgs = h.world_messages(b, from);
    assert!(
        msgs.iter()
            .any(|m| matches!(m, ServerMessage::UpdateBoolVisual(q)
            if q.object.id == me && q.quality == DEAD && q.value == 1)),
        "dead"
    );
    assert!(
        msgs.iter()
            .any(|m| matches!(m, ServerMessage::VisualObjectCall(c)
            if c.object.id == me && c.call.function.0 == WHO_KILLED)),
        "its killer announced"
    );
    assert!(
        calls_of(&h, b, from).iter().all(|(f, _)| *f != DEATH_STATE),
        "not yet told it is dead"
    );
    wait(&mut h, 9.0);
    assert!(calls_of(&h, b, after)
        .iter()
        .all(|(f, _)| *f != DEATH_STATE));
    wait(&mut h, 1.5);
    let states: Vec<Vec<Value>> = calls_of(&h, b, after)
        .into_iter()
        .filter(|(f, _)| *f == DEATH_STATE)
        .map(|(_, v)| v)
        .collect();
    assert_eq!(
        states,
        vec![vec![Value::Int(1)]],
        "dead, as its client knows"
    );
    let corpse = h
        .world_messages(b, after)
        .into_iter()
        .find_map(|m| match m {
            ServerMessage::CreateObject(o) if o.weenie.entity_did == CORPSE => Some(o),
            _ => None,
        });
    let corpse = corpse.expect("its corpse is made");
    assert_eq!(corpse.weenie.objects[3], me, "the player's corpse");
}

/// A player killed by a creature is told its death tick in 2017's order, as the server sends it:
/// dead, wounded, its death message, its killer announced, unselectable, its pose, its death.
/// Behaviour: server.death.player-dies-dead-before-its-message
#[test]
fn a_players_death_tick_is_dead_wounded_message_killer_unselectable_pose_death() {
    let mut h = Harness::new();
    let (b, me) = about_to_die(&mut h);
    let from = h.bots[b].inbox.len();
    run_to_death(&mut h, b, from);
    let tick: Vec<&str> = h
        .sent_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::UpdateBoolVisual(q)
                if q.object.id == me && q.quality == DEAD && q.value == 1 =>
            {
                Some("dead")
            }
            ServerMessage::DoFx(e) if e.sender.id == me && e.action.fx == 0x4100_0002 => {
                Some("wounded")
            }
            ServerMessage::PlayerCall(c) if c.function.0 == DEATH_MESSAGE => Some("message"),
            ServerMessage::VisualObjectCall(c)
                if c.object.id == me && c.call.function.0 == WHO_KILLED =>
            {
                Some("killer")
            }
            ServerMessage::UpdateBoolVisual(q)
                if q.object.id == me && q.quality == SELECTABLE && q.value == 0 =>
            {
                Some("unselectable")
            }
            ServerMessage::MoveSlider(e) if e.sender.id == me => Some("pose"),
            ServerMessage::PlayBehavior(e) if e.sender.id == me => Some("death"),
            _ => None,
        })
        .collect();
    let at = tick.iter().position(|t| *t == "dead").expect("dead");
    assert_eq!(
        tick[at..at + 7],
        [
            "dead",
            "wounded",
            "message",
            "killer",
            "unselectable",
            "pose",
            "death"
        ]
    );
}

/// The player's own death tick as its client takes it, from its death message, in the order of
/// the visual stamps: unselectable, a movement order (its flags and stamp), the pose, the death
/// behavior.
fn death_tick(h: &Harness, b: usize, from: usize, me: u64) -> Vec<String> {
    let msgs = h.world_messages(b, from);
    let at = msgs
        .iter()
        .position(|m| matches!(m, ServerMessage::PlayerCall(c) if c.function.0 == DEATH_MESSAGE))
        .expect("its death message");
    let mut tick: Vec<(u16, String)> = Vec::new();
    for m in &msgs[at..] {
        match m {
            ServerMessage::UpdateBoolVisual(q) if q.object.id == me && q.quality == SELECTABLE => {
                tick.push((q.object.other_stamp, "unselectable".into()));
            }
            ServerMessage::MoveTo(e) if e.sender.id == me => tick.push((
                e.sender.other_stamp,
                format!(
                    "order {:#x} stamp {}",
                    e.action.params.flags, e.action.stamp
                ),
            )),
            ServerMessage::MoveSlider(e) if e.sender.id == me => {
                tick.push((e.sender.other_stamp, "pose".into()));
            }
            ServerMessage::PlayBehavior(e) if e.sender.id == me => {
                tick.push((e.sender.other_stamp, "death".into()));
                break;
            }
            _ => {}
        }
    }
    tick.sort_by_key(|(stamp, _)| *stamp);
    tick.into_iter().map(|(_, what)| what).collect()
}

/// A player closing in on the creature it attacks, killed by it, is stopped in its death tick: an
/// order to stop under the close-in's own stamp, after it turns unselectable and before its pose,
/// so its client's body stops running at the creature where it fell.
/// Behaviour: server.death.player-order-stopped
#[test]
fn a_player_killed_while_closing_in_on_a_creature_is_stopped_where_it_fell() {
    let mut h = Harness::new();
    let (b, me) = about_to_die(&mut h);
    h.arm(b, me, 0x4700_051C);
    let creature = h
        .server
        .things
        .placed()
        .find(|t| t.alive())
        .map(|t| t.id)
        .expect("a creature");
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(
            yalain_world::combat::function::START_ATTACK,
            &[auberean_script::args::Arg::LongInt(creature)],
        ),
    );
    run_to_death(&mut h, b, from);
    let close_in = h
        .world_messages(b, from)
        .into_iter()
        .rev()
        .find_map(|m| match m {
            ServerMessage::MoveTo(e) if e.sender.id == me && e.action.params.flags == 0x46B => {
                Some(e.action.stamp)
            }
            _ => None,
        })
        .expect("the run closes in on the creature");
    assert_eq!(
        death_tick(&h, b, from, me),
        vec![
            "unselectable".to_string(),
            format!("order 0x40 stamp {close_in}"),
            "pose".into(),
            "death".into(),
        ]
    );
}

/// A player standing still when it is killed is sent no movement order in its death tick.
/// Behaviour: server.death.player-order-stopped
#[test]
fn a_player_killed_standing_still_is_sent_no_stop() {
    let mut h = Harness::new();
    let (b, me) = about_to_die(&mut h);
    let from = h.bots[b].inbox.len();
    run_to_death(&mut h, b, from);
    assert_eq!(
        death_tick(&h, b, from, me),
        vec!["unselectable", "pose", "death"]
    );
}

/// What follows an object made for a client since `from`: each message about it, after its
/// creation, as (what, its visual stamp).
fn block_after(h: &Harness, b: usize, from: usize, id: u64) -> Vec<(String, u16)> {
    let msgs = h.world_messages(b, from);
    let Some(at) = msgs
        .iter()
        .position(|m| matches!(m, ServerMessage::CreateObject(o) if o.id == id))
    else {
        return Vec::new();
    };
    msgs[at + 1..]
        .iter()
        .filter_map(|m| match m {
            ServerMessage::UpdateFloatVisual(q) if q.object.id == id => {
                Some((format!("{} {}", q.quality, q.value), q.object.other_stamp))
            }
            ServerMessage::DoFx(e) if e.sender.id == id => {
                Some((format!("fx {:x}", e.action.fx), e.sender.other_stamp))
            }
            ServerMessage::MoveSlider(e) if e.sender.id == id => {
                Some(("pose".into(), e.sender.other_stamp))
            }
            ServerMessage::SetMode(e) if e.sender.id == id => {
                Some((format!("mode {:x}", e.action.0), e.sender.other_stamp))
            }
            ServerMessage::UpdateInstanceIdVisual(q) if q.object.id == id => {
                Some((format!("{} {:x}", q.quality, q.value), q.object.other_stamp))
            }
            ServerMessage::UpdateStringInfoVisual(q) if q.object.id == id => {
                Some((format!("{} name", q.quality), q.object.other_stamp))
            }
            _ => None,
        })
        .collect()
}

/// A player's corpse is followed, as a creature's is, by the changes it was made up of: its scale,
/// its two effects, its pose, its mode, nobody's claim, its killer, the player it was and its name,
/// their visual stamps 2 to 10.
/// Behaviour: server.death.player-corpse-settles
#[test]
fn a_players_corpse_is_followed_by_its_settling_as_a_creatures_is() {
    let mut h = Harness::new();
    let (b, me) = about_to_die(&mut h);
    let from = h.bots[b].inbox.len();
    let after = run_to_death(&mut h, b, from);
    wait(&mut h, 10.5);
    let corpse = h
        .world_messages(b, after)
        .into_iter()
        .find_map(|m| match m {
            ServerMessage::CreateObject(o) if o.weenie.entity_did == CORPSE => Some(o),
            _ => None,
        })
        .expect("its corpse is made");
    let killer = h
        .server
        .things
        .get(corpse.id)
        .and_then(|t| t.corpse.as_ref())
        .map(|c| c.killer)
        .expect("a corpse");
    let block: Vec<(String, u16)> = block_after(&h, b, after, corpse.id)
        .into_iter()
        .take(9)
        .map(|(w, n)| {
            (
                if w.starts_with("298") {
                    "298".into()
                } else {
                    w
                },
                n,
            )
        })
        .collect();
    assert_eq!(
        block,
        vec![
            ("298".into(), 2),
            ("fx 41000002".into(), 3),
            ("fx 41000004".into(), 4),
            ("pose".into(), 5),
            ("mode 41000003".into(), 6),
            ("322 0".into(), 7),
            (format!("320 {killer:x}"), 8),
            (format!("321 {me:x}"), 9),
            ("256 name".into(), 10),
        ]
    );
}

/// Step for `seconds`, noting the time each client's messages arrived at (`times`, per client, one
/// for each message of its inbox).
fn wait_noting(h: &mut Harness, seconds: f64, times: &mut [Vec<f64>]) {
    let t = h.now.0 + seconds;
    while h.now.0 < t {
        h.step();
        for (b, bot) in h.bots.iter().enumerate() {
            let have = times[b].len();
            times[b].resize(have.max(bot.inbox.len()), h.now.0);
        }
    }
}

/// A client's world messages with the times they arrived at.
fn timed(h: &Harness, b: usize, times: &[f64]) -> Vec<(f64, ServerMessage)> {
    h.bots[b]
        .inbox
        .iter()
        .zip(times)
        .filter(|(r, _)| r.world)
        .filter_map(|(r, t)| r.message().map(|m| (*t, m)))
        .collect()
}

/// A client watching a player die, and the player itself, take every message about it and its
/// corpse as the client takes them: on the player's own connection the corpse's long making (a
/// player's look) is overtaken by the corpse's settling, which waits for it; a minute later the
/// corpse is forgotten and then seen to leave, in one instant.
/// Behaviour: server.death.watched-player-death-in-order
#[test]
fn a_player_death_and_its_corpse_reach_a_watching_client_in_an_order_it_takes() {
    use auberean_protocol::table::Table;
    use auberean_protocol::types::PartGroupDesc;
    use yalain_testkit::invariants::Checker;

    let mut h = Harness::new();
    let (b, me) = h.player_in_world("one", "Elara");
    let (w, _) = h.player_in_world("two", "Watcher");
    scene_drawn(&mut h, b);
    scene_drawn(&mut h, w);
    let world = h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .map(|(c, _)| *c)
        .expect("the player");
    // Dressed as a player is: a part group for each piece worn.
    let groups = (1..=20).map(|k| {
        (
            k,
            PartGroupDesc {
                bits: PartGroupDesc::KEY | PartGroupDesc::SETUP | PartGroupDesc::ANIM_MAP,
                key: k,
                setup: 0x0200_0000 + k,
                anim_map: 0x2200_0000 + k,
                ..PartGroupDesc::default()
            },
        )
    });
    if let Some(p) = h.server.players.get_mut(&world) {
        p.look.bits |= auberean_protocol::types::VisualDesc::PART_GROUPS;
        p.look.part_groups = Table::from_inserted(2, groups);
    }
    let mut times: Vec<Vec<f64>> = h
        .bots
        .iter()
        .map(|bot| vec![0.0; bot.inbox.len()])
        .collect();
    wait_noting(&mut h, 1.0, &mut times);
    let clock = h.clock();
    yalain_world::death::player_dies(&mut h.server, &mut h.net, world, None, clock);
    wait_noting(&mut h, 11.0, &mut times);
    let corpse = h
        .world_messages(w, 0)
        .into_iter()
        .find_map(|m| match m {
            ServerMessage::CreateObject(o) if o.weenie.entity_did == CORPSE => Some(o.id),
            _ => None,
        })
        .expect("the watcher is told of the corpse");
    wait_noting(&mut h, 61.0, &mut times);
    let about = |m: &ServerMessage| yalain_testkit::invariants::object_named(m).map(|o| o.id);
    // The player's own client is told of its corpse behind what it was told as it died: there the
    // corpse's long making is overtaken by its settling.
    let own = timed(&h, b, &times[b]);
    let making = own
        .iter()
        .position(|(_, m)| matches!(m, ServerMessage::CreateObject(o) if o.id == corpse))
        .expect("made");
    let settling = own
        .iter()
        .position(|(_, m)| about(m) == Some(corpse))
        .expect("settled");
    assert!(settling < making, "the settling overtakes the long making");
    for c in [b, w] {
        let gone: Vec<(f64, &str)> = timed(&h, c, &times[c])
            .iter()
            .filter_map(|(t, m)| match m {
                ServerMessage::ForgetObject(o) if o.0.id == corpse => Some((*t, "forgotten")),
                ServerMessage::ObjectLeaves(e) if e.sender.id == corpse => Some((*t, "leaves")),
                _ => None,
            })
            .collect();
        assert_eq!(gone.len(), 2, "client {c}: {gone:?}");
        assert_eq!(gone[0].0, gone[1].0, "client {c}: in one instant");
        assert_eq!(
            (gone[0].1, gone[1].1),
            ("forgotten", "leaves"),
            "client {c}"
        );
    }
    for c in [b, w] {
        let mut check = Checker::new();
        for (t, m) in timed(&h, c, &times[c]) {
            check.see(t, &m);
        }
        check.finish();
        assert_eq!(
            check.breaches,
            std::collections::BTreeMap::new(),
            "client {c} takes every message"
        );
    }
}

/// A resurrected player is told of what stands where it comes back, its own corpse among it, before
/// it leaves portal space.
/// Behaviour: server.places.new-place-before-the-exit
#[test]
fn a_resurrected_player_is_told_of_the_new_places_objects_before_it_comes_out() {
    let mut h = Harness::new();
    let (b, _) = about_to_die(&mut h);
    let from = h.bots[b].inbox.len();
    let _ = run_to_death(&mut h, b, from);
    wait(&mut h, 11.0);
    let corpse = h
        .server
        .things
        .placed()
        .find(|t| t.template == CORPSE)
        .map(|t| t.id)
        .expect("its corpse");
    let from = h.bots[b].inbox.len();
    h.bots[b].send(true, queue::WEENIE, &server_call(RESURRECT, &[]));
    wait(&mut h, 2.0);
    let sent = h.sent_messages(b, from);
    let made = sent
        .iter()
        .position(|m| matches!(m, ServerMessage::CreateObject(o) if o.id == corpse));
    let exit = sent.iter().position(|m| {
        matches!(m, ServerMessage::PlayerCall(c) if c.function.0 == yalain_world::places::function::EXIT_PORTAL_SPACE)
    });
    assert!(made.is_some() && made < exit, "{made:?} {exit:?}");
}

/// Asked for once the corpse is made, the resurrection takes the player through portal space; a
/// second and a half later it is alive again with three quarters of its health.
/// Behaviour: server.death.resurrection
#[test]
fn a_dead_player_is_resurrected_with_three_quarters_of_its_health() {
    let mut h = Harness::new();
    let (b, me) = about_to_die(&mut h);
    let from = h.bots[b].inbox.len();
    let _ = run_to_death(&mut h, b, from);
    wait(&mut h, 11.0);
    let from = h.bots[b].inbox.len();
    h.bots[b].send(true, queue::WEENIE, &server_call(RESURRECT, &[]));
    wait(&mut h, 2.0);
    let states: Vec<Vec<Value>> = calls_of(&h, b, from)
        .into_iter()
        .filter(|(f, _)| *f == DEATH_STATE)
        .map(|(_, v)| v)
        .collect();
    assert_eq!(states, vec![vec![Value::Int(0)]], "alive again");
    let p = h.server.players.values().next().expect("the player");
    let max = yalain_world::quals::int(&p.qualities, 263).unwrap_or(100);
    let health = yalain_world::quals::int(&p.qualities, 258).unwrap_or(0);
    assert_eq!(health, max * 3 / 4, "three quarters of its health");
    assert!(p.dying.is_none());
    assert!(
        h.world_messages(b, from)
            .iter()
            .any(|m| matches!(m, ServerMessage::UpdateBoolVisual(q)
            if q.object.id == me && q.quality == DEAD && q.value == 0)),
        "seen alive"
    );
}

/// Under vitae, a resurrected player comes back with three quarters of the greatest the vitae it
/// is left with makes: at level 10 and 95 vitae, health of 975 and vigor of 1,420 come back as
/// 657 and 958 at 90 vitae (three quarters of 877 and 1,278).
/// Behaviour: server.regen.vitae-lowers-the-greatest
#[test]
fn a_player_under_vitae_comes_back_with_three_quarters_of_the_greatest_its_new_vitae_makes() {
    use yalain_world::death::vitae_q::VITAE;
    let mut h = Harness::new();
    let (b, _) = about_to_die(&mut h);
    for p in h.server.players.values_mut() {
        for (k, v) in [(263, 975), (264, 1420), (302, 10)] {
            yalain_world::quals::set_int(&mut p.qualities, k, v);
        }
        yalain_world::quals::set_float(&mut p.qualities, VITAE, 95.0);
    }
    // Held at 1 health until the beetle's blow lands: a greatest of 975 regenerates past what its
    // blows take when its first few swings miss.
    let from = h.bots[b].inbox.len();
    let died = (0..4000).any(|_| {
        for p in h.server.players.values_mut() {
            if yalain_world::quals::int(&p.qualities, 258) > Some(1) {
                yalain_world::quals::set_int(&mut p.qualities, 258, 1);
            }
        }
        h.step();
        calls_of(&h, b, from)
            .iter()
            .any(|(f, _)| *f == DEATH_MESSAGE)
    });
    assert!(died, "the creature kills the player");
    wait(&mut h, 11.0);
    h.bots[b].send(true, queue::WEENIE, &server_call(RESURRECT, &[]));
    wait(&mut h, 2.0);
    let p = h.server.players.values().next().expect("the player");
    let vitae = yalain_world::quals::float(&p.qualities, VITAE).expect("vitae");
    assert!((vitae - 90.0).abs() < 1e-4, "{vitae}");
    assert_eq!(
        yalain_world::quals::int(&p.qualities, 258),
        Some(657),
        "health"
    );
    assert_eq!(
        yalain_world::quals::int(&p.qualities, 259),
        Some(958),
        "vigor"
    );
}

/// Death Hardiness gives its chance that a death takes no vitae: at its top rank's 0.6, of 400
/// returns from death at 90 vitae about three in five keep the 90 and the rest come back at 85;
/// without it every one takes 5.
/// Behaviour: server.perks.death-hardiness
#[test]
fn death_hardiness_gives_its_chance_that_a_death_takes_no_vitae() {
    use yalain_world::death::vitae_q::VITAE;
    let mut h = Harness::new();
    let _ = about_to_die(&mut h);
    let world = *h.server.players.keys().next().expect("in the world");
    let back = |h: &mut Harness| {
        let p = h.server.players.get_mut(&world).expect("in");
        yalain_world::quals::set_int(&mut p.qualities, 302, 30);
        yalain_world::quals::set_float(&mut p.qualities, VITAE, 90.0);
        yalain_world::death::returns(&mut h.server, &mut h.net, world);
        yalain_world::quals::float(&h.server.players[&world].qualities, VITAE).expect("vitae")
    };
    assert!((back(&mut h) - 85.0).abs() < 1e-4, "no perk: five taken");
    h.server.skill_table.skills.insert(
        9_016,
        yalain_world::skills::SkillDef {
            id: 9_016,
            perk: Some(yalain_world::skills::Perk {
                kind: 16,
                value: 0.6,
                priority: 3,
            }),
            ..yalain_world::skills::SkillDef::default()
        },
    );
    h.server
        .players
        .get_mut(&world)
        .expect("in")
        .play
        .skills
        .push(yalain_world::state::SkillState {
            skill: 9_016,
            granted: 0,
            mask: 0x1,
            cost: 0,
            children: 0,
            dependents: 0,
            spent: 0,
        });
    let (mut kept, mut taken) = (0, 0);
    for _ in 0..400 {
        let v = back(&mut h);
        if (v - 90.0).abs() < 1e-4 {
            kept += 1;
        } else {
            assert!((v - 85.0).abs() < 1e-4, "{v}");
            taken += 1;
        }
    }
    assert!(
        (200..=280).contains(&kept) && taken > 0,
        "{kept} kept, {taken} taken"
    );
}

/// As it goes, a resurrected player is set standing at once, never played into it: the mode set
/// to peace, the resurrection's glow, then the arrival's effect as it goes into portal space.
/// Behaviour: server.death.resurrection-look
#[test]
fn a_resurrected_player_is_set_standing_then_glows_as_it_goes() {
    let mut h = Harness::new();
    let (b, me) = about_to_die(&mut h);
    let from = h.bots[b].inbox.len();
    let _ = run_to_death(&mut h, b, from);
    wait(&mut h, 11.0);
    let from = h.bots[b].inbox.len();
    h.bots[b].send(true, queue::WEENIE, &server_call(RESURRECT, &[]));
    h.run(3);
    let look: Vec<(&str, u32)> = h
        .world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::SetMode(e) if e.sender.id == me => Some(("set", e.action.0)),
            ServerMessage::DoMode(e) if e.sender.id == me => Some(("played", e.action.0)),
            ServerMessage::DoFx(e) if e.sender.id == me => Some(("effect", e.action.fx)),
            _ => None,
        })
        .collect();
    assert_eq!(
        look,
        vec![
            ("set", 0x4000_0001),
            ("effect", 0x4100_000F),
            ("effect", 0x4100_0010)
        ]
    );
}

/// The freezes and frees of the player's movement a client was sent since `from`: true for a
/// freeze.
fn freezes(h: &Harness, b: usize, from: usize) -> Vec<bool> {
    calls_of(h, b, from)
        .into_iter()
        .filter(|(f, _)| *f == yalain_world::session::function::MOVEMENT_FROZEN)
        .map(|(_, v)| v != vec![Value::Int(0)])
        .collect()
}

/// A client holds its player still for as long as it has been frozen more often than freed. Its
/// death froze it and so does its resurrection's trip, whose freeze the drawn scene lifts; about
/// three seconds after it is back at its lifestone the death's is lifted too, so a resurrected
/// player can walk again, and every freeze since the death has been lifted once.
/// Behaviour: server.death.moves-again-after-return
#[test]
fn a_resurrected_player_is_freed_of_its_deaths_freeze_three_seconds_after_its_return() {
    let mut h = Harness::new();
    let (b, _) = about_to_die(&mut h);
    let from = h.bots[b].inbox.len();
    let _ = run_to_death(&mut h, b, from);
    wait(&mut h, 11.0);
    assert_eq!(freezes(&h, b, from), vec![true], "frozen once dead");
    h.bots[b].send(true, queue::WEENIE, &server_call(RESURRECT, &[]));
    let out = h.run_until(200, |h| {
        calls_of(h, b, from)
            .iter()
            .any(|(f, _)| *f == yalain_world::places::function::EXIT_PORTAL_SPACE)
    });
    assert!(out, "back out of portal space");
    let exit = h.now.0;
    scene_drawn(&mut h, b);
    while h.now.0 < exit + 2.5 {
        h.step();
    }
    assert_eq!(
        freezes(&h, b, from),
        vec![true, true, false],
        "the trip's freeze lifted as the scene is drawn, the death's not yet"
    );
    while h.now.0 < exit + 3.1 {
        h.step();
    }
    assert_eq!(
        freezes(&h, b, from),
        vec![true, true, false, false],
        "the death's freeze lifted too"
    );
}

/// Back from a resurrection, a player with more than ten focus has ten: sent after its health
/// and vigor, before it is told it is alive; one with ten or less keeps it, and nothing is sent.
/// Behaviour: server.focus.resurrection
#[test]
fn a_resurrected_player_with_more_than_ten_focus_comes_back_with_ten() {
    for (had, want) in [(60, Some(10)), (8, None)] {
        let mut h = Harness::new();
        let (b, _) = about_to_die(&mut h);
        for p in h.server.players.values_mut() {
            yalain_world::quals::set_int(&mut p.qualities, FOCUS, had);
        }
        let from = h.bots[b].inbox.len();
        let _ = run_to_death(&mut h, b, from);
        wait(&mut h, 11.0);
        let from = h.bots[b].inbox.len();
        h.bots[b].send(true, queue::WEENIE, &server_call(RESURRECT, &[]));
        wait(&mut h, 2.0);
        let seq: Vec<(u32, i32)> = h
            .world_messages(b, from)
            .iter()
            .filter_map(|m| match m {
                ServerMessage::UpdateInt(x) if [258, 259, FOCUS].contains(&x.quality) => {
                    Some((x.quality, x.value))
                }
                ServerMessage::PlayerCall(c) if c.function.0 == DEATH_STATE => Some((0, 0)),
                _ => None,
            })
            .collect();
        let order: Vec<u32> = seq.iter().map(|(k, _)| *k).take(4).collect();
        let focus: Vec<i32> = seq
            .iter()
            .filter(|(k, _)| *k == FOCUS)
            .map(|(_, v)| *v)
            .collect();
        assert_eq!(focus, want.into_iter().collect::<Vec<_>>(), "had {had}");
        let mut expected = vec![258, 259];
        if want.is_some() {
            expected.push(FOCUS);
        }
        expected.push(0);
        assert_eq!(order, expected, "health, vigor, focus, alive (had {had})");
    }
}

/// Vitae: none lost below level 6; above, at least 100 less 3 a level, between 60 and 99; what is
/// still to earn to raise it by one is the level's cost times the vitae's share, rounded, at
/// least 1, none at full vitae.
/// Behaviour: server.death.vitae
#[test]
fn vitae_has_its_floor_by_level_and_its_cost_to_raise() {
    assert!((min_vitae(5) - 100.0).abs() < f32::EPSILON);
    assert!((min_vitae(6) - 85.0).abs() < f32::EPSILON);
    assert!((min_vitae(14) - 61.0).abs() < f32::EPSILON);
    assert!((min_vitae(40) - 60.0).abs() < f32::EPSILON);
    let costs = [1, 1, 1, 1, 1, 2, 3, 4, 5, 6, 7, 8, 11, 15];
    assert_eq!(xp_to_raise(&costs, 14, 95.0), Some(14));
    assert_eq!(xp_to_raise(&costs, 6, 95.0), Some(2));
    assert_eq!(xp_to_raise(&costs, 1, 95.0), Some(1));
    assert_eq!(xp_to_raise(&costs, 14, 100.0), None);
}

const ADD_EFFECT: u32 = 0x8208_0017;
const REMOVE_EFFECT: u32 = 0x8208_0016;
const SHOW_TEXT: u32 = yalain_world::session::function::SHOW_TEXT;
const PROTECTION: u32 = 0x7000_0973;
const NO_LONGER_PROTECTED: u32 = 0x847E_5E83;
const GLOW: u32 = 0x4100_000F;

/// Resurrected: the protection added (its id, and a record of the protection's saved state for
/// three minutes); returns the effect's id.
fn resurrected(h: &mut Harness) -> (usize, u64, u32, usize) {
    let (b, me) = about_to_die(h);
    let from = h.bots[b].inbox.len();
    let _ = run_to_death(h, b, from);
    wait(h, 11.0);
    let from = h.bots[b].inbox.len();
    h.bots[b].send(true, queue::WEENIE, &server_call(RESURRECT, &[]));
    wait(h, 2.0);
    // Back in the world, its scene drawn: in sight of the creature that killed it.
    scene_drawn(h, b);
    let added: Vec<Vec<Value>> = calls_of(h, b, from)
        .into_iter()
        .filter(|(f, _)| *f == ADD_EFFECT)
        .map(|(_, v)| v)
        .collect();
    assert_eq!(added.len(), 1, "one effect added");
    let v = &added[0];
    let Some(Value::Int(eid)) = v.get(1) else {
        panic!("the effect's id: {v:?}")
    };
    let Some(yalain_world::calls::Node::Script { package, slots }) = v[0].root() else {
        panic!("a record")
    };
    assert_eq!(*package, 0x14E, "an effect record");
    assert_eq!(slots.len(), 26);
    assert_eq!(
        slots[6],
        (0x03, 180.0f32.to_bits()),
        "lasting three minutes"
    );
    assert_eq!(slots[20], (0x02, 0x44), "the protection's flags");
    assert_eq!(slots[23], (0x02, *eid), "its id in the record");
    let state = v[0]
        .entry(slots[14].1)
        .map(|n| matches!(n, yalain_world::calls::Node::Singleton(_)));
    assert_eq!(state, Some(true), "the effect a saved state's singleton");
    let raw = h.bots[b].inbox[from..]
        .iter()
        .filter_map(|r| match r.message() {
            Some(ServerMessage::PlayerCall(c)) if c.function.0 == ADD_EFFECT => {
                c.args.as_ref().map(|a| a.0.clone())
            }
            _ => None,
        })
        .next()
        .expect("the call's bytes");
    assert!(
        raw.windows(4).any(|w| w == PROTECTION.to_le_bytes()),
        "the lifestone protection's saved state"
    );
    (b, me, *eid, h.bots[b].inbox.len())
}

fn ended(h: &Harness, b: usize, from: usize, eid: u32, me: u64) -> bool {
    let calls = calls_of(h, b, from);
    let text = calls.iter().any(|(f, v)| {
        *f == SHOW_TEXT
            && v.iter().any(|x| match x {
                Value::Graph(g) => g.iter().any(|(_, n)| {
                    matches!(n, yalain_world::calls::Node::TextInfo(t) if t.string_id == NO_LONGER_PROTECTED)
                }),
                _ => false,
            })
    });
    let removed = calls
        .iter()
        .any(|(f, v)| *f == REMOVE_EFFECT && v.first() == Some(&Value::Int(eid)));
    let glow_stopped = h
        .world_messages(b, from)
        .iter()
        .any(|m| matches!(m, ServerMessage::StopFx(e) if e.sender.id == me && e.action.0 == GLOW));
    text && removed && glow_stopped
}

/// A resurrected player is under lifestone protection for three minutes: the protection added
/// with its record as it goes; creatures leave it be; when the three minutes are up, "You are no
/// longer protected by the Lifestone's Magic.", the protection taken away, its glow stopped.
/// Behaviour: server.death.lifestone-protection
#[test]
fn a_resurrected_player_is_protected_for_three_minutes_and_creatures_leave_it_be() {
    let mut h = Harness::new();
    let (b, me, eid, from) = resurrected(&mut h);
    assert!(h.server.players.values().all(|p| p.protected.is_some()));
    wait(&mut h, 170.0);
    assert!(
        !ended(&h, b, from, eid, me),
        "still protected before the time"
    );
    let attacked = h
        .world_messages(b, from)
        .iter()
        .any(|m| matches!(m, ServerMessage::DoStory(e) if e.sender.id != me));
    assert!(!attacked, "no creature takes on a protected player");
    wait(&mut h, 15.0);
    assert!(ended(&h, b, from, eid, me), "ended after three minutes");
    assert!(h.server.players.values().all(|p| p.protected.is_none()));
}

/// Attacking ends a lifestone protection at once: the text, the protection taken away, the glow
/// stopped.
/// Behaviour: server.death.lifestone-protection
#[test]
fn attacking_ends_a_lifestone_protection() {
    let mut h = Harness::new();
    let (b, me, eid, from) = resurrected(&mut h);
    let from = from.max(h.bots[b].inbox.len());
    h.arm(b, me, 0x4700_051C);
    let creature = h
        .server
        .things
        .placed()
        .find(|t| t.alive())
        .map(|t| t.id)
        .expect("a creature");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(
            yalain_world::combat::function::START_ATTACK,
            &[auberean_script::args::Arg::LongInt(creature)],
        ),
    );
    wait(&mut h, 1.0);
    assert!(ended(&h, b, from, eid, me), "ended by the attack");
}

const HEALTH: u32 = 258;
const VITAE: u32 = 286;
const LEVEL: u32 = 302;
/// More steps than a leaving takes.
const STEPS_TO_LEAVE: usize = 2000;

/// The bound lifestone of the tests that return to one.
fn a_lifestone() -> yalain_store::StoredPosition {
    yalain_store::StoredPosition {
        cell: 0x0102_0003,
        origin: [12.5, 34.5, 2.0],
        rotation: [1.0, 0.0, 0.0, 0.0],
    }
}

/// A player of level 30 (so a resurrection costs it vitae), bound to [`a_lifestone`], dead with its
/// corpse made and its box up: where a client may ask to be resurrected and has not.
fn dead_with_its_box_up(h: &mut Harness) -> (usize, u64) {
    let (b, me) = about_to_die(h);
    for p in h.server.players.values_mut() {
        yalain_world::quals::set_int(&mut p.qualities, LEVEL, 30);
        p.places
            .push((yalain_world::places::kind::LIFESTONE, a_lifestone()));
    }
    let from = h.bots[b].inbox.len();
    let _ = run_to_death(h, b, from);
    wait(h, 11.0);
    assert!(
        calls_of(h, b, from).contains(&(DEATH_STATE, vec![Value::Int(1)])),
        "its box is up"
    );
    (b, me)
}

fn log_off(h: &mut Harness, b: usize, id: u64) {
    let before = h.bots[b].inbox.len();
    h.bots[b].send(
        false,
        queue::LOGON,
        &auberean_protocol::ClientMessage::LogOff(auberean_protocol::login::CharacterRef {
            character: id,
        }),
    );
    assert!(h.run_until(STEPS_TO_LEAVE, |h| h.bots[b].inbox[before..]
        .iter()
        .any(|r| matches!(r.message(), Some(ServerMessage::CharacterList(_))))));
}

/// What an entry since `from` has to show of a character that left the world dead, once it has
/// come back: told it enters at no health, never told it is dead, nor of a death, nor to ask; then
/// its health and vigor, alive, standing and glowing where it entered, the place it returns to,
/// its vitae five less and three quarters of its health, protected, its actions no longer refused.
fn came_back_alive(h: &mut Harness, b: usize, me: u64, from: usize) {
    let messages = h.world_messages(b, from);
    let described = messages.iter().find_map(|m| match m {
        ServerMessage::PlayerQualities(q) => q.0.ints.get(HEALTH).copied(),
        _ => None,
    });
    assert_eq!(described, Some(0), "it enters at no health");
    wait(h, 2.0);
    let calls = calls_of(h, b, from);
    assert!(
        !calls.iter().any(|(f, _)| *f == DEATH_MESSAGE),
        "no death told"
    );
    let states: Vec<&Vec<Value>> = calls
        .iter()
        .filter(|(f, _)| *f == DEATH_STATE)
        .map(|(_, v)| v)
        .collect();
    assert_eq!(states, vec![&vec![Value::Int(0)]], "alive, never dead");
    assert_eq!(
        calls.iter().filter(|(f, _)| *f == ADD_EFFECT).count(),
        1,
        "protected"
    );
    let messages = h.world_messages(b, from);
    let look: Vec<(&str, u32)> = messages
        .iter()
        .filter_map(|m| match m {
            ServerMessage::SetMode(e) if e.sender.id == me => Some(("set", e.action.0)),
            ServerMessage::DoFx(e) if e.sender.id == me && e.action.fx == GLOW => {
                Some(("effect", e.action.fx))
            }
            ServerMessage::UpdateBoolVisual(q) if q.object.id == me && q.quality == DEAD => {
                Some(("dead", u32::from(q.value != 0)))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        look,
        vec![("set", 0x4000_0001), ("effect", GLOW), ("dead", 0)],
        "set standing, glowing, alive"
    );
    let p = h
        .server
        .players
        .values()
        .find(|p| p.character == me)
        .expect("in the world");
    assert!(p.trip.is_none(), "not through portal space");
    assert!(p.dying.is_none(), "no longer dying");
    assert_eq!(p.position.cell, a_lifestone().cell, "at its lifestone");
    assert_eq!(p.position.origin, a_lifestone().origin);
    let max = yalain_world::quals::int(&p.qualities, 263).unwrap_or(100);
    assert_eq!(
        yalain_world::quals::float(&p.qualities, VITAE),
        Some(95.0),
        "five vitae less"
    );
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    let back = ((max as f32 * 0.95).floor() * 0.75) as i32;
    assert_eq!(
        yalain_world::quals::int(&p.qualities, HEALTH),
        Some(back),
        "three quarters of its health at that vitae"
    );
    let kept = h
        .server
        .store
        .world_state(me)
        .expect("reads")
        .expect("kept");
    assert_eq!(
        yalain_world::quals::int(&kept.qualities, HEALTH),
        Some(back),
        "kept as it comes back, so a stop now does not take its vitae again"
    );
    let world = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .expect("in the world")
        .0;
    assert!(
        !yalain_world::places::actions_locked(&h.server, world),
        "its actions are no longer refused"
    );
}

/// A character that logs off dead, its box up, comes back alive by itself: at its lifestone, the
/// entry at no health, a moment later its vitae five less, three quarters of its health and vigor,
/// protected and playing its return, and no box and nothing to ask for.
/// Behaviour: server.death.back-at-entry
#[test]
fn a_character_that_logged_off_dead_comes_back_alive_at_its_lifestone() {
    let mut h = Harness::new();
    let (b, me) = dead_with_its_box_up(&mut h);
    log_off(&mut h, b, me);
    let from = h.bots[b].inbox.len();
    h.enter_world(b, "one", me);
    came_back_alive(&mut h, b, me, from);
}

/// A character whose client went away while it was dead comes back as one that logged off dead.
/// Behaviour: server.death.back-at-entry
#[test]
fn a_character_whose_client_went_away_dead_comes_back_alive_at_its_lifestone() {
    let mut h = Harness::new();
    let (b, me) = dead_with_its_box_up(&mut h);
    h.bots[b].leave();
    assert!(
        h.run_until(STEPS_TO_LEAVE, |h| h.server.players.is_empty()),
        "it lingers and leaves"
    );
    let again = h.at_character_list("one");
    let from = h.bots[again].inbox.len();
    h.enter_world(again, "one", me);
    came_back_alive(&mut h, again, me, from);
}

/// A character in the world dead as the server stops comes back as one that logged off dead when
/// the server is up again.
/// Behaviour: server.death.back-at-entry
#[test]
fn a_character_dead_at_a_server_restart_comes_back_alive_at_its_lifestone() {
    let mut h = Harness::new();
    let (_, me) = dead_with_its_box_up(&mut h);
    assert_eq!(
        yalain_world::shutdown::save_everyone(&mut h.server, &mut h.net),
        1
    );
    let store = std::mem::take(&mut h.server.store);
    let mut h = Harness::with(
        yalain_world::ServerConfig::default(),
        store,
        yalain_testkit::test_rules(),
    );
    let b = h.at_character_list("one");
    let from = h.bots[b].inbox.len();
    h.enter_world(b, "one", me);
    came_back_alive(&mut h, b, me, from);
}

/// A character kept dead by an older server, its place and everything else as it left them, needs
/// nothing done to what is kept: its next entry brings it back.
/// Behaviour: server.death.back-at-entry
#[test]
fn a_character_already_kept_dead_comes_out_of_it_at_its_next_entry() {
    let mut h = Harness::new();
    let (b, me) = h.player_in_world("one", "Elara");
    scene_drawn(&mut h, b);
    for p in h.server.players.values_mut() {
        yalain_world::quals::set_int(&mut p.qualities, LEVEL, 30);
        p.places
            .push((yalain_world::places::kind::LIFESTONE, a_lifestone()));
    }
    log_off(&mut h, b, me);
    let mut kept = h
        .server
        .store
        .world_state(me)
        .expect("reads")
        .expect("kept");
    yalain_world::quals::set_int(&mut kept.qualities, HEALTH, 0);
    h.server.store.set_world_state(me, &kept).expect("kept");
    let from = h.bots[b].inbox.len();
    h.enter_world(b, "one", me);
    came_back_alive(&mut h, b, me, from);
}

/// A character that was never bound comes back beside the nearest lifestone, as a resurrection
/// would take it: where it fell when the world has none near.
/// Behaviour: server.death.back-at-entry
#[test]
fn a_character_never_bound_comes_back_where_it_fell_with_no_lifestone_near() {
    let mut h = Harness::new();
    let (b, me) = about_to_die(&mut h);
    let from = h.bots[b].inbox.len();
    let _ = run_to_death(&mut h, b, from);
    let fell = h
        .server
        .players
        .values()
        .next()
        .expect("the player")
        .position;
    wait(&mut h, 11.0);
    log_off(&mut h, b, me);
    h.enter_world(b, "one", me);
    wait(&mut h, 2.0);
    let p = h.server.players.values().next().expect("the player");
    assert!(p.dying.is_none(), "alive");
    assert_eq!(p.position.cell, fell.cell);
    assert_eq!(p.position.origin, fell.origin);
    assert!(yalain_world::quals::int(&p.qualities, HEALTH).unwrap_or(0) > 0);
}

/// A lifestone of the world's, with the plain lifestone package, at a place.
fn stone_at(cell: u32, origin: [f32; 3]) -> yalain_content::ContentObject {
    yalain_content::ContentObject {
        object: Some(0x4002_9900_0000_0057),
        template: 0x4700_113A,
        description: yalain_content::Description {
            name_text: None,
            package: 0x1A00,
            look: 0x1F00_0ED5,
            enter_fx: 0,
            scale: None,
            name: None,
            numbers: [0; 6],
            source: "authored for a test".into(),
        },
        cell,
        origin,
        rotation: [1.0, 0.0, 0.0, 0.0],
        kind: yalain_content::Kind::Static,
        label: String::new(),
        placed: "authored for a test".into(),
        destination: None,
        facts: yalain_content::Facts::default(),
        walks: None,
        gate: None,
    }
}

/// A player never bound comes back beside the nearest lifestone on the ground there, not at the
/// height it fell from: outdoors at the land's height where it lands, even from a landblock beside
/// it on a hill; inside at the lifestone's own height.
/// Behaviour: server.death.unbound-return-on-the-ground
#[test]
fn a_player_never_bound_comes_back_on_the_ground_beside_the_nearest_lifestone() {
    let mut h = Harness::new();
    h.server.lifestone_packages.insert(0x1A00);
    // The ground rises a meter for every five east.
    h.server.ground = Some(Box::new(yalain_testkit::Slope));
    let outside = stone_at(0x3C73_0017, [57.63, 120.51, 145.22]);
    h.server.content.objects = vec![outside];
    let fell = yalain_store::StoredPosition {
        cell: 0x3B74_0006,
        origin: [19.29, 116.45, 154.43],
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    let at = yalain_world::places::resurrection_place_of(&h.server, &[], fell);
    // South of it, from another landblock: in the land cell below the stone's.
    assert_eq!(at.cell, 0x3C73_0016, "beside the lifestone");
    assert!(
        (at.origin[2] - at.origin[0] * 0.2).abs() < 1e-4,
        "on the land where it lands: {:?}",
        at.origin
    );

    let inside = stone_at(0x0299_0109, [60.0, 241.0, -44.6]);
    h.server.content.objects = vec![inside];
    let fell = yalain_store::StoredPosition {
        cell: 0x0299_0109,
        origin: [50.0, 241.0, -30.0],
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    let at = yalain_world::places::resurrection_place_of(&h.server, &[], fell);
    assert!(
        (at.origin[2] - -44.6).abs() < 1e-4,
        "inside, at the lifestone's height: {:?}",
        at.origin
    );
}

/// A player never bound who fell inside, below a lifestone on the land of the same landblock, comes
/// back in the land cell its place beside the stone lies in: not the cell it fell in, nor the
/// stone's own when the place lies across a land cell's edge from it.
/// Behaviour: server.places.beside-lies-in-its-own-cell
#[test]
fn a_return_beside_an_outdoor_lifestone_lies_in_the_land_cell_under_it_from_a_fall_inside() {
    let mut h = Harness::new();
    h.server.lifestone_packages.insert(0x1A00);
    h.server.ground = Some(Box::new(yalain_testkit::Slope));
    // In the land cell from 40 to 60 m east, a meter from its west edge.
    h.server.content.objects = vec![stone_at(0x3C73_0017, [41.0, 120.5, 8.2])];
    let fell = yalain_store::StoredPosition {
        cell: 0x3C73_0105,
        origin: [30.0, 120.5, -20.0],
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    let at = yalain_world::places::resurrection_place_of(&h.server, &[], fell);
    assert_eq!(
        at.cell, 0x3C73_000F,
        "the land cell from 20 to 40 m east: {at:?}"
    );
    assert!(
        (at.origin[0] - 36.5).abs() < 1e-4 && (at.origin[1] - 120.5).abs() < 1e-4,
        "toward where it fell: {at:?}"
    );
    assert!(
        (at.origin[2] - 36.5 * 0.2).abs() < 1e-4,
        "on the land: {at:?}"
    );
}

/// A character alive at its entry is not brought back: nothing of a return is sent.
/// Behaviour: server.death.back-at-entry
#[test]
fn a_character_alive_at_its_entry_is_not_brought_back() {
    let mut h = Harness::new();
    let (b, me) = h.player_in_world("one", "Elara");
    scene_drawn(&mut h, b);
    log_off(&mut h, b, me);
    let from = h.bots[b].inbox.len();
    h.enter_world(b, "one", me);
    wait(&mut h, 2.0);
    assert!(
        !calls_of(&h, b, from).iter().any(|(f, _)| *f == DEATH_STATE),
        "no death state"
    );
    assert!(
        !calls_of(&h, b, from).iter().any(|(f, _)| *f == ADD_EFFECT),
        "not protected"
    );
}

/// A creature's blow: its story first, and only as the blow reaches the player, most of a second
/// later, its health at none and the kill text, in that order, so the client has written the hit
/// before it writes the kill.
/// Behaviour: server.death.blow-before-kill-text
#[test]
fn a_creatures_killing_blow_is_told_before_the_health_and_the_kill_text() {
    let mut h = Harness::new();
    let (b, me) = about_to_die(&mut h);
    // A missed swing does not hurt it, so a run of misses would leave it regaining health at peace
    // and its first blow short of a kill: it regains none, and the first blow to land kills it.
    for p in h.server.players.values_mut() {
        yalain_world::quals::set_float(
            &mut p.qualities,
            yalain_world::regen::rate::PEACE_HEALTH,
            0.0,
        );
    }
    let from = h.bots[b].inbox.len();
    let told = |m: &ServerMessage| {
        matches!(m, ServerMessage::DoStory(e) if e.action.hooks.iter().any(|k| k.target == me
            && k.bits == yalain_world::combat::hook::HIT))
    };
    assert!(
        h.run_until(4000, |h| h.world_messages(b, from).iter().any(told)),
        "the creature lands a blow"
    );
    let struck = h.now.0;
    let at_none = |m: &ServerMessage| matches!(m, ServerMessage::UpdateInt(q) if q.quality == HEALTH && q.value == 0);
    assert!(
        !h.world_messages(b, from).iter().any(at_none),
        "not hurt as it is told"
    );
    assert!(
        h.run_until(4000, |h| h.world_messages(b, from).iter().any(at_none)),
        "the blow reaches it"
    );
    let gap = h.now.0 - struck;
    assert!(
        (0.4..1.1).contains(&gap),
        "the blow reached it {gap:.2} s after its story"
    );
    let _ = run_to_death(&mut h, b, from);
    let messages = h.world_messages(b, from);
    let at = |pick: &dyn Fn(&ServerMessage) -> bool| messages.iter().position(pick);
    let kill_text =
        at(&|m| matches!(m, ServerMessage::PlayerCall(c) if c.function.0 == DEATH_MESSAGE));
    assert!(at(&told) < at(&at_none), "the story before the health");
    assert!(at(&at_none) < kill_text, "the health before the kill text");
}

/// A blow told at a player who has since gone into portal space does not reach it.
/// Behaviour: server.death.blow-before-kill-text
#[test]
fn a_blow_at_a_player_gone_into_portal_space_does_not_hurt_it() {
    let mut h = Harness::new();
    let (b, me) = h.player_in_world("one", "Elara");
    scene_drawn(&mut h, b);
    let world = *h.server.players.keys().next().expect("the player");
    let clock = h.clock();
    let to = a_lifestone();
    yalain_world::places::resurrection_trip(&mut h.server, &mut h.net, world, to, clock);
    let before = yalain_world::quals::int(&h.server.players[&world].qualities, HEALTH);
    yalain_world::combat::creature_strike_lands(
        &mut h.server,
        &mut h.net,
        me,
        world,
        yalain_world::effects::Harm::plain(5),
        clock,
    );
    assert_eq!(
        yalain_world::quals::int(&h.server.players[&world].qualities, HEALTH),
        before
    );
}

/// A second blow told before the first killed the player does nothing when it reaches it: the
/// player does not die again.
/// Behaviour: server.death.blow-before-kill-text
#[test]
fn a_blow_at_a_player_already_dead_does_nothing() {
    let mut h = Harness::new();
    let (b, me) = h.player_in_world("one", "Elara");
    scene_drawn(&mut h, b);
    let world = *h.server.players.keys().next().expect("the player");
    for p in h.server.players.values_mut() {
        yalain_world::quals::set_int(&mut p.qualities, HEALTH, 0);
    }
    let clock = h.clock();
    yalain_world::combat::creature_strike_lands(
        &mut h.server,
        &mut h.net,
        me,
        world,
        yalain_world::effects::Harm::plain(5),
        clock,
    );
    assert!(h.server.players[&world].dying.is_none(), "no second death");
}

/// A player alive and in the world is hurt by a blow that reaches it: the same call's other side.
/// Behaviour: server.death.blow-before-kill-text
#[test]
fn a_blow_that_reaches_a_player_hurts_it_by_its_damage() {
    let mut h = Harness::new();
    let (b, me) = h.player_in_world("one", "Elara");
    scene_drawn(&mut h, b);
    let world = *h.server.players.keys().next().expect("the player");
    let clock = h.clock();
    let before = yalain_world::quals::int(&h.server.players[&world].qualities, HEALTH).unwrap_or(0);
    yalain_world::combat::creature_strike_lands(
        &mut h.server,
        &mut h.net,
        me,
        world,
        yalain_world::effects::Harm::plain(5),
        clock,
    );
    assert_eq!(
        yalain_world::quals::int(&h.server.players[&world].qualities, HEALTH),
        Some(before - 5)
    );
}

/// A record of an effect on a player, by hand.
fn record_on(h: &mut Harness, state: u32, flags: u32, timeout: f32) -> u32 {
    let cast = h.net.game_time(h.now);
    let p = h.server.players.values_mut().next().expect("the player");
    let eid = yalain_world::effects::next_eid(p);
    p.play.effects.push(yalain_world::state::EffectState {
        eid,
        state,
        cast,
        timeout,
        caster: 0,
        spellcraft: 10.0,
        pk: false,
        acting: 0,
        skill: 0,
        item: 0,
        flags,
        categories: 1,
    });
    eid
}

fn records_held(h: &Harness) -> Vec<u32> {
    let p = h.server.players.values().next().expect("the player");
    p.play.effects.iter().map(|e| e.eid).collect()
}

fn word(v: &Value) -> u32 {
    let Value::Int(i) = v else {
        panic!("a word: {v:?}")
    };
    *i
}

/// As a resurrection is asked for, the records whose effects a death ends are taken off, and only
/// then is the protection added; a worn item's record and one a death does not end stay. For all
/// who see it, it is set standing, what its death ended stops showing, and it glows.
/// Behaviour: server.death.resurrection-ends-effects
#[test]
fn a_resurrection_takes_off_what_a_death_ends_and_keeps_the_rest() {
    use yalain_testkit::{COURAGE_EFFECT, DASH_EFFECT, EASE_SUFFERING_EFFECT};
    use yalain_world::effects::{function as effect_fn, TIMED, WORN};
    let mut h = Harness::new();
    let (b, me) = about_to_die(&mut h);
    let dash = record_on(&mut h, DASH_EFFECT, TIMED, 3000.0);
    let worn = record_on(&mut h, EASE_SUFFERING_EFFECT, WORN, f32::MAX);
    let courage = record_on(&mut h, COURAGE_EFFECT, TIMED, 3000.0);
    let from = h.bots[b].inbox.len();
    let _ = run_to_death(&mut h, b, from);
    wait(&mut h, 11.0);
    assert_eq!(
        records_held(&h),
        vec![dash, worn, courage],
        "a death itself takes nothing off"
    );
    let from = h.bots[b].inbox.len();
    h.bots[b].send(true, queue::WEENIE, &server_call(RESURRECT, &[]));
    h.run(3);
    let told: Vec<(u32, Vec<Value>)> = calls_of(&h, b, from)
        .into_iter()
        .filter(|(f, _)| *f == effect_fn::REMOVE || *f == effect_fn::ADD)
        .collect();
    assert_eq!(told.len(), 2, "one record off, the protection on: {told:?}");
    assert_eq!(told[0].0, effect_fn::REMOVE);
    assert_eq!(word(&told[0].1[0]), dash);
    assert_eq!(told[1].0, effect_fn::ADD);
    assert_eq!(records_held(&h), vec![worn, courage]);
    let look: Vec<(&str, u32)> = h
        .world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::SetMode(e) if e.sender.id == me => Some(("set", e.action.0)),
            ServerMessage::StopFx(e) if e.sender.id == me => Some(("stop", e.action.0)),
            ServerMessage::DoFx(e) if e.sender.id == me => Some(("effect", e.action.fx)),
            _ => None,
        })
        .collect();
    assert_eq!(
        look,
        vec![
            ("set", 0x4000_0001),
            ("stop", 0x4100_04CB),
            ("effect", 0x4100_000F),
            ("effect", 0x4100_0010)
        ]
    );
}

/// A resurrection asked for sends, in order: standing at peace, each record a death ends taken off
/// with its appearance stopped right after it, the protection's glow, the protection's add, then
/// the arrival's effect as it goes into portal space.
/// Behaviour: server.death.resurrection-order
#[test]
fn a_resurrection_stands_then_stops_each_appearance_with_its_record_then_glows_and_is_protected() {
    use yalain_testkit::DASH_EFFECT;
    use yalain_world::effects::{function as effect_fn, TIMED};
    let mut h = Harness::new();
    let (b, me) = about_to_die(&mut h);
    record_on(&mut h, DASH_EFFECT, TIMED, 3000.0);
    let from = h.bots[b].inbox.len();
    let _ = run_to_death(&mut h, b, from);
    wait(&mut h, 11.0);
    let from = h.bots[b].inbox.len();
    h.bots[b].send(true, queue::WEENIE, &server_call(RESURRECT, &[]));
    h.run(3);
    let order: Vec<String> = h
        .sent_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::SetMode(e) if e.sender.id == me => Some("set".to_string()),
            ServerMessage::StopFx(e) if e.sender.id == me => Some(format!("stop {:x}", e.action.0)),
            ServerMessage::DoFx(e) if e.sender.id == me => Some(format!("fx {:x}", e.action.fx)),
            ServerMessage::PlayerCall(c) if c.function.0 == effect_fn::REMOVE => {
                Some("remove".to_string())
            }
            ServerMessage::PlayerCall(c) if c.function.0 == effect_fn::ADD => {
                Some("add".to_string())
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        order,
        [
            "set",
            "remove",
            "stop 410004cb",
            "fx 4100000f",
            "add",
            "fx 41000010"
        ],
        "{order:?}"
    );
}

/// A character that left the world dead loses what its death ends as it comes back, before its
/// protection, as one resurrected by asking does.
/// Behaviour: server.death.resurrection-ends-effects
#[test]
fn a_character_coming_back_at_its_entry_loses_what_a_death_ends() {
    use yalain_testkit::{COURAGE_EFFECT, DASH_EFFECT};
    use yalain_world::effects::{function as effect_fn, TIMED};
    let mut h = Harness::new();
    let (b, me) = dead_with_its_box_up(&mut h);
    let dash = record_on(&mut h, DASH_EFFECT, TIMED, 3000.0);
    let courage = record_on(&mut h, COURAGE_EFFECT, TIMED, 3000.0);
    log_off(&mut h, b, me);
    let from = h.bots[b].inbox.len();
    h.enter_world(b, "one", me);
    wait(&mut h, 2.0);
    let told: Vec<u32> = calls_of(&h, b, from)
        .into_iter()
        .filter(|(f, _)| *f == effect_fn::REMOVE || *f == effect_fn::ADD)
        .map(|(f, _)| f)
        .collect();
    assert_eq!(told, vec![effect_fn::REMOVE, effect_fn::ADD]);
    assert_eq!(records_held(&h), vec![courage], "not {dash}");
}

/// A player that sits dead with its box up and asks for nothing is sent back ten minutes after it
/// was told it was dead, as if it had asked: not before, then alive at its lifestone, protected.
/// Behaviour: server.death.sent-back-after-ten-minutes
#[test]
fn a_player_left_dead_ten_minutes_is_sent_back() {
    let mut h = Harness::new();
    let (b, _) = dead_with_its_box_up(&mut h);
    let from = h.bots[b].inbox.len();
    wait(&mut h, 595.0);
    assert!(
        h.server.players.values().all(|p| p.dying.is_some()),
        "still dead"
    );
    assert!(calls_of(&h, b, from).iter().all(|(f, _)| *f != DEATH_STATE));
    wait(&mut h, 8.0);
    let calls = calls_of(&h, b, from);
    assert_eq!(
        calls
            .iter()
            .filter(|(f, _)| *f == DEATH_STATE)
            .map(|(_, v)| v.clone())
            .collect::<Vec<_>>(),
        vec![vec![Value::Int(0)]],
        "alive again"
    );
    assert_eq!(
        calls.iter().filter(|(f, _)| *f == ADD_EFFECT).count(),
        1,
        "protected"
    );
    let p = h.server.players.values().next().expect("the player");
    assert!(p.dying.is_none());
    assert_eq!(p.position.cell, a_lifestone().cell, "at its lifestone");
}

/// A player that asked and died again is not sent back by its first death's ten minutes: the second
/// death has its own.
/// Behaviour: server.death.sent-back-after-ten-minutes
#[test]
fn a_second_death_is_not_cut_short_by_the_firsts_ten_minutes() {
    let mut h = Harness::new();
    let (b, _) = dead_with_its_box_up(&mut h);
    let world = *h.server.players.keys().next().expect("the player");
    let first = h.server.players[&world]
        .dying
        .and_then(|d| d.corpse)
        .expect("its corpse");
    h.bots[b].send(true, queue::WEENIE, &server_call(RESURRECT, &[]));
    wait(&mut h, 2.0);
    assert!(h.server.players[&world].dying.is_none());
    let clock = h.clock();
    yalain_world::death::player_dies(&mut h.server, &mut h.net, world, None, clock);
    wait(&mut h, 11.0);
    let second = h.server.players[&world]
        .dying
        .and_then(|d| d.corpse)
        .expect("a new corpse");
    assert_ne!(first, second);
    let clock = h.clock();
    yalain_world::death::sent_back(&mut h.server, &mut h.net, world, first, clock);
    assert!(h.server.players[&world].trip.is_none(), "the first's: no");
    yalain_world::death::sent_back(&mut h.server, &mut h.net, world, second, clock);
    assert!(h.server.players[&world].trip.is_some(), "its own: yes");
}

/// A corpse 100 m off that the player has set out for: its walk, under way.
fn walking_to_a_corpse(h: &mut Harness, b: usize, me: u64, world: yalain_net::ConnId) -> u64 {
    let beetle = h
        .server
        .catalog
        .get(yalain_testkit::TEST_CREATURE)
        .cloned()
        .expect("the beetle");
    let mut at = h.server.players[&world].position;
    at.origin[0] += 100.0;
    let id = h.server.things.new_id();
    let mut corpse =
        yalain_world::things::Thing::of(id, &beetle, yalain_world::things::Kind::Corpse);
    corpse.at = Some(at);
    corpse.corpse = Some(yalain_world::things::Corpse {
        killer: me,
        originator: 0,
        contents: Vec::new(),
        expires: f64::MAX,
        creature: yalain_testkit::TEST_CREATURE,
        butchered: false,
        killer_level: 0,
        ..Default::default()
    });
    h.server.things.insert(corpse);
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(0x8216_0016, &[yalain_testkit::play::use_of(id, me)]),
    );
    h.run(4);
    assert!(h.server.players[&world].walking_to.is_some(), "on its way");
    id
}

/// A player that dies on its way to something is on its way no longer: its use is answered, and back
/// from the dead it may use things again without first moving by hand.
/// Behaviour: server.places.a-walk-ends-with-a-trip-or-a-death
#[test]
fn a_death_ends_a_walk_and_leaves_no_use_refused_after_it() {
    let mut h = Harness::new();
    let (b, me) = about_to_die(&mut h);
    let world = *h.server.players.keys().next().expect("the player");
    walking_to_a_corpse(&mut h, b, me, world);
    let from = h.bots[b].inbox.len();
    let clock = h.clock();
    yalain_world::death::player_dies(&mut h.server, &mut h.net, world, None, clock);
    h.run(2);
    assert!(h.server.players[&world].walking_to.is_none(), "no longer");
    assert!(
        calls_of(&h, b, from)
            .iter()
            .any(|(f, _)| *f == yalain_world::places::function::USE_DONE),
        "its use is answered"
    );
    wait(&mut h, 11.0);
    h.bots[b].send(true, queue::WEENIE, &server_call(RESURRECT, &[]));
    wait(&mut h, 3.0);
    assert!(h.server.players[&world].dying.is_none(), "back");
    assert!(
        !yalain_world::places::actions_locked(&h.server, world),
        "and free to use things"
    );
}

/// The use answers refused as actions locked (1029) a client was sent since `from`: each one's
/// control flags, distance and effective target.
fn locked_answers(h: &Harness, b: usize, from: usize) -> Vec<(u32, f32, u64)> {
    use yalain_world::places::usage;
    calls_of(h, b, from)
        .into_iter()
        .filter(|(f, _)| *f == yalain_world::places::function::USE_DONE)
        .filter_map(|(_, v)| {
            let Some(Value::Graph(g)) = v.first() else {
                return None;
            };
            g.iter().find_map(|(_, n)| match n {
                calls::Node::Script { slots, .. }
                    if slots[usage::STATUS].1 == yalain_world::places::ACTIONS_LOCKED =>
                {
                    let target = u64::from(slots[usage::EFFECTIVE_TARGET + 1].1) << 32
                        | u64::from(slots[usage::EFFECTIVE_TARGET].1);
                    Some((
                        slots[usage::FLAGS].1,
                        f32::from_bits(slots[usage::DISTANCE].1),
                        target,
                    ))
                }
                _ => None,
            })
        })
        .collect()
}

/// A use asked while an earlier one is under way (a walk to its object) is refused as actions
/// locked with no effective target and no distance, the request's control flags kept.
/// Behaviour: server.places.actions-locked-answer
#[test]
fn a_use_while_another_is_under_way_is_refused_with_no_target_or_distance() {
    let mut h = Harness::new();
    let (b, me) = about_to_die(&mut h);
    let world = *h.server.players.keys().next().expect("the player");
    let corpse = walking_to_a_corpse(&mut h, b, me, world);
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(0x8216_0016, &[yalain_testkit::play::use_of(corpse, me)]),
    );
    h.run(4);
    assert_eq!(locked_answers(&h, b, from), vec![(316, -1.0, 0)]);
}

/// A dead player's use of its own corpse is refused as actions locked with a player's corpse's
/// control flags (800), no effective target and no distance.
/// Behaviour: server.places.actions-locked-answer
#[test]
fn a_dead_players_use_of_its_own_corpse_is_refused_with_a_player_corpses_flags() {
    let mut h = Harness::new();
    let (b, me) = about_to_die(&mut h);
    let world = *h.server.players.keys().next().expect("the player");
    let clock = h.clock();
    yalain_world::death::player_dies(&mut h.server, &mut h.net, world, None, clock);
    let own = |h: &Harness| {
        h.server.things.corpses().find(|id| {
            h.server
                .things
                .get(*id)
                .and_then(|t| t.corpse.as_ref())
                .is_some_and(|c| c.originator == me)
        })
    };
    assert!(h.run_until(400, |h| own(h).is_some()), "its corpse is made");
    let corpse = own(&h).expect("its corpse");
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(0x8216_0016, &[yalain_testkit::play::use_of(corpse, me)]),
    );
    h.run(4);
    assert_eq!(locked_answers(&h, b, from), vec![(800, -1.0, 0)]);
}

/// A dead player swings at nothing: a run asked for is refused as its other uses are, the creature
/// beside it isn't hurt, and it pays nothing.
/// Behaviour: server.death.dead-player-starts-no-attack
#[test]
fn a_dead_player_starts_no_attack_run() {
    let mut h = Harness::new();
    let (b, me) = dead_with_its_box_up(&mut h);
    h.arm(b, me, 0x4700_051C);
    let creature = h
        .server
        .things
        .placed()
        .find(|t| t.alive())
        .map(|t| (t.id, t.health))
        .expect("a creature");
    let from = h.bots[b].inbox.len();
    let start = yalain_world::combat::function::START_ATTACK;
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(start, &[auberean_script::args::Arg::LongInt(creature.0)]),
    );
    wait(&mut h, 8.0);
    let p = h
        .server
        .players
        .values()
        .find(|p| p.character == me)
        .expect("in");
    assert!(p.attacking.is_none(), "no run");
    assert_eq!(
        h.server.things.get(creature.0).map(|t| t.health),
        Some(creature.1),
        "not struck"
    );
    assert!(
        calls_of(&h, b, from).contains(&(
            yalain_world::combat::function::BASIC_ATTACKS_FAILED,
            vec![Value::Int(yalain_world::skill_use::refusal::DEAD)]
        )),
        "told why"
    );
}

/// A player killed in a run of basic attacks is told its death first, then that the run failed as
/// interrupted, then that it is attacking no more; it attacks no more.
/// Behaviour: server.death.attack-ends-after-the-message
#[test]
fn a_player_killed_while_attacking_is_told_its_death_before_its_attack_ends() {
    use yalain_world::combat::{error, function};
    let mut h = Harness::new();
    h.add_creatures(1);
    let (b, me) = h.player_in_world("one", "Elara");
    scene_drawn(&mut h, b);
    h.arm(b, me, 0x4700_051C);
    let creature = h
        .server
        .things
        .placed()
        .find(|t| t.alive())
        .map(|t| t.id)
        .expect("a creature");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(
            function::START_ATTACK,
            &[auberean_script::args::Arg::LongInt(creature)],
        ),
    );
    let world = *h.server.players.keys().next().expect("the player");
    assert!(
        h.run_until(400, |h| h.server.players[&world].attacking.is_some()),
        "a run under way"
    );
    h.run(4);
    let from = h.bots[b].inbox.len();
    let clock = h.clock();
    yalain_world::death::player_dies(&mut h.server, &mut h.net, world, Some(creature), clock);
    h.run(4);
    let told: Vec<(u32, Vec<Value>)> = calls_of(&h, b, from)
        .into_iter()
        .filter(|(f, _)| {
            [
                DEATH_MESSAGE,
                function::BASIC_ATTACKS_FAILED,
                function::ATTACK_STATE,
            ]
            .contains(f)
        })
        .map(|(f, v)| (f, if f == DEATH_MESSAGE { Vec::new() } else { v }))
        .collect();
    assert_eq!(
        told,
        vec![
            (DEATH_MESSAGE, Vec::new()),
            (
                function::BASIC_ATTACKS_FAILED,
                vec![Value::Int(error::INTERRUPTED)]
            ),
            (function::ATTACK_STATE, vec![Value::Int(0)]),
        ]
    );
    assert!(h.server.players[&world].attacking.is_none(), "no run");
}

/// A player dying in a fellowship: in the order sent, it is told dead, then that it died (a notice
/// of its own), then wounded, then its death message; a fellow beside it sees it dead, then
/// wounded, then is told it died.
/// Behaviour: server.death.player-dies-dead-before-its-message
#[test]
fn a_dying_fellows_notice_comes_after_its_dead_flag() {
    use yalain_world::fellowship::{text, Fellow, Fellowship, Vitals};
    let mut h = Harness::new();
    let (b, me) = h.player_in_world("one", "Elara");
    scene_drawn(&mut h, b);
    let (c, other) = h.player_in_world("two", "Bram");
    scene_drawn(&mut h, c);
    h.run(5);
    h.server.fellowships.list.push(Fellowship {
        name: "The Band".into(),
        leader: me,
        flags: 0x1,
        room: 1,
        fellows: [me, other]
            .iter()
            .map(|id| Fellow {
                id: *id,
                joined: 0.0,
                sent: Vitals::default(),
                next_check: f64::MAX,
            })
            .collect(),
        last_claimant: 0,
    });
    let (fb, fc) = (h.bots[b].inbox.len(), h.bots[c].inbox.len());
    let world = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .map(|(w, _)| w)
        .expect("in the world");
    let clock = h.clock();
    yalain_world::death::player_dies(&mut h.server, &mut h.net, world, None, clock);
    h.run(3);
    let told = |bot: usize, from: usize| -> Vec<&'static str> {
        h.sent_messages(bot, from)
            .iter()
            .filter_map(|m| match m {
                ServerMessage::UpdateBoolVisual(q)
                    if q.object.id == me && q.quality == DEAD && q.value == 1 =>
                {
                    Some("dead")
                }
                ServerMessage::DoFx(e) if e.sender.id == me && e.action.fx == 0x4100_0002 => {
                    Some("wounded")
                }
                ServerMessage::PlayerCall(c) if c.function.0 == DEATH_MESSAGE => Some("message"),
                ServerMessage::PlayerCall(c)
                    if c.args
                        .as_ref()
                        .and_then(|a| calls::values(&a.0).ok())
                        .is_some_and(|v| {
                            v.iter().any(|x| {
                                matches!(x.root(), Some(yalain_world::calls::Node::TextInfo(t))
                                    if t.string_id == text::FELLOW_DIED.0)
                            })
                        }) =>
                {
                    Some("notice")
                }
                _ => None,
            })
            .collect()
    };
    assert_eq!(
        told(b, fb),
        ["dead", "notice", "wounded", "message"],
        "the dying player"
    );
    assert_eq!(told(c, fc), ["dead", "wounded", "notice"], "its fellow");
}
