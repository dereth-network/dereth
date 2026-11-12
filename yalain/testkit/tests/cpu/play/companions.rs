//! Companion quests: a quest a world row says goes with another's holder. As the holder's quest is
//! done, the companions under way of the holder and of its fellows in the world are done; as it
//! fails or is cancelled, they fail; a player outside the fellowship keeps its own. Fixture: a
//! bearer's quest that fails on its player's death, done by using a person beside the start, and
//! its companion; three players, two of them fellows.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use auberean_script::native_object::QuestInfo;
use auberean_script::wire::In;
use yalain_content::quests::{QuestDef, QuestTable, Step, Trigger, Update};
use yalain_content::{ContentObject, ContentPack, Description, Facts, Kind, Words};
use yalain_net::ConnId;
use yalain_testkit::play::{server_call, use_of};
use yalain_testkit::Harness;
use yalain_world::calls::{Node, Value};
use yalain_world::fellowship::{Fellow, Fellowship, Vitals};
use yalain_world::quests::{self, status, update};
use yalain_world::state::QuestState;

const USE: u32 = 0x8216_0016;
const BEARER: u32 = 0x4000_005C;
const COMPANION: u32 = 0x4000_005D;
const TOTEM: u64 = 0x8130_0000_0000_0A01;
const TOTEM_T: u32 = 0x4700_2F42;

fn quest(id: u32, table: u32) -> QuestDef {
    QuestDef {
        id,
        name: Words::Ref(0xA860_7CEC, table),
        description: (0x6C45_9364, table),
        icon: 0x4100_034B,
        play_fx: true,
        journals: vec![(0x2E5C_1E52, table)],
        challenge: 22,
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
        once: false,
        fellows: yalain_content::quests::Fellows::default(),
        last_waits: false,
        hero_only: false,
        pay: yalain_content::quests::CompletionPay::default(),
        required: None,
        cancel_wait: None,
    }
}

fn step(quest: u32, needs: u32, trigger: Trigger, update: Update) -> Step {
    Step {
        key: String::new(),
        quest,
        needs,
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
        takes: Vec::new(),
        races: Vec::new(),
        effects_last: false,
        effects_quiet: false,
        wants: None,
        fellows: false,
        to_fellows: false,
        levels: None,
        xp_share: 0.0,
        to: None,
        transforms: None,
        chance: None,
    }
}

fn totem() -> ContentObject {
    ContentObject {
        object: Some(TOTEM),
        template: TOTEM_T,
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
        origin: [61.0, 241.0, -44.6],
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

/// The world: the bearer's quest, failing on its player's death and done at the totem; its
/// companion; the bearer and a fellow each holding the companion under way, and a stranger
/// holding it too. Returns the harness and each player's bot and world connection: the bearer,
/// the fellow, the stranger.
fn world() -> (Harness, [(usize, ConnId); 3]) {
    let mut h = Harness::new();
    h.server.content = ContentPack {
        objects: vec![totem()],
        ..ContentPack::default()
    };
    h.server.content.companions.insert(BEARER, vec![COMPANION]);
    h.server.quests = QuestTable {
        quests: vec![
            QuestDef {
                fails_on_death: true,
                ..quest(BEARER, 0x2500_144B)
            },
            quest(COMPANION, 0x2500_144A),
        ],
        steps: vec![step(BEARER, 1, Trigger::Use(TOTEM_T), Update::Complete)],
        gives: Vec::new(),
        rooms: Vec::new(),
    };
    let (a, me) = h.player_in_world("one", "Elara");
    let (b, you) = h.player_in_world("two", "Bran");
    let (c, _them) = h.player_in_world("three", "Cade");
    h.run(10);
    h.server.fellowships.list.push(Fellowship {
        name: "Wardens".into(),
        leader: me,
        flags: 0,
        room: 1,
        fellows: [me, you]
            .iter()
            .map(|&id| Fellow {
                id,
                joined: 0.0,
                sent: Vitals::default(),
                next_check: f64::INFINITY,
            })
            .collect(),
        last_claimant: 0,
    });
    let worlds: Vec<ConnId> = ["Elara", "Bran", "Cade"]
        .iter()
        .map(|name| {
            *h.server
                .players
                .iter()
                .find(|(_, p)| p.name == *name)
                .expect("in the world")
                .0
        })
        .collect();
    for (i, w) in worlds.iter().enumerate() {
        let p = h.server.players.get_mut(w).expect("in");
        let held = |quest| QuestState {
            quest,
            status: status::UNDERWAY,
            phase: 1,
            started: 0,
            updated: 0,
        };
        p.play.quests.push(held(COMPANION));
        if i == 0 {
            p.play.quests.push(held(BEARER));
        }
    }
    (h, [(a, worlds[0]), (b, worlds[1]), (c, worlds[2])])
}

/// The quest updates a client received since `from`: each one's kind, quest and status.
fn updates(h: &Harness, b: usize, from: usize) -> Vec<(u32, u32, u32)> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c) if c.function.0 == quests::UPDATE_QUEST => {
                let v = c
                    .args
                    .as_ref()
                    .and_then(|a| yalain_world::calls::values(&a.0).ok())?;
                let record = match v.get(1)?.root()? {
                    Node::Native { bytes, .. } => QuestInfo::unpack(&mut In::new(bytes)).ok()?,
                    _ => return None,
                };
                match v.last() {
                    Some(Value::Int(k)) => Some((*k, record.id, record.status)),
                    _ => None,
                }
            }
            _ => None,
        })
        .collect()
}

