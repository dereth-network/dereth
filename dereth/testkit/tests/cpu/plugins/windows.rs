//! The samples' windows: everything a sample's command changes is also in its window, and each
//! control does what the command does. Each runs the sample from `dereth/plugins` in a scripted
//! world (or at a scripted character list), reads its window's tree as the headless client dumps
//! it, and works its controls.

use dereth_client_net::client_session::SessionEvent;
use dereth_plugin_api::Value;
use dereth_rules::weenie::{bitfield, item_type};
use dereth_testkit::behaviours::note_asserted;
use dereth_testkit::plugins::{
    read_package, ui_event, PluginRun, ScriptedDoor, ScriptedObject, ScriptedPregame,
    SCRIPTED_PLAYER,
};

use super::{samples, SharedFiles};

dereth_testkit::scenarios! {
    scenario_auto_logins_window_chooses_the_character_and_orders_the_lines_typed_after_login => auto_logins_window_chooses_the_character_and_orders_the_lines_typed_after_login ["plugins.samples.auto-logins-window-chooses-the-character-and-orders-the-lines-typed-after-login"],
    scenario_the_chat_filters_rules_hide_highlight_and_log_lines_and_its_window_edits_them => the_chat_filters_rules_hide_highlight_and_log_lines_and_its_window_edits_them ["plugins.samples.the-chat-filters-rules-hide-highlight-and-log-lines-and-its-window-edits-them"],
    scenario_the_chat_filter_takes_up_what_the_chat_logger_kept => the_chat_filter_takes_up_what_the_chat_logger_kept ["plugins.samples.the-chat-filter-takes-up-what-the-chat-logger-kept"],
    scenario_follows_window_lists_the_other_clients_to_follow_and_sends_a_line_to_the_followers => follows_window_lists_the_other_clients_to_follow_and_sends_a_line_to_the_followers ["plugins.samples.follows-window-lists-the-other-clients-to-follow-and-sends-a-line-to-the-followers"],
    scenario_id_notices_window_sets_the_value_and_the_spell_words_that_matter => id_notices_window_sets_the_value_and_the_spell_words_that_matter ["plugins.samples.id-notices-window-sets-the-value-and-the-spell-words-that-matter"],
    scenario_id_notices_watches_for_names_and_unknown_scrolls_and_alerts_as_set => id_notices_watches_for_names_and_unknown_scrolls_and_alerts_as_set ["plugins.samples.id-notices-watches-for-names-and-unknown-scrolls-and-alerts-as-set"],
    scenario_the_looters_window_picks_a_profile_shows_its_rules_and_counts_what_it_kept => the_looters_window_picks_a_profile_shows_its_rules_and_counts_what_it_kept ["plugins.samples.the-looters-window-picks-a-profile-shows-its-rules-and-counts-what-it-kept"],
    scenario_the_looter_waits_while_the_tank_runs => the_looter_waits_while_the_tank_runs ["plugins.samples.the-looter-waits-while-the-tank-runs"],
    scenario_the_navigator_finds_a_place_as_it_is_typed_and_goes_there => the_navigator_finds_a_place_as_it_is_typed_and_goes_there ["plugins.samples.the-navigator-finds-a-place-as-it-is-typed-and-goes-there"],
    scenario_the_trackers_window_sets_the_low_warnings_and_clears_the_arrow => the_trackers_window_sets_the_low_warnings_and_clears_the_arrow ["plugins.samples.the-trackers-window-sets-the-low-warnings-and-clears-the-arrow"],
    scenario_the_tanks_route_page_makes_changes_saves_and_loads_a_vtank_route => the_tanks_route_page_makes_changes_saves_and_loads_a_vtank_route ["plugins.samples.the-tanks-route-page-makes-changes-saves-and-loads-a-vtank-route"],
    scenario_the_tank_records_a_route_while_the_character_walks => the_tank_records_a_route_while_the_character_walks ["plugins.samples.the-tank-records-a-route-while-the-character-walks"],
    scenario_the_tanks_window_sets_every_setting_edits_the_monster_list_and_hunts => the_tanks_window_sets_every_setting_edits_the_monster_list_and_hunts ["plugins.samples.the-tanks-window-sets-every-setting-edits-the-monster-list-and-hunts"],
    scenario_the_tank_opens_a_closed_door_across_its_route_before_walking_it => the_tank_opens_a_closed_door_across_its_route_before_walking_it ["plugins.samples.the-tank-opens-a-closed-door-across-its-route-before-walking-it"],
    scenario_the_tank_tells_once_of_a_locked_door_across_its_route => the_tank_tells_once_of_a_locked_door_across_its_route ["plugins.samples.the-tank-tells-once-of-a-locked-door-across-its-route"],
    scenario_the_tanks_controls_run_vtanks_commands => the_tanks_controls_run_vtanks_commands ["plugins.samples.the-tanks-controls-run-vtanks-commands"],
}

/// The dump's line for node `id` of a window, or an empty line.
fn node_line(dump: &str, id: &str) -> String {
    let tag = format!("#{id} ");
    dump.lines()
        .find(|l| l.contains(&tag) || l.trim_end().ends_with(&format!("#{id}")))
        .unwrap_or_default()
        .to_owned()
}

/// Allow every kind of automation in the scripted world.
fn allow_all(run: &mut PluginRun) {
    run.host().set_world_policy(
        dereth_plugin_host::policy_doc::parse(
            "policy_version = 1\n[automation]\ndefault = \"allow\"\n",
        )
        .expect("a policy"),
    );
}

/// Select row `row` (from 1) of list `node` in a plugin's main window, and run a frame.
fn select(run: &mut PluginRun, plugin: &str, node: &str, row: i64) {
    run.host()
        .ui_input(
            plugin,
            "main",
            dereth_plugin_host::UiInput::Select {
                node: node.into(),
                rows: vec![row],
            },
        )
        .unwrap_or_else(|e| panic!("{plugin} {node}: {e}"));
    run.frame();
}

// -------------------------------------------------------------------------------------------------
// Auto-login

const AUTOLOGIN: &str = "network.dereth.autologin";

