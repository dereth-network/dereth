//! Objects whose state items given them change, and uses in an order judged against those states:
//! an item a change names puts the object in its state for everyone and comes back; any other is
//! refused; the object goes back to its first state after its row's time; right uses in order move
//! a quest on a phase each, the first giving it; a wrong use takes it away and the order starts
//! over; one use moves on every order it fits. Fixture: the test hall with three tiles beside the
//! start, of a kind the server knows as no person's (an order naming them makes their use a
//! person's), two orbs that change them, and two orders of uses sharing their first.

use auberean_protocol::queue;
use auberean_script::args::Arg;
use yalain_content::item_rows::{ItemRows, ObjectStates, Sequence, StateChange};
use yalain_content::quests::{Place, QuestDef, QuestTable, Step, Trigger, Update};
use yalain_content::{ContentObject, ContentPack, Description, Facts, Kind, Props};
use yalain_testkit::play::server_call;
use yalain_testkit::Harness;
use yalain_world::calls::Node;
use yalain_world::gives::GIVE;
use yalain_world::inventory::{move_graph, Move};
use yalain_world::puzzles;

const USE: u32 = 0x8216_0016;
const HEAR_TELL: u32 = 0x8205_0017;
const PACKAGE: u32 = 0x13A4;
const TABLE: u32 = 0x2500_29AC;
const RIGHT: u32 = 0x257C_1464;
const WRONG: u32 = 0xFFFB_8534;
const STEP_LINE: u32 = 0x30B9_998D;

/// Three tiles, each its own template, beside the start.
const TILE_A: u64 = 0x8130_0000_0000_0901;
const TILE_B: u64 = 0x8130_0000_0000_0902;
const TILE_C: u64 = 0x8130_0000_0000_0903;
const A: u32 = 0x4700_7801;
const B: u32 = 0x4700_7802;
const C: u32 = 0x4700_7803;
/// The orbs: green on a red tile makes it white, red on a green one; green on a white one, red.
const RED_ORB: u32 = 0x4700_7811;
const GREEN_ORB: u32 = 0x4700_7812;
/// The orders' quests: one of A white, B white, C in any state; the other of A white, then C.
const LONG: u32 = 0x8000_7821;
const SHORT: u32 = 0x8000_7822;
const REVERTS: f64 = 30.0;
/// Where the long order's last right use takes its player.
const AWAY: Place = Place {
    cell: 0x0299_0109,
    origin: [20.0, 200.0, -44.6],
    heading: 0,
};

fn tile(id: u64, template: u32, x: f32) -> ContentObject {
    ContentObject {
        object: Some(id),
        template,
        description: Description {
            name_text: None,
            package: PACKAGE,
            look: 0x1F00_0FC8,
            enter_fx: 0,
            scale: None,
            name: Some((0xA860_7CEC, TABLE)),
            numbers: [0; 6],
            source: "authored for a test".into(),
        },
        cell: 0x0299_0109,
        origin: [x, 241.0, -44.6],
        rotation: [1.0, 0.0, 0.0, 0.0],
        kind: Kind::Npc,
        label: String::new(),
        placed: "authored for a test".into(),
        destination: None,
        facts: Facts::default(),
        walks: None,
        gate: None,
    }
}

fn quest(id: u32, phases: u32) -> QuestDef {
    QuestDef {
        id,
        name: (0, 0),
        description: (0, 0),
        icon: 0,
        play_fx: false,
        journals: (0..phases).map(|i| (i + 1, 0)).collect(),
        challenge: 0,
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
        once: true,
        fellows: yalain_content::quests::Fellows::default(),
        last_waits: false,
        hero_only: false,
        pay: yalain_content::quests::CompletionPay::default(),
        required: None,
        cancel_wait: None,
    }
}

fn states() -> ObjectStates {
    let change = |item, from: &str, to: &str| StateChange {
        item,
        from: from.into(),
        to: to.into(),
    };
    ObjectStates {
        key: "test-tiles".into(),
        templates: vec![(A, "red".into()), (B, "green".into()), (C, "red".into())],
        states: vec!["red".into(), "green".into(), "white".into()],
        reverts: Some(REVERTS),
        changes: vec![
            change(GREEN_ORB, "red", "white"),
            change(RED_ORB, "green", "white"),
            change(GREEN_ORB, "white", "red"),
        ],
    }
}

