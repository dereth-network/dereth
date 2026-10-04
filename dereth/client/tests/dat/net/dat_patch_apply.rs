//! The dat patch path end to end: a `0xF7E2 DDD_DataMessage` off the wire becomes a record in the
//! world's overlay, and the files read through the overlay show it; malformed or older records
//! write nothing; the iteration list is recorded when a revision completes; purges are tombstones;
//! the installed data files are never written.
//!
//! ACE's `Source/ACE.Server` tree is the oracle for the bytes a server sends:
//! `Network/Handlers/DDDHandler.cs`, `Managers/DDDManager.cs`, and
//! `Network/GameMessages/Messages/GameMessageDDD*.cs`.
//!
//! # Safety
//!
//! **Every write here goes into a scratch overlay folder** over the retail install at
//! `DERETH_TEST_DAT_DIR`, which is protected for the run and read only. The language and cell
//! files a test patches are digested before and checked again when the scratch guard drops.
//!
//! No socket is bound and no datagram is sent: the messages are built with `dereth_protocol` and pushed
//! through `dereth_client_net::client_session`'s own queue-5 dispatcher, so what reaches the patcher is what a server's
//! bytes decode to.

use std::path::{Path, PathBuf};

use dereth_client::ddd::{DatTarget, DataOutcome, DddPatcher, DddPhase, DddRefusal, OverlayTarget};
use dereth_client::present::NullPresentation;
use dereth_dat::overlay::OverlayDir;
use dereth_dat::write::SaveOutcome;
use dereth_dat::{DatFile, RetailDatStore};
use dereth_primitives::DataId;
use dereth_protocol::admin::{DddBeginDdd, DddData, DddError, PatchRevision};

// ---------------------------------------------------------------------------------------------
// A scratch overlay over the protected install, and the proof the installed files were not
// touched.
// ---------------------------------------------------------------------------------------------

fn digest(path: &Path) -> (u64, u64) {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in &bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x1000_0000_01b3);
    }
    (bytes.len() as u64, h)
}

/// The world name every scratch overlay here belongs to.
const WORLD: &str = "dat patch tests";

struct Scratch {
    overlay: OverlayDir,
    store: RetailDatStore,
    _directory: dereth_dat::testing::ScratchDir,
    pristine: Vec<(PathBuf, (u64, u64))>,
}

impl Scratch {
    fn new(name: &str) -> Self {
        let directory = dereth_dat::testing::ScratchDir::new(name).expect("a scratch directory");
        let overlay =
            OverlayDir::new(&directory.path().join("overlay")).expect("an overlay folder");
        let store = dereth_dat::testing::open_store_or_fail();
        let local = dereth_dat::RetailDat::Local.in_dir(&dereth_dat::testing::dat_dir());
        let pristine = vec![(local.clone(), digest(&local))];
        Self {
            overlay,
            store,
            _directory: directory,
            pristine,
        }
    }

    /// The cell file is digested too, for the test that tombstones a landblock in it.
    fn watch_cell(&mut self) {
        let cell = dereth_dat::RetailDat::Cell.in_dir(&dereth_dat::testing::dat_dir());
        let d = digest(&cell);
        self.pristine.push((cell, d));
    }

    fn patcher(&self) -> DddPatcher {
        DddPatcher::new(Some(OverlayTarget::new(
            self.overlay.clone(),
            &self.store,
            WORLD,
        )))
    }

    /// The install's own language file.
    fn base_local(&self) -> &DatFile {
        self.store.local()
    }

    /// The files as the world reads them now: the install with the overlay over it.
    fn world(&self) -> RetailDatStore {
        self.store
            .clone()
            .with_overlay(&self.overlay, Some(WORLD))
            .expect("the overlay opens over its base")
    }

    /// The language file as the world reads it.
    fn local(&self) -> DatFile {
        self.world().local().clone()
    }

    /// The overlay container over `target`'s file.
    fn container(&self, target: DatTarget) -> PathBuf {
        self.overlay.container(target)
    }

    /// The overlay container's own structure is sound.
    fn assert_sound(&self, target: DatTarget) {
        let f = DatFile::open(&self.container(target)).expect("the overlay container opens");
        assert!(
            f.verify_structure().expect("the tree walks").is_sound(),
            "the overlay's tree is sound"
        );
    }

    /// The language overlay container's length and digest, or nothing while it does not exist.
    fn overlay_digest(&self) -> Option<(u64, u64)> {
        let c = self.container(DatTarget::Local);
        c.is_file().then(|| digest(&c))
    }

    fn assert_pristine(&self) {
        for (src, before) in &self.pristine {
            let after = digest(src);
            assert_eq!(
                *before,
                after,
                "the retail dat {} changed during the test",
                src.display()
            );
        }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        self.assert_pristine();
    }
}

// ---------------------------------------------------------------------------------------------
// Message builders, in ACE's byte order.
// ---------------------------------------------------------------------------------------------

/// A `DataID` in the language dat's own space that no retail record uses.
///
/// `0x2100xxxx` is `STRING_TABLE` per the current type-range table, and the shipped file holds
/// `0x2100_0001`..`0x2100_0010`-ish, so a high one is free. `contains` is asserted false before
/// every add that means to be an add.
const NEW_STRING: DataId = DataId(0x2100_7FFF);

/// `GameMessageDDDDataMessage`'s field order, with `data_size = payload.len() + 4` exactly as ACE
/// writes it (`Writer.Write(datFileContents.Length + 4)`).
fn data_msg(id: DataId, payload: &[u8], version: u32, iteration: u32) -> DddData {
    DddData {
        dat_file_type: 1,
        dat_file_id: 3,
        resource_type: 0x21,
        resource_id: id.raw(),
        iteration,
        compressed: 0,
        version,
        data_size: payload.len() as u32 + 4,
        data: payload.to_vec(),
    }
}

/// The same, compressed: `[u32 uncompressed_length][zlib stream]`, which is
/// `DDDManager.PrependUncompressedFileSize(Compress(buffer), fileSize)`.
fn compressed_msg(id: DataId, uncompressed_len: usize, zlib: &[u8], version: u32) -> DddData {
    let mut data = (uncompressed_len as u32).to_le_bytes().to_vec();
    data.extend_from_slice(zlib);
    DddData {
        dat_file_type: 1,
        dat_file_id: 3,
        resource_type: 0x21,
        resource_id: id.raw(),
        iteration: 0,
        compressed: 1,
        version,
        data_size: data.len() as u32 + 4,
        data,
    }
}

/// `GameMessageDDDBeginDDD` for the language dat: one revision, `ids_to_download` as bare dwords
/// (the client derives `QualifiedDataID::Type` locally and never reads one off the wire, which is
/// why ACE's `Writer.Write(file)` per ID is right).
fn begin(iteration: u32, download: &[u32], expected: u32) -> DddBeginDdd {
    DddBeginDdd {
        data_expected: expected,
        revisions: vec![PatchRevision {
            dat_file_type: 1,
            dat_file_id: 3,
            iteration,
            ids_to_download: download.to_vec(),
            ids_to_purge: Vec::new(),
        }],
    }
}

/// A deterministic payload, deliberately not a valid `StringTable` payload: the container stores
/// bytes and decoding is a separate concern, so this suite asserts byte-exactness and leaves
/// decoding to the crate that owns it.
fn payload(seed: u8, n: usize) -> Vec<u8> {
    (0..n)
        .map(|i| (i as u8).wrapping_mul(37).wrapping_add(seed))
        .collect()
}

