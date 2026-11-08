//! Kingdom points: an effect a world's row names as a points bonus raises them when a give or a
//! quest step puts it on a player, up to the row's cap, and the kingdom skills the points then earn
//! are granted at once; steps and gives that name a kingdom are for its players alone.

use auberean_protocol::{queue, ClientMessage};
use yalain_content::kingdoms::{kingdom, KingdomTable, PointsBonus};
use yalain_content::quests::{Give, Need, QuestDef, QuestTable, Step, Trigger, Update};
use yalain_content::{ContentObject, ContentPack, Description, Facts, Kind, Props};
use yalain_testkit::play::server_call;
use yalain_testkit::Harness;
use yalain_world::gives::{self, GIVE, HANDED_AFTER};
use yalain_world::inventory::{move_graph, Move};
use yalain_world::kingdom_skills::{points, KingdomSkills, POINTS, POINTS_UNSET, UPDATED};
use yalain_world::skills::{mask, SkillDef};

/// A shrine beside the start, and its template.
const SHRINE: u64 = 0x8130_0000_0000_0702;
const SHRINE_T: u32 = 0x4700_100D;

/// Three gems and the effect each one's give puts on: so many points, up to a cap.
const SLIVER: u32 = 0x4700_080F;
const SHARD: u32 = 0x4700_16AB;
const FRAGMENT: u32 = 0x4700_16AA;
const SLIVER_BONUS: u32 = 0x6F00_188E;
const SHARD_BONUS: u32 = 0x6F00_188D;
const FRAGMENT_BONUS: u32 = 0x6F00_188C;
const GEMS: [(u32, u32, i32, i32); 3] = [
    (SLIVER, SLIVER_BONUS, 200, 1_500),
    (SHARD, SHARD_BONUS, 250, 1_750),
    (FRAGMENT, FRAGMENT_BONUS, 250, 2_000),
];

/// The line a gem's give shows once it is taken.
const REWARD: (u32, u32) = (0xA727_FC98, 0x2500_1147);

/// A kingdom skill the Dominion's table gives at 1,400 points, and one at 2,400.
const AT_1400: u32 = 21;
const AT_2400: u32 = 30;

const QUEST: u32 = 0x4000_024C;
const MOVE_DONE_STEPS: f64 = HANDED_AFTER + gives::HANDED_BACK_AFTER;

fn shrine() -> ContentObject {
    ContentObject {
        object: Some(SHRINE),
        template: SHRINE_T,
        description: Description {
            name_text: None,
            package: 0x13B1,
            look: 0x1F00_0ED5,
            enter_fx: 0,
            scale: None,
            name: None,
            numbers: [0; 6],
            source: "authored for a test".into(),
        },
        cell: 0x0299_0109,
        origin: [58.0, 241.0, -44.6],
        rotation: [1.0, 0.0, 0.0, 0.0],
        kind: Kind::Static,
        label: String::new(),
        placed: "authored for a test".into(),
        destination: None,
        facts: Facts::default(),
        walks: None,
        gate: None,
    }
}

/// The shrine takes one gem at a time, its effect put on the giver.
fn gem_give(item: u32, effect: u32) -> Give {
    Give {
        to: SHRINE_T,
        item: Some(item),
        wants: Some(1),
        gives: Vec::new(),
        counts: Vec::new(),
        effects: vec![effect],
        text: Some(REWARD),
        fx: None,
        requires: Vec::new(),
        placed: "authored for a test".into(),
    }
}

fn quest(id: u32) -> QuestDef {
    QuestDef {
        id,
        name: (0xA860_7CEC, 0x2500_2958),
        description: (0x6C45_9364, 0x2500_2958),
        icon: 0x4100_034B,
        play_fx: true,
        journals: vec![(0x2E5C_1E52, 0x2500_2958)],
        challenge: -999,
        duration: 0.0,
        timed_phase: 0,
        retry: 0.0,
        xp: 0,
        cancellable: true,
        placed: "authored for a test".into(),
        hidden: false,
        effects: Vec::new(),
        subquests: Vec::new(),
        min_level: 0,
        fails_on_death: false,
        once: false,
        fellows: yalain_content::quests::Fellows::default(),
        last_waits: false,
        hero_only: false,
        pay: yalain_content::quests::CompletionPay::default(),
        required: None,
    }
}

