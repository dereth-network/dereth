//! Creatures that mark the players they see, and creatures that take on only the players a
//! concealing effect makes them hate: a mark put on as a player comes within a marker's perception,
//! with its line for the player's race, again only after the player has left and come back; a
//! creature of the hated-only kind leaving alone a player without the mark, whoever else fights it,
//! and shooting one with it; the mark gone at a teleport and at logout. Fixture: the test world's
//! creature rows (a guard that marks, the turret that takes on only the marked, the mark's
//! concealing row), made-up templates of those kinds and of a creature that attacks first, in camps
//! beside where a character starts.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use auberean_script::native_object::{TextInfo, VariableValue};
use yalain_content::{Prop, Props};
use yalain_testkit::play::server_call;
use yalain_testkit::{
    test_template, Harness, MARK, MARKED_ONLY, MARKERS, MARK_HATED_BY, MARK_TABLE,
};
use yalain_world::calls::{self, Node};
use yalain_world::things::Mode;

/// A guard whose row marks players (perception 10 m), and a creature no row names that attacks
/// first (perception 12 m).
const MARKER: u32 = MARKERS[3];
const HOSTILE: u32 = 0x4700_7794;
const MISSILE: u32 = 1863;
const STARTER_SWORD: u32 = 0x4700_051C;
const START_ATTACK: u32 = 0x8204_0008;
const SAYING: u32 = 0x8205_0016;
const REMOVE_EFFECT: u32 = 0x8208_0016;
/// The marker's line of a Lugian.
const OF_A_LUGIAN: u32 = 0x4DE5_0D1C;
/// A friend table whose creatures never attack a player first.
const NEVER_FIRST: u32 = 0x0E;

/// The guard: it never attacks first, so only its mark is seen.
fn marker() -> Props {
    let mut p = Props::default();
    p.set("GameplayStatistics_Level", Prop::Int(70))
        .set("GameplayStatistics_MaxHealth", Prop::Int(50_000))
        .set("AICombat_NaturalDamage", Prop::Float(1.0))
        .set("AICombat_NaturalVariance", Prop::Float(0.0))
        .set("AI_PerceptionRadius", Prop::Float(10.0))
        .set("AI_LoveTable", Prop::Word(NEVER_FIRST));
    p
}

fn hostile() -> Props {
    let mut p = Props::default();
    p.set("GameplayStatistics_Level", Prop::Int(2))
        .set("GameplayStatistics_MaxHealth", Prop::Int(460))
        .set("AICombat_NaturalDamage", Prop::Float(1.0))
        .set("AICombat_NaturalVariance", Prop::Float(0.0))
        .set("AI_PerceptionRadius", Prop::Float(12.0));
    p
}

/// The turret: it turns where it stands, of the class the mark makes hate its holder, of a friend
/// table that never attacks first, its missile the test skill table's poisoned one.
fn turret() -> Props {
    let mut p = Props::default();
    p.set("GameplayStatistics_MaxHealth", Prop::Int(500_000))
        .set("AICombat_NaturalDamage", Prop::Float(1.0))
        .set("AICombat_NaturalVariance", Prop::Float(0.3))
        .set("AI_MovementType", Prop::Word(0x4000_0002))
        .set("AI_LoveTable", Prop::Word(0x23))
        .set("AI_AISuperClass", Prop::Word(MARK_HATED_BY))
        .set("AI_PerceptionRadius", Prop::Float(500.0))
        .set("NPC_StartingSkill1", Prop::Word(MISSILE))
        .set("NPC_StartingSkill1Level", Prop::Int(50));
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
        name: None,
    });
    h.server
        .camps
        .push(yalain_world::spawn::CampState::default());
}

/// The test world, the mark as the install defines it (four hours, gone at logout and at a
/// teleport), and these camps.
fn world(camps: &[(u32, Props, [f32; 3])]) -> Harness {
    use yalain_world::effect_defs::{EffectDef, Scale};
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    h.server.effect_table.effects.insert(
        MARK,
        EffectDef {
            duration: Scale(vec![(0.0, 14_400.0)]),
            ends_at_logout: true,
            ends_at_teleport: true,
            ..yalain_testkit::plain_effect(MARK)
        },
    );
    for (template, props, spot) in camps {
        camp(&mut h, *template, props.clone(), *spot);
    }
    h
}

