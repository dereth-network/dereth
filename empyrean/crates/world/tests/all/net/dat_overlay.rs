//! Divergence: V437, V438
//! A world served as base data files with a data overlay over them: a client that keeps overlays
//! (the overlay flag and its bases in `DDD_InterrogationResponseMessage`, `0xF7E6`) is sent the
//! overlay manifest (`0xF7EC`), then a `DDD_BeginDDDMessage` (`0xF7E7`) whose revisions download
//! the overlay's records, the world's cell records among them, and purge its deletions, then the
//! records; one holding another base is refused with the reason; a client without the flag is
//! answered as ACE answers it. On the February 2005 files, whose world is otherwise never patched,
//! a client that keeps overlays is patched too.
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`), the February 2005 dats
//! (`DERETH_TEST_PRETOD_DAT_DIR`) and an overlay written in a scratch folder.

use std::path::Path;
use std::sync::Arc;

use dereth_dat::overlay::{OverlayDir, OverlayWriter};
use dereth_dat::{DatFile, ModernDat, RetailDatStore};
use dereth_primitives::{DataId, NetQueue};
use dereth_protocol::admin::{
    DddBeginDdd, DddData, DddInterrogation, DddInterrogationResponse, DddOverlayManifest,
    MostlyConsecutiveIntSet, OverlayBase, TaggedIterationList,
};
use empyrean_dat::{DatManager, RealDats};
use empyrean_entity::enums::AccessLevel;
use empyrean_testkit::{ClientId, TestServer};

const WORLD: &str = "an overlay world";
/// A portal id no retail record uses, in the pictures' range.
const ADDED: DataId = DataId(0x0600_FFF0);

/// What the hand-made overlay changes, and the revisions it put them in.
struct Made {
    replaced: DataId,
    deleted: DataId,
    cell_replaced: DataId,
    cell_deleted: DataId,
    portal_revision: u32,
    cell_revision: u32,
}

fn revision(f: &DatFile) -> u32 {
    dereth_dat::decompose::revision_over(f)
}

/// An overlay over `store`'s portal and cell files: one record added, one replaced and one deleted
/// in the portal file, one room replaced and one deleted in the cell file.
fn make_overlay(
    store: &RetailDatStore,
    dir: &OverlayDir,
    portal_name: &str,
    cell_name: &str,
) -> Made {
    let portal = store.portal().base();
    let pictures: Vec<DataId> = portal
        .iter_ids()
        .filter(|i| i.raw() >> 24 == 0x06)
        .take(2)
        .collect();
    let (replaced, deleted) = (pictures[0], pictures[1]);
    let portal_revision = revision(&portal);
    let mut w = OverlayWriter::open_or_create(
        &dir.container(ModernDat::Portal),
        &portal,
        portal_name,
        WORLD,
        1,
    )
    .expect("the portal overlay");
    w.save(&portal, ADDED, b"an added picture", 1, portal_revision, 1)
        .expect("added");
    let mut new = portal.read(replaced).expect("the picture");
    new[0] ^= 0xFF;
    w.save(&portal, replaced, &new, 1, portal_revision, 1)
        .expect("replaced");
    w.tombstone(deleted, 0, portal_revision).expect("deleted");
    w.add_iteration(portal_revision, 1).expect("the revision");
    w.flush(1).expect("flushed");

    let cell = store.cell().base();
    let rooms: Vec<DataId> = cell
        .iter_ids()
        .filter(|i| i.raw() >> 16 == 0xA9B4 && (i.raw() & 0xFFFF) < 0xFFFE)
        .take(2)
        .collect();
    let (cell_replaced, cell_deleted) = (rooms[0], rooms[1]);
    let cell_revision = revision(&cell);
    let mut c =
        OverlayWriter::open_or_create(&dir.container(ModernDat::Cell), &cell, cell_name, WORLD, 1)
            .expect("the cell overlay");
    let room = cell.read(cell_replaced).expect("the room");
    c.save(&cell, cell_replaced, &room, 2, cell_revision, 1)
        .expect("the room");
    c.tombstone(cell_deleted, 0, cell_revision)
        .expect("deleted");
    c.add_iteration(cell_revision, 1).expect("the revision");
    c.flush(1).expect("flushed");
    Made {
        replaced,
        deleted,
        cell_replaced,
        cell_deleted,
        portal_revision,
        cell_revision,
    }
}

/// The list a client holding `f` sends: one run of its iterations.
fn run(f: &DatFile) -> MostlyConsecutiveIntSet {
    let n = i32::try_from(dereth_dat::overlay::base_iterations(f).len()).expect("a count");
    MostlyConsecutiveIntSet {
        iterations: n,
        ints: vec![-n, 1],
    }
}

/// The interrogation response of a client holding `store`'s files, with the overlay flag and its
/// bases when `keeps_overlays`.
fn response(store: &RetailDatStore, keeps_overlays: bool) -> DddInterrogationResponse {
    let files = [
        (0, 1, store.portal()),
        (1, 2, store.cell()),
        (1, 3, store.local()),
    ];
    DddInterrogationResponse {
        client_language: 1,
        iters_with_keys: files
            .iter()
            .map(|&(dat_file_type, dat_file_id, f)| TaggedIterationList {
                dat_file_type,
                dat_file_id,
                iterations: run(f),
            })
            .collect(),
        iters_without_keys: Vec::new(),
        flags: if keeps_overlays {
            DddInterrogationResponse::FLAG_OVERLAY
        } else {
            0
        },
        overlay_bases: if keeps_overlays {
            files
                .iter()
                .map(|&(dat_file_type, dat_file_id, f)| OverlayBase {
                    dat_file_type,
                    dat_file_id,
                    fingerprint: dereth_dat::overlay::fingerprint(f),
                })
                .collect()
        } else {
            Vec::new()
        },
    }
}

/// A server on `dats`, one client connected and interrogated, and its answer sent.
fn exchange(dats: &Arc<DatManager>, answer: &DddInterrogationResponse) -> (TestServer, ClientId) {
    let mut ts = TestServer::with_setup(Arc::clone(dats), |w| {
        let d = Arc::clone(&w.dats);
        empyrean_world::managers::ddd_manager::initialize(&mut w.ddd_manager, &d);
    });
    ts.auth()
        .create_account(
            "acct",
            "pw",
            AccessLevel::Player,
            std::net::Ipv4Addr::LOCALHOST.into(),
        )
        .expect("created");
    let id = ts.connect("acct", "pw");
    assert!(
        ts.run_until(1.0, |ts| !ts.received::<DddInterrogation>(id).is_empty()),
        "interrogation"
    );
    ts.send_message(id, NetQueue::ClientCache, answer);
    ts.run_until(2.0, |_| false);
    (ts, id)
}

fn overlaid(base: &Path, overlay: &Path, era: dereth_primitives::ContainerEra) -> Arc<DatManager> {
    let source = RealDats::open_era(base, era)
        .expect("the base")
        .with_overlay(overlay)
        .expect("the overlay over it");
    DatManager::initialize(Arc::new(source)).expect("the world's dats")
}

#[test]
fn a_client_that_keeps_overlays_is_sent_the_manifest_the_records_and_the_deletions() {
    let scratch = dereth_dat::testing::ScratchDir::new("server-overlay").expect("scratch");
    let dir = OverlayDir::new(&scratch.path().join("overlay")).expect("an overlay folder");
    let base_dir = dereth_dat::testing::dat_dir();
    let base = RetailDatStore::open_dir(&base_dir).expect("the retail dats");
    let made = make_overlay(&base, &dir, "client_portal.dat", "client_cell_1.dat");
    let dats = overlaid(&base_dir, dir.path(), dereth_primitives::ContainerEra::Modern);
    assert_eq!(
        dats.portal_dat().iteration(),
        i32::try_from(made.portal_revision).unwrap()
    );

    let (mut ts, id) = exchange(&dats, &response(&base, true));
    // The records follow BeginDDD after ACE's few seconds' pause, one a tick.
    ts.run_until(10.0, |ts| ts.received::<DddData>(id).len() >= 3);
    let order: Vec<u32> = ts
        .received_raw(id)
        .iter()
        .map(|m| m.opcode)
        .filter(|o| (0xF7E2..=0xF7EC).contains(o))
        .collect();
    let manifest_at = order
        .iter()
        .position(|o| *o == 0xF7EC)
        .expect("the manifest");
    let begin_at = order.iter().position(|o| *o == 0xF7E7).expect("BeginDDD");
    assert!(
        manifest_at < begin_at,
        "the manifest comes first: {order:04X?}"
    );

    let m = &ts.received::<DddOverlayManifest>(id)[0];
    assert_eq!(m.world_key, WORLD);
    let portal = m
        .files
        .iter()
        .find(|f| f.dat_file_id == 1)
        .expect("the portal's part");
    let ids: Vec<u32> = portal.records.iter().map(|r| r.id).collect();
    assert!(ids.contains(&ADDED.raw()) && ids.contains(&made.replaced.raw()));
    assert_eq!(portal.revisions, vec![made.portal_revision]);
    let cell = m
        .files
        .iter()
        .find(|f| f.dat_file_id == 2)
        .expect("the cell's part");
    assert!(cell
        .tombstones
        .iter()
        .any(|t| t.id == made.cell_deleted.raw() && t.mask == 0));

    let begin = &ts.received::<DddBeginDdd>(id)[0];
    let portal_rev = begin
        .revisions
        .iter()
        .find(|r| r.dat_file_id == 1 && r.iteration == made.portal_revision)
        .expect("the portal revision");
    assert_eq!(
        portal_rev.ids_to_download,
        vec![ADDED.raw(), made.replaced.raw()]
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
    );
    assert_eq!(portal_rev.ids_to_purge, vec![made.deleted.raw()]);
    let cell_rev = begin
        .revisions
        .iter()
        .find(|r| r.dat_file_id == 2 && r.iteration == made.cell_revision)
        .expect("the cell revision");
    assert_eq!(cell_rev.ids_to_download, vec![made.cell_replaced.raw()]);
    assert!(
        cell_rev.ids_to_purge.is_empty(),
        "a single room's deletion is the manifest's to say: {:?}",
        cell_rev.ids_to_purge
    );

    let data: Vec<u32> = ts
        .received::<DddData>(id)
        .iter()
        .map(|d| d.resource_id)
        .collect();
    for want in [ADDED, made.replaced, made.cell_replaced] {
        assert!(data.contains(&want.raw()), "{want:?} was sent: {data:08X?}");
    }
}

#[test]
fn a_client_holding_another_base_is_refused_with_the_reason_and_one_without_overlays_as_ace_does() {
    let scratch = dereth_dat::testing::ScratchDir::new("server-overlay-refused").expect("scratch");
    let dir = OverlayDir::new(&scratch.path().join("overlay")).expect("an overlay folder");
    let base_dir = dereth_dat::testing::dat_dir();
    let base = RetailDatStore::open_dir(&base_dir).expect("the retail dats");
    make_overlay(&base, &dir, "client_portal.dat", "client_cell_1.dat");
    let dats = overlaid(&base_dir, dir.path(), dereth_primitives::ContainerEra::Modern);

    let mut other = response(&base, true);
    other.overlay_bases[0].fingerprint = [0x5A; 32];
    let (ts, id) = exchange(&dats, &other);
    let got: Vec<u32> = ts.received_raw(id).iter().map(|m| m.opcode).collect();
    assert!(got.contains(&0xF7EC) && got.contains(&0xF7DC), "{got:04X?}");
    assert!(!got.contains(&0xF7E7), "no patch: {got:04X?}");
    let boot = ts
        .received_raw(id)
        .iter()
        .find(|m| m.opcode == 0xF7DC)
        .expect("the boot");
    assert!(String::from_utf8_lossy(&boot.body).contains("data overlay"));

    // Without the flag the client is missing iterations with patching off: ACE boots it.
    let (ts, id) = exchange(&dats, &response(&base, false));
    let got: Vec<u32> = ts.received_raw(id).iter().map(|m| m.opcode).collect();
    assert!(
        !got.contains(&0xF7EC) && !got.contains(&0xF7E7),
        "{got:04X?}"
    );
    assert!(got.contains(&0xF7DC), "{got:04X?}");
}

#[test]
fn a_february_2005_world_with_an_overlay_patches_a_client_that_keeps_overlays() {
    let scratch = dereth_dat::testing::ScratchDir::new("server-overlay-2005").expect("scratch");
    let dir = OverlayDir::new(&scratch.path().join("overlay")).expect("an overlay folder");
    let old_dir = dereth_dat::testing::classic_dat_dir().unwrap_or_else(|| {
        panic!(
            "{}",
            dereth_dat::testing::classic_shortfall().unwrap_or_default()
        )
    });
    let old = RetailDatStore::open_classic_dir(&old_dir).expect("the 2005 dats");
    let made = make_overlay(&old, &dir, "portal.dat", "cell.dat");
    let dats = overlaid(
        &old_dir,
        dir.path(),
        dereth_primitives::ContainerEra::Classic,
    );
    let later = RetailDatStore::open_dir(&dereth_dat::testing::dat_dir()).expect("later");
    // The client drawing that world: its 2005 portal and cell, its later language file.
    let mut answer = response(&old, true);
    answer.iters_with_keys[2].iterations = run(later.local());
    answer.overlay_bases[2].fingerprint = dereth_dat::overlay::fingerprint(later.local());
    let (ts, id) = exchange(&dats, &answer);
    let begin = ts.received::<DddBeginDdd>(id);
    assert_eq!(begin.len(), 1, "patched");
    assert!(begin[0]
        .revisions
        .iter()
        .any(|r| r.dat_file_id == 1 && r.iteration == made.portal_revision));
    assert!(
        begin[0].revisions.iter().all(|r| r.dat_file_id != 3),
        "nothing of the language list is patched"
    );
    let got: Vec<u32> = ts.received_raw(id).iter().map(|m| m.opcode).collect();
    assert!(!got.contains(&0xF7DC), "not booted: {got:04X?}");
}

/// A client that keeps overlays is sent its records at the overlay's rate (`[dat_overlay]
/// records_per_minute`, a thousand a second by default), not ACE's thousand a minute: 600 records
/// all arrive within ACE's five seconds' pause and one more second.
#[test]
fn a_client_that_keeps_overlays_is_sent_its_records_at_the_overlays_rate() {
    let scratch = dereth_dat::testing::ScratchDir::new("server-overlay-rate").expect("scratch");
    let dir = OverlayDir::new(&scratch.path().join("overlay")).expect("an overlay folder");
    let base_dir = dereth_dat::testing::dat_dir();
    let base = RetailDatStore::open_dir(&base_dir).expect("the retail dats");
    let portal = base.portal().base();
    let revision = revision(&portal);
    let mut w = OverlayWriter::open_or_create(
        &dir.container(ModernDat::Portal),
        &portal,
        "client_portal.dat",
        WORLD,
        1,
    )
    .expect("the portal overlay");
    for n in 0..600u32 {
        w.save(
            &portal,
            DataId(0x0600_F000 + n),
            &n.to_le_bytes(),
            1,
            revision,
            1,
        )
        .expect("added");
    }
    w.add_iteration(revision, 1).expect("the revision");
    w.flush(1).expect("flushed");
    drop(w);
    let dats = overlaid(&base_dir, dir.path(), dereth_primitives::ContainerEra::Modern);
    let (mut ts, id) = exchange(&dats, &response(&base, true));
    ts.run_until(6.0, |ts| ts.received::<DddData>(id).len() >= 600);
    assert_eq!(ts.received::<DddData>(id).len(), 600);
}
