//! Chests: a locked chest refuses a plain use and flashes; its key turns and it opens, showing the
//! opener alone what it holds, the key used up; one that only unlocks is then opened by a plain
//! use; closed, it locks again; what it holds is made anew at each opening, what was left cleared;
//! an item is taken out as from a corpse, unless its quest keeps it from the player. Fixture: the test hall with a chest beside the start, a
//! second player watching, chest templates of made-up kinds and a loot profile of two items.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_content::{
    ContentObject, ContentPack, Description, Facts, Kind, LootProfile, Prop, Props,
};
use yalain_testkit::play::server_call;
use yalain_testkit::Harness;
use yalain_world::calls::{self, Node, Value};
use yalain_world::usable::chests;

const CHEST: u64 = 0x4002_9900_0000_0081;
/// A sigil cache (its key opens it) and a small chest (its key only unlocks it).
const CACHE: u32 = 0x4700_0F58;
const SMALL: u32 = 0x4700_0F4B;
const PACKAGE: u32 = 0x0E8B;
const KEY: u32 = 0x4700_0F10;
const KEY_ID: u32 = 0x4000_0044;
const PROFILE: u32 = 0x3F0;
const GEM: u32 = 0x4700_0F20;
const CRYSTAL: u32 = 0x4700_0F21;
const USE: u32 = 0x8216_0016;
const USE_DONE: u32 = 0x8216_0021;
const OPEN_DONE: u32 = 0x820E_005D;
const CLOSE: u32 = 0x820E_000C;
const CLOSE_DONE: u32 = 0x820E_0060;
const DISCRETE_DESTROY: u32 = 0x820E_0058;

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

/// The hall with a chest of a template beside the start, its kind's lock as given, holding a gem or
/// two and always a crystal.
fn hall(template: u32, lockable: bool, opens_on_unlock: bool) -> Harness {
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    h.server.content = ContentPack {
        objects: vec![ContentObject {
            object: Some(CHEST),
            template,
            description: Description {
                package: PACKAGE,
                look: 0x1F00_0F58,
                enter_fx: 0,
                scale: None,
                name: Some((0x1C06_0002, 0x2500_0002)),
                numbers: [0; 6],
                source: "authored for a test".into(),
            },
            cell: 0x0299_0109,
            origin: [60.0, 240.2, -44.6],
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
    h.server.chests.packages.insert(PACKAGE);
    let mut chest = Props::default();
    chest
        .set("Lock_Lockable", Prop::Bool(lockable))
        .set("Lock_LockOnClose", Prop::Bool(true))
        .set("Lock_KeyID", Prop::Word(KEY_ID))
        .set("GeneratorProfile", Prop::Word(PROFILE));
    if opens_on_unlock {
        chest.set("Lock_OpenOnUnlock", Prop::Bool(true));
    } else {
        chest.set("Lock_IsLocked", Prop::Bool(true));
    }
    h.server
        .catalog
        .insert(yalain_testkit::test_template(template, PACKAGE, chest));
    let mut key = Props::default();
    key.set("Lock_KeyID", Prop::Word(KEY_ID));
    for (t, p) in [
        (KEY, key),
        (GEM, Props::default()),
        (CRYSTAL, Props::default()),
    ] {
        h.server
            .catalog
            .insert(yalain_testkit::test_template(t, 0x3A1, p));
    }
    h.server
        .catalog
        .profiles
        .insert(PROFILE, "GP_TestChestHoard".into());
    h.server.loot.profiles.push(LootProfile {
        name: "GP_TestChestHoard".into(),
        counts: vec![0.0, 0.5, 0.5],
        items: vec![(GEM, 1)],
        extras: vec![(CRYSTAL, 1.0)],
        placed: "authored for a test".into(),
        confidence: "authored".into(),
    });
    h
}

fn run_for(h: &mut Harness, seconds: f64) {
    let t = h.now.0 + seconds;
    while h.now.0 < t {
        h.step();
    }
}

fn send_use(h: &mut Harness, b: usize, me: u64, item: u64, on: u64, seconds: f64) {
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(USE, &[use_of(item, me, on)]),
    );
    run_for(h, seconds);
}

/// What a client saw of the chest and its contents since `from`, in order.
fn seen(h: &Harness, b: usize, from: usize) -> Vec<String> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::DoFx(e) if e.sender.id == CHEST => Some("flash".into()),
            ServerMessage::DoMode(e) if e.sender.id == CHEST => {
                Some(format!("mode {:X}", e.action.0))
            }
            ServerMessage::UpdateBoolVisual(q) if q.object.id == CHEST => {
                Some(if q.value == 1 { "open" } else { "shut" }.to_string())
            }
            ServerMessage::CreateObject(o) if o.weenie.entity_did == GEM => Some("gem".into()),
            ServerMessage::CreateObject(o) if o.weenie.entity_did == CRYSTAL => {
                Some("crystal".into())
            }
            // A clear: by the chest itself, the item then the chest.
            ServerMessage::VisualObjectCall(c)
                if c.object.id == CHEST && c.call.function.0 == chests::DESTROY_CONTAINED =>
            {
                let v = c
                    .call
                    .args
                    .as_ref()
                    .and_then(|a| calls::values(&a.0).ok())?;
                let item = v.iter().find_map(|x| match x {
                    Value::Long(id) if *id != CHEST => Some(*id),
                    _ => None,
                })?;
                let as_sent = auberean_protocol::script::ScriptArgs::from_values(&[
                    Arg::LongInt(item),
                    Arg::LongInt(CHEST),
                ])
                .ok();
                Some(if c.call.args == as_sent {
                    "cleared".into()
                } else {
                    "cleared, the arguments the other way".into()
                })
            }
            ServerMessage::PlayerCall(c) => {
                let v = c.args.as_ref().and_then(|a| calls::values(&a.0).ok())?;
                match c.function.0 {
                    OPEN_DONE => match v.first() {
                        Some(Value::Int(s)) => Some(format!("opened {s}")),
                        _ => None,
                    },
                    CLOSE_DONE => Some("closed".into()),
                    DISCRETE_DESTROY => Some("key gone".into()),
                    chests::DESTROY_CONTAINED => Some("cleared to the player alone".into()),
                    USE_DONE => {
                        let Some(Value::Graph(g)) = v.first() else {
                            return None;
                        };
                        g.iter().find_map(|(_, n)| match n {
                            Node::Script { slots, .. } => {
                                Some(format!("done {} flags {}", slots[11].1, slots[17].1))
                            }
                            _ => None,
                        })
                    }
                    _ => None,
                }
            }
            _ => None,
        })
        .collect()
}