fn status_of(h: &Harness, w: ConnId, quest: u32) -> u32 {
    h.server.players[&w]
        .play
        .quests
        .iter()
        .find(|q| q.quest == quest)
        .expect("held")
        .status
}

/// The bearer's quest failing as its player dies fails the companion after it, for the bearer
/// and its fellow, each told; the stranger's is kept, and nothing moves the bearer's quest when a
/// companion's holder dies.
///
/// Behaviour: server.quests.companions-end-with-their-bearers
#[test]
fn a_bearers_death_fails_its_quests_companion_for_it_and_its_fellows() {
    let (mut h, [(a, bearer), (b, fellow), (c, stranger)]) = world();
    // A companion's holder dying moves nothing.
    let clock = h.clock();
    yalain_world::death::player_dies(&mut h.server, &mut h.net, fellow, None, clock);
    h.run(5);
    assert_eq!(status_of(&h, fellow, COMPANION), status::UNDERWAY);
    assert_eq!(status_of(&h, bearer, BEARER), status::UNDERWAY);
    let from = [a, b, c].map(|x| h.bots[x].inbox.len());
    let clock = h.clock();
    yalain_world::death::player_dies(&mut h.server, &mut h.net, bearer, None, clock);
    h.run(5);
    assert_eq!(
        updates(&h, a, from[0]),
        vec![
            (update::FAIL, BEARER, status::FAILED),
            (update::FAIL, COMPANION, status::FAILED)
        ],
        "the bearer's own, then its companion"
    );
    assert_eq!(
        updates(&h, b, from[1]),
        vec![(update::FAIL, COMPANION, status::FAILED)],
        "the fellow's"
    );
    assert!(updates(&h, c, from[2]).is_empty(), "not the stranger's");
    assert_eq!(status_of(&h, stranger, COMPANION), status::UNDERWAY);
}

/// The bearer's quest done (its step at the totem) completes the companion after it, for the
/// bearer and its fellow; the stranger's is kept.
///
/// Behaviour: server.quests.companions-end-with-their-bearers
#[test]
fn a_bearers_quest_done_completes_its_companion_for_it_and_its_fellows() {
    let (mut h, [(a, bearer), (b, fellow), (c, stranger)]) = world();
    let me = h.server.players[&bearer].character;
    let from = [a, b, c].map(|x| h.bots[x].inbox.len());
    h.bots[a].send(true, queue::WEENIE, &server_call(USE, &[use_of(TOTEM, me)]));
    h.run(20);
    assert_eq!(
        updates(&h, a, from[0]),
        vec![
            (update::COMPLETE, BEARER, status::COMPLETED),
            (update::COMPLETE, COMPANION, status::COMPLETED)
        ]
    );
    assert_eq!(
        updates(&h, b, from[1]),
        vec![(update::COMPLETE, COMPANION, status::COMPLETED)]
    );
    assert!(updates(&h, c, from[2]).is_empty());
    assert_eq!(status_of(&h, fellow, COMPANION), status::COMPLETED);
    assert_eq!(status_of(&h, stranger, COMPANION), status::UNDERWAY);
}

/// The bearer cancelling its quest fails the companion for it and its fellow; a companion
/// cancelled moves nothing else.
///
/// Behaviour: server.quests.companions-end-with-their-bearers
#[test]
fn a_bearers_quest_cancelled_fails_its_companion_for_it_and_its_fellows() {
    let (mut h, [(a, bearer), (b, fellow), (c, stranger)]) = world();
    let cancel = |q: u32| [Arg::Int(q)];
    h.ask(c, quests::CANCEL_QUEST, &cancel(COMPANION), 3);
    assert_eq!(status_of(&h, stranger, COMPANION), status::CANCELLED);
    assert_eq!(status_of(&h, bearer, BEARER), status::UNDERWAY);
    assert_eq!(status_of(&h, fellow, COMPANION), status::UNDERWAY);
    let from = [a, b].map(|x| h.bots[x].inbox.len());
    h.ask(a, quests::CANCEL_QUEST, &cancel(BEARER), 3);
    assert_eq!(
        updates(&h, a, from[0]),
        vec![
            (update::CANCEL, BEARER, status::CANCELLED),
            (update::FAIL, COMPANION, status::FAILED)
        ]
    );
    assert_eq!(
        updates(&h, b, from[1]),
        vec![(update::FAIL, COMPANION, status::FAILED)]
    );
}
