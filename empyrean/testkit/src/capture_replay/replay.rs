//! The closed-loop replay of one recording against a [`TestServer`] on real content.
//!
//! **Server.** A `TestServer` on the retail dats and `world.pack` (the world content is installed
//! before `Program.Main`'s start-up steps, as a real server has it). One synthetic account holds
//! every recorded character; its access level is the one the recording's PlayerDescriptions show
//! (an Admin weenie type is an Admin account under ACE's default `OverrideCharacterPermissions`).
//! Characters the recording logs in to without creating them are reconstructed
//! ([`super::character`]) and seeded under their recorded guids before the server starts.
//!
//! **Client.** One `TestClient` logs in (its own handshake); then every message the recorded client
//! sent is sent again, in order, on its recorded queue:
//! - *timing*: each keeps its recorded gap from the last synchronisation point; gaps at the
//!   character screen (character list, creation, deletion, restore, entering the world, the first
//!   message) are shortened to 1 s, since nothing in the world waits on them;
//! - *closed loop*: a message that answers a server message waits (up to [`WAIT_S`]) until our
//!   server has sent that message as many times as ACE had: the DDD answer waits for the DDD
//!   interrogation, the enter-world request for the character list, the enter-world for the
//!   server-ready, LoginComplete for the PlayerCreate or teleport, a confirmation answer for its
//!   confirmation request. A message naming an object waits (up to [`WAIT_S`]) until our server
//!   has created a matching object;
//! - *translation*: the account name (enter world, character creation and deletion) and the
//!   character name (creation) become synthetic; every recorded guid in a message becomes ours:
//!   characters by their creation response (or their seeded guid), static objects as they are, and
//!   dynamic objects by matching their CreateObject: the same weenie, in the matched container,
//!   else the nearest one.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::time::Instant;

use dereth_primitives::NetQueue;
use dereth_primitives::ObjectId;
use dereth_protocol as proto;
use dereth_protocol::login::{
    CharGenVerificationResponse, CharacterSendCharGenResult, LoginCharacterSet,
    LoginPlayerDescription, LoginSendEnterWorld,
};
use dereth_protocol::objects::{ItemCreateObject, ObjectCreatePayload};
use empyrean_content::PackContent;
use empyrean_dat::{DatManager, RealDats};
use empyrean_entity::enums::AccessLevel;
use empyrean_world::managers::guid_manager::{self, ShardGuidQueries};
use empyrean_world::managers::world_manager::WorldStatusState;

use super::character::{self, Reconstructed};
use super::recording::{self, Msg, Recording};
use super::steer::{self, Steering};
use super::wire::{self, u32_at};
use crate::{ClientId, TestServer};

/// The synthetic account every replay logs in as.
pub const ACCOUNT: &str = "capturereplay";
const PASSWORD: &str = "pw";

/// The longest closed-loop wait for an answer, in virtual seconds.
pub const WAIT_S: f64 = 10.0;
/// The longest closed-loop wait for a named object, in virtual seconds.
pub const OBJECT_WAIT_S: f64 = 3.0;
/// The closed-loop wait for a named object in a phase whose roll outcomes already differ from
/// the recording's: the object is then most likely one our rolls never produced (a corpse
/// ACE's kill left), and every full wait puts the replay further behind the recording.
pub const RNG_OBJECT_WAIT_S: f64 = 0.5;
/// A character-screen gap is shortened to this.
const SCREEN_GAP_S: f64 = 1.0;
/// Added to the recorded tail: our step granularity (a few 1/60 s steps).
const TAIL_GRACE_S: f64 = 0.25;
/// How long the replay runs after the last recorded client message (at most).
const TAIL_S: f64 = 30.0;

/// Synthetic character names, handed out in order.
const NAMES: [&str; 12] = [
    "Replay Alpha",
    "Replay Bravo",
    "Replay Charlie",
    "Replay Delta",
    "Replay Echo",
    "Replay Foxtrot",
    "Replay Golf",
    "Replay Hotel",
    "Replay India",
    "Replay Juliet",
    "Replay Kilo",
    "Replay Lima",
];

