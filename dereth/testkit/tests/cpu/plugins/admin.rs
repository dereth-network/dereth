//! The Admin sample: an administrator's chat commands as a window of forms. Each runs the sample
//! from `dereth/plugins/admin` in a scripted world, and reads what the client sent.

use dereth_client_model::chat::text_type;
use dereth_protocol::comms::CommunicationTextboxString;
use dereth_testkit::behaviours::note_asserted;
use dereth_testkit::plugins::{read_package, ui_event, PluginRun, ScriptedObject};

dereth_testkit::scenarios! {
    scenario_only_an_administrator_has_the_admin_window => only_an_administrator_has_the_admin_window ["plugins.samples.only-an-administrator-has-the-admin-window"],
    scenario_the_admin_windows_buttons_send_the_typed_line_and_what_is_hard_to_undo_takes_a_second_press => the_admin_windows_buttons_send_the_typed_line_and_what_is_hard_to_undo_takes_a_second_press ["plugins.samples.the-admin-windows-buttons-send-the-typed-line-and-what-is-hard-to-undo-takes-a-second-press"],
    scenario_the_admin_window_takes_players_and_places_only_from_the_servers_replies => the_admin_window_takes_players_and_places_only_from_the_servers_replies ["plugins.samples.the-admin-window-takes-players-and-places-only-from-the-servers-replies"],
}

const ID: &str = "network.dereth.admin";
/// The administrator flag, bool property `0x2C`.
const IS_ADMIN: u32 = 0x2C;
/// An object to select.
const GEM: u32 = 0x8000_0101;
const OTHER_GEM: u32 = 0x8000_0102;

fn admin_plugin() -> PluginRun {
    let mut run = PluginRun::new();
    run.load(read_package(
        &dereth_testkit::conformance::workspace_root().join("dereth/plugins/admin"),
    ));
    run.scripted_world(
        "Tester",
        vec![
            ScriptedObject {
                id: GEM,
                name: "Gem".into(),
                ..ScriptedObject::default()
            },
            ScriptedObject {
                id: OTHER_GEM,
                name: "Other Gem".into(),
                ..ScriptedObject::default()
            },
        ],
    );
    run
}

/// The run with the character an administrator, the window open by its key.
fn opened() -> PluginRun {
    let mut run = admin_plugin();
    run.set_player_bool(IS_ADMIN, true);
    run.advance(1.2);
    run.press_hotkey(ID, "key");
    run.advance(0.2);
    run
}

fn window_open(run: &mut PluginRun) -> Option<bool> {
    run.host()
        .windows()
        .into_iter()
        .find(|(p, w, _)| p == ID && w == "main")
        .map(|(_, _, win)| win.open)
}

/// The chat lines the client sent, in order.
fn talk(run: &PluginRun) -> Vec<String> {
    run.sent()
        .into_iter()
        .filter_map(|l| l.strip_prefix("talk ").map(str::to_owned))
        .collect()
}

fn system(run: &mut PluginRun, ty: u32, text: &str) {
    run.server_says(&[ui_event(&CommunicationTextboxString {
        text: text.into(),
        text_type: ty,
    })]);
}

/// For a character without the administrator flag there is no window, no key and nothing on the
/// plugin bar, and `/admin` says why; with the flag, the key opens the window; and when the flag
/// goes, the forms go with it.
fn only_an_administrator_has_the_admin_window() {
    let mut run = admin_plugin();
    run.advance(1.2);
    assert_eq!(window_open(&mut run), None, "no window: {}", run.ui_dump());
    assert!(
        run.host().hotkeys().iter().all(|h| h.plugin != ID),
        "no key"
    );
    assert!(
        run.host()
            .bar()
            .iter()
            .all(|b| b.id != ID || b.window.is_none()),
        "nothing to open from the plugin bar"
    );
    run.type_line("/admin");
    run.advance(0.2);
    assert!(
        run.printed()
            .iter()
            .any(|l| l.contains("Only a character with the administrator flag")),
        "{:?}",
        run.printed()
    );
    assert_eq!(window_open(&mut run), None);
    assert!(talk(&run).is_empty(), "nothing sent: {:?}", run.sent());

    run.set_player_bool(IS_ADMIN, true);
    run.advance(1.2);
    assert_eq!(window_open(&mut run), Some(false), "declared, closed");
    run.press_hotkey(ID, "key");
    run.advance(0.2);
    assert_eq!(window_open(&mut run), Some(true), "opened by its key");
    assert_eq!(talk(&run), ["@listplayers"], "who is on, as it opens");

    run.set_player_bool(IS_ADMIN, false);
    run.advance(1.2);
    assert_eq!(window_open(&mut run), Some(false), "closed with the flag");
    assert!(
        run.host().hotkeys().iter().all(|h| h.plugin != ID),
        "its key gone with the forms"
    );
    assert!(!run.ui_dump().contains("teleto"), "{}", run.ui_dump());
    note_asserted("plugins.samples.only-an-administrator-has-the-admin-window");
}

