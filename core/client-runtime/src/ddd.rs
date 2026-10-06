//! The client cache's DDD (dynamic data download) half: receive, validate, and store a dat patch.
//!
//! The writer is `dereth_dat::write::DatWriter`; this is the protocol on top of it.
//! `dereth_client_net::client_session` decodes the five messages and hands them over as
//! [`dereth_client_net::client_session::DddEvent`]; [`crate::app::App`] routes each one here.
//!
//! # The native state machine
//!
//! One switch on the leading opcode dispatches every blob on queue 5
//! (the client-cache queue). The switch covers
//! `0xF7E2 .. 0xF7EB` (ten cases, indexed by `et - 0xF7E2`):
//!
//! | opcode | name | dispatch behavior |
//! |---|---|---|
//! | `0xF7E2` | `DDD_DataMessage` | deserialize; during interrogation track the early resource and compressed size, queue its save, then report data downloaded |
//! | `0xF7E4` | `DDD_ErrorMessage` | Fail the outstanding asynchronous get. **Nothing is written.** |
//! | `0xF7E5` | `DDD_InterrogationMessage` | answer with `0xF7E6` |
//! | `0xF7E7` | `DDD_BeginDDDMessage` | enter patching state, retain missing iterations, report expected bytes minus early saves |
//! | `0xF7EA` | `DDD_OnEndDDD` | finish the patch |
//! | `0xF7EB` | `DDD_EndDDDMessage` | report patching pending — "wait", *not* "end" |
//!
//! `0xF7E3 DDD_RequestDataMessage` is **not** part of this exchange at all. Its only producer is
//! the asynchronous-cache hook for "the disk controller
//! does not have this object" — a run-time cache miss, which is why ACE registers its handler on
//! `SessionState.WorldConnected` and answers only `LandBlock`, `LandBlockInfo` and `EnvCell`.
//! [`dereth_client_net::client_session::Session::request_ddd_data`] is that producer; see `request_message`.
//!
//! # What a patch does to the dat
//!
//! Asynchronous message saving queues a save request. The cache worker builds the disk entry
//! from the request and calls the data writer. The field mapping, checked against retail: the
//! version is shifted left 16 bits into the entry's flag word, the resource id becomes the entry's
//! id, and the iteration becomes the entry's iteration. The worker also passes the save flags and
//! cache payload.
//!
//! The message's compressed flag selects save flags `0xC` when set and `1` otherwise. The writer switches on
//! `flags & 7`, so that is **case 4 (decompress) or case 1 (store as-is)** — never case 2 or 3, the
//! compressing ones. A patched record therefore lands uncompressed, exactly like every one of
//! the 887,455 records already in the retail files.
//!
//! # Where a patch goes: the world's overlay
//!
//! **A deliberate divergence from retail** (client divergence CD-031). Retail saves every record
//! into its own data files. This client never writes the files a player installed: every record,
//! purge and iteration a server sends goes into that world's overlay folder
//! ([`dereth_dat::overlay`]), one container per file, and the world is read through the overlay
//! laid over the locked files. A purge is a tombstone there, not a deletion: a landblock purge
//! hides the whole landblock family, a record purge one record. The iterations the client reports
//! are the locked files' and the overlay's together, so a server's ordinary patch logic carries an
//! overlay's revisions as it would retail's.
//!
//! **The overlay extension.** With a server that knows it (Empyrean), the client receives the
//! world's overlay manifest (`0xF7EC`) before the patch: the world's name, the base each file's
//! overlay was made against, and every record's SHA-256 and every deletion. The client refuses an
//! overlay made against another base, a world on its blocklist, and a folder holding another
//! world's overlay, and it refuses any record whose bytes are not the ones the manifest names.
//! Without the extension (a stock ACE server) the patch is retail's, into the overlay all the same.
//!
//! # The safety rails that are not native's
//!
//! All deliberate, all reported rather than silent:
//!
//! 1. The locked files are never opened for writing; the patch's only writer is the overlay's.
//!    `DatWriter::open` also refuses any path in a directory declared read-only with
//!    `dereth_dat::protect_install`, which the client and headless binaries declare their data
//!    folders to be.
//! 2. A record's `DataID` must route to the dat file the message names it for — a portal id
//!    offered for the language dat is refused. Native checks nothing: the save is handed the
//!    disk controller the qualified id selects and stores whatever it is given.
//! 3. A compressed flag byte that is neither 0 nor 1 is refused. That one *is* native, in a
//!    roundabout way: archive decoding raises an error for any other value, and the per-frame
//!    update's guard, which saves only while flag bit 2 is clear, then skips the save *and* the
//!    notify.
//! 4. The overlay's own records (`0xFFFF0002` tombstones, `0xFFFF0003` manifest) are never taken
//!    from the wire.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use dereth_dat::overlay::{OverlayDir, OverlayError, OverlayWriter};
use dereth_dat::write::SaveOutcome;
use dereth_dat::{DatError, DatFile, DatKind, RetailDatStore};
use dereth_primitives::DataId;
use dereth_protocol::admin::{DddBeginDdd, DddData, DddError, DddOverlayManifest, DddRequestData};

/// The `0xFFFF0000` mask used for a type-1 purge, which removes the whole landblock
/// family containing the named record.
pub const LANDBLOCK_MASK: u32 = 0xFFFF_0000;

/// Which of the four containers a message's dat-file type and id pair names: the retail file
/// itself.
pub use dereth_dat::ModernDat as DatTarget;

/// `DDDManager.HiFi_String_As_Int` — the four bytes of `"HiFi"` read as a little-endian int.
pub const HIFI: u32 = u32::from_le_bytes(*b"HiFi");

/// The file a wire `(type, id)` pair names, or `None` for a pair no dat file answers to.
///
/// The pairs are `MasterDBMap`'s and are the same on both sides of the wire: ACE's
/// `GameMessageDDDDataMessage` writes exactly these
/// (ACE's `Source/ACE.Server/Network/GameMessages/Messages/GameMessageDDDDataMessage.cs`),
/// and `DDDHandler.DDD_InterrogationResponse` reads them back the same way: `(0, 1)` is
/// `client_portal.dat`, `(1, 2)` `client_cell_1.dat`, `(1, 3)` `client_local_English.dat` and
/// `("HiFi", 1)` `client_highres.dat`.
#[must_use]
pub fn target_from_wire(dat_file_type: u32, dat_file_id: u32) -> Option<DatTarget> {
    match (dat_file_type, dat_file_id) {
        (0, 1) => Some(DatTarget::Portal),
        (1, 2) => Some(DatTarget::Cell),
        (1, 3) => Some(DatTarget::Local),
        (HIFI, 1) => Some(DatTarget::HighRes),
        _ => None,
    }
}

