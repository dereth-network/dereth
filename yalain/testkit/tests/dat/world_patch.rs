//! A world's overlay sent to the clients that lack it, over the install's own files: what a client
//! is sent, how fast, and who is refused and why.

use std::sync::Arc;

use auberean_client_sdk::dat::overlay::{layout_of, WorldOverlay};
use auberean_dat::locate::role_file;
use auberean_dat::{QualifiedId, RecordType};
use auberean_protocol::login::{DataFileIterations, DataHeld, Empty, OverlayBase, OverlayHeld};
use auberean_protocol::text::StringInfo;
use auberean_protocol::types::{IterationRun, IterationSet};
use auberean_protocol::{queue, ClientMessage, ServerMessage};
use dereth_dat::overlay::OverlayDir;
use dereth_dat::{MemoryFolder, ModernDat};
use dereth_primitives::DataId;
use yalain_testkit::session::{self as play, Drive, InProcess};
use yalain_testkit::{authored, Harness};
use yalain_world::download::{InstallFiles, CELL_FILE, ENGLISH_FILE, PORTAL_FILE};
use yalain_world::reload::WorldTables;
use yalain_world::world_patch::{
    DatOverlaySettings, WorldPatch, NOT_PATCHING, OTHER_FILES, OTHER_WORLD, PAUSE_SECONDS,
};

const WORLD: &str = "a-world";

/// A world over the install: `blocks` of the install's landblocks each given bytes of their own in
/// `revisions` revisions past the file's own, one more of them per revision. The files, and each
/// revision's number.
fn overlaid(blocks: usize, revisions: u32) -> (InstallFiles, Vec<u32>) {
    let files = authored::install_files();
    let cell = files.cell.clone().expect("the cell file");
    let folder = Arc::new(MemoryFolder::new(std::path::Path::new("a-world")));
    let dir = OverlayDir::in_folder(folder.clone()).expect("a folder");
    let mut w = dir
        .writer(
            ModernDat::Cell,
            cell.shared(),
            role_file(ModernDat::Cell),
            WORLD,
            0,
        )
        .expect("a writer");
    let first = dereth_dat::decompose::revision_over(cell.shared());
    let ids: Vec<u32> = cell
        .entries()
        .map(|e| e.id)
        .filter(|id| id & 0xFFFF == 0xFFFF)
        .take(blocks)
        .collect();
    let numbers: Vec<u32> = (0..revisions).map(|r| first + r).collect();
    for (i, id) in ids.iter().enumerate() {
        let r = numbers[i.min(numbers.len() - 1)];
        w.put(DataId(*id), format!("block {i}").as_bytes(), 1, r, 0)
            .expect("the record");
    }
    for r in &numbers {
        w.add_iteration(*r, 0).expect("the revision");
    }
    w.flush(0).expect("written");
    let overlay = WorldOverlay::in_folder(folder).expect("the overlay");
    let cell = overlay.over(cell).expect("it lies over the file");
    (
        InstallFiles {
            cell: Some(cell),
            ..files
        },
        numbers,
    )
}

/// Records by id, with their bytes.
type Records = Vec<(u32, Vec<u8>)>;

/// A world over the install that replaces a saved state of the portal file, adds one past every
/// saved state it holds and replaces a string table of the English file, each file in one revision
/// past its own. The files, and the portal's and the English file's records by id.
fn overlaid_portal_and_english() -> (InstallFiles, Records, Records) {
    let files = authored::install_files();
    let portal = files.portal.clone().expect("the portal file");
    let english = files.english.clone().expect("the English file");
    let folder = Arc::new(MemoryFolder::new(std::path::Path::new("a-world")));
    let dir = OverlayDir::in_folder(folder.clone()).expect("a folder");
    let ids = |f: &auberean_dat::DatFile, r: std::ops::Range<u32>| -> Vec<u32> {
        f.entries()
            .map(|e| e.id)
            .filter(|id| r.contains(id))
            .collect()
    };
    let states = ids(&portal, 0x6F00_0000..0x7000_0000);
    let added = states.iter().max().expect("saved states") + 1;
    let tables = ids(&english, 0x2500_0000..0x2700_0000);
    let portal_records = vec![
        (states[0], b"a state of the world's".to_vec()),
        (added, b"a state of its own".to_vec()),
    ];
    let english_records = vec![(tables[0], b"a table of the world's".to_vec())];
    for (role, file, records) in [
        (ModernDat::Portal, &portal, &portal_records),
        (ModernDat::Local, &english, &english_records),
    ] {
        let mut w = dir
            .writer(role, file.shared(), role_file(role), WORLD, 0)
            .expect("a writer");
        let r = dereth_dat::decompose::revision_over(file.shared());
        for (id, bytes) in records {
            w.put(DataId(*id), bytes, 1, r, 0).expect("the record");
        }
        w.add_iteration(r, 0).expect("the revision");
        w.flush(0).expect("written");
    }
    let overlay = WorldOverlay::in_folder(folder).expect("the overlay");
    (
        InstallFiles {
            portal: Some(overlay.over(portal).expect("it lies over the portal")),
            english: Some(
                overlay
                    .over(english)
                    .expect("it lies over the English file"),
            ),
            ..files
        },
        portal_records,
        english_records,
    )
}

