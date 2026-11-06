//! Fights between players: who may harm whom, an attack on another player refused or swung, its
//! hits told as PK damage, a death at another player's hand; the permission that lets kingdoms
//! fight anywhere, and the protection that suspends it.

use auberean_protocol::types::StoryHook;
use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_content::Conflict;
use yalain_testkit::play::server_call;
use yalain_testkit::Harness;
use yalain_world::calls::{self, Node, Value};
use yalain_world::kingdoms::{self, kingdom, permission, refusal, Fighter};
use yalain_world::player::quality;
use yalain_world::quals;

const START_ATTACK: u32 = 0x8204_0008;
const ATTACK_STATE: u32 = 0x8204_001B;
const ATTACKS_FAILED: u32 = 0x8204_001C;
const WHO_KILLED: u32 = 0x8207_0030;
/// The human starting sword (a player attacks with a weapon in hand).
const STARTER_SWORD: u32 = 0x4700_051C;
/// Where every test character starts.
const START: u32 = 0x0299_0109;

fn fighter(id: u64, k: u32, status: u32) -> Fighter {
    Fighter {
        id,
        kingdom: k,
        status,
        allow: 0,
        deny: 0,
        dead: false,
        protected: false,
    }
}

/// Who may harm whom is the client's own check of a harmful act between two players: itself
/// always; never a dead or protected target; never where the target stands in peace, nor across
/// places of different statuses; anyone where all fight; else only a player of another kingdom,
/// and in a controlled area neither of them neutral (a player in no kingdom counts as neutral). A
/// player whom kingdoms may fight anywhere holds a peace area as kingdom against kingdom anywhere,
/// unless it is protected.
/// Behaviour: server.player-fights.who-may-harm
#[test]
fn who_may_harm_whom_follows_the_clients_check_of_a_harmful_act() {
    use kingdom::{DOMINION, NEUTRAL, ORDER, SHADOW};
    let may = |t: Fighter, a: Fighter| kingdoms::may_harm(&t, &a);
    // Peace.
    assert_eq!(
        may(fighter(1, SHADOW, 1), fighter(2, ORDER, 1)),
        Err(refusal::PEACE)
    );
    // Controlled.
    assert_eq!(may(fighter(1, SHADOW, 2), fighter(2, ORDER, 2)), Ok(()));
    assert_eq!(
        may(fighter(1, SHADOW, 2), fighter(2, NEUTRAL, 2)),
        Err(refusal::ATTACKER_NEUTRAL)
    );
    assert_eq!(
        may(fighter(1, NEUTRAL, 2), fighter(2, DOMINION, 2)),
        Err(refusal::TARGET_NEUTRAL)
    );
    assert_eq!(
        may(fighter(1, ORDER, 2), fighter(2, ORDER, 2)),
        Err(refusal::SAME_KINGDOM)
    );
    // Kingdom against kingdom: a neutral player may fight there.
    assert_eq!(may(fighter(1, NEUTRAL, 3), fighter(2, SHADOW, 3)), Ok(()));
    assert_eq!(
        may(fighter(1, DOMINION, 3), fighter(2, DOMINION, 3)),
        Err(refusal::SAME_KINGDOM)
    );
    // Free for all, a kingdom's own players too.
    assert_eq!(may(fighter(1, ORDER, 4), fighter(2, ORDER, 4)), Ok(()));
    // Two places of different statuses.
    assert_eq!(
        may(fighter(1, SHADOW, 3), fighter(2, ORDER, 4)),
        Err(refusal::OTHER_STATUS)
    );
    // Dead, protected, itself.
    let mut dead = fighter(1, SHADOW, 4);
    dead.dead = true;
    assert_eq!(may(dead, fighter(2, ORDER, 4)), Err(refusal::DEAD));
    let mut safe = fighter(1, SHADOW, 4);
    safe.protected = true;
    assert_eq!(may(safe, fighter(2, ORDER, 4)), Err(refusal::PROTECTED));
    assert_eq!(may(fighter(2, ORDER, 1), fighter(2, ORDER, 1)), Ok(()));
    // Anywhere: in peace for both, not for one alone, not while protected.
    assert_eq!(kingdoms::status_with(1, permission::ANYWHERE, false), 5);
    assert_eq!(kingdoms::status_with(1, permission::ANYWHERE, true), 1);
    assert_eq!(kingdoms::status_with(3, permission::ANYWHERE, false), 3);
    assert_eq!(kingdoms::status_with(1, 0xC0C, false), 1);
    assert_eq!(may(fighter(1, SHADOW, 5), fighter(2, ORDER, 5)), Ok(()));
    assert_eq!(
        may(fighter(1, SHADOW, 5), fighter(2, ORDER, 1)),
        Err(refusal::OTHER_STATUS)
    );
    assert_eq!(
        may(fighter(1, SHADOW, 1), fighter(2, ORDER, 5)),
        Err(refusal::PEACE)
    );
    assert_eq!(
        may(fighter(1, SHADOW, 5), fighter(2, SHADOW, 5)),
        Err(refusal::SAME_KINGDOM)
    );
    // Permissions that keep a kingdom's own players out.
    let mut kept = fighter(1, SHADOW, 4);
    kept.deny = permission::SAME_KINGDOM_TARGET;
    assert_eq!(may(kept, fighter(2, SHADOW, 4)), Err(refusal::SAME_KINGDOM));
}

