//! Butchery: a hunting knife used on a corpse, the walk to it, the work, the material gathered and
//! the butchery skill's experience; a corpse butchered twice, one another player is opening, one
//! its killer far outleveled, a cut that finds nothing; the knife's wear, and its wear kept across
//! a logout. Fixture: the test world's beetle (level 2) given a butchery profile that yields bone,
//! a crude knife with durability in the player's pack.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_content::{Prop, Props};
use yalain_testkit::play::server_call;
use yalain_testkit::{Harness, BUTCHERY, TEST_CREATURE};
use yalain_world::butchery::{self, refusal, wear, Yield};
use yalain_world::calls::{self, Node, Value};
use yalain_world::things::{Corpse, Kind, Thing};

const PROFILE: u32 = 0x6F00_12DD;
const BONE: u32 = 0x4700_06C8;
const CRUDE_KNIFE: u32 = 0x4700_0CBD;
const USE_DONE: u32 = 0x8216_0021;
const SHOW_TEXT: u32 = 0x8205_0011;

/// A use of a held item on an object, as the client asks for it.
fn use_on(item: u64, target: u64, me: u64) -> Arg {
    use yalain_world::graph::{Builder, Slot};
    let mut b = Builder::new();
    b.object(
        0x245,
        &[
            Slot::Ref(None),
            Slot::Ref(None),
            Slot::Long(item),
            Slot::Ref(None),
            Slot::Int(0),
            Slot::Long(me),
            Slot::Int((-1.0f32).to_bits()),
            Slot::Long(target),
            Slot::Int(0),
            Slot::Long(0),
            Slot::Int(0),
            Slot::Ref(None),
            Slot::Int(0),
            Slot::Int(316),
            Slot::Int(0),
            Slot::Int(0),
        ],
    );
    Arg::Object(b.graph())
}

/// A player in the world with a crude knife, and a beetle's corpse 1.5 m away; the client, the
/// player, its world connection, the knife and the corpse.
fn at_a_corpse() -> (Harness, usize, u64, yalain_net::ConnId, u64, u64) {
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    let (b, me) = h.player_in_world("one", "Elara");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
    let world = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .expect("in the world")
        .0;
    let mut beetle = h
        .server
        .catalog
        .get(TEST_CREATURE)
        .cloned()
        .expect("the beetle");
    beetle.props.set("ButcheryProfile", Prop::Word(PROFILE));
    h.server.catalog.insert(beetle.clone());
    h.server
        .catalog
        .insert(yalain_testkit::test_template(BONE, 0x3A1, Props::default()));
    let mut knife = Props::default();
    knife.set("Item_Durability_MaxLevel", Prop::Int(KNIFE_DURABILITY));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(CRUDE_KNIFE, 0x3A1, knife));
    h.server.crafting.butchery.insert(
        PROFILE,
        vec![Yield {
            template: BONE,
            weight: 1,
        }],
    );
    yalain_world::inventory::give(&mut h.server, &mut h.net, world, CRUDE_KNIFE, 1);
    h.run(3);
    let knife = h.server.players[&world].play.items[0].id;
    let id = corpse_by(&mut h, world, me, 0);
    (h, b, me, world, knife, id)
}

/// A crude knife's durability in the fixture.
const KNIFE_DURABILITY: i64 = 50;

/// Another beetle's corpse 1.5 m from the player, killed by `killer` at a level (0: no level
/// kept); its id.
fn corpse_by(h: &mut Harness, world: yalain_net::ConnId, killer: u64, killer_level: i32) -> u64 {
    let beetle = h
        .server
        .catalog
        .get(TEST_CREATURE)
        .cloned()
        .expect("the beetle");
    let mut at = h.server.players[&world].position;
    at.origin[0] += 1.5;
    let id = h.server.things.new_id();
    let mut corpse = Thing::of(id, &beetle, Kind::Corpse);
    corpse.at = Some(at);
    corpse.corpse = Some(Corpse {
        killer,
        originator: 0,
        contents: Vec::new(),
        expires: f64::MAX,
        creature: TEST_CREATURE,
        butchered: false,
        killer_level,
        ..Default::default()
    });
    h.server.things.insert(corpse);
    id
}

/// Ask to butcher a corpse with a knife and run the world `seconds` on; the inbox mark before.
fn butcher(h: &mut Harness, b: usize, me: u64, knife: u64, corpse: u64, seconds: f64) -> usize {
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(0x8216_0016, &[use_on(knife, corpse, me)]),
    );
    h.run(4);
    let t = h.now.0 + seconds;
    while h.now.0 < t {
        h.step();
    }
    from
}