fn hex(s: &str) -> Vec<u8> {
    (0..s.len() / 2)
        .map(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).expect("hex"))
        .collect()
}

// ---------------------------------------------------------------------------------------------
// (a) The request producer: the interrogation does not produce `0xF7E3`.
// ---------------------------------------------------------------------------------------------

/// `0xF7E3` is a **run-time cache miss**, not part of the patch handshake.
///
/// The whole handshake is server-driven: finishing the begin request puts every `IDsToDownload`
/// entry into the pending set and then *waits*; the only thing it sends is
/// `0xF7EA` when there is nothing to wait for. Nothing in the patch-handshake path builds a
/// `DDD_RequestDataMessage`. Its producer is the asynchronous cache-miss hook for "the disk
/// controller does not have this object":
///
/// ```text
/// construct a DDD_RequestDataMessage with opcode 0xF7E3
/// adopt and deliver it on client-cache queue 5
/// ```
///
/// ACE agrees from the other side: `DDD_RequestDataMessage` is registered on
/// `SessionState.WorldConnected` and answers only `LandBlock`, `LandBlockInfo` and `EnvCell` —
/// a login-time handler could not be, and cell types are exactly what a player walking into an
/// unpatched landblock would miss.
#[test]
fn a_run_time_cache_miss_produces_the_native_f7e3_bytes() {
    // The client's type mapping gives a landblock `0x01`; the ID is landblock (0xAB, 0xCD)'s
    // `0xFFFF` record, which is the shape ACE's handler special-cases.
    let m = dereth_client::ddd::request_message(1, DataId(0xABCD_FFFF));
    let body = dereth_protocol::write_blob(&m).expect("the request encodes");

    // Opcode dword then the two dwords of the `QualifiedDataID`, type first, no padding:
    // Serialization performs an eight-byte alignment check followed by an eight-byte read, and
    // archive alignment is off for every network archive in the client, so the check is a no-op.
    let mut want = Vec::new();
    want.extend_from_slice(&0xF7E3u32.to_le_bytes());
    want.extend_from_slice(&1u32.to_le_bytes());
    want.extend_from_slice(&0xABCD_FFFFu32.to_le_bytes());
    assert_eq!(
        body, want,
        "the 0xF7E3 blob is twelve bytes: opcode, Type, ID"
    );

    // And it goes out on queue 5 with no order header, which is what the opcode table says.
    let info = dereth_protocol::Opcode(0xF7E3)
        .info()
        .expect("a known opcode");
    assert_eq!(
        info.send_queue,
        Some(dereth_primitives::NetQueue::ClientCache)
    );
    assert_eq!(info.name, "DDD_RequestDataMessage");
}

// ---------------------------------------------------------------------------------------------
// (b) A data message is validated and applied.
// ---------------------------------------------------------------------------------------------

/// Behaviour: net.dat-patch.a-downloaded-record-is-written-and-seen-after-reopen
/// An uncompressed `0xF7E2` for a brand-new id is written, and the ordinary
/// read-only `DatFile` — the reader the whole client uses — reads it back byte for byte with the
/// iteration and version the message carried.
#[test]
fn an_uncompressed_record_is_applied_and_reopens_byte_exact() {
    let s = Scratch::new("apply_plain");
    let mut p = s.patcher();

    // Precondition: the id really is new, so "Added" means something.
    assert!(
        !s.local().contains(NEW_STRING),
        "the test id must not already be in the shipped file"
    );

    let body = payload(0x5A, 3000);
    let (outcome, action) = p.on_data(&data_msg(NEW_STRING, &body, 3, 77));
    assert_eq!(
        outcome,
        DataOutcome::Applied {
            id: NEW_STRING,
            target: DatTarget::Local,
            // The reported compressed size is message size minus its four-byte size field, i.e.
            // the wire payload length.
            bytes: body.len(),
            save: SaveOutcome::Added,
        }
    );
    assert!(
        !action.send_end,
        "no 0xF7E7 arrived, so nothing is pending and nothing is owed"
    );

    let f = s.local();
    assert_eq!(f.read(NEW_STRING).expect("the new record reads"), body);
    let e = *f.entry(NEW_STRING).expect("the directory has it");
    assert_eq!(
        e.iteration, 77,
        "the directory entry carries the message iteration"
    );
    assert_eq!(
        e.version(),
        3,
        "the directory entry carries the message version"
    );
    assert!(
        !e.compressed(),
        "both relevant save cases leave the stored compressed flag clear"
    );
    s.assert_sound(DatTarget::Local);
}

/// A record that **replaces** a shipped one, which is the case a real patch is made of, with the
/// old bytes read first so the change is measured rather than assumed.
#[test]
fn a_replaced_record_is_the_new_bytes_and_the_old_ones_are_gone() {
    let s = Scratch::new("apply_replace");
    let mut p = s.patcher();

    // Any shipped id will do; take the first one the reader lists that is not the iteration list.
    let (victim, before) = {
        let f = s.local();
        let id = f
            .iter_ids()
            .find(|i| *i != dereth_dat::ITERATION_LIST)
            .expect("the language dat has records");
        (id, f.read(id).expect("it reads"))
    };
    let body = payload(0xA5, 1000);
    assert_ne!(
        body, before,
        "the replacement must differ from what is there"
    );

    let (outcome, _) = p.on_data(&DddData {
        resource_id: victim.raw(),
        ..data_msg(victim, &body, 2, 0)
    });
    assert!(
        matches!(
            outcome,
            DataOutcome::Applied {
                save: SaveOutcome::Replaced,
                ..
            }
        ),
        "{outcome:?}"
    );

    let f = s.local();
    assert_eq!(f.read(victim).expect("the replacement reads"), body);
    s.assert_sound(DatTarget::Local);
}

/// The compressed path: compressed flag 1, payload `[u32 uncompressed_length][zlib stream]`,
/// inflated by the disk-controller save path before it is stored, so the record on disk is the
/// *plaintext* and its compressed flag stays 0.
///
/// The stream is CPython `zlib.compress(plain, 9)`, which is the same zlib ACE's `ZLibStream`
/// reimplements; `plain` is written the same way in both languages so the vector can be
/// regenerated without storing 3000 bytes.
#[test]
fn a_compressed_record_is_inflated_before_it_is_stored() {
    const ZLIB: &str = "78daedcaa11100400803c1a66828138142311174ff65bcb9d5ab6ce5da23efa43bb3d6b82fb551f9ce250e87c3e170381c0e87f3f53c4417af82";
    const AL: &[u8; 36] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let plain: Vec<u8> = (0..3000u64)
        .map(|i| AL[((i * i * 7 + i * 13) % 36) as usize])
        .collect();

    let s = Scratch::new("apply_compressed");
    let mut p = s.patcher();

    let m = compressed_msg(NEW_STRING, plain.len(), &hex(ZLIB), 3);
    // The whole point of compressing: far fewer bytes on the wire than in the dat.
    assert!(
        m.data.len() < plain.len() / 10,
        "the vector must actually be compressed"
    );
    let (outcome, _) = p.on_data(&m);
    assert!(
        matches!(
            outcome,
            DataOutcome::Applied {
                save: SaveOutcome::Added,
                ..
            }
        ),
        "{outcome:?}"
    );

    let f = s.local();
    assert_eq!(
        f.read(NEW_STRING).expect("it reads"),
        plain,
        "the dat holds the plaintext"
    );
    assert!(
        !f.entry(NEW_STRING).expect("the entry").compressed(),
        "the compressed-message save case inflates the payload and clears the stored compressed flag"
    );
}

