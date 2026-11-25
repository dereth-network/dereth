//! The target a run goes on to: while a player attacks, the client names the next target as its
//! player picks another; the run's next swing aims at it (or the one after, when that swing's turn
//! has been sent already), and nothing answers it. A swing aimed at the player itself, a monster's
//! corpse or within the fellowship is refused and the run ends. A run that was stopped is not begun
//! again, and a selection alone moves no run.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_net::ConnId;
use yalain_testkit::play::server_call;
use yalain_testkit::{Harness, TEST_CREATURE};
use yalain_world::calls::{self, Value};
use yalain_world::combat::function as f;
use yalain_world::fellowship::{Fellow, Fellowship, Vitals};
use yalain_world::skill_use::refusal;

const SWORD: u32 = 0x4700_051C;

/// A new player with the test camp's two beetles beside it, each made a practice target that never
/// fights and never falls; the player armed and attacking the first. Returns the harness, the
/// client, the player and the two beetles.
fn attacking() -> (Harness, usize, u64, u64, u64) {
    let mut h = Harness::new();
    h.add_creatures(2);
    let (b, me) = h.player_in_world("one", "Elara");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
    let beetles: Vec<u64> = h
        .world_messages(b, 0)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::CreateObject(o) if o.weenie.entity_did == TEST_CREATURE => Some(o.id),
            _ => None,
        })
        .collect();
    assert!(beetles.len() >= 2, "the camp's two beetles: {beetles:?}");
    // Both within a sword's reach of the player, one on either side.
    let w = conn(&h, me);
    let at = h.server.players[&w].position;
    for (id, dx) in beetles[..2].iter().zip([1.0, -1.0]) {
        if let Some(t) = h.server.things.get_mut(*id) {
            let mut there = at;
            there.origin[0] += dx;
            t.at = Some(there);
            t.health = 1_000_000;
            if let Some(c) = t.creature.as_mut() {
                c.stats.fights = false;
            }
        }
    }
    h.arm(b, me, SWORD);
    send(&mut h, b, f::START_ATTACK, beetles[0]);
    (h, b, me, beetles[0], beetles[1])
}

fn send(h: &mut Harness, b: usize, function: u32, target: u64) {
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(function, &[Arg::LongInt(target)]),
    );
}

fn conn(h: &Harness, id: u64) -> ConnId {
    *h.server
        .players
        .iter()
        .find(|(_, p)| p.character == id)
        .expect("in the world")
        .0
}

/// The targets of the player's swings since `from`, in order.
fn swings(h: &Harness, b: usize, from: usize, me: u64) -> Vec<u64> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::DoStory(e) if e.action.attacker == me => e
                .action
                .hooks
                .iter()
                .map(|k| k.target)
                .find(|t| *t != 0 && *t != me),
            _ => None,
        })
        .collect()
}

/// The attack states and failed attacks a client was told since `from`, in order.
fn told(h: &Harness, b: usize, from: usize) -> Vec<(u32, u32)> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c)
                if c.function.0 == f::ATTACK_STATE || c.function.0 == f::BASIC_ATTACKS_FAILED =>
            {
                let v = calls::values(&c.args.as_ref()?.0).ok()?;
                match v.first() {
                    Some(Value::Int(n)) => Some((c.function.0, *n)),
                    _ => None,
                }
            }
            _ => None,
        })
        .collect()
}

/// Steps until the player has swung `n` times since `from` (at most `limit` steps).
fn until_swings(h: &mut Harness, b: usize, from: usize, me: u64, n: usize, limit: usize) {
    for _ in 0..limit {
        if swings(h, b, from, me).len() >= n {
            return;
        }
        h.step();
    }
}

/// Named just after a swing, the run's next swing goes at the new target, and none at the old
/// again; nothing answers the naming itself. Named once the next swing's turn to the old target
/// has been sent, that swing goes at the old target and the one after at the new.
/// Behaviour: server.player-fights.a-run-goes-on-to-the-target-named
#[test]
fn a_run_goes_on_to_the_target_the_client_names() {
    let (mut h, b, me, first, second) = attacking();
    until_swings(&mut h, b, 0, me, 1, 400);
    assert_eq!(
        swings(&h, b, 0, me).last(),
        Some(&first),
        "attacking the first"
    );
    let from = h.bots[b].inbox.len();
    send(&mut h, b, f::SET_NEXT_TARGET, second);
    h.step();
    assert!(
        !h.world_messages(b, from)
            .iter()
            .any(|m| matches!(m, ServerMessage::PlayerCall(_))),
        "nothing answers it"
    );
    until_swings(&mut h, b, from, me, 2, 800);
    assert_eq!(swings(&h, b, from, me)[..2], [second, second]);
    // Back to the first, named while the coming swing's turn to the second has gone out.
    let w = conn(&h, me);
    for _ in 0..400 {
        let p = &h.server.players[&w];
        if p.attacking
            .is_some_and(|r| p.turned_for == r.next_swing && r.next_swing > h.now.0)
        {
            break;
        }
        h.step();
    }
    let from = h.bots[b].inbox.len();
    send(&mut h, b, f::SET_NEXT_TARGET, first);
    h.step();
    until_swings(&mut h, b, from, me, 2, 800);
    assert_eq!(
        swings(&h, b, from, me)[..2],
        [second, first],
        "the swing under way first"
    );
}

