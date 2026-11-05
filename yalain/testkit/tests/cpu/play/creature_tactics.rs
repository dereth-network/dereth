//! How creatures fight beyond closing in and striking: the combat mode their weapon gives them,
//! turning on the player that hurts them most, standing still while held, drawing their basic
//! attacks by weight, a damage debuff taking from their hits, and healing and buffing themselves;
//! effects that slow their attacks or their running, and armor debuffs adding to players' hits;
//! protectors buffing their kin and healers healing theirs; counters answering them and wards
//! keeping their effects off.
//! Fixture: creatures of templates made up for each test, in a camp beside where a character starts.

use auberean_protocol::{queue, ServerMessage};
use yalain_content::{Prop, Props};
use yalain_testkit::play::server_call;
use yalain_testkit::{
    test_template, Harness, BLOOD_LOATHER_EFFECT, CREATURE_ABSORB, CREATURE_HEAL,
    HEART_OF_ICE_EFFECT, PARALYZE_EFFECT, STILL_WATER_EFFECT, VACUUM_EFFECT,
};
use yalain_world::combat;
use yalain_world::effect_defs::{EffectDef, Kind, Scale};
use yalain_world::effects::{self, Cast, On};
use yalain_world::missiles::{self, AttackSkill, CREATURE_ATTACKS};
use yalain_world::rules::Outcome;
use yalain_world::things::Mode;

const TEMPLATE: u32 = 0x4700_2230;

/// A creature template of level 2 with 460 health that notices players within `perception`
/// meters, with these properties besides.
fn creature(perception: f32, extra: &[(&str, Prop)]) -> Props {
    let mut p = Props::default();
    p.set("GameplayStatistics_Level", Prop::Int(2))
        .set("GameplayStatistics_MaxHealth", Prop::Int(460))
        .set("AICombat_NaturalDamage", Prop::Float(1.0))
        .set("AICombat_NaturalVariance", Prop::Float(0.0))
        .set("AI_PerceptionRadius", Prop::Float(perception));
    for (k, v) in extra {
        p.set(k, v.clone());
    }
    p
}

/// A camp of one creature of these properties at `spot`.
fn camp(h: &mut Harness, props: Props, spot: [f32; 3]) {
    h.server.catalog = yalain_testkit::test_catalog();
    h.server
        .catalog
        .insert(test_template(TEMPLATE, 0x1169, props));
    h.server.spawns.camps.push(yalain_content::Camp {
        id: "made up".into(),
        cell: 0x0299_0109,
        origin: spot,
        radius: 0.0,
        population: 1,
        regen: 600.0,
        templates: vec![(TEMPLATE, 1)],
        spots: vec![(0x0299_0109, spot)],
        placed: "authored for the tests".into(),
        confidence: "authored".into(),
        moves: yalain_content::CampMoves::default(),
    });
    h.server
        .camps
        .push(yalain_world::spawn::CampState::default());
}

fn drawn(h: &mut Harness, b: usize) {
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
}

/// The camp's creature, as a client was told of it.
fn the_creature(h: &Harness, b: usize) -> u64 {
    h.world_messages(b, 0)
        .iter()
        .find_map(|m| match m {
            ServerMessage::CreateObject(o) if o.weenie.entity_did == TEMPLATE => Some(o.id),
            _ => None,
        })
        .expect("made for the player")
}

fn mode_of(h: &Harness, id: u64) -> Option<Mode> {
    h.server
        .things
        .get(id)
        .and_then(|t| t.creature.as_ref())
        .map(|c| c.mode)
}

fn target_of(h: &Harness, id: u64) -> Option<u64> {
    h.server
        .things
        .get(id)
        .and_then(|t| t.creature.as_ref())
        .and_then(|c| c.target)
}

/// A player beside a creature of these properties that has noticed it; the client, the player and
/// the creature.
fn noticed(extra: &[(&str, Prop)], spot: [f32; 3]) -> (Harness, usize, u64, u64) {
    let mut h = Harness::new();
    camp(&mut h, creature(20.0, extra), spot);
    let (b, me) = h.player_in_world("one", "Elara");
    drawn(&mut h, b);
    let id = the_creature(&h, b);
    assert!(
        h.run_until(200, |h| mode_of(h, id) == Some(Mode::Chasing)),
        "it goes after the player"
    );
    h.run(4);
    (h, b, me, id)
}

/// Keep a player alive whatever the creature does to it.
fn hardy(h: &mut Harness, me: u64) {
    use yalain_world::player::quality;
    let p = h
        .server
        .players
        .values_mut()
        .find(|p| p.character == me)
        .expect("in the world");
    yalain_world::quals::set_int(&mut p.qualities, quality::HEALTH_MAX, 100_000);
    yalain_world::quals::set_int(&mut p.qualities, quality::HEALTH, 100_000);
}

fn stats(extra: &[(&str, Prop)]) -> yalain_content::CreatureStats {
    yalain_content::CreatureStats::from_props(&creature(20.0, extra)).expect("a creature")
}

const WIELDS: (&str, Prop) = ("AICombat_WieldImplements", Prop::Bool(true));

/// A creature with a weapon takes the weapon's stance as it goes after a player, as a player does:
/// an axe with a shield beside it the shield's; two swords the two weapons'; a club alone the
/// one-handed weapon's; a polearm its own; none the monster's, and so one that wields no
/// implements whatever it names.
/// Behaviour: server.creatures.weapon-stance
#[test]
fn a_creature_takes_its_weapons_stance_as_it_goes_after_a_player() {
    let (h, b, _, id) = noticed(
        &[
            ("AI_Inventory1", Prop::Text("AxeGurogMinion".into())),
            ("AI_Inventory2", Prop::Text("ShieldGurogMinion".into())),
            ("AICombat_UseWeaponAndShield", Prop::Bool(true)),
            WIELDS,
        ],
        [61.0, 240.2, -44.6],
    );
    assert!(h.world_messages(b, 0).iter().any(
        |m| matches!(m, ServerMessage::DoMode(e) if e.sender.id == id && e.action.0 == 0x4100_000B)
    ));
    let mode = |extra: &[(&str, Prop)]| yalain_world::combat::creature_mode(&stats(extra));
    assert_eq!(mode(&[]), 0x4100_0012);
    assert_eq!(
        mode(&[
            ("AI_Inventory1", Prop::Text("ClubStoneDrudgeRobber".into())),
            WIELDS
        ]),
        0x4100_000A
    );
    assert_eq!(
        mode(&[("AI_Inventory1", Prop::Text("ClubStoneDrudgeRobber".into()))]),
        0x4100_0012,
        "no implements wielded"
    );
    assert_eq!(
        mode(&[
            ("AI_Inventory1", Prop::Text("SwordRevenantPrefect".into())),
            ("AI_Inventory1Quantity", Prop::Int(2)),
            ("AICombat_CanDualWield", Prop::Bool(true)),
            WIELDS,
        ]),
        0x4100_000E
    );
    assert_eq!(
        mode(&[
            ("AI_Inventory1", Prop::Text("PolearmJuggernautChaos".into())),
            WIELDS
        ]),
        0x4100_000C
    );
}

