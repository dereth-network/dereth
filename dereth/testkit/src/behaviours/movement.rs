//! Movement -- the player's own motion and other bodies' motion as the client shows it.
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
        id: "animation.sequence.parts-step-with-no-interpolation-and-hooks-fire-on-crossed-frames",
        says: "An animated body's parts hold each frame's pose until the next frame and never \
               blend between frames, however finely time is sampled.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-FRAME-TRACE-SEQUENCE"),
        station: "dereth-animation::dat::sequence::frame_crossing_trace::part_placement_is_a_step_function_of_time_with_no_interpolation",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.animation.the-players-own-animation-never-flaps-between-two-frames",
        says: "The player's own running or sidestepping animation never flips back and forth \
               between two poses at any frame rate: the body keeps its footing, the animation only \
               moves forward through its frames, and no pose ever returns right after being left.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F25-ANIMATION"),
        station: "dereth-client::gpu::movement::player_animation_stability::the_players_own_animation_does_not_oscillate_between_two_frames_at_any_frame_rate",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "movement.command-table.every-wire-index-resolves-to-the-command-the-dat-names",
        says: "Every motion command the shipped game data names, looked up from the short number \
               the shard sends on the wire, resolves to exactly the command the data gives it, \
               with no correction list.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F32-COMMAND-TABLE"),
        station: "dereth-animation::dat::commands::command_ids_match_dat::every_command_the_shipped_dat_names_resolves_to_the_id_the_dat_gives_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.contact.a-walking-body-stops-at-a-ledge-instead-of-walking-off",
        says: "A character walking forward at the edge of a ledge slides along it and keeps its \
               footing instead of walking off: it stays on the ground on every frame and never \
               drops below the ledge.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P145-CONTACT"),
        station: "dereth-client::gpu::movement::ledge_edge_slide::walking_forward_off_a_static_ledge_keeps_contact_and_does_not_fall",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "movement.door.an-accepted-door-turn-unsticks-the-previous-target",
        says: "When the shard's movement for using a door arrives while the player is still stuck \
               to an earlier combat target, the client first cancels and unsticks from that old \
               target and only then begins the turn toward the door, so the old follow cannot \
               survive the new command.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-DOOR-RECOVERY-DOOR"),
        station: "dereth-client::dat::movement::door_turn_and_open::actual_app::accepted_door_movement_unsticks_a_prior_combat_target_before_the_new_turn",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.emote.every-emote-and-stance-key-animates-the-human-body",
        says: "Every emote and stance command a key can issue is one the human body's shipped \
               animation table can play, as a looping pose or a one-off action, so no emote key \
               leaves the character standing still.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O961-EMOTE"),
        station: "dereth-client::dat::movement::emote_motion_keys::every_emote_and_stance_key_animates_the_human_body",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.era.an-older-worlds-client-sends-its-motion-in-its-files-numbering",
        says: "On a world of older data files the client sends its stance and its movement \
               commands numbered as those files and the client of their day number them (sitting \
               in the February 2005 world is that client's sitting, not the later one's), and on \
               the end-of-retail world exactly as before.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-MOTION-NUMBERING-WIRE"),
        station: "dereth-client::dat::movement::older_world_motions::on_an_older_world_the_client_sends_its_motion_state_in_its_files_numbering",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.era.an-older-worlds-logout-plays-its-own-departure",
        says: "On a world of older data files the server's logout, numbered as those files number \
               it, plays the departure those files give it on the body (the February 2005 one on \
               that world), and the end-of-retail logout plays the end-of-retail departure as \
               before.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-MOTION-NUMBERING-LOGOUT"),
        station: "dereth-client::dat::movement::older_world_motions::on_an_older_world_the_servers_logout_plays_the_departure_its_files_give_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.era.every-emote-key-animates-an-older-worlds-human-body",
        says: "On the world of February 2005 every emote and stance key animates the human body \
               from that world's own animation table, sitting, pointing and the snow angel \
               included, though those files number them differently from the end of retail.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-MOTION-NUMBERING-EMOTES"),
        station: "dereth-client::dat::movement::older_world_motions::every_emote_key_animates_the_february_2005_human_body",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.focus.losing-focus-leaves-a-server-approach-and-the-run-lock-running",
        says: "Switching away from the game window does not stop movement the shard is driving: a \
               character on a shard-directed approach keeps walking after the window loses focus \
               and the approach is not cancelled.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F48-FOCUS"),
        station: "dereth-client::dat::movement::focus_loss::unfocusing_the_client_during_a_server_move_to_leaves_the_approach_running",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.focus.losing-focus-stops-self-powered-walking",
        says: "A character walking on the player's own keys comes to rest shortly after the game \
               window loses focus, because the held movement is cleared and the body is told to \
               stand.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F48-FOCUS-LOSING"),
        station: "dereth-client::dat::movement::focus_loss::unfocusing_the_client_while_walking_under_your_own_power_does_stop_you",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.ground.the-body-stands-on-the-ground-the-renderer-draws",
        says: "The ground a body stands on is the ground that is drawn: across a dense sweep of a \
               town landblock, the height the physics uses and the height of the drawn terrain \
               triangle under the same point are the same.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-EMBODIED-GROUND"),
        station: "dereth-client::gpu::movement::embodied_character::the_ground_physics_stands_on_is_the_ground_the_renderer_draws",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "movement.hooks.a-set-omega-hook-turns-the-body",
        says: "An animation event that sets the body's spin, fired during a frame, gives the \
               player's body that spin, and the body keeps turning across the following simulated \
               second rather than turning once and stopping.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F69-HOOKS"),
        station: "dereth-client::gpu::movement::animation_hooks::a_set_omega_hook_fired_inside_the_frame_turns_the_physics_body",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "movement.hooks.an-ethereal-hook-reaches-the-physics-object",
        says: "An animation event that makes a body passable really changes its physics: playing a \
               door's open motion on a body makes it passable within the frames that follow, and \
               playing the close motion makes it solid again.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O112-HOOKS"),
        station: "dereth-client::gpu::movement::animation_hooks::an_ethereal_hook_fired_inside_the_frame_reaches_the_physics_object",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "movement.hooks.the-last-set-omega-of-a-step-wins",
        says: "When two spin-setting animation events fire in the same step, both run in order and \
               the body ends with the second one's spin, not the sum of the two.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F69-HOOKS-LAST"),
        station: "dereth-client::gpu::movement::animation_hooks::the_last_set_omega_hook_of_a_step_wins",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "movement.jump-charge.a-cancelled-charge-sends-no-jump",
        says: "Escape cancels a jump that is being charged, so the release that follows sends no \
               jump and the body stays on the ground; a fresh press after the cancel starts a new \
               charge, whatever order the keys arrive in within one frame.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-JUMP-CHARGE-JUMP-CHARGE-CANCELLED"),
        station: "dereth-client::dat::movement::jump_charge_release::escape_then_release_then_fresh_press_in_one_batch_cancels_only_the_old_charge",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.jump-charge.a-press-charges-and-only-the-release-jumps",
        says: "Pressing the jump key only starts charging: the body stays on the ground and no \
               jump is sent. Releasing it sends one jump request and the body leaves the ground \
               with an upward speed, and the floating power bar shows the charge.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-JUMP-CHARGE-JUMP-CHARGE"),
        station: "dereth-client::dat::movement::jump_charge_release::real_space_press_charges_without_launching_and_release_jumps",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.jump.the-jump-skill-is-read-through-the-shipped-quality-filter",
        says: "The jump skill that sets how high a character jumps is worked out from the shipped \
               skill table through the shipped quality filter, giving the values every recorded \
               character description implies; a character with no stamina left has a jump skill of \
               zero, while a missing stamina or jump skill entry means no answer at all rather \
               than zero.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-JUMP-QUALITIES-JUMP-JUMP"),
        station: "dereth-client::dat::movement::jump_skill_inquiry::recorded_jump_quality_inputs_and_real_filter_are_explicit",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.jump.the-shipped-key-starts-and-releases-once-and-key-repeat-does-not-restart-the-charge",
        says: "Jump is bound to the space bar alone in the shipped key map; pressing it starts the \
               jump once and releasing it ends it once, with or without Shift held, and the \
               keyboard's own auto-repeat while it is held does not start the charge again.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-JUMP-QUALITIES-JUMP"),
        station: "dereth-client::dat::movement::jump_key_binding::shipped_jump_binding_preserves_start_release_repeat_and_modifier_semantics",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.keys.releasing-the-second-key-resumes-the-first",
        says: "Holding forward and then pressing backward walks the character backward; releasing \
               backward resumes walking forward on the key still held, and releasing that too \
               stops the character.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O165-KEYS"),
        station: "dereth-client::dat::movement::command_lists::releasing_the_second_movement_key_resumes_the_first",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.legacy-opcodes.the-client-never-sends-the-four-legacy-movement-commands",
        says: "The client never sends Movement_DoMovementCommand, Movement_StopMovementCommand, \
               Movement_TurnToEvent or Movement_AutonomyLevel: a recorded session that walks and \
               jumps sends thousands of movement reports, Movement_MoveToState and \
               Movement_AutonomousPosition among them, and none of those four.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P26C-LEGACY-OPCODES"),
        station: "dereth-client-net::cpu::movement::legacy_movement_opcodes::the_recorded_client_sends_none_of_the_four",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "movement.move-to.a-moving-targets-position-leads-by-velocity-times-the-quantum",
        says: "A creature approaching a moving target aims where the target will be, its position \
               plus its current velocity times the approach's update interval, so a walking player \
               is led rather than chased.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P4-MOVE-TO"),
        station: "dereth-client::dat::movement::move_to_fidelity::a_moving_targets_interpolated_position_leads_by_its_velocity_times_the_quantum",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.move-to.a-remote-creature-walks-toward-the-destination-the-server-names",
        says: "A creature the shard sends toward a destination is really given walk and turn \
               commands: every moving creature in the recorded session has its target and a walk \
               or turn under way, and its pending motions drain instead of stalling.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O456-MOVE-TO"),
        station: "dereth-client::dat::movement::remote_move_to_arms::a_remote_creature_with_a_destination_walks_toward_it",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "movement.position-and-movement.a-stale-position-still-runs-the-movement-half",
        says: "When a combined position-and-movement message carries a position older than the one \
               the object already has, the position is refused and its stamp kept, but the \
               movement the message carries is still applied.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F20-POSITION-AND-MOVEMENT"),
        station: "dereth-client::gpu::movement::vector_and_position_receivers::a_stale_position_still_lets_the_movement_half_run",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "movement.remote.a-bodyless-viewer-animates-without-translating",
        says: "A client that keeps no physics world still plays a creature's animations, letting \
               transitions finish and turns follow, but does not move the creature along the \
               ground by its animation's stride.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-REMOTE-PHYSICS-REMOTE-BODYLESS"),
        station: "dereth-client::gpu::movement::remote_root_motion::bodyless_viewer_retains_its_animation_ladder_without_claiming_physical_translation",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "movement.remote.a-creature-told-to-approach-walks-up-turns-and-stops",
        says: "A creature the shard tells to approach the player turns toward him, walks up until \
               it is within reach, turns to face him and stops, and stays stopped with nothing \
               left pending.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P3-REMOTE"),
        station: "dereth-client::dat::movement::remote_move_to::a_recorded_move_to_object_walks_the_creature_up_turns_it_to_you_and_stops",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.remote.a-recorded-remote-walk-never-reaches-the-falling-animation",
        says: "A creature walking across a floor keeps its footing on every frame of its walk, so \
               it never flickers into the falling animation and its walk animation changes only \
               when a transition runs out.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F12-REMOTE"),
        station: "dereth-client::gpu::movement::remote_walk_animation::a_recorded_remote_walk_raises_no_ground_edges_and_never_reaches_the_falling_animation",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "movement.remote.a-remote-approach-moves-by-dat-root-motion-and-publishes-its-position",
        says: "A creature the shard sends walking toward a target is carried by its walking \
               animation's own stride through collision, rests on the floor without sinking into \
               it, arrives with nothing left pending, and the position the client reports for it \
               is where the body really got to.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-REMOTE-PHYSICS-REMOTE"),
        station: "dereth-client::gpu::movement::remote_root_motion::remote_approach_uses_real_dat_root_motion_and_publishes_achieved_position",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "movement.remote.an-arrow-key-release-clears-remote-turn-state",
        says: "When another player releases an arrow turn key, the stop that reaches this client \
               clears his turn, so his body stops turning and stays stopped through the quiet \
               frames that follow instead of spinning until his next movement.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P198-REMOTE"),
        station: "dereth-client::dat::movement::remote_turn_release::arrow_key_release_bytes_clear_remote_turn_state_during_quiet_physics_ticks",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.remote.position-events-move-objects",
        says: "Objects move when the shard sends their positions: in every recorded session with \
               position events, each one is applied and several objects move repeatedly, over real \
               distances.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-POPULATED-REMOTE"),
        station: "dereth-client::gpu::objects::populated_world::objects_move_from_position_events",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "movement.run-lock.any-movement-key-cancels-the-run-lock",
        says: "The run-lock key toggles auto-run on and off, and turning it off stops the \
               character; pressing a movement key while it is on cancels the run lock but keeps \
               walking on that key until it is released.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O165-RUN-LOCK"),
        station: "dereth-client::dat::movement::command_lists::the_run_lock_starts_on_dik_q_and_any_movement_key_cancels_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.run.the-body-covers-ground-at-the-run-skills-rate",
        says: "A running character covers ground at the rate its Run skill and carried load give: \
               with no character qualities known it runs at the base rate of about four metres a \
               second, and with a real character's skill correspondingly faster.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F44-RUN"),
        station: "dereth-client::dat::movement::run_speed::the_body_covers_ground_at_the_run_skills_rate",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.scenery.a-generated-tree-stops-the-body-and-decoration-does-not",
        says: "Walking into a tree the landscape generates stops the body short of the trunk while \
               it stays on the ground, whereas a decorative scenery piece with no solid shape does \
               not stop it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P169-SCENERY"),
        station: "dereth-client::gpu::movement::scenery_collision::walking_into_a_generated_tree_stops_the_body",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "movement.stance-change.stops-the-body-and-a-held-key-resumes-running",
        says: "Changing stance while a movement key is held stops the character for the stance \
               animation; once it finishes the character moves again on the still-held key by \
               himself, taking control back exactly once.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O478-STANCE-CHANGE"),
        station: "dereth-client::gpu::movement::stance_change_stop_and_resume::a_held_key_survives_the_stance_change_and_resumes_when_it_completes",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "movement.stick.a-remote-creature-stuck-to-an-object-is-pulled-toward-it",
        says: "A creature the shard says is stuck to another object is pulled toward it by the \
               client between server updates until it stands at the stick distance, while every \
               object that is not stuck stays exactly where the shard put it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O455-STICK"),
        station: "dereth-client::gpu::movement::remote_stick_to_object::a_stuck_creature_is_pulled_toward_what_it_is_stuck_to",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "movement.sticky.animation-done-unsticks-before-manual-motion",
        says: "When the animation of an action performed while stuck to a target finishes, the \
               client unsticks the body from that target and drops its interest in it before the \
               player's own movement takes over again.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-MOTION-CONTINUATIONS-STICKY-ANIMATION"),
        station: "dereth-client::dat::movement::motion_continuations::real_animation_done_unsticks_before_returning_to_manual_motion",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.sticky.arrival-sticks-to-the-target-before-the-next-operation",
        says: "When an approach that asked to stick to its target arrives, the client finishes the \
               approach and then sticks the body to that same target at the requested distance, so \
               the stick is in place before anything else runs; unsticking later clears the \
               target.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-MOTION-CONTINUATIONS-STICKY"),
        station: "dereth-client::dat::movement::motion_continuations::arrival_sticks_to_the_captured_target_before_the_next_operation",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.style.a-remote-creatures-stance-follows-the-style-word",
        says: "When the shard sends a creature a move or turn naming a stance it is not in, the \
               creature switches to that stance, here from peaceful to hand-to-hand combat.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O402-STYLE"),
        station: "dereth-client::gpu::movement::remote_motion_style::the_style_word_on_a_remote_arm_changes_the_creatures_stance",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "movement.style.a-repeated-style-word-issues-no-motion-and-a-new-one-does",
        says: "A movement update that repeats the stance the body is already in changes nothing \
               and queues no motion, while one naming a different stance moves the body into it; \
               repeating the new stance is then ignored in its turn.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O371-STYLE-REPEATED"),
        station: "dereth-animation::dat::motion::movement_style_guard::a_repeated_style_word_issues_no_motion_and_a_new_one_does",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.style.the-players-stance-follows-the-servers-style-word",
        says: "The player's stance follows the combat style the shard sends in his movement \
               messages, across every recorded change into and out of combat, including the \
               hand-to-hand, bow and magic stances.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O371-STYLE"),
        station: "dereth-client::gpu::movement::local_motion_style::the_players_stance_follows_the_servers_current_style",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "movement.teleport.an-accepted-teleport-cancels-the-approach-sticky-and-autorun",
        says: "An accepted teleport of the player ends whatever approach was under way, unsticks \
               the body from its target and turns off the run lock, so nothing from before the \
               teleport carries on at the destination.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-PLAYER-TELEPORT-TELEPORT"),
        station: "dereth-client::dat::movement::player_teleport_completion::actual_app_teleport_step_clears_autorun_and_the_old_move_to",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.tick.physics-runs-at-thirty-hertz-whatever-the-frame-rate",
        says: "Physics runs at most thirty times a second whatever the frame rate: at 250, 120 or \
               60 frames a second, a character walking for two seconds takes about the same number \
               of physics steps and covers the same distance.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-EMBODIED-TICK"),
        station: "dereth-client::gpu::movement::embodied_character::the_thirty_hertz_gate_bounds_physics_and_the_frame_rate_does_not",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "movement.vector-update.a-stale-update-is-refused",
        says: "A velocity update for an object is applied only when its stamp is strictly newer \
               than the last one applied; an equal or older stamp is refused outright and changes \
               neither the stamp nor the object's motion.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F20-VECTOR-UPDATE"),
        station: "dereth-client::gpu::movement::vector_and_position_receivers::a_vector_update_that_is_not_strictly_newer_is_refused_outright",
        tier: Tier::Gpu,
    },
];