/// A run moved on to the player itself is refused as on yourself, and ends.
/// Behaviour: server.player-fights.a-run-moved-onto-the-player-itself-is-refused
#[test]
fn a_run_moved_onto_the_player_itself_is_refused_as_on_yourself() {
    let (mut h, b, me, _, _) = attacking();
    until_swings(&mut h, b, 0, me, 1, 400);
    let from = h.bots[b].inbox.len();
    send(&mut h, b, f::SET_NEXT_TARGET, me);
    h.run(80);
    assert_eq!(
        told(&h, b, from),
        [
            (f::BASIC_ATTACKS_FAILED, refusal::ON_YOURSELF),
            (f::ATTACK_STATE, 0)
        ]
    );
    assert!(swings(&h, b, from, me).iter().all(|t| *t != me));
}

/// A run moved on to a monster's corpse is refused as on monster corpses, and ends.
/// Behaviour: server.player-fights.a-run-moved-onto-a-corpse-is-refused
#[test]
fn a_run_moved_onto_a_monsters_corpse_is_refused_as_on_monster_corpses() {
    let (mut h, b, me, _, second) = attacking();
    if let Some(t) = h.server.things.get_mut(second) {
        t.health = 0;
    }
    let clock = h.clock();
    yalain_world::death::creature_dies(&mut h.server, &mut h.net, second, me, clock);
    h.run(100);
    let corpse = h
        .server
        .things
        .by_id
        .values()
        .find(|t| {
            t.corpse
                .as_ref()
                .is_some_and(|c| c.creature == TEST_CREATURE)
        })
        .map(|t| t.id)
        .expect("the second beetle's corpse");
    until_swings(&mut h, b, 0, me, 1, 400);
    let from = h.bots[b].inbox.len();
    send(&mut h, b, f::SET_NEXT_TARGET, corpse);
    h.run(80);
    assert_eq!(
        told(&h, b, from),
        [
            (f::BASIC_ATTACKS_FAILED, refusal::ON_MONSTER_CORPSES),
            (f::ATTACK_STATE, 0)
        ]
    );
}

/// A run moved on to a fellow, or to its own pet, is refused as within the fellowship, and ends.
/// Behaviour: server.player-fights.a-run-moved-within-the-fellowship-is-refused
#[test]
fn a_run_moved_onto_a_fellow_or_its_own_pet_is_refused_as_within_the_fellowship() {
    for to_pet in [false, true] {
        let (mut h, b, me, _, second) = attacking();
        let (_, other) = h.player_in_world("two", "Bram");
        h.run(5);
        let at = h.server.players[&conn(&h, me)].position;
        if let Some(p) = h.server.players.get_mut(&conn(&h, other)) {
            p.position.origin = [at.origin[0] + 2.0, at.origin[1], at.origin[2]];
        }
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
        let target = if to_pet {
            let mut pet = yalain_world::pets::Pet {
                id: second,
                owner: me,
                template: TEST_CREATURE,
                class: 0,
                flags: 0,
                mode: 0,
                name: None,
                order: yalain_world::pets::Order::Follow,
                next_plan: f64::MAX,
                goal: None,
                effect: 0,
                assist: None,
                next_tend: f64::MAX,
                asked_tend: false,
                asked_buff: false,
                missed: false,
                closing: None,
                vigor: None,
                turn_to: None,
            };
            pet.owner = me;
            h.server.pets.by_id.insert(second, pet);
            second
        } else {
            other
        };
        until_swings(&mut h, b, 0, me, 1, 400);
        let from = h.bots[b].inbox.len();
        send(&mut h, b, f::SET_NEXT_TARGET, target);
        h.run(80);
        assert_eq!(
            told(&h, b, from),
            [
                (f::BASIC_ATTACKS_FAILED, refusal::WITHIN_FELLOWSHIP),
                (f::ATTACK_STATE, 0)
            ],
            "{}",
            if to_pet { "its own pet" } else { "a fellow" }
        );
    }
}

/// A run the client stops is not begun again by the next target it names with the stop: the run
/// ends, and no swing goes at the target named.
/// Behaviour: server.player-fights.a-stopped-run-is-not-begun-again-by-a-next-target
#[test]
fn a_stopped_run_is_not_begun_again_by_the_next_target_named_with_the_stop() {
    let (mut h, b, me, first, second) = attacking();
    until_swings(&mut h, b, 0, me, 1, 400);
    let from = h.bots[b].inbox.len();
    send(&mut h, b, f::STOP_ATTACKING, first);
    send(&mut h, b, f::SET_NEXT_TARGET, second);
    h.run(200);
    assert_eq!(told(&h, b, from), [(f::ATTACK_STATE, 0)]);
    assert!(
        swings(&h, b, from, me).is_empty(),
        "no swing after the stop"
    );
    assert!(h.server.players[&conn(&h, me)].attacking.is_none());
}

/// A selection of another target alone, with no next target named, moves no run: its swings go on
/// at the target it had.
/// Behaviour: server.player-fights.a-selection-alone-moves-no-run
#[test]
fn a_selection_alone_moves_no_run() {
    let (mut h, b, me, first, second) = attacking();
    until_swings(&mut h, b, 0, me, 1, 400);
    let from = h.bots[b].inbox.len();
    send(
        &mut h,
        b,
        yalain_world::requests::function::SET_SELECTION,
        second,
    );
    until_swings(&mut h, b, from, me, 2, 800);
    assert_eq!(swings(&h, b, from, me)[..2], [first, first]);
}
