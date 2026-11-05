//! Vaults: a glyph used on its vault plays its use, the vault flashes and the player goes through
//! portal space to the vault's destination, where the glyph is used up and the use answered that it
//! fits; a vault used bare or with another glyph needs its own; one another player's trip is going
//! through refuses at once, and a trip too soon after the vault's last is refused as its use ends.
//! Fixture: the test hall with a vault beside the start leading elsewhere in the hall, and glyphs
//! of two vaults' templates.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_content::{ContentObject, ContentPack, Description, Facts, Kind, Lock, Place, Props};
use yalain_testkit::play::server_call;
use yalain_testkit::Harness;
use yalain_world::calls::{self, Node, Value};
use yalain_world::usable::vaults::{self, refusal};

const VAULT: u64 = 0x4002_9900_0000_0072;
const VAULT_TEMPLATE: u32 = 0x4700_1CE8;
const ESPER_GLYPH: u32 = 0x4700_10B1;
const GEVOTH_GLYPH: u32 = 0x4700_10B3;
const ESPER_KEY: u32 = 0x4000_0021;
const USE: u32 = 0x8216_0016;
const USE_DONE: u32 = 0x8216_0021;
const DISCRETE_DESTROY: u32 = 0x820E_0058;
const USES_ON: u32 = 0x4100_00FC;
const FLASH: u32 = 0x4100_0044;
const FITS: u32 = 0x2317_692F;
const AWAY: [f32; 3] = [20.0, 200.0, -44.6];

/// A use of an item (on an object, or on nothing), as the client asks for it.
fn use_of(item: u64, me: u64, on: u64) -> Arg {
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
            Slot::Long(on),
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

/// The test hall with a vault a step from the start, locked to the Esper glyph, leading elsewhere
/// in the hall.
fn hall() -> Harness {
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    h.server.content = ContentPack {
        objects: vec![ContentObject {
            object: Some(VAULT),
            template: VAULT_TEMPLATE,
            description: Description {
                name_text: None,
                package: 0x1A01,
                look: 0x1F00_0ED5,
                enter_fx: 0,
                scale: None,
                name: Some((0x3F7C_29E0, 0x2500_1B40)),
                numbers: [0; 6],
                source: "authored for a test".into(),
            },
            cell: 0x0299_0109,
            origin: [60.0, 239.0, -44.6],
            rotation: [1.0, 0.0, 0.0, 0.0],
            kind: Kind::Static,
            label: String::new(),
            placed: "authored for a test".into(),
            destination: Some(Place {
                cell: 0x0299_0109,
                origin: AWAY,
                rotation: [1.0, 0.0, 0.0, 0.0],
            }),
            facts: Facts::default(),
            walks: None,
            gate: None,
        }],
        ..ContentPack::default()
    };
    h.server.locks.objects.insert(
        VAULT,
        Lock {
            key: Some(ESPER_KEY),
        },
    );
    for t in [ESPER_GLYPH, GEVOTH_GLYPH] {
        h.server.catalog.insert(yalain_content::Template {
            facts: yalain_content::Facts {
                name: Some((0x2B0C_0002, 0x2500_0002)),
                ..yalain_content::Facts::default()
            },
            ..yalain_testkit::test_template(t, 0x18F, Props::default())
        });
    }
    h
}

/// A player in the hall holding a glyph of a template: its client, id, connection and glyph.
fn with_glyph(h: &mut Harness, name: &str, glyph: u32) -> (usize, u64, yalain_net::ConnId, u64) {
    let (b, me) = h.player_in_world(name, name);
    h.run(10);
    let world = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .expect("in")
        .0;
    yalain_world::inventory::give(&mut h.server, &mut h.net, world, glyph, 1);
    h.run(3);
    let item = held(h, world, glyph).expect("a glyph");
    (b, me, world, item)
}

fn held(h: &Harness, world: yalain_net::ConnId, template: u32) -> Option<u64> {
    h.server.players[&world]
        .play
        .items
        .iter()
        .find(|i| i.template == template)
        .map(|i| i.id)
}

/// The use answers a client was sent since `from`: (status, control flags, its text).
fn answers(h: &Harness, b: usize, from: usize) -> Vec<(u32, u32, u32)> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c) if c.function.0 == USE_DONE => {
                let v = c.args.as_ref().and_then(|a| calls::values(&a.0).ok())?;
                let Some(Value::Graph(g)) = v.first() else {
                    return None;
                };
                let (status, flags) = g.iter().find_map(|(_, n)| match n {
                    Node::Script { slots, .. } => Some((slots[11].1, slots[17].1)),
                    _ => None,
                })?;
                let text = g
                    .iter()
                    .find_map(|(_, n)| match n {
                        Node::TextInfo(t) => Some(t.string_id),
                        _ => None,
                    })
                    .unwrap_or(0);
                Some((status, flags, text))
            }
            _ => None,
        })
        .collect()
}

