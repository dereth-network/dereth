//! Patching a client to a world served as its data files with an overlay over them: the records the
//! world changes go to the clients that lack them through the data download, revision by revision.
//!
//! The download's own messages carry it. What's the world's alone:
//!
//! - **A client that keeps overlays says so** ([`OverlayHeld`]) ahead of the data it holds, and names
//!   the files it holds. A retail client sends nothing of the kind, and a server that doesn't know
//!   the message goes on as if it hadn't come. Such a client is sent what it lacks, at the rate the
//!   operator chose ([`DatOverlaySettings`]), unless it holds other files than the overlay was made
//!   against, when it is refused with the reason. Holding the same records laid out otherwise, it
//!   checks them itself: it is told the records the overlay was made against
//!   ([`OverlayBases`]), and stops if it doesn't hold them. With patching off, it is refused.
//! - **A retail client** is refused unless the operator turned its patching on, as it can't take a
//!   world's records unless it runs with its files writable, on a copy kept for this world. Patched,
//!   it goes at the original servers' rate shared by every client.
//! - **Which records.** A client lacks a revision its list for that file doesn't hold. A client
//!   holding a revision past its file's own that the world doesn't have was patched to another
//!   world, and is refused.
//!
//! Every answer is packed ahead of time, so patching a client reads nothing as the world runs.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use auberean_dat::disk::download::stream_ids;
use auberean_protocol::login::{
    DataFileIterations, DataHeld, DataRecord, DownloadBegins, MissingIterations, OverlayBase,
    OverlayBases, OverlayHeld,
};
use auberean_protocol::types::IterationRun;
use auberean_protocol::{queue, ServerMessage};
use dereth_dat::overlay;
use dereth_primitives::DataId;
use yalain_net::{BlobKind, ConnId, ServerNet};
use yalain_store::Store;

use crate::download::{packed, InstallFiles, CELL_FILE, ENGLISH_FILE, PORTAL_FILE};
use crate::{Clock, Server};

/// Why a client that keeps overlays and holds other files is refused.
pub const OTHER_FILES: &str = "This world's data overlay was made against other data files than your client's. Install the data files the world names.";
/// Why a client that keeps no overlay, missing the world's, is refused.
pub const NOT_KEPT: &str = "This world changes the game's data files, and your client does not take a world's changes. Play it with Auberean, which the world patches.";
/// Why a client that keeps overlays is refused while the operator has patching off.
pub const NOT_PATCHING: &str = "This world changes the game's data files, and does not send them. Get its overlay from the world's operator.";
/// Why a client holding another world's revisions is refused.
pub const OTHER_WORLD: &str = "Your client's data files were patched for another world. Play this world from a copy of them kept for it.";

/// The original servers' rate, records a minute shared by every client, for the clients with no
/// rate of their own.
pub const SHARED_RECORDS_PER_MINUTE: u32 = 1_000;
/// How long a client has to take in that a download begins before its first record, seconds.
pub const PAUSE_SECONDS: f64 = 5.0;
/// The most records a client is sent in a pass of the world.
pub const MOST_A_TICK: usize = 32;
/// The iterations a server gives what it adds for a retail client of its own accord, which are no
/// world's: [`crate::admin_library`] keeps its own within them.
const SERVER_OWN_ITERATIONS: core::ops::Range<u32> = 9_000..10_000;

/// What the operator chose for a world's overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DatOverlaySettings {
    /// Patch a client that keeps overlays to the world. Off, such a client lacking it is refused.
    pub patching: bool,
    /// How many records a minute such a client is sent; 0 for the original servers' shared rate.
    pub records_per_minute: u32,
    /// Patch a client that keeps no overlay to the world too, at the original servers' rate. It
    /// must run with its files writable, or it stops at the first record.
    pub enable_dat_patching: bool,
}

impl Default for DatOverlaySettings {
    fn default() -> Self {
        Self {
            patching: true,
            records_per_minute: 60_000,
            enable_dat_patching: false,
        }
    }
}

/// One revision of one file.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Revision {
    dat_file: u64,
    iteration: u32,
    /// The records it brings, packed, and the ids it drops.
    records: Vec<Arc<DataRecord>>,
    purge: Vec<u32>,
}

/// What a file's overlay was made against.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Base {
    dat_file: u64,
    /// The layouts a file holding the base may have: the overlay's, and this server's file's.
    layouts: [[u8; 32]; 2],
    records: Option<[u8; 32]>,
    /// The file's own last iteration.
    last_own: u32,
}

/// A world's overlay, as clients are patched to it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WorldPatch {
    revisions: Vec<Revision>,
    bases: Vec<Base>,
}

/// What a client that told of its data is answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// It lacks nothing: on as for any world.
    Holds,
    /// It is sent what it lacks, the rate of its own when it has one.
    Patch {
        rate: Option<u32>,
        check_records: bool,
    },
    /// It is refused, for this reason.
    Refused(&'static str),
}

