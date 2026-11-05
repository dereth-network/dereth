//! Surveying and mining: a delving stone used from the pack conjures its resource node in front of
//! the player and is spent; the summoner mines the node for its material and the mining skill's
//! experience, three times; another player may not; a node's units follow its summoner's surveying
//! level; a vein placed in the world is anyone's, and comes back where it stood after it is spent;
//! a hoard's vein is mined only while the hoard's quest runs, and when its time runs out the miner
//! is sent out.
//! Fixture: the test world with the surveying and iron mining skills, a rusty delving stone and a
//! pickaxe in the player's pack, the summoned iron vein, a placed hoard vein and their ore as
//! made-up templates.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_content::{Prop, Props};
use yalain_testkit::play::server_call;
use yalain_testkit::Harness;
use yalain_world::calls::{self, Node, Value};
use yalain_world::craft::{CraftSkillDef, LevelMap};
use yalain_world::mining::{self, refusal};

const STONE: u32 = 0x4700_0BDC;
const VEIN: u32 = 0x4700_0C8C;
const ORE: u32 = 0x4700_06C1;
const PICKAXE: u32 = 0x4700_0CC1;
const IRON: u32 = 0x6F00_069C;
const USE: u32 = 0x8216_0016;
const USE_DONE: u32 = 0x8216_0021;

/// A use of an item or an object, as the client asks for it.
fn use_of(item: u64, me: u64) -> Arg {
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
            Slot::Long(0),
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

fn gathering(did: u32) -> CraftSkillDef {
    CraftSkillDef {
        bonus_stat: 0,
        common_name: (0, 0),
        did,
        cap: 150,
        name: (0xA860_7CEC, 0x2500_13CA),
        advancement: vec![0, 0, 10, 20, 30, 40, 50],
        daily: LevelMap {
            first: 1,
            values: vec![120; 10],
        },
        usage_xp: Some(LevelMap {
            first: 1,
            values: vec![2; 10],
        }),
        pool_share: 0.01,
        cannot_raise: false,
    }
}

/// The test world with surveying and iron mining, the stone, the vein, the ore and the pickaxe.
fn world() -> Harness {
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    h.server.crafting.skills.push(gathering(mining::SURVEYING));
    h.server.crafting.skills.push(gathering(IRON));
    let mut stone = Props::default();
    stone
        .set(
            "Usage_Effect1",
            Prop::Text("Eff_SummonMine_Iron_Delving".into()),
        )
        .set("Item_Durability_MaxLevel", Prop::Int(2));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(STONE, 0x3A1, stone));
    let mut vein = Props::default();
    vein.set("Mine_Object", Prop::Word(ORE))
        .set("Mine_ObjectQuantity", Prop::Int(50))
        .set("Mine_ObjectQuantityVariance", Prop::Float(0.1))
        .set("Mine_MaxUses", Prop::Int(3))
        .set("CraftSkill", Prop::Word(IRON))
        .set("Usage_SummonerOnly", Prop::Bool(true));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(VEIN, 0x1BFC, vein));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(ORE, 0x3A1, Props::default()));
    let mut pick = Props::default();
    pick.set("Usage_UserAnimation", Prop::Word(0x4100_014D));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(PICKAXE, 0x3A1, pick));
    h
}

/// A player of the test world in it, its scene drawn; the client, the player, its connection.
fn in_world(h: &mut Harness, name: &str) -> (usize, u64, yalain_net::ConnId) {
    let (b, me) = h.player_in_world(name, name);
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
    (b, me, world)
}

/// Give a player an item of a template; its id.
fn give(h: &mut Harness, world: yalain_net::ConnId, template: u32) -> u64 {
    yalain_world::inventory::give(&mut h.server, &mut h.net, world, template, 1);
    h.run(3);
    h.server.players[&world]
        .play
        .items
        .iter()
        .find(|i| i.template == template)
        .expect("given")
        .id
}