/// A server of that world, with these settings.
fn serving(files: InstallFiles, settings: DatOverlaySettings) -> Harness {
    let mut h = Harness::new();
    h.server.cfg.dat_overlay = settings;
    h.server.world_patch = WorldPatch::of(&files).map(Arc::new);
    assert!(h.server.world_patch.is_some(), "the files have an overlay");
    h.server.data = Some(Box::new(files));
    h
}

/// A client at the question of what it holds.
fn asked(h: &mut Harness) -> usize {
    let b = h.add_bot();
    let now = h.now;
    h.bots[b].log_in("one", b"ticket", now);
    assert!(h.run_until(2_000, |h| h.bots[b]
        .messages(false)
        .iter()
        .any(|m| matches!(m, ServerMessage::DataQuery(_)))));
    b
}

/// A list of a cell file holding its own iterations and these.
fn held(numbers: &[u32]) -> DataHeld {
    let cell = authored::install_files().cell.expect("the cell file");
    let own = dereth_dat::decompose::revision_over(cell.shared()) - 1;
    let mut runs = vec![IterationRun::Run {
        count: own,
        first: 1,
    }];
    runs.extend(numbers.iter().map(|n| IterationRun::One(n.cast_signed())));
    let total = own + u32::try_from(numbers.len()).unwrap_or(0);
    DataHeld {
        language: 0x8000_0001,
        files: vec![DataFileIterations {
            dat_file: CELL_FILE,
            iterations: IterationSet { total, runs },
        }],
    }
}

/// What a client holding the install's own files says of them, keeping overlays.
fn keeps_overlays(checks_records: bool) -> OverlayHeld {
    let cell = authored::install_files().cell.expect("the cell file");
    OverlayHeld {
        flags: if checks_records {
            OverlayHeld::CHECKS_RECORDS
        } else {
            0
        },
        bases: vec![OverlayBase {
            dat_file: CELL_FILE,
            fingerprint: layout_of(&cell),
        }],
    }
}

fn tell(h: &mut Harness, b: usize, keeps: Option<OverlayHeld>, data: DataHeld) {
    if let Some(k) = keeps {
        h.bots[b].send(false, queue::DATABASE, &ClientMessage::OverlayHeld(k));
    }
    h.bots[b].send(false, queue::DATABASE, &ClientMessage::DataHeld(data));
    h.run(10);
}

fn records(h: &Harness, b: usize) -> Vec<auberean_protocol::login::DataRecord> {
    h.bots[b]
        .messages(false)
        .into_iter()
        .filter_map(|m| match m {
            ServerMessage::DataRecord(r) => Some(r),
            _ => None,
        })
        .collect()
}

fn begun(h: &Harness, b: usize) -> Vec<auberean_protocol::login::DownloadBegins> {
    h.bots[b]
        .messages(false)
        .into_iter()
        .filter_map(|m| match m {
            ServerMessage::DownloadBegins(d) => Some(d),
            _ => None,
        })
        .collect()
}

/// The words a client was refused with, when it was.
fn refused(h: &Harness, b: usize) -> Option<StringInfo> {
    h.bots[b].messages(false).into_iter().find_map(|m| match m {
        ServerMessage::AccountBooted(t) => Some(t.0),
        _ => None,
    })
}

fn said(words: &str) -> Option<StringInfo> {
    Some(StringInfo::literal(words))
}