fn give_key(h: &mut Harness, me: u64) -> u64 {
    let world = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .expect("in")
        .0;
    yalain_world::inventory::give(&mut h.server, &mut h.net, world, KEY, 1);
    h.run(3);
    h.server.players[&world]
        .play
        .items
        .iter()
        .find(|i| i.template == KEY)
        .expect("a key")
        .id
}

fn close(h: &mut Harness, b: usize, seconds: f64) {
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(CLOSE, &[Arg::LongInt(CHEST)]),
    );
    run_for(h, seconds);
}

/// A locked chest refuses a plain use: its opening is answered locked and it flashes, and the use
/// is not answered.
/// Behaviour: server.places.locked-chest-refuses
#[test]
fn a_locked_chest_refuses_a_plain_use_and_flashes() {
    let mut h = hall(CACHE, true, true);
    let (a, me) = h.player_in_world("one", "Elara");
    h.run(10);
    let fa = h.bots[a].inbox.len();
    send_use(&mut h, a, me, CHEST, 0, 0.5);
    assert_eq!(
        seen(&h, a, fa),
        [format!("opened {}", chests::LOCKED), "flash".into()]
    );
}

/// A chest's own key turns, and the chest flashes and plays its opening for everyone; when that has
/// played its opener is told what it holds (made now: a gem or two and the crystal), everyone sees
/// it open, and only the opener is shown the contents; the key is used up and the use answered with
/// the flags of an item used on an object. Closed on request, it plays its closing and locks again.
/// Behaviour: server.places.chest-opens-to-its-key
#[test]
fn a_chests_key_opens_it_its_contents_shown_to_the_opener_alone_and_it_locks_as_it_closes() {
    let mut h = hall(CACHE, true, true);
    let (a, me) = h.player_in_world("one", "Elara");
    let (b, _) = h.player_in_world("two", "Babyfare");
    h.run(10);
    let key = give_key(&mut h, me);
    let (fa, fb) = (h.bots[a].inbox.len(), h.bots[b].inbox.len());
    send_use(&mut h, a, me, key, CHEST, 1.2);
    assert!(seen(&h, a, fa).is_empty(), "the key turns first");
    run_for(&mut h, 0.4);
    assert_eq!(seen(&h, b, fb), ["flash", "mode 41000002"]);
    run_for(&mut h, 4.0);
    let mine = seen(&h, a, fa);
    let gems = mine.iter().filter(|s| *s == "gem").count();
    assert!((1..=2).contains(&gems), "{mine:?}");
    let mut want: Vec<String> = vec![
        "flash".into(),
        "mode 41000002".into(),
        "opened 0".into(),
        "open".into(),
    ];
    want.extend(std::iter::repeat_n("gem".to_string(), gems));
    want.extend([
        "crystal".into(),
        "key gone".into(),
        "done 0 flags 317".into(),
    ]);
    assert_eq!(mine, want);
    assert_eq!(
        seen(&h, b, fb),
        ["flash", "mode 41000002", "open"],
        "no contents for others"
    );
    let fa = h.bots[a].inbox.len();
    close(&mut h, a, 3.0);
    assert_eq!(seen(&h, a, fa), ["mode 41000004"]);
    run_for(&mut h, 1.2);
    assert_eq!(seen(&h, a, fa), ["mode 41000004", "closed", "shut"]);
    let fa = h.bots[a].inbox.len();
    send_use(&mut h, a, me, CHEST, 0, 0.5);
    assert_eq!(
        seen(&h, a, fa),
        [format!("opened {}", chests::LOCKED), "flash".into()],
        "locked again"
    );
}

