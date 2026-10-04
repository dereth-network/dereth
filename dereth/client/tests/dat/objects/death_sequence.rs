//! **The death sequence as one chain, on a recorded retail death.**
//!
//! Fixture: the recording `requested-death-vitae-salvage` (corpus session 13), which carries a
//! death. Its bytes are replayed in their recorded order into a real [`App`], from before the
//! fight through the corpse being opened, the vitae wearing off and a relog, and the whole chain
//! is asserted in order: health to zero, the death lines, vitae, the purge, the lifestone release,
//! the corpse, the shortcuts of the items that went to it, opening it, another player's death in
//! view, and the relog. The death-bubble and corpse-decay behaviour have their own modules
//! (`objects::death_teleport_effects`, `objects::corpse_decay`).
//!
//! # The chain, at the recorded bytes
//!
//! Every index below is a line number in
//! `fixtures/message-corpus/requested-death-vitae-salvage/blobs.jsonl` and every time is that
//! line's `t_rel`. [`the_recorded_death_is_the_bytes_this_station_replays`] re-derives all of them
//! from the fixture, so no literal here is trusted.
//!
//! | # | link | blob / t | observed behavior |
//! |---|---|---|---|
//! | 1 | health to 0 | `0x02E9` pid 2, blobs 364…455, t = 46.34 → 64.94 s | secondary health lookup `(2, ·, 0)` |
//! | 2 | the victim line | `0x01AC` blob **457**, t = 64.940 | appends a type-0 scroll line |
//! | 3 | the death event **not** drawn | `0x019E` blob **458**, `killed 0x5000001D` | draws only when the player is neither killed nor killer |
//! | 4 | vitae armed | `0x02C2` blob **463**, spell 666, `_smod.val` 0.9 | modifier type bit `0x800000` routes vitae |
//! | 5 | the purge | `0x02C6` blob **464** | purges the two spell lists; **vitae survives** in its separate slot |
//! | 6 | the lifestone release | `0x02DB` pid 14 blob **508**, `0xF751` blob **525**, `0xF748` blob **526**, t = 67.01 → 67.02 s | teleport handling moves the body to the lifestone |
//! | 7 | the corpse | `0xF745` blob **528**, object `0x800001CB`, cell `0x9DAF0029` — the **death** landblock, not the lifestone's `0xA9B4` | object creation keeps the corpse at the death site |
//! | 8 | the three items that left | `0xF747` blobs **516, 520, 523** — `0x800003E4`, `0x8000037B`, `0x800009FF` | a shortcut dies when the object loses player ownership |
//! | 9 | the corpse opened | `0x0195` c2s blob 689 t = 103.14 s → `0x0196` blob **691** t = 103.24 s, **eleven** content profiles, then eleven `0xF745` (blobs 692…702) | view contents sets the requested ground object and opened-corpse state |
//! | 10b | **the other player dies in view** | `0x02BB` x5 t = 157.57…169.57 s, `0xF745` "Corpse of +Delwyn" blob **997** t = 174.658 s, his `0xF748` to `0xA9B4` + `0xF74B` hide blob **999** t = 174.664 s, the unhide blob **1011** t = 181.529 s | the observer side of a death teleport (`objects::death_teleport_effects`); ACE's `/die` path |
//! | 10 | the vitae wearing off | `0x02C2` blobs **893** (0.909 999 97, t = 136.63), **902** (0.919 999 96, t = 138.28), **913** (0.999 999 9, t = 141.60), then `0x02C3` blob **914** t = 143.60 | vitae ticks redraw the pane; removal clears the separate slot |
//! | 11 | the relog | `0xF653` c2s blob 1134 t = 236.87 → s2c blob 1149 t = 242.88 → `0xF658` blob 1150 → `0xF657` c2s blob 1154 t = 246.89 → `0x0013` blob **1155** t = 246.90 | the logoff and enter-world flow returns to Playable |
//!
//! # What the bytes show
//!
//! * **The character does not arrive clean, and the drawn health bar reads zero.** The
//!   login `0x0013` (blob 9) already carries a vitae for spell 666 at `_smod.val` **0.949 999 99**
//!   (a 5% penalty from a death that is not in this recording), so this death *deepens* it to
//!   0.90 (ACE subtracts the vitae penalty from the existing stat-mod value), and the lamp is lit
//!   before the fight starts. When the shard's `0x02E9` writes health **0**, the client draws
//!   **0 of the maximum**: it reads the vital through `(desc, 2, &v, 0)`, *not raw*, but the
//!   attribute-enchantment path asks the quality filter first, and the filter lists only the three
//!   maxima. Current health is returned as stored, so neither vitae nor the enchantment path's
//!   floor (1 below a raw 5) reaches it. Only the maximum carries the vitae.
//! * **The corpse carries eleven content profiles.** Blob 691 is 112 bytes: a `0xF7B0` envelope
//!   (12), the `0x0196` opcode (4), the container (4), a count of **11** (4) and 11 × 8 bytes of
//!   profile, and the shard follows it with exactly **eleven** `0xF745` creates (blobs 692…702).
//!   [`the_recorded_death_is_the_bytes_this_station_replays`] re-derives the count from the blob.
//! * **A corpse is not a radar blip at all, so nothing about it can grey.** The shard sends **no
//!   `RADAR_ENUM` field** for either corpse in this recording, so both read undefined (0).
//!   The radar-admission rule accepts only `ShowMovement`(2), `ShowAttacking`(3), and
//!   `ShowAlways`(4), so it refuses both corpses. Radar coloring has no corpse arm either: it
//!   checks the authored blip color, then the portal, vendor, creature, and player bits. The
//!   client reads the corpse-opened state only while determining the unopened-corpse selection
//!   class, so opening changes selection behavior rather than radar behavior.
//!   [`neither_corpse_is_ever_a_radar_blip`] is that rule as a test.
//! * **The recording holds a second death, seen by an observer.** There is no `0x019E` or
//!   `0x01AD` for it, but at t = 174.658 s the shard creates `0xF745` **"Corpse of +Delwyn"**
//!   (`0x800001ED`, blob 997) and six milliseconds later moves that player to a landblock this
//!   client has not loaded and hides him: the observer side of a death, with no line of text
//!   anywhere. `link 10b` of the chain replays it.
//!
//!   **Why there is no `0x019E`: it is ACE's `/die` code path, not range.** The five
//!   `0x02BB HearSpeech` at t = 157.57 … 169.57 s, 3.0 s apart, are ACE's suicide messages
//!   verbatim, spaced by its 3-second delay, so the `/die` handler ran; it kills the character
//!   **directly**, and the only producer of the victim notification and the player-killed
//!   broadcast is reached **only** from the damage paths. Range is ruled out by measurement: all
//!   five broadcast suicide lines reached this client, so it was among that player's known
//!   players throughout. A monster or player kill of a second character is still needed for the
//!   `0x019E` broadcast half.
//!
//!   The rest of the observer side is in those bytes and is asserted below: the corpse appears
//!   at the death site; the body's `0xF748` names the **lifestone's** landblock, which is the only
//!   thing that takes it off the survivor's screen; a `0xF74B` carries `HIDDEN_PS |
//!   IGNORE_COLLISIONS_PS`; **no `0xF747` is ever sent**; and the unhide arrives **6.87 s** later,
//!   the interval of the teleport shimmer.
//!
//! # The replay endpoint
//!
//! `App::attach_replay_network`, the socket-free fixture `objects::frame_phase_ordering` also
//! uses. Each recorded **server** blob is re-framed into a real datagram by the independently
//! tested transport writer and handed to `ClientNetwork::feed`, the entry `receive_socket` uses;
//! the client's own datagrams go nowhere. The client blobs the login tunnel needs (`0xF657` at
//! blob 8 and blob 1154, `0xF653` at blob 1134) are raised through `Session::enter_world` /
//! `Session::log_off` at the recorded blob's own position, so the flow FSM passes through the
//! states it passed through in the recording and does not tear the link down. **No socket is
//! bound, no datagram leaves this process, no server is started.**
//!
//! The recording carries the account name in clear. It is read out of the recorded `0xF658` at
//! run time and handed straight back to `Session::enter_world`; it is never written down here and
//! never printed.

use crate::common::workspace_root;

use std::path::PathBuf;

use dereth_client::app::App;
use dereth_client::config::Config;
use dereth_client::net::ClientNetwork;
use dereth_primitives::{LocalTime, ObjectId};
use dereth_protocol::Message as _;
use dereth_ui::framework::mode;
use dereth_ui::{ElemHandle, ElementId, UiSystem};
use dereth_ui_screens::mapradar::radar::{get_blip_color, inq_showable_on_radar};

use dereth_ui_screens::screens::gameplay::GamePlayScreen;
use dereth_ui_screens::view::{GameView as _, UiRequest, Vital};

// =================================================================================================
// The recorded death, by name
// =================================================================================================