/// The calls a client received since `from`: their functions and values.
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

fn scene_drawn(h: &mut Harness, b: usize) {
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
}

/// Set something of a player in the world.
fn with_player(h: &mut Harness, me: u64, f: impl FnOnce(&mut yalain_world::world::Player)) {
    let p = h
        .server
        .players
        .values_mut()
        .find(|p| p.character == me)
        .expect("the player is in the world");
    f(p);
}

fn health_of(h: &Harness, me: u64) -> i32 {
    h.server
        .players
        .values()
        .find(|p| p.character == me)
        .and_then(|p| quals::int(&p.qualities, quality::HEALTH))
        .expect("the player has health")
}

/// Two players beside each other where the start's landblock is `here`: an armed Shadow and an
/// Order one. Returns the harness and both (client, player).
fn two_players(here: Conflict) -> (Harness, (usize, u64), (usize, u64)) {
    let mut h = Harness::new();
    if here != Conflict::Peace {
        h.server.content.conflict.insert((START >> 16) as u16, here);
    }
    let (a, ida) = h.player_in_world("one", "Elara");
    let (b, idb) = h.player_in_world("two", "Babyfare");
    scene_drawn(&mut h, a);
    scene_drawn(&mut h, b);
    with_player(&mut h, ida, |p| {
        kingdoms::set_kingdom(&mut p.qualities, kingdom::SHADOW);
    });
    with_player(&mut h, idb, |p| {
        kingdoms::set_kingdom(&mut p.qualities, kingdom::ORDER);
    });
    h.arm(a, ida, STARTER_SWORD);
    h.run(10);
    (h, (a, ida), (b, idb))
}

fn attack(h: &mut Harness, a: usize, target: u64) {
    h.bots[a].send(
        true,
        queue::WEENIE,
        &server_call(START_ATTACK, &[Arg::LongInt(target)]),
    );
}

/// The hooks of the stories told by `attacker` aimed at `target` a client was sent since `from`.
fn hooks_at(h: &Harness, b: usize, from: usize, attacker: u64, target: u64) -> Vec<StoryHook> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::DoStory(e) if e.action.attacker == attacker => {
                Some(e.action.hooks.clone())
            }
            _ => None,
        })
        .flatten()
        .filter(|k| k.target == target)
        .collect()
}

/// An attack on another player that who may harm whom forbids (here a peace area) is refused: the
/// basic attacks fail with the reason, the player is not attacking, and the other is not struck.
/// Behaviour: server.player-fights.refused
#[test]
fn an_attack_on_another_player_the_rule_forbids_is_refused_with_its_reason() {
    let (mut h, (a, ida), (b, idb)) = two_players(Conflict::Peace);
    let from = h.bots[a].inbox.len();
    let from_b = h.bots[b].inbox.len();
    let health = health_of(&h, idb);
    attack(&mut h, a, idb);
    h.run(200);
    let calls = calls_of(&h, a, from);
    assert_eq!(
        calls
            .iter()
            .filter(|(f, _)| *f == ATTACKS_FAILED || *f == ATTACK_STATE)
            .cloned()
            .collect::<Vec<_>>(),
        [
            (ATTACKS_FAILED, vec![Value::Int(refusal::PEACE)]),
            (ATTACK_STATE, vec![Value::Int(0)])
        ]
    );
    assert!(hooks_at(&h, b, from_b, ida, idb).is_empty());
    assert_eq!(health_of(&h, idb), health);
}

/// Where who may harm whom allows it (here kingdom against kingdom, a Shadow and an Order player),
/// a player's attack run swings at another player: each hit's story, told to everyone who sees
/// it, takes the target's health and gives the same amount as PK damage, and the target's health
/// falls by it.
/// Behaviour: server.player-fights.hit-is-pk-damage
#[test]
fn a_hit_on_another_player_takes_its_health_and_tells_it_as_pk_damage() {
    let (mut h, (a, ida), (b, idb)) = two_players(Conflict::KingdomVsKingdom);
    let from_b = h.bots[b].inbox.len();
    let health = health_of(&h, idb);
    attack(&mut h, a, idb);
    let landed = |h: &Harness| {
        hooks_at(h, b, from_b, ida, idb)
            .iter()
            .any(|k| k.bits & 0x800 != 0 && k.numbers[3] < 0)
    };
    assert!(h.run_until(4000, landed), "a hit lands");
    let hits: Vec<StoryHook> = hooks_at(&h, b, from_b, ida, idb)
        .into_iter()
        .filter(|k| k.bits & 0x800 != 0)
        .collect();
    for k in &hits {
        assert_eq!(k.bits & 0x2000, 0x2000, "PK damage told: {k:?}");
        assert_eq!(k.numbers[5], -k.numbers[3]);
    }
    assert!(
        h.run_until(200, |h| health_of(h, idb) < health),
        "the hit lands"
    );
    assert_eq!(health_of(&h, idb), (health + hits[0].numbers[3]).max(0));
}