/// Why a `DDD_DataMessage` was not written.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DddRefusal {
    /// The revision's `(dat_file_type, dat_file_id)` names no file this client has open. Native
    /// skips the revision entirely (the begin-request worker accepts only file types below 4).
    UnknownDatFile {
        dat_file_type: u32,
        dat_file_id: u32,
    },
    /// Cache-record deserialization windows exactly `data_size` bytes *including*
    /// the size dword, so the payload that follows must be `data_size - 4` long. Anything else and
    /// reading the payload fails, the archive error latch trips, and the per-frame update drops
    /// the message.
    BadLength { declared: u32, actual: usize },
    /// Object serialization accepts only 0 or 1.
    BadCompressedFlag(u8),
    /// Saving a data record refuses version 0 outright.
    ZeroVersion,
    /// The directory entry's version field is 16 bits; retail carries 1, 2 or 3.
    VersionTooLarge(u32),
    /// A safety rail, not native: the id does not belong in the file the message named.
    WrongDatFile {
        id: u32,
        named: DatTarget,
        belongs: Option<DatKind>,
    },
    /// Disk decompression answered false.
    Decompress(String),
    /// One of the overlay's own records, which no server may send.
    ReservedId(u32),
    /// The record's bytes are not the ones the world's overlay manifest names.
    HashMismatch { id: u32 },
    /// The world's overlay was refused (blocked, made against another base, another world's
    /// folder, or no overlay folder at all), so nothing of this world is written.
    OverlayRefused(String),
    /// `DatWriter::save` refused or failed — including [`DatError::RetailDatRefused`], which is
    /// what a normal run gets for every record.
    Write(String),
}

impl std::fmt::Display for DddRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownDatFile {
                dat_file_type,
                dat_file_id,
            } => {
                write!(
                    f,
                    "no dat file is open for ({dat_file_type:#X}, {dat_file_id})"
                )
            }
            Self::BadLength { declared, actual } => write!(
                f,
                "the record declares {declared} bytes (payload {}) and carries {actual}",
                declared.saturating_sub(4)
            ),
            Self::BadCompressedFlag(b) => write!(f, "the compressed flag is {b}, not 0 or 1"),
            Self::ZeroVersion => {
                write!(f, "the record has version 0, which the dat writer refuses")
            }
            Self::VersionTooLarge(v) => write!(
                f,
                "version {v} does not fit the directory entry's version field"
            ),
            Self::WrongDatFile { id, named, belongs } => {
                write!(
                    f,
                    "{id:#010X} belongs in {belongs:?}, not in {}",
                    named.file_name()
                )
            }
            Self::ReservedId(id) => write!(f, "{id:#010X} is one of the overlay's own records"),
            Self::HashMismatch { id } => write!(
                f,
                "{id:#010X} is not the record the world's overlay manifest names"
            ),
            Self::OverlayRefused(why) => write!(f, "the world's overlay is refused: {why}"),
            Self::Decompress(e) | Self::Write(e) => write!(f, "{e}"),
        }
    }
}

/// What [`DddPatcher::on_data`] did with one `DDD_DataMessage`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DataOutcome {
    /// The record is in the dat. `bytes` is the wire payload length: the compressed size
    /// reported by the data message to the DDD event notification.
    Applied {
        id: DataId,
        target: DatTarget,
        bytes: usize,
        save: SaveOutcome,
    },
    /// The save's never-downgrade rule fired: the stored iteration is newer. Native reports this
    /// as **success**, and so does this.
    RefusedOlderIteration { id: DataId, target: DatTarget },
    /// The server sent the file's iteration list (`0xFFFF0001`) as one of the revision's
    /// downloads. It counts as delivered and its bytes are not written: the list this client
    /// keeps is its own, added to as each revision completes. Native saves the server's copy and
    /// then writes its own list over it when the revision completes; keeping the server's bytes
    /// out means an interrupted patch cannot leave the file claiming iterations it never
    /// received.
    IterationListKept { target: DatTarget },
    /// Nothing was written.
    Refused(DddRefusal),
}

impl DataOutcome {
    /// Did the dat change?
    #[must_use]
    pub fn wrote(&self) -> bool {
        matches!(self, Self::Applied { .. })
    }
}

/// The name misspells "Received" exactly as the original does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DddPhase {
    /// Nothing has happened yet.
    Idle,
    /// `0xF7E5` arrived and `0xF7E6` went back. Records that arrive now are *early saves*.
    InterrogationReceved,
    /// `0xF7E7` arrived; the missing-iterations list is populated.
    Patching,
    /// The client has sent its own `0xF7EA`.
    EndSent,
    /// Patching is over.
    RunTime,
}

/// What the caller must do after handing an event over.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DddAction {
    /// Send `0xF7EA DDD_OnEndDDD`. Retiring the last pending download does this when
    /// the pending-downloads list empties, and begin-request completion does
    /// it when there was nothing to download in the first place.
    pub send_end: bool,
}

/// One revision the server says this client is missing.
#[derive(Debug, Clone)]
struct Revision {
    target: DatTarget,
    iteration: u32,
    /// How many of `IDsToDownload` have not landed yet.
    outstanding: usize,
    /// Set once the iteration has been added to the file's `0xFFFF0001` set.
    recorded: bool,
    /// This revision's purges could not be performed, so its iteration must **not** be recorded
    /// however many downloads land. The iteration set is what the server reads next login to
    /// decide what to send, so recording an iteration whose purges never happened would tell it a
    /// revision landed that did not, and it would never offer the revision again.
    ///
    /// It is set only when a purge fails -- which in a normal run is every one of them, because
    /// `DatWriter::open` refuses the retail client directory outright.
    blocked: bool,
}

/// What the whole exchange did, for the log line and for the caller's invalidation decision.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DddSummary {
    /// Records written.
    pub applied: u32,
    /// Records refused, for any reason.
    pub refused: u32,
    /// Records the server's iteration rule made a no-op.
    pub stale: u32,
    /// Files whose bytes changed. Empty means no invalidation is needed at all.
    pub changed: Vec<DatTarget>,
    /// Downloads the server promised in `0xF7E7` and never sent.
    pub still_pending: usize,
}

impl DddSummary {
    /// Is there anything for a reader to re-read?
    #[must_use]
    pub fn changed_anything(&self) -> bool {
        !self.changed.is_empty()
    }
}

/// Where a world's patch is written: its overlay folder, over the files the world reads.
#[derive(Debug, Clone)]
pub struct OverlayTarget {
    dir: OverlayDir,
    /// Each file the world reads, as it is on disk, and its name: what that file's overlay
    /// container is made against.
    bases: BTreeMap<DatTarget, (DatFile, String)>,
    /// The world the overlay belongs to: the manifest's name for it, else the folder's, else the
    /// name the client knows the server by.
    world_key: String,
}