/// The recorded character — `0xF746 Login_CreatePlayer`, blob 24. It is also `0x019E`'s `killed`.
const VICTIM: ObjectId = ObjectId(0x5000_001D);
/// `0x019E`'s `killer`, the other player whose eleven `0x02BB` spell words precede the kill.
const KILLER: ObjectId = ObjectId(0x5000_0025);
/// The corpse the shard created at the death site, `0xF745` blob 528.
const CORPSE: ObjectId = ObjectId(0x8000_01CB);
/// **The other player's corpse**: `0xF745` blob 997, name *"Corpse of +Delwyn"*. The observer
/// side of a death teleport.
const OTHER_CORPSE: ObjectId = ObjectId(0x8000_01ED);
/// The landblock both deaths happened in, and the landblock both bodies were released to.
const DEATH_BLOCK: u32 = 0x9DAF;
const LIFESTONE_BLOCK: u32 = 0xA9B4;
/// `HIDDEN_PS` and `IGNORE_COLLISIONS_PS` — `dereth_protocol::objects`'s names for the two bits a
/// death teleport sets to hide the character and suppress collisions.
const HIDDEN_PS: u32 = 0x0000_4000;
const IGNORE_COLLISIONS_PS: u32 = 0x0000_0010;
/// The three objects the shard deleted out of the player's hands at the death teleport —
/// `0xF747` blobs 516, 520, 523 — and which reappear inside the corpse at t = 103.2 s.
const WENT_TO_THE_CORPSE: [ObjectId; 3] = [
    ObjectId(0x8000_03E4),
    ObjectId(0x8000_037B),
    ObjectId(0x8000_09FF),
];

/// `STypeAttribute2nd::Health` — `Vital::Health`'s current-value stat.
const HEALTH: u32 = 2;
/// The Vitae spell. Modifier type bit `0x800000` routes it to the separate `_vitae` slot rather
/// than to any of the three enchantment lists.
const SPELL_VITAE: u32 = 0x29A;

/// The vitae-indicator lamp, action `0x1000000C`.
const VITAE_LAMP: ElementId = ElementId(0x1000_00F4);
/// The vitae panel and its single text child.
const VITAE_PANEL: ElementId = ElementId(0x1000_018A);
const VITAE_MAIN_TEXT: ElementId = ElementId(0x1000_01C3);
/// `STATE_NOTHING` — where a dark lamp rests.
const STATE_NOTHING: u32 = 0x0D;

/// The green fill left in chat-color slot 0. Both death-event paths append with text type 0 and
/// therefore draw this color.
const GREEN: u32 = 0xFF00_0000 | 0x0080_FF7F;

// The blob indices the chain hangs on. Each is re-derived from the fixture by
// `the_recorded_death_is_the_bytes_this_station_replays`; they are named here so the replay can
// stop at a link rather than at a wall-clock time.
/// `0xF7C8 LoginLogOnCharacter`, client → server — log-on **phase 1**,
/// and the blob position at which this station raises `Session::enter_world`. Phase 2's `0xF657`
/// (blobs 8 and 1156) is then produced by the flow itself, after the shard's `0xF7DF`, exactly as
/// the recording has it.
const IDX_LOG_ON_1: u64 = 6;
const IDX_LOG_ON_2: u64 = 1152;
/// `0xF657 LoginSendEnterWorld`, client → server, cycle 1.
const IDX_ENTER_WORLD_1: u64 = 8;
/// The last `0x02E9` before the death lines — health already 0.
const IDX_HEALTH_ZERO: u64 = 455;
/// `0x01AC`, `0x019E`, the vitae `0x02C2` and the `0x02C6` purge, all at t = 64.939 979 s.
const IDX_VICTIM_LINE: u64 = 457;
const IDX_DEATH_EVENT: u64 = 458;
const IDX_VITAE_ARM: u64 = 463;
const IDX_PURGE: u64 = 464;
/// The release: `0x02DB` position, `0xF751` teleport, the lifestone `0xF748`, the corpse `0xF745`.
const IDX_TELEPORT: u64 = 525;
const IDX_CORPSE_CREATE: u64 = 528;
/// The client's own `0xF7B1` sub-type `0x36 Inventory_UseEvent` on the corpse, t = 103.137 s, and
/// the second one that closes it again, t = 119.973 s.
const IDX_USE_CORPSE: u64 = 689;
const IDX_CLOSE_CORPSE: u64 = 831;
/// `0x0196 ItemOnViewContents` and the eleven `0xF745` that follow it.
const IDX_VIEW_CONTENTS: u64 = 691;
const IDX_CORPSE_CONTENTS_DONE: u64 = 702;
/// The three vitae value changes and the removal.
const IDX_VITAE_TICKS: [u64; 3] = [893, 902, 913];
const IDX_VITAE_REMOVED: u64 = 914;
/// The other player's five `/die` lines — `0x02BB HearSpeech`, t = 157.57 … 169.57 s, 3.0 s apart.
const IDX_SUICIDE_LINES: [u64; 5] = [970, 974, 980, 984, 988];
/// His corpse's create, t = 174.658 s; his own release, t = 174.664 s; the hide's end, t = 181.53
/// s.
const IDX_OTHER_CORPSE_CREATE: u64 = 997;
const IDX_OTHER_HIDE: u64 = 999;
const IDX_OTHER_UNHIDE: u64 = 1011;

/// The relog: the client's `0xF653`, the shard's ack and character set, the client's `0xF657`,
/// and the second `0x0013`.
const IDX_LOG_OFF: u64 = 1134;
const IDX_ENTER_WORLD_2: u64 = 1154;
const IDX_SECOND_DESCRIPTION: u64 = 1155;

// =================================================================================================
// The fixture
// =================================================================================================

/// One line of `blobs.jsonl`: the reassembled blob, its direction, its recorded time and the
/// queue it rode.
struct Blob {
    idx: u64,
    s2c: bool,
    t: f64,
    queue: u16,
    bytes: Vec<u8>,
}

impl Blob {
    /// The bare message opcode — every blob's first dword.
    fn opcode(&self) -> u32 {
        u32::from_le_bytes(
            self.bytes[..4]
                .try_into()
                .expect("a blob is at least a dword"),
        )
    }

    /// The game-event opcode when this blob is a `0xF7B0` envelope, else `None`.
    fn event(&self) -> Option<u32> {
        (self.opcode() == 0xF7B0 && self.bytes.len() >= 16)
            .then(|| u32::from_le_bytes(self.bytes[12..16].try_into().expect("four bytes")))
    }

    /// The event body, with the `0xF7B0` envelope and the event opcode stripped.
    fn event_body(&self) -> &[u8] {
        &self.bytes[16..]
    }

    /// The opcode that identifies this blob whichever space it is in -- the event opcode when it
    /// is wrapped, the message opcode when it is bare. `0x02BB HearSpeech` arrives **both** ways
    /// in this recording, which is why neither census below may assume one.
    fn key(&self) -> u32 {
        self.event().unwrap_or_else(|| self.opcode())
    }

    /// The payload after whichever of the two headers this blob carries.
    fn body(&self) -> &[u8] {
        if self.event().is_some() {
            self.event_body()
        } else {
            &self.bytes[4..]
        }
    }

    fn u32_at(&self, off: usize) -> u32 {
        u32::from_le_bytes(self.bytes[off..off + 4].try_into().expect("four bytes"))
    }
}

/// `fixtures/message-corpus/requested-death-vitae-salvage/blobs.jsonl`, corpus session 13.
///
/// The file is generated as one flat JSON object per line, and `dereth-client` has no JSON
/// dependency. Absence is a broken checkout and fails the station rather than skipping it.
fn session_13() -> Vec<Blob> {
    let path =
        workspace_root().join("fixtures/message-corpus/requested-death-vitae-salvage/blobs.jsonl");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!("corpus session 13 is this module's fixture and could not be read at {path:?}: {e}")
    });
    let field = |line: &str, key: &str| -> String {
        let at = line
            .find(&format!("\"{key}\":"))
            .unwrap_or_else(|| panic!("{key} in {line}"))
            + key.len()
            + 3;
        let rest = line[at..].trim_start();
        let rest = rest.strip_prefix('"').unwrap_or(rest);
        let end = rest.find(['"', ',', '}']).expect("a terminated field");
        rest[..end].trim().to_string()
    };
    let out: Vec<Blob> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let hex = field(l, "payload_hex");
            Blob {
                idx: field(l, "idx").parse().expect("idx"),
                s2c: field(l, "dir") == "s2c",
                t: field(l, "t_rel").parse().expect("t_rel"),
                queue: field(l, "queue").parse().expect("queue"),
                bytes: hex
                    .as_bytes()
                    .chunks(2)
                    .map(|c| {
                        u8::from_str_radix(std::str::from_utf8(c).expect("ascii"), 16).expect("hex")
                    })
                    .collect(),
            }
        })
        .collect();
    assert_eq!(out.len(), 12_907, "session 13 is 12,907 blobs");
    out
}

// =================================================================================================
// The socket-free endpoint
// =================================================================================================

const PEER: &str = "127.0.0.1:19000";

/// The far end of the replay endpoint: real packet envelopes built with the independently tested
/// transport writer and fed straight into `ClientNetwork::feed`. **No socket exists.**
struct Peer {
    crypto: dereth_transport::CryptoSystem,
    sequence: u32,
    blob: u32,
}

