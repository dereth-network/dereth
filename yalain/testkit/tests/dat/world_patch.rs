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
use yalain_world::download::{InstallFiles, CELL_FILE};
use yalain_world::reload::WorldTables;
use yalain_world::world_patch::{
    DatOverlaySettings, WorldPatch, NOT_KEPT, NOT_PATCHING, OTHER_FILES, OTHER_WORLD, PAUSE_SECONDS,
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

/// A retail client, which says nothing of overlays, lacking the world's: refused with what to play
/// instead, unless the operator turned its patching on, when it is sent it.
/// Behaviour: server.data.a-retail-client-lacking-a-worlds-overlay-is-refused-unless-patching-is-on
#[test]
fn a_retail_client_lacking_the_overlay_is_refused_unless_the_operator_patches_it() {
    let (files, _) = overlaid(2, 1);
    let mut h = serving(files, DatOverlaySettings::default());
    let b = asked(&mut h);
    tell(&mut h, b, None, held(&[]));
    assert_eq!(refused(&h, b), said(NOT_KEPT));
    assert!(begun(&h, b).is_empty());

    let (files, _) = overlaid(2, 1);
    let on = DatOverlaySettings {
        enable_dat_patching: true,
        ..DatOverlaySettings::default()
    };
    let mut h = serving(files, on);
    let b = asked(&mut h);
    tell(&mut h, b, None, held(&[]));
    assert_eq!(refused(&h, b), None);
    assert_eq!(begun(&h, b).len(), 1);
    h.advance(PAUSE_SECONDS + 1.0);
    h.run(10);
    assert_eq!(records(&h, b).len(), 2);
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
    let on = DatOverlaySettings {
        enable_dat_patching: true,
        ..DatOverlaySettings::default()
    };
    let mut h = serving(files, on);
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