impl OverlayTarget {
    /// The overlay folder `dir` over the files `store` reads, for the world `world_key` (the
    /// folder's own world when it already holds an overlay).
    #[must_use]
    pub fn new(dir: OverlayDir, store: &RetailDatStore, world_key: &str) -> Self {
        let world_key = dir.world_key().unwrap_or_else(|| world_key.to_owned());
        let mut me = Self {
            dir,
            bases: BTreeMap::new(),
            world_key,
        };
        me.set_bases(store);
        me
    }

    /// The overlay folder `dir` over the given base files, for the world `world_key`.
    #[must_use]
    pub fn with_bases(
        dir: OverlayDir,
        bases: impl IntoIterator<Item = (DatTarget, DatFile)>,
        world_key: &str,
    ) -> Self {
        let world_key = dir.world_key().unwrap_or_else(|| world_key.to_owned());
        let bases = bases
            .into_iter()
            .map(|(t, f)| {
                let name = f
                    .path()
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                (t, (f.base(), name))
            })
            .collect();
        Self {
            dir,
            bases,
            world_key,
        }
    }

    /// Read the base files again from `store` (after it was reopened, or the high-resolution
    /// file was granted).
    pub fn set_bases(&mut self, store: &RetailDatStore) {
        self.bases.clear();
        for t in DatTarget::ALL {
            if let Some(f) = store.target_file(t) {
                let name = f
                    .path()
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.bases.insert(t, (f.base(), name));
            }
        }
    }

    #[must_use]
    pub fn dir(&self) -> &OverlayDir {
        &self.dir
    }

    #[must_use]
    pub fn world_key(&self) -> &str {
        &self.world_key
    }

    /// The base file `target` names, as it is on disk.
    #[must_use]
    pub fn base(&self, target: DatTarget) -> Option<&DatFile> {
        self.bases.get(&target).map(|(f, _)| f)
    }
}

/// The client cache's DDD state, its writers, and the bookkeeping the protocol needs.
///
/// One per client. Holds an overlay writer per file it has actually had to write to, opened
/// lazily — so a session with no patch never opens a container for writing at all, and the common
/// case costs nothing.
#[derive(Debug)]
pub struct DddPatcher {
    target: Option<OverlayTarget>,
    /// Why this world's overlay is refused, when it is: nothing is written and the patch ends at
    /// once.
    overlay_refused: Option<String>,
    /// The record hashes the world's overlay manifest names, by file and id.
    expected: BTreeMap<(DatTarget, u32), [u8; 32]>,
    /// The single-record deletions the manifest names, by file and revision.
    manifest_tombstones: BTreeMap<(DatTarget, u32), Vec<(u32, u32)>>,
    /// The world keys whose overlays the player refuses.
    blocklist: BTreeSet<String>,
    /// The overlay containers this patcher made, which a refusal takes away again: they hold only
    /// what this session wrote.
    created: BTreeSet<DatTarget>,
    /// The records written before the manifest arrived (they may come ahead of it), checked
    /// against it when it does.
    unchecked: Vec<(DatTarget, u32)>,
    /// Whether the manifest has arrived.
    manifest_seen: bool,
    /// The files whose overlay carries the world's own iteration list, by the manifest: their
    /// records are the world's whatever iteration the base has of them.
    exact: BTreeSet<DatTarget>,
    phase: DddPhase,
    writers: BTreeMap<DatTarget, OverlayWriter>,
    /// The missing iterations.
    revisions: Vec<Revision>,
    /// The pending downloads, keyed the way the wire keys it: the id, per file. Native's key is the
    /// whole `QualifiedDataID`, but the id serializer shows the type is **derived locally**
    /// and never sent, so the id is the whole of the wire key.
    pending: BTreeMap<(DatTarget, u32), usize>,
    /// The early saves — records that arrived between the interrogation and `0xF7E7`.
    early: Vec<(DatTarget, u32)>,
    /// The early-save byte count, which the begin handler subtracts from the byte count it shows
    /// the player.
    early_bytes: u64,
    /// The run-time gets this client has a `0xF7E3` outstanding
    /// for, keyed by `QualifiedDataID` exactly as native's hash is.
    gets: BTreeSet<(u32, u32)>,
    /// Records that arrived as the answer to one of those gets, for the caller's re-seed.
    resupplied: Vec<DataId>,
    /// Gets a `0xF7E4` failed, for the caller to forget so the next miss can ask again.
    failed: Vec<DataId>,
    changed: BTreeSet<DatTarget>,
    applied: u32,
    refused: u32,
    stale: u32,
    notices: Vec<String>,
}

impl DddPatcher {
    /// A patcher writing into `target`, the world's overlay; with none, every record is refused.
    #[must_use]
    pub fn new(target: Option<OverlayTarget>) -> Self {
        Self {
            target,
            overlay_refused: None,
            expected: BTreeMap::new(),
            manifest_tombstones: BTreeMap::new(),
            blocklist: BTreeSet::new(),
            created: BTreeSet::new(),
            unchecked: Vec::new(),
            manifest_seen: false,
            exact: BTreeSet::new(),
            phase: DddPhase::Idle,
            writers: BTreeMap::new(),
            revisions: Vec::new(),
            pending: BTreeMap::new(),
            early: Vec::new(),
            early_bytes: 0,
            gets: BTreeSet::new(),
            resupplied: Vec::new(),
            failed: Vec::new(),
            changed: BTreeSet::new(),
            applied: 0,
            refused: 0,
            stale: 0,
            notices: Vec::new(),
        }
    }

    /// The overlay folder patches go to, when there is one.
    #[must_use]
    pub fn dir(&self) -> Option<&Path> {
        self.target.as_ref().map(|t| t.dir.path())
    }

    /// Where patches go.
    #[must_use]
    pub fn target(&self) -> Option<&OverlayTarget> {
        self.target.as_ref()
    }

    /// Read the base files again from `store`.
    pub fn set_bases(&mut self, store: &RetailDatStore) {
        if let Some(t) = &mut self.target {
            t.set_bases(store);
        }
    }

    /// The world keys whose overlays are refused.
    pub fn set_blocklist(&mut self, keys: impl IntoIterator<Item = String>) {
        self.blocklist = keys.into_iter().collect();
        if let Some(t) = &self.target {
            if self.blocklist.contains(&t.world_key) {
                self.refuse(format!(
                    "the world {:?} is on the overlay blocklist",
                    t.world_key
                ));
            }
        }
    }