/// Send a use and run the world `seconds` on; the inbox mark before.
fn send_use(h: &mut Harness, b: usize, me: u64, item: u64, seconds: f64) -> usize {
    let from = h.bots[b].inbox.len();
    h.bots[b].send(true, queue::WEENIE, &server_call(USE, &[use_of(item, me)]));
    h.run(4);
    let t = h.now.0 + seconds;
    while h.now.0 < t {
        h.step();
    }
    from
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

/// The node a client was told of since `from`: its id and where it stands.
fn node_made(h: &Harness, b: usize, from: usize) -> Option<(u64, [f32; 3])> {
    h.world_messages(b, from).iter().find_map(|m| match m {
        ServerMessage::CreateObject(o) if o.weenie.entity_did == VEIN => {
            let p = o.physics.position.frame.origin;
            Some((o.id, [p.x, p.y, p.z]))
        }
        _ => None,
    })
}

/// A delving stone used from the pack: the surveying skill pays, the stone's durability falls by
/// one (at nothing it is destroyed through overuse, its node still made), the use is answered, and
/// the node its effect names appears two meters straight ahead of the player.
/// Behaviour: server.craft.delving-stone-summons-a-node
#[test]
fn a_delving_stone_conjures_its_node_in_front_of_the_player_and_is_spent() {
    let mut h = world();
    let (b, me, world) = in_world(&mut h, "Elara");
    let stone = give(&mut h, world, STONE);
    let at = h.server.players[&world].position;
    let from = send_use(&mut h, b, me, stone, 0.5);
    assert_eq!(use_answers(&h, b, from).first().map(|a| a.0), Some(0));
    let (_, p) = node_made(&h, b, from).expect("the node appears");
    let d = ((p[0] - at.origin[0]).powi(2) + (p[1] - at.origin[1]).powi(2)).sqrt();
    assert!((d - mining::SUMMON_DISTANCE).abs() < 0.01, "{d} m away");
    assert_eq!(
        h.server.things.get(stone).and_then(|t| t.durability),
        Some(1)
    );
    let again = send_use(&mut h, b, me, stone, 0.5);
    assert!(node_made(&h, b, again).is_some(), "a second node");
    assert!(
        !h.server.players[&world]
            .play
            .items
            .iter()
            .any(|i| i.template == STONE),
        "the stone is worn out"
    );
    let record = h.server.players[&world]
        .play
        .craft_skills
        .iter()
        .find(|c| c.skill == mining::SURVEYING)
        .map(|c| c.today);
    assert_eq!(record, Some(4), "surveying pays each use 2");
}

/// The summoner mines its node: the iron skill's behavior played and let go, about two seconds
/// later the answer with the units, the ore on the player's stack and the iron skill's pay; after
/// three uses the node flashes, a fourth is refused, and the node goes.
/// Behaviour: server.craft.mining-gathers-a-material
#[test]
fn the_summoner_mines_its_node_three_times_for_ore_and_experience() {
    let mut h = world();
    let (b, me, world) = in_world(&mut h, "Elara");
    give(&mut h, world, PICKAXE);
    let stone = give(&mut h, world, STONE);
    let from = send_use(&mut h, b, me, stone, 0.5);
    let (node, _) = node_made(&h, b, from).expect("the node");
    let mut total = 0;
    for n in 1..=3 {
        let from = send_use(&mut h, b, me, node, 0.5);
        assert!(use_answers(&h, b, from).is_empty(), "not done yet");
        let msgs = h.world_messages(b, from);
        assert!(msgs.iter().any(|m| matches!(m,
            ServerMessage::PlayBehavior(e) if e.sender.id == me
                && e.action.behavior == 0x4100_014D)));
        let t = h.now.0 + mining::WORK_SECONDS + 0.5;
        while h.now.0 < t {
            h.step();
        }
        let got = use_answers(&h, b, from);
        assert_eq!(got.len(), 1, "use {n} answered once");
        assert_eq!(got[0].0, 0, "use {n} made");
        let ore = h.server.players[&world]
            .play
            .items
            .iter()
            .find(|i| i.template == ORE)
            .map_or(0, |i| i.quantity);
        let units = ore - total;
        // A level 1 surveyor's vein of 50 gives 50 units, spread by a tenth; twice for a rich find.
        assert!(
            (45..=55).contains(&units) || (90..=110).contains(&units),
            "{units} units"
        );
        total = ore;
    }
    let record = h.server.players[&world]
        .play
        .craft_skills
        .iter()
        .find(|c| c.skill == IRON)
        .map(|c| c.today);
    assert_eq!(record, Some(6), "each use pays 2");
    let from = send_use(&mut h, b, me, node, 0.0);
    assert_eq!(
        use_answers(&h, b, from).first().map(|a| a.0),
        Some(refusal::USED_UP)
    );
    assert!(h.world_messages(b, 0).iter().any(|m| matches!(m,
        ServerMessage::DoFx(e) if e.sender.id == node && e.action.fx == mining::SPENT_FX)));
    let t = h.now.0 + mining::SPENT_SECONDS + 0.5;
    while h.now.0 < t {
        h.step();
    }
    assert!(h.server.things.get(node).is_none(), "the node is gone");
}

/// Another player may not mine a node it did not summon: refused, nothing gathered.
/// Behaviour: server.craft.mining-summoner-only
#[test]
fn only_the_summoner_may_mine_a_node() {
    let mut h = world();
    let (b, me, world) = in_world(&mut h, "Elara");
    let stone = give(&mut h, world, STONE);
    let from = send_use(&mut h, b, me, stone, 0.5);
    let (node, _) = node_made(&h, b, from).expect("the node");
    let (c, other, other_world) = in_world(&mut h, "Torvald");
    let at = h.server.players[&world].position;
    h.server
        .players
        .get_mut(&other_world)
        .expect("there")
        .position = at;
    let from = send_use(&mut h, c, other, node, mining::WORK_SECONDS + 0.5);
    assert_eq!(
        use_answers(&h, c, from).first().map(|a| a.0),
        Some(refusal::SUMMONER_ONLY)
    );
    assert!(!h.server.players[&other_world]
        .play
        .items
        .iter()
        .any(|i| i.template == ORE));
}

/// A node's units follow its summoner's surveying level: about the level and a tenth of the
/// node's quantity, never fewer than the quantity.
/// Behaviour: server.craft.mining-units-by-surveying
#[test]
fn a_nodes_units_follow_its_summoners_surveying_level() {
    assert!((mining::units_for(50, 1) - 50.0).abs() < 1e-3);
    assert!((mining::units_for(50, 63) - 68.0).abs() < 1e-3);
    assert!((mining::units_for(50, 150) - 155.0).abs() < 1e-3);
    assert!((mining::units_for(25, 1) - 25.0).abs() < 1e-3);
    assert!((mining::units_for(30, 84) - 87.0).abs() < 1e-3);
}

/// A mine a quest works, placed beside the start: a made-up barium mine of three units, and a ruby
/// mine its recipe works.
fn with_quest_mines(h: &mut Harness, quest: u32) -> (u64, u64) {
    use yalain_content::{ContentObject, ContentPack, Description, Facts, Kind};
    const BARIUM_MINE: u32 = 0x4700_18F2;
    const RUBY_MINE: u32 = 0x4700_0B3D;
    let mine = |id: u64, template: u32, x: f32| ContentObject {
        object: Some(id),
        template,
        description: Description {
            package: 0x3A0,
            look: 0x1F00_0001,
            enter_fx: 0,
            scale: None,
            name: Some((0x1C06_0003, 0x2500_0003)),
            numbers: [0; 6],
            source: "authored for a test".into(),
        },
        cell: 0x0299_0109,
        origin: [x, 241.0, -44.6],
        rotation: [1.0, 0.0, 0.0, 0.0],
        kind: Kind::Static,
        label: String::new(),
        placed: "authored for a test".into(),
        destination: None,
        facts: Facts::default(),
        walks: None,
        gate: None,
    };
    let (barium, ruby) = (0x4002_9900_0000_0091, 0x4002_9900_0000_0092);
    h.server.content = ContentPack {
        objects: vec![mine(barium, BARIUM_MINE, 61.0), mine(ruby, RUBY_MINE, 57.0)],
        ..ContentPack::default()
    };
    let mut b = Props::default();
    b.set("Mine_Object", Prop::Word(ORE))
        .set("Mine_ObjectQuantity", Prop::Int(3))
        .set("Mine_MaxUses", Prop::Int(0))
        .set("Usage_RequiredQuest", Prop::Word(quest))
        .set("Usage_UserAnimation", Prop::Word(0x4100_014D))
        .set("Usage_UserAnimationRepeatCount", Prop::Int(3));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(BARIUM_MINE, 0x3A0, b.clone()));
    b.set("Mine_Recipe", Prop::Text("Rcp_RubyUlkas.wc".into()));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(RUBY_MINE, 0x3A0, b));
    (barium, ruby)
}

