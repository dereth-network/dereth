//! The desktop client: the default key map's controls, the chase camera under the player's
//! control, the settings, and the shell carrying a session through the world.
//!
//! A row claims what the September 2013 client does with its keys, mouse and camera, or what the
//! shell does as the seam the game's interface plugs into.

use super::{Behaviour, Evidence, Tier, RETAIL, TOOLING};

/// This subject's rows, in id order.
pub static ROWS: &[Behaviour] = &[
    behaviour! {
        id: "desktop.bodies.a-behavior-with-an-emote-plays-when-the-objects-scripts-take-it",
        says: "A behavior the server asks of a body plays with the context 1; one carrying an emote \
               is first handed to the object's own scripts, and plays only when they take it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-EMOTE-BEHAVIOR"),
        station: "auberean-scene::corpus::combat::a_behavior_with_an_emote_plays_when_the_objects_scripts_take_it",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "desktop.bodies.a-held-object-takes-the-profile-its-qualities-give",
        says: "An object another holds takes the profile its own qualities give when its body is \
               made, as every object's body does: whether it slides along edges and ignores \
               collisions, and its own scale (drawn at its look's times it, and its holder's).",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-PROFILE-HELD"),
        station: "auberean-scene::corpus::combat::a_held_object_takes_the_profile_its_qualities_give",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "desktop.bodies.an-object-is-in-the-world-while-its-body-stands-there",
        says: "With the scene placing the bodies, an object's scripts are told it entered the \
               world when its body did (an object with no body, when its place's cell is \
               loaded) and that it left when its body left: a cell no longer loaded, a \
               placement that failed, held, contained or gone.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-LIVES"),
        station: "auberean-scene::corpus::combat::an_object_is_in_the_world_while_its_body_stands_there",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "desktop.bodies.the-scripts-scale-hide-and-slide-the-bodies",
        says: "What the objects' scripts set on their bodies reaches them when it changes: the \
               body's own scale (drawn and met at its look's scale times it, its entry in the \
               world's table reaching as far), whether it is drawn, whether it slides along \
               edges, whether it is undetectable and its jump scale; and the scripts read back \
               each body's element and the motion it was last asked for.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-BODY-STATES"),
        station: "auberean-scene::corpus::combat::the_scripts_scale_hide_and_slide_the_bodies",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "desktop.camera.a-change-of-view-glides-over-the-transition-time",
        says: "A change of view glides: each frame the camera moves from where it stood toward its \
               new place by the share of the transition time (a second) gone, and stands there \
               once it is up.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-CAMERA-GLIDE"),
        station: "auberean-scene::cpu::camera::a_change_of_view_glides_over_the_transition_time",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.camera.a-shake-moves-the-camera-by-the-clients-noise",
        says: "A shake under way moves the camera by the client's own noise (fractional Brownian \
               motion of three octaves over its gradient noise, sixty times the shake's seconds \
               along from each axis's start), its intensity times its weight over its source's \
               distance from the player (at least a meter), up and down three times as much.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CAMERA-SHAKE"),
        station: "auberean-scene::cpu::camera::a_shake_moves_the_camera_by_its_noise_over_its_distance",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.camera.align-to-slope-follows-the-climb",
        says: "Align to slope (on by default) lowers the camera while the player climbs, by its \
               rise each frame smoothed and eased, and levels it again on flat ground.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-CAMERA-SLOPE"),
        station: "auberean-scene::cpu::camera::align_to_slope_follows_the_climb",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.camera.auto-distance-pulls-a-close-camera-back-while-running",
        says: "With auto distance a camera 1.5 m or nearer pulls back toward 4 m after 2.5 seconds \
               of running and comes back after a second standing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-CAMERA-AUTO"),
        station: "auberean-scene::cpu::camera::auto_distance_pulls_a_close_camera_back_while_running",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.camera.close-in-it-slides-to-the-shoulder",
        says: "Nearer than 2.9 m the camera slides toward the right shoulder by 0.7 of the \
               player's radius times what is left of 2.9 m, easing there over a second, and never \
               stands nearer than half a meter across the ground.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-CAMERA-SHOULDER"),
        station: "auberean-scene::cpu::camera::close_in_it_slides_to_the_shoulder",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.camera.effects-played-on-objects-play-their-camera-part",
        says: "An effect played on an object plays the camera effect record its effect table \
               names: the player's own shake moves the camera while it lasts and lets it stand \
               again; a flash for the player only starts from the player, not from another object.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-CAMERA-FX-ROUTE"),
        station: "auberean-shell::dat::session::effects_played_on_objects_play_their_camera_part",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.camera.effects-ramp-and-add-to-the-field-of-view",
        says: "The field of view is 65 degrees plus the camera effects' sum, never under 10; an \
               effect ramps up, holds and ramps down over its period, and a distance effect adds \
               to the camera's distance.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-CAMERA-EFFECTS"),
        station: "auberean-scene::cpu::camera::effects_ramp_and_add_to_the_field_of_view",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.camera.held-keys-rotate-tilt-and-zoom-at-the-clients-rates",
        says: "Held keypad keys turn the chase camera a radian a second (five with Shift), tilt it \
               half a radian a second (one with Shift), and zoom it by 0.75 a second in and 1.5 \
               out (0.5625 and 2.25 with Shift); the pitch stays within 1.3744467 radians of \
               level.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-CAMERA-HELD"),
        station: "auberean-scene::cpu::camera::held_keys_rotate_tilt_and_zoom_at_the_clients_rates",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.camera.it-stands-behind-and-its-lines-start-at-the-shoulder",
        says: "At rest the camera stands 4.5 m behind the player along its 0.25 radian direction, \
               times the height scale, raised by the player's height; the lines keeping it off the \
               world start three quarters up the player at its right shoulder; first person puts \
               the eyes nine tenths up, 0.4 m ahead.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-CAMERA-PLACE"),
        station: "auberean-scene::cpu::camera::it_stands_behind_and_its_lines_start_at_the_shoulder",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.camera.mouse-look-turns-a-degree-a-count-smoothed",
        says: "In mouse look a mouse count turns the view a degree times the sensitivity, smoothed \
               over the newest ten counts weighted 1, 0.2, 0.04 and so on; moving the mouse toward \
               the player tilts the view down unless inverted; with steering on the character \
               turns.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-MOUSE-LOOK"),
        station: "auberean-scene::cpu::camera::mouse_look_turns_a_degree_a_count_smoothed",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.camera.solid-objects-stop-the-camera",
        says: "The line from the player's shoulder to where the camera wants to be stops at solid \
               objects, and the camera stands where it met one: beside the start hall's fir, \
               within 2 cm of where the retail client's camera stands from the same place.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-CAMERA-OBJECTS"),
        station: "auberean-shell::dat::session::solid_objects_stop_the_camera",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.camera.the-avatar-shows-in-first-person-during-combat-and-the-camera-rides-its-head",
        says: "With the camera at the eyes the player's body shows during combat only (the avatar \
               option's default): not at peace, and in a combat mode without its head. While a \
               behavior that moves the first-person camera plays, the camera rides the head bone a \
               fifth of a meter ahead and up, and goes back to the eyes when it ends; at peace it \
               stays at the eyes.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-CAMERA-AVATAR"),
        station: "auberean-shell::dat::session::the_avatar_shows_in_first_person_during_combat_and_the_camera_rides_its_head",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.camera.the-keypad-views-toggle-as-the-client-does",
        says: "The keypad's views: shoulder 1.3 m behind, first person at the head, top-down and \
               the map view pitched 1.37 radians (the map view 160 m out), reverse in front, swing \
               out about 67 degrees to a side; the same key again goes back behind.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-CAMERA-VIEWS"),
        station: "auberean-scene::cpu::camera::the_keypad_views_toggle_as_the_client_does",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.camera.the-scripts-cameras-hang-from-look-at-and-come-back",
        says: "The cameras effects and animations ask for: free (at an object, a third of the \
               distance out, facing its way) and back to the player over two seconds once its \
               period is over; looking at an object (the distance out from it times its height \
               scale); hung from the player looking at an object (back a third of the distance \
               along the line from it); hung from another object looking at the player. Each place \
               is the one the 0930 client's own code gives.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-CAMERA-SCRIPTED"),
        station: "auberean-scene::cpu::camera::the_scripts_cameras_hang_from_look_at_and_come_back",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.camera.the-shakes-noise-is-the-clients",
        says: "The shake's noise is the client's own: fractional Brownian motion (half the weight \
               an octave, twice the frequency, three octaves) over the client's gradient noise \
               gives what the 0930 client's own code gives.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CAMERA-SHAKE-NOISE"),
        station: "auberean-scene::cpu::camera::the_shakes_noise_is_the_clients",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.camera.the-wheel-steps-and-the-reset-returns-behind",
        says: "Each notch of the wheel moves the camera 1.5 m, between 0.11 m and 160 m; the reset \
               puts it behind at the default distance and tilt.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-CAMERA-WHEEL"),
        station: "auberean-scene::cpu::camera::the_wheel_steps_and_the_reset_returns_behind",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.combat.a-corpse-is-made-lying-dead",
        says: "An object is made in the mode its creation carries, at once, and with its sliders \
               at their values: a corpse lies dead from the first frame it is drawn.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-CORPSE-MODE"),
        station: "auberean-scene::corpus::combat::a_corpse_is_made_lying_dead",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "desktop.combat.a-dead-creature-stays-down",
        says: "A behavior that carries a mode puts the body in that mode before it plays: a \
               creature's death behavior leaves it in the dead mode, lying down, whatever the \
               server sends after.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-DEATH-MODE"),
        station: "auberean-scene::corpus::combat::a_dead_creature_stays_down",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "desktop.combat.a-missile-that-strikes-sticks-or-fades",
        says: "A missile let go of flies from where it was placed, never pulled back to the hand \
               that held it; one that strikes a moving object it can stick in sticks there, held \
               by the bone nearest it, until it fades.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-COMBAT-STICK"),
        station: "auberean-scene::corpus::combat::a_missile_that_strikes_sticks_or_fades",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "desktop.combat.a-turn-begun-stops-an-attack-made-while-moving-on-its-own",
        says: "Every movement action the player begins interrupts its body's own move to a \
               target; a turn also asks the player's scripts to stop an attack they started while \
               it moved so.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-MOVEMENT-START"),
        station: "auberean-scene::corpus::combat::a_turn_begun_stops_an_attack_made_while_moving_on_its_own",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "desktop.combat.recorded-missiles-are-placed-and-fly",
        says: "Every missile a story fires is put in the world and flies: the player's own arrows \
               held in its hand and let go of there, and missiles made as they are fired from a \
               look with no collision spheres.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-COMBAT-MISSILES"),
        station: "auberean-scene::corpus::combat::recorded_missiles_are_placed_and_fly_the_players_own_arrows_too",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "desktop.combat.recorded-stories-play-on-the-bodies",
        says: "A combat story the server tells on an object plays on its body: the attacker's \
               behavior on its figure (in the mode the server set), each hook when the figure's \
               animation reaches it, the hit's effects and the damage shown by the target's \
               scripts with its floating text, the player's attack begun and ended, and the \
               missiles made, held in the attacker's hand, fired and put in the world in flight.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-COMBAT-STORIES"),
        station: "auberean-scene::corpus::combat::recorded_stories_play_on_the_attackers_bodies_with_hits_texts_and_arrows",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "desktop.combat.the-animations-own-effects-play-on-the-bodies",
        says: "The effects a body's animations start at moments of their cycles are played on the \
               body once, and stopped when the animation that started them ends; the effects a \
               behavior or a change of mode plays once are played too.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-ANIMATION-FX"),
        station: "auberean-scene::corpus::combat::the_animations_own_effects_play_on_the_bodies",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "desktop.combat.the-creature-that-strikes-the-player-is-its-last-attacker",
        says: "A story whose hit strikes the player with a skill harmful by the attacker's scripts \
               makes the attacker the player's last attacker, which the last attacker's key \
               selects.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-COMBAT-LAST-ATTACKER"),
        station: "auberean-scene::corpus::combat::the_creature_that_strikes_the_player_is_its_last_attacker",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "desktop.combat.the-players-arrows-leave-the-drawn-bow-and-strike-their-target",
        says: "The player's arrows are fired when the bow attack's animation reaches its release, \
               so each is let go of where the drawn bow holds it, in front of the player at \
               shoulder height, flies clear of the shooter and strikes its target.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-COMBAT-ARROWS"),
        station: "auberean-scene::corpus::combat::the_players_arrows_leave_the_drawn_bow_and_strike_their_target",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "desktop.combat.the-server-turns-the-player-to-what-it-attacks",
        says: "The server turns the player to what it attacks: before an attack it orders the \
               player's own body to move to the target asking only to turn, and stops the order \
               after; the client's player body takes each order the server sends it (other bodies \
               take none), first stopping the behaviors a move cancels, and turns to face the \
               target as it stands (2017: each such turn ends within a degree of the target).",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-COMBAT-TURN-TO"),
        station: "auberean-scene::corpus::combat::the_server_turns_the_player_to_face_what_it_shoots",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "desktop.controls.a-movement-key-begins-its-action-once",
        says: "A movement key pressed begins its action of the key map once: forward, backward, \
               the strafes, the turns (strafes in mouse look, where the left button runs), the \
               jump and the run lock.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-MOVEMENT-KEYS"),
        station: "auberean-shell::dat::controls::a_movement_key_begins_its_action_once",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.controls.autorun-latches-and-a-jump-is-one-press",
        says: "Q or Num Lock latches running; forward or backward turns it off, strafing and \
               turning do not. Space or J jumps once a press.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-KEYS-AUTORUN"),
        station: "auberean-shell::dat::controls::autorun_latches_and_a_jump_is_one_press",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.controls.select-use-and-letting-go-on-focus-loss",
        says: "The selection keys select, Escape clears the selection and F uses what is selected; \
               a window that loses the keyboard lets every key go.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-KEYS-SELECT"),
        station: "auberean-shell::dat::controls::select_use_and_letting_go_on_focus_loss",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.controls.the-default-keys-move-the-body",
        says: "W and Up run forward, with Shift walk at 0.339 of running; S, X and Down back up \
               and Shift does not slow them; A and D turn; Z and C strafe.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-KEYS-MOVE"),
        station: "auberean-shell::dat::controls::the_default_keys_move_the_body",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.controls.the-keypad-and-mouse-drive-the-camera",
        says: "The keypad drives the camera; the wheel steps it; the middle button turns mouse \
               look on and off, and while it is on A and D strafe and the left button runs \
               forward.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-KEYS-CAMERA"),
        station: "auberean-shell::dat::controls::the_keypad_and_mouse_drive_the_camera",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.interface.a-click-in-the-world-selects-what-lies-under-it",
        says: "A click on a body standing in the world lands on the field over the world, and the \
               body under the pointer, found by the triangles drawn, is selected through the \
               player's scripts at the next tick; a click on empty ground selects nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-SELECTION-WORLD"),
        station: "auberean-interface::dat::screens::a_click_in_the_world_selects_what_lies_under_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.interface.every-panel-is-drawn-in-front-of-the-world",
        says: "In the world every panel is drawn in front of the world, which fills the window \
               behind every element: a panel's own viewport puts nothing behind the world. The \
               panels deeper than the viewport (the chat window, the status bar) lie over the \
               world and under the viewport's scene, the paper doll's figure; the shallower ones \
               (the toolbar, the inventory's own window) over the figure.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-IN-FRONT-OF-THE-WORLD"),
        station: "auberean-interface::dat::screens::every_panel_is_drawn_in_front_of_the_world",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.interface.the-left-button-runs-forward-in-mouse-look-over-the-world",
        says: "In mouse look the left button runs the body forward while held, though the field \
               over the world takes the press: the world's controls see the button too, and the \
               body stops when it is let go.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-SELECTION-MOUSE-LOOK"),
        station: "auberean-interface::dat::screens::the_left_button_runs_forward_in_mouse_look_over_the_world",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.interface.the-play-screen-follows-the-player-and-takes-war-and-chat",
        says: "In the world the status bar shows the player's health and vigor from its qualities, \
               the war button asks the player's scripts to attack while the buttons wait on their \
               word, and Enter starts a chat line that takes the keys typed (the body never moves) \
               and ends it, the entry emptied and the line handed to the player's command line.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-PLAY-PANELS"),
        station: "auberean-interface::dat::screens::the_play_screen_follows_the_player_and_takes_war_and_chat",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.interface.the-play-screen-runs-on-the-frameworks",
        says: "In the world the play screen runs on the frameworks: E shows the examination \
               window; the skills button shows the skills window and its tooltip turns to Hide; \
               F12 hides the toolbar and shows it again; the middle button turns mouse look on and \
               off, the camera following and the crosshair showing; the up arrow still runs the \
               body forward.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-PLAY-FRAMEWORKS"),
        station: "auberean-interface::dat::screens::the_play_screen_runs_on_the_frameworks",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.interface.the-screens-before-the-world-fill-the-window-at-1024-by-768",
        says: "The screens before the world are laid out on a 1024 by 768 screen whatever the \
               window's size (the client sets its display to 1024 by 768 for them), and their \
               picture fills the window: a click lands on the button drawn under it. The pointer \
               shows the data's default cursor. Credits shows the credits, which an interface key \
               ends; Options the key map, the client's maps by action in the game's order, whose \
               Back returns to the list.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-PREGAME-1024"),
        station: "auberean-interface::dat::screens::the_screens_before_the_world_fill_the_window_at_1024_by_768",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.interface.the-screens-take-a-player-from-the-list-into-the-world",
        says: "The game opens on its pictures; a press goes on to connecting, which logs in and, \
               the characters listed, brings up the character list. There Create opens the avatar \
               hall; the figure a ray through the pointer meets in the hall's scene is the avatar \
               chosen, which opens the appearance, whose slider buttons move a choice a stop; Next \
               the name; Enter there asks the \
               server for the character with the player's entity, the race's start area, class \
               and implement and all thirteen physique values, and once it is made enters the \
               world with it, where the play screen's panels come up.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-SCREENS-FLOW"),
        station: "auberean-interface::dat::screens::the_screens_take_a_player_from_the_list_into_the_world",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.interface.what-a-body-holds-is-picked-and-selected-as-itself",
        says: "What a body holds (a wielded bow) is drawn with its holder and a click on it \
               selects it as itself, its name and marks shown where it is drawn; a double press \
               on an item the player holds selects it without using it; the selection keys do \
               not find it, as it has no place of its own in the world.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-SELECTION-HELD"),
        station: "auberean-interface::dat::screens::what_a_body_holds_is_picked_and_selected_as_itself",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.keys.a-key-let-go-ends-what-its-press-began",
        says: "A key let go ends the action its press began, whatever modifier was let go in \
               between: a key pressed with Shift ends its Shift action even after Shift is let \
               go.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-KEYS-PRESS-PAIRS"),
        station: "auberean-shell::cpu::keys::a_key_let_go_ends_the_action_its_press_began",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.keys.a-saved-key-map-file-is-bound-in-play",
        says: "A player's saved key map files are the keys in play: with the WoW preset saved as \
               the plain and Ctrl maps, Q strafes and C opens the inventory, and with no Shift \
               file the Defaults' Shift map stays bound.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-KEYS-FILES-IN-PLAY"),
        station: "auberean-shell::dat::keys::a_saved_key_map_file_is_bound_in_play",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.keys.actions-toggle-as-the-action-map-says",
        says: "Each action toggles as the action map says: a held one is in force while its key \
               is down, a latched one from one press to the next (forward, backward and stop end \
               it), a single press for its frame only.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-KEYS-TOGGLES"),
        station: "auberean-shell::cpu::keys::held_latched_and_single_press_actions_toggle_as_the_action_map_says",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.keys.apply-writes-the-files-and-binds-them",
        says: "Applying keys on the key map screen writes the three key map files the \
               preferences name and binds them in place of the player's maps; when a file cannot \
               be written nothing changes.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-KEYS-APPLY"),
        station: "auberean-shell::cpu::keys::applying_writes_the_three_files_and_binds_them",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.keys.maps-come-from-their-files-else-the-defaults",
        says: "At startup the player's key maps come from their files, the plain one read with \
               the update binding R to reply and written again when it gains it; a file missing \
               or not reading leaves the data's Defaults map in its place.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-KEYS-START"),
        station: "auberean-shell::cpu::keys::the_key_maps_come_from_their_files_else_the_defaults",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.keys.modifiers-choose-an-actions-values",
        says: "The modifier actions in force choose an action's values: the entry for exactly \
               them, else without the eighth bit, else the plain one; Shift with forward walks.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-KEYS-MODIFIERS"),
        station: "auberean-shell::cpu::keys::the_modifiers_held_choose_an_actions_values",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.keys.the-front-map-binding-an-input-names-its-action",
        says: "The key maps are a stack: an input names the action of the first map from the \
               front that binds it and whose modifier keys are all held, so a Shift map's binding \
               comes first while Shift is held and the plain map answers the rest.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-KEYS-STACK"),
        station: "auberean-shell::cpu::keys::the_front_map_that_binds_an_input_with_its_modifiers_held_names_its_action",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.looks.an-effects-appearance-dresses-the-body-it-plays-on",
        says: "The appearance an object's effects lay over it or take off it changes the object's \
               own descriptions (its own with the effects' appearance laid on, kept when the \
               server sends it a new description), and its figure and body are brought up to date \
               with the total they give, keeping what they play, as for a new description the \
               server sends (the player's own too): the creatures the 2017 servers wound bleed \
               until the wound stops.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-APPEARANCE-FX"),
        station: "auberean-scene::corpus::combat::a_wounded_creature_bleeds_until_its_wound_stops",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "desktop.looks.every-recorded-look-draws-through-the-objects-own-copy",
        says: "Every object the 2017 sessions create puts together through its own copy of the \
               description sent; 2,063 of the 160,633 gather their atoms in another order than the \
               description itself would, and none of them draws another template, modifier order, \
               mesh or opacity.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-LOOK-OWN-COPY-CAPTURES"),
        station: "auberean-scene::corpus::looks::every_recorded_look_draws_through_the_objects_own_copy",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "desktop.others.a-body-out-of-the-world-is-let-go-of-after-25-seconds",
        says: "A body that leaves the world (its cell no longer held) is to be destroyed 25 \
               seconds later, with what it holds; entering the world again first calls that off; \
               left that long, the client lets it go.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-OTHERS-LEAVE-WORLD"),
        station: "auberean-shell::dat::session::a_body_out_of_the_world_is_let_go_of_after_25_seconds",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.others.a-body-takes-the-profile-its-objects-qualities-give",
        says: "A body made for an object takes the profile the object's qualities give, asked as \
               the object answers them: its description's, then its own, then those of the \
               qualities record they derive from (the one its qualities name, else its entity \
               template's). The player template's record makes its body slide along edges.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-PROFILE-QUALITIES"),
        station: "auberean-scene::cpu::others::a_body_takes_the_profile_its_objects_qualities_give",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.others.a-creation-again-puts-the-body-and-a-new-incarnation-replaces-it",
        says: "A creation sent again for the incarnation of an object the client has puts its \
               body where the creation says, with the creation's stamps; a creation of a newer \
               incarnation destroys the body the client had and makes it anew where it was \
               created, so the new incarnation's first positions are taken.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-OBJECTS-INCARNATIONS"),
        station: "auberean-shell::dat::session::a_creation_again_puts_the_body_and_a_new_incarnation_replaces_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.others.a-still-body-is-solid-at-its-whole-scale",
        says: "An object that does not move is solid in the world at its whole scale, its look's \
               times its own (the scale its qualities give), as it is drawn.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-PROFILE-SCALE"),
        station: "auberean-scene::cpu::others::a_still_body_is_solid_at_its_whole_scale",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.others.another-body-follows-its-reports-at-an-even-pace",
        says: "Another body sent a position every 0.2 to 0.3 s while it runs is steered by its own \
               motion along the path of its positions, a physics tick at a time, toward a tick of \
               its full speed beyond the nearest point of the path, the path running on past the \
               last position as its motion carries it; drawn between its last two ticks it moves \
               at an even pace, not lurching toward each position.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-OTHERS-PATH"),
        station: "auberean-scene::cpu::others::another_body_follows_its_reports_at_an_even_pace",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.others.another-body-stops-on-its-last-report",
        says: "A body whose last position stands still comes to rest on it: within 0.71 m of it \
               the body turns to the heading sent, within 5 cm it is there.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-OTHERS-STOP"),
        station: "auberean-scene::cpu::others::another_body_stops_on_its_last_report",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.others.bodies-anywhere-in-an-interior-are-drawn",
        says: "A body in the world is drawn wherever it stands in an interior: the interior's \
               cells belong to its landblock however far their places reach past the landblock's \
               edges (the human training hall's straw drudges stand 338 m north of its corner).",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-HALL-BODIES"),
        station: "auberean-shell::dat::session::a_body_far_into_the_hall_is_drawn",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.others.far-reports-and-stuck-bodies-are-put-in-place",
        says: "A position more than 15 m from where a body is puts it there at once; a body that \
               passes no position for 2 s, or is more than 15 m from its last one, is put on that \
               one; a position whose stamp is no newer than the last taken is ignored.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-OTHERS-SNAP"),
        station: "auberean-scene::cpu::others::far_reports_and_stuck_bodies_are_put_in_place",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.others.objects-in-cells-not-loaded-wait-out-of-the-world",
        says: "Only the land cells of the landblocks the land holds (the player's and the eight \
               beside it) and the interior cells of the player's block and those beside it are \
               loaded; an object sent into a cell not loaded leaves the world, kept in that cell's \
               list of lost objects with where it was sent, and enters the world there when the \
               cell is loaded again.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-OTHERS-LOST"),
        station: "auberean-scene::cpu::others::objects_in_cells_not_loaded_wait_out_of_the_world",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.scene.the-bodies-effects-place-lights-among-the-frames",
        says: "The lights the bodies' own effects place join the frame's lights with those of the                objects placed with the world: a light played on another human stands with it at its                effect's offset for as long as the effect says (as long as the human, or a few                seconds); with light source effects off, no effect places one.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCENE-BODY-EFFECT-LIGHTS"),
        station: "auberean-shell::dat::session::the_bodies_effects_place_lights_among_the_frames",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.settings.a-fresh-install-starts-at-the-quality-the-configuration-offers",
        says: "With no preferences kept the client starts at overall quality 4, as the \
               configuration program the original runs first offers on any machine that draws \
               with shaders (never the highest) and the player keeps; a kept overall quality of \
               1 to 5 sets its own preferences over those read beside it, 0 keeps them, and the \
               quality the preferences match is written back.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-FIRST-RUN-GRAPHICS"),
        station: "auberean-shell::cpu::settings::a_fresh_install_starts_at_the_quality_the_configuration_offers",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.settings.graphics-preferences-read-by-the-clients-names-and-kept",
        says: "The graphics preferences are read from their own section by the client's names (a \
               choice by its level's name or number, a yes-or-no, a number within its range) and \
               written back the same way, keeping the section's other lines.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-SETTINGS-GRAPHICS"),
        station: "auberean-shell::cpu::settings::graphics_preferences_read_by_the_clients_names_and_kept",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.settings.options-read-by-the-clients-names-and-kept",
        says: "The camera, mouse and avatar options are read by the client's own option names, \
               each within its range (the avatar option by name or number, during combat by \
               default), and written back keeping every other line.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-SETTINGS"),
        station: "auberean-shell::cpu::settings::options_read_by_the_clients_names_and_kept",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.settings.the-display-and-values-read-as-the-client-writes-them",
        says: "The display's size is kept as one number, its width above its height, with full \
               screen beside it; a yes-or-no reads only as 1 or 0, a number only when the whole \
               text is one, and numbers are written with six places.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-DISPLAY-PREFS"),
        station: "auberean-shell::cpu::settings::the_display_and_values_read_as_the_client_writes_them",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.shell.a-placed-player-stands-where-there-is-room",
        says: "A player the server puts where it does not fit stands where there is room nearest \
               that place, as the retail client stands it (in the start hall, against a fir: 0.685 \
               m up, within 6 cm across the ground).",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-PLACED-ROOM"),
        station: "auberean-shell::dat::session::a_placed_player_stands_where_there_is_room",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.shell.a-standing-jump-leads-in-then-leaves-the-ground",
        says: "The jump key with the player standing: its figure plays the jump's lead-in while \
               the body stays on the ground, until the lead-in's animation sets the jump off; \
               the body then leaves the ground at the jump's speed with its figure in the air, \
               the server is told of the jump then, and the figure is back on the earth once the \
               body lands.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-JUMP-LEAD-IN"),
        station: "auberean-shell::dat::session::a_standing_jump_leads_in_then_leaves_the_ground",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.shell.entering-the-world-goes-through-portal-space",
        says: "From the moment it asks to enter the world the client shows portal space; once the \
               server has the player in the world and the client its body, it goes out over two \
               seconds.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PORTAL-ENTRY"),
        station: "auberean-shell::dat::session::entering_the_world_goes_through_portal_space",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.shell.the-body-runs-with-the-default-keys-and-is-reported",
        says: "In the world, W held runs the player's body over the hall's floor at the running \
               speed, and its place is reported to the server by the report rule; closing the window \
               in the world ends the program at once with a goodbye on every connection and no \
               log-off first.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-RUN"),
        station: "auberean-shell::dat::session::the_body_runs_with_the_default_keys_and_is_reported",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.shell.the-interface-takes-input-first",
        says: "The game's interface sees every device event before the world's controls; an event \
               it takes never reaches them.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-SEAM"),
        station: "auberean-shell::dat::session::the_interface_takes_input_first",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.shell.the-land-kept-is-the-near-window-about-the-player",
        says: "The land kept is the near window about the player's cell: its block and the eight \
               beside it. Moving a block lets go of the column left and makes the one reached; \
               coming back finds the blocks let go of as they were; inside cells that do not see \
               the outdoors no land is kept.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-NEAR-WINDOW"),
        station: "auberean-shell::dat::session::the_land_kept_is_the_near_window_about_the_player",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.shell.the-world-is-drawn-by-the-kept-preferences-from-the-first-frame",
        says: "The graphics preferences the player kept draw the world from its first frame, as \
               the client reads them when it starts: the world is made with the kept dynamic \
               shadows and light source effects; a fresh install's draw volumetric shadows and \
               place the effects' lights.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-PREFERENCES-AT-START"),
        station: "auberean-shell::dat::session::the_world_is_drawn_by_the_kept_preferences_from_the_first_frame",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.shell.what-is-placed-over-the-world-takes-the-frames-own-camera",
        says: "Each frame the interface takes input before the world moves on, and once the world \
               moved on and its frame was built it places what stands over the world (names, the \
               selection's marks) from that frame's camera, as the game places them after the \
               world's update and before drawing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-FRAME-ORDER"),
        station: "auberean-shell::dat::session::what_is_placed_over_the_world_takes_the_frames_own_camera",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.world.the-landscape-preferences-set-the-far-landscape-and-the-frills",
        says: "The landscape's draw distance sets how far the far landscape reaches (none, 2, 4, \
               6 or 8 blocks); the frill distance where frills are drawn (none; the cells next to \
               the viewer's, fading sooner at the lowest setting; two cells about it at the \
               highest).",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-LANDSCAPE-PREFERENCES"),
        station: "auberean-scene::cpu::landscape::the_landscape_preferences_set_the_far_landscape_and_the_frills",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.world.the-sky-follows-the-camera-every-frame-and-the-ground-is-lit-on-the-lights-tick",
        says: "Outdoors the sky stands at the camera every frame and moves on with the moment, as \
               do the light and fog; the ground keeps the lighting it was lit with until the \
               light's tick, the sky's light interval (30 s of game time) after the last.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-SKY-EVERY-FRAME"),
        station: "auberean-shell::dat::session::the_sky_follows_the_camera_every_frame_and_the_ground_is_lit_on_the_lights_tick",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.world.the-worlds-frame-carries-the-clients-running-time",
        says: "The world's frame carries the client's running time every frame: the clouds' \
               shadows slide, the water's ripples move and the sun's flare fades by it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-RUNNING-TIME"),
        station: "auberean-shell::dat::session::the_worlds_frame_carries_the_clients_running_time",
        tier: Tier::Dat,
    },
];