/// What one replay produced.
#[derive(Debug, Clone)]
pub struct ReplayOutcome {
    pub name: String,
    /// The recording, in send order.
    pub recorded: Recording,
    /// Our stream: the client messages we sent and the server messages we received, in send order.
    pub ours: Vec<Msg>,
    /// The `not_ported!` sites hit in each phase (phase `k` runs from client message `k - 1` to
    /// client message `k`; phase 0 is before the first).
    pub not_ported: Vec<BTreeMap<&'static str, u64>>,
    /// Per client message: what the replay had to do or could not do (waits that timed out,
    /// guids it could not translate), as short structural notes.
    pub notes: Vec<(usize, String)>,
    /// Reconstructed characters (count) and possessions left out for want of a weenie.
    pub reconstructed: usize,
    pub missing_weenies: usize,
    /// Set when our server panicked; the replay stopped there.
    pub crashed: Option<String>,
    /// The recorded monster deaths anchored in our world ([`steer`]): (phase, our monster).
    pub steered: Vec<(usize, u32)>,
    /// The recorded monster deaths not anchored: (phase, monster, why).
    pub not_steered: Vec<(usize, u32, &'static str)>,
    /// Object waits cut to [`RNG_OBJECT_WAIT_S`] (in a phase whose roll outcomes differ) that
    /// found no match.
    pub capped_waits: usize,
    /// Virtual seconds replayed and wall seconds taken.
    pub virtual_s: f64,
    pub wall_s: f64,
}

fn dats() -> Arc<DatManager> {
    use std::sync::OnceLock;
    static DATS: OnceLock<Arc<DatManager>> = OnceLock::new();
    Arc::clone(DATS.get_or_init(|| {
        let dir = dereth_dat::testing::dat_dir();
        let source = RealDats::open(&dir).unwrap_or_else(|e| {
            panic!(
                "the capture tier needs the retail dats under {} (set DERETH_TEST_DAT_DIR): {e}",
                dir.display()
            )
        });
        DatManager::initialize(Arc::new(source)).expect("retail dats")
    }))
}

fn pack() -> Arc<PackContent> {
    let path = empyrean_common::test_paths::world_pack();
    Arc::new(PackContent::open(&path).unwrap_or_else(|e| {
        panic!(
            "the capture tier needs world.pack at {} (set EMPYREAN_TEST_WORLD_PACK): {e}",
            path.display()
        )
    }))
}

/// The guid allocators' view of the seeded shard: the highest seeded guid in a range.
struct SeededGuids(Vec<u32>);

impl ShardGuidQueries for SeededGuids {
    fn get_max_guid_found_in_range(&mut self, min: u32, max: u32) -> u32 {
        self.0
            .iter()
            .copied()
            .filter(|g| (min..=max).contains(g))
            .max()
            .unwrap_or(u32::MAX)
    }
    fn get_sequence_gaps(&mut self, _min: u32, _limit: u32) -> Vec<(u32, u32)> {
        Vec::new()
    }
}

/// The seam queue of a recorded queue id.
fn net_queue(id: u16) -> NetQueue {
    match id {
        2 => NetQueue::Control,
        3 => NetQueue::Weenie,
        4 => NetQueue::Logon,
        5 => NetQueue::ClientCache,
        9 => NetQueue::UiQueue,
        10 => NetQueue::WorldObjects,
        other => NetQueue::Other(u8::try_from(other).unwrap_or(0)),
    }
}

fn queue_id(q: NetQueue) -> u16 {
    match q {
        NetQueue::Control => 2,
        NetQueue::Weenie => 3,
        NetQueue::Logon => 4,
        NetQueue::ClientCache => 5,
        NetQueue::UiQueue => 9,
        NetQueue::WorldObjects => 10,
        NetQueue::Other(n) => u16::from(n),
    }
}

fn decode_create(p: &[u8]) -> Option<ObjectCreatePayload> {
    proto::read_body_padded::<ItemCreateObject>(p.get(4..)?)
        .ok()
        .map(|c| c.0)
}

/// A vendor's item list (`Vendor_VendorInfo`, event 0x0062): each item the client may name in a
/// Buy, placed in the vendor with its weenie. The vendor's items are never created on the client
/// (the list carries their descriptions), so this is how the client learns their guids.
fn vendor_items(p: &[u8]) -> Vec<(u32, Placed)> {
    if wire::event_type(p) != Some(0x0062) {
        return Vec::new();
    }
    let Some(v) = p
        .get(16..)
        .and_then(|b| proto::read_body_padded::<dereth_protocol::trade::VendorInfo>(b).ok())
    else {
        return Vec::new();
    };
    v.items
        .iter()
        .filter_map(|i| {
            i.pwd.as_ref().map(|d| {
                (
                    i.iid.0,
                    Placed {
                        wcid: d.wcid,
                        container: Some(v.merchant_id.0),
                        at: None,
                    },
                )
            })
        })
        .collect()
}

/// A CreateObject's weenie class, item type and description flags (numbers only; the dump's object
/// class breakdown).
#[must_use]
pub fn create_class(p: &[u8]) -> Option<(u32, u32, u32)> {
    let c = decode_create(p)?;
    Some((c.wdesc.wcid, c.wdesc.obj_type, c.wdesc.bitfield))
}

/// A CreateObject's (object, container or wielder), when it has one.
#[must_use]
pub fn create_owner(p: &[u8]) -> Option<(u32, u32)> {
    let c = decode_create(p)?;
    Some((c.id.0, c.wdesc.container_id.or(c.wdesc.wielder_id)?.0))
}

/// Where a create put its object: weenie, container, and global position (landblock grid metres).
#[derive(Debug, Clone, Copy)]
struct Placed {
    wcid: u32,
    container: Option<u32>,
    at: Option<(f64, f64, f64)>,
}

impl Placed {
    fn of(c: &ObjectCreatePayload) -> Self {
        let at = c.physicsdesc.position.as_ref().map(|p| {
            let (lbx, lby) = (
                f64::from((p.objcell_id >> 24) & 0xFF),
                f64::from((p.objcell_id >> 16) & 0xFF),
            );
            (
                lbx * 192.0 + f64::from(p.frame.origin.x),
                lby * 192.0 + f64::from(p.frame.origin.y),
                f64::from(p.frame.origin.z),
            )
        });
        Self {
            wcid: c.wdesc.wcid,
            container: c.wdesc.container_id.or(c.wdesc.wielder_id).map(|o| o.0),
            at,
        }
    }
}

/// Recorded guids to ours.
#[derive(Debug, Default)]
struct GuidMap {
    fwd: HashMap<u32, u32>,
    taken: HashSet<u32>,
    /// The seeded possessions: the same guid on both sides, so never another object's match.
    seeded: HashSet<u32>,
}

impl GuidMap {
    fn bind(&mut self, rec: u32, ours: u32) {
        self.fwd.insert(rec, ours);
        self.taken.insert(ours);
    }