/// The made-up templates of an axe and a shield a creature wields.
const AXE: u32 = 0x4700_1EEC;
const SHIELD: u32 = 0x4700_1F60;

/// A camp of one creature with an axe and a shield, and a player 30 m off, out of its sight:
/// the player's bot, its character, its place, the creature and its two weapons.
fn axe_and_shield(h: &mut Harness) -> (usize, u64, yalain_store::StoredPosition, u64, Vec<u64>) {
    camp(
        h,
        creature(
            20.0,
            &[
                ("AI_Inventory1", Prop::Text("AxeGurogMinion".into())),
                ("AI_Inventory2", Prop::Text("ShieldGurogMinion".into())),
                ("AICombat_UseWeaponAndShield", Prop::Bool(true)),
                WIELDS,
            ],
        ),
        [61.0, 240.2, -44.6],
    );
    for (t, name) in [(AXE, "AxeGurogMinion"), (SHIELD, "ShieldGurogMinion")] {
        h.server
            .catalog
            .insert(test_template(t, 0x10D9, Props::default()));
        h.server.creature_weapons.insert(name.into(), t);
    }
    let (b, me) = h.player_in_world("one", "Elara");
    let home = h
        .server
        .players
        .values()
        .find(|p| p.character == me)
        .map(|p| p.position)
        .expect("in the world");
    stand_off(h, me, home, 30.0);
    drawn(h, b);
    hardy(h, me);
    let id = the_creature(h, b);
    let held: Vec<u64> = h
        .server
        .things
        .get(id)
        .and_then(|t| t.creature.as_ref())
        .map(|c| c.weapons.clone())
        .expect("a creature");
    (b, me, home, id, held)
}

/// The player stands `by` meters west of its place.
fn stand_off(h: &mut Harness, me: u64, home: yalain_store::StoredPosition, by: f32) {
    if let Some(p) = h.server.players.values_mut().find(|p| p.character == me) {
        p.position.origin[0] = home.origin[0] - by;
    }
}

/// A creature wields its weapons as objects of their own: none sent while it is at peace; as it
/// goes after a player, each made hanging from it (the axe at its first place, the shield at its
/// second hand's), held by it, then its stance; as it gives up, each let go of (the shield first),
/// the monster's mode and peace; going after the player again, each attached again; as it leaves
/// the world, they go with it.
/// Behaviour: server.creatures.weapon-objects
#[test]
fn a_creature_draws_its_weapons_as_objects_hung_from_it() {
    let mut h = Harness::new();
    let (b, me, home, id, held) = axe_and_shield(&mut h);
    let far = |h: &mut Harness, by: f32| stand_off(h, me, home, by);
    assert_eq!(held.len(), 2, "an axe and a shield");
    let made = |h: &Harness, w: u64| {
        h.world_messages(b, 0).iter().find_map(|m| match m {
            ServerMessage::CreateObject(o) if o.id == w => Some(o.clone()),
            _ => None,
        })
    };
    h.run(100);
    assert!(held.iter().all(|w| made(&h, *w).is_none()), "none at peace");
    // It notices the player.
    far(&mut h, 0.0);
    let from = h.bots[b].inbox.len();
    assert!(
        h.run_until(200, |h| mode_of(h, id) == Some(Mode::Chasing)),
        "it goes after the player"
    );
    h.run(4);
    for (w, place) in held.iter().zip([1, 2]) {
        let o = made(&h, *w).expect("made as it draws");
        assert_eq!(o.physics.bits, 0xA2, "{w:x}");
        assert_eq!(o.physics.parent.parent, id);
        assert_eq!(o.physics.parent.location, place);
        assert_eq!(o.physics.mode, 0x4000_0001);
        assert_eq!(o.physics.stamps[0], 1);
        assert_eq!(o.weenie.objects[0], id, "held by it");
        assert_eq!(o.weenie.objects[1], id, "wielded by it");
    }
    let msgs = h.world_messages(b, from);
    let at = |pred: &dyn Fn(&ServerMessage) -> bool| msgs.iter().position(pred);
    let stance = at(&|m| matches!(m, ServerMessage::DoMode(e) if e.sender.id == id && e.action.0 == 0x4100_000B))
        .expect("its stance");
    assert!(
        at(&|m| matches!(m, ServerMessage::CreateObject(o) if o.id == held[0])).expect("made")
            < stance
    );
    // It gives up: let go of, shield first, then the monster's mode and peace.
    far(&mut h, 60.0);
    let from = h.bots[b].inbox.len();
    assert!(
        h.run_until(200, |h| mode_of(h, id) == Some(Mode::Returning)),
        "it gives up"
    );
    h.run(4);
    let msgs = h.world_messages(b, from);
    let gone: Vec<u64> = msgs
        .iter()
        .filter_map(|m| match m {
            ServerMessage::DeParent(e) if e.sender.id == id => Some(e.action.id),
            _ => None,
        })
        .collect();
    assert_eq!(gone, vec![held[1], held[0]]);
    let modes: Vec<u32> = msgs
        .iter()
        .filter_map(|m| match m {
            ServerMessage::DoMode(e) if e.sender.id == id => Some(e.action.0),
            _ => None,
        })
        .collect();
    assert_eq!(modes, vec![0x4100_0012, 0x4000_0001]);
    // Back at it: attached again, the count two more.
    far(&mut h, 0.0);
    let from = h.bots[b].inbox.len();
    assert!(
        h.run_until(800, |h| mode_of(h, id) == Some(Mode::Chasing)),
        "it goes after the player again"
    );
    h.run(4);
    let attached: Vec<(u64, u32, u16)> = h
        .world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::Parent(e) if e.action.parent.id == id => {
                Some((e.sender.id, e.action.location, e.action.parent.other_stamp))
            }
            _ => None,
        })
        .collect();
    assert_eq!(attached, vec![(held[0], 1, 3), (held[1], 2, 3)]);
    // Dead and gone: its weapons with it.
    if let Some(t) = h.server.things.get_mut(id) {
        t.health = 0;
    }
    let clock = h.clock();
    yalain_world::death::creature_dies(&mut h.server, &mut h.net, id, me, clock);
    let from = h.bots[b].inbox.len();
    assert!(
        h.run_until(400, |h| h.server.things.get(id).is_none()),
        "it leaves"
    );
    h.run(4);
    for w in &held {
        assert!(h
            .world_messages(b, from)
            .iter()
            .any(|m| matches!(m, ServerMessage::ForgetObject(o) if o.0.id == *w)));
        assert!(h.server.things.get(*w).is_none());
    }
}

