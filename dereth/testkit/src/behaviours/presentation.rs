//! Presentation -- the window, its size and display preferences, screenshots and shutdown.
//!
//! One file per subject, so that two changes adding rows at the same time do not edit the same
//! file. [`ROWS`] is in id order; the registry's own test asserts that, and that no id and no
//! evidence handle is repeated anywhere in it.

// `behaviour!` is `#[macro_export]`ed by `mod.rs` above this module's declaration, so it is in
// textual scope here and needs no import.
use super::{Behaviour, Evidence, Tier, RETAIL, THIS_CLIENT, TOOLING};

/// This subject's rows, in id order.
pub static ROWS: &[Behaviour] = &[
    behaviour! {
        id: "presentation.brightness.it-changes-the-games-own-picture-and-not-the-display",
        says: "The brightness setting is part of the game's own picture: a captured frame is \
               darker at the lowest setting and brighter at a high one than at the middle, by \
               about the factor the original's display curve used, because the curve is \
               applied as the picture is drawn rather than left to the display.",
        since: THIS_CLIENT,
        divergence: "CD-009",
        evidence: Evidence::Private("AC-EVID-GAMMA-IN-PICTURE"),
        station: "dereth-client::gpu::rendering::render_preferences::a_brighter_profile_draws_a_brighter_frame",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "presentation.clock.a-time-sync-puts-the-date-and-sky-on-the-shards-clock",
        says: "A single time sync from the shard moves the date shown on the map from this \
               session's own starting date onto the shard's calendar date.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F71-CLOCK"),
        station: "dereth-client::gpu::presentation::world_clock_sync::one_time_sync_datagram_puts_the_map_s_date_on_the_shard_s_clock",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "presentation.crash.a-fault-is-named-and-still-kills-the-process",
        says: "When the client crashes on a bad memory access it writes a report naming the fault \
               to its error output before it dies, and it still dies with the same error code \
               rather than carrying on.",
        since: TOOLING,
        evidence: Evidence::Private("AC-EVID-EXCEPTION-FILTER-CRASH"),
        station: "dereth-render::gpu::presentation::exception_filter::a_fault_is_reported_and_still_kills_the_process",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "presentation.crash.every-run-keeps-a-log-that-records-a-panic",
        says: "Every run of the client opens a log of its own in the crash-logs folder of the \
               client's settings folder with a record of its start, and a run that panics adds the \
               panic and a backtrace to it before it dies and leaves it there.",
        since: TOOLING,
        evidence: Evidence::Private("AC-EVID-CRASH-LOG"),
        station: "dereth-client::cpu::presentation::crash_log::a_run_that_panics_leaves_the_panic_in_its_crash_log",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "presentation.era.a-component-drags-from-the-create-spell-grid-onto-the-formula",
        says: "On the Create Spell page a carried component is picked up from the grid and \
               dragged onto the formula, where it is laid at the formula's end. While a \
               component (from the grid or the pack) is over the formula, the item slot's drag \
               hint shows on the place it would be laid, accepting while the formula has room and \
               refusing when it is full; it comes down when the drag leaves or drops. An empty \
               grid slot picks nothing up.",
        since: THIS_CLIENT,
        divergence: "CD-016",
        evidence: Evidence::Private("AC-EVID-R2-RESEARCH-DRAG"),
        station: "dereth-ui-screens::dat::panels::create_spell_tab::a_component_dragged_from_the_grid_onto_the_formula_shows_the_drag_hint_and_is_laid",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "presentation.era.the-panel-buttons-close-up-over-a-system-the-world-lacks",
        says: "When the world has no system for one of the toolbar's panel buttons (no journal on \
               an Infiltration world), the button is hidden and the row's other buttons spread \
               evenly from its first place to the end of its last, over the bar's black and gold \
               strip stretched behind the whole row, so no hole is left; on an end-of-retail world \
               every button is back in its own place and the strip its own size.",
        since: THIS_CLIENT,
        divergence: "CD-010",
        evidence: Evidence::Private("AC-EVID-UI-UNIFY-BUTTON-ROW"),
        station: "dereth-ui-screens::dat::panels::era_panels::on_an_infiltration_world_the_panel_buttons_close_up_over_the_missing_journal_button",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "presentation.era.the-retail-interface-draws-its-own-art-over-an-older-world",
        says: "Over the February 2005 world the retail interface's screens are read from the \
               end-of-retail files: where both sets of files hold a picture under the same id, \
               the interface draws the end-of-retail one (its panel frames, buttons, bars and \
               colours), and the world's own store still answers with the older one, which is \
               where the pictures the world names (item, spell, skill and component icons) are \
               drawn from.",
        since: THIS_CLIENT,
        divergence: "CD-010",
        evidence: Evidence::Private("AC-EVID-UI-UNIFY-CHROME"),
        station: "dereth-dat::dat::container::interface_files::the_interface_files_beside_an_older_world_answer_with_the_later_pictures",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "presentation.era.the-retail-magic-window-has-a-create-spell-tab-with-spell-research",
        says: "On a world with spell research the retail interface's magic window has a third \
               tab, Create Spell, sharing the tab strip with Spells and Components; on one \
               without it there is no such tab (a window left open on it moves to the Spells \
               tab). Its page lays carried components into a formula of up to eight, by a double \
               click on one or by dragging one from the pack onto the formula; a double click \
               on a laid component takes it out, Test sends the components in the order laid, \
               and Clear empties the formula.",
        since: THIS_CLIENT,
        divergence: "CD-016",
        evidence: Evidence::Private("AC-EVID-ERA-UI-RETAIL-RESEARCH"),
        station: "dereth-ui-screens::dat::panels::create_spell_tab::the_create_spell_tab_is_absent_without_spell_research_and_shares_the_strip_with_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "presentation.era.the-screens-leave-out-what-the-worlds-era-lacks",
        says: "On a world whose era has no contract tracker the quest page shows no Contracts \
               tab, and on one with no titles the character page no Titles tab (a page left open \
               on such a tab moves to its next tab, and the page's other tabs share its strip, \
               growing over the gap); an era with no cloaks or trinkets has no \
               cloak or trinket slot on the paper doll, and one with no luminance no luminance \
               section on the character sheet; one with no journal has no journal button, and its \
               quest page never opens; a world the server announces without trade, tinkering, \
               housing or chess never opens the secure-trade, salvage, house purchase or chess \
               window and has no House tab on the map page; on an end-of-retail world all are \
               there.",
        since: THIS_CLIENT,
        divergence: "CD-010",
        evidence: Evidence::Private("AC-EVID-ERA-2005-PANELS"),
        station: "dereth-ui-screens::dat::panels::era_panels::an_infiltration_world_has_no_contracts_tab_and_the_quest_page_leaves_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "presentation.frame.a-frame-with-no-physics-tick-draws-what-the-last-drew",
        says: "A frame on which physics did not tick draws exactly what the frame before it drew: \
               the running player's body and the camera hold still between ticks at every frame \
               rate tried.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F38-FRAME"),
        station: "dereth-client::gpu::presentation::frame_independence::a_frame_that_did_not_tick_draws_exactly_what_the_frame_before_it_drew",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "presentation.growth.leave-return-churn-returns-every-container-to-baseline",
        says: "Walking away from a landblock and back fifteen times, with its objects leaving and \
               returning, leaves every one of the client's object and scene tables at the size it \
               started at.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P54A-GROWTH"),
        station: "dereth-client::gpu::presentation::long_session_growth::nine_landblock_round_trips_return_every_container_to_baseline",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "presentation.headless.a-one-frame-capture-is-byte-identical-across-runs",
        says: "Running the client without a window and saving the one frame it draws gives exactly \
               the same image, byte for byte, on each of three runs.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-HEADLESS-CAPTURE-HEADLESS"),
        station: "dereth-client::gpu::presentation::headless_capture_determinism::the_headless_capture_is_byte_identical_across_three_runs",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "presentation.headless.a-recorded-session-drives-the-real-app-to-character-select-and-into-the-world",
        says: "The client with no window can be driven by a script from a recorded session: naming \
               a character from the recording takes it into the world as that character, a \
               scripted walk reaches the movement controls, and a snapshot shows the player the \
               shard described.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-MAIN-HEADLESS"),
        station: "dereth-headless::dat::presentation::headless_replay_run::a_named_character_reaches_the_world_and_a_walk_reaches_the_movement_route",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "presentation.headless.the-world-before-throne-of-destiny-draws-under-todays-screens",
        says: "Given the older world's data files (those of February 2005) beside the \
               end-of-retail ones, the client draws the world as it was then (its landscape, \
               buildings and scenery) under the end-of-retail screens.",
        since: THIS_CLIENT,
        divergence: "CD-010",
        evidence: Evidence::Private("AC-EVID-ERA-2005-CAPTURE"),
        station: "dereth-client::gpu::presentation::pre_tod_capture::the_february_2005_holtburg_draws_under_todays_screens",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "presentation.interface.a-switch-back-to-the-retail-interface-shows-the-screen-the-game-is-at",
        says: "Switched back to from the classic interface, the retail interface comes up on the \
               screen the game is at: the gameplay screen in the world, the character screen at \
               character selection, the disconnected screen after a disconnect. It does not \
               replay the opening screens, and a character list that arrived while the classic \
               interface was shown does not send a player in the world back to character \
               selection.",
        since: THIS_CLIENT,
        divergence: "CD-015",
        evidence: Evidence::Private("AC-EVID-R2-SWITCH-BACK"),
        station: "dereth-client::gpu::presentation::shell::a_retail_interface_shown_again_comes_up_on_the_screen_the_game_is_at",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "presentation.interface.a-switch-follows-the-choice-and-a-refused-one-goes-back",
        says: "The interface follows the Interface option on the next frame; a choice of the \
               classic interface that cannot be shown goes back to the retail one, and the chat \
               lines of the last while are kept for the interface switched to.",
        since: THIS_CLIENT,
        divergence: "CD-015",
        evidence: Evidence::Private("AC-EVID-ERA-UI-SWITCH"),
        station: "dereth-client-shell::lib::classic_face::tests::a_refused_classic_choice_goes_back_to_the_retail_interface",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "presentation.intro-movie.every-frame-decodes-at-the-movies-rate-and-it-never-loops",
        says: "Every frame of the opening movie decodes to a full opaque 640 by 480 picture, and \
               after the last frame the movie is over and stays over rather than starting again.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-VIDEO-DECODE-INTRO-MOVIE"),
        station: "dereth-audio::dat::presentation::intro_movie::every_frame_decodes_and_the_movie_never_loops",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "presentation.long-session.a-static-emitters-event-queue-stays-bounded",
        says: "Over a long session the scripted objects baked into Holtburg never build up a \
               backlog of unhandled events: every sound their scripts raise is taken in the same \
               step, and the scripts keep firing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1163-LONG-SESSION"),
        station: "dereth-client::gpu::presentation::static_event_queue_growth::a_landblock_statics_event_queue_stays_bounded_over_a_long_session",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "presentation.long-session.object-mesh-slots-stay-bounded",
        says: "A full replay of a long solo play session runs to its last message without ever \
               running out of graphics slots, and the slots held by object meshes stay within a \
               fixed bound at the peak and at the end.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O511-LONG-SESSION"),
        station: "dereth-client::gpu::presentation::long_session_mesh_eviction::a_full_long_solo_play_replay_completes_and_its_peak_occupancy_is_bounded",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "presentation.performance.a-key-or-the-option-shows-the-panel-over-any-interface",
        says: "The Performance Panel option, or a key the player binds to the panel (none is \
               bound at first), shows a panel over the game on any screen and in either \
               interface, with the frame rate, the mean and longest frame time and the time each \
               part of the frame takes; the key pressed again hides it.",
        since: THIS_CLIENT,
        divergence: "CD-018",
        evidence: Evidence::Private("AC-EVID-UI-UNIFY-PERF-PANEL"),
        station: "dereth-client::dat::ui::performance_panel_key::the_performance_panel_has_no_key_until_one_is_bound_and_then_fires_on_every_screen",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "presentation.preferences.the-selected-profile-is-loaded-and-saved-to",
        says: "A preferences profile named on the command line supplies the startup settings and \
               is where settings are saved; its resolution takes effect on entering the world, \
               while the screens before that stay at 800 by 600.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-PREFERENCES-SELECTED"),
        station: "dereth-client::gpu::presentation::selected_preferences_profile::the_selected_profile_reaches_the_gameplay_presentation",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "presentation.resolution.a-choice-in-the-dropdown-resizes-at-once-and-a-yes-keeps-it",
        says: "Choosing a resolution in the options drop-down resizes the display at once; a \
               confirmation box follows a moment later, and answering yes keeps the new size.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P182-RESOLUTION"),
        station: "dereth-client::gpu::presentation::resolution_dropdown::wired::physical_resolution_change_applies_before_the_delayed_confirmation_and_yes_keeps_it",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "presentation.resolution.every-screen-but-gameplay-is-forced-to-eight-hundred-by-six-hundred",
        says: "Every screen before entering the world is shown at 800 by 600: entering the world \
               switches to the player's chosen resolution, and logging out to character selection \
               goes back to 800 by 600 while the chosen size stays saved.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P183-RESOLUTION"),
        station: "dereth-client::gpu::presentation::screens_at_resolution::wired::logging_out_at_an_enlarged_resolution_returns_to_800x600",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "presentation.screenshot.the-shipped-key-writes-the-next-free-file-beside-preferences",
        says: "The screenshot key saves a picture named ScreenShot with the first free five-digit \
               number into the folder that holds the preferences file, and posts one message \
               saying so.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1144-SCREENSHOT"),
        station: "dereth-client::gpu::presentation::screenshot_file::the_screenshot_lands_beside_the_preferences_file_as_screenshot00000",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "presentation.settings.a-headless-run-leaves-the-players-settings-alone-unless-given-a-file",
        says: "A run with no window reads nothing from the player's settings folder and writes                nothing to it: the preferences and the key map are byte for byte what they were                afterwards. Naming a preferences file on the command line, with either spelling of                the switch, asks for that file, which the run then reads and writes on exit.",
        since: TOOLING,
        evidence: Evidence::Private("AC-EVID-HEADLESS-SETTINGS"),
        station: "dereth-client::gpu::presentation::headless_settings::a_headless_run_leaves_the_players_preferences_and_key_map_untouched",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "presentation.settings.both-interfaces-edit-one-store",
        says: "The classic interface's Client page opens on the same preferences the retail \
               interface's Client Options page edits, and what it applies the retail page \
               shows: the volumes, the sound switches, brightness, the camera's stiffness, \
               graphics performance, automatic degrading, the texture sizes, the detail \
               textures, the window's size and full screen. A settings file the classic \
               interface kept in its own folder is carried into the shared preferences once, \
               each setting the player had moved, and removed.",
        since: THIS_CLIENT,
        divergence: "CD-015",
        evidence: Evidence::Private("AC-EVID-UI-UNIFY-ONE-STORE"),
        station: "dereth-classic-ui::lib::settings_host::tests::the_page_opens_on_the_shared_store_and_commits_to_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "presentation.settings.the-client-keeps-its-files-in-a-folder-of-its-own",
        says: "The client keeps its preferences, key maps and other files in a folder named for \
               this client rather than for the original game: Dereth's client folder in the \
               roaming application data on Windows, and in each other platform's own place for \
               settings.",
        since: THIS_CLIENT,
        divergence: "CD-007",
        evidence: Evidence::Private("AC-EVID-CONFIG-SETTINGS-DIR"),
        station: "dereth-desktop::lib::folders::tests::the_settings_directory_follows_the_platform_convention",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "presentation.settings.the-default-key-map-is-named-for-this-client",
        says: "A player who has not named a key map file gets one called dereth.keymap in the \
               settings folder, whatever the program file is called; a key map the player named \
               is used by that name.",
        since: THIS_CLIENT,
        divergence: "CD-007",
        evidence: Evidence::Private("AC-EVID-KEYMAP-DEFAULT-NAME"),
        station: "dereth-client-shell::lib::input::tests::the_keymap_file_hangs_off_the_preferences_files_directory",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "presentation.shell.the-opening-mode-transitions-run-in-order",
        says: "Starting the client runs its opening screens in a fixed order, one switch per \
               frame: data patching, the intro, character selection and then the world, each \
               screen building without failure.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-SHELL-SHELL"),
        station: "dereth-client::gpu::presentation::shell::the_documented_opening_mode_transitions_run_in_the_documented_order",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "presentation.shutdown.the-teardown-runs-the-documented-order",
        says: "Shutting the client down tears things down in a fixed order: the network before the \
               interface, the interface before the game data, and the game data before the final \
               wait for the graphics device.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-SHUTDOWN-SHUTDOWN"),
        station: "dereth-client::gpu::presentation::shutdown::the_teardown_runs_the_documented_order",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "presentation.startup.a-second-client-starts-while-the-first-is-running",
        says: "A second client starts, and runs its interface, while another is still running on \
               the same machine; neither refuses the other.",
        since: THIS_CLIENT,
        divergence: "CD-008",
        evidence: Evidence::Private("AC-EVID-MULTI-CLIENT"),
        station: "dereth-client-shell::lib::app::tests::a_second_client_starts_while_the_first_is_running",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "presentation.sync-to-refresh.reaches-the-swap-chain-present-interval",
        says: "With sync-to-refresh on, each finished frame waits for one display refresh before \
               it is shown; with it off, frames are shown immediately.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P182-SYNC-TO-REFRESH"),
        station: "dereth-client::gpu::presentation::sync_to_refresh::a_real_hidden_swap_chain_presents_with_the_configured_interval",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "presentation.window.a-resolution-change-keeps-the-windows-top-left",
        says: "Picking a new resolution changes the window's size but keeps its top-left corner \
               where it was, instead of re-centring the window.",
        since: THIS_CLIENT,
        divergence: "CD-001",
        evidence: Evidence::Private("AC-EVID-P184-WINDOW"),
        station: "dereth-client::gpu::presentation::window_position::wired::picking_a_resolution_resizes_without_moving_the_window",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "presentation.window.leaving-full-screen-puts-the-window-back-where-it-was",
        says: "Leaving full screen puts the window back with its top-left corner where it was \
               before it went full screen, instead of centring it on the screen, moving it only \
               as far as it must to stay on the screen.",
        since: THIS_CLIENT,
        divergence: "CD-001",
        evidence: Evidence::Private("AC-EVID-WINDOW-POSITION-LEAVING-FULL-SCREEN"),
        station: "dereth-client::gpu::presentation::window_position::wired::leaving_full_screen_puts_the_window_back_where_it_was",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "presentation.window.the-picture-is-the-windows-real-pixels-at-any-desktop-scaling",
        says: "A resolution is the window's real pixels, whatever scaling the desktop applies: \
               when the desktop resizes the window, the picture and the interface follow its \
               real size, the window does not move, and the player's resolution setting is not \
               rewritten.",
        since: THIS_CLIENT,
        divergence: "CD-004",
        evidence: Evidence::Private("AC-EVID-P188-PHYSICAL-PIXELS"),
        station: "dereth-client-shell::lib::app::tests::physical_window_resize_reaches_the_backbuffer_and_ui_without_changing_preferences",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "presentation.window.the-title-names-the-account-as-it-was-typed",
        says: "The window's title is the client's name followed by the account it logged in \
               with, spelled as the player typed it, so several clients open at once can be told \
               apart on the task bar; with no account the title is the client's name alone.",
        since: THIS_CLIENT,
        divergence: "CD-006",
        evidence: Evidence::Private("AC-EVID-CONFIG-TITLE"),
        station: "dereth-client-runtime::lib::config::tests::the_window_title_carries_the_account_as_typed",
        tier: Tier::Cpu,
    },
];