/// A mine a quest works refuses a player without its quest under way; with it, the work plays
/// some seconds, then the quest moves on and the use is answered with the units extracted, the
/// material in the pack, no experience; a mine its recipe works refuses every use.
/// Behaviour: server.craft.quest-mines
#[test]
fn a_quest_mine_gives_its_material_and_moves_its_quest_on_and_a_recipe_mine_refuses() {
    use yalain_world::quests::{self, status};
    let quest = 0x8000_02E5;
    let mut h = world();
    h.server
        .quests
        .quests
        .push(yalain_content::quests::QuestDef {
            id: quest,
            name: (1, 0x2500_14B4),
            description: (2, 0x2500_14B4),
            icon: 0,
            play_fx: false,
            journals: vec![(3, 0x2500_14B4); 5],
            challenge: 0,
            duration: 3600.0,
            timed_phase: 0,
            retry: 86_400.0,
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
        });
    let (barium, ruby) = with_quest_mines(&mut h, quest);
    let (b, me, world) = in_world(&mut h, "Elara");
    let answers = |h: &Harness, from: usize| -> Vec<(u32, u32, u32)> {
        h.calls(b, from)
            .iter()
            .filter(|(f, _)| *f == USE_DONE)
            .filter_map(|(_, v)| match v.first() {
                Some(Value::Graph(g)) => {
                    let (st, fl) = g.iter().find_map(|(_, n)| match n {
                        Node::Script { slots, .. } => Some((slots[11].1, slots[17].1)),
                        _ => None,
                    })?;
                    let t = g
                        .iter()
                        .find_map(|(_, n)| match n {
                            Node::TextInfo(t) => Some(t.string_id),
                            _ => None,
                        })
                        .unwrap_or(0);
                    Some((st, fl, t))
                }
                _ => None,
            })
            .collect()
    };
    let send = |h: &mut Harness, item: u64, secs: f64| -> usize {
        let from = h.bots[b].inbox.len();
        h.bots[b].send(true, queue::WEENIE, &server_call(USE, &[use_of(item, me)]));
        let t = h.now.0 + secs;
        while h.now.0 < t {
            h.step();
        }
        from
    };
    let from = send(&mut h, barium, 1.0);
    assert_eq!(
        answers(&h, from),
        [(
            mining::quest_refusal::WRONG_QUEST,
            mining::quest_refusal::FLAGS,
            0
        )],
        "no quest under way"
    );
    h.server
        .players
        .get_mut(&world)
        .expect("in")
        .play
        .quests
        .push(yalain_world::state::QuestState {
            quest,
            status: status::UNDERWAY,
            phase: 1,
            started: 0,
            updated: 0,
        });
    let from = send(&mut h, ruby, 1.0);
    assert_eq!(
        answers(&h, from),
        [(
            mining::quest_refusal::BY_RECIPE,
            mining::quest_refusal::FLAGS,
            0
        )],
        "a recipe works it"
    );
    let from = send(&mut h, barium, 7.0);
    assert!(answers(&h, from).is_empty(), "the work still under way");
    let t = h.now.0 + 1.5;
    while h.now.0 < t {
        h.step();
    }
    assert_eq!(answers(&h, from), [(0, 316, mining::EXTRACTED.0)]);
    assert_eq!(
        quests::phase_of(&h.server.players[&world].play.quests, quest),
        2,
        "the quest moved on"
    );
    let ore: u32 = h.server.players[&world]
        .play
        .items
        .iter()
        .filter(|i| i.template == ORE)
        .map(|i| i.quantity.max(1))
        .sum();
    assert_eq!(ore, 3);
}

