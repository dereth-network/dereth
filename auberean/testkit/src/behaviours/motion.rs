//! Motion: how a body moves under the player's asking, gravity and the world, and how the chase
//! camera follows it.
//!
//! A row claims what the September 2013 client does when it moves a body.

use super::{Behaviour, Evidence, Tier, RETAIL, TOOLING};

/// This subject's rows, in id order.
pub static ROWS: &[Behaviour] = &[
    behaviour! {
        id: "motion.camera.a-line-meets-what-stands-in-the-next-landblock",
        says: "A line from a cell near a landblock's edge meets what stands just across the edge \
               in the next landblock, the landblocks of this world being 160 m long.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-CAMERA-BLOCK"),
        station: "auberean-physics::cpu::motion::camera::a_line_meets_what_stands_in_the_next_landblock",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "motion.camera.inside-doorways-do-not-stop-the-camera",
        says: "Inside, a cell's doorways do not stop the chase camera's lines: only the walls, \
               floors and ceilings the cells' collision trees hold do, so beside a doorway the \
               camera stands where it wants to, 4.5 m from the top of the body, in the room beyond.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MOTION-CAMERA-DOORWAYS"),
        station: "auberean-physics::dat::interior::inside_doorways_do_not_stop_the_camera",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "motion.camera.inside-the-camera-stops-at-the-walls",
        says: "Inside, the chase camera's lines stop at the walls, floors and ceilings of the \
               body's cell and the cells it sees, and the camera stands 0.2 m out from the surface \
               its second line stopped at.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MOTION-CAMERA-INSIDE"),
        station: "auberean-physics::dat::interior::inside_the_camera_stops_at_the_walls",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "motion.camera.inside-the-first-line-passes-what-stands-in-the-cells",
        says: "Inside, the chase camera's first line, from the body up to the top of it, stops \
               only at the cells' walls, floors and ceilings, not at what stands in the cells; the \
               second line stops at both.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MOTION-CAMERA-RAYS"),
        station: "auberean-physics::dat::interior::inside_the_first_line_passes_what_stands_in_the_cells",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "motion.camera.solid-objects-stop-the-second-line",
        says: "The camera's second line, from the shoulder to where the camera wants to be, stops \
               at solid objects standing in the world, but not at the player's own body, and \
               outdoors the camera stands where it stopped, in the land cell there; in the map view \
               only the surroundings stop it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MOTION-CAMERA-OBJECTS"),
        station: "auberean-physics::cpu::motion::camera::solid_objects_stop_the_second_line",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "motion.camera.the-camera-starts-behind-and-above",
        says: "At rest the chase camera looks at the top of the body from 4.5 m behind it, pitched \
               0.25 rad above level, with a 75 degree vertical view; the body's height scale \
               scales both the distance and the look-at height.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MOTION-CAMERA"),
        station: "auberean-physics::cpu::motion::camera::the_camera_starts_behind_and_above",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "motion.camera.the-camera-stops-at-the-ground",
        says: "When the place the camera wants is under the ground, it stops where the line from \
               its look-at point meets the ground.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MOTION-CAMERA-TRACE"),
        station: "auberean-physics::cpu::motion::camera::the_camera_stops_at_the_ground",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "motion.collision.a-body-knows-the-ground-and-water-it-touches",
        says: "What a body stands on names a physics material of the world's: the ground's \
               by the vertex nearest it, a road's on a road, and a lake's block names its \
               water's.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MOTION-GROUND-MATERIAL"),
        station: "auberean-physics::dat::walk::a_body_knows_the_ground_and_water_it_touches",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "motion.collision.a-body-stops-at-the-edge-of-a-block-it-may-not-walk-on",
        says: "A body running at a block the world's map shuts is turned back at the edge of the \
               land cell it would enter, told it collided along that axis, and never reaches the \
               block.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MOTION-BLOCK-PASSABILITY"),
        station: "auberean-physics::cpu::contact::collision::a_body_stops_at_the_edge_of_a_block_it_may_not_walk_on",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "motion.corpus.a-body-driven-by-recorded-reports-indoors-reaches-the-next-report",
        says: "A body put at a report of a recorded steady run in the rooms of an interior and \
               moved as it asks, at its full speed, reaches the next report over the rooms' floors, \
               steps and ramps after a whole number of ticks: within 3 mm for four pairs in five, \
               within a tick's travel for 99 in 100.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MOTION-REPLAY-INDOOR"),
        station: "auberean-physics::corpus::replay::a_body_driven_by_recorded_reports_indoors_reaches_the_next_report",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "motion.corpus.a-body-driven-by-recorded-reports-reaches-the-next-report",
        says: "A body put at a report of a recorded steady run outdoors and moved as it asks, at \
               its full speed (the motion values' times the speed scale the server last sent the \
               player, in whichever session of the client's run), reaches the next report after a \
               whole number of ticks: within 3 mm for three pairs in four, within a tick's travel \
               for 99 in 100.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MOTION-REPLAY"),
        station: "auberean-physics::corpus::replay::a_body_driven_by_recorded_reports_reaches_the_next_report",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "motion.corpus.no-recorded-player-stands-on-a-block-the-map-shuts",
        says: "Every 2017 report from the land, over 1.4 million on over 3,000 blocks, is on a \
               block the world's passability map lets a body walk on, though the map shuts most \
               of the world.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MOTION-MAP-CORPUS"),
        station: "auberean-physics::corpus::replay::no_recorded_player_stands_on_a_block_the_map_shuts",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "motion.corpus.recorded-players-fall-by-a-rooms-own-gravity",
        says: "Falling in the rooms whose own properties make gravity 3 m/s2, the 2017 sessions' players fall at 3 m/s2: \
               the middle half of over a thousand accelerations lies within 0.5 of it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MOTION-ROOM-GRAVITY-CORPUS"),
        station: "auberean-physics::corpus::replay::recorded_players_fall_by_a_rooms_own_gravity",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "motion.drag.a-body-on-dragging-ground-slows-by-its-drag",
        says: "On ground whose motion values have drag, a body nothing pushes keeps one less the \
               drag of its velocity each second, decayed tick by tick.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-DRAG"),
        station: "auberean-physics::cpu::motion::steering::a_body_on_dragging_ground_slows_by_its_drag",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "motion.draw.a-body-drawn-between-ticks-keeps-its-whole-rotation",
        says: "A body drawn between ticks keeps its frame's whole rotation: one pitched along its path, as a missile is, stays pitched.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAWN-WHOLE-ROTATION"),
        station: "auberean-physics::cpu::motion::moving::a_body_drawn_between_ticks_keeps_its_whole_rotation",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "motion.draw.a-body-is-drawn-between-its-last-two-ticks",
        says: "Between physics ticks a moving body is drawn between where it was before its last \
               tick and where it is, by how far the frame is past that tick (its heading turned \
               the short way), so it is drawn up to a tick behind; the chase camera follows the \
               body as drawn.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MOTION-DRAW-BETWEEN-TICKS"),
        station: "auberean-physics::cpu::motion::moving::a_body_is_drawn_between_its_last_two_ticks",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "motion.element.a-body-takes-what-its-qualities-say-of-it",
        says: "A body made for an object takes what the object's qualities say of it: it reports \
               collisions, ignores them and falls unless they say it does not; it is lit, \
               inelastic and slides along edges only where they say it is; it is placed in the \
               pose and at the scale they name (pose 0, scale 1 when they name none). Scaled \
               unevenly, a body with a physics tree is marked so.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-PROFILE"),
        station: "auberean-physics::cpu::contact::objects::a_body_takes_what_its_qualities_say_of_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "motion.element.a-body-that-is-not-mobile-stays",
        says: "A new body is mobile, edge sliding, lit, under gravity and reporting its \
               collisions with all its updates running; a body that is not mobile stays where it \
               is and its movement stops.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-STATE"),
        station: "auberean-physics::cpu::motion::element::a_body_that_is_not_mobile_stays",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "motion.element.a-body-with-a-figure-is-in-the-element-its-figure-stands-in",
        says: "A body with a figure stays in its old element through the step that takes the figure \
               to the new one, while the element it is going to leads.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-ELEMENT-FIGURE"),
        station: "auberean-physics::cpu::motion::element::a_body_with_a_figure_is_in_the_element_its_figure_stands_in",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "motion.element.a-bodys-height-is-above-what-is-under-it",
        says: "A body's height is above what is under it, not above the land: a sight line straight down through cells and objects, and with nothing below, up, as a negative height.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-HEIGHT-ABOVE-WALKABLE"),
        station: "auberean-physics::cpu::motion::moving::a_bodys_height_is_above_what_is_under_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "motion.element.a-fall-toward-deep-water-asks-for-the-air-mode",
        says: "A body falling faster than 2 m/s toward water nearer than the ground and more than \
               2 m below its water line asks its animation for the air's mode (unless swimming is \
               already wanted); the change runs its animation, and a frozen animation asks \
               nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-DOMODE"),
        station: "auberean-physics::cpu::motion::element::a_fall_toward_deep_water_asks_for_the_air_mode",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "motion.element.a-new-element-goes-to-the-animation",
        says: "A body that goes into another element asks its animation to go there too, once: \
               jumping, into the air; landing, back onto the earth.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-ELEMENT-ANIMATION"),
        station: "auberean-physics::cpu::motion::element::a_new_element_goes_to_the_animation",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "motion.element.a-placement-finishes-a-change-of-mode",
        says: "A placement asks that a change of mode under way finish at once.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PLACEMENT-SNAPS-MODE"),
        station: "auberean-physics::cpu::motion::moving::a_placement_finishes_a_change_of_mode",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "motion.element.a-player-on-a-deep-bed-floats-up-and-the-camera-with-it",
        says: "A player put on the bed of deep water rises and floats with its water line at the \
               surface, and the camera looking up from under it stands where it wants.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CLIENT-FLOATS-IN-DEEP-WATER"),
        station: "auberean-physics::dat::camera::a_player_on_a_deep_bed_floats_up_and_the_camera_looks_up_from_under_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "motion.element.a-position-too-far-to-follow-is-placed",
        says: "A position more than 15 m from a body is placed by its next step, not only set: in the air it touches nothing and falls.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-QUICK-MOVES-PLACED"),
        station: "auberean-physics::cpu::motion::moving::a_position_too_far_to_follow_is_placed",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "motion.element.a-sent-hover-height-is-ignored",
        says: "A hover height set through the body changes nothing: the motion interpreter's \
               setter does nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-HOVER-IGNORED"),
        station: "auberean-physics::cpu::motion::element::a_sent_hover_height_is_ignored",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "motion.element.pushers-carry-and-lift",
        says: "A pusher's velocity carries a body that far over the tick; a pusher lifting it \
               takes it off the ground and out of gravity until the lift stops.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-PUSHERS"),
        station: "auberean-physics::cpu::motion::element::pushers_carry_and_lift",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "motion.element.the-animation-speed-takes-the-acceleration-over-each-step",
        says: "Between ticks the speed a body's figure is animated at (how far it moved over its \
               last tick, ten times the move in its frame before it) takes half the body's \
               acceleration times each animation step squared, in its frame; not while its \
               animation is frozen.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-ANIM-ACCEL"),
        station: "auberean-physics::cpu::motion::element::the_animation_speed_takes_the_acceleration_over_each_step",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "motion.element.the-ground-under-a-body-is-its-height-above-walkable-below",
        says: "The place on the ground under a body is its own place lowered by its height above \
               what is walkable under it; a body out of the world answers its own place.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-GROUND-POSITION"),
        station: "auberean-physics::cpu::motion::moving::the_ground_under_a_body_is_its_height_above_walkable_below",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "motion.element.water-decides-swimming-and-the-sea-floor",
        says: "Out of water a body touching the ground is on the earth, else in the air; in water \
               over the line three quarters up it swims; with that line above the surface and the \
               line a tenth up below it, standing, it walks the sea floor.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-ELEMENT"),
        station: "auberean-physics::cpu::motion::element::water_decides_swimming_and_the_sea_floor",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "motion.element.water-lifts-by-depth",
        says: "In water a body is lifted by its whole lift more than a meter under the surface \
               and by its lift times its depth within a meter, against gravity.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-LIFT"),
        station: "auberean-physics::cpu::motion::element::water_lifts_by_depth",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "motion.hold.a-body-sticks-to-the-bone-nearest-it",
        says: "A body sticks to another where it stands: held by the holder's bone nearest it, at \
               its turn as seen from the holder (the place dropped), and out of the world; a \
               holder with no bones cannot hold it so.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-STICKY-PARENT"),
        station: "auberean-physics::cpu::motion::moving::holding::a_body_sticks_to_the_bone_nearest_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "motion.hold.a-child-hangs-where-its-holder-is-drawn",
        says: "A held child is placed each time its holder is drawn: at the holder's drawn place \
               and cell, the hanging point's place scaled by the holder's scale and turned with it \
               (the point's own turn too, unless the child drops it), then the child's own offset; \
               with no hanging point it hangs at the holder's own frame.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-HOLD-PLACE"),
        station: "auberean-physics::cpu::motion::moving::holding::a_child_hangs_where_its_holder_is_drawn",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "motion.hold.a-child-let-go-of-tells-its-holder",
        says: "A child let go of tells its holder, which takes it off its list (the last child \
               taking its place), and goes back to the game's default mode; the server's word that \
               it is let go of counts only with a newer position stamp, and then it also leaves \
               the world.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-HOLD-LET-GO"),
        station: "auberean-physics::cpu::motion::moving::holding::a_child_let_go_of_tells_its_holder",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "motion.hold.a-held-child-fades-in-water",
        says: "When a holder's element has changed since it was last drawn, a child in its first \
               holding location, or hung at its second whatever from, fades out over half a second \
               while the holder is headed into water and back in otherwise; a temporary child \
               never does.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-HOLD-FADE"),
        station: "auberean-physics::cpu::motion::moving::holding::a_held_child_fades_in_water",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "motion.hold.a-held-childs-scale-takes-its-holders",
        says: "A held object's whole scale takes its holder's: its look's scale times its own, \
               times, while held and not flying as a missile, the holder's whole scale and the \
               scale the holder's look gives what it holds.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-TOTAL-SCALE"),
        station: "auberean-physics::cpu::motion::moving::holding::a_held_childs_scale_takes_its_holders",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "motion.hold.a-holder-takes-up-a-child-which-leaves-the-world",
        says: "A holder takes up a child at its own frame, or at a holding location its setup has, \
               never itself or its own holder; the child lets go of any holder it had, leaves the \
               world, goes to the holder's mode, fades in over half a second unless it is \
               temporary, and knows its holder.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-HOLD-TAKE-UP"),
        station: "auberean-physics::cpu::motion::moving::holding::a_holder_takes_up_a_child_which_leaves_the_world",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "motion.impulse.a-local-velocity-is-not-turned",
        says: "A velocity set in the body's own frame is taken as the landblock-frame velocity as \
               it is, not turned by the body's heading.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-LOCAL-VELOCITY"),
        station: "auberean-physics::cpu::motion::element::a_local_velocity_is_not_turned",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "motion.impulse.a-targeted-impulse-pushes-away",
        says: "An impulse away from another object pushes the body straight away from it at the \
               given speed plus the upward part, off the ground and into the air; asked to, the \
               push is divided by the distance once more.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-IMPULSE"),
        station: "auberean-physics::cpu::motion::element::a_targeted_impulse_pushes_away",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "motion.interior.a-body-falls-by-the-gravity-where-it-is",
        says: "A body falls by the gravity where it is: a room's own properties may set it (3 m/s2 in some, 1 m/s2 \
               upward in others), and the region lays it over the land, block by block (9.8039 m/s2 nearly \
               everywhere, 0.157 in the low blocks).",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MOTION-MATERIAL-GRAVITY"),
        station: "auberean-physics::dat::interior::a_body_falls_by_the_gravity_where_it_is",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "motion.interior.a-body-moved-by-hand-meets-the-walls",
        says: "A body a host moves by hand, a step at a time with no motion of its own, goes \
               through the transition as a tick's move does: it keeps to the floor, passes \
               through doorways into other cells and comes to rest against a wall.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-MOTION-BY-HAND"),
        station: "auberean-physics::dat::interior::a_body_moved_by_hand_meets_the_walls",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "motion.interior.a-body-stands-and-runs-in-a-room-with-no-doorways",
        says: "A body stands in a cell with no doorways and runs on its floor as retail's players \
               did: the training ground's second area, from where the menhir portal puts a player \
               up the room's length, on the heights a 2017 session reported.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MOTION-CLOSED-ROOM"),
        station: "auberean-physics::dat::interior::a_body_stands_and_runs_in_a_room_with_no_doorways",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "motion.interior.a-body-stands-and-runs-on-an-interior-floor",
        says: "Dropped in a cell of the human training hall, the player's body lands on its floor; \
               running, it passes from cell to cell through the doorways, never falls through a \
               floor, and comes to rest against a wall or something solid standing in a cell.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MOTION-INTERIOR"),
        station: "auberean-physics::dat::interior::a_body_stands_and_runs_on_an_interior_floor",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "motion.interior.a-body-steps-onto-what-stands-in-the-cells",
        says: "Inside, the solid parts of what stands in the cells collide with the body as the \
               walls do: running from the human training hall's starting place onto the platform \
               of steps before it, the body climbs the steps and crosses the top.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MOTION-INTERIOR-SOLIDS"),
        station: "auberean-physics::dat::interior::a_body_steps_onto_what_stands_in_the_cells",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "motion.interior.a-bodys-height-is-above-what-the-world-placed",
        says: "A body's height is above what the world placed under it: over a building's deck and over a pedestal in a room, tick by tick through a jump, it is what the client measures.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-HEIGHT-OVER-PLACED"),
        station: "auberean-physics::dat::interior::a_bodys_height_is_above_what_the_world_placed",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "motion.interior.a-cell-with-no-collision-tree-is-open-air",
        says: "A cell whose mesh has no collision tree is open air, not a missing cell: the great \
               caverns' 13 cells are all built with their doorways, and a body in one falls \
               through the air and on through the doorway in its floor.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MOTION-OPEN-AIR-CELL"),
        station: "auberean-physics::dat::interior::a_cell_with_no_collision_tree_is_open_air",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "motion.interior.a-room-names-the-material-of-its-floor",
        says: "A body on an interior floor touches the physics material the cell's own properties                name; a cell that names none gives none.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CELL-FLOOR-MATERIAL"),
        station: "auberean-physics::dat::walk::a_room_names_the_material_of_its_floor",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "motion.interior.at-a-rooms-edge-a-bodys-height-meets-what-is-placed",
        says: "A line straight down at the edge of a room meets what is placed there, so a body over a pedestal's rim has a height.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-HEIGHT-ROOM-EDGE"),
        station: "auberean-physics::dat::interior::at_a_rooms_edge_a_bodys_height_meets_what_is_placed",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "motion.interior.in-a-room-a-bodys-height-is-above-the-floor",
        says: "In a room a body in the air is as high as the room's floor is below it, wherever the land is.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-HEIGHT-ABOVE-WALKABLE-ROOM"),
        station: "auberean-physics::dat::interior::in_a_room_a_bodys_height_is_above_the_floor",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "motion.interp.a-body-far-behind-is-put-at-the-end",
        says: "A body more than 15 m from the last position the server sent it (or making no \
               headway along them for two seconds) is put at that position at once, moving on as \
               it asked.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-INTERP-BLIP"),
        station: "auberean-physics::cpu::motion::steering::a_body_far_behind_is_put_at_the_end",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "motion.interp.a-body-follows-the-positions-it-is-sent",
        says: "A body given the positions the server sends walks along them at its own speed, \
               steering for a point one tick's full-speed travel past the nearest point of their \
               path, never back along it, and comes to rest on the last.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-INTERP"),
        station: "auberean-physics::cpu::motion::steering::a_body_follows_the_positions_it_is_sent",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "motion.interp.a-sent-position-far-away-is-taken-at-once",
        says: "A position the server sends within 15 m of a body is moved toward over the ticks; \
               one farther away is taken at once.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-INTERP-FAR"),
        station: "auberean-physics::cpu::motion::element::a_sent_position_far_away_is_taken_at_once",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "motion.jump.a-jump-follows-its-arc-and-waits-to-repeat",
        says: "A jump leaves the ground at 6.26 m/s and follows 6.26 t - 4.9 t squared at each \
               tick until it lands; the body cannot jump again until it has stood on the ground \
               half a second.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MOTION-JUMP"),
        station: "auberean-physics::cpu::motion::moving::a_jump_follows_its_arc_and_waits_to_repeat",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "motion.jump.a-jump-stops-the-behaviors-a-move-cancels",
        says: "A jump stops the behaviors a move cancels; a jump the body may not make stops                nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MOTION-JUMP-CANCELS"),
        station: "auberean-physics::cpu::motion::moving::a_jump_stops_the_behaviors_a_move_cancels",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "motion.jump.a-slow-player-leads-in-before-jumping",
        says: "Asked to jump, the player's body moving under 2 m/s along the ground plays the \
               jump's lead-in and stays down until it ends; faster, or anyone else's body, jumps \
               at once.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-JUMP-LEADIN"),
        station: "auberean-physics::cpu::motion::element::a_slow_player_leads_in_before_jumping",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "motion.moveto.a-body-turns-then-walks-to-the-place-and-stops",
        says: "A body the server sends to a place walks only while facing within a quarter turn of \
               it, turning first otherwise; it stops within the desired distance, and the move \
               stays open until the server ends it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-MOVETO"),
        station: "auberean-physics::cpu::motion::steering::a_body_turns_then_walks_to_the_place_and_stops",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "motion.moveto.a-turn-stops-on-its-heading",
        says: "A turn to a heading slows to nothing on the heading and never turns past it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-TURNTO"),
        station: "auberean-physics::cpu::motion::steering::a_turn_stops_on_its_heading",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "motion.moveto.only-the-player-takes-an-ordered-move",
        says: "A move the server orders is taken only by the player's body, which keeps the \
               order's stamp, stops the behaviors a move cancels and sets off toward the place; \
               anyone else's body stays.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-MOVETO-ORDER"),
        station: "auberean-physics::cpu::motion::element::only_the_player_takes_an_ordered_move",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "motion.placing.a-body-is-placed-at-its-whole-scale",
        says: "A body is placed and moved by its spheres at its whole scale along z: its look's \
               scale times its own.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-WHOLE-SCALE"),
        station: "auberean-physics::cpu::contact::objects::a_body_is_placed_at_its_whole_scale",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "motion.placing.a-body-of-no-spheres-is-placed-as-a-small-sphere",
        says: "A body with no collision spheres of its own is placed and moved as one sphere of \
               radius 0.1 resting on its origin, unscaled.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-NO-SPHERES"),
        station: "auberean-physics::cpu::contact::objects::a_body_of_no_spheres_is_placed_as_a_small_sphere",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "motion.placing.a-body-put-on-open-ground-lands-there",
        says: "A body put on open ground is put there through a placement, the place kept as its \
               placement when the order asks; a place that names no kind of cell has no cell.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-SET-POSITION"),
        station: "auberean-physics::cpu::contact::objects::a_body_put_on_open_ground_lands_there",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "motion.placing.a-missile-put-in-the-world-is-let-go-of",
        says: "A missile held and then put in the world first lets go of its holder, which takes \
               it off its list of what it holds.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-MISSILE-LET-GO"),
        station: "auberean-physics::cpu::contact::objects::a_missile_put_in_the_world_is_let_go_of",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "motion.placing.a-taken-place-is-refused-forced-or-scattered",
        says: "A place another object takes is refused (no valid position, or collided when the \
               body kept a touch); forced, the body goes there anyway; told to scatter when \
               blocked, it takes the first place around it a roll gives that is free.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-SET-POSITION-TAKEN"),
        station: "auberean-physics::cpu::contact::objects::a_taken_place_is_refused_forced_or_scattered",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "motion.rules.this-worlds-transition-settings",
        says: "Ground stops being walkable at 45.5 degrees; a move's sub-steps are nine tenths of \
               the body's sphere radius; landblocks are 160 m; restored ground keeps a body's \
               velocity; each cell is asked once an attempt.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MOTION-RULES"),
        station: "auberean-physics::cpu::motion::rules::this_worlds_transition_settings",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "motion.run.a-body-runs-and-jumps-at-its-own-scale-not-its-looks",
        says: "A body's speeds and its jump take its own scale along each axis (the one its \
               qualities give), not the scale its look is drawn at: a character drawn smaller runs \
               and jumps as fast as any other.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SPEED-OWN-SCALE"),
        station: "auberean-physics::cpu::motion::element::a_body_runs_and_jumps_at_its_own_scale_not_its_looks",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "motion.run.a-run-starts-and-stops-tick-by-tick",
        says: "A body starting to run moves nothing on the first tick, then 0.225, 0.375, 0.525 \
               and 0.625 m, at full speed on the fifth; let go, it stops in three ticks covering \
               0.525, 0.275 and 0.075 m.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MOTION-RUN"),
        station: "auberean-physics::cpu::motion::moving::a_run_starts_and_stops_tick_by_tick",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "motion.run.backward-and-sideways-speeds",
        says: "Backing up runs at 4 m/s and sidestepping at 4.5.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MOTION-SPEEDS"),
        station: "auberean-physics::cpu::motion::moving::backward_and_sideways_speeds",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "motion.run.diagonals-are-not-faster",
        says: "Forward with a sidestep is no faster than forward: past 0.7 forward the sideways \
               part runs at the forward speed and the whole is cut to the forward speed.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MOTION-DIAGONAL"),
        station: "auberean-physics::cpu::motion::moving::diagonals_are_not_faster",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "motion.sound.what-silences-a-bodys-sounds",
        says: "The sounds a body makes are silent while a static body's cell is among the free \
               blocks; while a moving body is out of the world with no parent, or more than 96 \
               meters from the player; while the parent of a held body is silent; and while any \
               other body is out of both the world and the workspace.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MOTION-SOUND-SILENT"),
        station: "auberean-physics::cpu::motion::sound::a_moving_body_is_silent_out_of_the_world_or_out_of_range",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "motion.stamps.a-newer-stamp-is-kept-and-an-older-one-refused",
        says: "A body keeps one stamp per kind of update the server sends it; an update is newer \
               when its stamp is ahead of the kept one by less than half the range (stamps count \
               round through 65536, so one exactly half the range or more behind counts as \
               ahead), and a newer stamp is kept; an equal or older one is refused.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-STAMPS"),
        station: "auberean-physics::cpu::motion::moving::a_newer_stamp_is_kept_and_an_older_one_refused",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "motion.target.a-followed-body-tells-its-follower-where-it-is",
        says: "A body that follows another asks it to watch for the follower; the body followed \
               tells the follower at once where it is (its place, not yet projected ahead), how \
               it moves and its radius, height and scale; the follower keeps that under the \
               context it followed by, and letting go asks the other to stop.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-TARGET"),
        station: "auberean-physics::cpu::motion::moving::a_followed_body_tells_its_follower_where_it_is",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "motion.target.a-teleport-tells-watchers-and-a-newer-leave-world-takes-a-body-out",
        says: "A teleport beginning stops a body following sent positions and unconstrains it, \
               drops its target, tells its watchers it teleported (status 3) and moves its \
               teleport stamp on; the server's leave-world with a newer position stamp makes a \
               body that is not the player's let go of its holder and leave the world, which ends \
               its tether, and an older stamp does nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-TELEPORT-LEAVE"),
        station: "auberean-physics::cpu::motion::moving::a_teleport_tells_watchers_and_a_newer_leave_world_takes_a_body_out",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "motion.tick.animation-steps-by-the-frames-time",
        says: "Animation steps once a frame at least a thirtieth of a second after the last, by \
               all the time since up to a fifth of a second; after a longer gap it steps a \
               twentieth of a second only.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-ANIM-CLOCK"),
        station: "auberean-physics::cpu::motion::moving::animation_steps_by_the_frames_time",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "motion.tick.physics-runs-in-tenths-of-a-second",
        says: "Physics ticks in whole tenths of a second as frame time passes; a backlog over a \
               second is dropped rather than run.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MOTION-TICK"),
        station: "auberean-physics::cpu::motion::moving::physics_runs_in_tenths_of_a_second",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "motion.turn.a-headings-rotation-comes-through-a-move-unchanged",
        says: "The rotation a heading gives a body is one a move leaves bit for bit, so a body \
               pressed against something knows it did not move.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-ROTATION-KEPT"),
        station: "auberean-physics::cpu::motion::steering::a_headings_rotation_comes_through_a_move_unchanged",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "motion.turn.a-motion-with-a-heading-moves-and-turns-to-it",
        says: "A motion asked of a body with a heading wakes its movement, asks the motion of its \
               interpreter and turns it to the heading (a turn-to made a 255th of a turn, not \
               instant).",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-DO-MOTION"),
        station: "auberean-physics::cpu::motion::moving::a_motion_with_a_heading_moves_and_turns_to_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "motion.turn.a-placed-body-turns-to-a-255th-of-a-turn",
        says: "A body placed turns at once on its next tick to the placement's heading, fallen to \
               a 255th of a turn.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-PLACED-TURN"),
        station: "auberean-physics::cpu::motion::steering::a_placed_body_turns_to_a_255th_of_a_turn",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "motion.turn.a-set-heading-falls-to-a-255th-of-a-turn",
        says: "A heading set on a body falls to the 255th of a turn below it, as headings are \
               sent.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-SET-HEADING"),
        station: "auberean-physics::cpu::motion::element::a_set_heading_falls_to_a_255th_of_a_turn",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "motion.turn.standing-turns-at-the-still-rate",
        says: "Holding a turn key standing still turns the body at 90 degrees a second once the \
               turn is up to speed.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MOTION-TURN"),
        station: "auberean-physics::cpu::motion::moving::standing_turns_at_the_still_rate",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "motion.walk.a-running-body-keeps-to-the-ground-and-stops-at-a-wall",
        says: "Run north from the 2017 sessions' commonest arrival point, the player's body keeps \
               to the ground, is turned aside by a building's pillars and comes to rest against \
               its wall.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MOTION-WALK"),
        station: "auberean-physics::dat::walk::a_running_body_keeps_to_the_ground_and_stops_at_a_wall",
        tier: Tier::Dat,
    },
];
