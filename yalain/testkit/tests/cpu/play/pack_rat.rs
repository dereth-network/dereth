//! The bags' perk (Pack Rat): its ranks give the last bag more places, those of the highest rank
//! held; training or untraining one tells the client the pack's size, its contents in their
//! segments and to lay out its bags again; untraining is refused while the last bag is full; a
//! reset leaves the ranks whose places hold items; and a bag that shrinks loses nothing. Fixture: a
//! new player in the world with test items, and six made-up ranks of the perk, each the child of
//! the one before, worth one place more than it.

use std::collections::HashMap;

use auberean_protocol::ServerMessage;
use auberean_script::args::Arg;
use yalain_net::ConnId;
use yalain_testkit::{test_catalog, Harness, TEST_ITEM};
use yalain_world::calls::{Node, Value};
use yalain_world::inventory::{self, function as pack_calls, SEGMENTS};
use yalain_world::player::quality::PACK_SIZE;
use yalain_world::quals;
use yalain_world::skills::{function, perk_kind, reset, Perk, SkillDef, SkillTable};
use yalain_world::state::{Held, SkillState};

/// The six ranks' skills, the first first.
const RANKS: [u32; 6] = [2038, 2039, 2040, 2041, 2042, 2043];
/// A skill that is no perk.
const PLAIN: u32 = 15;
const LAST_BAG: usize = 3;

/// A world whose perk ranks are worth these places; a player in it holding none, with credits to
/// train every rank, and an empty pack: the harness, its client, its character and connection.
fn player(values: [f64; 6]) -> (Harness, usize, u64, ConnId) {
    let mut h = Harness::new();
    h.server.catalog = test_catalog();
    let mut skills = HashMap::new();
    for (i, (skill, value)) in RANKS.into_iter().zip(values).enumerate() {
        skills.insert(
            skill,
            SkillDef {
                id: skill,
                name: (0xA860_7CEC, 0x2500_085B),
                cost: 1,
                flags: 0x1,
                parents: if i == 0 {
                    Vec::new()
                } else {
                    vec![(RANKS[i - 1], 0)]
                },
                perk: Some(Perk {
                    kind: perk_kind::BAGS,
                    value,
                    priority: i32::try_from(i).expect("few") + 1,
                }),
                ..SkillDef::default()
            },
        );
    }
    skills.insert(
        PLAIN,
        SkillDef {
            id: PLAIN,
            name: (0xA860_7CEC, 0x2500_085B),
            cost: 1,
            flags: 0x1,
            ..SkillDef::default()
        },
    );
    h.server.skill_table = SkillTable {
        skills,
        tables: HashMap::new(),
        ..SkillTable::default()
    };
    let (b, me) = h.player_in_world("one", "Elara");
    let world = *h.server.players.keys().next().expect("in");
    let p = h.server.players.get_mut(&world).expect("in");
    p.play.items.clear();
    p.play.credits = 10;
    h.run(2);
    (h, b, me, world)
}

/// Ranks worth one place, two, ... six, as the install's are.
fn one_to_six() -> [f64; 6] {
    [1.0, 2.0, 3.0, 4.0, 5.0, 6.0]
}

/// The player holds the first `n` ranks, as trained.
fn holds(h: &mut Harness, world: ConnId, n: usize) {
    let p = h.server.players.get_mut(&world).expect("in");
    p.play.skills.retain(|s| !RANKS.contains(&s.skill));
    for (i, skill) in RANKS.into_iter().enumerate().take(n) {
        p.play.skills.push(SkillState {
            skill,
            granted: 0,
            mask: 0x3,
            cost: 1,
            children: u32::from(i + 1 < n),
            dependents: 0,
            spent: 0,
        });
    }
}

/// The player's pack holds so many items in each segment, from each one's first slot.
fn fill(h: &mut Harness, world: ConnId, counts: [u32; 4]) -> Vec<u64> {
    let mut items = Vec::new();
    let mut id = 0x8130_0000_0FFF_0000;
    for (segment, n) in counts.into_iter().enumerate() {
        for place in 0..n {
            id += 1;
            items.push(Held {
                id,
                template: TEST_ITEM,
                slot: inventory::segment_start(segment) + place,
                location: 0,
                quantity: 1,
            });
        }
    }
    let ids = items.iter().map(|x| x.id).collect();
    h.server.players.get_mut(&world).expect("in").play.items = items;
    ids
}