    /// Ours for a recorded guid, matching a dynamic object's create when it is not bound yet.
    fn resolve(
        &mut self,
        g: u32,
        rec: &HashMap<u32, Placed>,
        ours: &[(u32, Placed)],
    ) -> Option<u32> {
        if let Some(&m) = self.fwd.get(&g) {
            return Some(m);
        }
        let want = rec.get(&g)?;
        // the same guid with the same weenie (seeded possessions, static objects)
        if let Some((o, _)) = ours
            .iter()
            .rev()
            .find(|(o, p)| *o == g && p.wcid == want.wcid)
        {
            let o = *o;
            self.bind(g, o);
            return Some(o);
        }
        if !wire::is_dynamic_guid(g) {
            // a static object is the same world-database instance on both servers; a character
            // the replay did not bind is another player's
            return (!wire::is_player_guid(g)).then_some(g);
        }
        // a container translates as the object it is: a bound one to ours, a static one (a vendor,
        // a chest of the world database) to itself
        let container = want.container.and_then(|c| {
            self.fwd
                .get(&c)
                .copied()
                .or_else(|| (!wire::is_dynamic_guid(c) && !wire::is_player_guid(c)).then_some(c))
        });
        // not a seeded possession: that one is its own match (the rule above), found when it is named
        let candidates = ours.iter().filter(|(o, p)| {
            p.wcid == want.wcid && !self.taken.contains(o) && !self.seeded.contains(o)
        });
        let best = if let Some(c) = container {
            candidates
                .filter(|(_, p)| p.container == Some(c))
                .map(|(o, _)| *o)
                .next()
        } else if let Some((x, y, z)) = want.at {
            candidates
                .filter_map(|(o, p)| {
                    p.at.map(|(a, b, cz)| {
                        (*o, (a - x).powi(2) + (b - y).powi(2) + (cz - z).powi(2))
                    })
                })
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(o, _)| o)
        } else {
            candidates.map(|(o, _)| *o).next()
        };
        if let Some(o) = best {
            self.bind(g, o);
        }
        best
    }
}

/// The recorded client message `k` translated into what our client sends, or `Err` with the
/// recorded guids it could not translate.
fn rewrite_string16(p: &[u8], account: &str) -> Option<Vec<u8>> {
    // opcode, String16L account (u16 length, bytes, padded to 4 from the length field), the rest
    let len = usize::from(wire::u16_at(p, 4)?);
    let end = 4 + (2 + len).div_ceil(4) * 4;
    let rest = p.get(end..)?;
    let mut out = p[..4].to_vec();
    let a = account.as_bytes();
    out.extend_from_slice(&u16::try_from(a.len()).ok()?.to_le_bytes());
    out.extend_from_slice(a);
    while !(out.len() - 4).is_multiple_of(4) {
        out.push(0);
    }
    out.extend_from_slice(rest);
    Some(out)
}

/// The recorded state the replay walks alongside: what the recorded client had been told up to
/// the message being replayed.
#[derive(Debug, Default)]
struct RecordedView {
    creates: HashMap<u32, Placed>,
    /// Every guid the recorded client could name: created objects and the recorded characters.
    known: HashSet<u32>,
    /// The recorded labels sent so far, counted.
    counts: HashMap<String, usize>,
    /// The last character list's characters, in slot order.
    slots: Vec<u32>,
    /// The monsters created (dynamic, not players, ItemType Creature): steering's candidates.
    creatures: HashSet<u32>,
    /// The roll outcomes of the current phase (see [`super::compare::roll_outcome`]).
    rolls: [usize; super::compare::ROLL_KINDS],
}

/// Our client's view: our creates, label counts, and the answers to our character creations.
#[derive(Debug, Default)]
struct OurView {
    creates: Vec<(u32, Placed)>,
    counts: HashMap<String, usize>,
    /// One entry per character creation we sent: the created guid, or `None` when refused.
    chargen: Vec<Option<u32>>,
    /// Character creations sent and not answered yet.
    chargen_pending: usize,
    /// The last character list's characters, in slot order.
    slots: Vec<u32>,
    /// The roll outcomes of the current phase (see [`super::compare::roll_outcome`]).
    rolls: [usize; super::compare::ROLL_KINDS],
}

/// A character list's characters (guids), in slot order.
fn character_slots(p: &[u8]) -> Option<Vec<u32>> {
    let list = proto::read_body_padded::<LoginCharacterSet>(p.get(4..)?).ok()?;
    Some(list.characters.iter().map(|c| c.gid.0).collect())
}

/// The first answer to a character creation: `Some(guid)` when accepted. `None` for anything else
/// (a refusal; ACE's restore answer shares the opcode and is not paired with a creation).
fn chargen_answer(p: &[u8]) -> Option<Option<u32>> {
    let r = proto::read_body_padded::<CharGenVerificationResponse>(p.get(4..)?).ok()?;
    Some((r.response_type == 1).then_some(r.identity.gid.0))
}

/// Recorded character names to synthetic ones, handed out in order of first appearance, so that a
/// name the recording reuses (a second creation under a taken name) is reused here too. The
/// recorded names are only keys in memory.
#[derive(Debug, Default)]
struct NameMap(HashMap<String, &'static str>);

impl NameMap {
    fn synthetic(&mut self, recorded: &str) -> &'static str {
        let key = recorded.trim_start_matches('+').to_lowercase();
        let n = self.0.len();
        self.0
            .entry(key)
            .or_insert_with(|| NAMES.get(n).copied().unwrap_or("Replay Zulu"))
    }
}