fn listed(h: &Harness, b: usize) -> bool {
    h.bots[b]
        .messages(false)
        .iter()
        .any(|m| matches!(m, ServerMessage::CharacterList(_)))
}

/// A client that keeps overlays and lacks the world's is told what's coming and sent it after the
/// pause; when it has stored it, it is told the download ended and sees its characters.
/// Behaviour: server.data.a-client-that-keeps-overlays-is-sent-what-it-lacks
#[test]
fn a_client_that_keeps_overlays_is_sent_what_it_lacks_and_then_its_characters() {
    let (files, numbers) = overlaid(3, 1);
    let mut h = serving(files, DatOverlaySettings::default());
    let b = asked(&mut h);
    tell(&mut h, b, Some(keeps_overlays(false)), held(&[]));

    let begins = begun(&h, b);
    assert_eq!(begins.len(), 1);
    let part = &begins[0].files[0];
    assert_eq!((part.dat_file, part.iteration), (CELL_FILE, numbers[0]));
    assert_eq!(part.download.len(), 3);
    assert!(part.purge.is_empty());
    assert!(records(&h, b).is_empty(), "not before the pause");
    assert!(!listed(&h, b));

    h.advance(PAUSE_SECONDS);
    h.run(10);
    let got = records(&h, b);
    assert_eq!(got.len(), 3);
    for (r, id) in got.iter().zip(&part.download) {
        assert_eq!(r.id, *id);
        assert_eq!(r.dat_file, CELL_FILE);
        // The kind the client's download waits for this id under.
        assert_eq!(r.kind, RecordType::LANDBLOCK.0);
    }
    assert_eq!(
        begins[0].bytes as usize,
        got.iter().map(|r| r.bytes.len()).sum::<usize>(),
        "the bytes it was told are the bytes it gets"
    );
    assert!(!listed(&h, b), "not till it has stored them");

    h.bots[b].send(false, queue::DATABASE, &ClientMessage::DownloadEnds(Empty));
    assert!(h.run_until(200, |h| listed(h, b)));
}

/// A client holding what the world has is sent nothing: not a client read through the same overlay,
/// and not a retail client patched to it. Holding one revision of two, it is sent the other.
/// Behaviour: server.data.a-client-that-keeps-overlays-is-sent-what-it-lacks
#[test]
fn a_client_holding_the_worlds_revisions_is_sent_only_what_it_lacks() {
    let (files, numbers) = overlaid(3, 2);
    let mut h = serving(files, DatOverlaySettings::default());
    let b = asked(&mut h);
    tell(&mut h, b, Some(keeps_overlays(false)), held(&numbers));
    assert!(begun(&h, b).is_empty());
    assert!(h.run_until(200, |h| listed(h, b)));

    let (files, numbers) = overlaid(3, 2);
    let mut h = serving(files, DatOverlaySettings::default());
    let b = asked(&mut h);
    tell(&mut h, b, None, held(&numbers));
    assert!(begun(&h, b).is_empty());
    assert!(h.run_until(200, |h| listed(h, b)));

    let (files, numbers) = overlaid(3, 2);
    let mut h = serving(files, DatOverlaySettings::default());
    let b = asked(&mut h);
    tell(&mut h, b, Some(keeps_overlays(false)), held(&numbers[..1]));
    let begins = begun(&h, b);
    assert_eq!(begins[0].files.len(), 1, "only the revision it lacks");
    assert_eq!(begins[0].files[0].iteration, numbers[1]);
    assert_eq!(begins[0].files[0].download.len(), 2, "the second and third");
}