/// A chest whose key only unlocks it: the key turns, is used up and the use answered, nothing on
/// the chest; a plain use then opens it, unanswered as a use.
/// Behaviour: server.places.chest-unlocked-then-opened
#[test]
fn a_chest_its_key_only_unlocks_opens_to_a_plain_use_after() {
    let mut h = hall(SMALL, true, false);
    let (a, me) = h.player_in_world("one", "Elara");
    h.run(10);
    let key = give_key(&mut h, me);
    let fa = h.bots[a].inbox.len();
    send_use(&mut h, a, me, key, CHEST, 1.6);
    assert_eq!(seen(&h, a, fa), ["key gone", "done 0 flags 317"]);
    let fa = h.bots[a].inbox.len();
    send_use(&mut h, a, me, CHEST, 0, 1.6);
    let opened = seen(&h, a, fa);
    assert!(
        opened.starts_with(&["flash".into(), "mode 41000002".into(), "opened 0".into()]),
        "{opened:?}"
    );
    assert!(
        !opened.iter().any(|s| s.starts_with("done")),
        "no answer to a plain opening"
    );
}

/// What a chest holds is made anew at each opening, what was left in it cleared for everyone who
/// sees it, by the chest itself between its flash and its opening; an item in an open chest is
/// taken out as from a corpse, into the opener's pack.
/// Behaviour: server.places.chest-filled-at-each-opening
#[test]
fn a_chest_is_filled_anew_at_each_opening_and_an_item_is_taken_out_into_the_pack() {
    let mut h = hall(SMALL, false, false);
    let (a, me) = h.player_in_world("one", "Elara");
    h.run(10);
    send_use(&mut h, a, me, CHEST, 0, 1.6);
    let first = h.server.chests.by_id[&CHEST].contents.clone();
    let crystal = *first
        .iter()
        .find(|i| {
            h.server
                .things
                .get(**i)
                .is_some_and(|t| t.template == CRYSTAL)
        })
        .expect("a crystal");
    send_use(&mut h, a, me, crystal, 0, 3.0);
    let world = *h.server.players.keys().next().expect("in");
    assert!(
        h.server.players[&world]
            .play
            .items
            .iter()
            .any(|i| i.id == crystal),
        "in the pack"
    );
    close(&mut h, a, 1.0);
    let fa = h.bots[a].inbox.len();
    send_use(&mut h, a, me, CHEST, 0, 1.6);
    let again = seen(&h, a, fa);
    assert_eq!(
        again.iter().filter(|s| *s == "cleared").count(),
        first.len() - 1,
        "what was left is cleared, not what was taken: {again:?}"
    );
    let at = |what: &str| again.iter().position(|s| s.starts_with(what));
    let (flash, cleared, mode) = (at("flash"), at("cleared"), at("mode"));
    assert!(
        flash.is_some() && flash < cleared && cleared < mode,
        "the flash, the clears, the opening: {again:?}"
    );
    let now = &h.server.chests.by_id[&CHEST].contents;
    assert!(now.iter().all(|i| !first.contains(i)), "made anew");
    assert!(
        first
            .iter()
            .filter(|i| **i != crystal)
            .all(|i| h.server.things.get(*i).is_none()),
        "what was left is gone from the world"
    );
}