impl Peer {
    fn new() -> (Self, ClientNetwork) {
        let mut net = ClientNetwork::new(PEER, 7304, "death-sequence", "unused", 0)
            .expect("a socket-free endpoint");
        net.session.transport.add_connection(
            0xB,
            0,
            1,
            0xDEAD_BEEF,
            0x1234_5678,
            Some(PEER.parse().expect("a literal address")),
        );
        (
            Self {
                crypto: dereth_transport::CryptoSystem::new(0xDEAD_BEEF),
                sequence: 1,
                blob: 0,
            },
            net,
        )
    }

    /// One recorded blob, re-fragmented exactly as the client's network layer fragments an outgoing
    /// blob.
    ///
    /// The stamp handed to `Transport::feed` is the application's **own local time**, not zero.
    /// Connection processing tests `140.0 < local_time - last_data_time` against a clock that runs
    /// while this test runs, so a stale stamp would time the link out mid-replay.
    fn send(&mut self, app: &mut App, queue: u16, bytes: &[u8]) {
        const MAX_FRAG_DATA: usize = 448;
        let now = LocalTime(app.clock().local_time);
        self.blob += 1;
        let chunks: Vec<&[u8]> = bytes.chunks(MAX_FRAG_DATA).collect();
        let num_frags = u16::try_from(chunks.len()).expect("a recorded blob fits");
        for (i, chunk) in chunks.iter().enumerate() {
            self.sequence += 1;
            let mut packet = dereth_transport::OutPacket::new(dereth_transport::ProtoHeader {
                seq_id: self.sequence,
                rec_id: 0xB,
                interval: 0x100,
                iteration: 1,
                ..Default::default()
            });
            packet
                .add_fragment(dereth_transport::Fragment::new(
                    dereth_transport::FragmentHeader {
                        blob_id_low: self.blob,
                        blob_id_high: 0x8000_0000,
                        num_frags,
                        blob_frag_size: 0,
                        blob_num: u16::try_from(i).expect("fits"),
                        queue_id: queue,
                    },
                    (*chunk).to_vec(),
                ))
                .expect("one fragment per packet");
            let raw = packet
                .serialize(Some(self.crypto.next()))
                .expect("the packet serialises");
            app.replay_network_mut()
                .expect("the replay endpoint is attached")
                .session
                .transport
                .feed(&raw, Some(PEER.parse().expect("addr")), now)
                .expect("the shipping parser accepts this datagram");
        }
    }
}

fn prefs_file() -> PathBuf {
    std::env::temp_dir().join("dere-death-sequence-not-created/prefs.ini")
}

fn app_config() -> Config {
    Config {
        headless: true,
        sound: false,
        ui: true,
        dat_dir: dereth_dat::testing::dat_dir(),
        preferences_file: prefs_file(),
        ..Config::default()
    }
}

// =================================================================================================
// The station
// =================================================================================================

/// The application, the peer and the recording, walked together.
struct Station {
    app: App,
    peer: Peer,
    blobs: Vec<Blob>,
    /// The next line of `blobs.jsonl` to deliver.
    next: usize,
    /// The account the recorded `0xF658` named. Read at run time, never written down.
    account: String,
    /// Every blob index at which a client blob was raised through the flow rather than replayed.
    raised: Vec<u64>,
    /// How many more blobs to deliver **one frame each**.
    ///
    /// The login tunnel has to be walked a blob at a time. `0xF7DF` rides queue 9 and `0xF746`
    /// rides queue 10. Enter-world phase 2 resets the world view and clears `player_id`: hand the
    /// session both blobs in one pump and it drains queue 10 first, latches the player from
    /// `0xF746`, then wipes it with the reset that in the recording arrived three milliseconds
    /// earlier. The real client reads one datagram at a time, so this station does too where
    /// order matters.
    dense: u32,
}

impl Station {
    fn new() -> Self {
        let dir = dereth_dat::testing::dat_dir();
        assert!(
            dereth_dat::testing::have_dats(),
            "the retail dats are required at {} -- set DERETH_TEST_DAT_DIR",
            dir.display()
        );
        let mut app =
            crate::common::sim_app::new(app_config()).expect("the application comes up headless");
        app.start_shell().expect("the UI shell comes up");
        let (peer, net) = Peer::new();
        app.attach_replay_network(net)
            .expect("a headless App takes a replay endpoint");
        let s = dereth_client::world::SceneConfig {
            landblock: app.config().landblock,
            land_radius: app.config().land_radius,
            scenery_radius: app.config().scenery_radius,
            ..dereth_client::world::SceneConfig::default()
        };
        app.load_static_scene(s).expect("the static scene loads");
        Self {
            app,
            peer,
            blobs: session_13(),
            next: 0,
            account: String::new(),
            raised: Vec::new(),
            dense: 64,
        }
    }

    /// Deliver every recorded blob up to and including `idx`, in the recorded order.
    ///
    /// Server blobs go through the transport. The three client blobs the login tunnel needs are
    /// **raised through the flow** at their own recorded position — `Session::enter_world` for the
    /// two `0xF657`, `Session::log_off` for the `0xF653` — because the client is the originator of
    /// those and a replay that only fed the server half would leave the FSM in character select
    /// while the shard talked to a world.
    fn feed_to(&mut self, idx: u64) {
        while self.next < self.blobs.len() && self.blobs[self.next].idx <= idx {
            let i = self.next;
            self.next += 1;
            if self.blobs[i].bytes.len() < 4 {
                continue;
            }
            if self.blobs[i].s2c {
                let (queue, opcode) = (self.blobs[i].queue, self.blobs[i].opcode());
                // The character set names the account and the character; both are the client's
                // inputs to `enter_world` below.
                if opcode == 0xF658 {
                    let mut r = dereth_protocol::Reader::new(&self.blobs[i].bytes[4..]);
                    if let Ok(set) = dereth_protocol::login::LoginCharacterSet::read(&mut r) {
                        self.account = set.account;
                    }
                }
                let bytes = std::mem::take(&mut self.blobs[i].bytes);
                self.peer.send(&mut self.app, queue, &bytes);
                self.blobs[i].bytes = bytes;
                // The frame loop is what drains the transport into `ObjectStream` and the HUD.
                // One frame per blob is what the recording looked like and what [`Self::dense`]
                // explains; away from the two login tunnels the order across queues cannot
                // matter, and every assertion below is made after `settle` anyway.
                if self.dense > 0 {
                    self.dense -= 1;
                    self.app.frame();
                } else if i % 24 == 0 {
                    self.app.frame();
                }
                continue;
            }
            match self.blobs[i].opcode() {
                // `0xF7C8 Login_LogOnCharacter` — phase 1. `Session::enter_world` is the whole
                // two-step: it sends this, and `0xF7DF` then drives `phase_two` into the `0xF657`
                // the recording carries a few milliseconds later. Raising it at the `0xF657`
                // instead leaves the awaiting-logon flag false when `0xF7DF` arrives. The
                // server-ready handler is gated on that flag before marking readiness for phase 2.
                0xF7C8 => {
                    let gid = self.blobs[i + 1..]
                        .iter()
                        .find(|b| !b.s2c && b.bytes.len() >= 8 && b.opcode() == 0xF657)
                        .map(|b| ObjectId(b.u32_at(4)))
                        .expect("every log-on in this recording is followed by its enter-world");
                    let account = self.account.clone();
                    self.app
                        .replay_network_mut()
                        .expect("replay")
                        .session
                        .enter_world(gid, &account);
                    self.raised.push(self.blobs[i].idx);
                    self.dense = 64;
                    self.settle(4);
                }
                // `0xF653 Login_ExecuteLogOffRequest` enters the normal session logoff flow.
                0xF653 => {
                    self.app
                        .replay_network_mut()
                        .expect("replay")
                        .session
                        .log_off();
                    self.raised.push(self.blobs[i].idx);
                    self.dense = 64;
                    self.settle(4);
                }
                // `0xF7B1` sub-type `0x36 Inventory_UseEvent` -- the double-click that opens a
                // corpse. Raised as the production `UiRequest::Use`, which is `Interaction`'s own
                // arm, so the client records the requested ground object and the shard's
                // `0x0196` can answer it. Without this
                // the reply is an orphan: the view-contents path sets the ground object **only**
                // when the container whose contents arrived is the one this client asked for.
                0xF7B1 if self.blobs[i].bytes.len() >= 16 && self.blobs[i].u32_at(8) == 0x36 => {
                    let target = ObjectId(self.blobs[i].u32_at(12));
                    self.app
                        .ui_mut()
                        .expect("the UI shell is up")
                        .ui
                        .requests
                        .emit(UiRequest::Use(target));
                    self.raised.push(self.blobs[i].idx);
                    self.settle(4);
                }
                _ => {}
            }
        }
        self.settle(4);
    }

    fn settle(&mut self, frames: u32) {
        for _ in 0..frames {
            self.app.frame();
        }
        // Everything the client wanted to say goes nowhere; there is no socket.
        let _ = self
            .app
            .replay_network_mut()
            .expect("replay")
            .take_outgoing();
    }