/// A client that keeps overlays and holds other files is refused with the reason; told it checks
/// them itself, it is told what to check and sent the rest; with patching off, it is refused too.
/// Behaviour: server.data.a-client-the-overlay-doesnt-suit-is-refused-with-the-reason
#[test]
fn a_client_holding_other_files_is_refused_unless_it_checks_them_itself() {
    let other = |checks| {
        let mut k = keeps_overlays(checks);
        k.bases[0].fingerprint = [0x5A; 32];
        k
    };
    let (files, _) = overlaid(1, 1);
    let mut h = serving(files, DatOverlaySettings::default());
    let b = asked(&mut h);
    tell(&mut h, b, Some(other(false)), held(&[]));
    assert_eq!(refused(&h, b), said(OTHER_FILES));
    assert!(begun(&h, b).is_empty());

    // Naming no file of that kind is holding another.
    let (files, _) = overlaid(1, 1);
    let mut h = serving(files, DatOverlaySettings::default());
    let b = asked(&mut h);
    let none = OverlayHeld {
        flags: OverlayHeld::CHECKS_RECORDS,
        bases: Vec::new(),
    };
    tell(&mut h, b, Some(none), held(&[]));
    assert_eq!(refused(&h, b), said(OTHER_FILES));

    let (files, _) = overlaid(1, 1);
    let want = dereth_dat::overlay::records_digest(
        &files.cell.as_ref().expect("the cell file").shared().base(),
    )
    .expect("the file's records");
    let mut h = serving(files, DatOverlaySettings::default());
    let b = asked(&mut h);
    tell(&mut h, b, Some(other(true)), held(&[]));
    assert_eq!(refused(&h, b), None);
    let told: Vec<_> = h.bots[b]
        .messages(false)
        .into_iter()
        .filter_map(|m| match m {
            ServerMessage::OverlayBases(o) => Some(o),
            _ => None,
        })
        .collect();
    assert_eq!(told.len(), 1);
    assert_eq!(told[0].bases[0].dat_file, CELL_FILE);
    assert_eq!(told[0].bases[0].fingerprint, want);
    assert_eq!(begun(&h, b).len(), 1);

    let (files, _) = overlaid(1, 1);
    let off = DatOverlaySettings {
        patching: false,
        ..DatOverlaySettings::default()
    };
    let mut h = serving(files, off);
    let b = asked(&mut h);
    tell(&mut h, b, Some(keeps_overlays(false)), held(&[]));
    assert_eq!(refused(&h, b), said(NOT_PATCHING));
}

/// A retail client, which says nothing of overlays, lacking the world's is sent it, whether or not
/// the operator turned retail patching on: the world says it patches retail clients.
/// Behaviour: server.data.a-retail-client-lacking-a-worlds-overlay-is-patched
#[test]
fn a_retail_client_lacking_the_overlay_is_patched_whatever_the_operator_chose() {
    for enable_dat_patching in [false, true] {
        let (files, _) = overlaid(2, 1);
        let settings = DatOverlaySettings {
            enable_dat_patching,
            ..DatOverlaySettings::default()
        };
        let mut h = serving(files, settings);
        assert!(
            yalain_world::world_patch::patches_retail(&h.server),
            "patching {enable_dat_patching}: the world says it patches retail clients"
        );
        let b = asked(&mut h);
        tell(&mut h, b, None, held(&[]));
        assert_eq!(refused(&h, b), None, "patching {enable_dat_patching}");
        assert_eq!(begun(&h, b).len(), 1, "patching {enable_dat_patching}");
        h.advance(PAUSE_SECONDS + 1.0);
        h.run(10);
        assert_eq!(records(&h, b).len(), 2, "patching {enable_dat_patching}");
    }
}

/// A client holding a revision past its file's own that this world doesn't have was patched to
/// another world, and is refused, however it answered.
/// Behaviour: server.data.a-client-the-overlay-doesnt-suit-is-refused-with-the-reason
#[test]
fn a_client_patched_to_another_world_is_refused() {
    let (files, numbers) = overlaid(1, 1);
    let mut h = serving(files, DatOverlaySettings::default());
    let b = asked(&mut h);
    let theirs = numbers[0] + 50;
    tell(
        &mut h,
        b,
        Some(keeps_overlays(false)),
        held(&[numbers[0], theirs]),
    );
    assert_eq!(refused(&h, b), said(OTHER_WORLD));
    assert!(begun(&h, b).is_empty());
}

