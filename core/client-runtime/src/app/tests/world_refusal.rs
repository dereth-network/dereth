//! A world whose data the client refuses is not played: refused at the start, the client never
//! connects; refused during the data patch, the session ends before character select. Each world a
//! server serves keeps its own folder in the server's folder.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use dereth_client_contract::pregame::{DisconnectNotice, GamePhase};
use dereth_client_net::client_session::{DddEvent, SessionEvent};
use dereth_dat::overlay::OverlayDir;
use dereth_dat::{DatFile, ModernDat, RetailDatStore};
use dereth_primitives::DataId;
use dereth_protocol::admin::{DddBeginDdd, DddData, DddInterrogation, DddOverlayManifest};

use super::*;

/// A picture id the retail portal file holds, which the tests' worlds replace.
const PICTURE: DataId = DataId(0x0600_0001);

fn retail_store() -> Arc<RetailDatStore> {
    Arc::new(dereth_dat::testing::open_store().unwrap_or_else(|| {
        panic!(
            "the retail dats are this test's oracle and they are not under {}",
            dereth_dat::testing::dat_dir().display()
        )
    }))
}

/// Write `bytes` as the picture of the world `world` in the overlay folder `dir`, in a portal
/// container made over `base`.
fn write_picture(dir: &Path, base: &DatFile, world: &str, bytes: &[u8]) {
    let dir = OverlayDir::new(dir).expect("an overlay folder");
    let mut w = dir
        .writer(ModernDat::Portal, base, "client_portal.dat", world, 1)
        .expect("an overlay writer");
    let next = dereth_dat::overlay::base_iterations(base)
        .last()
        .copied()
        .unwrap_or(0)
        + 1;
    w.put(PICTURE, bytes, 1, next, 1).expect("a record");
    w.add_iteration(next, 1).expect("its revision");
    w.flush(1).expect("written");
}

fn bring_up(cfg: Config, store: &Arc<RetailDatStore>) -> App<NullShell> {
    App::<NullShell>::bring_up_with_store(
        cfg,
        Some(Arc::clone(store)),
        |_| Ok(Platform::headless(64, 64)),
        |_, _, _, _| Ok(Box::new(crate::present::NullPresentation::new(64, 64))),
    )
    .expect("bring-up")
}

/// A scripted run with no interface that connects to a server, its preferences in `settings`.
fn connecting(settings: &Path) -> Config {
    Config {
        headless: true,
        frames: None,
        connect: true,
        sound: false,
        host: "127.0.0.1".into(),
        port: 9000,
        account: "builder".into(),
        vg_password: "password".into(),
        enter_world: true,
        start_char: "Builder".into(),
        preferences_file: settings.join("UserPreferences.ini"),
        dat_dir: PathBuf::from("a directory that does not exist"),
        ..Config::default()
    }
}

/// Where the session stands, as the interfaces read it.
fn phase(app: &mut App<NullShell>) -> GamePhase {
    app.build_host_state();
    app.host_state().phase.clone()
}

fn character_set() -> SessionEvent {
    use dereth_protocol::login::{CharacterIdentity, LoginCharacterSet};
    SessionEvent::CharacterSet(Box::new(LoginCharacterSet {
        characters: vec![CharacterIdentity {
            gid: dereth_primitives::ObjectId(0x5000_0001),
            name: "Builder".into(),
            seconds_greyed_out: 0,
        }],
        account: "builder".into(),
        ..LoginCharacterSet::default()
    }))
}

