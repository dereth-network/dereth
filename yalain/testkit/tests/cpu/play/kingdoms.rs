//! Kingdoms: joined at a faction shrine, kept, named in the profile, given their own chat room; the
//! conflict status of where a player is, told as it enters, walks and arrives, and the movement
//! ethereality that follows it.

use auberean_primitives::Vec3;
use auberean_protocol::login::{ChatPacket, Empty};
use auberean_protocol::{queue, ClientMessage, ServerMessage};
use dereth_protocol::turbine::{decode_incoming, ExtraBlob, IncomingPayload, SendToRoomById};
use yalain_content::{Conflict, ContentObject, ContentPack, Description, Facts, Kind, Place};
use yalain_testkit::play::{report, server_call, use_of};
use yalain_testkit::Harness;
use yalain_world::calls::{self, Node, Value};
use yalain_world::kingdoms::{self, kingdom, quality};
use yalain_world::requests::function;
use yalain_world::session::function::{SESSION_START, SHOW_TEXT};
use yalain_world::world::IDLE_MOTION;

const SHRINE: u64 = 0x4002_9900_0000_0063;
const PORTAL: u64 = 0x4002_9900_0000_0056;
const DOMINION_SHRINE: u32 = 0x4700_100D;
const SHRINE_PACKAGE: u32 = 0x13B1;
/// Where every test character starts, and the landblock beside it.
const START: u32 = 0x0299_0109;
const NEXT_LANDBLOCK: u16 = 0x029A;

fn object(id: u64, template: u32, package: u32, destination: Option<Place>) -> ContentObject {
    ContentObject {
        object: Some(id),
        template,
        description: Description {
            name_text: None,
            package,
            look: 0x1F00_0ED5,
            enter_fx: 0,
            scale: None,
            name: None,
            numbers: [0; 6],
            source: "authored for a test".into(),
        },
        cell: START,
        origin: [61.0, 240.2, -44.6],
        rotation: [1.0, 0.0, 0.0, 0.0],
        kind: Kind::Static,
        label: String::new(),
        placed: "authored for a test".into(),
        destination,
        facts: Facts::default(),
        walks: None,
        gate: None,
    }
}

/// A world with a Dominion shrine beside the start, the start's landblock of `here`.
fn world_with_a_shrine(here: Conflict) -> Harness {
    let mut h = Harness::new();
    let mut content = ContentPack {
        objects: vec![object(SHRINE, DOMINION_SHRINE, SHRINE_PACKAGE, None)],
        ..ContentPack::default()
    };
    if here != Conflict::Peace {
        content.conflict.insert((START >> 16) as u16, here);
    }
    h.server.content = content;
    h.server.shrine_packages.insert(SHRINE_PACKAGE);
    h.server
        .shrine_kingdoms
        .insert(SHRINE_PACKAGE, kingdom::DOMINION);
    h
}

/// The kingdom a player in the world holds.
fn kingdom_of(h: &Harness, me: u64) -> Option<u32> {
    h.server
        .players
        .values()
        .find(|p| p.character == me)
        .and_then(|p| kingdoms::of(&p.qualities))
}

fn log_off(h: &mut Harness, b: usize, id: u64) {
    let before = h.bots[b].inbox.len();
    h.bots[b].send(
        false,
        queue::LOGON,
        &ClientMessage::LogOff(auberean_protocol::login::CharacterRef { character: id }),
    );
    assert!(
        h.run_until(yalain_testkit::play::STEPS, |h| h.bots[b].inbox[before..]
            .iter()
            .any(|r| matches!(
                r.message(),
                Some(ServerMessage::CharacterList(_))
            )))
    );
}

/// Put a player in the world in a kingdom (as a join would leave it).
fn set_kingdom(h: &mut Harness, me: u64, k: u32) {
    for p in h.server.players.values_mut() {
        if p.character == me {
            kingdoms::set_kingdom(&mut p.qualities, k);
        }
    }
}

/// The values of every call of `f` a client was sent since `from`.
fn calls_of(h: &Harness, b: usize, from: usize, f: u32) -> Vec<Vec<Value>> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c) if c.function.0 == f => {
                Some(calls::values(&c.args.as_ref()?.0).unwrap_or_default())
            }
            _ => None,
        })
        .collect()
}

/// The conflict statuses a client was told since `from`.
fn statuses(h: &Harness, b: usize, from: usize) -> Vec<u32> {
    calls_of(h, b, from, kingdoms::STATUS)
        .iter()
        .filter_map(|v| match v.as_slice() {
            [Value::Int(s)] => Some(*s),
            _ => None,
        })
        .collect()
}

/// The visual whole-number qualities of `who` a client was told since `from`: (quality, value).
fn visuals(h: &Harness, b: usize, from: usize, who: u64) -> Vec<(u32, i32)> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::UpdateIntVisual(v) if v.object.id == who => Some((v.quality, v.value)),
            _ => None,
        })
        .collect()
}

/// The string references of the texts a client was shown since `from`, with the string id of a
/// nested variable when there is one.
fn texts(h: &Harness, b: usize, from: usize) -> Vec<((u32, u32), Option<u32>)> {
    calls_of(h, b, from, SHOW_TEXT)
        .iter()
        .filter_map(|v| {
            v.iter().find_map(|x| match x.root() {
                Some(Node::TextInfo(t)) => {
                    let nested = t.variables.iter().find_map(|var| match &var.value {
                        auberean_script::native_object::VariableValue::Nested(n) => {
                            Some(n.string_id)
                        }
                        _ => None,
                    });
                    Some(((t.string_id, t.table), nested))
                }
                _ => None,
            })
        })
        .collect()
}

/// The faction channel of each channel list a client was sent since `from` (each list one of a
/// kingdom's): its room, its faction and its name.
fn channel_lists(h: &Harness, b: usize, from: usize) -> Vec<(u32, u32, String)> {
    let world = h.server.cfg.world_name.clone();
    let mut out = Vec::new();
    for m in h.world_messages(b, from) {
        let ServerMessage::PlayerCall(c) = &m else {
            continue;
        };
        if c.function.0 != 0x8205_001E {
            continue;
        }
        for k in kingdom::ALL {
            let list = yalain_world::chat::channels_of(Some(k));
            let ours = yalain_world::chat::channels(&world, &list);
            if ours.encode() == m.encode() {
                let f = &list[3];
                out.push((f.room, f.faction, f.name.to_string()));
            }
        }
    }
    out
}