/// A creature taking its weapons up again tells a client that knew them, weapon by weapon: no one
/// holds or wields it, then the creature wields and holds it, then it is attached; each update a
/// visual one of the weapon's own, one stamp after another.
/// Behaviour: server.creatures.weapon-taken-up-again
#[test]
fn a_creature_taking_its_weapons_up_again_wields_and_holds_each_before_attaching_it() {
    let mut h = Harness::new();
    let (b, me, home, id, held) = axe_and_shield(&mut h);
    stand_off(&mut h, me, home, 0.0);
    assert!(
        h.run_until(200, |h| mode_of(h, id) == Some(Mode::Chasing)),
        "it goes after the player"
    );
    stand_off(&mut h, me, home, 60.0);
    assert!(
        h.run_until(200, |h| mode_of(h, id) == Some(Mode::Returning)),
        "it gives up"
    );
    h.run(4);
    stand_off(&mut h, me, home, 0.0);
    let from = h.bots[b].inbox.len();
    assert!(
        h.run_until(800, |h| mode_of(h, id) == Some(Mode::Chasing)),
        "it goes after the player again"
    );
    h.run(4);
    let about: Vec<(u64, String, u16)> = h
        .world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::UpdateInstanceIdVisual(q) if held.contains(&q.object.id) => {
                let who = match q.value {
                    0 => "none",
                    v if v == id => "it",
                    _ => "other",
                };
                Some((
                    q.object.id,
                    format!("{}={who}", q.quality),
                    q.object.other_stamp,
                ))
            }
            ServerMessage::Parent(e) if held.contains(&e.sender.id) => {
                Some((e.sender.id, "attached".into(), 0))
            }
            _ => None,
        })
        .collect();
    let mut want = Vec::new();
    for w in &held {
        for step in ["261=none", "258=none", "261=it", "258=it", "attached"] {
            want.push((*w, step.to_string()));
        }
    }
    let got: Vec<(u64, String)> = about.iter().map(|(w, s, _)| (*w, s.clone())).collect();
    assert_eq!(got, want, "weapon by weapon");
    for w in &held {
        let stamps: Vec<u16> = about
            .iter()
            .filter(|(x, s, _)| x == w && s != "attached")
            .map(|(_, _, n)| *n)
            .collect();
        assert!(
            stamps.windows(2).all(|p| p[1] == p[0].wrapping_add(1)),
            "one stamp after another: {stamps:?}"
        );
    }
}

/// Each second for `seconds`, a second player hurts the creature (as it has lately done it more
/// harm than the player it fights); whether the creature turned on it.
fn harmed_by(h: &mut Harness, id: u64, other: u64, seconds: usize) -> bool {
    for _ in 0..seconds {
        let now = h.now.0;
        if let Some(c) = h
            .server
            .things
            .get_mut(id)
            .and_then(|t| t.creature.as_mut())
        {
            c.harm.push((now, other, 40));
        }
        if h.run_until(20, |h| target_of(h, id) == Some(other)) {
            return true;
        }
    }
    false
}

/// A creature fighting one player turns on another that has hurt it over the last ten seconds
/// when the one it fights has not, at one of its next attacks.
/// Behaviour: server.creatures.turn-on-the-most-harm
#[test]
fn a_creature_turns_on_the_player_that_has_hurt_it_most_lately() {
    let (mut h, _, me, id) = noticed(&[], [61.0, 240.2, -44.6]);
    hardy(&mut h, me);
    let (c, other) = h.player_in_world("two", "Babyfare");
    drawn(&mut h, c);
    hardy(&mut h, other);
    assert!(
        harmed_by(&mut h, id, other, 40),
        "it turns on the one hurting it"
    );
}

/// A creature draws its target again among the players that hurt it by their harm, the nearest of
/// them three times as likely: of two that did it the same harm, the nearer about three times in
/// four; of a nearer one that did a third of the other's, about one time in two.
/// Behaviour: server.creatures.redraw-by-harm-and-nearness
#[test]
fn a_creature_draws_its_target_by_harm_and_nearness() {
    let mut h = Harness::new();
    let share = |h: &mut Harness, from: &[(u64, f32, f64)]| {
        let n = 8000;
        let near = (0..n)
            .filter(|_| yalain_world::ai::draw(&mut h.server, from) == Some(1))
            .count();
        near as f64 / f64::from(n)
    };
    let even = share(&mut h, &[(1, 2.0, 50.0), (2, 9.0, 50.0)]);
    assert!((even - 0.75).abs() < 0.02, "{even}");
    let third = share(&mut h, &[(1, 2.0, 30.0), (2, 9.0, 90.0)]);
    assert!((third - 0.5).abs() < 0.02, "{third}");
}

/// A creature that no player has hurt over the last ten seconds turns now and then to another
/// player within its perception; with no other player there it keeps to the one it fights.
/// Behaviour: server.creatures.unharmed-turn-to-another
#[test]
fn a_creature_no_one_hurts_turns_now_and_then_to_another_player_near_it() {
    let (mut h, _, me, id) = noticed(&[], [61.0, 240.2, -44.6]);
    hardy(&mut h, me);
    h.run(600);
    assert_eq!(target_of(&h, id), Some(me), "no one else, no turn");
    let (c, other) = h.player_in_world("two", "Babyfare");
    drawn(&mut h, c);
    hardy(&mut h, other);
    assert!(
        h.run_until(2400, |h| target_of(h, id) == Some(other)),
        "it turns to the other"
    );
}

/// A held creature (Paralyze) does not move while the hold lasts, though the player it goes after
/// is beyond its reach; freed, it runs at the player again.
/// Behaviour: server.creatures.held-stand-still
#[test]
fn a_held_creature_stays_where_it_is_until_the_hold_ends() {
    let (mut h, _, me, id) = noticed(&[], [61.0, 240.2, -44.6]);
    hardy(&mut h, me);
    // The player steps 15 m away; the creature is held for ten seconds.
    if let Some(p) = h.server.players.values_mut().find(|p| p.character == me) {
        p.position.origin[0] -= 15.0;
    }
    let clock = h.clock();
    let cast = Cast {
        caster: me,
        spellcraft: 50.0,
        ..Cast::default()
    };
    effects::apply_on(
        &mut h.server,
        &mut h.net,
        On::Creature(id),
        PARALYZE_EFFECT,
        cast,
        clock,
    );
    let at = |h: &Harness| h.server.things.get(id).and_then(|t| t.at).expect("placed");
    let held = at(&h);
    for _ in 0..150 {
        h.step();
        let now = at(&h);
        assert!(
            (now.origin[0] - held.origin[0]).abs() < 1e-3
                && (now.origin[1] - held.origin[1]).abs() < 1e-3,
            "held still"
        );
    }
    h.run(100);
    let freed = at(&h);
    assert!(
        (freed.origin[0] - held.origin[0]).abs() > 1.0,
        "running again once freed"
    );
}

