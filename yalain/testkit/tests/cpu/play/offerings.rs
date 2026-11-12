//! Offerings: an object that takes a set of items from players keeps each part given it, whoever
//! gives it, standing a flare for each part it holds; the last part opens its portal for its time,
//! the flares gone; a part it holds, or any while it is open, is handed back; once the portal goes
//! it holds nothing again. What it holds is a world record. Fixture: the test hall with the object
//! beside the start, three flares and a portal near it, two players, and three part templates.

use auberean_protocol::{queue, ServerMessage};
use yalain_content::offerings::{Offering, OfferingTable, Part};
use yalain_content::{ContentObject, ContentPack, Description, Facts, Kind, Props};
use yalain_testkit::play::server_call;
use yalain_testkit::Harness;
use yalain_world::gives::{GIVE, HANDED_AFTER};
use yalain_world::inventory::{move_graph, Move};
use yalain_world::offerings::Offerings;

const ALTAR: u64 = 0x8130_0000_0000_0E81;
const FLARES: [u64; 3] = [
    0x8130_0000_0000_0E82,
    0x8130_0000_0000_0E83,
    0x8130_0000_0000_0E84,
];
const PORTAL: u64 = 0x8130_0000_0000_0E85;
const ALTAR_T: u32 = 0x4700_2F5D;
const FLARE_T: u32 = 0x4700_2F58;
const PORTAL_T: u32 = 0x4700_131E;
const PARTS: [u32; 3] = [0x4700_7E81, 0x4700_7E82, 0x4700_7E83];
const LASTS: u32 = 300;

