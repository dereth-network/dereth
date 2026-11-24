//! Wanting objects: each placed object a world's row names wants one item of the row's set, drawn
//! at random the first time an item of the set comes to it and kept, the same for every player,
//! until a use or a give of it runs a quest step; another item of the set is refused and kept, and
//! an item outside the set is not judged. Fixture: the test hall beside where a character starts,
//! two made-up crystals of one template, three serums and an item outside the set, and a quest each
//! serum used on or given to a crystal completes.

use auberean_protocol::queue;
use auberean_script::args::Arg;
use yalain_content::quests::{QuestDef, QuestTable, Step, Trigger, Update};
use yalain_content::wanting::{WantTable, Wanting};
use yalain_content::{ContentObject, ContentPack, Description, Facts, Kind, Props};
use yalain_testkit::play::server_call;
use yalain_testkit::Harness;
use yalain_world::calls::{Node, Value};
use yalain_world::gives::{GIVE, HANDED_AFTER};
use yalain_world::graph::{Builder, Slot};
use yalain_world::inventory::{move_graph, Move};
use yalain_world::quests::{phase_of, status};
use yalain_world::state::QuestState;

const HEAR_TELL: u32 = 0x8205_0017;
const USE: u32 = 0x8216_0016;
const CRYSTALS: [u64; 2] = [0x8130_0000_0000_0E91, 0x8130_0000_0000_0E92];
const CRYSTAL_T: u32 = 0x4700_2F94;
const SERUMS: [u32; 3] = [0x4700_7E91, 0x4700_7E92, 0x4700_7E93];
/// An item outside the set, whose use on a crystal moves the quest too.
const OTHER: u32 = 0x4700_7E94;
const QUEST: u32 = 0x4000_02B1;
/// What a crystal says to a serum it does not want.
const REFUSED: (u32, u32) = (0xC844_788B, 0x2500_28F0);