/// A creature of the generic monsters' tables draws its plain, `Strong` and `Stronger` basic
/// attacks four to two to one; any other creature draws them evenly.
/// Behaviour: server.creatures.basic-attacks-weighted
#[test]
fn a_generic_monster_favors_its_plain_attack_four_to_two_to_one() {
    let mut h = Harness::new();
    let basics: Vec<AttackSkill> = CREATURE_ATTACKS[..3].to_vec();
    let shares = |h: &mut Harness, extra: &[(&str, Prop)]| -> Vec<f32> {
        let st = stats(extra);
        let mut n = [0u32; 3];
        for _ in 0..7000 {
            let a = missiles::pick_basic(&mut h.server, &st, &basics).expect("one");
            let i = basics
                .iter()
                .position(|b| b.skill == a.skill)
                .expect("of them");
            n[i] += 1;
        }
        n.iter().map(|x| *x as f32 / 7000.0).collect()
    };
    let generic = shares(&mut h, &[("AI_AISubClass", Prop::Word(7))]);
    for (got, want) in generic.iter().zip([4.0 / 7.0, 2.0 / 7.0, 1.0 / 7.0]) {
        assert!((got - want).abs() < 0.03, "{generic:?}");
    }
    let even = shares(&mut h, &[]);
    for got in &even {
        assert!((got - 1.0 / 3.0).abs() < 0.03, "{even:?}");
    }
}

/// A damage debuff on a creature takes from each of its hits (Blood Loather at a rating of 50
/// takes 40).
/// Behaviour: server.creatures.damage-debuffs
#[test]
fn a_damage_debuff_takes_from_a_creatures_hits() {
    let (mut h, b, me, id) = noticed(
        &[("AICombat_NaturalDamage", Prop::Float(100.0))],
        [61.0, 240.2, -44.6],
    );
    assert_eq!(missiles::effects_add(&h.server, id), 0.0);
    let clock = h.clock();
    effects::apply_on(
        &mut h.server,
        &mut h.net,
        On::Creature(id),
        BLOOD_LOATHER_EFFECT,
        Cast {
            caster: me,
            spellcraft: 50.0,
            ..Cast::default()
        },
        clock,
    );
    assert_eq!(missiles::effects_add(&h.server, id), -40.0);
    // Its hits through the lowest levels' armor (all of it): 100 less 40.
    let hits = |h: &Harness| -> Vec<i32> {
        h.world_messages(b, 0)
            .iter()
            .filter_map(|m| match m {
                ServerMessage::DoStory(e) if e.sender.id == id && e.action.attacker == id => e
                    .action
                    .hooks
                    .iter()
                    .find(|k| k.target == me && k.numbers[3] < 0)
                    .map(|k| -k.numbers[3]),
                _ => None,
            })
            .collect()
    };
    let before = hits(&h).len();
    hardy(&mut h, me);
    h.run(200);
    let after: Vec<i32> = hits(&h).into_iter().skip(before).collect();
    assert!(!after.is_empty(), "it hits");
    assert!(after.iter().all(|d| *d == 60 || *d == 90), "{after:?}");
}

/// A creature with a heal, hurt to its warning level, heals itself at one of its turns: its story
/// on itself (its own behavior, the skill, a hit on itself), a second later the heal's appearance
/// and its story of the health it gets back (300 at a rating of 50), its health up; not again for
/// forty seconds. Unhurt, it does not heal.
/// Behaviour: server.creatures.heal-themselves
#[test]
fn a_hurt_creature_heals_itself_and_an_unhurt_one_does_not() {
    let heals = [
        ("NPC_StartingSkill1", Prop::Word(CREATURE_HEAL)),
        ("NPC_StartingSkill1Level", Prop::Int(50)),
    ];
    let (mut h, b, me, id) = noticed(&heals, [61.0, 240.2, -44.6]);
    hardy(&mut h, me);
    let story = |m: &ServerMessage| matches!(m, ServerMessage::DoStory(e) if e.sender.id == id && e.action.skill == CREATURE_HEAL);
    h.run(400);
    assert!(!h.world_messages(b, 0).iter().any(story), "unhurt, no heal");
    if let Some(t) = h.server.things.get_mut(id) {
        t.health = 200;
    }
    let from = h.bots[b].inbox.len();
    assert!(
        h.run_until(400, |h| h.world_messages(b, from).iter().any(story)),
        "it heals"
    );
    let msgs = h.world_messages(b, from);
    let ServerMessage::DoStory(e) = msgs.iter().find(|m| story(m)).expect("told") else {
        unreachable!()
    };
    assert_eq!(e.action.bits, 0x16);
    assert_eq!(e.action.attacker_behavior.behavior, 0x4100_0074);
    assert!(e.action.hooks.iter().any(|k| k.target == id));
    h.run(30);
    let msgs = h.world_messages(b, from);
    assert!(msgs.iter().any(
        |m| matches!(m, ServerMessage::DoFx(e) if e.sender.id == id && e.action.fx == 0x4100_0033)
    ));
    assert!(msgs.iter().any(
        |m| matches!(m, ServerMessage::DoStory(e) if e.sender.id == id
        && e.action.hooks.iter().any(|k| k.bits == 0x981 && k.numbers[3] == 300))
    ));
    let health = h.server.things.get(id).map_or(0, |t| t.health);
    assert!(health >= 460, "healed: {health}");
    // Hurt again at once, it waits out the forty seconds before it heals again.
    if let Some(t) = h.server.things.get_mut(id) {
        t.health = 200;
    }
    let again = h.bots[b].inbox.len();
    h.run(600);
    assert!(
        !h.world_messages(b, again).iter().any(story),
        "barred for forty seconds"
    );
}

/// A creature with a buff uses it on itself in its fight, and its appearance shows for its ten
/// seconds.
/// Behaviour: server.creatures.buff-themselves
#[test]
fn a_creature_buffs_itself_in_its_fight() {
    let (mut h, b, me, id) = noticed(
        &[
            ("NPC_StartingSkill1", Prop::Word(CREATURE_ABSORB)),
            ("NPC_StartingSkill1Level", Prop::Int(20)),
        ],
        [61.0, 240.2, -44.6],
    );
    hardy(&mut h, me);
    let shows = |m: &ServerMessage| matches!(m, ServerMessage::DoFx(e) if e.sender.id == id && e.action.fx == 0x4100_0518);
    assert!(
        h.run_until(1200, |h| h.world_messages(b, 0).iter().any(shows)),
        "it buffs itself"
    );
    assert!(h.world_messages(b, 0).iter().any(|m| matches!(m,
        ServerMessage::DoStory(e) if e.sender.id == id && e.action.skill == CREATURE_ABSORB)));
    let from = h.bots[b].inbox.len();
    assert!(h.run_until(230, |h| h
        .world_messages(b, from)
        .iter()
        .any(|m| matches!(m,
        ServerMessage::StopFx(e) if e.sender.id == id && e.action.0 == 0x4100_0518))));
}