/// A hoard's vein placed beside the start, of quantity 60 and three uses, anyone's; its id.
fn with_hoard_vein(h: &mut Harness) -> u64 {
    use yalain_content::{ContentObject, ContentPack, Description, Facts, Kind};
    let mut v = Props::default();
    v.set("Mine_Object", Prop::Word(ORE))
        .set("Mine_ObjectQuantity", Prop::Int(60))
        .set("Mine_ObjectQuantityVariance", Prop::Float(0.1))
        .set("Mine_MaxUses", Prop::Int(3))
        .set("CraftSkill", Prop::Word(IRON));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(HOARD_VEIN, 0x1BFC, v));
    h.server.content = ContentPack {
        objects: vec![ContentObject {
            object: None,
            template: HOARD_VEIN,
            description: Description {
                package: 0x1BFC,
                look: 0x1F00_0001,
                enter_fx: 0,
                scale: None,
                name: Some((0x1C06_0003, 0x2500_0003)),
                numbers: [0; 6],
                source: "authored for a test".into(),
            },
            cell: 0x0299_0109,
            origin: [59.0, 241.0, -44.6],
            rotation: [1.0, 0.0, 0.0, 0.0],
            kind: Kind::Static,
            label: String::new(),
            placed: "authored for a test".into(),
            destination: None,
            facts: Facts::default(),
            walks: None,
            gate: None,
        }],
        ..ContentPack::default()
    };
    mining::place_veins(&mut h.server);
    assert!(
        h.server.content.objects.is_empty(),
        "the vein is no longer a placed object"
    );
    h.server
        .things
        .by_id
        .values()
        .find(|t| t.template == HOARD_VEIN)
        .map(|t| t.id)
        .expect("the vein stands")
}