/// The rates: a client that keeps overlays has its own, a retail client the rate all retail clients
/// share. Counted with the harness's clock, up to the first second that matters; each by what has
/// gone by then, at most.
/// Behaviour: server.data.patch-records-go-at-the-operators-rate
#[test]
fn a_clients_records_go_at_its_rate_and_retail_clients_share_theirs() {
    // 600 records at a thousand a second: all gone in a second past the pause.
    let (files, _) = overlaid(600, 1);
    let mut h = serving(files, DatOverlaySettings::default());
    let b = asked(&mut h);
    tell(&mut h, b, Some(keeps_overlays(false)), held(&[]));
    h.advance(PAUSE_SECONDS);
    for _ in 0..60 {
        h.step_by(0.02);
    }
    assert_eq!(records(&h, b).len(), 600);

    // At sixty a minute: one a second, however many passes of the world there are.
    let (files, _) = overlaid(20, 1);
    let slow = DatOverlaySettings {
        records_per_minute: 60,
        ..DatOverlaySettings::default()
    };
    let mut h = serving(files, slow);
    let b = asked(&mut h);
    tell(&mut h, b, Some(keeps_overlays(false)), held(&[]));
    h.advance(PAUSE_SECONDS);
    for _ in 0..100 {
        h.step_by(0.02);
    }
    let n = records(&h, b).len();
    assert!((2..=4).contains(&n), "{n} in two seconds");

    // Retail, at the original servers' thousand a minute: two clients between them.
    let (files, _) = overlaid(200, 1);
    let mut h = serving(files, DatOverlaySettings::default());
    let a = asked(&mut h);
    tell(&mut h, a, None, held(&[]));
    let b = h.add_bot();
    let now = h.now;
    h.bots[b].log_in("two", b"ticket", now);
    assert!(h.run_until(2_000, |h| h.bots[b]
        .messages(false)
        .iter()
        .any(|m| matches!(m, ServerMessage::DataQuery(_)))));
    tell(&mut h, b, None, held(&[]));
    h.advance(PAUSE_SECONDS);
    for _ in 0..300 {
        h.step_by(0.02);
    }
    let n = records(&h, a).len() + records(&h, b).len();
    // Six seconds at 1,000 a minute is a hundred.
    assert!((80..=120).contains(&n), "{n} between them in six seconds");
}

/// Our client, lacking a world's overlay, says it keeps overlays, stores what it's sent as it logs
/// in and reads the world's record as its own from then on; its characters follow.
/// Behaviour: session.server.our-client-takes-a-worlds-overlay-as-it-logs-in
#[test]
fn our_client_takes_a_worlds_overlay_as_it_logs_in_and_reads_it() {
    let (files, _) = overlaid(1, 1);
    let cell = files.cell.clone().expect("the cell file");
    let id = cell
        .shared()
        .layer()
        .expect("an overlay")
        .records()
        .next()
        .expect("a record")
        .0
        .raw();
    let bytes = cell.read(id).expect("the world's record");
    let mut s = InProcess::new();
    s.h.server.world_patch = WorldPatch::of(&files).map(Arc::new);
    s.h.server.data = Some(Box::new(files));

    play::character_list(&mut s).expect("the characters, after what it lacks");
    let c = s.client_ref();
    let said = c
        .sent()
        .iter()
        .position(|m| matches!(m.message, ClientMessage::OverlayHeld(_)))
        .expect("it says it keeps overlays");
    let held = c
        .sent()
        .iter()
        .position(|m| matches!(m.message, ClientMessage::DataHeld(_)))
        .expect("it tells what it holds");
    assert!(said < held, "first");
    let q = QualifiedId {
        id,
        kind: RecordType::LANDBLOCK.0,
    };
    let stores = c.data_download().stores().expect("the stores");
    assert_eq!(
        stores.load_data(q).as_deref(),
        Some(&bytes[..]),
        "the record it reads is the world's"
    );
}

