//! The desktop client: the default key map's controls, the chase camera under the player's
//! control, the settings, and the shell carrying a session through the world.
//!
//! A row claims what the September 2013 client does with its keys, mouse and camera, or what the
//! shell does as the seam the game's interface plugs into.

use super::{Behaviour, Evidence, Tier, RETAIL, THIS_CLIENT, TOOLING};

/// This subject's rows, in id order.
pub static ROWS: &[Behaviour] = &[
    behaviour! {
        id: "desktop.bodies.a-behavior-that-destroys-its-object-takes-a-server-object-away",
        says: "A behavior that destroys its object as it ends hides a server's object at once, and the object goes three seconds on.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-FX-REMOVE-SERVER-OBJECT"),
        station: "auberean-shell::dat::session::a_behavior_that_destroys_its_object_takes_a_server_object_away",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.bodies.a-behavior-that-fades-children-fades-what-the-body-holds",
        says: "A behavior that fades its object's children fades what the body holds as it starts, and brings it back as it ends.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-FX-CHILDREN-FADE"),
        station: "auberean-shell::dat::session::a_behavior_that_fades_children_fades_what_the_body_holds",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.bodies.a-behavior-that-locks-actions-holds-the-players-keys",
        says: "While a behavior that locks the player's actions plays, the body stops at once and what \
               the keys ask of it waits; a behavior a key names plays and stops on the player's figure.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-INPUT-LOCKED-KEYS"),
        station: "auberean-scene::corpus::combat::a_behavior_that_locks_actions_holds_the_players_keys",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "desktop.bodies.a-behavior-the-server-started-it-can-stop-or-let-go",
        says: "A hold the server started loops until the server lets it go, stops those a move                cancels, ends its layer, or, while it is within the cycles it was asked for, stops                all behaviors; each of them ends it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-SERVER-STOPS"),
        station: "auberean-shell::dat::session::a_behavior_the_server_started_it_can_stop_or_let_go",
        tier: Tier::Dat,
    },
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
        id: "desktop.bodies.a-behaviors-target-and-clone-go-to-its-effects",
        says: "A behavior that names a target and a look to clone gives them to the body, and the effects its animation and script set off aim there: they hide the item and take a clone of it up. The clone has no place of its own, so the entry that lets go leaves it held; as the effect ends the item shows again and the clone fades out in the hand and goes three seconds later.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-FX-BEHAVIOR-AIM"),
        station: "auberean-shell::dat::session::a_behaviors_target_and_clone_go_to_its_effects",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.bodies.a-body-does-the-rest-of-what-the-server-asks",
        says: "A body keeps a global and a terrain modifier, clears the terrain modifiers, is moved \n               by a push toward an object, and flies when a missile is launched.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-SERVER-EVENTS-REST"),
        station: "auberean-shell::dat::session::a_body_does_the_rest_of_what_the_server_asks",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.bodies.a-body-faded-out-is-not-seen",
        says: "Each body drawn is seen as faded out exactly when its figure's opacity (its fade \
               slider, held to 0..1) is under 0.0002, as the arrow on the string is when let go of; at \
               that opacity or more it is seen.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-BODIES-FADED"),
        station: "auberean-scene::corpus::objects::a_body_faded_out_by_its_animation_is_seen_as_faded",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "desktop.bodies.a-body-goes-round-a-stone-in-the-way",
        says: "A body run into a round stone a little to one side holds a moment, slides round it and goes on where retail's does, never inside it, though the stone reaches a cell beyond its own and was made before its cell was read.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-STONE"),
        station: "auberean-shell::dat::session::a_body_goes_round_a_stone_in_the_way",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.bodies.a-body-held-against-a-wall-is-animated-standing",
        says: "A body's figure is animated at how far the body moved over its last physics tick \
               (ten times the move, in the body's frame before it), not at the velocity it asks \
               for: a body held against a wall or at a ledge while it asks to run stands.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-FEET-WALL"),
        station: "auberean-scene::cpu::feet::a_body_held_against_a_wall_is_animated_standing",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.bodies.a-body-is-drawn-in-the-color-its-sliders-hold",
        says: "A body is drawn in the color and at the opacity its sliders hold: its red, green,                blue and alpha sliders multiply its draws, its fade slider their alpha too.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-ANIM-COLOR"),
        station: "auberean-shell::dat::session::a_body_is_drawn_in_the_color_its_sliders_hold",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.bodies.a-body-takes-a-script-a-motion-description-and-private-effects",
        says: "A body the server asks plays an effect script and takes a new motion description; a                private effect plays on the player whoever else is near.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-SERVER-ASKS"),
        station: "auberean-shell::dat::session::a_body_takes_a_script_a_motion_description_and_private_effects",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.bodies.a-body-turns-its-head-to-what-it-looks-at",
        says: "A body asked to look at an object turns its head to it, blended in over half a \
               second and held: its turn the object's angle from straight ahead, its pitch the \
               weighted rise to three quarters up a body that moves on its own (half way up \
               anything else) over the level distance.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-GAZE-LOOK"),
        station: "auberean-scene::cpu::gaze::a_body_looking_at_an_object_turns_its_head_to_it_over_half_a_second_and_holds",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.bodies.a-direction-from-the-server-turns-a-head",
        says: "The direction the server sends for another body turns its head there over half a \
               second, once; a body looking at nothing asks nothing else of its look sliders.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-GAZE-DIR"),
        station: "auberean-scene::cpu::gaze::a_direction_from_the_server_turns_another_bodys_head_over_half_a_second_once",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.bodies.a-frozen-body-and-figure-stand-still",
        says: "What the scripts freeze stays: a body with its movement frozen does not move, and \
               while its animation is frozen its figure does not advance.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-FROZEN"),
        station: "auberean-scene::corpus::physics_object::a_frozen_body_and_figure_stand_still",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "desktop.bodies.a-frozen-body-drops-behaviors-and-modes",
        says: "A body whose animation is frozen, as it is through a portal, plays no behavior and takes no mode, and neither waits for the thaw.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-FROZEN-BODY"),
        station: "auberean-shell::dat::session::a_frozen_body_drops_behaviors_and_modes",
        tier: Tier::Dat,
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
        id: "desktop.bodies.a-released-look-blends-back-out",
        says: "A look released blends back out over half a second from where it held.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-GAZE-RELEASE"),
        station: "auberean-scene::cpu::gaze::released_the_look_blends_back_out_over_half_a_second",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.bodies.a-step-up-or-down-shows-in-the-figures-vertical-speed",
        says: "A body that steps up onto something or down off it within one tick has its figure \
               told, for the next tick, the vertical speed of that step (0.3 m in a tick is 3 m/s), \
               though the body's own velocity has none; with each animation step it also takes \
               half its acceleration times the step squared.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-FEET-STEP"),
        station: "auberean-scene::cpu::feet::a_step_up_or_down_shows_in_the_figures_vertical_speed",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.bodies.an-aimed-effect-acts-on-its-target",
        says: "An effect aimed at another object acts on it: an entry that hides its target hides \n               it until the effect is over.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-FX-AIMED"),
        station: "auberean-shell::dat::session::an_aimed_effect_acts_on_its_target",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.bodies.an-effect-asked-out-of-sight-plays-at-once",
        says: "An effect asked of the player's body in portal space plays at once, its systems \
               running when the body is next drawn: the mist of arrival shows out of the white.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-FX-ARRIVAL"),
        station: "auberean-shell::dat::session::an_effect_asked_out_of_sight_plays_at_once",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.bodies.an-object-an-effect-makes-goes-with-its-behavior",
        says: "An object an effect makes hangs from the object playing it at the entry's holding                location or bone. An entry naming a behavior plays it on the object; as it ends the                object is hidden at once and goes three seconds later, whenever the effect ends.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-FX-MADE-BEHAVIOR"),
        station: "auberean-shell::dat::session::an_object_an_effect_makes_goes_with_its_behavior",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.bodies.an-object-an-effect-makes-lasts-as-its-entry-says",
        says: "An object an effect makes with no behavior lasts as long as its entry says: 15 seconds                if it says nothing, until the effect is stopped if it says less than none.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-FX-MADE-LIFETIME"),
        station: "auberean-shell::dat::session::an_object_an_effect_makes_lasts_as_its_entry_says",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.bodies.an-object-fades-in-as-it-enters-the-world",
        says: "An object fades in over half a second as it enters the world.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-FX-ENTER-FADE"),
        station: "auberean-shell::dat::session::an_object_fades_in_as_it_enters_the_world",
        tier: Tier::Dat,
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
        id: "desktop.bodies.an-object-starts-the-behaviors-and-effects-its-description-gives",
        says: "An object the server creates plays at once the behaviors its description lists, and                the effects it lists, those that make objects making them.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-DESCRIPTION-PLAYS"),
        station: "auberean-shell::dat::session::an_object_starts_the_behaviors_and_effects_its_description_gives",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.bodies.an-object-takes-up-its-description",
        says: "An object the server creates goes to the slider values and velocity its description \
               gives.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-OBJECT-DESCRIPTION"),
        station: "auberean-shell::dat::session::an_object_takes_up_its_description",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.bodies.an-object-takes-up-its-stamps-and-flight-from-its-description",
        says: "An object takes up its four stamps from its description and, described as a missile, \n               its aim and that it flies.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-DESC-MISSILE"),
        station: "auberean-shell::dat::session::an_object_takes_up_its_stamps_and_flight_from_its_description",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.bodies.an-objects-startup-effects-make-their-objects",
        says: "An object whose look's startup effects name a look to make has those objects from the end of its making, each hanging from it at the entry's holding location or bone. They go when the effect or the object does.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-FX-STARTUP-OBJECTS"),
        station: "auberean-shell::dat::session::an_objects_startup_effects_make_their_objects",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.bodies.each-body-is-drawn-once-a-frame",
        says: "A body is drawn once in a frame, however many cells it reaches into.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-DRAWN-ONCE"),
        station: "auberean-scene::corpus::objects::each_body_is_drawn_once_a_frame",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "desktop.bodies.every-look-that-makes-a-figure-makes-a-body",
        says: "Every look of the data that puts a figure together makes a body to stand in the                world, those whose mode their motion description does not list among them: a                town's crafting workshop is drawn and named.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-BODY-MODE-UNLISTED"),
        station: "auberean-scene::corpus::looks::every_look_that_makes_a_figure_makes_a_body",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "desktop.bodies.recorded-heads-turn-to-the-servers-directions",
        says: "In a recorded 2017 session, every other body near the player has its look sliders at \
               the direction the server last sent for it two thirds of a second after it came.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-GAZE-RECORDED-DIR"),
        station: "auberean-scene::corpus::gaze::other_bodies_heads_turn_to_the_direction_the_server_sends",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "desktop.bodies.recorded-looks-turn-heads-toward-their-objects",
        says: "In recorded 2017 sessions, a body the server says looks at an object (the player's \
               selections echoed, other players' looks) turns its head toward it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-GAZE-RECORDED-LOOK"),
        station: "auberean-scene::corpus::gaze::a_body_turns_its_head_toward_what_the_server_says_it_looks_at",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "desktop.bodies.the-ground-gives-a-body-its-terrain-type",
        says: "The ground under a body gives it the terrain type of its physics material: its motion takes it, and its effects play their variants for it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-TERRAIN-TYPE"),
        station: "auberean-shell::dat::session::the_ground_gives_a_body_its_terrain_type",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.bodies.the-player-moves-while-its-body-does-falling-too",
        says: "The player moves while its body does, falling as much as running: dropped from high \
               up, it moves as it falls straight down with no speed along the ground, and stops \
               moving once it stands.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-BODIES-MOVING"),
        station: "auberean-shell::dat::session::the_player_moves_while_its_body_does_falling_too",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.bodies.the-player-stops-looking-at-what-is-long-behind-it",
        says: "The player stops looking at what it looks at once that has been behind it or more \
               than 10 m off for more than 2 seconds; another body keeps looking.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-GAZE-AWAY"),
        station: "auberean-scene::cpu::gaze::the_player_stops_looking_at_an_object_behind_it_for_more_than_two_seconds",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.bodies.the-players-body-does-what-the-server-asks",
        says: "A push from the server takes the player's body off the ground; a slider the network                moves goes to its value, one the character template leaves to the client does not.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYSICS-EVENTS"),
        station: "auberean-shell::dat::session::the_players_body_does_what_the_server_asks",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.bodies.the-players-head-follows-the-camera",
        says: "With nothing to look at, the player's head turns the way the camera looks (in mouse \
               look, the camera's direction turned into the body's frame), divided by the body's \
               size, over a tenth of a second (a fifth when far off); the look at camera option \
               turns it back toward a camera in front, and without the look around option it looks \
               straight ahead.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-GAZE-CAMERA"),
        station: "auberean-scene::cpu::gaze::the_players_head_follows_the_camera_with_its_options",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.bodies.the-players-head-follows-the-camera-from-its-arrival",
        says: "In a recorded 2017 session, the player's head follows the camera from its \
               arrival with nothing looked at: tipped down by the camera's tilt over its height \
               scale and turned straight ahead behind it, level and straight with the option to \
               look around off; the client holds where it points for the server, and takes the \
               option to look at the selection.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-GAZE-PLAYER-FROM-ARRIVAL"),
        station: "auberean-scene::corpus::gaze::the_players_head_follows_the_camera_from_its_arrival_as_the_options_say",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "desktop.bodies.the-scripts-read-each-bodys-physics-object",
        says: "What the scripts read of a body is what its physics object holds after the frame: \
               the speed and acceleration scales the server last sent for it (kept while its body \
               is away and the object stays), its jump scale, its radius and height (its setup's \
               by its look's height scale and its own), the mode its figure was last asked to go \
               to (for its mode and the mode it is going to alike; 0 with no figure) and the \
               motion the server last sent while it follows the server's positions.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-PHYSICS-READ"),
        station: "auberean-scene::corpus::physics_object::the_scripts_read_each_bodys_physics_object",
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
        id: "desktop.bodies.the-servers-speed-scales-reach-the-bodies",
        says: "The scales of a body's speed, acceleration and jump the server sends after making \
               it reach the body (the player's or another's) the frame they take effect, and stay \
               until the next (a 2017 player sped to 2.185 times its speed ran at 14.2 m/s).",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-SCALES"),
        station: "auberean-scene::corpus::combat::the_servers_speed_and_acceleration_scales_reach_the_bodies",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "desktop.bodies.what-a-body-holds-is-drawn-with-it",
        says: "An object a body holds is drawn from where the holder is drawn between its ticks, at the scale the holder's whole scale and child scale give it. One told to fade with no delay goes at once, before the fade shows.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-TEMPORARY-HELD-DRAWN"),
        station: "auberean-shell::dat::session::what_a_body_holds_is_drawn_with_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.bodies.what-the-scripts-ask-of-a-body-is-carried-out",
        says: "What the scripts ask of a body is carried out the next frame, in order: its speed \
               and acceleration scales, drawn cloaked (and so not drawn), reporting and ignoring \
               collisions, a push in the world's frame or its own (in the air at that velocity), \
               a push away from another object that has a place (none away from itself), a layer \
               of its animation stopped; a request for an object with no body does nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-PHYSICS-ASK"),
        station: "auberean-scene::corpus::physics_object::what_the_scripts_ask_of_a_body_is_carried_out",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "desktop.camera.a-behavior-naming-another-object-gives-it-the-camera",
        says: "A behavior of the player's that names another object for the camera has that object play the behavior it names, and the camera hangs from one of its bones and looks at another; when the object isn't there, or can't play it, the camera stays on the player.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CAMERA-BEHAVIOR-ON-OBJECT"),
        station: "auberean-shell::dat::session::a_behavior_naming_another_object_gives_it_the_camera",
        tier: Tier::Dat,
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
        id: "desktop.camera.a-forced-position-glides-outside-first-person",
        says: "A newer forced position of the player's own turns on the camera's transition unless \
               the camera is at the eyes.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESK-FORCED-CAMERA"),
        station: "auberean-shell::dat::session::a_forced_position_glides_the_camera_outside_first_person",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.camera.a-glide-turns-the-whole-view",
        says: "A glide between two views turns the camera's whole frame, not the way it looks: on the way it swings round and leans, where a turn of the look alone would dip through the ground.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CAMERA-GLIDE-TURN"),
        station: "auberean-scene::cpu::camera::a_glide_turns_the_whole_view_rather_than_its_look",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.camera.a-new-default-distance-or-tilt-applies-at-once",
        says: "A new default camera distance or tilt takes the camera there at once; another option, or the same ones again, leaves it where it was put.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CAMERA-PREFS-LIVE"),
        station: "auberean-scene::cpu::camera::a_new_default_distance_or_tilt_applies_at_once",
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
        id: "desktop.camera.an-effect-asked-again-does-not-start-its-camera-part-again",
        says: "An effect asked again while it still plays does not start its camera part again.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-FX-CAMERA-ONCE"),
        station: "auberean-shell::dat::session::an_effect_asked_again_does_not_start_its_camera_part_again",
        tier: Tier::Dat,
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
        id: "desktop.camera.first-person-mouse-look-steers-whatever-the-option-says",
        says: "In mouse look in first person the mouse steers the character even with the option \
               that the mouse steers it switched off; the option speaks only for the third person \
               view.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CSTREAM-FIRST-PERSON-STEERS"),
        station: "auberean-scene::cpu::camera::first_person_mouse_look_steers_whatever_the_option_says",
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
        id: "desktop.camera.mouse-look-sets-the-heading-from-the-camera",
        says: "While mouse look steers, the character's heading is the way the camera looks, set \
               every frame rather than turned by the mouse: a camera looking off to one side as \
               mouse look starts turns the character that way, and a turn key turns nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CCAMERA-MOUSE-LOOK-HEADING"),
        station: "auberean-scene::cpu::camera::mouse_look_gives_the_character_the_way_the_camera_looks",
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
        id: "desktop.camera.objects-dont-stop-a-camera-on-a-bone",
        says: "Only the walls and the ground stop a camera in the map view or looking at a bone; otherwise objects stop it too.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CAMERA-ENV-ONLY"),
        station: "auberean-scene::cpu::camera::objects_dont_stop_a_camera_looking_at_a_bone",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.camera.outdoors-the-cameras-own-object-keeps-it-off-the-land",
        says: "Outdoors the camera stands where its own object finds room at the end of its line of sight: its sphere, a quarter of a meter, keeps it off the land and off what it does not pass through.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CAMERA-OWN-OBJECT"),
        station: "auberean-shell::dat::session::outdoors_the_cameras_own_object_keeps_it_off_the_land",
        tier: Tier::Dat,
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
               fifth of a meter ahead and up (placed on it each time the body ticks), and goes \
               back to the eyes when it ends; at peace it stays at the eyes.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-CAMERA-AVATAR"),
        station: "auberean-shell::dat::session::the_avatar_shows_in_first_person_during_combat_and_the_camera_rides_its_head",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.camera.the-camera-is-drawn-between-its-places-at-the-last-two-ticks",
        says: "The camera is placed each time the player's body ticks (ten times a second; each \
               frame in mouse look, with nothing drawn between) and each frame drawn between its \
               places at the last two ticks by the share of the tick gone, as the body is: along \
               the straight line between them, in the cell it stood in before; the shakes under \
               way move where it stands now, which the frames after draw.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-CAMERA-TICKS"),
        station: "auberean-shell::dat::session::the_camera_is_drawn_between_its_places_at_the_last_two_ticks",
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
        id: "desktop.camera.the-scripts-stop-the-cameras-effects",
        says: "The player's scripts stopping the camera's effects, as a played scene ends, ends \
               everything playing on the camera.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-CAMERA-FX-STOP"),
        station: "auberean-scene::corpus::physics_object::the_scripts_stop_the_cameras_effects",
        tier: Tier::Corpus,
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
        id: "desktop.camera.the-view-leans-as-its-frame-does",
        says: "The view leans as the camera's frame does, riding a bone or part way through a glide; the view of a frame with no lean stands level.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CAMERA-LEAN"),
        station: "auberean-scene::cpu::camera::the_view_leans_as_its_frame_does",
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
        id: "desktop.combat.a-missile-that-goes-off-on-a-miss-stops-on-the-creature-it-meets",
        says: "A missile that goes off on a miss goes off on the creature it meets, as the \
               object's kind says, rather than flying on through it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-COMBAT-ON-A-MISS"),
        station: "auberean-scene::corpus::combat::a_missile_that_goes_off_on_a_miss_stops_on_the_creature_it_meets",
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
        id: "desktop.command-line.a-line-that-does-not-read-shows-why-and-the-usage",
        says: "A line that does not read writes why, then every option's usage sorted by long \
               name, padded to 20 columns and wrapped at 80 indented 22; unknown options, loose \
               words, missing values and -? fail it, and a line read whole still needs an \
               account and a host.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-COMMAND-USAGE"),
        station: "auberean-shell::cpu::command_line::a_line_that_does_not_read_shows_why_and_the_usage",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.command-line.the-clients-options-read-as-retail-reads-them",
        says: "The command line names options by -name, --name, /name or a single letter, keeps \
               each value by its form (a port in any base, the account in small letters, a \
               switch's own value), lets a value option take the next word whatever it is, and \
               sets some preferences for the run; a line splits at spaces outside double quotes.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-COMMAND-LINE"),
        station: "auberean-shell::cpu::command_line::the_clients_options_read_as_retail_reads_them",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.controls.a-behavior-action-plays-and-stops-its-behavior",
        says: "An action of the behavior class has the player play its behavior as it begins and stop it as it ends, its value read with no modifier held when it ends.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-ACTION-BEHAVIOR"),
        station: "auberean-shell::dat::controls::a_behavior_action_plays_and_stops_its_behavior",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.controls.a-move-of-the-bodys-own-ends-the-latched-run",
        says: "A latched run ends once the player's body is making a move on its own, walking up to                what it uses or a move the server ordered, so it does not run on when the move is done.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-INPUT-MOVE-ENDS-RUN-LOCK"),
        station: "auberean-shell::dat::session::a_move_ordered_ends_the_latched_run",
        tier: Tier::Dat,
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
        id: "desktop.controls.a-teleport-ends-the-latched-run",
        says: "A latched run ends when the server moves the player with a new teleport stamp, so it does not run on from where it arrives.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-TELEPORT-RUN-LOCK"),
        station: "auberean-shell::dat::session::a_teleport_ends_the_latched_run",
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
        id: "desktop.controls.mouse-look-switching-while-typing-lets-actions-go",
        says: "Mouse look switching lets the actions in force go while a text entry has the keys, and \
               leaves them when none does.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-INPUT-MOUSE-LOOK-FOCUS"),
        station: "auberean-shell::dat::controls::mouse_look_switching_while_a_text_entry_has_the_keys_lets_actions_go",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.controls.no-modifier-key-reaches-the-rows-for-bit-eight",
        says: "The action map's rows for modifier bit 8 give movement values no other row gives, and \
               no modifier action sets that bit, so every movement action keeps its ordinary value.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MODIFIER-BIT-EIGHT"),
        station: "auberean-shell::dat::controls::no_modifier_key_reaches_the_rows_for_bit_eight",
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
        id: "desktop.controls.text-mode-lets-held-actions-go",
        says: "When a text entry takes the keys, each key held is let go and each held action in \
               force ends, but those of the modifier and interface classes; a latched run and a \
               single press stay, and giving the keys back starts nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-TEXT-MODE"),
        station: "auberean-shell::dat::controls::a_text_entry_taking_the_keys_lets_held_actions_go_and_leaves_a_run_and_a_press",
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
        id: "desktop.input.a-click-that-brings-the-window-forward-is-not-heard",
        says: "A window coming active has the devices from its next frame: a button pressed before                then is not heard, nor is that press's release.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-INPUT-FOCUS-CLICK"),
        station: "auberean-shell::cpu::input_manager::a_click_that_brings_the_window_forward_is_not_heard",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.input.an-inactive-window-hears-no-device",
        says: "While the window is not the active one nothing hears a press or a release, the wheel, \
               mouse motion or the pointer; where the pointer went is heard when the window is \
               active again.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-INPUT-ACTIVE-WINDOW"),
        station: "auberean-shell::cpu::input_manager::an_inactive_window_hears_no_device_and_then_the_pointer_where_it_went",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.input.mouse-look-holds-the-pointer-at-the-middle",
        says: "While the pointer is held for mouse look nothing hears where it is; taking hold of it \
               puts it at the middle of the display.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-INPUT-MOUSE-LOOK-MODE"),
        station: "auberean-shell::cpu::input_manager::mouse_look_holds_the_pointer_at_the_middle_of_the_display",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.input.the-shell-passes-events-through-the-gate",
        says: "The interface and the scene hear the pointer only while the window is active and the \
               pointer is not held; the shell puts it at the middle of the display when mouse look \
               takes hold of it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-INPUT-SHELL-GATE"),
        station: "auberean-shell::dat::session::the_shell_passes_events_through_the_gate",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.interface.a-body-past-its-visible-distance-keeps-its-box",
        says: "A body past its visible distance is not drawn but keeps the box what marks it stands on: the selection's name, marks and meters go on showing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-FAR-MARKS"),
        station: "auberean-interface::dat::screens::a_body_past_its_visible_distance_keeps_its_box",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.interface.a-boot-at-login-shows-its-line-on-the-disconnected-screen",
        says: "An account the server ends with a boot as it logs in (before the character list) \
               goes from connecting to the disconnected screen, which shows the boot's line (the \
               account is banned) and stays.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-BOOT-AT-LOGIN"),
        station: "auberean-interface::dat::screens::a_boot_at_login_shows_its_line_on_the_disconnected_screen",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.interface.a-boot-ends-the-session-on-the-disconnected-screen-with-its-line",
        says: "A player in the world whose login the server ends with a boot goes to the \
               disconnected screen showing the boot's line (the character was booted): the play \
               screen goes, the character is logged off and the world let go; the screen stays, \
               and its button leaves the game.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-BOOT-IN-WORLD"),
        station: "auberean-interface::dat::screens::a_boot_ends_the_session_on_the_disconnected_screen_with_its_line",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.interface.a-character-named-on-the-line-goes-into-the-world",
        says: "A character named on the command line goes into the world as soon as the account's \
               characters are listed: the list never comes up and no character is made.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-NAMED-ON-LINE"),
        station: "auberean-interface::dat::screens::a_character_named_on_the_line_goes_into_the_world",
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
        id: "desktop.interface.a-layout-file-leaves-the-log-at-its-newest-lines",
        says: "A layout file that makes the chat window smaller leaves the log showing its newest \
               lines, the command's own line among them.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-LAYOUT-NEWEST-LINES"),
        station: "auberean-interface::dat::screens::a_layout_file_leaves_the_log_at_its_newest_lines",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.interface.a-ray-meets-triangles-as-the-clients-trace-finds-them",
        says: "A ray meets a list of triangles as the client's trace finds it: the nearest triangle \
               whose plane the ray reaches, as a share of the ray's length, with the triangle's normal; \
               one-sided only from in front; a triangle whose normal is under 0.0002 long, a ray ending \
               short of the plane, and a ray within 0.0002 of the plane miss. The picks try each drawn \
               batch two-sided.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PICK-TRILIST"),
        station: "auberean-scene::cpu::view::a_ray_meets_triangles_as_the_clients_trace_finds_them",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.interface.a-record-not-stored-ends-the-session-with-the-clients-line",
        says: "A record the server sends that the client cannot store stops the client: the \
               session ends and the disconnected screen shows the client's own line, that it \
               failed to save data, and stays.",
        since: THIS_CLIENT,
        divergence: "CD2-stops-on-the-disconnected-screen",
        evidence: Evidence::Private("AC2-EVID-STOP-SAVE-FAILED"),
        station: "auberean-interface::dat::screens::a_record_that_cannot_be_stored_ends_the_session_on_the_disconnected_screen_with_its_line",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.interface.a-renamed-selection-is-named-anew",
        says: "When the server renames an object (a visual update of its name quality) the \
               player's scripts say its radar look changed; the interface asks its name again and \
               the panels hear that notice a frame later, with the world that has the new name, so \
               the selection's name over it is the new one.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-RENAMED-SELECTION"),
        station: "auberean-interface::dat::screens::a_renamed_selection_is_named_anew",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.interface.a-resized-chat-windows-scroll-bar-follows-it",
        says: "A chat window made taller keeps its scroll bar: with the text all showing, the \
               widget fills the taller bar once the frame is drawn.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-RESIZED-CHAT-BAR"),
        station: "auberean-interface::dat::screens::a_resized_chat_windows_scroll_bar_follows_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.interface.a-server-gone-ends-the-session-on-the-disconnected-screen",
        says: "A player in the world whose world server goes (its goodbye answered, or 140 \
               seconds of silence) goes to the disconnected screen saying the connection to the \
               server was lost: the session is over and the play screen gone; the screen stays.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CGAME4-SERVER-GONE-SCREEN"),
        station: "auberean-interface::dat::screens::a_world_server_gone_ends_the_session_on_the_disconnected_screen",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.interface.a-story-is-played-before-what-the-server-said-after-it",
        says: "A story takes effect as it arrives, before what the server said after it: a killing arrow's hit, told by its target with its death right behind, writes its line and raises its number, while the same hit told on a target already dead shows neither.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-STORY-ORDER"),
        station: "auberean-interface::dat::screens::a_killing_arrows_hit_is_shown_before_the_target_is_dead",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.interface.an-inactive-window-lets-go-of-what-the-elements-hold",
        says: "With the window going inactive the elements let go of every key and button they \
               hold: mouse look held by the right button ends, a panel held by the left button is \
               let go, Shift and Ctrl read as up.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-INACTIVE-LETS-GO"),
        station: "auberean-interface::dat::screens::an_inactive_window_lets_go_of_what_the_elements_hold",
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
        id: "desktop.interface.request-assistance-opens-the-issues-page",
        says: "The options window's Request Assistance button opens the project's issues page in the \
               player's browser; when the browser cannot be started, a box gives the error code and \
               the page's address.",
        since: THIS_CLIENT,
        divergence: "CD2-assistance-opens-issues-page",
        evidence: Evidence::Private("AC2-EVID-DESKTOP-ASSISTANCE-PAGE"),
        station: "auberean-interface::dat::screens::request_assistance_opens_the_issues_page_or_says_it_could_not",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.interface.the-body-holds-its-place-before-the-camera-in-mouse-look",
        says: "Running while mouse look turns it, the player's body holds its place before the \
               camera from frame to frame, as far from it and facing the way it faces, though its \
               physics ticks less often than frames are drawn.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-MOUSE-LOOK-BODY-STEADY"),
        station: "auberean-interface::dat::screens::the_body_holds_its_place_before_the_camera_in_mouse_look",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.interface.the-character-screen-runs-the-portal-under-its-backdrop",
        says: "The character screen runs the portal's scene in its viewport, under the backdrop, from when it comes up, with or without a character to enter.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-SELECT-PORTAL"),
        station: "auberean-interface::dat::screens::the_character_screen_runs_the_portal_under_its_backdrop",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.interface.the-input-method-writes-over-the-chat-entry-while-it-takes-the-keys",
        says: "An editable entry that takes the keyboard turns the platform's input method on over \
               the part of it that shows, unless it is marked to keep it off; the host is told \
               where in the window, and told it is off once the entry lets the keyboard go.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-IME-HOST"),
        station: "auberean-interface::dat::screens::the_input_method_writes_over_the_chat_entry_while_it_takes_the_keys",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.interface.the-layout-commands-write-and-read-the-panels-places",
        says: "The player's commands write the panels' places to the file they name and put them \
               back from it, and a file that isn't there changes nothing. With no name the file \
               is this character's, world's and screen's; the world's name is the login's, with \
               or without the friends service.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-LAYOUT-FILE"),
        station: "auberean-interface::dat::screens::the_layout_commands_write_and_read_the_panels_places",
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
        id: "desktop.interface.the-panels-hear-portal-space-as-it-is-drawn",
        says: "The panels are told portal space is drawn for as long as the scene draws it, past \
               the server's having put the player back.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-PANELS-PORTAL"),
        station: "auberean-interface::dat::screens::the_panels_hear_portal_space_as_it_is_drawn",
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
        id: "desktop.interface.the-play-screen-sends-the-options-to-keep",
        says: "What the play screen asks to keep the player's options (the radar's coordinates \
               button, its end, logging off) sends them to the server through the player's \
               scripts.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-KEEP-OPTIONS"),
        station: "auberean-interface::dat::screens::the_radars_coordinates_button_sends_the_options_to_keep",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.interface.the-play-screens-kept-places-go-into-a-table-and-come-back",
        says: "The play screen's kept places go into a table of saved places as the client keeps \
               them (each panel under its layout and name, its corners and whether it shows) and \
               come back from one: a panel moved after the table was made is put back where it \
               keeps it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-KEPT-PLACES-TABLE"),
        station: "auberean-interface::dat::screens::the_play_screens_kept_places_go_into_a_table_and_come_back",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.interface.the-players-scripts-write-its-own-name",
        says: "The servers name the player in its own description, a written name with its meta \
               letters after it, as all 433 player creations of the January 2017 sessions did; the \
               player's scripts write its own name from it, the meta letters left off.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-PLAYER-OWN-NAME"),
        station: "auberean-interface::dat::screens::the_players_scripts_write_its_own_name",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.interface.the-screens-before-the-world-keep-1024-by-768-in-any-window",
        says: "The screens before the world are laid out on a 1024 by 768 screen whatever the \
               window's size (the client sets its display to 1024 by 768 for them), shown as large \
               as the window lets it at those proportions, in its middle: a click lands on the \
               button drawn under it, and one beside the screen on nothing. The pointer \
               shows the data's default cursor. Credits shows the credits, which an interface key \
               ends; Options the key map, the client's maps by action in the game's order, whose \
               Back returns to the list.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-PREGAME-1024"),
        station: "auberean-interface::dat::screens::the_screens_before_the_world_keep_1024_by_768_in_any_window",
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
        id: "desktop.interface.the-screenshot-key-asks-for-a-screenshot",
        says: "The key the player's key map names for screenshots asks for one when it is let go, once; the keys the map gives other jobs do not.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-SCREENSHOT-KEY"),
        station: "auberean-interface::dat::screens::the_screenshot_key_asks_for_a_screenshot",
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
        id: "desktop.interface.what-the-play-screen-asks-is-carried-out",
        says: "Each request of the play screen reaches what carries it out: being brought back, turning an offer down, getting down from a mount, keeping the options, yes to the server's question, the groove level, the world hidden and shown, what the status bar covers, and the examination panel opened.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-PLAY-REQUESTS"),
        station: "auberean-interface::dat::screens::what_the_play_screen_asks_is_carried_out",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.keys.a-behavior-action-plays-and-stops-on-the-player",
        says: "An action of the behavior class asks the player to play its value when it begins and to \
               stop it when it ends, once each.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-INPUT-BEHAVIOR-ACTION"),
        station: "auberean-shell::cpu::keys::a_behavior_action_asks_the_player_to_play_it_and_to_stop_it",
        tier: Tier::Cpu,
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
        id: "desktop.keys.a-preset-binds-and-writes-nothing",
        says: "A key map preset is bound at once and written to no file until Apply, so Undo does not take it back.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-KEYBOARD-PRESET-LIVE"),
        station: "auberean-shell::cpu::keys::a_preset_binds_and_writes_nothing",
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
        id: "desktop.looks.a-bodys-systems-draw-at-its-whole-scale",
        says: "The particle systems an object's effects carry draw at the object's whole scale, its                look's times its own, not its look's alone: a gateway sign's haze stays a thin rim                at its foot.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-SYSTEM-WHOLE-SCALE"),
        station: "auberean-scene::corpus::combat::a_bodys_systems_draw_at_its_whole_scale",
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
        id: "desktop.others.a-body-created-again-comes-back-into-the-world",
        says: "A body waiting out of the world in a cell not loaded, created again, stands where                its creation says at once, in the world when that cell is loaded and waiting in                that cell's list when it is not: a player already at the far end of a portal is                seen by one who follows without having to move.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-OTHERS-CREATED-AGAIN"),
        station: "auberean-interface::dat::screens::a_body_created_again_where_the_player_went_stands_there",
        tier: Tier::Dat,
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
        id: "desktop.others.a-moving-body-past-the-range-stops-until-a-position-comes",
        says: "A moving body checks the player's range as it moves. Over 96 meters away it stops \
               where it is, its sounds silent, until a position for it comes; a body that can't \
               move doesn't check.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-OTHERS-RANGE-EACH-MOVE"),
        station: "auberean-scene::cpu::others::a_moving_body_past_the_range_stops_until_a_position_comes",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.others.a-place-in-the-world-takes-an-item-out-of-its-pack",
        says: "An object given a place in the world stands there and is drawn, in no container,                with or without word that it left the pack it was in: the server tells the player                who put an item down only its place.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-OTHERS-PUT-DOWN"),
        station: "auberean-interface::dat::screens::an_item_given_a_place_in_the_world_is_drawn",
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
        id: "desktop.others.an-animations-jump-lifts-another-body",
        says: "A jump another body's animation reaches wakes that body and lifts it, with no                position sent; its hooks are still heard.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-OTHERS-ANIM-JUMP"),
        station: "auberean-scene::cpu::others::an_animations_jump_lifts_another_body",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.others.an-object-hanging-from-another-is-lost-in-no-cell",
        says: "An object lost in a cell goes out of any cell it was lost in first; one that hangs \
               from another is then lost in none. A cell loaded gives back the objects lost in it, \
               in the order they were lost.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-OTHERS-LOST-PARENT"),
        station: "auberean-scene::cpu::others::an_object_hanging_from_another_is_lost_in_no_cell",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.others.an-object-moves-as-its-qualities-record-says",
        says: "Whether an object moves is asked of one qualities record, its entity template's \
               when its description names an entity (none when the id is not an entity's), else \
               its script package's: it moves when that record does not say otherwise, never when \
               the data has no such record, and a body that does not move stays where it stands \
               whatever positions are sent for it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-MOBILE-RULE"),
        station: "auberean-scene::cpu::others::an_object_moves_as_its_qualities_record_says",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.others.an-object-whose-qualities-record-is-missing-stays-where-it-was-made",
        says: "An object made of an entity template with no qualities record in the data does \
               not move: sent a place 2 m on, it stays where it was made, while one whose record \
               says it moves walks there.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-MOBILE-MISSING"),
        station: "auberean-shell::dat::session::an_object_whose_qualities_record_is_missing_stays_where_it_was_made",
        tier: Tier::Dat,
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
               loaded, and none with no player; an object sent into a cell not loaded leaves the \
               world, kept in that cell's list of lost objects with where it was sent, and enters \
               the world there when the cell is loaded again.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-OTHERS-LOST"),
        station: "auberean-scene::cpu::others::objects_in_cells_not_loaded_wait_out_of_the_world",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.others.whether-a-recorded-object-moves-is-its-records-word",
        says: "Every object the 2017 sessions create names an entity template whose qualities \
               record the data holds; 342 of those 3,282 records (the corpse's among them) say \
               what they describe does not move, so 40,595 of the 160,633 creations are still.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-MOBILE-RECORDED"),
        station: "auberean-scene::corpus::mobility::whether_a_recorded_object_moves_is_its_records_word",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "desktop.player.an-impulse-of-the-servers-throws-the-body",
        says: "A position of the player's own with an impulse throws the body the host moves: it \
               leaves the ground at the position's velocity.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESK-PLAYER-IMPULSE"),
        station: "auberean-shell::dat::session::a_position_of_the_players_own_with_an_impulse_throws_the_body",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.preferences.a-value-set-for-the-run-comes-before-the-files",
        says: "A preference set for the run (from the command line) is read before the file's; a \
               number only when the whole value reads as one; an empty value or an unnamed \
               preference is none.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-PREFERENCES-RUN"),
        station: "auberean-shell::cpu::preferences::a_value_set_for_the_run_comes_before_the_files",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.preferences.a-value-written-lands-where-the-clients-system-puts-it",
        says: "A value written replaces the old one in place, or goes after its section's last \
               key (at the end when the section is last and ends blank), or in a new section at \
               the end; new lines end in CR LF, and an unended last line is ended first.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-PREFERENCES-WRITE"),
        station: "auberean-shell::cpu::profile::a_value_written_lands_where_the_clients_system_puts_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.preferences.named-by-the-datas-two-enumerations",
        says: "The preferences' sections and keys are named by two enumerations in the data (Camera \
               and CameraDistance, KeyMap and Normal, Net and UserSpecifiedPort, International \
               and Language), and a preference asked for by number reads the file by those names.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-PREFERENCES-NAMES"),
        station: "auberean-shell::dat::preferences::named_by_the_datas_two_enumerations",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.preferences.the-file-reads-as-the-clients-system-reads-it",
        says: "The preferences file reads as the client's system reads it: the first section of a \
               name, keys and values trimmed, a value quoted at both ends unquoted, comments and \
               lines with no equals sign no keys, a missing key's default less its trailing \
               spaces, cut to the buffer.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-PREFERENCES-FILE"),
        station: "auberean-shell::cpu::profile::the_file_reads_as_the_clients_system_reads_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.preferences.the-language-is-the-lines-else-the-preferences-else-english",
        says: "The language to run in is the command line's, else the preferences' International \
               Language (trimmed), by its name in the languages enumeration without letter case; \
               English when neither names one.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-LANGUAGE"),
        station: "auberean-shell::cpu::preferences::the_language_is_the_lines_else_the_preferences_else_english",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.scene.the-applied-graphics-preferences-reach-the-next-frame",
        says: "The graphics preferences the player applies reach the world's next frame: its \
               textures' detail and filtering, and its materials' detail.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-PREFERENCES-APPLIED"),
        station: "auberean-shell::dat::session::the_applied_graphics_preferences_reach_the_next_frame",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.scene.the-bodies-effects-place-lights-among-the-frames",
        says: "The lights the bodies' own effects place join the frame's lights with those of the \
               objects placed with the world: a light played on another human stands with it at its \
               effect's offset for as long as the effect says (as long as the human, or a few \
               seconds), at the body's opacity; with light source effects off, no effect places \
               one.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCENE-BODY-EFFECT-LIGHTS"),
        station: "auberean-shell::dat::session::the_bodies_effects_place_lights_among_the_frames",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.session.leaving-the-server-says-goodbye",
        says: "Leaving the server says goodbye on the login and the world connections, and the session \
               goes.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SESSION-LEAVE"),
        station: "auberean-shell::dat::session::leaving_the_server_says_goodbye_on_every_connection",
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
        id: "desktop.settings.a-mac-window-counts-its-size-in-points",
        says: "On macOS a window's size is in points, so it opens as large on a Retina screen \
               as on any other: the picture is drawn at that size and scaled to fill the \
               window, and the pointer lands on the same place in the picture.",
        since: THIS_CLIENT,
        divergence: "CD2-mac-window-goes-by-points",
        evidence: Evidence::Private("AC2-EVID-MAC-WINDOW-POINTS"),
        station: "auberean-shell::cpu::settings::a_mac_window_counts_its_size_in_points",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.settings.full-screen-keeps-the-pictures-shape-at-the-size-asked-for",
        says: "Full screen leaves the monitor's display mode alone: the picture is drawn at the \
               display's size and scaled as large as its shape goes in the middle of the monitor, \
               black bars around it, and the pointer lands on the same place in the picture; on a \
               bar it is held to the picture's edge and a press there is not heard.",
        since: THIS_CLIENT,
        divergence: "CD2-full-screen-keeps-its-shape",
        evidence: Evidence::Private("AC2-EVID-FULL-SCREEN-BORDERLESS"),
        station: "auberean-shell::cpu::settings::full_screen_keeps_the_pictures_shape_at_the_size_asked_for",
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
        id: "desktop.settings.the-avatars-looks-read-by-the-clients-names",
        says: "The avatar's three looks (around with the camera, back at a camera in front, at \
               each new selection) are read from its section by the client's names, each on \
               unless the file says 0, and written back.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-AVATAR-LOOKS"),
        station: "auberean-shell::cpu::settings::the_avatars_looks_read_by_the_clients_names",
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
        id: "desktop.settings.the-lands-blending-is-read-from-the-file-and-the-line",
        says: "The land's blending is on unless the preferences file or the command line turns it off, the line's say going over the file's.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-LANDSCAPE-BLENDING-PREF"),
        station: "auberean-shell::cpu::settings::the_lands_blending_is_read_from_the_file_and_the_line",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.settings.the-rendering-options-read-by-the-clients-names-and-kept",
        says: "The rendering options are read by the client's own names, and written back by them: \
               a first start waits for the display's refresh and smooths its edges, with no third \
               buffer.",
        since: THIS_CLIENT,
        divergence: "CD2-the-window-can-be-any-size",
        evidence: Evidence::Private("AC2-EVID-OPTIONS-RENDERING-SETTINGS"),
        station: "auberean-shell::cpu::settings::the_rendering_options_read_by_the_clients_names_and_kept",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.settings.the-screen-brightness-is-the-preference-kept",
        says: "The brightness the screen takes is the preference kept; with none kept the picture \
               stands as it is.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SETTINGS-BRIGHTNESS"),
        station: "auberean-shell::cpu::settings::the_screen_brightness_is_the_preference_kept",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.shell.a-behavior-that-locks-actions-holds-the-player",
        says: "A behavior asked with its actions locked holds the player's own movement, its turning and its jumping while it plays; they come back when it ends.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-ACTION-LOCK"),
        station: "auberean-shell::dat::session::a_behavior_that_locks_actions_holds_the_player_until_it_ends",
        tier: Tier::Dat,
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
        id: "desktop.shell.a-player-put-where-there-is-no-room-stays-in-portal-space",
        says: "A player the server puts where there is no room for the body stays in portal \
               space and out of the world, trying again each frame: it does not fall and nothing \
               is reported of it. A place with room brings it out.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-NO-ROOM"),
        station: "auberean-shell::dat::session::a_player_put_where_there_is_no_room_stays_in_portal_space",
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
        id: "desktop.shell.figures-animate-a-thirtieth-of-a-second-apart-at-any-frame-rate",
        says: "The figures animate only on a frame a thirtieth of a second or more after their \
               last step, by the time since: at 144 frames a second a running player's figure \
               takes a step every fifth frame, about 29 a second, while its body moves every \
               tick.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCENE-ANIM-CLOCK"),
        station: "auberean-shell::dat::session::figures_animate_a_thirtieth_of_a_second_apart_at_any_frame_rate",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.shell.losing-the-keyboard-ends-held-moves-and-keeps-a-latched-run",
        says: "The window losing the keyboard lets each action in force go as its key would: a \
               held strafe ends and the next reports carry no sideways motion, while a run Q \
               latched goes on.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-FOCUS-LOSS"),
        station: "auberean-shell::dat::session::losing_the_keyboard_ends_a_held_strafe_and_keeps_the_latched_run",
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
        id: "desktop.shell.the-host-shows-the-game-as-the-interface-asks",
        says: "How the game is shown goes to the host and into the settings as the interface asks: \
               a flip of full screen is the interface's to answer, and what it answers is carried \
               out.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-OPTIONS-RENDERING-FLIP-HOST"),
        station: "auberean-shell::dat::session::the_host_shows_the_game_as_the_interface_asks",
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
        id: "desktop.shell.the-reports-carry-the-motion-the-body-was-asked",
        says: "The client's reports of where the player is carry the motion its body's \
               interpreter was asked (a diagonal cut to length 1), not the keys: while a move the \
               server ordered runs the body, the motion that move asks.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-REPORT-MOTION"),
        station: "auberean-shell::dat::session::the_reports_carry_the_motion_a_move_ordered_asks",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.shell.the-scene-is-reported-after-a-frame-of-the-world",
        says: "The server hears the scene is ready only once a frame of the world has been drawn.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCENE-AFTER-FRAME"),
        station: "auberean-shell::dat::session::the_scene_is_reported_after_a_frame_of_the_world",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.shell.the-window-title-names-the-account",
        says: "The window's title is the client's name followed by the account the player logged \
               in with (`Auberean | kara`), as Dereth's client titles its own; before a login it \
               is the client's name alone.",
        since: THIS_CLIENT,
        divergence: "CD2-window-title-names-the-account",
        evidence: Evidence::Private("AC2-EVID-WINDOW-TITLE"),
        station: "auberean-shell::dat::session::the_window_title_names_the_account_the_player_logged_in_with",
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
        id: "desktop.sound.a-jump-and-its-landing-are-heard",
        says: "The body leaving the ground plays the jump effect's sound, and coming down plays                the landing effect's.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SOUND-JUMP-LAND"),
        station: "auberean-shell::dat::session::a_jump_and_its_landing_are_heard",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.sound.an-object-out-of-the-world-is-silent-and-one-gone-stops",
        says: "The sounds of an object the client still holds wait while it is out of the world, \
               to play again when it is back; only an object the client no longer holds stops \
               them for good.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-SOUND-OUT-OF-WORLD"),
        station: "auberean-shell::cpu::sound::an_object_out_of_the_world_is_silent_and_one_gone_stops",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.sound.portal-space-is-heard",
        says: "As portal space starts its start sound plays and its two loops stay on while it \
               lasts; as it goes, its stop sound plays and the loops stop.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SOUND-PORTAL-SPACE"),
        station: "auberean-shell::dat::session::portal_space_is_heard",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.sound.the-radars-groove-level-reaches-the-music",
        says: "The groove level the radar works out from what is near sets the level the music's patterns follow.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SOUND-GROOVE-REACHES"),
        station: "auberean-shell::dat::session::the_radars_groove_level_reaches_the_music",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.text.references-the-files-lack-are-asked-for-through-the-clients-cache",
        says: "Once a client is here the interface writes string references through its data cache \
               (a reference its files write reads as before and asks for nothing), and a reference \
               naming a table the files lack starts a string download there, which waits.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-STRDL-INTERFACE"),
        station: "auberean-interface::dat::text::references_the_files_lack_are_asked_for_through_the_clients_cache",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.world.a-blocks-placed-objects-fade-in-with-it",
        says: "The objects placed on a landblock taken up fade in with it, whole a second on.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-BLOCK-FADE-OBJECTS"),
        station: "auberean-shell::dat::session::a_blocks_placed_objects_fade_in_with_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.world.a-body-drawn-has-a-place-on-the-screen",
        says: "A body drawn in front of the camera has a place on the screen, the middle of \
               its box; one behind the camera or not drawn has none.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-BODY-ON-SCREEN"),
        station: "auberean-scene::cpu::seen::a_body_drawn_has_a_place_on_the_screen",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.world.a-landblock-the-server-sends-is-made-into-the-land",
        says: "A landblock the files lack is asked of the server when the cells' rings reach it; \
               the one it sends is read by the world as the files' own and made into the land the \
               player stands on.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CELLS-SERVED"),
        station: "auberean-scene::corpus::cells::a_landblock_the_server_sends_is_made_into_the_land_the_player_stands_on",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "desktop.world.a-landblocks-objects-fade-in-over-a-second-as-it-is-taken-up",
        says: "A landblock's objects fade in over the second after the near window takes it up, made \
               or found again among the free blocks: each frame lists the blocks taken up less than a \
               second before (not at the very moment) with the time since as their objects' opacity, \
               and a body standing on such a block's land is drawn at that opacity; the blocks the \
               window keeps are not listed.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-FADE-IN"),
        station: "auberean-shell::dat::session::a_landblocks_objects_fade_in_over_a_second_as_it_is_taken_up",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.world.a-moved-game-clock-relights-the-ground",
        says: "A time of day moved by an admin lights the ground on the next frame, and the                light's tick is shortened by the rate the world's time runs at.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-MOVED-CLOCK-RELIGHTS"),
        station: "auberean-shell::dat::session::a_moved_game_clock_relights_the_ground_at_once_and_a_faster_one_more_often",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.world.a-player-in-deep-water-rises-and-swims",
        says: "A player on the bottom of deep water is lifted until its water line is near the                surface, and the forward key moves it at its motion description's water speed.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WATER-MOTION"),
        station: "auberean-shell::dat::session::a_player_in_deep_water_rises_and_swims",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.world.a-press-falls-on-the-body-under-it-or-failing-that-on-its-box",
        says: "A press of the pointer falls on the nearest body whose drawn triangles lie under \
               it; failing any, on the body whose box lies under it and reaches least deep. The \
               player's own body is found only by its triangles and behind everything else met, \
               and never with the camera at its eyes.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-PRESS-SELECTION"),
        station: "auberean-shell::dat::session::a_press_falls_on_the_body_under_it_or_failing_that_on_its_box",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.world.a-press-finds-a-box-with-a-corner-on-the-screen",
        says: "A press finds a body by its box only when a corner of the box stands on the screen                and the box's square of clip space meets the pointer's: the pixel up and to the left                of it to that one.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-PRESS-BOX"),
        station: "auberean-scene::cpu::seen::a_press_finds_a_box_only_with_a_corner_on_the_screen",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.world.a-server-object-that-says-so-casts-on-the-land",
        says: "An object the server made standing outdoors in the detailed blocks prints on the sun's shadow map when its drawing properties say it casts on the land. It prints as it stands when the light ticks and stays that way until the next tick.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-LAND-CASTERS-DYNAMIC"),
        station: "auberean-shell::dat::session::a_server_object_that_says_so_casts_on_the_land",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.world.an-effects-decal-is-laid-where-its-object-stands",
        says: "An effect with a decal lays it where its object stands, on the land outdoors and on its cell's ground inside, and it is drawn with the frame.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-FX-DECAL-LAID"),
        station: "auberean-shell::dat::session::an_effects_decal_is_laid_where_its_object_stands",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.world.an-object-is-told-the-mode-its-body-goes-into",
        says: "Each body the server asks into a mode, or that a behavior carrying one puts there, \
               tells its object the mode, as a notice about the object; every mode the server asked \
               of a body the scene has is told.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-MODE-NOTICE"),
        station: "auberean-scene::corpus::objects::an_object_is_told_each_mode_its_body_goes_into",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "desktop.world.no-world-is-drawn-from-a-cell-still-waited-for",
        says: "The player put in a cell that isn't there yet, with no portal space on the way: while its cells are waited for no world is drawn at all, and once they are in it is drawn from there.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-NOT-DRAWN-WAITING"),
        station: "auberean-shell::dat::session::no_world_is_drawn_from_a_cell_still_waited_for",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.world.rain-falls-on-a-rainy-day",
        says: "On a rainy day the rain falls about the camera as its day says, and none on a \n               dry moment.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-RAIN-WORLD"),
        station: "auberean-shell::dat::session::rain_falls_on_a_rainy_day",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.world.the-cells-asks-are-the-2017-clients",
        says: "As the player moves, the cells about it are asked of the data cache a ring of \
               landblocks at a time; the records the files lack are asked of the server as the 2017 \
               clients asked for them, and as often.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CELLS-ASKS"),
        station: "auberean-scene::corpus::cells::the_cells_asks_for_records_the_files_lack_are_the_2017_clients",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "desktop.world.the-cells-asks-are-told-by-their-own-number",
        says: "The data cache tells each listener only its own batches: the cells' asks are \
               told by a number of their own, neither the objects' creations' nor the \
               interface's string downloads', so the scene hears when the cells about the \
               player are in.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-CELLS-LISTENER"),
        station: "auberean-scene::cpu::cells::the_cells_asks_are_told_by_a_number_of_their_own",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.world.the-client-knows-where-the-scenes-bodies-stand",
        says: "Every body the scene draws is made through the client from the physics record its \
               object's repository names (for many objects their template's); the client is told \
               the cell each body stands in, and none for one out of the world; the camera's two \
               stand-ins are among the client's objects from the player's arrival; the region the \
               player's creation names is taken up; the world's refresh makes the landscape again.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-SEAM"),
        station: "auberean-scene::corpus::objects::the_client_knows_where_each_body_stands_and_names_its_physics_record",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "desktop.world.the-clients-objects-wait-with-the-world-for-its-cells",
        says: "While the cells about the player the server just placed are asked for, the world \
               stands still and the client's objects wait with it (their upkeep does not run); \
               once the cells are in, both go on.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-OBJECTS-WAIT-FOR-CELLS"),
        station: "auberean-shell::dat::session::the_clients_objects_wait_with_the_world_for_its_cells",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.world.the-days-wind-blows-in-the-world",
        says: "The day's wind blows on the world's missiles: the day's own while it rains, and one that wanders                with the time of day on a dry moment.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-WIND"),
        station: "auberean-shell::dat::session::the_days_wind_blows_in_the_world",
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
        id: "desktop.world.the-objects-the-client-makes-are-among-its-objects",
        says: "Every object the client makes itself (an arrow) is among the client's objects while \
               the scene has it, with its timeout as its lifetime; one the client is asked to \
               destroy goes a quarter of a second later, from the scene with it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-TEMPORARIES"),
        station: "auberean-scene::corpus::objects::the_objects_the_client_makes_are_among_its_objects_while_they_last",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "desktop.world.the-player-arrives-and-runs-in-a-room-with-no-doorways",
        says: "Put in the training ground's second area, a closed room with no doorways, the \
               player stands where it is put and runs on the floor, never leaving the cell or \
               sinking through the floor.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SHELL-CLOSED-ROOM"),
        station: "auberean-shell::dat::session::the_player_arrives_and_runs_in_a_room_with_no_doorways",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.world.the-players-arrival-preloads-its-scripts-singletons",
        says: "The player's arrival has its scripts preload what they keep about the player: by the \
               first frame the scene has the player, the master skill list is made.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-PRELOAD"),
        station: "auberean-scene::corpus::objects::the_players_arrival_preloads_the_master_skill_list",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "desktop.world.the-shore-effects-preference-sets-what-the-shores-draw",
        says: "The landscape's shore effects preference says what the shores draw: nothing when \
               disabled; their strips in the frame's alpha at medium; those and their foam at \
               high.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-SHORE-EFFECTS"),
        station: "auberean-scene::cpu::landscape::the_shore_effects_preference_sets_what_the_shores_draw",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.world.the-sky-follows-the-camera-every-frame-and-the-ground-is-lit-on-the-lights-tick",
        says: "Outdoors the sky stands at the camera every frame and moves on with the moment, as \
               do the light and fog; the ground keeps the lighting it was lit with until the \
               light's tick, the sky's light interval (30 s) after the last.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-SKY-EVERY-FRAME"),
        station: "auberean-shell::dat::session::the_sky_follows_the_camera_every_frame_and_the_ground_is_lit_on_the_lights_tick",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.world.the-world-hidden-is-not-drawn",
        says: "A world hidden while a story plays makes no frame, and is drawn again when shown.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-HIDDEN"),
        station: "auberean-shell::dat::session::the_world_hidden_is_not_drawn_and_shown_again_is",
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
    behaviour! {
        id: "desktop.world.under-water-the-fog-is-the-waters-at-the-cameras-depth",
        says: "With the camera under its cell's water the fog is the water's at the camera's depth, \
               the light the same; above it, the day's.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-UNDERWATER"),
        station: "auberean-shell::dat::session::under_water_the_fog_is_the_waters_at_the_cameras_depth",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.world.what-the-ambient-sound-hears-follows-the-players-cells",
        says: "What the ambient sound hears about the player follows the cells about it each frame: its \
               cell and place; whether the cells a placement asks for are waited for; the interior \
               cells in the visible set (outdoors, those the near window's landblocks see from \
               outside; inside, its cell and those it sees, with the outside ones when it sees the \
               outdoors); whether the landscape is heard; and the window's landblocks.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-HEARING"),
        station: "auberean-shell::dat::session::what_the_ambient_sound_hears_follows_the_players_cells",
        tier: Tier::Dat,
    },
];