/// A player on foot who uses a faction shrine of another kingdom is told its rating is recorded,
/// and 3.83 s later joins the shrine's kingdom: its kingdom and its physics type change (every
/// client that knows it is told), it is told "You join {the} %1." naming the kingdom, and a moment
/// later it is sent its channels with its kingdom's faction channel. Its ethereality in a peace
/// area is the same in any kingdom, so it is not sent.
/// Behaviour: server.kingdoms.shrine-join
#[test]
fn a_player_on_foot_joins_a_shrines_kingdom_a_little_after_its_rating() {
    use yalain_world::usable::shrines::RECORDED;
    let mut h = world_with_a_shrine(Conflict::Peace);
    let (a, ida) = h.player_in_world("one", "Elara");
    let (b, _) = h.player_in_world("two", "Babyfare");
    h.run(10);
    assert_eq!(
        kingdom_of(&h, ida),
        None,
        "a new character is in no kingdom"
    );
    let (fa, fb) = (h.bots[a].inbox.len(), h.bots[b].inbox.len());
    h.bots[a].send(
        true,
        queue::WEENIE,
        &server_call(function::USE, &[use_of(SHRINE, ida)]),
    );
    // The rating: the moment the join is counted from.
    assert!(h.run_until(100, |h| texts(h, a, fa).iter().any(|t| t.0 == RECORDED)));
    let rated = h.now.0;
    h.run_to(yalain_net::LocalTime(rated + 3.83 - 0.2));
    h.run(3);
    assert_eq!(kingdom_of(&h, ida), None, "not yet");
    assert!(visuals(&h, a, fa, ida).is_empty(), "nothing yet");
    h.run_to(yalain_net::LocalTime(rated + 3.83 + 0.1));
    h.run(3);
    assert_eq!(kingdom_of(&h, ida), Some(kingdom::DOMINION));
    let dominion_physics = 0x2080_0020;
    assert_eq!(
        visuals(&h, a, fa, ida),
        [
            (quality::KINGDOM, 4),
            (quality::PHYSICS_TYPE, dominion_physics)
        ],
        "its kingdom, its physics type; no ethereality in peace"
    );
    assert_eq!(
        visuals(&h, b, fb, ida),
        [
            (quality::KINGDOM, 4),
            (quality::PHYSICS_TYPE, dominion_physics)
        ],
        "the other player sees it too"
    );
    let told = texts(&h, a, fa);
    assert_eq!(
        told.last(),
        Some(&(kingdoms::JOINED, Some(0x1270_95B3))),
        "joined the Dominion: {told:?}"
    );
    h.run(5);
    assert_eq!(
        channel_lists(&h, a, fa),
        [(5, 4, "Faction Faction3 ".to_string())],
        "its channels again, with the Dominion's"
    );
}

/// A player of the shrine's kingdom already is told its rating and nothing more; so is a player
/// in no kingdom at the neutral shrine. A player on a mount is told its use is canceled, and joins
/// nothing either.
/// Behaviour: server.kingdoms.shrine-of-its-own
#[test]
fn a_player_of_the_shrines_kingdom_already_joins_nothing() {
    for (shrine, mine, mounted) in [
        (kingdom::DOMINION, Some(kingdom::DOMINION), false),
        (kingdom::NEUTRAL, None, false),
        (kingdom::DOMINION, None, true),
    ] {
        let mut h = world_with_a_shrine(Conflict::Peace);
        h.server.shrine_kingdoms.insert(SHRINE_PACKAGE, shrine);
        let (a, ida) = h.player_in_world("one", "Elara");
        if let Some(k) = mine {
            set_kingdom(&mut h, ida, k);
        }
        if mounted {
            for p in h.server.players.values_mut() {
                p.riding = Some(yalain_world::mounts::Riding {
                    eid: 1,
                    state: 0,
                    look: auberean_protocol::types::VisualDesc::default(),
                    speed: 1.0,
                    checking: false,
                });
            }
        }
        h.run(10);
        let from = h.bots[a].inbox.len();
        h.bots[a].send(
            true,
            queue::WEENIE,
            &server_call(function::USE, &[use_of(SHRINE, ida)]),
        );
        h.run_to(yalain_net::LocalTime(h.now.0 + 10.0));
        h.run(5);
        assert_eq!(kingdom_of(&h, ida), mine, "{shrine:#x}");
        assert!(visuals(&h, a, from, ida).is_empty(), "{shrine:#x}");
        assert!(channel_lists(&h, a, from).is_empty(), "{shrine:#x}");
        let told = if mounted { 2 } else { 1 };
        assert_eq!(
            texts(&h, a, from).len(),
            told,
            "the rating (and the cancel) alone"
        );
    }
}

/// The texts a client was shown since `from`, whole.
fn shown(h: &Harness, b: usize, from: usize) -> Vec<auberean_script::native_object::TextInfo> {
    calls_of(h, b, from, SHOW_TEXT)
        .iter()
        .filter_map(|v| {
            v.iter().find_map(|x| match x.root() {
                Some(Node::TextInfo(t)) => Some(t.clone()),
                _ => None,
            })
        })
        .collect()
}

/// A text's variable `id` as a number, or the string id of the reference it holds.
fn variable(t: &auberean_script::native_object::TextInfo, id: u32) -> Option<u64> {
    use auberean_script::native_object::VariableValue;
    t.variables
        .iter()
        .find(|v| v.id == id)
        .and_then(|v| match &v.value {
            VariableValue::Word(n) => Some(u64::from(*n)),
            VariableValue::Long(n) => Some(*n),
            VariableValue::Nested(n) => Some(u64::from(n.string_id)),
            _ => None,
        })
}

/// Change a player in the world.
fn with_player(h: &mut Harness, me: u64, f: impl FnOnce(&mut yalain_world::world::Player)) {
    if let Some(p) = h.server.players.values_mut().find(|p| p.character == me) {
        f(p);
    }
}

fn set_level_and_money(h: &mut Harness, me: u64, level: i32, money: i32) {
    with_player(h, me, |p| {
        yalain_world::quals::set_int(&mut p.qualities, yalain_world::examine::LEVEL, level);
        yalain_world::quals::set_int(&mut p.qualities, yalain_world::store::MONEY, money);
    });
}

fn money_of(h: &Harness, me: u64) -> u32 {
    h.server
        .players
        .values()
        .find(|p| p.character == me)
        .map_or(0, |p| yalain_world::store::money(&p.qualities))
}

const DOMINION_NAME: u64 = 0x1270_95B3;
const NEUTRAL_NAME: u64 = 0x1268_95B3;
const TWO_WEEKS: u32 = 1_209_600;

/// A join's case: the player's level, its kingdom, the seconds since it left one and its money; and
/// the refusal it meets, with a variable of it and its value (none for a join).
type Case = (
    i32,
    Option<u32>,
    Option<i64>,
    i32,
    Option<((u32, u32), u32, u64)>,
);