/// A button sends the line an administrator would type; one that is hard to undo sends it only
/// at a second press of the same command on the same selection; and a command on the selection
/// is sent after the selection's appraisal. Every command of its table, listed once, reaches the
/// server as it is typed: the client answers none of them itself.
fn the_admin_windows_buttons_send_the_typed_line_and_what_is_hard_to_undo_takes_a_second_press() {
    use dereth_client_model::cmd::{CommandInterp, CommandOutcome, TalkFocus};
    let table = std::fs::read_to_string(
        dereth_testkit::conformance::workspace_root().join("dereth/plugins/admin/commands.txt"),
    )
    .expect("the table");
    let mut interp = CommandInterp::new();
    let mut seen = std::collections::BTreeSet::new();
    for row in table.lines().filter(|l| !l.is_empty()) {
        let fields: Vec<&str> = row.split('\t').collect();
        assert_eq!(fields.len(), 5, "{row:?}");
        let verb = fields[0];
        assert!(
            !verb.contains(' '),
            "{verb}: the server splits a line on spaces"
        );
        assert!(
            seen.insert(verb.to_ascii_lowercase()),
            "{verb} is listed twice"
        );
        let line = format!("@{verb}");
        assert_eq!(
            interp.on_chat_command(&line, 1, TalkFocus::Say),
            CommandOutcome::ForwardVerbatim(line),
            "the client would answer it itself"
        );
    }

    let mut run = opened();
    run.set(ID, "main.name", "Bravo Player".into());
    run.click(ID, "main.teleto");
    run.advance(0.2);
    assert_eq!(
        talk(&run).last().map(String::as_str),
        Some("@teleto Bravo Player")
    );

    let before = talk(&run).len();
    run.click(ID, "main.world_close");
    run.advance(0.2);
    assert_eq!(talk(&run).len(), before, "the first press sends nothing");
    assert!(run.ui_dump().contains("Press Again"), "{}", run.ui_dump());
    run.click(ID, "main.world_close");
    run.advance(1.2);
    assert_eq!(talk(&run).last().map(String::as_str), Some("@world close"));

    // A dangerous command on the selection: armed with one gem selected, it is armed afresh, not
    // sent, when the second press finds another selected. (A plugin sends three lines in any three
    // seconds.)
    run.advance(3.0);
    let before = talk(&run).len();
    run.select(GEM);
    run.click(ID, "main.delete");
    run.select(OTHER_GEM);
    run.click(ID, "main.delete");
    run.advance(0.5);
    assert_eq!(
        talk(&run).len(),
        before,
        "nothing deleted: {:?}",
        run.sent()
    );
    run.click(ID, "main.delete");
    run.advance(1.0);
    let sent = run.sent();
    let appraised = sent
        .iter()
        .rposition(|l| l == &format!("appraise {OTHER_GEM:#x}"))
        .expect("the selection appraised");
    let deleted = sent
        .iter()
        .rposition(|l| l == "talk @delete")
        .unwrap_or_else(|| {
            panic!(
                "@delete sent: {sent:#?} {:?} {}",
                run.printed(),
                run.ui_dump()
            )
        });
    assert!(appraised < deleted, "appraised first: {sent:#?}");
    note_asserted(
        "plugins.samples.the-admin-windows-buttons-send-the-typed-line-and-what-is-hard-to-undo-takes-a-second-press",
    );
}

/// The list of players and the location box are filled from the server's own replies; a player's
/// emote or speech that reads like one fills neither.
fn the_admin_window_takes_players_and_places_only_from_the_servers_replies() {
    let mut run = opened();
    run.hear("emote", "Mallory", 0x5000_0099, "Impostor : 7");
    run.hear(
        "say",
        "Mallory",
        0x5000_0099,
        "Your location is: 0x01020304 [1 2 3]",
    );
    system(&mut run, text_type::SYSTEM, "Bravo Player : 12");
    system(&mut run, text_type::SYSTEM, "Total connected Players: 1");
    run.advance(0.2);
    let dump = run.ui_dump();
    assert!(dump.contains("Bravo Player"), "{dump}");
    assert!(!dump.contains("Impostor"), "{dump}");

    run.click(ID, "main.loc");
    run.hear(
        "say",
        "Mallory",
        0x5000_0099,
        "Your location is: 0x01020304 [1 2 3]",
    );
    run.advance(0.2);
    assert!(!run.ui_dump().contains("0x01020304"), "{}", run.ui_dump());
    system(
        &mut run,
        text_type::SYSTEM,
        "Your location is: 0x7F0401AD [12.3 -28.4 0.0] 1 0 0 0",
    );
    run.advance(0.2);
    assert!(
        run.ui_dump().contains("0x7F0401AD [12.3 -28.4 0.0]"),
        "{}",
        run.ui_dump()
    );
    note_asserted(
        "plugins.samples.the-admin-window-takes-players-and-places-only-from-the-servers-replies",
    );
}