const HOARD_VEIN: u32 = 0x4700_12C0;

/// A vein placed in the world is anyone's: two players mine it, each use's units drawn for its
/// miner (the vein's quantity and half the miner's level in iron: 60 for one new to iron, 110 at
/// level 100); at its third use it flashes
/// and goes, and two minutes after it went a new vein stands in its place.
/// Behaviour: server.craft.veins-are-anyones-and-come-back
#[test]
fn a_placed_vein_is_mined_by_anyone_and_comes_back_where_it_stood() {
    assert!((mining::vein_units(60, 150) - 135.0).abs() < 1e-3);
    assert!((mining::vein_units(60, 0) - 60.0).abs() < 1e-3);
    let mut h = world();
    let vein = with_hoard_vein(&mut h);
    let (b, me, _) = in_world(&mut h, "Elara");
    let (c, other, other_world) = in_world(&mut h, "Torvald");
    // Torvald is an iron miner of level 100.
    let skills = &mut h
        .server
        .players
        .get_mut(&other_world)
        .expect("there")
        .play
        .craft_skills;
    match skills.iter_mut().find(|r| r.skill == IRON) {
        Some(r) => r.level = 100,
        None => skills.push(yalain_world::state::CraftSkillState {
            skill: IRON,
            level: 100,
            today: 0,
            available: 0,
        }),
    }
    let ore = |h: &Harness, who: u64| {
        h.server
            .players
            .values()
            .find(|p| p.character == who)
            .and_then(|p| p.play.items.iter().find(|i| i.template == ORE))
            .map_or(0, |i| i.quantity)
    };
    for (n, (bot, who)) in [(b, me), (c, other), (b, me)].into_iter().enumerate() {
        let before = ore(&h, who);
        let from = send_use(&mut h, bot, who, vein, mining::WORK_SECONDS + 0.5);
        assert_eq!(
            use_answers(&h, bot, from).first().map(|a| a.0),
            Some(0),
            "use {} made",
            n + 1
        );
        let units = ore(&h, who) - before;
        // Elara, new to iron: about 60 units; Torvald: 110; spread by a tenth, twice when rich.
        let around = if who == other { 110 } else { 60 };
        let within = |u: u32| u * 10 >= around * 9 - 10 && u * 10 <= around * 11 + 10;
        assert!(
            within(units) || within(units / 2),
            "{units} units for {}",
            if who == other { "Torvald" } else { "Elara" }
        );
    }
    let t = h.now.0 + mining::SPENT_SECONDS + 0.5;
    while h.now.0 < t {
        h.step();
    }
    assert!(
        h.server.things.get(vein).is_none(),
        "the spent vein is gone"
    );
    let from = h.bots[b].inbox.len();
    // 2017: back 122.3 and 123.0 s after it went.
    let went = h.now.0;
    let t = went + 118.0;
    while h.now.0 < t {
        h.step_by(1.0);
    }
    assert!(
        !h.server
            .things
            .by_id
            .values()
            .any(|t| t.template == HOARD_VEIN),
        "not back yet"
    );
    let t = went + 128.0;
    while h.now.0 < t {
        h.step_by(1.0);
    }
    let back = h
        .server
        .things
        .by_id
        .values()
        .find(|t| t.template == HOARD_VEIN)
        .expect("a new vein");
    assert_ne!(back.id, vein, "a new one");
    assert_eq!(
        back.at.map(|a| (a.cell, a.origin)),
        Some((0x0299_0109, [59.0, 241.0, -44.6])),
        "where the old one stood"
    );
    let id = back.id;
    assert!(
        h.world_messages(b, from)
            .iter()
            .any(|m| matches!(m, ServerMessage::CreateObject(o) if o.id == id)),
        "seen by the player near"
    );
}

