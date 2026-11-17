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
        duration_by_calendar: false,
        id,
        name: yalain_content::Words::Ref(0, 0),
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
        fails_on_logout: false,
        fails_on_teleport: false,
        once: true,
        fellows: yalain_content::quests::Fellows::default(),
        ranges: Vec::new(),
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
            quest: Some(LONG),
            forgets_after: None,
            uses: vec![
                (A, Some("white".into())),
                (B, Some("white".into())),
                (C, None),
            ],
            objects: Vec::new(),
            wrong_starts_over: true,
            right: Some((RIGHT, TABLE)),
            wrong: Some((WRONG, TABLE)),
            to: Some(AWAY),
        },
        Sequence {
            key: "short".into(),
            quest: Some(SHORT),
            forgets_after: None,
            uses: vec![(A, Some("white".into())), (C, None)],
            objects: Vec::new(),
            wrong_starts_over: true,
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
            q.effects = vec![(yalain_world::quests::update::ADVANCE | 3, RECALL, 1.0)];
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
        out_of_turn: false,
        skills: Vec::new(),
        key: String::new(),
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
        to_fellows: false,
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

/// Three pillars of one template, and an order naming each by placement: the second, the first,
/// then the third.
fn pillars(wrong_starts_over: bool) -> (Harness, [u64; 3]) {
    const PILLARS: [u64; 3] = [
        0x8130_0000_0000_0911,
        0x8130_0000_0000_0912,
        0x8130_0000_0000_0913,
    ];
    let mut h = hall(true);
    h.server.content.objects = PILLARS
        .iter()
        .zip([59.0, 60.0, 61.0])
        .map(|(id, x)| tile(*id, A, x))
        .collect();
    h.server.item_rows = ItemRows {
        sequences: vec![Sequence {
            key: "pillars".into(),
            quest: Some(LONG),
            forgets_after: None,
            uses: vec![(A, None), (A, None), (A, None)],
            objects: vec![Some(1), Some(0), Some(2)],
            wrong_starts_over,
            right: Some((RIGHT, TABLE)),
            wrong: Some((WRONG, TABLE)),
            to: None,
        }],
        ..ItemRows::default()
    };
    (h, PILLARS)
}

/// An order's uses named by placement fit only their own objects, though all are of one template:
/// the second pillar first, then the first, then the third, each moving the quest on with the
/// right line; a pillar out of its turn is a wrong use, and the order starts over.
/// Behaviour: server.puzzles.uses-by-placement
#[test]
fn uses_named_by_placement_fit_only_their_own_objects_of_one_template() {
    let (mut h, p) = pillars(true);
    let (a, one) = player(&mut h, "one", "Elara");
    let from = use_tile(&mut h, a, one, p[0]);
    assert_eq!(phase(&h, one, LONG), 0, "not the first of the order");
    assert_eq!(tells(&h, a, from), [WRONG], "not its turn");
    let from = use_tile(&mut h, a, one, p[1]);
    assert_eq!(phase(&h, one, LONG), 1);
    assert_eq!(tells(&h, a, from), [RIGHT]);
    let from = use_tile(&mut h, a, one, p[2]);
    assert_eq!(phase(&h, one, LONG), 0, "out of its turn: started over");
    assert_eq!(tells(&h, a, from), [WRONG]);
    for (k, want) in [(1, 1), (0, 2), (2, 3)] {
        let from = use_tile(&mut h, a, one, p[k]);
        assert_eq!(phase(&h, one, LONG), want, "pillar {k}");
        assert_eq!(tells(&h, a, from), [RIGHT]);
    }
}

/// An order that starts over only for an object already used lets a pillar further on be tried:
/// told the wrong line, nothing moved; one already used starts it over.
/// Behaviour: server.puzzles.uses-by-placement
#[test]
fn an_order_starting_over_only_for_a_used_object_lets_one_further_on_be_tried() {
    let (mut h, p) = pillars(false);
    let (a, one) = player(&mut h, "one", "Elara");
    use_tile(&mut h, a, one, p[1]);
    assert_eq!(phase(&h, one, LONG), 1);
    let from = use_tile(&mut h, a, one, p[2]);
    assert_eq!(phase(&h, one, LONG), 1, "tried: nothing moved");
    assert_eq!(tells(&h, a, from), [WRONG]);
    let from = use_tile(&mut h, a, one, p[0]);
    assert_eq!(phase(&h, one, LONG), 2);
    assert_eq!(tells(&h, a, from), [RIGHT]);
    let from = use_tile(&mut h, a, one, p[1]);
    assert_eq!(phase(&h, one, LONG), 0, "one already used: started over");
    assert_eq!(tells(&h, a, from), [WRONG]);
}

/// How long a count of its own lasts after its last right use, in the temple's order.
const FORGETS: f64 = 120.0;
/// The temple's way in: an object of another template beside the pillars.
const WAY_IN: u64 = 0x8130_0000_0000_0914;

/// The temple: three pillars of one template and an order of them keeping its own count (the
/// second, the first, then the third), starting over only for a pillar already used, forgotten
/// [`FORGETS`] after its last right use; and a way in whose step gives the short quest only where
/// the order stands as `need` says.
fn temple(need: yalain_content::item_rows::SequenceNeed) -> (Harness, [u64; 3]) {
    use yalain_content::item_rows::SequenceGate;
    let (mut h, p) = pillars(false);
    h.server.content.objects.push(tile(WAY_IN, B, 62.0));
    let q = &mut h.server.item_rows.sequences[0];
    q.key = "temple".into();
    q.quest = None;
    q.forgets_after = Some(FORGETS);
    let step = Step {
        out_of_turn: false,
        skills: Vec::new(),
        key: "in".into(),
        quest: SHORT,
        needs: 0,
        trigger: Trigger::Use(B),
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
        to_fellows: false,
    };
    h.server.quests.steps.push(step.clone());
    h.server.item_rows.sequence_steps.push((
        step,
        SequenceGate {
            key: "temple".into(),
            need,
        },
    ));
    (h, p)
}

/// Put players in one fellowship, the first leading.
fn fellows(h: &mut Harness, ids: &[u64]) {
    use yalain_world::fellowship::{Fellow, Fellowship, Vitals};
    h.server.fellowships.list.push(Fellowship {
        name: "The Band".into(),
        leader: ids[0],
        flags: 0x1,
        room: 7,
        fellows: ids
            .iter()
            .map(|id| Fellow {
                id: *id,
                joined: 0.0,
                sent: Vitals::default(),
                next_check: f64::MAX,
            })
            .collect(),
        last_claimant: 0,
    });
}

/// An order with no quest keeps its count for the fellowship: fellows' right uses add up in one
/// count, told the right line, and give no quest; once it is done, every fellow is let in by the
/// way in's step and the order judges no more of their uses; a player in no fellowship has a count
/// of its own, none made, and is kept out.
/// Behaviour: server.puzzles.an-order-counted-for-the-fellowship
#[test]
fn an_order_of_its_own_counts_for_the_whole_fellowship_and_lets_every_fellow_in() {
    use yalain_content::item_rows::SequenceNeed;
    let (mut h, p) = temple(SequenceNeed::Done);
    let (a, one) = player(&mut h, "one", "Elara");
    let (b, two) = player(&mut h, "two", "Tysis");
    let (c, three) = player(&mut h, "three", "Orun");
    fellows(&mut h, &[one, two]);
    let from = use_tile(&mut h, a, one, p[1]);
    assert_eq!(tells(&h, a, from), [RIGHT]);
    let from = use_tile(&mut h, b, two, p[0]);
    assert_eq!(tells(&h, b, from), [RIGHT], "a fellow's use counts on");
    let from = use_tile(&mut h, a, one, p[2]);
    assert_eq!(tells(&h, a, from), [RIGHT], "the last");
    assert_eq!(phase(&h, one, LONG), 0, "no quest given");
    let from = use_tile(&mut h, b, two, p[1]);
    assert!(tells(&h, b, from).is_empty(), "done: not judged");
    for (bot, me) in [(a, one), (b, two)] {
        let from = use_tile(&mut h, bot, me, WAY_IN);
        assert_eq!(tells(&h, bot, from), [STEP_LINE], "a fellow let in");
        assert_eq!(phase(&h, me, SHORT), 1);
    }
    let from = use_tile(&mut h, c, three, WAY_IN);
    assert_ne!(
        tells(&h, c, from),
        [STEP_LINE],
        "one of no fellowship kept out"
    );
    assert_eq!(phase(&h, three, SHORT), 0);
    // Its own count: the second pillar first moves it on.
    let from = use_tile(&mut h, c, three, p[1]);
    assert_eq!(tells(&h, c, from), [RIGHT], "a count of its own");
}

/// A fellow's use of a pillar already used starts the fellowship's count over for them all; one
/// further on is only told the wrong line. A step waiting on so many right uses runs only at that
/// count.
/// Behaviour: server.puzzles.an-order-counted-for-the-fellowship
#[test]
fn a_fellows_wrong_use_starts_the_count_over_for_all_and_a_step_may_wait_on_a_count() {
    use yalain_content::item_rows::SequenceNeed;
    let (mut h, p) = temple(SequenceNeed::At(1));
    let (a, one) = player(&mut h, "one", "Elara");
    let (b, two) = player(&mut h, "two", "Tysis");
    fellows(&mut h, &[one, two]);
    use_tile(&mut h, a, one, p[1]);
    let from = use_tile(&mut h, b, two, p[2]);
    assert_eq!(tells(&h, b, from), [WRONG], "further on: told");
    let from = use_tile(&mut h, b, two, WAY_IN);
    assert_eq!(tells(&h, b, from), [STEP_LINE], "at one right use");
    let from = use_tile(&mut h, b, two, p[1]);
    assert_eq!(tells(&h, b, from), [WRONG], "already used: started over");
    let from = use_tile(&mut h, a, one, WAY_IN);
    assert_ne!(
        tells(&h, a, from),
        [STEP_LINE],
        "started over for its fellows too"
    );
    assert_eq!(phase(&h, one, SHORT), 0);
    let from = use_tile(&mut h, a, one, p[1]);
    assert_eq!(tells(&h, a, from), [RIGHT], "from the first again");
}

/// A count of its own is forgotten its row's time after its last right use, done or not: the way in
/// is shut to the fellowship again and the order is judged from its first use.
/// Behaviour: server.puzzles.an-order-counted-for-the-fellowship
#[test]
fn an_orders_own_count_is_forgotten_its_rows_time_after_its_last_right_use() {
    use yalain_content::item_rows::SequenceNeed;
    let (mut h, p) = temple(SequenceNeed::Done);
    let (a, one) = player(&mut h, "one", "Elara");
    for k in [1, 0, 2] {
        use_tile(&mut h, a, one, p[k]);
    }
    run_for(&mut h, FORGETS - 20.0);
    let from = use_tile(&mut h, a, one, p[1]);
    assert!(tells(&h, a, from).is_empty(), "still done");
    run_for(&mut h, 20.0);
    let from = use_tile(&mut h, a, one, WAY_IN);
    assert_ne!(tells(&h, a, from), [STEP_LINE], "forgotten: kept out");
    let from = use_tile(&mut h, a, one, p[1]);
    assert_eq!(tells(&h, a, from), [RIGHT], "judged from the first again");
}

/// A give waiting on an order of its own takes its item only from a player whose count stands
/// done; before, even with one use still to make, the item is handed back.
/// Behaviour: server.puzzles.an-order-counted-for-the-fellowship
#[test]
fn a_give_waiting_on_an_order_of_its_own_takes_its_item_only_once_the_order_is_done() {
    use yalain_content::item_rows::{SequenceGate, SequenceNeed};
    let (mut h, p) = temple(SequenceNeed::Done);
    let row = yalain_content::quests::Give {
        to: B,
        item: Some(GREEN_ORB),
        wants: None,
        gives: Vec::new(),
        counts: Vec::new(),
        effects: Vec::new(),
        text: None,
        effects_quiet: false,
        fx: None,
        requires: Vec::new(),
        placed: "authored for a test".into(),
    };
    h.server.quests.gives.push(row.clone());
    h.server.item_rows.sequence_gives.push((
        row,
        SequenceGate {
            key: "temple".into(),
            need: SequenceNeed::Done,
        },
    ));
    let (a, one) = player(&mut h, "one", "Elara");
    let orb = hold(&mut h, one, GREEN_ORB);
    give(&mut h, a, one, orb, WAY_IN);
    assert!(
        holds(&h, one, orb.0),
        "handed back before the order is begun"
    );
    for k in [1, 0] {
        use_tile(&mut h, a, one, p[k]);
    }
    give(&mut h, a, one, orb, WAY_IN);
    assert!(
        holds(&h, one, orb.0),
        "handed back with a use still to make"
    );
    use_tile(&mut h, a, one, p[2]);
    give(&mut h, a, one, orb, WAY_IN);
    assert!(!holds(&h, one, orb.0), "taken once it is");
}