/// A Lugian in the world, its scene drawn, as tough as a turret's shots need.
fn enter(h: &mut Harness, account: &str, name: &str) -> (usize, u64) {
    let (b, me) = h.player_in_world(account, name);
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    if let Some(p) = h.server.players.values_mut().find(|p| p.character == me) {
        p.race = 2;
        for q in [
            yalain_world::player::quality::HEALTH_MAX,
            yalain_world::player::quality::HEALTH,
        ] {
            yalain_world::quals::set_int(&mut p.qualities, q, 1_000_000);
        }
    }
    h.run(5);
    (b, me)
}

fn moves(h: &mut Harness, me: u64, dx: f32) {
    if let Some(p) = h.server.players.values_mut().find(|p| p.character == me) {
        p.position.origin[0] += dx;
    }
}

fn of_template(h: &Harness, template: u32) -> u64 {
    h.server
        .things
        .by_id
        .values()
        .find(|t| t.template == template)
        .map(|t| t.id)
        .expect("made by its camp")
}

fn mode_of(h: &Harness, id: u64) -> Option<Mode> {
    h.server
        .things
        .get(id)
        .and_then(|t| t.creature.as_ref())
        .map(|c| c.mode)
}

/// The casters of the marks a player holds.
fn marks_on(h: &Harness, me: u64) -> Vec<u64> {
    h.server
        .players
        .values()
        .find(|p| p.character == me)
        .map(|p| {
            p.play
                .effects
                .iter()
                .filter(|e| e.state == MARK)
                .map(|e| e.caster)
                .collect()
        })
        .unwrap_or_default()
}

/// The texts a client heard a speaker say since `from`.
fn sayings(h: &Harness, b: usize, speaker: u64, from: usize) -> Vec<TextInfo> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::CellObjectCall(c)
                if c.object.id == speaker && c.call.function.0 == SAYING =>
            {
                c.call.args.as_ref()
            }
            _ => None,
        })
        .filter_map(|a| calls::values(&a.0).ok())
        .filter_map(|v| {
            v.iter().find_map(|x| match x.root() {
                Some(Node::TextInfo(t)) => Some(t.clone()),
                _ => None,
            })
        })
        .collect()
}

/// The literal text a line names in its variable.
fn named(t: &TextInfo) -> Option<String> {
    t.variables.iter().find_map(|v| match &v.value {
        VariableValue::Nested(n) => n.literal.as_ref().map(|l| String::from_utf16_lossy(l)),
        _ => None,
    })
}

fn shots(h: &Harness, b: usize, shooter: u64, from: usize) -> usize {
    h.world_messages(b, from)
        .iter()
        .filter(|m| {
            matches!(m, ServerMessage::DoStory(e)
                if e.sender.id == shooter && e.action.skill == MISSILE)
        })
        .count()
}

fn removed(h: &Harness, b: usize, from: usize) -> usize {
    h.world_messages(b, from)
        .iter()
        .filter(|m| matches!(m, ServerMessage::PlayerCall(c) if c.function.0 == REMOVE_EFFECT))
        .count()
}

/// A player coming within a marker's perception is marked by it, and the marker says its line for
/// the player's race aloud, naming that player to everyone who knows it.
/// Behaviour: server.creatures.marks-players-coming-in-sight
#[test]
fn a_player_coming_within_a_markers_perception_is_marked_and_named_by_its_line_for_its_race() {
    let mut h = world(&[(MARKER, marker(), [80.0, 240.0, -44.6])]);
    let (b, me) = enter(&mut h, "one", "Elara");
    let (other, _) = enter(&mut h, "two", "Bram");
    let guard = of_template(&h, MARKER);
    h.run(50);
    assert!(marks_on(&h, me).is_empty(), "beyond its perception");
    let from = h.bots[b].inbox.len();
    let heard_from = h.bots[other].inbox.len();
    // Both start at the same spot, 21 m from the guard; one walks to 7 m from it.
    moves(&mut h, me, 14.0);
    assert!(h.run_until(20, |h| !marks_on(h, me).is_empty()));
    assert_eq!(marks_on(&h, me), [guard], "cast by the guard");
    h.run(2);
    let name = h
        .server
        .players
        .values()
        .find(|p| p.character == me)
        .map(|p| yalain_world::player::world_name(&p.name, p.sex))
        .expect("in the world");
    for (bot, since) in [(b, from), (other, heard_from)] {
        let said = sayings(&h, bot, guard, since);
        assert_eq!(said.len(), 1, "one line");
        assert_eq!(
            (said[0].string_id, said[0].table, named(&said[0])),
            (OF_A_LUGIAN, MARK_TABLE, Some(name.clone())),
            "its line of a Lugian, naming the one marked"
        );
    }
}