/// A skill that would make a pet at the player it lands on tells its user the summoning fails: pets
/// take on creatures alone, and none appears.
/// Behaviour: server.pets.summoned-at-a-player-fails
#[test]
fn a_summoning_aimed_at_a_player_tells_its_user_it_fails() {
    use yalain_world::effect_defs::{EffectDef, Kind, Scale};
    use yalain_world::pets;
    const EFFECT: u32 = 0x7000_0805;
    let (mut h, (a, ida), (_, idb)) = two_players(Conflict::KingdomVsKingdom);
    h.server.effect_table.effects.insert(
        EFFECT,
        EffectDef {
            kind: Kind::Pet {
                class: 0x10,
                lifetime: Scale::default(),
            },
            ..yalain_testkit::plain_effect(EFFECT)
        },
    );
    for d in h.server.skill_table.skills.values_mut() {
        if d.auto_attack {
            d.usage.as_mut().expect("used").target_effects = vec![EFFECT];
        }
    }
    let from = h.bots[a].inbox.len();
    attack(&mut h, a, idb);
    assert!(
        h.run_until(4000, |h| h
            .texts(a, from)
            .contains(&(1, pets::text::SUMMONING_FAILS.0))),
        "its user is told"
    );
    assert!(h.server.pets.of(ida).is_empty());
}

/// A basic attack that names a target effect puts it on the player it lands on, too.
/// Behaviour: server.combat.a-basic-attacks-target-effects-land
#[test]
fn a_basic_attack_that_names_a_target_effect_lands_it_on_another_player() {
    let (mut h, (a, _), (_, idb)) = two_players(Conflict::KingdomVsKingdom);
    for d in h.server.skill_table.skills.values_mut() {
        if d.auto_attack {
            d.usage.as_mut().expect("used").target_effects = vec![yalain_testkit::VACUUM_EFFECT];
        }
    }
    attack(&mut h, a, idb);
    let on = |h: &Harness| {
        h.server.players.values().any(|p| {
            p.character == idb
                && p.play
                    .effects
                    .iter()
                    .any(|e| e.state == yalain_testkit::VACUUM_EFFECT)
        })
    };
    assert!(h.run_until(4000, on), "it lands with a hit");
}

/// A strike at another player is told first; the target loses the health as the strike lands, as
/// long after the story as one at a creature, and with none left dies only then, so its client has
/// written the hit before it writes the kill.
/// Behaviour: server.player-fights.strike-lands
#[test]
fn a_strike_at_another_player_hurts_it_as_it_lands_and_the_kill_text_comes_after() {
    let (mut h, (a, ida), (b, idb)) = two_players(Conflict::FreeForAll);
    with_player(&mut h, idb, |p| {
        quals::set_int(&mut p.qualities, quality::HEALTH, 1);
    });
    let from_b = h.bots[b].inbox.len();
    attack(&mut h, a, idb);
    let hit = |m: &ServerMessage| {
        matches!(m, ServerMessage::DoStory(e) if e.action.attacker == ida
            && e.action.hooks.iter().any(|k| k.target == idb && k.bits & 0x800 != 0))
    };
    assert!(
        h.run_until(4000, |h| h.world_messages(b, from_b).iter().any(hit)),
        "a hit is told"
    );
    let told = h.now.0;
    let (skill, scale) = h
        .world_messages(b, from_b)
        .iter()
        .find_map(|m| match m {
            ServerMessage::DoStory(e) if hit(m) => {
                Some((e.action.skill, e.action.attacker_behavior.time_scale))
            }
            _ => None,
        })
        .expect("its story");
    assert_eq!(health_of(&h, idb), 1, "not hurt as it is told");
    let sword = Some(yalain_world::skill_use::implement::SWORD);
    let lands = yalain_world::combat::hit_seconds(skill, sword) / f64::from(scale);
    let at_none = |m: &ServerMessage| matches!(m, ServerMessage::UpdateInt(q) if q.quality == quality::HEALTH && q.value == 0);
    assert!(
        h.run_until(100, |h| h.world_messages(b, from_b).iter().any(at_none)),
        "it lands"
    );
    let gap = h.now.0 - told;
    assert!(
        (gap - lands).abs() < 0.12,
        "it landed {gap:.3} s after its story, not {lands:.3} s"
    );
    let messages = h.world_messages(b, from_b);
    let at = |pick: &dyn Fn(&ServerMessage) -> bool| messages.iter().position(pick);
    let health = at(
        &|m| matches!(m, ServerMessage::UpdateInt(q) if q.quality == quality::HEALTH && q.value == 0),
    );
    let kill_text = at(&|m| {
        matches!(m, ServerMessage::PlayerCall(c)
            if c.function.0 == yalain_world::death::player_fn::DEATH_MESSAGE)
    });
    assert!(health.is_some() && kill_text.is_some(), "health and text");
    assert!(at(&hit) < health, "the story before the health");
    assert!(health < kill_text, "the health before the kill text");
}