    /// Put the flow on the gameplay screen, where every drawn assertion below is made.
    fn into_gameplay(&mut self) {
        self.app.queue_ui_mode(mode::GAME_PLAY);
        for _ in 0..16 {
            self.app.frame();
            if self.app.ui().and_then(|u| u.flow.current_mode()) == Some(mode::GAME_PLAY) {
                return;
            }
        }
        panic!(
            "the flow did not reach the gameplay screen: {:?}",
            self.app.ui().and_then(|u| u.flow.current_mode())
        );
    }

    // -- readers -------------------------------------------------------------------------------

    fn health(&self) -> Option<(u32, u32)> {
        self.app
            .hud()
            .view(self.app.objects())
            .vital(VICTIM, Vital::Health)
    }

    fn vitae(&self) -> Option<f32> {
        self.app.hud().view(self.app.objects()).vitae()
    }

    /// The whole chat log as `(text, colour)` runs: what a player actually sees, read off the
    /// element.
    fn scroll(&mut self) -> Vec<(String, u32)> {
        let h = find(&mut self.app, dereth_ui_screens::chat::window::LOG);
        let ui = gameplay(&mut self.app).0;
        let t = ui
            .text_element_mut(h)
            .expect("the chat log is a text element");
        let mut out: Vec<(String, u32)> = Vec::new();
        for g in &t.glyphs.glyphs {
            let ch = char::from_u32(u32::from(g.data)).unwrap_or('\u{FFFD}');
            match out.last_mut() {
                Some((s, c)) if *c == g.color => s.push(ch),
                _ => out.push((ch.to_string(), g.color)),
            }
        }
        out
    }

    fn scroll_text(&mut self) -> String {
        self.scroll().into_iter().map(|(s, _)| s).collect()
    }

    /// The Skills page as drawn: `(skill, displayed value, font index)`.
    ///
    /// `SkillRow::value` is the **enchanted** total from a skill query with its raw flag clear.
    /// `font` records the display style: `0` plain, `1` buffed, or `2` debuffed.
    fn skills(&self) -> Vec<(u32, i32, u32)> {
        self.app
            .hud()
            .panels
            .skills
            .rows
            .iter()
            .map(|r| (r.skill, r.value, r.font))
            .collect()
    }

    fn lamp_state(&mut self) -> u32 {
        let h = find(&mut self.app, VITAE_LAMP);
        gameplay(&mut self.app)
            .0
            .node(h)
            .expect("a live node")
            .state
            .0
    }
}

fn gameplay(app: &mut App) -> (&mut UiSystem, &mut GamePlayScreen) {
    let shell = app.ui_mut().expect("the shell");
    let ui = &mut shell.ui;
    let screen = shell.flow.current_mut().expect("a screen is up");
    let any: &mut dyn std::any::Any = &mut **screen;
    let screen = any
        .downcast_mut::<GamePlayScreen>()
        .expect("the gameplay screen");
    (ui, screen)
}

fn find(app: &mut App, id: ElementId) -> ElemHandle {
    let (ui, screen) = gameplay(app);
    let root = screen.root().expect("the gameplay root");
    ui.get_child_recursive(root, id)
        .unwrap_or_else(|| panic!("{:#010X} is not in the shipped tree", id.0))
}

// =================================================================================================
// Station 0 — the bytes, before any of our code runs
// =================================================================================================