/// The player's pack in its client's order (each segment's items in turn).
fn pack_order(h: &Harness, world: ConnId) -> Vec<u64> {
    let mut v: Vec<&Held> = h.server.players[&world]
        .play
        .items
        .iter()
        .filter(|x| x.location == 0)
        .collect();
    v.sort_by_key(|x| inventory::pack_order(x.slot));
    v.into_iter().map(|x| x.id).collect()
}

/// The player's segments as its contents would name them now: each one's places and items.
fn segments(h: &Harness, world: ConnId) -> Vec<(u32, u32)> {
    let p = &h.server.players[&world];
    inventory::pack_segments(&p.play.items, &inventory::rooms_of(&h.server, p))
}

/// What the client was told of its pack since `from`: its size quality's values, the
/// segments each contents message named, and each word to lay out its bags.
#[derive(Debug, PartialEq)]
enum Told {
    Size(i32),
    Segments(Vec<(u32, u32)>),
    LayOut,
}

fn told(h: &Harness, b: usize, from: usize) -> Vec<Told> {
    let mut out = Vec::new();
    for m in h.world_messages(b, from) {
        match m {
            ServerMessage::UpdateInt(q) if q.quality == PACK_SIZE => out.push(Told::Size(q.value)),
            ServerMessage::PlayerCall(c) if c.function.0 == pack_calls::UPDATE_BACKPACK => {
                out.push(Told::LayOut);
            }
            ServerMessage::PlayerCall(c) if c.function.0 == pack_calls::RESET_CONTENTS => {
                let v = c
                    .args
                    .as_ref()
                    .and_then(|a| yalain_world::calls::values(&a.0).ok())
                    .unwrap_or_default();
                let mut segments = Vec::new();
                for x in &v {
                    let Value::Graph(g) = x else { continue };
                    for (_, n) in g {
                        if let Node::Script { package, slots } = n {
                            if *package == 0x1AF6 {
                                segments.push((slots[0].1, slots[1].1));
                            }
                        }
                    }
                }
                out.push(Told::Segments(segments));
            }
            _ => {}
        }
    }
    out
}

/// Each of these was told once, and nothing else of the pack.
fn told_once(got: &[Told], want: &[Told]) {
    assert_eq!(got.len(), want.len(), "{got:?}");
    for w in want {
        assert!(got.contains(w), "{w:?} in {got:?}");
    }
}

fn ask(h: &mut Harness, b: usize, f: u32, args: &[Arg]) -> usize {
    let from = h.bots[b].inbox.len();
    h.ask(b, f, args, 5);
    from
}

fn holds_skill(h: &Harness, world: ConnId, skill: u32) -> bool {
    h.server.players[&world]
        .play
        .skills
        .iter()
        .any(|s| s.skill == skill)
}

/// A player holding three ranks has the third's three places more in its last bag, not the six the
/// three add up to: a pack and bags full to their own places take the next item into the last bag,
/// at the place past its own, and its segments name 15 places there.
/// Behaviour: server.inventory.the-bags-perk-grows-the-last-bag
#[test]
fn pack_rat_gives_the_last_bag_the_highest_ranks_places_more() {
    let (mut h, _b, _me, world) = player(one_to_six());
    holds(&mut h, world, 3);
    let before = fill(&mut h, world, SEGMENTS);
    let p = &h.server.players[&world];
    assert_eq!(
        inventory::rooms_of(&h.server, p),
        [42, 12, 12, 15],
        "the highest rank's, on the last bag"
    );
    let new = inventory::give(&mut h.server, &mut h.net, world, TEST_ITEM, 1).expect("given");
    h.run(2);
    let p = &h.server.players[&world];
    let slot = p
        .play
        .items
        .iter()
        .find(|x| x.id == new)
        .expect("held")
        .slot;
    assert_eq!(slot, 66, "at the front of the last bag, the one with room");
    assert_eq!(
        segments(&h, world),
        [(42, 42), (12, 12), (12, 12), (15, 13)]
    );
    let order = pack_order(&h, world);
    assert_eq!(
        order[..66],
        before[..66],
        "the pack and first bags as they were"
    );
    assert_eq!(order[66], new);
    assert_eq!(order[67..], before[66..], "the last bag's moved up one");
    for _ in 0..2 {
        inventory::give(&mut h.server, &mut h.net, world, TEST_ITEM, 1);
    }
    h.run(2);
    assert_eq!(segments(&h, world)[LAST_BAG], (15, 15), "every place held");
    inventory::give(&mut h.server, &mut h.net, world, TEST_ITEM, 1);
    h.run(2);
    assert_eq!(
        segments(&h, world),
        [(42, 43), (12, 12), (12, 12), (15, 15)],
        "then past every segment, the pack itself"
    );
}