/// Behaviour: net.dat-patch.an-overlay-the-client-cannot-take-is-refused-and-nothing-is-written
/// A world whose folder the client refuses when it starts is never connected to, for each reason
/// the client can tell then: the folder's world is on the blocklist, it holds another world's
/// data, it was made for other game files, it cannot be read. The session's end says why, the run
/// with no interface stops, and no connection can be attached after. The same folder, not
/// refused, is connected to.
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the retail dats: --features retail-dats"
)]
fn a_world_refused_at_the_start_is_never_connected_to_for_each_local_reason() {
    let store = retail_store();
    let scratch = dereth_dat::testing::ScratchDir::new("refused-at-start").expect("scratch");
    type Make = fn(&Path, &RetailDatStore);
    let cases: [(&str, Make, &str); 4] = [
        (
            "blocked",
            |dir, store| {
                write_picture(dir, store.portal(), "Elsewhere", b"elsewhere");
                let settings = dir.parent().unwrap().join("blocked-settings");
                std::fs::create_dir_all(&settings).unwrap();
                std::fs::write(
                    settings.join(crate::world_overlay::BLOCKLIST_FILE),
                    "Elsewhere\n",
                )
                .unwrap();
            },
            "\"Elsewhere\" is on your overlay blocklist",
        ),
        (
            "other-world",
            |dir, store| {
                write_picture(dir, store.portal(), "Here", b"here");
                let dir = OverlayDir::new(dir).unwrap();
                let mut w = dir
                    .writer(
                        ModernDat::Cell,
                        store.cell(),
                        "client_cell_1.dat",
                        "There",
                        1,
                    )
                    .unwrap();
                w.put(DataId(0xA9B4_FFFF), b"there", 1, 1, 1).unwrap();
                w.flush(1).unwrap();
            },
            "holds another world's data (\"There\")",
        ),
        (
            "other-files",
            // A portal container made over the language file: not the player's portal file.
            |dir, store| write_picture(dir, store.local(), "Mine", b"mine"),
            "made for other game files than yours",
        ),
        (
            "unreadable",
            |dir, _| {
                std::fs::create_dir_all(dir).unwrap();
                std::fs::write(
                    dir.join(dereth_dat::overlay::container_name(ModernDat::Portal)),
                    b"not an overlay at all",
                )
                .unwrap();
            },
            "its folder could not be read",
        ),
    ];
    for (name, make, says) in cases {
        let folder = scratch.path().join(name);
        make(&folder, &store);
        let settings = scratch.path().join(format!("{name}-settings"));
        std::fs::create_dir_all(&settings).unwrap();
        let cfg = Config {
            overlay_dat_dir: Some(folder),
            ..connecting(&settings)
        };
        let mut app = bring_up(cfg, &store);
        assert!(app.link.is_none(), "{name}: never connected");
        let text = app.world_refused().map(str::to_owned);
        assert!(
            text.as_deref().is_some_and(|t| t.contains(says)),
            "{name}: {text:?}"
        );
        assert!(!app.store.has_overlay(), "{name}: nothing of it is read");
        assert_eq!(
            phase(&mut app),
            GamePhase::Disconnected(DisconnectNotice::OverlayRefused(text.unwrap())),
            "{name}"
        );
        let host = app.host_state();
        assert!(
            host.has_packet_controller && !host.connected && !host.patch_finished,
            "{name}: the screens show a run that connects, not connected, its data not checked"
        );
        app.process_logon_event_queue(&mut NullShell, Vec::new());
        assert_eq!(app.script, EnterWorldScript::Done, "{name}: the run stops");
        let net = crate::net::ClientNetwork::new("127.0.0.1", 9000, "builder", "password", 1)
            .expect("an endpoint");
        assert!(
            app.attach_replay_network(net).is_err(),
            "{name}: no connection after"
        );
        assert_eq!(app.host_state().entered_character, None, "{name}");
    }

    // Not refused, the same kind of folder is connected to.
    let folder = scratch.path().join("taken");
    write_picture(&folder, store.portal(), "Taken", b"taken");
    let settings = scratch.path().join("taken-settings");
    std::fs::create_dir_all(&settings).unwrap();
    let app = bring_up(
        Config {
            overlay_dat_dir: Some(folder),
            ..connecting(&settings)
        },
        &store,
    );
    assert_eq!(app.world_refused(), None);
    assert!(app.link.is_some(), "connected");
    assert_eq!(
        app.store.read_portal(PICTURE).expect("the picture"),
        b"taken"
    );
}

