//! Two things the inventory scenarios share: the three-step drag, and the
//! **recorded ground container**.
//!
//! # The drag
//!
//! `Grab`, `Over`, `Drop` and `Release` are [`crate::player`]'s, beside [`crate::Player::Drag`],
//! because the panel scenarios want the same gesture. They are re-exported here so that an
//! inventory scenario can name them from its own adapter; new scenarios name [`crate::Player`]
//! directly.
//!
//! # The recorded chest
//!
//! [`given_recorded_chest`] is the *given* that gets one recorded Holtburg chest open on screen,
//! so that no scenario carries its own JSONL reader, connection-sequence scan, socket-free
//! `ClientNetwork` and per-datagram replay loop to get there.
//!
//! ## Why the chest cannot simply be opened
//!
//! **The shard never volunteers a ground container.** It answers one the client asked for, and
//! the client's own use sets the ground object *locally* without raising the panel -- the panel
//! goes up on `Notice::SetGroundObject`, and the arriving contents message emits that notice only
//! when the arriving `Item_OnViewContents` names the container the client has **already asked
//! about**. So the order is fixed, and it is the whole difficulty: the chest's own object has to
//! have arrived, then the use has to be made, and only then may the recorded answer land.
//!
//! That is why the given splits the recording in two at the answer instead of replaying it
//! straight through. The split point is **found**, by [`first_blob_where`], and not written down
//! as a number: a hand-copied index is a fact about the recording no reader of the scenario can
//! check, and it goes stale silently.
//!
//! ## Blobs, not datagrams
//!
//! This given delivers **reassembled blobs**, through [`crate::Inbound::from_corpus`], which is
//! the reader the shop fixture in this subject uses too. Nothing these scenarios claim turns on
//! reassembly, ordering or the connection sweep -- they are about what a drop on an open chest
//! sends -- and the corpus reader runs no frame per datagram, which keeps the given cheap.
//! [`crate::Inbound::from_raw_capture`] stays the right step for a scenario whose subject **is**
//! a lost fragment or a silence.

use dereth_client_contract::UiRequest;
use dereth_client_net::client_session::testing::{Corpus, Direction};
use dereth_client_net::client_session::SessionEvent;
use dereth_primitives::ObjectId;
use dereth_protocol::Message;

use crate::client::HeadlessClient;
use crate::inbound::{event_of, Inbound};
use crate::player::Player;

pub use crate::player::Player::{Drop, Grab, Over, Release};

/// The recording the chest is in.
pub const CHEST_SESSION: &str = "long-solo-play";

/// The Holtburg chest the recording opens, and empty.
pub const RECORDED_CHEST: ObjectId = ObjectId(0x77F0_3037);

/// The **locked** Holtburg chest of the same recording. It is never opened -- a locked container refuses the use.
pub const RECORDED_LOCKED_CHEST: ObjectId = ObjectId(0x77F0_3053);

/// The player the recording is of. A corpus replay carries no login, so nothing else sets this.
pub const RECORDED_PLAYER: ObjectId = ObjectId(0x5000_000A);

/// The external-container page, and `<ENVP>`, its host.
pub const EXTERNAL_PAGE: dereth_ui::ElementId = dereth_ui::ElementId(0x1000_005D);
pub const EXTERNAL_HOST: dereth_ui::ElementId = dereth_ui::ElementId(0x1000_05FD);

/// The index of the first server-to-client blob of `session` whose decoded event satisfies `f`.
///
/// This is how a scenario names a moment in a recording without writing down a number. The
/// events are [`crate::inbound::event_of`]'s, so what `f` sees is exactly what
/// [`crate::Inbound::from_corpus`] would deliver.
///
/// # Panics
/// Panics when the recording is absent or does not parse.
#[must_use]
pub fn first_blob_where(session: &str, mut f: impl FnMut(&SessionEvent) -> bool) -> Option<usize> {
    let corpus = Corpus::load(session)
        .unwrap_or_else(|e| panic!("the recording {session} does not parse: {e}"))
        .unwrap_or_else(|| {
            panic!(
                "the decoded corpus has no scenario {session}; it is generated from the \
                 committed recordings and a missing one is a broken checkout"
            )
        });
    corpus
        .blobs
        .iter()
        .filter(|b| b.dir == Direction::ServerToClient)
        .find(|b| f(&event_of(b)))
        .map(|b| b.idx)
}