    /// Whether this run keeps the world's overlay: it has a folder to write the world's patches
    /// into. A run with none refuses every patch.
    #[must_use]
    pub fn keeps_overlay(&self) -> bool {
        self.target.is_some()
    }

    /// Why this world's overlay is refused, when it is.
    #[must_use]
    pub fn refused(&self) -> Option<&str> {
        self.overlay_refused.as_deref()
    }

    /// The file a patch target names, as this client holds it (`portal.dat` beside an older
    /// world, `client_portal.dat` beside a later one).
    fn name_of(&self, target: DatTarget) -> String {
        self.target
            .as_ref()
            .and_then(|t| t.bases.get(&target))
            .map_or_else(|| target.file_name().to_owned(), |(_, n)| n.clone())
    }

    fn refuse(&mut self, why: String) {
        if self.overlay_refused.is_none() {
            tracing::warn!(
                "DDD the world's overlay is refused and nothing of it is written: {why}"
            );
            self.notices
                .push(format!("the world's overlay is refused: {why}"));
            self.overlay_refused = Some(why);
            self.writers.clear();
            // What this session made of the overlay goes with it.
            if let Some(t) = &self.target {
                for c in std::mem::take(&mut self.created) {
                    let _ = t.dir.remove(c);
                }
            }
        }
    }

    /// `0xF7EC`, the world's overlay manifest: check it names this client's bases and a world it
    /// takes, and keep its record hashes and deletions for the patch that follows.
    pub fn on_manifest(&mut self, m: &DddOverlayManifest) {
        if self.blocklist.contains(&m.world_key) {
            self.refuse(format!(
                "the world {:?} is on the overlay blocklist",
                m.world_key
            ));
            return;
        }
        self.manifest_seen = true;
        let Some(target) = &mut self.target else {
            self.refuse("there is no overlay folder for this world".into());
            return;
        };
        if let Some(found) = target.dir.world_key() {
            // Records may arrive ahead of the manifest. The containers they went into carry the
            // name the client knew the world by; when this session made every one of them, the
            // world's own name is theirs.
            let ours = found == target.world_key
                && target
                    .dir
                    .containers()
                    .iter()
                    .all(|p| self.created.iter().any(|c| target.dir.container(*c) == *p));
            if found != m.world_key && ours {
                for w in self.writers.values_mut() {
                    w.set_world_key(&m.world_key);
                }
            } else if found != m.world_key {
                let why = format!(
                    "{} holds the overlay of the world {found:?}, and this world is {:?}",
                    target.dir.path().display(),
                    m.world_key
                );
                self.refuse(why);
                return;
            }
        }
        target.world_key.clone_from(&m.world_key);
        let mut mismatch = None;
        for f in &m.files {
            let Some(t) = target_from_wire(f.dat_file_type, f.dat_file_id) else {
                continue;
            };
            let Some((base, name)) = target.bases.get(&t) else {
                continue;
            };
            let have = dereth_dat::overlay::fingerprint(base);
            if have != f.base_fingerprint {
                mismatch = Some(format!(
                    "the world's {name} overlay was made against {} {}, and this client holds {name} {}",
                    f.base_name,
                    dereth_dat::overlay::hex(&f.base_fingerprint),
                    dereth_dat::overlay::hex(&have)
                ));
                break;
            }
            if f.exact_iterations {
                self.exact.insert(t);
            }
            for r in &f.records {
                self.expected.insert((t, r.id), r.sha256);
            }
            for x in &f.tombstones {
                self.manifest_tombstones
                    .entry((t, x.iteration))
                    .or_default()
                    .push((x.id, x.mask));
            }
        }
        if let Some(why) = mismatch {
            self.refuse(why);
            return;
        }
        // The records that came ahead of the manifest are held to it now.
        let forged = self.unchecked.iter().find(|(t, id)| {
            let want = self.expected.get(&(*t, *id));
            let have = self.writers.get(t).and_then(|w| w.hash_of(DataId(*id)));
            want.is_some_and(|w| have != Some(*w))
        });
        if let Some((t, id)) = forged.copied() {
            self.refuse(format!(
                "{id:#010X} of {} is not the record the world's overlay manifest names",
                self.name_of(t)
            ));
            return;
        }
        self.unchecked.clear();
        self.note(format!(
            "0xF7EC the world {:?} names {} file(s), {} record(s) and {} deletion(s)",
            m.world_key,
            m.files.len(),
            self.expected.len(),
            self.manifest_tombstones
                .values()
                .map(Vec::len)
                .sum::<usize>()
        ));
    }

    #[must_use]
    pub fn phase(&self) -> DddPhase {
        self.phase
    }

    /// Downloads promised and not yet delivered.
    #[must_use]
    pub fn pending(&self) -> usize {
        self.pending.len()
    }

    #[must_use]
    pub fn applied(&self) -> u32 {
        self.applied
    }

    /// Every refusal, in arrival order, as the line a log wants. Never cleared: a session that
    /// refused a patch should still be able to say so at the end of it.
    #[must_use]
    pub fn notices(&self) -> &[String] {
        &self.notices
    }

    /// `0xF7E5` was answered. Records that arrive from here to `0xF7E7` are early saves.
    pub fn on_interrogation(&mut self) {
        self.phase = DddPhase::InterrogationReceved;
    }

    /// Answer whether this `QualifiedDataID` needs a `0xF7E3` sent
    /// for it, and remember that one was.
    ///
    /// `false` means a get is already outstanding for it, which is what
    /// finding a hit in the asynchronous cache's pending-get table means:
    /// the new caller attaches to the existing request and no second
    /// message goes out.
    pub fn request_once(&mut self, resource_type: u32, id: DataId) -> bool {
        self.gets.insert((resource_type, id.raw()))
    }

    /// The run-time gets still outstanding.
    #[must_use]
    pub fn outstanding_gets(&self) -> usize {
        self.gets.len()
    }

    /// Records that arrived as the answer to a run-time get since the last drain, for the caller
    /// to hand to whatever cached the miss.
    #[must_use]
    pub fn take_resupplied(&mut self) -> Vec<DataId> {
        std::mem::take(&mut self.resupplied)
    }

    /// Run-time gets a `0xF7E4` failed since the last drain.
    ///
    /// Asynchronous-get failure takes the request out of the pending-gets list, but
    /// that on its own only lets a *new* get be issued -- something still has to ask. Here the
    /// asker is a cache that remembers the miss, so the caller has to tell it to forget:
    /// [`dereth_world_data::land_source::DatLandSource::forget_blocks`].
    #[must_use]
    pub fn take_failed_gets(&mut self) -> Vec<DataId> {
        std::mem::take(&mut self.failed)
    }