// ---------------------------------------------------------------------------------------------
// (c) Malformed and refused data: nothing is written, and the reason is reported.
// ---------------------------------------------------------------------------------------------

/// Behaviour: net.dat-patch.a-malformed-or-older-record-writes-nothing
/// Seven refusal branches followed by one aggregate file length-and-digest comparison, so "no
/// write" is measured for the sequence rather than inferred from its return values.
#[test]
fn a_malformed_record_is_refused_and_not_one_byte_is_written() {
    let s = Scratch::new("refusals");
    let mut p = s.patcher();
    let untouched = s.overlay_digest();
    let body = payload(0x11, 500);

    // `data_size` counts the size dword, so a declared 504 needs a 500-byte payload.
    // The pack decoder windows `data_size` bytes and its read fails when they are not there; the
    // archive error latch then makes the cache update drop the message.
    let mut short = data_msg(NEW_STRING, &body, 3, 0);
    short.data_size = 9999;
    assert_eq!(
        p.on_data(&short).0,
        DataOutcome::Refused(DddRefusal::BadLength {
            declared: 9999,
            actual: 500
        })
    );

    // A dat file pair nothing answers to.
    let mut wrong_file = data_msg(NEW_STRING, &body, 3, 0);
    wrong_file.dat_file_id = 9;
    assert_eq!(
        p.on_data(&wrong_file).0,
        DataOutcome::Refused(DddRefusal::UnknownDatFile {
            dat_file_type: 1,
            dat_file_id: 9
        })
    );

    // A portal ID offered for the language dat. The original save path would store it, and this
    // refusal is deliberately stricter: the dat-file type picks the file, and a server that
    // disagrees with the client about where an id lives would otherwise corrupt routing silently.
    let mut cross = data_msg(DataId(0x0100_0001), &body, 3, 0);
    cross.resource_id = 0x0100_0001;
    assert!(
        matches!(
            p.on_data(&cross).0,
            DataOutcome::Refused(DddRefusal::WrongDatFile { .. })
        ),
        "a GFXOBJ id must not be written into the language dat"
    );

    // The save path refuses version zero before it does anything else.
    assert_eq!(
        p.on_data(&data_msg(NEW_STRING, &body, 0, 0)).0,
        DataOutcome::Refused(DddRefusal::ZeroVersion)
    );

    // The boolean decoder raises an archive error for a value other than 0 or 1.
    let mut odd_flag = data_msg(NEW_STRING, &body, 3, 0);
    odd_flag.compressed = 7;
    assert_eq!(
        p.on_data(&odd_flag).0,
        DataOutcome::Refused(DddRefusal::BadCompressedFlag(7))
    );

    // A compressed payload whose stream is corrupt: decompression and the compressed save case
    // return false, so nothing is stored.
    let mut z = hex("78daedcaa11100400803c1a66828138142311174ff65bcb9d5ab6ce5da23efa43bb3d6b82fb551f9ce250e87c3e170381c0e87f3f53c4417af82");
    z[20] ^= 0x55;
    assert!(matches!(
        p.on_data(&compressed_msg(NEW_STRING, 3000, &z, 3)).0,
        DataOutcome::Refused(DddRefusal::Decompress(_))
    ));

    // And a compressed payload below the client's own floor: the decompressor uses `size - 4`
    // and requires at least 16 bytes after the four-byte size field before reading the length.
    assert!(matches!(
        p.on_data(&compressed_msg(NEW_STRING, 8, &[0u8; 4], 3)).0,
        DataOutcome::Refused(DddRefusal::Decompress(_))
    ));

    assert_eq!(
        s.overlay_digest(),
        untouched,
        "seven refusals and the file is byte-identical"
    );
    assert_eq!(p.applied(), 0);
    assert_eq!(
        p.notices().len(),
        7,
        "every refusal is reported: {:?}",
        p.notices()
    );
}

/// Behaviour: net.dat-patch.a-malformed-or-older-record-writes-nothing
/// The original save path's never-downgrade rule, through the protocol: an older iteration is a
/// no-op reported as **success**, so the record that is there stays and the patch does not regress.
#[test]
fn an_older_iteration_is_a_no_op_and_the_newer_record_stands() {
    let s = Scratch::new("stale_iteration");
    let mut p = s.patcher();

    let new = payload(0x01, 800);
    let old = payload(0x02, 800);
    assert!(p.on_data(&data_msg(NEW_STRING, &new, 3, 100)).0.wrote());
    let after_new = s.overlay_digest();

    let (outcome, _) = p.on_data(&data_msg(NEW_STRING, &old, 3, 50));
    assert_eq!(
        outcome,
        DataOutcome::RefusedOlderIteration {
            id: NEW_STRING,
            target: DatTarget::Local
        }
    );
    assert!(!outcome.wrote());
    assert_eq!(
        s.overlay_digest(),
        after_new,
        "iteration 50 after 100 changes nothing"
    );
    assert_eq!(s.local().read(NEW_STRING).expect("reads"), new);
}

/// `0xF7E4`: the cache update records an asynchronous-source failure and takes no save path, so
/// the pending download **stays pending** and the client does not send
/// `0xF7EA`. A server that errors on every request is why the patch screen can hang.
#[test]
fn an_error_message_writes_nothing_and_leaves_the_download_pending() {
    let s = Scratch::new("error_message");
    let mut p = s.patcher();
    let untouched = s.overlay_digest();

    p.on_interrogation();
    let (expected, action) = p.on_begin(&begin(9, &[NEW_STRING.raw()], 4096));
    assert_eq!(expected, 4096, "no early saves, so nothing is subtracted");
    assert!(!action.send_end, "one download is outstanding");
    assert_eq!(p.pending(), 1);

    p.on_error(&DddError {
        resource_type: 0x21,
        resource_id: NEW_STRING.raw(),
        error: 1,
    });
    assert_eq!(p.pending(), 1, "an error does not retire the download");
    assert_eq!(p.applied(), 0);
    assert_eq!(s.overlay_digest(), untouched);

    let summary = p.on_end();
    assert_eq!(summary.still_pending, 1);
    assert!(
        !summary.changed_anything(),
        "nothing changed, so nothing needs invalidating"
    );
}

/// Behaviour: net.dat-patch.the-owners-client-directory-is-never-written
/// The production posture: a patch goes into the world's overlay and nowhere else. The installed
/// language file is byte-identical after a record lands; the data folder itself is refused as an
/// overlay folder; and a client with no overlay folder writes nothing at all and says so.
#[test]
fn a_patch_never_writes_the_installed_files() {
    let dir = dereth_dat::testing::dat_dir();
    assert!(
        dereth_dat::testing::have_dats(),
        "PREFLIGHT: set DERETH_TEST_DAT_DIR; {} has no language dat",
        dir.display()
    );
    let before = digest(&dereth_dat::RetailDat::Local.in_dir(&dir));

    let s = Scratch::new("installed_untouched");
    let mut p = s.patcher();
    let body = payload(0x77, 64);
    assert!(p.on_data(&data_msg(NEW_STRING, &body, 3, 0)).0.wrote());
    p.on_end();
    assert_eq!(
        s.local().read(NEW_STRING).expect("the world reads it"),
        body
    );
    assert!(
        !s.base_local().contains(NEW_STRING),
        "the installed file does not have it"
    );

    // The data folder is never an overlay folder.
    assert!(matches!(
        OverlayDir::new(&dir),
        Err(dereth_dat::overlay::OverlayError::BaseFolder(_))
    ));
    // With no overlay folder, nothing is written and the refusal is reported.
    let mut none = DddPatcher::new(None);
    let (outcome, _) = none.on_data(&data_msg(NEW_STRING, &body, 3, 0));
    assert!(
        matches!(outcome, DataOutcome::Refused(DddRefusal::OverlayRefused(_))),
        "{outcome:?}"
    );
    assert_eq!(
        digest(&dereth_dat::RetailDat::Local.in_dir(&dir)),
        before,
        "the installed language file is byte-identical"
    );
}