/// Put an effect on a creature, cast by a player at a spellcraft.
fn put_on(h: &mut Harness, id: u64, by: u64, state: u32, spellcraft: f32) {
    let clock = h.clock();
    effects::apply_on(
        &mut h.server,
        &mut h.net,
        On::Creature(id),
        state,
        Cast {
            caster: by,
            spellcraft,
            ..Cast::default()
        },
        clock,
    );
}

/// The time scales of a creature's strikes a client was told of since `from`.
fn strike_scales(h: &Harness, b: usize, id: u64, from: usize) -> Vec<f32> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::DoStory(e) if e.sender.id == id && e.action.attacker == id => {
                Some(e.action.attacker_behavior.time_scale)
            }
            _ => None,
        })
        .collect()
}

/// A creature's strike comes when it is due, not at the world's next step: each gap is its strike's
/// own, within the harness's step.
/// Behaviour: server.creatures.attacks-when-due
#[test]
fn a_creatures_strike_comes_when_it_is_due_not_at_the_worlds_next_step() {
    let (mut h, b, me, id) = noticed(&[], [61.0, 240.2, -44.6]);
    hardy(&mut h, me);
    let from = h.bots[b].inbox.len();
    let mut at: Vec<(f64, u32)> = Vec::new();
    let mut told = 0;
    for _ in 0..1500 {
        h.step();
        let now: Vec<u32> = h
            .world_messages(b, from)
            .iter()
            .filter_map(|m| match m {
                ServerMessage::DoStory(e) if e.sender.id == id && e.action.attacker == id => {
                    Some(e.action.skill)
                }
                _ => None,
            })
            .collect();
        if now.len() > told {
            at.push((h.now.0, *now.last().expect("told")));
            told = now.len();
        }
    }
    assert!(at.len() > 10, "{} strikes", at.len());
    for pair in at.windows(2) {
        let gap = pair[1].0 - pair[0].0;
        let due = combat::strike_gap(pair[0].1);
        assert!(
            (gap - due).abs() <= 0.051 + 0.001,
            "a gap of {gap:.3} where {due:.3} was due"
        );
    }
}

/// Between the world's steps only the creatures fighting are looked at for an attack due: none at
/// rest, however many stand in the world, and the one that takes on a player from the next step,
/// well before its first attack, until the step after its fight ends.
/// Behaviour: server.creatures.attacks-due-looked-for-among-those-fighting
#[test]
fn only_creatures_fighting_are_looked_at_for_an_attack_due() {
    let (mut h, b, me, id) = noticed(&[], [61.0, 240.2, -44.6]);
    hardy(&mut h, me);
    assert!(
        h.run_until(40, |h| h.server.fighting == [id]),
        "the one fighting, from a step on"
    );
    let from = h.bots[b].inbox.len();
    let struck = |h: &Harness| {
        h.world_messages(b, from).iter().any(|m| {
            matches!(m, ServerMessage::DoStory(e) if e.sender.id == id && e.action.attacker == id)
        })
    };
    assert!(!struck(&h), "listed before its first attack");

    // A hundred more at rest, far from anyone.
    h.server.spawns.camps[0].spots = vec![(0x0299_0109, [-500.0, 240.0, -44.6])];
    let now = h.now.0;
    for _ in 0..100 {
        assert!(yalain_world::spawn::spawn(&mut h.server, 0, now).is_some());
    }
    h.run(40);
    let standing = h.server.things.by_id.values().filter(|t| t.alive()).count();
    assert_eq!(standing, 101);
    assert_eq!(h.server.fighting, [id], "of {standing}");
    assert!(struck(&h), "and it strikes");

    assert!(h.server.things.remove(id).is_some());
    h.run(10);
    assert_eq!(h.server.fighting, Vec::<u64>::new(), "none once it is gone");
}

/// A creature that redraws its target between the world's steps and turns to one out of its reach
/// runs only on the step: it moves at no other time.
/// Behaviour: server.creatures.attacks-when-due
#[test]
fn a_creature_turning_to_a_far_target_between_steps_runs_only_on_the_step() {
    let (mut h, _, me, id) = noticed(&[], [61.0, 240.2, -44.6]);
    hardy(&mut h, me);
    let (c, other) = h.player_in_world("two", "Babyfare");
    drawn(&mut h, c);
    hardy(&mut h, other);
    let home = h.server.things.get(id).and_then(|t| t.at).expect("placed");
    stand_off(&mut h, other, home, 12.0);
    let mut turned = 0;
    for tick in 0..6000 {
        let (was, target) = (
            h.server.things.get(id).and_then(|t| t.at).expect("placed"),
            target_of(&h, id),
        );
        let stepped = h.server.last_step;
        if tick % 20 == 0 {
            let now = h.now.0;
            let by = if target == Some(me) { other } else { me };
            if let Some(c) = h
                .server
                .things
                .get_mut(id)
                .and_then(|t| t.creature.as_mut())
            {
                c.harm.push((now, by, 40));
            }
        }
        h.step();
        turned += usize::from(target_of(&h, id) != target);
        let at = h.server.things.get(id).and_then(|t| t.at).expect("placed");
        assert!(
            at == was || h.server.last_step != stepped,
            "it moved between steps"
        );
    }
    assert!(turned > 3, "it turned {turned} times");
}