/// Whether `e` is the shard listing `container`'s contents.
#[must_use]
pub fn is_view_contents_for(e: &SessionEvent, container: ObjectId) -> bool {
    let Some((opcode, body)) = e.ui_body() else {
        return false;
    };
    if opcode != dereth_protocol::Opcode::ITEM_ON_VIEW_CONTENTS {
        return false;
    }
    let mut r = dereth_protocol::archive::Reader::new(body);
    dereth_protocol::objects::ItemOnViewContents::read(&mut r)
        .is_ok_and(|m| m.container == container)
}

/// Whether `e` is an appraisal of `object` that carries the lock block.
///
/// The recording answers the locked chest **twice** and only the first carries it; the second
/// arrives after the key and drops the whole block. A scenario about the lock has to stop at the
/// first, which is what this predicate is for.
#[must_use]
pub fn is_locked_appraisal_of(e: &SessionEvent, object: ObjectId) -> bool {
    let Some((opcode, body)) = e.ui_body() else {
        return false;
    };
    if opcode != dereth_protocol::Opcode::ITEM_SET_APPRAISE_INFO {
        return false;
    }
    let mut r = dereth_protocol::archive::Reader::new(body);
    dereth_protocol::objects::ItemSetAppraiseInfo::read(&mut r).is_ok_and(|m| {
        m.object == object
            && m.profile
                .tables
                .bools
                .as_ref()
                .is_some_and(|t| t.entries.iter().any(|(k, _)| *k == LOCKED_BOOL))
    })
}

/// Boolean property `3`, the locked flag.
pub const LOCKED_BOOL: u32 = 3;

/// Leave the recorded Holtburg chest open on screen, with its own item list bound. Answers the
/// blob index the recording has been replayed to, so that a scenario wanting more of it can say
/// [`crate::Inbound::from_corpus`]`(CHEST_SESSION, at..)` and not replay what it already has.
///
/// The client must be an `Assets::Retail` one with the gameplay screen up
/// ([`crate::ClientSpec::gameplay`]).
///
/// # Panics
/// Panics when the recording carries no answer for the chest, and when the page it leaves is not
/// on screen -- either of which would leave a scenario gesturing at a window that is not there.
pub fn given_recorded_chest(client: &mut HeadlessClient) -> usize {
    let at = first_blob_where(CHEST_SESSION, |e| is_view_contents_for(e, RECORDED_CHEST))
        .unwrap_or_else(|| {
            panic!(
                "{CHEST_SESSION} carries no Item_OnViewContents for {RECORDED_CHEST:?}; that \
                 answer is this given's oracle"
            )
        });

    // The identity is chosen before the description arrives, and a corpus replay carries no
    // login step to carry it across. The shop fixture in this subject does the same.
    client.world_mut().player = Some(RECORDED_PLAYER);
    client
        .when(Inbound::from_corpus(CHEST_SESSION, 0..at))
        .tick(2);

    assert!(
        client
            .app_mut()
            .probe_mut()
            .objects_mut()
            .world
            .weenie(RECORDED_CHEST)
            .is_some_and(|w| w.pwd.bitfield & dereth_rules::weenie::bitfield::OPENABLE != 0),
        "the premise: the recording has described {RECORDED_CHEST:?} as an openable container \
         before it answers about its contents"
    );

    // The client's own use entry point sets the
    // requested ground object; the recorded answer that follows is what raises the panel.
    client
        .when(Player::Ui(vec![UiRequest::Use(RECORDED_CHEST)]))
        .tick(2);
    client
        .when(Inbound::from_corpus(CHEST_SESSION, at..at + 1))
        .tick(2);

    assert_open(client);
    at + 1
}

/// The premise every scenario built on this given rests on, asserted once here rather than eight
/// times over.
fn assert_open(client: &mut HeadlessClient) {
    assert_eq!(
        client
            .app_mut()
            .hud()
            .panels
            .external_container
            .ground_object,
        Some(RECORDED_CHEST),
        "the recorded answer opens the external container on the chest"
    );
    let snap = client.ui_snapshot();
    snap.assert_visible(EXTERNAL_PAGE);
    snap.assert_visible(EXTERNAL_HOST);
    assert!(
        client
            .app_mut()
            .hud()
            .panels
            .external_container
            .item_list
            .is_some(),
        "the chest's own item list is bound, or there is nothing to drop anything on"
    );
}

