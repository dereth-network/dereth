//! Skills whose hooks reach an area: every creature within the hook's radius of its center (the
//! target, or the user) is named in the story with its own outcome, and those it lands on take the
//! skill's effects; creatures beyond are left alone. Fixture: the test camp's beetles as practice
//! targets, placed around the target or the player.

use auberean_protocol::types::StoryHook;
use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_store::StoredPosition;
use yalain_testkit::play::server_call;
use yalain_testkit::{
    Harness, EYE_OF_THE_STORM, EYE_OF_THE_STORM_EFFECT, TEST_CREATURE, VACUUM, VACUUM_EFFECT,
};
use yalain_world::effects::{self, On};
use yalain_world::player::quality;
use yalain_world::quals;
use yalain_world::skill_use::function;

const STAVE: u32 = 0x4700_051D;
const SWORD: u32 = 0x4700_051C;
const HIT: u32 = 0x4000_0008;
const CRITICAL: u32 = 0x4000_0009;

/// A new player with the scene drawn and vigor to spare beside `n` of the camp's beetles made
/// practice targets; returns the client, the player and the beetles.
fn beside_beetles(n: u32) -> (Harness, usize, u64, Vec<u64>) {
    let mut h = Harness::new();
    h.add_creatures(n);
    let (b, me) = h.player_in_world("one", "Elara");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
    let beetles: Vec<u64> = h
        .server
        .things
        .placed()
        .filter(|t| t.template == TEST_CREATURE)
        .map(|t| t.id)
        .collect();
    assert_eq!(beetles.len(), n as usize);
    for id in &beetles {
        if let Some(c) = h
            .server
            .things
            .get_mut(*id)
            .and_then(|t| t.creature.as_mut())
        {
            c.stats.fights = false;
        }
    }
    let p = h
        .server
        .players
        .values_mut()
        .find(|p| p.character == me)
        .expect("in the world");
    quals::set_int(&mut p.qualities, quality::VIGOR_MAX, 5000);
    quals::set_int(&mut p.qualities, quality::VIGOR, 5000);
    (h, b, me, beetles)
}

/// Put a beetle so far (x, y) from the player.
fn place(h: &mut Harness, me: u64, beetle: u64, dx: f32, dy: f32) {
    let at = h
        .server
        .players
        .values()
        .find(|p| p.character == me)
        .map(|p| p.position)
        .expect("in the world");
    if let Some(t) = h.server.things.get_mut(beetle) {
        t.at = Some(StoredPosition {
            origin: [at.origin[0] + dx, at.origin[1] + dy, at.origin[2]],
            ..at
        });
    }
}