fn sequences() -> Vec<Sequence> {
    vec![
        Sequence {
            key: "long".into(),
            quest: LONG,
            uses: vec![
                (A, Some("white".into())),
                (B, Some("white".into())),
                (C, None),
            ],
            right: Some((RIGHT, TABLE)),
            wrong: Some((WRONG, TABLE)),
            to: Some(AWAY),
        },
        Sequence {
            key: "short".into(),
            quest: SHORT,
            uses: vec![(A, Some("white".into())), (C, None)],
            right: Some((RIGHT, TABLE)),
            wrong: Some((WRONG, TABLE)),
            to: None,
        },
    ]
}

/// The hall with the three tiles, the orbs, the two orders' quests and, unless `bare`, the rows.
fn hall(bare: bool) -> Harness {
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    h.server.content = ContentPack {
        objects: vec![
            tile(TILE_A, A, 59.0),
            tile(TILE_B, B, 60.0),
            tile(TILE_C, C, 61.0),
        ],
        ..ContentPack::default()
    };
    for t in [RED_ORB, GREEN_ORB] {
        h.server
            .catalog
            .insert(yalain_testkit::test_template(t, 0x1005, Props::default()));
    }
    h.server.quests = QuestTable {
        quests: vec![quest(LONG, 3), quest(SHORT, 2)],
        ..QuestTable::default()
    };
    if !bare {
        h.server.item_rows = ItemRows {
            object_states: vec![states()],
            sequences: sequences(),
            ..ItemRows::default()
        };
    }
    h
}

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

fn world_of(h: &Harness, me: u64) -> yalain_net::ConnId {
    *h.server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .map(|(c, _)| c)
        .expect("in the world")
}

fn run_for(h: &mut Harness, seconds: f64) {
    let t = h.now.0 + seconds;
    while h.now.0 < t {
        h.step();
    }
}