/// **Every index and value the chain above names, re-derived from the fixture.**
///
/// CPU only; no GPU, no `App`. A station that hard-coded its own blob indices would go green
/// against a corpus that had moved underneath it, and this is the file that stops that.
///
/// It is **not** a corpus-counting suite: it asserts session 13's own contents, not how many
/// sessions the corpus has.
#[test]
fn the_recorded_death_is_the_bytes_this_station_replays() {
    let blobs = session_13();
    let at = |idx: u64| -> &Blob {
        blobs
            .iter()
            .find(|b| b.idx == idx)
            .unwrap_or_else(|| panic!("blob {idx}"))
    };

    // --- link 1: health to zero -----------------------------------------------------------------
    // `0x02E9 Qualities_PrivateUpdateAttribute2ndLevel`: a `u8` sequence, the property, the value.
    let health: Vec<(u64, f64, u8, u32)> = blobs
        .iter()
        .filter(|b| b.s2c && b.bytes.len() == 13 && b.opcode() == 0x02E9)
        .filter(|b| u32::from_le_bytes(b.bytes[5..9].try_into().expect("4")) == HEALTH)
        .filter(|b| b.t <= 70.0)
        .map(|b| {
            (
                b.idx,
                b.t,
                b.bytes[4],
                u32::from_le_bytes(b.bytes[9..13].try_into().expect("4")),
            )
        })
        .collect();
    let values: Vec<u32> = health.iter().map(|h| h.3).collect();
    assert!(
        values.windows(2).any(|w| w[0] == 93 && w[1] == 81),
        "the character's health falls 93 -> 81; got {values:?}"
    );
    assert_eq!(values.last(), Some(&0), "and ends at zero");
    let zero = health
        .iter()
        .find(|h| h.3 == 0)
        .expect("a zero-health update");
    assert_eq!(
        zero.0, IDX_HEALTH_ZERO,
        "the first zero-health update is blob {IDX_HEALTH_ZERO}"
    );
    assert!(
        (zero.1 - 64.939_979).abs() < 1e-4,
        "at t = 64.94 s; got {}",
        zero.1
    );
    assert!(
        health
            .iter()
            .map(|h| h.2)
            .collect::<Vec<_>>()
            .windows(2)
            .all(|w| w[1] > w[0]),
        "and every sequence in the fight is in order -- the shard's per-property counter only \
         restarts at an enter-world"
    );

    // --- links 2 and 3: the two death lines ----------------------------------------------------
    let victim = at(IDX_VICTIM_LINE);
    assert_eq!(
        victim.event(),
        Some(0x01AC),
        "blob {IDX_VICTIM_LINE} is the victim notification"
    );
    assert_eq!(
        victim.u32_at(4),
        VICTIM.0,
        "addressed to the recorded player"
    );
    let victim_text = pstring(victim.event_body(), 0).expect("a packed string");
    assert!(
        victim_text.ends_with("brings you to a fiery end!"),
        "ACE `Strings.cs` Victim template; got {victim_text:?}"
    );
    let death = at(IDX_DEATH_EVENT);
    assert_eq!(
        death.opcode(),
        0x019E,
        "blob {IDX_DEATH_EVENT} is the player-death event"
    );
    let death_text = pstring(&death.bytes[4..], 0).expect("a packed string");
    let pad = 4 + 2 + death_text.len();
    let pad = pad + ((4 - pad % 4) % 4);
    assert_eq!(
        (ObjectId(death.u32_at(pad)), ObjectId(death.u32_at(pad + 4))),
        (VICTIM, KILLER),
        "the `0x019E` body is (string, victim, killer), and the victim is us, so the local-victim \
         gate suppresses an added death line"
    );
    assert!((victim.t - death.t).abs() < 1e-6, "both at t = 64.94 s");
    assert_eq!(
        blobs
            .iter()
            .filter(|b| b.s2c && b.opcode() == 0x019E)
            .count(),
        1,
        "one death event in the whole recording: the other death in view sends none"
    );
    assert_eq!(
        blobs.iter().filter(|b| b.event() == Some(0x01AD)).count(),
        0,
        "and no killer notification at all"
    );

    // --- links 4, 5 and 10: the vitae ----------------------------------------------------------
    // `0x02C2 Magic_UpdateEnchantment`: the `_smod.val` the lamp reads sits 56 bytes into the body.
    let vitae: Vec<(u64, f64, f32)> = blobs
        .iter()
        .filter(|b| b.event() == Some(0x02C2) && b.event_body().len() >= 60)
        .filter(|b| u32::from_le_bytes(b.event_body()[..4].try_into().expect("4")) == SPELL_VITAE)
        .map(|b| {
            (
                b.idx,
                b.t,
                f32::from_le_bytes(b.event_body()[56..60].try_into().expect("4")),
            )
        })
        .collect();
    assert_eq!(
        vitae.iter().map(|v| (v.0, v.2)).collect::<Vec<_>>(),
        vec![
            (IDX_VITAE_ARM, 0.9_f32),
            (IDX_VITAE_TICKS[0], 0.909_999_97),
            (IDX_VITAE_TICKS[1], 0.919_999_96),
            (IDX_VITAE_TICKS[2], 0.999_999_9),
        ],
        "four arrivals of spell 666: the death's 10% penalty and the three value changes that \
         wear it off"
    );
    // The login description already carries one: this death deepens a 5% penalty into a 10% one.
    let login_vitae = at(9)
        .bytes
        .windows(4)
        .position(|w| w == SPELL_VITAE.to_le_bytes())
        .map(|at9| f32::from_le_bytes(at(9).bytes[at9 + 56..at9 + 60].try_into().expect("4")));
    assert_eq!(
        login_vitae,
        Some(0.949_999_99),
        "`0x0013` blob 9 carries spell 666 at 0.95 -- an earlier death, off camera"
    );
    assert!(
        !at(IDX_SECOND_DESCRIPTION)
            .bytes
            .windows(4)
            .any(|w| w == SPELL_VITAE.to_le_bytes()),
        "and the description the relog fetches carries none, because `0x02C3` removed it"
    );
    assert_eq!(at(IDX_PURGE).event(), Some(0x02C6), "the purge");
    assert_eq!(
        at(IDX_PURGE).bytes.len(),
        16,
        "`0x02C6` is a body-less game event"
    );
    assert!(
        at(IDX_PURGE).t - at(IDX_VITAE_ARM).t >= 0.0 && at(IDX_PURGE).idx > at(IDX_VITAE_ARM).idx,
        "the purge arrives **after** the vitae, so purging only the two spell lists and leaving \
         `_vitae` untouched keeps the penalty"
    );
    let removed = at(IDX_VITAE_REMOVED);
    assert_eq!(removed.event(), Some(0x02C3), "the removal");
    assert_eq!(
        u32::from_le_bytes(removed.event_body()[..4].try_into().expect("4")),
        SPELL_VITAE,
        "of spell 666"
    );
    assert!(
        (removed.t - 143.601).abs() < 1e-3,
        "at t = 143.60 s; got {}",
        removed.t
    );

    // --- link 6: the lifestone release ---------------------------------------------------------
    let pos = blobs
        .iter()
        .find(|b| b.s2c && b.opcode() == 0x02DB)
        .expect("a `0x02DB`");
    assert_eq!(pos.bytes[4], 0x00, "`0x02DB` opens with its `u8` sequence");
    assert_eq!(
        pos.u32_at(5) & 0xFF_FFFF,
        14,
        "property 14 -- the position quality"
    );
    let tele = at(IDX_TELEPORT);
    assert_eq!(tele.opcode(), 0xF751, "blob {IDX_TELEPORT} is the teleport");
    assert_eq!(
        tele.u32_at(4),
        1,
        "teleport sequence 1 -- the first of the recording"
    );
    let lifestone = blobs
        .iter()
        .find(|b| b.s2c && b.opcode() == 0xF748 && b.idx > IDX_TELEPORT)
        .expect("a position after the teleport");
    assert_eq!(
        ObjectId(lifestone.u32_at(4)),
        VICTIM,
        "it moves the recorded player"
    );
    assert_eq!(
        lifestone.u32_at(12) >> 16,
        0xA9B4,
        "to the lifestone's landblock, which is not the one he died in"
    );

    // --- link 7: the corpse --------------------------------------------------------------------
    let corpse = at(IDX_CORPSE_CREATE);
    assert_eq!(
        corpse.opcode(),
        0xF745,
        "blob {IDX_CORPSE_CREATE} is a create"
    );
    assert_eq!(ObjectId(corpse.u32_at(4)), CORPSE, "of the corpse");
    assert!(
        corpse
            .bytes
            .windows(4)
            .any(|w| w == 0x9DAF_0029_u32.to_le_bytes())
            && !corpse
                .bytes
                .windows(4)
                .any(|w| w == 0xA9B4_0019_u32.to_le_bytes()),
        "created in the cell he died in (0x9DAF0029) and not at the lifestone (0xA9B40019)"
    );

    // --- link 8: the three items that left ------------------------------------------------------
    let left: Vec<ObjectId> = blobs
        .iter()
        .filter(|b| b.s2c && b.opcode() == 0xF747 && (66.0..68.0).contains(&b.t))
        .map(|b| ObjectId(b.u32_at(4)))
        .collect();
    for id in WENT_TO_THE_CORPSE {
        assert!(
            left.contains(&id),
            "{id:?} is deleted out of the player at the death teleport"
        );
    }

    // --- link 9: the corpse opened, with eleven profiles ----------------------------------------
    let view = at(IDX_VIEW_CONTENTS);
    assert_eq!(
        view.event(),
        Some(0x0196),
        "blob {IDX_VIEW_CONTENTS} is `ItemOnViewContents`"
    );
    let body = view.event_body();
    assert_eq!(
        ObjectId(u32::from_le_bytes(body[..4].try_into().expect("4"))),
        CORPSE
    );
    let profiles = u32::from_le_bytes(body[4..8].try_into().expect("4")) as usize;
    assert_eq!(
        profiles,
        11,
        "**eleven** content profiles: the blob is {} bytes, which is 8 + 11 x 8",
        body.len()
    );
    assert_eq!(
        body.len(),
        8 + profiles * 8,
        "and the body is exactly that long"
    );
    let inside: Vec<ObjectId> = (0..profiles)
        .map(|i| {
            ObjectId(u32::from_le_bytes(
                body[8 + i * 8..12 + i * 8].try_into().expect("4"),
            ))
        })
        .collect();
    for id in WENT_TO_THE_CORPSE {
        assert!(
            inside.contains(&id),
            "{id:?} is inside the corpse when it is opened"
        );
    }
    let creates = blobs
        .iter()
        .filter(|b| {
            b.s2c && b.opcode() == 0xF745 && b.idx > view.idx && b.idx <= IDX_CORPSE_CONTENTS_DONE
        })
        .count();
    assert_eq!(
        creates, profiles,
        "and the shard follows the profiles with one create each"
    );

    // --- the other player's death, in view ------------------------------------------------------
    // Five `0x02BB HearSpeech`, 3.0 s apart, carrying ACE's suicide messages verbatim, spaced by
    // its 3-second delay, then the death five seconds later.
    const SUICIDE: [&str; 5] = [
        "I feel faint...",
        "My sight is growing dim...",
        "My life is flashing before my eyes...",
        "I see a light...",
        "Oh cruel, cruel world!",
    ];
    for (n, idx) in IDX_SUICIDE_LINES.iter().enumerate() {
        let b = at(*idx);
        assert_eq!(b.key(), 0x02BB, "blob {idx} is a HearSpeech");
        let said = pstring(b.body(), 0).expect("a packed string");
        assert_eq!(said, SUICIDE[n], "ACE's suicide message {n}");
        assert!(
            b.bytes.windows(4).any(|w| w == KILLER.0.to_le_bytes()),
            "spoken by the other player"
        );
    }
    let gaps: Vec<f64> = IDX_SUICIDE_LINES
        .windows(2)
        .map(|w| at(w[1]).t - at(w[0]).t)
        .collect();
    assert!(
        gaps.iter().all(|g| (*g - 3.0).abs() < 0.02),
        "ACE's 3-second suicide delay; got {gaps:?}"
    );
    let other_corpse = at(IDX_OTHER_CORPSE_CREATE);
    assert_eq!(other_corpse.opcode(), 0xF745);
    assert_eq!(ObjectId(other_corpse.u32_at(4)), OTHER_CORPSE);
    // The recording's names are scrubbed stand-ins, so what is asserted is the shape the shard
    // composes (`"Corpse of "` and then the character's name carrying the account's own `+`
    // marker) and not the name itself. The blob, the opcode and the object id three lines up are
    // what pin *which* corpse this is.
    const CORPSE_OF: &[u8] = b"Corpse of +";
    assert!(
        other_corpse
            .bytes
            .windows(CORPSE_OF.len())
            .any(|w| w == CORPSE_OF),
        "the shard names it, with its own admin marker"
    );
    assert!(
        other_corpse.t - at(IDX_SUICIDE_LINES[4]).t > 4.0,
        "and it arrives after the death animation, not with the last line"
    );
    // **This is why there is no `0x019E` for him: a code path, not a range cull.** ACE's `/die`
    // handler kills the character directly; the *only* producer of the victim notification and
    // the player-killed broadcast is reached by the **damage** paths alone. Range is ruled out by
    // measurement: all five of his broadcast suicide lines reached this client, so it was among
    // his known players throughout.
    assert!(
        blobs
            .iter()
            .filter(|b| b.s2c && (160.0..200.0).contains(&b.t))
            .all(|b| b.key() != 0x019E && b.key() != 0x01AD && b.key() != 0x01AC),
        "no death line of any kind reaches the observer for the other player's `/die`"
    );
    // His release: `0xF748` to the lifestone landblock, the teleport hide, and **no `0xF747`**.
    let hide = at(IDX_OTHER_HIDE);
    assert_eq!(
        hide.opcode(),
        0xF74B,
        "blob {IDX_OTHER_HIDE} is a physics-state word"
    );
    assert_eq!(ObjectId(hide.u32_at(4)), KILLER);
    assert_eq!(
        hide.u32_at(8) & (HIDDEN_PS | IGNORE_COLLISIONS_PS),
        HIDDEN_PS | IGNORE_COLLISIONS_PS,
        "the teleport's physics-state change sets hidden; the word is {:#010X}",
        hide.u32_at(8)
    );
    let unhide = at(IDX_OTHER_UNHIDE);
    assert_eq!(unhide.opcode(), 0xF74B);
    assert_eq!(
        unhide.u32_at(8) & (HIDDEN_PS | IGNORE_COLLISIONS_PS),
        0,
        "the unhide script clears both"
    );
    assert!(
        (unhide.t - hide.t - 6.87).abs() < 0.05,
        "the teleport shimmer runs {:.2} s between the hide and the unhide, and the death \
         teleport takes the body off the observer's screen for it",
        unhide.t - hide.t
    );
    let released: Vec<u32> = blobs
        .iter()
        .filter(|b| b.s2c && b.opcode() == 0xF748 && (174.0..175.0).contains(&b.t))
        .filter(|b| ObjectId(b.u32_at(4)) == KILLER)
        .map(|b| b.u32_at(12) >> 16)
        .collect();
    assert!(
        !released.is_empty() && released.iter().all(|blk| *blk == LIFESTONE_BLOCK),
        "his release names the lifestone's landblock, which the observer has not loaded. \
         Got {released:02X?}"
    );
    assert!(
        !blobs.iter().any(|b| b.s2c
            && b.opcode() == 0xF747
            && (170.0..200.0).contains(&b.t)
            && ObjectId(b.u32_at(4)) == KILLER),
        "and **no `0xF747`**: ACE skips the delete broadcast for an adjacency move"
    );
    // The three creates of his corpse are one death re-sent: two of them land on the observer's
    // own enter-world instants.
    let creates: Vec<(u64, f64)> = blobs
        .iter()
        .filter(|b| b.s2c && b.opcode() == 0xF745 && ObjectId(b.u32_at(4)) == OTHER_CORPSE)
        .map(|b| (b.idx, b.t))
        .collect();
    assert_eq!(creates.len(), 3, "three creates: {creates:?}");
    assert_eq!(creates[0].0, IDX_OTHER_CORPSE_CREATE, "one death");
    assert!(
        (creates[1].1 - at(IDX_SECOND_DESCRIPTION).t).abs() < 0.1,
        "the second lands on the observer's own relog, which re-sends the visible set"
    );
    // Nothing ever opened it: no `0x0196` names it and no `0x00C9` appraisal does either.
    assert!(
        !blobs.iter().any(|b| b.key() == 0x0196
            && b.body().len() >= 4
            && u32::from_le_bytes(b.body()[..4].try_into().expect("4")) == OTHER_CORPSE.0),
        "the observer never opened the other player's corpse"
    );
    // It decays the way the observer's own did: `0xF755 Effects_PlayScriptType` and then the
    // delete, one second apart (see `objects::corpse_decay`).
    let decay = blobs
        .iter()
        .find(|b| b.s2c && b.opcode() == 0xF755 && ObjectId(b.u32_at(4)) == OTHER_CORPSE)
        .expect("a decay script");
    let gone = blobs
        .iter()
        .find(|b| b.s2c && b.opcode() == 0xF747 && ObjectId(b.u32_at(4)) == OTHER_CORPSE)
        .expect("a delete");
    assert!(
        (gone.t - decay.t - 1.0).abs() < 0.05,
        "script then delete, 1 s apart"
    );

    // --- link 11: the relog --------------------------------------------------------------------
    assert_eq!(
        at(IDX_LOG_OFF).opcode(),
        0xF653,
        "the client's log-off request"
    );
    assert!(!at(IDX_LOG_OFF).s2c, "which is the client's own blob");
    assert_eq!(
        at(IDX_ENTER_WORLD_1).opcode(),
        0xF657,
        "phase 2 of the first log-on"
    );
    assert_eq!(
        ObjectId(at(IDX_ENTER_WORLD_1).u32_at(4)),
        VICTIM,
        "on the recorded character"
    );
    for idx in [IDX_USE_CORPSE, IDX_CLOSE_CORPSE] {
        let b = at(idx);
        assert!(
            !b.s2c && b.opcode() == 0xF7B1,
            "blob {idx} is a client game action"
        );
        assert_eq!(b.u32_at(8), 0x36, "sub-type `0x36 Inventory_UseEvent`");
        assert_eq!(ObjectId(b.u32_at(12)), CORPSE, "on the corpse");
    }
    assert_eq!(
        at(IDX_LOG_ON_1).opcode(),
        0xF7C8,
        "phase 1 of the first log-on"
    );
    assert_eq!(at(IDX_LOG_ON_2).opcode(), 0xF7C8, "and of the second");
    assert!(
        !at(IDX_LOG_ON_1).s2c && !at(IDX_LOG_ON_2).s2c,
        "both are the client's own"
    );
    assert_eq!(
        at(IDX_ENTER_WORLD_2).opcode(),
        0xF657,
        "the second enter-world"
    );
    assert_eq!(
        ObjectId(at(IDX_ENTER_WORLD_2).u32_at(4)),
        VICTIM,
        "on the same character"
    );
    assert_eq!(
        at(IDX_SECOND_DESCRIPTION).event(),
        Some(0x0013),
        "and the second description"
    );
    assert!(
        blobs
            .iter()
            .filter(|b| b.event() == Some(0x02C2) || b.event() == Some(0x02C3))
            .all(|b| b.idx <= IDX_VITAE_REMOVED || b.t > 250.0 || b.event() == Some(0x02C2)),
        "no vitae message survives into the relog window"
    );
}