/// A kingdom's join, as the world's row asks it, refuses in turn a player under its least level
/// (naming the level), one sworn to another kingdom, one that left a kingdom within its wait
/// (naming the kingdom), and one short of the gold its level costs (naming the gold): each is told
/// why after its rating, and its kingdom and money stay. A player past the wait who asks nothing
/// else of it joins, paying what its level costs (here nothing).
/// Behaviour: server.kingdoms.shrine-join-refusals
#[test]
fn a_kingdoms_join_refuses_the_low_the_sworn_the_recent_and_the_poor() {
    use kingdoms::join_text as t;
    use yalain_content::kingdoms::Join;
    let row = Join {
        least_level: 10,
        sworn_refused: true,
        wait: TWO_WEEKS,
        costs: vec![(0, 0), (41, 500_000)],
    };
    let cases: [Case; 5] = [
        (9, None, None, 0, Some((t::TOO_LOW, t::LEVEL, 10))),
        (10, Some(kingdom::ORDER), None, 0, Some((t::SWORN, 0, 0))),
        (
            10,
            Some(kingdom::NEUTRAL),
            Some(i64::from(TWO_WEEKS) - 60),
            0,
            Some((t::TOO_SOON, t::KINGDOM, DOMINION_NAME)),
        ),
        (
            41,
            None,
            None,
            499_999,
            Some((t::TOO_POOR, t::GOLD, 500_000)),
        ),
        (
            40,
            Some(kingdom::NEUTRAL),
            Some(i64::from(TWO_WEEKS) + 60),
            7,
            None,
        ),
    ];
    for (level, mine, since, money, refused) in cases {
        let mut h = world_with_a_shrine(Conflict::Peace);
        h.server
            .kingdom_rows
            .joins
            .insert(kingdom::DOMINION, row.clone());
        let (a, ida) = h.player_in_world("one", "Elara");
        set_level_and_money(&mut h, ida, level, money);
        if let Some(k) = mine {
            set_kingdom(&mut h, ida, k);
        }
        let unix = h.unix;
        with_player(&mut h, ida, |p| {
            p.play.left_kingdom = since.map_or(0, |s| unix - s);
        });
        h.run(10);
        let from = use_shrine(&mut h, a, ida);
        let told = shown(&h, a, from);
        let last = told.last().expect("told something");
        let case = format!("level {level}, {mine:?}, {since:?}, {money}");
        if let Some((text, var, value)) = refused {
            assert_eq!(told.len(), 2, "the rating, then why: {case}");
            assert_eq!((last.string_id, last.table), text, "{case}");
            if var != 0 {
                assert_eq!(variable(last, var), Some(value), "{case}");
            }
            assert_eq!(kingdom_of(&h, ida), mine, "{case}");
            assert!(visuals(&h, a, from, ida).is_empty(), "{case}");
        } else {
            assert_eq!((last.string_id, last.table), kingdoms::JOINED, "{case}");
            assert_eq!(kingdom_of(&h, ida), Some(kingdom::DOMINION), "{case}");
        }
        assert_eq!(
            money_of(&h, ida),
            money.cast_unsigned(),
            "nothing paid: {case}"
        );
    }
}

/// A join its row charges for takes the gold of the player's level band, the player's money sent
/// to it, and says "You join {the} %1 for the cost of %2 gold."; a player who so leaves one of the
/// three kingdoms is then kept as having left it, and another kingdom's shrine that waits after a
/// leaving refuses it until the wait is out, even after it has been away. A player in no kingdom
/// has left none.
/// Behaviour: server.kingdoms.shrine-join-cost-and-wait
#[test]
fn going_neutral_costs_the_gold_of_the_players_level_and_starts_the_wait_for_another_kingdom() {
    use kingdoms::join_text as t;
    use yalain_content::kingdoms::Join;
    let mut h = world_with_a_shrine(Conflict::Peace);
    h.server
        .shrine_kingdoms
        .insert(SHRINE_PACKAGE, kingdom::NEUTRAL);
    h.server.kingdom_rows.joins.insert(
        kingdom::NEUTRAL,
        Join {
            least_level: 10,
            costs: vec![(0, 0), (21, 15_000), (31, 75_000), (41, 500_000)],
            ..Join::default()
        },
    );
    h.server.kingdom_rows.joins.insert(
        kingdom::DOMINION,
        Join {
            least_level: 10,
            sworn_refused: true,
            wait: TWO_WEEKS,
            ..Join::default()
        },
    );
    let (a, ida) = h.player_in_world("one", "Elara");
    set_level_and_money(&mut h, ida, 25, 20_000);
    set_kingdom(&mut h, ida, kingdom::DOMINION);
    h.run(10);
    let from = use_shrine(&mut h, a, ida);
    assert_eq!(kingdom_of(&h, ida), Some(kingdom::NEUTRAL));
    assert_eq!(money_of(&h, ida), 5_000, "15,000 at level 25");
    assert!(
        h.world_messages(a, from).iter().any(|m| matches!(m,
            ServerMessage::UpdateInt(q) if q.quality == yalain_world::store::MONEY && q.value == 5_000)),
        "its money sent"
    );
    let told = shown(&h, a, from);
    let last = told.last().expect("told");
    assert_eq!((last.string_id, last.table), t::JOINED_FOR);
    assert_eq!(variable(last, t::KINGDOM), Some(NEUTRAL_NAME));
    assert_eq!(variable(last, t::GOLD), Some(15_000));
    let left = h
        .server
        .players
        .values()
        .find(|p| p.character == ida)
        .map(|p| p.play.left_kingdom)
        .expect("in the world");
    assert!(
        left > 0 && left <= h.unix,
        "kept as having left the Dominion"
    );

    // Back at the Dominion's shrine within the wait, after a day away from the world: refused.
    h.server
        .shrine_kingdoms
        .insert(SHRINE_PACKAGE, kingdom::DOMINION);
    h.unix += 86_400;
    let from = use_shrine(&mut h, a, ida);
    let told = shown(&h, a, from);
    assert_eq!(
        told.last().map(|l| (l.string_id, l.table)),
        Some(t::TOO_SOON)
    );
    assert_eq!(kingdom_of(&h, ida), Some(kingdom::NEUTRAL));

    // The wait out: it joins, and leaving the neutral kingdom starts no new wait.
    h.unix = left + i64::from(TWO_WEEKS);
    let from = use_shrine(&mut h, a, ida);
    assert_eq!(kingdom_of(&h, ida), Some(kingdom::DOMINION));
    assert_eq!(
        shown(&h, a, from).last().map(|l| (l.string_id, l.table)),
        Some(kingdoms::JOINED)
    );
    assert_eq!(
        h.server
            .players
            .values()
            .find(|p| p.character == ida)
            .map(|p| p.play.left_kingdom),
        Some(left),
        "it left the neutral kingdom, no kingdom of the three"
    );
}