    /// The cache's begin and request-finished handlers use this state.
    ///
    /// Returns the byte count the UI should show — expected bytes minus early-save bytes, the
    /// subtraction the begin handler performs — together with whether the client owes a `0xF7EA`
    /// already, which it does whenever there is nothing left to wait for.
    pub fn on_begin(&mut self, m: &DddBeginDdd) -> (u64, DddAction) {
        self.phase = DddPhase::Patching;
        self.revisions.clear();
        self.pending.clear();
        if self.overlay_refused.is_some() {
            // Nothing of a refused world is written, so nothing is waited for either: the patch
            // ends now and the world is played over the locked files alone.
            self.phase = DddPhase::EndSent;
            return (0, DddAction { send_end: true });
        }

        for rev in &m.revisions {
            let Some(target) = target_from_wire(rev.dat_file_type, rev.dat_file_id) else {
                // The begin-request worker accepts only file types below 4: a revision for a file
                // this client has not opened is skipped whole.
                self.note(format!(
                    "0xF7E7 names ({:#X}, {}), which is no dat file this client has open",
                    rev.dat_file_type, rev.dat_file_id
                ));
                continue;
            };
            // The begin-request worker performs the revision's purges *before*
            // the revision can finish; its completion handler then updates the
            // iteration bookkeeping.
            let mut blocked = !self.purge(target, rev.iteration, &rev.ids_to_purge);
            // The deletions only the manifest can say (a single interior cell), in the revision
            // that made them.
            if let Some(mut more) = self.manifest_tombstones.remove(&(target, rev.iteration)) {
                // Those the revision's purges already wrote are not written twice.
                let mask = if target == DatTarget::Cell {
                    LANDBLOCK_MASK
                } else {
                    0
                };
                more.retain(|(id, m)| {
                    *m != mask
                        || !rev.ids_to_purge.iter().any(|p| {
                            if mask == 0 {
                                p == id
                            } else {
                                p & mask == id & mask
                            }
                        })
                });
                blocked |= !self.tombstone_all(target, rev.iteration, &more);
            }
            let index = self.revisions.len();
            self.revisions.push(Revision {
                target,
                iteration: rev.iteration,
                outstanding: rev.ids_to_download.len(),
                recorded: false,
                blocked,
            });
            for id in &rev.ids_to_download {
                self.pending.insert((target, *id), index);
            }
        }

        // The end of the begin-DDD request: anything already saved during the interrogation
        // window is not pending.
        for key in self.early.clone() {
            self.retire(key);
        }
        // The same function's head: a revision with an empty download list is complete the moment
        // it arrives, so its iteration goes into the file's set now.
        for i in 0..self.revisions.len() {
            if self.revisions[i].outstanding == 0 {
                self.record_iteration(i);
            }
        }

        tracing::info!(
            "DDD 0xF7E7 {} revision(s), {} download(s) pending, {} byte(s) expected",
            self.revisions.len(),
            self.pending.len(),
            m.data_expected
        );
        let expected = u64::from(m.data_expected).saturating_sub(self.early_bytes);
        (
            expected,
            DddAction {
                send_end: self.pending.is_empty(),
            },
        )
    }

    /// `0xF7E2 DDD_DataMessage`: validate the record and put it in the dat.
    pub fn on_data(&mut self, m: &DddData) -> (DataOutcome, DddAction) {
        let outcome = self.apply(m);
        let mut action = DddAction::default();
        match &outcome {
            DataOutcome::Applied {
                id, target, bytes, ..
            } => {
                self.applied += 1;
                self.changed.insert(*target);
                if !self.manifest_seen {
                    self.unchecked.push((*target, id.raw()));
                }
                // Save completion retires the request the record answers.
                // A record that answered a *run-time* get (`0xF7E3`) also has to reach whatever
                // cached the miss, which is what `take_resupplied` is for.
                if self.retire_get(*id) {
                    self.resupplied.push(*id);
                }
                if self.phase == DddPhase::InterrogationReceved {
                    self.early.push((*target, id.raw()));
                    self.early_bytes += *bytes as u64;
                }
                let key = (*target, id.raw());
                if self.retire(key) && self.pending.is_empty() && self.phase == DddPhase::Patching {
                    self.phase = DddPhase::EndSent;
                    action.send_end = true;
                }
            }
            DataOutcome::RefusedOlderIteration { id, target } => {
                self.stale += 1;
                let key = (*target, id.raw());
                if self.retire(key) && self.pending.is_empty() && self.phase == DddPhase::Patching {
                    self.phase = DddPhase::EndSent;
                    action.send_end = true;
                }
            }
            DataOutcome::IterationListKept { target } => {
                let key = (*target, dereth_dat::ITERATION_LIST.raw());
                if self.retire(key) && self.pending.is_empty() && self.phase == DddPhase::Patching {
                    self.phase = DddPhase::EndSent;
                    action.send_end = true;
                }
            }
            DataOutcome::Refused(r) => {
                self.refused += 1;
                self.note(format!("0xF7E2 refused: {r}"));
            }
        }
        (outcome, action)
    }

    /// `0xF7E4 DDD_ErrorMessage`: the cache's per-frame update reports the asynchronous get as
    /// failed — the outstanding get fails and **no byte is
    /// written**. The pending download stays pending, which is why a server that answers every
    /// request with `0xF7E4` leaves the client on the patch screen.
    pub fn on_error(&mut self, e: &DddError) {
        // Asynchronous-get failure takes the request out of the pending-gets list,
        // which is what lets the client ask again for the same id later. Without this a server
        // that answered one `0xF7E3` with `0xF7E4` would be asked once and never again for the
        // rest of the session.
        if self.gets.remove(&(e.resource_type, e.resource_id)) {
            self.failed.push(DataId(e.resource_id));
        }
        self.note(format!(
            "0xF7E4 for type {:#X} id {:#010X}: error {} -- the get fails and nothing is written",
            e.resource_type, e.resource_id, e.error
        ));
    }

    /// Finish the patch: drop the missing iterations, pending downloads and
    /// early saves, move to [`DddPhase::RunTime`], and tell the caller what changed so it can
    /// invalidate.
    ///
    /// The `0xF7EA` reply native sends from here is `dereth_client_net::client_session`'s
    /// (`dispatch::database::dispatch`'s `send_end`, gated on the interrogation-received flag exactly
    /// as retail's end-of-DDD handler gates it); this half only closes the books.
    pub fn on_end(&mut self) -> DddSummary {
        let date = entry_date(crate::platform::clock::system_unix_time());
        let mut failed = Vec::new();
        for w in self.writers.values_mut() {
            if let Err(e) = w.flush(date) {
                failed.push(format!("{}: {e}", w.path().display()));
            }
        }
        for line in failed {
            self.note(format!("the overlay could not be written: {line}"));
        }
        let summary = DddSummary {
            applied: self.applied,
            refused: self.refused,
            stale: self.stale,
            changed: self.changed.iter().copied().collect(),
            still_pending: self.pending.len(),
        };
        self.revisions.clear();
        self.pending.clear();
        self.early.clear();
        self.early_bytes = 0;
        // Close every writer. The reader reopens the file in `App::invalidate_after_ddd`, and a
        // writer still holding the handle is the kind of thing that works on this platform and
        // not on the next one.
        self.writers.clear();
        self.expected.clear();
        self.manifest_tombstones.clear();
        self.phase = DddPhase::RunTime;
        summary
    }