/// A strike told at a player who has gone into portal space does not hurt it as it lands.
/// Behaviour: server.player-fights.strike-lands
#[test]
fn a_strike_at_a_player_gone_into_portal_space_does_not_hurt_it() {
    let (mut h, (_, ida), (_, idb)) = two_players(Conflict::FreeForAll);
    let wa = yalain_world::social::world_of(&h.server, ida).expect("in the world");
    let wb = yalain_world::social::world_of(&h.server, idb).expect("in the world");
    let strike = yalain_world::combat::Landing {
        world: wa,
        target: idb,
        skill: 690,
        outcome: yalain_world::rules::Outcome::Hit,
        damage: 5,
        back: 0,
        index: 0,
        thrown: false,
        anim_hook: 1,
    };
    let health = health_of(&h, idb);
    let clock = h.clock();
    let to = h.server.players[&wb].position;
    yalain_world::places::resurrection_trip(&mut h.server, &mut h.net, wb, to, clock);
    yalain_world::combat::lands(&mut h.server, &mut h.net, strike, clock);
    assert_eq!(health_of(&h, idb), health);
}

/// A player killed by another is announced, to everyone who sees it, as killed by that player:
/// the killer's name as the world writes it and its object.
/// Behaviour: server.player-fights.death-names-the-killer
#[test]
fn a_player_killed_by_another_is_announced_with_its_killers_name() {
    let (mut h, (a, ida), (b, idb)) = two_players(Conflict::FreeForAll);
    with_player(&mut h, idb, |p| {
        quals::set_int(&mut p.qualities, quality::HEALTH, 1);
    });
    let from_b = h.bots[b].inbox.len();
    attack(&mut h, a, idb);
    let announced = |h: &Harness| {
        h.world_messages(b, from_b).iter().find_map(|m| match m {
            ServerMessage::VisualObjectCall(c)
                if c.object.id == idb && c.call.function.0 == WHO_KILLED =>
            {
                c.call.args.as_ref().and_then(|x| calls::values(&x.0).ok())
            }
            _ => None,
        })
    };
    assert!(h.run_until(4000, |h| announced(h).is_some()), "the death");
    let v = announced(&h).expect("announced");
    let name = v.iter().find_map(|x| match x.root() {
        Some(Node::TextInfo(t)) => t.literal.as_ref().map(|l| String::from_utf16_lossy(l)),
        _ => None,
    });
    assert_eq!(name.as_deref(), Some("Elara [Mv]"));
    assert!(v.contains(&Value::Long(ida)), "{v:?}");
}

/// Two players of different kingdoms whom kingdoms may fight anywhere fight in a peace area: each
/// passes through its own kingdom alone, and a run of one at the other swings and hurts. While one
/// is protected it passes through every kingdom again (its ethereality told as the protection
/// begins and as it ends), and a run on it is refused as protected.
/// Behaviour: server.player-fights.anywhere
#[test]
fn players_whom_kingdoms_may_fight_anywhere_fight_in_peace_unless_protected() {
    let (mut h, (a, ida), (b, idb)) = two_players(Conflict::Peace);
    let wa = yalain_world::social::world_of(&h.server, ida).expect("in the world");
    let wb = yalain_world::social::world_of(&h.server, idb).expect("in the world");
    let from = h.bots[a].inbox.len();
    for (id, w) in [(ida, wa), (idb, wb)] {
        with_player(&mut h, id, |p| {
            kingdoms::set_permissions(&mut p.qualities, permission::ANYWHERE | 0xC0C, 0);
        });
        let (s, net) = (&mut h.server, &mut h.net);
        kingdoms::settle_ethereality(s, net, w);
    }
    h.run(5);
    let ethereality = |h: &Harness, from: usize, who: u64| -> Vec<i32> {
        h.world_messages(a, from)
            .iter()
            .filter_map(|m| match m {
                ServerMessage::UpdateIntVisual(v) if v.object.id == who && v.quality == 7 => {
                    Some(v.value)
                }
                _ => None,
            })
            .collect()
    };
    assert_eq!(ethereality(&h, from, ida), [0x0A20_0010]);
    assert_eq!(ethereality(&h, from, idb), [0x1220_0010]);
    // A run in peace hurts.
    let from_b = h.bots[b].inbox.len();
    attack(&mut h, a, idb);
    let landed = |h: &Harness| {
        hooks_at(h, b, from_b, ida, idb)
            .iter()
            .any(|k| k.bits & 0x2000 != 0)
    };
    assert!(h.run_until(4000, landed), "a hit lands in peace");
    // Protected: every kingdom passed through, and the run on it refused as protected.
    let from = h.bots[a].inbox.len();
    let from_b = h.bots[b].inbox.len();
    let clock = h.clock();
    {
        let (s, net) = (&mut h.server, &mut h.net);
        yalain_world::effects::protect(s, net, wb, clock);
    }
    h.run(200);
    assert_eq!(ethereality(&h, from, idb), [0x3E20_0010]);
    let calls = calls_of(&h, a, from);
    assert!(calls.contains(&(ATTACKS_FAILED, vec![Value::Int(refusal::PROTECTED)])));
    assert!(calls.contains(&(ATTACK_STATE, vec![Value::Int(0)])));
    assert!(hooks_at(&h, b, from_b, ida, idb).is_empty(), "not struck");
    let from = h.bots[a].inbox.len();
    {
        let (s, net) = (&mut h.server, &mut h.net);
        yalain_world::effects::unprotect(s, net, wb, |_, _| {});
    }
    h.run(5);
    assert_eq!(ethereality(&h, from, idb), [0x1220_0010]);
}