/// The kingdom skills a player holds, as its play keeps them: the skill and its mask.
fn held_skills(h: &Harness, me: u64) -> Vec<(u32, u32)> {
    h.server
        .players
        .values()
        .find(|p| p.character == me)
        .map(|p| p.play.skills.iter().map(|k| (k.skill, k.mask)).collect())
        .unwrap_or_default()
}

/// A use of a shrine by a player in the world, run until its join would be through.
fn use_shrine(h: &mut Harness, a: usize, me: u64) -> usize {
    let from = h.bots[a].inbox.len();
    h.bots[a].send(
        true,
        queue::WEENIE,
        &server_call(function::USE, &[use_of(SHRINE, me)]),
    );
    h.run_to(yalain_net::LocalTime(h.now.0 + 10.0));
    h.run(5);
    from
}

/// A use of one's own kingdom's shrine grants each kingdom skill of its table that the player's
/// kingdom points earn and it does not hold, trained at no cost (a skill under another counting
/// as its child), tells each one and then "The shrine updates your Kingdom Skills!"; a skill the
/// points do not reach is not granted. A second use grants nothing and says nothing more; so does
/// a use at the points every player starts with, and a use of another kingdom's shrine.
/// Behaviour: server.kingdoms.kingdom-skills-granted
#[test]
fn a_players_own_shrine_grants_the_kingdom_skills_its_points_earn() {
    use yalain_world::kingdom_skills::{KingdomSkills, POINTS, UPDATED};
    use yalain_world::skills::{function::UPDATE_INFO, mask, SkillDef};
    const FIRST: u32 = 21;
    const UNDER_IT: u32 = 27;
    const FAR: u32 = 30;
    let with_table = |h: &mut Harness| {
        for (id, parents) in [(FIRST, vec![]), (UNDER_IT, vec![(FIRST, 0)]), (FAR, vec![])] {
            h.server.skill_table.skills.insert(
                id,
                SkillDef {
                    id,
                    cost: 1,
                    flags: 0x4C,
                    parents,
                    ..SkillDef::default()
                },
            );
        }
        h.server.kingdom_skills = KingdomSkills {
            by_kingdom: [(
                kingdom::DOMINION,
                vec![(1_000, FIRST), (2_800, UNDER_IT), (4_500, FAR)],
            )]
            .into(),
        };
    };
    let set_points = |h: &mut Harness, me: u64, points: i32| {
        for p in h.server.players.values_mut() {
            if p.character == me {
                yalain_world::quals::set_int(&mut p.qualities, POINTS, points);
            }
        }
    };

    let mut h = world_with_a_shrine(Conflict::Peace);
    with_table(&mut h);
    let (a, ida) = h.player_in_world("one", "Elara");
    set_kingdom(&mut h, ida, kingdom::DOMINION);
    set_points(&mut h, ida, 3_000);
    h.run(10);
    let from = use_shrine(&mut h, a, ida);
    let mut held = held_skills(&h, ida);
    held.sort_unstable();
    assert_eq!(
        held,
        [(FIRST, mask::TRAINED), (UNDER_IT, mask::TRAINED)],
        "the two its points earn, the one at 4,500 not"
    );
    let first = h
        .server
        .players
        .values()
        .find(|p| p.character == ida)
        .and_then(|p| p.play.skills.iter().find(|k| k.skill == FIRST).copied())
        .expect("the first granted");
    assert_eq!((first.children, first.cost), (1, 0), "a child, and no cost");
    assert_eq!(calls_of(&h, a, from, UPDATE_INFO).len(), 2, "each told");
    let told: Vec<_> = texts(&h, a, from).into_iter().map(|(t, _)| t).collect();
    assert_eq!(
        told,
        [yalain_world::usable::shrines::RECORDED, UPDATED],
        "its rating, then the update"
    );
    assert_eq!(
        kingdom_of(&h, ida),
        Some(kingdom::DOMINION),
        "joined nothing"
    );

    let from = use_shrine(&mut h, a, ida);
    assert!(
        calls_of(&h, a, from, UPDATE_INFO).is_empty(),
        "nothing more"
    );
    assert_eq!(texts(&h, a, from).len(), 1, "the rating alone");

    // At the points every player starts with, nothing.
    let mut h = world_with_a_shrine(Conflict::Peace);
    with_table(&mut h);
    let (a, ida) = h.player_in_world("one", "Elara");
    set_kingdom(&mut h, ida, kingdom::DOMINION);
    h.run(10);
    let from = use_shrine(&mut h, a, ida);
    assert!(held_skills(&h, ida).is_empty());
    assert!(calls_of(&h, a, from, UPDATE_INFO).is_empty());
    assert_eq!(texts(&h, a, from).len(), 1, "the rating alone");

    // Another kingdom's shrine: it joins, and is granted nothing.
    let mut h = world_with_a_shrine(Conflict::Peace);
    with_table(&mut h);
    let (a, ida) = h.player_in_world("one", "Elara");
    set_kingdom(&mut h, ida, kingdom::ORDER);
    set_points(&mut h, ida, 6_000);
    h.run(10);
    let from = use_shrine(&mut h, a, ida);
    assert_eq!(kingdom_of(&h, ida), Some(kingdom::DOMINION), "joined");
    assert!(held_skills(&h, ida).is_empty());
    assert!(calls_of(&h, a, from, UPDATE_INFO).is_empty());
}