/// A world of the shrine, a Dominion table of two skills, the gems' gives for the Dominion's
/// players alone, and, with `rows`, the points each gem's effect adds.
fn world(rows: bool) -> Harness {
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    h.server.content = ContentPack {
        objects: vec![shrine()],
        ..ContentPack::default()
    };
    for id in [AT_1400, AT_2400] {
        h.server.skill_table.skills.insert(
            id,
            SkillDef {
                id,
                cost: 1,
                flags: 0x4C,
                ..SkillDef::default()
            },
        );
    }
    h.server.kingdom_skills = KingdomSkills {
        by_kingdom: [(kingdom::DOMINION, vec![(1_400, AT_1400), (2_400, AT_2400)])].into(),
    };
    let gives: Vec<Give> = GEMS.iter().map(|g| gem_give(g.0, g.1)).collect();
    h.server.kingdom_rows = KingdomTable {
        points: if rows {
            GEMS.iter()
                .map(|g| {
                    (
                        g.1,
                        PointsBonus {
                            add: g.2,
                            up_to: g.3,
                        },
                    )
                })
                .collect()
        } else {
            std::collections::BTreeMap::new()
        },
        steps: Vec::new(),
        gives: gives
            .iter()
            .map(|g| (g.clone(), kingdom::DOMINION))
            .collect(),
    };
    h.server.quests = QuestTable {
        quests: vec![quest(QUEST)],
        steps: Vec::new(),
        gives,
        rooms: Vec::new(),
    };
    h
}

/// A player in the world whose client has drawn it, of a kingdom (none: in none), at these points.
fn player(h: &mut Harness, name: &str, k: Option<u32>, at: Option<i32>) -> (usize, u64) {
    let (b, me) = h.player_in_world(name, name);
    h.bots[b].send(
        true,
        queue::WEENIE,
        &ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
    let world = world_of(h, me);
    let p = h.server.players.get_mut(&world).expect("in the world");
    if let Some(k) = k {
        yalain_world::kingdoms::set_kingdom(&mut p.qualities, k);
    }
    if let Some(at) = at {
        yalain_world::quals::set_int(&mut p.qualities, POINTS, at);
    }
    (b, me)
}

fn world_of(h: &Harness, me: u64) -> yalain_net::ConnId {
    *h.server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .map(|(c, _)| c)
        .expect("in the world")
}

fn points_of(h: &Harness, me: u64) -> i32 {
    points(&h.server.players[&world_of(h, me)].qualities)
}

fn skills_of(h: &Harness, me: u64) -> Vec<(u32, u32)> {
    let mut held: Vec<(u32, u32)> = h.server.players[&world_of(h, me)]
        .play
        .skills
        .iter()
        .map(|k| (k.skill, k.mask))
        .collect();
    held.sort_unstable();
    held
}

/// Put one of a template in a player's pack; returns the item and its slot.
fn hold(h: &mut Harness, me: u64, template: u32) -> (u64, i32) {
    if h.server.catalog.get(template).is_none() {
        h.server.catalog.insert(yalain_testkit::test_template(
            template,
            0x1067,
            Props::default(),
        ));
    }
    let world = world_of(h, me);
    let item =
        yalain_world::inventory::give(&mut h.server, &mut h.net, world, template, 1).expect("made");
    h.run(3);
    let slot = h.server.players[&world]
        .play
        .items
        .iter()
        .find(|x| x.id == item)
        .map_or(0, |x| x.slot);
    (item, i32::try_from(slot).unwrap_or(0))
}

fn holds(h: &Harness, me: u64, item: u64) -> bool {
    h.server.players[&world_of(h, me)]
        .play
        .items
        .iter()
        .any(|x| x.id == item)
}

/// Give a held item to the shrine, as the 2017 clients asked, and wait until the shrine is
/// through with it. Returns where the client's messages stood before.
fn give_to_shrine(h: &mut Harness, b: usize, me: u64, item: (u64, i32)) -> usize {
    let from = h.bots[b].inbox.len();
    let m = Move {
        move_type: GIVE,
        merge: true,
        unlock: true,
        check_take: true,
        all_taken: true,
        grab: true,
        contain: true,
        auto_merge: true,
        target: SHRINE,
        actual_target: SHRINE,
        merge_container: SHRINE,
        from: me,
        actual_from: me,
        item: item.0,
        split_item: item.0,
        quantity: 1,
        quantity_left: 1,
        from_slot: item.1,
        actual_from_slot: item.1,
        ..Move::default()
    };
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(yalain_world::drops::function::MOVE_ITEM, &[move_graph(&m)]),
    );
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let n = (MOVE_DONE_STEPS / yalain_testkit::STEP).ceil() as usize;
    h.run(n + 8);
    from
}