/// Ask to use a skill on a target, as the client packs it (the use's number, the skill, the
/// target).
fn use_skill(h: &mut Harness, b: usize, id: u32, skill: u32, target: u64) -> usize {
    h.all_hold(skill);
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(
            yalain_world::skill_use::function::USE,
            &[Arg::Int(id), Arg::Int(skill), Arg::LongInt(target)],
        ),
    );
    from
}

/// Vigor to spare, for many uses.
fn rested(h: &mut Harness, me: u64) {
    with_player(h, me, |p| {
        quals::set_int(&mut p.qualities, quality::VIGOR_MAX, 5000);
        quals::set_int(&mut p.qualities, quality::VIGOR, 5000);
    });
}

/// A special skill aimed at another player that who may harm whom forbids (here a peace area) is
/// refused with the reason, and the other is not struck.
/// Behaviour: server.player-fights.special-refused
#[test]
fn a_special_at_another_player_the_rule_forbids_is_refused_with_its_reason() {
    let (mut h, (a, ida), (b, idb)) = two_players(Conflict::Peace);
    let from_b = h.bots[b].inbox.len();
    let from = use_skill(&mut h, a, 7, RIPOSTE, idb);
    h.run(100);
    let refused: Vec<Vec<Value>> = calls_of(&h, a, from)
        .into_iter()
        .filter(|(f, _)| *f == yalain_world::skill_use::function::REFUSED)
        .map(|(_, v)| v)
        .collect();
    assert_eq!(refused.len(), 1, "{refused:?}");
    assert!(
        refused[0].contains(&Value::Int(refusal::PEACE)),
        "{refused:?}"
    );
    assert!(hooks_at(&h, b, from_b, ida, idb).is_empty());
}

const RIPOSTE: u32 = 176;
const STAVE: u32 = 0x4700_051D;
const CHARGED_AIR_EFFECT: u32 = 0x7000_08CD;
const ADD_EFFECT: u32 = 0x8208_0017;

/// Where who may harm whom allows it, a special aimed at another player is used as on a creature:
/// its story from the player with the use's number and the skill; each hook that hurts the other
/// gives the health lost as PK damage, and its health falls by it.
/// Behaviour: server.player-fights.special-is-pk-damage
#[test]
fn a_special_at_another_player_hurts_it_and_tells_the_pk_damage() {
    let (mut h, (a, ida), (b, idb)) = two_players(Conflict::KingdomVsKingdom);
    rested(&mut h, ida);
    let health = health_of(&h, idb);
    let mut id = 1;
    let hit = loop {
        assert!(id < 40, "no use landed");
        let from_b = h.bots[b].inbox.len();
        use_skill(&mut h, a, id, RIPOSTE, idb);
        h.run(60);
        let story = h
            .world_messages(b, from_b)
            .into_iter()
            .find_map(|m| match m {
                ServerMessage::DoStory(e)
                    if e.sender.id == ida && e.action.attack_context == id =>
                {
                    Some(e.action)
                }
                _ => None,
            });
        let story = story.expect("the special's story");
        assert_eq!((story.bits, story.skill), (0x1E, RIPOSTE));
        if let Some(k) = story
            .hooks
            .iter()
            .find(|k| k.target == idb && k.bits & 0x800 != 0)
        {
            break k.clone();
        }
        id += 1;
    };
    assert_eq!(hit.bits & 0x2000, 0x2000, "{hit:?}");
    assert_eq!(hit.numbers[5], -hit.numbers[3]);
    assert!(hit.numbers[3] < 0);
    assert!(health_of(&h, idb) < health, "hurt");
}