/// A player's kingdom is kept: as it enters again its description and its object carry it (its
/// faction and its physics type's kingdom bit), and so does the object another player is made.
/// Behaviour: server.kingdoms.kept
#[test]
fn a_joined_kingdom_is_kept_and_carried_by_the_players_description_and_object() {
    let mut h = world_with_a_shrine(Conflict::Peace);
    let (a, ida) = h.player_in_world("one", "Elara");
    h.run(10);
    let from = h.bots[a].inbox.len();
    h.bots[a].send(
        true,
        queue::WEENIE,
        &server_call(function::USE, &[use_of(SHRINE, ida)]),
    );
    h.run_to(yalain_net::LocalTime(h.now.0 + 6.0));
    h.run(5);
    assert!(!visuals(&h, a, from, ida).is_empty(), "joined");
    log_off(&mut h, a, ida);
    let from = h.bots[a].inbox.len();
    h.enter_world(a, "one", ida);
    let (b, _) = h.player_in_world("two", "Babyfare");
    h.run(10);
    let faction =
        |w: &auberean_protocol::types::WeenieDesc| (w.mask & 0x2000 != 0).then_some(w.numbers[2]);
    let mine = h.world_messages(a, from);
    let desc = mine
        .iter()
        .find_map(|m| match m {
            ServerMessage::PlayerQualities(q) => Some(q.0.weenie.clone()),
            _ => None,
        })
        .expect("a description");
    assert_eq!(faction(&desc), Some(4));
    assert_eq!(desc.numbers[5], 0x2080_0020, "the Dominion's physics type");
    let object = |msgs: &[ServerMessage]| {
        msgs.iter().find_map(|m| match m {
            ServerMessage::CreateObject(o) if o.id == ida => Some(o.weenie.clone()),
            _ => None,
        })
    };
    let own = object(&mine).expect("its object");
    assert_eq!((faction(&own), own.numbers[5]), (Some(4), 0x2080_0020));
    let seen = object(&h.world_messages(b, 0)).expect("seen by the other");
    assert_eq!((faction(&seen), seen.numbers[5]), (Some(4), 0x2080_0020));
}

/// A player's profile names its kingdom; a player in none is shown in the neutral kingdom.
/// Behaviour: server.kingdoms.profile
#[test]
fn a_players_profile_names_its_kingdom() {
    use yalain_world::profile::{kingdom as name_of, line};
    let mut h = Harness::new();
    let (a, _) = h.player_in_world("one", "Elara");
    let (_, idb) = h.player_in_world("two", "Babyfare");
    h.run(10);
    let examine = |h: &mut Harness| {
        let from = h.bots[a].inbox.len();
        let mut b = yalain_world::graph::Builder::new();
        b.native(
            154,
            auberean_script::native_object::NativeObject::Packed {
                ty: 154,
                bytes: [0x4000_0002u32, 0]
                    .iter()
                    .flat_map(|w| w.to_le_bytes())
                    .chain(idb.to_le_bytes())
                    .chain(0x203u32.to_le_bytes())
                    .collect(),
            },
        );
        h.ask(
            a,
            function::EXAMINE,
            &[auberean_script::args::Arg::Object(b.graph())],
            10,
        );
        h.world_messages(a, from)
            .iter()
            .rev()
            .find_map(|m| match m {
                ServerMessage::PlayerCall(c)
                    if c.function.0 == yalain_world::profile::EXAMINATION_PROFILE =>
                {
                    Some(ServerMessage::PlayerCall(c.clone()).encode())
                }
                _ => None,
            })
            .expect("a profile")
    };
    let has = |bytes: &[u8], id: u32| bytes.windows(4).any(|w| w == id.to_le_bytes());
    let neutral = examine(&mut h);
    assert!(has(&neutral, line::NEUTRAL), "none: the neutral kingdom");
    set_kingdom(&mut h, idb, kingdom::ORDER);
    let order = examine(&mut h);
    assert!(has(&order, name_of(kingdom::ORDER)), "the Order");
    assert!(!has(&order, line::NEUTRAL), "and not the neutral one");
}

/// A player's faction channel is its kingdom's: the Shadow's room 3, the Order's 4, the
/// Dominion's 5, the neutral kingdom's (and a player in none) 6; a line said into a kingdom's
/// room reaches the players of that kingdom alone.
/// Behaviour: server.kingdoms.room-reaches-its-kingdom
#[test]
fn a_kingdoms_room_line_reaches_its_kingdoms_players_alone() {
    let mut h = Harness::new();
    let (a, ida) = h.player_in_world("one", "Elara");
    let (b, idb) = h.player_in_world("two", "Babyfare");
    let (c, _) = h.player_in_world("three", "Hirel");
    set_kingdom(&mut h, ida, kingdom::SHADOW);
    set_kingdom(&mut h, idb, kingdom::SHADOW);
    h.run(10);
    let k = [
        (Some(kingdom::SHADOW), 3),
        (Some(kingdom::ORDER), 4),
        (Some(kingdom::DOMINION), 5),
        (Some(kingdom::NEUTRAL), 6),
        (None, 6),
    ];
    for (kingdom, room) in k {
        assert_eq!(yalain_world::chat::channels_of(kingdom)[3].room, room);
        assert_eq!(kingdoms::room(kingdom), room);
    }
    let (fa, fb, fc) = (
        h.bots[a].inbox.len(),
        h.bots[b].inbox.len(),
        h.bots[c].inbox.len(),
    );
    let packet = SendToRoomById {
        context: 7,
        room: 3,
        text: "For the Shadow".into(),
        extra: ExtraBlob::Speaker {
            sender: 1,
            chat_type: 100 << 16 | 1,
        },
    }
    .network_packet()
    .expect("packs");
    h.bots[a].send(false, 4, &ClientMessage::Chat(ChatPacket(packet)));
    h.run(30);
    let heard = |h: &Harness, b: usize, from: usize| -> usize {
        h.bots[b].inbox[from..]
            .iter()
            .filter(|r| !r.world)
            .filter_map(|r| match r.message() {
                Some(ServerMessage::Chat(ChatPacket(p))) => decode_incoming(
                    &[
                        &u32::try_from(p.len()).expect("small").to_le_bytes()[..],
                        &p,
                    ]
                    .concat(),
                )
                .ok(),
                _ => None,
            })
            .filter(|i| matches!(&i.payload, IncomingPayload::RoomEvent(e) if e.room == 3))
            .count()
    };
    assert_eq!(heard(&h, a, fa), 1, "the speaker's echo");
    assert_eq!(heard(&h, b, fb), 1, "a player of the Shadow");
    assert_eq!(heard(&h, c, fc), 0, "a player in no kingdom");
}

/// The session start carries the conflict status of where the character enters, and nothing more
/// is said of it to a character that has been in the world; a new character is told it once more
/// right after its description, and its object's ethereality, its template's, is set right after
/// the arrival effect.
/// Behaviour: server.kingdoms.status-at-entry
#[test]
fn the_session_start_carries_the_status_of_where_the_character_enters() {
    let mut h = Harness::new();
    h.server
        .content
        .conflict
        .insert((START >> 16) as u16, Conflict::Controlled);
    let b = h.at_character_list("one");
    let id = h.create_character(b, "one", "Elara");
    let session_status = |h: &Harness, from: usize| -> Vec<(u32, u32)> {
        calls_of(h, b, from, SESSION_START)
            .iter()
            .filter_map(|v| match v.as_slice() {
                [.., Value::Int(local), Value::Int(server)] => Some((*local, *server)),
                _ => None,
            })
            .collect()
    };
    let from = h.bots[b].inbox.len();
    h.enter_world(b, "one", id);
    assert_eq!(
        session_status(&h, from),
        [(2, 0)],
        "controlled, and no server status"
    );
    assert_eq!(statuses(&h, b, from), [2], "a new character is told it");
    // A player in no kingdom in a controlled area passes through every kingdom, as in peace.
    assert_eq!(
        visuals(&h, b, from, id),
        [(quality::ETHEREALITY, 0x3E20_0010)]
    );
    log_off(&mut h, b, id);
    let from = h.bots[b].inbox.len();
    h.enter_world(b, "one", id);
    assert_eq!(session_status(&h, from), [(2, 0)]);
    assert!(statuses(&h, b, from).is_empty(), "told nothing more");
    assert!(visuals(&h, b, from, id).is_empty());
}