    // -----------------------------------------------------------------------------------------
    // Internals.
    // -----------------------------------------------------------------------------------------

    /// The validate-then-write core, with no bookkeeping, so the bookkeeping above reads as one
    /// thing and this reads as the other.
    fn apply(&mut self, m: &DddData) -> DataOutcome {
        let Some(target) = target_from_wire(m.dat_file_type, m.dat_file_id) else {
            return DataOutcome::Refused(DddRefusal::UnknownDatFile {
                dat_file_type: m.dat_file_type,
                dat_file_id: m.dat_file_id,
            });
        };
        if m.compressed > 1 {
            return DataOutcome::Refused(DddRefusal::BadCompressedFlag(m.compressed));
        }
        if u64::from(m.data_size) != m.data.len() as u64 + 4 {
            return DataOutcome::Refused(DddRefusal::BadLength {
                declared: m.data_size,
                actual: m.data.len(),
            });
        }
        if m.version == 0 {
            return DataOutcome::Refused(DddRefusal::ZeroVersion);
        }
        let Ok(version) = u16::try_from(m.version) else {
            return DataOutcome::Refused(DddRefusal::VersionTooLarge(m.version));
        };
        let id = DataId(m.resource_id);
        if id == dereth_dat::ITERATION_LIST && target != DatTarget::Cell {
            return DataOutcome::IterationListKept { target };
        }
        if dereth_dat::overlay::is_reserved(id) {
            return DataOutcome::Refused(DddRefusal::ReservedId(id.raw()));
        }
        if let Some(why) = &self.overlay_refused {
            return DataOutcome::Refused(DddRefusal::OverlayRefused(why.clone()));
        }
        // A cell id is keyed by its landblock, and a landblock's number can fall in a portal
        // type's range (`0x2562xxxx` is a landblock and also a portal id map's range): for the
        // cell file the id's shape decides.
        let belongs = if target == DatTarget::Cell {
            dereth_dat::classify_cell_id(id).map(|t| t.dat())
        } else {
            id_home(id)
        };
        if belongs != Some(target.kind()) {
            return DataOutcome::Refused(DddRefusal::WrongDatFile {
                id: id.raw(),
                named: target,
                belongs,
            });
        }
        let payload = if m.compressed == 1 {
            match dereth_dat::inflate::decompress_ddd_record(&m.data) {
                Ok(p) => p,
                Err(e) => return DataOutcome::Refused(DddRefusal::Decompress(e.to_string())),
            }
        } else {
            m.data.clone()
        };

        if let Some(want) = self.expected.get(&(target, id.raw())) {
            if dereth_dat::overlay::record_hash(&payload) != *want {
                return DataOutcome::Refused(DddRefusal::HashMismatch { id: id.raw() });
            }
        }

        let bytes = m.data.len();
        let iteration = m.iteration;
        let date = entry_date(crate::platform::clock::system_unix_time());
        let run_time = self.phase == DddPhase::RunTime;
        let exact = self.exact.contains(&target);
        let Some(base) = self.target.as_ref().and_then(|t| t.base(target)).cloned() else {
            return DataOutcome::Refused(DddRefusal::OverlayRefused(
                "there is no overlay folder for this world".into(),
            ));
        };
        match self.writer(target) {
            Err(e) => DataOutcome::Refused(DddRefusal::Write(e.to_string())),
            Ok(w) => match if exact {
                // The world numbers its iterations its own way: its record is its own.
                w.put(id, &payload, version, iteration, date)
            } else {
                w.save(&base, id, &payload, version, iteration, date)
            }
            .and_then(|s| {
                // A run-time answer has no end of patch behind it to write the manifest.
                if run_time {
                    w.flush(date)?;
                }
                Ok(s)
            }) {
                Ok(SaveOutcome::RefusedOlderIteration) => {
                    DataOutcome::RefusedOlderIteration { id, target }
                }
                Ok(save) => DataOutcome::Applied {
                    id,
                    target,
                    bytes,
                    save,
                },
                Err(e) => DataOutcome::Refused(DddRefusal::Write(e.to_string())),
            },
        }
    }

    /// The cache worker's `IDsToPurge` loop.
    ///
    /// For each id: a type of 1 (the cell dat) deletes by mask `(id, 0xFFFF0000)` through the
    /// file's disk controller; any other type purges the single record.
    ///
    /// The type is not on the wire. The receiver derives it, and for the
    /// **cell** dat it sets the type to 1 unconditionally, bypassing type inference.
    /// So every id in a cell revision takes the mask arm and the
    /// whole `0xXXXX0000` landblock family goes; a portal id divines to 6 and a language id to
    /// 0x25 (the same function's fallbacks), so those use the single-record purge,
    /// which deletes `(id, 0)` -- one record, no mask.
    ///
    /// Answers whether every purge in the revision was performed. A revision whose purges could
    /// not be performed must not record its iteration: the set at `0xFFFF0001` is what the server
    /// reads next login, and claiming a revision landed when its deletions did not means the
    /// server never offers it again.
    fn purge(&mut self, target: DatTarget, iteration: u32, ids: &[u32]) -> bool {
        let mask = if target == DatTarget::Cell {
            LANDBLOCK_MASK
        } else {
            0
        };
        let pairs: Vec<(u32, u32)> = ids.iter().map(|id| (*id, mask)).collect();
        self.tombstone_all(target, iteration, &pairs)
    }

