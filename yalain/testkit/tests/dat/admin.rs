//! An admin's whole session with the September 2013 data: our client and our server together, an
//! admin character's commands typed into the client's command line and run by the game's own scripts.

use auberean_assets::property::{Property, PropertyCollection, PropertyGroup, PropertyValue};
use auberean_client_sdk::runtime::body::BodyMaterial;
use auberean_client_sdk::Action;
use yalain_testkit::session::{self as play, Drive, InProcess};
use yalain_world::admin::{self, Issuer};

fn ok<T>(r: Result<T, String>) -> T {
    r.unwrap_or_else(|e| panic!("{e}"))
}

/// Run a console command.
fn console(s: &mut InProcess, line: &str) {
    let c = admin::parse(line).expect("a command").expect("it reads");
    let clock = s.h.clock();
    admin::run(&mut s.h.server, &mut s.h.net, clock, &Issuer::Console, &c);
}

/// An admin character in the world.
fn admin_session() -> InProcess {
    let mut s = InProcess::new();
    ok(play::character_list(&mut s));
    let me = ok(play::create(&mut s, "Keeper"));
    console(&mut s, "set-accountadmin ours");
    console(&mut s, "set-characteradmin Keeper");
    ok(play::enter(&mut s, me));
    s
}

/// Type a line and let it run.
fn typed(s: &mut InProcess, line: &str) {
    s.client().act(Action::Say { text: line.into() });
    play::wait(s, 0.5);
}

/// The lines the client shows, in order.
fn shown(s: &InProcess) -> Vec<String> {
    s.client_ref()
        .shown
        .iter()
        .filter_map(|l| l.text.literal.as_ref())
        .map(|l| String::from_utf16_lossy(l))
        .collect()
}

/// An admin character's client knows the game's admin commands, and typing one that runs on the
/// client alone reaches every native under it: each runs to its end, none stopping for want of a
/// port. The commands of the 48 natives only an admin's commands reach: physics properties, the
/// script machine's tables, the object table, the time, the quests and acts, the places, test scripts,
/// reading a file of lines, a report, the debugger.
/// Behaviour: script.admin.client-side-commands-run-the-admin-natives
#[test]
fn an_admin_characters_client_side_commands_run_to_their_end() {
    let mut s = admin_session();
    // `/cliprops` lists what the cells made of the player's body. This session has no scene, so no
    // bodies and no front end to publish one: it is handed over here, as the scene hands it over.
    let me = s.client_ref().model.player.expect("the player");
    s.client()
        .with_host(|h| {
            h.materials.insert(
                me,
                BodyMaterial {
                    cell: 0,
                    properties: PropertyCollection {
                        groups: vec![PropertyGroup {
                            group: 1,
                            properties: vec![Property {
                                id: 1,
                                group: 1,
                                value: PropertyValue::Integer(1),
                                ..Property::default()
                            }],
                            optional: Vec::new(),
                        }],
                    },
                },
            );
        })
        .expect("the player's scripts run");
    for line in [
        "/cliprops",
        "/vm query cli",
        "/vm perf cli",
        "/vm proc cli",
        "/vm procall cli",
        "/queryreference cli",
        "/time",
        "/time 12",
        "/dumpobjmaint",
        "/qst list client",
        "/storyquest list client",
        "/help teleplace",
        "/trade query cli",
        "/unlocksql",
        "/ctest nothing",
        "/loadfile nothing.txt",
        "/reportbug code nothing to report",
        "/cdbg help",
        "/location",
        "/myid",
    ] {
        typed(&mut s, line);
    }
    let stops = s.client_ref().script_stops();
    assert!(stops.is_empty(), "{stops:#?}");
    let all = shown(&s).join("\n");
    assert!(all.contains("Property group"), "{all}");
    assert!(all.contains("Property name"), "{all}");
    assert!(all.contains("perf:"), "{all}");
    assert!(all.contains("Object Maintainter Dump"), "{all}");
}

/// An admin's command that goes to the server runs the game's scripts to a plain call, the server
/// answers it, and the client's own admin script shows the line, under the name of its log type.
/// Behaviour: session.server.an-admins-command-reaches-the-server-and-is-answered
#[test]
fn an_admins_server_command_is_answered_in_the_clients_window() {
    let mut s = admin_session();
    typed(&mut s, "/serverlist");
    play::wait(&mut s, 2.0);
    let all = shown(&s).join("\n");
    assert!(all.contains("characters in the world"), "{all}");
    assert!(all.contains("\\[Admin\\] "), "{all}");
    let stops = s.client_ref().script_stops();
    assert!(stops.is_empty(), "{stops:#?}");
    assert!(s.h.server.admin_calls.refused.is_empty());
}

/// What the client has for an admin who wants to be left alone by the world: `/ethereality`,
/// `/neversaydie` and `/attackable` typed run the game's own scripts to calls the server honors, and
/// every other client that knows the admin is told what it now passes through.
/// Behaviour: session.server.an-admin-passes-through-and-cannot-die
#[test]
fn an_admin_types_its_way_out_of_harm_and_through_the_world() {
    let mut s = admin_session();
    typed(&mut s, "/ethereality super");
    typed(&mut s, "/neversaydie on");
    typed(&mut s, "/attackable super");
    play::wait(&mut s, 1.0);
    assert!(s.h.server.admin_calls.refused.is_empty());
    let p = s.h.server.players.values().next().expect("in the world");
    assert_eq!(
        p.admin.ethereality,
        yalain_world::admin_calls::Ethereality::Super
    );
    assert!(p.admin.never_say_die);
    assert_eq!(
        p.admin.attackable,
        yalain_world::admin_calls::Attackable::Super
    );
    assert_eq!(yalain_world::kingdoms::kept_ethereality(&p.qualities), -1);
    typed(&mut s, "/ethereality normal");
    play::wait(&mut s, 1.0);
    let p = s.h.server.players.values().next().expect("in the world");
    assert_ne!(yalain_world::kingdoms::kept_ethereality(&p.qualities), -1);
    let stops = s.client_ref().script_stops();
    assert!(stops.is_empty(), "{stops:#?}");
}