/// A client lacking a world's records of the portal and English files is sent them as it is sent
/// cell records: one part of the download for each file, at that file's revision, each record under
/// its own kind; nothing of a file the world doesn't change.
/// Behaviour: server.data.a-client-that-keeps-overlays-is-sent-what-it-lacks
#[test]
fn a_client_lacking_portal_and_english_records_is_sent_a_part_for_each_file() {
    let (files, portal_records, english_records) = overlaid_portal_and_english();
    let first = |f: &Option<auberean_dat::DatFile>| {
        dereth_dat::decompose::revision_over(f.as_ref().expect("the file").shared())
    };
    let (portal_revision, english_revision) = (first(&files.portal), first(&files.english));
    let install = authored::install_files();
    let keeps = OverlayHeld {
        flags: 0,
        bases: [
            (PORTAL_FILE, &install.portal),
            (CELL_FILE, &install.cell),
            (ENGLISH_FILE, &install.english),
        ]
        .into_iter()
        .map(|(dat_file, f)| OverlayBase {
            dat_file,
            fingerprint: layout_of(f.as_ref().expect("the file")),
        })
        .collect(),
    };
    let mut h = serving(files, DatOverlaySettings::default());
    let b = asked(&mut h);
    tell(&mut h, b, Some(keeps), held(&[]));

    let begins = begun(&h, b);
    assert_eq!(begins.len(), 1);
    let parts = &begins[0].files;
    let part = |dat_file| {
        parts
            .iter()
            .find(|p| p.dat_file == dat_file)
            .unwrap_or_else(|| panic!("a part for {dat_file:#x}: {parts:?}"))
    };
    assert_eq!(part(PORTAL_FILE).iteration, portal_revision);
    assert_eq!(part(ENGLISH_FILE).iteration, english_revision);
    assert_eq!(parts.len(), 2, "nothing of the cell file");

    h.advance(PAUSE_SECONDS);
    h.run(10);
    let got = records(&h, b);
    for (dat_file, records, kind) in [
        (PORTAL_FILE, &portal_records, RecordType::SCRIPT_STATE),
        (ENGLISH_FILE, &english_records, RecordType::STRING_TABLE),
    ] {
        let mut sent: Vec<_> = got
            .iter()
            .filter(|r| r.dat_file == dat_file)
            .map(|r| (r.id, r.kind))
            .collect();
        sent.sort_unstable();
        let mut want: Vec<_> = records.iter().map(|(id, _)| (*id, kind.0)).collect();
        want.sort_unstable();
        assert_eq!(sent, want, "{dat_file:#x}");
    }
}

/// Our client stores a world's records of the portal and English files as it logs in, and reads
/// them as its own.
/// Behaviour: session.server.our-client-takes-a-worlds-overlay-as-it-logs-in
#[test]
fn our_client_takes_a_worlds_portal_and_english_records_and_reads_them() {
    let (files, portal_records, english_records) = overlaid_portal_and_english();
    let mut s = InProcess::new();
    s.h.server.world_patch = WorldPatch::of(&files).map(Arc::new);
    s.h.server.data = Some(Box::new(files));

    play::character_list(&mut s).expect("the characters, after what it lacks");
    let stores = s.client_ref().data_download().stores().expect("the stores");
    for (records, kind) in [
        (&portal_records, RecordType::SCRIPT_STATE),
        (&english_records, RecordType::STRING_TABLE),
    ] {
        for (id, bytes) in records {
            let q = QualifiedId {
                id: *id,
                kind: kind.0,
            };
            assert_eq!(
                stores.load_data(q).as_deref(),
                Some(&bytes[..]),
                "{id:#x} reads as the world's"
            );
        }
    }
}

/// A reload that opened the files again patches clients to the overlay as it now is, and a world
/// whose overlay it no longer finds patches no one.
/// Behaviour: server.data.a-reload-answers-from-the-overlay-as-it-now-is
#[test]
fn a_reload_patches_the_next_client_to_the_overlay_as_it_now_is() {
    let (files, numbers) = overlaid(1, 1);
    let mut h = serving(files, DatOverlaySettings::default());
    let (files, later) = overlaid(1, 2);
    let t = WorldTables {
        patch: WorldPatch::of(&files),
        files: Some(Box::new(files)),
        ..WorldTables::default()
    };
    let clock = h.clock();
    h.server.reload(&mut h.net, clock, t);
    let b = asked(&mut h);
    tell(&mut h, b, Some(keeps_overlays(false)), held(&numbers));
    let begins = begun(&h, b);
    assert_eq!(begins[0].files.len(), 1);
    assert_eq!(begins[0].files[0].iteration, later[1]);

    let t = WorldTables {
        patch: None,
        files: Some(Box::new(authored::install_files())),
        ..WorldTables::default()
    };
    let clock = h.clock();
    h.server.reload(&mut h.net, clock, t);
    assert!(h.server.world_patch.is_none());
}