/// Give one gem of a template to the shrine; returns whether it was taken.
fn hand_in(h: &mut Harness, b: usize, me: u64, gem: u32) -> bool {
    let item = hold(h, me, gem);
    give_to_shrine(h, b, me, item);
    !holds(h, me, item.0)
}

/// The texts a client was shown since `from`.
fn shown(h: &Harness, b: usize, from: usize) -> Vec<(u32, u32)> {
    use yalain_world::calls::Node;
    h.calls(b, from)
        .into_iter()
        .filter(|(f, _)| *f == yalain_world::session::function::SHOW_TEXT)
        .filter_map(|(_, v)| {
            v.iter().find_map(|x| match x.root() {
                Some(Node::TextInfo(t)) => Some((t.string_id, t.table)),
                _ => None,
            })
        })
        .collect()
}

/// Each gem handed to one's own kingdom's shrine is taken and raises the giver's kingdom points by
/// its row's amount; each stops at its cap (a Sliver at 1,400 makes 1,500) and is refused once the
/// points are at or above it, so points above it are never lowered; points are kept with the
/// character and come back with it.
/// Behaviour: server.kingdoms.points-bonus
#[test]
fn each_gem_raises_kingdom_points_by_its_amount_up_to_its_cap() {
    for (gem, _, add, up_to) in GEMS {
        let mut h = world(true);
        let (b, me) = player(&mut h, "Elara", Some(kingdom::DOMINION), None);
        assert!(hand_in(&mut h, b, me, gem), "{gem:#x} taken");
        assert_eq!(points_of(&h, me), POINTS_UNSET + add, "{gem:#x} from 500");

        // Just under the cap: up to it and no further.
        let mut h = world(true);
        let (b, me) = player(&mut h, "Elara", Some(kingdom::DOMINION), Some(up_to - 100));
        assert!(hand_in(&mut h, b, me, gem));
        assert_eq!(points_of(&h, me), up_to, "{gem:#x} stops at its cap");
        // At the cap and above it: refused, and never lowered.
        assert!(!hand_in(&mut h, b, me, gem), "{gem:#x} refused at its cap");
        assert_eq!(points_of(&h, me), up_to, "{gem:#x} at its cap");
        let mut h = world(true);
        let (b, me) = player(&mut h, "Elara", Some(kingdom::DOMINION), Some(up_to + 100));
        assert!(
            !hand_in(&mut h, b, me, gem),
            "{gem:#x} refused above its cap"
        );
        assert_eq!(points_of(&h, me), up_to + 100, "{gem:#x} never lowers");
    }

    // The ruling's own numbers: a Sliver at 1,400 is taken and makes 1,500.
    let mut h = world(true);
    let (b, me) = player(&mut h, "Elara", Some(kingdom::DOMINION), Some(1_400));
    assert!(hand_in(&mut h, b, me, SLIVER));
    assert_eq!(points_of(&h, me), 1_500);

    // Many gems, one after another, each its own amount: the Sliver's to 1,500, then the Shard's.
    let mut h = world(true);
    let (b, me) = player(&mut h, "Elara", Some(kingdom::DOMINION), None);
    for _ in 0..5 {
        assert!(hand_in(&mut h, b, me, SLIVER));
    }
    assert!(!hand_in(&mut h, b, me, SLIVER), "the sixth refused");
    assert_eq!(points_of(&h, me), 1_500, "five Slivers to the cap");
    assert!(hand_in(&mut h, b, me, SHARD));
    assert_eq!(points_of(&h, me), 1_750);

    // Kept: the character comes back with them.
    let before = h.bots[b].inbox.len();
    h.bots[b].send(
        false,
        queue::LOGON,
        &ClientMessage::LogOff(auberean_protocol::login::CharacterRef { character: me }),
    );
    assert!(
        h.run_until(yalain_testkit::play::STEPS, |h| h.bots[b].inbox[before..]
            .iter()
            .any(|r| matches!(
                r.message(),
                Some(auberean_protocol::ServerMessage::CharacterList(_))
            ))),
        "left the world"
    );
    assert!(h.server.players.values().all(|p| p.character != me));
    h.enter_world(b, "Elara", me);
    h.run(10);
    assert_eq!(points_of(&h, me), 1_750, "kept with the character");
}

