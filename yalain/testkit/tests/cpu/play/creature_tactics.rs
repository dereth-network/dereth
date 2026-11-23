//! How creatures fight beyond closing in and striking: the combat mode their weapon gives them,
//! turning on the player that hurts them most, standing still while held, drawing their basic
//! attacks by weight, a damage debuff taking from their hits, and healing and buffing themselves;
//! effects that slow their attacks or their running, and armor debuffs adding to players' hits;
//! protectors buffing their kin and healers healing theirs; counters answering them and wards
//! keeping their effects off; an idle holding a fight till it is through, and the idle some kinds
//! play as they notice a player.
//! Fixture: creatures of templates made up for each test, in a camp beside where a character starts.

use auberean_protocol::{queue, ServerMessage};
use yalain_content::{Prop, Props};
use yalain_testkit::play::server_call;
use yalain_testkit::{
    test_template, Harness, BLOOD_LOATHER_EFFECT, CREATURE_ABSORB, CREATURE_FORTIFY, CREATURE_HEAL,
    CREATURE_RESTORE_VIGOR, HEART_OF_ICE_EFFECT, PARALYZE_EFFECT, STILL_WATER_EFFECT,
    VACUUM_EFFECT,
};
use yalain_world::ai;
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
        name: None,
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
    // The other hurts it each second; at each of its attacks, its target freshly the first again,
    // it turns on the other at the draw's three in ten. Uncounted harm would leave only its far
    // smaller chance of turning to another player now and then.
    fn creature(h: &mut Harness, id: u64) -> &mut yalain_world::things::Creature {
        h.server
            .things
            .get_mut(id)
            .and_then(|t| t.creature.as_mut())
            .expect("a creature")
    }
    let (mut attacks, mut turns) = (0_u32, 0_u32);
    let mut considered = creature(&mut h, id).considered;
    let mut hurt_at = f64::NEG_INFINITY;
    for _ in 0..60_000 {
        let now = h.now.0;
        let c = creature(&mut h, id);
        if now - hurt_at >= 1.0 {
            c.harm.push((now, other, 40));
            hurt_at = now;
        }
        if c.considered != considered {
            considered = c.considered;
            attacks += 1;
            turns += u32::from(c.target == Some(other));
            c.target = Some(me);
            c.target_since = now;
        }
        if attacks == 600 {
            break;
        }
        h.step();
    }
    assert_eq!(attacks, 600, "it attacks");
    let share = f64::from(turns) / f64::from(attacks);
    assert!(
        share > 0.2,
        "it turns on the one hurting it: {turns} of {attacks}"
    );
}

/// Two players beside a creature of 1,000 health, which fights the first.
fn beside_a_creature_of_a_thousand() -> (Harness, u64, u64, u64) {
    let (mut h, _, me, id) = noticed(&[], [61.0, 240.2, -44.6]);
    hardy(&mut h, me);
    let (c, other) = h.player_in_world("two", "Babyfare");
    drawn(&mut h, c);
    hardy(&mut h, other);
    h.server
        .things
        .get_mut(id)
        .expect("the creature")
        .max_health = 1000;
    (h, me, other, id)
}

/// A taunt effect landing on a creature counts for its caster, a share of the creature's whole
/// health, up or down, for twenty seconds, and each adds its own; on a player it does nothing, and
/// a player who has done no harm at all is drawn by it.
/// Behaviour: server.creatures.a-taunt-counts-at-the-draw
#[test]
fn a_taunt_landing_counts_for_its_caster_for_twenty_seconds_by_its_share_of_whole_health() {
    const TAUNT: u32 = 0x6F00_7304;
    const LOW: u32 = 0x6F00_7305;
    let (mut h, me, other, id) = beside_a_creature_of_a_thousand();
    for (state, share) in [(TAUNT, 0.25), (LOW, 0.05)] {
        h.server.effect_table.effects.insert(
            state,
            effect_of(state, Kind::Taunt(Scale(vec![(1.0, share)]))),
        );
    }
    let land = |h: &mut Harness, on: On, state: u32, by: u64| {
        let clock = h.clock();
        let cast = Cast {
            caster: by,
            spellcraft: 1.0,
            ..Cast::default()
        };
        effects::apply_on(&mut h.server, &mut h.net, on, state, cast, clock)
    };
    let taunts = |h: &Harness| -> Vec<(u64, i64)> {
        let c = h.server.things.get(id).and_then(|t| t.creature.as_ref());
        c.expect("a creature")
            .taunts
            .iter()
            .map(|e| (e.1, e.2))
            .collect()
    };
    assert!(land(&mut h, On::Creature(id), TAUNT, other));
    assert!(land(&mut h, On::Creature(id), LOW, other));
    assert_eq!(taunts(&h), [(other, 250), (other, 50)]);
    let player = On::Player(conn(&h, me));
    assert!(!land(&mut h, player, TAUNT, other));
    // It draws at one of its attacks, by a chance: thinking again many times within the taunts'
    // twenty seconds, its target freshly the first each time, it is drawn at the draw's three in
    // ten. Taunts uncounted would leave only its far smaller chance of turning to another.
    let now = h.now.0;
    let n = 1000;
    let drawn = (0..n)
        .filter(|_| {
            let c = h
                .server
                .things
                .get_mut(id)
                .and_then(|t| t.creature.as_mut())
                .expect("a creature");
            c.target = Some(me);
            c.target_since = now;
            c.next_attack = now;
            c.considered = f64::NEG_INFINITY;
            yalain_world::ai::reconsider_for_test(&mut h.server, id, now);
            target_of(&h, id) == Some(other)
        })
        .count();
    #[allow(clippy::cast_precision_loss)]
    let share = drawn as f64 / f64::from(n);
    assert!(share > 0.2, "drawn by its taunts, no harm done: {share}");
    h.now.0 += yalain_world::ai::TAUNT_REMEMBERED;
    assert!(land(&mut h, On::Creature(id), LOW, me));
    assert_eq!(taunts(&h), [(me, 50)], "the old ones gone");
}

/// A taunt counts by its share at the spellcraft it is cast at (its skill's rating): along the line
/// its data draws from its lowest spellcraft to its highest, held at either end. Growl's 5% at 1
/// to 10% at 50 of a creature of a thousand; a detaunt's 10% to 15% below nothing the same.
/// Behaviour: server.creatures.a-taunts-share-follows-its-rating
#[test]
fn a_taunt_counts_by_its_share_at_the_rating_it_is_cast_at() {
    const GROWL: u32 = 0x6F00_730F;
    const BLUFF: u32 = 0x6F00_7310;
    let (mut h, _, other, id) = beside_a_creature_of_a_thousand();
    for (state, low, high) in [(GROWL, 0.05, 0.10), (BLUFF, -0.10, -0.15)] {
        let line = Scale(vec![(1.0, low), (50.0, high)]);
        h.server
            .effect_table
            .effects
            .insert(state, effect_of(state, Kind::Taunt(line)));
    }
    let mut counted = vec![];
    for (state, rating) in [
        (GROWL, 1.0),
        (GROWL, 25.5),
        (GROWL, 50.0),
        (GROWL, 70.0),
        (BLUFF, 1.0),
        (BLUFF, 50.0),
    ] {
        if let Some(c) = h
            .server
            .things
            .get_mut(id)
            .and_then(|t| t.creature.as_mut())
        {
            c.taunts.clear();
        }
        let clock = h.clock();
        let cast = Cast {
            caster: other,
            spellcraft: rating,
            ..Cast::default()
        };
        assert!(effects::apply_on(
            &mut h.server,
            &mut h.net,
            On::Creature(id),
            state,
            cast,
            clock
        ));
        let c = h.server.things.get(id).and_then(|t| t.creature.as_ref());
        counted.extend(c.expect("a creature").taunts.iter().map(|e| e.2));
    }
    assert_eq!(counted, [50, 75, 100, 100, -100, -150]);
}

/// How often, of `n` times it thinks again fighting `me` with what `prepare` gives it, a creature
/// turned on `other`.
fn turns_to(
    h: &mut Harness,
    (id, me, other): (u64, u64, u64),
    prepare: &dyn Fn(&mut yalain_world::things::Creature, f64),
) -> f64 {
    let n = 8000;
    let mut turned = 0;
    for _ in 0..n {
        let now = h.now.0;
        let c = h
            .server
            .things
            .get_mut(id)
            .and_then(|t| t.creature.as_mut())
            .expect("a creature");
        c.target = Some(me);
        c.target_since = now;
        c.next_attack = now;
        c.considered = f64::NEG_INFINITY;
        c.harm.clear();
        c.taunts.clear();
        prepare(c, now);
        yalain_world::ai::reconsider_for_test(&mut h.server, id, now);
        turned += i32::from(target_of(h, id) == Some(other));
    }
    f64::from(turned) / f64::from(n)
}

