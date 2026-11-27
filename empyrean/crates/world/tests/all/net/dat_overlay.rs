//! Divergence: V437, V438, V520, V578
//! A world served as base data files with a data overlay over them: a client that keeps overlays
//! (the overlay flag and its bases in `DDD_InterrogationResponseMessage`, `0xF7E6`) is sent the
//! overlay manifest (`0xF7EC`), then a `DDD_BeginDDDMessage` (`0xF7E7`) whose revisions download
//! the overlay's records, the world's cell records among them, and purge its deletions, then the
//! records; one holding another base is refused with the reason, unless it checks the bases by
//! their records itself, when it is sent them and patched; a client without the flag is answered as
//! ACE answers it, except that one missing the world's overlay with its own files' patching off is
//! told to play the world with Dereth. Each publish of a world is the revision after the last: a
//! client patched to one is patched to the next with what changed. On the February 2005 files,
//! whose world is otherwise never patched, a client that keeps overlays is patched too. A client
//! keeping a folder for each world at an address that answers for another world is told this
//! world's name and asked again, once, so two worlds taking turns at one address each keep their
//! data and going back to one sends nothing. A Launch world on the October 31, 1999 portal file
//! patches a client on the retail CD's portal file up to its own first, so clients on either end
//! with the same data; with no overlay, it builds what that takes only when the first such client
//! asks. A client a Launch world does not patch is told the files the world plays. A world whose
//! overlay folder holds its own places document sends it whole to a client that names its maps by
//! it, so a client the world patches names the world's towns and landmarks; a world without one,
//! and a client that does not ask, are sent none.
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`), the February 2005 dats
//! (`DERETH_TEST_PRETOD_DAT_DIR`), the 1999 captures (`DERETH_TEST_DAT_CAPTURES_DIR`) and an
//! overlay written in a scratch folder.

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

/// A server on `dats`, one client connected and interrogated, and its answer sent. Its patch
/// state is built at start, as the server builds it, unless the world is on a later 1999 portal
/// file with no overlay, which builds what it needs when a client on the earlier file asks.
fn exchange(dats: &Arc<DatManager>, answer: &DddInterrogationResponse) -> (TestServer, ClientId) {
    let plain = dats.portal_dat().overlay().is_none() && dats.cell_dat().overlay().is_none();
    let at_start = !plain || empyrean_world::managers::launch_base::of_world(dats).is_none();
    exchange_with(dats, answer, at_start)
}

/// [`exchange`], its patch state built at start or not as `at_start` says. Built at start, the
/// files no overlay lies over are walked once for every test.
fn exchange_with(
    dats: &Arc<DatManager>,
    answer: &DddInterrogationResponse,
    at_start: bool,
) -> (TestServer, ClientId) {
    let mut ts = TestServer::with_setup(Arc::clone(dats), |w| {
        if at_start {
            let d = Arc::clone(&w.dats);
            crate::support::ddd_start::initialize(&mut w.ddd_manager, &d);
        }
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
    let dats = overlaid(
        &base_dir,
        dir.path(),
        dereth_primitives::ContainerEra::Modern,
    );
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
    assert_eq!(
        portal.base_records, None,
        "a client that does not check records is not sent them"
    );
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

/// The servers here start with the patch state the server's own start builds, though the files no
/// overlay lies over are walked once for every test: over the retail dats with an overlay over the
/// portal and cell files, every database's iterations, file sizes and compressed records are the
/// server's own. Asked twice, so that the second answer is from the walks kept.
#[test]
fn a_test_servers_start_state_with_the_walks_kept_for_every_test_is_the_servers_own() {
    use empyrean_common::config_manager::ConfigManager;
    use empyrean_common::master_configuration::MasterConfiguration;
    use empyrean_dat::DatDatabaseType;
    use empyrean_world::managers::ddd_manager::{self, DddManagerState};
    // The walks read the default configuration, on this thread alone.
    let _config = ConfigManager::override_for_thread(MasterConfiguration::default());
    let scratch = dereth_dat::testing::ScratchDir::new("server-overlay-start").expect("scratch");
    let dir = OverlayDir::new(&scratch.path().join("overlay")).expect("an overlay folder");
    let base_dir = dereth_dat::testing::dat_dir();
    let base = RetailDatStore::open_dir(&base_dir).expect("the retail dats");
    make_overlay(&base, &dir, "client_portal.dat", "client_cell_1.dat");
    let dats = overlaid(
        &base_dir,
        dir.path(),
        dereth_primitives::ContainerEra::Modern,
    );
    let mut own = DddManagerState::new();
    ddd_manager::initialize(&mut own, &dats);
    assert!(
        own.dat_file_sizes[&DatDatabaseType::Language]
            .values()
            .any(|s| s.compressed_file_size > 0),
        "the language file, with no overlay, is compressed"
    );
    for _ in 0..2 {
        let mut kept = DddManagerState::new();
        crate::support::ddd_start::initialize(&mut kept, &dats);
        for db in [
            DatDatabaseType::Portal,
            DatDatabaseType::Cell,
            DatDatabaseType::Language,
            DatDatabaseType::HighRes,
        ] {
            assert!(
                own.iterations.get(&db) == kept.iterations.get(&db),
                "{db:?}: iterations"
            );
            assert!(
                own.dat_file_sizes.get(&db) == kept.dat_file_sizes.get(&db),
                "{db:?}: sizes"
            );
            assert!(
                own.compressed_dat_files_cache.get(&db) == kept.compressed_dat_files_cache.get(&db),
                "{db:?}: compressed records"
            );
        }
    }
}

#[test]
fn a_client_holding_another_base_is_refused_with_the_reason_and_one_without_overlays_as_ace_does() {
    let scratch = dereth_dat::testing::ScratchDir::new("server-overlay-refused").expect("scratch");
    let dir = OverlayDir::new(&scratch.path().join("overlay")).expect("an overlay folder");
    let base_dir = dereth_dat::testing::dat_dir();
    let base = RetailDatStore::open_dir(&base_dir).expect("the retail dats");
    make_overlay(&base, &dir, "client_portal.dat", "client_cell_1.dat");
    let dats = overlaid(
        &base_dir,
        dir.path(),
        dereth_primitives::ContainerEra::Modern,
    );

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
fn a_client_without_overlays_missing_the_worlds_overlay_is_told_to_play_it_with_dereth() {
    let scratch = dereth_dat::testing::ScratchDir::new("server-overlay-retail").expect("scratch");
    let dir = OverlayDir::new(&scratch.path().join("overlay")).expect("an overlay folder");
    let base_dir = dereth_dat::testing::dat_dir();
    let base = RetailDatStore::open_dir(&base_dir).expect("the retail dats");
    make_overlay(&base, &dir, "client_portal.dat", "client_cell_1.dat");
    let dats = overlaid(
        &base_dir,
        dir.path(),
        dereth_primitives::ContainerEra::Modern,
    );
    // Its own files' patching is off: it is booted, with the reason.
    let (ts, id) = exchange(&dats, &response(&base, false));
    let boot = ts
        .received_raw(id)
        .iter()
        .find(|m| m.opcode == 0xF7DC)
        .expect("the boot");
    let said = String::from_utf8_lossy(&boot.body).into_owned();
    assert!(said.contains("Play it with Dereth"), "{said}");
    assert!(!said.contains("incomplete"), "not ACE's message: {said}");
    // A world with no overlay boots it as ACE does.
    let plain = DatManager::initialize(Arc::new(
        RealDats::open_era(&base_dir, dereth_primitives::ContainerEra::Modern).expect("the base"),
    ))
    .expect("the world's dats");
    let mut older = response(&base, false);
    older.iters_with_keys[0].iterations = MostlyConsecutiveIntSet {
        iterations: 1,
        ints: vec![1],
    };
    let (ts, id) = exchange(&plain, &older);
    let boot = ts
        .received_raw(id)
        .iter()
        .find(|m| m.opcode == 0xF7DC)
        .expect("the boot");
    let said = String::from_utf8_lossy(&boot.body).into_owned();
    assert!(!said.contains("Dereth, which"), "{said}");
}

#[test]
fn a_client_that_checks_records_is_patched_over_files_laid_out_otherwise() {
    let scratch = dereth_dat::testing::ScratchDir::new("server-overlay-records").expect("scratch");
    let dir = OverlayDir::new(&scratch.path().join("overlay")).expect("an overlay folder");
    let base_dir = dereth_dat::testing::dat_dir();
    let base = RetailDatStore::open_dir(&base_dir).expect("the retail dats");
    make_overlay(&base, &dir, "client_portal.dat", "client_cell_1.dat");
    let dats = overlaid(
        &base_dir,
        dir.path(),
        dereth_primitives::ContainerEra::Modern,
    );

    // Its portal file is laid out as neither the overlay's maker's nor this server's.
    let mut answer = response(&base, true);
    answer.flags |= DddInterrogationResponse::FLAG_RECORDS;
    answer.overlay_bases[0].fingerprint = [0x5A; 32];
    let (mut ts, id) = exchange(&dats, &answer);
    ts.run_until(10.0, |ts| !ts.received::<DddBeginDdd>(id).is_empty());
    let got: Vec<u32> = ts.received_raw(id).iter().map(|m| m.opcode).collect();
    assert!(got.contains(&0xF7EC) && got.contains(&0xF7E7), "{got:04X?}");
    assert!(!got.contains(&0xF7DC), "not booted: {got:04X?}");

    let m = &ts.received::<DddOverlayManifest>(id)[0];
    let portal = m
        .files
        .iter()
        .find(|f| (f.dat_file_type, f.dat_file_id) == (0, 1))
        .expect("the portal's part");
    assert_eq!(
        portal.base_records,
        Some(dereth_dat::overlay::records_digest(base.portal()).expect("the records")),
        "the client is sent the records to check its file against"
    );
    let cell = m
        .files
        .iter()
        .find(|f| (f.dat_file_type, f.dat_file_id) == (1, 2))
        .expect("the cell's part");
    assert_eq!(
        cell.base_fingerprint,
        dereth_dat::overlay::fingerprint(base.cell()),
        "a file laid out as the server's is named by its layout"
    );
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
    let dats = overlaid(
        &base_dir,
        dir.path(),
        dereth_primitives::ContainerEra::Modern,
    );
    let (mut ts, id) = exchange(&dats, &response(&base, true));
    ts.run_until(6.0, |ts| ts.received::<DddData>(id).len() >= 600);
    assert_eq!(ts.received::<DddData>(id).len(), 600);
}

/// A Dereth client's interrogation response over `store` (its files with its overlay over them).
fn dereth_answer(store: &RetailDatStore) -> DddInterrogationResponse {
    dereth_client_runtime::app::ddd_interrogation_response(store, 0, true, None)
}

/// A world's own places document, as its overlay folder holds it: two towns, a landmark and a
/// lifestone, one named in letters beyond the game's code page.
const PLACES: &str = "dereth-places-store\t1\nworld\tNew Dereth\n\
place\ttown\tpublished\t-\tKehan\t0x76970012 54.000 38.000 58.350\t0\t0\t-\t-\n\
place\tlifestone\tpublished\t-\tLife Stone\t0x76970012 60.000 38.000 58.350\t0\t0\t-\t-\n\
place\tlandmark\tpublished\t-\tDrudge Windmill\t0x769C001A 94.000 38.000 74.925\t0\t0\t-\t-\n\
place\ttown\tpublished\t-\tAyan Baqur \u{2014} the Oasis\t0x74A20036 158.000 126.000 11.550\t0\t0\t-\t-\n";

/// A world over the retail files with [`make_overlay`]'s overlay in `dir`, and the base files.
fn overlay_world(dir: &OverlayDir) -> (Arc<DatManager>, RetailDatStore) {
    let base_dir = dereth_dat::testing::dat_dir();
    let base = RetailDatStore::open_dir(&base_dir).expect("the retail dats");
    make_overlay(&base, dir, "client_portal.dat", "client_cell_1.dat");
    let dats = overlaid(
        &base_dir,
        dir.path(),
        dereth_primitives::ContainerEra::Modern,
    );
    (dats, base)
}

/// The towns and landmarks a client's maps name after taking in what the server sent `id` as the
/// client's own session reads it; `None` when it named none.
fn names_on_the_maps(ts: &TestServer, id: ClientId) -> Option<Vec<String>> {
    use dereth_client_net::client_session::dispatch::database::{dispatch, DddState};
    use dereth_client_net::client_session::{DddEvent, SessionEvent};
    let mut travel = dereth_client_runtime::travel::Travel::new(
        dereth_client_runtime::travel::StoreFiles::default(),
    );
    let mut state = DddState::InterrogationReceved;
    for m in ts.received_raw(id) {
        if let SessionEvent::Ddd(DddEvent::WorldPlaces(p)) = dispatch(&mut state, m).event {
            travel.take_world_places(&p.document);
        }
    }
    travel
        .published_own_places()
        .map(|own| own.0.iter().map(|p| p.name.clone()).collect())
}

#[test]
fn a_world_naming_its_own_places_sends_them_to_a_client_it_patches_whose_maps_name_its_towns() {
    use dereth_protocol::admin::WorldPlaces;
    let scratch = dereth_dat::testing::ScratchDir::new("server-overlay-places").expect("scratch");
    let dir = OverlayDir::new(&scratch.path().join("overlay")).expect("an overlay folder");
    let (dats, base) = overlay_world(&dir);
    std::fs::write(
        dir.path()
            .join(empyrean_world::managers::dat_overlay::WORLD_PLACES_FILE),
        PLACES,
    )
    .expect("the world's places");

    let answer = dereth_answer(&base);
    assert_ne!(answer.flags & DddInterrogationResponse::FLAG_PLACES, 0);
    let (ts, id) = exchange(&dats, &answer);
    assert_eq!(
        ts.received::<WorldPlaces>(id),
        vec![WorldPlaces {
            document: PLACES.to_owned()
        }],
        "the document whole, once"
    );
    assert!(
        !ts.received::<DddBeginDdd>(id).is_empty(),
        "and the client is patched"
    );
    assert_eq!(
        names_on_the_maps(&ts, id),
        Some(vec![
            "Kehan".to_owned(),
            "Drudge Windmill".to_owned(),
            "Ayan Baqur \u{2014} the Oasis".to_owned()
        ]),
        "its towns and landmarks, in the document's order"
    );

    // A client that does not name its maps by a world's places is sent none.
    let (ts, id) = exchange(&dats, &response(&base, true));
    assert!(ts.received::<WorldPlaces>(id).is_empty());
    assert_eq!(names_on_the_maps(&ts, id), None);
}

#[test]
fn a_world_without_a_places_document_sends_a_client_none() {
    use dereth_protocol::admin::WorldPlaces;
    let scratch =
        dereth_dat::testing::ScratchDir::new("server-overlay-no-places").expect("scratch");
    let dir = OverlayDir::new(&scratch.path().join("overlay")).expect("an overlay folder");
    let (dats, base) = overlay_world(&dir);
    let (ts, id) = exchange(&dats, &dereth_answer(&base));
    assert!(
        !ts.received::<DddBeginDdd>(id).is_empty(),
        "the client is patched"
    );
    assert!(ts.received::<WorldPlaces>(id).is_empty());
    assert_eq!(names_on_the_maps(&ts, id), None);

    // Nor does a world with no overlay.
    let plain = DatManager::initialize(Arc::new(
        RealDats::open_era(
            &dereth_dat::testing::dat_dir(),
            dereth_primitives::ContainerEra::Modern,
        )
        .expect("the base"),
    ))
    .expect("the world's dats");
    let (ts, id) = exchange(&plain, &dereth_answer(&base));
    assert!(ts.received::<WorldPlaces>(id).is_empty());
}

/// One login of a Dereth client keeping its overlay in `client` over the files `base`: the server
/// on `dats` interrogates it, and everything it sends is applied by the client's own patcher.
/// Answers the revisions the patch named, as `(dat file id, iteration)`.
fn patch_client(dats: &Arc<DatManager>, base: &RetailDatStore, client: &Path) -> Vec<(u32, u32)> {
    let dir = OverlayDir::new(client).expect("the client's overlay folder");
    let store = if dir.containers().is_empty() {
        base.clone()
    } else {
        base.clone()
            .with_overlay(&dir, None)
            .expect("the client's overlay lies over its files")
    };
    let (mut ts, id) = exchange(dats, &dereth_answer(&store));
    let begins = ts.received::<DddBeginDdd>(id);
    let Some(begin) = begins.first() else {
        return Vec::new();
    };
    let expected: usize = begin
        .revisions
        .iter()
        .map(|r| r.ids_to_download.len())
        .sum();
    ts.run_until(30.0, |ts| ts.received::<DddData>(id).len() >= expected);
    let mut patcher = dereth_client_runtime::ddd::DddPatcher::new(Some(
        dereth_client_runtime::ddd::OverlayTarget::new(dir, base, "127.0.0.1:9000"),
    ));
    patcher.on_interrogation();
    for m in ts.received::<DddOverlayManifest>(id) {
        patcher.on_manifest(&m);
    }
    patcher.on_begin(begin);
    for d in ts.received::<DddData>(id) {
        patcher.on_data(&d);
    }
    let summary = patcher.on_end();
    assert_eq!(patcher.refused(), None, "{:?}", patcher.notices());
    assert_eq!(
        (summary.refused, summary.still_pending),
        (0, 0),
        "{summary:?}"
    );
    begin
        .revisions
        .iter()
        .map(|r| (r.dat_file_id, r.iteration))
        .collect()
}

/// `base` read through the overlay folder at `dir`.
fn through(base: &RetailDatStore, dir: &Path) -> RetailDatStore {
    let dir = OverlayDir::new(dir).expect("an overlay folder");
    base.clone()
        .with_overlay(&dir, None)
        .expect("the overlay lies over the base")
}

/// The client reads exactly as `world` does: every id either overlay names in the portal and cell
/// files, as many records, and every revision `published` has.
fn assert_client_reads(
    client: &RetailDatStore,
    world: &RetailDatStore,
    published: &RetailDatStore,
) {
    for (mine, theirs, iterations) in [
        (client.portal(), world.portal(), published.portal()),
        (client.cell(), world.cell(), published.cell()),
    ] {
        assert_eq!(mine.len(), theirs.len(), "as many records");
        let ids: std::collections::BTreeSet<DataId> = mine
            .layer()
            .into_iter()
            .chain(theirs.layer())
            .flat_map(|l| {
                l.records()
                    .map(|(id, _)| id)
                    .chain(l.tombstones().iter().map(|t| DataId(t.id)))
                    .collect::<Vec<_>>()
            })
            .collect();
        for id in ids {
            assert_eq!(mine.read(id).ok(), theirs.read(id).ok(), "{id:?}");
        }
        assert_eq!(
            mine.iteration_list().expect("the client's iterations"),
            iterations.iteration_list().expect("the world's iterations")
        );
    }
}

/// What one build changes in a file: the records it writes and the ids it deletes.
type Build<'a> = (&'a [(DataId, Vec<u8>)], &'a [DataId]);

/// A world builder's overlay over `store`'s portal and cell files, named `names` on disk: every
/// change in one revision past the base, as a build writes it.
fn built(
    store: &RetailDatStore,
    names: [&str; 2],
    dir: &Path,
    portal: Build<'_>,
    cell: Build<'_>,
) -> OverlayDir {
    let out = OverlayDir::new(dir).expect("an overlay folder");
    for (target, file, name, (put, gone)) in [
        (ModernDat::Portal, store.portal().base(), names[0], portal),
        (ModernDat::Cell, store.cell().base(), names[1], cell),
    ] {
        let revision = revision(&file);
        let mut w = OverlayWriter::open_or_create(&out.container(target), &file, name, WORLD, 0)
            .expect("the overlay");
        for id in gone {
            w.tombstone(*id, 0, revision).expect("deleted");
        }
        for (id, bytes) in put {
            w.put(*id, bytes, 3, revision, 0).expect("put");
        }
        w.add_iteration(revision, 0).expect("the revision");
        w.flush(0).expect("flushed");
    }
    out
}

/// A world over the files in `base_dir` (of `era`, named `names`) published twice, and a Dereth
/// client whose files are `base` patched to each publish in turn, then a client with none patched
/// to the second: the second publish's revision alone is sent to the first client (a picture
/// changed, one added and one removed, a room deleted), and each client then reads exactly the
/// new world.
fn publish_twice_and_patch(
    base_dir: &Path,
    era: dereth_primitives::ContainerEra,
    names: [&str; 2],
    base: &RetailDatStore,
    tag: &str,
) {
    const ADDED_SECOND: DataId = DataId(0x0600_FFF1);
    let scratch = dereth_dat::testing::ScratchDir::new(tag).expect("scratch");
    let portal = base.portal().base();
    let pictures: Vec<DataId> = portal
        .iter_ids()
        .filter(|i| i.raw() >> 24 == 0x06)
        .take(3)
        .collect();
    let (changed, removed_first, removed_second) = (pictures[0], pictures[1], pictures[2]);
    let cell = base.cell().base();
    let rooms: Vec<DataId> = cell
        .iter_ids()
        .filter(|i| i.raw() >> 16 == 0xA9B4 && (i.raw() & 0xFFFF) < 0xFFFE)
        .take(2)
        .collect();
    let flipped = |f: &DatFile, id: DataId, at: usize| {
        let mut b = f.read(id).expect("the record");
        b[at] ^= 0xFF;
        b
    };
    let room = vec![(rooms[0], flipped(&cell, rooms[0], 0))];
    let first_portal = [
        (changed, flipped(&portal, changed, 0)),
        (ADDED, b"an added picture".to_vec()),
    ];
    let first = built(
        base,
        names,
        &scratch.path().join("build-1"),
        (&first_portal, &[removed_first]),
        (&room, &[]),
    );
    let second_portal = [
        (changed, flipped(&portal, changed, 1)),
        (ADDED, b"an added picture".to_vec()),
        (ADDED_SECOND, b"another added picture".to_vec()),
    ];
    let second = built(
        base,
        names,
        &scratch.path().join("build-2"),
        (&second_portal, &[removed_first, removed_second]),
        (&room, &[rooms[1]]),
    );
    let bases = [
        (ModernDat::Portal, portal.clone()),
        (ModernDat::Cell, cell.clone()),
    ];
    let publish = |n: u32, last: Option<&OverlayDir>, fresh: &OverlayDir| {
        let out = OverlayDir::new(&scratch.path().join(format!("publish-{n}"))).expect("a folder");
        dereth_dat::revise::revise_folder(&bases, last, fresh, &out, WORLD).expect("revised");
        out
    };
    let publish_1 = publish(1, None, &first);
    let publish_2 = publish(2, Some(&publish_1), &second);
    let (p1, p2) = (revision(&portal), revision(&portal) + 1);
    let (c1, c2) = (revision(&cell), revision(&cell) + 1);

    let client = scratch.path().join("client");
    let world_1 = overlaid(base_dir, publish_1.path(), era);
    assert_eq!(patch_client(&world_1, base, &client), [(1, p1), (2, c1)]);
    assert_client_reads(
        &through(base, &client),
        &through(base, first.path()),
        &through(base, publish_1.path()),
    );

    let world_2 = overlaid(base_dir, publish_2.path(), era);
    assert_eq!(
        patch_client(&world_2, base, &client),
        [(1, p2), (2, c2)],
        "the second publish's revision alone"
    );
    let now = through(base, &client);
    assert_client_reads(
        &now,
        &through(base, second.path()),
        &through(base, publish_2.path()),
    );
    assert!(
        now.portal().read(removed_second).is_err() && now.portal().read(removed_first).is_err()
    );
    assert_eq!(
        now.portal().read(ADDED_SECOND).expect("added"),
        b"another added picture"
    );
    assert!(now.cell().read(rooms[1]).is_err(), "the room is deleted");
    assert!(
        patch_client(&world_2, base, &client).is_empty(),
        "patched again, it is current"
    );

    let new_client = scratch.path().join("new-client");
    assert_eq!(
        patch_client(&world_2, base, &new_client),
        [(1, p1), (1, p2), (2, c1), (2, c2)]
    );
    assert_client_reads(
        &through(base, &new_client),
        &through(base, second.path()),
        &through(base, publish_2.path()),
    );
}

/// Each publish of a world is the revision after the last: a Dereth client patched to the first
/// publish is patched to the second with what changed, and reads exactly the new world.
#[test]
fn a_client_patched_to_one_publish_is_patched_to_the_next_and_reads_exactly_the_new_world() {
    let base_dir = dereth_dat::testing::dat_dir();
    let base = RetailDatStore::open_dir(&base_dir).expect("the retail dats");
    publish_twice_and_patch(
        &base_dir,
        dereth_primitives::ContainerEra::Modern,
        ["client_portal.dat", "client_cell_1.dat"],
        &base,
        "server-overlay-publish",
    );
}

/// The same over the February 2005 files: a client drawing that world (its 2005 portal and cell,
/// its later language file) is patched from one publish to the next.
#[test]
fn a_february_2005_world_published_again_patches_a_client_from_one_publish_to_the_next() {
    let old_dir = dereth_dat::testing::classic_dat_dir().unwrap_or_else(|| {
        panic!(
            "{}",
            dereth_dat::testing::classic_shortfall().unwrap_or_default()
        )
    });
    let base = RetailDatStore::open_classic_with_modern(&old_dir, &dereth_dat::testing::dat_dir())
        .expect("the 2005 dats with the later files beside them");
    publish_twice_and_patch(
        &old_dir,
        dereth_primitives::ContainerEra::Classic,
        ["portal.dat", "cell.dat"],
        &base,
        "server-overlay-publish-2005",
    );
}

/// A world `world` over `store`'s files: the picture [`ADDED`] in the portal file, `bytes`, in
/// one revision past the base.
fn one_picture_world(store: &RetailDatStore, dir: &Path, world: &str, bytes: &[u8]) -> OverlayDir {
    let out = OverlayDir::new(dir).expect("an overlay folder");
    let file = store.portal().base();
    let revision = revision(&file);
    let mut w = OverlayWriter::open_or_create(
        &out.container(ModernDat::Portal),
        &file,
        "client_portal.dat",
        world,
        0,
    )
    .expect("the overlay");
    w.put(ADDED, bytes, 3, revision, 0).expect("put");
    w.add_iteration(revision, 0).expect("the revision");
    w.flush(0).expect("flushed");
    out
}

/// What one login of a Dereth client keeping a folder for each world at an address saw: the
/// worlds the server named, how often it asked, and how many records it sent.
#[derive(Debug, Default, PartialEq, Eq)]
struct Turn {
    named: Vec<String>,
    asked: usize,
    records: usize,
}

/// One login, to the server on `dats`, of a Dereth client over the files `base` that keeps a
/// folder for each world the server at its address serves, in `root`: it starts on the folder of
/// the world served there last, answers from it naming that world, follows the world the server
/// names into that world's own folder, and answers again when asked; everything it is sent is
/// written by the client's own patcher.
fn take_a_turn(dats: &Arc<DatManager>, base: &RetailDatStore, root: &Path) -> Turn {
    use dereth_client_runtime::ddd::{DddPatcher, OverlayTarget};
    use dereth_client_runtime::world_overlay as wo;
    const ADDRESS: &str = "127.0.0.1:9000";
    let world = wo::last_world(root).unwrap_or_else(|| ADDRESS.to_owned());
    let dir = OverlayDir::new(&wo::world_folder(root, &world)).expect("the world's folder");
    let mut store = wo::lay_folder(base.clone(), Some(&dir), &Default::default()).0;
    let mut patcher = DddPatcher::new(Some(
        OverlayTarget::new(dir, base, ADDRESS).for_each_world_in(root, ADDRESS, &world),
    ));
    let answer = |store: &RetailDatStore| {
        let laid = wo::laid_world(store);
        (
            dereth_client_runtime::app::ddd_interrogation_response(store, 0, true, laid.as_deref()),
            laid,
        )
    };
    let (first, laid) = answer(&store);
    let (mut ts, id) = exchange(dats, &first);
    patcher.on_interrogation();
    patcher.answered_for(laid.as_deref());
    let mut seen = 0;
    let mut follow = |ts: &TestServer, patcher: &mut DddPatcher| {
        let manifests = ts.received::<DddOverlayManifest>(id);
        for m in &manifests[seen..] {
            patcher.on_manifest(m);
        }
        seen = manifests.len();
        patcher.take_switched()
    };
    if follow(&ts, &mut patcher) {
        let dir = patcher.target().map(|t| t.dir().clone());
        store = wo::lay_folder(base.clone(), dir.as_ref(), &Default::default()).0;
    }
    if ts.received::<DddInterrogation>(id).len() > 1 {
        let (again, laid) = answer(&store);
        ts.send_message(id, NetQueue::ClientCache, &again);
        ts.run_until(2.0, |_| false);
        patcher.on_interrogation();
        patcher.answered_for(laid.as_deref());
        follow(&ts, &mut patcher);
    }
    let mut turn = Turn {
        named: ts
            .received::<DddOverlayManifest>(id)
            .iter()
            .map(|m| m.world_key.clone())
            .collect(),
        asked: ts.received::<DddInterrogation>(id).len(),
        records: 0,
    };
    if let Some(begin) = ts.received::<DddBeginDdd>(id).first() {
        let expected: usize = begin
            .revisions
            .iter()
            .map(|r| r.ids_to_download.len())
            .sum();
        ts.run_until(30.0, |ts| ts.received::<DddData>(id).len() >= expected);
        patcher.on_begin(begin);
        for d in ts.received::<DddData>(id) {
            patcher.on_data(&d);
        }
        patcher.on_end();
    }
    assert_eq!(patcher.refused(), None, "{:?}", patcher.notices());
    turn.records = ts.received::<DddData>(id).len();
    turn
}

/// Two worlds taking turns at one address, each one revision past the same base: the client keeps
/// each world in its own folder. Answered for as the world the client played there last, a world
/// names itself and asks again; the client answers from that world's own folder, so a world played
/// before is sent nothing it already has, and each folder keeps its own world's data. A world that
/// keeps no overlay names none and is answered for with the files as installed. The server asks
/// once: an answer naming another world again is patched as it stands.
#[test]
fn two_worlds_taking_turns_at_one_address_each_keep_their_data_and_going_back_sends_nothing() {
    use dereth_client_runtime::world_overlay as wo;
    let scratch = dereth_dat::testing::ScratchDir::new("server-overlay-turns").expect("scratch");
    let base_dir = dereth_dat::testing::dat_dir();
    let base = RetailDatStore::open_dir(&base_dir).expect("the retail dats");
    let era = dereth_primitives::ContainerEra::Modern;
    let first = one_picture_world(&base, &scratch.path().join("first"), "First", b"first");
    let second = one_picture_world(&base, &scratch.path().join("second"), "Second", b"second");
    let serves = |dir: &OverlayDir| overlaid(&base_dir, dir.path(), era);
    let (first_world, second_world) = (serves(&first), serves(&second));
    let root = scratch.path().join("client/127.0.0.1-9000");
    let reads = |world: &str| {
        let dir = OverlayDir::new(&wo::world_folder(&root, world)).expect("a folder");
        base.clone()
            .with_overlay(&dir, Some(world))
            .expect("the world's own")
            .read_portal(ADDED)
            .expect("the world's picture")
    };

    // First, new to the client: everything, into its own folder.
    let turn = take_a_turn(&first_world, &base, &root);
    assert_eq!(
        (turn.named.clone(), turn.asked),
        (vec!["First".to_owned()], 1)
    );
    assert!(turn.records > 0, "{turn:?}");
    assert_eq!(reads("First"), b"first");
    // Second, answered for as First: asked again, and patched into its own folder.
    let turn = take_a_turn(&second_world, &base, &root);
    assert_eq!(turn.asked, 2, "{turn:?}");
    assert_eq!(turn.named.first().map(String::as_str), Some("Second"));
    assert!(turn.records > 0, "{turn:?}");
    assert_eq!(reads("Second"), b"second");
    assert_eq!(
        reads("First"),
        b"first",
        "First's folder keeps First's data"
    );
    // Back to First, then to Second: each is asked again and sent nothing.
    for (world, server) in [("First", &first_world), ("Second", &second_world)] {
        let turn = take_a_turn(server, &base, &root);
        assert_eq!((turn.asked, turn.records), (2, 0), "{world}: {turn:?}");
        assert_eq!(wo::last_world(&root).as_deref(), Some(world));
    }
    assert_eq!(reads("First"), b"first");
    assert_eq!(reads("Second"), b"second");

    // A world with no overlay at the address: it names none, is answered for with the installed
    // files, and sends nothing.
    let plain = DatManager::initialize(Arc::new(
        RealDats::open_era(&base_dir, era).expect("the base"),
    ))
    .expect("the world's dats");
    let turn = take_a_turn(&plain, &base, &root);
    assert_eq!(
        (turn.named, turn.asked, turn.records),
        (vec![String::new()], 2, 0)
    );

    // Asked once: the same answer naming another world again is patched as it stands.
    let laid = base
        .clone()
        .with_overlay(
            &OverlayDir::new(&wo::world_folder(&root, "First")).expect("First's folder"),
            Some("First"),
        )
        .expect("First's overlay");
    let answer =
        dereth_client_runtime::app::ddd_interrogation_response(&laid, 0, true, Some("First"));
    let (mut ts, id) = exchange(&second_world, &answer);
    assert_eq!(ts.received::<DddInterrogation>(id).len(), 2, "asked again");
    ts.send_message(id, NetQueue::ClientCache, &answer);
    ts.run_until(2.0, |_| false);
    assert_eq!(
        ts.received::<DddInterrogation>(id).len(),
        2,
        "not asked a third time"
    );
}

/// The 1999 files of the captures (`DERETH_TEST_DAT_CAPTURES_DIR`): the retail CD's folder (its
/// portal and cell files) and a folder holding the October 31 portal file beside the CD's cell
/// file, linked into `scratch`.
fn files_of_1999(scratch: &Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let captures = dereth_dat::testing::dat_captures_or_fail();
    let find = |capture: &str, name: &str| {
        captures
            .iter()
            .find(|(c, n, _)| c == capture && n.eq_ignore_ascii_case(name))
            .map(|(_, _, p)| p.clone())
            .unwrap_or_else(|| panic!("the captures hold no {capture}/{name}"))
    };
    let cd = find("1999-10-09", "portal.dat")
        .parent()
        .expect("its folder")
        .to_path_buf();
    let later = scratch.join("1999-10-31");
    std::fs::create_dir_all(&later).expect("a folder");
    for (from, name) in [
        (find("1999-10-31", "portal.dat"), "portal.dat"),
        (find("1999-10-09", "cell.dat"), "cell.dat"),
    ] {
        let to = later.join(name);
        if std::fs::hard_link(&from, &to).is_err() {
            std::fs::copy(&from, &to).expect("a copy");
        }
    }
    (cd, later)
}

/// A client's files over the 1999 folder `dir`, its language file the later set's.
fn client_of_1999(dir: &Path) -> RetailDatStore {
    RetailDatStore::open_classic_with_modern(dir, &dereth_dat::testing::dat_dir())
        .expect("the 1999 files with the later files beside them")
}

/// A Launch world's overlay over `server`'s portal file: the picture [`ADDED`], `bytes`, in one
/// revision past it.
fn launch_world(server: &RetailDatStore, dir: &Path, bytes: &[u8]) -> OverlayDir {
    let out = OverlayDir::new(dir).expect("an overlay folder");
    let file = server.portal().base();
    let revision = revision(&file);
    let mut w = OverlayWriter::open_or_create(
        &out.container(ModernDat::Portal),
        &file,
        "portal.dat",
        WORLD,
        0,
    )
    .expect("the overlay");
    w.put(ADDED, bytes, 3, revision, 0).expect("put");
    w.add_iteration(revision, 0).expect("the revision");
    w.flush(0).expect("flushed");
    out
}

/// What a client keeping its folders in `root` reads after a turn: its files with the world's
/// folder laid over them.
fn after_turn(base: &RetailDatStore, root: &Path, world: &str) -> RetailDatStore {
    use dereth_client_runtime::world_overlay as wo;
    let dir = OverlayDir::new(&wo::world_folder(root, world)).expect("the world's folder");
    let (laid, refused) = wo::lay_folder(base.clone(), Some(&dir), &Default::default());
    assert_eq!(refused, None);
    laid
}

/// Whether two clients read the same portal and cell files: every record of `world` (the
/// world's own files), byte for byte, and the same iterations.
fn reads_the_same(a: &RetailDatStore, b: &RetailDatStore, world: &RetailDatStore) {
    // A file read through no overlay keeps its iterations in its header.
    let iterations = |f: &DatFile| {
        f.iteration_list()
            .unwrap_or_else(|_| dereth_dat::overlay::base_iterations(f))
    };
    assert_eq!(
        iterations(a.portal()),
        iterations(b.portal()),
        "the same portal iterations"
    );
    assert_eq!(
        iterations(a.cell()),
        iterations(b.cell()),
        "the same cell iterations"
    );
    let ids: Vec<DataId> = world.portal().base().iter_ids().collect();
    assert!(ids.len() > 17_000, "{} records", ids.len());
    for id in ids.into_iter().chain([ADDED]) {
        let want = world.read_portal(id).ok();
        assert_eq!(a.read_portal(id).ok(), want, "{id:?}");
        assert_eq!(b.read_portal(id).ok(), want, "{id:?}");
    }
}

/// A Launch world on the October 31, 1999 portal file, with an overlay over it, patches a Dereth
/// client on the retail CD's portal file of October 9 up to its own (the records the later file
/// adds or changes, in the three revisions between the files) and then with the world's overlay,
/// and a client on the October 31 file with the overlay alone: both then read the same records
/// with the same iterations, and log in again to nothing. A client changing its files between
/// the two is patched afresh each way, and what it kept for the other files is never read over
/// these.
#[test]
fn a_launch_world_patches_a_client_on_either_1999_portal_file_to_the_same_data() {
    use dereth_client_runtime::world_overlay as wo;
    let scratch = dereth_dat::testing::ScratchDir::new("server-launch-base").expect("scratch");
    let (cd, later) = files_of_1999(scratch.path());
    let era = dereth_primitives::ContainerEra::Classic;
    let server = RetailDatStore::open_classic_dir(&later).expect("the world's files");
    let overlay = launch_world(
        &server,
        &scratch.path().join("overlay"),
        b"a launch picture",
    );
    let dats = overlaid(&later, overlay.path(), era);
    let world = server
        .clone()
        .with_overlay(&overlay, Some(WORLD))
        .expect("the world as it reads");
    let (on_cd, on_later) = (client_of_1999(&cd), client_of_1999(&later));
    let step = empyrean_world::managers::launch_base::CD_TO_OCTOBER_31;
    let stepped = step.added.len() + step.changed.len();

    // The manifest names the client's own file as the portal's base, and the step's records and
    // revisions as the overlay's.
    let answer = dereth_client_runtime::app::ddd_interrogation_response(&on_cd, 0, true, None);
    let (ts, id) = exchange(&dats, &answer);
    let m = &ts.received::<DddOverlayManifest>(id)[0];
    let portal = m
        .files
        .iter()
        .find(|f| (f.dat_file_type, f.dat_file_id) == (0, 1))
        .expect("the portal's part");
    assert_eq!(
        (
            portal.base_fingerprint,
            portal.base_records,
            portal.base_iterations
        ),
        (step.from.fingerprint, Some(step.from.records), 102)
    );
    let next = revision(&server.portal().base());
    assert_eq!(portal.revisions, [103, 104, 105, next]);
    assert_eq!(portal.records.len(), stepped + 1);
    drop(ts);

    let cd_root = scratch.path().join("cd-client/127.0.0.1-9000");
    let turn = take_a_turn(&dats, &on_cd, &cd_root);
    assert_eq!(turn.records, stepped + 1, "{turn:?}");
    let later_root = scratch.path().join("later-client/127.0.0.1-9000");
    let turn = take_a_turn(&dats, &on_later, &later_root);
    assert_eq!(turn.records, 1, "{turn:?}");
    let (a, b) = (
        after_turn(&on_cd, &cd_root, WORLD),
        after_turn(&on_later, &later_root, WORLD),
    );
    reads_the_same(&a, &b, &world);
    for (files, root) in [(&on_cd, &cd_root), (&on_later, &later_root)] {
        assert_eq!(take_a_turn(&dats, files, root).records, 0, "nothing again");
    }

    // One client changing its files: each way, patched afresh and never read over the others.
    for (files, records) in [(&on_later, 1), (&on_cd, stepped + 1), (&on_later, 1)] {
        let turn = take_a_turn(&dats, files, &cd_root);
        assert_eq!(turn.records, records, "{turn:?}");
        let dir = OverlayDir::new(&wo::world_folder(&cd_root, WORLD)).expect("its folder");
        let other = if std::ptr::eq(files, &on_cd) {
            &on_later
        } else {
            &on_cd
        };
        assert!(
            other.clone().with_overlay(&dir, Some(WORLD)).is_err(),
            "what it keeps is for these files alone"
        );
        reads_the_same(&after_turn(files, &cd_root, WORLD), &b, &world);
    }
}

/// A Launch world on the October 31, 1999 portal file with no overlay patches a Dereth client on
/// the retail CD's portal file up to its own, and the client then reads the world's files; a
/// client on the October 31 file is sent nothing, and one that keeps no overlays is told which
/// files the world plays, as before.
#[test]
fn a_launch_world_without_an_overlay_patches_a_client_on_the_retail_cds_portal_file_up_to_it() {
    let scratch = dereth_dat::testing::ScratchDir::new("server-launch-plain").expect("scratch");
    let (cd, later) = files_of_1999(scratch.path());
    let era = dereth_primitives::ContainerEra::Classic;
    let server = RetailDatStore::open_classic_dir(&later).expect("the world's files");
    let dats = DatManager::initialize(Arc::new(
        RealDats::open_era(&later, era).expect("the world's files"),
    ))
    .expect("the world's dats");
    let (on_cd, on_later) = (client_of_1999(&cd), client_of_1999(&later));
    let step = empyrean_world::managers::launch_base::CD_TO_OCTOBER_31;

    let root = scratch.path().join("cd-client/127.0.0.1-9000");
    let turn = take_a_turn(&dats, &on_cd, &root);
    assert_eq!(
        turn.records,
        step.added.len() + step.changed.len(),
        "{turn:?}"
    );
    let world = dereth_client_runtime::world_overlay::last_world(&root)
        .unwrap_or_else(|| "127.0.0.1:9000".to_owned());
    let a = after_turn(&on_cd, &root, &world);
    assert_eq!(
        a.portal().iteration_list().expect("its iterations"),
        (1..=105).collect::<Vec<_>>()
    );
    for id in server.portal().iter_ids() {
        assert_eq!(
            a.read_portal(id).ok(),
            server.read_portal(id).ok(),
            "{id:?}"
        );
    }
    assert_eq!(
        take_a_turn(&dats, &on_cd, &root).records,
        0,
        "nothing again"
    );

    let other = scratch.path().join("later-client/127.0.0.1-9000");
    assert_eq!(take_a_turn(&dats, &on_later, &other).records, 0);

    let (ts, id) = exchange(&dats, &response(&on_cd, false));
    assert!(booted_saying(&ts, id).contains("the October 31, 1999 portal.dat"));
}

/// What `id` was booted saying.
fn booted_saying(ts: &TestServer, id: ClientId) -> String {
    let boot = ts
        .received_raw(id)
        .iter()
        .find(|m| m.opcode == 0xF7DC)
        .expect("the boot");
    String::from_utf8_lossy(&boot.body).into_owned()
}

/// A Launch world on the October 31, 1999 portal file with no overlay builds nothing for patching
/// at start, nor for a client on its own file or one it boots: it builds what the patch needs
/// when the first client on the retail CD's portal file asks, a few dozen records, and that client
/// is sent exactly what it would have been sent had the whole patch state been built at start.
#[test]
fn a_launch_world_without_an_overlay_builds_its_patch_only_for_the_first_client_on_the_retail_cds_file(
) {
    let scratch = dereth_dat::testing::ScratchDir::new("server-launch-lazy").expect("scratch");
    let (cd, later) = files_of_1999(scratch.path());
    let era = dereth_primitives::ContainerEra::Classic;
    let dats = DatManager::initialize(Arc::new(
        RealDats::open_era(&later, era).expect("the world's files"),
    ))
    .expect("the world's dats");
    let (on_cd, on_later) = (client_of_1999(&cd), client_of_1999(&later));
    let step = empyrean_world::managers::launch_base::CD_TO_OCTOBER_31;

    for answer in [
        response(&on_later, true),
        response(&on_later, false),
        response(&on_cd, false),
    ] {
        let (ts, _) = exchange(&dats, &answer);
        assert!(ts.world.ddd_manager.iterations.is_empty(), "nothing built");
    }

    let answer = response(&on_cd, true);
    let patched = |at_start: bool| {
        let (mut ts, id) = exchange_with(&dats, &answer, at_start);
        let begin = ts.received::<DddBeginDdd>(id);
        assert_eq!(begin.len(), 1, "patched");
        let expected: usize = begin[0]
            .revisions
            .iter()
            .map(|r| r.ids_to_download.len())
            .sum();
        ts.run_until(30.0, |ts| ts.received::<DddData>(id).len() >= expected);
        let data = ts.received::<DddData>(id);
        let sized = ts
            .world
            .ddd_manager
            .dat_file_sizes
            .values()
            .map(std::collections::HashMap::len)
            .sum::<usize>();
        (begin, data, sized)
    };
    let (begin, data, sized) = patched(false);
    assert_eq!(data.len(), step.added.len() + step.changed.len());
    assert!(sized <= data.len() + 16, "{sized} records sized");
    let (full_begin, full_data, full_sized) = patched(true);
    assert!(full_sized > 100_000, "{full_sized}");
    assert_eq!((begin, data), (full_begin, full_data));
}

/// A client whose files are not a Launch world's is told the files the world plays: a world on
/// the retail CD's files names them, one on the October 31, 1999 portal file names it and the CD's
/// cell file, and on a world with an overlay a client on the files it was made against that keeps
/// no overlays is told to play it with Dereth. A world on the February 2005 files says what it
/// said before.
#[test]
fn a_client_on_other_files_than_a_launch_worlds_is_told_the_worlds_own() {
    use empyrean_world::network::handlers::ddd_handler::{
        other_files_reason, PRE_TOD_OTHER_CLIENT_REASON,
    };
    let scratch = dereth_dat::testing::ScratchDir::new("server-launch-boot").expect("scratch");
    let (cd, later) = files_of_1999(scratch.path());
    let era = dereth_primitives::ContainerEra::Classic;
    let world = |dir: &Path| {
        DatManager::initialize(Arc::new(RealDats::open_era(dir, era).expect("the files")))
            .expect("the world's dats")
    };
    let (on_cd, on_later) = (client_of_1999(&cd), client_of_1999(&later));

    let (ts, id) = exchange(&world(&cd), &response(&on_later, false));
    assert!(booted_saying(&ts, id).contains(&other_files_reason("the retail CD's data files")));
    let (ts, id) = exchange(&world(&later), &response(&on_cd, false));
    assert!(booted_saying(&ts, id).contains(&other_files_reason(
        "the October 31, 1999 portal.dat and the retail CD's cell.dat"
    )));

    let server = RetailDatStore::open_classic_dir(&later).expect("the world's files");
    let overlay = launch_world(&server, &scratch.path().join("overlay"), b"a picture");
    let dats = overlaid(&later, overlay.path(), era);
    let (ts, id) = exchange(&dats, &response(&on_later, false));
    assert!(booted_saying(&ts, id).contains("Play it with Dereth"));
    let (ts, id) = exchange(&dats, &response(&on_cd, false));
    assert!(booted_saying(&ts, id).contains(&other_files_reason(
        "the October 31, 1999 portal.dat and the retail CD's cell.dat"
    )));

    let old_dir = dereth_dat::testing::classic_dat_dir().unwrap_or_else(|| {
        panic!(
            "{}",
            dereth_dat::testing::classic_shortfall().unwrap_or_default()
        )
    });
    let (ts, id) = exchange(&world(&old_dir), &response(&on_later, false));
    assert!(booted_saying(&ts, id).contains(PRE_TOD_OTHER_CLIENT_REASON));
}