/// A gem whose points bonus would add nothing is refused: at its cap (a Sliver at 1,500) or above
/// it (a Sliver at 1,600) the gem stays in the pack, the points stay and no line is shown; a give
/// that does something besides is taken as before.
/// Behaviour: server.kingdoms.points-bonus
#[test]
fn a_gem_that_would_add_nothing_is_refused_and_kept() {
    for at in [1_500, 1_600] {
        let mut h = world(true);
        let (b, me) = player(&mut h, "Elara", Some(kingdom::DOMINION), Some(at));
        let gem = hold(&mut h, me, SLIVER);
        let from = give_to_shrine(&mut h, b, me, gem);
        assert!(holds(&h, me, gem.0), "{at}: the gem is still in the pack");
        assert_eq!(points_of(&h, me), at, "{at}: the points stay");
        assert!(shown(&h, b, from).is_empty(), "{at}: no line");
    }

    // A give that also gives something is taken at the cap all the same.
    const TOKEN: u32 = 0x4700_1758;
    let mut h = world(true);
    h.server.catalog.insert(yalain_testkit::test_template(
        TOKEN,
        0x1067,
        Props::default(),
    ));
    for g in &mut h.server.quests.gives {
        g.gives = vec![TOKEN];
    }
    h.server.kingdom_rows.gives = h
        .server
        .quests
        .gives
        .iter()
        .map(|g| (g.clone(), kingdom::DOMINION))
        .collect();
    let (b, me) = player(&mut h, "Elara", Some(kingdom::DOMINION), Some(1_600));
    assert!(hand_in(&mut h, b, me, SLIVER), "taken for what it gives");
    assert_eq!(points_of(&h, me), 1_600);
}

/// A gem whose give crosses a kingdom skill's least points grants that skill at once, told as the
/// shrine tells its grants, with no use of the shrine; one that crosses none grants nothing.
/// Behaviour: server.kingdoms.points-bonus
#[test]
fn a_gem_crossing_a_kingdom_skills_points_grants_it_at_once() {
    let mut h = world(true);
    let (b, me) = player(&mut h, "Elara", Some(kingdom::DOMINION), Some(1_300));
    let from = h.bots[b].inbox.len();
    assert!(hand_in(&mut h, b, me, SLIVER));
    assert_eq!(points_of(&h, me), 1_500);
    assert_eq!(
        skills_of(&h, me),
        [(AT_1400, mask::TRAINED)],
        "the skill at 1,400, not the one at 2,400"
    );
    assert_eq!(
        shown(&h, b, from),
        [UPDATED, REWARD],
        "told as at the shrine, then the give's line"
    );

    let from = h.bots[b].inbox.len();
    assert!(hand_in(&mut h, b, me, SHARD));
    assert_eq!(points_of(&h, me), 1_750);
    assert_eq!(skills_of(&h, me), [(AT_1400, mask::TRAINED)], "nothing new");
    assert_eq!(shown(&h, b, from), [REWARD], "and nothing more told");
}

/// A world with no row for a gem's effect leaves the giver's points as they are, whatever the
/// give puts on.
/// Behaviour: server.kingdoms.points-bonus
#[test]
fn a_world_without_the_points_rows_leaves_points_as_they_are() {
    let mut h = world(false);
    let (b, me) = player(&mut h, "Elara", Some(kingdom::DOMINION), None);
    for (gem, ..) in GEMS {
        assert!(hand_in(&mut h, b, me, gem), "the give is still the world's");
    }
    assert_eq!(points_of(&h, me), POINTS_UNSET);
    assert!(skills_of(&h, me).is_empty());
}