/// Training a rank tells the client the pack's size, its contents with the last bag's new places,
/// and to lay out its bags again, and the size is kept; training a skill that is no
/// bags' perk tells none of it.
/// Behaviour: server.inventory.the-bags-perk-tells-the-client
#[test]
fn training_pack_rat_tells_the_client_the_packs_size_its_segments_and_to_lay_out_its_bags() {
    let (mut h, b, _me, world) = player(one_to_six());
    holds(&mut h, world, 1);
    inventory::bags_follow_skills(&mut h.server, &mut h.net, world);
    h.run(2);
    fill(&mut h, world, [3, 0, 0, 2]);
    let from = ask(&mut h, b, function::TRAIN, &[Arg::Int(RANKS[1])]);
    assert!(holds_skill(&h, world, RANKS[1]), "trained");
    told_once(
        &told(&h, b, from),
        &[
            Told::Size(80),
            Told::Segments(vec![(42, 3), (12, 0), (12, 0), (14, 2)]),
            Told::LayOut,
        ],
    );
    assert_eq!(
        quals::int(&h.server.players[&world].qualities, PACK_SIZE),
        Some(80),
        "kept"
    );
    let from = ask(&mut h, b, function::TRAIN, &[Arg::Int(PLAIN)]);
    assert!(holds_skill(&h, world, PLAIN), "trained");
    assert!(told(&h, b, from).is_empty(), "nothing of the pack");
}

/// A character holding ranks enters again with its pack's size and its grown last bag: kept, in
/// its description, and in its session start's segments.
/// Behaviour: server.inventory.the-bags-perk-tells-the-client
#[test]
fn a_character_holding_pack_rat_enters_with_its_packs_size_and_grown_last_bag() {
    use auberean_protocol::login::CharacterRef;
    use auberean_protocol::{queue, ClientMessage};
    let (mut h, b, me, world) = player(one_to_six());
    holds(&mut h, world, 4);
    fill(&mut h, world, [1, 0, 0, 14]);
    yalain_world::world::keep_player(&mut h.server, world);
    let before = h.bots[b].inbox.len();
    h.bots[b].send(
        false,
        queue::LOGON,
        &ClientMessage::LogOff(CharacterRef { character: me }),
    );
    assert!(h.run_until(400, |h| h.bots[b].inbox[before..]
        .iter()
        .any(|r| matches!(r.message(), Some(ServerMessage::CharacterList(_))))));
    let from = h.bots[b].inbox.len();
    h.enter_world(b, "one", me);
    let world = *h.server.players.keys().next().expect("in again");
    let p = &h.server.players[&world];
    assert_eq!(quals::int(&p.qualities, PACK_SIZE), Some(82), "kept");
    let described = h.world_messages(b, from).into_iter().find_map(|m| match m {
        ServerMessage::PlayerQualities(q) => Some(q),
        _ => None,
    });
    let described = described.expect("described");
    assert_eq!(described.0.ints.get(PACK_SIZE), Some(&82), "described");
    let start = h
        .calls(b, from)
        .into_iter()
        .find(|(f, _)| *f == yalain_world::session::function::SESSION_START)
        .map(|(_, v)| v)
        .expect("a session start");
    let named: Vec<(u32, u32)> = start
        .iter()
        .filter_map(|v| match v {
            Value::Graph(g) => Some(g),
            _ => None,
        })
        .flat_map(|g| g.iter())
        .filter_map(|(_, n)| match n {
            Node::Script { package, slots } if *package == 0x1AF6 => Some((slots[0].1, slots[1].1)),
            _ => None,
        })
        .collect();
    assert_eq!(named, [(42, 1), (12, 0), (12, 0), (16, 14)]);
}