/// A special's effects reach the other player it landed on, cast by its user (Charged Air's
/// damage over time, added on that player's client).
/// Behaviour: server.player-fights.special-effects-reach
#[test]
fn a_specials_effects_reach_the_other_player_it_lands_on() {
    let (mut h, (a, ida), (b, idb)) = two_players(Conflict::KingdomVsKingdom);
    rested(&mut h, ida);
    h.arm(a, ida, STAVE);
    // Six meters apart, beyond the skill's least range.
    let at = h
        .server
        .players
        .values()
        .find(|p| p.character == ida)
        .map(|p| p.position)
        .expect("in the world");
    with_player(&mut h, idb, |p| {
        p.position.origin = [at.origin[0] + 6.0, at.origin[1], at.origin[2]];
    });
    let from_b = h.bots[b].inbox.len();
    let mut landed = false;
    for id in 1..40 {
        use_skill(&mut h, a, id, yalain_testkit::CHARGED_AIR, idb);
        h.run(80);
        if yalain_world::effects::records_of(
            &h.server,
            yalain_world::effects::On::Player(
                yalain_world::social::world_of(&h.server, idb).expect("in the world"),
            ),
        )
        .iter()
        .any(|e| e.state == CHARGED_AIR_EFFECT && e.caster == ida)
        {
            landed = true;
            break;
        }
    }
    assert!(landed, "the effect is put on the other player");
    assert!(calls_of(&h, b, from_b)
        .iter()
        .any(|(f, _)| *f == ADD_EFFECT));
}

/// A player's health, once a damage over time has brought it to nothing, whoever put it on: its
/// death told by the killer's id, a creature's or none for its own.
fn over_time_to_the_end(
    caster: impl FnOnce(u64) -> u64,
    pk: bool,
) -> (Harness, usize, u64, u64, usize) {
    let mut h = Harness::new();
    let (b, me) = h.player_in_world("one", "Elara");
    scene_drawn(&mut h, b);
    with_player(&mut h, me, |p| {
        quals::set_int(&mut p.qualities, quality::HEALTH, 3);
    });
    let world = yalain_world::social::world_of(&h.server, me).expect("in the world");
    let from = h.bots[b].inbox.len();
    let clock = h.clock();
    let caster = caster(me);
    let (s, net) = (&mut h.server, &mut h.net);
    yalain_world::effects::apply(
        s,
        net,
        world,
        CHARGED_AIR_EFFECT,
        yalain_world::effects::Cast {
            caster,
            spellcraft: 1.0,
            pk,
            ..yalain_world::effects::Cast::default()
        },
        clock,
    );
    (h, b, me, caster, from)
}

fn killers_told(
    h: &Harness,
    b: usize,
    me: u64,
    from: usize,
) -> Vec<Vec<yalain_world::calls::Value>> {
    h.world_messages(b, from)
        .into_iter()
        .filter_map(|m| match m {
            ServerMessage::VisualObjectCall(c)
                if c.object.id == me && c.call.function.0 == WHO_KILLED =>
            {
                c.call
                    .args
                    .as_ref()
                    .and_then(|a| yalain_world::calls::values(&a.0).ok())
            }
            _ => None,
        })
        .collect()
}

/// A damage over time a creature put on a player kills it when its health is gone, the creature its
/// killer, as a hit would; the player doesn't stand at no health with nothing to end it.
/// Behaviour: server.effects.harm-over-time-kills
#[test]
fn a_creatures_damage_over_time_kills_the_player_it_takes_the_last_health_of() {
    const A_CREATURE: u64 = 0x8130_0000_0000_0042;
    let (mut h, b, me, _, from) = over_time_to_the_end(|_| A_CREATURE, false);
    assert!(
        h.run_until(4000, |h| health_of(h, me) == 0),
        "the ticks take its health"
    );
    h.run(5);
    let told = killers_told(&h, b, me, from);
    assert_eq!(told.len(), 1, "its death told once: {told:?}");
    assert!(
        told[0]
            .iter()
            .any(|v| matches!(v, yalain_world::calls::Value::Long(k) if *k == A_CREATURE)),
        "{told:?}"
    );
}

/// A player's own harm kills it too, with no killer: health at none is death, whoever took the last
/// of it.
/// Behaviour: server.effects.own-harm-kills
#[test]
fn a_players_own_harm_kills_it_with_no_killer() {
    let (mut h, b, me, _, from) = over_time_to_the_end(|me| me, true);
    assert!(
        h.run_until(4000, |h| health_of(h, me) == 0),
        "the ticks take its health"
    );
    h.run(5);
    let told = killers_told(&h, b, me, from);
    assert_eq!(told.len(), 1, "its death told once: {told:?}");
    assert!(
        told[0]
            .iter()
            .any(|v| matches!(v, yalain_world::calls::Value::Long(0))),
        "{told:?}"
    );
}