// ---------------------------------------------------------------------------------------------
// (d) End of DDD: the iteration list, the invalidation, and persistence across a reopen.
// ---------------------------------------------------------------------------------------------

/// The whole exchange, in the order a server drives it, with the two things `0xF7E7` promises
/// checked at the end: the record is there, and the file's `0xFFFF0001` iteration set has grown by
/// the revision's iteration.
///
/// When a revision's last download lands, the client adds its iteration, sorts and saves the
/// iteration list. Once the pending-download set is empty it enters the end-sent state and sends
/// `0xF7EA` itself.
#[test]
fn a_whole_exchange_records_its_iteration_and_asks_to_end() {
    let s = Scratch::new("whole_exchange");
    let mut p = s.patcher();

    let iterations_before = s.local().iteration_list().expect("a set");
    let next = iterations_before
        .iter()
        .copied()
        .max()
        .expect("a non-empty set")
        + 1;
    assert!(!iterations_before.contains(&next));

    p.on_interrogation();
    let body = payload(0xC3, 2048);
    let (_, action) = p.on_begin(&begin(next, &[NEW_STRING.raw()], body.len() as u32));
    assert!(!action.send_end);
    assert_eq!(p.phase(), DddPhase::Patching);

    let (outcome, action) = p.on_data(&data_msg(NEW_STRING, &body, 3, next));
    assert!(outcome.wrote(), "{outcome:?}");
    assert!(
        action.send_end,
        "the last pending download landed, so the client owes 0xF7EA"
    );
    assert_eq!(p.phase(), DddPhase::EndSent);
    assert_eq!(p.pending(), 0);

    let summary = p.on_end();
    assert_eq!(summary.applied, 1);
    assert_eq!(summary.still_pending, 0);
    assert_eq!(summary.changed, vec![DatTarget::Local]);
    assert_eq!(p.phase(), DddPhase::RunTime);

    let f = s.local();
    assert_eq!(f.read(NEW_STRING).expect("reads"), body);
    let after = f.iteration_list().expect("a set");
    assert!(
        after.contains(&next),
        "iteration {next} is in 0xFFFF0001: {after:?}"
    );
    assert_eq!(
        after.len(),
        iterations_before.len() + 1,
        "exactly one iteration was added"
    );
    s.assert_sound(DatTarget::Local);
}

/// A revision with **no** downloads is complete on arrival, so its iteration is recorded straight
/// away and the client owes `0xF7EA` immediately. The zero-download branch only adds, sorts, and
/// saves the iteration list. In retail this is how a cell revision containing only purges is
/// acknowledged.
#[test]
fn a_revision_with_nothing_to_download_records_its_iteration_immediately() {
    let s = Scratch::new("empty_revision");
    let mut p = s.patcher();
    let before = s.local().iteration_list().expect("a set");
    let next = before.iter().copied().max().expect("non-empty") + 1;

    let (_, action) = p.on_begin(&begin(next, &[], 0));
    assert!(
        action.send_end,
        "nothing to wait for, so 0xF7EA goes out at once"
    );
    let summary = p.on_end();
    assert_eq!(
        summary.changed,
        vec![DatTarget::Local],
        "the iteration list was rewritten"
    );

    let after = s.local().iteration_list().expect("a set");
    assert!(after.contains(&next), "{after:?}");
}

/// A revision that asks for a **purge** completes.
///
/// The begin-request worker walks `IDsToPurge` before the revision can finish. Type 1 uses a
/// `0xFFFF0000` mask through the selected disk controller; every other type deletes only the
/// named ID from its own file. Language-dat IDs derive type `0x25`, so this revision takes the
/// single-record arm without a mask.
///
/// The whole revision lands and its iteration is recorded -- which tells the server on the next
/// login that it does not need to send this revision again.
#[test]
fn a_revision_with_purges_completes_and_records_its_iteration() {
    let s = Scratch::new("purge_done");
    let mut p = s.patcher();

    let (before, victims) = {
        let f = s.local();
        let victims: Vec<DataId> = f
            .iter_ids()
            .filter(|i| *i != dereth_dat::ITERATION_LIST)
            .take(2)
            .collect();
        assert_eq!(
            victims.len(),
            2,
            "the language dat must have records to purge"
        );
        (f.iteration_list().expect("a set"), victims)
    };
    let next = before.iter().copied().max().expect("non-empty") + 1;

    let mut m = begin(next, &[NEW_STRING.raw()], 2048);
    m.revisions[0].ids_to_purge = victims.iter().map(|v| v.raw()).collect();
    let (_, action) = p.on_begin(&m);
    assert!(!action.send_end, "the download is still outstanding");
    assert!(
        p.notices().iter().all(|n| !n.contains("not implemented")),
        "nothing about this revision is refused any more: {:?}",
        p.notices()
    );

    // The purge happens at `0xF7E7`, before the downloads land: the begin-request worker applies
    // purges before the request-finished step performs iteration bookkeeping.
    {
        let mid = s.local();
        for v in &victims {
            assert!(!mid.contains(*v), "{v:?} should already be gone");
        }
    }

    let body = payload(0x77, 900);
    let (outcome, action) = p.on_data(&data_msg(NEW_STRING, &body, 3, next));
    assert!(outcome.wrote(), "{outcome:?}");
    assert!(
        action.send_end,
        "nothing is left pending, so the client owes 0xF7EA"
    );
    let summary = p.on_end();
    assert!(summary.changed_anything(), "{summary:?}");

    let f = s.local();
    assert_eq!(
        f.read(NEW_STRING).expect("reads"),
        body,
        "the download landed"
    );
    for v in &victims {
        assert!(!f.contains(*v), "{v:?} survived the purge");
    }
    let after = f.iteration_list().expect("a set");
    assert!(
        after.contains(&next),
        "the completed revision's iteration is recorded: {after:?}"
    );
    s.assert_sound(DatTarget::Local);
}