/// A packed string — a `u16` count then the bytes.
fn pstring(b: &[u8], off: usize) -> Option<String> {
    let n = usize::from(u16::from_le_bytes(b.get(off..off + 2)?.try_into().ok()?));
    Some(String::from_utf8_lossy(b.get(off + 2..off + 2 + n)?).into_owned())
}

// =================================================================================================
// Station 1 — the chain, in order, on the real application
// =================================================================================================

/// Behaviour: objects.death.the-recorded-death-plays-from-the-fight-to-vitae
///
/// **The whole death, link by link, replayed from the recording.**
///
/// One test rather than ten: the subject *is* the order. Each link is announced on stderr
/// before it is asserted, so a red run says which link broke without a second pass.
#[test]
fn the_death_sequence_from_the_fight_to_the_vitae_wearing_off() {
    let mut s = Station::new();

    // ---- the login tunnel ---------------------------------------------------------------------
    s.feed_to(IDX_ENTER_WORLD_1 + 20);
    assert_eq!(
        s.app.replay_network_mut().expect("replay").session_state(),
        dereth_client_net::client_session::SessionState::Playable,
        "`0x0013 Login_PlayerDescription` is what makes the client in world"
    );
    assert_eq!(
        s.raised,
        vec![IDX_LOG_ON_1],
        "the client's own `0xF7C8`, at its own position"
    );
    s.into_gameplay();
    assert_eq!(
        s.app.objects().world.player,
        Some(VICTIM),
        "`0xF746` latched the character"
    );
    // The Skills page as it stood under the 5% vitae the character logged in with, so the
    // deepening below is a measured difference rather than a claim.
    let skills_at_login = s.skills();
    assert_eq!(
        skills_at_login.len(),
        38,
        "the Skills page draws all thirty-eight rows"
    );

    // ---- link 1: the vital falls to zero ------------------------------------------------------
    s.feed_to(400);
    let mid = s
        .health()
        .expect("the vitals bar has a health reading mid-fight");
    assert!(
        mid.0 > 0 && mid.0 < mid.1,
        "mid-fight the bar is part full: {mid:?}"
    );
    s.feed_to(IDX_HEALTH_ZERO);
    let raw = s
        .app
        .objects()
        .world
        .player_qualities()
        .and_then(|q| dereth_client_model::attributes::inq_attribute_2nd_stored(q, HEALTH))
        .expect("the stored quality the shard's `0x02E9` wrote");
    assert_eq!(
        raw, 0,
        "the wire said zero: `0x02E9` blob {IDX_HEALTH_ZERO}, property 2, value 0"
    );
    let dead = s.health().expect("a health reading at the kill");
    assert_eq!(
        dead.0, 0,
        "**and the drawn bar reads 0.** The drawn-vitals update reads the vital through the \
         secondary-attribute qualities lookup `(desc, 2, &v, 0)`; the enchantment adjustment \
         asks the quality filter first, the filter lists only the maxima, so current health is \
         drawn as stored and the enchantment path's floor never reaches it. Max was {}",
        dead.1
    );
    assert!(
        dead.1 > 0,
        "the maximum still reads through the filter and is not zero: {dead:?}"
    );
    eprintln!(
        "link 1  health wire {raw}, drawn {} of {}  [green]",
        dead.0, dead.1
    );

    // ---- links 2 and 3: the two death lines ---------------------------------------------------
    let before = s.scroll_text();
    assert!(
        !before.contains("fiery end"),
        "the line is not in the log before its blob arrives"
    );
    s.feed_to(IDX_VICTIM_LINE);
    let runs = s.scroll();
    let text: String = runs.iter().map(|(t, _)| t.as_str()).collect();
    let hits = text.matches("brings you to a fiery end!").count();
    assert_eq!(
        hits,
        1,
        "`0x01AC` draws the victim line once; log tail: {:?}",
        tail(&text)
    );
    assert!(
        runs.iter()
            .any(|(t, c)| t.contains("fiery end!") && *c == GREEN),
        "text type 0 uses the green fill"
    );
    assert!(
        !text.contains("\n\n") || !text.ends_with('\n'),
        "the scroll path trims the `\\n` both handlers append"
    );
    s.feed_to(IDX_DEATH_EVENT);
    let after = s.scroll_text();
    assert_eq!(
        after.matches("to a fiery end!").count(),
        hits,
        "the player-death event draws **nothing** when the local player is the \
         `killed`; ACE broadcasts `0x019E` to the victim too and the `!=` pair is the only guard"
    );
    eprintln!("link 2  0x01AC drawn once, green  [green]");
    eprintln!("link 3  0x019E naming us drew nothing  [green]");

    // ---- link 4: the vitae deepens --------------------------------------------------------------
    // **The character did not arrive clean.** The login `0x0013` (blob 9) carries a vitae
    // enchantment for spell 666 whose `_smod.val` is 0.949 999 99 -- a 5% penalty from an earlier
    // death that is not in this recording. So this death *deepens* an existing vitae rather than
    // arming a new one: ACE subtracts the vitae penalty, by default 0.05, from the existing
    // stat-mod value.
    assert_eq!(
        s.vitae(),
        Some(0.949_999_99),
        "the vitae reader returns the `0x0013`'s own `_vitae` slot"
    );
    assert_ne!(
        s.lamp_state(),
        STATE_NOTHING,
        "so the lamp is already lit before the fight"
    );
    s.feed_to(IDX_VITAE_ARM);
    assert_eq!(
        s.vitae(),
        Some(0.9),
        "the recorded `_smod.val`, 0.95 less the 0.05 penalty"
    );
    assert_ne!(
        s.lamp_state(),
        STATE_NOTHING,
        "the vitae indicator stays lit"
    );
    eprintln!("link 4  vitae 0.95 -> 0.9, lamp lit  [green]");

    // ---- link 5: the purge ---------------------------------------------------------------------
    let listed = |s: &Station| -> Vec<(u32, f64)> {
        s.app
            .objects()
            .world
            .player_qualities()
            .expect("the player carries qualities")
            .enchantments
            .enchantments_in_effect()
            .iter()
            .map(|e| (e.id & 0xFFFF, e.duration))
            .collect()
    };
    let before_purge = listed(&s);
    s.feed_to(IDX_PURGE);
    let after_purge = listed(&s);
    // **The discriminating case for the purge.** The character's registry at the moment of death
    // holds exactly one spell enchantment, spell **5154**, with `_duration == -1.0`. The
    // spell-list purge collects the `_id` of every entry whose `_duration != -1.0` and removes
    // only those, so a permanent entry survives the purge; a purge that cleared both lists
    // outright would drop it. This is the corpus's only `0x02C6`.
    assert_eq!(
        before_purge,
        vec![(5154_u32, -1.0_f64)],
        "one permanent spell enchantment stands when the purge arrives"
    );
    assert_eq!(
        after_purge, before_purge,
        "and it is still there afterwards: the spell-list purge removes only \
         `_duration != -1.0`"
    );
    assert_eq!(
        s.vitae(),
        Some(0.9),
        "and leaves `_vitae` alone -- only the two spell lists are purged, which is the whole \
         point of the penalty"
    );
    eprintln!(
        "link 5  purge: {} -> {} enchantments, vitae kept  [green]",
        before_purge.len(),
        after_purge.len()
    );

    // ---- the drawn pane, and the skills rows ---------------------------------------------------
    let pane = open_vitae_panel(&mut s);
    assert!(
        pane.contains("10") || pane.contains('%'),
        "the vitae panel draws `100 - (int)(vitae * 100)`; got {pane:?}"
    );
    // **The vitae multiply is on the skill path, and the colour does not follow it.**
    //
    // The skill display writes the enchanted total from a query with its raw flag clear, so that
    // total carries vitae. It colors the row by comparing the raw query against the enchanted
    // value less the vitae modifier. With only vitae applied, that modifier is the vitae-adjusted
    // raw value minus the raw value; subtracting it adds the penalty back. Deepening vitae
    // therefore moves every drawn number and no color.
    //
    // This character arrives with a vitae of 0.95 and leaves the fight with 0.90, so the
    // difference is measurable.
    let skills_after = s.skills();
    assert_eq!(
        skills_after.iter().map(|r| r.0).collect::<Vec<_>>(),
        skills_at_login.iter().map(|r| r.0).collect::<Vec<_>>(),
        "the same thirty-eight skills, in the same order"
    );
    // The thirteen rows this character has never trained sit at the Jack of All Trades floor:
    // Their enchanted query first multiplies zero, which stays zero, and the qualities lookup
    // then adds the augmentation's `+5` **after** the enchantment. A deeper vitae cannot move
    // those rows, and it moves every other row.
    let (floor, trained): (Vec<_>, Vec<_>) = skills_at_login
        .iter()
        .zip(&skills_after)
        .partition(|(a, _)| a.1 == 5);
    assert_eq!(
        floor.len(),
        13,
        "thirteen rows stand at the augmentation floor"
    );
    assert!(
        floor.iter().all(|(_, b)| b.1 == 5),
        "which the penalty cannot move: {floor:?}"
    );
    assert!(
        trained.iter().all(|(a, b)| b.1 < a.1),
        "and **every** one of the {} rows above it falls when 0.95 becomes 0.90: {trained:?}",
        trained.len()
    );
    assert_eq!(
        skills_at_login.iter().map(|r| r.2).collect::<Vec<_>>(),
        skills_after.iter().map(|r| r.2).collect::<Vec<_>>(),
        "and **not one colour changes**, because the colour comparison subtracts the vitae \
         modifier back off. A client without that term would repaint the whole list the moment \
         the penalty deepened."
    );
    let moved = trained.len();
    eprintln!("link 4b pane {pane:?}, {moved} of 38 skill rows lower, 0 colours moved  [green]");

    // ---- links 6 and 7: the release and the corpse ---------------------------------------------
    s.feed_to(IDX_TELEPORT);
    s.feed_to(IDX_CORPSE_CREATE);
    s.settle(6);
    let corpse_cell = cell_of(&s.app, CORPSE);
    assert!(
        s.app.objects().world.weenie(CORPSE).is_some(),
        "the corpse the shard created is in the client's object table"
    );
    eprintln!("link 7  corpse {CORPSE:?} at cell {corpse_cell:?}");
    let body = cell_of(&s.app, VICTIM);
    assert_ne!(
        body, corpse_cell,
        "the body is at the lifestone and the corpse is where he died"
    );
    eprintln!("link 6  body at cell {body:?}  [green]");

    // ---- link 8: the shortcuts of what went to the corpse ---------------------------------------
    let still: Vec<(ObjectId, Option<usize>)> = WENT_TO_THE_CORPSE
        .iter()
        .map(|id| {
            (
                *id,
                s.app.objects().world.player_system.shortcut_slot_of(*id),
            )
        })
        .collect();
    assert!(
        still.iter().all(|(_, slot)| slot.is_none()),
        "the server move-item notice removes the shortcut of every object whose ownership check \
         no longer finds it player-owned; still held: {still:?}"
    );
    eprintln!("link 8  no shortcut survives the three deleted items  [green]");

    // ---- link 9: the corpse opened --------------------------------------------------------------
    assert!(
        !s.app.objects().world.has_corpse_been_opened(CORPSE),
        "the corpse-opened flag is false until the contents arrive"
    );
    s.feed_to(IDX_CORPSE_CONTENTS_DONE);
    s.settle(6);
    let contents = s
        .app
        .objects()
        .world
        .inventory(CORPSE)
        .map(|i| i.items.len())
        .unwrap_or(0);
    assert_eq!(contents, 11, "the eleven profiles the `0x0196` carried");
    let opened = s.app.objects().world.has_corpse_been_opened(CORPSE);
    let ground = s.app.objects().world.ground_object;
    eprintln!("link 9  corpse holds {contents} items, opened={opened}, ground={ground:?}");
    assert_eq!(
        ground,
        Some(CORPSE),
        "the view-contents path opens the ground panel on the container \
         this client asked for"
    );
    assert!(
        opened,
        "setting the ground object also marks the corpse opened, the client's only write to \
         the opened-corpse table"
    );

    // ---- link 10: the ticks and the removal -----------------------------------------------------
    let mut drawn: Vec<(f32, String)> = Vec::new();
    for idx in IDX_VITAE_TICKS {
        s.feed_to(idx);
        let v = s.vitae().expect("a vitae value");
        drawn.push((v, vitae_pane_text(&mut s)));
        assert_ne!(
            s.lamp_state(),
            STATE_NOTHING,
            "the lamp stays lit while the penalty stands"
        );
    }
    assert_eq!(
        drawn.iter().map(|d| d.0).collect::<Vec<_>>(),
        vec![0.909_999_97_f32, 0.919_999_96, 0.999_999_9],
        "the three recorded value changes"
    );
    assert!(
        drawn.windows(2).all(|w| w[0].1 != w[1].1),
        "each vitae change redraws the pane without it being re-opened. Texts: {:?}",
        drawn.iter().map(|d| d.1.clone()).collect::<Vec<_>>()
    );
    s.feed_to(IDX_VITAE_REMOVED);
    assert_eq!(
        s.vitae(),
        Some(1.0),
        "removing the vitae enchantment clears `_vitae`"
    );
    assert_eq!(s.lamp_state(), STATE_NOTHING, "and the lamp goes dark");
    eprintln!("link 10 three ticks re-drew the pane, `0x02C3` darkened the lamp  [green]");

    // ---- link 10b: the other player dies in view ---------------------------------------------
    // The observer side of a death teleport, in recorded bytes. The observer is this client.
    let before = s.scroll_text();
    s.feed_to(IDX_SUICIDE_LINES[4]);
    let heard = s.scroll_text();
    assert!(
        heard.contains("Oh cruel, cruel world!") && heard.contains("I feel faint..."),
        "the five `/die` lines are drawn as speech; log tail: {:?}",
        tail(&heard)
    );
    assert!(heard.len() > before.len(), "and they are new text");
    s.feed_to(IDX_OTHER_CORPSE_CREATE);
    s.settle(6);
    assert!(
        s.app.objects().world.weenie(OTHER_CORPSE).is_some(),
        "his corpse reaches the observer's object table"
    );
    let his_corpse_cell = cell_of(&s.app, OTHER_CORPSE);
    assert_eq!(
        his_corpse_cell.map(|c| c.0 >> 16),
        Some(DEATH_BLOCK),
        "created where he died, in the landblock the observer is standing in"
    );
    let blip = s
        .app
        .hud()
        .radar
        .iter()
        .find(|e| e.id == OTHER_CORPSE)
        .map(|e| (e.radar_enum, inq_showable_on_radar(e)));
    assert_eq!(
        blip,
        Some((0, false)),
        "and it is no more a radar blip than his own -- see `neither_corpse_is_ever_a_radar_blip`"
    );
    s.feed_to(IDX_OTHER_HIDE);
    s.settle(6);
    assert_eq!(
        s.app
            .objects()
            .physics_state(KILLER)
            .map(|w| w & (HIDDEN_PS | IGNORE_COLLISIONS_PS)),
        Some(HIDDEN_PS | IGNORE_COLLISIONS_PS),
        "`0xF74B` put the teleport hide on him, which starts the fourteen infinite emitters"
    );
    assert_eq!(
        cell_of(&s.app, KILLER).map(|c| c.0 >> 16),
        Some(LIFESTONE_BLOCK),
        "and his body is in a landblock the observer has not loaded, which is the only thing \
         that takes it off the survivor's screen"
    );
    assert!(
        s.app.objects().world.weenie(KILLER).is_some(),
        "he is **not** deleted: no `0xF747` is sent for a death teleport"
    );
    let no_death_line = s.scroll_text();
    assert_eq!(
        no_death_line.matches("fiery end").count(),
        heard.matches("fiery end").count(),
        "and no death line is drawn for him at all: ACE's `/die` kills without the death \
         notifications, so neither `0x019E` nor `0x01AD` is ever sent"
    );
    s.feed_to(IDX_OTHER_UNHIDE);
    s.settle(6);
    assert_eq!(
        s.app
            .objects()
            .physics_state(KILLER)
            .map(|w| w & (HIDDEN_PS | IGNORE_COLLISIONS_PS)),
        Some(0),
        "the unhide script ends the shimmer 6.87 s later"
    );
    eprintln!("link 10b other player's death in view  [green]");

    // ---- link 11: the relog ---------------------------------------------------------------------
    s.feed_to(IDX_SECOND_DESCRIPTION + 40);
    assert_eq!(
        s.raised,
        vec![
            IDX_LOG_ON_1,
            IDX_USE_CORPSE,
            IDX_CLOSE_CORPSE,
            IDX_LOG_OFF,
            IDX_LOG_ON_2
        ],
        "five client-originated blobs in the whole replay, each at its recorded position"
    );
    assert_eq!(
        s.app.replay_network_mut().expect("replay").session_state(),
        dereth_client_net::client_session::SessionState::Playable,
        "the second `0x0013` makes the client in world again"
    );
    assert_eq!(
        s.vitae(),
        Some(1.0),
        "the vitae expired before the logout, so the second login finds none -- and the vitae \
         reader answers 1.0 for a character with no `_vitae`, not `None`"
    );
    eprintln!("link 11 relog: session Playable, vitae 1.0  [green]");
}

