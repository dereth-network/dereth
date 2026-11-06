//! The world database's veins taken again while the server runs: a new vein stands at once and
//! one taken away leaves the world; one the same keeps its uses, and a spent one its coming back,
//! while one taken away spent no longer comes back; a player mining a vein taken away has the use
//! answered as the vein used up. Fixture: the test hall with iron veins of a made-up template
//! placed beside the start, anyone's, of three uses, and a player in the world, its scene drawn.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_content::{ContentObject, ContentPack, Description, Facts, Kind, Prop, Props};
use yalain_testkit::play::server_call;
use yalain_testkit::Harness;
use yalain_world::calls::{self, Node, Value};
use yalain_world::craft::{CraftSkillDef, LevelMap};
use yalain_world::mining::{self, refusal};
use yalain_world::reload::table;
use yalain_world::Timer;

use super::reload::{reload, tables_of};

const VEIN: u32 = 0x4700_12C0;
const ORE: u32 = 0x4700_06C1;
const IRON: u32 = 0x6F00_069C;
const CELL: u32 = 0x0299_0109;
const USE: u32 = 0x8216_0016;
const USE_DONE: u32 = 0x8216_0021;

fn vein_at(x: f32) -> ContentObject {
    ContentObject {
        object: None,
        template: VEIN,
        description: Description {
            name_text: None,
            package: 0x1BFC,
            look: 0x1F00_0001,
            enter_fx: 0,
            scale: None,
            name: Some((0x1C06_0003, 0x2500_0003)),
            numbers: [0; 6],
            source: "authored for a test".into(),
        },
        cell: CELL,
        origin: [x, 241.0, -44.6],
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

/// The hall with veins at these points (as the server started with them), and a player in the
/// world beside them, its scene drawn.
fn hall(xs: &[f32]) -> (Harness, usize, u64) {
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    h.server.crafting.skills.push(CraftSkillDef {
        bonus_stat: 0,
        common_name: (0, 0),
        did: IRON,
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
    });
    let mut v = Props::default();
    v.set("Mine_Object", Prop::Word(ORE))
        .set("Mine_ObjectQuantity", Prop::Int(60))
        .set("Mine_MaxUses", Prop::Int(3))
        .set("CraftSkill", Prop::Word(IRON));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(VEIN, 0x1BFC, v));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(ORE, 0x3A1, Props::default()));
    let veins: Vec<ContentObject> = xs.iter().map(|x| vein_at(*x)).collect();
    h.server.content = ContentPack {
        objects: veins.clone(),
        ..ContentPack::default()
    };
    // As the server's start does.
    mining::place_veins(&mut h.server);
    h.server.placed.veins = veins;
    h.server.placed.loaded = Some(Vec::new());
    let (b, me) = h.player_in_world("one", "Elara");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
    (h, b, me)
}

/// The veins standing, by where (x), with their ids and uses.
fn standing(h: &Harness) -> Vec<(f32, u64, u32)> {
    let mut v: Vec<_> = h
        .server
        .things
        .by_id
        .values()
        .filter(|t| t.template == VEIN)
        .filter_map(|t| Some((t.at?.origin[0], t.id, t.node.as_ref()?.uses)))
        .collect();
    v.sort_by(|a, b| a.0.total_cmp(&b.0));
    v
}

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

/// The statuses of the use answers a client was sent since `from`.
fn answers(h: &Harness, b: usize, from: usize) -> Vec<u32> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c) if c.function.0 == USE_DONE => {
                let v = c.args.as_ref().and_then(|a| calls::values(&a.0).ok())?;
                let Some(Value::Graph(g)) = v.first() else {
                    return None;
                };
                g.iter().find_map(|(_, n)| match n {
                    Node::Script { slots, .. } => Some(slots[11].1),
                    _ => None,
                })
            }
            _ => None,
        })
        .collect()
}

/// Mine a vein and let the work finish.
fn mine(h: &mut Harness, b: usize, me: u64, vein: u64) {
    h.bots[b].send(true, queue::WEENIE, &server_call(USE, &[use_of(vein, me)]));
    let t = h.now.0 + mining::WORK_SECONDS + 1.0;
    while h.now.0 < t {
        h.step();
    }
}

fn ore(h: &Harness, me: u64) -> u32 {
    h.server
        .players
        .values()
        .find(|p| p.character == me)
        .and_then(|p| p.play.items.iter().find(|i| i.template == ORE))
        .map_or(0, |i| i.quantity)
}