/// An effect slowing a creature's attacks (Still Water at a spellcraft of 50 halves their pace)
/// slows them by the share its template takes (0.7 here: a pace of 0.65): its strikes are told at
/// that much of the time scale, and come that much less often. A hold slows them to a fifth at
/// most; a template that names no share takes all of it.
/// Behaviour: server.creatures.attack-speed-effects
#[test]
fn an_effect_slowing_a_creatures_attacks_slows_them_by_its_share() {
    let (mut h, b, me, id) = noticed(
        &[("NPC_CombatSpeedResistance", Prop::Float(0.7))],
        [61.0, 240.2, -44.6],
    );
    hardy(&mut h, me);
    assert_eq!(combat::attack_pace(&h.server, id), 1.0);
    let before = h.bots[b].inbox.len();
    h.run(400);
    let plain = strike_scales(&h, b, id, before);
    put_on(&mut h, id, me, STILL_WATER_EFFECT, 50.0);
    assert!((combat::attack_pace(&h.server, id) - 0.65).abs() < 1e-4);
    // What was already on its way is left out.
    h.run(3);
    let during = h.bots[b].inbox.len();
    h.run(400);
    let slowed = strike_scales(&h, b, id, during);
    assert!(!plain.is_empty() && !slowed.is_empty());
    for t in &plain {
        assert!((1.166..=1.426).contains(t), "{plain:?}");
    }
    for t in &slowed {
        assert!((0.758..=0.927).contains(t), "{slowed:?}");
    }
    assert!(
        slowed.len() * 10 <= plain.len() * 8,
        "fewer strikes: {} against {}",
        slowed.len(),
        plain.len()
    );
    // A hold: no attack speed, a fifth of the pace for a template taking all of it, 0.35 for one
    // taking 0.65.
    let held = |share: Option<f32>| {
        let extra: Vec<(&str, Prop)> = share
            .map(|r| vec![("NPC_CombatSpeedResistance", Prop::Float(r))])
            .unwrap_or_default();
        let (mut h, _, me, id) = noticed(&extra, [61.0, 240.2, -44.6]);
        put_on(&mut h, id, me, PARALYZE_EFFECT, 50.0);
        combat::attack_pace(&h.server, id)
    };
    assert!((held(None) - 0.2).abs() < 1e-4);
    assert!((held(Some(0.65)) - 0.35).abs() < 1e-4);
}

/// A slowed creature runs at its speed times the slow (Heart of Ice at a spellcraft of 50: 0.6 of
/// 6.4 m/s, a stride of 0.96 m a quarter second against 1.6 m).
/// Behaviour: server.creatures.slowed-run-slower
#[test]
fn a_slowed_creature_runs_at_its_speed_times_the_slow() {
    let (mut h, _, me, id) = noticed(
        &[("AI_PerceptionRadius", Prop::Float(80.0))],
        [61.0, 240.2, -44.6],
    );
    hardy(&mut h, me);
    let at = |h: &Harness| h.server.things.get(id).and_then(|t| t.at).expect("placed");
    // The longest stride over two seconds of chasing a player 40 m off.
    let stride = |h: &mut Harness| {
        let away = at(h).origin[0] - 40.0;
        if let Some(p) = h.server.players.values_mut().find(|p| p.character == me) {
            p.position.origin[0] = away;
        }
        let mut most: f32 = 0.0;
        for _ in 0..40 {
            let a = at(h);
            h.step();
            let b = at(h);
            most = most.max(
                ((b.origin[0] - a.origin[0]).powi(2) + (b.origin[1] - a.origin[1]).powi(2)).sqrt(),
            );
        }
        most
    };
    let free = stride(&mut h);
    assert!((free - 1.6).abs() < 0.05, "{free}");
    put_on(&mut h, id, me, HEART_OF_ICE_EFFECT, 50.0);
    let slowed = stride(&mut h);
    assert!((slowed - 0.96).abs() < 0.05, "{slowed}");
}

/// The damage of a player's hits on a creature with its armor rating lowered (Vacuum at a
/// spellcraft of 50 takes 100) is that much more.
/// Behaviour: server.creatures.armor-debuffs
#[test]
fn a_players_hits_gain_what_an_armor_debuff_took_from_the_creature() {
    let (mut h, b, me, id) = noticed(&[], [61.0, 240.2, -44.6]);
    hardy(&mut h, me);
    if let Some(t) = h.server.things.get_mut(id) {
        t.max_health = 1_000_000;
        t.health = 1_000_000;
    }
    h.arm(b, me, 0x4700_051C);
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(0x8204_0008, &[auberean_script::args::Arg::LongInt(id)]),
    );
    let hits = |h: &Harness, from: usize| -> Vec<i32> {
        h.world_messages(b, from)
            .iter()
            .filter_map(|m| match m {
                ServerMessage::DoStory(e) if e.action.attacker == me => e
                    .action
                    .hooks
                    .iter()
                    .find(|k| k.target == id && k.numbers[3] < 0)
                    .map(|k| -k.numbers[3]),
                _ => None,
            })
            .collect()
    };
    h.run(120);
    let plain = hits(&h, 0);
    assert_eq!(combat::armor_taken(&h.server, id), 0);
    put_on(&mut h, id, me, VACUUM_EFFECT, 50.0);
    assert_eq!(combat::armor_taken(&h.server, id), 100);
    let from = h.bots[b].inbox.len();
    h.run(120);
    let debuffed = hits(&h, from);
    assert!(!plain.is_empty() && !debuffed.is_empty());
    assert!(plain.iter().all(|d| *d <= 16), "{plain:?}");
    assert!(
        debuffed.iter().all(|d| (104..=116).contains(d)),
        "{debuffed:?}"
    );
}

/// A camp of two creatures of these properties side by side, both gone after a player; the
/// client, the player and the two.
fn pair(extra: &[(&str, Prop)]) -> (Harness, usize, u64, u64, u64) {
    let mut h = Harness::new();
    let mut props = creature(20.0, extra);
    props.set("AI_LoveTable", Prop::Word(0x06));
    h.server.catalog = yalain_testkit::test_catalog();
    h.server
        .catalog
        .insert(test_template(TEMPLATE, 0x1169, props));
    let spots = [[61.0, 240.2, -44.6], [63.0, 240.2, -44.6]];
    h.server.spawns.camps.push(yalain_content::Camp {
        id: "made up".into(),
        cell: 0x0299_0109,
        origin: spots[0],
        radius: 0.0,
        population: 2,
        regen: 600.0,
        templates: vec![(TEMPLATE, 1)],
        spots: spots.iter().map(|o| (0x0299_0109, *o)).collect(),
        placed: "authored for the tests".into(),
        confidence: "authored".into(),
        moves: yalain_content::CampMoves::default(),
    });
    h.server
        .camps
        .push(yalain_world::spawn::CampState::default());
    let (b, me) = h.player_in_world("one", "Elara");
    drawn(&mut h, b);
    hardy(&mut h, me);
    let ids: Vec<u64> = h
        .world_messages(b, 0)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::CreateObject(o) if o.weenie.entity_did == TEMPLATE => Some(o.id),
            _ => None,
        })
        .collect();
    assert_eq!(ids.len(), 2, "both made");
    assert!(
        h.run_until(200, |h| ids
            .iter()
            .all(|id| mode_of(h, *id) == Some(Mode::Chasing))),
        "both go after the player"
    );
    (h, b, me, ids[0], ids[1])
}

/// The stories in which a creature uses a skill on another, with their hooks on it.
fn helps_on(h: &Harness, b: usize, by: u64, on: u64, skill: u32) -> Vec<(u32, i32)> {
    h.world_messages(b, 0)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::DoStory(e) if e.sender.id == by && e.action.skill == skill => e
                .action
                .hooks
                .iter()
                .find(|k| k.target == on)
                .map(|k| (k.bits, k.numbers[3])),
            _ => None,
        })
        .collect()
}