/// A world's manifest naming a record of the portal file whose bytes are `sha256`, over the
/// player's portal file as `base_fingerprint` names it.
fn manifest(world: &str, base_fingerprint: [u8; 32], sha256: [u8; 32]) -> DddOverlayManifest {
    DddOverlayManifest {
        world_key: world.into(),
        total_bytes: 0,
        files: vec![dereth_protocol::admin::OverlayFileManifest {
            dat_file_type: 0,
            dat_file_id: 1,
            base_name: "client_portal.dat".into(),
            base_fingerprint,
            base_records: None,
            base_iterations: 0,
            exact_iterations: false,
            revisions: Vec::new(),
            records: vec![dereth_protocol::admin::OverlayRecord {
                id: PICTURE.raw(),
                iteration: 1,
                size: 5,
                sha256,
            }],
            tombstones: Vec::new(),
        }],
    }
}

/// The world's picture, sent during the patch.
fn picture(bytes: &[u8], iteration: u32) -> DddData {
    DddData {
        dat_file_type: 0,
        dat_file_id: 1,
        resource_type: 6,
        resource_id: PICTURE.raw(),
        iteration,
        compressed: 0,
        version: 1,
        data_size: u32::try_from(bytes.len() + 4).unwrap(),
        data: bytes.to_vec(),
    }
}

/// Behaviour: net.dat-patch.an-overlay-the-client-cannot-take-is-refused-and-nothing-is-written
/// A world whose data the client refuses during the data patch ends the session there, for each
/// reason only the server can tell: the world is on the blocklist, its overlay was made for other
/// game files, a record is not the one its manifest names. The rest of the patch is not taken,
/// the patch never finishes, character select is never reached, the scripted entry stops, and no
/// character goes in; nothing of the world is left in its folder.
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the retail dats: --features retail-dats"
)]
fn a_world_refused_during_the_patch_ends_the_session_before_character_select() {
    let store = retail_store();
    let base = dereth_dat::overlay::fingerprint(store.portal());
    let good = dereth_dat::overlay::record_hash(b"good!");
    let scratch = dereth_dat::testing::ScratchDir::new("refused-at-patch").expect("scratch");
    let cases: [(&str, Vec<DddEvent>, &str); 3] = [
        (
            "blocked",
            vec![DddEvent::OverlayManifest(Box::new(manifest(
                "Bad", base, good,
            )))],
            "\"Bad\" is on your overlay blocklist",
        ),
        (
            "other-files",
            vec![DddEvent::OverlayManifest(Box::new(manifest(
                "Good", [0xAB; 32], good,
            )))],
            "made for other game files than yours",
        ),
        (
            "bad-record",
            vec![
                // Ahead of the manifest, and newer than the installed file's own record.
                DddEvent::Data(Box::new(picture(b"forge", 100_000))),
                DddEvent::OverlayManifest(Box::new(manifest("Good", base, good))),
            ],
            "the server sent data that does not match it",
        ),
    ];
    for (name, events, says) in cases {
        let settings = scratch.path().join(format!("{name}-settings"));
        std::fs::create_dir_all(&settings).unwrap();
        std::fs::write(settings.join(crate::world_overlay::BLOCKLIST_FILE), "Bad\n").unwrap();
        let folder = scratch.path().join(name);
        let cfg = Config {
            connect: false,
            overlay_dat_dir: Some(folder.clone()),
            ..connecting(&settings)
        };
        let mut app = bring_up(cfg, &store);
        let net = crate::net::ClientNetwork::new("127.0.0.1", 9000, "builder", "password", 1)
            .expect("an endpoint");
        assert!(app.attach_replay_network(net).is_ok());
        let mut shell = NullShell;
        app.process_logon_event_queue(&mut shell, vec![character_set()]);
        app.process_logon_event_queue(
            &mut shell,
            vec![SessionEvent::Ddd(DddEvent::Interrogation(
                DddInterrogation::default(),
            ))],
        );
        assert!(
            !app.host_state().patch_finished,
            "{name}: the patch is under way"
        );
        // The rest of the patch comes in the same frame, and is not taken.
        let mut batch: Vec<SessionEvent> = events.into_iter().map(SessionEvent::Ddd).collect();
        batch.extend([
            SessionEvent::Ddd(DddEvent::Begin(DddBeginDdd::default())),
            SessionEvent::Ddd(DddEvent::Data(Box::new(picture(b"good!", 1)))),
            SessionEvent::Ddd(DddEvent::End),
        ]);
        app.process_logon_event_queue(&mut shell, batch);
        let text = app.world_refused().map(str::to_owned);
        assert!(
            text.as_deref().is_some_and(|t| t.contains(says)),
            "{name}: {text:?}"
        );
        assert!(!app.host_state().patch_finished, "{name}");
        assert_eq!(
            phase(&mut app),
            GamePhase::Disconnected(DisconnectNotice::OverlayRefused(text.unwrap())),
            "{name}"
        );
        assert_eq!(app.script, EnterWorldScript::Done, "{name}");
        app.process_logon_event_queue(&mut shell, vec![character_set()]);
        assert_eq!(app.host_state().entered_character, None, "{name}");
        assert!(
            !matches!(
                phase(&mut app),
                GamePhase::CharacterSelect | GamePhase::EnteringWorld
            ),
            "{name}"
        );
        assert!(
            OverlayDir::new(&folder).unwrap().containers().is_empty(),
            "{name}: nothing of the world is written"
        );
    }
}