/// A reload whose database moves one of two veins: the moved one leaves the world, forgotten by
/// the player, a new vein stands where it now is, made for the player, and the other is left as
/// it stood.
/// Behaviour: server.operator.reload-places-veins
#[test]
fn a_vein_a_reload_adds_stands_at_once_and_one_taken_away_leaves_the_world() {
    let (mut h, b, _) = hall(&[59.0, 61.0]);
    let before = standing(&h);
    let from = h.bots[b].inbox.len();
    let mut t = tables_of(&h);
    t.content.objects = vec![vein_at(63.0), vein_at(61.0)];
    let r = reload(&mut h, t);
    h.run(5);

    assert_eq!(r.applied, [table::VEINS]);
    assert_eq!(r.details, ["veins: 1 new, 1 taken away"]);
    let now = standing(&h);
    assert_eq!(now.len(), 2);
    assert_eq!(now[0], before[1], "the one left alone");
    assert_eq!(now[1].0, 63.0);
    let msgs = h.world_messages(b, from);
    assert!(msgs
        .iter()
        .any(|m| matches!(m, ServerMessage::ForgetObject(o) if o.0.id == before[0].1)));
    assert!(msgs
        .iter()
        .any(|m| matches!(m, ServerMessage::CreateObject(o) if o.id == now[1].1)));
    assert!(
        h.server.content.objects.is_empty(),
        "no placed object of it"
    );
}

/// A reload that adds a vein leaves the others' state: one mined once keeps its use, one spent
/// still comes back where it stood; a reload that then takes the spent one away means it never
/// does.
/// Behaviour: server.operator.reload-keeps-unchanged-veins
#[test]
fn an_unchanged_vein_keeps_its_uses_and_a_spent_one_taken_away_never_comes_back() {
    let (mut h, b, me) = hall(&[59.0, 60.0]);
    let [(_, mined, _), (_, spent, _)] = standing(&h)[..] else {
        panic!("two veins");
    };
    mine(&mut h, b, me, mined);
    for _ in 0..3 {
        mine(&mut h, b, me, spent);
    }
    let t = h.now.0 + mining::SPENT_SECONDS + 1.0;
    while h.now.0 < t {
        h.step();
    }
    assert!(h.server.things.get(spent).is_none(), "spent and gone");
    let coming_back = |h: &Harness| {
        h.server
            .timers
            .iter()
            .any(|(_, t)| matches!(t, Timer::VeinBack { at, .. } if at.origin[0] == 60.0))
    };
    assert!(coming_back(&h));

    let mut t = tables_of(&h);
    t.content.objects = vec![vein_at(59.0), vein_at(60.0), vein_at(62.0)];
    reload(&mut h, t);
    let now = standing(&h);
    assert_eq!(now[0], (59.0, mined, 1), "mined once, the same vein");
    assert!(coming_back(&h), "the spent one still comes back");

    let mut t = tables_of(&h);
    t.content.objects = vec![vein_at(59.0), vein_at(62.0)];
    reload(&mut h, t);
    assert!(!coming_back(&h), "taken away, it does not");
    let t = h.now.0 + mining::VEIN_BACK_SECONDS + 5.0;
    while h.now.0 < t {
        h.step_by(1.0);
    }
    assert!(standing(&h).iter().all(|v| v.0 != 60.0), "never back");
}

/// A reload that takes away a vein a player is mining: the use is answered at once, the vein used
/// up, the vein leaves the world, and no ore comes of the work.
/// Behaviour: server.operator.reload-lets-go-of-veins-in-use
#[test]
fn a_vein_taken_away_while_mined_answers_its_miner_used_up() {
    let (mut h, b, me) = hall(&[59.0]);
    let vein = standing(&h)[0].1;
    h.bots[b].send(true, queue::WEENIE, &server_call(USE, &[use_of(vein, me)]));
    h.run(10);
    assert!(
        h.server
            .things
            .get(vein)
            .and_then(|t| t.node.as_ref())
            .is_some_and(|n| n.busy),
        "being mined"
    );
    let from = h.bots[b].inbox.len();
    let mut t = tables_of(&h);
    t.content.objects.clear();
    reload(&mut h, t);
    let t = h.now.0 + mining::WORK_SECONDS + 1.0;
    while h.now.0 < t {
        h.step();
    }

    assert_eq!(answers(&h, b, from), [refusal::USED_UP]);
    assert!(h.server.things.get(vein).is_none());
    assert_eq!(ore(&h, me), 0, "no ore");
}