fn crystal(id: u64, origin: [f32; 2]) -> ContentObject {
    ContentObject {
        object: Some(id),
        template: CRYSTAL_T,
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

fn quest() -> QuestDef {
    QuestDef {
        duration_by_calendar: false,
        id: QUEST,
        name: yalain_content::Words::Ref(0xA860_7CEC, 0x2500_1F0B),
        description: (0x6C45_9364, 0x2500_1F0B),
        icon: 0x4100_034B,
        play_fx: true,
        journals: vec![(0x2E5C_1E52, 0x2500_1F0B), (0x2E5C_1E53, 0x2500_1F0B)],
        challenge: -999,
        duration: 0.0,
        timed_phase: 0,
        retry: 0.0,
        xp: 0,
        cancellable: true,
        placed: "authored for a test".into(),
        hidden: false,
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

fn step(trigger: Trigger, update: Update, takes: Vec<u32>) -> Step {
    Step {
        out_of_turn: false,
        skills: Vec::new(),
        key: String::new(),
        quest: QUEST,
        needs: 1,
        trigger,
        tell: None,
        update,
        xp: 0,
        gives: Vec::new(),
        counts: Vec::new(),
        effects: Vec::new(),
        text: None,
        recipes: Vec::new(),
        placed: "authored for a test".into(),
        requires: Vec::new(),
        takes,
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

/// The hall: the two crystals (places 0 and 1) wanting by one row with these weights, and the
/// quest's steps: each serum used on a crystal or given to one completes it, taken; the item
/// outside the set used on one moves it to its second phase.
fn hall(weights: [u32; 3]) -> Harness {
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    for t in SERUMS.into_iter().chain([OTHER]) {
        h.server
            .catalog
            .insert(yalain_testkit::test_template(t, 0x1067, Props::default()));
    }
    h.server.content = ContentPack {
        objects: vec![
            crystal(CRYSTALS[0], [58.0, 241.0]),
            crystal(CRYSTALS[1], [59.0, 241.0]),
        ],
        wanting: WantTable {
            rows: vec![Wanting {
                key: "crystals".into(),
                objects: vec![0, 1],
                items: SERUMS.into_iter().zip(weights).collect(),
                refused: Some(REFUSED),
            }],
        },
        ..ContentPack::default()
    };
    let mut steps: Vec<Step> = SERUMS
        .iter()
        .flat_map(|&s| {
            [
                step(Trigger::UseOn(s, CRYSTAL_T), Update::Complete, vec![s]),
                step(Trigger::Give(s, CRYSTAL_T), Update::Complete, vec![s]),
            ]
        })
        .collect();
    steps.push(step(
        Trigger::UseOn(OTHER, CRYSTAL_T),
        Update::Advance(2),
        Vec::new(),
    ));
    h.server.quests = QuestTable {
        quests: vec![quest()],
        steps,
        gives: Vec::new(),
        rooms: Vec::new(),
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

fn world_of(h: &Harness, me: u64) -> yalain_net::ConnId {
    *h.server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .expect("in the world")
        .0
}

/// Put a player's quest at its first phase, under way.
fn under_way(h: &mut Harness, me: u64) {
    let world = world_of(h, me);
    let p = h.server.players.get_mut(&world).expect("in the world");
    p.play.quests.retain(|q| q.quest != QUEST);
    p.play.quests.push(QuestState {
        quest: QUEST,
        status: status::UNDERWAY,
        phase: 1,
        started: 0,
        updated: 0,
    });
}

/// The quest's phase and status for a player, if held.
fn quest_of(h: &Harness, me: u64) -> Option<(u32, u32)> {
    let p = &h.server.players[&world_of(h, me)];
    p.play
        .quests
        .iter()
        .find(|q| q.quest == QUEST)
        .map(|q| (q.status, phase_of(&p.play.quests, QUEST)))
}

fn completed(h: &Harness, me: u64) -> bool {
    quest_of(h, me).is_some_and(|(s, _)| s == status::COMPLETED)
}

/// An item put in a player's pack; the item and its slot.
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

fn holds(h: &Harness, me: u64, item: u64) -> bool {
    h.server.players[&world_of(h, me)]
        .play
        .items
        .iter()
        .any(|x| x.id == item)
}

/// A use of a held item on an object, as the client asks for it.
fn use_on(item: u64, target: u64, player: u64) -> Arg {
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

/// A player uses an item of a template, put in its pack, on a crystal; the item, and where its
/// client's messages from the use begin.
fn used_on(h: &mut Harness, b: usize, me: u64, template: u32, crystal: usize) -> (u64, usize) {
    let (item, _) = hold(h, me, template);
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(USE, &[use_on(item, CRYSTALS[crystal], me)]),
    );
    h.run(40);
    (item, from)
}

/// A player gives an item of a template, put in its pack, to a crystal, handed over and answered;
/// the item, and where its client's messages from the give begin.
fn given_to(h: &mut Harness, b: usize, me: u64, template: u32, crystal: usize) -> (u64, usize) {
    let (item, slot) = hold(h, me, template);
    let target = CRYSTALS[crystal];
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
        item,
        split_item: item,
        quantity: 1,
        quantity_left: 1,
        from_slot: slot,
        actual_from_slot: slot,
        ..Move::default()
    };
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(yalain_world::drops::function::MOVE_ITEM, &[move_graph(&m)]),
    );
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    h.run((HANDED_AFTER / yalain_testkit::STEP).ceil() as usize + 8);
    (item, from)
}

/// The lines the client was told since `from`.
fn told(h: &Harness, b: usize, from: usize) -> Vec<u32> {
    h.calls(b, from)
        .into_iter()
        .filter(|(f, _)| *f == HEAR_TELL)
        .filter_map(|(_, v)| {
            v.iter().find_map(|x: &Value| match x.root() {
                Some(Node::TextInfo(t)) => Some(t.string_id),
                _ => None,
            })
        })
        .collect()
}

fn wanted(h: &Harness, crystal: usize) -> Option<u32> {
    yalain_world::wanting::wanted(&h.server, crystal)
}

/// The first serum used on a crystal draws its want. Every other serum of the set is refused: the
/// crystal says its line, the serum stays in the pack and the quest does not move. The one it wants
/// runs the step, taken, and the want is met and forgotten. An item outside the set is not judged.
/// Behaviour: server.places.an-object-wants-one-item-drawn-at-random
#[test]
fn a_crystal_takes_only_the_serum_it_drew_and_refuses_the_others_which_stay_in_the_pack() {
    let mut h = hall([1, 1, 1]);
    let (b, me) = player(&mut h, "one", "Elara");
    let (c, you) = player(&mut h, "two", "Bran");
    under_way(&mut h, me);
    assert_eq!(wanted(&h, 0), None, "nothing drawn before a serum comes");
    // Bran holds no quest: his serum draws the want and moves nothing.
    used_on(&mut h, c, you, SERUMS[0], 0);
    let want = wanted(&h, 0).expect("drawn as the first serum came");
    assert!(SERUMS.contains(&want), "one of the set");

    for serum in SERUMS.into_iter().filter(|s| *s != want) {
        let (item, from) = used_on(&mut h, b, me, serum, 0);
        assert!(holds(&h, me, item), "{serum:08X} refused and kept");
        assert_eq!(told(&h, b, from), [REFUSED.0]);
        assert_eq!(quest_of(&h, me), Some((status::UNDERWAY, 1)));
        assert_eq!(wanted(&h, 0), Some(want), "the want kept");
    }
    // Outside the set, an item is not judged: its step runs, and the want stays as it was.
    let (_, from) = used_on(&mut h, b, me, OTHER, 0);
    assert_eq!(
        quest_of(&h, me),
        Some((status::UNDERWAY, 2)),
        "its step ran"
    );
    assert!(told(&h, b, from).is_empty());
    assert_eq!(wanted(&h, 0), Some(want));

    under_way(&mut h, me);
    let (item, from) = used_on(&mut h, b, me, want, 0);
    assert!(completed(&h, me), "the serum it wants runs the step");
    assert!(!holds(&h, me, item), "taken");
    assert!(told(&h, b, from).is_empty(), "not refused");
    assert_eq!(wanted(&h, 0), None, "met and forgotten");
}

/// A crystal's want is its own and the same for every player: another player's serum it does not
/// want is refused too, and the one it wants used by a player whose quest it does not move leaves
/// the want as it is. Meeting one crystal's want leaves the other's. Each met want is drawn afresh
/// by the row's weights: over many draws of an even set every serum comes up, and a set weighted
/// a billion to one comes up the heavy one.
/// Behaviour: server.places.an-object-wants-one-item-drawn-at-random
#[test]
fn a_crystals_want_is_the_same_for_every_player_kept_until_met_and_drawn_afresh_by_weight() {
    let mut h = hall([1, 1, 1]);
    let (b, me) = player(&mut h, "one", "Elara");
    let (c, you) = player(&mut h, "two", "Bran");
    under_way(&mut h, me);

    // Bran holds no quest: his serums move nothing, whatever the crystal wants.
    used_on(&mut h, c, you, SERUMS[0], 0);
    let want = wanted(&h, 0).expect("drawn as Bran's serum came");
    let other = *SERUMS.iter().find(|s| **s != want).expect("another");
    let (item, from) = used_on(&mut h, c, you, other, 0);
    assert!(holds(&h, you, item), "refused for Bran too");
    assert_eq!(told(&h, c, from), [REFUSED.0]);
    used_on(&mut h, c, you, want, 0);
    assert_eq!(
        wanted(&h, 0),
        Some(want),
        "a use that runs nothing leaves it"
    );

    used_on(&mut h, c, you, SERUMS[0], 1);
    let second = wanted(&h, 1).expect("the second crystal's own want");
    let (_, from) = used_on(&mut h, b, me, other, 0);
    assert_eq!(told(&h, b, from), [REFUSED.0], "Elara refused alike");
    used_on(&mut h, b, me, want, 0);
    assert!(completed(&h, me), "met by Elara");
    assert_eq!(wanted(&h, 0), None, "the first crystal's want met");
    assert_eq!(wanted(&h, 1), Some(second), "the second's kept");

    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..60 {
        under_way(&mut h, me);
        used_on(&mut h, b, me, SERUMS[0], 0);
        let want = wanted(&h, 0).unwrap_or(SERUMS[0]);
        seen.insert(want);
        if wanted(&h, 0).is_some() {
            used_on(&mut h, b, me, want, 0);
        }
        assert!(completed(&h, me), "met");
        assert_eq!(wanted(&h, 0), None);
    }
    assert_eq!(seen.len(), 3, "every serum drawn in time: {seen:08X?}");

    let mut h = hall([1, 1, 1_000_000_000]);
    let (b, me) = player(&mut h, "one", "Elara");
    for _ in 0..20 {
        under_way(&mut h, me);
        used_on(&mut h, b, me, SERUMS[2], 0);
        assert!(completed(&h, me), "the heavy serum drawn");
    }
}

/// Given in place of used, a serum the crystal does not want is handed back and moves nothing; the
/// one it wants runs the give's step, kept by the crystal, and the want is met; a give row taking it
/// meets it alike.
/// Behaviour: server.places.an-object-wants-one-item-drawn-at-random
#[test]
fn a_serum_given_to_a_crystal_that_wants_another_is_handed_back_and_the_one_it_wants_is_taken() {
    let mut h = hall([1, 1, 1]);
    let (b, me) = player(&mut h, "one", "Elara");
    under_way(&mut h, me);
    // Draw the want first, by a use that runs nothing for a player without the quest.
    let (c, you) = player(&mut h, "two", "Bran");
    used_on(&mut h, c, you, SERUMS[0], 0);
    let want = wanted(&h, 0).expect("drawn");

    for serum in SERUMS.into_iter().filter(|s| *s != want) {
        let (item, from) = given_to(&mut h, b, me, serum, 0);
        assert!(holds(&h, me, item), "{serum:08X} handed back");
        assert_eq!(told(&h, b, from), [REFUSED.0], "the crystal says its line");
        assert_eq!(
            quest_of(&h, me),
            Some((status::UNDERWAY, 1)),
            "nothing moved"
        );
        assert_eq!(wanted(&h, 0), Some(want));
    }
    let (item, _) = given_to(&mut h, b, me, want, 0);
    assert!(completed(&h, me), "the give's step ran");
    assert!(!holds(&h, me, item), "kept by the crystal");
    assert_eq!(wanted(&h, 0), None, "met and forgotten");

    // A give row that takes the serum, moving no quest, meets the want as a step does.
    for serum in SERUMS {
        h.server.quests.gives.push(yalain_content::quests::Give {
            to: CRYSTAL_T,
            item: Some(serum),
            wants: None,
            gives: Vec::new(),
            counts: Vec::new(),
            effects: Vec::new(),
            text: None,
            effects_quiet: false,
            fx: None,
            requires: Vec::new(),
            placed: "authored for a test".into(),
        });
    }
    used_on(&mut h, c, you, SERUMS[0], 0);
    let want = wanted(&h, 0).expect("drawn afresh");
    let other = *SERUMS.iter().find(|s| **s != want).expect("another");
    let (item, _) = given_to(&mut h, c, you, other, 0);
    assert!(holds(&h, you, item), "handed back before the give row");
    let (item, _) = given_to(&mut h, c, you, want, 0);
    assert!(!holds(&h, you, item), "taken by the give row");
    assert_eq!(wanted(&h, 0), None, "met by the give row");
}

/// A serum whose interaction table names the crystal is judged alike, its table carried out only
/// for the serum the crystal wants: the others are refused and kept, the one it wants runs its step.
/// Behaviour: server.places.an-object-wants-one-item-drawn-at-random
#[test]
fn a_serum_whose_table_names_the_crystal_is_refused_alike_unless_it_is_the_one_wanted() {
    use yalain_content::Prop;
    use yalain_world::usable::interactions::{self, Interaction};
    const TABLE: u32 = 0x6F00_7E91;
    let mut h = hall([1, 1, 1]);
    let mut props = Props::default();
    props.set(interactions::TABLE, Prop::Word(TABLE));
    for t in SERUMS {
        h.server
            .catalog
            .insert(yalain_testkit::test_template(t, 0x1067, props.clone()));
    }
    h.server.interactions.tables.insert(
        TABLE,
        vec![Interaction {
            targets: vec![CRYSTAL_T],
            reach: 8.0,
            behavior: None,
            message: None,
            uses_up: false,
            uses_up_target: false,
            gate: None,
        }],
    );
    let (b, me) = player(&mut h, "one", "Elara");
    let (c, you) = player(&mut h, "two", "Bran");
    under_way(&mut h, me);
    used_on(&mut h, c, you, SERUMS[0], 0);
    let want = wanted(&h, 0).expect("drawn");

    for serum in SERUMS.into_iter().filter(|s| *s != want) {
        let (item, from) = used_on(&mut h, b, me, serum, 0);
        assert!(holds(&h, me, item), "{serum:08X} refused and kept");
        assert_eq!(told(&h, b, from), [REFUSED.0]);
        assert_eq!(quest_of(&h, me), Some((status::UNDERWAY, 1)));
    }
    let (item, _) = used_on(&mut h, b, me, want, 0);
    assert!(completed(&h, me), "the serum it wants runs the step");
    assert!(!holds(&h, me, item), "taken");
    assert_eq!(wanted(&h, 0), None, "met and forgotten");
}
