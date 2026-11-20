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
        says: "A hold the server started loops until the server lets it go, stops those a move \
               cancels, ends its layer, or, while it is within the cycles it was asked for, stops \
               all behaviors; each of them ends it.",
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
        says: "A body keeps a global and a terrain modifier, clears the terrain modifiers, is \
               moved by a push toward an object, and flies when a missile is launched.",
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
        says: "A body is drawn in the color and at the opacity its sliders hold: its red, green, \
               blue and alpha sliders multiply its draws, its fade slider their alpha too.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-ANIM-COLOR"),
        station: "auberean-shell::dat::session::a_body_is_drawn_in_the_color_its_sliders_hold",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.bodies.a-body-kept-out-of-the-game-is-not-drawn",
        says: "A body whose drawing properties keep it out of the game is not drawn, and so casts no shadow; it keeps its place in the world.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-BODY-NOT-IN-GAME"),
        station: "auberean-scene::corpus::combat::a_body_kept_out_of_the_game_is_not_drawn",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "desktop.bodies.a-body-takes-a-script-a-motion-description-and-private-effects",
        says: "A body the server asks plays an effect script and takes a new motion description; \
               a private effect plays on the player whoever else is near.",
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
        id: "desktop.bodies.a-dying-creature-turns-passable-when-the-server-says",
        says: "A creature the server sends its physics type with the passable bit as its corpse \
               appears is passable to the player from the frame the client has it; a word of one \
               of its masks sent after leaves its type as it is.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-DYING-PASSABLE"),
        station: "auberean-scene::corpus::physics_object::a_dying_creature_turns_passable_when_the_server_says",
        tier: Tier::Corpus,
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
        says: "An effect aimed at another object acts on it: an entry that hides its target hides \
               it until the effect is over.",
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
        says: "An object an effect makes hangs from the object playing it at the entry's holding \
               location or bone. An entry naming a behavior plays it on the object; as it ends \
               the object is hidden at once and goes three seconds later, whenever the effect \
               ends.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-FX-MADE-BEHAVIOR"),
        station: "auberean-shell::dat::session::an_object_an_effect_makes_goes_with_its_behavior",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.bodies.an-object-an-effect-makes-lasts-as-its-entry-says",
        says: "An object an effect makes with no behavior lasts as long as its entry says: 15 \
               seconds if it says nothing, until the effect is stopped if it says less than none.",
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
        says: "An object the server creates plays at once the behaviors its description lists, \
               and the effects it lists, those that make objects making them.",
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
        says: "An object takes up its four stamps from its description and, described as a \
               missile, its aim and that it flies.",
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
        says: "Every look of the data that puts a figure together makes a body to stand in the \
               world, those whose mode their motion description does not list among them: a \
               town's crafting workshop is drawn and named.",
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
        says: "A push from the server takes the player's body off the ground; a slider the \
               network moves goes to its value, one the character template leaves to the client \
               does not.",
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
        id: "desktop.camera.a-frame-that-isnt-one-is-not-taken",
        says: "A place for the camera whose frame is not a frame is not taken; the camera stays where it stood.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CAMERA-FRAME-VALID"),
        station: "auberean-scene::cpu::camera::a_frame_that_isnt_one_is_not_taken",
        tier: Tier::Cpu,
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
        id: "desktop.camera.in-mouse-look-it-moves-smoothly-over-the-land-and-a-blocks-edge",
        says: "In mouse look, running over land that bends between the body's ticks and over a \
               block's edge, the camera moves on smoothly every frame, never further in a frame \
               than the body moves in a tick: its lines of sight start where the body stood at its \
               last tick, on the ground.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-CAMERA-MOUSE-LOOK-SMOOTH"),
        station: "auberean-shell::dat::session::in_mouse_look_the_camera_moves_smoothly_over_the_land_and_a_blocks_edge",
        tier: Tier::Dat,
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
        id: "desktop.controls.a-control-pressed-through-the-contract-moves-the-player-as-its-key",
        says: "Our choice: a control pressed through the contract, as a script, a headless driver \
               or a plugin presses it, moves the player as the key bound to it does: forward \
               pressed as itself for two seconds runs the body to the same place as W held for \
               those seconds, reported the same way.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-CONTROLS-CONTRACT"),
        station: "auberean-shell::dat::session::a_control_pressed_through_the_contract_moves_the_player_as_its_key_does",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.controls.a-creature-in-the-way-holds-the-body-up-the-same-with-and-without-the-scene",
        says: "Our choice: a creature the server puts in the player's way holds the player's \
               body up, and turns it off round the creature, the same with the window's scene and \
               with none: the scene runs the other bodies' steps of the world without a window \
               among its own, so the body stands in the same place on every frame and is reported \
               the same way.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SIM-OTHERS-SAME-AS-SCENE"),
        station: "auberean-shell::dat::session::a_creature_in_the_way_holds_the_body_up_the_same_with_and_without_the_scene",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.controls.a-move-of-the-bodys-own-ends-the-latched-run",
        says: "A latched run ends once the player's body is making a move on its own, walking up \
               to what it uses or a move the server ordered, so it does not run on when the move \
               is done.",
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
        id: "desktop.controls.a-teleport-ends-the-approach-the-server-ordered",
        says: "A teleport takes the player's body out of the world for the trip: a move the server \
               ordered it to make (closing in on what it attacks) ends, and it arrives standing and \
               stays still, reported with no motion, as after a respawn at its lifestone.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-TELEPORT-ENDS-MOVE"),
        station: "auberean-shell::dat::session::a_teleport_ends_the_approach_the_server_ordered",
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
        says: "The selection keys ask for their selections, Escape selects nothing and F uses \
               what is selected; a window that loses the keyboard lets every key go.",
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
        id: "desktop.controls.the-controls-move-the-body-the-same-with-and-without-the-scene",
        says: "Our choice: the same controls move the player's body to the same place with the \
               window's scene and with none: forward pressed for two seconds runs it through the \
               hall in the shell and on the SDK's driver with its world without a window to the \
               same place on every frame, reported to the server the same way.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SIM-SAME-AS-SCENE"),
        station: "auberean-shell::dat::session::the_controls_move_the_body_the_same_with_and_without_the_scene",
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
        id: "desktop.controls.the-servers-moves-take-the-body-the-same-with-and-without-the-scene",
        says: "Our choice: the moves the server orders the player's body to make (its speed \
               halved, a push, a walk to a place, a teleport) take it the same way with the \
               window's scene and with none: both play the server's events for bodies through the \
               world without a window's steps, so the body stands in the same place on every frame \
               and is reported the same way.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SIM-SERVER-MOVES-SAME-AS-SCENE"),
        station: "auberean-shell::dat::session::the_servers_moves_take_the_body_the_same_with_and_without_the_scene",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.dev-login.makes-its-character-as-the-creation-screens-do",
        says: "The developer's login makes the human it plays as the creation screens make one: \
               the player's entity, the race's start area, class and implement, and every \
               physique value the table lays out, its looks picked as a new avatar's are.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-DEV-LOGIN-CREATES-AS-SCREENS"),
        station: "auberean-shell::dat::session::the_developers_login_makes_its_human_as_the_creation_screens_do",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.horizon.a-first-switch-to-the-game-s-own-interface-shows-its-login-s-panels",
        says: "The game's own interface chosen for the first time with the player in the world \
               shows what logging in with it shows: the panels its login shows, the windows the \
               player's options keep open, and the shortcut bar on the set the player's scripts \
               hold, the one Horizon showed.",
        since: THIS_CLIENT,
        divergence: "CD2-interface-choice",
        evidence: Evidence::Private("AC2-EVID-HORIZON-SWITCH-FIRST-RETAIL"),
        station: "auberean-horizon::dat::interfaces::a_first_switch_to_the_game_s_own_interface_shows_what_its_login_shows",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.horizon.a-line-selected-in-the-log-is-copied-by-control-c",
        says: "Our choice: text dragged over in Horizon's log is copied to the system's clipboard \
               by Control-C or Control-Insert, as in Dereth's chat; the key maps do not see the \
               key.",
        since: THIS_CLIENT,
        divergence: "CD2-interface-choice",
        evidence: Evidence::Private("AC2-EVID-HORIZON-CHAT-COPY"),
        station: "auberean-horizon::dat::interfaces::a_line_selected_in_horizons_log_is_copied_by_control_c",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.horizon.a-line-typed-goes-to-the-command-line-and-its-answer-is-logged",
        says: "A line typed in Horizon's chat entry goes to the player's command line as the \
               game's own chat sends it: the command line's answer shows in Horizon's log (a line \
               naming no command is answered with the game's line asking for /help, in the \
               error's color), and nothing is sent to the server for it.",
        since: THIS_CLIENT,
        divergence: "CD2-interface-choice",
        evidence: Evidence::Private("AC2-EVID-HORIZON-CHAT-LINE"),
        station: "auberean-horizon::dat::interfaces::a_line_typed_in_horizon_goes_to_the_command_line_and_its_answer_is_logged",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.horizon.a-switch-carries-the-chat-lines-written-before",
        says: "The chat lines written while one interface is shown reach the other's log when it \
               is chosen: Horizon chosen for the first time starts from the lines written before \
               it, and chosen again it has those written while the game's own interface was \
               shown, each once.",
        since: THIS_CLIENT,
        divergence: "CD2-interface-choice",
        evidence: Evidence::Private("AC2-EVID-HORIZON-CHAT-CARRIED"),
        station: "auberean-horizon::dat::interfaces::a_switch_of_interface_carries_the_chat_lines_written_before",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.horizon.before-the-world-its-own-screens-log-in-and-enter",
        says: "With Horizon chosen, the screens before the world are Horizon's own: it logs in \
               itself with the account the client was started with, its connecting screen up \
               until the character list comes; started with a character to play, it enters the \
               world as that character, and Horizon shows its HUD there.",
        since: THIS_CLIENT,
        divergence: "CD2-interface-choice",
        evidence: Evidence::Private("AC2-EVID-HORIZON-BEFORE-WORLD"),
        station: "auberean-horizon::dat::pregame::horizon_logs_in_itself_and_enters_the_world_as_the_character_it_was_started_with",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.horizon.character-select-lists-the-characters-and-enters-the-one-chosen",
        says: "Horizon's character select lists the account's characters, the selected one \
               standing in a scene of its own; nothing is entered by itself, and Enter asks \
               whether to enter the world as the selected character before it does, once.",
        since: THIS_CLIENT,
        divergence: "CD2-interface-choice",
        evidence: Evidence::Private("AC2-EVID-HORIZON-CHARACTER-SELECT"),
        station: "auberean-horizon::dat::pregame::horizon_s_character_select_lists_the_characters_and_enters_the_one_chosen",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.horizon.character-select-shows-a-different-view-each-time-unless-one-is-chosen",
        says: "Horizon's character select, unless its settings name one view, shows one picked \
               with the client's random numbers each time it comes up, never the one shown the \
               time before, and keeps it until the player leaves for the world; the view picked \
               is kept in Horizon's own file, so the next run's is another.",
        since: THIS_CLIENT,
        divergence: "CD2-interface-choice",
        evidence: Evidence::Private("AC2-EVID-HORIZON-VISTAS-RANDOM"),
        station: "auberean-horizon::cpu::before::vista::random_picks_another_view_each_time_character_select_comes_up_from_a_fixed_seed",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.horizon.character-select-stands-over-a-view-of-cragstone-and-names-the-world",
        says: "Horizon's character select stands over a view of Cragstone drawn from the install \
               with no server, the camera turning slowly as its path says, and names the world \
               the login named; once the player asks to enter the world the view is let go of \
               and nothing of the world is drawn until the player is in it.",
        since: THIS_CLIENT,
        divergence: "CD2-interface-choice",
        evidence: Evidence::Private("AC2-EVID-HORIZON-SELECT-VISTA"),
        station: "auberean-horizon::dat::pregame::horizon_s_character_select_stands_over_cragstone_and_names_the_world",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.horizon.character-select-stands-over-the-view-chosen",
        says: "Horizon's settings choose the view behind character select, Random or one of \
               Cragstone, the Drudge Citadel, Arramora, Linvak Tukal, Aun Tanua and Ikeras, kept \
               in its own file; the one chosen is drawn from the install with no server, its \
               camera turning slowly as its path says.",
        since: THIS_CLIENT,
        divergence: "CD2-interface-choice",
        evidence: Evidence::Private("AC2-EVID-HORIZON-VISTA-DRUDGE-CITADEL"),
        station: "auberean-horizon::dat::views::character_select_stands_over_the_drudge_citadel_once_it_is_chosen",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.horizon.creation-s-height-shows-in-the-figure",
        says: "Horizon's creation scales the whole figure by the height chosen, in a viewport \
               framed the same for every height, so a taller character stands taller in it.",
        since: THIS_CLIENT,
        divergence: "CD2-interface-choice",
        evidence: Evidence::Private("AC2-EVID-HORIZON-HEIGHT-SCALE"),
        station: "auberean-horizon::dat::figures::creation_s_height_scales_the_whole_figure_in_a_frame_that_stays_the_same",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.horizon.entering-shows-portal-space-and-its-fade-as-the-game-s-own-interface",
        says: "Entering the world with Horizon shown, portal space shows from the frame after it is \
               first drawn with nothing of Horizon's over it, and goes out to white and comes out into \
               the world on the same timeline as under the game's own interface.",
        since: THIS_CLIENT,
        divergence: "CD2-interface-choice",
        evidence: Evidence::Private("AC2-EVID-HORIZON-ENTRY-PORTAL-SPACE"),
        station: "auberean-horizon::dat::entry_fade::entering_under_horizon_shows_portal_space_and_its_fade_as_the_game_s_own_interface",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.horizon.its-scale-and-hud-layout-are-kept-in-the-player-s-files",
        says: "Horizon keeps its own settings and each character's HUD layout in the player's \
               files: the interface scale chosen in its settings, and the layout saved in its \
               HUD layout editor, are written there (`auberean-horizon.txt`, \
               `auberean-horizon-hud-<world>-<name>.txt`), and read back when Horizon is next \
               shown and the character is next in the world.",
        since: THIS_CLIENT,
        divergence: "CD2-interface-choice",
        evidence: Evidence::Private("AC2-EVID-HORIZON-KEPT-FILES"),
        station: "auberean-horizon::dat::extras::horizons_scale_and_hud_layout_are_kept_in_the_players_files_and_read_back",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.horizon.log-out-asks-first-in-the-game-s-words",
        says: "Horizon's Log Out (its key, the menu bar's and the settings') asks first in the \
               game's own words, \"Are you sure you want to log out?\", with Yes and No, as the \
               game's own Leave World does; nothing leaves the world until Yes.",
        since: THIS_CLIENT,
        divergence: "CD2-interface-choice",
        evidence: Evidence::Private("AC2-EVID-HORIZON-LOG-OUT-ASKS"),
        station: "auberean-horizon::dat::extras::horizons_log_out_key_asks_first_in_the_games_words_and_yes_asks_to_leave",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.horizon.logging-out-shows-the-wait-then-portal-space-then-character-select",
        says: "Logging out under Horizon puts the HUD away and says it is logging out over the \
               world for as long as the server holds the character; portal space then shows, \
               nothing over it, and stays in sight two seconds after the session leaves the \
               world before character select.",
        since: THIS_CLIENT,
        divergence: "CD2-interface-choice",
        evidence: Evidence::Private("AC2-EVID-HORIZON-LOGOUT-TIMELINE"),
        station: "auberean-horizon::dat::leaving::logging_out_shows_the_wait_then_portal_space_long_enough_to_see_then_character_select",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.horizon.switching-back-keeps-horizon-s-windows-and-the-set-shown",
        says: "Horizon left for the game's own interface and chosen again opens the windows that \
               were open in it again, where they were, and its hotbar shows the set it showed; the \
               game's own shortcut bar shows that set meanwhile.",
        since: THIS_CLIENT,
        divergence: "CD2-interface-choice",
        evidence: Evidence::Private("AC2-EVID-HORIZON-SWITCH-HORIZON-WINDOWS"),
        station: "auberean-horizon::dat::interfaces::switching_back_keeps_horizon_s_windows_where_they_were_and_the_set_shown",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.horizon.switching-back-keeps-the-game-s-own-windows-and-the-set-shown",
        says: "The game's own interface left for Horizon and chosen again shows the windows that \
               were open in it and no others, also when the window was resized meanwhile, and its \
               shortcut bar shows the set Horizon last showed; Horizon shows the set it showed.",
        since: THIS_CLIENT,
        divergence: "CD2-interface-choice",
        evidence: Evidence::Private("AC2-EVID-HORIZON-SWITCH-RETAIL-WINDOWS"),
        station: "auberean-horizon::dat::interfaces::switching_back_keeps_the_game_s_own_windows_and_shows_the_set_horizon_showed",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.horizon.switching-keeps-the-session-and-each-interface-s-log",
        says: "In the world the game's own interface can be left for Horizon and Horizon for it \
               again in one session: the player stays in the world and nothing logs in again; \
               Horizon draws its HUD as textured batches and lets go of its textures when it is \
               left; a choice made in Horizon's settings is kept in the preferences file; and \
               Horizon's log still holds what it showed when it is chosen again.",
        since: THIS_CLIENT,
        divergence: "CD2-interface-choice",
        evidence: Evidence::Private("AC2-EVID-HORIZON-SWITCH"),
        station: "auberean-horizon::dat::interfaces::switching_to_horizon_and_back_keeps_the_session_and_its_log",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.horizon.t-starts-a-reply-in-the-chat-entry",
        says: "Our choice: T, Horizon's reply key, opens its chat entry with the game's reply \
               command in it, as the game's own reply key starts its entry.",
        since: THIS_CLIENT,
        divergence: "CD2-horizon-key-maps",
        evidence: Evidence::Private("AC2-EVID-HORIZON-REPLY-KEY"),
        station: "auberean-horizon::dat::interfaces::t_starts_a_reply_to_the_last_tell_in_horizons_chat_entry",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.horizon.the-figures-before-the-world-play-their-idle",
        says: "Horizon's figures on character select and in creation stand playing their idle, \
               as the game's own screens' do, and go on playing it while creation's is turned.",
        since: THIS_CLIENT,
        divergence: "CD2-interface-choice",
        evidence: Evidence::Private("AC2-EVID-HORIZON-FIGURE-IDLE"),
        station: "auberean-horizon::dat::figures::the_figures_on_character_select_and_in_creation_play_their_idle",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.horizon.the-game-s-own-pointers",
        says: "Our choice: Horizon shows the game's own pointers from its records, as the game's \
               own interface does: the default one, and over the world the usage one while an \
               item waits for its target.",
        since: THIS_CLIENT,
        divergence: "CD2-interface-choice",
        evidence: Evidence::Private("AC2-EVID-HORIZON-CURSORS"),
        station: "auberean-horizon::dat::interfaces::horizon_shows_the_game_s_pointers_and_the_usage_one_while_an_item_waits_for_its_target",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.horizon.the-host-s-pad-plays-the-game-through-the-game-s-controls",
        says: "In the world under Horizon the host's pad, read by the shell once a frame, plays \
               the game: a first press switches gamepad mode on and does nothing else, the left \
               stick pushed ahead holds the forward control in the client as a key would, and a \
               key pressed switches gamepad mode off and lets go of what the pad held.",
        since: THIS_CLIENT,
        divergence: "CD2-interface-choice",
        evidence: Evidence::Private("AC2-EVID-HORIZON-PAD"),
        station: "auberean-horizon::dat::extras::the_hosts_pad_plays_the_game_in_gamepad_mode_and_a_key_switches_it_off",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.horizon.the-inventory-shows-the-player-s-figure-over-its-window",
        says: "In the world Horizon's inventory window opens on Horizon's inventory key (I) and \
               shows the player's figure as it looks, in a scene of its own over the window: the \
               window's batches drawn before the figure's place among them stand under the \
               scene, and the rest over it.",
        since: THIS_CLIENT,
        divergence: "CD2-interface-choice",
        evidence: Evidence::Private("AC2-EVID-HORIZON-INVENTORY-FIGURE"),
        station: "auberean-horizon::dat::things::horizons_inventory_shows_the_players_figure_over_its_window",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.horizon.the-key-bindings-page-writes-horizon-s-key-files",
        says: "Horizon's key bindings page, in its settings, binds a key pressed for an action to \
               it; Apply writes Horizon's own key files and binds them from then on, leaving the \
               player's own key map files alone.",
        since: THIS_CLIENT,
        divergence: "CD2-horizon-key-maps",
        evidence: Evidence::Private("AC2-EVID-HORIZON-KEY-BINDINGS"),
        station: "auberean-horizon::dat::interfaces::horizons_key_bindings_page_writes_its_key_files_and_binds_them",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.horizon.the-map-draws-the-world-from-its-landblocks",
        says: "Horizon's map draws its own picture of the world, not the game's map pictures: \
               every landblock read off the frame, a pixel to each square of the land, the land in \
               the far landscape's colors as the ground picks them and lit by its slope, the sea \
               where no landblock lies or the ground is under water, and a picture of the whole \
               2048 pixels a side.",
        since: THIS_CLIENT,
        divergence: "CD2-interface-choice",
        evidence: Evidence::Private("AC2-EVID-HORIZON-MAP-PICTURE"),
        station: "auberean-horizon::dat::map::the_map_draws_the_world_from_its_landblocks_in_the_far_landscapes_colors",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.horizon.the-map-frames-the-land",
        says: "Horizon's map, fully zoomed out, frames the land rather than the whole world: the \
               bounds of all its land, the placeholder land left out, in the middle with a margin \
               of a twenty-fifth of their longer side about them, past the world's edge where the \
               land comes near it, the sea drawn beyond. It opens there, Fit and the wheel zoom \
               out no further, and a pan stops at the frame's edge.",
        since: THIS_CLIENT,
        divergence: "CD2-interface-choice",
        evidence: Evidence::Private("AC2-EVID-HORIZON-MAP-FRAMED"),
        station: "auberean-horizon::dat::map::the_map_frames_the_continents_and_their_islands",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.horizon.the-map-leaves-out-the-placeholder-islands",
        says: "Horizon's map leaves out, as sea, the placeholder land: each stretch of land that \
               copies another, its landblocks laid out as the other's are and its heights the \
               other's at all but a hundredth of their vertices; and each speck of dead-flat \
               ground more than 32 landblocks from all other land. The nine copies of one island \
               north of the continents go, and the two specks at the world's south-west corner \
               and out in its north-east; the continents and every other island stay.",
        since: THIS_CLIENT,
        divergence: "CD2-interface-choice",
        evidence: Evidence::Private("AC2-EVID-HORIZON-MAP-PLACEHOLDERS"),
        station: "auberean-horizon::dat::map::the_map_leaves_out_the_nine_placeholder_islands_and_two_specks_and_no_other_land",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.horizon.the-map-reads-the-world-s-size-and-labels-from-the-region",
        says: "Horizon's map reads from the world's region the land's size and the closer maps' \
               labels, written from the game's strings; the five continents' names from the game's \
               map window, as its buttons for them name them, each put on its own one of the five \
               largest stretches of land; the fellows' marker's picture; and the game's date and \
               time of day from the region's calendar.",
        since: THIS_CLIENT,
        divergence: "CD2-interface-choice",
        evidence: Evidence::Private("AC2-EVID-HORIZON-MAP"),
        station: "auberean-horizon::dat::map::the_map_reads_the_worlds_size_and_the_closer_maps_labels_from_the_region",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.horizon.the-map-writes-the-time-of-day-and-the-date-from-the-calendar",
        says: "Horizon's map writes the game's time of day and, under it, the date, by the region's \
               calendar and the game's strings: late on the 21st of Leafcull in 664 it says Sunset, \
               then Leafcull 21, 664 P.Y., as the game's own map window does.",
        since: THIS_CLIENT,
        divergence: "CD2-interface-choice",
        evidence: Evidence::Private("AC2-EVID-HORIZON-MAP-DATE"),
        station: "auberean-horizon::dat::map::the_map_writes_the_time_of_day_and_the_date_from_the_calendar",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.horizon.the-minimap-places-the-player-and-holds-the-radar-s-objects",
        says: "In the world Horizon's minimap places the player as the game's radar does (inside \
               a building or a dungeon it reads Inside and reaches the shorter range; outdoors, the \
               map coordinates of the world's size) and holds the objects the player's scripts put \
               on the radar within its reach, the player not among them.",
        since: THIS_CLIENT,
        divergence: "CD2-interface-choice",
        evidence: Evidence::Private("AC2-EVID-HORIZON-MINIMAP"),
        station: "auberean-horizon::dat::interfaces::horizons_minimap_places_the_player_and_holds_the_radars_objects",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.horizon.the-minimap-s-reach-ends-at-its-frame-as-the-game-s-radar-s-does",
        says: "Horizon's minimap's reach ends where its frame's inside edge begins, as the game's \
               own radar's 60-texel reach ends at its bezel: a blip at the edge of the reach \
               stands at the frame, no disc shows past the reach, and the last ring stands \
               0.770 of the way to the frame.",
        since: THIS_CLIENT,
        divergence: "CD2-interface-choice",
        evidence: Evidence::Private("AC2-EVID-HORIZON-RADAR-REACH"),
        station: "auberean-horizon::cpu::play::minimap::the_minimap_s_reach_ends_at_its_frame_as_the_game_s_radar_s_does",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.horizon.the-minimap-s-rings-stand-where-the-game-s-radar-s-do",
        says: "Horizon's minimap draws four rings at the game's own radar's places, 0.187, 0.384, \
               0.581 and 0.770 of its reach from the middle, and they stay there whatever the \
               reach: inside an interior cell (50 meters) they mark 9.3, 19.2, 29.1 and 38.5 \
               meters, outdoors (75 meters) 14.0, 28.8, 43.6 and 57.7; only how far the blips \
               stand changes.",
        since: THIS_CLIENT,
        divergence: "CD2-interface-choice",
        evidence: Evidence::Private("AC2-EVID-HORIZON-RADAR-RINGS"),
        station: "auberean-horizon::cpu::play::minimap::the_minimap_s_rings_stand_where_the_game_s_radar_s_do_inside_and_outdoors",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.horizon.the-money-is-the-game-s-gold-picture",
        says: "Horizon draws the money (the inventory's money bag, coins carried, a sold listing) \
               with the game's own picture of a pile of gold coins, the one the game's inventory \
               shows.",
        since: THIS_CLIENT,
        divergence: "CD2-interface-choice",
        evidence: Evidence::Private("AC2-EVID-HORIZON-GOLD"),
        station: "auberean-horizon::dat::things::the_money_is_the_games_gold_picture",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.horizon.the-parameter-bar-reads-each-vital-s-current-and-greatest",
        says: "Horizon's parameter bar shows the player's health of its greatest health and its \
               vigor of its greatest vigor, as the qualities the server sent give them.",
        since: THIS_CLIENT,
        divergence: "CD2-interface-choice",
        evidence: Evidence::Private("AC2-EVID-HORIZON-VITALS"),
        station: "auberean-horizon::dat::vitals::the_parameter_bar_reads_each_vitals_current_and_greatest",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.horizon.the-settings-open-on-their-key-and-set-the-orbit-camera",
        says: "Horizon's settings open on Horizon's key for them (K, from the key layers the shell \
               pushes while Horizon is shown) and close on Escape; a movement chosen there is \
               applied to the orbit camera and kept in the preferences file as `[Orbit] \
               Movement`.",
        since: THIS_CLIENT,
        divergence: "CD2-interface-choice",
        evidence: Evidence::Private("AC2-EVID-HORIZON-SETTINGS"),
        station: "auberean-horizon::dat::interfaces::horizons_settings_open_on_their_key_and_set_the_orbit_cameras_movement",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.horizon.the-shell-s-plugins-hear-the-game-and-horizon-draws-their-windows",
        says: "The desktop shell runs the plugins its product hands it as each session's frame \
               hook: a plugin hears the player enter the world; a line typed in Horizon that the \
               plugin claims is the plugin's and never said; what the plugin prints shows in \
               Horizon's log in the plugins' own color, not as an error; the window it shows is drawn by Horizon, and a press on its button \
               reaches the plugin.",
        since: THIS_CLIENT,
        divergence: "CD2-plugins",
        evidence: Evidence::Private("AC2-EVID-HORIZON-PLUGIN-SEAT"),
        station: "auberean-horizon::dat::extras::the_shells_plugins_hear_the_game_and_horizon_draws_their_windows",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.horizon.the-social-window-opens-on-its-key-and-asks-the-game",
        says: "Horizon's social window opens on Horizon's key for it (O); a fellowship asked for \
               there, its name typed, goes to the player's scripts, which send the server their \
               call for it; Escape closes the window.",
        since: THIS_CLIENT,
        divergence: "CD2-interface-choice",
        evidence: Evidence::Private("AC2-EVID-HORIZON-SOCIAL"),
        station: "auberean-horizon::dat::interfaces::horizons_social_window_opens_on_its_key_and_a_fellowship_asked_for_reaches_the_server",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.horizon.the-vitae-lamp-gives-the-game-s-words-and-examines-the-vitae",
        says: "Horizon's vitae lamp shows below full vitae only, giving the game's own words for \
               the vitae (\"Vitae: 95% XP to recover next 1 point: 1234.\", the vitae cut down to \
               a whole percent); a click on it examines the vitae in the examine window and lets \
               the selection go. At full vitae nothing of it is drawn and a click asks for nothing.",
        since: THIS_CLIENT,
        divergence: "CD2-interface-choice",
        evidence: Evidence::Private("AC2-EVID-HORIZON-VITAE-LAMP"),
        station: "auberean-horizon::cpu::play::status::the_vitae_lamp_gives_the_game_s_words_and_a_click_below_full_examines_the_vitae",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.input.a-click-that-brings-the-window-forward-is-not-heard",
        says: "A window coming active has the devices from its next frame: a button pressed \
               before then is not heard, nor is that press's release.",
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
        id: "desktop.input.command-with-v-c-or-x-pastes-copies-or-cuts-on-a-mac",
        says: "On a Mac, Command with V, C or X (the letter the layout puts on the key) pastes, \
               copies or cuts in a text entry as Control does elsewhere; Command with any other \
               key types nothing.",
        since: THIS_CLIENT,
        divergence: "CD2-mac-command-is-control-for-the-clipboard",
        evidence: Evidence::Private("AC2-EVID-MAC-COMMAND-CLIPBOARD"),
        station: "auberean-client::lib::keys::tests::command_with_v_c_or_x_pastes_copies_or_cuts_on_a_mac",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.input.ctrl-with-a-letter-types-its-control-character",
        says: "A key pressed with Control types the character the system makes of it with Control \
               applied, so Ctrl+V, Ctrl+C and Ctrl+X reach an entry as its paste, copy and cut, \
               and no letter is typed with them.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-INPUT-CTRL-CHARACTERS"),
        station: "auberean-client::lib::keys::tests::ctrl_with_a_letter_types_its_control_character",
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
        id: "desktop.interface.a-line-naming-no-command-shows-the-clients-error-line",
        says: "A line typed that names a command the player's scripts don't know shows the client's own \
               error line on the line of messages: type /help for a list of the commands.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-CHAT-UNKNOWN-COMMAND"),
        station: "auberean-interface::dat::screens::a_line_naming_no_command_shows_the_clients_error_line",
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
        id: "desktop.interface.each-key-map-preset-is-chosen-at-character-select",
        says: "The key map screen at character select starts on the keys the game's installer \
               laid down, X examining; each of its five presets, pressed and agreed to, binds its \
               three maps from the data at once, and Apply writes the one chosen to the player's \
               three files.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-KEYBOARD-PRESETS"),
        station: "auberean-interface::dat::screens::each_key_map_preset_is_chosen_at_character_select",
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
        id: "desktop.interface.the-chat-entry-steps-back-to-a-line-typed-through-the-client",
        says: "The up arrow in a chat line asks the client for the line typed before, and the client's \
               answer, a frame later, fills the entry.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-CHAT-HISTORY-ANSWER"),
        station: "auberean-interface::dat::screens::the_chat_entry_steps_back_to_a_line_typed_through_the_client",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.interface.the-connecting-screen-counts-the-login-attempts",
        says: "While nobody answers the login, the connecting screen counts each attempt out of \
               the twenty the client makes, two seconds apart.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-LOGIN-ATTEMPTS"),
        station: "auberean-interface::dat::screens::the_connecting_screen_counts_the_login_attempts",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.interface.the-connecting-screen-says-when-it-waits-for-the-server",
        says: "Once the login server has answered and been acknowledged, the connecting screen \
               says it is logging in until the server's first packet.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-LOGIN-WAITING"),
        station: "auberean-interface::dat::screens::the_connecting_screen_says_when_it_waits_for_the_server",
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
        says: "In the world the play screen runs on the frameworks: X, the installed keys' \
               examine, shows the examination window; the skills button shows the skills window \
               and its tooltip turns to Hide; F12 hides the toolbar and shows it again; the middle \
               button turns mouse look on and off, the camera following and the crosshair \
               showing; the up arrow still runs the body forward.",
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
        id: "desktop.interface.the-play-screens-settings-reach-the-client-as-its-actions",
        says: "What the play screen sets of the chat windows, the names over heads and the panels' places, and \
               what it asks of the command line, reaches the client as its actions, not as calls \
               into the player's scripts: an attack ceased, names shown, the commands made known, a \
               line typed, each chat window setting and the chat options kept, a list stepped \
               through, the panels' places kept in the order the play screen made them.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-UI-SETTINGS-ASKED"),
        station: "auberean-interface::dat::screens::the_play_screens_settings_reach_the_client_as_its_actions",
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
        id: "desktop.interface.the-selection-keys-select-what-actions-alone-select",
        says: "Our choice: the selection keys pressed through the retail interface select, press \
               by press, what the same selections asked through actions alone select on the \
               SDK's driver with no window, by one set of the game's rules.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SELECTION-KEYS-AS-ASKED"),
        station: "auberean-interface::dat::screens::the_selection_keys_select_what_actions_alone_select",
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
        id: "desktop.keys.a-fresh-start-has-the-installed-keys",
        says: "With no key map file a player starts with the keys the game's installer laid down: \
               the WoW preset's keys alone (X examines, Q and E strafe, R replies), the Defaults' \
               Shift map, and the WoW preset's Ctrl map less its developer keys and Ctrl+F6.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-KEYS-INSTALLED"),
        station: "auberean-shell::dat::keys::a_fresh_start_has_the_keys_the_installer_laid_down",
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
        id: "desktop.keys.an-interfaces-own-maps-are-written-to-its-files",
        says: "Our choice: an interface's own key maps as its bindings page applies them are \
               written to its own files (keys alone, with Shift, with Ctrl), which are its layers \
               from then on; the player's own key map files are left as they were, and the retail \
               interface writes none.",
        since: THIS_CLIENT,
        divergence: "CD2-horizon-key-maps",
        evidence: Evidence::Private("AC2-EVID-KEYS-INTERFACE-WRITTEN"),
        station: "auberean-shell::cpu::keys::an_interfaces_own_maps_are_written_to_its_files_and_read_back_as_its_layers",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.keys.an-interfaces-own-maps-stand-in-for-the-players",
        says: "Our choice: while an interface with key maps of its own is shown, its maps are looked up \
               first and stand in for the player's, with only the base map under them; with none, the \
               player's maps answer as before.",
        since: THIS_CLIENT,
        divergence: "CD2-horizon-key-maps",
        evidence: Evidence::Private("AC2-EVID-KEYS-INTERFACE-LAYERS"),
        station: "auberean-shell::cpu::keys::an_interfaces_own_maps_stand_in_for_the_players_over_the_base_map",
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
        id: "desktop.keys.horizons-key-files-read-back-as-written",
        says: "Our choice: Horizon's whole layout, written to its three key files as its Key \
               Bindings page applies it, reads back over the data as the same layout, binding for \
               binding, the game's actions and this client's own alike.",
        since: THIS_CLIENT,
        divergence: "CD2-horizon-key-maps",
        evidence: Evidence::Private("AC2-EVID-HORIZON-KEY-EXPORT"),
        station: "auberean-shell::dat::keys::horizons_layout_written_to_its_files_reads_back_as_it_is",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.keys.horizons-own-keys-are-in-use-only-while-it-is-shown",
        says: "Our choice: Horizon's own key maps are in use only while it is shown, the player's maps \
               standing aside meanwhile and back as they were after; the orbit camera it asked for \
               ends with the switch.",
        since: THIS_CLIENT,
        divergence: "CD2-horizon-key-maps",
        evidence: Evidence::Private("AC2-EVID-KEYS-HORIZON-SHOWN"),
        station: "auberean-shell::dat::interfaces::horizons_own_keys_are_in_use_only_while_it_is_shown",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.keys.maps-come-from-their-files-else-the-defaults",
        says: "At startup the player's key maps come from their files, the plain one read with \
               the update binding R to reply and written again when it gains it; a file not \
               reading leaves the data's Defaults map in its place, and a file not there the map \
               the game's installer laid down.",
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
        id: "desktop.keys.own-actions-are-held-by-the-world-controls",
        says: "Our choice: a key bound to one of this client's own actions (Horizon's look at the front) \
               is held by the world's controls while down and never sent to the client as a control; \
               letting it go or the window losing the keyboard ends it.",
        since: THIS_CLIENT,
        divergence: "CD2-horizon-key-maps",
        evidence: Evidence::Private("AC2-EVID-KEYS-OWN-HELD"),
        station: "auberean-shell::cpu::keys::an_own_action_is_held_by_the_world_controls_and_never_sent",
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
        says: "The particle systems an object's effects carry draw at the object's whole scale, \
               its look's times its own, not its look's alone: a gateway sign's haze stays a thin \
               rim at its foot.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-SYSTEM-WHOLE-SCALE"),
        station: "auberean-scene::corpus::combat::a_bodys_systems_draw_at_its_whole_scale",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "desktop.looks.a-look-sent-again-plays-its-effects-again",
        says: "A visual description the server sends again, unchanged, is applied as any other: \
               the object's parts stay, and its effects, its look's startup effects with them, \
               play again.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-LOOK-SENT-AGAIN"),
        station: "auberean-scene::cpu::others::a_look_sent_again_plays_its_effects_again",
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
        id: "desktop.net.the-game-socket-binds-as-the-preferences-say",
        says: "The game socket takes the first free port of a series worked out from its interface \
               unless the preferences' ComputeUniquePort is off; then the port the line's --outport \
               names, else UserSpecifiedPort. BindInterface, a network and mask, picks the interface; \
               one it does not match means any.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-NET-BINDING"),
        station: "auberean-shell::cpu::net_binding::the_game_socket_binds_as_the_preferences_say",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.orbit.a-plugin-choosing-the-orbit-camera-shows-the-world-through-it",
        says: "Our choice: a plugin that chooses the orbit camera has the world shown through it, \
               under an interface that asks for none, until the interface asks for another.",
        since: THIS_CLIENT,
        divergence: "CD2-orbit-camera",
        evidence: Evidence::Private("AC2-EVID-PLUGINS-CAMERA-SCENE"),
        station: "auberean-shell::dat::schemes::a_plugin_choosing_the_orbit_camera_shows_the_world_through_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.orbit.an-interface-shows-the-world-through-the-orbit-camera",
        says: "Our choice: an interface that asks for the orbit camera shows the world through it, mouse \
               look off: the camera stands behind the player within its distance, looking its way, and \
               turned to look west it stands east of the player; W then turns the body west and runs \
               it there in the window's scene.",
        since: THIS_CLIENT,
        divergence: "CD2-orbit-camera",
        evidence: Evidence::Private("AC2-EVID-ORBIT-SCENE"),
        station: "auberean-shell::dat::schemes::an_interface_asking_for_the_orbit_camera_shows_the_world_through_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.orbit.camera-based-movement-runs-the-way-the-controls-point",
        says: "Our choice: camera-based, the body runs forward the way the movement controls point \
               against where the orbit camera looks (backward turned round toward it, the turns and \
               strafes across its view, two together between), at the pace asked; with nothing asked \
               it stands as it faces, and the camera never turns it while the server moves it.",
        since: THIS_CLIENT,
        divergence: "CD2-orbit-camera",
        evidence: Evidence::Private("AC2-EVID-ORBIT-CAMERA-BASED"),
        station: "auberean-client-sim::cpu::orbit::camera_based_movement_runs_the_way_the_controls_point_against_the_camera",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.orbit.character-based-movement-is-the-games-own",
        says: "Our choice: character-based, the movement is the game's own unless the right button is \
               held, when the body faces where the camera looks and the turns step sideways; moving \
               or turning with no button held brings the camera round behind the player in a fifth of \
               a second, never past.",
        since: THIS_CLIENT,
        divergence: "CD2-orbit-camera",
        evidence: Evidence::Private("AC2-EVID-ORBIT-CHARACTER-BASED"),
        station: "auberean-client-sim::cpu::orbit::character_based_movement_is_the_games_own_and_brings_the_camera_behind",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.orbit.the-mouse-the-controls-and-the-wheel-move-the-orbit-camera",
        says: "Our choice: the mouse turns the orbit camera only while a mouse button is held (right \
               looks right, away from the player looks up, each the other way when reversed); the \
               camera's rotate and tilt controls turn and tilt it at their rate, the tilt held to its \
               range; the wheel brings it closer and further, between its nearest and farthest.",
        since: THIS_CLIENT,
        divergence: "CD2-orbit-camera",
        evidence: Evidence::Private("AC2-EVID-ORBIT-TURNING"),
        station: "auberean-client-sim::cpu::orbit::the_mouse_turns_it_while_a_button_is_held_and_the_controls_and_wheel_move_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.orbit.the-orbit-camera-moves-smoothly-over-the-land-and-a-blocks-edge",
        says: "Our choice: under the orbit camera, running over land that bends between the body's \
               ticks and over a block's edge, behind the player or turned by the mouse, the camera \
               moves on smoothly every frame, never further in a frame than the body moves in a \
               tick.",
        since: THIS_CLIENT,
        divergence: "CD2-orbit-camera",
        evidence: Evidence::Private("AC2-EVID-ORBIT-SMOOTH"),
        station: "auberean-shell::dat::schemes::the_orbit_camera_moves_smoothly_over_the_land_and_a_blocks_edge",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.orbit.the-orbit-camera-stands-back-along-the-way-it-looks",
        says: "Our choice: the orbit camera stands its distance back from a point most of the way up the \
               player's body, along the way it looks: level and facing north it is due south, tilted \
               down it stands above. Holding the look-at-front control turns the view to the \
               player's front and leaves the way the player moves as it was.",
        since: THIS_CLIENT,
        divergence: "CD2-orbit-camera",
        evidence: Evidence::Private("AC2-EVID-ORBIT-PLACE"),
        station: "auberean-client-sim::cpu::orbit::the_orbit_camera_stands_its_distance_back_along_the_way_it_looks",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.orbit.the-orbit-preferences-are-laid-over-what-the-interface-asks",
        says: "Our choice: the player's orbit preferences (the movement, the reversed directions), as \
               applied, are laid over what the interface asks of the orbit camera, which keeps its \
               place; applied character-based, W runs the body on its own way.",
        since: THIS_CLIENT,
        divergence: "CD2-orbit-camera",
        evidence: Evidence::Private("AC2-EVID-ORBIT-PREFERENCES"),
        station: "auberean-shell::dat::schemes::the_orbit_preferences_are_laid_over_what_the_interface_asks",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.orbit.with-no-window-forward-runs-the-way-each-movement-says",
        says: "Our choice: with no window, under the orbit camera turned a quarter left of the player, \
               forward camera-based turns the body to where the camera looks and runs it there; \
               character-based it runs the body on the way it faced.",
        since: THIS_CLIENT,
        divergence: "CD2-orbit-camera",
        evidence: Evidence::Private("AC2-EVID-ORBIT-WALK"),
        station: "auberean-shell::dat::schemes::with_no_window_forward_runs_where_the_camera_looks_only_camera_based",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.others.a-body-created-again-comes-back-into-the-world",
        says: "A body waiting out of the world in a cell not loaded, created again, stands where \
               its creation says at once, in the world when that cell is loaded and waiting in \
               that cell's list when it is not: a player already at the far end of a portal is \
               seen by one who follows without having to move.",
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
        id: "desktop.others.a-body-put-in-the-air-falls",
        says: "A body the server puts at once is placed: put in the air it touches nothing and falls, there or when it first stands in the world's table.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-OTHERS-PLACED"),
        station: "auberean-scene::cpu::others::a_body_put_in_the_air_falls",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.others.a-body-put-on-another-takes-a-free-spot",
        says: "A body the server puts at once where another stands takes a free spot nearby.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-OTHERS-PLACED-SLIDE"),
        station: "auberean-scene::cpu::others::a_body_put_on_another_takes_a_free_spot",
        tier: Tier::Cpu,
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
        id: "desktop.others.a-jump-sent-for-another-body-lifts-it",
        says: "A position carrying a jump lifts another body that has stood long enough, at the \
               jump impulse along the arc of a jump, keeping its heading; reports from the air \
               don't pull it off the arc.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-OTHERS-SENT-JUMP"),
        station: "auberean-scene::cpu::others::a_jump_sent_for_another_body_lifts_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "desktop.others.a-jump-too-soon-does-not-lift-another-body",
        says: "A position carrying a jump leaves a body with under half a second on the ground, \
               counting only ticks its movement ran, on the ground; it follows what comes next, \
               and the time that takes counts for the next jump.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-OTHERS-JUMP-TOO-SOON"),
        station: "auberean-scene::cpu::others::a_jump_too_soon_does_not_lift_another_body",
        tier: Tier::Cpu,
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
        says: "An object given a place in the world stands there and is drawn, in no container, \
               with or without word that it left the pack it was in: the server tells the player \
               who put an item down only its place.",
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
        says: "A jump another body's animation reaches wakes that body and lifts it, with no \
               position sent; its hooks are still heard.",
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
        id: "desktop.others.an-object-sent-into-a-room-it-stands-outside-stands-outdoors",
        says: "An object sent into an interior cell seen from outside, standing in none of the \
               cells seen from it, stands in the land cell its place falls in, buildings and \
               other objects with no spheres too; sent before that cell is loaded, it moves there \
               once it is. One standing in the cell stays in it, and so does one outside a cell \
               not seen from outside.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-OTHERS-PUT-CELL"),
        station: "auberean-scene::cpu::others::an_object_sent_into_a_room_it_stands_outside_stands_outdoors",
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
        id: "desktop.preferences.graphics-at-startup-are-the-settings",
        says: "The graphics preferences the client asks at startup are named in the data as the \
               settings name them (Graphics, OverallGraphicsQuality and LandscapeTerrainBlending), so \
               both read one value from the file, with the line's -lb and -nolb over it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-PREFERENCES-STARTUP"),
        station: "auberean-shell::dat::preferences::graphics_at_startup_are_the_settings",
        tier: Tier::Dat,
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
        id: "desktop.settings.the-device-is-made-again-as-the-client-shows-the-game-another-way",
        says: "Showing the game another way makes the device again (its stages start over): a new \
               size, full screen or not, or the edges smoothed or not; in full screen also another \
               way of buffering or of waiting for the refresh.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-DEVICE-RESTART"),
        station: "auberean-shell::cpu::settings::the_device_is_made_again_as_the_client_shows_the_game_another_way",
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
        id: "desktop.shell.a-refused-interface-keeps-the-one-shown-and-says-why",
        says: "An interface that can't be shown is refused when it is chosen: the interface shown \
               stays, the interface preference is put back to it (the client page shows it, and \
               the preferences file keeps it), and the player is told why.",
        since: THIS_CLIENT,
        divergence: "CD2-interface-choice",
        evidence: Evidence::Private("AC2-EVID-UI-INTERFACE-REFUSED"),
        station: "auberean-shell::dat::interfaces::a_refused_interface_keeps_the_one_shown_and_says_why",
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
        id: "desktop.shell.an-arrival-is-reported-once-portal-space-is-over",
        says: "After a trip the scene is reported once portal space has gone out entirely, not \
               while the world shows through it as it goes.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCENE-AFTER-PORTAL"),
        station: "auberean-shell::dat::session::an_arrival_is_reported_once_portal_space_is_over",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.shell.an-arrival-under-a-story-is-reported-after-it",
        says: "A player sent somewhere while a story hides the world is reported arrived only \
               once the story is over and a frame of the world has been drawn.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCENE-AFTER-STORY"),
        station: "auberean-shell::dat::session::an_arrival_while_a_story_hides_the_world_is_reported_once_it_shows",
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
        id: "desktop.shell.portal-space-readies-the-world-before-coming-out",
        says: "Going out of portal space, the client readies on the device what the world's first \
               frame puts there, a share a frame while the world is still hidden: the pictures it \
               is drawn into first, then its pictures a few a frame, then its draws and the land's \
               pieces, all of them before coming out begins but those of the effects the player's \
               arrival sets going as the world is first drawn.",
        since: THIS_CLIENT,
        divergence: "CD2-portal-space-fades-out-after-the-world-s-first-frame",
        evidence: Evidence::Private("AC2-EVID-PORTAL-WARMUP"),
        station: "auberean-shell::dat::session::portal_space_readies_the_world_before_coming_out",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.shell.switching-the-interface-leaves-one-then-enters-the-other",
        says: "A choice of interface is followed at the start of the next frame: the interface shown \
               is left, the world's controls let go of what they held and mouse look ends, then the \
               interface chosen is entered and shown from then on. Each interface is built the first \
               time it is chosen and kept.",
        since: THIS_CLIENT,
        divergence: "CD2-interface-choice",
        evidence: Evidence::Private("AC2-EVID-UI-INTERFACE-SWITCH"),
        station: "auberean-shell::dat::interfaces::switching_the_interface_leaves_one_then_enters_the_other",
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
        says: "How the game is shown goes to the host and into the settings as the client's \
               rendering preferences say: a flip of full screen goes through the full-screen \
               preference, and what the preferences then say is carried out.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-OPTIONS-RENDERING-FLIP-HOST"),
        station: "auberean-shell::dat::session::the_host_shows_the_game_as_the_interface_asks",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.shell.the-interface-chosen-is-kept-in-the-preferences-file",
        says: "The interface chosen is kept in the preferences file as `[UI] Interface`, `Retail` or \
               `Horizon`, and the next run starts in it; a run whose file names an interface that \
               can't be shown starts in the retail one, and the player is told why.",
        since: THIS_CLIENT,
        divergence: "CD2-interface-choice",
        evidence: Evidence::Private("AC2-EVID-UI-INTERFACE-KEPT"),
        station: "auberean-shell::dat::interfaces::the_interface_chosen_is_kept_in_the_preferences_file",
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
        says: "The body leaving the ground plays the jump effect's sound, and coming down plays \
               the landing effect's.",
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
        id: "desktop.text.a-nested-reference-the-files-lack-is-asked-for",
        says: "A string reference holding a nested reference to a table the files lack (a quest's \
               description and journal entry in the quest journal's text) starts a string download \
               for that table and waits, though the files write the outer reference.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-STRDL-NESTED"),
        station: "auberean-interface::dat::text::a_reference_holding_a_nested_reference_the_files_lack_asks_for_its_table",
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
        says: "A time of day moved by an admin lights the ground on the next frame, and the \
               light's tick is shortened by the rate the world's time runs at.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DESKTOP-MOVED-CLOCK-RELIGHTS"),
        station: "auberean-shell::dat::session::a_moved_game_clock_relights_the_ground_at_once_and_a_faster_one_more_often",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "desktop.world.a-player-in-deep-water-rises-and-swims",
        says: "A player on the bottom of deep water is lifted until its water line is near the \
               surface, and the forward key moves it at its motion description's water speed.",
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
        says: "A press finds a body by its box only when a corner of the box stands on the screen \
               and the box's square of clip space meets the pointer's: the pixel up and to the \
               left of it to that one.",
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
        says: "On a rainy day the rain falls about the camera as its day says, and none on a dry \
               moment.",
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
        says: "The day's wind blows on the world's missiles: the day's own while it rains, and \
               one that wanders with the time of day on a dry moment.",
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