/// The s2c label a client message waits for, as (label, and a second label counted with it).
fn trigger(c2s: &[u8]) -> Option<(&'static str, Option<&'static str>)> {
    let op = u32_at(c2s, 0)?;
    Some(match op {
        0xF7E6 => ("F7E5", None),
        0xF7EA => ("F7EA", None),
        wire::ENTER_WORLD_REQUEST => ("F658", None),
        wire::ENTER_WORLD => ("F7DF", None),
        wire::GAME_ACTION => match wire::action_type(c2s)? {
            wire::ACT_LOGIN_COMPLETE => ("F746", Some("F751")),
            wire::ACT_CONFIRMATION_RESPONSE => ("ev 0274", None),
            _ => return None,
        },
        _ => return None,
    })
}

/// Character-screen messages: their recorded gaps are shortened.
fn is_screen(op: u32) -> bool {
    matches!(
        op,
        wire::ENTER_WORLD_REQUEST | wire::ENTER_WORLD | wire::CHAR_GEN | 0xF655 | 0xF7D9
    )
}

/// The replay's pre-scan of a recording: the account level, the characters to reconstruct, and
/// the recorded character guids in creation order.
struct Plan {
    access: AccessLevel,
    names: NameMap,
    /// The synthetic name of each character in `reconstruct`.
    reconstruct_names: Vec<&'static str>,
    /// Per reconstructed character: its lifestone, where the recording shows one (see
    /// [`recorded_sanctuary`]).
    sanctuaries: HashMap<u32, empyrean_entity::models::PropertiesPosition>,
    /// Per reconstructed character: the weenies of the objects it used that teleported it (see
    /// [`recorded_portal_uses`]).
    portal_uses: HashMap<u32, Vec<u32>>,
    /// (recorded guid, its first login's PlayerDescription, own create, possessions)
    reconstruct: Vec<(
        u32,
        LoginPlayerDescription,
        ObjectCreatePayload,
        Vec<ObjectCreatePayload>,
    )>,
}

fn plan(rec: &Recording) -> Plan {
    let msgs = &rec.msgs;
    let mut access = AccessLevel::Player;
    for m in msgs.iter().filter(|m| !m.c2s) {
        if wire::event_type(&m.payload) == Some(wire::EV_PLAYER_DESCRIPTION) {
            match u32_at(&m.payload, 20) {
                Some(11) => access = AccessLevel::Admin,
                Some(41) if access != AccessLevel::Admin => access = AccessLevel::Sentinel,
                _ => {}
            }
        }
    }
    // the characters the recording creates
    let created: HashSet<u32> = msgs
        .iter()
        .filter(|m| !m.c2s && m.opcode() == wire::CHAR_GEN_RESPONSE)
        .filter_map(|m| {
            proto::read_body_padded::<CharGenVerificationResponse>(&m.payload[4..]).ok()
        })
        .filter(|r| r.response_type == 1)
        .map(|r| r.identity.gid.0)
        .collect();
    let mut reconstruct = Vec::new();
    let mut names = NameMap::default();
    let mut reconstruct_names = Vec::new();
    let mut done = HashSet::new();
    for (i, m) in msgs.iter().enumerate() {
        if !m.c2s || m.opcode() != wire::ENTER_WORLD {
            continue;
        }
        let Some(g) = u32_at(&m.payload, 4) else {
            continue;
        };
        if created.contains(&g) || !done.insert(g) {
            continue;
        }
        // this login's answer: up to the client's LoginComplete (the client may act before the
        // answer arrives)
        let entry: Vec<&Msg> = msgs[i + 1..]
            .iter()
            .take_while(|m| {
                !(m.c2s && wire::action_type(&m.payload) == Some(wire::ACT_LOGIN_COMPLETE))
            })
            .filter(|m| !m.c2s)
            .collect();
        let pd = entry
            .iter()
            .find(|m| wire::event_type(&m.payload) == Some(wire::EV_PLAYER_DESCRIPTION))
            .and_then(|m| proto::read_body_padded::<LoginPlayerDescription>(&m.payload[16..]).ok());
        let creates: Vec<ObjectCreatePayload> = entry
            .iter()
            .filter(|m| m.opcode() == wire::CREATE_OBJECT)
            .filter_map(|m| decode_create(&m.payload))
            .collect();
        let own = creates.iter().find(|c| c.id.0 == g).cloned();
        let mut owners: HashSet<u32> = HashSet::from([g]);
        let mut possessions = Vec::new();
        for c in &creates {
            if c.id.0 == g {
                continue;
            }
            let owner = c.wdesc.container_id.or(c.wdesc.wielder_id).map(|o| o.0);
            if owner.is_some_and(|o| owners.contains(&o)) {
                owners.insert(c.id.0);
                possessions.push(c.clone());
            }
        }
        if let (Some(pd), Some(own)) = (pd, own) {
            let recorded_name = pd
                .qualities
                .base
                .tables
                .strings
                .iter()
                .flat_map(|h| &h.entries)
                .find(|e| e.0 == 1)
                .map(|e| e.1.clone());
            reconstruct_names.push(names.synthetic(&recorded_name.unwrap_or_default()));
            reconstruct.push((g, pd, own, possessions));
        }
    }
    let sanctuaries = plan_reconstruct_sanctuaries(msgs, &reconstruct);
    let portal_uses = recorded_portal_uses(msgs);
    Plan {
        access,
        names,
        reconstruct_names,
        sanctuaries,
        portal_uses,
        reconstruct,
    }
}