/// The library an admin's retail client is served, made from the install's own: every encrypted
/// send of the 175 the game has becomes a plain one, in the same bytes otherwise.
/// Behaviour: server.admin.library-sends-admin-calls-in-the-clear
#[test]
fn the_admin_library_is_the_installs_with_every_send_in_the_clear() {
    use yalain_world::admin_library::{plain, AdminLibrary, RECORD};
    let dir = auberean_dat::testing::require_target_install();
    let (_, path) = auberean_dat::locate::install_dats(&dir)
        .expect("list the install")
        .into_iter()
        .find(|(name, _)| *name == auberean_dat::locate::DatName::Portal)
        .expect("the install's portal file");
    let portal = auberean_dat::DatFile::open(&path).expect("opens");
    let record = portal.read(RECORD).expect("the library");
    let (changed, n) = plain(&record);
    assert_eq!(n, 175);
    assert_eq!(changed.len(), record.len());
    let different = changed.iter().zip(&record).filter(|(a, b)| a != b).count();
    assert_eq!(different, 175, "one byte of each operand");
    assert!(AdminLibrary::of(&record).is_ok());
}

/// The client finds a log type's names through its master map, in a record the install hasn't got:
/// the record the library brings is the one the map names, and the one it lacks.
/// Behaviour: server.admin.library-names-the-log-type
#[test]
fn the_log_types_names_are_the_record_the_client_looks_for_and_the_install_lacks() {
    use auberean_assets::tables::IdMap;
    use auberean_assets::Database;
    use yalain_world::admin_library::LOG_TYPE_RECORD;
    const LOG_TYPE: u32 = 0x4000_002C;
    const ENUMERATIONS: u32 = 0x2800_0002;
    let dir = auberean_dat::testing::require_target_install();
    let (_, path) = auberean_dat::locate::install_dats(&dir)
        .expect("list the install")
        .into_iter()
        .find(|(name, _)| *name == auberean_dat::locate::DatName::Portal)
        .expect("the install's portal file");
    let portal = auberean_dat::DatFile::open(&path).expect("opens");
    assert!(
        portal.read(LOG_TYPE_RECORD).is_err(),
        "the install lacks it"
    );
    let db = Database::new(vec![std::sync::Arc::new(portal)]);
    let table = db
        .get::<IdMap>(ENUMERATIONS)
        .expect("the table of enumerations");
    assert!(table
        .ids
        .iter()
        .chain(table.tool_ids.iter())
        .any(|(k, d)| *k == LOG_TYPE && d.0 == LOG_TYPE_RECORD));
}

/// Our client on an admin's account gets no library from a server that serves one: asked which data
/// it holds, it lists the library, and its characters come straight on, with nothing downloaded.
/// Behaviour: server.admin.library-not-sent-to-our-client
#[test]
fn our_client_on_an_admins_account_is_not_sent_the_library() {
    use auberean_dat::disk::download::event;
    use yalain_testkit::session::{client, Ours};
    use yalain_world::admin_library::{AdminLibrary, RECORD};
    let mut s = InProcess::new();
    ok(play::character_list(&mut s));
    console(&mut s, "set-accountadmin ours");
    let dir = auberean_dat::testing::require_target_install();
    let (_, path) = auberean_dat::locate::install_dats(&dir)
        .expect("list the install")
        .into_iter()
        .find(|(name, _)| *name == auberean_dat::locate::DatName::Portal)
        .expect("the install's portal file");
    let record = auberean_dat::DatFile::open(&path)
        .expect("opens")
        .read(RECORD)
        .expect("the library");
    s.h.server.admin_library = Some(AdminLibrary::of(&record).expect("made"));
    s.ours = Ours {
        addr: std::net::SocketAddr::from(([127, 0, 0, 1], 51_001)),
        client: client(
            yalain_testkit::port_addr(yalain_net::Port::LOGIN_EVEN),
            "ours",
        ),
    };
    ok(play::character_list(&mut s));
    let events = &s.client_ref().data_download().events;
    assert!(
        !events
            .iter()
            .any(|(e, _)| [event::PATCHING, event::PROGRESS].contains(e)),
        "nothing came: {events:?}"
    );
    assert!(s.h.server.storing_library.is_empty());
}

/// An effect typed on oneself takes effect in one's own client, and ending it too: its visual stamp
/// follows the player's, so the client neither drops it as old nor holds it for stamps that never
/// come.
/// Behaviour: server.admin.effect-typed-takes-effect
#[test]
fn an_effect_typed_on_oneself_reaches_the_admins_own_client() {
    let mut s = admin_session();
    for line in ["/dofx levelup", "/stopfx levelup"] {
        let before = s.client_ref().queue.stats();
        typed(&mut s, line);
        assert_eq!(s.client_ref().queue.stats(), before, "{line}");
    }
    let lines = shown(&s).join("\n");
    assert!(lines.contains("shown on"), "{lines}");
    assert!(lines.contains("stopped on"), "{lines}");
    let stops = s.client_ref().script_stops();
    assert!(stops.is_empty(), "{stops:#?}");
}