fn flashed(h: &Harness, b: usize, from: usize) -> bool {
    h.world_messages(b, from).iter().any(
        |m| matches!(m, ServerMessage::DoFx(e) if e.sender.id == VAULT && e.action.fx == FLASH),
    )
}

fn send_use(h: &mut Harness, b: usize, me: u64, item: u64, on: u64, seconds: f64) {
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(USE, &[use_of(item, me, on)]),
    );
    run_for(h, seconds);
}

fn run_for(h: &mut Harness, seconds: f64) {
    let t = h.now.0 + seconds;
    while h.now.0 < t {
        h.step();
    }
}

/// A glyph used on its vault: the player plays its use and nothing is answered yet; when the use
/// has played the vault flashes and the player goes through portal space; on arrival at the vault's
/// destination the glyph is used up and the use answered with the flags of an item used on an
/// object and "Your glyph fits the lock.".
/// Behaviour: server.places.glyph-opens-its-vault
#[test]
fn a_glyph_used_on_its_vault_takes_the_player_through_and_is_used_up() {
    let mut h = hall();
    let (a, me, world, glyph) = with_glyph(&mut h, "one", ESPER_GLYPH);
    let fa = h.bots[a].inbox.len();
    send_use(&mut h, a, me, glyph, VAULT, 1.0);
    let msgs = h.world_messages(a, fa);
    assert!(
        msgs.iter().any(|m| matches!(m,
            ServerMessage::PlayBehavior(e) if e.action.behavior == USES_ON)),
        "the glyph's use plays"
    );
    assert!(answers(&h, a, fa).is_empty(), "nothing answered yet");
    assert!(!flashed(&h, a, fa), "the vault waits for the use");
    run_for(&mut h, vaults::OPENS_AFTER);
    assert!(flashed(&h, a, fa), "the vault flashes as it opens");
    run_for(&mut h, 2.0);
    assert_eq!(answers(&h, a, fa), [(0, 317, FITS)], "answered on arrival");
    assert!(
        h.world_messages(a, fa).iter().any(|m| matches!(m,
            ServerMessage::PlayerCall(c) if c.function.0 == DISCRETE_DESTROY)),
        "the glyph destroyed"
    );
    assert_eq!(held(&h, world, ESPER_GLYPH), None, "used up");
    let at = h.server.players[&world].position.origin;
    assert!(
        (at[0] - AWAY[0]).abs() < 0.01 && (at[1] - AWAY[1]).abs() < 0.01,
        "at the vault's destination"
    );
}

/// A vault used bare, or with a glyph not its own, needs its glyph: refused with the vault's flash,
/// the glyph kept and the player not moved.
/// Behaviour: server.places.vault-needs-its-glyph
#[test]
fn a_vault_used_bare_or_with_another_glyph_needs_its_own() {
    let mut h = hall();
    let (a, me, world, other) = with_glyph(&mut h, "one", GEVOTH_GLYPH);
    let fa = h.bots[a].inbox.len();
    send_use(&mut h, a, me, VAULT, 0, 0.5);
    assert_eq!(
        answers(&h, a, fa),
        [(refusal::GLYPH_NEEDED, refusal::GLYPH_NEEDED_FLAGS, 0)]
    );
    assert!(flashed(&h, a, fa));
    let fa = h.bots[a].inbox.len();
    send_use(&mut h, a, me, other, VAULT, 4.0);
    assert_eq!(
        answers(&h, a, fa),
        [(refusal::GLYPH_NEEDED, refusal::GLYPH_NEEDED_FLAGS, 0)],
        "not its glyph"
    );
    assert!(held(&h, world, GEVOTH_GLYPH).is_some(), "kept");
    assert!(h.server.players[&world].trip.is_none(), "not taken");
}