/// At the character list the window offers the account's characters; choosing one turns logging
/// in on, as `/autologin set` does; lines added in the window are typed after login in the order
/// the window puts them, and "Log in the next one" at the character list logs the character after
/// the chosen one in at once.
fn auto_logins_window_chooses_the_character_and_orders_the_lines_typed_after_login() {
    let mut run = PluginRun::new();
    run.load(read_package(&samples().join("autologin")));
    run.scripted_pregame(ScriptedPregame {
        characters: vec![(0x5000_0001, "Alpha".into()), (0x5000_0002, "Bravo".into())],
        still_in_world: Vec::new(),
        objects: Vec::new(),
    });
    run.advance(0.5);
    let dump = run.ui_dump();
    assert!(
        node_line(&dump, "character").contains(r#"options=["","Alpha","Bravo"]"#),
        "the character list in the choice:\n{dump}"
    );
    assert!(node_line(&dump, "on").contains("value=false"), "{dump}");
    run.set(AUTOLOGIN, "main.character", Value::from("Bravo"));
    let dump = run.ui_dump();
    assert!(
        node_line(&dump, "on").contains("value=true")
            && node_line(&dump, "status").contains("Logs Bravo in at the character list."),
        "choosing a character turns logging in on, as /autologin set does:\n{dump}"
    );
    // The command and the window are the same settings: switched off by command, the window shows
    // it; on again in the window.
    run.type_line("/autologin off");
    assert!(node_line(&run.ui_dump(), "on").contains("value=false"));
    run.set(AUTOLOGIN, "main.on", Value::Bool(true));
    for line in ["/em waves", "/em bows"] {
        run.set(AUTOLOGIN, "main.line", Value::from(line));
        run.click(AUTOLOGIN, "main.add");
    }
    // The second line moved up: bowing first.
    select(&mut run, AUTOLOGIN, "commands", 2);
    run.click(AUTOLOGIN, "main.up");
    let dump = run.ui_dump();
    assert!(
        node_line(&dump, "commands").contains(r#"rows=[[1,"/em bows"],[2,"/em waves"]]"#),
        "the lines in the order the window put them:\n{dump}"
    );
    // A line removed by command goes from the window too.
    run.type_line("/autologin cmd add /em cheers");
    run.type_line("/autologin cmd remove 3");
    assert!(!run.ui_dump().contains("/em cheers"), "{}", run.ui_dump());
    // "Log in the next one" at the character list: the one after Bravo, Alpha, now.
    run.click(AUTOLOGIN, "main.next");
    run.advance(4.0);
    assert_eq!(
        run.moves(),
        ["log_on Alpha"],
        "{:?}",
        run.console(AUTOLOGIN)
    );
    let sent = run.sent();
    let bows = sent.iter().position(|s| s == "emote bows");
    let waves = sent.iter().position(|s| s == "emote waves");
    assert!(
        bows.is_some() && waves.is_some() && bows < waves,
        "the lines typed after login, in order: {sent:?}"
    );
    note_asserted(
        "plugins.samples.auto-logins-window-chooses-the-character-and-orders-the-lines-typed-after-login",
    );
}

// -------------------------------------------------------------------------------------------------
// Chat Filter

const CHATFILTER: &str = "network.dereth.chatfilter";

/// A line of `text_type` the server sends as it is.
fn server_line(text: &str, text_type: u32) -> SessionEvent {
    ui_event(&dereth_protocol::comms::CommunicationTextboxString {
        text: text.into(),
        text_type,
    })
}

/// A tell from `who`.
fn tell(who: &str, text: &str) -> SessionEvent {
    ui_event(&dereth_protocol::comms::CommunicationHearDirectSpeech {
        message: text.into(),
        sender_name: who.into(),
        sender_id: dereth_primitives::ObjectId(0x5000_0099),
        target_id: dereth_primitives::ObjectId(SCRIPTED_PLAYER),
        text_type: 3,
        secret_flags: 0,
    })
}

/// The rules: a new character logs tells; a rule added in the window hides the lines holding its
/// words and is switched off and on there; a rule on a sender highlights that sender's lines
/// (hidden, and shown again in the rule's style); `/chatfilter add` is the same as the window's
/// Add; the log page lists what the rules kept.
fn the_chat_filters_rules_hide_highlight_and_log_lines_and_its_window_edits_them() {
    let mut run = PluginRun::new();
    run.load(read_package(&samples().join("chat-filter")));
    run.scripted_world("Tester", Vec::new());
    run.advance(0.5);
    let dump = run.ui_dump();
    assert!(
        node_line(&dump, "rules").contains(r#"rows=[["on","channel","tell","log"]]"#),
        "a new character's rule: tells logged:\n{dump}"
    );
    run.set(CHATFILTER, "main.match", Value::from("text"));
    run.set(CHATFILTER, "main.value", Value::from("golem"));
    run.set(CHATFILTER, "main.action", Value::from("hide"));
    run.click(CHATFILTER, "main.add");
    run.server_says(&[server_line("A golem lumbers into view.", 0)]);
    assert_eq!(run.hidden(), ["A golem lumbers into view."]);
    assert!(
        node_line(&run.ui_dump(), "hidden").contains("Hidden this session: 1"),
        "{}",
        run.ui_dump()
    );
    // The rule switched off: the next line shows.
    select(&mut run, CHATFILTER, "rules", 2);
    run.click(CHATFILTER, "main.switch");
    assert!(
        node_line(&run.ui_dump(), "rules").contains(r#"["off","text","golem","hide"]"#),
        "{}",
        run.ui_dump()
    );
    run.server_says(&[server_line("Another golem.", 0)]);
    assert_eq!(run.hidden().len(), 1, "off, it hides nothing more");
    // The command adds the same rule the window does.
    run.type_line("/chatfilter add spam");
    assert!(
        node_line(&run.ui_dump(), "rules").contains(r#"["on","text","spam","hide"]"#),
        "{}",
        run.ui_dump()
    );
    // A sender's lines highlighted: hidden, and shown again in the rule's style.
    run.set(CHATFILTER, "main.match", Value::from("sender"));
    run.set(CHATFILTER, "main.value", Value::from("Bob"));
    run.set(CHATFILTER, "main.action", Value::from("highlight"));
    run.set(CHATFILTER, "main.style", Value::from("advancement"));
    run.click(CHATFILTER, "main.add");
    run.server_says(&[server_line("Bob says, \"hello there\"", 2)]);
    assert!(run.hidden().iter().any(|l| l.contains("hello there")));
    assert!(
        run.printed()
            .iter()
            .any(|l| l == "Bob says, \"hello there\""),
        "shown again: {:?}",
        run.printed()
    );
    // A tell, logged by the first rule, on the log page.
    run.server_says(&[tell("Carol", "meet at the lifestone")]);
    let dump = run.ui_dump();
    assert!(
        node_line(&dump, "log").contains("meet at the lifestone")
            && node_line(&dump, "logged").contains("1 lines kept"),
        "the log page lists what the rules kept:\n{dump}"
    );
    note_asserted(
        "plugins.samples.the-chat-filters-rules-hide-highlight-and-log-lines-and-its-window-edits-them",
    );
}

/// What the Chat Logger kept is the Chat Filter's: the logger's filter words hide lines as rules
/// of their own, its log is the filter's, and since the logger kept every line, a rule keeping
/// every line comes with them.
fn the_chat_filter_takes_up_what_the_chat_logger_kept() {
    let legacy = dereth_plugin_host::Package {
        origin: "legacy".into(),
        files: [
            (
                "plugin.toml".to_owned(),
                b"id = \"network.dereth.chatlogger\"\nname = \"Chat Logger\"\nversion = \"0.1.0\"\napi = \"0.1\"\n"
                    .to_vec(),
            ),
            (
                "main.lua".to_owned(),
                br#"on("session.login", function()
    store.character.filters = { "golem" }
    store.character.log = { { kind = "speech", text = "an old line" } }
    store.character.filtering = true
end)"#
                    .to_vec(),
            ),
        ]
        .into(),
    };
    let mut run = PluginRun::new();
    run.load(legacy);
    run.scripted_world("Tester", Vec::new());
    run.advance(0.5);
    run.unload("network.dereth.chatlogger");
    run.load(read_package(&samples().join("chat-filter")));
    run.advance(0.5);
    let dump = run.ui_dump();
    let rules = node_line(&dump, "rules");
    assert!(
        rules.contains(r#"["on","text","golem","hide"]"#)
            && rules.contains(r#"["on","text","(any line)","log"]"#),
        "the logger's words as rules, and every line kept:\n{dump}"
    );
    assert!(
        node_line(&dump, "log").contains("an old line"),
        "the logger's log:\n{dump}"
    );
    run.server_says(&[server_line("A golem lumbers into view.", 0)]);
    assert_eq!(run.hidden(), ["A golem lumbers into view."]);
    note_asserted("plugins.samples.the-chat-filter-takes-up-what-the-chat-logger-kept");
}

// -------------------------------------------------------------------------------------------------
// Follow

const FOLLOW: &str = "network.dereth.follow";

/// Two clients on one machine: each lists the other in its window; the leader leads by its
/// button; the follower picks the leader in its window's choice and follows it as it moves; a
/// line sent from the leader's window is typed by the follower; Stop stops.
fn follows_window_lists_the_other_clients_to_follow_and_sends_a_line_to_the_followers() {
    use dereth_plugin_host::LocalBus;
    const LEADER: u32 = 0x5000_0002;
    let bus = LocalBus::new();
    let mut leader = PluginRun::new()
        .with_peers(Box::new(bus.endpoint()))
        .with_player_id(LEADER);
    let mut follower = PluginRun::new().with_peers(Box::new(bus.endpoint()));
    leader.load(read_package(&samples().join("follow")));
    follower.load(read_package(&samples().join("follow")));
    leader.scripted_world("Leader", Vec::new());
    follower.scripted_world(
        "Follower",
        vec![ScriptedObject {
            id: LEADER,
            name: "Leader".into(),
            obj_type: item_type::CREATURE,
            place: Some((42.25, 33.8)),
            ..ScriptedObject::default()
        }],
    );
    for _ in 0..8 {
        leader.advance(0.25);
        follower.advance(0.25);
    }
    assert!(
        node_line(&follower.ui_dump(), "leader").contains(r#"options=["","Leader"]"#),
        "the other client listed:\n{}",
        follower.ui_dump()
    );
    leader.click(FOLLOW, "main.lead");
    assert!(
        node_line(&leader.ui_dump(), "state").contains("Leading"),
        "{}",
        leader.ui_dump()
    );
    follower.set(FOLLOW, "main.leader", Value::from("Leader"));
    follower.click(FOLLOW, "main.follow");
    for step in 0..40 {
        leader.advance(0.25);
        follower.advance(0.25);
        if step == 20 {
            follower.move_object(LEADER, 42.25, 33.85);
        }
    }
    let (ns, ew, _) = follower.player_place().expect("placed");
    let d = (((ns - 42.25) * 240.0).powi(2) + ((ew - 33.85) * 240.0).powi(2)).sqrt();
    assert!(d <= 4.5, "the follower closed on the leader: {d:.1} m");
    assert!(
        node_line(&follower.ui_dump(), "state").contains("Following Leader"),
        "{}",
        follower.ui_dump()
    );
    leader.set(FOLLOW, "main.line", Value::from("/em waves"));
    leader.click(FOLLOW, "main.send");
    for _ in 0..4 {
        leader.advance(0.25);
        follower.advance(0.25);
    }
    assert!(
        follower.sent().iter().any(|s| s == "emote waves"),
        "the follower typed the leader's line: {:?}",
        follower.sent()
    );
    follower.click(FOLLOW, "main.stop");
    assert!(
        node_line(&follower.ui_dump(), "state").contains("Neither leading nor following"),
        "{}",
        follower.ui_dump()
    );
    note_asserted(
        "plugins.samples.follows-window-lists-the-other-clients-to-follow-and-sends-a-line-to-the-followers",
    );
}

// -------------------------------------------------------------------------------------------------
// ID Notices

const IDN: &str = "network.dereth.idnotices";

/// The settings window sets the value an item must be worth for a notice and the spell words
/// that make one, as `/idn` does: an item worth more than the new value is noticed, one worth
/// less is not, and a word removed in the window no longer counts.
fn id_notices_window_sets_the_value_and_the_spell_words_that_matter() {
    const CHEST: u32 = 0x8000_0300;
    let mut run = PluginRun::new();
    run.load(read_package(&samples().join("id-notices")));
    run.scripted_world(
        "Tester",
        vec![
            ScriptedObject {
                id: CHEST,
                name: "Chest".into(),
                obj_type: item_type::CONTAINER,
                bitfield: bitfield::OPENABLE | bitfield::STUCK,
                ..ScriptedObject::default()
            },
            ScriptedObject {
                id: 0x8000_0301,
                name: "Gold Ring".into(),
                obj_type: item_type::JEWELRY,
                container: Some(CHEST),
                value: 9500,
                ints: vec![(19, 9500)],
                ..ScriptedObject::default()
            },
            ScriptedObject {
                id: 0x8000_0302,
                name: "Silver Ring".into(),
                obj_type: item_type::JEWELRY,
                container: Some(CHEST),
                value: 8000,
                ints: vec![(19, 8000)],
                ..ScriptedObject::default()
            },
        ],
    );
    run.advance(0.5);
    run.type_line("/idn");
    run.set(IDN, "main.value", Value::Int(9000));
    run.set(IDN, "main.word", Value::from("Incantation"));
    run.click(IDN, "main.add");
    let dump = run.ui_dump();
    assert!(
        node_line(&dump, "words").contains(r#"["Incantation"]"#)
            && node_line(&dump, "summary").contains("worth 9000 or more"),
        "{dump}"
    );
    // The command reads back what the window set.
    let from = run.printed().len();
    run.type_line("/idn");
    assert!(
        run.printed()[from..]
            .iter()
            .any(|l| l == "ID notices: value 9000, words Legendary, Epic, Major, Incantation"),
        "{:?}",
        run.printed()
    );
    // Removed in the window: gone.
    select(&mut run, IDN, "words", 1);
    run.click(IDN, "main.remove");
    assert!(
        !node_line(&run.ui_dump(), "words").contains("Legendary"),
        "{}",
        run.ui_dump()
    );
    run.player_uses(CHEST);
    run.advance(3.0);
    let printed = run.printed();
    assert!(
        printed.iter().any(|l| l == "[ID] Gold Ring: worth 9500")
            && !printed.iter().any(|l| l.contains("Silver Ring")),
        "worth more than the window's value, noticed; less, not: {printed:?}"
    );
    note_asserted(
        "plugins.samples.id-notices-window-sets-the-value-and-the-spell-words-that-matter",
    );
}

/// Alinco's lists and alerts: in a chest, the scroll of a spell the character does not know (at
/// the level set or above) is noticed and one of a spell it knows is not; an item of a name watched for in
/// quotes is noticed and one that only holds it is not; a creature watched for (by part of its
/// name, added in the window) is noticed as it comes into view, with a toast as its alert says; a
/// portal, its alert off, is not.
fn id_notices_watches_for_names_and_unknown_scrolls_and_alerts_as_set() {
    const CHEST: u32 = 0x8000_0310;
    let mut run = PluginRun::new()
        .with_spells(&[(80, "Flame Bolt I", 1)], false)
        .with_spells(&[(81, "Acid Stream I", 1)], true);
    run.load(read_package(&samples().join("id-notices")));
    let in_chest = |id: u32, name: &str, obj_type: u32, spell: Option<u16>| ScriptedObject {
        id,
        name: name.into(),
        obj_type,
        container: Some(CHEST),
        spell_id: spell,
        ..ScriptedObject::default()
    };
    run.scripted_world(
        "Tester",
        vec![
            ScriptedObject {
                id: CHEST,
                name: "Chest".into(),
                obj_type: item_type::CONTAINER,
                bitfield: bitfield::OPENABLE | bitfield::STUCK,
                ..ScriptedObject::default()
            },
            in_chest(
                0x8000_0311,
                "Scroll of Flame Bolt I",
                item_type::WRITABLE,
                Some(80),
            ),
            in_chest(
                0x8000_0312,
                "Scroll of Acid Stream I",
                item_type::WRITABLE,
                Some(81),
            ),
            in_chest(0x8000_0313, "Pyreal Mote", item_type::MISC, None),
            in_chest(0x8000_0314, "Pyreal Mote Pouch", item_type::MISC, None),
        ],
    );
    run.advance(0.5);
    for line in [
        "/idn scroll 1",
        "/idn watch \"Pyreal Mote\"",
        "/idn alert portal off",
    ] {
        run.type_line(line);
    }
    // A name added in the window: any name holding it.
    run.type_line("/idn");
    run.set(IDN, "main.watchname", Value::from("Olthoi"));
    run.click(IDN, "main.watchadd");
    assert!(
        node_line(&run.ui_dump(), "watch").contains(r#"["Olthoi"]"#),
        "{}",
        run.ui_dump()
    );
    run.player_uses(CHEST);
    run.advance(3.0);
    let printed = run.printed();
    assert!(
        printed
            .iter()
            .any(|l| l
                == "[ID] Scroll of Flame Bolt I: Flame Bolt I, a level 1 spell you don't know"),
        "{printed:#?}"
    );
    assert!(
        printed.iter().any(|l| l == "[ID] Pyreal Mote: watched for"),
        "{printed:#?}"
    );
    assert!(
        !printed
            .iter()
            .any(|l| l.contains("Acid Stream") || l.contains("Pouch")),
        "a spell known, a name that only holds the one watched for: {printed:#?}"
    );
    let near = |id: u32, name: &str, obj_type: u32, bits: u32| ScriptedObject {
        id,
        name: name.into(),
        obj_type,
        bitfield: bits,
        place: Some((
            dereth_testkit::plugins::START_NS,
            dereth_testkit::plugins::START_EW + 0.05,
        )),
        ..ScriptedObject::default()
    };
    run.spawn(vec![
        near(
            0x8000_0320,
            "Olthoi Soldier",
            item_type::CREATURE,
            bitfield::ATTACKABLE,
        ),
        near(
            0x8000_0321,
            "Drudge Skulker",
            item_type::CREATURE,
            bitfield::ATTACKABLE,
        ),
        near(0x8000_0322, "Portal to Holtburg", item_type::PORTAL, 0),
    ]);
    run.advance(1.0);
    let printed = run.printed();
    assert!(
        printed
            .iter()
            .any(|l| l == "[ID] Watched for: Olthoi Soldier is here"),
        "{printed:#?}"
    );
    assert!(
        !printed
            .iter()
            .any(|l| l.contains("Drudge Skulker") || l.contains("Portal")),
        "{printed:#?}"
    );
    // The watched creature's alert is a toast as well.
    let toasts = run
        .host()
        .window(dereth_plugin_host::HOST_ID, dereth_plugin_host::TOASTS)
        .map(|w| format!("{w:?}"))
        .unwrap_or_default();
    assert!(toasts.contains("Olthoi Soldier is here"), "{toasts}");
    note_asserted(
        "plugins.samples.id-notices-watches-for-names-and-unknown-scrolls-and-alerts-as-set",
    );
}

// -------------------------------------------------------------------------------------------------
// Looter

const LOOTER: &str = "network.dereth.looter";

/// A corpse holding pyreals, a gem and a dagger, a short walk from the start.
fn corpse_with_loot(corpse: u32) -> Vec<ScriptedObject> {
    let item = |id: u32, name: &str, obj_type: u32, value: u32| ScriptedObject {
        id,
        name: name.into(),
        obj_type,
        container: Some(corpse),
        value,
        ..ScriptedObject::default()
    };
    vec![
        ScriptedObject {
            id: corpse,
            name: "Corpse of a Drudge".into(),
            obj_type: item_type::CONTAINER,
            bitfield: bitfield::CORPSE | bitfield::OPENABLE | bitfield::STUCK,
            ..ScriptedObject::default()
        },
        item(corpse + 1, "Pyreal", item_type::MONEY, 100),
        item(corpse + 2, "Opal", item_type::GEM, 6000),
        item(corpse + 3, "Rusty Dagger", item_type::MELEE_WEAPON, 5),
    ]
}

/// The profile choice offers the profiles in the looter's data folder beside the one it comes
/// with; the rules page shows the chosen profile's rules in plain words; choosing another loots
/// by it, as `/loot profile` does; what was kept is counted on the Kept page.
fn the_looters_window_picks_a_profile_shows_its_rules_and_counts_what_it_kept() {
    let files = SharedFiles::default();
    {
        use dereth_plugin_host::PluginFiles;
        // A profile keeping gems only.
        files
            .clone()
            .write(
                LOOTER,
                "gems.utl",
                b"UTL\r\n1\r\n1\r\nGems only\r\n\r\n0;1;7\r\n3\r\n11\r\n",
            )
            .expect("written");
    }
    let mut run = PluginRun::new().with_files(Box::new(files));
    run.load(read_package(&samples().join("looter")));
    run.scripted_world("Tester", corpse_with_loot(0x8000_0100));
    run.advance(0.5);
    let dump = run.ui_dump();
    assert!(
        node_line(&dump, "profile").contains(r#"options=["rules.utl","gems.utl"]"#),
        "{dump}"
    );
    assert!(
        node_line(&dump, "rules").contains(r#"["Valuables","keep","its value at least 2500"]"#)
            && node_line(&dump, "rules").contains(r#"["Gems","keep","its class is gem"]"#),
        "the profile's rules in plain words:\n{dump}"
    );
    run.set(LOOTER, "main.profile", Value::from("gems.utl"));
    assert!(
        node_line(&run.ui_dump(), "rules").contains(r#"[["Gems only","keep","its class is gem"]]"#),
        "{}",
        run.ui_dump()
    );
    run.select(0x8000_0100);
    run.click(LOOTER, "main.loot");
    run.advance(4.0);
    assert_eq!(
        [0x8000_0101, 0x8000_0102, 0x8000_0103].map(|i| run.container_of(i)),
        [Some(0x8000_0100), Some(SCRIPTED_PLAYER), Some(0x8000_0100)],
        "only the gem, by the profile chosen in the window"
    );
    assert!(
        node_line(&run.ui_dump(), "kept").contains(r#"[["Opal",1]]"#),
        "{}",
        run.ui_dump()
    );
    // The command and the window are one: a profile named by command shows as chosen.
    run.type_line("/loot profile rules.utl");
    assert!(node_line(&run.ui_dump(), "profile").contains(r#"value="rules.utl""#));
    note_asserted(
        "plugins.samples.the-looters-window-picks-a-profile-shows-its-rules-and-counts-what-it-kept",
    );
}

/// With Auto ticked, a corpse the player opens is looted; while the Tank runs (its `running`
/// service says so), a corpse opened is left to the Tank and the window says why, though a click
/// on Loot still loots it; once the Tank stops, Auto loots again.
fn the_looter_waits_while_the_tank_runs() {
    let mut run = PluginRun::new();
    allow_all(&mut run);
    run.load(read_package(&samples().join("looter")));
    run.load(read_package(&samples().join("tank")));
    let mut objects = corpse_with_loot(0x8000_0100);
    objects.extend(corpse_with_loot(0x8000_0200));
    run.scripted_world("Tester", objects);
    run.advance(0.5);
    run.set(LOOTER, "main.auto", Value::Bool(true));
    run.type_line("/tank start");
    run.advance(3.0);
    run.player_uses(0x8000_0100);
    run.advance(3.0);
    assert_eq!(
        run.container_of(0x8000_0102),
        Some(0x8000_0100),
        "the Tank runs: Auto leaves the corpse to it (console {:?})",
        run.console(LOOTER)
    );
    assert!(
        node_line(&run.ui_dump(), "status").contains("Auto waits: the Tank is running"),
        "{}",
        run.ui_dump()
    );
    // A click still loots the corpse open.
    run.click(LOOTER, "main.loot");
    run.advance(4.0);
    assert_eq!(run.container_of(0x8000_0102), Some(SCRIPTED_PLAYER));
    run.type_line("/tank stop");
    run.advance(3.0);
    run.player_uses(0x8000_0200);
    run.advance(4.0);
    assert_eq!(
        run.container_of(0x8000_0202),
        Some(SCRIPTED_PLAYER),
        "the Tank stopped: Auto loots again (console {:?})",
        run.console(LOOTER)
    );
    note_asserted("plugins.samples.the-looter-waits-while-the-tank-runs");
}

// -------------------------------------------------------------------------------------------------
// Navigator

const NAV: &str = "network.dereth.navigator";

/// Typing in "Where to?" lists the places as typed (by name, by type, coordinates); a result
/// picked shows where it is; Point puts the arrow on it and Go travels there, the status line
/// telling the journey; saved places and spots clicked on the map have tabs of their own.
fn the_navigator_finds_a_place_as_it_is_typed_and_goes_there() {
    use dereth_client_contract::panels::map::{MapButton, MapClick};
    let mut run = PluginRun::new();
    run.load(read_package(&samples().join("navigator")));
    run.scripted_world("Tester", Vec::new());
    run.advance(0.5);
    run.type_line("/nav");
    run.set(NAV, "main.search", Value::from("holt"));
    let dump = run.ui_dump();
    assert!(
        node_line(&dump, "results").contains(r#"["Holtburg","town","42.2N 33.8E","0 m"]"#),
        "found as typed:\n{dump}"
    );
    run.set(NAV, "main.search", Value::from("42.2N 33.9E"));
    let dump = run.ui_dump();
    assert!(
        node_line(&dump, "results")
            .contains(r#"[["42.2N 33.9E","coordinates","42.2N 33.9E","24 m"]]"#),
        "coordinates typed:\n{dump}"
    );
    select(&mut run, NAV, "results", 1);
    assert!(
        node_line(&run.ui_dump(), "to").contains("To: 42.2N 33.9E (42.2N 33.9E), 24 m E"),
        "{}",
        run.ui_dump()
    );
    assert!(
        !run.overlays().contains("arrow network.dereth.navigator"),
        "picked, not yet pointed at:\n{}",
        run.overlays()
    );
    run.click(NAV, "main.go");
    assert!(run.overlays().contains("arrow network.dereth.navigator"));
    run.advance(10.0);
    let (ns, ew, _) = run.player_place().expect("placed");
    assert!(
        (ns - 42.2).abs() < 0.02 && (ew - 33.9).abs() < 0.02,
        "went there: {ns} {ew}"
    );
    assert!(
        node_line(&run.ui_dump(), "status").contains("Arrived at 42.2N 33.9E."),
        "{}",
        run.ui_dump()
    );
    // Saved places: kept by the window's Save here, and listed on their page.
    run.set(NAV, "main.save_name", Value::from("Camp"));
    run.click(NAV, "main.save");
    assert!(
        node_line(&run.ui_dump(), "saved").contains("Camp"),
        "{}",
        run.ui_dump()
    );
    // A spot clicked on the game's map: pointed at, and kept on its page.
    run.map_click(&MapClick {
        ns: 42.3,
        ew: 33.8,
        button: MapButton::Left,
        ctrl: false,
        shift: false,
        alt: false,
    });
    assert!(
        node_line(&run.ui_dump(), "clicks").contains("42.3N 33.8E"),
        "{}",
        run.ui_dump()
    );
    // Stop, by the window, as /nav stop.
    run.click(NAV, "main.stop");
    assert!(node_line(&run.ui_dump(), "status").contains("Stopped."));
    note_asserted("plugins.samples.the-navigator-finds-a-place-as-it-is-typed-and-goes-there");
}

// -------------------------------------------------------------------------------------------------
// Trackers

const TRACKERS: &str = "network.dereth.trackers";

/// The Items page sets a low warning for an item by name and count, as `/track low` does, and
/// stops it; the Corpses page clears the arrow, as `/track corpse clear` does.
fn the_trackers_window_sets_the_low_warnings_and_clears_the_arrow() {
    let mut run = PluginRun::new();
    run.load(read_package(&samples().join("trackers")));
    run.scripted_world("Tester", Vec::new());
    run.advance(0.5);
    run.set(TRACKERS, "main.low_name", Value::from("Mana Stone"));
    run.set(TRACKERS, "main.low_n", Value::Int(3));
    run.click(TRACKERS, "main.low_set");
    assert!(
        node_line(&run.ui_dump(), "i_low").contains("Warns: Mana Stone under 3"),
        "{}",
        run.ui_dump()
    );
    run.type_line("/track low Healing Kit 5");
    assert!(
        node_line(&run.ui_dump(), "i_low").contains("Healing Kit under 5, Mana Stone under 3"),
        "{}",
        run.ui_dump()
    );
    run.click(TRACKERS, "main.low_off");
    assert!(
        !node_line(&run.ui_dump(), "i_low").contains("Mana Stone"),
        "{}",
        run.ui_dump()
    );
    // The character's own corpse seen, and the arrow put on it by the page's button.
    run.die();
    run.spawn(vec![ScriptedObject {
        id: 0x8000_0401,
        name: "Corpse of Tester".into(),
        obj_type: item_type::CONTAINER,
        bitfield: bitfield::CORPSE | bitfield::OPENABLE,
        place: Some((42.25, 33.8)),
        ..ScriptedObject::default()
    }]);
    run.advance(1.5);
    run.click(TRACKERS, "main.point");
    assert!(
        run.overlays().contains("arrow network.dereth.trackers"),
        "{}",
        run.overlays()
    );
    run.click(TRACKERS, "main.clear_arrow");
    assert!(
        !run.overlays().contains("arrow network.dereth.trackers"),
        "{}",
        run.overlays()
    );
    note_asserted("plugins.samples.the-trackers-window-sets-the-low-warnings-and-clears-the-arrow");
}

// -------------------------------------------------------------------------------------------------
// Tank

const TANK: &str = "network.dereth.tank";

/// A Tank in a scripted world that allows its automation, its data folder shared with the test.
fn tank(objects: Vec<ScriptedObject>) -> (PluginRun, SharedFiles) {
    let files = SharedFiles::default();
    let mut run = PluginRun::new().with_files(Box::new(files.clone()));
    allow_all(&mut run);
    run.load(read_package(&samples().join("tank")));
    run.scripted_world("Tester", objects);
    run.advance(0.5);
    (run, files)
}

/// The Route page makes a route as VTank's Route tab does: a new one of its kind, a point, a jump
/// at the heading faced with its charge, walk and sidestep, a pause, a chat line, a recall and a
/// checkpoint; it moves and removes waypoints, runs the route reversed (its points in the order
/// made), saves it as a VTank route that reads back, and loads one from the routes it lists.
fn the_tanks_route_page_makes_changes_saves_and_loads_a_vtank_route() {
    use dereth_plugin_host::PluginFiles;
    let (mut run, files) = tank(Vec::new());
    run.set(TANK, "main.r_kind", Value::from("linear"));
    run.click(TANK, "main.r_new");
    run.click(TANK, "main.r_add");
    run.place_player(42.2, 33.82);
    run.set(TANK, "main.r_charge", Value::Int(300));
    run.set(TANK, "main.r_walk", Value::Bool(true));
    run.set(TANK, "main.r_way", Value::from("strafeleft"));
    run.click(TANK, "main.r_jump");
    run.set(TANK, "main.r_pause", Value::Int(1500));
    run.click(TANK, "main.r_add_pause");
    run.set(TANK, "main.r_chat", Value::from("/em salutes"));
    run.click(TANK, "main.r_add_chat");
    run.set(TANK, "main.r_recall", Value::Int(1635));
    run.click(TANK, "main.r_add_recall");
    run.click(TANK, "main.r_checkpoint");
    let dump = run.ui_dump();
    let points = node_line(&dump, "r_points");
    assert!(
        points.contains(r#"[1,"point","42.20N 33.80E"]"#)
            && points.contains(
                r#"[2,"jump","heading 0, 300 ms, walking, strafe_left at 42.20N 33.82E"]"#
            )
            && points.contains(r#"[3,"pause","1500 ms"]"#)
            && points.contains(r#"[4,"chat","/em salutes"]"#)
            && points.contains(r#"[5,"recall","spell 1635"]"#)
            && points.contains(r#"[6,"checkpoint","42.20N 33.82E"]"#),
        "the waypoints made in the window:\n{points}"
    );
    assert!(
        node_line(&dump, "r_title").contains("linear, 6 waypoints"),
        "{dump}"
    );
    // The pause moved up, then the chat line removed.
    select(&mut run, TANK, "r_points", 3);
    run.click(TANK, "main.r_up");
    select(&mut run, TANK, "r_points", 4);
    run.click(TANK, "main.r_remove");
    let points = node_line(&run.ui_dump(), "r_points");
    assert!(
        points.contains(r#"[2,"pause","1500 ms"]"#)
            && points.contains(r#"[3,"jump""#)
            && !points.contains("salutes"),
        "{points}"
    );
    // Reversed, the route runs the other way round; its points keep their order.
    run.click(TANK, "main.r_reverse");
    let dump = run.ui_dump();
    assert!(
        node_line(&dump, "r_points").contains(r#"[1,"point""#)
            && node_line(&dump, "r_title").contains("linear (run reversed), 5 waypoints"),
        "{dump}"
    );
    run.set(TANK, "main.r_save_name", Value::from("made"));
    run.click(TANK, "main.r_save");
    let text = files
        .clone()
        .read(TANK, "routes/made.nav")
        .expect("saved as a VTank route");
    let route =
        dereth_plugin_host::navfile::parse(&String::from_utf8_lossy(&text)).expect("it reads back");
    let kinds: Vec<String> = route
        .get("points")
        .as_list()
        .expect("points")
        .iter()
        .map(|p| p.get("kind").as_str().unwrap_or("?").to_owned())
        .collect();
    assert_eq!(route.get("kind").as_str(), Some("linear"));
    assert_eq!(kinds, ["point", "pause", "jump", "recall", "checkpoint"]);
    // Load: the saved route among the routes listed, and the route the Tank has.
    let dump = run.ui_dump();
    assert!(
        node_line(&dump, "r_load").contains(r#""made.nav""#)
            && node_line(&dump, "r_load").contains(r#""holtburg-loop.nav""#),
        "the routes listed, the player's and the Tank's own:\n{dump}"
    );
    run.set(TANK, "main.r_load", Value::from("holtburg-loop.nav"));
    run.click(TANK, "main.r_load_go");
    let dump = run.ui_dump();
    assert!(
        node_line(&dump, "r_title").contains("Route holtburg-loop.nav"),
        "{dump}"
    );
    assert!(
        node_line(&dump, "s:Route").contains(r#"value="holtburg-loop.nav""#),
        "the Nav page's route is the one loaded:\n{dump}"
    );
    note_asserted(
        "plugins.samples.the-tanks-route-page-makes-changes-saves-and-loads-a-vtank-route",
    );
}

/// Recording while walking adds a point each time the character has gone the distance the
/// window sets from the last point it added, and stops when switched off.
fn the_tank_records_a_route_while_the_character_walks() {
    let (mut run, _) = tank(Vec::new());
    run.click(TANK, "main.r_new");
    run.set(TANK, "main.s:RouteRecordMetres", Value::Int(10));
    run.set(TANK, "main.r_record", Value::Bool(true));
    run.advance(0.5);
    // 42.2N 33.8E, then about 6, 12 and 24 metres north of it.
    for ns in [42.225, 42.25, 42.30] {
        run.place_player(ns, 33.8);
        run.advance(0.5);
    }
    let points = node_line(&run.ui_dump(), "r_points");
    assert!(
        points.contains(r#"[1,"point","42.20N 33.80E"]"#)
            && points.contains(r#"[2,"point","42.25N 33.80E"]"#)
            && points.contains(r#"[3,"point","42.30N 33.80E"]"#)
            && !points.contains("42.23N"),
        "a point every 10 m or more: {points}"
    );
    run.set(TANK, "main.r_record", Value::Bool(false));
    run.place_player(42.4, 33.8);
    run.advance(0.5);
    assert!(
        !node_line(&run.ui_dump(), "r_points").contains("42.40N"),
        "stopped"
    );
    note_asserted("plugins.samples.the-tank-records-a-route-while-the-character-walks");
}

/// The Settings page lists every setting and sets one picked, as `/tank set` does (and back to its
/// default); the Monsters page saves and removes a monster row, as `/tank monster` does; the Run
/// page hunts at the place typed, as `/tank hunt` does.
fn the_tanks_window_sets_every_setting_edits_the_monster_list_and_hunts() {
    let (mut run, _) = tank(Vec::new());
    let dump = run.ui_dump();
    let all = node_line(&dump, "all");
    assert!(
        all.contains(r#"["TiredAttackPower","10"]"#) && all.contains(r#"["HealSpells",""]"#),
        "every setting, those of no page of their own too:\n{all}"
    );
    let names: Vec<String> = all
        .split_once("rows=")
        .map_or("", |(_, rows)| rows)
        .split("[\"")
        .skip(1)
        .filter_map(|s| s.split('"').next().map(str::to_owned))
        .collect();
    let row = names
        .iter()
        .position(|n| n == "TiredAttackPower")
        .expect("listed")
        + 1;
    select(&mut run, TANK, "all", i64::try_from(row).expect("small"));
    run.set(TANK, "main.all_value", Value::from("25"));
    run.click(TANK, "main.all_set");
    let from = run.printed().len();
    run.type_line("/tank settings");
    run.advance(0.5);
    assert!(
        run.printed()[from..]
            .iter()
            .any(|l| l == "[Tank] TiredAttackPower = 25"),
        "set in the window, as /tank set sets it: {:?} {:?}",
        &run.printed()[from..],
        run.console(TANK)
    );
    run.type_line("/tank set TiredAttackPower 40");
    assert!(
        node_line(&run.ui_dump(), "all").contains(r#"["TiredAttackPower","40"]"#),
        "set by command, shown in the window"
    );
    select(&mut run, TANK, "all", i64::try_from(row).expect("small"));
    run.click(TANK, "main.all_default");
    assert!(node_line(&run.ui_dump(), "all").contains(r#"["TiredAttackPower","10"]"#));
    // A monster row saved, then removed.
    run.set(TANK, "main.monster_name", Value::from("Drudge Skulker"));
    run.set(TANK, "main.monster_priority", Value::Int(3));
    run.set(TANK, "main.monster_damage", Value::from("fire"));
    run.set(TANK, "main.monster_debuff:imperil", Value::Bool(true));
    run.click(TANK, "main.monster_save");
    let from = run.printed().len();
    run.type_line("/tank monsters");
    assert!(
        run.printed()[from..]
            .iter()
            .any(|l| l == "[Tank] Drudge Skulker: priority 3, attack, damage fire, debuffs imperil"),
        "{:?}",
        run.printed()
    );
    assert!(
        node_line(&run.ui_dump(), "monsters")
            .contains(r#"["Drudge Skulker",3,"attack","fire","","imperil","none","pauto"]"#),
        "{}",
        run.ui_dump()
    );
    run.click(TANK, "main.monster_remove");
    assert!(!node_line(&run.ui_dump(), "monsters").contains("Drudge Skulker"));
    // Hunting at a place typed in the Run page.
    run.set(TANK, "main.hunt_at", Value::from("42.3N 33.8E"));
    run.click(TANK, "main.hunt");
    assert!(
        run.printed()
            .iter()
            .any(|l| l == "[Tank] Hunting at 42.3N 33.8E."),
        "{:?}",
        run.printed()
    );
    run.type_line("/tank stop");
    note_asserted(
        "plugins.samples.the-tanks-window-sets-every-setting-edits-the-monster-list-and-hunts",
    );
}

/// A closed door, `locked` or not, across the way from the start to a point 30 m east.
fn door_across(locked: bool) -> ScriptedObject {
    ScriptedObject {
        id: 0x7000_0D00,
        name: "Door".into(),
        obj_type: item_type::MISC,
        place: Some((42.2, 33.85)),
        door: Some(ScriptedDoor { locked }),
        ..ScriptedObject::default()
    }
}

/// A once route of two points: the start, and 30 m east past the door.
fn route_past_the_door(run: &mut PluginRun) {
    run.click(TANK, "main.r_new");
    run.set(TANK, "main.r_kind", Value::from("once"));
    run.click(TANK, "main.r_new");
    run.click(TANK, "main.r_add");
    run.type_line("/tank route add 42.2N 33.925E");
}

/// A closed door stands across a leg of the route: the Tank uses it, waits for it to open and walks
/// through, with no stuck recovery.
fn the_tank_opens_a_closed_door_across_its_route_before_walking_it() {
    let (mut run, _) = tank(vec![door_across(false)]);
    route_past_the_door(&mut run);
    run.type_line("/tank set EnableCombat false");
    run.type_line("/tank start");
    run.advance(30.0);
    let console = run.console(TANK);
    let logged = |s: &str| console.iter().any(|l| l.contains(s));
    assert!(
        run.sent().contains(&"use 0x70000d00".to_owned()),
        "the door was used: {:?}",
        run.sent()
    );
    assert!(
        logged("route: Door is open") && logged("route: finished"),
        "opened, and the route walked: {console:#?}"
    );
    assert!(
        !logged("stuck") && !logged("travelling round"),
        "no stuck recovery: {console:#?}"
    );
    let (_, ew, _) = run.player_place().expect("placed");
    assert!(ew > 33.9, "past the door: {ew}");
    note_asserted(
        "plugins.samples.the-tank-opens-a-closed-door-across-its-route-before-walking-it",
    );
}

/// A locked door across the route does not open: the Tank says once that it blocks the way, and
/// goes on as it did before it opened doors, by its stuck recovery (here backing off round it).
fn the_tank_tells_once_of_a_locked_door_across_its_route() {
    let (mut run, _) = tank(vec![door_across(true)]);
    route_past_the_door(&mut run);
    run.type_line("/tank set EnableCombat false");
    run.type_line("/tank set NavJourneys false");
    run.type_line("/tank set StuckTries 1");
    run.type_line("/tank start");
    run.advance(90.0);
    let console = run.console(TANK);
    let told = console
        .iter()
        .filter(|l| l.contains("a locked door (Door) blocks the way"))
        .count();
    assert_eq!(told, 1, "said once: {console:#?}");
    assert!(
        console.iter().any(|l| l.contains("stuck: backing off")),
        "its stuck recovery, as before: {console:#?}"
    );
    assert!(
        !run.sent().contains(&"use 0x70000d00".to_owned())
            || run.sent().iter().filter(|s| *s == "use 0x70000d00").count() == 1,
        "the locked door used once, not again: {:?}",
        run.sent()
    );
    note_asserted("plugins.samples.the-tank-tells-once-of-a-locked-door-across-its-route");
}

/// The Tank's window runs from its controls what its VTank commands do: Pause, Force buff and its
/// cancel, the meta's state set, an expression run, the selected item tested against the loot
/// profile, a setting set in every settings profile, a log type switched off, and the route's
/// waypoints using the selected NPC (used once, or until it speaks).
fn the_tanks_controls_run_vtanks_commands() {
    use super::tank::{int, met, NEVER, NONE};
    use dereth_plugin_host::PluginFiles;
    const NPC: u32 = 0x8000_0070;
    const GEM: u32 = 0x8000_00E0;
    let (mut run, files) = tank(vec![
        ScriptedObject {
            id: NPC,
            name: "Town Crier".into(),
            obj_type: item_type::CREATURE,
            bitfield: bitfield::STUCK,
            place: Some((42.2, 33.81)),
            ..ScriptedObject::default()
        },
        ScriptedObject {
            id: GEM,
            name: "Black Garnet".into(),
            obj_type: item_type::GEM,
            container: Some(SCRIPTED_PLAYER),
            value: 3000,
            ..ScriptedObject::default()
        },
    ]);
    let mut put = files.clone();
    let meta = met(&[
        (NEVER, int(0), NONE, int(0), "Default"),
        (NEVER, int(0), NONE, int(0), "Hunt"),
    ]);
    put.write(TANK, "metas/m.met", meta.as_bytes())
        .expect("in memory");
    put.write(
        TANK,
        "p1.usd",
        "1\r\nSettings\r\n2\r\nSetting\r\nValue\r\ny\r\nn\r\n0\r\n".as_bytes(),
    )
    .expect("in memory");
    run.type_line("/tank set EnableNav false");
    run.type_line("/tank set EnableBuffing false");
    run.click(TANK, "main.start");
    run.advance(1.0);
    run.click(TANK, "main.pause");
    run.click(TANK, "main.forcebuff");
    run.click(TANK, "main.cancelforcebuff");
    let console = run.console(TANK);
    let logged = |s: &str| console.iter().any(|l| l.contains(s));
    assert!(
        logged("paused: by you")
            && logged("force buff: every buff is cast again")
            && logged("force buff cancelled after 0 buffs"),
        "{console:#?}"
    );
    // The meta's states, to set one.
    run.set(TANK, "main.s:Meta", Value::from("m.met"));
    assert!(
        node_line(&run.ui_dump(), "meta_state").contains(r#""Hunt""#),
        "{}",
        run.ui_dump()
    );
    run.set(TANK, "main.meta_state", Value::from("Hunt"));
    run.click(TANK, "main.meta_set_state");
    run.set(TANK, "main.mexec", Value::from("1+1"));
    run.click(TANK, "main.mexec_go");
    // The selected item against the loot profile.
    run.set(TANK, "main.s:LootProfile", Value::from("basic.utl"));
    run.select(GEM);
    run.click(TANK, "main.vt:testitem");
    run.advance(0.5);
    // A setting set in every settings profile.
    let names: Vec<String> = node_line(&run.ui_dump(), "all")
        .split_once("rows=")
        .map_or("", |(_, rows)| rows)
        .split("[\"")
        .skip(1)
        .filter_map(|s| s.split('"').next().map(str::to_owned))
        .collect();
    let row = names
        .iter()
        .position(|n| n == "AttackPower")
        .expect("listed")
        + 1;
    select(&mut run, TANK, "all", i64::try_from(row).expect("small"));
    run.set(TANK, "main.all_value", Value::from("70"));
    run.click(TANK, "main.all_setinall");
    // A log type off.
    run.set(TANK, "main.log:Loot", Value::Bool(false));
    let printed = run.printed();
    let said = |s: &str| printed.iter().any(|l| l.contains(s));
    assert!(said("[Tank] meta state Hunt"), "{printed:#?}");
    assert!(said("[Tank] mexec: 2"), "{printed:#?}");
    assert!(
        said("[Tank] Black Garnet: rule Valuables, kept"),
        "{printed:#?}"
    );
    assert!(
        said("[Tank] AttackPower = 70, and in 1 settings profiles"),
        "{printed:#?}"
    );
    assert!(said("Loot off"), "{printed:#?}");
    assert!(
        node_line(&run.ui_dump(), "log:Loot").contains("value=false"),
        "{}",
        run.ui_dump()
    );
    // The selected NPC, used once, then until it speaks; a vendor it is not.
    run.click(TANK, "main.r_new");
    run.select(NPC);
    run.click(TANK, "main.r_portal");
    run.click(TANK, "main.r_talk");
    run.click(TANK, "main.r_vendor");
    let points = node_line(&run.ui_dump(), "r_points");
    assert!(
        points.contains(r#"[1,"portal","Town Crier at"#)
            && points.contains(r#"[2,"use_npc","Town Crier at"#)
            && !points.contains("[3,"),
        "{points}"
    );
    assert!(
        run.console(TANK)
            .iter()
            .any(|l| l.contains("route: Town Crier is not a vendor")),
        "{:#?}",
        run.console(TANK)
    );
    run.type_line("/tank stop");
    note_asserted("plugins.samples.the-tanks-controls-run-vtanks-commands");
}
