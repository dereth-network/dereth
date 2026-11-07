//! The weapons creatures hold and the stance they fight in: the stance is the held weapon's own
//! (its template's, else the world database's, else its kind's) for one weapon, a weapon and a
//! shield, or two; a template asking two of a sword may be given one, at the share the world
//! database says; drawing and putting the weapons away moves the drawn slider.
//! Fixture: creatures of templates made up for each test, in a camp beside where a character starts.

use auberean_protocol::{queue, ServerMessage};
use yalain_content::{Prop, Props, WeaponRule};
use yalain_testkit::{test_template, Harness};
use yalain_world::arms;
use yalain_world::things::Mode;

const TEMPLATE: u32 = 0x4700_2230;
/// The made-up templates of the weapons.
const STAFF: u32 = 0x4700_1EE9;
const SWORD: u32 = 0x4700_1F50;
const NEST: u32 = 0x4700_07FD;

const WIELDS: (&str, Prop) = ("AICombat_WieldImplements", Prop::Bool(true));

/// A creature template of level 2 that notices players within 20 m and holds these.
fn creature(extra: &[(&str, Prop)]) -> Props {
    let mut p = Props::default();
    p.set("GameplayStatistics_Level", Prop::Int(2))
        .set("GameplayStatistics_MaxHealth", Prop::Int(460))
        .set("AICombat_NaturalDamage", Prop::Float(1.0))
        .set("AICombat_NaturalVariance", Prop::Float(0.0))
        .set("AI_PerceptionRadius", Prop::Float(20.0));
    for (k, v) in extra {
        p.set(k, v.clone());
    }
    p
}

/// A harness with a camp of one creature of these properties beside where a character starts,
/// and the weapons' templates (each with its own properties) by name.
fn world(props: Props, weapons: &[(&str, u32, Props)]) -> Harness {
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    h.server
        .catalog
        .insert(test_template(TEMPLATE, 0x1169, props));
    for (name, t, p) in weapons {
        h.server
            .catalog
            .insert(test_template(*t, 0x1159, p.clone()));
        h.server.creature_weapons.insert((*name).into(), *t);
    }
    let spot = [61.0, 240.2, -44.6];
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
    h
}

/// A player enters beside the camp and is noticed; the client, the player and the creature.
fn noticed(h: &mut Harness) -> (usize, u64, u64) {
    let (b, me) = h.player_in_world("one", "Elara");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
    let id = h
        .world_messages(b, 0)
        .iter()
        .find_map(|m| match m {
            ServerMessage::CreateObject(o) if o.weenie.entity_did == TEMPLATE => Some(o.id),
            _ => None,
        })
        .expect("made for the player");
    assert!(
        h.run_until(200, |h| mode_of(h, id) == Some(Mode::Chasing)),
        "it goes after the player"
    );
    h.run(4);
    (b, me, id)
}

fn mode_of(h: &Harness, id: u64) -> Option<Mode> {
    h.server
        .things
        .get(id)
        .and_then(|t| t.creature.as_ref())
        .map(|c| c.mode)
}

fn held(h: &Harness, id: u64) -> Vec<u64> {
    h.server
        .things
        .get(id)
        .and_then(|t| t.creature.as_ref())
        .map(|c| c.weapons.clone())
        .unwrap_or_default()
}

/// The modes a client was told of a creature since a message.
fn modes(h: &Harness, b: usize, from: usize, id: u64) -> Vec<u32> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::DoMode(e) if e.sender.id == id => Some(e.action.0),
            _ => None,
        })
        .collect()
}

fn stance_props(single: u32) -> Props {
    let mut p = Props::default();
    p.set("Weapon_SingleWeaponStance", Prop::Word(single));
    p
}