/// Untraining the highest rank is refused while its grown last bag has no free place, as the bags'
/// own would be; with one free it is untrained, the bag a place smaller and every item where it was,
/// and the client told.
/// Behaviour: server.skills.the-bags-perk-wants-room
#[test]
fn untraining_pack_rat_is_refused_while_its_grown_last_bag_is_full() {
    let (mut h, b, _me, world) = player(one_to_six());
    holds(&mut h, world, 3);
    inventory::bags_follow_skills(&mut h.server, &mut h.net, world);
    h.run(2);
    let items = fill(&mut h, world, [0, 0, 0, 15]);
    let from = ask(&mut h, b, function::UNTRAIN, &[Arg::Int(RANKS[2])]);
    assert!(holds_skill(&h, world, RANKS[2]), "full: refused");
    assert!(told(&h, b, from).is_empty());
    h.server
        .players
        .get_mut(&world)
        .expect("in")
        .play
        .items
        .pop();
    let from = ask(&mut h, b, function::UNTRAIN, &[Arg::Int(RANKS[2])]);
    assert!(!holds_skill(&h, world, RANKS[2]), "a free place: untrained");
    assert_eq!(segments(&h, world)[LAST_BAG], (14, 14));
    assert_eq!(
        pack_order(&h, world),
        items[..14],
        "every item where it was"
    );
    told_once(
        &told(&h, b, from),
        &[
            Told::Size(80),
            Told::Segments(vec![(42, 0), (12, 0), (12, 0), (14, 14)]),
            Told::LayOut,
        ],
    );
}

/// A rank whose places shrink the last bag past the items it holds (ranks worth one, then five)
/// leaves none of them out: those past the places left go to the pack itself, after its own items,
/// in their order.
/// Behaviour: server.inventory.a-shrunk-last-bag-loses-nothing
#[test]
fn items_past_a_shrunk_last_bag_go_to_the_pack_itself_in_their_order() {
    let (mut h, b, _me, world) = player([1.0, 5.0, 6.0, 7.0, 8.0, 9.0]);
    holds(&mut h, world, 2);
    inventory::bags_follow_skills(&mut h.server, &mut h.net, world);
    h.run(2);
    let items = fill(&mut h, world, [2, 0, 0, 16]);
    ask(&mut h, b, function::UNTRAIN, &[Arg::Int(RANKS[1])]);
    assert!(!holds_skill(&h, world, RANKS[1]), "a free place: untrained");
    assert_eq!(
        segments(&h, world),
        [(42, 5), (12, 0), (12, 0), (13, 13)],
        "three to the pack itself"
    );
    let order = pack_order(&h, world);
    assert_eq!(order.len(), items.len(), "none lost");
    assert_eq!(order[..2], items[..2], "the pack's own first");
    assert_eq!(
        order[2..5],
        items[15..],
        "then the three past the bag's places"
    );
    assert_eq!(order[5..], items[2..15], "the bag's first thirteen stay");
}

/// A reset leaves the ranks whose places the last bag's items are in, as untraining one at a time
/// from the highest would stop at a full bag: of six ranks with 14 items there, the first two stay
/// and the rest go; with none there, all go.
/// Behaviour: server.skills.a-held-reset-untrains-every-bought-skill
#[test]
fn a_reset_leaves_the_pack_rat_ranks_whose_places_hold_items() {
    for (in_bag, kept) in [(14, 2), (0, 0)] {
        let (mut h, b, _me, world) = player(one_to_six());
        holds(&mut h, world, 6);
        inventory::bags_follow_skills(&mut h.server, &mut h.net, world);
        h.run(2);
        h.run(2);
        let items = fill(&mut h, world, [0, 0, 0, in_bag]);
        let p = h.server.players.get_mut(&world).expect("in");
        quals::set_int(&mut p.qualities, reset::SKILLS_HELD, 1);
        ask(&mut h, b, function::SELL_BACK_ALL, &[]);
        let held: Vec<bool> = RANKS.iter().map(|r| holds_skill(&h, world, *r)).collect();
        let want: Vec<bool> = (0..6).map(|i| i < kept).collect();
        assert_eq!(held, want, "{in_bag} in the bag");
        assert_eq!(pack_order(&h, world), items, "every item where it was");
        let places = 12 + u32::try_from(kept).expect("few");
        assert_eq!(segments(&h, world)[LAST_BAG], (places, in_bag));
    }
}