/// While another player's trip through a vault is under way a glyph use is refused at once; a trip
/// that would come within five seconds of the vault's last is refused as the use ends, the glyph
/// kept.
/// Behaviour: server.places.vault-one-trip-at-a-time
#[test]
fn a_vault_refuses_while_another_goes_through_and_too_soon_after() {
    let mut h = hall();
    let (a, me, _, glyph) = with_glyph(&mut h, "one", ESPER_GLYPH);
    let (b, you, world_b, yours) = with_glyph(&mut h, "two", ESPER_GLYPH);
    send_use(&mut h, a, me, glyph, VAULT, 0.5);
    let fb = h.bots[b].inbox.len();
    send_use(&mut h, b, you, yours, VAULT, 0.5);
    assert_eq!(
        answers(&h, b, fb),
        [(refusal::IN_USE, refusal::IN_USE_FLAGS, 0)],
        "the other's trip under way"
    );
    // The first trip arrives; a use now would open within five seconds of it.
    run_for(&mut h, 3.4);
    assert!(h
        .server
        .players
        .values()
        .any(|p| p.character == me && p.trip.is_none()));
    let fb = h.bots[b].inbox.len();
    send_use(&mut h, b, you, yours, VAULT, 0.5);
    assert!(answers(&h, b, fb).is_empty(), "its use plays first");
    run_for(&mut h, vaults::OPENS_AFTER);
    assert_eq!(
        answers(&h, b, fb),
        [(refusal::TOO_SOON, refusal::TOO_SOON_FLAGS, FITS)],
        "too soon after the vault's last trip"
    );
    assert!(held(&h, world_b, ESPER_GLYPH).is_some(), "kept");
    assert!(h.server.players[&world_b].trip.is_none(), "not taken");
}

/// A move of a glyph, as a client asks for it to go down.
fn put_down(me: u64, glyph: u64) -> auberean_protocol::ClientMessage {
    use yalain_world::inventory::{move_graph, Move};
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
        item: glyph,
        split_item: glyph,
        quantity: 1,
        quantity_left: 1,
        ..Move::default()
    };
    server_call(
        yalain_world::drops::function::MOVE_ITEM,
        &[move_graph(&down)],
    )
}

/// A glyph stays in the pack for the whole of its trip, as it plays and through portal space, and
/// is used up on arrival; one on its way to the ground opens no vault.
/// Behaviour: server.places.a-glyph-stays-for-its-trip
#[test]
fn a_glyph_cannot_be_put_down_on_its_trip_and_one_put_down_opens_nothing() {
    for when in [0.05, 2.5] {
        let mut h = hall();
        let (a, me, world, glyph) = with_glyph(&mut h, "one", ESPER_GLYPH);
        send_use(&mut h, a, me, glyph, VAULT, when);
        h.bots[a].send(true, queue::WEENIE, &put_down(me, glyph));
        run_for(&mut h, 6.0);
        assert_eq!(held(&h, world, ESPER_GLYPH), None, "used up");
        assert!(h.server.things.get(glyph).is_none(), "gone at {when}");
    }
    let mut h = hall();
    let (a, me, world, glyph) = with_glyph(&mut h, "one", ESPER_GLYPH);
    let was = h.server.players[&world].position.origin;
    h.bots[a].send(true, queue::WEENIE, &put_down(me, glyph));
    h.run(2);
    send_use(&mut h, a, me, glyph, VAULT, 6.0);
    assert_eq!(
        h.server.players[&world].position.origin, was,
        "it went nowhere"
    );
}

/// A player taken out of the world on its way through a vault leaves it free for the next.
/// Behaviour: server.places.a-vault-is-free-once-its-traveler-has-left
#[test]
fn a_vault_is_free_for_the_next_once_its_traveler_has_left_the_world() {
    let mut h = hall();
    let (a, ida, _, glyph) = with_glyph(&mut h, "one", ESPER_GLYPH);
    let (b, idb, _, second) = with_glyph(&mut h, "two", ESPER_GLYPH);
    send_use(&mut h, a, ida, glyph, VAULT, 0.5);
    let clock = h.clock();
    let name = h
        .server
        .players
        .values()
        .find(|p| p.character == ida)
        .expect("in")
        .name
        .clone();
    yalain_world::characters::force_logoff(&mut h.server, &mut h.net, clock, &name)
        .expect("out of the world");
    run_for(&mut h, vaults::BETWEEN_TRIPS + 5.0);
    let from = h.bots[b].inbox.len();
    send_use(&mut h, b, idb, second, VAULT, 0.5);
    assert!(
        !answers(&h, b, from)
            .iter()
            .any(|(status, ..)| *status == refusal::IN_USE),
        "not held for one no longer there"
    );
    assert!(h
        .server
        .players
        .values()
        .any(|p| p.character == idb && p.trip.is_some()));
}