/// The knife's durability updates a client was sent since `from`.
fn wear_updates(h: &Harness, b: usize, from: usize, knife: u64) -> Vec<i32> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::UpdateIntVisual(q)
                if q.object.id == knife && q.quality == wear::DURABILITY =>
            {
                Some(q.value)
            }
            _ => None,
        })
        .collect()
}

/// The use answers a client was sent since `from`: (status, the message's string id).
fn use_answers(h: &Harness, b: usize, from: usize) -> Vec<(u32, u32)> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c) if c.function.0 == USE_DONE => {
                let v = c.args.as_ref().and_then(|a| calls::values(&a.0).ok())?;
                let Some(Value::Graph(g)) = v.first() else {
                    return None;
                };
                let status = g.iter().find_map(|(_, n)| match n {
                    Node::Script { slots, .. } => Some(slots[11].1),
                    _ => None,
                })?;
                let text = g.iter().find_map(|(_, n)| match n {
                    Node::TextInfo(t) => Some(t.string_id),
                    _ => None,
                });
                Some((status, text.unwrap_or(0)))
            }
            _ => None,
        })
        .collect()
}

/// A crude knife used on a corpse: the player walks to it (to 3 m), plays its butchering behavior
/// and lets it go, and about two seconds later the butchery skill's experience (its pool, then its
/// record), the craft effect, the material made in the pack (11 units, or 22 for an expert cut)
/// and the use answered "You butcher ... units of ... from ...". The same corpse again is refused,
/// butchered already.
/// Behaviour: server.craft.butchery-gathers-a-material
#[test]
fn a_knife_used_on_a_corpse_butchers_it_once_for_a_material_and_experience() {
    let (mut h, b, me, world, knife, corpse) = at_a_corpse();
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(0x8216_0016, &[use_on(knife, corpse, me)]),
    );
    h.run(4);
    let msgs = h.world_messages(b, from);
    assert!(msgs.iter().any(|m| matches!(m,
        ServerMessage::MoveTo(e) if e.sender.id == me
            && e.action.params.flags == 0x63
            && (e.action.params.desired_distance - butchery::REACH).abs() < 1e-6)));
    assert!(msgs.iter().any(|m| matches!(m,
        ServerMessage::PlayBehavior(e) if e.sender.id == me
            && e.action.behavior == butchery::BEHAVIOR)));
    assert!(msgs.iter().any(|m| matches!(m,
        ServerMessage::LetGoBehavior(e) if e.action.behavior == butchery::BEHAVIOR)));
    assert!(use_answers(&h, b, from).is_empty(), "not done yet");
    let t = h.now.0 + 2.2;
    while h.now.0 < t {
        h.step();
    }
    let got = use_answers(&h, b, from);
    assert_eq!(got.len(), 1, "answered once");
    assert_eq!(got[0].0, 0, "not refused");
    assert!(
        got[0].1 == butchery::text::BUTCHERED.0 || got[0].1 == butchery::text::EXPERTLY.0,
        "the butchery's text"
    );
    let p = &h.server.players[&world];
    let bone = p
        .play
        .items
        .iter()
        .find(|i| i.template == BONE)
        .expect("bone");
    assert!(
        bone.quantity == 11 || bone.quantity == 22,
        "{}",
        bone.quantity
    );
    let record = p.play.craft_skills.iter().find(|c| c.skill == BUTCHERY);
    assert_eq!(record.map(|c| c.today), Some(2), "a level 1 cut pays 2");
    assert_eq!(yalain_world::quals::long(&p.qualities, 1001), Some(1));
    let msgs = h.world_messages(b, from);
    assert!(msgs.iter().any(|m| matches!(m,
        ServerMessage::DoFx(e) if e.sender.id == me
            && (e.action.fx == 0x4100_051E || e.action.fx == 0x4100_051F))));

    let again = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(0x8216_0016, &[use_on(knife, corpse, me)]),
    );
    h.run(4);
    assert_eq!(
        use_answers(&h, b, again).first().map(|a| a.0),
        Some(refusal::BUTCHERED)
    );
}