/// A player staying within a marker's perception is marked once; one that leaves and comes back
/// is marked again.
/// Behaviour: server.creatures.marks-players-coming-in-sight
#[test]
fn a_player_staying_within_is_marked_once_and_again_only_after_coming_back() {
    let mut h = world(&[(MARKER, marker(), [80.0, 240.0, -44.6])]);
    let (b, me) = enter(&mut h, "one", "Elara");
    let guard = of_template(&h, MARKER);
    let from = h.bots[b].inbox.len();
    moves(&mut h, me, 14.0);
    h.run(200);
    assert_eq!(marks_on(&h, me).len(), 1, "once, staying");
    assert_eq!(sayings(&h, b, guard, from).len(), 1);
    moves(&mut h, me, -14.0);
    h.run(10);
    moves(&mut h, me, 14.0);
    h.run(10);
    assert_eq!(marks_on(&h, me).len(), 2, "again, come back");
    assert_eq!(sayings(&h, b, guard, from).len(), 2);
}

/// A creature that takes on only the players it hates leaves alone a player without the mark: not
/// roused when a creature near it goes after the player, not when the player strikes it.
/// Behaviour: server.creatures.only-the-hated
#[test]
fn a_hated_only_turret_leaves_an_unmarked_player_alone_whoever_fights_it() {
    // The turret 25 m from the player, the hostile creature 30 m.
    let mut h = world(&[
        (MARKED_ONLY, turret(), [84.06, 240.2, -44.6]),
        (HOSTILE, hostile(), [89.06, 240.2, -44.6]),
    ]);
    let (b, me) = enter(&mut h, "one", "Elara");
    let turret_id = of_template(&h, MARKED_ONLY);
    let hostile_id = of_template(&h, HOSTILE);
    let from = h.bots[b].inbox.len();
    moves(&mut h, me, 22.0);
    assert!(h.run_until(200, |h| mode_of(h, hostile_id) == Some(Mode::Chasing)));
    h.run(300);
    assert_eq!(mode_of(&h, turret_id), Some(Mode::Idle), "not roused");
    assert_eq!(shots(&h, b, turret_id, from), 0);
    // Beside the turret, the player strikes it.
    moves(&mut h, me, 1.5);
    h.arm(b, me, STARTER_SWORD);
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(START_ATTACK, &[Arg::LongInt(turret_id)]),
    );
    assert!(
        h.run_until(400, |h| h
            .server
            .things
            .get(turret_id)
            .is_some_and(|t| t.health < t.max_health)),
        "struck"
    );
    h.run(100);
    assert_eq!(mode_of(&h, turret_id), Some(Mode::Idle), "not struck back");
    assert_eq!(shots(&h, b, turret_id, from), 0);
}

/// A player a marker marks is taken on by the creatures that take on only its holders, which shoot
/// it from where they stand.
/// Behaviour: server.creatures.only-the-hated
#[test]
fn a_hated_only_turret_shoots_the_player_a_marker_marks() {
    let mut h = world(&[
        (MARKED_ONLY, turret(), [84.06, 240.2, -44.6]),
        (MARKER, marker(), [80.0, 240.0, -44.6]),
    ]);
    let (b, me) = enter(&mut h, "one", "Elara");
    let turret_id = of_template(&h, MARKED_ONLY);
    h.run(100);
    assert_eq!(mode_of(&h, turret_id), Some(Mode::Idle), "unmarked");
    let from = h.bots[b].inbox.len();
    moves(&mut h, me, 14.0);
    assert!(
        h.run_until(400, |h| shots(h, b, turret_id, from) > 0),
        "shot"
    );
    assert!(!marks_on(&h, me).is_empty());
}

