//! Plugins: packages of Lua scripts the client loads, which hear the game's events and ask the
//! game for what the player could, through Auberean's plugin host.
//!
//! A row claims this client's own rules for plugins; the original client had none.

use super::{Behaviour, Evidence, Tier, THIS_CLIENT, TOOLING};

/// This subject's rows, in id order.
pub static ROWS: &[Behaviour] = &[
    behaviour! {
        id: "plugins.headless.a-plugins-camera-is-the-walks",
        says: "Our choice: in the headless client, a camera a plugin chooses is the walk's camera \
               from the next frame: the orbit camera with the plugin's way of moving.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-PLUGINS-HEADLESS-CAMERA"),
        station: "auberean-headless::dat::script::a_camera_a_plugin_chooses_is_the_walks_from_the_next_frame",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "plugins.headless.plugin-lines-load-list-command-and-unload",
        says: "Our choice: the headless client's plugin lines load a package from its folder, list \
               the loaded plugins with their state, carry a typed command and a press on a window \
               to the plugin that owns it, show a window's tree and a plugin's record, and unload \
               it; each load and unload is told.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-PLUGINS-HEADLESS-LINES"),
        station: "auberean-headless::cpu::plugins::the_plugin_lines_load_the_sample_list_it_run_its_command_and_unload_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.host.a-failing-plugin-is-told-and-suspended",
        says: "A plugin whose handler fails is told in the chat, one line each time; at the tenth \
               failure within a minute it is suspended, the player is told how to start it again, \
               and it hears nothing more.",
        since: THIS_CLIENT,
        divergence: "CD2-plugins",
        evidence: Evidence::Private("AC2-EVID-PLUGINS-FAULTS"),
        station: "auberean-plugin-host::cpu::host::a_failing_plugin_is_told_each_time_and_suspended_at_the_tenth_failure",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.host.a-plugin-chooses-the-camera",
        says: "A plugin chooses the orbit camera with its settings, its turning rates held to their \
               ranges, or the game's own; the program it runs in takes the frame's last choice \
               once and carries it out as an interface's choice of camera.",
        since: THIS_CLIENT,
        divergence: "CD2-plugins",
        evidence: Evidence::Private("AC2-EVID-PLUGINS-CAMERA"),
        station: "auberean-plugin-host::cpu::asks::a_plugin_chooses_the_orbit_camera_or_the_games_own_and_the_product_takes_the_last",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.host.a-plugin-enters-the-world-with-a-listed-character",
        says: "A plugin can enter the world with one of the characters the server lists: the entry \
               is sent once, the plugin hears the character enter, and what it asked completes \
               once the character is in the world.",
        since: THIS_CLIENT,
        divergence: "CD2-plugins",
        evidence: Evidence::Private("AC2-EVID-PLUGINS-SESSION-ENTRY"),
        station: "auberean-plugin-host::cpu::session::a_plugin_enters_the_world_with_the_first_character_listed_and_hears_it_enter",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.host.a-plugin-for-another-game-is-refused-and-told",
        says: "A plugin written for Auberean loads; one written for Dereth (or naming no game) is \
               refused, and the player is told it is a plugin for Dereth.",
        since: THIS_CLIENT,
        divergence: "CD2-plugins",
        evidence: Evidence::Private("AC2-EVID-PLUGINS-GAME-REFUSED"),
        station: "auberean-plugin-host::cpu::host::an_auberean_plugin_loads_and_a_dereth_plugin_is_refused_with_the_reason_told",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.host.a-plugin-hears-the-notices-the-panels-hear",
        says: "What the player's scripts tell the panels reaches a plugin listening as its part's event, \
               what it says as a word with its fields, after the frame's other events and in the \
               order told; a notice the client does not read is not told.",
        since: THIS_CLIENT,
        divergence: "CD2-plugins",
        evidence: Evidence::Private("AC2-EVID-PLUGINS-NOTICES"),
        station: "auberean-plugin-host::cpu::notices::the_scripts_notices_reach_a_plugin_as_its_parts_events_in_the_order_told",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.host.a-plugin-presses-controls-as-a-key-would",
        says: "A plugin presses a control of the game's action map and lets it go as a key bound \
               to it would, and lets go of every control in force, each as the game's own action.",
        since: THIS_CLIENT,
        divergence: "CD2-plugins",
        evidence: Evidence::Private("AC2-EVID-PLUGINS-CONTROLS"),
        station: "auberean-plugin-host::cpu::reads::a_plugin_presses_and_releases_controls_as_a_key_would",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.host.a-plugin-reads-and-answers-the-games-questions",
        says: "A plugin reads the game's questions open (each one's number, kind, words and who it \
               names) and answers one yes or no, as the client's answer to it.",
        since: THIS_CLIENT,
        divergence: "CD2-plugins",
        evidence: Evidence::Private("AC2-EVID-PLUGINS-QUESTIONS"),
        station: "auberean-plugin-host::cpu::reads::a_plugin_reads_the_games_questions_and_answers_one",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.host.a-plugin-reads-the-game-from-the-view",
        says: "A plugin reads the player, the objects around, the selection, the inventory and the \
               effects from the view the game's interfaces read, the game's ids as text; with no \
               view, a read fails saying so.",
        since: THIS_CLIENT,
        divergence: "CD2-plugins",
        evidence: Evidence::Private("AC2-EVID-PLUGINS-READS"),
        station: "auberean-plugin-host::cpu::reads::a_plugin_reads_the_player_the_objects_the_selection_and_the_inventory_from_the_view",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.host.a-plugin-reads-the-views-later-parts",
        says: "A plugin reads where the player stands, its fellowship, allegiance, friends and kingdom, \
               its journal, the map's markers, its pets and mount, its shortcut bar and its \
               crafting from the view the game's interfaces read, ids as text and kinds as words.",
        since: THIS_CLIENT,
        divergence: "CD2-plugins",
        evidence: Evidence::Private("AC2-EVID-PLUGINS-VIEW-PARTS"),
        station: "auberean-plugin-host::cpu::parts::a_plugin_reads_the_social_journal_markers_pets_shortcuts_crafting_and_place_from_the_view",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.host.a-plugins-answer-to-an-offer-is-the-games-own",
        says: "A plugin's answer to a fellowship's place or an oath offered is the game's own: \
               taken, or turned down with the reasons the game's interface sends.",
        since: THIS_CLIENT,
        divergence: "CD2-plugins",
        evidence: Evidence::Private("AC2-EVID-PLUGINS-OFFER-ANSWERS"),
        station: "auberean-plugin-host::cpu::host::a_plugins_answer_to_a_place_or_an_oath_offered_is_the_games_own",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.host.a-plugins-later-requests-are-the-clients-actions",
        says: "What a plugin asks of the selection keys, its pets, the shortcut bar, its fellowship, \
               allegiance and friends, and crafting is what the game's own interface asks for the \
               same gesture, in the order asked; a wrong argument fails the call and asks nothing.",
        since: THIS_CLIENT,
        divergence: "CD2-plugins",
        evidence: Evidence::Private("AC2-EVID-PLUGINS-LATER-REQUESTS"),
        station: "auberean-plugin-host::cpu::asks::a_plugins_selections_pets_shortcuts_social_and_crafting_requests_are_the_clients_actions",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.host.a-plugins-requests-are-the-clients-actions",
        says: "What a plugin asks of the game (a selection, a line said, a chat window's setting, an \
               attack stopped) is what the game's own interface asks for the same gesture, in the \
               order asked, carried out as the frame ends.",
        since: THIS_CLIENT,
        divergence: "CD2-plugins",
        evidence: Evidence::Private("AC2-EVID-PLUGINS-REQUESTS"),
        station: "auberean-plugin-host::cpu::host::what_a_plugin_asks_of_the_game_becomes_the_clients_actions_in_order",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.host.a-plugins-store-outlives-it",
        says: "What a plugin keeps in its store is written when it unloads and is there again when \
               it loads; its character's store is kept under the world, the account and the \
               character.",
        since: THIS_CLIENT,
        divergence: "CD2-plugins",
        evidence: Evidence::Private("AC2-EVID-PLUGINS-KEPT"),
        station: "auberean-plugin-host::cpu::host::a_plugins_store_is_written_on_unload_and_read_again_on_load",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.host.a-typed-command-is-the-players-gesture",
        says: "A command a plugin claimed reaches it when the player types it, as the player's \
               gesture: a window the plugin then shows opens, where one it shows unasked stays \
               closed.",
        since: THIS_CLIENT,
        divergence: "CD2-plugins",
        evidence: Evidence::Private("AC2-EVID-PLUGINS-GESTURE"),
        station: "auberean-plugin-host::cpu::host::a_typed_command_reaches_its_plugin_as_a_gesture_and_opens_the_window_it_shows",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.host.past-its-share-of-a-frame-a-plugins-events-wait",
        says: "A plugin's time in one frame has a share; past it, the events still due for the \
               plugin wait, in order, for the next frame, before that frame's own.",
        since: THIS_CLIENT,
        divergence: "CD2-plugins",
        evidence: Evidence::Private("AC2-EVID-PLUGINS-FRAME-SHARE"),
        station: "auberean-plugin-host::cpu::host::past_its_share_of_a_frame_a_plugins_events_wait_in_order_for_the_next",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.host.the-character-playing-is-the-views",
        says: "A plugin's character store opens under the character the game's view names and the \
               world it plays on, with no word from the program the plugins run in.",
        since: THIS_CLIENT,
        divergence: "CD2-plugins",
        evidence: Evidence::Private("AC2-EVID-PLUGINS-PLAYING"),
        station: "auberean-plugin-host::cpu::reads::the_character_store_opens_under_the_character_and_world_the_view_names",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.host.the-games-events-reach-handlers-in-order",
        says: "The game's events (the character list, effects, what is said, trades and the rest) \
               reach the plugins listening for them in the order the game told them, the game's \
               ids written as text.",
        since: THIS_CLIENT,
        divergence: "CD2-plugins",
        evidence: Evidence::Private("AC2-EVID-PLUGINS-HEARD"),
        station: "auberean-plugin-host::cpu::host::the_games_events_reach_the_handlers_listening_in_the_order_told",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.host.the-player-closing-a-window-is-told-to-its-plugin",
        says: "The player closing a plugin's window closes it and tells the plugin (`ui.closed`) \
               at the next frame, once; a window the plugin never declared is refused.",
        since: THIS_CLIENT,
        divergence: "CD2-plugins",
        evidence: Evidence::Private("AC2-EVID-PLUGINS-CLOSED"),
        station: "auberean-plugin-host::cpu::host::the_player_closing_a_window_closes_it_and_its_plugin_hears_it_once",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.host.timers-come-due-on-the-frame-clock",
        says: "A plugin's timer is due on the client's frame clock, once, not before its time; a \
               plugin not active in the session's part hears no timer until it is.",
        since: THIS_CLIENT,
        divergence: "CD2-plugins",
        evidence: Evidence::Private("AC2-EVID-PLUGINS-TIMERS"),
        station: "auberean-plugin-host::cpu::host::a_timer_comes_due_on_the_frame_clock_and_waits_while_its_plugin_is_inactive",
        tier: Tier::Cpu,
    },
];