/// The cell dat, which is the file a purge actually arrives for: ACE's own note is that "PCAPs
/// show Cell updates were purges only, while the other two were downloads", and a cell revision
/// carries no downloads at all; the empty-`IDsToDownload` branch completes it on arrival.
///
/// Every cell purge takes the mask arm because cell-file ID serialization sets type 1
/// **unconditionally**, without consulting the general type mapping. The whole `0xXXXX0000`
/// landblock family therefore goes, not just the named ID: here it is a family tombstone in the
/// world's overlay, and the installed cell file keeps every record.
#[test]
fn a_cell_revision_purges_the_whole_landblock_family_and_completes_on_arrival() {
    let mut s = Scratch::new("purge_cell");
    s.watch_cell();
    let mut p = s.patcher();

    let block = 0xA9B4u32;
    let (before, family) = {
        let f = s.world().cell().clone();
        let family: Vec<DataId> = f.iter_ids().filter(|i| i.raw() >> 16 == block).collect();
        assert!(family.len() > 1, "the fixture landblock must have a family");
        (f.iteration_list().expect("a set"), family)
    };
    let next = before.iter().copied().max().expect("non-empty") + 1;

    // One revision for `(1, 2)`, no downloads, one purge naming the landblock record.
    let m = DddBeginDdd {
        data_expected: 0,
        revisions: vec![PatchRevision {
            dat_file_type: 1,
            dat_file_id: 2,
            iteration: next,
            ids_to_download: Vec::new(),
            ids_to_purge: vec![(block << 16) | 0xFFFF],
        }],
    };
    let (_, action) = p.on_begin(&m);
    assert!(
        action.send_end,
        "a revision with nothing to download is complete on arrival"
    );
    let summary = p.on_end();
    assert_eq!(summary.changed, vec![DatTarget::Cell], "{summary:?}");

    let f = s.world().cell().clone();
    for id in &family {
        assert!(!f.contains(*id), "{id:?} survived the mask purge");
    }
    // At least one record from outside the purged landblock family remains.
    assert!(
        f.iter_ids().any(|i| i.raw() >> 16 != block),
        "the rest of the cell dat is still there"
    );
    let after = f.iteration_list().expect("a set");
    assert!(
        after.contains(&next),
        "the cell revision's iteration is recorded: {after:?}"
    );
    for id in &family {
        assert!(
            s.store.cell().contains(*id),
            "{id:?} is still in the installed file"
        );
    }
    s.assert_sound(DatTarget::Cell);
}

/// A revision whose purges cannot be *performed* withholds its iteration however many downloads
/// land.
///
/// The iteration set at `0xFFFF0001` is what the server reads on the next login to decide what to
/// send; recording an iteration whose purges never happened would tell it a revision landed that
/// did not, and it would never offer that revision again. Withholding it makes the next login
/// offer the whole revision afresh, which is the recoverable failure.
///
/// The failure is manufactured the way a real one would arise: the overlay folder cannot be
/// written (it is declared read-only for the run), so the purge's tombstone cannot be kept.
#[test]
fn a_revision_whose_purges_fail_still_withholds_its_iteration() {
    let s = Scratch::new("purge_failed");
    std::fs::create_dir_all(s.overlay.path()).expect("the overlay folder");
    dereth_dat::protect_install(s.overlay.path());
    let mut p = s.patcher();

    let victim = {
        let f = s.local();
        let id = f.iter_ids().find(|i| *i != dereth_dat::ITERATION_LIST);
        id.expect("the language dat has records")
    };

    let before = s.local().iteration_list().expect("a set");
    let next = before.iter().copied().max().expect("non-empty") + 1;
    let m = DddBeginDdd {
        data_expected: 0,
        revisions: vec![PatchRevision {
            dat_file_type: 1,
            dat_file_id: 3,
            iteration: next,
            ids_to_download: Vec::new(),
            ids_to_purge: vec![victim.raw()],
        }],
    };
    let (_, action) = p.on_begin(&m);
    assert!(
        p.notices()
            .iter()
            .any(|n| n.contains("purge") && n.contains("failed")),
        "the failure is reported: {:?}",
        p.notices()
    );
    assert!(
        action.send_end,
        "the protocol half still finishes: there is nothing to download"
    );
    p.on_end();

    let after = s.local().iteration_list().expect("a set");
    assert!(
        !after.contains(&next),
        "iteration {next} must not be recorded while its purges failed: {after:?}"
    );
    assert_eq!(after, before, "the iteration set is untouched");
}

/// **Cache invalidation.** A `DatFile` caches the whole B-tree at open, so a reader that was
/// already open when the patch landed is *stale* — and not harmlessly so: the entry it holds names
/// a chain that is now on the free list. `DatFile::reload` / `RetailDatStore::reload` is what
/// `App::invalidate_after_ddd` calls, and this is the test that shows it is needed.
#[test]
fn a_reader_open_across_the_patch_is_stale_until_it_is_reloaded() {
    let s = Scratch::new("invalidation");
    let mut p = s.patcher();

    // The reader that was up before the patch. Replace a *shipped* id, so the stale reader has a
    // stale entry rather than no entry — the failure mode that matters.
    let mut reader = s.local();
    let victim = reader
        .iter_ids()
        .find(|i| *i != dereth_dat::ITERATION_LIST)
        .expect("the language dat has records");
    let old = reader.read(victim).expect("it reads");
    let new = payload(0x3C, 4000);
    assert_ne!(old, new);

    assert!(p.on_data(&data_msg(victim, &new, 3, 0)).0.wrote());
    p.on_end();

    // Before the reload the open reader answers with the *old* record — or fails, because the
    // chain it names has been freed. Either way it is not the new bytes, which is the whole point.
    let stale = reader.read(victim);
    assert_ne!(
        stale.as_deref().ok(),
        Some(new.as_slice()),
        "an open reader cannot see a patch without being told"
    );

    // A reader opened before the patch has no overlay container to reopen; the store reopened
    // over the overlay (`App::invalidate_after_ddd` does this) sees the record.
    reader.reload().expect("the reader reloads");
    assert_ne!(
        reader.read(victim).ok(),
        Some(new.clone()),
        "the installed file is unchanged"
    );
    assert_eq!(
        s.local().read(victim).expect("reads"),
        new,
        "reopened over the overlay it is the new record"
    );
    let mut world = s.world();
    world.reload().expect("the world reloads");
    assert_eq!(world.local().read(victim).expect("reads"), new);
    s.assert_sound(DatTarget::Local);
}

/// Behaviour: net.dat-patch.a-downloaded-record-is-written-and-seen-after-reopen
/// Persistence across fresh opens in the same process: `on_end` drops every patch writer, and new
/// `DatFile` readers opened afterwards see the record.
#[test]
fn a_patched_record_survives_a_fresh_open() {
    let s = Scratch::new("persistence");
    let body = payload(0x6E, 1500);
    {
        let mut p = s.patcher();
        assert!(p.on_data(&data_msg(NEW_STRING, &body, 3, 5)).0.wrote());
        let summary = p.on_end();
        assert!(summary.changed_anything());
    } // every writer closed here

    // Two fresh `DatFile` opens check payload and iteration; a raw length check independently
    // confirms only that the file remains above the minimum expected size.
    assert_eq!(s.local().read(NEW_STRING).expect("reads"), body);
    let raw = std::fs::read(s.container(DatTarget::Local)).expect("the overlay reads as bytes");
    assert!(raw.len() >= 1024, "still a container");
    assert_eq!(s.local().entry(NEW_STRING).expect("the entry").iteration, 5);
}

// ---------------------------------------------------------------------------------------------
// (e) Ordinary play with no DDD is unchanged.
// ---------------------------------------------------------------------------------------------

/// A session in which the server says "you are up to date" touches nothing: `DDDHandler`'s last
/// branch sends `GameMessageDDDEndDDD` and no `0xF7E7` at all, so the patcher never opens a dat
/// for writing and `on_end` reports no change — which is what tells `App` not to invalidate.
#[test]
fn a_session_with_no_patch_opens_no_writer_and_changes_nothing() {
    let s = Scratch::new("no_patch");
    let mut p = s.patcher();
    let untouched = s.overlay_digest();

    p.on_interrogation();
    let summary = p.on_end();

    assert_eq!(
        summary,
        Default::default(),
        "an untouched exchange is the default summary"
    );
    assert!(!summary.changed_anything());
    assert!(p.notices().is_empty(), "{:?}", p.notices());
    assert_eq!(s.overlay_digest(), untouched);
}