/// At the draw a taunt weighs as harm: with all the weight on the taunter a creature turns at the
/// draw's own chance and no more; twenty seconds on it weighs nothing; even with its target's harm
/// it takes a share. A detaunt takes from its caster's weight, and one left with none isn't drawn.
/// Behaviour: server.creatures.a-detaunt-takes-from-the-draw
#[test]
fn a_taunt_weighs_as_harm_at_the_draw_and_a_detaunt_takes_from_it() {
    let (mut h, me, other, id) = beside_a_creature_of_a_thousand();
    let who = (id, me, other);
    let alone = turns_to(&mut h, who, &|c, now| {
        c.taunts.push((now - 19.0, other, 50));
    });
    assert!((alone - 0.30).abs() < 0.03, "{alone}");
    let gone = turns_to(&mut h, who, &|c, now| {
        c.taunts.push((now - 21.0, other, 50));
        c.harm.push((now, me, 40));
    });
    assert!(gone < 0.001, "{gone}");
    let even = turns_to(&mut h, who, &|c, now| {
        c.taunts.push((now, other, 50));
        c.harm.push((now, me, 50));
    });
    assert!(even > 0.05 && even < 0.26, "{even}");
    let none_left = turns_to(&mut h, who, &|c, now| {
        c.harm.push((now, other, 50));
        c.taunts.push((now, other, -80));
        c.harm.push((now, me, 50));
    });
    assert!(none_left < 0.001, "{none_left}");
    let some_left = turns_to(&mut h, who, &|c, now| {
        c.harm.push((now, me, 90));
        c.harm.push((now, other, 80));
        c.taunts.push((now, other, -50));
    });
    assert!(some_left > 0.01 && some_left < even, "{some_left} {even}");
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
    // At 6.5% of its attacks (11% to one nearer): twenty minutes of them leave no dice a real
    // chance of none.
    assert!(
        h.run_until(24_000, |h| target_of(h, id) == Some(other)),
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

/// A creature uses one of its specials at an attack by how many are at hand: a generic monster
/// 0.081 with one, 0.142 with two and 0.201 with three, any other 0.223 with two (and, by this
/// server's own numbers, a generic monster 0.201 with more, any other 0.136 with one); drawn among
/// those at hand.
/// Behaviour: server.creatures.specials-by-how-many-at-hand
/// Behaviour: server.creatures.specials-at-hand-where-ours
#[test]
fn a_creature_uses_a_special_by_how_many_it_has_at_hand() {
    assert_eq!(
        (1..=5)
            .map(|n| missiles::special_chance(n, true))
            .collect::<Vec<_>>(),
        [0.081, 0.142, 0.201, 0.201, 0.201]
    );
    assert_eq!(
        (1..=2)
            .map(|n| missiles::special_chance(n, false))
            .collect::<Vec<_>>(),
        [0.136, 0.223]
    );
    assert_eq!(missiles::special_chance(0, true), 0.0);
    // Specials of the start area's kind that never bar, so every draw has them all at hand.
    let special = |skill: u32| AttackSkill {
        skill,
        special_chance: 0.0,
        barred_after: false,
        max_range: 40.0,
        ..CREATURE_ATTACKS[3]
    };
    let three = [special(901), special(902), special(903)];
    let (mut h, _, _, id) = noticed(&[], [61.0, 240.2, -44.6]);
    let share = |h: &mut Harness, generic: bool, n: usize| -> f32 {
        if let Some(c) = h
            .server
            .things
            .get_mut(id)
            .and_then(|t| t.creature.as_mut())
        {
            c.stats.sub_class = generic.then_some(7);
        }
        let now = h.now.0;
        let used = (0..20_000)
            .filter(|_| missiles::pick_special(&mut h.server, id, &three[..n], 5.0, now).is_some())
            .count();
        used as f32 / 20_000.0
    };
    for (generic, n, want) in [
        (true, 1, 0.081),
        (true, 2, 0.142),
        (true, 3, 0.201),
        (false, 1, 0.136),
        (false, 2, 0.223),
    ] {
        let got = share(&mut h, generic, n);
        assert!(
            (got - want).abs() < 0.012,
            "generic {generic}, {n} at hand: {got}"
        );
    }
}

/// A creature not of the generic monsters' tables with three or more specials at hand uses one at
/// an attack 0.10 for each, at most 1 (this server's own); drawn among those at hand.
/// Behaviour: server.creatures.specials-at-hand-where-ours
#[test]
fn an_older_creature_with_three_or_more_specials_at_hand_uses_one_a_tenth_for_each() {
    assert_eq!(
        [3, 4, 10, 12].map(|n| missiles::special_chance(n, false)),
        [0.3, 0.4, 1.0, 1.0]
    );
    let special = |skill: u32| AttackSkill {
        skill,
        special_chance: 0.0,
        barred_after: false,
        max_range: 40.0,
        ..CREATURE_ATTACKS[3]
    };
    let four = [special(901), special(902), special(903), special(904)];
    let (mut h, _, _, id) = noticed(&[], [61.0, 240.2, -44.6]);
    if let Some(c) = h
        .server
        .things
        .get_mut(id)
        .and_then(|t| t.creature.as_mut())
    {
        c.stats.sub_class = None;
    }
    for (n, want) in [(3, 0.3), (4, 0.4)] {
        let now = h.now.0;
        let used = (0..20_000)
            .filter(|_| missiles::pick_special(&mut h.server, id, &four[..n], 5.0, now).is_some())
            .count();
        #[allow(clippy::cast_precision_loss)]
        let got = used as f32 / 20_000.0;
        assert!((got - want).abs() < 0.015, "{n} at hand: {got}");
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
    // Up by the 300, as far as its whole health (drawn for each creature) allows.
    let whole = h.server.things.get(id).map_or(0, |t| t.max_health);
    assert!(
        health >= (200 + 300).min(whole),
        "healed: {health} of {whole}"
    );
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
    // A buff at about one in six of its first turns and one in twelve of its later ones: ten
    // minutes of turns leave no dice a real chance of none.
    assert!(
        h.run_until(12_000, |h| h.world_messages(b, 0).iter().any(shows)),
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

/// Each strike of a creature told for `steps` of the world: when it was told, and its time scale.
fn timed_strikes(h: &mut Harness, b: usize, id: u64, steps: u32) -> Vec<(f64, f32)> {
    let from = h.bots[b].inbox.len();
    let mut at = Vec::new();
    for _ in 0..steps {
        h.step();
        let told = strike_scales(h, b, id, from);
        if told.len() > at.len() {
            at.push((h.now.0, *told.last().expect("told")));
        }
    }
    at
}

/// The gaps between strikes, each against its due by the time scale of the strike before it.
fn gaps_as_due(at: &[(f64, f32)]) -> Vec<f64> {
    at.windows(2)
        .map(|pair| {
            let gap = pair[1].0 - pair[0].0;
            let due = yalain_world::timings::ends(&yalain_world::timings::unknown(), pair[0].1)
                + yalain_world::timings::THING_PAUSE;
            assert!(
                (gap - due).abs() <= 0.051 + 0.001,
                "a gap of {gap:.3} where {due:.3} was due"
            );
            gap
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
    let mut at: Vec<(f64, f32)> = Vec::new();
    let mut told = 0;
    for _ in 0..1500 {
        h.step();
        let now: Vec<f32> = h
            .world_messages(b, from)
            .iter()
            .filter_map(|m| match m {
                ServerMessage::DoStory(e) if e.sender.id == id && e.action.attacker == id => {
                    Some(e.action.attacker_behavior.time_scale)
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
        let due = yalain_world::timings::ends(&yalain_world::timings::unknown(), pair[0].1)
            + yalain_world::timings::THING_PAUSE;
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
    let plain = timed_strikes(&mut h, b, id, 400);
    put_on(&mut h, id, me, STILL_WATER_EFFECT, 50.0);
    assert!((combat::attack_pace(&h.server, id) - 0.65).abs() < 1e-4);
    // What was already on its way is left out.
    h.run(3);
    let slowed = timed_strikes(&mut h, b, id, 400);
    assert!(plain.len() > 3 && slowed.len() > 3, "{plain:?} {slowed:?}");
    for (_, t) in &plain {
        assert!((1.166..=1.426).contains(t), "{plain:?}");
    }
    for (_, t) in &slowed {
        assert!((0.758..=0.927).contains(t), "{slowed:?}");
    }
    // Each next strike comes by the time scale of the one before it, so every slowed gap is longer
    // than any plain one, whatever the draws within their bands.
    let longest_plain = gaps_as_due(&plain).into_iter().fold(0.0, f64::max);
    let shortest_slowed = gaps_as_due(&slowed).into_iter().fold(f64::MAX, f64::min);
    assert!(
        shortest_slowed > longest_plain,
        "less often: {shortest_slowed:.3} against {longest_plain:.3}"
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
/// spellcraft of 50 takes 100) is that much more: each hit of the weapon's own range (doubled for a
/// critical), and 100 over it with the debuff.
/// Behaviour: server.creatures.armor-debuffs
#[test]
fn a_players_hits_gain_what_an_armor_debuff_took_from_the_creature() {
    let (mut h, b, me, id) = noticed(&[], [61.0, 240.2, -44.6]);
    hardy(&mut h, me);
    if let Some(t) = h.server.things.get_mut(id) {
        t.max_health = 1_000_000;
        t.health = 1_000_000;
    }
    let sword = h.arm(b, me, 0x4700_051C);
    let w = combat::weapon_damage(&h.server, sword).expect("the sword's damage");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(0x8204_0008, &[auberean_script::args::Arg::LongInt(id)]),
    );
    let critical = yalain_world::rules::Outcome::Critical.code().cast_signed();
    // Each landed hit on the creature: whether critical, and its damage.
    let hits = |h: &Harness, from: usize| -> Vec<(bool, i32)> {
        h.world_messages(b, from)
            .iter()
            .filter_map(|m| match m {
                ServerMessage::DoStory(e) if e.action.attacker == me => e
                    .action
                    .hooks
                    .iter()
                    .find(|k| k.target == id && k.numbers[3] < 0)
                    .map(|k| (k.numbers[0] == critical, -k.numbers[3])),
                _ => None,
            })
            .collect()
    };
    // The creature has no armor of its own: a hit is the weapon's roll, a critical twice it.
    let within = |hits: &[(bool, i32)], over: i32| {
        hits.iter().all(|(crit, d)| {
            let times = if *crit { 2 } else { 1 };
            (times * w.least() + over..=times * w.most + over).contains(d)
        })
    };
    // Its first swings may come before the sword is in its hand.
    h.run(120);
    let from = h.bots[b].inbox.len();
    // Until a few hits have landed (each lands at a chance).
    h.run_until(600, |h| hits(h, from).len() >= 3);
    let plain = hits(&h, from);
    assert_eq!(combat::armor_taken(&h.server, id), 0);
    put_on(&mut h, id, me, VACUUM_EFFECT, 50.0);
    assert_eq!(combat::armor_taken(&h.server, id), 100);
    // A swing's story is on its way for a step: one more delivers those drawn before the debuff.
    h.step();
    let from = h.bots[b].inbox.len();
    // Within the debuff's thirty seconds.
    h.run_until(500, |h| hits(h, from).len() >= 3);
    let debuffed = hits(&h, from);
    assert!(!plain.is_empty() && !debuffed.is_empty());
    assert!(within(&plain, 0), "{plain:?}");
    assert!(within(&debuffed, 100), "{debuffed:?}");
}

/// A camp of two creatures of these properties side by side, both gone after a player; the
/// client, the player and the two.
fn pair(extra: &[(&str, Prop)]) -> (Harness, usize, u64, u64, u64) {
    let (h, b, me, ids) = pack(extra, 2);
    (h, b, me, ids[0], ids[1])
}

/// A camp of `n` creatures of these properties a pace apart, all gone after a player; the client,
/// the player and the creatures.
fn pack(extra: &[(&str, Prop)], n: usize) -> (Harness, usize, u64, Vec<u64>) {
    let mut h = Harness::new();
    let mut props = creature(20.0, extra);
    props.set("AI_LoveTable", Prop::Word(0x06));
    h.server.catalog = yalain_testkit::test_catalog();
    h.server
        .catalog
        .insert(test_template(TEMPLATE, 0x1169, props));
    #[allow(clippy::cast_precision_loss)]
    let spots: Vec<[f32; 3]> = (0..n)
        .map(|i| [61.0 + 2.0 * i as f32, 240.2, -44.6])
        .collect();
    h.server.spawns.camps.push(yalain_content::Camp {
        id: "made up".into(),
        cell: 0x0299_0109,
        origin: spots[0],
        radius: 0.0,
        population: u32::try_from(n).expect("a few"),
        regen: 600.0,
        templates: vec![(TEMPLATE, 1)],
        spots: spots.iter().map(|o| (0x0299_0109, *o)).collect(),
        placed: "authored for the tests".into(),
        confidence: "authored".into(),
        moves: yalain_content::CampMoves::default(),
        name: None,
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
    assert_eq!(ids.len(), n, "all made");
    assert!(
        h.run_until(200, |h| ids
            .iter()
            .all(|id| mode_of(h, *id) == Some(Mode::Chasing))),
        "all go after the player"
    );
    (h, b, me, ids)
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
    let on = if helps_on(&h, b, one, two, CREATURE_ABSORB).is_empty() {
        one
    } else {
        two
    };
    // The appearance comes as the buff's hook lands, by the drawn time scale of its story.
    assert!(
        h.run_until(200, |h| h.world_messages(b, 0).iter().any(|m| matches!(
            m,
            ServerMessage::DoFx(e) if e.sender.id == on && e.action.fx == 0x4100_0518
        ))),
        "the buff shows on its kin"
    );
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

/// Turns of a protector with `kin` of its kin in its fight near (the rest of the pack at rest),
/// each a fresh turn (its first of a fight, or a later one, nothing helped the turn before): the
/// share of turns it buffs a kin, and the kin it buffed, by how often each.
fn kin_turns(h: &mut Harness, ids: &[u64], kin: usize, first: bool, turns: u32) -> (f32, Vec<u32>) {
    let (me, rest) = ids.split_first().expect("a pack");
    for (i, id) in rest.iter().enumerate() {
        if let Some(c) = h
            .server
            .things
            .get_mut(*id)
            .and_then(|t| t.creature.as_mut())
        {
            c.mode = if i < kin { Mode::Chasing } else { Mode::Idle };
        }
    }
    let mut on = vec![0u32; rest.len()];
    let mut buffed = 0;
    let clock = h.clock();
    for _ in 0..turns {
        if let Some(c) = h
            .server
            .things
            .get_mut(*me)
            .and_then(|t| t.creature.as_mut())
        {
            c.actions = if first { 0 } else { 5 };
            c.kin_helped = None;
        }
        missiles::helps(&mut h.server, &mut h.net, *me, clock);
        let helped = h
            .server
            .things
            .get(*me)
            .and_then(|t| t.creature.as_ref())
            .and_then(|c| c.kin_helped);
        if let Some((_, k)) = helped {
            buffed += 1;
            on[rest.iter().position(|x| *x == k).expect("one of the pack")] += 1;
        }
    }
    #[allow(clippy::cast_precision_loss)]
    let share = buffed as f32 / turns as f32;
    (share, on)
}

/// A protector buffs one of its kin at a turn by one chance for how many of its kin in a fight are
/// near: at its first turn 0.19, 0.24, 0.29 and 0.32 with one, two, three and four or more, at a
/// later one 0.016, 0.028, 0.03 and 0.043; the kin drawn evenly among them.
/// Behaviour: server.creatures.protectors-buff-kin-by-how-many
#[test]
fn a_protectors_chance_to_buff_a_kin_goes_by_how_many_are_near() {
    let extra = [
        ("NPC_StartingSkill1", Prop::Word(CREATURE_ABSORB)),
        ("NPC_StartingSkill1Level", Prop::Int(20)),
        ("AI_AISubClass", Prop::Word(7)),
    ];
    let (mut h, _, _, ids) = pack(&extra, 6);
    for (k, first, want) in [
        (1, true, 0.19),
        (2, true, 0.24),
        (3, true, 0.29),
        (5, true, 0.32),
        (1, false, 0.016),
        (2, false, 0.028),
        (4, false, 0.043),
    ] {
        let (got, _) = kin_turns(&mut h, &ids, k, first, 20_000);
        let room = if first { 0.015 } else { 0.006 };
        assert!(
            (got - want).abs() < room,
            "{k} kin near, first turn {first}: {got}"
        );
    }
    let (_, on) = kin_turns(&mut h, &ids, 4, true, 20_000);
    let all: u32 = on.iter().sum();
    for n in &on[..4] {
        #[allow(clippy::cast_precision_loss)]
        let share = *n as f32 / all as f32;
        assert!((share - 0.25).abs() < 0.03, "drawn evenly: {on:?}");
    }
    assert_eq!(on[4], 0, "not one at rest");
    assert_eq!(missiles::kin_buff_chance(0, true), 0.0, "none with no kin");
}

/// A protector that buffed a kin at its last turn buffs that kin again at this one a little under
/// one time in two, while it is near.
/// Behaviour: server.creatures.protectors-buff-kin-by-how-many
#[test]
fn a_protector_buffs_the_kin_it_buffed_again_a_little_under_half_the_time() {
    let extra = [
        ("NPC_StartingSkill1", Prop::Word(CREATURE_ABSORB)),
        ("NPC_StartingSkill1Level", Prop::Int(20)),
        ("AI_AISubClass", Prop::Word(7)),
    ];
    let (mut h, _, _, one, two) = pair(&extra);
    let clock = h.clock();
    let mut again = 0;
    for _ in 0..20_000 {
        if let Some(c) = h
            .server
            .things
            .get_mut(one)
            .and_then(|t| t.creature.as_mut())
        {
            c.actions = 5;
            c.kin_helped = Some((CREATURE_ABSORB, two));
        }
        missiles::helps(&mut h.server, &mut h.net, one, clock);
        let helped = h
            .server
            .things
            .get(one)
            .and_then(|t| t.creature.as_ref())
            .and_then(|c| c.kin_helped);
        again += u32::from(helped.is_some_and(|(_, k)| k == two));
    }
    // The repeat, or else the kin buff's own chance with one kin near.
    let want = 0.43 + 0.57 * 0.016;
    #[allow(clippy::cast_precision_loss)]
    let got = again as f32 / 20_000.0;
    assert!((got - want).abs() < 0.015, "{got}");
}

/// The share of `turns` turns at which a creature with these buffs, those in `barred` barred,
/// buffs itself, at its first turn of a fight or a later one; and how many times each buff.
fn self_turns(
    h: &mut Harness,
    one: u64,
    barred: &[u32],
    first: bool,
    turns: u32,
) -> (f32, std::collections::BTreeMap<u32, u32>) {
    let clock = h.clock();
    let until = clock.local.0 + 60.0;
    let mut each = std::collections::BTreeMap::new();
    for _ in 0..turns {
        if let Some(c) = h
            .server
            .things
            .get_mut(one)
            .and_then(|t| t.creature.as_mut())
        {
            c.actions = if first { 0 } else { 5 };
            c.barred = barred.iter().map(|k| (*k, until)).collect();
        }
        missiles::helps(&mut h.server, &mut h.net, one, clock);
        let used = h
            .server
            .things
            .get(one)
            .and_then(|t| t.creature.as_ref())
            .and_then(|c| {
                c.barred
                    .iter()
                    .map(|(k, _)| *k)
                    .find(|k| !barred.contains(k))
            });
        if let Some(k) = used {
            *each.entry(k).or_insert(0) += 1;
        }
    }
    #[allow(clippy::cast_precision_loss)]
    let share = each.values().sum::<u32>() as f32 / turns as f32;
    (share, each)
}

/// A creature at full health buffs itself at a turn by one chance for how many of its buffs are
/// free: with one, 0.155 at its first turn of a fight and 0.084 at a later one (this server's fit:
/// 2017's 0.090); with two, 0.249 and 0.157, the buff drawn evenly; with two and one barred, as
/// with one.
/// Behaviour: server.creatures.buffs-itself-by-its-turn
/// Behaviour: server.creatures.buffs-itself-later-with-one-free-ours
#[test]
fn a_creature_buffs_itself_by_one_chance_for_how_many_of_its_buffs_are_free() {
    let one_buff = [
        ("NPC_StartingSkill1", Prop::Word(CREATURE_ABSORB)),
        ("NPC_StartingSkill1Level", Prop::Int(20)),
    ];
    let two_buffs = [
        ("NPC_StartingSkill1", Prop::Word(CREATURE_ABSORB)),
        ("NPC_StartingSkill1Level", Prop::Int(20)),
        ("NPC_StartingSkill2", Prop::Word(CREATURE_FORTIFY)),
        ("NPC_StartingSkill2Level", Prop::Int(20)),
    ];
    let (mut h, _, _, one, _) = pair(&one_buff);
    for (first, want) in [(true, 0.155), (false, 0.084)] {
        let (got, _) = self_turns(&mut h, one, &[], first, 20_000);
        assert!(
            (got - want).abs() < 0.01,
            "one buff, first turn {first}: {got}"
        );
    }
    let (mut h, _, _, one, _) = pair(&two_buffs);
    for (first, want) in [(true, 0.249), (false, 0.157)] {
        let (got, each) = self_turns(&mut h, one, &[], first, 20_000);
        assert!(
            (got - want).abs() < 0.012,
            "two buffs, first turn {first}: {got}"
        );
        let (a, b) = (each[&CREATURE_ABSORB], each[&CREATURE_FORTIFY]);
        #[allow(clippy::cast_precision_loss)]
        let half = a as f32 / (a + b) as f32;
        assert!((half - 0.5).abs() < 0.04, "drawn evenly: {each:?}");
    }
    let (got, each) = self_turns(&mut h, one, &[CREATURE_ABSORB], false, 20_000);
    assert!((got - 0.084).abs() < 0.01, "one of two barred: {got}");
    assert!(!each.contains_key(&CREATURE_ABSORB), "not the barred one");
}

/// Set a creature's greatest vigor and its vigor now.
fn vigor(h: &mut Harness, id: u64, now: i32, most: i32) {
    if let Some(t) = h.server.things.get_mut(id) {
        t.max_vigor = most;
        t.vigor = now;
    }
}

/// A creature under two fifths of its vigor (the warning level where its template names none) with
/// a restore free restores itself about one time in two, the turn its restore's alone; at two fifths
/// or above it never does, and one holding a buff besides buffs as one with the buff alone. The
/// restore puts its vigor back.
/// Behaviour: server.creatures.restores-its-vigor-when-short
#[test]
fn a_creature_short_of_vigor_restores_it_about_one_time_in_two() {
    let with_buff = [
        ("NPC_StartingSkill1", Prop::Word(CREATURE_ABSORB)),
        ("NPC_StartingSkill1Level", Prop::Int(20)),
        ("NPC_StartingSkill2", Prop::Word(CREATURE_RESTORE_VIGOR)),
        ("NPC_StartingSkill2Level", Prop::Int(20)),
    ];
    let (mut h, _, _, one, _) = pair(&with_buff);
    for at in [1000, 400] {
        vigor(&mut h, one, at, 1000);
        let (got, each) = self_turns(&mut h, one, &[], false, 20_000);
        assert!(
            (got - 0.084).abs() < 0.01,
            "{at} of 1000: as the buff alone: {got}"
        );
        assert!(
            !each.contains_key(&CREATURE_RESTORE_VIGOR),
            "{at}: {each:?}"
        );
    }
    vigor(&mut h, one, 399, 1000);
    let (got, each) = self_turns(&mut h, one, &[], false, 20_000);
    assert!((got - 0.5).abs() < 0.02, "short of vigor: {got}");
    assert!(
        !each.contains_key(&CREATURE_ABSORB),
        "its restore's turn: {each:?}"
    );
    // Used, its vigor comes back.
    vigor(&mut h, one, 100, 1000);
    let clock = h.clock();
    let mut used = false;
    for _ in 0..200 {
        if missiles::helps(&mut h.server, &mut h.net, one, clock) {
            used = true;
            break;
        }
    }
    assert!(used, "it restores itself");
    h.run(30);
    let after = h.server.things.get(one).map_or(0, |t| t.vigor);
    assert!(after > 100, "its vigor back: {after}");
}

/// What a creature's special costs in vigor: its skill's share of its greatest vigor, reckoned in
/// single precision and cut to a whole number (37,500 x 0.01 is 375), less the skill's add; one it
/// lacks the vigor for is never used, and one used takes its cost from its vigor.
/// Behaviour: server.creatures.specials-cost-vigor
#[test]
fn a_creatures_special_costs_its_vigor_and_one_it_cannot_pay_for_is_not_used() {
    const COSTLY: u32 = 905;
    let (mut h, _, _, id) = noticed(&[], [61.0, 240.2, -44.6]);
    h.server.skill_table.skills.insert(
        COSTLY,
        yalain_world::skills::SkillDef {
            id: COSTLY,
            usage: Some(yalain_world::skills::Usage {
                vigor_mult: 0.01,
                vigor_add: -5.0,
                ..yalain_world::skills::Usage::default()
            }),
            ..yalain_world::skills::SkillDef::default()
        },
    );
    vigor(&mut h, id, 37_500, 37_500);
    assert_eq!(missiles::special_cost(&h.server, id, COSTLY), 380);
    let special = [AttackSkill {
        skill: COSTLY,
        special_chance: 0.0,
        barred_after: false,
        max_range: 40.0,
        ..CREATURE_ATTACKS[3]
    }];
    let now = h.now.0;
    vigor(&mut h, id, 379, 37_500);
    let used = (0..2_000)
        .filter(|_| missiles::pick_special(&mut h.server, id, &special, 5.0, now).is_some())
        .count();
    assert_eq!(used, 0, "short of its cost");
    vigor(&mut h, id, 380, 37_500);
    let used = (0..2_000)
        .find(|_| missiles::pick_special(&mut h.server, id, &special, 5.0, now).is_some());
    assert!(used.is_some(), "with its cost, it is used");
    assert_eq!(
        h.server.things.get(id).map(|t| t.vigor),
        Some(0),
        "its cost spent"
    );
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
    // Held at a quarter until it is healed: what it regains by itself would otherwise lift it out
    // of reach of a heal before the healer's turn comes.
    let mut healed = false;
    for _ in 0..200 {
        set(&mut h, two, 120);
        h.run(20);
        if !helps_on(&h, b, one, two, CREATURE_HEAL).is_empty() {
            healed = true;
            break;
        }
    }
    assert!(healed, "it heals its kin");
    h.run(30);
    let health = h.server.things.get(two).map_or(0, |t| t.health);
    assert!(health > 300, "healed: {health}");
}

/// Whether a healer of a behavior table heals its kin held at a quarter of its health, within 200
/// turns of 20 steps, the world's rows saying whether the table heals kin.
fn heals_hurt_kin(table: &str, heals_kin: bool) -> bool {
    let extra = [
        ("NPC_StartingSkill1", Prop::Word(CREATURE_HEAL)),
        ("NPC_StartingSkill1Level", Prop::Int(50)),
        ("AI_NPCTable", Prop::Text(table.into())),
    ];
    let (mut h, b, _, one, two) = pair(&extra);
    h.server
        .creature_rows
        .heals_kin
        .insert(table.to_string(), heals_kin);
    if let Some(c) = h
        .server
        .things
        .get_mut(two)
        .and_then(|t| t.creature.as_mut())
    {
        c.barred.push((CREATURE_HEAL, f64::MAX));
    }
    for _ in 0..200 {
        if let Some(t) = h.server.things.get_mut(two) {
            t.health = 120;
        }
        h.run(20);
        if !helps_on(&h, b, one, two, CREATURE_HEAL).is_empty() {
            return true;
        }
    }
    false
}

/// The world's behavior table rows decide which healers heal their kin: a table its row names a
/// healer's does, and a healer's table its row says doesn't, doesn't.
/// Behaviour: server.creatures.behavior-as-rows-say
#[test]
fn a_behavior_table_row_decides_whose_healers_heal_their_kin() {
    assert!(
        heals_hurt_kin("NPCTable_RiverHealers", true),
        "its row: heals"
    );
    assert!(
        !heals_hurt_kin("NPCTable_MimbuHealer", false),
        "its row: doesn't"
    );
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
        ends_at_logout: false,
        ends_at_teleport: false,
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
    let count = |h: &Harness| {
        let (mut landed, mut evaded, mut answers) = (0, 0, 0);
        for m in &h.world_messages(b, from) {
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
        (landed, evaded, answers)
    };
    // Strikes until more than ten have landed and one has been evaded: about one in three lands,
    // so 50 s holds about ten, and ten minutes leave no dice a real chance of fewer.
    for _ in 0..60 {
        h.run(200);
        let (landed, evaded, _) = count(&h);
        if landed > 10 && evaded > 0 {
            break;
        }
    }
    let (landed, evaded, answers) = count(&h);
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
    // Its first tick comes as much as 5.6 s after it is put on (`effects::TICK_FIRST`): it lasts
    // beyond that.
    h.server.effect_table.effects.insert(
        BURN,
        EffectDef {
            duration: Scale(vec![(0.0, 12.0)]),
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
    let health = |h: &Harness| {
        h.server.players.get(&world).and_then(|p| {
            yalain_world::quals::int(&p.qualities, yalain_world::player::quality::HEALTH)
        })
    };
    assert!(h.run_until(130, |h| health(h) < Some(100_000)), "it burned");
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
            effects: Vec::new(),
            chance: Scale(vec![(0.0, 0.15)]),
        },
    );
    holds(
        &mut h,
        me,
        SURE,
        Kind::Ward {
            classes: vec![22],
            effects: Vec::new(),
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

/// A ward of a world's row keeps off the effects of its classes and the effects it names by its
/// chance, and nothing else of theirs, not even another effect of a named one's class.
/// Behaviour: server.effects.ward-from-a-row
#[test]
fn a_ward_from_a_row_keeps_off_its_classes_and_effects_by_its_chance() {
    use std::collections::BTreeMap;
    use yalain_world::effect_defs::{ward_kind, Ward};
    let (mut h, _, me, _) = noticed(&[], [61.0, 240.2, -44.6]);
    hardy(&mut h, me);
    let world = conn(&h, me);
    const WARD: u32 = 0x6F00_7309;
    const SURE: u32 = 0x6F00_730A;
    const OF_ITS_CLASS: u32 = 0x6F00_730B;
    const NAMED: u32 = 0x6F00_730C;
    const NAMED_SURE: u32 = 0x6F00_730D;
    const OF_NO_CLASS: u32 = 0x6F00_730E;
    let rows = BTreeMap::from([
        (
            WARD,
            Ward {
                classes: vec![93],
                effects: vec![NAMED],
                chance: 0.15,
            },
        ),
        (
            SURE,
            Ward {
                classes: Vec::new(),
                effects: vec![NAMED_SURE],
                chance: 1.0,
            },
        ),
    ]);
    let wards = rows;
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
        harm(OF_ITS_CLASS, 93),
        harm(NAMED, 0),
        harm(NAMED_SURE, 0),
        harm(OF_NO_CLASS, 0),
    ] {
        h.server.effect_table.effects.insert(d.state, d);
    }
    for (state, w) in &wards {
        holds(&mut h, me, *state, ward_kind(w));
    }
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
    let through = count(NAMED, &mut h);
    assert!((3200..3600).contains(&through), "{through} of 4,000");
    assert_eq!(count(NAMED_SURE, &mut h), 0);
    assert_eq!(count(OF_NO_CLASS, &mut h), 4000);
}

/// The camp's creature beside where a character starts, the player's client having drawn it, and the
/// creature not yet looking about (it does so within the next two seconds); the client, the player
/// and the creature.
fn about_to_notice(extra: &[(&str, Prop)]) -> (Harness, usize, u64, u64) {
    let mut h = Harness::new();
    camp(&mut h, creature(20.0, extra), [61.0, 240.2, -44.6]);
    let (b, me) = h.player_in_world("one", "Elara");
    let id = h
        .server
        .things
        .by_id
        .values()
        .find(|t| t.template == TEMPLATE)
        .map(|t| t.id)
        .expect("the camp's creature");
    let later = h.now.0 + 2.0;
    if let Some(c) = h
        .server
        .things
        .get_mut(id)
        .and_then(|t| t.creature.as_mut())
    {
        c.next_look = later;
    }
    drawn(&mut h, b);
    assert_eq!(mode_of(&h, id), Some(Mode::Idle), "not yet noticed");
    (h, b, me, id)
}

fn playing_an_idle(h: &mut Harness, id: u64, seconds: f64) {
    let now = h.now.0;
    let c = h
        .server
        .things
        .get_mut(id)
        .and_then(|t| t.creature.as_mut())
        .expect("a creature");
    c.idle_until = now + seconds;
}

/// A creature playing an idle as a player comes within its perception goes after the player only
/// once the idle is through, and then at once; one playing none goes after it within its look about.
/// Behaviour: server.creatures.an-idle-holds-a-fight
#[test]
fn a_creature_noticing_a_player_while_it_plays_an_idle_goes_after_it_once_the_idle_is_through() {
    for idle in [None, Some(4.5)] {
        let (mut h, _b, me, id) = about_to_notice(&[]);
        if let Some(seconds) = idle {
            playing_an_idle(&mut h, id, seconds);
        }
        let from = h.now.0;
        assert!(h.run_until(200, |h| mode_of(h, id) == Some(Mode::Chasing)));
        let took = h.now.0 - from;
        assert_eq!(target_of(&h, id), Some(me));
        match idle {
            None => assert!(took < 2.0, "within its look about: {took}"),
            Some(seconds) => assert!(
                took >= seconds && took < seconds + 0.6,
                "once the idle is through: {took}"
            ),
        }
    }
}

/// A creature struck while it plays an idle goes after its striker once the idle is through, not
/// before.
/// Behaviour: server.creatures.an-idle-holds-a-fight
#[test]
fn a_creature_struck_while_it_plays_an_idle_goes_after_its_striker_once_the_idle_is_through() {
    let (mut h, _b, me, id) = about_to_notice(&[]);
    playing_an_idle(&mut h, id, 3.0);
    let clock = h.clock();
    yalain_world::ai::go_after(&mut h.server, &mut h.net, id, me, clock);
    h.run(steps(2.5));
    assert_eq!(mode_of(&h, id), Some(Mode::Idle), "the idle still plays");
    h.run(steps(1.0));
    assert_eq!(mode_of(&h, id), Some(Mode::Chasing));
    assert_eq!(target_of(&h, id), Some(me));
}

/// How many of the harness's steps make `seconds` (rounded up).
fn steps(seconds: f64) -> usize {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let n = (seconds / yalain_testkit::STEP).ceil() as usize;
    n
}

/// The idle behaviors a creature played that a client was told of since `from`.
fn idles_of(h: &Harness, b: usize, from: usize, id: u64) -> usize {
    h.world_messages(b, from)
        .iter()
        .filter(|m| {
            matches!(m, ServerMessage::PlayBehavior(e) if e.sender.id == id
                && yalain_world::ai::IDLE_BEHAVIORS.contains(&e.action.behavior))
        })
        .count()
}

/// A creature of a class the world gives a chance of an idle on noticing plays one as it notices a
/// player, every time at a chance of 1; with no such row, or a row for another class, none.
/// Behaviour: server.creatures.notice-idle-by-class
#[test]
fn a_creature_of_a_class_with_a_notice_idle_plays_an_idle_as_it_notices_a_player() {
    for (row, played) in [(Some(6), 1), (Some(0x1D), 0), (None, 0)] {
        let (mut h, b, _me, id) = about_to_notice(&[("AI_AISuperClass", Prop::Word(6))]);
        if let Some(class) = row {
            h.server.creature_rows.notice_idle.insert(class, 1.0);
        }
        // No resting idle comes due meanwhile.
        if let Some(c) = h
            .server
            .things
            .get_mut(id)
            .and_then(|t| t.creature.as_mut())
        {
            c.next_idle = f64::INFINITY;
        }
        let from = h.bots[b].inbox.len();
        assert!(h.run_until(200, |h| mode_of(h, id) == Some(Mode::Chasing)));
        h.run(2);
        assert_eq!(idles_of(&h, b, from, id), played, "row {row:?}");
    }
}

/// The creature's last blow, when it may set off home, and its mode.
fn going_home(h: &Harness, id: u64) -> (f64, f64, Mode) {
    h.server
        .things
        .get(id)
        .and_then(|t| t.creature.as_ref())
        .map(|c| (c.last_blow, c.home_after, c.mode))
        .expect("a creature")
}

fn place(h: &Harness, id: u64) -> [f32; 3] {
    h.server
        .things
        .get(id)
        .and_then(|t| t.at)
        .expect("in the world")
        .origin
}

/// A creature that fought a player and gives up when the player goes beyond its perception stands
/// where the fight left it for its drawn wait after its last blow, cut at eleven seconds (the
/// ninetieth percentile, this server's choice), and only then runs home.
/// Behaviour: server.creatures.a-pause-before-going-home
#[test]
fn a_creature_giving_up_stands_its_wait_after_its_last_blow_then_runs_home() {
    let (mut h, _b, me, id) = noticed(&[], [61.0, 240.2, -44.6]);
    hardy(&mut h, me);
    h.run(60);
    let home = h
        .server
        .things
        .get(id)
        .and_then(|t| t.creature.as_ref())
        .map(|c| c.home)
        .expect("a creature");
    stand_off(&mut h, me, home, 80.0);
    assert!(h.run_until(200, |h| going_home(h, id).2 == Mode::Returning));
    let (blow, after, _) = going_home(&h, id);
    let now = h.now.0;
    assert!(blow.is_finite(), "it came to blows");
    let wait = after - blow;
    assert!(
        wait >= 0.0 && wait <= ai::HOME_AFTER_A_BLOW[18].max(now - blow) + 1e-9,
        "its wait after its last blow: {wait}"
    );
    let stood = place(&h, id);
    while h.now.0 + yalain_testkit::STEP < after {
        h.run(1);
        assert_eq!(
            place(&h, id),
            stood,
            "it stands until {after} ({})",
            h.now.0
        );
    }
    assert!(
        h.run_until(20, |h| place(h, id) != stood),
        "then it sets off home"
    );
}

/// A creature's wait is kept from its last blow, not from when it gives up: one whose last blow is
/// twenty seconds past sets off at once.
/// Behaviour: server.creatures.a-pause-before-going-home
#[test]
fn a_creature_whose_last_blow_is_long_past_sets_off_home_as_it_gives_up() {
    let (mut h, _b, me, id) = noticed(&[], [61.0, 240.2, -44.6]);
    hardy(&mut h, me);
    let home = h
        .server
        .things
        .get(id)
        .and_then(|t| t.creature.as_ref())
        .map(|c| c.home)
        .expect("a creature");
    stand_off(&mut h, me, home, 80.0);
    let past = h.now.0 - 20.0;
    // No blow falls from here on: the player is far off.
    if let Some(c) = h
        .server
        .things
        .get_mut(id)
        .and_then(|t| t.creature.as_mut())
    {
        c.last_blow = past;
    }
    assert!(h.run_until(200, |h| going_home(h, id).2 == Mode::Returning));
    let (blow, after, _) = going_home(&h, id);
    assert!((blow - past).abs() < 1e-9, "no blow since");
    assert!(
        after <= h.now.0 + 1e-9,
        "it sets off at once: due {} s on",
        after - h.now.0
    );
}

/// A creature that went after a player and never came to blows stands its wait after its peace,
/// cut at about a second and a half (the ninetieth percentile, this server's choice), then runs
/// home.
/// Behaviour: server.creatures.a-pause-before-going-home
#[test]
fn a_creature_that_never_came_to_blows_stands_its_wait_after_its_peace() {
    let (mut h, _b, me, id) = noticed(&[], [61.0, 240.2, -44.6]);
    let home = h
        .server
        .things
        .get(id)
        .and_then(|t| t.creature.as_ref())
        .map(|c| c.home)
        .expect("a creature");
    stand_off(&mut h, me, home, 80.0);
    if let Some(c) = h
        .server
        .things
        .get_mut(id)
        .and_then(|t| t.creature.as_mut())
    {
        c.last_blow = f64::NEG_INFINITY;
    }
    assert!(h.run_until(200, |h| going_home(h, id).2 == Mode::Returning));
    let (blow, after, _) = going_home(&h, id);
    assert!(blow.is_infinite(), "no blow");
    let wait = after - h.now.0;
    assert!(
        (0.0..=ai::HOME_AFTER_A_CHASE[18] + 1e-9).contains(&wait),
        "its wait after its peace: {wait}"
    );
}

/// The waits' median and quartiles are 2017's: after a blow about 2.2 s, and about 1.1 and 4.6 s;
/// after a chase about 0.95 s, and about 0.69 and 1.17 s. Each wait is cut at the ninetieth
/// percentile, 11 s and 1.58 s, this server's choice, the longer stands left out.
/// Behaviour: server.creatures.a-pause-before-going-home
#[test]
fn the_waits_before_going_home_have_2017s_median_and_quartiles_and_are_cut_at_the_ninetieth() {
    let mut rng = yalain_world::rng::Rng::new(0x1153);
    let quantiles = |rng: &mut yalain_world::rng::Rng, table: &[f64; 19]| {
        let mut v: Vec<f64> = (0..8000).map(|_| ai::home_wait(rng, table)).collect();
        v.sort_by(f64::total_cmp);
        (v[2000], v[4000], v[6000], v[7999])
    };
    let (q1, m, q3, top) = quantiles(&mut rng, &ai::HOME_AFTER_A_BLOW);
    assert!((m - 2.22).abs() < 0.15, "median {m}");
    assert!(
        (q1 - 1.13).abs() < 0.15 && (q3 - 4.57).abs() < 0.3,
        "quartiles {q1} {q3}"
    );
    assert!((top - 11.0).abs() < 1e-9, "cut at 11 s: {top}");
    let (q1, m, q3, top) = quantiles(&mut rng, &ai::HOME_AFTER_A_CHASE);
    assert!((m - 0.95).abs() < 0.06, "median {m}");
    assert!(
        (q1 - 0.69).abs() < 0.08 && (q3 - 1.17).abs() < 0.08,
        "quartiles {q1} {q3}"
    );
    assert!((top - 1.58).abs() < 1e-9, "cut at 1.58 s: {top}");
}

/// A fight an idle held, given up, still ends in the creature's peace, its pause and its run home:
/// struck while it plays, its pause is kept from the blow; drawn in by noticing, from its peace.
/// Home again, an idle holds a new fight as before.
/// Behaviour: server.creatures.a-pause-before-going-home
#[test]
fn a_fight_an_idle_held_given_up_pauses_then_runs_home_and_an_idle_holds_the_next() {
    for struck in [true, false] {
        let (mut h, _b, me, id) = about_to_notice(&[]);
        hardy(&mut h, me);
        let home = h
            .server
            .things
            .get(id)
            .and_then(|t| t.creature.as_ref())
            .map(|c| c.home)
            .expect("a creature");
        playing_an_idle(&mut h, id, 3.0);
        let blow = h.now.0;
        if struck {
            let clock = h.clock();
            ai::go_after(&mut h.server, &mut h.net, id, me, clock);
        } else {
            assert!(h.run_until(steps(2.5), |h| h
                .server
                .things
                .get(id)
                .and_then(|t| t.creature.as_ref())
                .is_some_and(|c| c.after_idle == Some(me))));
        }
        assert_eq!(
            mode_of(&h, id),
            Some(Mode::Idle),
            "{struck}: the idle holds the fight"
        );
        stand_off(&mut h, me, home, 80.0);
        assert!(h.run_until(steps(4.0), |h| mode_of(h, id) == Some(Mode::Chasing)));
        assert!(h.run_until(200, |h| mode_of(h, id) == Some(Mode::Returning)));
        let (last, after, _) = going_home(&h, id);
        let now = h.now.0;
        if struck {
            assert!(
                (last - blow).abs() < 1e-9,
                "the blow in its idle counts: {last}"
            );
            assert!(
                after >= now - yalain_testkit::STEP
                    && after <= (blow + ai::HOME_AFTER_A_BLOW[18]).max(now) + 1e-9,
                "its wait after the blow: {}",
                after - now
            );
        } else {
            assert!(last.is_infinite(), "no blow: {last}");
            assert!(
                after > now - yalain_testkit::STEP
                    && after <= now + ai::HOME_AFTER_A_CHASE[18] + 1e-9,
                "its wait after its peace: {}",
                after - now
            );
        }
        let stood = place(&h, id);
        while h.now.0 + yalain_testkit::STEP < after {
            h.run(1);
            assert_eq!(place(&h, id), stood, "{struck}: it stands until {after}");
        }
        assert!(
            h.run_until(steps(60.0), |h| mode_of(h, id) == Some(Mode::Idle)),
            "{struck}: then it runs home"
        );
        stand_off(&mut h, me, home, 5.0);
        playing_an_idle(&mut h, id, 3.0);
        let clock = h.clock();
        ai::go_after(&mut h.server, &mut h.net, id, me, clock);
        h.run(steps(2.5));
        assert_eq!(
            mode_of(&h, id),
            Some(Mode::Idle),
            "{struck}: the next idle holds"
        );
        h.run(steps(1.0));
        assert_eq!(
            mode_of(&h, id),
            Some(Mode::Chasing),
            "{struck}: then it fights"
        );
        assert_eq!(target_of(&h, id), Some(me));
    }
}

/// A detaunt as the install defines Bluff's: 10% to 15% of the creature's whole health below
/// nothing, along its spellcraft from 1 to 50.
const BLUFF: u32 = 0x7000_07E3;

/// A creature that has fought `me` a while, the only one to fight it, with Bluff's detaunt known
/// and the world's chance of its dropping all threat (none: no row).
fn bluffed(chance: Option<f32>) -> (Harness, u64, u64) {
    let (mut h, _b, me, id) = noticed(&[], [61.0, 240.2, -44.6]);
    hardy(&mut h, me);
    h.run(40);
    h.server.effect_table.effects.insert(
        BLUFF,
        effect_of(BLUFF, Kind::Taunt(Scale(vec![(1.0, -0.10), (50.0, -0.15)]))),
    );
    if let Some(c) = chance {
        h.server.effect_table.drops_threat.insert(BLUFF, c);
    }
    // Bluff lands, and its blow follows it in the same instant.
    let clock = h.clock();
    let cast = Cast {
        caster: me,
        spellcraft: 20.0,
        ..Cast::default()
    };
    assert!(effects::apply_on(
        &mut h.server,
        &mut h.net,
        On::Creature(id),
        BLUFF,
        cast,
        clock
    ));
    ai::go_after(&mut h.server, &mut h.net, id, me, clock);
    (h, me, id)
}

/// A detaunt the world gives a chance of dropping all threat, succeeding, makes a creature whose
/// only foe its caster was forget the caster's harm and give up: it pauses, goes home, and doesn't
/// take the caster on again standing beside it, until the caster strikes it again.
/// Behaviour: server.creatures.a-detaunt-may-drop-all-threat
#[test]
fn a_bluff_that_drops_all_threat_makes_a_creature_give_up_and_fight_again_only_when_struck() {
    let (mut h, me, id) = bluffed(Some(1.0));
    let c = h
        .server
        .things
        .get(id)
        .and_then(|t| t.creature.as_ref())
        .expect("a creature");
    assert_eq!(c.mode, Mode::Returning, "it gives up");
    assert!(
        c.harm.iter().all(|e| e.1 != me),
        "the caster's harm forgotten"
    );
    assert!(
        c.taunts.iter().all(|e| e.1 != me),
        "and no taunt of its counts"
    );
    assert!(c.home_after >= h.now.0, "it pauses first");
    assert!(
        h.run_until(steps(40.0), |h| mode_of(h, id) == Some(Mode::Idle)),
        "it goes home"
    );
    h.run(steps(10.0));
    assert_eq!(mode_of(&h, id), Some(Mode::Idle), "it leaves the caster be");
    let clock = h.clock();
    ai::go_after(&mut h.server, &mut h.net, id, me, clock);
    assert_eq!(
        mode_of(&h, id),
        Some(Mode::Chasing),
        "struck again, it fights"
    );
    assert_eq!(target_of(&h, id), Some(me));
}

/// A detaunt whose chance of dropping all threat fails, or one the world gives no such chance,
/// only takes from its caster's weight: the creature fights on.
/// Behaviour: server.creatures.a-detaunt-may-drop-all-threat
#[test]
fn a_bluff_that_fails_to_drop_threat_or_has_no_chance_only_takes_from_the_weight() {
    for chance in [Some(0.0), None] {
        let (h, me, id) = bluffed(chance);
        let c = h
            .server
            .things
            .get(id)
            .and_then(|t| t.creature.as_ref())
            .expect("a creature");
        assert_eq!(c.mode, Mode::Chasing, "{chance:?}: it fights on");
        assert_eq!(c.target, Some(me));
        assert!(
            c.taunts.iter().any(|e| e.1 == me && e.2 < 0),
            "{chance:?}: the weight taken"
        );
    }
}

/// A creature that lost interest in a player neither notices the player beside it nor joins its
/// kin's fight with the player; once the player has left its perception and come back, it notices
/// the player again.
/// Behaviour: server.creatures.a-detaunt-may-drop-all-threat
#[test]
fn a_creature_that_lost_interest_neither_notices_its_caster_nor_joins_its_kin_against_it() {
    let (mut h, me, id) = bluffed(Some(1.0));
    assert!(h.run_until(steps(40.0), |h| mode_of(h, id) == Some(Mode::Idle)));
    let home = h
        .server
        .things
        .get(id)
        .and_then(|t| t.creature.as_ref())
        .map(|c| c.home)
        .expect("a creature");
    stand_off(&mut h, me, home, 3.0);
    let now = h.now.0;
    if let Some(c) = h
        .server
        .things
        .get_mut(id)
        .and_then(|t| t.creature.as_mut())
    {
        c.joining = Some((me, now + 1.0));
    }
    h.run(steps(5.0));
    assert_eq!(mode_of(&h, id), Some(Mode::Idle), "it lets the caster be");
    stand_off(&mut h, me, home, 80.0);
    h.run(steps(2.0));
    stand_off(&mut h, me, home, 3.0);
    assert!(
        h.run_until(steps(5.0), |h| mode_of(h, id) == Some(Mode::Chasing)),
        "back again, the caster is noticed"
    );
    assert_eq!(target_of(&h, id), Some(me));
}

/// A detaunt that drops all threat on a creature whose fight with its caster an idle holds drops
/// that fight too: once the idle is through, it lets the caster be.
/// Behaviour: server.creatures.a-detaunt-may-drop-all-threat
#[test]
fn a_bluff_that_drops_all_threat_during_an_idle_drops_the_fight_the_idle_held() {
    let (mut h, _b, me, id) = about_to_notice(&[]);
    hardy(&mut h, me);
    h.server.effect_table.effects.insert(
        BLUFF,
        effect_of(BLUFF, Kind::Taunt(Scale(vec![(1.0, -0.10), (50.0, -0.15)]))),
    );
    h.server.effect_table.drops_threat.insert(BLUFF, 1.0);
    playing_an_idle(&mut h, id, 3.0);
    let clock = h.clock();
    ai::go_after(&mut h.server, &mut h.net, id, me, clock);
    h.run(steps(1.0));
    let clock = h.clock();
    let cast = Cast {
        caster: me,
        spellcraft: 20.0,
        ..Cast::default()
    };
    assert!(effects::apply_on(
        &mut h.server,
        &mut h.net,
        On::Creature(id),
        BLUFF,
        cast,
        clock
    ));
    ai::go_after(&mut h.server, &mut h.net, id, me, clock);
    h.run(steps(5.0));
    assert_eq!(mode_of(&h, id), Some(Mode::Idle), "it lets the caster be");
}

/// Another player in the world, drawn its scene; its character.
fn another_player(h: &mut Harness, account: &str, name: &str) -> u64 {
    let (b, who) = h.player_in_world(account, name);
    drawn(h, b);
    who
}

/// Put players `east` meters east of where the creature stands, with no step between: the
/// creature's place and each player's, as set.
fn put_east(h: &mut Harness, id: u64, who: &[(u64, f32)]) -> yalain_store::StoredPosition {
    let at = h.server.things.get(id).and_then(|t| t.at).expect("placed");
    for (character, east) in who {
        let p = h
            .server
            .players
            .values_mut()
            .find(|p| p.character == *character)
            .expect("in the world");
        p.position.cell = at.cell;
        p.position.origin = [at.origin[0] + east, at.origin[1], at.origin[2]];
    }
    at
}

/// The creature fights `me` again, whatever it turned to as the others came.
fn fights_again(h: &mut Harness, id: u64, me: u64) {
    if let Some(c) = h
        .server
        .things
        .get_mut(id)
        .and_then(|t| t.creature.as_mut())
    {
        c.target = Some(me);
    }
}

/// The player `me` dies where it stands: its health gone, then its death.
fn dies(h: &mut Harness, me: u64) {
    let clock = h.clock();
    let w = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .map(|(c, _)| c)
        .expect("in the world");
    if let Some(p) = h.server.players.get_mut(&w) {
        yalain_world::quals::set_int(&mut p.qualities, yalain_world::player::quality::HEALTH, 0);
    }
    yalain_world::death::player_dies(&mut h.server, &mut h.net, w, None, clock);
}

/// A creature whose target dies takes on a living player within its perception radius, one that
/// never hurt it included, without giving up first; with no player within the radius it gives up
/// and goes home, and so does one that does not attack first with only players that never hurt it
/// near.
/// Behaviour: server.creatures.a-targets-death-takes-another-within-perception
#[test]
fn a_creature_whose_target_dies_takes_a_player_within_its_perception_else_goes_home() {
    let (mut h, _, me, id) = noticed(&[], [61.0, 240.2, -44.6]);
    let near = another_player(&mut h, "two", "Torvald");
    let far = another_player(&mut h, "three", "Brannoc");
    put_east(&mut h, id, &[(near, 6.0), (far, -30.0)]);
    fights_again(&mut h, id, me);
    dies(&mut h, me);
    let mut took = false;
    for _ in 0..200 {
        h.step();
        assert_ne!(mode_of(&h, id), Some(Mode::Returning), "it never gives up");
        if target_of(&h, id) == Some(near) {
            took = true;
            break;
        }
    }
    assert!(
        took,
        "it takes on the one within its radius, which never hurt it"
    );
    assert_eq!(mode_of(&h, id), Some(Mode::Chasing));

    let (mut h, _, me, id) = noticed(&[], [61.0, 240.2, -44.6]);
    let far = another_player(&mut h, "two", "Torvald");
    put_east(&mut h, id, &[(far, 30.0)]);
    fights_again(&mut h, id, me);
    dies(&mut h, me);
    assert!(
        h.run_until(200, |h| mode_of(h, id) == Some(Mode::Returning)),
        "with none within its radius it gives up"
    );
    // One that does not attack first takes on no one that never hurt it.
    let (mut h, _, me, id) = noticed(&[], [61.0, 240.2, -44.6]);
    let near = another_player(&mut h, "two", "Torvald");
    put_east(&mut h, id, &[(near, 6.0)]);
    if let Some(c) = h
        .server
        .things
        .get_mut(id)
        .and_then(|t| t.creature.as_mut())
    {
        c.stats.attacks_first = false;
    }
    fights_again(&mut h, id, me);
    dies(&mut h, me);
    assert!(
        h.run_until(200, |h| mode_of(h, id) == Some(Mode::Returning)),
        "one that waits to be struck gives up"
    );
}

/// Among several players within its perception, a creature whose target died takes one drawn by
/// nearness, each by one over its distance: one at 2 m four times as often as one at 8 m, one past
/// the radius never.
/// Behaviour: server.creatures.a-targets-death-draws-the-next-by-nearness
#[test]
fn after_its_targets_death_a_creature_draws_the_next_player_by_nearness() {
    let (mut h, _, me, id) = noticed(&[], [61.0, 240.2, -44.6]);
    let two = another_player(&mut h, "two", "Torvald");
    let eight = another_player(&mut h, "three", "Brannoc");
    let past = another_player(&mut h, "four", "Edda");
    let at = put_east(&mut h, id, &[(two, 2.0), (eight, 8.0), (past, 21.0)]);
    dies(&mut h, me);
    let perception = yalain_world::ai::DEFAULT_PERCEPTION;
    let mut taken = std::collections::HashMap::new();
    for _ in 0..20_000 {
        let p = yalain_world::ai::after_a_death(&mut h.server, id, &at, perception);
        *taken.entry(p).or_insert(0u32) += 1;
    }
    assert!(
        !taken.contains_key(&Some(past)),
        "never past the radius: {taken:?}"
    );
    assert!(!taken.contains_key(&None), "{taken:?}");
    #[allow(clippy::cast_precision_loss)]
    let share = taken[&Some(two)] as f32 / 20_000.0;
    assert!(
        (share - 0.8).abs() < 0.015,
        "one over the distance: {share}"
    );
    assert!(taken[&Some(eight)] > 0);
}

/// A pet of `owner`, alive, `east` meters east of the creature.
fn a_pet(h: &mut Harness, id: u64, owner: u64, east: f32) -> u64 {
    let creature = h.server.things.get(id).expect("the creature");
    let mut at = creature.at.expect("placed");
    let template = h
        .server
        .catalog
        .get(creature.template)
        .cloned()
        .expect("its template");
    at.origin[0] += east;
    let pet = h.server.things.new_id();
    let mut t =
        yalain_world::things::Thing::of(pet, &template, yalain_world::things::Kind::Creature);
    t.at = Some(at);
    t.health = 100;
    t.max_health = 100;
    h.server.things.insert(t);
    h.server.pets.by_id.insert(
        pet,
        yalain_world::pets::Pet {
            id: pet,
            owner,
            template: template.id,
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
        },
    );
    pet
}

/// Whom 20,000 draws after the target's death took, and how often.
fn drawn_after(
    h: &mut Harness,
    id: u64,
    at: &yalain_store::StoredPosition,
) -> std::collections::HashMap<Option<u64>, u32> {
    let mut taken = std::collections::HashMap::new();
    for _ in 0..20_000 {
        let p = ai::after_a_death(&mut h.server, id, at, ai::DEFAULT_PERCEPTION);
        *taken.entry(p).or_insert(0u32) += 1;
    }
    taken
}

/// Another player's pet within the radius joins the draw after the target's death, whether or not
/// it hurt the creature, weighing a quarter of a player at its distance: beside a player at 4 m, a
/// pet at 4 m is taken 0.27 in 1.27 times.
/// Behaviour: server.creatures.a-targets-death-draws-a-pet-at-a-quarter
/// Behaviour: server.creatures.a-targets-death-takes-a-pet-hurter-or-not
#[test]
fn after_its_targets_death_a_creature_draws_another_players_pet_at_a_quarter_of_a_player() {
    let (mut h, _, me, id) = noticed(&[], [61.0, 240.2, -44.6]);
    let near = another_player(&mut h, "two", "Torvald");
    let owner = another_player(&mut h, "three", "Brannoc");
    let at = put_east(&mut h, id, &[(near, 4.0), (owner, 30.0)]);
    let pet = a_pet(&mut h, id, owner, -4.0);
    dies(&mut h, me);
    let taken = drawn_after(&mut h, id, &at);
    #[allow(clippy::cast_precision_loss)]
    let share = taken.get(&Some(pet)).copied().unwrap_or(0) as f32 / 20_000.0;
    assert!(
        (share - 0.27 / 1.27).abs() < 0.015,
        "a quarter of a player: {share} {taken:?}"
    );
    assert_eq!(
        taken.get(&Some(near)).copied().unwrap_or(0) + taken.get(&Some(pet)).copied().unwrap_or(0),
        20_000,
        "the player or the pet: {taken:?}"
    );
}

/// With no player within the radius a pet there keeps no creature whose target died: it takes no
/// one, however near the pet.
/// Behaviour: server.creatures.a-targets-death-takes-a-pet-only-beside-a-player
#[test]
fn after_its_targets_death_a_creature_takes_no_pet_with_no_player_within_its_radius() {
    let (mut h, _, me, id) = noticed(&[], [61.0, 240.2, -44.6]);
    let owner = another_player(&mut h, "two", "Torvald");
    let at = put_east(&mut h, id, &[(owner, 30.0)]);
    a_pet(&mut h, id, owner, 2.0);
    dies(&mut h, me);
    let taken = drawn_after(&mut h, id, &at);
    assert_eq!(taken.get(&None), Some(&20_000), "no one: {taken:?}");
}

/// The dead player's own pet, still standing, is never drawn after its owner's death.
/// Behaviour: server.creatures.a-targets-death-draws-a-pet-at-a-quarter
#[test]
fn after_its_targets_death_a_creature_never_draws_the_dead_players_own_pet() {
    let (mut h, _, me, id) = noticed(&[], [61.0, 240.2, -44.6]);
    let other = another_player(&mut h, "two", "Torvald");
    let at = put_east(&mut h, id, &[(other, 8.0)]);
    dies(&mut h, me);
    let pet = a_pet(&mut h, id, me, 1.0);
    let taken = drawn_after(&mut h, id, &at);
    assert!(
        !taken.contains_key(&Some(pet)),
        "never the dead's pet: {taken:?}"
    );
    assert_eq!(taken.get(&Some(other)), Some(&20_000), "{taken:?}");
}

/// A creature's vigor and greatest vigor now.
fn vigor_of(h: &Harness, id: u64) -> (i32, i32) {
    h.server
        .things
        .get(id)
        .map(|t| (t.vigor, t.max_vigor))
        .expect("a creature")
}

/// A creature made has all its vigor; its template's combat share of vigor is the one it fights by,
/// its peaceful one no part of it.
/// Behaviour: server.creatures.start-with-all-their-vigor
/// Behaviour: server.creatures.regain-vigor-in-a-fight-by-their-combat-share
#[test]
fn a_creature_starts_with_all_its_vigor_and_fights_by_its_combat_share() {
    let (h, _b, _me, id) = noticed(
        &[("GameplayStatistics_MaxVigor", Prop::Int(6000))],
        [61.0, 240.2, -44.6],
    );
    assert_eq!(vigor_of(&h, id), (6000, 6000), "all of it as it is made");
    let s = stats(&[
        ("GameplayStatistics_CombatVigorRegenRate", Prop::Float(0.02)),
        ("GameplayStatistics_VigorRegenRate", Prop::Float(0.5)),
    ]);
    assert!(
        (s.combat_vigor_regen - 0.02).abs() < 1e-6,
        "{}",
        s.combat_vigor_regen
    );
    let s = stats(&[("GameplayStatistics_VigorRegenRate", Prop::Float(0.5))]);
    assert!(s.combat_vigor_regen == 0.0, "none where it sets none");
}

/// In a fight a creature regains its combat share of its greatest vigor a minute in steps every ten
/// seconds, hurt or not: 0.02 of 6,000 is 20 a step, none before the first; with no combat share it
/// regains nothing in a fight.
/// Behaviour: server.creatures.regain-vigor-in-a-fight-by-their-combat-share
/// Behaviour: server.creatures.vigor-in-a-fight-comes-in-steps
#[test]
fn a_fighting_creature_regains_its_combat_share_of_vigor_in_steps_of_ten_seconds() {
    for share in [0.02_f32, 0.0] {
        let (mut h, _b, me, id) = noticed(
            &[("GameplayStatistics_MaxVigor", Prop::Int(6000))],
            [61.0, 240.2, -44.6],
        );
        hardy(&mut h, me);
        if let Some(t) = h.server.things.get_mut(id) {
            t.vigor = 1000;
            if let Some(c) = t.creature.as_mut() {
                c.stats.combat_vigor_regen = share;
            }
        }
        h.run(1);
        let start = h.now.0;
        h.run(steps(9.5));
        assert_eq!(
            vigor_of(&h, id).0,
            1000,
            "{share}: none before the first step"
        );
        h.run_to(yalain_net::LocalTime(start + 31.0));
        assert_eq!(mode_of(&h, id), Some(Mode::Chasing), "still fighting");
        let want = if share > 0.0 { 1060 } else { 1000 };
        assert_eq!(vigor_of(&h, id).0, want, "{share}: three steps");
    }
}

/// Out of a fight a creature's vigor holds until eight seconds have gone with no blow by it or on
/// it, then is all back at once (at the next second's regaining).
/// Behaviour: server.creatures.vigor-full-again-after-a-fight
/// Behaviour: server.creatures.vigor-full-eight-seconds-after-the-last-blow
#[test]
fn a_creature_out_of_a_fight_has_all_its_vigor_again_eight_seconds_after_its_last_blow() {
    let (mut h, _b, me, id) = noticed(
        &[("GameplayStatistics_MaxVigor", Prop::Int(6000))],
        [61.0, 240.2, -44.6],
    );
    hardy(&mut h, me);
    let home = h
        .server
        .things
        .get(id)
        .and_then(|t| t.creature.as_ref())
        .map(|c| c.home)
        .expect("a creature");
    stand_off(&mut h, me, home, 80.0);
    assert!(h.run_until(200, |h| mode_of(h, id) == Some(Mode::Returning)));
    let now = h.now.0;
    if let Some(t) = h.server.things.get_mut(id) {
        t.vigor = 100;
        // Hurt two seconds ago, its last blow struck now.
        t.last_hurt = now - 2.0;
        if let Some(c) = t.creature.as_mut() {
            c.last_blow = now;
        }
    }
    h.run_to(yalain_net::LocalTime(now + 7.5));
    assert_eq!(vigor_of(&h, id).0, 100, "held");
    // Regaining comes at the world's next look at it after the eight seconds.
    h.run_to(yalain_net::LocalTime(now + 9.5));
    assert_eq!(vigor_of(&h, id), (6000, 6000), "all of it back");
}