fn world_of(h: &Harness, me: u64) -> yalain_net::ConnId {
    *h.server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .expect("in the world")
        .0
}

/// Put the mark on a player, as a marker of level 70 would; its record's id.
fn mark(h: &mut Harness, me: u64) -> u32 {
    let world = world_of(h, me);
    let clock = h.clock();
    let cast = yalain_world::effects::Cast {
        caster: 1,
        spellcraft: 70.0,
        ..yalain_world::effects::Cast::default()
    };
    assert!(yalain_world::effects::apply(
        &mut h.server,
        &mut h.net,
        world,
        MARK,
        cast,
        clock
    ));
    h.server.players[&world]
        .play
        .effects
        .iter()
        .find(|e| e.state == MARK)
        .map(|e| e.eid)
        .expect("marked")
}

fn target_of(h: &Harness, id: u64) -> Option<u64> {
    h.server
        .things
        .get(id)
        .and_then(|t| t.creature.as_ref())
        .and_then(|c| c.target)
}

/// A creature that takes on only the players it hates gives up on one whose mark is taken off.
/// Behaviour: server.creatures.only-the-hated
#[test]
fn a_hated_only_turret_gives_up_on_a_player_whose_mark_is_taken_off() {
    let mut h = world(&[(MARKED_ONLY, turret(), [84.06, 240.2, -44.6])]);
    let (_, me) = enter(&mut h, "one", "Elara");
    let turret_id = of_template(&h, MARKED_ONLY);
    let eid = mark(&mut h, me);
    assert!(h.run_until(400, |h| mode_of(h, turret_id) == Some(Mode::Chasing)));
    let world = world_of(&h, me);
    assert!(yalain_world::effects::remove(
        &mut h.server,
        &mut h.net,
        world,
        eid
    ));
    assert!(
        h.run_until(50, |h| mode_of(h, turret_id) != Some(Mode::Chasing)),
        "gives up"
    );
}

/// A creature that takes on only the players it hates, even of a friend table that attacks first,
/// takes on a marked player past a nearer one without the mark, keeps to it while the other strikes
/// it, and leaves the other alone once the marked one has gone.
/// Behaviour: server.creatures.only-the-hated
#[test]
fn a_hated_only_turret_takes_on_the_marked_past_a_nearer_striker_and_never_turns_to_it() {
    let mut props = turret();
    props.set("AI_LoveTable", Prop::Word(0x42));
    // The turret 2 m from where both start; the marked one goes 9 m the other way.
    let mut h = world(&[(MARKED_ONLY, props, [61.0, 240.2, -44.6])]);
    let (b, striker) = enter(&mut h, "one", "Elara");
    let (_, marked) = enter(&mut h, "two", "Bram");
    let turret_id = of_template(&h, MARKED_ONLY);
    moves(&mut h, marked, -9.0);
    mark(&mut h, marked);
    assert!(h.run_until(100, |h| mode_of(h, turret_id) == Some(Mode::Chasing)));
    assert_eq!(
        target_of(&h, turret_id),
        Some(marked),
        "past the nearer one"
    );
    h.arm(b, striker, STARTER_SWORD);
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(START_ATTACK, &[Arg::LongInt(turret_id)]),
    );
    for _ in 0..600 {
        h.step();
        assert_eq!(target_of(&h, turret_id), Some(marked), "kept to");
    }
    let hurt = h
        .server
        .things
        .get(turret_id)
        .is_some_and(|t| t.health < t.max_health);
    assert!(hurt, "struck");
    let elsewhere = yalain_store::StoredPosition {
        cell: 0x0102_0003,
        origin: [12.5, 34.5, 2.0],
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    let world = world_of(&h, marked);
    let clock = h.clock();
    yalain_world::places::recall_trip(&mut h.server, &mut h.net, world, elsewhere, None, clock);
    for _ in 0..300 {
        h.step();
        assert_ne!(target_of(&h, turret_id), Some(striker), "never the striker");
    }
    assert_ne!(mode_of(&h, turret_id), Some(Mode::Chasing));
}