/// The queue-5 decoder in front of all of this: the bytes a server sends are what the patcher
/// receives. Built with `dereth_protocol`, dispatched by `dereth_client_net::client_session`'s own queue-5 handler, and the
/// resulting event is the one `App` routes — so the wire layout is part of this suite rather than
/// something the tests above assume.
#[test]
fn the_wire_bytes_decode_to_the_event_the_patcher_applies() {
    let s = Scratch::new("wire");
    let mut p = s.patcher();

    let body = payload(0x9D, 600);
    let blob =
        dereth_protocol::write_blob(&data_msg(NEW_STRING, &body, 3, 12)).expect("it encodes");
    // ACE's own field order, asserted against the bytes rather than against the struct: type, id,
    // resource type, resource id, iteration, one compressed byte, version, size, payload.
    let mut want = Vec::new();
    for v in [0xF7E2u32, 1, 3, 0x21, NEW_STRING.raw(), 12] {
        want.extend_from_slice(&v.to_le_bytes());
    }
    want.push(0);
    want.extend_from_slice(&3u32.to_le_bytes());
    want.extend_from_slice(&(body.len() as u32 + 4).to_le_bytes());
    want.extend_from_slice(&body);
    assert_eq!(
        blob, want,
        "the 0xF7E2 body is GameMessageDDDDataMessage's field order"
    );

    let mut state = dereth_client_net::client_session::DddState::Patching;
    let incoming = dereth_primitives::IncomingMessage {
        opcode: 0xF7E2,
        queue: dereth_primitives::NetQueue::ClientCache,
        sender: dereth_primitives::RecipientId(0),
        blob_id: dereth_primitives::NetBlobId(0),
        // `dispatch` is handed the body after the opcode is read off, matching the original cache
        // update's four-byte opcode read before its switch.
        body: blob[4..].to_vec(),
    };
    let d = dereth_client_net::client_session::dispatch::database::dispatch(&mut state, &incoming);
    let dereth_client_net::client_session::SessionEvent::Ddd(
        dereth_client_net::client_session::DddEvent::Data(decoded),
    ) = d.event
    else {
        panic!(
            "queue 5 should decode 0xF7E2 as a data event, got {:?}",
            d.event
        );
    };
    let (outcome, _) = p.on_data(&decoded);
    assert!(outcome.wrote(), "{outcome:?}");
    assert_eq!(s.local().read(NEW_STRING).expect("reads"), body);
}

// ---------------------------------------------------------------------------------------------
// The App hop: a real `App`, a real `Session`, a real blob, and the client directory's dats afterwards.
// ---------------------------------------------------------------------------------------------

/// **This is the production path, end to end.** A `0xF7E2` blob is handed to the transport; the
/// real `ClientNetwork` reassembles it, the real `Session` dispatches it on queue 5, `App`'s own
/// `0xF7E2` arm hands it to its `DddPatcher`, and the patcher answers to `App::ddd()`.
///
/// What it demonstrates is where the record goes: `App`'s patcher writes into the world's
/// overlay folder (`--overlay-dat-dir`, here a scratch folder), and the installed data files,
/// hashed before and after, are untouched.
///
/// The application uses a `NullPresentation`; this station exercises transport, session, app,
/// patcher, and UI host-state routing without creating a GPU device.
#[test]
fn a_data_message_off_the_wire_reaches_the_apps_patcher() {
    use dereth_client::app::App;
    use dereth_client::config::Config;
    use dereth_client::net::ClientNetwork;

    let dir = dereth_dat::testing::dat_dir();
    assert!(
        dereth_dat::testing::have_dats(),
        "PREFLIGHT: set DERETH_TEST_DAT_DIR; {} has no language dat",
        dir.display()
    );
    let before: Vec<_> = [
        "client_portal.dat",
        "client_cell_1.dat",
        "client_local_English.dat",
    ]
    .iter()
    .map(|n| (dir.join(n), digest(&dir.join(n))))
    .collect();
    let scratch =
        dereth_dat::testing::ScratchDir::new("dat-patch-app").expect("a scratch directory");
    let overlay = OverlayDir::new(&scratch.path().join("overlay")).expect("an overlay folder");

    // `ui: true` and the shell, because the screen hop is part of what is being shown:
    // `App::build_host_state` -- which drains `pending_ddd` into `HostState::ddd` -- runs inside
    // the UI path and nowhere else.
    let mut app = App::with_presentation(
        Config {
            headless: true,
            sound: false,
            ui: true,
            preferences_file: std::env::temp_dir().join("dereth-dat-patch-not-created/prefs.ini"),
            dat_dir: dir.clone(),
            overlay_dat_dir: Some(overlay.path().to_path_buf()),
            ..Default::default()
        },
        Box::new(NullPresentation::new(800, 600)),
    )
    .expect("the application comes up with a null presentation");
    app.start_shell().expect("the UI shell comes up");

    // A socket-free replay endpoint using the same transport construction as the login stations.
    let mut net = ClientNetwork::new("127.0.0.1:19000", 7304, "dat-patch", "unused", 0)
        .expect("a socket-free network client");
    net.session.transport.add_connection(
        0xB,
        0,
        1,
        0xDEAD_BEEF,
        0x1234_5678,
        Some("127.0.0.1:19000".parse().expect("a literal address")),
    );
    let mut crypto = dereth_transport::CryptoSystem::new(0xDEAD_BEEF);
    app.attach_replay_network(net)
        .map_err(|_| ())
        .expect("the endpoint attaches");

    // One blob on queue 5 -- `NetQueue::ClientCache`, where the opcode table puts every `0xF7Ex`.
    let mut seq = 1u32;
    let mut blob_id = 0u32;
    let mut feed = |app: &mut App, bytes: Vec<u8>| {
        seq += 1;
        blob_id += 1;
        let mut packet = dereth_transport::OutPacket::new(dereth_transport::ProtoHeader {
            seq_id: seq,
            rec_id: 0xB,
            interval: 0x100,
            iteration: 1,
            ..Default::default()
        });
        packet
            .add_fragment(dereth_transport::Fragment::new(
                dereth_transport::FragmentHeader {
                    blob_id_low: blob_id,
                    blob_id_high: 0x8000_0000,
                    num_frags: 1,
                    blob_frag_size: 0,
                    blob_num: 0,
                    queue_id: 5,
                },
                bytes,
            ))
            .expect("one fragment fits");
        let raw = packet
            .serialize(Some(crypto.next()))
            .expect("the envelope serialises");
        app.replay_network_mut()
            .expect("an explicitly socket-free endpoint")
            .session
            .transport
            .feed(&raw, None, dereth_primitives::LocalTime(0.0))
            .expect("the transport accepts its own envelope");
    };

    // A blob is never acted on in the frame it is fed; `HostState::ddd` is *drained* every frame
    // (it is a queue the screen consumes), so the events are collected as they go.
    let mut seen: Vec<dereth_ui_screens::screens::datapatch::DddEvent> = Vec::new();
    let run = |app: &mut App, n: u32, seen: &mut Vec<_>| {
        for i in 0..n {
            assert!(app.frame(), "frame {i} does not end the client");
            seen.extend(app.host_state().ddd.iter().cloned());
        }
    };

    let body = payload(0x4D, 256);

    // 1. `0xF7E7 DDD_BeginDDDMessage` -- one revision, one download.
    feed(
        &mut app,
        dereth_protocol::write_blob(&begin(9, &[NEW_STRING.raw()], 4096)).expect("0xF7E7"),
    );
    run(&mut app, 2, &mut seen);
    assert_eq!(
        app.ddd().phase(),
        DddPhase::Patching,
        "0xF7E7 reached the patcher"
    );
    assert_eq!(app.ddd().pending(), 1, "one download is outstanding");
    assert!(
        seen.contains(
            &dereth_ui_screens::screens::datapatch::DddEvent::PatchtimeBegin { expected: 4096 }
        ),
        "the patch screen is told how much to expect: {seen:?}"
    );

    // 2. `0xF7E4 DDD_ErrorMessage` -- reported, nothing written, still pending.
    feed(
        &mut app,
        dereth_protocol::write_blob(&DddError {
            resource_type: 0x21,
            resource_id: NEW_STRING.raw(),
            error: 1,
        })
        .expect("0xF7E4"),
    );
    run(&mut app, 2, &mut seen);
    assert!(
        app.ddd().notices().iter().any(|n| n.contains("0xF7E4")),
        "the error reached the patcher: {:?}",
        app.ddd().notices()
    );
    assert_eq!(
        app.ddd().pending(),
        1,
        "an error does not retire the download"
    );

    // 3. `0xF7E2 DDD_DataMessage` -- the record itself.
    feed(
        &mut app,
        dereth_protocol::write_blob(&data_msg(NEW_STRING, &body, 3, 1)).expect("0xF7E2"),
    );
    run(&mut app, 2, &mut seen);

    // The screen receives a data-downloaded event carrying the payload byte count.
    assert!(
        seen.contains(
            &dereth_ui_screens::screens::datapatch::DddEvent::DataDownloaded {
                bytes: body.len() as u64,
            }
        ),
        "the patch screen sees the download: {seen:?}"
    );

    // And the patcher wrote it into the world's overlay, not the installed files.
    assert_eq!(app.ddd().applied(), 1, "{:?}", app.ddd().notices());
    let container = overlay.container(DatTarget::Local);
    assert!(
        DatFile::open(&container)
            .expect("the overlay container")
            .contains(NEW_STRING),
        "the record is in the overlay"
    );
    for (path, was) in &before {
        assert_eq!(&digest(path), was, "{} changed", path.display());
    }

    // 4. `0xF7EA DDD_OnEndDDD` closes the exchange and lets the application process the summary.
    // Nothing changed, so no dat reader is reopened.
    feed(
        &mut app,
        dereth_protocol::write_blob(&dereth_protocol::admin::DddEndDdd).expect("0xF7EA"),
    );
    run(&mut app, 2, &mut seen);
    assert_eq!(
        app.ddd().phase(),
        DddPhase::RunTime,
        "0xF7EA reached the patcher"
    );
    assert!(
        seen.contains(&dereth_ui_screens::screens::datapatch::DddEvent::PatchtimeEnd),
        "the patch screen is told the phase is over: {seen:?}"
    );
    for (path, was) in &before {
        assert_eq!(
            &digest(path),
            was,
            "{} changed after the whole exchange",
            path.display()
        );
    }
}