/// How many units a knife cuts: ten above its own yield, whatever the creature: a crude knife's 1
/// gives 11; an item that is not a knife cuts nothing.
/// Behaviour: server.craft.butchery-units-by-knife
#[test]
fn a_knife_cuts_ten_units_above_its_own_yield() {
    assert_eq!(
        butchery::units(butchery::knife_yield(CRUDE_KNIFE, None)),
        11
    );
    assert_eq!(
        butchery::units(butchery::knife_yield(0x4700_0CBB, None)),
        16
    );
    // 2017's examined knives: yield 16 cut 26, 22 cut 32, 25 cut 35, 28 cut 38, 33 cut 43.
    for (own, cut) in [(16, 26), (22, 32), (25, 35), (28, 38), (33, 43)] {
        assert_eq!(
            butchery::units(butchery::knife_yield(0x4700_0CBB, Some(own))),
            cut
        );
    }
    assert!(butchery::is_knife(CRUDE_KNIFE) && butchery::is_knife(0x4700_0CBB));
    assert!(!butchery::is_knife(BONE));
}

/// An expert cut's chance goes by the butcher's level alone, a six hundredth of it: 25% at level
/// 150, nothing at no level.
/// Behaviour: server.craft.butchery-expert-by-skill
#[test]
fn an_expert_cuts_chance_goes_by_the_butchers_level_alone() {
    assert!((butchery::expert_chance(150) - 0.25).abs() < 1e-6);
    assert!((butchery::expert_chance(60) - 0.1).abs() < 1e-6);
    assert!(butchery::expert_chance(0).abs() < 1e-6);
    // A cut asks it of the butcher's own level: at the level of certainty every cut is an expert one.
    let (mut h, b, me, world, knife, corpse) = at_a_corpse();
    let state = yalain_world::state::CraftSkillState {
        skill: BUTCHERY,
        level: 600,
        today: 0,
        available: 0,
    };
    let held = &mut h
        .server
        .players
        .get_mut(&world)
        .expect("in")
        .play
        .craft_skills;
    held.retain(|c| c.skill != BUTCHERY);
    held.push(state);
    let mut corpses = vec![corpse];
    for _ in 0..4 {
        corpses.push(corpse_by(&mut h, world, me, 0));
    }
    for c in corpses {
        let from = butcher(&mut h, b, me, knife, c, 2.2);
        let got = use_answers(&h, b, from);
        assert_eq!(got.len(), 1, "answered once");
        assert_eq!(got[0].1, butchery::text::EXPERTLY.0);
    }
}

/// A corpse another player is on its way to open is refused at once with "target locked"; once
/// that player is no longer on its way, it is butchered.
/// Behaviour: server.craft.butchery-refused-while-another-opens
#[test]
fn a_corpse_another_player_is_on_its_way_to_open_is_refused() {
    let (mut h, b, me, _, knife, corpse) = at_a_corpse();
    let (_, other) = h.player_in_world("two", "Rowan");
    let other_world = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == other)
        .expect("in the world")
        .0;
    let from_where = h.server.players[&other_world].position;
    let opening = yalain_world::places::Use {
        item: corpse,
        target: 0,
        slots: Vec::new(),
        message: None,
        from: from_where,
    };
    if let Some(p) = h.server.players.get_mut(&other_world) {
        p.walking_to = Some((opening, 2.0));
    }
    let from = butcher(&mut h, b, me, knife, corpse, 0.0);
    assert_eq!(
        use_answers(&h, b, from).first().map(|a| a.0),
        Some(refusal::LOCKED)
    );
    assert!(
        !h.world_messages(b, from).iter().any(|m| matches!(m,
            ServerMessage::PlayBehavior(e) if e.action.behavior == butchery::BEHAVIOR)),
        "no work begun"
    );
    if let Some(p) = h.server.players.get_mut(&other_world) {
        p.walking_to = None;
    }
    let from = butcher(&mut h, b, me, knife, corpse, 2.2);
    assert_eq!(use_answers(&h, b, from).first().map(|a| a.0), Some(0));
}

/// A corpse whose killer was 9 or more levels above the creature is too damaged to butcher, refused
/// at once; one killed 8 levels above is butchered.
/// Behaviour: server.craft.butchery-too-damaged
#[test]
fn a_corpse_its_killer_far_outleveled_is_too_damaged_to_butcher() {
    let (mut h, b, me, world, knife, _) = at_a_corpse();
    let damaged = corpse_by(&mut h, world, me, 2 + 9);
    let from = butcher(&mut h, b, me, knife, damaged, 0.0);
    assert_eq!(
        use_answers(&h, b, from).first().map(|a| a.0),
        Some(refusal::TOO_DAMAGED)
    );
    let fit = corpse_by(&mut h, world, me, 2 + 8);
    let from = butcher(&mut h, b, me, knife, fit, 2.2);
    assert_eq!(use_answers(&h, b, from).first().map(|a| a.0), Some(0));
    assert!(butchery::too_damaged(20, 11));
    assert!(!butchery::too_damaged(19, 11));
    assert!(!butchery::too_damaged(0, -20), "no player killed it");
}