// ---------------------------------------------------------------------------------------------
// The equips the recordings contain.
// ---------------------------------------------------------------------------------------------

/// One equip a recording's own client asked for: the thing, and the whole list of places that
/// thing says it could go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordedEquip {
    /// The thing the recorded client asked to put on.
    pub item: ObjectId,
    /// That thing's own list of the places it could go, as the shard described it.
    pub places: u32,
}

/// The two recordings that carry a real equip.
pub const EQUIP_SESSIONS: [&str; 2] = ["early-inventory-and-casting", "long-solo-play"];

/// Every equip `session`'s own client asked for, joined to the thing's own list of places.
///
/// **The join is the point.** That list is the sole input to the gate a drop on the figure makes,
/// so a client whose gate is wrong fails against this without any mask being written into a
/// scenario. Each ask is required to be answered by the shard's own "it is worn" as well, so a
/// recording that stopped carrying equips reads as a broken checkout and not as a pass over
/// nothing.
///
/// # Panics
/// Panics when the recording is absent or does not parse, when it records no equip, when the
/// asks and the answers do not balance, and when the recording never described a thing its own
/// client asked to put on.
#[must_use]
pub fn recorded_equips(session: &str) -> Vec<RecordedEquip> {
    use std::collections::BTreeMap;

    /// `Inventory_GetAndWieldItem`.
    const ASK: u32 = 0x001A;
    /// `Item_WearItem`, the shard's own answer to one.
    const ANSWER: u32 = 0x0023;
    /// `Item_CreateObject`, which is where a thing's list of places comes from.
    const DESCRIBED: u32 = 0xF745;
    /// `Ordered_GameEvent`.
    const EVENT: u32 = 0xF7B0;

    let corpus = Corpus::load(session)
        .unwrap_or_else(|e| panic!("the recording {session} does not parse: {e}"))
        .unwrap_or_else(|| {
            panic!(
                "the decoded corpus has no scenario {session}; it is generated from the \
                 committed recordings and a missing one is a broken checkout"
            )
        });

    let dword = |b: &[u8], o: usize| -> u32 {
        b.get(o..o + 4)
            .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
            .unwrap_or_default()
    };

    let mut places: BTreeMap<ObjectId, u32> = BTreeMap::new();
    let mut answers = 0_usize;
    for b in corpus
        .blobs
        .iter()
        .filter(|b| b.dir == Direction::ServerToClient)
    {
        if b.opcode == DESCRIBED {
            if let Ok(m) = dereth_protocol::read_body_padded::<
                dereth_protocol::objects::ItemCreateObject,
            >(b.payload.get(4..).unwrap_or_default())
            {
                if let Some(v) = m.0.wdesc.valid_locations {
                    places.insert(m.0.id, v);
                }
            }
        } else if b.opcode == EVENT && b.payload.len() >= 24 && dword(&b.payload, 12) == ANSWER {
            answers += 1;
        }
    }

    let asks: Vec<ObjectId> = crate::outbound::Outbound::all(session)
        .into_iter()
        .filter(|s| s.message == ASK)
        .map(|s| ObjectId(s.field(0).unwrap_or_default()))
        .collect();
    assert!(!asks.is_empty(), "{session} records no equip at all");
    assert_eq!(
        asks.len(),
        answers,
        "{session}: every equip the recorded client asked for is answered by the shard, and \
         these do not balance -- the reader is reading the wrong thing"
    );

    asks.into_iter()
        .map(|item| RecordedEquip {
            item,
            places: *places.get(&item).unwrap_or_else(|| {
                panic!(
                    "{session}: the recording never described {item:?}, which its own client \
                     asked to put on"
                )
            }),
        })
        .collect()
}

/// [`recorded_equips`] over every recording that carries one.
///
/// # Panics
/// As [`recorded_equips`].
#[must_use]
pub fn all_recorded_equips() -> Vec<RecordedEquip> {
    EQUIP_SESSIONS
        .iter()
        .flat_map(|s| recorded_equips(s))
        .collect()
}