    /// Write each `(id, mask)` deletion into the overlay as a tombstone of revision `iteration`.
    /// Answers whether every one was written.
    fn tombstone_all(&mut self, target: DatTarget, iteration: u32, ids: &[(u32, u32)]) -> bool {
        if ids.is_empty() {
            return true;
        }
        let mut removed = 0usize;
        let mut failure: Option<String> = self.overlay_refused.clone();
        match self.writer(target) {
            Err(e) => failure = Some(e.to_string()),
            Ok(_) if failure.is_some() => {}
            Ok(w) => {
                for (id, mask) in ids {
                    match w.tombstone(DataId(*id), *mask, iteration) {
                        Ok(n) => removed += n,
                        // Native ignores the result and carries on through the list; so does this,
                        // but the first failure is what blocks the iteration.
                        Err(e) => {
                            if failure.is_none() {
                                failure = Some(e.to_string());
                            }
                        }
                    }
                }
                // The deletions are on disk before the revision's downloads land, as the retail
                // purge is.
                let date = entry_date(crate::platform::clock::system_unix_time());
                if let Err(e) = w.flush(date) {
                    failure.get_or_insert(e.to_string());
                }
            }
        }
        if failure.is_none() {
            // A tombstone changes what the world reads even where the overlay held nothing.
            self.changed.insert(target);
        }
        match failure {
            None => {
                self.note(format!(
                    "0xF7E7 iteration {iteration}: {} deletion(s) of {} written to the overlay \
                     ({removed} of its own record(s) went)",
                    ids.len(),
                    self.name_of(target)
                ));
                true
            }
            Some(e) => {
                self.note(format!(
                    "0xF7E7 iteration {iteration}: the purge of {} id(s) from {} failed ({e}); \
                     {removed} record(s) went, and the iteration is not recorded",
                    ids.len(),
                    self.name_of(target)
                ));
                false
            }
        }
    }

    /// Open the file's overlay writer, once.
    fn writer(&mut self, target: DatTarget) -> Result<&mut OverlayWriter, DatError> {
        if !self.writers.contains_key(&target) {
            let t = self.target.as_ref().ok_or(DatError::NotFound(DataId(0)))?;
            let (base, name) = t.bases.get(&target).ok_or(DatError::NotFound(DataId(0)))?;
            let date = entry_date(crate::platform::clock::system_unix_time());
            let fresh = !t.dir.holds(target);
            let w = match t.dir.writer(target, base, name, &t.world_key, date) {
                Ok(w) => w,
                Err(OverlayError::Dat(e)) => return Err(e),
                Err(e) => {
                    let why = e.to_string();
                    self.refuse(why.clone());
                    return Err(DatError::Io(std::io::Error::other(why)));
                }
            };
            let mut w = w;
            if fresh {
                self.created.insert(target);
            }
            // A world with its own iteration list starts from the base's: the iterations of the
            // records the client already holds.
            if self.exact.contains(&target) && !w.manifest().exact_iterations {
                let have = w.iterations()?;
                let mut all = dereth_dat::overlay::base_iterations(base);
                all.extend(have);
                w.set_exact_iterations(&all, date)?;
                w.flush(date)?;
            }
            self.writers.insert(target, w);
        }
        Ok(self
            .writers
            .get_mut(&target)
            .unwrap_or_else(|| unreachable!("just inserted")))
    }

    /// Retire a run-time get this record answers, whatever `QualifiedDataID::Type` it was asked
    /// for under -- the wire's `0xF7E2` carries a `resource_type` of its own and the client's
    /// pending-gets key is the pair, so matching on the id alone is what makes the answer to
    /// "landblock `0xAABBFFFF`" retire the request for it.
    fn retire_get(&mut self, id: DataId) -> bool {
        let before = self.gets.len();
        self.gets.retain(|(_, i)| *i != id.raw());
        self.gets.len() != before
    }

    /// Pending-download removal: drop the key, and when that empties its revision, add
    /// the revision's iteration to the file's `0xFFFF0001` set
    /// (add, sort, then save the mostly-consecutive integer set's iteration list).
    ///
    /// Answers whether the key was pending at all.
    fn retire(&mut self, key: (DatTarget, u32)) -> bool {
        let Some(index) = self.pending.remove(&key) else {
            return false;
        };
        if let Some(rev) = self.revisions.get_mut(index) {
            rev.outstanding = rev.outstanding.saturating_sub(1);
            if rev.outstanding == 0 {
                self.record_iteration(index);
            }
        }
        true
    }

    /// Save the iteration list for one completed revision.
    fn record_iteration(&mut self, index: usize) {
        let Some(rev) = self.revisions.get(index) else {
            return;
        };
        if rev.recorded || rev.blocked {
            return;
        }
        let (target, iteration) = (rev.target, rev.iteration);
        let outcome = self.writer(target).and_then(|w| {
            w.add_iteration(
                iteration,
                entry_date(crate::platform::clock::system_unix_time()),
            )
        });
        match outcome {
            Ok(()) => {
                if let Some(rev) = self.revisions.get_mut(index) {
                    rev.recorded = true;
                }
                self.changed.insert(target);
            }
            Err(e) => self.note(format!(
                "iteration {iteration} could not be recorded in {}: {e}",
                self.name_of(target)
            )),
        }
    }

    fn note(&mut self, line: String) {
        tracing::info!("DDD {line}");
        self.notices.push(line);
    }
}

/// Which container a `DataID` lives in, for the routing rail.
///
/// Resource-type classification uses a range table over the portal and language id spaces; the cell dat
/// is keyed by landblock instead, so an id that is not a portal or local type falls through to
/// `classify_cell_id`. That is the same two-step `RetailDatStore::resolve` takes, minus the
/// membership check — a patch is allowed to *add* an id the file does not have yet.
#[must_use]
fn id_home(id: DataId) -> Option<DatKind> {
    if let Some(t) = dereth_dat::divine_type(id) {
        let kind = t.dat();
        if kind != DatKind::None {
            return Some(kind);
        }
    }
    dereth_dat::classify_cell_id(id).map(|t| t.dat())
}

/// The client's one `0xF7E3` producer constructs the request from its resource type and id,
/// then delivers it through queue 5. Serialization writes the opcode and the two dwords of the
/// `QualifiedDataID`, type first — sixteen bytes on the wire with the blob's own framing, eight in
/// the body. It carries **no** iteration and no dat-file id: the server works both out from the
/// type (`DDDHandler.DDD_RequestDataMessage` switches on `qdid_type`).
#[must_use]
pub fn request_message(resource_type: u32, resource_id: DataId) -> DddRequestData {
    DddRequestData {
        resource_type,
        resource_id: resource_id.raw(),
    }
}