/// A cut that draws its profile's share of nothing is answered "empty" at the result: no material,
/// no experience, and the corpse may be butchered again.
/// Behaviour: server.craft.butchery-finds-nothing
#[test]
fn a_cut_that_finds_nothing_is_answered_empty_and_leaves_the_corpse() {
    let (mut h, b, me, world, knife, corpse) = at_a_corpse();
    h.server.crafting.butchery.insert(
        PROFILE,
        vec![Yield {
            template: butchery::NOTHING_FOUND,
            weight: 1,
        }],
    );
    let from = butcher(&mut h, b, me, knife, corpse, 2.2);
    let msgs = h.world_messages(b, from);
    assert!(
        msgs.iter().any(|m| matches!(m,
            ServerMessage::PlayBehavior(e) if e.action.behavior == butchery::BEHAVIOR)),
        "the work done"
    );
    assert_eq!(use_answers(&h, b, from), [(refusal::EMPTY, 0)]);
    let p = &h.server.players[&world];
    assert!(
        !p.play.items.iter().any(|i| i.template == BONE),
        "no material"
    );
    assert_eq!(
        p.play
            .craft_skills
            .iter()
            .find(|c| c.skill == BUTCHERY)
            .map(|c| c.today),
        Some(0),
        "no experience"
    );
    assert_eq!(
        h.server
            .things
            .get(corpse)
            .and_then(|t| t.corpse.as_ref())
            .map(|c| c.butchered),
        Some(false),
        "still to butcher"
    );
}

/// Each cut lowers the knife's durability by one at even odds, the client told the new value; a
/// knife worn to nothing is destroyed with "Your ... has been destroyed through overuse!" and
/// leaves the pack.
/// Behaviour: server.craft.knife-wears-out
#[test]
fn a_knife_loses_durability_on_about_half_its_cuts_and_breaks_at_nothing() {
    let (mut h, b, me, world, knife, _) = at_a_corpse();
    let mut lowered = Vec::new();
    for _ in 0..20 {
        let corpse = corpse_by(&mut h, world, me, 0);
        let from = butcher(&mut h, b, me, knife, corpse, 2.2);
        assert_eq!(use_answers(&h, b, from).first().map(|a| a.0), Some(0));
        lowered.extend(wear_updates(&h, b, from, knife));
    }
    let start = i32::try_from(KNIFE_DURABILITY).expect("small");
    let expected: Vec<i32> = (1..=i32::try_from(lowered.len()).expect("few"))
        .map(|n| start - n)
        .collect();
    assert_eq!(lowered, expected, "one at a time");
    assert!(
        (3..=17).contains(&lowered.len()),
        "about half of 20 cuts: {}",
        lowered.len()
    );
    // Worn down to its last point, the knife breaks on the cut that wears it.
    if let Some(t) = h.server.things.get_mut(knife) {
        t.durability = Some(1);
    }
    let mut broke = None;
    for _ in 0..40 {
        let corpse = corpse_by(&mut h, world, me, 0);
        let from = butcher(&mut h, b, me, knife, corpse, 2.2);
        if h.server.things.get(knife).is_none() {
            broke = Some(from);
            break;
        }
    }
    let from = broke.expect("worn out within 40 cuts");
    assert!(
        wear_updates(&h, b, from, knife).is_empty(),
        "no update to nothing"
    );
    let said = h.world_messages(b, from).iter().any(|m| match m {
        ServerMessage::PlayerCall(c) if c.function.0 == SHOW_TEXT => c
            .args
            .as_ref()
            .and_then(|a| calls::values(&a.0).ok())
            .is_some_and(|v| {
                v.iter().any(|x| match x {
                    Value::Graph(g) => g.iter().any(
                        |(_, n)| matches!(n, Node::TextInfo(t) if t.string_id == wear::DESTROYED.0),
                    ),
                    _ => false,
                })
            }),
        _ => false,
    });
    assert!(said, "destroyed through overuse");
    assert!(
        !h.server.players[&world]
            .play
            .items
            .iter()
            .any(|i| i.id == knife),
        "out of the pack"
    );
}