// ---------------------------------------------------------------------------------------------
// The iteration list as one of a revision's downloads (V290).
// ---------------------------------------------------------------------------------------------

/// Behaviour: net.dat-patch.the-iteration-list-counts-as-delivered
/// ACE lists the dat's iteration list `0xFFFF0001` among iteration 1's files, and the server now
/// sends it (with type 37 in the language dat) rather than dying on it. It counts as delivered:
/// the revision completes when its other file lands and the client owes its `0xF7EA`. The list on
/// disk is then the client's own plus the new iteration, not the server's copy, which here
/// claims 5000 iterations.
#[test]
fn the_servers_iteration_list_counts_as_delivered_and_is_not_written() {
    let s = Scratch::new("rr82_iteration_list");
    let mut p = s.patcher();
    let before = s.local().iteration_list().expect("its list reads");
    let next = before.last().copied().expect("a list") + 1;

    let list = dereth_dat::ITERATION_LIST;
    let (_, action) = p.on_begin(&begin(next, &[NEW_STRING.raw(), list.raw()], 0));
    assert!(!action.send_end, "two downloads are pending");

    let servers = dereth_dat::iteration::encode(&(1..=5000).collect::<Vec<u32>>());
    let (outcome, action) = p.on_data(&DddData {
        resource_type: 37,
        ..data_msg(list, &servers, 3, 1)
    });
    assert_eq!(
        outcome,
        DataOutcome::IterationListKept {
            target: DatTarget::Local
        }
    );
    assert!(!action.send_end, "the string is still pending");
    assert_eq!(p.pending(), 1);

    let (outcome, action) = p.on_data(&data_msg(NEW_STRING, &payload(0x11, 200), 3, next));
    assert!(
        matches!(
            outcome,
            DataOutcome::Applied {
                save: SaveOutcome::Added,
                ..
            }
        ),
        "{outcome:?}"
    );
    assert!(
        action.send_end,
        "the last download retires the revision and the client ends DDD"
    );

    let mut want = before;
    want.push(next);
    let f = s.local();
    assert_eq!(
        f.iteration_list().expect("the list reads"),
        want,
        "the client's list gained the one iteration"
    );
    s.assert_sound(DatTarget::Local);
}

// ---------------------------------------------------------------------------------------------
// The overlay extension: the world's manifest, ahead of its patch.
// ---------------------------------------------------------------------------------------------

/// The manifest a server that knows the extension sends for one language-file revision.
fn manifest(
    s: &Scratch,
    world: &str,
    records: &[(DataId, &[u8])],
    tombstones: &[(u32, u32)],
    iteration: u32,
) -> dereth_protocol::admin::DddOverlayManifest {
    use dereth_protocol::admin::{OverlayFileManifest, OverlayRecord, OverlayTombstone};
    dereth_protocol::admin::DddOverlayManifest {
        world_key: world.into(),
        total_bytes: 0,
        files: vec![OverlayFileManifest {
            dat_file_type: 1,
            dat_file_id: 3,
            base_name: "client_local_English.dat".into(),
            base_fingerprint: dereth_dat::overlay::fingerprint(s.base_local()),
            base_iterations: 0,
            exact_iterations: false,
            revisions: vec![iteration],
            records: records
                .iter()
                .map(|(id, b)| OverlayRecord {
                    id: id.raw(),
                    iteration,
                    size: b.len() as u32,
                    sha256: dereth_dat::overlay::record_hash(b),
                })
                .collect(),
            tombstones: tombstones
                .iter()
                .map(|(id, mask)| OverlayTombstone {
                    id: *id,
                    mask: *mask,
                    iteration,
                })
                .collect(),
        }],
    }
}

/// The next iteration the installed language file does not have.
fn next_iteration(s: &Scratch) -> u32 {
    s.base_local()
        .iteration_list()
        .expect("a set")
        .last()
        .copied()
        .unwrap_or(0)
        + 1
}

