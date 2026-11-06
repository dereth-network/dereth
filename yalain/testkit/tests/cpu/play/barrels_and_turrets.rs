//! Creatures that never move and whose templates give no level: barrels, made by their camps,
//! that neither move nor fight; turrets, that turn where they stand and shoot, roused when a
//! creature near them goes after a player.
//! Fixture: made-up templates of a barrel, a turret and a guard in camps beside where a character
//! starts; the turret's missile is the test skill table's poisoned missile.

use auberean_protocol::{queue, ServerMessage};
use yalain_content::{Prop, Props};
use yalain_testkit::{test_template, Harness};
use yalain_world::things::Mode;

const BARREL: u32 = 0x4700_26C2;
const TURRET: u32 = 0x4700_2C40;
const GUARD: u32 = 0x4700_2794;
const MISSILE: u32 = 1863;

/// A template with no level that never moves: a barrel (`turns` false) or a turret.
fn unlevelled(turns: bool) -> Props {
    let mut p = Props::default();
    p.set("GameplayStatistics_MaxHealth", Prop::Int(1))
        .set("AICombat_NaturalDamage", Prop::Float(1.0))
        .set("AICombat_NaturalVariance", Prop::Float(0.3));
    if turns {
        p.set("AI_MovementType", Prop::Word(0x4000_0002))
            .set("AI_LoveTable", Prop::Word(0x23))
            .set("AI_PerceptionRadius", Prop::Float(500.0))
            .set("GameplayStatistics_MaxHealth", Prop::Int(500_000))
            .set("NPC_StartingSkill1", Prop::Word(MISSILE))
            .set("NPC_StartingSkill1Level", Prop::Int(50));
    } else {
        p.set("AI_MovementType", Prop::Word(0x4000_0001))
            .set("AI_LoveTable", Prop::Word(0x18));
    }
    p
}

fn guard() -> Props {
    let mut p = Props::default();
    p.set("GameplayStatistics_Level", Prop::Int(2))
        .set("GameplayStatistics_MaxHealth", Prop::Int(460))
        .set("AICombat_NaturalDamage", Prop::Float(1.0))
        .set("AICombat_NaturalVariance", Prop::Float(0.0))
        .set("AI_PerceptionRadius", Prop::Float(12.0));
    p
}

/// A camp of one creature of a template at a spot.
fn camp(h: &mut Harness, template: u32, props: Props, spot: [f32; 3]) {
    h.server
        .catalog
        .insert(test_template(template, 0x1169, props));
    h.server.spawns.camps.push(yalain_content::Camp {
        id: format!("{template:x}"),
        cell: 0x0299_0109,
        origin: spot,
        radius: 0.0,
        population: 1,
        regen: 600.0,
        templates: vec![(template, 1)],
        spots: vec![(0x0299_0109, spot)],
        placed: "authored for the tests".into(),
        confidence: "authored".into(),
        moves: yalain_content::CampMoves::default(),
    });
    h.server
        .camps
        .push(yalain_world::spawn::CampState::default());
}

fn enter(h: &mut Harness) -> (usize, u64) {
    let (b, me) = h.player_in_world("one", "Elara");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
    (b, me)
}