/// A knife's wear is kept with it across a logout.
/// Behaviour: server.craft.tool-wear-kept-across-sessions
#[test]
fn a_worn_knife_keeps_its_durability_across_a_logout() {
    use auberean_protocol::login::CharacterRef;
    use auberean_protocol::ClientMessage;
    let (mut h, b, me, world, knife, _) = at_a_corpse();
    if let Some(t) = h.server.things.get_mut(knife) {
        t.durability = Some(7);
    }
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
    assert!(h.server.things.get(knife).is_none(), "out of the world");
    h.enter_world(b, "one", me);
    assert_eq!(
        h.server.things.get(knife).and_then(|t| t.durability),
        Some(7)
    );
}

/// A made knife cuts ten above its own yield, whatever its template, and wears down from its own
/// durability.
/// Behaviour: server.craft.butchery-units-by-knife
#[test]
fn a_made_knife_cuts_by_its_own_yield_and_wears_from_its_own_durability() {
    let (mut h, b, me, world, knife, corpse) = at_a_corpse();
    h.server.things.get_mut(knife).expect("the knife").own.made = Some(yalain_world::made::Made {
        tool_yield: Some(25),
        durability: Some(200),
        craft_needed: Some(1),
        ..yalain_world::made::Made::default()
    });
    let from = butcher(&mut h, b, me, knife, corpse, 5.0);
    let p = &h.server.players[&world];
    let got = p
        .play
        .items
        .iter()
        .find(|i| i.template == BONE)
        .map_or(0, |i| i.quantity);
    assert!(got == 35 || got == 70, "ten above 25, or twice that: {got}");
    for left in wear_updates(&h, b, from, knife) {
        assert_eq!(left, 199, "from its own 200");
    }
}

/// A knife that asks more of the player's craft skill than it has is refused, and cuts nothing.
/// Behaviour: server.craft.tool-asks-its-craft-skill
#[test]
fn a_knife_asking_more_craft_skill_than_the_player_has_is_refused() {
    let (mut h, b, me, world, knife, corpse) = at_a_corpse();
    h.server.things.get_mut(knife).expect("the knife").own.made = Some(yalain_world::made::Made {
        tool_yield: Some(25),
        durability: Some(100),
        craft_needed: Some(60),
        ..yalain_world::made::Made::default()
    });
    let from = butcher(&mut h, b, me, knife, corpse, 5.0);
    assert!(
        use_answers(&h, b, from)
            .iter()
            .any(|(status, _)| *status == refusal::CRAFT_TOO_LOW),
        "{:?}",
        use_answers(&h, b, from)
    );
    let p = &h.server.players[&world];
    assert!(!p.play.items.iter().any(|i| i.template == BONE));
}

/// A cut with no place in the pack for its material is refused at the result, as the work ends: no
/// material, the corpse still to butcher, nothing paid. With a place, the same corpse is cut.
/// Behaviour: server.craft.butchery-wants-a-place-in-the-pack
#[test]
fn a_cut_with_no_place_in_the_pack_is_refused_and_leaves_the_corpse_to_butcher() {
    let (mut h, b, me, world, knife, corpse) = at_a_corpse();
    let items = &mut h.server.players.get_mut(&world).expect("there").play.items;
    for n in u32::try_from(items.len()).expect("few")..yalain_world::inventory::PACK_ROOM {
        items.push(yalain_world::state::Held {
            id: 0x8130_0000_0FFF_0000 + u64::from(n),
            template: BONE - 1,
            slot: n,
            location: 0,
            quantity: 1,
        });
    }
    let from = butcher(&mut h, b, me, knife, corpse, 2.5);
    assert_eq!(
        use_answers(&h, b, from).first().map(|a| a.0),
        Some(refusal::NO_ROOM)
    );
    let held = |h: &Harness, t: u32| {
        h.server.players[&world]
            .play
            .items
            .iter()
            .filter(|i| i.template == t)
            .count()
    };
    assert_eq!(held(&h, BONE), 0, "no material");
    assert!(
        wear_updates(&h, b, from, knife).is_empty(),
        "the knife unworn"
    );
    let open = h.server.things.get(corpse).and_then(|t| t.corpse.as_ref());
    assert!(open.is_some_and(|c| !c.butchered), "still to butcher");
    h.server
        .players
        .get_mut(&world)
        .expect("there")
        .play
        .items
        .retain(|i| i.template != BONE - 1);
    let from = butcher(&mut h, b, me, knife, corpse, 2.5);
    assert_eq!(use_answers(&h, b, from).first().map(|a| a.0), Some(0));
    assert_eq!(held(&h, BONE), 1);
}