/// Our client in a world's shop: the world's copy of Sabine's store template, under a title of its
/// own, sent to it as it logs in, and a keeper whose page names the copy. The shop window its
/// scripts show is called by the copy's title; the sales are the store's.
/// Behaviour: session.server.our-client-shows-a-worlds-shop-under-its-title
#[test]
fn our_client_shows_a_worlds_shop_under_its_own_title() {
    use auberean_client_sdk::runtime::store::{notice, SHOP};
    use auberean_script::native_object::{Field, NativeObject};
    use yalain_content::stores::{Keeper, Sale, SaleGate};
    use yalain_world_db::compile::Shop;

    const SABINE: u32 = 0x6F00_16E5;
    const COPY: u32 = 0x6F10_0000;
    const TITLE: &str = "Bob's Fine Goods";
    let p = authored::Portal::open();
    let shop = Shop {
        key: "bobs".into(),
        from: SABINE,
        id: COPY,
        name_text: TITLE.into(),
        description_text: None,
        pinned: true,
    };
    let copy = yalain_world_db::overlay::shop_copies(
        std::slice::from_ref(&shop),
        &|id| p.dat.read(id).ok(),
        &p.ctx,
        &p.library(),
    )
    .expect("the copy");
    let files = authored::install_files();
    let portal = files.portal.clone().expect("the portal file");
    let folder = Arc::new(MemoryFolder::new(std::path::Path::new("a-world")));
    let dir = OverlayDir::in_folder(folder.clone()).expect("a folder");
    let mut w = dir
        .writer(
            ModernDat::Portal,
            portal.shared(),
            role_file(ModernDat::Portal),
            WORLD,
            0,
        )
        .expect("a writer");
    let r = dereth_dat::decompose::revision_over(portal.shared());
    for (id, bytes) in &copy {
        w.put(DataId(*id), bytes, 1, r, 0).expect("the record");
    }
    w.add_iteration(r, 0).expect("the revision");
    w.flush(0).expect("written");
    let overlay = WorldOverlay::in_folder(folder).expect("the overlay");
    let files = InstallFiles {
        portal: Some(overlay.over(portal).expect("it lies over the portal")),
        ..files
    };

    let mut s = InProcess::new();
    s.h.server.world_patch = WorldPatch::of(&files).map(Arc::new);
    s.h.server.data = Some(Box::new(files));
    s.h.server.stores.keepers = vec![Keeper {
        template: authored::GUARD,
        store: SABINE,
        shown: Some(COPY),
        greeting: Some(yalain_content::Words::Literal("Welcome".into())),
        placed: "authored for a test".into(),
    }];
    s.h.server.stores.sales = vec![Sale {
        store: SABINE,
        ordinal: 0,
        product: authored::SWORD,
        cost: 10.0,
        currency: 0x4700_0538,
        stack: 1,
        icon: Vec::new(),
        name: (0x01A2_D809, 0x2500_23D3),
        placed: "authored for a test".into(),
        gate: SaleGate::default(),
    }];
    play::character_list(&mut s).expect("the characters, after what it lacks");
    let me = play::create(&mut s, "Aubrey").expect("made");
    play::enter(&mut s, me).expect("in the world");
    play::see(&mut s, &[authored::GUARD]).expect("the keeper");
    let keeper = play::find(s.client_ref(), authored::GUARD).expect("the keeper");
    let from = s.client_ref().notices.len();
    play::talk_to(&mut s, keeper.id).expect("greeted");
    let shown = |c: &auberean_client_sdk::Client| {
        c.notices[from..]
            .iter()
            .filter(|n| n.native == notice::VIEW_STORE)
            .flat_map(|n| n.objects.iter().flatten())
            .find_map(|o| match o {
                NativeObject::Adaptor { ty, fields } if *ty == SHOP => Some(fields.clone()),
                _ => None,
            })
    };
    play::until(&mut s, play::HANG, "the shop window", |c| {
        shown(c).is_some()
    })
    .expect("the shop shows");
    let fields = shown(s.client_ref()).expect("the shop");
    let Some(Field::Value(name)) = fields.get("Name") else {
        panic!("a shop with no name: {fields:?}");
    };
    let NativeObject::TextInfo(t) = name.as_ref() else {
        panic!("a name that is no text: {name:?}");
    };
    assert_eq!(
        t.literal
            .as_deref()
            .map(String::from_utf16_lossy)
            .as_deref(),
        Some(TITLE),
        "{t:?}"
    );
    let Some(Field::Many(sales)) = fields.get("Sale") else {
        panic!("no sales: {fields:?}");
    };
    assert_eq!(sales.len(), 1, "the store's own sale");
}