/// A weapon's stances are its template's own where it sets them; else the world database's;
/// else its kind's (a stave held alone, a nest, a club alone, with a shield or two at once). A
/// creature goes after a player in the stance of the weapon it holds.
/// Behaviour: server.creatures.weapon-stance-is-the-held-weapons
#[test]
fn a_creatures_stance_is_its_weapons_own_else_the_databases_else_its_kinds() {
    let listed = WeaponRule {
        two_share: None,
        stance: Some(0x4100_0011),
    };
    assert_eq!(
        arms::stances("StaffGurogWarlockConjurer", None, Some(&listed)),
        [0x4100_0011; 3]
    );
    assert_eq!(
        arms::stances(
            "StaffMumiyahNomadic",
            Some(&stance_props(0x4100_000A)),
            Some(&listed)
        )[0],
        0x4100_000A,
        "the template's own first"
    );
    assert_eq!(
        arms::stances(
            "NestTumerokHivekeeperLeader",
            Some(&stance_props(0x4100_0013)),
            None
        )[0],
        0x4100_0013
    );
    assert_eq!(
        arms::stances("ClubBurunBrowerk", None, None),
        [0x4100_000A, 0x4100_000B, 0x4100_000E]
    );
    assert_eq!(
        arms::stances("PolearmJuggernautChaos", None, None),
        [0x4100_000C; 3]
    );

    let mut h = world(
        creature(&[
            (
                "AI_Inventory1",
                Prop::Text("StaffGurogWarlockConjurer".into()),
            ),
            WIELDS,
        ]),
        &[("StaffGurogWarlockConjurer", STAFF, Props::default())],
    );
    h.server
        .creature_weapon_rules
        .insert("StaffGurogWarlockConjurer".into(), listed);
    let (b, _, id) = noticed(&mut h);
    assert_eq!(held(&h, id).len(), 1);
    assert_eq!(
        modes(&h, b, 0, id),
        vec![0x4100_0011],
        "the stave's own stance"
    );

    let mut h = world(
        creature(&[
            (
                "AI_Inventory1",
                Prop::Text("NestTumerokHivekeeperLeader".into()),
            ),
            WIELDS,
        ]),
        &[(
            "NestTumerokHivekeeperLeader",
            NEST,
            stance_props(0x4100_0013),
        )],
    );
    let (b, _, id) = noticed(&mut h);
    assert_eq!(modes(&h, b, 0, id), vec![0x4100_0013], "the nest's");
}

/// A creature whose template asks two of a sword it wields two at once is given two at the share
/// the world database says for that sword, else one; it fights two-handed only with two.
/// Behaviour: server.creatures.weapon-pairs-by-share
#[test]
fn a_creature_asking_two_swords_is_given_two_at_the_listed_share_and_fights_as_it_holds() {
    let berserker = || {
        creature(&[
            ("AI_Inventory1", Prop::Text("SwordBerserkerChaos".into())),
            ("AI_Inventory1Quantity", Prop::Int(2)),
            ("AICombat_CanDualWield", Prop::Bool(true)),
            WIELDS,
        ])
    };
    for (share, count, stance) in [(0.0, 1, 0x4100_000A), (1.0, 2, 0x4100_000E)] {
        let mut h = world(
            berserker(),
            &[("SwordBerserkerChaos", SWORD, Props::default())],
        );
        h.server.creature_weapon_rules.insert(
            "SwordBerserkerChaos".into(),
            WeaponRule {
                two_share: Some(share),
                stance: None,
            },
        );
        let (b, _, id) = noticed(&mut h);
        assert_eq!(held(&h, id).len(), count, "share {share}");
        assert_eq!(modes(&h, b, 0, id), vec![stance], "share {share}");
    }
    // Unlisted, both are given.
    let mut h = world(
        berserker(),
        &[("SwordBerserkerChaos", SWORD, Props::default())],
    );
    let (_, _, id) = noticed(&mut h);
    assert_eq!(held(&h, id).len(), 2);
}

/// A creature drawing its weapons moves the drawn slider (to 0 over a tenth of a second) just
/// before its stance; putting them away, after letting go of them and before the monster's mode.
/// A creature holding nothing moves no slider.
/// Behaviour: server.creatures.weapon-drawn-slider
#[test]
fn a_creature_moves_the_drawn_slider_as_it_draws_and_puts_away_its_weapons() {
    let slid = |m: &ServerMessage, id: u64| {
        matches!(m, ServerMessage::MoveSlider(e) if e.sender.id == id
            && e.action.slider == 0x4000_001F && e.action.value == 0.0
            && (e.action.time - 0.1).abs() < 1e-6)
    };
    let mut h = world(
        creature(&[
            (
                "AI_Inventory1",
                Prop::Text("StaffGurogWarlockConjurer".into()),
            ),
            WIELDS,
        ]),
        &[("StaffGurogWarlockConjurer", STAFF, Props::default())],
    );
    let (b, me, id) = noticed(&mut h);
    let msgs = h.world_messages(b, 0);
    let slider = msgs.iter().position(|m| slid(m, id)).expect("the slider");
    let stance = msgs
        .iter()
        .position(|m| matches!(m, ServerMessage::DoMode(e) if e.sender.id == id))
        .expect("its stance");
    assert_eq!(slider + 1, stance, "just before its stance");
    // The player walks away: it gives up.
    if let Some(p) = h.server.players.values_mut().find(|p| p.character == me) {
        p.position.origin[0] -= 60.0;
    }
    let from = h.bots[b].inbox.len();
    assert!(h.run_until(200, |h| mode_of(h, id) == Some(Mode::Returning)));
    h.run(4);
    let msgs = h.world_messages(b, from);
    let let_go = msgs
        .iter()
        .position(|m| matches!(m, ServerMessage::DeParent(e) if e.sender.id == id))
        .expect("let go of");
    let slider = msgs.iter().position(|m| slid(m, id)).expect("the slider");
    let monster = msgs
        .iter()
        .position(|m| matches!(m, ServerMessage::DoMode(e) if e.sender.id == id && e.action.0 == 0x4100_0012))
        .expect("the monster's mode");
    assert!(let_go < slider && slider < monster);

    // Holding nothing, no slider.
    let mut h = world(creature(&[]), &[]);
    let (b, _, id) = noticed(&mut h);
    assert!(!h.world_messages(b, 0).iter().any(|m| slid(m, id)));
}