/// **The production caller for `0xF7E3`.**
///
/// Asynchronous cache lookup follows this order:
///
/// ```text
///   already decoded in memory?             done
///   may load from disk and it is on disk?  queue the worker read
///   may request from the network?          ask other sources -> 0xF7E3
/// ```
///
/// [`dereth_world_data::land_source::DatLandSource::take_missing`] is the "neither" arm: a record the store
/// does not carry, found while a landblock was being built. The two conditions this adds are
/// native's own:
///
/// * **A connection must exist.** The network lookup opens with
///   the packet-controller singleton and returns `false` when it is null -- the get fails outright,
///   nothing is queued for later. The caller's `Option<Session>` is that check.
/// * **One request per `QualifiedDataID`.** The pending-request table is a
///   table from `QualifiedDataID` to pending asynchronous request; a second get for an outstanding id
///   attaches to the existing request rather than issuing another `0xF7E3`
///   through the table's lookup arm. [`DddPatcher::request_once`] is that hash,
///   and it is retired by the answering `0xF7E2` or by a `0xF7E4` --
///   the asynchronous-get failure path, which is what makes a retry possible at all.
///
/// ACE's side agrees: `DDD_RequestDataMessage` is registered on `SessionState.WorldConnected` and
/// answers only `LandBlock`, `LandBlockInfo` and `EnvCell`, and a `LandBlock` request also gets
/// the `0xFFFE` `LandBlockInfo` sent with it.
///
/// Returns how many requests went out.
pub fn drain_cache_misses<T: dereth_primitives::Transport>(
    land: &dereth_world_data::land_source::DatLandSource,
    patcher: &mut DddPatcher,
    session: &mut dereth_client_net::client_session::Session<T>,
) -> usize {
    let mut sent = 0usize;
    for (resource_type, id) in land.take_missing() {
        if patcher.request_once(resource_type, id) {
            session.request_ddd_data(resource_type, id.raw());
            sent += 1;
        }
    }
    sent
}

/// DAT entries store whole Unix seconds in a wrapping unsigned 32-bit field.
fn entry_date(time: Option<std::time::Duration>) -> u32 {
    #[allow(clippy::cast_possible_truncation)]
    // LINT-OK: the stored field deliberately keeps the low 32 bits of Unix seconds.
    time.map_or(0, |duration| duration.as_secs() as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Behaviour: none (timestamp conversion preserves the stored unsigned seconds field).
    #[test]
    fn entry_dates_use_whole_seconds_with_zero_fallback_and_wrap() {
        use std::time::Duration;
        assert_eq!(entry_date(None), 0);
        assert_eq!(entry_date(Some(Duration::ZERO)), 0);
        assert_eq!(entry_date(Some(Duration::new(123, 999_999_999))), 123);
        assert_eq!(
            entry_date(Some(Duration::from_secs(u64::from(u32::MAX)))),
            u32::MAX
        );
        assert_eq!(
            entry_date(Some(Duration::from_secs(u64::from(u32::MAX) + 2))),
            1
        );
    }

    /// Behaviour: none (completed patch writes stamp both records and revision metadata at the host boundary).
    #[test]
    fn patch_records_and_completed_revisions_receive_wall_clock_dates() {
        let scratch =
            dereth_dat::testing::ScratchDir::new("ddd-entry-dates").expect("scratch directory");
        let dir = scratch.path().to_path_buf();
        let base_path = DatTarget::Local.in_dir(&dir.join("base"));
        std::fs::create_dir_all(dir.join("base")).expect("base folder");
        {
            let mut writer =
                dereth_dat::write::DatWriter::create(&base_path, 0x400, 1, 3, 64 * 1024)
                    .expect("create");
            writer
                .save(
                    dereth_dat::ITERATION_LIST,
                    &dereth_dat::iteration::encode(&[1]),
                    1,
                    0,
                    1,
                )
                .expect("seed iterations");
        }
        let base = DatFile::open(&base_path).expect("the base");
        let overlay = OverlayDir::new(&dir.join("overlay")).expect("an overlay folder");
        let id = DataId(0x2100_0001);
        let mut patcher = DddPatcher::new(Some(OverlayTarget::with_bases(
            overlay.clone(),
            [(DatTarget::Local, base.clone())],
            "a world",
        )));
        patcher.on_interrogation();
        let (_, action) = patcher.on_begin(&DddBeginDdd {
            data_expected: 4,
            revisions: vec![dereth_protocol::admin::PatchRevision {
                dat_file_type: 1,
                dat_file_id: 3,
                iteration: 2,
                ids_to_download: vec![id.raw()],
                ids_to_purge: Vec::new(),
            }],
        });
        assert!(!action.send_end);
        let before = entry_date(crate::platform::clock::system_unix_time());
        assert!(before > 1, "the host clock differs from the seeded date");
        let (outcome, action) = patcher.on_data(&DddData {
            dat_file_type: 1,
            dat_file_id: 3,
            resource_type: 0x21,
            resource_id: id.raw(),
            iteration: 2,
            compressed: 0,
            version: 1,
            data_size: 8,
            data: vec![1, 2, 3, 4],
        });
        let after = entry_date(crate::platform::clock::system_unix_time());
        assert!(
            matches!(outcome, DataOutcome::Applied { .. }),
            "{outcome:?}"
        );
        assert!(action.send_end, "the revision completed");
        patcher.on_end();
        let reader = DatFile::open(&overlay.container(DatTarget::Local)).expect("the overlay");
        for record in [id, dereth_dat::ITERATION_LIST] {
            let date = reader.entry(record).expect("written entry").date;
            assert!(
                (before..=after).contains(&date),
                "{record:?}: {date} outside {before}..={after}"
            );
        }
        assert_eq!(reader.iteration_list().expect("iterations"), [2]);
        // Read over its base, the file's iterations are the base's and the overlay's.
        let layer = dereth_dat::overlay::Layer::over(&base, reader, Some("a world"))
            .expect("the overlay over its base");
        assert_eq!(
            base.layered(std::sync::Arc::new(layer))
                .iteration_list()
                .unwrap(),
            [1, 2]
        );
        assert!(
            DatFile::open(&base_path).unwrap().entry(id).is_none(),
            "the base is untouched"
        );
        drop(base);
        drop(patcher);
        drop(scratch);
    }

    #[test]
    fn the_four_dat_file_pairs_are_the_ones_ace_writes() {
        // GameMessageDDDDataMessage.cs's switch, transcribed.
        assert_eq!(target_from_wire(0, 1), Some(DatTarget::Portal));
        assert_eq!(target_from_wire(1, 2), Some(DatTarget::Cell));
        assert_eq!(target_from_wire(1, 3), Some(DatTarget::Local));
        assert_eq!(target_from_wire(HIFI, 1), Some(DatTarget::HighRes));
        assert_eq!(target_from_wire(1, 1), None);
        // "HiFi" little-endian. ACE computes it the same way and never writes the literal.
        assert_eq!(HIFI, 0x6946_6948);
    }

    #[test]
    fn the_request_message_is_the_qualified_data_id_and_nothing_else() {
        let m = request_message(1, DataId(0x0100_0134));
        assert_eq!(
            m,
            DddRequestData {
                resource_type: 1,
                resource_id: 0x0100_0134
            }
        );
    }
}