/// A mark ends as its holder goes into portal space, told gone, and the creatures that took the
/// holder on for it give up.
/// Behaviour: server.effects.ends-at-teleport
#[test]
fn a_teleport_takes_the_mark_off_and_the_turret_gives_up() {
    let mut h = world(&[
        (MARKED_ONLY, turret(), [84.06, 240.2, -44.6]),
        (MARKER, marker(), [80.0, 240.0, -44.6]),
    ]);
    let (b, me) = enter(&mut h, "one", "Elara");
    let turret_id = of_template(&h, MARKED_ONLY);
    let from = h.bots[b].inbox.len();
    moves(&mut h, me, 14.0);
    assert!(h.run_until(400, |h| shots(h, b, turret_id, from) > 0));
    let world_id = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .expect("in the world")
        .0;
    let elsewhere = yalain_store::StoredPosition {
        cell: 0x0102_0003,
        origin: [12.5, 34.5, 2.0],
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    let leaving = h.bots[b].inbox.len();
    let clock = h.clock();
    yalain_world::places::recall_trip(&mut h.server, &mut h.net, world_id, elsewhere, None, clock);
    assert!(marks_on(&h, me).is_empty(), "gone as it goes");
    h.run(5);
    assert_eq!(removed(&h, b, leaving), 1, "told gone");
    assert!(h.run_until(50, |h| mode_of(h, turret_id) != Some(Mode::Chasing)));
}

/// A mark ends at logout: it is not in its holder's next session, and the creatures that take on
/// only its holders leave the player alone when it is back.
/// Behaviour: server.effects.ends-at-logout
#[test]
fn logging_off_takes_the_mark_off() {
    use auberean_protocol::login::CharacterRef;
    use auberean_protocol::ClientMessage;
    let mut h = world(&[(MARKED_ONLY, turret(), [84.06, 240.2, -44.6])]);
    let (b, me) = enter(&mut h, "one", "Elara");
    let turret_id = of_template(&h, MARKED_ONLY);
    let world_id = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .expect("in the world")
        .0;
    let clock = h.clock();
    let cast = yalain_world::effects::Cast {
        caster: 1,
        spellcraft: 70.0,
        ..yalain_world::effects::Cast::default()
    };
    assert!(yalain_world::effects::apply(
        &mut h.server,
        &mut h.net,
        world_id,
        MARK,
        cast,
        clock
    ));
    assert!(h.run_until(400, |h| mode_of(h, turret_id) == Some(Mode::Chasing)));
    let before = h.bots[b].inbox.len();
    h.bots[b].send(
        false,
        queue::LOGON,
        &ClientMessage::LogOff(CharacterRef { character: me }),
    );
    assert!(h.run_until(400, |h| h.bots[b].inbox[before..]
        .iter()
        .any(|r| matches!(r.message(), Some(ServerMessage::CharacterList(_))))));
    h.run(50);
    h.enter_world(b, "one", me);
    h.bots[b].send(
        true,
        queue::WEENIE,
        &ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
    assert!(marks_on(&h, me).is_empty(), "not back");
    let from = h.bots[b].inbox.len();
    h.run(300);
    assert_eq!(shots(&h, b, turret_id, from), 0, "left alone");
}

/// A world whose rows name no marker and no hated-only creature marks no one, and a turret is roused
/// as any: a creature near it going after a player rouses it to shoot the nearest player in reach.
/// Behaviour: server.creatures.turrets-roused-by-their-guards
#[test]
fn a_world_without_such_rows_marks_no_one_and_rouses_its_turrets() {
    let mut h = world(&[
        (MARKED_ONLY, turret(), [84.06, 240.2, -44.6]),
        (HOSTILE, hostile(), [89.06, 240.2, -44.6]),
        (MARKER, marker(), [70.0, 240.0, -44.6]),
    ]);
    h.server.creature_rows.marks.clear();
    h.server.creature_rows.only_hated.clear();
    let (b, me) = enter(&mut h, "one", "Elara");
    let guard = of_template(&h, MARKER);
    let turret_id = of_template(&h, MARKED_ONLY);
    let from = h.bots[b].inbox.len();
    // Within the guard's perception, then on to the hostile creature.
    moves(&mut h, me, 8.5);
    h.run(20);
    assert!(marks_on(&h, me).is_empty(), "not marked");
    assert!(sayings(&h, b, guard, from).is_empty());
    moves(&mut h, me, 13.5);
    assert!(
        h.run_until(400, |h| shots(h, b, turret_id, from) > 0),
        "roused"
    );
}
