//! Detection against the world: the bodies watching each cell and what they are told of the
//! objects standing in it, the lines of sight the cells trace, the detections a body makes at once
//! against what stands in the world, the rays that cover an object, and a ray cut at landblock
//! edges.
//!
//! A row claims what the September 2013 client does with the cells' watchers and its detections.

use super::{Behaviour, Evidence, Tier, RETAIL};

/// This subject's rows, in id order.
pub static ROWS: &[Behaviour] = &[
    behaviour! {
        id: "detection.cells.a-cell-and-its-landblock-answer-for-what-stands-in-them",
        says: "A cell and a landblock answer with the objects whose own cell they hold, those that \
               move when asked; an object knows how many cells' lists it is in and whether each is \
               loaded; an object held is in no list.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DETECT-CELL-OBJECTS"),
        station: "auberean-physics::cpu::contact::detections::rooms::a_cell_and_its_landblock_answer_for_what_stands_in_them",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "detection.cells.a-point-finds-the-interior-cell-holding-it",
        says: "A point lying in an interior cell finds it, the lowest id of those that hold it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DETECT-CELL-POINT"),
        station: "auberean-physics::cpu::contact::detections::rooms::a_point_finds_the_interior_cell_holding_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "detection.intersect.a-detection-made-at-once-finds-what-is-in-its-volume",
        says: "A detection made at once searches the cells the box around its volume reaches from \
               its origin, in each landblock that box reaches, and notes once each object there \
               it is after (not one it ignores, and of a kind it reports) whose shape is inside \
               the volume; with lines of sight wanted, rays are cast toward each from the origin \
               with the object itself passed through, and an object they find wholly hidden is \
               dropped.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DETECT-WORLD-SEARCH"),
        station: "auberean-physics::cpu::contact::detections::a_detection_made_at_once_finds_what_is_in_its_volume",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "detection.intersect.each-detection-of-a-target-takes-it-again",
        says: "The world keeps one stamp that every search and every insert attempt of a move \
               counts on, and an object is taken once a count: a detection centered on an object \
               takes it again in the same tick, a second taking with nothing between does not, \
               and one after the body has moved does.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-GLOBAL-STAMP"),
        station: "auberean-physics::cpu::contact::detections::each_detection_of_a_target_takes_it_again",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "detection.ray.a-ray-is-cut-at-landblock-edges",
        says: "A ray leaving its landblock is cut where it leaves: the part inside, then the rest \
               in the frame of the landblock it goes on into (across a side or, at a corner, \
               the corner landblock), its start brought just inside; a ray ending inside its \
               landblock is one piece. A box reaches every landblock it overlaps by more than \
               0.0002.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DETECT-BLOCK-RAY"),
        station: "auberean-physics::cpu::contact::detections::a_ray_is_cut_at_landblock_edges",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "detection.ray.an-object-is-covered-by-more-rays-as-asked",
        says: "The rays a line of sight casts at a sphere: from inside it one of no length; else \
               one to its middle, four more to its rim across the line at resolution 2, four more \
               between those at 3, and eight more at half of each at 4. At a box: one to its \
               middle, then one to each corner on a side the origin is beyond or not short of \
               (an origin within the box's span on an axis counts as on its least side), and \
               at resolution 3 the middles of the edges and faces whose corners were taken.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DETECT-COVER-RAYS"),
        station: "auberean-physics::cpu::contact::detections::an_object_is_covered_by_more_rays_as_asked",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "detection.sight.a-line-of-sight-from-the-land-goes-into-a-cave-by-its-mouth",
        says: "A line of sight from the land through the world's cave mouth ends in the cave.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DETECT-LOS-CAVE"),
        station: "auberean-physics::dat::camera::a_line_of_sight_from_the_land_goes_into_a_cave_by_its_mouth",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "detection.sight.a-line-of-sight-goes-out-through-a-doorway-onto-the-land",
        says: "A line of sight leaving a room through a doorway out goes on over the land from the \
               doorway, where what stands outside stops it, reported in the last land cell of the \
               box around the line; when the line's start lies over another land cell than the doorway does, the land traces \
               the line again from its start.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DETECT-LOS-OUT"),
        station: "auberean-physics::cpu::contact::detections::rooms::a_line_of_sight_goes_out_through_a_doorway_onto_the_land",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "detection.sight.a-line-of-sight-goes-through-doorways-into-the-rooms-beyond",
        says: "A line of sight inside goes on through a room's doorway into the room beyond, \
               never back through the doorway it came in by, and the room it ends in is its final \
               cell; each room's own walls (the solid side of its physics tree) and what stands in \
               it stop it. Tracing doorways only it stops at the first doorway; tracing doorways \
               into rooms it goes on through them and no wall counts; told not to go through \
               doorways it stays in its first room.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DETECT-LOS-DOORWAYS"),
        station: "auberean-physics::cpu::contact::detections::rooms::a_line_of_sight_goes_through_doorways_into_the_rooms_beyond",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "detection.sight.a-line-of-sight-over-the-land-goes-in-through-the-doorways-of-rooms-it-touches",
        says: "A line of sight over the land asks the rooms touching each land cell it crosses: \
               through a room's doorway out it goes on inside, where the room's wall stops it, in \
               the room, facing back; past the doorway it goes on over the land.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DETECT-LOS-LAND-ROOMS"),
        station: "auberean-physics::cpu::contact::detections::rooms::a_line_of_sight_over_the_land_goes_in_through_the_doorways_of_rooms_it_touches",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "detection.sight.a-line-tracing-doorways-passes-what-stands-on-the-land",
        says: "What stands on the land stops a line tracing physics shapes, facing back along it, \
               and passes a line tracing doorways only.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DETECT-LOS-LAND-OBJECTS"),
        station: "auberean-physics::cpu::contact::detections::rooms::a_line_tracing_doorways_passes_what_stands_on_the_land",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "detection.sight.a-room-with-a-doorway-out-touches-the-land-cells-under-its-sphere",
        says: "A room the block sees from outside, with a doorway out and a physics tree, touches \
               the land cells under the box around its tree's sphere; a land cell past the block's \
               edge counts as the cell at the same place in this block.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DETECT-LAND-TOUCHING"),
        station: "auberean-physics::cpu::contact::detections::rooms::a_room_with_a_doorway_out_touches_the_land_cells_under_its_sphere",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "detection.sight.objects-and-walls-stop-a-line-of-sight",
        says: "A line of sight from a loaded cell stops at the solid side of an interior cell's \
               walls, at the ground, and at an object standing across it unless it is of a kind \
               the line passes through or the line names it as passed through, or the line traces \
               the surroundings only; stopped, it ends where it was stopped.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DETECT-LOS"),
        station: "auberean-physics::cpu::contact::detections::objects_and_walls_stop_a_line_of_sight",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "detection.sight.the-land-stops-a-line-of-sight-at-its-triangles-facing-up",
        says: "The land stops a line of sight at the first of its triangles the line crosses, from \
               either side, and the line's impact faces the way the ground does.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DETECT-LOS-GROUND"),
        station: "auberean-physics::cpu::contact::detections::rooms::the_land_stops_a_line_of_sight_at_its_triangles_facing_up",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "detection.watch.a-bodys-own-hiding-reaches-its-cells-watchers",
        says: "A body's own setters reach the world it moves in: made undetectable, its cell's \
               watchers hear it leave the next time it ticks (once: hidden already, it is not told of \
               again); made detectable, they hear it come back, each time it is so made; ticking with \
               nothing asked tells them nothing; its state word set whole does the same.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-UNDETECTABLE"),
        station: "auberean-physics::cpu::contact::detections::a_bodys_own_hiding_reaches_its_cells_watchers",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "detection.watch.a-watcher-is-told-what-comes-and-goes",
        says: "A body watching a cell is told when an object comes to stand in it and when it \
               leaves it (into another cell or out of the world), and when one in it is hidden \
               or shown again; never of itself, of an object that never moves, or of an \
               undetectable one.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DETECT-WATCHERS"),
        station: "auberean-physics::cpu::contact::detections::a_watcher_is_told_what_comes_and_goes",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "detection.watch.watches-are-counted-and-the-first-and-last-tell",
        says: "A body's watches of a cell are counted: the first tells it of each object already \
               standing there, as entering, a second only counts, and the last to end tells it \
               of each as leaving. A body's detection around itself watches the cells within its \
               radius.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DETECT-WATCH-COUNT"),
        station: "auberean-physics::cpu::contact::detections::watches_are_counted_and_the_first_and_last_tell",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "detection.world.a-bodys-updates-check-its-spheres-against-the-world",
        says: "Each tick a body's other updates run while woken, and they check its detection spheres against the world it moves in: a sphere around another body notices it once a second has passed since the last check and the other is within the sphere's radius and its own together; the updates keep running while the body has a sphere.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DETECT-UPDATE"),
        station: "auberean-physics::cpu::contact::detections::a_bodys_updates_check_its_spheres_against_the_world",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "detection.world.an-object-with-no-game-object-is-of-every-kind",
        says: "A detection takes an object with no game object for any kind it reports, unless it \
               ignores it or has other targets.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DETECT-NO-GAME-OBJECT"),
        station: "auberean-physics::cpu::contact::detections::an_object_with_no_game_object_is_of_every_kind_a_detection_reports",
        tier: Tier::Cpu,
    },
];