/// Never say die holds against a change over time as against a blow.
/// Behaviour: server.admin.never-say-die-keeps-an-admin-alive
#[test]
fn never_say_die_holds_against_a_damage_over_time() {
    const A_CREATURE: u64 = 0x8130_0000_0000_0042;
    let (mut h, b, me, _, from) = over_time_to_the_end(|_| A_CREATURE, false);
    with_player(&mut h, me, |p| p.admin.never_say_die = true);
    assert!(
        h.run_until(4000, |h| health_of(h, me) == 1),
        "the ticks take its health"
    );
    h.run_to(yalain_net::LocalTime(h.now.0 + 20.0));
    assert!(health_of(&h, me) >= 1, "it stops at one");
    assert!(killers_told(&h, b, me, from).is_empty(), "no death");
}

/// A damage over time another player put on a player: each tick, told by the player with the
/// caster as its attacker, gives the health lost as PK damage; with no health left the player dies
/// at the caster's hand; dead, its ticks are still told and change nothing.
/// Behaviour: server.player-fights.harm-over-time
#[test]
fn a_damage_over_time_from_another_player_is_pk_damage_and_kills() {
    let (mut h, (a, ida), (_, idb)) = two_players(Conflict::KingdomVsKingdom);
    with_player(&mut h, idb, |p| {
        quals::set_int(&mut p.qualities, quality::HEALTH, 3);
    });
    let wb = yalain_world::social::world_of(&h.server, idb).expect("in the world");
    let from = h.bots[a].inbox.len();
    let clock = h.clock();
    {
        let (s, net) = (&mut h.server, &mut h.net);
        yalain_world::effects::apply(
            s,
            net,
            wb,
            CHARGED_AIR_EFFECT,
            yalain_world::effects::Cast {
                caster: ida,
                spellcraft: 1.0,
                pk: true,
                ..yalain_world::effects::Cast::default()
            },
            clock,
        );
    }
    let ticks = |h: &Harness| -> Vec<StoryHook> {
        h.world_messages(a, from)
            .into_iter()
            .filter_map(|m| match m {
                ServerMessage::DoStory(e)
                    if e.sender.id == idb && e.action.attacker == ida && e.action.bits == 0x2 =>
                {
                    e.action.hooks.first().cloned()
                }
                _ => None,
            })
            .collect()
    };
    let died = |h: &Harness| {
        h.world_messages(a, from).iter().any(|m| {
            matches!(m, ServerMessage::VisualObjectCall(c)
                if c.object.id == idb && c.call.function.0 == WHO_KILLED)
        })
    };
    assert!(h.run_until(4000, died), "the tick kills");
    let told = ticks(&h);
    assert!(!told.is_empty());
    for k in &told {
        assert_eq!(k.bits, 0x2981, "{k:?}");
        assert_eq!(k.numbers[5], -k.numbers[3]);
    }
    let before = told.len();
    assert!(
        h.run_until(4000, |h| ticks(h).len() > before),
        "ticks told after the death"
    );
    assert_eq!(health_of(&h, idb), 0, "and they change nothing");
}

/// A dead player hurts no other player, whatever the place allows.
/// Behaviour: server.player-fights.dead-attacker-harms-no-one
#[test]
fn a_dead_player_strikes_no_other_player() {
    let (mut h, (a, ida), (_, idb)) = two_players(Conflict::FreeForAll);
    with_player(&mut h, ida, |p| {
        quals::set_int(&mut p.qualities, quality::HEALTH, 0);
    });
    let health = health_of(&h, idb);
    attack(&mut h, a, idb);
    h.run(400);
    assert_eq!(health_of(&h, idb), health);
    let p = h
        .server
        .players
        .values()
        .find(|p| p.character == ida)
        .expect("in");
    assert!(p.attacking.is_none());
}

/// A player in portal space begins no run, and one it began before it left swings at nothing until
/// it is out again.
/// Behaviour: server.player-fights.no-swing-from-portal-space
#[test]
fn a_run_waits_while_its_player_is_in_portal_space() {
    let (mut h, (a, ida), (_, idb)) = two_players(Conflict::FreeForAll);
    let wa = yalain_world::social::world_of(&h.server, ida).expect("in the world");
    let clock = h.clock();
    let to = h.server.players[&wa].position;
    yalain_world::places::resurrection_trip(&mut h.server, &mut h.net, wa, to, clock);
    let from = h.bots[a].inbox.len();
    let health = health_of(&h, idb);
    attack(&mut h, a, idb);
    h.run(4);
    assert_eq!(health_of(&h, idb), health, "not struck");
    assert!(h.server.players[&wa].attacking.is_none(), "no run begun");
    assert!(
        calls_of(&h, a, from)
            .iter()
            .any(|(f, _)| *f == ATTACKS_FAILED),
        "told"
    );
    // A run under way as it goes: no stories while it is away.
    let (mut h, (a, ida), (b, idb)) = two_players(Conflict::FreeForAll);
    let wa = yalain_world::social::world_of(&h.server, ida).expect("in the world");
    attack(&mut h, a, idb);
    h.run(2);
    let clock = h.clock();
    let to = h.server.players[&wa].position;
    yalain_world::places::resurrection_trip(&mut h.server, &mut h.net, wa, to, clock);
    let from_b = h.bots[b].inbox.len();
    let swung_while_away = |h: &Harness| {
        h.world_messages(b, from_b)
            .iter()
            .any(|m| matches!(m, ServerMessage::DoStory(e) if e.action.attacker == ida))
    };
    while h.server.players[&wa].trip.is_some() {
        assert!(!swung_while_away(&h), "nothing swung from portal space");
        h.step();
    }
}