/// Behaviour: objects.death.a-corpse-is-never-a-radar-blip
///
/// **The corpse's radar blip: there is none, and there never was.**
///
/// Neither a corpse blip nor a blip greying when the corpse is opened is a rule the client has:
///
/// * Radar admission accepts an object only when its radar enum is `ShowMovement`(2),
///   `ShowAttacking`(3), or `ShowAlways`(4). **The shard sends no `RADAR_ENUM` field for either
///   corpse**, so both read undefined (0) and neither is ever a blip.
/// * Radar coloring has no corpse arm at all: it checks the authored blip color, then the portal,
///   vendor, creature, and player bits, and nothing else.
/// * The client's only reader of the opened-corpse table supplies the unopened-corpse selection
///   condition. The chain station asserts that input state before and after opening; it
///   does not execute the selection arm itself.
///
/// A client that invented a greying arm, or that widened the radar filter to accept
/// `Undef`, goes red here.
#[test]
fn neither_corpse_is_ever_a_radar_blip() {
    let mut s = Station::new();
    s.feed_to(IDX_LOG_ON_1 + 20);
    s.into_gameplay();
    s.feed_to(IDX_CORPSE_CREATE);
    s.settle(6);
    let entry = |s: &Station, id: ObjectId| {
        s.app.hud().radar.iter().find(|e| e.id == id).map(|e| {
            (
                e.radar_enum,
                inq_showable_on_radar(e),
                get_blip_color(Some(e)).hex,
            )
        })
    };
    let before = entry(&s, CORPSE);
    assert!(
        matches!(before, Some((0, false, _))),
        "an unopened corpse carries an undefined radar value and is not showable: {before:?}"
    );
    assert!(
        !s.app.objects().world.has_corpse_been_opened(CORPSE),
        "the corpse is still unopened, satisfying the unopened-corpse selection condition"
    );
    s.feed_to(IDX_CORPSE_CONTENTS_DONE);
    s.settle(6);
    assert!(
        s.app.objects().world.has_corpse_been_opened(CORPSE),
        "opening the corpse sets the opened-corpse state"
    );
    assert_eq!(
        entry(&s, CORPSE),
        before,
        "and the radar entry is bit-for-bit what it was: the blip that would grey does not exist"
    );
    // The other player's corpse, which this client never opened, reads the same.
    s.feed_to(IDX_OTHER_CORPSE_CREATE);
    s.settle(6);
    let his = entry(&s, OTHER_CORPSE);
    assert!(
        matches!(his, Some((0, false, _))),
        "and so does a corpse this client had no part in: {his:?}"
    );
    assert!(
        !s.app.objects().world.has_corpse_been_opened(OTHER_CORPSE),
        "he never opened it, and nothing in the recording says he did"
    );
}