fn made(h: &Harness, b: usize, template: u32) -> u64 {
    h.world_messages(b, 0)
        .iter()
        .find_map(|m| match m {
            ServerMessage::CreateObject(o) if o.weenie.entity_did == template => Some(o.id),
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

fn place_of(h: &Harness, id: u64) -> [f32; 3] {
    h.server
        .things
        .get(id)
        .and_then(|t| t.at)
        .map(|a| a.origin)
        .expect("in the world")
}

/// A barrel, whose template gives no level and never moves, is made by its camp, and stays where
/// it is without ever going after a player beside it.
/// Behaviour: server.creatures.barrels-stand
#[test]
fn a_barrel_is_made_by_its_camp_and_never_moves_or_fights() {
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    camp(&mut h, BARREL, unlevelled(false), [61.0, 240.2, -44.6]);
    let (b, _) = enter(&mut h);
    let id = made(&h, b, BARREL);
    let at = place_of(&h, id);
    h.run(300);
    assert_eq!(mode_of(&h, id), Some(Mode::Idle));
    assert_eq!(place_of(&h, id), at);
}

/// A turret's shot comes when it is due, not at the world's next step: each gap is its rhythm's over
/// the shot's time scale, within the harness's step.
/// Behaviour: server.creatures.attacks-when-due
#[test]
fn a_turrets_shot_comes_when_it_is_due_not_at_the_worlds_next_step() {
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    camp(&mut h, TURRET, unlevelled(true), [84.06, 240.2, -44.6]);
    camp(&mut h, GUARD, guard(), [89.06, 240.2, -44.6]);
    let (b, me) = enter(&mut h);
    let turret = made(&h, b, TURRET);
    let guard_id = made(&h, b, GUARD);
    if let Some(p) = h.server.players.values_mut().find(|p| p.character == me) {
        p.position.origin[0] += 22.0;
        yalain_world::quals::set_int(
            &mut p.qualities,
            yalain_world::player::quality::HEALTH_MAX,
            100_000,
        );
        yalain_world::quals::set_int(
            &mut p.qualities,
            yalain_world::player::quality::HEALTH,
            100_000,
        );
    }
    assert!(h.run_until(200, |h| mode_of(h, guard_id) == Some(Mode::Chasing)));
    let from = h.bots[b].inbox.len();
    let gap = yalain_world::missiles::rhythm(TURRET).gap;
    let mut at: Vec<(f64, f32)> = Vec::new();
    let mut told = 0;
    for _ in 0..1000 {
        h.step();
        let now: Vec<f32> = h
            .world_messages(b, from)
            .iter()
            .filter_map(|m| match m {
                ServerMessage::DoStory(e)
                    if e.sender.id == turret
                        && e.action.skill == MISSILE
                        && e.action.attacker_behavior.time_scale > 0.0 =>
                {
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
    assert!(at.len() > 10, "{} shots", at.len());
    for pair in at.windows(2) {
        let seen = pair[1].0 - pair[0].0;
        let due = f64::from(gap / pair[0].1);
        assert!(
            (seen - due).abs() <= 0.051 + 0.001,
            "a gap of {seen:.3} where {due:.3} was due"
        );
    }
}

/// A turret, whose template gives no level and turns without moving, does not shoot first; when a
/// creature near it goes after a player, it takes on the nearest player within its missile's
/// reach and shoots from where it stands.
/// Behaviour: server.creatures.turrets-roused-by-their-guards
#[test]
fn a_turret_shoots_from_where_it_stands_once_a_creature_near_it_fights() {
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    // The turret 25 m from the player, the guard 30 m (beyond its perception).
    camp(&mut h, TURRET, unlevelled(true), [84.06, 240.2, -44.6]);
    camp(&mut h, GUARD, guard(), [89.06, 240.2, -44.6]);
    let (b, me) = enter(&mut h);
    let turret = made(&h, b, TURRET);
    let guard_id = made(&h, b, GUARD);
    let at = place_of(&h, turret);
    h.run(200);
    assert_eq!(
        mode_of(&h, turret),
        Some(Mode::Idle),
        "it never shoots first"
    );
    // The player walks up to the guard, which goes after it; the turret is roused.
    if let Some(p) = h.server.players.values_mut().find(|p| p.character == me) {
        p.position.origin[0] += 22.0;
    }
    assert!(h.run_until(200, |h| mode_of(h, guard_id) == Some(Mode::Chasing)));
    assert_eq!(mode_of(&h, turret), Some(Mode::Chasing), "in the same tick");
    let from = h.bots[b].inbox.len();
    assert!(
        h.run_until(200, |h| h.world_messages(b, from).iter().any(|m| matches!(
            m,
            ServerMessage::DoStory(e) if e.sender.id == turret && e.action.skill == MISSILE
        ))),
        "it shoots"
    );
    assert_eq!(place_of(&h, turret), at, "from where it stands");
}