impl WorldPatch {
    /// The patch of the files as the server reads them; `None` for files with no overlay over them.
    /// Packs every record, so it's made beside the world, not in it.
    #[must_use]
    pub fn of(files: &InstallFiles) -> Option<Self> {
        let mut patch = Self::default();
        for (dat, dat_file) in [
            (&files.portal, PORTAL_FILE),
            (&files.cell, CELL_FILE),
            (&files.english, ENGLISH_FILE),
        ] {
            let Some(dat) = dat else { continue };
            let Some(layer) = dat.shared().layer() else {
                continue;
            };
            let set = (dat_file >> 32) as u32;
            let m = layer.manifest();
            patch.bases.push(Base {
                dat_file,
                layouts: [
                    m.base_fingerprint,
                    overlay::fingerprint(&dat.shared().base()),
                ],
                records: m.base_records,
                last_own: overlay::base_iterations(&dat.shared().base())
                    .into_iter()
                    .filter(|i| i.cast_signed() >= 0)
                    .max()
                    .unwrap_or(0),
            });
            for iteration in layer.own_iterations() {
                let ids: Vec<u32> = layer
                    .records()
                    .filter(|(_, e)| e.iteration == *iteration)
                    .map(|(id, _)| id.raw())
                    .collect();
                let records = stream_ids(&ids, set)
                    .into_iter()
                    .filter_map(|q| {
                        let bytes = layer.read(DataId(q.id)).ok()?;
                        Some(Arc::new(packed(dat_file, q.kind, q.id, &bytes)))
                    })
                    .collect();
                // A deletion of a whole landblock is its landblock's id: the client drops what
                // goes with it.
                let purge = layer
                    .tombstones()
                    .iter()
                    .filter(|t| t.iteration == *iteration)
                    .map(|t| if t.mask == 0 { t.id } else { t.id | 0xFFFF })
                    .collect();
                patch.revisions.push(Revision {
                    dat_file,
                    iteration: *iteration,
                    records,
                    purge,
                });
            }
        }
        (!patch.bases.is_empty()).then_some(patch)
    }

    /// The revisions a client holding these lists lacks, each by its place in the patch.
    fn lacked(&self, files: &[DataFileIterations]) -> Vec<usize> {
        self.revisions
            .iter()
            .enumerate()
            .filter(|(_, r)| !holds(files, r.dat_file, r.iteration))
            .map(|(i, _)| i)
            .collect()
    }

    /// Whether a client holding these lists holds a revision past its file's own that this world
    /// doesn't have, of any file the overlay lies over.
    fn holds_another_worlds(&self, files: &[DataFileIterations]) -> bool {
        self.bases.iter().any(|b| {
            files
                .iter()
                .filter(|f| f.dat_file == b.dat_file)
                .flat_map(|f| &f.iterations.runs)
                .flat_map(|r| match *r {
                    IterationRun::One(v) => v.cast_unsigned()..v.cast_unsigned().saturating_add(1),
                    IterationRun::Run { count, first } => first..first.saturating_add(count),
                })
                .filter(|i| i.cast_signed() >= 0 && !SERVER_OWN_ITERATIONS.contains(i))
                .any(|i| {
                    i > b.last_own
                        && !self
                            .revisions
                            .iter()
                            .any(|r| r.dat_file == b.dat_file && r.iteration == i)
                })
        })
    }

    /// What a client is answered: it told what it holds and, if it keeps overlays, which files.
    #[must_use]
    pub fn verdict(
        &self,
        settings: &DatOverlaySettings,
        files: &[DataFileIterations],
        keeps: Option<&OverlayHeld>,
    ) -> Verdict {
        if self.holds_another_worlds(files) {
            return Verdict::Refused(OTHER_WORLD);
        }
        if self.lacked(files).is_empty() {
            return Verdict::Holds;
        }
        let Some(keeps) = keeps else {
            return if settings.enable_dat_patching {
                Verdict::Patch {
                    rate: None,
                    check_records: false,
                }
            } else {
                Verdict::Refused(NOT_KEPT)
            };
        };
        if !settings.patching {
            return Verdict::Refused(NOT_PATCHING);
        }
        let mut check_records = false;
        for b in &self.bases {
            let held = keeps
                .bases
                .iter()
                .find(|h| h.dat_file == b.dat_file)
                .map(|h| h.fingerprint);
            if held.is_some_and(|h| b.layouts.contains(&h)) {
                continue;
            }
            if keeps.checks_records() && held.is_some() && b.records.is_some() {
                check_records = true;
                continue;
            }
            return Verdict::Refused(OTHER_FILES);
        }
        Verdict::Patch {
            rate: (settings.records_per_minute > 0).then_some(settings.records_per_minute),
            check_records,
        }
    }
}