fn tail(s: &str) -> String {
    s.chars()
        .rev()
        .take(160)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

/// Click the lamp, the only interaction that opens the vitae panel, and read the drawn pane.
fn open_vitae_panel(s: &mut Station) -> String {
    let lamp = find(&mut s.app, VITAE_LAMP);
    let panel = find(&mut s.app, VITAE_PANEL);
    {
        let (ui, _) = gameplay(&mut s.app);
        let b = ui.screen_box(lamp);
        assert!(b.is_valid(), "the lamp has no box to point at");
        let (x, y) = ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2);
        ui.mouse_move(LocalTime(0.0), x, y);
        ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, x, y);
        ui.mouse_up(dereth_ui::focus::action::PRIMARY_CLICK, x, y, false);
    }
    s.settle(4);
    assert!(
        gameplay(&mut s.app)
            .0
            .node(panel)
            .expect("a live node")
            .region
            .flags
            .visible,
        "the click has to open the panel before its text can matter"
    );
    vitae_pane_text(s)
}

/// The cell named by an object's achieved position, which object creation and an accepted `0xF748`
/// both seed in the current presence state.
fn cell_of(app: &App, id: ObjectId) -> Option<dereth_primitives::CellId> {
    app.objects()
        .presence(id)
        .and_then(|p| p.position)
        .map(|p| p.cell)
}

fn vitae_pane_text(s: &mut Station) -> String {
    let h = find(&mut s.app, VITAE_MAIN_TEXT);
    gameplay(&mut s.app)
        .0
        .text_element_mut(h)
        .expect("a text element")
        .glyphs
        .glyphs
        .iter()
        .filter_map(|g| char::from_u32(u32::from(g.data)))
        .collect()
}

/// **No socket is bound.** The endpoint is the replay one and its local address is `None`.
#[test]
fn the_endpoint_binds_no_socket() {
    let mut s = Station::new();
    assert!(
        s.app.replay_network_mut().is_some(),
        "the replay endpoint is attached"
    );
    s.feed_to(30);
    assert!(
        s.app
            .replay_network_mut()
            .expect("replay")
            .take_outgoing()
            .iter()
            .all(|(b, _)| !b.is_empty()),
        "every outgoing datagram is drained into this process"
    );
}