fn use_skill(h: &mut Harness, b: usize, id: u32, skill: u32, target: u64) -> usize {
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

/// The hooks of the player's story of a skill's use since `from`.
fn story_hooks(h: &Harness, b: usize, from: usize, me: u64, skill: u32) -> Vec<StoryHook> {
    h.world_messages(b, from)
        .iter()
        .find_map(|m| match m {
            ServerMessage::DoStory(e)
                if e.sender.id == me && e.action.attacker == me && e.action.skill == skill =>
            {
                Some(e.action.hooks.clone())
            }
            _ => None,
        })
        .expect("the use's story")
}

fn holds(h: &Harness, beetle: u64, state: u32) -> bool {
    effects::records_of(&h.server, On::Creature(beetle))
        .iter()
        .any(|e| e.state == state)
}

/// A skill at another player's pet, and an area around another creature, spare the pet while its
/// owner may not be harmed; where the two may fight, each reaches it.
/// Behaviour: server.areas.another-players-pet-goes-as-its-owner
#[test]
fn a_skill_and_an_area_spare_another_players_pet_while_its_owner_may_not_be_harmed() {
    let (mut h, b, me, beetles) = beside_beetles(2);
    let (target, pet) = (beetles[0], beetles[1]);
    let (_, them) = h.player_in_world("two", "Babyfare");
    h.run(10);
    h.server.pets.by_id.insert(
        pet,
        yalain_world::pets::Pet {
            id: pet,
            owner: them,
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
        },
    );
    place(&mut h, me, target, 6.0, 0.0);
    place(&mut h, me, pet, 9.0, 2.0);
    h.arm(b, me, STAVE);
    let from = use_skill(&mut h, b, 1, VACUUM, pet);
    h.run(120);
    let told = h.world_messages(b, from);
    assert!(
        !told
            .iter()
            .any(|m| matches!(m, ServerMessage::DoStory(e) if e.action.skill == VACUUM)),
        "aimed at the pet: refused"
    );
    let from = use_skill(&mut h, b, 2, VACUUM, target);
    h.run(120);
    let named: Vec<u64> = story_hooks(&h, b, from, me, VACUUM)[2..]
        .iter()
        .map(|k| k.target)
        .collect();
    assert_eq!(named, [target], "the area leaves the pet out");
    for p in h.server.players.values_mut() {
        p.conflict = 4;
    }
    h.run(200);
    let from = use_skill(&mut h, b, 3, VACUUM, target);
    h.run(120);
    let named: Vec<u64> = story_hooks(&h, b, from, me, VACUUM)[2..]
        .iter()
        .map(|k| k.target)
        .collect();
    assert!(named.contains(&pet), "where all may fight, it is reached");
}

/// Vacuum on one beetle reaches the beetle beside it (its area of ten meters) but not one twenty
/// meters off: after the hook that only plays, the area hook names the target and its neighbor
/// one after another (by id), each with its own outcome and the damage when it hurts; each one it
/// landed on takes Vacuum's debuff, the far one nothing.
/// Behaviour: server.areas.every-creature-within-the-radius
#[test]
fn an_area_hook_reaches_every_creature_within_its_radius_each_with_its_own_outcome() {
    let (mut h, b, me, beetles) = beside_beetles(3);
    let (target, near, far) = (beetles[0], beetles[1], beetles[2]);
    place(&mut h, me, target, 6.0, 0.0);
    place(&mut h, me, near, 9.0, 2.0);
    place(&mut h, me, far, 26.0, 0.0);
    h.arm(b, me, STAVE);
    let from = use_skill(&mut h, b, 1, VACUUM, target);
    h.run(120);
    let hooks = story_hooks(&h, b, from, me, VACUUM);
    assert_eq!(
        hooks
            .iter()
            .map(|k| (k.bits, k.target))
            .take(2)
            .collect::<Vec<_>>(),
        [(0x2, 0), (0x180, target)]
    );
    let area: Vec<&StoryHook> = hooks[2..].iter().collect();
    let named: Vec<u64> = area.iter().map(|k| k.target).collect();
    let mut want = vec![target, near];
    want.sort_unstable();
    assert_eq!(named, want, "the two within its radius, by id");
    for k in &area {
        let strike = if k.numbers[3] < 0 { 0xF81 } else { 0x781 };
        assert_eq!(k.bits, strike, "a strike with the hook's index");
        assert_eq!(k.numbers[2], 1, "the skill's second hook");
        let landed = [HIT, CRITICAL].contains(&k.numbers[0].cast_unsigned());
        assert_eq!(landed, holds(&h, k.target, VACUUM_EFFECT), "{k:?}");
    }
    assert!(!holds(&h, far, VACUUM_EFFECT));
    assert!(!h.world_messages(b, from).iter().any(|m| matches!(m,
        ServerMessage::DoStory(e) if e.sender.id == far)));
}

/// An area skill used on its user (the Eye of the Storm) is centered on the user: its hook that
/// only plays names the user, and its area names the creatures within ten meters of it.
/// Behaviour: server.areas.centered-on-the-user
#[test]
fn an_area_skill_used_on_its_user_reaches_the_creatures_around_the_user() {
    let (mut h, b, me, beetles) = beside_beetles(3);
    place(&mut h, me, beetles[0], 3.0, 0.0);
    place(&mut h, me, beetles[1], -4.0, 5.0);
    place(&mut h, me, beetles[2], 30.0, 0.0);
    h.arm(b, me, SWORD);
    let from = use_skill(&mut h, b, 1, EYE_OF_THE_STORM, 0);
    h.run(80);
    let hooks = story_hooks(&h, b, from, me, EYE_OF_THE_STORM);
    assert_eq!((hooks[1].bits, hooks[1].target), (0x180, me));
    let mut named: Vec<u64> = hooks[2..].iter().map(|k| k.target).collect();
    named.sort_unstable();
    let mut want = vec![beetles[0], beetles[1]];
    want.sort_unstable();
    assert_eq!(named, want);
    for k in &hooks[2..] {
        let landed = [HIT, CRITICAL].contains(&k.numbers[0].cast_unsigned());
        assert_eq!(landed, holds(&h, k.target, EYE_OF_THE_STORM_EFFECT));
    }
    assert!(!holds(&h, beetles[2], EYE_OF_THE_STORM_EFFECT));
}