/// A character's lifestone (Sanctuary) is never sent to the client, but a lifestone recall shows
/// it: `Player.HandleActionTeleToLifestone` ends in `Teleport(Sanctuary)`, whose PlayerTeleport is
/// followed by the "fake" UpdatePosition at the destination (`Sanctuary` with its z raised
/// 0.005 per unit of scale). The first recall of each reconstructed character, if any.
fn recorded_sanctuary(
    msgs: &[Msg],
    character: u32,
) -> Option<empyrean_entity::models::PropertiesPosition> {
    let mut playing = None;
    let mut recalled = false;
    let mut teleported = false;
    for m in msgs {
        if m.c2s {
            if m.opcode() == wire::ENTER_WORLD {
                playing = u32_at(&m.payload, 4);
                (recalled, teleported) = (false, false);
            } else if playing == Some(character) && wire::action_type(&m.payload) == Some(0x0063) {
                recalled = true;
            }
            continue;
        }
        if !recalled || playing != Some(character) {
            continue;
        }
        if m.opcode() == wire::PLAYER_TELEPORT {
            teleported = true;
        } else if teleported
            && m.opcode() == wire::UPDATE_POSITION
            && u32_at(&m.payload, 4) == Some(character)
        {
            let e = proto::read_body_padded::<dereth_protocol::movement::MovementPositionEvent>(
                m.payload.get(4..)?,
            )
            .ok()?;
            let (o, q) = (e.position.origin, e.position.orientation);
            return Some(empyrean_entity::models::PropertiesPosition {
                obj_cell_id: o.objcell_id,
                position_x: o.origin.x,
                position_y: o.origin.y,
                position_z: o.origin.z - 0.005,
                rotation_w: q.w,
                rotation_x: q.x,
                rotation_y: q.y,
                rotation_z: q.z,
            });
        }
    }
    None
}

/// The quest flags a character holds are never sent to the client, but a portal that let the
/// recorded character through shows its `QuestRestriction` was met (`Portal.CheckUseRequirements`:
/// `HasQuest && !CanSolve`). Per character: the weenie of each object it used whose answer was a
/// PlayerTeleport (before its next action).
fn recorded_portal_uses(msgs: &[Msg]) -> HashMap<u32, Vec<u32>> {
    let mut wcid_of: HashMap<u32, u32> = HashMap::new();
    let mut out: HashMap<u32, Vec<u32>> = HashMap::new();
    let mut playing = None;
    let mut pending: Option<u32> = None;
    for m in msgs {
        if m.c2s {
            if m.opcode() == wire::ENTER_WORLD {
                playing = u32_at(&m.payload, 4);
            }
            if !wire::is_movement(&m.payload) {
                pending = (wire::action_type(&m.payload) == Some(0x0036))
                    .then(|| u32_at(&m.payload, 12))
                    .flatten();
            }
            continue;
        }
        if m.opcode() == wire::CREATE_OBJECT {
            if let Some(c) = decode_create(&m.payload) {
                wcid_of.insert(c.id.0, c.wdesc.wcid);
            }
        } else if m.opcode() == wire::PLAYER_TELEPORT {
            if let (Some(c), Some(used)) = (playing, pending.take()) {
                out.entry(c)
                    .or_default()
                    .extend(wcid_of.get(&used).copied());
            }
        }
    }
    out
}

fn plan_reconstruct_sanctuaries(
    msgs: &[Msg],
    reconstruct: &[(
        u32,
        LoginPlayerDescription,
        ObjectCreatePayload,
        Vec<ObjectCreatePayload>,
    )],
) -> HashMap<u32, empyrean_entity::models::PropertiesPosition> {
    reconstruct
        .iter()
        .filter_map(|(g, ..)| recorded_sanctuary(msgs, *g).map(|p| (*g, p)))
        .collect()
}

/// Replays one recording (see the module docs). Never panics on our server's behalf: a panic in
/// the server is caught and reported as `crashed`.
///
/// # Panics
/// When the recording, the dats or the pack cannot be read.
#[must_use]
pub fn replay_session(name: &str) -> ReplayOutcome {
    let wall = Instant::now();
    let rec = recording::load(&recording::captures_root(), name);
    let mut out = ReplayOutcome {
        name: name.to_owned(),
        recorded: rec.clone(),
        ours: Vec::new(),
        not_ported: vec![BTreeMap::new()],
        notes: Vec::new(),
        reconstructed: 0,
        missing_weenies: 0,
        crashed: None,
        steered: Vec::new(),
        not_steered: Vec::new(),
        capped_waits: 0,
        virtual_s: 0.0,
        wall_s: 0.0,
    };
    let (mut got, mut sent) = (Vec::new(), Vec::new());
    let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        run(&rec, &mut out, &mut got, &mut sent)
    }));
    if let Err(e) = result {
        let why = e
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| e.downcast_ref::<&str>().map(|s| (*s).to_owned()))
            .unwrap_or_default();
        // keep the panic's first line only, and nothing that could be payload text
        out.crashed = Some(
            why.lines()
                .next()
                .unwrap_or("panic")
                .chars()
                .take(160)
                .collect(),
        );
    }
    got.sort_by_key(|m| m.seq);
    out.ours = recording::interleave(got, sent);
    out.wall_s = wall.elapsed().as_secs_f64();
    out
}

/// Our side of the run: the stream as it arrives, stamped with the virtual time.
struct Driver<'a> {
    ts: TestServer,
    id: ClientId,
    /// Our received messages (arrival time, message).
    got: &'a mut Vec<Msg>,
    seen: usize,
    view: OurView,
    /// The recorded deaths anchored and applied as the server steps.
    steer: Steering,
}