/// A player in portal space uses no skill: refused, and the other isn't struck.
/// Behaviour: server.player-fights.no-skill-from-portal-space
#[test]
fn a_skill_is_refused_in_portal_space() {
    let (mut h, (a, ida), (b, idb)) = two_players(Conflict::KingdomVsKingdom);
    rested(&mut h, ida);
    let wa = yalain_world::social::world_of(&h.server, ida).expect("in the world");
    let clock = h.clock();
    let to = h.server.players[&wa].position;
    yalain_world::places::resurrection_trip(&mut h.server, &mut h.net, wa, to, clock);
    let health = health_of(&h, idb);
    let from_b = h.bots[b].inbox.len();
    let from = use_skill(&mut h, a, 7, RIPOSTE, idb);
    h.run(10);
    assert!(
        calls_of(&h, a, from)
            .iter()
            .any(|(f, _)| *f == yalain_world::skill_use::function::REFUSED),
        "refused"
    );
    assert!(hooks_at(&h, b, from_b, ida, idb).is_empty());
    assert_eq!(health_of(&h, idb), health);
}

/// A special that walks to another player is told only if who may harm whom still allows it when it
/// gets there, as each swing of a run: one protected on the way is not struck, and the use ends with
/// the reason.
/// Behaviour: server.player-fights.special-asks-again-as-it-is-told
#[test]
fn a_special_walking_to_another_player_ends_if_it_may_no_longer_harm_it() {
    let (mut h, (a, ida), (b, idb)) = two_players(Conflict::KingdomVsKingdom);
    rested(&mut h, ida);
    let wb = yalain_world::social::world_of(&h.server, idb).expect("in the world");
    let at = h.server.players[&wb].position;
    with_player(&mut h, idb, |p| p.position.origin[0] += 20.0);
    let from_b = h.bots[b].inbox.len();
    let from = use_skill(&mut h, a, 1, RIPOSTE, idb);
    h.run(10);
    assert!(
        hooks_at(&h, b, from_b, ida, idb).is_empty(),
        "not there yet"
    );
    let clock = h.clock();
    yalain_world::effects::protect(&mut h.server, &mut h.net, wb, clock);
    with_player(&mut h, idb, |p| p.position = at);
    h.run(60);
    assert!(hooks_at(&h, b, from_b, ida, idb).is_empty(), "not struck");
    assert!(
        calls_of(&h, a, from)
            .iter()
            .any(|(f, v)| *f == yalain_world::skill_use::function::REFUSED
                && v.contains(&Value::Int(refusal::PROTECTED))),
        "told why"
    );
}

/// A special's strike or missile does not hurt a player gone into portal space by the time it lands,
/// as a basic attack's does not.
/// Behaviour: server.player-fights.special-spares-one-gone-into-portal-space
#[test]
fn a_specials_strike_at_a_player_gone_into_portal_space_does_not_hurt_it() {
    let (mut h, (a, ida), (_, idb)) = two_players(Conflict::KingdomVsKingdom);
    rested(&mut h, ida);
    let wb = yalain_world::social::world_of(&h.server, idb).expect("in the world");
    let to = h.server.players[&wb].position;
    let wa = yalain_world::social::world_of(&h.server, ida).expect("in the world");
    let health = health_of(&h, idb);
    let before = h.server.players[&wb].play.effects.len();
    if let Some(u) = h
        .server
        .skill_table
        .skills
        .get_mut(&RIPOSTE)
        .and_then(|d| d.usage.as_mut())
    {
        u.target_effects = vec![CHARGED_AIR_EFFECT];
    }
    let mut id = 1;
    loop {
        assert!(id < 80, "no use reached its landing");
        use_skill(&mut h, a, id, RIPOSTE, idb);
        id += 1;
        let told = h.run_until(30, |h| {
            h.server.players[&wa]
                .using
                .as_ref()
                .is_some_and(|x| x.told && x.landings.iter().any(|l| l.damage > 0))
        });
        if told {
            break;
        }
        h.run(60);
    }
    assert_eq!(health_of(&h, idb), health, "not yet");
    let clock = h.clock();
    yalain_world::places::resurrection_trip(&mut h.server, &mut h.net, wb, to, clock);
    // Its landing comes while the other is away.
    let landed = |h: &Harness| h.server.players[&wa].using.is_none();
    while !landed(&h) {
        assert!(h.server.players[&wb].trip.is_some(), "still away");
        h.step();
        assert_eq!(health_of(&h, idb), health);
    }
    // Nor do its effects reach it (coming back from the trip has its own say on its health).
    assert_eq!(h.server.players[&wb].play.effects.len(), before);
}