fn place(id: u64, template: u32, origin: [f32; 2]) -> ContentObject {
    ContentObject {
        object: Some(id),
        template,
        description: Description {
            name_text: None,
            package: 0x103E,
            look: 0x1F00_0100,
            enter_fx: 0,
            scale: None,
            name: Some((0xA860_7CEC, 0x2500_1B44)),
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

/// The hall: the object (place 0), a flare for each part (1 to 3) and the portal (4), the
/// offering taking the three parts and opening the portal for its time.
fn hall() -> Harness {
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    for part in PARTS {
        h.server.catalog.insert(yalain_testkit::test_template(
            part,
            0x1067,
            Props::default(),
        ));
    }
    h.server.content = ContentPack {
        objects: vec![
            place(ALTAR, ALTAR_T, [58.0, 241.0]),
            place(FLARES[0], FLARE_T, [57.0, 243.0]),
            place(FLARES[1], FLARE_T, [58.0, 243.0]),
            place(FLARES[2], FLARE_T, [59.0, 243.0]),
            place(PORTAL, PORTAL_T, [58.0, 244.0]),
        ],
        offerings: OfferingTable {
            offerings: vec![Offering {
                key: "chaos-portal".into(),
                object: 0,
                takes: (0..3)
                    .map(|i| Part {
                        item: PARTS[i],
                        shows: Some(i + 1),
                    })
                    .collect(),
                opens: vec![4],
                lasts: LASTS,
            }],
        },
        ..ContentPack::default()
    };
    h
}

/// A player in the world whose client has drawn it.
fn player(h: &mut Harness, account: &str, name: &str) -> (usize, u64) {
    let (b, me) = h.player_in_world(account, name);
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
    (b, me)
}

/// A part put in a player's pack; the item and its slot.
fn hold(h: &mut Harness, me: u64, template: u32) -> (u64, i32) {
    let world = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .expect("in the world")
        .0;
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
    h.server
        .players
        .values()
        .find(|p| p.character == me)
        .is_some_and(|p| p.play.items.iter().any(|x| x.id == item))
}

/// A player gives a part of its pack to the object, handed over and answered.
fn give(h: &mut Harness, b: usize, me: u64, template: u32) -> u64 {
    let (item, slot) = hold(h, me, template);
    let m = Move {
        move_type: GIVE,
        merge: true,
        unlock: true,
        check_take: true,
        all_taken: true,
        grab: true,
        contain: true,
        auto_merge: true,
        target: ALTAR,
        actual_target: ALTAR,
        merge_container: ALTAR,
        from: me,
        actual_from: me,
        item,
        split_item: item,
        quantity: 1,
        quantity_left: 1,
        from_slot: slot,
        actual_from_slot: slot,
        ..Move::default()
    };
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(yalain_world::drops::function::MOVE_ITEM, &[move_graph(&m)]),
    );
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    h.run((HANDED_AFTER / yalain_testkit::STEP).ceil() as usize + 8);
    item
}

/// The placed objects among `ids` a client was made, and those it forgot, since `from`.
fn made_and_forgotten(h: &Harness, b: usize, from: usize, ids: &[u64]) -> (Vec<u64>, Vec<u64>) {
    let mut made = Vec::new();
    let mut gone = Vec::new();
    for m in h.world_messages(b, from) {
        match m {
            ServerMessage::CreateObject(o) if ids.contains(&o.id) => made.push(o.id),
            ServerMessage::ForgetObject(o) if ids.contains(&o.0.id) => gone.push(o.0.id),
            _ => {}
        }
    }
    (made, gone)
}

fn known(h: &Harness, me: u64, id: u64) -> bool {
    h.server
        .players
        .values()
        .find(|p| p.character == me)
        .is_some_and(|p| p.known.contains_key(&id))
}

/// Was an item destroyed in the object, as a client watching was told?
fn destroyed_in_it(h: &Harness, b: usize, from: usize) -> bool {
    h.world_messages(b, from).iter().any(|m| {
        matches!(m, ServerMessage::VisualObjectCall(c)
            if c.object.id == ALTAR && c.call.function.0 == yalain_world::gives::DESTROY_CONTAINED)
    })
}

/// Run the clock on to `unix`, a second at a time.
fn run_to_unix(h: &mut Harness, unix: i64) {
    while h.unix < unix {
        h.step_by(1.0);
    }
    h.run(4);
}

/// Each part given, by either player, is kept (gone from the giver's pack, destroyed in the object
/// as the other player is told) and its flare is made for both; a part it holds already is handed
/// back. The third keeps the part, the two flares go and the portal is made, the third's flare
/// never; a part given while it is open is handed back. As its time ends the portal goes, and the
/// first part is taken again.
/// Behaviour: server.gives.offering-opens-the-way-once-it-holds-its-set
#[test]
fn an_offering_keeps_each_part_and_opens_its_portal_for_its_time_once_it_holds_them_all() {
    let mut h = hall();
    let (a, me) = player(&mut h, "one", "Elara");
    let (b, other) = player(&mut h, "two", "Bran");
    let all: Vec<u64> = FLARES.iter().copied().chain([PORTAL]).collect();
    assert!(
        all.iter().all(|id| !known(&h, me, *id)),
        "no flare and no portal before anything is given"
    );

    let fb = h.bots[b].inbox.len();
    let first = give(&mut h, a, me, PARTS[0]);
    assert!(!holds(&h, me, first), "kept");
    assert!(destroyed_in_it(&h, b, fb), "destroyed in the object");
    assert_eq!(
        made_and_forgotten(&h, b, fb, &all),
        (vec![FLARES[0]], vec![]),
        "its flare made for the other player"
    );

    let again = give(&mut h, a, me, PARTS[0]);
    assert!(holds(&h, me, again), "a part it holds is handed back");

    let fa = h.bots[a].inbox.len();
    let second = give(&mut h, b, other, PARTS[1]);
    assert!(!holds(&h, other, second), "kept, whoever gives it");
    assert_eq!(
        made_and_forgotten(&h, a, fa, &all),
        (vec![FLARES[1]], vec![])
    );

    let fa = h.bots[a].inbox.len();
    let third = give(&mut h, a, me, PARTS[2]);
    assert!(!holds(&h, me, third));
    let (made, mut gone) = made_and_forgotten(&h, a, fa, &all);
    gone.sort_unstable();
    assert_eq!(
        (made, gone),
        (vec![PORTAL], vec![FLARES[0], FLARES[1]]),
        "the portal comes as the flares go"
    );
    assert!(!known(&h, me, FLARES[2]), "the third flare never stands");

    let late = give(&mut h, b, other, PARTS[1]);
    assert!(holds(&h, other, late), "handed back while it is open");
    assert!(known(&h, other, PORTAL), "the portal stands for everyone");

    let opened = h.unix;
    run_to_unix(&mut h, opened + i64::from(LASTS) - 10);
    assert!(known(&h, me, PORTAL), "it stands its time");
    let fa = h.bots[a].inbox.len();
    run_to_unix(&mut h, opened + i64::from(LASTS) + 2);
    assert_eq!(
        made_and_forgotten(&h, a, fa, &all),
        (vec![], vec![PORTAL]),
        "then it goes"
    );
    let fresh = give(&mut h, a, me, PARTS[0]);
    assert!(!holds(&h, me, fresh), "it holds nothing again");
    assert!(known(&h, me, FLARES[0]));
}

/// The world record of what offerings hold reads back as it was written, and keeps only what the
/// world's offerings still name.
/// Behaviour: server.gives.offering-opens-the-way-once-it-holds-its-set
#[test]
fn what_offerings_hold_is_kept_as_a_record_that_reads_back() {
    let mut o = Offerings::default();
    o.held
        .entry("chaos-portal".into())
        .or_default()
        .extend([PARTS[0], PARTS[2]]);
    o.held.entry("gone".into()).or_default().insert(PARTS[1]);
    o.open_until.insert("chaos-portal".into(), 1_700_000_300);
    let back = Offerings::from_bytes(&o.to_bytes()).expect("reads");
    assert_eq!(back, o);
    let mut kept = back;
    kept.keep_only(&hall().server.content.offerings);
    assert_eq!(kept.held.keys().collect::<Vec<_>>(), ["chaos-portal"]);
    assert_eq!(kept.open_until.len(), 1);
}
