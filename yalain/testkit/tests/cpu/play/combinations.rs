//! Combinations: a skill leaves a link on a creature for a few seconds; a later skill whose
//! combination finds it takes it off and adds its own effect, and its caster's client plays the
//! combination's private effect; without the link it adds nothing. Fixture: the test camp's beetle
//! as a practice target, a bow in hand, Sinew Snap's link and Lilitha's Vengeance's combination as
//! the data defines them.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_store::StoredPosition;
use yalain_testkit::play::server_call;
use yalain_testkit::{Harness, LILITHAS_VENGEANCE, SINEW_SNAP, SINEW_SNAP_LINK, TEST_CREATURE};
use yalain_world::effects::{self, On, COMBO_FX};
use yalain_world::player::quality;
use yalain_world::quals;
use yalain_world::skill_use::function;

const BOW: u32 = 0x4700_051B;
const STRIKE_FX: u32 = 0x4100_02CB;

/// A new player with the scene drawn, vigor to spare and a bow in hand, the camp's beetle a
/// practice target six meters off; returns the client, the player and the beetle.
fn with_a_bow_by_a_beetle() -> (Harness, usize, u64, u64) {
    let mut h = Harness::new();
    h.add_creatures(1);
    let (b, me) = h.player_in_world("one", "Elara");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
    let beetle = h
        .server
        .things
        .placed()
        .find(|t| t.template == TEST_CREATURE)
        .map(|t| t.id)
        .expect("the camp's beetle");
    let at = {
        let p = h
            .server
            .players
            .values_mut()
            .find(|p| p.character == me)
            .expect("in the world");
        quals::set_int(&mut p.qualities, quality::VIGOR_MAX, 5000);
        quals::set_int(&mut p.qualities, quality::VIGOR, 5000);
        p.position
    };
    if let Some(t) = h.server.things.get_mut(beetle) {
        t.at = Some(StoredPosition {
            origin: [at.origin[0] + 6.0, at.origin[1], at.origin[2]],
            ..at
        });
        t.max_health = 100_000;
        t.health = 100_000;
        if let Some(c) = t.creature.as_mut() {
            c.stats.fights = false;
        }
    }
    h.arm(b, me, BOW);
    (h, b, me, beetle)
}

fn use_skill(h: &mut Harness, b: usize, id: u32, skill: u32, target: u64) -> usize {
    h.all_hold(skill);
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(
            function::USE,
            &[Arg::Int(id), Arg::Int(skill), Arg::LongInt(target)],
        ),
    );
    from
}

fn linked(h: &Harness, beetle: u64) -> bool {
    effects::records_of(&h.server, On::Creature(beetle))
        .iter()
        .any(|e| e.state == SINEW_SNAP_LINK)
}

/// Lilitha's Vengeance landing on a beetle that holds Sinew Snap's link: its caster's client
/// plays the combination's private effect, the link is taken off, and the beetle tells of the
/// strike the combination adds (twenty more damage at a rating of 1, the strike's appearance).
/// Behaviour: server.combinations.link-found
#[test]
fn a_combination_that_finds_its_link_takes_it_off_and_strikes() {
    let (mut h, b, me, beetle) = with_a_bow_by_a_beetle();
    let mut id = 1;
    let combined = loop {
        assert!(id < 60, "a combination within the tries");
        // Sinew Snap until its link is on the beetle.
        while !linked(&h, beetle) {
            use_skill(&mut h, b, id, SINEW_SNAP, beetle);
            id += 1;
            if !h.run_until(60, |h| linked(h, beetle)) {
                // Missed: again after its recovery.
                h.run(60);
            }
            assert!(id < 60, "a link within the tries");
        }
        let from = use_skill(&mut h, b, id, LILITHAS_VENGEANCE, beetle);
        id += 1;
        h.run(70);
        let msgs = h.world_messages(b, from);
        if msgs
            .iter()
            .any(|m| matches!(m, ServerMessage::PrivateFx(f) if f.0 == vec![COMBO_FX]))
        {
            break msgs;
        }
    };
    assert!(!linked(&h, beetle), "the link taken off");
    let strike = combined
        .iter()
        .find_map(|m| match m {
            ServerMessage::DoStory(e)
                if e.sender.id == beetle && e.action.bits == 0x2 && e.action.attacker == me =>
            {
                Some(e.action.hooks[0].clone())
            }
            _ => None,
        })
        .expect("the beetle tells of the strike");
    assert_eq!((strike.bits, strike.numbers[3]), (0x981, -20));
    assert!(combined.iter().any(
        |m| matches!(m, ServerMessage::DoFx(e) if e.sender.id == beetle && e.action.fx == STRIKE_FX)
    ));
    let private = combined
        .iter()
        .position(|m| matches!(m, ServerMessage::PrivateFx(_)))
        .expect("private");
    let told = combined
        .iter()
        .position(|m| matches!(m, ServerMessage::DoStory(e) if e.sender.id == beetle && e.action.bits == 0x2))
        .expect("told");
    assert!(private < told, "the private effect first");
}

/// Lilitha's Vengeance on a beetle with no link adds nothing: no strike of the beetle's own; its
/// caster is told the combination found nothing, an empty private effect, for each one that lands.
/// Behaviour: server.combinations.link-missing
#[test]
fn a_combination_without_its_link_adds_nothing() {
    let (mut h, b, me, beetle) = with_a_bow_by_a_beetle();
    let mut landed = false;
    for id in 1..20 {
        let from = use_skill(&mut h, b, id, LILITHAS_VENGEANCE, beetle);
        h.run(110);
        let msgs = h.world_messages(b, from);
        assert!(!msgs
            .iter()
            .any(|m| matches!(m, ServerMessage::PrivateFx(f) if f.0 == vec![COMBO_FX])));
        assert!(!msgs.iter().any(|m| matches!(m,
            ServerMessage::DoStory(e) if e.sender.id == beetle && e.action.bits == 0x2 && e.action.attacker == me)));
        landed |= msgs.iter().any(|m| {
            matches!(m,
            ServerMessage::DoStory(e) if e.sender.id == beetle && e.action.attacker == me
                && e.action.hooks.iter().any(|k| k.bits == 0xB81))
        });
        if landed {
            assert!(
                msgs.iter()
                    .any(|m| matches!(m, ServerMessage::PrivateFx(f) if f.0.is_empty())),
                "told it found nothing"
            );
            break;
        }
    }
    assert!(landed, "an arrow landed");
}