/// Has a client told of its data holding an iteration of a file?
#[must_use]
pub fn holds(files: &[DataFileIterations], dat_file: u64, iteration: u32) -> bool {
    files
        .iter()
        .filter(|f| f.dat_file == dat_file)
        .flat_map(|f| &f.iterations.runs)
        .any(|r| match *r {
            IterationRun::One(v) => v.cast_unsigned() == iteration,
            IterationRun::Run { count, first } => first <= iteration && iteration - first < count,
        })
}

/// A client being patched: what's still to go, and when its next record may.
#[derive(Debug)]
struct Patching {
    records: VecDeque<Arc<DataRecord>>,
    /// Its rate, records a minute; `None` for the rate all share.
    rate: Option<u32>,
    next: f64,
}

/// The clients being patched to the world.
#[derive(Debug, Default)]
pub struct Patches {
    patching: HashMap<ConnId, Patching>,
    /// What each told of its data, kept for when it has stored what it was sent.
    held: HashMap<ConnId, DataHeld>,
    /// When the next record at the rate all share may go.
    shared_next: f64,
}

impl Patches {
    /// A connection is gone.
    pub fn closed(&mut self, conn: ConnId) {
        self.patching.remove(&conn);
        self.held.remove(&conn);
    }

    /// A client has stored what it was sent: what it told of its data, if it was being patched.
    pub fn stored(&mut self, conn: ConnId) -> Option<DataHeld> {
        self.patching.remove(&conn);
        self.held.remove(&conn)
    }
}

/// Answer a client that told of its data, as far as the world's overlay goes. Answers whether that
/// is all it is answered for now: refused, or being patched.
pub fn on_held<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    conn: ConnId,
    held: &DataHeld,
    clock: Clock,
) -> bool {
    let Some(patch) = s.world_patch.clone() else {
        return false;
    };
    let keeps = s.overlay_clients.get(&conn);
    match patch.verdict(&s.cfg.dat_overlay, &held.files, keeps) {
        Verdict::Holds => false,
        Verdict::Refused(why) => {
            tracing::info!(?conn, why, "a client refused over the world's overlay");
            crate::admin::end_login_saying(s, net, conn, why, clock);
            true
        }
        Verdict::Patch {
            rate,
            check_records,
        } => {
            if check_records {
                let bases = patch
                    .bases
                    .iter()
                    .filter_map(|b| {
                        Some(OverlayBase {
                            dat_file: b.dat_file,
                            fingerprint: b.records?,
                        })
                    })
                    .collect();
                let m = ServerMessage::OverlayBases(OverlayBases { bases });
                net.send(conn, queue::DATABASE, BlobKind::LOGIN, m.encode());
            }
            let lacked = patch.lacked(&held.files);
            let revisions: Vec<&Revision> = lacked.iter().map(|i| &patch.revisions[*i]).collect();
            let records: VecDeque<Arc<DataRecord>> = revisions
                .iter()
                .flat_map(|r| r.records.iter().cloned())
                .collect();
            let bytes: usize = records.iter().map(|r| r.bytes.len()).sum();
            let begins = ServerMessage::DownloadBegins(DownloadBegins {
                bytes: u32::try_from(bytes).unwrap_or(u32::MAX),
                files: revisions
                    .iter()
                    .map(|r| MissingIterations {
                        dat_file: r.dat_file,
                        iteration: r.iteration,
                        download: r.records.iter().map(|d| d.id).collect(),
                        purge: r.purge.clone(),
                    })
                    .collect(),
            });
            net.send(conn, queue::DATABASE, BlobKind::LOGIN, begins.encode());
            tracing::info!(
                ?conn,
                revisions = revisions.len(),
                records = records.len(),
                "patching a client to the world's overlay"
            );
            s.patches.held.insert(conn, held.clone());
            s.patches.patching.insert(
                conn,
                Patching {
                    records,
                    rate,
                    next: clock.local.0 + PAUSE_SECONDS,
                },
            );
            true
        }
    }
}

/// Send what is due of the clients being patched, in a pass of the world.
pub fn send_due<S: Store>(s: &mut Server<S>, net: &mut ServerNet, clock: Clock) {
    let now = clock.local.0;
    let Patches {
        patching,
        shared_next,
        ..
    } = &mut s.patches;
    for (conn, p) in patching.iter_mut() {
        let mut sent = 0;
        while sent < MOST_A_TICK && now >= p.next {
            let Some(record) = p.records.pop_front() else {
                break;
            };
            let m = ServerMessage::DataRecord((*record).clone());
            net.send(*conn, queue::DATABASE, BlobKind::LOGIN, m.encode());
            sent += 1;
            match p.rate {
                Some(rate) => p.next += 60.0 / f64::from(rate.max(1)),
                None => {
                    *shared_next =
                        shared_next.max(now) + 60.0 / f64::from(SHARED_RECORDS_PER_MINUTE);
                    p.next = p.next.max(*shared_next);
                }
            }
        }
    }
}
