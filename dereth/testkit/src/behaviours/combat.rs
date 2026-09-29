//! Combat -- readiness, the swing, and the mode toggle.
//!
//! What the combat table the login carried says a body is ready for, what the power bar fires,
//! and what leaving combat mode puts on the wire.
//!
//! One file per subject, so that two changes adding rows at the same time do not edit the same
//! file. [`ROWS`] is in id order; the registry's own test asserts that, and that no id and no
//! evidence handle is repeated anywhere in it.

// `behaviour!` is `#[macro_export]`ed by `mod.rs` above this module's declaration, so it is in
// textual scope here and needs no import.
use super::{Behaviour, Evidence, Tier, RETAIL};

/// This subject's rows, in id order.
pub static ROWS: &[Behaviour] = &[
    behaviour! {
        id: "combat.advanced.a-release-that-reaches-the-shard-holds-the-strip-until-the-answer",
        says: "A release whose swing really reaches the shard leaves the strip up and frozen at the \
               power it went out at; it is the shard's answer to that swing, and not the release, \
               that takes it down. So the strip going on release is true of a release that sends \
               nothing and not of one that does, and the two tails are a frame apart in play.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O490-SEND"),
        station: "dereth-testkit::dat::combat::scenario_a_release_that_reaches_the_shard_holds_the_strip_until_the_answer",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "combat.advanced.the-option-chooses-which-display-is-live-and-only-one-ever-is",
        says: "With the advanced interface off the combat window's own meter carries the charge and \
               the strip never comes up or is even written; with it on the strip carries it and the \
               window's meter is never written at all. The option is what chooses, and the two are \
               never both drawing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O490-DISPLAYS"),
        station: "dereth-testkit::dat::combat::scenario_the_advanced_option_chooses_which_display_is_live",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "combat.advanced.the-option-is-off-at-login-and-the-toggle-is-one-bit-on-the-wire",
        says: "A recorded session shows the option arriving off in the character's own description \
               -- the word the shard sent is the shipped default exactly -- and the saves the \
               player's own client sent afterwards differ from it in that one bit and no other, \
               turning it on and then off again. So off by default is read off the wire rather \
               than taken from anybody's word for it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O490-WIRE"),
        station: "dereth-testkit::dat::combat::scenario_the_advanced_option_is_off_at_login_and_the_toggle_is_one_bit",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "combat.advanced.the-option-suppresses-the-classic-combat-window",
        says: "With the advanced interface on, entering combat does not put the classic combat \
               window on the screen and does not raise its cluster of controls either -- although \
               the character really is in that combat mode. With the option off the same change \
               opens both, so the one bit is what decides it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O490"),
        station: "dereth-testkit::dat::combat::scenario_the_advanced_combat_option_suppresses_the_classic_combat_window",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "combat.advanced.the-recorded-attacks-are-charges-after-the-toggle-and-notches-before",
        says: "The swings that recorded session sent before the option went on land on the notches \
               the gauge offers, and the ones after it land nowhere near them and are each a \
               partial hold of the bar -- which is the recorded evidence that the two arms really \
               do release differently, and not an argument about the code. The one earlier swing \
               that is off the notches arrives as a pair at one instant, which is a different \
               thing and is named rather than tolerated.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O490-ATTACKS"),
        station: "dereth-testkit::dat::combat::scenario_the_recorded_attacks_are_charges_after_the_toggle_and_notches_before",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "combat.advanced.the-strip-comes-up-while-an-attack-key-is-held-and-goes-on-release",
        says: "With the advanced interface on, holding an attack key raises the standalone strip -- \
               only the one that hears the charge notices; the other stays down, so the bar is never \
               drawn twice -- and it charges while the key is held. \
               Letting go takes it down again and empties it, and the classic window never appears \
               at any point.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O490-STRIP"),
        station: "dereth-testkit::dat::combat::scenario_the_strip_comes_up_while_an_attack_key_is_held_and_goes_on_release",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "combat.advanced.turning-the-option-on-while-the-window-is-up-takes-it-down",
        says: "Turning the advanced interface on while the classic window is already on the screen \
               takes it down there and then, rather than leaving it until the player next changes \
               combat mode; turning the option off again brings it back.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O490-EDGE"),
        station: "dereth-testkit::dat::combat::scenario_turning_the_advanced_option_on_while_the_window_is_up_takes_it_down",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "combat.attack-done.a-clean-acknowledgement-rearms-the-loop-and-sends-nothing",
        says: "When the shard says the swing finished cleanly, the automatic attack stays armed and \
               the bar starts filling again by itself; nothing is cancelled and nothing new goes to \
               the shard. The rebuilt bar fills and then stops, because on an automatic repeat it is \
               the shard that keeps swinging and the player's client that waits.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1125-ZERO"),
        station: "dereth-testkit::dat::combat::scenario_a_clean_acknowledgement_rearms_the_loop",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "combat.attack-done.a-moved-slider-refires-the-swing-at-its-new-power",
        says: "A player who moves the power slider while a swing is still in flight gets one more \
               swing at the new setting the moment the shard answers, and the setting he asked for \
               is the one it is sent at. A slider left where it was refires nothing at all.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1125-SLIDER"),
        station: "dereth-testkit::dat::combat::scenario_a_moved_slider_refires_at_its_new_power",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "combat.attack-done.any-other-value-cancels-the-repeat-and-empties-the-bar",
        says: "Any answer from the shard other than a clean finish stops the automatic attack: the \
               repeat is disarmed, exactly one cancel goes out, the power bar is emptied and taken \
               down, and both of the flags that say an attack is under way are cleared whatever the \
               answer said.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1125-NONZERO"),
        station: "dereth-testkit::dat::combat::scenario_any_other_acknowledgement_cancels_the_repeat",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "combat.attack-done.the-acknowledgement-writes-no-line-whatever-it-carries",
        says: "The shard's answer to a swing never writes a line in the player's chat window, for \
               any value it can carry -- a miss or an evasion is told to him by the messages that \
               arrive beside it -- and the client is shown to have handled every one of those values \
               rather than dropped them.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1125-SILENT"),
        station: "dereth-testkit::dat::combat::scenario_the_acknowledgement_writes_no_line",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "combat.attack-hook.a-landed-attack-hook-plays-the-attackers-default-script",
        says: "When a swing's moment of attack finds a body in front of the attacker, the client \
               plays the attacker's own default effect if the attacker is marked to script \
               collisions, and leaves the body that was hit exactly where and as it was; the \
               damage is the shard's business.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F68-ATTACK-HOOK"),
        station: "dereth-client::gpu::combat::attack_hook::seam::an_attack_hook_that_lands_plays_the_attackers_default_script",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "combat.attack.a-click-charges-the-bar-and-the-swing-comes-when-it-fills",
        says: "A single click on an attack control starts the power bar charging and the swing is \
               sent on the frame the bar reaches the setting the slider is at -- not on the release, \
               and not on any frame before it. One click is one swing: nothing re-fires it on its own.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O431-CLICK"),
        station: "dereth-testkit::dat::combat::scenario_one_click_charges_the_bar_and_swings_when_it_fills",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "combat.attack.a-held-control-charges-to-full-and-swings-only-on-release",
        says: "Holding an attack control down asks for full power: the bar charges past the slider's \
               setting to the top and nothing is sent however many frames pass, and letting go swings \
               at the level reached and again clamped to the slider -- which is how a hold and a click \
               are told apart.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O431-HOLD"),
        station: "dereth-testkit::dat::combat::scenario_a_held_control_charges_to_full_and_swings_on_release",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "combat.attack.every-height-key-moves-the-height-in-both-modes-and-the-swing-carries-it",
        says: "Each of the three attack-height keys sets that height and starts the charge, in the \
               mode that swings and in the mode that shoots alike, and letting go ends the request \
               without moving the height again. A swing carries the height that is **set** and not \
               the key that was let go of, which is only visible when the two differ.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O410-HEIGHT"),
        station: "dereth-testkit::dat::combat::scenario_every_height_key_moves_the_height_in_both_modes",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "combat.attack.holding-a-height-key-refuses-once-and-not-once-per-repeat",
        says: "Holding an attack key down with nothing to swing at refuses once per frame while \
               nothing has been latched, and once something has been latched every later repeat of \
               the same height does nothing at all -- so a player holding the key is not told off \
               once a frame. Asking for a different height runs the whole thing again, and even \
               then the charge already running is not restarted.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O410-REPEAT"),
        station: "dereth-testkit::dat::combat::scenario_holding_a_height_key_refuses_once_and_not_once_per_repeat",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "combat.attack.the-window-button-and-the-key-start-the-same-charge",
        says: "The attack-height buttons in the combat window and the attack-height keys are one \
               control: pressing either leaves the same requested height, the same charge under way, \
               started at the very same instant, and the attack request itself ended in both.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O431-TRIGGERS"),
        station: "dereth-testkit::dat::combat::scenario_the_button_and_the_key_start_the_same_charge",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "combat.auto-attack.a-release-a-turn-and-an-idle-walk-cancel-nothing",
        says: "Letting the backward key go, turning on the spot, and walking backwards with no attack \
               running all move the body and cancel nothing: a release is not a new movement, a turn \
               is not one either, and a walk with nothing in flight raises the edge and is refused, \
               which is why ordinary walking does not spray cancels at the shard.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1105-CONTROLS"),
        station: "dereth-testkit::dat::combat::scenario_a_release_a_turn_and_an_idle_walk_cancel_nothing",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "combat.auto-attack.walking-backwards-breaks-a-repeating-swing",
        says: "Pressing the backward key during a repeating melee attack breaks it: the body walks \
               backwards as it always did, the repeat is disarmed, and exactly one cancel and no \
               further swing reaches the shard -- after which the answer to the swing already in \
               flight arms nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1105"),
        station: "dereth-testkit::dat::combat::scenario_walking_backwards_breaks_a_repeating_swing",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "combat.damage-line.a-critical-a-sneak-and-a-reckless-swing-each-say-so",
        says: "A critical hit, an overpowering hit, a sneak attack and a reckless swing each \
               announce themselves in front of the sentence, in their own words for the player's \
               own swings and for the ones he \
               takes, and in one order when more than one applies. A target whose augmentation \
               turned the critical aside adds a whole sentence after the damage instead.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1124-PREFIX"),
        station: "dereth-testkit::dat::combat::scenario_a_critical_a_sneak_and_a_reckless_swing_each_say_so",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "combat.damage-line.both-lines-reach-the-window-and-a-squelch-stops-them",
        says: "Both damage lines reach the main chat window's log and the element the player reads, \
               with the trailing line break trimmed off; and a player who has squelched the combat \
               channel gets neither -- they are not composed at all rather than composed and \
               dropped, which is counted so the silence can be told from a lost message.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1124-DRAWN"),
        station: "dereth-testkit::dat::combat::scenario_both_lines_reach_the_window_and_a_squelch_stops_them",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "combat.damage-line.says-what-was-hit-how-hard-and-with-what",
        says: "The line for a swing names the target, the verb, the number of points and the kind of \
               damage, and the line for a swing taken names the attacker and the body part as well; \
               exactly one point drops the plural and none takes it; several kinds of damage are \
               joined in the client's own order, and a drain of health, stamina or mana names no \
               kind at all. Every body part the shard can name has its own word.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1124"),
        station: "dereth-testkit::dat::combat::scenario_a_damage_line_says_what_was_hit_how_hard_and_with_what",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "combat.damage-line.the-verb-steps-at-the-thresholds-and-an-unranked-hit-just-hits",
        says: "The verb in a damage line is chosen by how much of the target's health the blow took, \
               in four steps per damage kind, and a blow landing exactly on a step falls in the \
               weaker one of the two. A share the shard reports as below nothing, and a kind of \
               damage with no verbs of its own, both leave the plain word hit.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1124-ADJECTIVES"),
        station: "dereth-testkit::dat::combat::scenario_the_verb_steps_at_the_thresholds",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "combat.input-map.a-running-client-swaps-the-keys-the-frame-after-the-mode-changes",
        says: "A running client whose combat mode changes -- because the shard said so, with no \
               request of its own going out -- swaps its keys on the next frame rather than on the \
               call, and swaps them back when the player leaves combat. Before the change it \
               carries none of the three, which is what makes the swap visible at all.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O349-LIVE"),
        station: "dereth-testkit::dat::combat::scenario_a_running_client_swaps_the_keys_the_frame_after_the_mode_changes",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "combat.input-map.every-shipped-quickslot-key-reaches-the-toolbar-except-the-nine-magic-takes",
        says: "Every key the shipped bindings give the quick bar reaches the quick bar and resolves \
               to the slot it names, in peace and in both of the two combat modes that do not bind \
               the number row. In magic combat the plain number keys cast spells instead -- and \
               that is a key taken by another map, not a key that reaches nothing -- while every \
               key held with a modifier still reaches the quick bar. The quick bar is registered \
               ahead of the window commands, which is what decides the ones they share.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O350"),
        station: "dereth-testkit::dat::combat::scenario_every_shipped_quickslot_key_reaches_the_toolbar",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "combat.input-map.exactly-one-is-registered-and-it-follows-the-mode",
        says: "Exactly one set of combat keys is live at a time and it is the one the player's \
               current mode names: at peace none of the three is up at all, entering a mode puts \
               that mode's own in front of everything else, entering it again moves nothing, and \
               leaving combat takes it away again. None of the three is up when the client starts.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O349"),
        station: "dereth-testkit::dat::combat::scenario_exactly_one_combat_map_is_registered_and_it_follows_the_mode",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "combat.input-map.one-set-of-keys-means-three-different-things-and-the-mode-picks-which",
        says: "The keys all three combat modes share mean three different things, and no meaning is \
               reachable in more than one mode: in one mode they are the swing's height and power, \
               in another the shot's aim and accuracy, and in the third the spell bar's own \
               commands. That is what makes one set of keys behind a mode switch a measurement \
               rather than a design note.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O410-KEYS"),
        station: "dereth-testkit::dat::combat::scenario_one_set_of_keys_means_three_different_things",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "combat.input-map.the-combat-map-itself-is-always-up-and-never-moves",
        says: "The keys that belong to combat itself rather than to one mode are up throughout, \
               listed once however many times the mode changes and never moved to the front -- \
               which is what keeps the mode's own keys in front of them. Returning to peace leaves \
               the whole arrangement exactly as it started, and a mode of no mode at all selects \
               none of the three.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O349-STABLE"),
        station: "dereth-testkit::dat::combat::scenario_the_combat_map_itself_is_always_up_and_never_moves",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "combat.input-map.the-mode-decides-which-map-takes-the-contested-keys",
        says: "The keys all three combat modes bind are taken by the mode the player is in: press \
               each of them in each mode and the mode's own set answers every one, the other two \
               answer none, and at peace they reach nothing at all -- while a key that belongs to \
               combat itself still answers in every mode, so the silences are a registration \
               answer and not a dead keyboard.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O349-KEYS"),
        station: "dereth-testkit::dat::combat::scenario_the_mode_decides_which_map_takes_the_contested_keys",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "combat.mode.a-change-in-the-model-opens-the-window-and-closing-combat-shuts-it",
        says: "The combat window and the cluster of controls that goes with it are down while the \
               player is at peace, come up together the moment his combat mode changes, and go down \
               again when he leaves combat -- so the window follows the mode rather than being \
               opened once and left.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O290"),
        station: "dereth-testkit::dat::combat::scenario_a_combat_mode_change_in_the_model_opens_the_window",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "combat.mode.a-change-the-body-is-not-ready-for-is-queued-and-nothing-is-sent",
        says: "A stance change asked for while the body is not in a position to make it is parked \
               rather than sent: the mode the player asked for is remembered, the mode he is standing \
               in does not move, and nothing at all is built or reaches the shard. The frame asks the \
               body itself every pass rather than trusting an answer taken earlier.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O450-MODE"),
        station: "dereth-testkit::dat::combat::scenario_a_stance_change_the_body_refuses_is_queued",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "combat.mode.an-update-that-is-not-this-players-or-is-older-than-the-last-is-refused",
        says: "A mode the shard sends about somebody else, about another of this character's own \
               numbers, or stamped older than the one already applied, leaves the mode where it \
               was and is counted as stale rather than silently dropped; a message that does not \
               decode is counted too, and neither kind is mistaken for the other.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O290-STALE"),
        station: "dereth-testkit::dat::combat::scenario_an_update_that_is_not_this_players_or_is_older_is_refused",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "combat.mode.at-peace-no-combat-key-reaches-an-arm",
        says: "The same key, the same character and the same press reach an arm in a combat mode \
               and reach nothing at all at peace -- which is the rung of the ladder that makes \
               every other silence in combat mean something.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O410-PEACE"),
        station: "dereth-testkit::dat::combat::scenario_at_peace_no_combat_key_reaches_an_arm",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "combat.mode.the-recorded-changes-reach-the-window-the-buttons-and-the-keys",
        says: "Every mode change the recorded sessions really carry reaches the client the same \
               way: the model takes it, the keys for that mode are registered, the toolbar lights \
               the button for that mode and no other, the combat cluster is up for the two modes \
               that fight and the casting page for the one that casts -- and nothing is sent back, \
               because the shard is the one that said so.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O290-CORPUS"),
        station: "dereth-testkit::dat::combat::scenario_the_recorded_combat_mode_changes_reach_the_window_the_buttons_and_the_keys",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "combat.mode.the-shards-own-word-opens-the-window-and-the-client-sends-nothing",
        says: "When the shard says this character's combat mode has changed, the client takes it \
               without asking for it back: the window and the cluster come up, the keys for that \
               mode are registered, a request the player had pending is left where it was, and \
               nothing at all goes out. The public form of the same message about this character \
               is taken the same way, including the return to peace.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O290-AUTHORITY"),
        station: "dereth-testkit::dat::combat::scenario_the_shards_own_word_opens_the_window_and_the_client_sends_nothing",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "combat.mode.the-toggle-out-of-combat-reaches-the-shard",
        says: "Leaving combat mode tells the shard once that the player is at peace, rather than \
               parking the change for ever, and nothing is left pending behind it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F41-MODE"),
        station: "dereth-testkit::cpu::combat::scenario_leaving_combat_reaches_the_shard",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "combat.mode.whether-the-player-may-change-is-decided-by-the-mode-he-is-leaving",
        says: "Whether the player is allowed to change combat mode is decided by the mode he is \
               standing in, never by the one he is asking for. A player who has just gone to missile \
               and whose weapon stance is not up yet has his request to stand down held back, and \
               nothing at all reaches the shard until the stance arrives; with the stance up the \
               identical request goes out at once and the mode really changes.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O476"),
        station: "dereth-testkit::cpu::combat::scenario_leaving_a_mode_is_judged_by_the_mode_being_left",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "combat.notification.an-attack-notice-decodes-with-either-attack-conditions-width",
        says: "Every recorded notice that the player hit something or was hit reads the same \
               whether its attack-conditions field is taken as four bytes or eight: the extra four \
               bytes the shard sends are always zero, and the message written back out is the \
               recorded bytes exactly.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O438-NOTIFICATION"),
        station: "dereth-client-net::cpu::combat::attack_notification_width::every_recorded_body_is_the_clients_read_plus_one_trailing_zero_dword",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "combat.pk-status.live-updates-drive-attackability-radar-and-selection-colour",
        says: "When the shard marks the player and another player as player killers while they \
               play, the two can at once attack each other -- a swing goes out as a targeted \
               attack -- and the other player's radar blip turns player-killer red, unless he is a \
               fellow, whose colour wins.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P197-PK-STATUS"),
        station: "dereth-client::gpu::combat::player_killer_status::live_pk_updates_drive_attack_eligibility_and_the_radar_from_the_same_pwd_bits",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "combat.power-bar.a-jump-raises-the-standalone-bar-and-not-the-classic-one",
        says: "A jump uses the same standalone bar an advanced swing does and leaves the classic \
               one down, so the bar is never drawn twice -- and finishing \
               the jump puts both away and empties them.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-CHARGE-JUMP"),
        station: "dereth-testkit::dat::combat::scenario_a_jump_raises_the_standalone_bar_and_not_the_classic_one",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "combat.power-bar.a-notice-with-no-panel-to-hear-it-is-dropped-and-never-replayed",
        says: "A charge raised while there is no panel to hear it is dropped rather than kept: a \
               screen that comes up afterwards starts empty and down, and charges raised while \
               another screen was up do not animate the rebuilt one. Nothing accumulates in the \
               meantime, so the client cannot grow a journal of them.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-CHARGE-NOSUB"),
        station: "dereth-testkit::dat::combat::scenario_a_notice_with_no_panel_to_hear_it_is_dropped",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "combat.power-bar.a-terminal-answer-empties-the-classic-meter-and-the-next-charge-works",
        says: "An answer from the shard that ends the swing empties the combat window's meter and \
               leaves it empty on the frames after, and the next charge fills it again and swings \
               again -- so the emptying is the answer's doing and not the meter being broken.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-CHARGE"),
        station: "dereth-testkit::dat::combat::scenario_a_terminal_answer_empties_the_classic_meter_and_the_next_charge_works",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "combat.power-bar.a-two-handed-style-charges-a-quarter-faster-over-the-same-frames",
        says: "The same frames at the same clock draw a steeper ramp while the character is \
               fighting in the style that swings faster -- a quarter as much again per frame -- \
               which is what separates a bar wired to the client's own clock from one wired to \
               anything else.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O430-RATE2"),
        station: "dereth-testkit::dat::combat::scenario_a_two_handed_style_charges_a_quarter_faster",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "combat.power-bar.a-value-between-notches-rounds-to-the-nearest-before-stepping",
        says: "A gauge sitting between two notches is rounded to the nearest one before it steps, \
               so stepping up from between them lands on the notch above the nearer one rather \
               than a sixth further along from where it was; stepping down does the same the other \
               way. Both ends stop rather than wrapping.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O410-GAUGE-MATHS"),
        station: "dereth-testkit::dat::combat::scenario_a_value_between_notches_rounds_to_the_nearest_before_stepping",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "combat.power-bar.exactly-one-of-the-two-displays-is-live-and-they-never-differ",
        says: "There are two places a charge can be drawn and exactly one of them is live at a \
               time: in ordinary combat the window's own meter carries it and the standalone bar \
               is never written at all, not even with a nought, and stays down; with the advanced \
               interface the standalone bar carries it and is shown while the window's meter \
               refuses to move. The number shown is the one the combat system last sent, and the \
               same charge drawn either way is the same number -- two views of one value.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O430-EXCLUSIVE"),
        station: "dereth-testkit::dat::combat::scenario_exactly_one_of_the_two_power_displays_is_live",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "combat.power-bar.it-follows-the-clock-at-one-full-charge-a-second",
        says: "The standalone bar follows the client's own clock: a charge rises at one full bar a \
               second, in even steps and strictly upwards, and it is the bar the notices really \
               reach that moves -- the other one in the shipped tree is never written.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O430"),
        station: "dereth-testkit::dat::combat::scenario_the_power_bar_follows_the_clock_at_one_full_charge_a_second",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "combat.power-bar.it-is-shown-when-the-charge-begins-and-hidden-and-emptied-when-it-ends",
        says: "The bar is down before a charge, comes up when one begins -- only the one that hears \
               the notices, the other stays down -- charges while it is up, and goes down again \
               emptied when the charge ends, with the mode it was showing cleared.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O430-EDGES"),
        station: "dereth-testkit::dat::combat::scenario_the_power_bar_is_shown_when_a_charge_begins_and_emptied_when_it_ends",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "combat.power-bar.the-bodys-motion-style-sets-the-charge-rate",
        says: "The power bar's charge time follows the stance the player's own body is in: a full \
               second out of combat, eight tenths of a second once the body takes up the \
               dual-wield stance, and a full second again when it leaves it, with nothing else \
               setting it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O410-POWER-BAR"),
        station: "dereth-client::gpu::combat::motion_style_charge_rate::the_local_bodys_motion_style_reaches_the_combat_system_and_changes_the_charge_time",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "combat.power-bar.the-display-holds-the-level-the-swing-went-out-at",
        says: "When the swing goes out the display holds the power it went out at rather than \
               dropping to empty, although the clock the charge was measured against reads nothing \
               from that moment. While the charge is still building the two agree, which is why a \
               display that recomputed instead looked right until the swing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O430-SEAM"),
        station: "dereth-testkit::dat::combat::scenario_the_display_holds_the_level_the_swing_went_out_at",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "combat.power-bar.the-drag-is-continuous-and-lands-between-the-notches",
        says: "Dragging the gauge is continuous where the keyboard is not: one unit of drag moves \
               it by one thousandth, twenty consecutive units are twenty different values and none \
               of them is a notch, and the two ends saturate rather than wrapping -- including a \
               value so large it could only have come from a negative number read the other way.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O430-DRAG"),
        station: "dereth-testkit::dat::combat::scenario_the_power_drag_is_continuous_and_lands_between_the_notches",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "combat.power-bar.the-gauge-keys-step-by-a-sixth-in-both-modes-and-saturate",
        says: "The two gauge keys step the cap by a sixth each press, from the half it starts at, \
               in the mode that swings and the mode that shoots alike -- and they stop at full and \
               at empty rather than going past.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O410-GAUGE"),
        station: "dereth-testkit::dat::combat::scenario_the_gauge_keys_step_by_a_sixth_in_both_modes",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "combat.power-bar.the-keyboard-gauge-has-seven-notches-and-starts-half-way",
        says: "The gauge the keyboard moves has seven positions, evenly spaced from empty to full, \
               and a fresh one starts exactly half way -- which is itself one of the seven, so \
               stepping up and back down returns to it. Both ends stop rather than wrapping round.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O430-NOTCHES"),
        station: "dereth-testkit::dat::combat::scenario_the_keyboard_gauge_has_seven_notches_and_starts_half_way",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "combat.power-bar.the-notch-and-the-fill-are-different-things-on-different-elements",
        says: "How much power the player has asked for and how much has built up are two different \
               marks on two different parts of the window, and they hold different numbers at the \
               same time -- so a claim about the one the player drags is not a claim about the one \
               the clock fills.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O430-TWO-MARKS"),
        station: "dereth-testkit::dat::combat::scenario_the_notch_and_the_fill_are_different_things",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "combat.power-bar.the-standalone-bar-and-the-classic-meter-are-separate-routes",
        says: "A charge meant for the standalone bar never writes the combat window's meter, which \
               keeps whatever the last classic charge left on it; each of the three things that can \
               happen to a charge reaches the panel that hears them even when the state they end \
               in is identical, so the panel is told what happened and not merely shown the result; \
               and a frame on which nothing happened tells it nothing again.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-CHARGE-ROUTES"),
        station: "dereth-testkit::dat::combat::scenario_the_standalone_bar_and_the_classic_meter_are_separate_routes",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "combat.projectile.a-bolt-stops-being-a-missile-when-it-hits",
        says: "A bolt that reaches the ground stops being a missile the moment it lands: its three \
               in-flight marks come off and nothing else about it changes, and it stops dead there \
               rather than bouncing or sliding on.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F50-PROJECTILE"),
        station: "dereth-client::gpu::combat::projectile_impact::a_bolt_that_reaches_the_ground_stops_being_a_missile",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "combat.projectile.an-impact-plays-the-projectiles-default-script",
        says: "When a spell bolt hits, the client plays the bolt's own default effect, because the \
               bolt is marked to script its collisions; that effect is the client's whole visible \
               answer to the hit.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F50-PROJECTILE-IMPACT"),
        station: "dereth-client::gpu::combat::projectile_impact::an_impact_calls_the_projectiles_default_script",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "combat.readiness.a-melee-swing-needs-the-weapons-own-combat-table",
        says: "A melee swing needs the table the wielded weapon's own data carries. Without it the \
               same body is refused both a swing and a stance change, and the only thing that has to \
               change for the identical body to be allowed to swing is that the table is there.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O457-TABLE"),
        station: "dereth-testkit::dat::combat::scenario_a_melee_swing_needs_the_weapons_combat_table",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "combat.readiness.a-missile-swing-needs-the-stance-up-and-the-body-standing-ready",
        says: "A missile swing wants two things and neither excuses the other: the character's stance \
               has to be one the shipped table accepts -- a settled character stands in the peace one \
               until he draws -- and the body has to be standing in its ready command rather than \
               walking. With the stance up and the body standing, the same request goes through.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O457-MISSILE"),
        station: "dereth-testkit::dat::combat::scenario_a_missile_swing_needs_the_stance_and_the_ready_command",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "combat.readiness.a-swing-and-a-stance-change-ask-different-questions-of-one-body",
        says: "Whether a player may swing and whether he may change stance are two different questions \
               of the same body at the same instant: with a motion outstanding the swing is allowed \
               and the stance change is not. Standing still the two agree, and they agree again once \
               the motion drains, so the disagreement is a reading rather than a latch.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O457"),
        station: "dereth-testkit::dat::combat::scenario_the_two_flavours_disagree_on_one_body",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "combat.readiness.is-the-bodys-own-motion-queue-and-not-whether-it-is-on-the-ground",
        says: "Whether the player is in a position to act is decided by whether his body still has a \
               motion outstanding, never by whether it is standing on the ground. A body that jumps \
               stops being ready and becomes ready again while it is still in the air; a body walking \
               on flat ground is in contact and is not ready; and a player with no body at all is \
               answered no outright rather than left unknown.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O450"),
        station: "dereth-testkit::dat::combat::scenario_ready_is_the_bodys_own_motion_queue",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "combat.readiness.the-window-the-keyboard-and-the-shards-reply-all-ask-the-body",
        says: "All three ways an attack can start -- the combat window's own button, the attack key \
               through the frame, and the shard's answer re-arming an automatic repeat -- ask the body \
               the same question and refuse the same state. The shard's answer asks it again at the \
               moment the message arrives rather than carrying the answer from the press.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O457-CONSUMERS"),
        station: "dereth-testkit::dat::combat::scenario_all_three_attack_arms_consume_the_produced_flavour",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "combat.swing.the-attack-cone-reports-each-target-in-reach-once",
        says: "A swing at a target standing straight ahead within reach hits it exactly once, in \
               one height band and on one side, and from behind when the attacker is behind it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F68-SWING"),
        station: "dereth-physics::cpu::attack::attack_cone::a_target_straight_ahead_is_reported_once",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "combat.window.choosing-one-height-leaves-every-other-button-alone",
        says: "Choosing an attack height clears the one that was chosen before and touches no \
               other button, however deep in the panel the chosen one sits; naming a button the \
               panel does not have clears the old one and chooses nothing in its place, and \
               re-reading the choice with nothing new to say puts the same one back rather than \
               losing it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-COMBAT-GROUP"),
        station: "dereth-testkit::dat::combat::scenario_choosing_one_height_leaves_every_other_button_alone",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "combat.window.every-control-does-what-the-client-does",
        says: "Every control in the combat window reaches the model through the client's own seam: \
               pressing an attack-height button sets that height and starts the charge and \
               releasing it swings, dragging the gauge sets the cap in thousandths and clamps it, \
               and each of the three option boxes names its own option and its new value -- which \
               reaches the character's options and is queued for the shard. A drag outside the \
               window does not move the gauge. The two things an option is supposed to make the \
               game do are counted as not yet done rather than half done.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O410-WINDOW"),
        station: "dereth-testkit::dat::combat::scenario_every_control_in_the_combat_window_does_what_the_client_does",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "combat.window.it-reflects-the-height-the-power-and-the-notch",
        says: "The window follows what the model tells it: the chosen height lights its own button \
               and moves when the height moves, the meter takes a charge that belongs to it and \
               refuses one that does not, and the gauge's mark takes the number it is given. Being \
               told the same thing twice writes nothing, and being told of a height that is no \
               height leaves everything where it was.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O410-READBACK"),
        station: "dereth-testkit::dat::combat::scenario_the_combat_window_reflects_the_height_the_power_and_the_notch",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "combat.window.pressing-the-chosen-height-again-keeps-it-chosen",
        says: "Pressing the attack height that is already chosen leaves it chosen rather than \
               turning it off -- which matters because the press itself has already turned the \
               button off by the time the panel hears about it. A panel told not to accept a \
               repeat does not pass the press on, and still leaves the button lit; and it does not \
               stop a press on a different height being accepted.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-COMBAT-RESELECT"),
        station: "dereth-testkit::dat::combat::scenario_pressing_the_chosen_height_again_keeps_it_chosen",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "combat.window.the-attack-height-buttons-follow-the-notice-and-draw-the-chosen-one",
        says: "The combat window opens on the middle attack height, as the shipped panel says, and \
               each change of height lights that button with the picture the shipped data gives a \
               chosen one and leaves the other two with the plain picture -- in the draw list and \
               not only in the panel's own state. A change to no height at all is refused and \
               leaves the height where it was.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-COMBAT-HEIGHT"),
        station: "dereth-testkit::dat::combat::scenario_the_attack_height_buttons_follow_the_notice",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "combat.window.the-frames-own-pass-drives-the-read-backs",
        says: "Those read-backs are driven by the client's own per-frame pass and not by something \
               only a test calls: one frame of a fresh character draws the middle height, a frame \
               of a character part way through a charge draws the height, the charge and the mark, \
               and a frame that says the same thing again writes nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O410-FRAME"),
        station: "dereth-testkit::dat::combat::scenario_the_frames_own_pass_drives_the_combat_windows_read_backs",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "combat.window.the-option-boxes-draw-their-shipped-captions-and-carry-their-shipped-help",
        says: "The three option boxes in the combat window draw the words the shipped text table \
               gives them -- as glyphs really put in the draw list, not only as text on the element \
               -- and each carries the shipped help line for the option it stands for.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-COMBAT-OPTIONS"),
        station: "dereth-testkit::dat::combat::scenario_the_combat_option_boxes_draw_their_shipped_captions",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "combat.window.the-recklessness-meter-appears-at-exactly-the-trained-class",
        says: "The recklessness meter is hidden until the character's skill in it reaches the \
               trained class and is shown from there on -- exactly at it, not one past it, which \
               is the boundary a check of nothing and of plenty cannot tell apart.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O410-RECKLESS"),
        station: "dereth-testkit::dat::combat::scenario_the_recklessness_meter_appears_at_exactly_the_trained_class",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "melee.attack.swings-when-the-power-bar-fills-and-repeats",
        says: "A click with a target selected charges the power bar and, when it fills, sends exactly \
               one melee attack naming that target, at the chosen height and at the power the slider \
               was set to. While the shard repeats the attack for the player nothing further is sent, \
               and the next click swings again.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F41-SWING"),
        station: "dereth-testkit::cpu::combat::scenario_a_melee_attack_swings_and_repeats",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "melee.readiness.comes-from-the-combat-table-the-login-carried",
        says: "Whether the player may swing at all is decided by the combat table the login description \
               carries, and that table reaches the game model; a description that arrives before the \
               player's own body does is kept and installed when the body appears.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F41"),
        station: "dereth-testkit::cpu::combat::scenario_readiness_comes_from_the_combat_table",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "movement.stance.a-body-that-is-in-its-stance-can-attack-and-can-leave-combat-mode",
        says: "Whether the player can attack, and whether he can put his weapon away, are both decided by \
               whether his body is standing in its stance. A body that is in one gets both, and each reaches \
               the shard as its own message rather than being quietly parked.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F32-READY"),
        station: "dereth-testkit::dat::world::motion::scenario_a_body_in_its_stance_can_attack_and_leave_combat",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.stance.the-one-the-shard-sends-for-a-thrown-weapon-reaches-the-body",
        says: "The stance the shard sends for a thrown weapon arrives as a stance and puts the player into \
               it, rather than being filed as some unrelated pose and leaving him standing there.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F32-STANCE"),
        station: "dereth-testkit::dat::world::motion::scenario_the_thrown_weapon_stance_reaches_the_body",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "vitals.row.the-numbers-are-drawn-into-the-shipped-template-in-both-layouts",
        says: "The health, stamina and mana rows are drawn by filling the shipped template with \
               the current number and the maximum, in both the stacked and the side-by-side \
               layouts at once, and a frame whose numbers have not moved does not rewrite them.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-VITALS"),
        station: "dereth-testkit::dat::combat::scenario_the_vital_rows_are_drawn_into_the_shipped_template",
        tier: Tier::Dat,
    },
];
