//! Quests that fail when their player leaves the world or goes through portal space: a trip
//! through portal space (a portal used, a recall, an effect's sending) fails each such quest once
//! the player is in portal space, its companions with it, and nothing else; a dead player's return
//! fails none; leaving the world fails each such quest as the character goes, its fellows'
//! companions with it, and one the server never saw leave fails as the character next enters.
//! Fixture: a portal beside the start; a quest failing on portals, one failing on leaving, one on
//! death only, and a companion of each of the first two; two players in a fellowship.

use auberean_protocol::login::CharacterRef;
use auberean_protocol::{queue, ClientMessage, ServerMessage};
use auberean_script::native_object::QuestInfo;
use auberean_script::wire::In;
use yalain_content::quests::{QuestDef, QuestTable};
use yalain_content::{ContentObject, ContentPack, Description, Facts, Kind, Words};
use yalain_net::ConnId;
use yalain_store::StoredPosition;
use yalain_testkit::play::{server_call, use_of};
use yalain_testkit::Harness;
use yalain_world::calls::{Node, Value};
use yalain_world::fellowship::{Fellow, Fellowship, Vitals};
use yalain_world::quests::{self, status, update};
use yalain_world::state::QuestState;

const USE: u32 = 0x8216_0016;
const ENTER_PORTAL_SPACE: u32 = 0x8212_0020;
const PORTAL: u64 = 0x4002_9900_0000_0056;
/// Fails on portal space (and death, as the install's own such quests do).
const BURDEN: u32 = 0x4000_005C;
/// Fails as its player leaves the world (and on death).
const ERRAND: u32 = 0x4000_01FA;
/// Fails on death only.
const OPAL: u32 = 0x4000_005B;
/// The companions of the first two.
const GUARD: u32 = 0x4000_005D;
const WARD: u32 = 0x4000_0257;
const THERE: u32 = 0x7000_0001;

