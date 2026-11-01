//! The desktop client: the default key map's controls, the chase camera under the player's
//! control, the settings, and the shell carrying a session through the world.
//!
//! A row claims what the September 2013 client does with its keys, mouse and camera, or what the
//! shell does as the seam the game's interface plugs into.

use super::{Behaviour, Evidence, Tier, RETAIL};

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
];