impl Driver<'_> {
    fn now(&self) -> f64 {
        self.ts.seconds()
    }

    /// One world iteration; the messages that arrived are stamped and indexed.
    fn step(&mut self) {
        self.ts.step();
        let t = self.ts.seconds();
        let inbox = self.ts.received_raw(self.id);
        for m in &inbox[self.seen..] {
            let mut payload = m.opcode.to_le_bytes().to_vec();
            payload.extend_from_slice(&m.body);
            let label = wire::label(&payload);
            if let Some(i) = super::compare::roll_outcome(&payload, &label) {
                self.view.rolls[i] += 1;
            }
            *self.view.counts.entry(label).or_insert(0) += 1;
            if m.opcode == wire::CREATE_OBJECT {
                if let Some(c) = decode_create(&payload) {
                    self.view.creates.push((c.id.0, Placed::of(&c)));
                }
            }
            if m.opcode == wire::CHARACTER_LIST {
                if let Some(slots) = character_slots(&payload) {
                    self.view.slots = slots;
                }
            }
            for item in vendor_items(&payload) {
                self.view.creates.push(item);
            }
            if m.opcode == wire::CHAR_GEN_RESPONSE && self.view.chargen_pending > 0 {
                if let Some(a) = chargen_answer(&payload) {
                    self.view.chargen.push(a);
                    self.view.chargen_pending -= 1;
                }
            }
            self.got.push(Msg {
                t,
                c2s: false,
                queue: queue_id(m.queue),
                seq: m.blob_id.low32(),
                payload,
            });
        }
        self.seen = inbox.len();
        self.steer
            .apply(&mut self.ts.world, t, self.view.creates.iter().map(|c| c.0));
    }

    fn step_until(&mut self, t: f64) {
        while self.now() < t {
            self.step();
        }
    }

    /// Steps until `done` holds or `max` virtual seconds pass; whether it held.
    fn wait(&mut self, max: f64, mut done: impl FnMut(&Self) -> bool) -> bool {
        let end = self.now() + max;
        while !done(self) {
            if self.now() >= end {
                return false;
            }
            self.step();
        }
        true
    }
}