/// What a creature's attacks came out as on the player since a message, by the skill they used:
/// struck or arrived, each hook's result.
fn results_of(h: &Harness, b: usize, from: usize, id: u64, skill: u32) -> Vec<u32> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::DoStory(e) if e.action.attacker == id && e.action.skill == skill => {
                Some(e.action.hooks.clone())
            }
            _ => None,
        })
        .flatten()
        .map(|k| k.numbers[0].cast_unsigned())
        .filter(|r| *r != 0)
        .collect()
}

/// A creature's landed attack is critical at its skill's chance with what the weapon it holds adds,
/// its template's own: struck or loosed, a weapon that adds all of it makes every landed attack
/// critical whatever the skill, a skill that sets all of it does with a weapon that adds nothing,
/// and with neither none is.
/// Behaviour: server.combat.creature-critical-by-weapon
#[test]
fn a_creatures_weapon_adds_its_critical_chance_to_its_skills() {
    use yalain_world::missiles::{self, CREATURE_ATTACKS};
    use yalain_world::rules::Outcome;
    let (hit, critical) = (Outcome::Hit.code(), Outcome::Critical.code());
    // The weapon's addition, the skill's chance, loosed: the results of 30 attacks.
    let results = |adds: f32, chance: f64, loosed: bool| -> Vec<u32> {
        let mut sword = Props::default();
        sword.set("Weapon_CriticalHitMod", Prop::Float(adds));
        let mut h = world(
            creature(&[
                ("AI_Inventory1", Prop::Text("SwordBerserkerChaos".into())),
                WIELDS,
            ]),
            &[("SwordBerserkerChaos", SWORD, sword)],
        );
        let (b, me, id) = noticed(&mut h);
        assert_eq!(held(&h, id).len(), 1);
        let world = *h
            .server
            .players
            .iter()
            .find(|(_, p)| p.character == me)
            .expect("in the world")
            .0;
        // A strike and a missile of a skill the creature's own attacks never use.
        let skill = missiles::AttackSkill {
            critical: chance,
            ..CREATURE_ATTACKS[if loosed { 5 } else { 2 }]
        };
        let from = h.bots[b].inbox.len();
        for _ in 0..30 {
            if let Some(c) = h
                .server
                .things
                .get_mut(id)
                .and_then(|t| t.creature.as_mut())
            {
                c.next_attack = f64::MAX;
            }
            let clock = h.clock();
            if loosed {
                missiles::launch(&mut h.server, &mut h.net, id, world, skill, false, clock);
            } else {
                yalain_world::combat::creature_strike_with(
                    &mut h.server,
                    &mut h.net,
                    id,
                    world,
                    skill,
                    false,
                    clock,
                );
            }
            h.run(30);
        }
        let landed: Vec<u32> = results_of(&h, b, from, id, skill.skill)
            .into_iter()
            .filter(|r| *r == hit || *r == critical)
            .collect();
        assert!(landed.len() >= 5, "{adds} {chance} {loosed}: {landed:x?}");
        landed
    };
    for loosed in [false, true] {
        assert!(results(1.0, 0.0, loosed).iter().all(|r| *r == critical));
        assert!(results(0.0, 1.0, loosed).iter().all(|r| *r == critical));
        assert!(results(0.0, 0.0, loosed).iter().all(|r| *r == hit));
    }
}