/// A protector buffs a kin of its in the same fight (most often at its first turn, and often at its
/// next turn again on the same one), and the buff's appearance shows on the kin; a heal's or a
/// buff's hook takes a point of its target's health about one time in two.
/// Behaviour: server.creatures.protectors-buff-kin
#[test]
fn a_protector_buffs_its_kin_in_a_fight() {
    let extra = [
        ("NPC_StartingSkill1", Prop::Word(CREATURE_ABSORB)),
        ("NPC_StartingSkill1Level", Prop::Int(20)),
        ("AI_AISubClass", Prop::Word(7)),
    ];
    let (mut h, b, _, one, two) = pair(&extra);
    assert!(
        h.run_until(4000, |h| !helps_on(h, b, one, two, CREATURE_ABSORB)
            .is_empty()
            || !helps_on(h, b, two, one, CREATURE_ABSORB).is_empty()),
        "one buffs the other"
    );
    h.run(30);
    let (by, on) = if helps_on(&h, b, one, two, CREATURE_ABSORB).is_empty() {
        (two, one)
    } else {
        (one, two)
    };
    assert!(h.world_messages(b, 0).iter().any(
        |m| matches!(m, ServerMessage::DoFx(e) if e.sender.id == on && e.action.fx == 0x4100_0518)
    ));
    let _ = by;
    // Over many uses, the hooks: a point taken (with the health change) or nothing.
    h.run(6000);
    let hooks: Vec<(u32, i32)> = [(one, two), (two, one), (one, one), (two, two)]
        .iter()
        .flat_map(|(by, on)| helps_on(&h, b, *by, *on, CREATURE_ABSORB))
        .collect();
    assert!(hooks
        .iter()
        .all(|(bits, n)| (*bits == 0x381 && *n == 0) || (*bits == 0xB81 && *n == -1)));
    assert!(hooks.iter().any(|(bits, _)| *bits == 0x381));
    assert!(hooks.iter().any(|(bits, _)| *bits == 0xB81));
}

/// A creature that is neither a protector buffs only itself.
/// Behaviour: server.creatures.protectors-buff-kin
#[test]
fn a_creature_that_is_no_protector_buffs_only_itself() {
    let extra = [
        ("NPC_StartingSkill1", Prop::Word(CREATURE_ABSORB)),
        ("NPC_StartingSkill1Level", Prop::Int(20)),
    ];
    let (mut h, b, _, one, two) = pair(&extra);
    h.run(3000);
    assert!(helps_on(&h, b, one, two, CREATURE_ABSORB).is_empty());
    assert!(helps_on(&h, b, two, one, CREATURE_ABSORB).is_empty());
    assert!(
        !helps_on(&h, b, one, one, CREATURE_ABSORB).is_empty()
            || !helps_on(&h, b, two, two, CREATURE_ABSORB).is_empty()
    );
}

/// A healer heals its most wounded kin once that one is down to 40% of its health or less; a kin
/// above it is left to itself.
/// Behaviour: server.creatures.healers-heal-kin
#[test]
fn a_healer_heals_its_badly_hurt_kin() {
    let extra = [
        ("NPC_StartingSkill1", Prop::Word(CREATURE_HEAL)),
        ("NPC_StartingSkill1Level", Prop::Int(50)),
        ("AI_NPCTable", Prop::Text("NPCTable_MimbuHealer".into())),
    ];
    let (mut h, b, _, one, two) = pair(&extra);
    // The hurt one's own heal is barred: only its kin heals it.
    if let Some(c) = h
        .server
        .things
        .get_mut(two)
        .and_then(|t| t.creature.as_mut())
    {
        c.barred.push((CREATURE_HEAL, f64::MAX));
    }
    let set = |h: &mut Harness, id: u64, health: i32| {
        if let Some(t) = h.server.things.get_mut(id) {
            t.health = health;
        }
    };
    // Held at half its health for a minute and a quarter.
    for _ in 0..75 {
        set(&mut h, two, 230);
        h.run(20);
    }
    assert!(
        helps_on(&h, b, one, two, CREATURE_HEAL).is_empty(),
        "half its health: not yet"
    );
    set(&mut h, two, 120);
    assert!(
        h.run_until(4000, |h| !helps_on(h, b, one, two, CREATURE_HEAL)
            .is_empty()),
        "it heals its kin"
    );
    h.run(30);
    let health = h.server.things.get(two).map_or(0, |t| t.health);
    assert!(health > 300, "healed: {health}");
}

/// A player's worn record of a made-up counter or ward, its definition in the table.
fn holds(h: &mut Harness, me: u64, state: u32, kind: yalain_world::effect_defs::Kind) {
    let world = conn(h, me);
    h.server.effect_table.effects.insert(
        state,
        EffectDef {
            kind,
            internal_flags: 0x0100_0783,
            ..effect_of(state, Kind::Other)
        },
    );
    let clock = h.clock();
    let on = effects::put_on_worn(
        &mut h.server,
        &mut h.net,
        world,
        0x8130_0000_0000_0001,
        &[(state, 30.0)],
        clock,
    );
    assert_eq!(on.len(), 1, "it is on");
}

fn conn(h: &Harness, me: u64) -> yalain_net::ConnId {
    *h.server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .expect("in the world")
        .0
}

fn effect_of(state: u32, kind: Kind) -> EffectDef {
    EffectDef {
        state,
        kind,
        duration: Scale::default(),
        priority: Scale::default(),
        fx: 0,
        external_flags: 0,
        internal_flags: 0,
        enum_value: 0,
        eq_class: 0,
        name: None,
    }
}

const COUNTER: u32 = 0x6F00_7301;
const ANSWER: u32 = 0x6F00_7302;

/// A counter of this chance, answering by harming the one it answers for 1.
fn counter_held(h: &mut Harness, me: u64, chance: f32) {
    h.server.effect_table.effects.insert(
        ANSWER,
        effect_of(
            ANSWER,
            Kind::InstantVital {
                flags: 1,
                change: Scale(vec![(0.0, -1.0)]),
            },
        ),
    );
    holds(
        h,
        me,
        COUNTER,
        Kind::Counter {
            effect: ANSWER,
            chance: Scale(vec![(0.0, chance)]),
        },
    );
}