/// Behaviour: net.dat-patch.an-overlay-the-client-cannot-take-is-refused-and-nothing-is-written
/// A world's overlay is refused, reported, and nothing of it written, when its manifest names
/// another base than the file the client holds, when the world is on the player's blocklist, or
/// when the folder already holds another world's overlay; the patch then ends at once and the
/// world is read from the installed files alone.
#[test]
fn an_overlay_made_against_another_base_or_for_a_blocked_or_other_world_is_refused() {
    let next = 9999;
    let body = payload(0x42, 300);
    // Another base.
    let s = Scratch::new("other_base");
    let mut p = s.patcher();
    let mut m = manifest(&s, WORLD, &[(NEW_STRING, &body)], &[], next);
    m.files[0].base_fingerprint = [0xAB; 32];
    p.on_manifest(&m);
    assert!(
        p.refused().is_some_and(|r| r.contains("made against")),
        "{:?}",
        p.refused()
    );
    let (_, action) = p.on_begin(&begin(next, &[NEW_STRING.raw()], 300));
    assert!(action.send_end, "a refused patch ends at once");
    assert!(matches!(
        p.on_data(&data_msg(NEW_STRING, &body, 3, next)).0,
        DataOutcome::Refused(DddRefusal::OverlayRefused(_))
    ));
    p.on_end();
    assert_eq!(s.overlay_digest(), None, "no overlay container was made");

    // A blocked world.
    let s = Scratch::new("blocked");
    let mut p = s.patcher();
    p.set_blocklist(["a bad world".to_owned()]);
    p.on_manifest(&manifest(
        &s,
        "a bad world",
        &[(NEW_STRING, &body)],
        &[],
        next,
    ));
    assert!(p.refused().is_some_and(|r| r.contains("blocklist")));
    assert!(!p.on_data(&data_msg(NEW_STRING, &body, 3, next)).0.wrote());
    assert_eq!(s.overlay_digest(), None);

    // A folder holding another world's overlay.
    let s = Scratch::new("other_world");
    let mut first = s.patcher();
    assert!(first.on_data(&data_msg(NEW_STRING, &body, 3, 0)).0.wrote());
    first.on_end();
    let held = s.overlay_digest();
    let mut p = s.patcher();
    p.on_manifest(&manifest(
        &s,
        "another world",
        &[(NEW_STRING, &body)],
        &[],
        next,
    ));
    assert!(
        p.refused().is_some_and(|r| r.contains("another world")),
        "{:?}",
        p.refused()
    );
    assert!(!p.on_data(&data_msg(NEW_STRING, &body, 3, next)).0.wrote());
    assert_eq!(
        s.overlay_digest(),
        held,
        "the other world's overlay is untouched"
    );
}

/// Behaviour: net.dat-patch.a-record-the-manifest-does-not-name-is-refused
/// With the world's manifest in hand, a record whose bytes are not the ones it names is refused
/// and not written; the one it names is written, and the revision completes when it lands.
#[test]
fn a_record_whose_bytes_are_not_the_manifests_is_refused() {
    let s = Scratch::new("manifest_hash");
    let mut p = s.patcher();
    let next = next_iteration(&s);
    let body = payload(0x24, 700);
    p.on_manifest(&manifest(&s, WORLD, &[(NEW_STRING, &body)], &[], next));
    assert_eq!(p.refused(), None);
    p.on_interrogation();
    let (_, action) = p.on_begin(&begin(next, &[NEW_STRING.raw()], 700));
    assert!(!action.send_end);
    let forged = payload(0x25, 700);
    assert_eq!(
        p.on_data(&data_msg(NEW_STRING, &forged, 3, next)).0,
        DataOutcome::Refused(DddRefusal::HashMismatch {
            id: NEW_STRING.raw()
        })
    );
    let (outcome, action) = p.on_data(&data_msg(NEW_STRING, &body, 3, next));
    assert!(outcome.wrote(), "{outcome:?}");
    assert!(action.send_end, "the revision's one download landed");
    p.on_end();
    assert_eq!(s.local().read(NEW_STRING).expect("reads"), body);
    assert!(s.local().iteration_list().expect("a set").contains(&next));
}

/// Behaviour: net.dat-patch.a-deletion-the-manifest-names-hides-that-record-alone
/// A deletion only the manifest can say -- one record, where a retail purge of the cell file
/// takes a whole landblock -- hides that record alone, in the revision that made it.
#[test]
fn a_deletion_the_manifest_names_hides_that_record_alone() {
    let s = Scratch::new("manifest_tombstone");
    let mut p = s.patcher();
    let ids: Vec<DataId> = s
        .base_local()
        .iter_ids()
        .filter(|i| *i != dereth_dat::ITERATION_LIST)
        .take(2)
        .collect();
    let next = next_iteration(&s);
    p.on_manifest(&manifest(&s, WORLD, &[], &[(ids[0].raw(), 0)], next));
    let (_, action) = p.on_begin(&begin(next, &[], 0));
    assert!(action.send_end);
    p.on_end();
    let world = s.local();
    assert!(!world.contains(ids[0]), "the named record is hidden");
    assert!(world.contains(ids[1]), "its neighbour stands");
    assert!(
        s.base_local().contains(ids[0]),
        "the installed file still holds it"
    );
    assert!(world.iteration_list().expect("a set").contains(&next));
}

/// Behaviour: net.dat-patch.a-record-the-manifest-does-not-name-is-refused
/// Records may arrive ahead of the world's manifest (a long manifest is overtaken by short
/// records). Those the session wrote go into containers under the name the client knew the world
/// by, take the world's own name when the manifest arrives, and are held to it then: one whose
/// bytes the manifest does not name refuses the world's overlay, and the containers this session
/// made go with it.
#[test]
fn records_ahead_of_the_manifest_take_the_worlds_name_and_are_held_to_it() {
    let next = 9998;
    let body = payload(0x31, 400);
    let early = |s: &Scratch| {
        let mut p = DddPatcher::new(Some(OverlayTarget::new(
            s.overlay.clone(),
            &s.store,
            "127.0.0.1:19661",
        )));
        p.on_interrogation();
        assert!(p.on_data(&data_msg(NEW_STRING, &body, 3, next)).0.wrote());
        p
    };
    let s = Scratch::new("early_adopted");
    let mut p = early(&s);
    p.on_manifest(&manifest(&s, WORLD, &[(NEW_STRING, &body)], &[], next));
    assert_eq!(p.refused(), None, "{:?}", p.notices());
    let (_, action) = p.on_begin(&begin(next, &[NEW_STRING.raw()], 400));
    assert!(action.send_end, "the early record retired its download");
    p.on_end();
    assert_eq!(s.overlay.world_key().as_deref(), Some(WORLD));
    assert_eq!(s.local().read(NEW_STRING).expect("reads"), body);

    let s = Scratch::new("early_forged");
    let mut p = early(&s);
    let other = payload(0x32, 400);
    p.on_manifest(&manifest(&s, WORLD, &[(NEW_STRING, &other)], &[], next));
    assert!(
        p.refused().is_some_and(|r| r.contains("not the record")),
        "{:?}",
        p.refused()
    );
    assert_eq!(
        s.overlay_digest(),
        None,
        "the session's container went with the refusal"
    );
}

/// Behaviour: net.dat-patch.a-malformed-or-older-record-writes-nothing
/// The routing rail reads a cell id by its shape: a room of landblock `0x2562`, whose number also
/// lies in a portal type's range, is a cell record and is written to the cell file's overlay; a
/// portal id offered for the cell file is still refused.
#[test]
fn a_room_whose_landblock_number_is_a_portal_range_is_still_a_cell_record() {
    let s = Scratch::new("cell_rail");
    let mut p = s.patcher();
    let cell = |id: u32| DddData {
        dat_file_type: 1,
        dat_file_id: 2,
        resource_type: 3,
        resource_id: id,
        ..data_msg(DataId(id), &payload(0x41, 96), 3, 0)
    };
    assert!(p.on_data(&cell(0x2562_0100)).0.wrote());
    assert!(matches!(
        p.on_data(&cell(0x0100_0001)).0,
        DataOutcome::Refused(DddRefusal::WrongDatFile { .. })
    ));
    p.on_end();
    assert!(s.world().cell().contains(DataId(0x2562_0100)));
}