/// The knife stays in the pack while it cuts: asking to put it down is refused and the cut goes on.
/// One gone from the pack by the end all the same cuts nothing, pays nothing and leaves the corpse.
/// Behaviour: server.craft.a-knife-stays-while-it-cuts
#[test]
fn a_knife_is_not_put_down_while_it_cuts_and_one_gone_pays_nothing() {
    use yalain_world::inventory::{move_graph, Move};
    let bone = |h: &Harness, world| {
        h.server.players[&world]
            .play
            .items
            .iter()
            .filter(|i| i.template == BONE)
            .count()
    };
    let (mut h, b, me, world, knife, corpse) = at_a_corpse();
    butcher(&mut h, b, me, knife, corpse, 0.5);
    let down = Move {
        move_type: yalain_world::drops::DROP,
        merge: true,
        unlock: true,
        check_take: true,
        all_taken: true,
        grab: true,
        contain: true,
        auto_merge: true,
        from: me,
        actual_from: me,
        item: knife,
        split_item: knife,
        quantity: 1,
        quantity_left: 1,
        ..Move::default()
    };
    let call = server_call(
        yalain_world::drops::function::MOVE_ITEM,
        &[move_graph(&down)],
    );
    h.bots[b].send(true, queue::WEENIE, &call);
    h.run(4);
    let t = h.now.0 + butchery::WORK_SECONDS + 0.5;
    while h.now.0 < t {
        h.step();
    }
    let held = |h: &Harness| {
        h.server.players[&world]
            .play
            .items
            .iter()
            .any(|i| i.id == knife)
    };
    assert!(held(&h), "still in the pack");
    assert_eq!(bone(&h, world), 1, "and it cut");
    // The lock on the pack is over with the cut.
    assert!(!yalain_world::places::pack_spoken_for(&h.server, world));

    let (mut h, b, me, world, knife, corpse) = at_a_corpse();
    butcher(&mut h, b, me, knife, corpse, 0.5);
    h.server
        .players
        .get_mut(&world)
        .expect("in")
        .play
        .items
        .retain(|i| i.id != knife);
    let from = h.bots[b].inbox.len();
    let t = h.now.0 + butchery::WORK_SECONDS + 0.5;
    while h.now.0 < t {
        h.step();
    }
    assert_eq!(bone(&h, world), 0, "paid nothing");
    assert_eq!(
        use_answers(&h, b, from).last().map(|a| a.0),
        Some(refusal::EMPTY),
        "answered"
    );
    assert!(
        !h.server
            .things
            .get(corpse)
            .and_then(|t| t.corpse.as_ref())
            .expect("the corpse")
            .butchered,
        "the corpse is left"
    );
}

/// A corpse still another's is refused the knife as it is refused opening, and its material stays.
/// Behaviour: server.loot.a-knife-asks-too
#[test]
fn a_knife_is_refused_a_corpse_still_another_players() {
    let (mut h, b, me, world, knife, _) = at_a_corpse();
    let (_, other) = h.player_in_world("two", "Rowan");
    let corpse = corpse_by(&mut h, world, other, 0);
    let now = h.now.0;
    if let Some(c) = h
        .server
        .things
        .get_mut(corpse)
        .and_then(|t| t.corpse.as_mut())
    {
        c.unlocks = now + 60.0;
    }
    let from = butcher(&mut h, b, me, knife, corpse, 0.0);
    assert_eq!(
        use_answers(&h, b, from).first().map(|a| a.0),
        Some(yalain_world::butchery::refusal::NO_RIGHT)
    );
    let cut = h
        .server
        .things
        .get(corpse)
        .and_then(|t| t.corpse.as_ref())
        .is_some_and(|c| c.butchered);
    assert!(!cut, "still to butcher");
    if let Some(c) = h
        .server
        .things
        .get_mut(corpse)
        .and_then(|t| t.corpse.as_mut())
    {
        c.unlocks = now - 1.0;
    }
    let from = butcher(&mut h, b, me, knife, corpse, 3.0);
    assert!(
        use_answers(&h, b, from).iter().any(|a| a.0 == 0),
        "once it is free"
    );
}