/// The client's request to take everything from a container, as the 2017 clients sent it.
fn take_all_request(container: u64, me: u64) -> Arg {
    use yalain_world::graph::{tag, Builder, Slot};
    #[allow(clippy::cast_possible_truncation)]
    let long = |v: u64| {
        [
            Slot::Tagged(tag::LONG, v as u32),
            Slot::Tagged(tag::HIGH, (v >> 32) as u32),
        ]
    };
    let mut slots = vec![
        Slot::Int(0),
        Slot::Int(0),
        Slot::Int(1),
        Slot::Int(0),
        Slot::Int(0),
        Slot::Int(0),
    ];
    slots.extend(long(container));
    slots.extend([Slot::Ref(None), Slot::Int(0), Slot::Int(0), Slot::Ref(None)]);
    slots.extend(long(me));
    let mut g = Builder::new();
    g.object(yalain_world::pack::TAKE_ALL_DESC, &slots);
    Arg::Object(g.graph())
}

/// Taking everything from an open chest brings all it holds into the opener's pack.
/// Behaviour: server.places.chest-taken-all
#[test]
fn everything_in_an_open_chest_is_taken_at_once() {
    let mut h = hall(SMALL, false, false);
    let (a, me) = h.player_in_world("one", "Elara");
    h.run(10);
    send_use(&mut h, a, me, CHEST, 0, 1.6);
    let held = h.server.chests.by_id[&CHEST].contents.clone();
    assert!(!held.is_empty());
    h.bots[a].send(
        true,
        queue::WEENIE,
        &server_call(
            yalain_world::pack::function::TAKE_ALL,
            &[take_all_request(CHEST, me)],
        ),
    );
    run_for(&mut h, 3.0);
    let world = *h.server.players.keys().next().expect("in");
    let pack: Vec<u64> = h.server.players[&world]
        .play
        .items
        .iter()
        .map(|i| i.id)
        .collect();
    assert!(held.iter().all(|i| pack.contains(i)), "all in the pack");
}

/// A chest a quest keeps refuses at once a player whose quest is not at the phase it names: "wrong
/// quest", its flags, nothing opened; a player at that phase opens it.
/// Behaviour: server.places.chest-kept-by-a-quest
#[test]
fn a_chest_a_quest_keeps_refuses_a_player_not_at_its_phase() {
    let mut h = hall(SMALL, false, false);
    let quest = 0x8000_038F;
    let mut chest = Props::default();
    chest
        .set("GeneratorProfile", Prop::Word(PROFILE))
        .set("Usage_RequiredQuest", Prop::Word(quest))
        .set("Usage_RequiredQuestStatus", Prop::Word(0x1000_0002));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(SMALL, PACKAGE, chest));
    let (a, me) = h.player_in_world("one", "Elara");
    h.run(10);
    let fa = h.bots[a].inbox.len();
    send_use(&mut h, a, me, CHEST, 0, 0.5);
    assert_eq!(
        seen(&h, a, fa),
        [format!(
            "done {} flags {}",
            chests::WRONG_QUEST,
            chests::WRONG_QUEST_FLAGS
        )]
    );
    for p in h.server.players.values_mut() {
        p.play.quests.push(yalain_world::state::QuestState {
            quest,
            status: yalain_world::quests::status::UNDERWAY,
            phase: 1,
            started: 0,
            updated: 0,
        });
    }
    let fa = h.bots[a].inbox.len();
    send_use(&mut h, a, me, CHEST, 0, 0.5);
    assert_eq!(
        seen(&h, a, fa),
        [format!(
            "done {} flags {}",
            chests::WRONG_QUEST,
            chests::WRONG_QUEST_FLAGS
        )],
        "under way at another phase"
    );
    for p in h.server.players.values_mut() {
        p.play.quests[0].phase = 2;
    }
    let fa = h.bots[a].inbox.len();
    send_use(&mut h, a, me, CHEST, 0, 2.0);
    assert!(
        seen(&h, a, fa).contains(&"opened 0".to_string()),
        "at its phase it opens"
    );
}

