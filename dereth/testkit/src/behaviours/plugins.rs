//! Plugins: what a plugin can hear, do and show through the client's plugin host, and the limits
//! the host holds it to.
//!
//! Every row here is this client's own rule (the host is an addition to retail, published as a
//! divergence), and each is asserted by running one of the spec's conformance cases
//! (`docs/plugins/conformance/core/`) over a replayed recording, with no data file.

// `behaviour!` is `#[macro_export]`ed by `mod.rs` above this module's declaration, so it is in
// textual scope here and needs no import.
use super::{Behaviour, Evidence, Tier, THIS_CLIENT};

/// This subject's rows, in id order.
pub static ROWS: &[Behaviour] = &[
    behaviour! {
        id: "plugins.actions.a-click-loots-a-corpse-one-awaited-request-at-a-time",
        says: "A click on a plugin's button starts a loot: the corpse is opened, only the item the \
               loot profile cannot decide from what every client sees is appraised, and each \
               item kept is moved to the pack only after the one before it has arrived.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-LOOT"),
        station: "dereth-testkit::cpu::plugins::scenario_a_click_loots_a_corpse_one_awaited_request_at_a_time",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.actions.a-refusal-of-something-the-character-said-settles-no-action",
        says: "A plugin action's promise settles on the answer to that action, never on whichever \
               message comes next: a refusal of something the character said (a tell to someone not \
               there, a channel, a chat emote, a squelch, being gagged or away) settles no action, \
               and a refusal naming an object settles only an action on that object. After a vendor's \
               window closes, a tell's refusal arriving while a buy, a use and a give wait for their \
               answers leaves each to end as the server answers it.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-STRAY-REFUSAL"),
        station: "dereth-testkit::cpu::plugins::scenario_a_refusal_of_something_the_character_said_settles_no_action",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.actions.a-refusal-of-the-action-itself-fails-it-with-its-reason",
        says: "A refusal of the action itself still fails its promise, with the reason the server \
               gave: a buy refused naming the item bought, or refused whole as a failed save of the \
               character with the reason in its acknowledgement; a sale refused naming the item sold; \
               a use on a target refused naming the target; a trade add refused naming the item; and \
               a cast refused outright.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-OWN-REFUSAL"),
        station: "dereth-testkit::cpu::plugins::scenario_a_refusal_of_the_action_itself_fails_it_with_its_reason",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.actions.a-trade-and-a-salvage-end-at-the-servers-word",
        says: "A plugin's sale and purchase at an open vendor end when the vendor shows its wares \
               again, five of a stackable bought being five (the stack size the vendor's list \
               gives a stock row is given it first); a salvage ends when the items have left the \
               pack for bags of their material; a loot profile's salvage combining reads as its \
               ranges.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TRADE-SALVAGE"),
        station: "dereth-testkit::cpu::plugins::scenario_a_trade_and_a_salvage_end_at_the_servers_word",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.actions.an-appraisal-settles-its-promise-with-what-it-told",
        says: "A plugin's appraisal settles once the client has the answer, with what the appraisal \
               told, every plugin listening hears that the item was appraised, and an appraisal \
               nobody answers fails after its time runs out.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-APPRAISE"),
        station: "dereth-testkit::cpu::plugins::scenario_an_appraisal_settles_its_promise_with_what_it_told",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.actions.an-item-used-on-a-target-sends-one-use-with-target-and-a-wielded-caster-stays-in-hand",
        says: "A plugin's use of an item on a target is the game interface's use of one item on \
               another for plugins: the target taken for the use without changing the player's \
               selection, one use-with-target request (Inventory_UseWithTargetEvent) sent at once \
               and the action settled at the server's acknowledgement. A wielded item is used as \
               the targeted use's second click, so a caster's own spell is cast and the caster \
               stays in hand where a plain use would put it in the pack; an item used on what it \
               does not take is refused by the client with nothing sent.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-USE-ON"),
        station: "dereth-testkit::cpu::plugins::scenario_an_item_used_on_a_target_sends_one_use_with_target_and_a_wielded_caster_stays_in_hand",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.actions.without-a-gesture-an-action-needs-its-domain-and-the-worlds-leave",
        says: "An action a plugin takes on its own needs the world's leave for its kind of \
               automation, and one the world denies is refused with a gesture too; a character's \
               advancement never happens without the player; and for half a minute after the \
               player types one of its commands its actions follow the player.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-GESTURES"),
        station: "dereth-testkit::cpu::plugins::scenario_without_a_gesture_an_action_needs_its_domain_and_the_worlds_leave",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.audit.the-audit-names-each-loaded-plugin-and-the-permissions-its-manifest-names",
        says: "A server that asks which plugins are running is told each loaded plugin's id and \
               version, and asked about one is told its name, authors and the permissions its \
               manifest names.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-AUDIT"),
        station: "dereth-testkit::cpu::plugins::scenario_the_audit_names_each_loaded_plugin_and_the_permissions_its_manifest_names",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.automation.a-heal-is-its-own-domain-and-a-debuff-is-cast-once-per-target",
        says: "A Life spell cast as a heal is automated healing, which a world may allow while it denies \
               buffing; the debuffer casts a debuff on a target once until its recast time has passed.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-HEAL-DEBUFF"),
        station: "dereth-testkit::cpu::plugins::scenario_a_heal_is_its_own_domain_and_a_debuff_is_cast_once_per_target",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.automation.a-meta-runs-its-states-rules-on-a-timer",
        says: "A meta checks its current state's rules on a timer and runs the action of each rule whose \
               condition holds; its actions change its state and may move the character.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-META"),
        station: "dereth-testkit::cpu::plugins::scenario_a_meta_runs_its_states_rules_on_a_timer",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.automation.a-metas-chat-capture-takes-the-colors-asked-and-its-persistent-seconds-outlast-a-stop",
        says: "A meta's chat capture with a list of chat colors (VTank's ChatMessageCapture) \
               matches only lines of those types and keeps the matched line's type in \
               capturecolor; its persistent seconds-in-state condition counts from when the state \
               was entered through a stop and a start, where the plain one counts again from the \
               start.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-META-CAPTURE-COLOR"),
        station: "dereth-testkit::cpu::plugins::tank::scenario_a_metas_chat_capture_takes_the_colors_asked_and_its_persistent_seconds_outlast_a_stop",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.automation.a-player-killers-automation-stops-where-the-world-says",
        says: "In a world whose policy turns automation off for player killers, a player killer's plugin \
               cannot act on its own, and still acts on the player's command.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-PK"),
        station: "dereth-testkit::cpu::plugins::scenario_a_player_killers_automation_stops_where_the_world_says",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.automation.a-plugin-fights-through-the-combat-controls",
        says: "A plugin enters combat through the combat toggle, swings at a target with the combat \
               window's own height and power, and returns to peace; each settles on what the \
               client saw.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-COMBAT"),
        station: "dereth-testkit::cpu::plugins::scenario_a_plugin_fights_through_the_combat_controls",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.automation.a-vtank-meta-file-imports-and-runs-in-vtanks-order",
        says: "A VTank meta file imports as a meta checked by VTank's rule, each true rule of its \
               state running once per stay in it; its expressions, options, embedded route and meta \
               view work, and what it could not carry is listed.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-MET-IMPORT"),
        station: "dereth-testkit::cpu::plugins::scenario_a_vtank_meta_file_imports_and_runs_in_vtanks_order",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.automation.a-vtank-meta-file-read-as-data-writes-back-as-vtank-writes-it",
        says: "A VTank meta file read as data writes back as VTank writes it, line ends, embedded                route and meta view kept, and a rule added as data runs once written and loaded.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-MET-WRITE"),
        station: "dereth-testkit::cpu::plugins::scenario_a_vtank_meta_file_read_as_data_writes_back_as_vtank_writes_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.automation.a-watchdog-moves-a-still-meta-on",
        says: "A meta's watchdog enters its state when the character has not moved its distance in its \
               time; setting it again does not restart its clock.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-WATCHDOG"),
        station: "dereth-testkit::cpu::plugins::scenario_a_watchdog_moves_a_still_meta_on",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.automation.an-attack-settles-on-what-the-server-does-with-it",
        says: "An attack settles on what the server does with it: the end of the attack sequence \
               after a swing, or a swing begun or ended, is a swing made; a refusal fails with its \
               code; an attack the client did not make fails at once.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-ATTACK-OUTCOMES"),
        station: "dereth-testkit::cpu::plugins::scenario_an_attack_settles_on_what_the_server_does_with_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.automation.an-attacks-swing-goes-only-at-its-target",
        says: "An attack's swing goes only at its target: one whose target stops being the \
               selection before the swing goes out (its body went and the client's automatic \
               targeting picked another monster, or another was selected) is dropped, or \
               cancelled if it went out, and fails target_gone; it is never sent at the new \
               selection.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-ATTACK-TARGET"),
        station: "dereth-testkit::cpu::plugins::scenario_an_attacks_swing_goes_only_at_its_target",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.automation.beneficial-spells-on-another-character-are-their-own-domain",
        says: "A beneficial spell cast on another character is the support domain, apart from combat: \
               a world that denies combat and allows support lets a plugin take up its caster, enter \
               the magic stance and buff another player for support, and refuses its debuff on a \
               monster.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-SUPPORT-DOMAIN"),
        station: "dereth-testkit::cpu::plugins::scenario_beneficial_spells_on_another_character_are_their_own_domain",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.automation.expressions-evaluate-in-vtanks-language",
        says: "Expressions in VTank's language evaluate with its operators, precedence, variables, lists, \
               patterns and game functions; an unsupported function or a malformed expression is an \
               error, not a value.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-EXPRESSIONS"),
        station: "dereth-testkit::cpu::plugins::scenario_expressions_evaluate_in_vtanks_language",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.automation.portal-space-and-death-are-heard-in-order",
        says: "Entering and leaving portal space and the character's death are events heard in order, and \
               a meta's portal and death conditions hold at its next check after them.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-PORTAL-DEATH"),
        station: "dereth-testkit::cpu::plugins::scenario_portal_space_and_death_are_heard_in_order",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.automation.read-parity-limits-a-search-to-the-radar-and-the-pack",
        says: "In a world whose policy asks for read parity, a plugin's search finds only what the radar \
               shows and what the character holds.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-PARITY"),
        station: "dereth-testkit::cpu::plugins::scenario_read_parity_limits_a_search_to_the_radar_and_the_pack",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.automation.stopping-an-attack-cancels-it-and-fails-its-promise",
        says: "Stopping an attack cancels at once the attack being charged, or released and waiting for \
               the last swing's answer, as the escape key does, and fails the plugin's attacks in \
               flight or queued as stopped.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-COMBAT-STOP"),
        station: "dereth-testkit::cpu::plugins::scenario_stopping_an_attack_cancels_it_and_fails_its_promise",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.automation.stopping-and-letting-go-are-always-allowed",
        says: "Stopping is always allowed: with no gesture behind it and in a world that stops a \
               player killer's automation, a plugin still stops the attack it started and lets go \
               of the run lock it set, while starting again needs what any action needs.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-LET-GO"),
        station: "dereth-testkit::cpu::plugins::scenario_stopping_and_letting_go_are_always_allowed",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.automation.utilitybelts-additions-to-the-expression-language-evaluate-as-documented",
        says: "Expressions in VTank's language take UtilityBelt's additions as it documents them: \
               dictionaries, indexing and slicing, the list library with item expressions, math and \
               bitwise operators, kept and shared variables, stopwatches, a lazy ifthen, exec, the \
               game calendar and the meta's own state and settings; a bracket a meta leaves open at \
               the end is closed there.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-EXPRESSIONS-UB"),
        station: "dereth-testkit::cpu::plugins::scenario_utilitybelts_additions_to_the_expression_language_evaluate_as_documented",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.character.a-plugin-reads-the-characters-fellowship-titles-augmentations-bars-and-options",
        says: "A plugin reads the character's fellowship with each fellow's vitals and hears it \
               joined, and reads its titles, augmentations, spell bars, options, attributes, \
               allegiance and contracts as the game's own panels show them.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-CHARACTER"),
        station: "dereth-testkit::cpu::plugins::scenario_a_plugin_reads_the_characters_fellowship_titles_augmentations_bars_and_options",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.chat.a-line-reaches-a-plugin-as-the-player-reads-it",
        says: "A chat line reaches a plugin as the player reads it: a web address in it is the \
               plain text that was written, with no link markup; a Dereth Network channel's line \
               is a channel line, and the player's own line told to a channel is the player's \
               channel line, while a tell from a character stays private.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-CHAT-AS-READ"),
        station: "dereth-testkit::cpu::plugins::scenario_a_line_reaches_a_plugin_as_the_player_reads_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.chat.a-plugin-hears-each-line-and-prints-its-own-locally",
        says: "A plugin hears every chat line in the order it arrived, with its kind and its type's \
               number, and a line the plugin prints appears in the chat window without being sent \
               anywhere.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-CHAT"),
        station: "dereth-testkit::cpu::plugins::scenario_a_plugin_hears_each_line_and_prints_its_own_locally",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.chat.a-plugin-hides-incoming-lines",
        says: "A plugin can keep an incoming line from being shown, and the line is not shown.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-CHAT-FILTER"),
        station: "dereth-testkit::cpu::plugins::scenario_a_plugin_hides_incoming_lines",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.chat.a-plugins-own-line-is-never-taken-for-a-command",
        says: "A line a plugin sends is never taken for a command, however its verb is spelled (a \
               full-width slash or letters, an @, a comma after the verb) and even when the verb \
               is claimed after the line was sent: it reaches neither a plugin nor the plugin \
               host's own command and opens no thirty seconds of interactive actions, while a \
               command the player types still does.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-CHAT-SEND"),
        station: "dereth-testkit::cpu::plugins::scenario_a_plugins_own_line_is_never_taken_for_a_command",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.commands.a-claimed-command-reaches-its-plugin-and-the-clients-own-win",
        says: "A slash command a plugin adds reaches that plugin with the words the player typed \
               after it, in any letter case, and a command the client already has cannot be \
               taken over by a plugin.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-COMMANDS"),
        station: "dereth-testkit::cpu::plugins::scenario_a_claimed_command_reaches_its_plugin_and_the_clients_own_win",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.data.a-plugin-reads-a-landblock-and-a-dungeon-cell-of-the-game-data",
        says: "A plugin reads a landblock of the game's data, its heights in metres, terrain, \
               roads and the cells inside it, and a dungeon cell, the cells its portals lead to \
               and those seen from it, its floor's outline and its box; what the data does not \
               hold reads nil.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-GAME-DATA"),
        station: "dereth-testkit::dat::plugins::scenario_a_plugin_reads_a_landblock_and_a_dungeon_cell_of_the_game_data",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "plugins.events.state-changes-arrive-in-the-order-the-client-applied-them",
        says: "A plugin hears the login, the entry into the world and then each change of \
               experience in the order the client applied the messages behind them, so a replayed \
               session gives the same events every time.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-EVENTS"),
        station: "dereth-testkit::cpu::plugins::scenario_state_changes_arrive_in_the_order_the_client_applied_them",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.faults.a-failing-or-runaway-plugin-is-suspended-and-the-client-carries-on",
        says: "A plugin whose handlers keep failing, or keep running past their time budget, is \
               stopped each time and then suspended, and the player is told; the rest of the \
               client carries on.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-FAULTS"),
        station: "dereth-testkit::cpu::plugins::scenario_a_failing_or_runaway_plugin_is_suspended",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.files.a-file-is-given-only-once-the-player-has-picked-one",
        says: "The client's question for a file cannot be answered yes until the player has \
               picked a file from its list; the answer names the file picked.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-IMPORT-PICK"),
        station: "dereth-plugin-ui::lib::host_tests::a_question_asking_for_a_line_is_answered_yes_only_with_one_picked",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.files.a-file-is-picked-from-the-list-behind-the-question-in-front",
        says: "A file a plugin asks for is a question listing the files it may be given, one \
               picked and given or the request refused; it is answered by the file picked, and \
               the question put stays in front until it is answered, whatever is asked after it.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-IMPORT-QUESTION"),
        station: "dereth-plugin-host::lib::tests::the_question_put_stays_in_front_until_it_is_answered_and_a_file_is_picked_from_the_list",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.files.a-file-the-player-picks-reaches-the-plugin-and-nothing-else-does",
        says: "A plugin asks for a file and the host asks the player each time, listing the files \
               of the import folder the plugin takes; the file the player gives reaches the \
               plugin, and a refusal fails the request.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-IMPORT"),
        station: "dereth-testkit::cpu::plugins::scenario_a_file_the_player_picks_reaches_the_plugin_and_nothing_else_does",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.files.an-answer-reaches-only-the-question-it-was-given-to",
        says: "Each dialog the host puts for a file has its own id: an answer with any other id, \
               or the same answer given again, answers nothing, and the next plugin's dialog waits \
               its turn.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-QUESTION-ID"),
        station: "dereth-plugin-host::lib::tests::an_answer_reaches_only_the_question_it_was_given_to",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.files.escape-refuses-the-hosts-question",
        says: "Escape refuses the host's dialog for a file: no file is given by a key.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-QUESTION-KEYS"),
        station: "dereth-plugin-ui::lib::host_tests::escape_refuses_the_question",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.files.every-interface-asks-for-a-file-in-a-dialog-of-its-own",
        says: "Under the modern, the classic and the Horizon interface alike, a file a plugin asks \
               for is asked for in a dialog naming the plugin: its give button gives nothing until \
               a file is picked, and its refuse button or Escape fails the request; then the next \
               plugin's dialog is put.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-HOST-QUESTION"),
        station: "dereth-client-shell::lib::front_end::plugin_tests::under_every_interface_a_plugins_file_is_asked_for_in_a_dialog_its_buttons_and_escape_answer",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "plugins.files.the-hosts-dialog-for-a-file-is-drawn-in-the-games-frame-beside-the-plugins",
        says: "On the desktop under the modern interface the host's dialog for a file is drawn in \
               the game's own frame, as a plugin window is, with the plugins' HUDs still drawn \
               beside it, and the player's pick and its first button give the plugin the file.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-FILE-DIALOG-DESKTOP"),
        station: "dereth-client::gpu::ui::plugin_windows::the_hosts_dialog_for_a_file_is_drawn_in_the_games_frame_beside_the_plugins_and_its_buttons_answer",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "plugins.files.the-hosts-question-for-a-file-is-a-dialog-beside-the-plugin-windows",
        says: "The host's dialog for a file is an ordinary dialog of the interface: the plugins' \
               windows are still drawn beside it and take their presses there, and only the \
               dialog's own buttons answer it.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-FILE-DIALOG"),
        station: "dereth-plugin-ui::lib::host_tests::the_hosts_question_is_a_dialog_and_the_plugin_windows_are_still_drawn_beside_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.game.the-spell-book-is-read-whole-or-a-page-at-a-time",
        says: "A plugin reads the character's spell book in id order, whole or a page at a time \
               (from a position counting from 1, so many spells); a page past the end is empty; \
               a spell carries its category and how long it lasts.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-SPELL-BOOK"),
        station: "dereth-testkit::cpu::plugins::scenario_the_spell_book_is_read_whole_or_a_page_at_a_time",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.interfaces.a-plugins-hotkey-and-marks-work-under-every-interface",
        says: "A plugin's hotkey runs its command, and its markers and shapes are drawn in the \
               world, under the modern, the classic and the Horizon interface alike.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-EVERY-INTERFACE"),
        station: "dereth-client-shell::lib::front_end::plugin_tests::a_plugins_hotkey_runs_its_command_and_its_mark_is_drawn_in_the_world_under_every_interface",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "plugins.interfaces.a-plugins-windows-and-huds-are-drawn-under-every-interface",
        says: "A plugin's window, its HUD and the plugin bar are drawn under the modern, the \
               classic and the Horizon interface alike, and a click on the window's button reaches \
               the plugin once under each: Horizon seats the window among its own windows, the \
               others draw it in the game's font and colours over the game.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-WINDOWS-EVERY-INTERFACE"),
        station: "dereth-client-shell::lib::front_end::plugin_tests::a_plugins_window_and_hud_are_drawn_under_every_interface_and_a_click_on_its_button_reaches_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "plugins.interfaces.a-press-on-each-interfaces-map-is-told-to-the-plugins",
        says: "A press on the world map of the modern, the classic and the Horizon interface \
               reaches the plugins listening with the place under the pointer, the place a map \
               marker of theirs is drawn on there, and the button pressed.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-MAP-CLICKS-EVERY-INTERFACE"),
        station: "dereth-client-shell::lib::front_end::plugin_tests::a_press_on_each_interfaces_map_reaches_the_plugins_with_the_place_under_the_pointer",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "plugins.interfaces.switching-interfaces-leaves-no-plugin-surface-behind",
        says: "Switching interfaces with plugins loaded leaves nothing of the last one behind: \
               the question is asked only by the interface shown, the markers stay drawn, a key \
               let go under one interface is not held under the next, and the plugin windows' \
               places among the modern interface's windows go when it does.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-SWITCH-CLEAN"),
        station: "dereth-client-shell::lib::front_end::plugin_tests::switching_interfaces_with_plugins_loaded_leaves_nothing_of_the_last_interface_behind",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "plugins.interfaces.under-classic-a-plugin-window-and-hud-are-drawn-on-the-device",
        says: "Under the classic interface a plugin's window is drawn on the device over the \
               interface's windows, framed and lettered in the classic interface's own art, text \
               and colours, its HUD with it, and a click on its button reaches the plugin.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-WINDOWS-CLASSIC-DRAWN"),
        station: "dereth-client::gpu::ui::plugin_windows::under_the_classic_interface_a_plugin_window_and_hud_are_drawn_in_its_own_art_and_its_button_reaches_the_plugin",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "plugins.interfaces.under-horizon-a-plugin-window-is-drawn-on-the-device-in-its-art",
        says: "Under Horizon a plugin's window is drawn on the device in Horizon's own art, a click \
               on its button reaches the plugin, and once it is closed nothing of it is drawn.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-WINDOWS-HORIZON-DRAWN"),
        station: "dereth-client::gpu::ui::plugin_windows::under_horizon_a_plugin_window_is_drawn_in_its_art_its_button_reaches_the_plugin_and_closed_it_is_gone",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "plugins.interfaces.under-modern-a-plugin-window-stands-among-the-interfaces-windows",
        says: "Under the modern interface a plugin's window stands among the interface's own \
               windows: one of the interface's opened over it is drawn over it and takes the \
               press there, and a press on the plugin's window brings it in front of that one.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-WINDOWS-INTERLEAVED"),
        station: "dereth-client-shell::lib::front_end::plugin_tests::under_modern_a_plugin_window_stands_among_the_interface_s_windows_and_a_press_brings_it_forward",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "plugins.load.a-found-plugin-is-listed-off-until-the-player-ticks-it",
        says: "Every plugin the client finds (in its plugins folder, among the launcher's installs, \
               among the ones that come with it) is listed, by `/plugin list` and in the plugin \
               panel, and off: none is loaded until the player ticks it, by `/plugin on <id>` or \
               the panel's Turn on button, and then it loads as any plugin does. The client \
               remembers which are ticked, so it loads those at its next start and no other, and \
               `/plugin off <id>` unloads one. A plugin of the same id among those named to load \
               is that one.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-FOUND"),
        station: "dereth-testkit::cpu::plugins::scenario_a_found_plugin_is_listed_off_until_the_player_ticks_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.load.a-plugin-that-needs-what-the-host-lacks-is-refused-with-the-reason",
        says: "A plugin that needs something this client does not offer is not loaded, and the \
               player is told what is missing.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-LOAD"),
        station: "dereth-testkit::cpu::plugins::scenario_a_plugin_that_needs_what_the_host_lacks_is_refused",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.load.a-plugin-that-replaces-another-keeps-what-the-player-had-under-it",
        says: "A plugin whose manifest says it replaces another (a plugin renamed to a new id) is \
               turned on where the player had turned the other on, and opens each of its stores \
               from the other's where it has none of its own, keeping it under its own id from \
               then on and leaving the other's as it was.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-REPLACES"),
        station: "dereth-plugin-host::lib::tests::a_plugin_that_replaces_another_is_on_where_it_was_and_keeps_its_stores",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.load.no-plugin-loads-while-plugins-are-off",
        says: "While the player's Plugins (experimental) setting is off, no plugin the client \
               finds loads, ticked or not, and the client shows nothing of the plugins: no bar, no \
               panel, no `/plugin`. Turned on, the ticked ones load; turned off again, they \
               unload. A plugin the command line names to load (`--plugins`) loads whatever the \
               setting says.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-OFF"),
        station: "dereth-plugin-host::lib::tests::with_plugins_off_no_found_plugin_loads_and_the_host_shows_nothing",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.load.plugins-are-off-until-the-player-turns-them-on",
        says: "Plugins (experimental) is a setting of the Client Options page, off at first and \
               in a profile saved before it existed, and kept in the profile once the player turns \
               it on.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-SWITCH"),
        station: "dereth-client-contract::lib::options::plugins::tests::plugins_are_off_at_first_and_in_an_old_profile_and_a_saved_one_brings_them_back_on",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.load.with-no-plugins-the-panel-says-how-to-add-one",
        says: "With no plugin loaded and none that comes with the client, the plugin panel and \
               `/plugin list` say there is none and how to add one: on the desktop, where its \
               plugins folder is and that `--plugins <folder>` names another.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-NONE"),
        station: "dereth-plugin-host::lib::tests::with_no_plugins_the_panel_and_the_list_say_how_to_add_one",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.messages.a-plugin-reads-each-message-decoded-and-can-send-none",
        says: "A plugin hears each message the client sent and received, by its name in the opcode \
               table or as any message, with the fields its decoder reads, and the stream has no \
               way to send.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-MESSAGES"),
        station: "dereth-testkit::cpu::plugins::scenario_a_plugin_reads_each_message_decoded_and_can_send_none",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.nav.a-body-steps-across-a-narrow-gap-between-floors-a-step-apart",
        says: "Where two floors a step apart in height lie a narrow gap apart (two sampling columns \
               or less, under 0.75 m: a crack between a dais and its steps, a riser between treads), \
               the client's planner joins them, standing the body on the higher edge, as a body far \
               wider than the gap steps across it; a wall in the gap still stops it. In the Holtburg \
               Redoubt, a dais 0.92 m above the floor is planned onto by its steps and off again, and \
               a body walks both.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-NAV-NARROW-GAP"),
        station: "dereth-navigation::dat::gaps::the_holtburg_redoubts_dais_is_walked_onto_from_its_floor_and_off_again",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "plugins.nav.a-follow-holds-within-range-until-stopped",
        says: "A follow closes on its object to within range and holds there until it is stopped, which \
               lets the run lock go and fails the follow as stopped.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-NAV-FOLLOW"),
        station: "dereth-testkit::cpu::plugins::scenario_a_follow_holds_within_range_until_stopped",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.nav.a-held-motion-presses-a-movement-control-for-the-time-asked",
        says: "A held motion presses one of the game's movement controls (a step forward or back, \
               a sidestep or a turn), with the walk key when asked, for the time asked and then \
               lets it go; an unknown motion, or one held longer than ten seconds, is refused.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-NAV-MOTION"),
        station: "dereth-testkit::cpu::plugins::scenario_a_held_motion_presses_a_movement_control_for_the_time_asked",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.nav.a-journey-between-two-parts-of-a-dungeon-takes-the-portal-inside-it",
        says: "A journey between two parts of a dungeon that no walk joins goes by the portal \
               linking them, however near the two places are, and with no portal known out of \
               the part the character stands in there is no journey: from a new character's first \
               rooms in the Training Academy, its exit portal is not walked to.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-NAV-DUNGEON-PARTS"),
        station: "dereth-navigation::dat::parts::a_journey_between_parts_of_a_dungeon_takes_its_portal_and_none_leaves_a_part_with_no_way_out",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "plugins.nav.a-journey-needs-its-domain-and-ends-when-stopped-or-interrupted",
        says: "A journey with no gesture behind it needs the world's leave for navigation; \
               stopping ends it as stopped, and the player's own movement as interrupted.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-NAV-JOURNEY-GATE"),
        station: "dereth-testkit::cpu::plugins::scenario_a_journey_needs_its_domain_and_ends_when_stopped_or_interrupted",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.nav.a-journey-out-of-the-academy-hands-jonathans-token-back",
        says: "From a part of the Training Academy no portal leaves (its first rooms, where a new \
               character starts), a journey out plans Jonathan's token handed back as a leg: \
               walk to him, take his Academy Exit Token, give it back, and arrive where the \
               Academy's own exit portal sends, as the server's published table or a use \
               has told; a portal out of the same part is taken before it, from the exit \
               portal's part the portal is taken, and with no way out known there is no \
               journey. The Academy is known by the character-creation table's starts and \
               Jonathan by his name where the world shows him standing.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-NAV-ACADEMY-EXIT"),
        station: "dereth-headless::dat::presentation::navigator_plans_the_academy_exit::a_plan_out_of_a_recorded_academy_hands_jonathans_token_back",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "plugins.nav.a-journey-reaches-a-dungeon-through-the-portal-that-leads-there",
        says: "A journey to a dungeon walks to the portal that leads there, uses it and walks on to the goal; its legs and what the use taught reach the plugin as events, its promise settles on arrival, and the world's store keeps the portal's destination as learned.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-NAV-JOURNEY"),
        station: "dereth-testkit::cpu::plugins::scenario_a_journey_reaches_a_dungeon_through_the_portal_that_leads_there",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.nav.a-journey-sets-out-in-a-session-the-player-entered",
        says: "A journey a plugin asks for in a session the player entered (not only a scripted \
               run) sets out from where the character stands once it is in the world, is carried \
               out and settles; out of the world none is carried out.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-NAV-PLAYER-SESSION"),
        station: "dereth-headless::dat::presentation::navigator_go::the_navigators_go_walks_the_character_to_its_target_in_a_players_own_session",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "plugins.nav.a-journey-to-someone-behind-a-counter-ends-within-reach-of-them",
        says: "A journey to someone no walk reaches, a shopkeeper in a booth with a window onto \
               the street, walks to the place nearest them that a walk reaches, within reach of \
               them, and settles there as arrived.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-NAV-APPROACH"),
        station: "dereth-headless::dat::presentation::navigator_go_to_a_booth::the_navigators_go_to_a_shopkeeper_in_a_booth_walks_to_his_window_and_arrives",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "plugins.nav.a-jump-faces-its-heading-charges-for-the-time-asked-and-leaps-the-way-held",
        says: "A jump turns the character to its heading, presses the jump and charges it for the \
               time asked, holding the walk key and the direction it leaps in, then lets go: the \
               leap; with no direction it goes straight up. The player's own movement while it \
               charges puts it down without a leap, and a charge past the full power bar or an \
               unknown direction is refused.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-NAV-JUMP"),
        station: "dereth-testkit::cpu::plugins::scenario_a_jump_faces_its_heading_charges_for_the_time_asked_and_leaps_the_way_held",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.nav.a-move-turns-toward-the-place-runs-and-stops-within-range",
        says: "A move to a place turns the character toward it, runs with the run lock while it is ahead, \
               and lets the run lock go within range, where its promise settles.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-NAV-MOVE"),
        station: "dereth-testkit::cpu::plugins::scenario_a_move_turns_toward_the_place_runs_and_stops_within_range",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.nav.a-plan-answers-the-journey-without-moving",
        says: "A plan answers a journey's legs and expected seconds without moving the character; with portals left out, a dungeon has no journey.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-NAV-JOURNEY-PLAN"),
        station: "dereth-testkit::cpu::plugins::scenario_a_plan_answers_the_journey_without_moving",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.nav.a-plugins-movement-gives-way-when-the-server-takes-control",
        says: "When the server takes control of the character's movement, a plugin's movement ends and \
               the run lock the host set is let go.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-NAV-SERVER"),
        station: "dereth-testkit::cpu::plugins::scenario_a_plugins_movement_gives_way_when_the_server_takes_control",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.nav.a-refused-portal-is-learned-and-the-journey-planned-again",
        says: "A portal that refuses the character is learned as refused, and the journey is planned again by another way, told with the reason.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-NAV-JOURNEY-REFUSED"),
        station: "dereth-testkit::cpu::plugins::scenario_a_refused_portal_is_learned_and_the_journey_planned_again",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.nav.a-route-into-a-room-sunk-below-the-ground-is-found-and-walked",
        says: "The walking route from Shoushi's street to its healer, in a room sunk below the \
               ground the building stands on, goes down into the room and is walked there: the \
               building's shell bounds the walk only outdoors.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-NAV-SUNKEN-ROOM"),
        station: "dereth-navigation::dat::interiors::a_route_from_shoushis_street_down_into_its_healers_sunken_room_is_found_and_walked",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "plugins.nav.a-route-is-followed-at-any-frame-rate",
        says: "A walking route is followed with the turn and forward keys at 60, 20, 10 or 5 \
               frames a second alike: the character aims a little ahead along it and runs \
               through its bends, cuts a corner only as far as the line across is walkable, \
               counts a waypoint passed by the ground a frame covered, lets a turn key go before \
               it overshoots, walks a leg it stopped on through the places the planner went \
               through, and plans round where it stopped when that fails too. In Holtburg's \
               Training Academy the walks from both courtyard portals, up the hive's ramp, to a \
               waypoint three metres off and into the woodpile arrive, never stuck.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-NAV-FRAME-RATE"),
        station: "dereth-navigation::dat::steering::academy_routes_that_went_wrong_are_driven_to_their_end_at_60_20_10_and_5_frames_a_second",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "plugins.nav.a-route-to-someone-walled-off-ends-as-near-them-as-a-body-walks",
        says: "The walking route to someone whose own place no walk reaches ends at the place \
               nearest them that a walk does reach, within reach of them, and is walked there: \
               from the street or from inside his shop, Holtburg's elder shopkeeper is reached at \
               his window.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-NAV-APPROACH-ROUTE"),
        station: "dereth-navigation::dat::approach::a_route_to_holtburgs_elder_shopkeeper_ends_outside_his_window_from_the_street_or_the_shop",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "plugins.nav.a-vtank-route-is-walked-point-by-point",
        says: "A route read from a VTank navigation file is walked point by point with the plugin's \
               movement, its pauses slept, and a route that runs once ends at its last point.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-NAVFILE"),
        station: "dereth-testkit::cpu::plugins::scenario_a_vtank_route_is_walked_point_by_point",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.nav.a-walking-route-goes-round-walls-and-through-doors-and-a-line-of-sight-stops-at-them",
        says: "Asked without moving, the client answers a walking route round a wall to a monster \
               behind it, one through a closed door in a wall's gap naming the door to open, and \
               none to a place walled in; a line of sight is blocked by a wall and by a closed \
               door and clear across open ground.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-NAV-PATH"),
        station: "dereth-testkit::cpu::plugins::scenario_a_walking_route_goes_round_walls_and_through_doors_and_a_line_of_sight_stops_at_them",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.nav.a-walking-route-on-the-retail-data-goes-round-a-building-and-a-line-of-sight-stops-at-it",
        says: "In a session the player entered, the client's line of sight to a place behind a \
               building is blocked and the one up the open street clear; its walking route there \
               bends round the building, longer than the straight line, and walked corner by \
               corner brings the character there, in sight of it.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-NAV-PATH-RETAIL"),
        station: "dereth-headless::dat::presentation::path_and_sight::a_route_round_a_building_brings_the_character_behind_it_and_sight_stops_at_its_walls",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "plugins.nav.a-way-only-a-drop-leads-on-is-no-route-and-names-the-drop",
        says: "The client's planner only walks: it never takes a fall or a jump by itself. Where \
               no walk reaches a place but a drop off a ledge does (a metre of open air beyond \
               the ledge, nothing to catch the body before the floor below), it answers no \
               route and names the drop: where to stand, where the body lands and how far it \
               falls. In the Black Dominion, below the floor its stairs and ramps reach 48 m \
               down from the portal, it names a ledge six metres high, and a body jumping off \
               there comes down on the floor below.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-NAV-DROP"),
        station: "dereth-navigation::dat::drops::a_route_down_the_black_dominion_past_its_ledge_is_none_and_names_the_drop_a_jump_takes",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "plugins.nav.an-unloaded-or-suspended-plugin-lets-go-of-the-character",
        says: "A plugin unloaded or suspended in the middle of a movement lets go of the character: its \
               movement stops and the run lock it set is let go.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-NAV-RELEASE"),
        station: "dereth-testkit::cpu::plugins::scenario_an_unloaded_or_suspended_plugin_lets_go_of_the_character",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.nav.handing-a-token-back-asks-the-player-first",
        says: "A journey that would hand a token back (leaving the Training Academy for good) asks \
               the player first, in the client's own yes-or-no box that every interface \
               shows, saying the Academy cannot be entered again; a yes carries it out, a \
               no, a box taken down unanswered, or no interface to show it ends the journey \
               as declined. At the person the token is taken (the person used again a while \
               later when it does not come, a few times at most), given back once they have \
               done talking (again while it stays in the pack), and the journey waits to be \
               sent on once it has left the pack.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-NAV-ACADEMY-ASK"),
        station: "dereth-client-runtime::lib::dialogs::tests::a_journeys_question_is_asked_in_a_box_and_its_answer_or_its_loss_goes_back_to_the_journey",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.nav.movement-without-a-gesture-needs-its-domain-and-the-worlds-leave",
        says: "A movement with no gesture behind it needs the world's leave: a world that allows \
               navigation only after the player's click refuses it.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-NAV-AUTO"),
        station: "dereth-testkit::cpu::plugins::scenario_movement_without_a_gesture_needs_its_domain_and_the_worlds_leave",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.nav.planning-never-holds-a-frame",
        says: "Journeys and walking routes are planned off the frame: on a thread of their own, \
               or where there is none a few milliseconds of each frame. Asked for a route through \
               a window of landblocks never built, seconds of work, the client's frames go on \
               (none held 20 ms on a thread) and the route comes some frames later, the same \
               either way; the character stands while it waits.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-NAV-OFF-FRAME"),
        station: "dereth-client-runtime::lib::travel::tests::a_route_through_a_window_of_landblocks_never_built_is_answered_without_holding_a_frame",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "plugins.nav.routes-are-searched-a-patch-of-places-at-a-time",
        says: "The client's planner merges a region's standing places into patches, rectangles \
               of places on one floor that a body walks straight across, and searches a route a \
               patch at a time, place by place only round a door, a creature or an object in \
               the way and round the route's ends. A ledge higher than a step divides patches. \
               In a dungeon of many levels, its stairs and ramps, the patches are a fiftieth as \
               many as the places, and the longest walk across it, over a kilometre, is found \
               and walked by the body.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-NAV-PATCHES"),
        station: "dereth-navigation::dat::patches::a_dungeons_places_merge_into_patches_a_fiftieth_as_many_and_its_longest_walk_is_walked",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "plugins.nav.the-players-own-movement-ends-a-plugins-movement-and-comes-first",
        says: "The player's own movement ends a plugin's movement at once and leaves the run lock to the \
               player; for five seconds after it, a movement no gesture asked for is refused.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-NAV-PLAYER-FIRST"),
        station: "dereth-testkit::cpu::plugins::scenario_the_players_own_movement_ends_a_plugins_movement_and_comes_first",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.net.requests-reach-only-the-listed-domains-over-https",
        says: "A plugin's HTTPS requests go out only when its manifest names net.http and lists \
               the request's domain (its subdomains too), never over plain http; any other request \
               fails its promise saying which is missing, and each settles with the answer or with \
               why it failed.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-NET"),
        station: "dereth-testkit::cpu::plugins::scenario_requests_reach_only_the_listed_domains_over_https",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.objects.a-plugin-hears-objects-change-and-move-and-finds-them-by-their-properties",
        says: "A plugin hears which of an object's properties changed and that it walked more than \
               a metre; reads its 64-bit, float, string, boolean and data-id properties and what \
               the client knows of a creature's health; and finds objects by property conditions, \
               class, nearness and a predicate of its own.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-OBJECTS"),
        station: "dereth-testkit::cpu::plugins::scenario_a_plugin_hears_objects_change_and_move_and_finds_them_by_their_properties",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.overlay.a-journey-is-drawn-until-the-plugin-hides-it",
        says: "The host draws a plugin's journey while it runs, an arrow to where the leg goes, the way on the map and the portals ahead on the map and radar, until the plugin hides it.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-OVERLAY-JOURNEY"),
        station: "dereth-testkit::cpu::plugins::scenario_a_journey_is_drawn_until_the_plugin_hides_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.overlay.a-mark-may-stay-on-top-of-what-stands-in-front-and-nametags-keep-to-the-nearest",
        says: "A shape, a world marker or a nametags layer may ask to stay on top of whatever stands \
               in front of it, and a nametags layer labels only its nearest objects, nearest first \
               and as many as its limit.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-DEPTH"),
        station: "dereth-testkit::cpu::plugins::scenario_a_mark_may_stay_on_top_of_what_stands_in_front_and_nametags_keep_to_the_nearest",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.overlay.a-plugin-draws-lines-rings-squares-text-and-nametags-in-the-world",
        says: "A plugin draws lines, rings, squares and floating text in the world, each resolved \
               every frame to an outline where its object or place is, and the nametags layer \
               labels each object of the kinds it names within its range.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-SHAPES"),
        station: "dereth-testkit::cpu::plugins::scenario_a_plugin_draws_lines_rings_squares_text_and_nametags_in_the_world",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.overlay.a-press-on-the-map-is-heard-where-it-falls",
        says: "A press on the game's map reaches every plugin listening with the place under the \
               pointer, one the markers take, the button and the modifiers held; it is no \
               gesture, a press off the world is not heard, and a host that tells no map presses \
               refuses the listening.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-MAP-CLICKS"),
        station: "dereth-testkit::cpu::plugins::scenario_a_press_on_the_map_is_heard_where_it_falls",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.overlay.markers-are-kept-with-what-they-mark-and-go-when-removed-or-due",
        says: "A plugin's markers (in the world, on the radar, on the map, and one arrow) are kept with the \
               object or place they mark and resolved to where it is each frame; a marker goes \
               when the plugin removes it or its time is up, and the arrow's turn and distance \
               are from the character.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-OVERLAYS"),
        station: "dereth-testkit::cpu::plugins::scenario_markers_are_kept_with_what_they_mark_and_go_when_removed_or_due",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.packages.a-compiled-plugin-counts-what-the-same-lua-plugin-counts",
        says: "A plugin written in Rust and compiled to a WebAssembly component (the XP Meter, as \
               the test suite builds it) runs in the same host as the Lua one, through the same \
               operations: loaded together over a recorded session, each answers its command \
               with the same line and shows the same total.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-WASM"),
        station: "dereth-testkit::cpu::plugins::scenario_a_compiled_plugin_counts_what_the_same_lua_plugin_counts",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.packages.a-package-loads-whatever-signature-file-it-carries",
        says: "A package loads by its manifest and its files alone: a signature file in it is not \
               read, whatever it holds.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-UNSIGNED"),
        station: "dereth-testkit::cpu::plugins::scenario_a_package_loads_whatever_signature_file_it_carries",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.peers.a-plugin-hears-its-copies-in-the-other-clients",
        says: "A message a plugin sends reaches the same plugin in every other client on the machine, with \
               the sending character, and not the sender; a world may cap how many clients it hears.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-PEERS"),
        station: "dereth-testkit::cpu::plugins::scenario_a_plugin_hears_its_copies_in_the_other_clients",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.permissions.a-plugin-loads-at-once-and-may-do-what-every-permission-covers",
        says: "A plugin loads at once with nothing asked of the player, may do what every \
               permission covers whether its manifest names it or not, and is listed with the \
               permissions its manifest names.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-NO-PROMPTS"),
        station: "dereth-plugin-host::lib::tests::a_plugin_naming_permissions_loads_at_once_with_nothing_asked_and_may_do_all_of_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.permissions.a-plugin-that-names-no-permission-may-do-what-each-covers",
        says: "A plugin whose manifest names no permission is asked nothing and may do what every \
               permission but net.http covers, and in a world that publishes no policy it acts \
               without a gesture in every domain.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-UNASKED"),
        station: "dereth-testkit::cpu::plugins::scenario_a_plugin_that_names_no_permission_may_do_what_each_covers",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.policy.a-published-policy-sets-each-domains-rule-and-denies-the-rest",
        says: "A world's published policy document sets the rule of each automation domain and of session \
               control, denies what it leaves out, and is what plugins read and hear.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-POLICY"),
        station: "dereth-testkit::cpu::plugins::scenario_a_published_policy_sets_each_domains_rule_and_denies_the_rest",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.policy.a-world-with-no-policy-allows-every-kind-of-automation",
        says: "A world that publishes no policy allows every automation domain and session \
               control, with or without a gesture, while a published policy denies what it does \
               not name.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-POLICY-NONE"),
        station: "dereth-testkit::cpu::plugins::scenario_a_world_with_no_policy_allows_every_kind_of_automation",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.pregame.a-plugin-is-active-only-in-the-parts-of-the-session-it-names",
        says: "A plugin is active only in the contexts its manifest names (the world by default, the \
               character screens, character creation): elsewhere it hears only the session's comings \
               and goings, its timers wait and its windows are hidden, and it hears each context \
               begin and end.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-CONTEXTS"),
        station: "dereth-testkit::cpu::plugins::scenario_a_plugin_is_active_only_in_the_parts_of_the_session_it_names",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.pregame.a-plugin-picks-a-character-and-retries-a-refused-entry",
        says: "A plugin loaded before login hears the character list, enters the world as the character it \
               picks, hears why the server refused an entry, and may try again.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-PREGAME"),
        station: "dereth-testkit::cpu::plugins::scenario_a_plugin_picks_a_character_and_retries_a_refused_entry",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.pregame.a-world-that-denies-session-control-refuses-a-plugins-entry",
        says: "In a world whose policy denies session control, a plugin cannot enter the world for the player.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-PREGAME-DENIED"),
        station: "dereth-testkit::cpu::plugins::scenario_a_world_that_denies_session_control_refuses_a_plugins_entry",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.auto-login-enters-the-chosen-character-and-retries",
        says: "The Auto-login sample logs the chosen character in when the character list comes up, tries \
               again while the server says it is still in the world, and types the listed \
               commands after login.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-SAMPLE-AUTOLOGIN"),
        station: "dereth-testkit::cpu::plugins::scenario_auto_login_enters_the_chosen_character_and_retries_while_it_is_still_in_the_world",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.auto-logins-window-chooses-the-character-and-orders-the-lines-typed-after-login",
        says: "The Auto-login sample's window offers the account's characters at the character \
               list; choosing one turns logging in on, as its command does; the lines it types \
               after login are added, removed and put in order there and typed in that order; and \
               its button logs the next character in.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-WINDOW-AUTOLOGIN"),
        station: "dereth-testkit::cpu::plugins::windows::scenario_auto_logins_window_chooses_the_character_and_orders_the_lines_typed_after_login",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.follow-brings-another-client-to-the-leader",
        says: "The Follow sample, leading in one client and following in another on the same machine, \
               brings the follower within range of the leader as it moves.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-SAMPLE-FOLLOW"),
        station: "dereth-testkit::cpu::plugins::scenario_follow_brings_another_client_to_the_leader",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.follows-window-lists-the-other-clients-to-follow-and-sends-a-line-to-the-followers",
        says: "The Follow sample's window lists the player's other clients on the machine; it \
               leads, follows the client chosen (which closes on the leader as it moves) and \
               stops by its buttons, says what it is doing, and sends a line from the leader for \
               every follower to type.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-WINDOW-FOLLOW"),
        station: "dereth-testkit::cpu::plugins::windows::scenario_follows_window_lists_the_other_clients_to_follow_and_sends_a_line_to_the_followers",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.id-notices-watches-for-names-and-unknown-scrolls-and-alerts-as-set",
        says: "ID Notices notices the names it watches for (part of a name, or the whole of one in \
               quotes) on items in an opened container and on creatures coming into view, and \
               scrolls of spells the character does not know from a level up, each kind under its \
               own alert: chat, a toast as well, or off.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-IDNOTICES-ALERTS"),
        station: "dereth-testkit::cpu::plugins::windows::scenario_id_notices_watches_for_names_and_unknown_scrolls_and_alerts_as_set",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.id-notices-window-sets-the-value-and-the-spell-words-that-matter",
        says: "The ID Notices sample's settings window sets the value an item must be worth for a \
               notice and the spell words that make one, as its command does; an item worth more \
               is noticed and one worth less is not.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-WINDOW-IDNOTICES"),
        station: "dereth-testkit::cpu::plugins::windows::scenario_id_notices_window_sets_the_value_and_the_spell_words_that_matter",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.tanks-in-one-fellow-on-a-machine-heal-each-other-and-share-their-debuffs",
        says: "Tanks in two clients on one machine in one fellow of their own (PeerFellow, VI2's \
               fellow) heal each other from the vitals each tells over the machine's bus, with no \
               fellowship between them; a vulnerability one has put on a monster the other does not \
               cast again, and fights that monster before a nearer one; a Tank in no fellow of its \
               own hears none of it.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-PEER-FELLOW"),
        station: "dereth-testkit::cpu::plugins::tank_fellows::scenario_tanks_in_one_fellow_on_a_machine_heal_each_other_and_share_their_debuffs",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-buffbot-answers-tells-and-buffs-each-player-in-turn",
        says: "The Buffbot sample, in a world that denies combat and allows support, answers help and \
               list tells and buffs each player who tells it a profile's name in turn: each family of \
               the profile at the highest level it knows, the player told its place in line, and Done \
               when its buffs are cast.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-BUFFBOT-TELLS"),
        station: "dereth-testkit::cpu::plugins::buffbot::scenario_the_buffbot_answers_tells_and_buffs_each_player_in_turn",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-buffbot-buys-its-components-again-when-low-and-goes-back-to-its-post",
        says: "What the Buffbot sample's Restock setting lists (an item, how many to keep, and the \
               vendor that sells it) is bought again when the character holds under RestockAt percent \
               of it: between players, with the line empty or with an item run out, it walks to each \
               vendor it needs, buys up to the counts its pyreals pay for, and walks back to its post \
               (where it was first turned on); a request made while it is away is told to wait in \
               line and is buffed once it is back, and a character moved off its post walks back to \
               it.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-BUFFBOT-RESTOCK"),
        station: "dereth-testkit::cpu::plugins::buffbot::scenario_the_buffbot_buys_its_components_again_when_low_and_goes_back_to_its_post",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-buffbot-caps-each-players-requests-and-its-line",
        says: "The Buffbot sample takes at most its cap of requests from a player in an hour and at \
               most its longest line, tells a player already waiting its place again, and lets a \
               player leave the line with cancel.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-BUFFBOT-CAP"),
        station: "dereth-testkit::cpu::plugins::buffbot::scenario_the_buffbot_caps_each_players_requests_and_its_line",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-buffbot-casts-a-fizzle-again-and-waits-for-a-player-out-of-range",
        says: "The Buffbot sample casts a fizzled spell again, three tries at most, and asks a player \
               further than its range to come closer, buffing it once it is near and skipping it when \
               it does not come within the wait.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-BUFFBOT-RANGE"),
        station: "dereth-testkit::cpu::plugins::buffbot::scenario_the_buffbot_casts_a_fizzle_again_and_waits_for_a_player_out_of_range",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-buffbot-casts-its-own-buffs-by-a-self-profile-and-keeps-them-up",
        says: "The Buffbot sample casts the character's own buffs by a self profile, on the \
               character and on the weapon and armor it wields and wears, each at the best level \
               it casts reliably and only when it runs low, and with SelfBuff on keeps them up \
               between players.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-BUFFBOT-SELF"),
        station: "dereth-testkit::cpu::plugins::buffbot::scenario_the_buffbot_casts_its_own_buffs_by_a_self_profile_and_keeps_them_up",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-buffbot-gets-its-mana-back-and-warns-of-low-components",
        says: "The Buffbot sample gets its mana back between players when it runs low (Stamina to \
               Mana, a mana potion, a rest), telling the player waiting, and warns once of each \
               component it is using up and has few of left.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-BUFFBOT-MANA"),
        station: "dereth-testkit::cpu::plugins::buffbot::scenario_the_buffbot_gets_its_mana_back_and_warns_of_low_components",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-chat-filter-takes-up-what-the-chat-logger-kept",
        says: "The Chat Filter takes the place of the Chat Logger: the logger's filter words hide \
               lines as rules of the filter's own, its log is the filter's, and a rule keeping \
               every line keeps on what the logger did.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-CHATFILTER-CARRIED"),
        station: "dereth-testkit::cpu::plugins::windows::scenario_the_chat_filter_takes_up_what_the_chat_logger_kept",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-chat-filters-rules-hide-highlight-and-log-lines-and-its-window-edits-them",
        says: "The Chat Filter sample's rules match a line by its words, its channel or its \
               sender, and hide it, highlight it (shown again in the rule's style) or keep it in \
               its log; its window adds, changes, removes and switches the rules, as its command \
               does, and lists what the log kept.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-WINDOW-CHATFILTER"),
        station: "dereth-testkit::cpu::plugins::windows::scenario_the_chat_filters_rules_hide_highlight_and_log_lines_and_its_window_edits_them",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-inventory-logs-each-character-and-finds-items-across-them",
        says: "The Inventory sample logs what each character holds and wears for the world, and \
               another character finds those items by words of their names, spells or sets, reads \
               a logged item's line, and is told what an item it selected is when its appraisal \
               comes.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-INVENTORY-LOG"),
        station: "dereth-testkit::cpu::plugins::inventory::scenario_the_inventory_logs_each_character_and_finds_items_across_them",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-inventory-stacks-packs-puts-gear-on-again-and-trades-by-a-vendors-profile",
        says: "The Inventory sample merges partial stacks, puts items in the packs its rules name, \
               puts a saved set of gear on again, and at a vendor sells and buys by the vendor's \
               own loot profile.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-INVENTORY-TOOLS"),
        station: "dereth-testkit::cpu::plugins::inventory::scenario_the_inventory_stacks_packs_puts_gear_on_again_and_trades_by_a_vendors_profile",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-looter-keeps-what-its-profile-keeps-and-id-notices-reports-it",
        says: "The Looter sample, clicked with a corpse selected, opens it, keeps what its loot \
               profile keeps and leaves the rest, while the ID Notices sample appraises what the \
               corpse held and reports the valuable item in the chat window.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-SAMPLES"),
        station: "dereth-testkit::cpu::plugins::scenario_the_looter_keeps_what_its_profile_keeps_and_id_notices_reports_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-looter-waits-while-the-tank-runs",
        says: "With Auto ticked the Looter sample loots each corpse the player opens, but while the \
               Tank sample runs (its running service says so) it leaves the corpse to the Tank and \
               says why in its window; a click on Loot still loots, and Auto loots again once the \
               Tank stops.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-LOOTER-TANK"),
        station: "dereth-testkit::cpu::plugins::windows::scenario_the_looter_waits_while_the_tank_runs",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-looters-window-picks-a-profile-shows-its-rules-and-counts-what-it-kept",
        says: "The Looter sample's window chooses its loot profile among those in its data folder \
               and the one it comes with, as its command does, shows the chosen profile's rules in \
               plain words, and counts what it kept.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-WINDOW-LOOTER"),
        station: "dereth-testkit::cpu::plugins::windows::scenario_the_looters_window_picks_a_profile_shows_its_rules_and_counts_what_it_kept",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-navigator-finds-a-place-as-it-is-typed-and-goes-there",
        says: "The Navigator sample's Where to? box lists places as they are typed (by name, by \
               type, coordinates); a place picked shows where it is, Point puts the arrow on it and \
               Go travels there, its status line telling the journey; saved places and spots \
               clicked on the map have tabs of their own, and Stop stops.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-WINDOW-NAVIGATOR"),
        station: "dereth-testkit::cpu::plugins::windows::scenario_the_navigator_finds_a_place_as_it_is_typed_and_goes_there",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-navigator-finds-each-way-to-a-place-and-goes-the-one-picked",
        says: "The Navigator sample asks the client for each way to its target (the best one, then \
               without recalls, without portals and on foot, a way the same as one before said \
               once), lists them with the legs of the one picked, and going the way picked tells \
               each leg as it begins until it arrives.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-NAVIGATOR-ROUTES"),
        station: "dereth-testkit::cpu::plugins::navigator::scenario_the_navigator_finds_each_way_to_a_place_and_goes_the_one_picked",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-navigator-finds-places-by-name-and-type-and-points-at-the-nearest",
        says: "The Navigator sample finds places by a part of their name and by type, nearest \
               first: the towns of its list, the portals the client knows and the dungeons they \
               lead into, and the lifestones and portals the character has seen; it points at the \
               nearest of a type, and goes into a dungeon through its portal.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-NAVIGATOR-PLACES"),
        station: "dereth-testkit::cpu::plugins::navigator::scenario_the_navigator_finds_places_by_name_and_type_and_points_at_the_nearest",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-navigator-keeps-the-players-own-places-outdoors-and-in-a-dungeon",
        says: "The Navigator sample keeps the places the player saves by name for the world, \
               outdoors by their coordinates and in a dungeon by their cell, points at one from \
               elsewhere, says where the character is, and forgets one when told.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-NAVIGATOR-SAVED"),
        station: "dereth-testkit::cpu::plugins::navigator::scenario_the_navigator_keeps_the_players_own_places_outdoors_and_in_a_dungeon",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-navigator-lists-coordinates-seen-in-chat-and-points-at-the-one-picked",
        says: "The Navigator sample lists the coordinates heard in chat, newest first with who \
               said them, and points its arrow at the one the player picks, or at the newest when \
               told.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-NAVIGATOR-CHAT"),
        station: "dereth-testkit::cpu::plugins::navigator::scenario_the_navigator_lists_coordinates_seen_in_chat_and_points_at_the_one_picked",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-navigator-maps-the-dungeon-floor-around-the-character",
        says: "The Navigator sample, in a dungeon with the game's data, reads the dungeon's rooms \
               a few at a time and draws on its Map page the outline of the floors at the \
               character's level around it, with the character's dot, within a canvas's limit.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-NAVIGATOR-MAP"),
        station: "dereth-testkit::dat::plugins::scenario_the_navigator_maps_the_dungeon_floor_around_the_character",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "plugins.samples.the-navigator-points-at-a-place-and-walks-there",
        says: "The Navigator sample points at coordinates typed to it with an arrow, a world marker and a \
               map marker, shows the distance and direction on its HUD, and walks there when \
               asked.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-SAMPLE-NAVIGATOR"),
        station: "dereth-testkit::cpu::plugins::scenario_the_navigator_points_at_a_place_and_walks_there",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-navigator-points-where-the-player-clicks-on-the-map",
        says: "A left click on the game's map points the Navigator sample's arrow at the place \
               clicked, with its markers there, as GoArrow's click on its map did; other buttons \
               leave the target as it was.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-NAVIGATOR-MAP-CLICK"),
        station: "dereth-testkit::cpu::plugins::navigator::scenario_the_navigator_points_where_the_player_clicks_on_the_map",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-navigators-arrow-follows-the-selected-object-and-its-hud-shows-where-the-character-is",
        says: "The Navigator sample's HUD shows the character's own coordinates, and its arrow, \
               put on the selected object, follows the object as it moves, the HUD's distance with \
               it.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-NAVIGATOR-FOLLOW"),
        station: "dereth-testkit::cpu::plugins::navigator::scenario_the_navigators_arrow_follows_the_selected_object_and_its_hud_shows_where_the_character_is",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-suit-builder-finds-the-best-suit-across-characters-with-its-locks",
        says: "The Inventory sample's suit builder finds, from every character's logged gear, the \
               best suit for the character playing by the spells and set asked for, leaving out \
               what it may not use and keeping the pieces locked, and puts on the pieces the \
               character holds.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-INVENTORY-SUITS"),
        station: "dereth-testkit::cpu::plugins::inventory::scenario_the_suit_builder_finds_the_best_suit_across_characters_with_its_locks",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-answers-vtanks-information-and-log-commands",
        says: "The Tank answers VTank's information commands: the selected item against the loot \
               profile and its properties, the selected monster against the monster list, a spell \
               against the casting thresholds, that it summons no pets, the monster variables, \
               the game's tables written to its data folder; and its log's types switch off and \
               on.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-INFO-COMMANDS"),
        station: "dereth-testkit::cpu::plugins::tank::scenario_the_tank_answers_vtanks_information_and_log_commands",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-answers-vtanks-meta-and-profile-commands",
        says: "The Tank answers VTank's meta and profile commands: the meta's state set, a death \
               faked, an expression run and its variables listed, a line echoed, options read \
               back (one it has not got kept for the run), another meta loaded in place of the \
               running one and saved, the loot profile switched and saved, and settings saved as \
               a profile and as one of the character's own that reads back.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-META-COMMANDS"),
        station: "dereth-testkit::cpu::plugins::tank::scenario_the_tank_answers_vtanks_meta_and_profile_commands",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-answers-vtanks-route-and-jump-commands",
        says: "The Tank answers VTank's route and jump commands as a meta or the player types \
               them: a point and a jump waypoint added, the route saved as a VTank route and run \
               reversed (its points in the order made), a jump made with its heading, walk key, \
               charge and sidestep.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-VT-COMMANDS"),
        station: "dereth-testkit::cpu::plugins::scenario_the_tank_answers_vtanks_route_and_jump_commands",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-asks-its-buffbot-for-buffs-when-they-run-low-and-waits-for-them",
        says: "With no buffs from its buffbot yet, or one of them under BuffbotMinutes or gone, the \
               Tank sample's running macro goes to the buffbot (a journey when it is far), tells it \
               the request, waits while it is told its place in line, keeps the buffs that land until \
               the buffbot says Done, and goes back to where it left.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-BUFFBOT"),
        station: "dereth-testkit::cpu::plugins::tank_buffbot::scenario_the_tank_asks_its_buffbot_for_buffs_when_they_run_low_and_waits_for_them",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-buffs-at-the-level-its-skill-reaches-and-climbs-as-it-rises",
        says: "The Tank casts each buff at the highest level its buffed skill casts reliably (the \
               spell's difficulty and SpellDiffExcessThreshold-Buff more), climbs to a higher \
               level once the skill reaches it, and with too little mana for a buff casts Stamina \
               to Mana first.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-PYRAMID"),
        station: "dereth-testkit::cpu::plugins::scenario_the_tank_buffs_at_the_level_its_skill_reaches_and_climbs_as_it_rises",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-casts-a-buff-again-until-its-effect-shows",
        says: "The Tank's buff step is done only once the buff's effect shows on its target: a \
               fizzle or a cast that ends with nothing to show casts the same spell again as soon \
               as it can be cast, never the next buff; a refusal casting again cannot mend skips \
               it, said once.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-STRICT-BUFFS"),
        station: "dereth-testkit::cpu::plugins::scenario_the_tank_casts_a_buff_again_until_its_effect_shows",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-casts-random-protections-on-players-nearby",
        says: "With VTank's RandomHelperBuffs on and nothing to fight, the Tank casts a \
               protection it knows on a player nearby every RandomHelperIntervalSeconds; with it \
               off, none.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-HELPER-BUFFS"),
        station: "dereth-testkit::cpu::plugins::tank::scenario_the_tank_casts_random_protections_on_players_nearby",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-casts-the-protections-and-banes-its-buff-profiles-name-and-uses-buff-items",
        says: "VTank's buff profiles choose the protections and banes the Tank casts (Custom letters SF: \
               blade and fire only; banes B: the bludgeon bane only), a buff added to the \
               default set is cast, and a buff item the Consumables name is used for a buff \
               the character does not cast itself.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-BUFF-PROFILES"),
        station: "dereth-testkit::cpu::plugins::tank_items::scenario_the_tank_casts_the_protections_and_banes_its_buff_profiles_name_and_uses_buff_items",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-casts-void-curses-and-damage-over-time-and-drains-and-harms",
        says: "The Tank casts a row's void curses and damage over time: a curse again \
               DebuffPrecastSeconds before it runs out, damage over time only once it has run \
               its course; a row set to Drain Auto drains the monster's health, or its mana when \
               the character's is under its top-off mark; one set to Harm casts the harm spell.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-VOID"),
        station: "dereth-testkit::cpu::plugins::tank::scenario_the_tank_casts_void_curses_and_damage_over_time_and_drains_and_harms",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-chooses-its-next-target-itself-when-a-fallen-monsters-body-goes-mid-swing",
        says: "A swing the Tank sent at a monster that has just fallen, still charging when its \
               body goes and the client selects the next monster, is dropped, not sent at that \
               one; the Tank chooses its next target by its own rules and walks within the \
               attack distance of it before swinging, so no charge goes across the room \
               (\"You charged too far!\").",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-RETARGET"),
        station: "dereth-testkit::cpu::plugins::tank::scenario_the_tank_chooses_its_next_target_itself_when_a_fallen_monsters_body_goes_mid_swing",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-confirms-an-item-buff-by-the-items-appraisal",
        says: "The Tank's buff on an item it wears or wields is done only once the item's \
               appraisal lists the buff's enchantment: a fizzle casts it again at once, a cast \
               that ends with nothing in the appraisal casts it again, and once it shows the \
               next item's buff is cast; where the weapon buffs are auras, the one for the \
               character's caster or weapon is cast on the character and confirmed there.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-ITEM-BUFFS"),
        station: "dereth-testkit::cpu::plugins::scenario_the_tank_confirms_an_item_buff_by_the_items_appraisal",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-debuffs-each-monster-first-with-its-extra-vuln-and-recasts-before-they-end",
        says: "With VTank's DebuffEachFirst at Priority the Tank debuffs every monster of its \
               target's priority in reach before its first attack spell; a monster row's extra \
               vulnerability is cast with its debuffs; and a debuff is cast again \
               DebuffPrecastSeconds before its spell's duration ends.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-DEBUFF-FIRST"),
        station: "dereth-testkit::cpu::plugins::tank::scenario_the_tank_debuffs_each_monster_first_with_its_extra_vuln_and_recasts_before_they_end",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-debuffs-with-a-wand-before-melee-and-holds-a-shield-beside-its-sword",
        says: "With VTank's SwitchWandsToDebuff the Tank takes up a wand to debuff a monster its \
               row fights with a sword, then the sword again, and the row's offhand shield \
               beside it; without the setting the sword fights at once.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-HYBRID"),
        station: "dereth-testkit::cpu::plugins::tank::scenario_the_tank_debuffs_with_a_wand_before_melee_and_holds_a_shield_beside_its_sword",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-dispels-a-heavy-vulnerability-with-an-item-or-its-spell",
        says: "With a vulnerability of level 7 on the character, VTank's UseDispelItems has the \
               Tank use a Gem of Stillness and its CastDispelSelf cast Eradicate Life Magic \
               Self; a level 6 vulnerability is left.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-DISPEL"),
        station: "dereth-testkit::cpu::plugins::tank::scenario_the_tank_dispels_a_heavy_vulnerability_with_an_item_or_its_spell",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-fights-a-monster-in-sight-before-one-behind-a-wall",
        says: "Of two monsters of one priority, the nearer behind a wall that a bolt would hit, \
               the Tank fights the one in a straight line of sight first, as VTank's wall \
               detection chooses another target.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-WALLS"),
        station: "dereth-testkit::cpu::plugins::tank::scenario_the_tank_fights_a_monster_in_sight_before_one_behind_a_wall",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-fights-by-its-monster-list-with-debuffs-and-spells",
        says: "The Tank sample leaves alone a monster its list says to, fights the one of highest \
               priority first, and with a caster held casts the debuffs the list asks for on it \
               before its attack spell.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-MONSTERS"),
        station: "dereth-testkit::cpu::plugins::scenario_the_tank_fights_by_its_monster_list_with_debuffs_and_spells",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-fights-loots-its-kill-and-walks-its-route",
        says: "The Tank sample, started in a world that allows it, closes on a monster in range and \
               attacks it, counts the kill when its corpse appears, opens the corpse and keeps what \
               its loot profile keeps, then walks its route.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-SAMPLE-TANK"),
        station: "dereth-testkit::cpu::plugins::scenario_the_tank_fights_loots_its_kill_and_walks_its_route",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-follows-its-leader-straight-or-around-corners-by-a-follow-route",
        says: "VTank's follow route in the Tank follows the character it names, from a route file, \
               from the selection (VTank's FC, /tank follow) or embedded in a meta: held within \
               NavCloseStopRange of it, straight at it, or with FollowAroundCorners along the \
               places it passed, round a wall a straight walk stops at; saved, it is VTank's follow \
               route file.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-FOLLOW"),
        station: "dereth-testkit::cpu::plugins::tank_fellows::scenario_the_tank_follows_its_leader_straight_or_around_corners_by_a_follow_route",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-force-buffs-every-buff-once-held-by-a-pause-and-ended-by-a-cancel",
        says: "VTank's Force Buff in the Tank casts every buff again at once, each once; a pause \
               holds the sequence and the resumed run carries it on without casting again what it \
               cast; a cancel ends it; with the macro stopped, a run for the buffs alone casts \
               them and stops.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-FORCE-BUFF"),
        station: "dereth-testkit::cpu::plugins::tank::scenario_the_tank_force_buffs_every_buff_once_held_by_a_pause_and_ended_by_a_cancel",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-gives-up-on-a-buffbot-that-is-absent-or-silent-and-carries-on",
        says: "The Tank sample asks a buffbot that does not answer three times, half a minute apart, \
               and waits a minute for one not where it was; then it goes back and carries on, asking \
               again only after BuffbotRetryMinutes.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-BUFFBOT-ABSENT"),
        station: "dereth-testkit::cpu::plugins::tank_buffbot::scenario_the_tank_gives_up_on_a_buffbot_that_is_absent_or_silent_and_carries_on",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-goes-back-to-its-corpse-after-a-death-and-takes-everything-again",
        says: "After a death the Tank sample travels back toward where the character fell, walks the \
               last of the way in the corpse's landblock stopping to fight what comes near, opens the \
               corpse of its own name and takes back everything in it, then carries on; a death the \
               game says left nothing is not gone back for, and three deaths in a row on the way back \
               give the recovery up.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-CORPSE"),
        station: "dereth-testkit::cpu::plugins::tank_buffbot::scenario_the_tank_goes_back_to_its_corpse_after_a_death_and_takes_everything_again",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-goes-somewhere-by-a-journey-with-the-macro-stopped",
        says: "The Tank's go (its own command, and the Run page's Go beside Hunt) stops the macro \
               running and travels to the place by the client's journey, through portals, by \
               recalls or by a token handed back, saying when it arrives; it does not hunt \
               there.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-GO"),
        station: "dereth-testkit::cpu::plugins::tank_quests::scenario_the_tank_goes_somewhere_by_a_journey_with_the_macro_stopped",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-goes-to-its-vendor-when-its-pack-is-full-or-a-meta-says-so",
        says: "With OnFullPack vendor a full pack sends the running Tank to its vendor and back, and it \
               hunts on; a pack still full after the vendor stops it; a meta's chat action \
               typing /tank vendor sends it too.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-VENDOR-TRIGGERS"),
        station: "dereth-testkit::cpu::plugins::tank_items::scenario_the_tank_goes_to_its_vendor_when_its_pack_is_full_or_a_meta_says_so",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-hands-an-item-over-picks-one-up-and-uses-one-on-something-before-its-route-goes-on",
        says: "The Tank's quest steps, its own (VTank gives and uses items only through its meta \
               expressions): typed by a route's chat waypoint, a meta's chat action or the \
               player, it walks to the nearest person of the name given and hands them the \
               item from the pack, giving again a while later while it stays there; walks \
               to the nearest item of the name lying on the ground and picks it up; uses an \
               item from the pack on the nearest object of the name, or on an item it \
               holds. A route waits for each before it goes on. The Route page makes each \
               as a waypoint from the item named and the selection, or does it at once.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-QUEST-STEPS"),
        station: "dereth-testkit::cpu::plugins::tank_quests::scenario_the_tank_hands_an_item_over_picks_one_up_and_uses_one_on_something_before_its_route_goes_on",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-heals-restams-and-gives-mana-to-fellows-within-reach-and-heals-with-a-healers-heart",
        says: "With VTank's DoHelp the Tank heals, gives stamina and gives mana to a fellow under \
               the helper marks (Recharge-Helper-HitP, -Stam, -Mana) and within the helper \
               distances, by Heal Other, Revitalize Other and Mana Boost Other, and leaves a fellow \
               beyond them; with UseHealersHeart a Healer's Heart in hand is used on the fellow, \
               its own heal cast, the heart kept in hand.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-HELP"),
        station: "dereth-testkit::cpu::plugins::tank_fellows::scenario_the_tank_heals_restams_and_gives_mana_to_fellows_within_reach_and_heals_with_a_healers_heart",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-is-driven-by-a-vtank-meta",
        says: "The Tank sample runs a VTank meta on top of itself: the meta's options switch the \
               macro's parts (combat off, it fights nothing), the VTank commands it types load a \
               route the macro walks, and its states move on as VTank's do.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-METAS"),
        station: "dereth-testkit::cpu::plugins::scenario_the_tank_is_driven_by_a_vtank_meta",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-keeps-a-keep-up-to-rules-items-until-the-pack-holds-its-count",
        says: "A loot profile's keep-up-to rule has the Tank take the rule's items from a corpse \
               until the pack holds the rule's count, counting what it held before, and leave the \
               rest.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-KEEP-UP-TO"),
        station: "dereth-testkit::cpu::plugins::tank::scenario_the_tank_keeps_a_keep_up_to_rules_items_until_the_pack_holds_its_count",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-keeps-its-buffs-up",
        says: "The Tank sample, with nothing to fight, takes up its caster and enters magic stance, \
               and casts on itself each attribute and protection buff it knows (the best level), \
               once, each only after the one before it shows.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-BUFFS"),
        station: "dereth-testkit::cpu::plugins::scenario_the_tank_keeps_its_buffs_up",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-leaves-a-ghost-monster-alone-for-good",
        says: "The Tank leaves alone for good a ghost (a monster the client shows that the server \
               does not have): one whose health does not change in GhostDeleteHPTrackerSeconds of \
               fighting it with the health tracker on, one whose casts never begin \
               GhostMonsterSpellAttemptCount times running, and one the player marks with VTank's \
               /vt deletemonster.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-GHOSTS"),
        station: "dereth-testkit::cpu::plugins::tank::scenario_the_tank_leaves_a_ghost_monster_alone_for_good",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-leaves-out-excluded-buffs-scarabs-and-items-and-rebuffs-together",
        says: "The Tank never casts a buff the player excluded, nor a spell of an excluded \
               scarab's level (a lower level of the buff instead), nor buffs an item listed as \
               never buffed; and after a buff it recasts the others BuffCastRecast_Seconds \
               earlier for BuffCastRecastReset_Seconds, those cast meanwhile excepted.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-EXCLUSIONS"),
        station: "dereth-testkit::cpu::plugins::tank::scenario_the_tank_leaves_out_excluded_buffs_scarabs_and_items_and_rebuffs_together",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-loots-fellows-and-others-corpses-and-only-rare-ones-when-told",
        says: "By a corpse's appraisal (\"Killed by ...\"), VTank's LootFellowCorpses has the Tank loot a \
               fellow's kill and not a stranger's, LootAllCorpses a stranger's too, and \
               LootOnlyRareCorpses only a corpse that generated a rare.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-OTHERS-CORPSES"),
        station: "dereth-testkit::cpu::plugins::tank_items::scenario_the_tank_loots_fellows_and_others_corpses_and_only_rare_ones_when_told",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-makes-a-route-in-the-game-and-walks-it-jumping-and-through-its-portal",
        says: "The Tank makes a route in the game as VTank's Route tab makes one (a point where \
               the character stands, a jump with its heading, walk key and charge, the selected \
               portal), saves it as a VTank route file that reads back, and walks it: at the jump \
               waypoint it jumps, at the portal waypoint it uses the portal and goes on once \
               through.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-ROUTES"),
        station: "dereth-testkit::cpu::plugins::scenario_the_tank_makes_a_route_in_the_game_and_walks_it_jumping_and_through_its_portal",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-opens-a-closed-door-across-its-route-before-walking-it",
        says: "Before the Tank sample walks a leg of its route, a closed door standing across it is \
               used and waited for until it stands open; the leg is then walked with no stuck \
               recovery.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-DOOR-OPENED"),
        station: "dereth-testkit::cpu::plugins::windows::scenario_the_tank_opens_a_closed_door_across_its_route_before_walking_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-pauses-a-domain-it-may-not-automate-and-says-why-once",
        says: "The Tank sample pauses an automation domain the world does not allow without a \
               gesture once the player's own start no longer covers it, says once why, and resumes \
               when allowed, with no error repeating.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-DOMAINS"),
        station: "dereth-testkit::cpu::plugins::scenario_the_tank_pauses_a_domain_it_may_not_automate_and_says_why_once",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-pauses-and-resumes-its-run-where-it-was",
        says: "The Tank's pause (VTank's Run Macro switch) stops it doing anything, the plugin \
               bar showing it paused, and starting it again resumes the same run, its tally kept, \
               rather than a new one.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-PAUSE"),
        station: "dereth-testkit::cpu::plugins::tank::scenario_the_tank_pauses_and_resumes_its_run_where_it_was",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-picks-a-locked-doors-lock-when-its-skill-beats-it",
        says: "A locked door across the Tank's route is looked at and its lock picked with a \
               lockpick when the character's lockpicking and the pick's bonus beat the lock by \
               DoorLockpickDiffExcessThreshold (VTank's lockpicks for doors), then opened and \
               walked through; a lock beyond the character is left, said.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-LOCKPICK"),
        station: "dereth-testkit::cpu::plugins::tank_fellows::scenario_the_tank_picks_a_locked_doors_lock_when_its_skill_beats_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-picks-its-target-by-turning-or-range-and-the-players-selection",
        says: "Of monsters of one priority the Tank fights first, by VTank's TargetSelectMethod, \
               the nearest, the one it turns least to face, or by turning near by; and with \
               TargetLock, the monster the player selects.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-TARGETING"),
        station: "dereth-testkit::cpu::plugins::tank::scenario_the_tank_picks_its_target_by_turning_or_range_and_the_players_selection",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-reads-and-writes-a-vtank-profiles-items-consumables-and-buffs",
        says: "The Tank sample reads and writes a VTank settings profile's other tables as its players' published \
               profiles hold them: kits, stones and buff items as consumables, buffs added and left out by \
               their families' first spells, combat items by their ids with their item buffs or none, and a \
               monster row's rings, streaks, void curses and weapon.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-PROFILE-TABLES"),
        station: "dereth-testkit::cpu::plugins::tank_windows::scenario_the_tank_reads_and_writes_a_vtank_profiles_items_consumables_and_buffs",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-reads-and-writes-vtank-settings-profiles",
        says: "The Tank sample reads a VTank settings profile from its data folder into its settings \
               and monster list, a row's damage type at VTank's default and any unpublished one read \
               as auto, names the settings it does not have, takes its route from the data folder, \
               and saves its own as a profile that reads back the same.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-PROFILES"),
        station: "dereth-testkit::cpu::plugins::scenario_the_tank_reads_and_writes_vtank_settings_profiles",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-reads-the-unknown-scrolls-it-can-learn",
        says: "With VTank's ReadUnknownScrolls the Tank takes and reads, once a corpse is looted, \
               a scroll the loot profile does not keep of a spell the character does not know and \
               can learn, and leaves one it knows; switched off, it reads none.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-SCROLLS"),
        station: "dereth-testkit::cpu::plugins::tank::scenario_the_tank_reads_the_unknown_scrolls_it_can_learn",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-recharges-by-vtanks-lists-and-refills-worn-mana",
        says: "The Tank sample gets health, stamina and mana back by VTank's lists for the vital \
               and the stance (kits, foods and potions by the vital their spell restores, the \
               restoring spells), and refills a worn item under the refill mark from a charged \
               mana stone used on the character.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-RECHARGE"),
        station: "dereth-testkit::cpu::plugins::scenario_the_tank_recharges_by_vtanks_lists_and_refills_worn_mana",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-recharges-with-its-wands-own-spell-keeping-the-wand-in-hand",
        says: "VTank's wand recharge spells in the Tank: a caster in hand whose own spell restores \
               what is low (the Casting Stein's Stamina to Mana) is used on the character before \
               its own spell, the caster kept in hand.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-WAND-SPELL"),
        station: "dereth-testkit::cpu::plugins::tank_fellows::scenario_the_tank_recharges_with_its_wands_own_spell_keeping_the_wand_in_hand",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-records-a-route-while-the-character-walks",
        says: "The Tank sample, recording while the character walks, adds a point to its route each \
               time the character has gone the distance its window sets from the last point it \
               added, and stops when switched off.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-ROUTE-RECORD"),
        station: "dereth-testkit::cpu::plugins::windows::scenario_the_tank_records_a_route_while_the_character_walks",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-recruits-on-xp-tells-and-answers-vtanks-fellowship-commands",
        says: "With VTank's AutoFellowManagement the Tank, leading a fellowship, recruits a player \
               who tells it xp and stands near, with VTank's own lines; tells one too far so and \
               recruits it on its second chance once near; answers help with the commands; and \
               answers nothing with the option off.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-FELLOW-MANAGER"),
        station: "dereth-testkit::cpu::plugins::tank_fellows::scenario_the_tank_recruits_on_xp_tells_and_answers_vtanks_fellowship_commands",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-renews-its-buffbots-buffs-on-a-vendor-trip-when-they-would-run-low-before-the-next",
        says: "On a vendor trip, buffs from its buffbot with under twice BuffbotMinutes left are \
               renewed while the character is in town: the Tank sample goes on from the vendor to the \
               buffbot, and from there back to where the vendor trip set out; with longer left, the \
               vendor trip goes back as it is; and a buffbot trip under way when the vendor trip \
               comes up goes on from the vendor.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-BUFFBOT-VENDOR"),
        station: "dereth-testkit::cpu::plugins::tank_buffbot::scenario_the_tank_renews_its_buffbots_buffs_on_a_vendor_trip_when_they_would_run_low_before_the_next",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-rings-a-crowd-and-casts-streaks-and-arcs-as-its-rows-and-settings-say",
        says: "A monster row marked for rings has the Tank cast its ring once MinimumRingTargets \
               monsters are within RingDistance and a bolt with fewer, and one marked for rings \
               that does not attack rings with one monster near; a row marked for streaks casts \
               the streak; with UseArcs the arc is cast at monsters ArcRange away or more and \
               the bolt nearer.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-RINGS"),
        station: "dereth-testkit::cpu::plugins::tank::scenario_the_tank_rings_a_crowd_and_casts_streaks_and_arcs_as_its_rows_and_settings_say",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-runs-a-reversed-route-backwards-and-stops-navigating-far-off-it",
        says: "A route the Tank runs reversed (VTank's reverse route) is run the other way round, \
               from the first point to the last; and further off its route than NavFarStopRange, \
               the Tank turns navigation off.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-REVERSE-FAR"),
        station: "dereth-testkit::cpu::plugins::tank::scenario_the_tank_runs_a_reversed_route_backwards_and_stops_navigating_far_off_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-salvages-what-its-profile-salvages-and-combines-bags-by-its-ranges",
        says: "What a loot profile's Salvage rule takes from a corpse the Tank salvages once the corpse is \
               done (an inscribed item kept), and with CombineSalvage it combines the bags \
               of one material whose workmanship falls in one of the profile's ranges, while \
               they fill no more than a bag.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-SALVAGE"),
        station: "dereth-testkit::cpu::plugins::tank_items::scenario_the_tank_salvages_what_its_profile_salvages_and_combines_bags_by_its_ranges",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-sells-its-loot-and-buys-its-buy-list-on-its-vendor-trip",
        says: "On /tank vendor the Tank walks to the vendor it was given, sells what the loot profile's \
               Sell rules match (never the weapon its settings name nor what a Keep rule \
               keeps), buys its buy list up to the counts with what the pack holds counted, \
               walks back, and says what it sold and bought and the pyreals before and after.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-VENDOR-TRIP"),
        station: "dereth-testkit::cpu::plugins::tank_items::scenario_the_tank_sells_its_loot_and_buys_its_buy_list_on_its_vendor_trip",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-sets-an-option-in-every-profile",
        says: "VTank's /vt opt setinall in the Tank sets the option now and in each settings \
               profile in its data folder, the rest of each profile kept.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-SETINALL"),
        station: "dereth-testkit::cpu::plugins::tank::scenario_the_tank_sets_an_option_in_every_profile",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-side-steps-its-casts-looks-again-after-turning-and-backs-off-to-buff",
        says: "With DoJiggle the Tank side-steps after its casts; with the breakable turn, \
               having turned to a monster off to the side it fights a monster of higher priority \
               that came meanwhile instead; with fast-cast buffs it moves backward while it \
               buffs; with each off, it does none of these.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-JIGGLE-TURN-FASTCAST"),
        station: "dereth-testkit::cpu::plugins::tank::scenario_the_tank_side_steps_its_casts_looks_again_after_turning_and_backs_off_to_buff",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-stacks-partial-stacks-and-crams-its-main-pack-into-its-side-pack",
        says: "VTank's AutoStack has the Tank merge stacks toward the bottom-most of their kind, the one \
               partial stack left top-most; its AutoCram moves the main pack's items into a \
               side pack.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-STACK-CRAM"),
        station: "dereth-testkit::cpu::plugins::tank_items::scenario_the_tank_stacks_partial_stacks_and_crams_its_main_pack_into_its_side_pack",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-summons-a-pet-of-its-mastery-and-the-monsters-element-once-enough-gather",
        says: "With VTank's SummonPets the Tank uses a summoning device of the character's mastery \
               whose element the monster's row asks for (PetDmg) once PetMonsterDensity monsters \
               its rows summon for are in pet range and no pet of the character's is out; /vt \
               testpet says why none would be.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-SUMMON"),
        station: "dereth-testkit::cpu::plugins::tank_fellows::scenario_the_tank_summons_a_pet_of_its_mastery_and_the_monsters_element_once_enough_gather",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-swings-at-the-power-its-weapon-recklessness-and-attack-bar-say",
        says: "The Tank swings a weapon of a thrust and a slash on the side of the thrust mark that \
               strikes with the monster row's damage type (VTank's AutoAttackPower), keeps the \
               power within Recklessness's band with UseRecklessness and the skill trained, swings \
               at the power VTank's /vt setattackbar sets, and takes a step at a time toward the \
               monster's weapon at its /vt equipitemsfor.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-MELEE-POWER"),
        station: "dereth-testkit::cpu::plugins::tank::scenario_the_tank_swings_at_the_power_its_weapon_recklessness_and_attack_bar_say",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-takes-the-selected-item-for-a-slot",
        says: "The Tank sample's window takes the item selected in the game for its weapon, bow, \
               wand or ammunition slot at a click, and refuses an item that does not fit, saying \
               why.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-SLOTS"),
        station: "dereth-testkit::cpu::plugins::scenario_the_tank_takes_the_selected_item_for_a_slot",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-takes-up-a-route-set-again-mid-leg-from-its-nearest-waypoint",
        says: "A route set again while one of its legs is walked (a meta entering the state it is \
               in anew, which hands the Tank the same route; a death; a vendor trip's end) is \
               taken up from its nearest waypoint once the walk ends, with no error, and runs on \
               to its end.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-ROUTE-RESET"),
        station: "dereth-testkit::cpu::plugins::tank::scenario_the_tank_takes_up_a_route_set_again_mid_leg_from_its_nearest_waypoint",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-takes-up-the-weapon-for-a-monsters-damage-and-a-striking-one-on-a-vulned-monster",
        says: "For a row whose weapon is auto the Tank takes up, of the weapons its Weapons \
               setting names, the one striking with the row's damage type, a rending one first, \
               and on a monster it has made vulnerable a critical-strike one first, as VTank's \
               automatic weapon choice does.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-WEAPON-CHOICE"),
        station: "dereth-testkit::cpu::plugins::tank::scenario_the_tank_takes_up_the_weapon_for_a_monsters_damage_and_a_striking_one_on_a_vulned_monster",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-takes-what-sell-read-and-user-rules-match",
        says: "Every loot rule's action takes its item from the corpse: a Sell rule's, a Read rule's (read \
               once the corpse is done) and a User rule's; an item no rule matches is left.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-LOOT-ACTIONS"),
        station: "dereth-testkit::cpu::plugins::tank_items::scenario_the_tank_takes_what_sell_read_and_user_rules_match",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-tells-once-of-a-locked-door-across-its-route",
        says: "A locked door across the Tank sample's route does not open when used: the Tank says \
               once that it blocks the way and skips the waypoint after its stuck recovery, as \
               before; it uses no key or lockpick.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-DOOR-LOCKED"),
        station: "dereth-testkit::cpu::plugins::windows::scenario_the_tank_tells_once_of_a_locked_door_across_its_route",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-throws-grenades-at-a-crowd-casts-fists-ahead-and-picks-a-random-damage-type",
        says: "A fighter whose row is marked for rings throws a grenade its Consumables list \
               names at a crowd and swings with fewer about; a row set to fists has the Tank \
               face the monster and cast Tusker Fists ahead; a row set to random fights with a \
               damage type it has a spell of.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-GRENADES-FISTS"),
        station: "dereth-testkit::cpu::plugins::tank::scenario_the_tank_throws_grenades_at_a_crowd_casts_fists_ahead_and_picks_a_random_damage_type",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-times-an-item-buff-by-the-characters-age",
        says: "The Tank times an item buff by the character's age (seconds played), as VTank \
               does, against its spell's duration: it is recast once the character has played \
               that long less RebuffTimeRemainingSeconds, and not before.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-ITEM-BUFF-AGE"),
        station: "dereth-testkit::cpu::plugins::tank::scenario_the_tank_times_an_item_buff_by_the_characters_age",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-trades-by-a-vendors-own-profile-at-a-routes-vendor-waypoint",
        says: "At a route's vendor waypoint the Tank opens the vendor and trades by its own profile \
               (vendors/<name>.utl): its Sell rules sold, its Keep # rule bought up to the \
               count, its Keep rule bought once when the pack has none.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-VENDOR-PROFILE"),
        station: "dereth-testkit::cpu::plugins::tank_items::scenario_the_tank_trades_by_a_vendors_own_profile_at_a_routes_vendor_waypoint",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-travels-to-its-hunting-place-and-hunts-there",
        says: "The Tank sample sent to hunt somewhere far travels there by a journey, through a \
               portal when one is the way, and runs its loop where it arrives.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-TRAVEL"),
        station: "dereth-testkit::cpu::plugins::scenario_the_tank_travels_to_its_hunting_place_and_hunts_there",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-tries-a-corpse-that-will-not-open-again-then-leaves-it",
        says: "A kill's corpse that does not open is tried again by the Tank \
               CorpseOpenTimeoutSeconds later, and after BlacklistCorpseOpenAttemptCount tries \
               left alone for BlacklistCorpseOpenTimeoutSeconds.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-CORPSE-RETRY"),
        station: "dereth-testkit::cpu::plugins::tank::scenario_the_tank_tries_a_corpse_that_will_not_open_again_then_leaves_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-uses-an-npc-until-it-speaks-and-a-use-npc-waypoints-npc-once",
        says: "At VTank's NPC Talk waypoint the Tank uses the NPC again and again until it tells \
               the character something; at a Use Portal/NPC waypoint naming an NPC it uses it \
               once and goes on, waiting for no teleport.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-NPC-WAYPOINTS"),
        station: "dereth-testkit::cpu::plugins::tank::scenario_the_tank_uses_an_npc_until_it_speaks_and_a_use_npc_waypoints_npc_once",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-uses-kits-in-peace-and-keeps-recharging-after-switching",
        says: "With GoToPeaceModeToUseKits the Tank drops from its fighting stance to peace mode \
               before a healing kit; after it switches to magic stance for a restoring spell, the \
               recharge boost (RechargeBoostTimeSeconds, RechargeBoostAmount) has it recharge \
               again above its usual mark.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-RECHARGE-BOOST"),
        station: "dereth-testkit::cpu::plugins::tank_items::scenario_the_tank_uses_kits_in_peace_and_keeps_recharging_after_switching",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-waits-at-a-checkpoint-until-the-server-places-it-there",
        says: "At VTank's checkpoint waypoint the Tank waits, stepping forward now and then, \
               until the server's own word on the character's place (Movement_PositionEvent) puts \
               it there, then goes on.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-CHECKPOINT"),
        station: "dereth-testkit::cpu::plugins::tank::scenario_the_tank_waits_at_a_checkpoint_until_the_server_places_it_there",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-walks-a-route-leg-round-a-corner-along-the-clients-route",
        says: "A route leg whose waypoint is out of a straight line of sight (round the corner of \
               an L-shaped hallway) is walked along the client's own walking route from its \
               start, round the corner and through the doors on the way, and the waypoint is \
               reached never stuck; a leg in sight is walked straight, as VTank walks it, and \
               with the Tank's NavStraightLegs every leg is, into the corner's wall and stuck \
               there first. A monster behind a wall is walked to along the client's whole route \
               the same way, not a corner at a time.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-ROUTE-LEGS"),
        station: "dereth-testkit::cpu::plugins::tank::scenario_the_tank_walks_a_route_leg_round_a_corner_along_the_clients_route",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-walks-a-route-leg-round-a-retail-dungeon-corner",
        says: "On the retail data, in the Training Academy: a route leg from the room a new \
               character starts in to a waypoint round the corner of the passage out, out of a \
               straight line of sight, is walked along the client's own route, and the waypoint \
               is reached never stuck.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-ROUTE-LEGS-RETAIL"),
        station: "dereth-headless::dat::presentation::tank_corner::the_tank_walks_a_route_leg_round_an_academy_corner_along_the_clients_route",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "plugins.samples.the-tank-walks-on-toward-a-monster-it-cannot-see-until-it-does",
        says: "A monster within reach of a walk but out of a straight line of sight that the walk \
               itself does not go round (below the edge of a ramp, which cuts the line but not \
               the way) is walked on toward, as near as melee distance, until it is in sight, \
               then fought: the Tank does not stand at the attack distance waiting for a line of \
               sight, as it stood on the Training Academy's hive ramp.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-SIGHT-WALK"),
        station: "dereth-testkit::cpu::plugins::tank::scenario_the_tank_walks_on_toward_a_monster_hidden_below_a_ramps_edge_until_it_sees_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-walks-round-a-wall-to-a-monster-a-step-away-across-it-before-attacking",
        says: "A monster within melee distance but out of a straight line of sight (a step away \
               across a wall or a ledge) is walked to along the client's route before the Tank \
               attacks it, as one further off is: the server's charge would go the long way \
               round and give up (\"You charged too far!\"). Walked right up to it once and \
               still out of sight, it is attacked.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-MELEE-SIGHT"),
        station: "dereth-testkit::cpu::plugins::tank::scenario_the_tank_walks_round_a_wall_to_a_monster_a_step_away_across_it_before_attacking",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-walks-round-someone-in-the-way-of-its-charge-before-attacking",
        says: "With a person (a townsperson, a vendor, a player) standing across the straight \
               line to a monster beyond melee distance, the Tank walks round them along the \
               client's route before attacking: from there the attack would be a charge straight \
               at the monster, which their body stops short. Someone off the line does not hold \
               the attack up.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-CHARGE-BLOCKED"),
        station: "dereth-testkit::cpu::plugins::tank::scenario_the_tank_walks_round_someone_in_the_way_of_its_charge_before_attacking",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-walks-round-walls-and-through-doors-to-a-monster-and-leaves-one-no-route-reaches",
        says: "The Tank walks to a monster behind a wall along the client's walking route round \
               the wall's end, and to one behind a closed door in a wall's gap along the route \
               through the door, opening it on the way, and fights each; a monster walled in, \
               which no route reaches, it leaves alone, and fights one it can reach.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-APPROACH"),
        station: "dereth-testkit::cpu::plugins::tank::scenario_the_tank_walks_round_walls_and_through_doors_to_a_monster_and_leaves_one_no_route_reaches",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-walks-to-its-vendor-from-where-it-was-set-however-far-it-stands-from-there",
        says: "On the vendor trip, from where the player stood when the vendor was set, the Tank \
               sample walks to the vendor however far it stands from there (fifty meters off, across \
               a town square), and trades.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-VENDOR-STAND"),
        station: "dereth-testkit::cpu::plugins::tank_items::scenario_the_tank_walks_to_its_vendor_from_where_it_was_set_however_far_it_stands_from_there",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tank-walks-within-reach-of-what-it-uses-an-item-on",
        says: "Before it uses an item on something in the world (a key on a locked door), the \
               Tank walks to within a metre and a half of it, so the use is made where the \
               character stands, not after a walk of the server's own, which a closed door can \
               stop short.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-USE-REACH"),
        station: "dereth-testkit::cpu::plugins::tank_quests::scenario_the_tank_walks_within_reach_of_a_door_before_using_a_key_on_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tanks-academy-profile-reads-whole-for-every-starting-town",
        says: "The Tank ships a Training Academy profile: a VTank meta for each starting town's \
               Academy (Holtburg, Shoushi, Yaraq, Sanamar), its routes embedded, which runs \
               the Academy's quests in order by what the game says (the item come into the \
               pack, the person's tell, the portal passed) and leaves for the town by a \
               journey; each reads whole, with its loot profile keeping the quest's items.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-ACADEMY"),
        station: "dereth-testkit::cpu::plugins::tank_quests::scenario_the_tanks_academy_profile_reads_whole_for_every_starting_town",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tanks-buffs-page-lists-its-buffs-and-adds-or-leaves-out-one",
        says: "The Tank sample's Buffs page lists the buffs it keeps up with the spell each is cast with, adds one \
               by name as VTank's Buffs tab adds a skill not trained, and leaves one out or takes it back.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-BUFFS-PAGE"),
        station: "dereth-testkit::cpu::plugins::tank_windows::scenario_the_tanks_buffs_page_lists_its_buffs_and_adds_or_leaves_out_one",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tanks-controls-run-vtanks-commands",
        says: "The Tank sample's window runs from its controls what its VTank commands do: pause, \
               Force Buff and its cancel, the meta's state, an expression, the item and monster \
               tests, a setting set in every profile, a log type switched, and the route's \
               portal, NPC talk and vendor waypoints.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-WINDOW-COMMANDS"),
        station: "dereth-testkit::cpu::plugins::windows::scenario_the_tanks_controls_run_vtanks_commands",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tanks-hotkeys-run-and-pause-it-set-the-attack-height-loot-the-open-container-and-enter-meta-states",
        says: "The Tank sample's hotkeys, keys set in its window as VTank's are bound: one runs and pauses the \
               macro, one sets the attack height, one loots the open container by the loot profile, and a \
               meta state bound to a key is entered at it; a key a hotkey cannot take is not bound.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-HOTKEYS"),
        station: "dereth-testkit::cpu::plugins::tank_windows::scenario_the_tanks_hotkeys_run_and_pause_it_set_the_attack_height_loot_the_open_container_and_enter_meta_states",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tanks-items-page-keeps-combat-items-and-consumables-from-the-selection",
        says: "The Tank sample's Items page keeps VTank's Items and Consumables tabs: combat items and pets added \
               from the selection, buffed or not, a consumable added from the selection or by name with \
               its kind and count, and each removed; the monster list takes the selected monster and \
               offers the combat items as a row's weapon.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-ITEMS-PAGE"),
        station: "dereth-testkit::cpu::plugins::tank_windows::scenario_the_tanks_items_page_keeps_combat_items_and_consumables_from_the_selection",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tanks-journey-steps-down-a-ledge-only-a-drop-leads-on-from-when-no-higher-than-a-step-down",
        says: "Where only a drop off a ledge leads on (the client's journeys leave a drop to the \
               player), a journey of the Tank sample's steps down one no higher than the body steps \
               down without falling (1.5 m): it walks to the ledge's edge, steps off and makes the \
               journey again; a higher one it leaves, the journey failing.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-STEP-DOWN"),
        station: "dereth-testkit::cpu::plugins::tank_quests::scenario_the_tanks_journey_steps_down_a_ledge_only_a_drop_leads_on_from_when_no_higher_than_a_step_down",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tanks-litmus-profile-sets-up-travels-to-its-dungeon-and-goes-back-when-far",
        says: "The Tank sample's litmus profile (a VTank meta for Holtburg) reads whole, loads its \
               loot profile, sets the macro up and its vendor, travels by a journey through the \
               Holtburg Redoubt portal into the dungeon and walks its route there; put back in town \
               (a death that left nothing to recover), the macro travels to its hunting place again.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-LITMUS"),
        station: "dereth-testkit::cpu::plugins::tank_quests::scenario_the_tanks_litmus_profile_sets_up_travels_to_its_dungeon_and_goes_back_when_far",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tanks-meta-page-edits-a-metas-rules-and-the-running-meta-follows",
        says: "The Tank sample's Meta page edits the meta's rules as VTank's Meta tab does: a rule chosen is \
               changed, added (an All of conditions edited a part at a time, a route file embedded), moved \
               or removed, the running meta following at once with the rules that ran in its state still \
               run, and the meta saved as a VTank meta; a new meta is made.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-META-EDITOR"),
        station: "dereth-testkit::cpu::plugins::tank_windows::scenario_the_tanks_meta_page_edits_a_metas_rules_and_the_running_meta_follows",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tanks-monster-expressions-read-species-health-shield-and-settings",
        says: "The Tank's monster list expressions read VTank's variables: a species by name and \
               a maximum health from the monster's appraisal pick the row that sends it after a \
               farther monster, and a monster wielding a shield or one a setting rules out is \
               left alone.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-MONSTER-EXPRESSIONS"),
        station: "dereth-testkit::cpu::plugins::tank::scenario_the_tanks_monster_expressions_read_species_health_shield_and_settings",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tanks-remote-control-force-buffs-follows-and-switches-corners-and-target-lock",
        says: "The Tank sample's remote control, a bar on the screen while it is on, has VTank's F, CF, FC, C and \
               lock: Force Buff, its cancel, follow the selected, and the follow-around-corners and \
               target-lock switches, which show their settings however they are set.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-REMOTE"),
        station: "dereth-testkit::cpu::plugins::tank_windows::scenario_the_tanks_remote_control_force_buffs_follows_and_switches_corners_and_target_lock",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tanks-route-page-makes-changes-saves-and-loads-a-vtank-route",
        says: "The Tank sample's Route page makes a route as VTank's Route tab does (new of its \
               kind; a point, a jump at the heading faced, a pause, a chat line, a recall, a \
               checkpoint), moves and removes waypoints, runs it reversed (its points in the \
               order made, as VTank's reverse route), saves it as a VTank route and loads one \
               from the routes it lists, its own among them.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-ROUTE-PAGE"),
        station: "dereth-testkit::cpu::plugins::windows::scenario_the_tanks_route_page_makes_changes_saves_and_loads_a_vtank_route",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tanks-settings-page-finds-a-setting-by-part-of-its-name",
        says: "The Tank sample's Settings page narrows its list of every setting to those whose name                holds what is typed, as VTank's advanced options panel is filterable, and sets the one                picked from it.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-SETTINGS-FILTER"),
        station: "dereth-testkit::cpu::plugins::tank_windows::scenario_the_tanks_settings_page_finds_a_setting_by_part_of_its_name",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tanks-settings-page-loads-this-characters-own-profiles-from-a-list",
        says: "The Tank sample's Settings page lists the settings profiles this character saved, as VTank's [Char] \
               profiles, and loads the one chosen.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TANK-CHAR-PROFILES"),
        station: "dereth-testkit::cpu::plugins::tank_windows::scenario_the_tanks_settings_page_loads_this_characters_own_profiles_from_a_list",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-tanks-window-sets-every-setting-edits-the-monster-list-and-hunts",
        says: "The Tank sample's window lists every setting and sets one or puts it back to its \
               default, as its command does; saves and removes monster rows, as its command does; \
               and hunts at a place typed, as its command does.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-WINDOW-TANK"),
        station: "dereth-testkit::cpu::plugins::windows::scenario_the_tanks_window_sets_every_setting_edits_the_monster_list_and_hunts",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-trackers-count-each-creatures-damage-by-element-and-the-share-of-attacks-that-landed",
        says: "The Trackers sample counts the damage dealt to and taken from each creature from \
               the combat lines, by element, with the share of the player's attacks that landed, \
               of the creature's attacks the player evaded and of its spells the player resisted, \
               critical hits, and the killing blows the server tells the player of once the \
               creature's corpse shows; a reset starts the session over and the lifetime kept with \
               the character stays.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TRACKERS-COMBAT"),
        station: "dereth-testkit::cpu::plugins::trackers::scenario_the_trackers_count_each_creatures_damage_by_element_and_the_share_of_attacks_that_landed",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-trackers-count-the-consumables-used-and-warn-when-one-runs-low",
        says: "The Trackers sample counts the spell components, healing kits, mana stones, trade \
               notes and salvage the character holds against the session's start, with the change, \
               its rate per hour and the change in their worth, and warns once when a component \
               the player named falls under the count given.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TRACKERS-ITEMS"),
        station: "dereth-testkit::cpu::plugins::trackers::scenario_the_trackers_count_the_consumables_used_and_warn_when_one_runs_low",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-trackers-keep-each-player-seen-and-where-it-was-last",
        says: "The Trackers sample keeps each player seen this session with when and where it was \
               last seen: here while in view, and for one gone out of view, how long ago and the \
               place it was last seen.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TRACKERS-PLAYERS"),
        station: "dereth-testkit::cpu::plugins::trackers::scenario_the_trackers_keep_each_player_seen_and_where_it_was_last",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-trackers-remember-your-corpse-and-point-the-arrow-at-it",
        says: "The Trackers sample tells where the character's corpse lies when it shows, keeps it \
               with the character, and points the arrow at it on /track corpse, after the corpse \
               is out of view and after the plugin is loaded afresh; the corpses it lists are the \
               player's own, other players', fellows', the player's kills or others'.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TRACKERS-CORPSE"),
        station: "dereth-testkit::cpu::plugins::trackers::scenario_the_trackers_remember_your_corpse_and_point_the_arrow_at_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-trackers-show-experience-and-luminance-per-hour-and-over-the-last-five-minutes",
        says: "The Trackers sample shows the experience and luminance earned this session, per \
               hour since the first gain and per hour over the last five minutes, and the time to \
               the next level at the session's rate where the experience table says how far it is.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TRACKERS-XP"),
        station: "dereth-testkit::cpu::plugins::trackers::scenario_the_trackers_show_experience_and_luminance_per_hour_and_over_the_last_five_minutes",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-trackers-tell-how-long-each-worn-items-mana-lasts-and-warn-before-it-runs-out",
        says: "The Trackers sample appraises the items the character wears and wields a few at a \
               time and lists each one's mana, how long that lasts at its burn, and whether its \
               spells are on the character, warns once when one falls under the minutes the player \
               set, and notes the server's low-on-mana lines.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TRACKERS-MANA"),
        station: "dereth-testkit::cpu::plugins::trackers::scenario_the_trackers_tell_how_long_each_worn_items_mana_lasts_and_warn_before_it_runs_out",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.samples.the-trackers-window-sets-the-low-warnings-and-clears-the-arrow",
        says: "The Trackers sample's window sets and stops a low warning for an item by name and \
               count, as its command does, and clears the arrow it put on the character's corpse.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-WINDOW-TRACKERS"),
        station: "dereth-testkit::cpu::plugins::windows::scenario_the_trackers_window_sets_the_low_warnings_and_clears_the_arrow",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.sandbox.a-plugin-reaches-nothing-but-its-package-and-the-host",
        says: "A plugin has no file, process or code-loading functions, cannot change the \
               libraries or the plugin interface, and can load only code from its own package.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-SANDBOX"),
        station: "dereth-testkit::cpu::plugins::scenario_a_plugin_reaches_nothing_but_its_package_and_the_host",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.services.a-plugin-calls-a-service-another-plugin-offers",
        says: "A plugin calls a service another plugin in the same client offers and gets its \
               answer or its error as a promise; a call to a plugin or a service that is not \
               there, or one the provider does not let it use, fails at once; and the Tank offers \
               its meta's state and its settings so.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-SERVICES"),
        station: "dereth-testkit::cpu::plugins::scenario_a_plugin_calls_a_service_another_plugin_offers",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.store.the-character-store-opens-at-login-and-keeps-plain-values",
        says: "A plugin's per-character store opens once a character is logged in and keeps \
               numbers, text, lists and tables, but not functions.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-STORE"),
        station: "dereth-testkit::cpu::plugins::scenario_the_character_store_opens_at_login_and_keeps_plain_values",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.store.the-global-world-and-character-stores-keep-values-within-their-quota",
        says: "A plugin keeps values for everywhere, for the world and for the character, and a write \
               that would take a store past its size limit is refused and changes nothing.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-STORE-SCOPES"),
        station: "dereth-testkit::cpu::plugins::scenario_the_global_world_and_character_stores_keep_values_within_their_quota",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.tasks.timers-and-sleeping-tasks-run-on-the-frame-clock",
        says: "A plugin's repeating timer fires on its period until cancelled, a one-shot timer \
               fires once, and a task that sleeps wakes after its delay, all on the frame clock.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TASKS"),
        station: "dereth-testkit::cpu::plugins::scenario_timers_and_sleeping_tasks_run_on_the_frame_clock",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.ubscript.a-named-script-active-before-the-world-logs-in-and-hears-its-end",
        says: "A script in a package that names it a UtilityBelt script, active before the world, \
               starts at the character list, logs a character in with UtilityBelt's action, hears \
               the client's state change, loads a module of its own with its globals, and hears \
               its own end when it is unloaded.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-UBSCRIPT-LIFECYCLE"),
        station: "dereth-testkit::cpu::plugins::ubscript::scenario_a_named_script_active_before_the_world_logs_in_and_hears_its_end",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.ubscript.a-script-hears-utilitybelts-events",
        says: "A script hears UtilityBelt's events over the plugin interface's: objects coming and \
               going, the selection, a container opened with its items, a tell, speech and an \
               emote with the speaker and the words, a line eaten, a handler that waits while the \
               next is heard, Once and Until, and the world's tick.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-UBSCRIPT-EVENTS"),
        station: "dereth-testkit::cpu::plugins::ubscript::scenario_a_script_hears_utilitybelts_events",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.ubscript.a-script-reaches-utilitybelts-other-modules",
        says: "A script reaches UtilityBelt's other modules and globals over the plugin interface: \
               its own folder read only and its data folder read and written, JSON, dates and \
               spans, regular expressions, coordinates, marks in the world, enumerations and their \
               flags, property tables by typed id or by number, vectors, and the client's message \
               stream.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-UBSCRIPT-MODULES"),
        station: "dereth-testkit::cpu::plugins::ubscript::scenario_a_script_reaches_utilitybelts_other_modules",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.ubscript.a-scripts-actions-go-through-utilitybelts-queue",
        says: "A script's actions go through UtilityBelt's queue over the plugin interface's \
               actions: an immediate action at once, the rest one at a time and the highest \
               priority first, each ending with its error and details and its callback heard; an \
               action named and given its own priority; a group run in its own order; a removed \
               action never run; and chat lines typed at the interface's pace.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-UBSCRIPT-ACTIONS"),
        station: "dereth-testkit::cpu::plugins::ubscript::scenario_a_scripts_actions_go_through_utilitybelts_queue",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.ubscript.a-scripts-character-reads-its-fellowship-vitals-and-own-properties",
        says: "A script's character, read through UtilityBelt's API, has its fellowship and each \
               fellow's figures, its vitals, its own properties by typed id, its inventory counted \
               by name and class, and its allegiance, trade and vendor as the client knows them; \
               joining a fellowship is heard.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-UBSCRIPT-CHARACTER"),
        station: "dereth-testkit::cpu::plugins::ubscript::scenario_a_scripts_character_reads_its_fellowship_vitals_and_own_properties",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.ubscript.a-scripts-imgui-hud-is-a-declared-window",
        says: "A HUD a script draws the immediate way, with UtilityBelt's views and ImGui modules, \
               is a declared window: each render builds its nodes and only what changed is set \
               again, and a click, a toggle, text typed, a number stepped, a choice picked and a \
               tab chosen reach the next render as ImGui reports them.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-UBSCRIPT-HUDS"),
        station: "dereth-testkit::cpu::plugins::ubscript::scenario_a_scripts_imgui_hud_is_a_declared_window",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.ubscript.a-utilitybelt-script-runs-from-its-folder-and-reads-the-game",
        says: "A script written for UtilityBelt's scripting runs from its folder alone, with \
               UtilityBelt's globals: it reads its character, the world's objects by class and an \
               object's properties by typed id as UtilityBelt answers them, waits on an action and \
               hears an action's callback.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-UBSCRIPT-BASICS"),
        station: "dereth-testkit::cpu::plugins::ubscript::scenario_a_utilitybelt_script_runs_from_its_folder_and_reads_the_game",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.ubscript.what-cannot-be-done-here-is-refused-plainly",
        says: "What the plugin interface cannot do for a script written for UtilityBelt's \
               scripting is refused plainly: a member, an event or a module that cannot be \
               answered here raises an error saying why, and an action that cannot be carried out \
               ends failed with the reason.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-UBSCRIPT-REFUSALS"),
        station: "dereth-testkit::cpu::plugins::ubscript::scenario_what_cannot_be_done_here_is_refused_plainly",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.ui.a-canvas-and-a-chart-draw-what-a-plugin-gives-them",
        says: "A canvas keeps a plugin's draw commands (lines, rectangles, circles, text) and takes \
               new ones, a chart keeps its series, both draw as solid fills and placed text, and a \
               draw command of no known operation is refused.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-CANVAS"),
        station: "dereth-testkit::cpu::plugins::scenario_a_canvas_and_a_chart_draw_what_a_plugin_gives_them",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.ui.a-declared-window-takes-clicks-and-changes",
        says: "A window a plugin declares is kept as a tree of labels, buttons, toggles, meters \
               and lists; a click or a change reaches the plugin, what the plugin changes shows \
               in the window, and a misspelt part, a text over 4,096 characters, a window over \
               64 KiB of text and a canvas over 1,000 commands are refused.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-UI"),
        station: "dereth-testkit::cpu::plugins::scenario_a_declared_window_takes_clicks_and_changes",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.ui.a-hotkey-runs-its-plugins-command-as-a-gesture",
        says: "A hotkey a plugin declares runs its command as if the player had typed it, a player \
               gesture; a hotkey whose command the plugin has not claimed is told to the player, \
               and a hotkey asking for a key with neither Ctrl nor Alt is refused, so a plain key \
               stays the game's.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-HOTKEYS"),
        station: "dereth-testkit::cpu::plugins::scenario_a_hotkey_runs_its_plugins_command_as_a_gesture",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.ui.a-hud-is-kept-with-its-anchor-and-changes-like-a-window",
        says: "A plugin's HUD is kept as a tree with the screen corner or edge it asked for, changes \
               like a window, and a HUD with a misspelt anchor, inside a window, or holding a \
               control other than a button, a toggle or a list is refused.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-HUD"),
        station: "dereth-testkit::cpu::plugins::scenario_a_hud_is_kept_with_its_anchor_and_changes_like_a_window",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.ui.a-list-scrolled-to-its-end-keeps-its-newest-row-in-sight",
        says: "A list scrolled to its end, or with every row in sight, keeps its last row in sight as \
               rows are added at its end: a log shows its newest line.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-LAYOUT-LIST-END"),
        station: "dereth-plugin-ui::lib::frame_tests::a_list_at_its_end_keeps_its_newest_row_in_sight_as_rows_are_added",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.ui.a-lists-columns-are-fixed-or-share-its-width",
        says: "A list's columns share its width: a column of a `width` is that many cells, the \
               growing columns share what is left in proportion to their `grow`, and where no \
               column says grow the last one of no width grows to the list's right edge; a \
               column's `min_width` holds the list, and so its window, at least that wide.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-LAYOUT-COLUMNS"),
        station: "dereth-testkit::cpu::plugins::scenario_a_lists_columns_are_fixed_or_share_its_width",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.ui.a-node-stretches-across-its-container-unless-it-keeps-its-own-size",
        says: "Across a column every node is as wide as the column, and in a row a list, a text box \
               or a container is as tall as the row, but a picture (an image, a canvas, a chart) \
               keeps its own size and a one-line control in a row stands in its middle; a \
               container's `align` keeps its nodes at their own size and its `justify` places \
               the room they leave.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-LAYOUT-STRETCH"),
        station: "dereth-testkit::cpu::plugins::scenario_a_node_stretches_across_its_container_unless_it_keeps_its_own_size",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.ui.a-plugin-hides-and-moves-the-games-panels-and-lets-go-when-it-unloads",
        says: "A plugin hides, shows and moves the game's own panels by name, gives one back with \
               reset, and lets go of them all when it unloads; a panel the game does not have is \
               refused.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-RETAIL-UI"),
        station: "dereth-testkit::cpu::plugins::scenario_a_plugin_hides_and_moves_the_games_panels_and_lets_go_when_it_unloads",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.ui.a-plugin-hud-is-drawn-at-its-corner-without-a-frame",
        says: "On the desktop a plugin's HUD is drawn at the screen corner it names, in the \
               interface's own font, with no frame, title or close box.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-HUD-DRAWN"),
        station: "dereth-client::gpu::ui::plugin_windows::a_plugin_hud_is_drawn_at_its_corner_without_a_frame",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "plugins.ui.a-plugin-window-is-drawn-over-the-game-and-its-button-reaches-the-plugin",
        says: "On the desktop a plugin's window is drawn over the game, framed and lettered in the \
               interface's own style, and a click on one of its buttons reaches the plugin, whose \
               change then shows in the window.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-UI-DRAWN"),
        station: "dereth-client::gpu::ui::plugin_windows::a_plugin_window_is_drawn_over_the_game_and_its_button_reaches_the_plugin",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "plugins.ui.a-plugin-window-opens-only-when-the-player-asks",
        says: "A plugin's windows and HUDs start closed and open only when the player asks: from \
               the plugin bar (where a plugin with only a HUD has its HUD), or by the plugin while \
               it answers the player (a press on its window, one of its commands). One a plugin \
               declares or shows as it loads, on a timer or on a game event stays closed, and a \
               window the player had open opens again when its plugin reloads.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-WINDOWS-CLOSED"),
        station: "dereth-plugin-host::lib::tests::a_plugin_window_opens_only_when_the_player_asks",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.ui.a-resizable-window-takes-the-players-size-within-its-bounds",
        says: "A window its plugin declares resizable takes the size the player gives it within the \
               bounds the plugin declared, keeps it when declared again, and the plugin hears the \
               new size.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-RESIZE"),
        station: "dereth-testkit::cpu::plugins::scenario_a_resizable_window_takes_the_players_size_within_its_bounds",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.ui.a-window-and-the-bar-come-back-where-the-player-left-them-on-the-screen",
        says: "A plugin window opens where and at the size the player left it, and the plugin bar \
               where and as wide as the player left it, remembered for the character; a window \
               closed and opened again comes back where it was left; each is kept on the screen \
               as it opens and whenever the screen or the interface's scale changes.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-PLACES"),
        station: "dereth-plugin-ui::lib::frame_tests::a_window_opens_where_and_at_the_size_left_and_stays_on_the_screen",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.ui.a-window-is-never-narrower-than-its-nodes-least-width",
        says: "A plugin window is never narrower than its nodes' least width (its widest line of \
               controls that keep their size, a list's columns at their least) or its title's \
               room: the player's drag stops there.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-LAYOUT-LEAST-WIDTH"),
        station: "dereth-plugin-ui::lib::frame_tests::a_window_is_never_dragged_narrower_than_its_nodes_least_width",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.ui.a-window-shorter-than-its-nodes-scrolls-through-them",
        says: "A window shorter than its nodes' least height scrolls through them: they are laid \
               out at their least height, as wide as the body less the scrollbar's room, and \
               the wheel, the scrollbar and the pad's focus move through them.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-LAYOUT-SCROLL"),
        station: "dereth-testkit::cpu::plugins::scenario_a_window_shorter_than_its_nodes_scrolls_through_them",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.ui.a-window-takes-the-size-the-player-drags-its-edges-to",
        says: "The player sizes a plugin window by its left, right or bottom edge, a bottom corner \
               or its grip, within the screen and the sizes it declares, the edges not held \
               staying put: a window declared resizable is laid out again and its plugin hears \
               the size; one declared neither way is laid out again too, its plugin hearing \
               nothing; one declared `resizable = false` keeps the size its nodes ask for.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-RESIZE-EDGES"),
        station: "dereth-plugin-ui::lib::frame_tests::a_windows_edges_and_corner_take_the_size_the_player_drags_them_to",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.ui.a-windows-grip-is-part-of-its-frame",
        says: "A plugin window's grip is part of its frame, in the frame's bottom right corner \
               inside its border (the interface's own mark, or none where the frame's corner is \
               a handle already): it takes no room from the window's body, and a press there \
               sizes the window both ways.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-GRIP"),
        station: "dereth-plugin-ui::lib::frame_tests::a_windows_grip_takes_no_room_from_its_body_and_stands_in_the_frames_corner",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.ui.an-image-shows-a-png-from-the-package-or-one-fetched",
        says: "An image node shows a PNG file from the plugin's package, or one the plugin fetched as an \
               image, by the name the fetch answered; a fetched file that is not a PNG fails, and \
               a picture over 1,024 pixels a side is not kept, from the package or fetched.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-IMAGES"),
        station: "dereth-testkit::cpu::plugins::scenario_an_image_shows_a_png_from_the_package_or_one_fetched",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.ui.controls-are-as-wide-as-their-text-needs",
        says: "A control asks for the width its text needs: a button its label, in the face it is \
               drawn in, with even room at both ends for its art's cap and a gap; a choice its \
               longest option, up to forty cells; and the labelled controls of a column have \
               their labels as wide as the widest, so their boxes line up.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-LAYOUT-CONTROLS"),
        station: "dereth-plugin-ui::lib::frame_tests::a_button_is_as_wide_as_its_drawn_label_with_even_room_at_both_ends",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.ui.growing-nodes-share-the-room-left-in-proportion-within-their-bounds",
        says: "The room left along a window, a column or a row goes to its growing nodes in \
               proportion to their `grow`, none past its most size (`max_width`, `max_height`), \
               the room it leaves going to the others; room short is taken from the nodes above \
               their least size in proportion to how far above it they are.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-LAYOUT-GROW"),
        station: "dereth-testkit::cpu::plugins::scenario_growing_nodes_share_the_room_left_in_proportion_within_their_bounds",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.ui.tabs-a-number-and-a-choice-take-the-players-changes",
        says: "A tabs node of group pages, a number and a choice are kept in the window's tree; the \
               player's change of each reaches the plugin as an event and stays on the node.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-UI-CONTROLS"),
        station: "dereth-testkit::cpu::plugins::scenario_tabs_a_number_and_a_choice_take_the_players_changes",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.ui.text-inputs-sliders-rich-list-cells-tooltips-and-toasts-work-as-declared",
        says: "Text typed in an input or a text box and a slider moved reach the plugin as changes \
               and stay on their nodes; list cells carry their own colour and icon and nodes their \
               tooltips; a toast shows at the top of the screen with the plugin's name and goes \
               when its time is up.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-INPUTS"),
        station: "dereth-testkit::cpu::plugins::scenario_text_inputs_sliders_rich_list_cells_tooltips_and_toasts_work_as_declared",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.ui.the-hosts-own-window-stands-over-every-plugin-window",
        says: "The host's own window (the plugin panel) is drawn as a window of the interface over \
               every plugin window: a press on it there is its own, and the plugin window under it \
               takes nothing.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-HOST-WINDOW"),
        station: "dereth-plugin-ui::lib::host_tests::the_hosts_own_window_stands_over_every_plugin_window_and_takes_the_press_there",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.ui.the-layout-as-text-shows-each-nodes-place-and-size",
        says: "A host with no display lays a window out as text, each character a cell: each node's \
               kind, id, place and size, a labelled control's label, a list's columns and the \
               rows it shows, and the height a scrolling body scrolls through; the headless \
               client prints it with `ui layout`.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-LAYOUT-TEXT"),
        station: "dereth-headless::dat::presentation::plugin_layout::ui_layout_prints_each_open_windows_nodes_places_and_sizes",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "plugins.ui.the-plugin-bar-is-drawn-and-a-click-on-its-button-closes-the-main-window",
        says: "On the desktop the plugin bar is drawn over the game with a button carrying each \
               plugin's name, and a click on a plugin's button closes its open main window.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-BAR-DRAWN"),
        station: "dereth-client::gpu::ui::plugin_windows::the_plugin_bar_is_drawn_and_a_click_on_its_button_closes_the_main_window",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "plugins.ui.the-plugin-bar-scrolls-when-its-buttons-do-not-fit-and-takes-the-width-given-it",
        says: "The plugin bar is never wider than the screen: where its buttons do not all fit it \
               scrolls them along, by the wheel over it or its steps, as far as the last; the \
               player widens or narrows it by its grip, and it keeps that width.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-BAR-SCROLL"),
        station: "dereth-plugin-ui::lib::frame_tests::the_plugin_bar_scrolls_when_its_buttons_do_not_fit_and_takes_the_width_given_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.ui.the-plugin-bar-shows-each-plugin-and-toggles-its-main-window",
        says: "The plugin bar lists each plugin active in the session's part with what it says it \
               is doing (running, paused, idle; error when stopped on a fault) and whether its \
               main window is open; a click opens or closes that window and the plugin hears it.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-BAR"),
        station: "dereth-testkit::cpu::plugins::scenario_the_plugin_bar_shows_each_plugin_and_toggles_its_main_window",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.ui.the-plugin-panel-is-drawn-as-a-window-of-each-interface",
        says: "On the desktop the plugin panel is drawn as a window of each interface: in the \
               game's own frame under the modern interface, in the classic interface's own art \
               under it, and in Horizon's art under Horizon.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-PANEL-LOOK"),
        station: "dereth-client::gpu::ui::plugin_windows::the_plugin_panel_is_drawn_as_a_window_of_each_interface",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "plugins.ui.unlocked-a-huds-place-is-the-players-kept-on-the-screen",
        says: "While the player has the HUDs unlocked, a press on a plugin's HUD moves it where \
               it is dragged, outlined; its place is kept for the character and given back at the \
               next start, and the HUD is kept wholly on the screen as the screen and the scale \
               change. Locked again, it stays where it was put and only its controls take the \
               pointer.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-HUD-UNLOCK-PLACE"),
        station: "dereth-plugin-ui::lib::frame_tests::an_unlocked_hud_is_dragged_kept_where_it_is_put_and_stays_on_the_screen",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.ui.where-no-plugin-window-is-drawn-a-toast-is-told-in-the-chat",
        says: "Under an interface that draws no plugin windows, each toast is a line in the \
               chat, once.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-TOAST-CHAT"),
        station: "dereth-plugin-host::lib::tests::where_no_plugin_window_is_drawn_a_toast_is_told_in_the_chat_once",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.ui.where-nothing-says-the-last-list-or-text-box-grows",
        says: "Where no node of a window says `grow`, its last list or text box grows, and on each \
               tab's page the same, page by page; what holds a growing node grows with it.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-LAYOUT-AUTO-GROW"),
        station: "dereth-testkit::cpu::plugins::scenario_where_nothing_says_the_last_list_or_text_box_grows",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.ui.while-the-huds-are-unlocked-a-huds-controls-take-no-input",
        says: "While the player has the HUDs unlocked (the plugin panel's toggle, or `/plugin \
               huds unlock`), a HUD's controls take no input and its plugin hears nothing of a \
               press on one; locked again, they answer.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-HUD-UNLOCK-INPUT"),
        station: "dereth-testkit::cpu::plugins::scenario_while_the_huds_are_unlocked_a_huds_controls_take_no_input",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "plugins.ui.with-tabs-only-the-pages-content-scrolls",
        says: "A window with tabs too short for the chosen page keeps its row of tabs where it is \
               and scrolls the page alone: its nodes are laid out at their least height, as wide \
               as the page less the scrollbar's room, and the wheel, the scrollbar and the pad's \
               focus move through them.",
        since: THIS_CLIENT,
        divergence: "CD-plugins",
        evidence: Evidence::Private("AC-EVID-PLUGINS-LAYOUT-TABS-SCROLL"),
        station: "dereth-testkit::cpu::plugins::scenario_with_tabs_only_the_pages_content_scrolls",
        tier: Tier::Cpu,
    },
];