/// Each of a creature's strikes that reaches a player holding a counter that always answers is
/// answered, and none it evades.
/// Behaviour: server.effects.counter-answers-an-attack
#[test]
fn a_counter_answers_a_strike_that_lands_on_its_holder_and_not_one_evaded() {
    let (mut h, b, me, id) = noticed(&[], [61.0, 240.2, -44.6]);
    hardy(&mut h, me);
    counter_held(&mut h, me, 1.0);
    let from = h.bots[b].inbox.len();
    h.run(1000);
    let told = h.world_messages(b, from);
    let (mut landed, mut evaded, mut answers) = (0, 0, 0);
    for m in &told {
        let ServerMessage::DoStory(e) = m else {
            continue;
        };
        if e.action.attacker == me && e.sender.id == id {
            answers += 1;
        } else if e.action.attacker == id {
            let code = e.action.hooks.last().map_or(0, |k| k.numbers[0]);
            if code == Outcome::Hit.code().cast_signed()
                || code == Outcome::Critical.code().cast_signed()
            {
                landed += 1;
            } else {
                evaded += 1;
            }
        }
    }
    assert!(
        landed > 10 && evaded > 0,
        "{landed} landed, {evaded} evaded"
    );
    // One may still be on its way.
    assert!(
        (landed - 1..=landed).contains(&answers),
        "{answers} answers of {landed} landed, {evaded} evaded"
    );
}

/// A counter answers by its chance, each attack drawn alone.
/// Behaviour: server.effects.counter-answers-an-attack
#[test]
fn a_counter_answers_by_its_chance() {
    let (mut h, _, me, id) = noticed(&[], [61.0, 240.2, -44.6]);
    counter_held(&mut h, me, 0.25);
    let world = conn(&h, me);
    if let Some(t) = h.server.things.get_mut(id) {
        t.health = 1_000_000;
        t.max_health = 1_000_000;
    }
    let clock = h.clock();
    for _ in 0..4000 {
        effects::answer_attack(&mut h.server, &mut h.net, world, id, clock);
    }
    let answers = 1_000_000 - h.server.things.get(id).map_or(0, |t| t.health);
    assert!((800..1200).contains(&answers), "{answers} of 4,000");
}

/// A counter answers another player who attacks its holder, on that player.
/// Behaviour: server.effects.counter-answers-an-attack
#[test]
fn a_counter_answers_a_player_on_that_player() {
    let (mut h, _, me, id) = noticed(&[], [61.0, 240.2, -44.6]);
    hardy(&mut h, me);
    counter_held(&mut h, me, 1.0);
    let (_, other) = h.player_in_world("two", "Babyfare");
    let health = |h: &Harness, who: u64| {
        h.server
            .players
            .values()
            .find(|p| p.character == who)
            .and_then(|p| {
                yalain_world::quals::int(&p.qualities, yalain_world::player::quality::HEALTH)
            })
    };
    let (world, was, creature) = (
        conn(&h, me),
        health(&h, other),
        h.server.things.get(id).map(|t| t.health),
    );
    let clock = h.clock();
    effects::answer_attack(&mut h.server, &mut h.net, world, other, clock);
    assert_eq!(health(&h, other), was.map(|x| x - 1));
    assert_eq!(h.server.things.get(id).map(|t| t.health), creature);
}

/// Harm over time an effect does its holder is no attack: a counter leaves it unanswered.
/// Behaviour: server.effects.counter-answers-an-attack
#[test]
fn a_counter_does_not_answer_harm_over_time() {
    const BURN: u32 = 0x6F00_7303;
    let mut h = Harness::new();
    camp(&mut h, creature(0.1, &[]), [61.0, 240.2, -44.6]);
    let (b, me) = h.player_in_world("one", "Elara");
    drawn(&mut h, b);
    let id = the_creature(&h, b);
    hardy(&mut h, me);
    counter_held(&mut h, me, 1.0);
    h.server.effect_table.effects.insert(
        BURN,
        EffectDef {
            duration: Scale(vec![(0.0, 5.0)]),
            ..effect_of(
                BURN,
                Kind::VitalOverTime {
                    flags: 1,
                    per_second: Scale(vec![(0.0, -3.0)]),
                },
            )
        },
    );
    let (world, clock) = (conn(&h, me), h.clock());
    assert!(effects::apply(
        &mut h.server,
        &mut h.net,
        world,
        BURN,
        Cast {
            caster: id,
            spellcraft: 1.0,
            pk: true,
            ..Cast::default()
        },
        clock,
    ));
    let was = h.server.things.get(id).map(|t| t.health);
    h.run(120);
    let health = h.server.players.get(&world).and_then(|p| {
        yalain_world::quals::int(&p.qualities, yalain_world::player::quality::HEALTH)
    });
    assert!(health < Some(100_000), "it burned");
    assert_eq!(h.server.things.get(id).map(|t| t.health), was);
}

/// A ward keeps off effects of its classes by its chance, and no others; a ward of chance 1 keeps
/// off every one.
/// Behaviour: server.effects.ward-keeps-off-its-classes
#[test]
fn a_ward_keeps_off_the_effects_of_its_classes_by_its_chance() {
    let (mut h, _, me, _) = noticed(&[], [61.0, 240.2, -44.6]);
    hardy(&mut h, me);
    let world = conn(&h, me);
    const WARD: u32 = 0x6F00_7304;
    const SURE: u32 = 0x6F00_7305;
    const OF_ITS_CLASS: u32 = 0x6F00_7306;
    const OF_ANOTHER: u32 = 0x6F00_7307;
    const OF_THE_SURE: u32 = 0x6F00_7308;
    let harm = |state, class| EffectDef {
        eq_class: class,
        ..effect_of(
            state,
            Kind::InstantVital {
                flags: 1,
                change: Scale(vec![(0.0, -1.0)]),
            },
        )
    };
    for d in [
        harm(OF_ITS_CLASS, 48),
        harm(OF_ANOTHER, 49),
        harm(OF_THE_SURE, 22),
    ] {
        h.server.effect_table.effects.insert(d.state, d);
    }
    holds(
        &mut h,
        me,
        WARD,
        Kind::Ward {
            classes: vec![48],
            chance: Scale(vec![(0.0, 0.15)]),
        },
    );
    holds(
        &mut h,
        me,
        SURE,
        Kind::Ward {
            classes: vec![22],
            chance: Scale(vec![(0.0, 1.0)]),
        },
    );
    let clock = h.clock();
    let count = |state: u32, h: &mut Harness| {
        (0..4000)
            .filter(|_| {
                effects::apply(
                    &mut h.server,
                    &mut h.net,
                    world,
                    state,
                    Cast {
                        spellcraft: 1.0,
                        ..Cast::default()
                    },
                    clock,
                )
            })
            .count()
    };
    let through = count(OF_ITS_CLASS, &mut h);
    assert!((3200..3600).contains(&through), "{through} of 4,000");
    assert_eq!(count(OF_ANOTHER, &mut h), 4000);
    assert_eq!(count(OF_THE_SURE, &mut h), 0);
}