/// An open chest whose opener goes more than six meters from it closes: it plays its closing, the
/// opener is told, and everyone sees it shut.
/// Behaviour: server.places.chest-closes-when-its-opener-walks-away
#[test]
fn an_open_chest_closes_when_its_opener_walks_away() {
    let mut h = hall(SMALL, false, false);
    let (a, me) = h.player_in_world("one", "Elara");
    h.run(10);
    send_use(&mut h, a, me, CHEST, 0, 2.0);
    let world = *h.server.players.keys().next().expect("in");
    let start = h.server.players[&world].position;
    let stamp = h.server.players[&world].teleport_stamp;
    let step = |h: &mut Harness, dx: f32| {
        let to =
            auberean_primitives::Vec3::new(start.origin[0] + dx, start.origin[1], start.origin[2]);
        h.bots[a].send(
            true,
            queue::WEENIE,
            &yalain_testkit::play::report(
                start.cell,
                to,
                0,
                yalain_world::world::IDLE_MOTION,
                stamp,
            ),
        );
        run_for(h, 2.0);
    };
    let fa = h.bots[a].inbox.len();
    step(&mut h, 4.0);
    assert!(seen(&h, a, fa).is_empty(), "still near");
    step(&mut h, 8.0);
    assert_eq!(seen(&h, a, fa), ["mode 41000004", "closed", "shut"]);
}

/// An item in the pack that its quest keeps from the player is used no more than taken: refused, the
/// answer as a take's; when the quest is where it asks, the use goes on to what the item does, here
/// nothing, so refused as unusable.
/// Behaviour: server.inventory.use-refused-by-the-quest-for-taking
#[test]
fn an_item_in_the_pack_its_quest_keeps_is_refused_by_its_use() {
    use yalain_content::quests::{QuestDef, QuestTable};
    use yalain_world::item_use::refusal;
    use yalain_world::quests::status;
    use yalain_world::state::QuestState;
    const QUEST: u32 = 0x8000_037B;
    const PIECE: u32 = 0x4700_7B09;
    let mut h = hall(SMALL, false, false);
    let mut piece = Props::default();
    piece
        .set("Usage_RequiredQuest_ForTaking", Prop::Word(QUEST))
        .set(
            "Usage_RequiredQuestStatus_ForTaking",
            Prop::Word(status::UNDERWAY | 2),
        );
    h.server
        .catalog
        .insert(yalain_testkit::test_template(PIECE, 0x200, piece));
    let def = |hidden: bool| QuestDef {
        timed_phase: 0,
        id: QUEST,
        name: (0xA860_7CEC, 0x2500_17EE),
        description: (0x6C45_9364, 0x2500_17EE),
        icon: 0,
        play_fx: false,
        journals: Vec::new(),
        challenge: -999,
        duration: 0.0,
        retry: 0.0,
        xp: 0,
        cancellable: false,
        placed: "authored for a test".into(),
        hidden,
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
    };
    h.server.quests = QuestTable {
        quests: vec![def(false)],
        steps: Vec::new(),
        gives: Vec::new(),
    };
    let (a, me) = h.player_in_world("one", "Elara");
    h.run(10);
    let world = *h.server.players.keys().next().expect("in");
    yalain_world::inventory::give(&mut h.server, &mut h.net, world, PIECE, 1);
    h.run(3);
    let item = h.server.players[&world]
        .play
        .items
        .iter()
        .find(|i| i.template == PIECE)
        .expect("given")
        .id;
    let set = |h: &mut Harness, phase: Option<u32>| {
        h.server.players.get_mut(&world).expect("in").play.quests = phase
            .map(|phase| QuestState {
                quest: QUEST,
                status: status::UNDERWAY,
                phase,
                started: 0,
                updated: 0,
            })
            .into_iter()
            .collect();
    };
    let answer = |h: &mut Harness| {
        let from = h.bots[a].inbox.len();
        send_use(h, a, me, item, 0, 3.0);
        seen(h, a, from)
            .into_iter()
            .filter(|s| s.starts_with("done"))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        answer(&mut h),
        [format!("done {} flags 316", refusal::LACKING_QUEST_TAKE)],
        "never had"
    );
    set(&mut h, Some(1));
    assert_eq!(
        answer(&mut h),
        [format!(
            "done {} flags 316",
            refusal::WRONG_QUEST_STATUS_TAKE
        )],
        "at another phase"
    );
    h.server.quests.quests = vec![def(true)];
    assert_eq!(
        answer(&mut h),
        [format!("done {} flags 316", refusal::MYSTERIOUS_TAKE)],
        "a hidden quest"
    );
    set(&mut h, Some(2));
    assert_eq!(
        answer(&mut h),
        [format!(
            "done {} flags 316",
            yalain_world::places::USE_STATUS_IN_USE
        )],
        "where it asks: nothing it does"
    );
}