/// The hoard's time runs out: the hoard's hidden quest given puts the Shaper's Hoard effect on
/// (and tells nothing of the quest); the vein, refused as mysterious without the quest, is mined
/// while it runs; 300 s on the effect is gone and the quest failed, untold, and the vein refuses
/// as mysterious again; ten seconds after that the player goes through portal space to the place
/// the quest's failure sends it, with no word of it.
/// Behaviour: server.craft.hoard-time-runs-out
#[test]
fn the_hoards_time_runs_out_and_its_miner_is_sent_out() {
    use yalain_content::quests::{QuestDef, QuestTable, Update};
    use yalain_world::effect_defs::{EffectDef, Kind as EffectKind, Scale};
    use yalain_world::item_use::refusal::MYSTERIOUS_USE;
    use yalain_world::quests::status;
    const MINE: u32 = 0x8000_00EF;
    const HOARD_EFFECT: u32 = 0x6F00_0DB4;
    const SEND_OUT: u32 = 0x6F00_0DBA;
    const OUTSIDE: u32 = 0x7000_0001;
    let mut h = world();
    let vein = with_hoard_vein(&mut h);
    let mut v = h
        .server
        .catalog
        .get(HOARD_VEIN)
        .expect("the vein")
        .props
        .clone();
    v.set("Usage_RequiredQuest", Prop::Word(MINE))
        .set("Usage_RequiredQuestStatus", Prop::Word(status::UNDERWAY));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(HOARD_VEIN, 0x1BFC, v));
    h.server.quests = QuestTable {
        quests: vec![QuestDef {
            id: MINE,
            name: (0, 0),
            description: (0, 0),
            icon: 0,
            play_fx: true,
            journals: Vec::new(),
            challenge: -999,
            duration: 300.0,
            timed_phase: 0,
            retry: 0.0,
            xp: 0,
            cancellable: false,
            placed: "authored for a test".into(),
            hidden: true,
            effects: vec![(0x1000_0000, HOARD_EFFECT), (0x4000_0000, SEND_OUT)],
            subquests: Vec::new(),
            min_level: 0,
            fails_on_death: false,
            once: false,
            fellows: yalain_content::quests::Fellows::default(),
            last_waits: false,
            hero_only: false,
            pay: yalain_content::quests::CompletionPay::default(),
            required: None,
        }],
        steps: Vec::new(),
        gives: Vec::new(),
    };
    let def = |state, kind, seconds: f32| EffectDef {
        state,
        kind,
        duration: Scale(vec![(0.0, seconds)]),
        priority: Scale::default(),
        fx: 0,
        external_flags: 0,
        internal_flags: 0,
        enum_value: 0,
        eq_class: 0,
        name: None,
    };
    let outside = yalain_store::StoredPosition {
        cell: OUTSIDE,
        origin: [96.45, 120.76, 78.2],
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    h.server
        .effect_table
        .effects
        .insert(HOARD_EFFECT, def(HOARD_EFFECT, EffectKind::Other, 300.0));
    h.server
        .effect_table
        .effects
        .insert(SEND_OUT, def(SEND_OUT, EffectKind::Teleport(outside), 10.0));
    let (b, me, world) = in_world(&mut h, "Elara");
    let from = send_use(&mut h, b, me, vein, 0.5);
    assert_eq!(
        use_answers(&h, b, from).first().map(|a| a.0),
        Some(MYSTERIOUS_USE),
        "no quest, no mining"
    );
    let start = h.bots[b].inbox.len();
    let given = h.now.0;
    let clock = h.clock();
    yalain_world::quests::apply(
        &mut h.server,
        &mut h.net,
        world,
        MINE,
        Update::Bestow,
        0,
        clock,
    );
    let hoard_on = |h: &Harness| {
        h.server.players[&world]
            .play
            .effects
            .iter()
            .any(|e| e.state == HOARD_EFFECT)
    };
    let mine_status = |h: &Harness| {
        h.server.players[&world]
            .play
            .quests
            .iter()
            .find(|q| q.quest == MINE)
            .map(|q| q.status)
    };
    assert!(hoard_on(&h), "the Shaper's Hoard effect on");
    let from = send_use(&mut h, b, me, vein, mining::WORK_SECONDS + 0.5);
    assert_eq!(
        use_answers(&h, b, from).first().map(|a| a.0),
        Some(0),
        "mined"
    );
    h.run_to(yalain_net::LocalTime(given + 298.5));
    assert!(hoard_on(&h));
    assert_eq!(mine_status(&h), Some(status::UNDERWAY));
    h.run_to(yalain_net::LocalTime(given + 301.0));
    assert!(!hoard_on(&h), "the effect run out");
    assert_eq!(mine_status(&h), Some(status::FAILED), "the quest run out");
    let from = send_use(&mut h, b, me, vein, 0.5);
    assert_eq!(
        use_answers(&h, b, from).first().map(|a| a.0),
        Some(MYSTERIOUS_USE),
        "refused as mysterious again"
    );
    let portal_space = |h: &Harness, from: usize| {
        h.functions(b, from)
            .contains(&yalain_world::places::function::ENTER_PORTAL_SPACE)
    };
    h.run_to(yalain_net::LocalTime(given + 308.5));
    assert!(!portal_space(&h, start), "not sent out yet");
    let from = h.bots[b].inbox.len();
    h.run_to(yalain_net::LocalTime(given + 314.0));
    assert!(portal_space(&h, from), "sent out ten seconds on");
    assert_eq!(h.server.players[&world].position.cell, OUTSIDE);
    assert!(h.texts(b, from).is_empty(), "with no word of it");
    assert!(
        !h.functions(b, start)
            .contains(&yalain_world::quests::UPDATE_QUEST),
        "nothing of the quest told"
    );
}