/// An orb of a template in the player's pack: the item and its slot.
fn hold(h: &mut Harness, me: u64, template: u32) -> (u64, i32) {
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

/// The player gives an item to a tile, and the give runs its course.
fn give(h: &mut Harness, b: usize, me: u64, item: (u64, i32), target: u64) {
    let m = Move {
        move_type: GIVE,
        merge: true,
        unlock: true,
        check_take: true,
        all_taken: true,
        grab: true,
        contain: true,
        auto_merge: true,
        target,
        actual_target: target,
        merge_container: target,
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
    run_for(h, 4.0);
}

fn use_of(item: u64, player: u64) -> Arg {
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
            Slot::Long(player),
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

/// The player uses a tile; returns where its inbox stood before.
fn use_tile(h: &mut Harness, b: usize, me: u64, tile: u64) -> usize {
    let from = h.bots[b].inbox.len();
    h.bots[b].send(true, queue::WEENIE, &server_call(USE, &[use_of(tile, me)]));
    run_for(h, 2.0);
    from
}

/// The lines told the client since `from`.
fn tells(h: &Harness, b: usize, from: usize) -> Vec<u32> {
    h.calls(b, from)
        .into_iter()
        .filter(|(f, _)| *f == HEAR_TELL)
        .filter_map(|(_, v)| {
            v.iter().find_map(|x| match x.root() {
                Some(Node::TextInfo(t)) => Some(t.string_id),
                _ => None,
            })
        })
        .collect()
}

fn phase(h: &Harness, me: u64, quest: u32) -> u32 {
    yalain_world::quests::phase_of(&h.server.players[&world_of(h, me)].play.quests, quest)
}

fn state(h: &Harness, tile: u64) -> String {
    puzzles::state_of(&h.server, tile, h.clock()).expect("a tile holds a state")
}

fn holds(h: &Harness, me: u64, item: u64) -> bool {
    h.server.players[&world_of(h, me)]
        .play
        .items
        .iter()
        .any(|x| x.id == item)
}

/// An item a change names, given to an object in the change's first state, puts the object in the
/// change's other state and is handed back; the state is the object's, so another player's use is
/// judged by it too.
/// Behaviour: server.puzzles.an-item-given-changes-an-objects-state
#[test]
fn an_item_a_change_names_changes_the_objects_state_for_everyone_and_comes_back() {
    let mut h = hall(false);
    let (a, one) = player(&mut h, "one", "Elara");
    let (b, two) = player(&mut h, "two", "Corwin");
    assert_eq!(state(&h, TILE_A), "red", "its template's first");
    let orb = hold(&mut h, one, GREEN_ORB);
    give(&mut h, a, one, orb, TILE_A);
    assert_eq!(state(&h, TILE_A), "white");
    assert!(holds(&h, one, orb.0), "handed back, nothing used up");
    let fb = use_tile(&mut h, b, two, TILE_A);
    assert_eq!(
        phase(&h, two, LONG),
        1,
        "the other player's use finds it white"
    );
    assert_eq!(tells(&h, b, fb), [RIGHT]);
    give(&mut h, a, one, orb, TILE_A);
    assert_eq!(state(&h, TILE_A), "red", "a change from the state it is in");
}

/// An item no change names for the object's state is answered as any other give: refused and
/// handed back, the state as it was.
/// Behaviour: server.puzzles.an-item-given-changes-an-objects-state
#[test]
fn an_item_no_change_names_for_the_state_is_refused_and_the_state_stays() {
    let mut h = hall(false);
    let (a, one) = player(&mut h, "one", "Elara");
    let orb = hold(&mut h, one, RED_ORB);
    give(&mut h, a, one, orb, TILE_A);
    assert_eq!(state(&h, TILE_A), "red");
    assert!(holds(&h, one, orb.0), "handed back");
    give(&mut h, a, one, orb, TILE_B);
    assert_eq!(
        state(&h, TILE_B),
        "white",
        "red on the green tile does change it"
    );
}

/// An object goes back to its first state its row's time after its last change, not before.
/// Behaviour: server.puzzles.an-objects-state-goes-back
#[test]
fn an_object_goes_back_to_its_first_state_its_rows_time_after_its_last_change() {
    let mut h = hall(false);
    let (a, one) = player(&mut h, "one", "Elara");
    let orb = hold(&mut h, one, GREEN_ORB);
    give(&mut h, a, one, orb, TILE_A);
    assert_eq!(state(&h, TILE_A), "white");
    run_for(&mut h, REVERTS - 5.0);
    assert_eq!(state(&h, TILE_A), "white", "before its time");
    give(&mut h, a, one, orb, TILE_A);
    give(&mut h, a, one, orb, TILE_A);
    run_for(&mut h, REVERTS - 10.0);
    assert_eq!(state(&h, TILE_A), "white", "timed from the last change");
    run_for(&mut h, 12.0);
    assert_eq!(state(&h, TILE_A), "red", "back to its first");
}

/// Right uses in order, each object in the state its use names, move the order's quest on a phase
/// each, the first giving it, with the right line; the last use brings the quest to its last
/// phase, after which the order is past judging.
/// Behaviour: server.puzzles.uses-in-order-move-a-quest
#[test]
fn right_uses_in_order_move_the_quest_a_phase_each_the_first_giving_it() {
    let mut h = hall(false);
    // No trip at the last use: the player stays beside the tiles.
    h.server.item_rows.sequences[0].to = None;
    let (a, one) = player(&mut h, "one", "Elara");
    let green = hold(&mut h, one, GREEN_ORB);
    let red = hold(&mut h, one, RED_ORB);
    give(&mut h, a, one, green, TILE_A);
    give(&mut h, a, one, red, TILE_B);
    let from = use_tile(&mut h, a, one, TILE_A);
    assert_eq!((phase(&h, one, LONG), phase(&h, one, SHORT)), (1, 1));
    assert_eq!(tells(&h, a, from), [RIGHT], "said once for both");
    let from = use_tile(&mut h, a, one, TILE_B);
    assert_eq!(phase(&h, one, LONG), 2);
    assert_eq!(tells(&h, a, from), [RIGHT]);
    let from = use_tile(&mut h, a, one, TILE_C);
    assert_eq!(phase(&h, one, LONG), 3, "its last phase, C in any state");
    assert_eq!(tells(&h, a, from), [RIGHT]);
    let from = use_tile(&mut h, a, one, TILE_B);
    assert_eq!(phase(&h, one, LONG), 3, "past judging");
    assert_eq!(tells(&h, a, from), Vec::<u32>::new(), "no order judged it");
}

/// The last right use of an order whose row names a place takes its player there through portal
/// space; an order that names none leaves the player where it stands.
/// Behaviour: server.puzzles.the-last-right-use-takes-the-player-on
#[test]
fn the_last_right_use_takes_the_player_where_its_row_says() {
    let mut h = hall(false);
    let (a, one) = player(&mut h, "one", "Elara");
    let green = hold(&mut h, one, GREEN_ORB);
    let red = hold(&mut h, one, RED_ORB);
    give(&mut h, a, one, green, TILE_A);
    give(&mut h, a, one, red, TILE_B);
    use_tile(&mut h, a, one, TILE_A);
    use_tile(&mut h, a, one, TILE_B);
    let world = world_of(&h, one);
    let before = h.server.players[&world].position;
    use_tile(&mut h, a, one, TILE_C);
    assert_eq!(phase(&h, one, SHORT), 2, "the short order's last use");
    assert_eq!(phase(&h, one, LONG), 3);
    run_for(&mut h, 8.0);
    let here = h.server.players[&world].position;
    assert_ne!(here.origin, before.origin, "taken away");
    assert_eq!(here.cell, AWAY.cell);
    let (dx, dy) = (
        here.origin[0] - AWAY.origin[0],
        here.origin[1] - AWAY.origin[1],
    );
    assert!(dx * dx + dy * dy < 1.0, "{here:?}");
}

/// The last right use of an order whose row names a place takes its player there even when its
/// quest's update carries a recall elsewhere: the row's place wins, and the player goes once.
/// Behaviour: server.quests.a-steps-place-wins-over-its-updates-recall
#[test]
fn an_orders_own_place_wins_over_the_recall_its_last_update_carries() {
    use yalain_world::effect_defs::{EffectDef, Kind as EffectKind, Recall, Scale};
    const RECALL: u32 = 0x6F00_7801;
    let mut h = hall(false);
    let elsewhere = yalain_content::Place {
        cell: 0x049D_0105,
        origin: [10.0, 20.0, 3.0],
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    h.server
        .places_by_name
        .insert("test_elsewhere".into(), elsewhere);
    h.server.effect_table.effects.insert(
        RECALL,
        EffectDef {
            state: RECALL,
            kind: EffectKind::Recall(Recall::Named {
                names: vec!["test_elsewhere".into()],
                told: false,
            }),
            duration: Scale::default(),
            priority: Scale::default(),
            fx: 0,
            external_flags: 0,
            internal_flags: 0,
            enum_value: 0,
            eq_class: 0,
            name: None,
            ends_at_logout: false,
            ends_at_teleport: false,
        },
    );
    for q in &mut h.server.quests.quests {
        if q.id == LONG {
            q.effects = vec![(yalain_world::quests::update::ADVANCE | 3, RECALL)];
        }
    }
    let (a, one) = player(&mut h, "one", "Elara");
    let green = hold(&mut h, one, GREEN_ORB);
    let red = hold(&mut h, one, RED_ORB);
    give(&mut h, a, one, green, TILE_A);
    give(&mut h, a, one, red, TILE_B);
    use_tile(&mut h, a, one, TILE_A);
    use_tile(&mut h, a, one, TILE_B);
    let from = h.bots[a].inbox.len();
    use_tile(&mut h, a, one, TILE_C);
    assert_eq!(phase(&h, one, LONG), 3);
    run_for(&mut h, 8.0);
    let world = world_of(&h, one);
    assert_eq!(
        h.server.players[&world].position.cell, AWAY.cell,
        "the row's place"
    );
    let trips = h
        .world_messages(a, from)
        .iter()
        .filter(|m| {
            matches!(m, auberean_protocol::ServerMessage::Position(p) if p.sender.id == one && p.teleport_stamp > 0)
        })
        .count();
    assert_eq!(trips, 1, "one trip");
}

/// A use of another object the order names, or of the next in another state, takes the quest
/// away from a player who has it under way, with the wrong line, and the order starts over; one
/// use may move one order on and start another over.
/// Behaviour: server.puzzles.a-wrong-use-starts-the-order-over
#[test]
fn a_wrong_use_takes_the_quest_away_and_the_order_starts_over() {
    let mut h = hall(false);
    let (a, one) = player(&mut h, "one", "Elara");
    let from = use_tile(&mut h, a, one, TILE_A);
    assert_eq!(
        phase(&h, one, LONG),
        0,
        "the first in another state gives nothing"
    );
    assert_eq!(tells(&h, a, from), [WRONG]);
    let green = hold(&mut h, one, GREEN_ORB);
    give(&mut h, a, one, green, TILE_A);
    use_tile(&mut h, a, one, TILE_A);
    assert_eq!((phase(&h, one, LONG), phase(&h, one, SHORT)), (1, 1));
    let from = use_tile(&mut h, a, one, TILE_A);
    assert_eq!(
        (phase(&h, one, LONG), phase(&h, one, SHORT)),
        (0, 0),
        "A again, in the state B's use wants: not the next, both taken away"
    );
    assert_eq!(tells(&h, a, from), [WRONG]);
    use_tile(&mut h, a, one, TILE_A);
    assert_eq!((phase(&h, one, LONG), phase(&h, one, SHORT)), (1, 1));
    let from = use_tile(&mut h, a, one, TILE_B);
    assert_eq!(phase(&h, one, LONG), 0, "B is green, not white: taken away");
    assert_eq!(
        phase(&h, one, SHORT),
        1,
        "an order not naming B is not judged by it"
    );
    assert_eq!(tells(&h, a, from), [WRONG]);
    let from = use_tile(&mut h, a, one, TILE_C);
    assert_eq!(phase(&h, one, SHORT), 2, "right for the other");
    assert_eq!(
        phase(&h, one, LONG),
        0,
        "not the first of this one: nothing"
    );
    assert_eq!(tells(&h, a, from), [RIGHT]);
    use_tile(&mut h, a, one, TILE_A);
    assert_eq!(phase(&h, one, LONG), 1, "starts over");
    assert_eq!(phase(&h, one, SHORT), 2, "past its last use");
}

/// A world with no such rows answers a use and a give as before: the tile's quest step runs and
/// an orb is refused; with the rows a use of a tile an order names is judged by it alone.
/// Behaviour: server.puzzles.uses-in-order-move-a-quest
#[test]
fn without_such_rows_a_use_runs_its_step_and_a_give_is_refused() {
    let step = Step {
        quest: SHORT,
        needs: 0,
        trigger: Trigger::Use(A),
        tell: Some(yalain_content::Words::Ref(STEP_LINE, TABLE)),
        update: Update::Bestow,
        xp: 0,
        gives: Vec::new(),
        counts: Vec::new(),
        effects: Vec::new(),
        text: None,
        recipes: Vec::new(),
        placed: "authored for a test".into(),
        requires: Vec::new(),
        takes: Vec::new(),
        to: None,
        races: Vec::new(),
        effects_last: false,
        effects_quiet: false,
        fellows: false,
        wants: None,
        levels: None,
        xp_share: 0.0,
        transforms: None,
        chance: None,
    };
    let mut h = hall(true);
    h.server.quests.steps.push(step.clone());
    let (a, one) = player(&mut h, "one", "Elara");
    let orb = hold(&mut h, one, GREEN_ORB);
    give(&mut h, a, one, orb, TILE_A);
    assert!(puzzles::state_of(&h.server, TILE_A, h.clock()).is_none());
    assert!(holds(&h, one, orb.0));
    let from = use_tile(&mut h, a, one, TILE_A);
    assert_eq!(phase(&h, one, SHORT), 1, "its step ran");
    assert_eq!(tells(&h, a, from), [STEP_LINE]);

    let mut h = hall(false);
    h.server.quests.steps.push(step);
    let (a, one) = player(&mut h, "one", "Elara");
    let from = use_tile(&mut h, a, one, TILE_A);
    assert_eq!(phase(&h, one, SHORT), 0, "judged by its orders alone");
    assert_eq!(tells(&h, a, from), [WRONG]);
}