/// A give for one kingdom's players alone is refused to a player of another kingdom, and to one in
/// none (who counts as of the neutral kingdom): the shrine hands the gem back at once and the
/// points stay.
/// Behaviour: server.kingdoms.rows-for-one-kingdom
#[test]
fn another_kingdoms_player_keeps_the_gem_and_its_points() {
    for k in [Some(kingdom::ORDER), Some(kingdom::SHADOW), None] {
        let mut h = world(true);
        let (b, me) = player(&mut h, "Elara", k, Some(1_000));
        let gem = hold(&mut h, me, SLIVER);
        give_to_shrine(&mut h, b, me, gem);
        assert!(holds(&h, me, gem.0), "{k:?}: the gem is back in the pack");
        assert_eq!(points_of(&h, me), 1_000, "{k:?}: the points stay");
    }
}

/// A give requiring a quest and a kingdom needs both: a Dominion player without the quest done
/// keeps the gem, so does an Order player with it done, and a Dominion player with it done hands
/// it in.
/// Behaviour: server.kingdoms.rows-for-one-kingdom
#[test]
fn a_give_requiring_a_quest_and_a_kingdom_needs_both() {
    let with_quest = |h: &mut Harness| {
        for g in &mut h.server.quests.gives {
            g.requires = vec![(QUEST, Need::Done)];
        }
        h.server.kingdom_rows.gives = h
            .server
            .quests
            .gives
            .iter()
            .map(|g| (g.clone(), kingdom::DOMINION))
            .collect();
    };
    let done = |h: &mut Harness, me: u64| {
        let world = world_of(h, me);
        let p = h.server.players.get_mut(&world).expect("in the world");
        p.play.quests.push(yalain_world::state::QuestState {
            quest: QUEST,
            status: yalain_world::quests::status::COMPLETED,
            phase: 0,
            started: 0,
            updated: 0,
        });
    };
    for (k, quest_done, taken) in [
        (kingdom::DOMINION, false, false),
        (kingdom::ORDER, true, false),
        (kingdom::DOMINION, true, true),
    ] {
        let mut h = world(true);
        with_quest(&mut h);
        let (b, me) = player(&mut h, "Elara", Some(k), None);
        if quest_done {
            done(&mut h, me);
        }
        assert_eq!(hand_in(&mut h, b, me, SLIVER), taken, "{k} {quest_done}");
        let expected = if taken { 700 } else { POINTS_UNSET };
        assert_eq!(points_of(&h, me), expected, "{k} {quest_done}");
    }
}

/// A quest step for one kingdom's players alone runs only for them: a Dominion player's gem moves
/// the quest and its effect raises the points, an Order player's is handed back, the quest not
/// given.
/// Behaviour: server.kingdoms.rows-for-one-kingdom
#[test]
fn a_step_for_one_kingdom_runs_only_for_its_players() {
    let step = Step {
        quest: QUEST,
        needs: 0,
        trigger: Trigger::Give(SLIVER, SHRINE_T),
        tell: None,
        update: Update::Bestow,
        xp: 0,
        gives: Vec::new(),
        counts: Vec::new(),
        effects: vec![SLIVER_BONUS],
        text: None,
        placed: "authored for a test".into(),
        requires: Vec::new(),
        takes: vec![SLIVER],
        races: Vec::new(),
        effects_last: false,
        wants: None,
        fellows: false,
        levels: None,
        xp_share: 0.0,
        to: None,
    };
    for (k, runs) in [(kingdom::DOMINION, true), (kingdom::ORDER, false)] {
        let mut h = world(true);
        h.server.quests.gives.clear();
        h.server.kingdom_rows.gives.clear();
        h.server.quests.steps.push(step.clone());
        h.server
            .kingdom_rows
            .steps
            .push((step.clone(), kingdom::DOMINION));
        let (b, me) = player(&mut h, "Elara", Some(k), None);
        assert_eq!(hand_in(&mut h, b, me, SLIVER), runs, "{k}");
        let given = h.server.players[&world_of(&h, me)]
            .play
            .quests
            .iter()
            .any(|q| q.quest == QUEST);
        assert_eq!(given, runs, "{k}: the quest");
        let expected = if runs { 700 } else { POINTS_UNSET };
        assert_eq!(points_of(&h, me), expected, "{k}: the points");
    }
}
