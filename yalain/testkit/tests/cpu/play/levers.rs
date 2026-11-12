//! Levers: a lever a world's rows name, pulled, opens their doors for everyone, whatever their
//! locks, for the row's time (or until a use closes them); a row of two levers opens only once both
//! are pulled within its time. Fixture: the test hall with a locked door beside the start (facing
//! north, the start in front of it, so a plain use is refused), two levers a step away, and a
//! second player watching.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_content::levers::{Lever, LeverTable};
use yalain_content::{ContentObject, ContentPack, Description, Facts, Kind, Lock, Prop, Props};
use yalain_testkit::play::server_call;
use yalain_testkit::Harness;
use yalain_world::calls::{self, Node, Value};
use yalain_world::usable::doors::{self, mode, quality};
use yalain_world::usable::levers::PULLING;

const DOOR: u64 = 0x4002_9900_0000_0E71;
const LEVER: u64 = 0x4002_9900_0000_0E72;
const OTHER_LEVER: u64 = 0x4002_9900_0000_0E73;
const DOOR_TEMPLATE: u32 = 0x4700_0FCE;
const DOOR_PACKAGE: u32 = 0x0EB2;
const LEVER_TEMPLATE: u32 = 0x4700_0CDD;
const LEVER_PACKAGE: u32 = 0x27FF;
const LEVER_TURNS: u32 = 0x4100_0086;
const USE: u32 = 0x8216_0016;
const USE_DONE: u32 = 0x8216_0021;

fn placed(id: u64, template: u32, package: u32, origin: [f32; 2]) -> ContentObject {
    ContentObject {
        object: Some(id),
        template,
        description: Description {
            name_text: None,
            package,
            look: 0x1F00_0FCE,
            enter_fx: 0,
            scale: None,
            name: Some((0x1C06_0001, 0x2500_0001)),
            numbers: [0; 6],
            source: "authored for a test".into(),
        },
        cell: 0x0299_0109,
        origin: [origin[0], origin[1], -44.6],
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

/// The hall: a locked door no key opens, facing north just south of the start, and two levers a
/// step from the start, in the rows given (objects by their places: the door 0, the levers 1, 2).
fn hall(rows: Vec<Lever>) -> Harness {
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    h.server.content = ContentPack {
        objects: vec![
            placed(DOOR, DOOR_TEMPLATE, DOOR_PACKAGE, [60.0, 239.0]),
            placed(LEVER, LEVER_TEMPLATE, LEVER_PACKAGE, [58.0, 241.0]),
            placed(OTHER_LEVER, LEVER_TEMPLATE, LEVER_PACKAGE, [60.0, 241.5]),
        ],
        levers: LeverTable { rows },
        ..ContentPack::default()
    };
    h.server
        .doors
        .packages
        .insert(DOOR_PACKAGE, doors::Kind::Plain);
    h.server.locks.objects.insert(DOOR, Lock { key: None });
    let mut lever = Props::default();
    lever.set("Usage_Animation", Prop::Word(LEVER_TURNS));
    h.server.catalog.insert(yalain_testkit::test_template(
        LEVER_TEMPLATE,
        LEVER_PACKAGE,
        lever,
    ));
    h
}

fn row(levers: Vec<usize>, open_for: Option<f64>, within: Option<f64>) -> Lever {
    Lever {
        key: "gate".into(),
        levers,
        opens: vec![0],
        open_for,
        within,
    }
}

/// A use of an object, as the client asks for it.
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

/// What a client saw of the door, of the pulls and of its own uses since `from`, in order.
fn seen(h: &Harness, b: usize, from: usize) -> Vec<String> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::UpdateIntVisual(q)
                if q.object.id == DOOR && q.quality == quality::PHYSICS =>
            {
                Some(if q.value == quality::PASSABLE {
                    "passable".to_string()
                } else {
                    "solid".to_string()
                })
            }
            ServerMessage::DoMode(e) if e.sender.id == DOOR => Some(match e.action.0 {
                mode::OPEN => "opening".to_string(),
                mode::CLOSED => "closing".to_string(),
                other => format!("mode {other:X}"),
            }),
            ServerMessage::UpdateBoolVisual(q)
                if q.object.id == DOOR && q.quality == quality::OPEN =>
            {
                Some(if q.value == 1 { "open" } else { "shut" }.to_string())
            }
            ServerMessage::PlayBehavior(e) if e.action.behavior == PULLING => {
                Some("pulls".to_string())
            }
            ServerMessage::PlayBehavior(e)
                if e.action.behavior == LEVER_TURNS
                    && [LEVER, OTHER_LEVER].contains(&e.sender.id) =>
            {
                Some("lever turns".to_string())
            }
            ServerMessage::PlayerCall(c) if c.function.0 == USE_DONE => {
                let v = c.args.as_ref().and_then(|a| calls::values(&a.0).ok())?;
                let Some(Value::Graph(g)) = v.first() else {
                    return None;
                };
                let status = g.iter().find_map(|(_, n)| match n {
                    Node::Script { slots, .. } => Some(slots[11].1),
                    _ => None,
                })?;
                Some(format!("done {status}"))
            }
            _ => None,
        })
        .collect()
}