/// A node refuses a player with no place in its pack for what it would give, and is not worn by it.
/// Behaviour: server.craft.mining-wants-a-place-in-the-pack
#[test]
fn a_node_refuses_a_player_with_no_place_in_its_pack() {
    let mut h = world();
    let (b, me, world) = in_world(&mut h, "Elara");
    give(&mut h, world, PICKAXE);
    let stone = give(&mut h, world, STONE);
    let from = send_use(&mut h, b, me, stone, 0.5);
    let (node, _) = node_made(&h, b, from).expect("the node");
    let items = &mut h.server.players.get_mut(&world).expect("there").play.items;
    for n in u32::try_from(items.len()).expect("few")..mining_room() {
        items.push(yalain_world::state::Held {
            id: 0x8130_0000_0FFF_0000 + u64::from(n),
            template: STONE,
            slot: n,
            location: 0,
            quantity: 1,
        });
    }
    let from = send_use(&mut h, b, me, node, mining::WORK_SECONDS + 0.5);
    assert_eq!(
        use_answers(&h, b, from).first().map(|a| a.0),
        Some(refusal::NO_ROOM)
    );
    assert!(!h.server.players[&world]
        .play
        .items
        .iter()
        .any(|i| i.template == ORE));
    h.server
        .players
        .get_mut(&world)
        .expect("there")
        .play
        .items
        .retain(|i| i.template != STONE);
    let from = send_use(&mut h, b, me, node, mining::WORK_SECONDS + 0.5);
    assert_eq!(use_answers(&h, b, from).first().map(|a| a.0), Some(0));
}

fn mining_room() -> u32 {
    yalain_world::inventory::PACK_ROOM
}