#[allow(clippy::too_many_lines)]
fn run(rec: &Recording, out: &mut ReplayOutcome, got: &mut Vec<Msg>, ours_c2s: &mut Vec<Msg>) {
    let plan = plan(rec);
    let content = pack();
    let mut seeded_guids = Vec::new();
    let mut reconstructed: Vec<Reconstructed> = Vec::new();
    let access = plan.access;
    let mut plan = plan;
    let mut names = plan.reconstruct_names.clone().into_iter();
    let setup = |w: &mut empyrean_world::World| {
        w.content = Arc::<PackContent>::clone(&content);
        let account_id = w
            .auth
            .lock()
            .create_account(
                ACCOUNT,
                PASSWORD,
                access,
                std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            )
            .expect("the replay account")
            .account_id;
        for (g, pd, own, possessions) in &plan.reconstruct {
            let name = names.next().unwrap_or("Replay Zulu");
            let mut r =
                character::reconstruct(&*content, *g, account_id, name, pd, own, possessions);
            // a spell's registry row keeps its MetaSpellType (`EnchantmentManager.Add`)
            for row in r.biota.properties_enchantment_registry.iter_mut().flatten() {
                if row.enchantment_category == character::ENCHANTMENT_CATEGORY_FROM_SPELL {
                    let spell =
                        empyrean_world::entity::spell::Spell::from_int(w, row.spell_id, true);
                    row.enchantment_category = spell.meta_spell_type().0.cast_unsigned();
                }
            }
            if let Some(p) = plan.sanctuaries.get(g) {
                r.biota.set_property_position(
                    empyrean_entity::enums::PositionType::Sanctuary,
                    p.clone(),
                );
            }
            // the portals' quest restrictions it met, as solved once just now (so `HasQuest`, and
            // `CanSolve` answers what the quest's timer or solve limit says)
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let now = w.now.unix_time as u32;
            for wcid in plan.portal_uses.get(g).into_iter().flatten() {
                let restriction = content.get_cached_weenie(*wcid).and_then(|wn| {
                    wn.get_property(empyrean_entity::enums::PropertyString::QuestRestriction)
                });
                if let Some(q) = restriction {
                    let name = q.split('@').next().unwrap_or_default().to_owned();
                    if !r
                        .character
                        .character_properties_quest_registry
                        .iter()
                        .any(|e| e.quest_name.eq_ignore_ascii_case(&name))
                    {
                        r.character.character_properties_quest_registry.push(
                            empyrean_store::models::shard::CharacterPropertiesQuestRegistry {
                                character_id: *g,
                                quest_name: name,
                                last_time_completed: now,
                                num_times_completed: 1,
                            },
                        );
                    }
                }
            }
            seeded_guids.push(*g);
            seeded_guids.extend(r.possessions.iter().map(|b| b.id));
            assert!(
                w.shard.base_database().add_character_in_parallel(
                    &mut r.biota,
                    &mut r.possessions,
                    &r.character
                ),
                "seeding a reconstructed character"
            );
            reconstructed.push(r);
        }
    };
    let mut ts = TestServer::with_setup(dats(), setup);
    ts.world.world_manager.world_status = WorldStatusState::Open;
    let seeded: HashSet<u32> = seeded_guids.iter().copied().collect();
    guid_manager::initialize(&mut ts.world, &mut SeededGuids(seeded_guids));
    out.reconstructed = reconstructed.len();
    out.missing_weenies = reconstructed.iter().map(|r| r.missing_weenies).sum();

    let id = ts.connect(ACCOUNT, PASSWORD);
    let _ = TestServer::take_not_ported();
    let mut d = Driver {
        ts,
        id,
        got,
        seen: 0,
        view: OurView::default(),
        steer: Steering::default(),
    };
    // CAPTURE_REPLAY_STEER=0: no anchored deaths and no capped waits (a debugging comparison only)
    let steering = std::env::var("CAPTURE_REPLAY_STEER").map_or(true, |v| v != "0");

    let mut map = GuidMap {
        seeded,
        ..GuidMap::default()
    };
    for (g, ..) in &plan.reconstruct {
        map.bind(*g, *g);
    }
    let mut rv = RecordedView::default();
    // recorded characters are nameable from the start (the character list names them)
    for m in rec
        .msgs
        .iter()
        .filter(|m| m.c2s && m.opcode() == wire::ENTER_WORLD)
    {
        rv.known.extend(u32_at(&m.payload, 4));
    }
    // one entry per recorded character creation: the created guid, or None when refused
    let mut rec_chargen: Vec<Option<u32>> = Vec::new();
    let mut rec_chargen_pending = 0usize;
    let mut chargen_bound = 0usize;

    let (mut rec_anchor, mut our_anchor) = (0.0f64, d.now());
    // per triggered message kind: the (recorded, our) trigger counts when it was last sent
    let mut last_trigger: HashMap<String, (usize, usize)> = HashMap::new();
    let mut prev_t = 0.0f64;
    let mut phase = 0usize;
    // our time when the last phase-opening (non-movement) message was sent
    let mut last_action_sent = f64::NEG_INFINITY;
    // our character in the world (the killer of a steered death)
    let mut our_player: Option<u32> = None;

    for m in &rec.msgs {
        if !m.c2s {
            // what the recorded client has been told
            let label = wire::label(&m.payload);
            if let Some(i) = super::compare::roll_outcome(&m.payload, &label) {
                rv.rolls[i] += 1;
            }
            *rv.counts.entry(label).or_insert(0) += 1;
            match m.opcode() {
                wire::CREATE_OBJECT => {
                    if let Some(c) = decode_create(&m.payload) {
                        rv.known.insert(c.id.0);
                        rv.creates.insert(c.id.0, Placed::of(&c));
                        let g = c.id.0;
                        if steer::is_creature_type(c.wdesc.obj_type)
                            && wire::is_dynamic_guid(g)
                            && !wire::is_player_guid(g)
                        {
                            rv.creatures.insert(g);
                        }
                    }
                }
                wire::CHARACTER_LIST => {
                    if let Some(slots) = character_slots(&m.payload) {
                        rv.slots = slots;
                    }
                }
                wire::GAME_EVENT => {
                    for (g, placed) in vendor_items(&m.payload) {
                        rv.known.insert(g);
                        rv.creates.insert(g, placed);
                    }
                }
                wire::CHAR_GEN_RESPONSE => {
                    if let Some(a) = chargen_answer(&m.payload) {
                        rv.known.extend(a);
                        if rec_chargen_pending > 0 {
                            rec_chargen.push(a);
                            rec_chargen_pending -= 1;
                        }
                    }
                }
                _ => {}
            }
            // a monster's recorded death: anchored at its recorded time on our clock
            let due = our_anchor + (m.t - rec_anchor);
            if steering {
                d.steer
                    .observe(&m.payload, &rv.creatures, &map.fwd, our_player, due, phase);
            }
            continue;
        }

        // timing
        let op = m.opcode();
        let gap = m.t - prev_t;
        if (is_screen(op) || phase == 0) && gap > SCREEN_GAP_S {
            rec_anchor += gap - SCREEN_GAP_S;
        }
        prev_t = m.t;
        d.step_until(our_anchor + (m.t - rec_anchor));

        // closed loop: the server message this one answers. Counted since the previous client
        // message of the same kind, on each side, so one missing answer does not hold up the rest.
        if let Some((a, b)) = trigger(&m.payload) {
            let count = |c: &HashMap<String, usize>| {
                c.get(a).copied().unwrap_or(0) + b.map_or(0, |b| c.get(b).copied().unwrap_or(0))
            };
            let kind = wire::label(&m.payload);
            let (rec_before, our_before) = last_trigger.get(&kind).copied().unwrap_or((0, 0));
            let need = count(&rv.counts).saturating_sub(rec_before);
            let before = d.now();
            if need > 0
                && !d.wait(WAIT_S, |d| {
                    count(&d.view.counts).saturating_sub(our_before) >= need
                })
            {
                out.notes
                    .push((phase, format!("{kind}: waited {WAIT_S} s for {a} x{need}")));
            }
            if d.now() > before {
                (rec_anchor, our_anchor) = (m.t, d.now());
            }
            last_trigger.insert(kind, (count(&rv.counts), count(&d.view.counts)));
        }

        // bind the recorded characters created so far to ours, creation by creation
        while chargen_bound < rec_chargen.len().min(d.view.chargen.len()) {
            if let (Some(a), Some(b)) = (rec_chargen[chargen_bound], d.view.chargen[chargen_bound])
            {
                map.bind(a, b);
            }
            chargen_bound += 1;
        }

        // translation
        let payload = match op {
            wire::ENTER_WORLD => {
                let g = u32_at(&m.payload, 4).unwrap_or(0);
                let ours = map.fwd.get(&g).copied().unwrap_or(g);
                our_player = Some(ours);
                proto::write_blob(&LoginSendEnterWorld {
                    character: ObjectId(ours),
                    account: ACCOUNT.to_owned(),
                })
                .expect("encodes")
            }
            wire::CHAR_GEN => {
                match proto::read_body_padded::<CharacterSendCharGenResult>(&m.payload[4..]) {
                    Ok(mut c) => {
                        c.account = ACCOUNT.to_owned();
                        c.result.name = plan.names.synthetic(&c.result.name).to_owned();
                        proto::write_blob(&c).expect("encodes")
                    }
                    Err(_) => m.payload.clone(),
                }
            }
            0xF655 => {
                // the account, and the slot: the recorded slot's character, in our list
                let mut p =
                    rewrite_string16(&m.payload, ACCOUNT).unwrap_or_else(|| m.payload.clone());
                let at = p.len().saturating_sub(4);
                let slot = u32_at(&p, at).and_then(|s| usize::try_from(s).ok());
                let ours = slot
                    .and_then(|s| rv.slots.get(s))
                    .and_then(|g| map.fwd.get(g))
                    .and_then(|g| d.view.slots.iter().position(|o| o == g))
                    .and_then(|s| u32::try_from(s).ok());
                match ours {
                    Some(s) => p[at..].copy_from_slice(&s.to_le_bytes()),
                    None => out.notes.push((
                        phase + 1,
                        "F655: untranslated slot (a character the recording never logs in to)"
                            .to_owned(),
                    )),
                }
                p
            }
            _ => {
                let from = if op == wire::GAME_ACTION { 12 } else { 4 };
                let mut p = m.payload.clone();
                let mut unresolved = Vec::new();
                for at in (from..p.len().saturating_sub(3)).step_by(4) {
                    let Some(g) = u32_at(&p, at) else { continue };
                    if !rv.known.contains(&g) {
                        continue;
                    }
                    let mut ours = map.resolve(g, &rv.creates, &d.view.creates);
                    if ours.is_none() && rv.creates.contains_key(&g) && wire::is_dynamic_guid(g) {
                        // closed loop: wait for our server to create a matching object
                        let before = d.now();
                        let mut found = None;
                        // capped in a phase whose roll outcomes already differ
                        let capped = steering && rv.rolls != d.view.rolls;
                        d.wait(
                            if capped {
                                RNG_OBJECT_WAIT_S
                            } else {
                                OBJECT_WAIT_S
                            },
                            |d| {
                                found = map.resolve(g, &rv.creates, &d.view.creates);
                                found.is_some()
                            },
                        );
                        if capped && found.is_none() {
                            out.capped_waits += 1;
                        }
                        ours = found;
                        if d.now() > before {
                            (rec_anchor, our_anchor) = (m.t, d.now());
                        }
                    }
                    match ours {
                        Some(o) => p[at..at + 4].copy_from_slice(&o.to_le_bytes()),
                        None => unresolved.push(g),
                    }
                }
                if !unresolved.is_empty() {
                    let kinds: Vec<&str> = unresolved
                        .iter()
                        .map(|g| {
                            if wire::is_player_guid(*g) {
                                "character"
                            } else if wire::is_dynamic_guid(*g) {
                                "dynamic"
                            } else {
                                "static"
                            }
                        })
                        .collect();
                    out.notes.push((
                        phase + 1,
                        format!(
                            "{}: untranslated guid ({})",
                            wire::label(&m.payload),
                            kinds.join(", ")
                        ),
                    ));
                }
                p
            }
        };

        // the phase that ends here
        out.not_ported[phase] = TestServer::take_not_ported();
        phase += 1;
        out.not_ported.push(BTreeMap::new());

        if op == wire::CHAR_GEN {
            rec_chargen_pending += 1;
            d.view.chargen_pending += 1;
        }
        // Each action gets a server step of its own: ACE handled a message as it arrived (the
        // recorded client's actions a few ms apart were answered one by one), while two sent in
        // the same step here would be answered together and every answer compared under the last
        // one's phase.
        if !wire::is_movement(&payload) {
            if d.now() <= last_action_sent {
                d.step();
            }
            last_action_sent = d.now();
            // a new phase: its roll outcomes, on both sides
            rv.rolls = [0; super::compare::ROLL_KINDS];
            d.view.rolls = [0; super::compare::ROLL_KINDS];
        }
        d.ts.client_mut(d.id).send(net_queue(m.queue), &payload);
        ours_c2s.push(Msg {
            t: d.now(),
            c2s: true,
            queue: m.queue,
            seq: 0,
            payload,
        });
    }

    // the recorded tail after the last client message
    let last_c2s = rec.msgs.iter().rev().find(|m| m.c2s).map_or(0.0, |m| m.t);
    let end = rec.msgs.last().map_or(0.0, |m| m.t);
    // plus a grace: our server's steps are 1/60 s and a chained action lands on a step boundary,
    // so the recorded server's last answer (a logoff's LogOffComplete 6.006 s after the request)
    // arrives a few steps later on ours (6.05 s) and would otherwise be cut off
    let tail = (end - last_c2s).clamp(1.0, TAIL_S) + TAIL_GRACE_S;
    let until = d.now() + tail;
    d.step_until(until);
    out.not_ported[phase] = TestServer::take_not_ported();
    out.virtual_s = d.now();
    out.steered = std::mem::take(&mut d.steer.steered);
    out.not_steered = std::mem::take(&mut d.steer.not_steered);
}