fn send_use(h: &mut Harness, b: usize, me: u64, item: u64, seconds: f64) {
    h.bots[b].send(true, queue::WEENIE, &server_call(USE, &[use_of(item, me)]));
    run_for(h, seconds);
}

fn run_for(h: &mut Harness, seconds: f64) {
    let t = h.now.0 + seconds;
    while h.now.0 < t {
        h.step();
    }
}

/// A lever pulled plays the pulling on its puller and its turning on itself, and the use is
/// answered; the locked door its row names, which refuses a plain use, opens for everyone as a use
/// opens a door, stays open the row's thirty seconds (not a door's ten), then closes by itself.
/// Behaviour: server.places.lever-opens-its-doors-for-a-while
#[test]
fn a_lever_pulled_opens_its_locked_door_for_everyone_for_its_time() {
    let mut h = hall(vec![row(vec![1], Some(30.0), None)]);
    let (a, me) = h.player_in_world("one", "Elara");
    let (b, _) = h.player_in_world("two", "Babyfare");
    h.run(10);
    let fa = h.bots[a].inbox.len();
    send_use(&mut h, a, me, DOOR, 1.0);
    assert_eq!(seen(&h, a, fa), ["done 10030"], "locked to a plain use");

    let (fa, fb) = (h.bots[a].inbox.len(), h.bots[b].inbox.len());
    send_use(&mut h, a, me, LEVER, 2.0);
    assert_eq!(
        seen(&h, a, fa),
        [
            "lever turns",
            "pulls",
            "done 0",
            "passable",
            "opening",
            "open"
        ]
    );
    assert_eq!(
        seen(&h, b, fb),
        ["lever turns", "pulls", "passable", "opening", "open"],
        "the door opens for everyone"
    );
    run_for(&mut h, 25.0);
    assert_eq!(
        seen(&h, b, fb).len(),
        5,
        "still open past a door's own ten seconds"
    );
    run_for(&mut h, 6.0);
    assert_eq!(
        seen(&h, b, fb)[5..],
        ["solid", "closing"],
        "closes by itself at the row's time"
    );
}

/// A row of two levers opens nothing at one pull; a pull of the other within the row's time
/// opens the door; a pull left waiting past that time is forgotten, so the other's pull alone
/// opens nothing.
/// Behaviour: server.places.lever-opens-its-doors-for-a-while
#[test]
fn a_row_of_two_levers_opens_its_door_only_once_both_are_pulled_within_its_time() {
    let mut h = hall(vec![row(vec![1, 2], Some(30.0), Some(20.0))]);
    let (a, me) = h.player_in_world("one", "Elara");
    h.run(10);
    let opened = |h: &Harness, from: usize| seen(h, a, from).iter().any(|x| x == "opening");
    let from = h.bots[a].inbox.len();
    send_use(&mut h, a, me, LEVER, 3.0);
    assert!(!opened(&h, from), "one pull opens nothing");
    run_for(&mut h, 25.0);
    send_use(&mut h, a, me, OTHER_LEVER, 3.0);
    assert!(!opened(&h, from), "the first pull was forgotten");
    let from = h.bots[a].inbox.len();
    send_use(&mut h, a, me, LEVER, 3.0);
    assert!(opened(&h, from), "both within the row's time open the door");
}

/// A row with no time holds its door open until a use closes it.
/// Behaviour: server.places.lever-opens-its-doors-for-a-while
#[test]
fn a_lever_with_no_time_holds_its_door_open_until_a_use_closes_it() {
    let mut h = hall(vec![row(vec![1], None, None)]);
    let (a, me) = h.player_in_world("one", "Elara");
    h.run(10);
    let from = h.bots[a].inbox.len();
    send_use(&mut h, a, me, LEVER, 2.0);
    run_for(&mut h, 60.0);
    assert_eq!(
        seen(&h, a, from)[3..],
        ["passable", "opening", "open"],
        "open a minute on"
    );
    let from = h.bots[a].inbox.len();
    send_use(&mut h, a, me, DOOR, 4.0);
    assert_eq!(
        seen(&h, a, from),
        ["solid", "closing", "shut", "done 0"],
        "a use closes it"
    );
}