thread_local! {
    /// The per-user cache the per-world tests keep their servers' folders in.
    static CACHE: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

/// The disk, with the per-user cache in [`CACHE`].
fn keep_cache_in(dir: &Path) {
    CACHE.with(|c| *c.borrow_mut() = Some(dir.to_path_buf()));
    crate::platform::files::install(crate::platform::files::FileHost {
        cache_dir: || CACHE.with(|c| c.borrow().clone()),
        ..crate::platform::files::DISK
    });
}

/// Behaviour: net.dat-patch.each-world-a-server-serves-keeps-its-own-folder
/// A run keeping a folder per server starts on the folder of the world the server served last,
/// and its answer names that world. When the server names itself as another world, that world's
/// folder takes its place: the files read through it, the next answer is that world's, and the
/// next run starts on it; a world new to the server has an empty folder, and its answer names no
/// world. A server that patches it anyway, as the world the answer named, is refused: what it sent
/// is not the world's.
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the retail dats: --features retail-dats"
)]
fn a_world_naming_itself_as_another_is_read_and_answered_for_from_its_own_folder() {
    use dereth_protocol::admin::DddInterrogationResponse;
    let store = retail_store();
    let scratch = dereth_dat::testing::ScratchDir::new("each-world").expect("scratch");
    keep_cache_in(&scratch.path().join("cache"));
    let settings = scratch.path().join("settings");
    std::fs::create_dir_all(&settings).unwrap();
    let root = scratch.path().join("cache/overlays/127.0.0.1-9000");
    write_picture(
        &crate::world_overlay::world_folder(&root, "Alpha"),
        store.portal(),
        "Alpha",
        b"alpha",
    );
    write_picture(
        &crate::world_overlay::world_folder(&root, "Beta"),
        store.portal(),
        "Beta",
        b"beta!",
    );
    crate::world_overlay::remember_world(&root, "Alpha").unwrap();

    let start = |switch_to: &str| {
        let mut app = bring_up(connecting(&settings), &store);
        // The test speaks for the server: the run's connection is one it feeds.
        app.link = Some(crate::net::NetLink::replay(
            crate::net::ClientNetwork::new("127.0.0.1", 9000, "builder", "password", 1)
                .expect("an endpoint"),
        ));
        assert_eq!(
            crate::world_overlay::laid_world(&app.store).as_deref(),
            Some("Alpha"),
            "the world the server served last"
        );
        let mut shell = NullShell;
        app.process_logon_event_queue(
            &mut shell,
            vec![
                SessionEvent::Ddd(DddEvent::Interrogation(DddInterrogation::default())),
                SessionEvent::Ddd(DddEvent::OverlayManifest(Box::new(DddOverlayManifest {
                    world_key: switch_to.into(),
                    ..DddOverlayManifest::default()
                }))),
            ],
        );
        app
    };
    let answer = |app: &App<NullShell>| {
        crate::app::ddd_interrogation_response(
            &app.store,
            0,
            true,
            crate::world_overlay::laid_world(&app.store).as_deref(),
        )
    };

    // Beta names itself: its folder is read, the next answer is Beta's, and Beta is noted.
    let mut app = start("Beta");
    assert_eq!(app.world_refused(), None, "{:?}", app.ddd().notices());
    assert_eq!(app.store.read_portal(PICTURE).unwrap(), b"beta!");
    assert_eq!(
        answer(&app).world(),
        Some(dereth_dat::overlay::world_hash("Beta"))
    );
    assert_eq!(
        app.ddd().dir(),
        Some(crate::world_overlay::world_folder(&root, "Beta").as_path())
    );
    assert_eq!(
        crate::world_overlay::last_world(&root).as_deref(),
        Some("Beta")
    );
    // The server asks again, and the patch goes on as Beta's.
    let mut shell = NullShell;
    app.process_logon_event_queue(
        &mut shell,
        vec![
            SessionEvent::Ddd(DddEvent::Interrogation(DddInterrogation::default())),
            SessionEvent::Ddd(DddEvent::Begin(DddBeginDdd::default())),
            SessionEvent::Ddd(DddEvent::End),
        ],
    );
    assert_eq!(app.world_refused(), None, "{:?}", app.ddd().notices());
    assert!(app.host_state().patch_finished);
    assert_eq!(app.store.read_portal(PICTURE).unwrap(), b"beta!");

    // A world new to the server: an empty folder of its own, and an answer naming no world.
    crate::world_overlay::remember_world(&root, "Alpha").unwrap();
    let app = start("Gamma");
    assert_eq!(app.world_refused(), None);
    assert!(!app.store.has_overlay());
    let r = answer(&app);
    assert_eq!(r.world(), None);
    assert_eq!(r.flags & DddInterrogationResponse::FLAG_WORLD, 0);
    assert_eq!(
        app.ddd().dir(),
        Some(crate::world_overlay::world_folder(&root, "Gamma").as_path())
    );

    // A server that patches Beta as the world the answer named is refused.
    crate::world_overlay::remember_world(&root, "Alpha").unwrap();
    let mut app = start("Beta");
    app.process_logon_event_queue(
        &mut shell,
        vec![SessionEvent::Ddd(DddEvent::Begin(DddBeginDdd::default()))],
    );
    let text = app.world_refused().map(str::to_owned);
    assert!(
        text.as_deref().is_some_and(|t| t.contains("Connect again")),
        "{text:?}"
    );
    assert_eq!(app.script, EnterWorldScript::Done);
    assert_eq!(
        crate::world_overlay::last_world(&root).as_deref(),
        Some("Beta"),
        "the next run starts on Beta's folder"
    );
    assert_eq!(
        OverlayDir::new(&crate::world_overlay::world_folder(&root, "Alpha"))
            .unwrap()
            .world_key()
            .as_deref(),
        Some("Alpha"),
        "Alpha's folder keeps Alpha's data"
    );
}