/// Walking into a landblock of another conflict status tells the player the new status; its
/// ethereality follows, and every client that knows it is told. Walking on into a landblock of
/// the same status says nothing.
/// Behaviour: server.kingdoms.status-on-the-way
#[test]
fn walking_into_a_landblock_of_another_status_tells_it_and_the_ethereality_follows() {
    let mut h = Harness::new();
    h.server
        .content
        .conflict
        .insert(NEXT_LANDBLOCK, Conflict::Controlled);
    h.server
        .content
        .conflict
        .insert(NEXT_LANDBLOCK + 1, Conflict::Controlled);
    let (a, ida) = h.player_in_world("one", "Elara");
    let (b, _) = h.player_in_world("two", "Babyfare");
    set_kingdom(&mut h, ida, kingdom::SHADOW);
    // Their clients have drawn the world they entered.
    for x in [a, b] {
        h.bots[x].send(
            true,
            queue::WEENIE,
            &ClientMessage::SceneRenderingComplete(Empty),
        );
    }
    h.run(10);
    let walk = |h: &mut Harness, x: usize, landblock: u16| {
        let cell = u32::from(landblock) << 16 | 0x0001;
        h.bots[x].send(
            true,
            queue::WEENIE,
            &report(cell, Vec3::new(10.0, 10.0, 0.0), 0x40, IDLE_MOTION, 0),
        );
        h.run(10);
    };
    // The other player goes ahead, to see the first come.
    walk(&mut h, b, NEXT_LANDBLOCK);
    let (fa, fb) = (h.bots[a].inbox.len(), h.bots[b].inbox.len());
    walk(&mut h, a, NEXT_LANDBLOCK);
    assert_eq!(statuses(&h, a, fa), [2], "a controlled area");
    // The Shadow there passes through the Shadow and the neutral kingdom.
    assert_eq!(
        visuals(&h, a, fa, ida),
        [(quality::ETHEREALITY, 0x0E20_0010)]
    );
    assert_eq!(
        visuals(&h, b, fb, ida),
        [(quality::ETHEREALITY, 0x0E20_0010)]
    );
    let (fa, fb) = (h.bots[a].inbox.len(), h.bots[b].inbox.len());
    walk(&mut h, a, NEXT_LANDBLOCK + 1);
    assert!(statuses(&h, a, fa).is_empty(), "the same status: nothing");
    assert!(visuals(&h, a, fa, ida).is_empty());
    assert!(visuals(&h, b, fb, ida).is_empty());
    walk(&mut h, a, 0x029C);
    assert_eq!(statuses(&h, a, fa), [1], "back in peace");
    assert_eq!(
        visuals(&h, a, fa, ida),
        [(quality::ETHEREALITY, 0x3E20_0010)]
    );
}