/// An item its quest keeps from the player is not taken out of a chest: refused at once, the
/// request's flags kept, for a quest the player never had (10022), for one not where the item
/// needs it (10021), for a hidden one either way (10042); with the quest where it needs it, the
/// item is taken, and one that names no status is taken with the quest under way at any phase.
/// Behaviour: server.items.take-kept-by-a-quest
#[test]
fn an_item_its_quest_keeps_is_refused_when_taken_until_the_quest_is_where_it_needs_it() {
    use yalain_content::quests::{QuestDef, QuestTable};
    use yalain_world::item_use::refusal;
    use yalain_world::quests::status;
    use yalain_world::state::QuestState;
    const QUEST: u32 = 0x8000_037B;
    let mut h = hall(SMALL, false, false);
    let mut crystal = Props::default();
    crystal
        .set("Usage_RequiredQuest_ForTaking", Prop::Word(QUEST))
        .set(
            "Usage_RequiredQuestStatus_ForTaking",
            Prop::Word(status::UNDERWAY | 2),
        );
    h.server
        .catalog
        .insert(yalain_testkit::test_template(CRYSTAL, 0x3A1, crystal));
    // A gem that names the quest and no status: the quest under way, at any phase, will do.
    let mut gem = Props::default();
    gem.set("Usage_RequiredQuest_ForTaking", Prop::Word(QUEST));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(GEM, 0x3A1, gem));
    let def = |hidden: bool| QuestDef {
        id: QUEST,
        name: (0xA860_7CEC, 0x2500_17EE),
        description: (0x6C45_9364, 0x2500_17EE),
        icon: 0,
        play_fx: false,
        journals: Vec::new(),
        challenge: -999,
        duration: 0.0,
        timed_phase: 0,
        retry: 0.0,
        xp: 0,
        cancellable: false,
        placed: "authored for a test".into(),
        hidden,
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
    };
    h.server.quests = QuestTable {
        quests: vec![def(false)],
        steps: Vec::new(),
        gives: Vec::new(),
    };
    let (a, me) = h.player_in_world("one", "Elara");
    h.run(10);
    send_use(&mut h, a, me, CHEST, 0, 1.6);
    let crystal = *h.server.chests.by_id[&CHEST]
        .contents
        .iter()
        .find(|i| {
            h.server
                .things
                .get(**i)
                .is_some_and(|t| t.template == CRYSTAL)
        })
        .expect("a crystal");
    let gem = *h.server.chests.by_id[&CHEST]
        .contents
        .iter()
        .find(|i| h.server.things.get(**i).is_some_and(|t| t.template == GEM))
        .expect("a gem");
    let world = *h.server.players.keys().next().expect("in");
    let set = |h: &mut Harness, phase: Option<u32>| {
        h.server.players.get_mut(&world).expect("in").play.quests = phase
            .map(|phase| QuestState {
                quest: QUEST,
                status: status::UNDERWAY,
                phase,
                started: 0,
                updated: 0,
            })
            .into_iter()
            .collect();
    };
    let taken = |h: &Harness, item: u64| {
        h.server.players[&world]
            .play
            .items
            .iter()
            .any(|i| i.id == item)
    };
    let answer_to = |h: &mut Harness, item: u64| {
        let from = h.bots[a].inbox.len();
        send_use(h, a, me, item, 0, 3.0);
        seen(h, a, from)
            .into_iter()
            .filter(|s| s.starts_with("done"))
            .collect::<Vec<_>>()
    };
    let answer = |h: &mut Harness| answer_to(h, crystal);
    assert_eq!(
        answer(&mut h),
        [format!("done {} flags 316", refusal::LACKING_QUEST_TAKE)],
        "never had"
    );
    set(&mut h, Some(1));
    assert_eq!(
        answer(&mut h),
        [format!(
            "done {} flags 316",
            refusal::WRONG_QUEST_STATUS_TAKE
        )],
        "at another phase"
    );
    h.server.quests.quests = vec![def(true)];
    assert_eq!(
        answer(&mut h),
        [format!("done {} flags 316", refusal::MYSTERIOUS_TAKE)],
        "hidden"
    );
    assert!(!taken(&h, crystal), "not taken while refused");
    h.server.quests.quests = vec![def(false)];
    set(&mut h, Some(2));
    assert!(answer(&mut h).is_empty(), "a take is not answered as a use");
    assert!(taken(&h, crystal), "taken with the quest at its phase");
    assert!(answer_to(&mut h, gem).is_empty());
    assert!(
        taken(&h, gem),
        "an item naming no status: taken with the quest under way"
    );
}