fn quest(id: u32) -> QuestDef {
    QuestDef {
        id,
        name: Words::Ref(0xA860_7CEC, 0x2500_144B),
        description: (0x6C45_9364, 0x2500_144B),
        icon: 0x4100_034B,
        play_fx: true,
        journals: vec![(0x2E5C_1E52, 0x2500_144B)],
        challenge: 20,
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

fn portal() -> ContentObject {
    ContentObject {
        object: Some(PORTAL),
        template: 0x4700_1154,
        description: Description {
            name_text: None,
            package: 0x103E,
            look: 0x1F00_0100,
            enter_fx: 0,
            scale: None,
            name: Some((0xA860_7CEC, 0x2500_28B9)),
            numbers: [0; 6],
            source: "authored for a test".into(),
        },
        cell: 0x0299_0109,
        origin: [60.0, 241.0, -44.6],
        rotation: [1.0, 0.0, 0.0, 0.0],
        kind: Kind::Static,
        label: String::new(),
        placed: "authored for a test".into(),
        destination: Some(yalain_content::Place {
            cell: THERE,
            origin: [10.0, 10.0, 50.0],
            rotation: [1.0, 0.0, 0.0, 0.0],
        }),
        facts: Facts::default(),
        walks: None,
        gate: None,
    }
}

/// The world: the portal, the quests and their companions; two players in a fellowship, the
/// first holding the three quests and both companions, the second both companions, all under way.
/// Returns the harness and each player's bot, id and world connection.
fn world() -> (Harness, [(usize, u64, ConnId); 2]) {
    let mut h = Harness::new();
    h.server.content = ContentPack {
        objects: vec![portal()],
        ..ContentPack::default()
    };
    h.server.content.companions.insert(BURDEN, vec![GUARD]);
    h.server.content.companions.insert(ERRAND, vec![WARD]);
    h.server.quests = QuestTable {
        quests: vec![
            QuestDef {
                fails_on_death: true,
                fails_on_teleport: true,
                ..quest(BURDEN)
            },
            QuestDef {
                fails_on_death: true,
                fails_on_logout: true,
                ..quest(ERRAND)
            },
            QuestDef {
                fails_on_death: true,
                ..quest(OPAL)
            },
            quest(GUARD),
            quest(WARD),
        ],
        steps: Vec::new(),
        gives: Vec::new(),
        rooms: Vec::new(),
    };
    let (a, me) = h.player_in_world("one", "Elara");
    let (b, you) = h.player_in_world("two", "Bran");
    for x in [a, b] {
        h.bots[x].send(
            true,
            queue::WEENIE,
            &ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
        );
    }
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
    let held = |quest| QuestState {
        quest,
        status: status::UNDERWAY,
        phase: 1,
        started: 0,
        updated: 0,
    };
    let (wa, wb) = (conn_of(&h, me), conn_of(&h, you));
    let p = h.server.players.get_mut(&wa).expect("in");
    p.play
        .quests
        .extend([BURDEN, ERRAND, OPAL, GUARD, WARD].map(held));
    let p = h.server.players.get_mut(&wb).expect("in");
    p.play.quests.extend([GUARD, WARD].map(held));
    (h, [(a, me, wa), (b, you, wb)])
}

fn conn_of(h: &Harness, me: u64) -> ConnId {
    *h.server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .expect("in the world")
        .0
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

/// Each quest a player holds and its status, in the order held.
fn statuses(h: &Harness, w: ConnId) -> Vec<(u32, u32)> {
    h.server.players[&w]
        .play
        .quests
        .iter()
        .map(|q| (q.quest, q.status))
        .collect()
}

fn away() -> StoredPosition {
    StoredPosition {
        cell: THERE,
        origin: [12.5, 34.5, 2.0],
        rotation: [1.0, 0.0, 0.0, 0.0],
    }
}

/// A player using a portal: once it is told it is in portal space, its quest that fails on portal
/// space fails, then its companion, for it and its fellow; its other quests (one failing on death,
/// one on leaving) are kept, and so are its fellow's other companions.
///
/// Behaviour: server.quests.fails-on-portal-space
#[test]
fn a_quest_that_fails_on_portal_space_fails_as_its_player_goes_through_a_portal() {
    let (mut h, [(a, me, wa), (b, _, wb)]) = world();
    let from = [a, b].map(|x| h.bots[x].inbox.len());
    h.bots[a].send(
        true,
        queue::WEENIE,
        &server_call(USE, &[use_of(PORTAL, me)]),
    );
    h.run(60);
    assert_eq!(h.server.players[&wa].position.cell, THERE, "went through");
    let calls: Vec<u32> = h
        .calls(a, from[0])
        .into_iter()
        .map(|(f, _)| f)
        .filter(|f| *f == ENTER_PORTAL_SPACE || *f == quests::UPDATE_QUEST)
        .collect();
    assert_eq!(
        calls,
        [
            ENTER_PORTAL_SPACE,
            quests::UPDATE_QUEST,
            quests::UPDATE_QUEST
        ],
        "into portal space, then the failures"
    );
    assert_eq!(
        updates(&h, a, from[0]),
        [
            (update::FAIL, BURDEN, status::FAILED),
            (update::FAIL, GUARD, status::FAILED)
        ]
    );
    assert_eq!(
        updates(&h, b, from[1]),
        [(update::FAIL, GUARD, status::FAILED)],
        "the fellow's companion"
    );
    assert_eq!(
        statuses(&h, wa),
        [
            (BURDEN, status::FAILED),
            (ERRAND, status::UNDERWAY),
            (OPAL, status::UNDERWAY),
            (GUARD, status::FAILED),
            (WARD, status::UNDERWAY)
        ]
    );
    assert_eq!(
        statuses(&h, wb),
        [(GUARD, status::FAILED), (WARD, status::UNDERWAY)]
    );
}

/// A recall and an effect's sending are trips through portal space too: each fails the quest; a
/// dead player's return to life does not.
///
/// Behaviour: server.quests.fails-on-portal-space
#[test]
fn a_recall_or_a_sending_fails_it_too_and_a_return_to_life_does_not() {
    let failed = |h: &Harness, w| {
        statuses(h, w)
            .into_iter()
            .find(|(q, _)| *q == BURDEN)
            .map(|(_, st)| st)
    };
    let (mut h, [(_, _, wa), _]) = world();
    let clock = h.clock();
    yalain_world::places::recall_trip(&mut h.server, &mut h.net, wa, away(), None, clock);
    assert_eq!(failed(&h, wa), Some(status::FAILED), "a recall");
    let (mut h, [(_, _, wa), _]) = world();
    let clock = h.clock();
    yalain_world::places::sent_away(&mut h.server, &mut h.net, wa, away(), clock);
    assert_eq!(failed(&h, wa), Some(status::FAILED), "a sending");
    let (mut h, [(_, _, wa), _]) = world();
    h.server.quests.quests[0].fails_on_death = false;
    let clock = h.clock();
    yalain_world::places::resurrection_trip(&mut h.server, &mut h.net, wa, away(), clock);
    h.run(10);
    assert_eq!(failed(&h, wa), Some(status::UNDERWAY), "a return to life");
}

/// A player logging off: as it leaves the world its quest that fails on leaving fails, and its
/// fellow's companion of it; its quest failing on portal space is kept through the logging off's
/// portal space, and entering again it holds them so.
///
/// Behaviour: server.quests.fails-on-leaving
#[test]
fn a_quest_that_fails_on_leaving_fails_as_its_player_logs_off() {
    let (mut h, [(a, me, _), (b, _, wb)]) = world();
    let from = [a, b].map(|x| h.bots[x].inbox.len());
    h.bots[a].send(
        false,
        queue::LOGON,
        &ClientMessage::LogOff(CharacterRef { character: me }),
    );
    assert!(h.run_until(400, |h| h.bots[a].inbox[from[0]..]
        .iter()
        .any(|r| matches!(r.message(), Some(ServerMessage::CharacterList(_))))));
    assert_eq!(
        updates(&h, a, from[0]),
        [
            (update::FAIL, ERRAND, status::FAILED),
            (update::FAIL, WARD, status::FAILED)
        ]
    );
    assert_eq!(
        statuses(&h, wb),
        [(GUARD, status::UNDERWAY), (WARD, status::FAILED)],
        "the fellow's companion, as its bearer left"
    );
    h.enter_world(a, "one", me);
    let wa = conn_of(&h, me);
    assert_eq!(
        statuses(&h, wa),
        [
            (BURDEN, status::UNDERWAY),
            (ERRAND, status::FAILED),
            (OPAL, status::UNDERWAY),
            (GUARD, status::UNDERWAY),
            (WARD, status::FAILED)
        ]
    );
}

/// A player whose client went away: as its character goes out of the world its quest that fails on
/// leaving fails, and its fellow's companion of it, while the rest are kept.
///
/// Behaviour: server.quests.fails-on-leaving
#[test]
fn a_quest_that_fails_on_leaving_fails_as_its_players_client_goes_away() {
    let (mut h, [(a, me, wa), (_, _, wb)]) = world();
    let clock = h.clock();
    assert!(yalain_world::world::dropped(
        &mut h.server,
        &mut h.net,
        wa,
        clock
    ));
    assert!(h.run_until(400, |h| !h.server.players.contains_key(&wa)));
    assert_eq!(
        statuses(&h, wb),
        [(GUARD, status::UNDERWAY), (WARD, status::FAILED)],
        "the fellow's companion, as its bearer left"
    );
    h.enter_world(a, "one", me);
    let wa = conn_of(&h, me);
    assert_eq!(
        statuses(&h, wa),
        [
            (BURDEN, status::UNDERWAY),
            (ERRAND, status::FAILED),
            (OPAL, status::UNDERWAY),
            (GUARD, status::UNDERWAY),
            (WARD, status::FAILED)
        ]
    );
}

/// A quest that fails on leaving, still under way as its character enters (the server stopped
/// before it saw the character go), fails as of when the character was last kept; others are kept.
///
/// Behaviour: server.quests.fails-on-leaving
#[test]
fn one_still_under_way_at_entry_fails_as_of_when_its_character_was_last_kept() {
    let table = QuestTable {
        quests: vec![
            QuestDef {
                fails_on_logout: true,
                ..quest(ERRAND)
            },
            QuestDef {
                fails_on_teleport: true,
                fails_on_death: true,
                ..quest(BURDEN)
            },
        ],
        steps: Vec::new(),
        gives: Vec::new(),
        rooms: Vec::new(),
    };
    let at = |quest, status| QuestState {
        quest,
        status,
        phase: 1,
        started: 1000,
        updated: 1000,
    };
    let mut held = [
        at(ERRAND, status::UNDERWAY),
        at(BURDEN, status::UNDERWAY),
        at(ERRAND, status::COMPLETED),
    ];
    quests::failed_on_leaving(&table, &mut held, 9_000);
    assert_eq!(
        held.map(|q| (q.status, q.phase, q.updated)),
        [
            (status::FAILED, 0, 9_000),
            (status::UNDERWAY, 1, 1000),
            (status::COMPLETED, 1, 1000)
        ]
    );
}

/// The server stopping keeps a character as it stands, its quest that fails on leaving still under
/// way; the next run fails it as the character enters, its other quests kept.
///
/// Behaviour: server.quests.fails-on-leaving
#[test]
fn a_quest_still_under_way_after_the_server_stopped_fails_as_its_character_enters() {
    let (mut h, [(_, me, wa), _]) = world();
    yalain_world::world::keep_player(&mut h.server, wa);
    yalain_world::shutdown::save_everyone(&mut h.server, &mut h.net);
    let quests = h.server.quests.clone();
    let store = std::mem::take(&mut h.server.store);
    let mut h = Harness::with(
        yalain_world::ServerConfig::default(),
        store,
        yalain_testkit::test_rules(),
    );
    h.server.quests = quests;
    let b = h.at_character_list("one");
    h.enter_world(b, "one", me);
    let wa = conn_of(&h, me);
    assert_eq!(
        statuses(&h, wa),
        [
            (BURDEN, status::UNDERWAY),
            (ERRAND, status::FAILED),
            (OPAL, status::UNDERWAY),
            (GUARD, status::UNDERWAY),
            (WARD, status::UNDERWAY)
        ]
    );
}