/// A player taken by a portal into a landblock of another conflict status is told the new status
/// once its arrival is through (its client has drawn the world), not before.
/// Behaviour: server.kingdoms.status-on-arrival
#[test]
fn arriving_through_a_portal_tells_the_status_once_the_world_is_drawn() {
    let mut h = Harness::new();
    let to = Place {
        cell: 0x7000_0001,
        origin: [10.0, 10.0, 50.0],
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    h.server.content = ContentPack {
        objects: vec![object(PORTAL, 0x4700_1154, 0x1A00, Some(to))],
        ..ContentPack::default()
    };
    h.server
        .content
        .conflict
        .insert(0x7000, Conflict::KingdomVsKingdom);
    let (a, ida) = h.player_in_world("one", "Elara");
    set_kingdom(&mut h, ida, kingdom::ORDER);
    h.run(10);
    let from = h.bots[a].inbox.len();
    h.bots[a].send(
        true,
        queue::WEENIE,
        &server_call(function::USE, &[use_of(PORTAL, ida)]),
    );
    h.run(60);
    assert!(statuses(&h, a, from).is_empty(), "not while arriving");
    h.bots[a].send(
        true,
        queue::WEENIE,
        &ClientMessage::SceneRenderingComplete(Empty),
    );
    h.run(10);
    assert_eq!(statuses(&h, a, from), [3], "kingdom against kingdom");
    // The Order there passes through its own kingdom alone.
    assert_eq!(
        visuals(&h, a, from, ida),
        [(quality::ETHEREALITY, 0x1220_0010)]
    );
}

/// A player's movement ethereality names the kingdoms of the players it may not fight where it
/// stands, by the client's own rule (a player in none counts as neutral): in peace every kingdom;
/// in a controlled area its own and the neutral one (every kingdom for a neutral player); kingdom
/// against kingdom its own; free for all none.
/// Behaviour: server.kingdoms.ethereality
#[test]
fn a_players_ethereality_passes_through_the_kingdoms_it_may_not_fight() {
    use kingdoms::ethereality;
    for k in [None, Some(1), Some(2), Some(4), Some(kingdom::NEUTRAL)] {
        assert_eq!(ethereality(1, k), 0x3E20_0010, "peace {k:?}");
        assert_eq!(ethereality(4, k), 0x0220_0010, "free for all {k:?}");
    }
    assert_eq!(ethereality(2, Some(1)), 0x0E20_0010);
    assert_eq!(ethereality(2, Some(2)), 0x1620_0010);
    assert_eq!(ethereality(2, Some(4)), 0x2620_0010);
    assert_eq!(ethereality(2, None), 0x3E20_0010);
    assert_eq!(ethereality(3, Some(1)), 0x0A20_0010);
    assert_eq!(ethereality(3, Some(2)), 0x1220_0010);
    assert_eq!(ethereality(3, Some(4)), 0x2220_0010);
    assert_eq!(ethereality(3, None), 0x0620_0010);
    assert_eq!(ethereality(5, Some(4)), 0x2220_0010);
    assert!(!kingdoms::may_fight(2, 1, kingdom::NEUTRAL));
    assert!(kingdoms::may_fight(2, 1, 2));
    assert!(!kingdoms::may_fight(3, 4, 4));
    assert!(kingdoms::may_fight(4, 4, 4));
    assert_eq!(kingdoms::physics_type(None), 0x0480_0020);
    assert_eq!(kingdoms::physics_type(Some(kingdom::NEUTRAL)), 0x0480_0020);
    assert_eq!(kingdoms::physics_type(Some(1)), 0x0880_0020);
    assert_eq!(kingdoms::physics_type(Some(2)), 0x1080_0020);
}

const WAR: u64 = 0x4002_9900_0000_0071;
const PEACE: u64 = 0x4002_9900_0000_0072;
const WAR_TEMPLATE: u32 = 0x4700_100E;
const PEACE_TEMPLATE: u32 = 0x4700_100C;
const BANNER_QUEST: u32 = 0x8000_02C7;

fn banner_quest() -> yalain_content::quests::QuestDef {
    yalain_content::quests::QuestDef {
        duration_by_calendar: false,
        id: BANNER_QUEST,
        name: yalain_content::Words::Ref(0xA860_7CEC, 0x2500_16A1),
        description: (0x6C45_9364, 0x2500_16A1),
        icon: 0x4100_034B,
        play_fx: false,
        journals: Vec::new(),
        challenge: -1,
        duration: 0.0,
        timed_phase: 0,
        retry: 0.0,
        xp: 0,
        cancellable: false,
        placed: "authored for a test".into(),
        hidden: true,
        effects: Vec::new(),
        subquests: Vec::new(),
        min_level: 0,
        fails_on_death: false,
        fails_on_logout: false,
        fails_on_teleport: false,
        once: false,
        fellows: yalain_content::quests::Fellows::default(),
        ranges: Vec::new(),
        hero_only: false,
        pay: yalain_content::quests::CompletionPay::default(),
        required: None,
        cancel_wait: None,
    }
}

/// A step of the banner quest run by a use of a template, moving nothing.
fn banner_step(key: &str, template: u32) -> yalain_content::quests::Step {
    yalain_content::quests::Step {
        out_of_turn: false,
        skills: Vec::new(),
        key: key.into(),
        quest: BANNER_QUEST,
        needs: 0,
        trigger: yalain_content::quests::Trigger::Use(template),
        tell: None,
        update: yalain_content::quests::Update::Nothing,
        xp: 0,
        gives: Vec::new(),
        counts: Vec::new(),
        effects: Vec::new(),
        text: None,
        recipes: Vec::new(),
        placed: "authored for a test".into(),
        requires: Vec::new(),
        takes: Vec::new(),
        races: Vec::new(),
        effects_last: false,
        told_refused: false,
        effects_quiet: false,
        wants: None,
        fellows: false,
        levels: None,
        xp_share: 0.0,
        to: None,
        transforms: None,
        chance: None,
        to_fellows: false,
    }
}

/// A use of a placed object by a player in the world, run until it is through.
fn use_object(h: &mut Harness, a: usize, me: u64, id: u64) -> usize {
    let from = h.bots[a].inbox.len();
    h.bots[a].send(
        true,
        queue::WEENIE,
        &server_call(function::USE, &[use_of(id, me)]),
    );
    h.run_to(yalain_net::LocalTime(h.now.0 + 10.0));
    h.run(5);
    from
}

/// The permissions a player in the world always lets through.
fn allowed(h: &Harness, me: u64) -> u32 {
    h.server
        .players
        .values()
        .find(|p| p.character == me)
        .map_or(0, |p| {
            kingdoms::permissions_of(&yalain_world::quals::read(&p.qualities)).0
        })
}

/// A step a world's row has grant the permission to be fought anywhere adds it to its player's
/// permissions: every client that knows the player is told them, and its ethereality in a peace
/// area follows (it passes through its own kingdom alone). Run again it changes nothing and tells
/// nothing. A step that revokes it takes it away, told the same, the player passing through every
/// kingdom again.
/// Behaviour: server.kingdoms.step-permissions
#[test]
fn a_step_grants_and_revokes_the_permission_to_be_fought_anywhere_and_every_watcher_is_told() {
    use yalain_content::kingdoms::Permits;
    let mut h = world_with_a_shrine(Conflict::Peace);
    for (id, template) in [(WAR, WAR_TEMPLATE), (PEACE, PEACE_TEMPLATE)] {
        h.server
            .content
            .objects
            .push(object(id, template, 0x19B, None));
    }
    h.server.quests = yalain_content::quests::QuestTable {
        quests: vec![banner_quest()],
        steps: vec![
            banner_step("war", WAR_TEMPLATE),
            banner_step("peace", PEACE_TEMPLATE),
        ],
        gives: Vec::new(),
        rooms: Vec::new(),
    };
    let anywhere = kingdoms::permission::ANYWHERE;
    h.server.kingdom_rows.permits.insert(
        (BANNER_QUEST, "war".into()),
        Permits {
            grant: anywhere,
            revoke: 0,
        },
    );
    h.server.kingdom_rows.permits.insert(
        (BANNER_QUEST, "peace".into()),
        Permits {
            grant: 0,
            revoke: anywhere,
        },
    );
    let (a, ida) = h.player_in_world("one", "Elara");
    let (b, _) = h.player_in_world("two", "Babyfare");
    set_kingdom(&mut h, ida, kingdom::DOMINION);
    h.run(10);
    assert_eq!(allowed(&h, ida), 0);

    let fb = h.bots[b].inbox.len();
    use_object(&mut h, a, ida, WAR);
    assert_eq!(allowed(&h, ida), anywhere, "granted");
    let told: Vec<_> = visuals(&h, b, fb, ida)
        .into_iter()
        .filter(|(q, _)| *q == quality::PERMISSIONS || *q == quality::ETHEREALITY)
        .collect();
    assert_eq!(
        told,
        [
            (quality::PERMISSIONS, anywhere.cast_signed()),
            (quality::ETHEREALITY, 0x2220_0010)
        ],
        "its permissions, then its own kingdom alone passed through"
    );

    let fb = h.bots[b].inbox.len();
    use_object(&mut h, a, ida, WAR);
    assert_eq!(allowed(&h, ida), anywhere);
    assert!(
        visuals(&h, b, fb, ida).is_empty(),
        "no change, nothing told"
    );

    let fb = h.bots[b].inbox.len();
    use_object(&mut h, a, ida, PEACE);
    assert_eq!(allowed(&h, ida), 0, "revoked");
    let told: Vec<_> = visuals(&h, b, fb, ida)
        .into_iter()
        .filter(|(q, _)| *q == quality::PERMISSIONS || *q == quality::ETHEREALITY)
        .collect();
    assert_eq!(
        told,
        [
            (quality::PERMISSIONS, 0),
            (quality::ETHEREALITY, 0x3E20_0010)
        ],
        "every kingdom passed through again"
    );
}

/// The session start's statuses a client was sent since `from`: (local, server).
fn session_statuses(h: &Harness, b: usize, from: usize) -> Vec<(u32, u32)> {
    calls_of(h, b, from, SESSION_START)
        .iter()
        .filter_map(|v| match v.as_slice() {
            [.., Value::Int(local), Value::Int(server)] => Some((*local, *server)),
            _ => None,
        })
        .collect()
}

/// A world's ruleset is the session start's server status: none on a standard world, whose areas
/// decide (peace where they name none); kingdom against kingdom or free for all on such a world,
/// where every landblock holds that status whatever its area names, an area of more conflict too.
/// A new character on a standard world is told the status of where it enters, one on a world with
/// a server status nothing (the client would print an entering text); the ethereality of a player
/// in no kingdom follows the status it holds.
/// Behaviour: server.kingdoms.world-ruleset
#[test]
fn a_worlds_ruleset_is_the_server_status_every_landblock_holds() {
    use kingdoms::Ruleset;
    use Conflict::{Controlled, FreeForAll, KingdomVsKingdom};
    // Each world's ruleset, the start's area (none where no area names it), the session start's
    // (local, server) statuses, the statuses told, and the ethereality of a player in no kingdom.
    let cases = [
        (Ruleset::Standard, None, (1, 0), Some(1), 0x3E20_0010),
        (
            Ruleset::Standard,
            Some(Controlled),
            (2, 0),
            Some(2),
            0x3E20_0010,
        ),
        (
            Ruleset::Standard,
            Some(FreeForAll),
            (4, 0),
            Some(4),
            0x0220_0010,
        ),
        (Ruleset::KingdomVsKingdom, None, (3, 3), None, 0x0620_0010),
        (
            Ruleset::KingdomVsKingdom,
            Some(Controlled),
            (3, 3),
            None,
            0x0620_0010,
        ),
        (
            Ruleset::KingdomVsKingdom,
            Some(FreeForAll),
            (3, 3),
            None,
            0x0620_0010,
        ),
        (Ruleset::FreeForAll, None, (4, 4), None, 0x0220_0010),
        (
            Ruleset::FreeForAll,
            Some(Controlled),
            (4, 4),
            None,
            0x0220_0010,
        ),
        (
            Ruleset::FreeForAll,
            Some(KingdomVsKingdom),
            (4, 4),
            None,
            0x0220_0010,
        ),
    ];
    for (ruleset, area, session, told, ethereal) in cases {
        let mut h = Harness::new();
        h.server.cfg.ruleset = ruleset;
        if let Some(a) = area {
            h.server.content.conflict.insert((START >> 16) as u16, a);
        }
        let (b, id) = h.player_in_world("one", "Elara");
        let case = format!("{ruleset:?} with {area:?}");
        assert_eq!(session_statuses(&h, b, 0), [session], "{case}");
        assert_eq!(statuses(&h, b, 0), Vec::from_iter(told), "{case}");
        let p = h
            .server
            .players
            .values()
            .find(|p| p.character == id)
            .expect("in the world");
        assert_eq!(kingdoms::kept_ethereality(&p.qualities), ethereal, "{case}");
    }
}

/// On a kingdom against kingdom world no area holds its own status: a player walking into a
/// controlled area, then on into a free for all one, is told no status and its ethereality stays.
/// Two players of one kingdom there may not harm each other, as kingdom against kingdom allows; and
/// a player whom kingdoms may fight anywhere counts as kingdom against kingdom, not as anywhere so,
/// where no area names any conflict.
/// Behaviour: server.kingdoms.world-ruleset-on-the-way
#[test]
fn on_a_kingdom_against_kingdom_world_no_area_tells_a_status_or_holds_its_own() {
    let mut h = Harness::new();
    h.server.cfg.ruleset = kingdoms::Ruleset::KingdomVsKingdom;
    h.server
        .content
        .conflict
        .insert(NEXT_LANDBLOCK, Conflict::Controlled);
    h.server
        .content
        .conflict
        .insert(NEXT_LANDBLOCK + 1, Conflict::FreeForAll);
    let (a, ida) = h.player_in_world("one", "Elara");
    let (b, idb) = h.player_in_world("two", "Oriel");
    for bot in [a, b] {
        h.bots[bot].send(
            true,
            queue::WEENIE,
            &ClientMessage::SceneRenderingComplete(Empty),
        );
    }
    h.run(10);
    set_kingdom(&mut h, ida, kingdom::DOMINION);
    set_kingdom(&mut h, idb, kingdom::DOMINION);
    let walk = |h: &mut Harness, bot: usize, landblock: u16| {
        let cell = u32::from(landblock) << 16 | 0x0001;
        h.bots[bot].send(
            true,
            queue::WEENIE,
            &report(cell, Vec3::new(10.0, 10.0, 0.0), 0x40, IDLE_MOTION, 0),
        );
        h.run(10);
    };
    let fighter = |h: &Harness, id: u64| {
        kingdoms::fighter(
            h.server
                .players
                .values()
                .find(|p| p.character == id)
                .expect("in the world"),
        )
    };
    let from = h.bots[a].inbox.len();
    walk(&mut h, a, NEXT_LANDBLOCK);
    walk(&mut h, a, NEXT_LANDBLOCK + 1);
    walk(&mut h, b, NEXT_LANDBLOCK);
    walk(&mut h, b, NEXT_LANDBLOCK + 1);
    assert!(statuses(&h, a, from).is_empty(), "no area tells a status");
    assert!(visuals(&h, a, from, ida).is_empty(), "nor an ethereality");
    let (fa, fb) = (fighter(&h, ida), fighter(&h, idb));
    assert_eq!((fa.status, fb.status), (3, 3), "a free for all area");
    assert_eq!(
        kingdoms::may_harm(&fb, &fa),
        Err(kingdoms::refusal::SAME_KINGDOM)
    );
    for p in h.server.players.values_mut() {
        if p.character == ida {
            kingdoms::set_permissions(&mut p.qualities, kingdoms::permission::ANYWHERE, 0);
        }
    }
    let from = h.bots[a].inbox.len();
    walk(&mut h, a, 0x029C);
    assert!(statuses(&h, a, from).is_empty());
    assert_eq!(
        fighter(&h, ida).status,
        3,
        "kingdom against kingdom, not anywhere so"
    );
}
