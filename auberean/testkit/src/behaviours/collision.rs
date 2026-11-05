//! Collision and detection: what a body passes through, the touches and properties it reports,
//! the pushes of what it touches, the volumes that detect bodies, and their binary form.
//!
//! A row claims what the September 2013 client does with a body's collisions and detections.

use super::{Behaviour, Evidence, Tier, RETAIL};

/// This subject's rows, in id order.
pub static ROWS: &[Behaviour] = &[
    behaviour! {
        id: "collision.body.a-fall-stopped-during-the-move-reports-no-impact",
        says: "A fall stopped during the move, a landing that could not be kept, has no speed left when the body tells of the move, so it reports no collision normal or impact.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-COLLISION-KILL-VELOCITY"),
        station: "auberean-physics::cpu::contact::solid::a_fall_stopped_during_the_move_reports_no_impact",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "collision.contact.a-body-on-the-ground-is-on-the-landscape",
        says: "After each move a body's collisions note what it touches: the plane, whether it is \
               under water, and whether the plane is the landscape's ground or an interior's own \
               floors and walls (on the landscape) rather than an object's; touching nothing, it \
               is off the landscape and on no terrain.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-CONTACT-INFO"),
        station: "auberean-physics::cpu::contact::solid::a_body_on_the_ground_is_on_the_landscape",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "collision.contact.a-held-property-is-unchanged-or-changed",
        says: "A property of something touched, touched again with the same value (a vector's \
               parts within a tolerance), is reported unchanged; with a new value of the same \
               kind it takes the value and is reported changed; a value of another kind is \
               ignored.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-COLL-PROPERTY"),
        station: "auberean-physics::cpu::contact::collision::a_held_property_is_unchanged_or_changed",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "collision.contact.a-hit-reports-its-normal-and-impact-speed",
        says: "A hit reports its collision normal when its vertical speed exceeds the least \
               reported impact speed (never less than 0.0002 m/s); its impact speed is the \
               vertical speed into the normal's vertical part; the normal is cleared after each \
               report.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-COLL-NORMAL"),
        station: "auberean-physics::cpu::contact::collision::a_hit_reports_its_normal_and_impact_speed",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "collision.contact.a-touch-ends-a-second-after-it-was-last-renewed",
        says: "An object a body touches begins when the body reports collisions; once not renewed \
               by the step's check and more than a second old, it ends, is forgotten, and the \
               object is told the body no longer touches it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-COLL-CONTACT"),
        station: "auberean-physics::cpu::contact::collision::a_touch_ends_a_second_after_it_was_last_renewed",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "collision.contact.an-inelastic-body-stops-dead-where-another-rebounds",
        says: "A body that hits something with a collision normal, unless it stood on walkable \
               ground before and after, loses 1.005 of its speed into the normal (rebounding a \
               little); an inelastic body loses all its velocity; a body the transition counts as \
               stopped or stuck for two steps or more loses all of it too.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-REBOUND"),
        station: "auberean-physics::cpu::contact::solid::an_inelastic_body_stops_dead_where_another_rebounds",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "collision.dat.every-material-s-properties-survive-packing",
        says: "Every physics material of the September 2013 portal file gives a body touching its surroundings its inquirable properties as newly begun contact properties, and the body keeps every one through its binary form.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-COLL-MATERIALS"),
        station: "auberean-physics::dat::collision::every_material_s_properties_survive_packing",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "collision.dat.the-collision-properties-have-the-kinds-read",
        says: "In the September 2013 property list the terrain property is an enumeration, the twelve push properties are vectors and the local-push property a flag; the environment, water and camera physics types are flags with bits below 64.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-COLL-PROPERTY-KINDS"),
        station: "auberean-physics::dat::collision::the_collision_properties_have_the_kinds_read",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "collision.detect.a-bodys-detections-survive-packing",
        says: "A body's detections pack their spheres by context, the time of the last check and \n               the detections made at once by context.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DETECT-MANAGER-PACK"),
        station: "auberean-physics::cpu::contact::detections::a_bodys_detections_survive_packing",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "collision.detect.a-bodys-sphere-reaches-its-own-radius-further",
        says: "A body's detection sphere reaches the radius asked plus the body's own (its \
               setup's radius by its scale); making one wakes the body's other updates; the body \
               counts its spheres around objects (none around object 0) and around itself, and \
               ends one when asked.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-DETECT-SPHERES"),
        station: "auberean-physics::cpu::contact::detections::a_bodys_sphere_reaches_its_own_radius_further",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "collision.detect.a-cone-reaches-what-lies-inside-its-angle",
        says: "A detection cone reaches a sphere inside its angle and radius, or one at about its \n               radius whose own size reaches over its edge there; not one off its angle wholly \n               inside its radius, however wide; a direction is brought to unit length only when \n               longer than one.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DETECT-CONE"),
        station: "auberean-physics::cpu::contact::detect::a_cone_reaches_what_lies_inside_its_angle",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "collision.detect.a-cylinder-volume-streams-its-radius-before-its-height",
        says: "A detection volume that is an upright cylinder is streamed as its base, then its \
               radius, then its height.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DETECT-VOLUME-CYLINDER"),
        station: "auberean-physics::cpu::contact::detect::a_cylinder_volume_streams_its_radius_before_its_height",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "collision.detect.a-detection-made-at-once-finds-its-origin",
        says: "A detection made at once stands on the body (its base then kept as a place) or at \n               its target as far as a line of sight gets; a line that hits something blocks it \n               unless it is made where the line stops, and a missing or too distant target leaves \n               it not placed.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DETECT-ORIGIN"),
        station: "auberean-physics::cpu::contact::detections::a_detection_made_at_once_finds_its_origin",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "collision.detect.a-detection-notes-an-object-it-is-after-once",
        says: "A detection is after an object it does not ignore: one it targets when it has \
               targets, else one its game object says is of a kind it reports; it notes such an \
               object once, with what the game object notes, then where the object is and its \
               distance from the detection's origin; an object already noted is left as it was.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-DETECT-TARGETS"),
        station: "auberean-physics::cpu::contact::solid::a_detection_notes_an_object_it_is_after_once",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "collision.detect.a-hidden-object-loses-its-sighting",
        says: "When a detection's line of sight finds an object hidden (blocked coverage at least 0.9998) the object's sighting is dropped; otherwise the sighting keeps the greatest coverage found; objects without a sighting are pruned from the landblock's rays.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DETECT-SIGHT"),
        station: "auberean-physics::cpu::contact::detect::a_hidden_object_loses_its_sighting",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "collision.detect.a-place-on-a-body-is-raised-or-traced-through-walls",
        says: "A place on a body (part of its height up and an offset in its frame, scaled when \
               asked) straight above an outdoor place is just raised; otherwise a line of sight \
               through the cells' own walls and floors only runs from the body's origin, and the \
               place is where it stops, said to be clear only when nothing stopped it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-OFFSET-POSITION"),
        station: "auberean-physics::cpu::contact::detections::a_place_on_a_body_is_raised_or_traced_through_walls",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "collision.detect.a-point-volume-cannot-be-streamed",
        says: "A detection volume that is a point has no binary form: packing one writes its \
               place and kind only, and reading one fails after them.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DETECT-VOLUME-POINT"),
        station: "auberean-physics::cpu::contact::detect::a_point_volume_cannot_be_streamed",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "collision.detect.a-point-volume-copies-as-the-zero-point",
        says: "Copying a detection volume that is a point gives the zero point and fails; other volumes copy whole.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DETECT-VOLUME-COPY"),
        station: "auberean-physics::cpu::contact::detect::a_point_volume_copies_as_the_zero_point",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "collision.detect.a-shell-reaches-what-lies-in-its-cut",
        says: "A detection shell cut to headings and elevations reaches a sphere in its cut between \n               its radii, or one the ray along the nearest heading and height the cut allows \n               meets; a heading range whose least bound is the greater wraps through north.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DETECT-SHELL"),
        station: "auberean-physics::cpu::contact::detect::a_shell_reaches_what_lies_in_its_cut",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "collision.detect.a-sphere-notices-its-object-enter-and-leave",
        says: "A detection sphere around an object notices it entering once nearer than the two \n               radii together (exactly that far is outside) and leaving once not, or once gone from \n               the world; each change is reported once.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DETECT-SPHERE"),
        station: "auberean-physics::cpu::contact::detections::a_sphere_notices_its_object_enter_and_leave",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "collision.detect.a-volume-takes-in-shapes-from-elsewhere",
        says: "A detection volume tests a sphere, a cylinder or a box placed elsewhere after carrying \n               it into its own frame, a sphere scaled first only when its scale is even; its box is \n               the box around its shape where it stands; a point or a ray reaches nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DETECT-WITHIN"),
        station: "auberean-physics::cpu::contact::detect::a_volume_takes_in_shapes_from_elsewhere",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "collision.detect.an-attack-cone-orders-and-clamps-its-bounds",
        says: "An attack-cone detection puts its radii in order (each at least 0), brings each heading into 0 to 360 degrees, and puts its elevations in order, each held to -90 to 90 degrees; the heights of its cut are the sines of the elevations taken as radians.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DETECT-ATTACK-CONE"),
        station: "auberean-physics::cpu::contact::detect::an_attack_cone_orders_and_clamps_its_bounds",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "collision.detect.an-object-answers-for-its-parts-and-its-type",
        says: "An object is inside a detection's volume when one of its spheres (at its scale), \
               one of its cylinders, or (with physics trees) the box around them is; and it \
               blocks a line of sight unless the line passes through its physics type and does \
               not list it as solid, or lists it as passed through.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-OBJECT-DETECTED"),
        station: "auberean-physics::cpu::contact::solid::an_object_answers_for_its_parts_and_its_type",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "collision.detect.cylinders-and-boxes-span-what-they-are-given",
        says: "A detection cylinder reaches its height above its center and the given depth below it; a detection box spans the two corners it is given, in either order.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DETECT-VOLUME"),
        station: "auberean-physics::cpu::contact::detect::cylinders_and_boxes_span_what_they_are_given",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "collision.detect.detection-state-survives-packing",
        says: "A detection's settings with any shape of volume, a detection sphere, a sighting and an instant detection with its landblocks and rays keep every field sent through their binary form; the line-of-sight resolution, an instant detection's sightings and its line of sight are not sent.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DETECT-PACK"),
        station: "auberean-physics::cpu::contact::detect::detection_state_survives_packing",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "collision.detect.spheres-and-cylinders-meet-and-stop-rays",
        says: "An upright cylinder reaches a sphere within their two radii of the nearest point of \n               its axis, and another cylinder whose height overlaps within their radii across the \n               ground; a ray meets a sphere or a cylinder at the fraction of its length where it \n               first touches, at once from inside, never heading away.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DETECT-SHAPES"),
        station: "auberean-physics::cpu::contact::detect::spheres_and_cylinders_meet_and_stop_rays",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "collision.detect.spheres-are-checked-each-second-and-destroyed-at-a-check",
        says: "A body's detection spheres are checked a second after the last check or when forced; \n               a sphere marked for destruction stops counting at once and goes at the next check.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DETECT-CHECK"),
        station: "auberean-physics::cpu::contact::detections::spheres_are_checked_each_second_and_destroyed_at_a_check",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "collision.detect.the-global-detection-takes-the-cells-changes",
        says: "The global detection takes the cells' changes: an object entering becomes known, a \n               leaving cancelled by an entry before it is reported leaves it as it was, an entry \n               undone before it is reported is forgotten, and each report forgets those that left.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DETECT-GLOBAL"),
        station: "auberean-physics::cpu::contact::detections::the_global_detection_takes_the_cells_changes",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "collision.detect.the-global-detection-watches-the-cells-around-the-body",
        says: "The global detection watches the cells within its radius of the body, works them out \n               again once the body has moved 5 m (a cell still reached stays watched once), and \n               stops every watch when destroyed; a body keeps one global detection.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DETECT-CELLS"),
        station: "auberean-physics::cpu::contact::detections::the_global_detection_watches_the_cells_around_the_body",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "collision.ethereal.a-mask-takes-one-new-type-at-a-time",
        says: "A body's mask of the scenery physics types it passes through takes on only the \
               lowest type it lacks each time; a physics-type bit is set with a 32-bit shift \
               widened with its sign, so bit 31 also sets the upper word and bits 32 to 63 land \
               on bits 0 to 31.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-COLL-ETHEREAL-MASK"),
        station: "auberean-physics::cpu::contact::collision::a_mask_takes_one_new_type_at_a_time",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "collision.ethereal.a-sight-line-ends-where-its-ray-stopped",
        says: "A line of sight's final ray runs from its start to where its current ray stopped, \
               across landblocks, and its end point is in the final cell's landblock frame; with \
               only one of the start and final cells set it fails.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-COLL-SIGHT-LINE"),
        station: "auberean-physics::cpu::contact::collision::a_sight_line_ends_where_its_ray_stopped",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "collision.ethereal.water-always-blocks-a-sight-line",
        says: "Asking a line of sight to pass through water or not, the water layer ends up \
               blocking it either way.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-COLL-SIGHT-WATER"),
        station: "auberean-physics::cpu::contact::collision::water_always_blocks_a_sight_line",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "collision.objects.a-body-in-step-passes-what-cannot-move",
        says: "A body keeping in step with another passes a body that cannot move, which stops one that is not.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-COLLISION-IN-STEP-STILL"),
        station: "auberean-physics::cpu::contact::objects::a_body_in_step_passes_a_body_that_cannot_move",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "collision.objects.a-body-is-in-the-lists-of-cells-not-yet-in",
        says: "A moving object reaching over a block's edge stands in the lists of the next \
               block's cells whether or not that block is in.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-INIT-DYNAMIC"),
        station: "auberean-physics::cpu::contact::objects::a_body_is_in_the_lists_of_cells_not_yet_in",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "collision.objects.a-body-runs-across-a-dais-reaching-into-its-cell",
        says: "A cell files what it places in every cell the box around what the thing draws \
               reaches: a body standing on the human training hall's dais in the cell beside the \
               dais's own runs across its top to the edge instead of stopping where the top is \
               not in its cell.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-STATIC-CELLS"),
        station: "auberean-physics::dat::objects::a_body_runs_across_a_dais_reaching_into_its_cell",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "collision.objects.a-body-stops-at-an-object-put-in-the-world",
        says: "A body running at an object put in the world stops against it, its sphere outside the \n               object's cylinder; once the object is taken out of the world a body making the \n               same run goes on past where it stood.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-OBJECT-STOPS"),
        station: "auberean-physics::cpu::contact::objects::a_body_stops_at_an_object_put_in_the_world",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "collision.objects.a-body-stops-at-the-movement-sign",
        says: "The Movement sign the server puts in the human training hall collides through its \n               look's one upright cylinder (1.41 m in radius, scaled 0.7): a body running west from \n               where a new character starts stops against it and never comes inside it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-MOVEMENT-SIGN"),
        station: "auberean-physics::dat::objects::a_body_stops_at_the_movement_sign",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "collision.objects.a-cells-object-at-an-uneven-scale-holds-nothing-up",
        says: "An object a cell places with a physics tree at an uneven scale holds nothing up: a \
               body dropped on a flat one 6 x 6 x 0.1, its top 0.3 m above the cell's floor, falls \
               past it to the floor.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-UNEVEN-CELL"),
        station: "auberean-physics::dat::objects::a_cells_object_at_an_uneven_scale_holds_nothing_up",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "collision.objects.a-polygon-clips-to-a-plane",
        says: "A polygon clipped to a plane's positive side keeps its corners on that side and in \n               the plane and gains the crossing of each edge that leaves one side for the other; \n               one wholly behind is negative, one lying in the plane is in it only when corners \n               in the plane are kept.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-CLIP"),
        station: "auberean-physics::cpu::contact::objects::a_polygon_clips_to_a_plane",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "collision.objects.a-solid-over-a-block-edge-is-in-the-next-blocks-cells",
        says: "A solid reaching over a block's edge is met from the land cells it reaches in the \n               block next to it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-LAND-OVERHANG"),
        station: "auberean-physics::cpu::contact::objects::a_solid_over_a_block_edge_is_in_the_next_blocks_cells",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "collision.objects.a-tree-tested-at-a-scale-along-each-axis-reaches-that-far",
        says: "A tree tested at a scale along each axis reaches that far: one at four times its \
               length, its width and six tenths its height meets a sphere 16 m along its length \
               but not 17 m, and 4.5 m across but not 5.5 m.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-TREE-AXES"),
        station: "auberean-physics::dat::objects::a_tree_tested_at_a_scale_along_each_axis_reaches_that_far",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "collision.objects.an-object-outside-stands-in-every-land-cell-under-it",
        says: "An object outside stands in the collision list of every land cell the box around \n               its shape reaches, across a landblock's edge too; inside, of its own cell and of \n               each cell through a doorway whose polygon the box reaches.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-FIND-CELLS"),
        station: "auberean-physics::cpu::contact::objects::an_object_outside_stands_in_every_land_cell_under_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "collision.objects.an-object-with-a-tree-at-an-uneven-scale-is-walked-through",
        says: "An object with a physics tree at an uneven scale is walked through: a body runs \
               through the arch of one placed four times its length, its width and six tenths its \
               height, and through a wall section a tenth taller than its look.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-UNEVEN-PASSED"),
        station: "auberean-physics::dat::objects::an_object_with_a_tree_at_an_uneven_scale_is_walked_through",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "collision.objects.bodies-in-one-world-meet-each-other",
        says: "Bodies, the objects put in the world and the world's scenery share one object table \n               and one collision list per cell: a passable object stops nothing, a solid body \n               stops another, and each body's entry moves with it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-OBJECT-TABLE"),
        station: "auberean-physics::cpu::contact::objects::bodies_in_one_world_meet_each_other",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "collision.objects.boxes-turn-grow-and-meet-spheres-and-cylinders",
        says: "A box carried into a turned frame is the box over its eight corners there; a box \n               grows to take in a point; a sphere or an upright cylinder reaches a box when the \n               box's nearest point is within its radius (for a cylinder, with their heights \n               overlapping).",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-BBOX"),
        station: "auberean-physics::cpu::contact::objects::boxes_turn_grow_and_meet_spheres_and_cylinders",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "collision.objects.only-a-body-whose-qualities-say-so-is-held-at-a-ledge",
        says: "Running west off the top of the human training hall's dais, a body made from the player template's qualities record, which says it slides along edges, is held on the dais's lower tier at its edge, 0.86 m over the floor; a body made from a record that does not say so walks off to the floor.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-PROFILE-LEDGE"),
        station: "auberean-physics::dat::objects::only_a_body_whose_qualities_say_so_is_held_at_a_ledge",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "collision.objects.placed-scenery-with-cylinders-is-solid",
        says: "What a cell's records place is a solid object of the world even when it collides only \n               through its setup's cylinders: in the human training hall a body running north from \n               beside the start stops short of one 2.68 m in reach.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-PLACED"),
        station: "auberean-physics::dat::objects::placed_scenery_with_cylinders_is_solid",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "collision.objects.the-halls-dais-holds-a-body-where-the-client-holds-it",
        says: "The 0930 client's own player, ordered by the server around the human training hall's dais, stops where a body here stops, within a centimeter: held on the dais's lower tier at its west edge, 0.86 m over the floor; held at the foot of its north-west side, which it does not climb, from two places in the hollow; and from the hollow north of it, up onto the lower tier and the top.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-DAIS-LIVE"),
        station: "auberean-physics::dat::objects::the_halls_dais_holds_a_body_where_the_client_holds_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "collision.pack.collision-state-survives-packing",
        says: "A body's collisions (settings, touches, properties, water, ground, pushes), a \
               collision report and a line of sight keep every field through their binary form; \
               a push's local flag and the check time are not sent.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-COLL-PACK"),
        station: "auberean-physics::cpu::contact::collision::collision_state_survives_packing",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "collision.push.pushes-sum-and-walking-on-air-doubles",
        says: "The push properties of what a body touches sum into one push per object; a \
               walk-on-air push replaces what was summed before it with its own velocity and adds \
               it again, and an object whose only push is a water velocity gives none.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-COLL-PUSH"),
        station: "auberean-physics::cpu::contact::collision::pushes_sum_and_walking_on_air_doubles",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "collision.solid.a-body-and-what-it-runs-into-both-keep-the-touch",
        says: "A body's move hands each object it hit to the body's collisions and, when the object moves and keeps collisions, the body to the object's: both keep the touch, and the body reports it beginning.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-HANDLE-COLLISIONS"),
        station: "auberean-physics::cpu::contact::solid::a_body_and_what_it_runs_into_both_keep_the_touch",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "collision.solid.a-body-that-does-not-report-keeps-its-touches-unsaid",
        says: "A body whose state says it does not report its collisions keeps its touches, and neither they nor its normal, water or ground are reported; turning it on begins the touches kept.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-COLLISION-REPORT-FLAG"),
        station: "auberean-physics::cpu::contact::solid::a_body_that_does_not_report_keeps_its_touches_unsaid",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "collision.solid.a-bodys-collision-state-is-its-type-and-what-it-passes-through",
        says: "A body's collision settings as they stand are its physics type, what it passes \
               through being placed, and what it would pass through moving (its desired types, \
               not what its last move let it through), each marked set.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-COLLISION-STATE"),
        station: "auberean-physics::cpu::contact::solid::a_bodys_collision_state_is_its_type_and_what_it_passes_through",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "collision.solid.a-bodys-collisions-take-its-slowest-impact",
        says: "The slowest impact a body's collisions report is the one its speed and jump scales \n               set.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-MIN-IMPACT"),
        station: "auberean-physics::cpu::contact::solid::a_bodys_collisions_take_its_slowest_impact",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "collision.solid.a-creature-blocks-a-player-and-another-player-does-not",
        says: "Objects take the physics type and the movement mask their descriptions give; a player passes through another player (whose type carries a bit of the player's mask) and stops against a creature (whose type carries none).",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-SOLID-MASKS"),
        station: "auberean-physics::cpu::contact::solid::a_creature_blocks_a_player_and_another_player_does_not",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "collision.solid.an-object-made-from-an-entity-takes-its-properties",
        says: "An object made from an entity record takes that entity's properties as a whole description; an entity made from another entity is that one first, then takes its own, so the outer entity's own properties stand (an outer entity with none leaves none) while the ethereal bits the inner one set stay set. A record that is not an entity applies nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-ENTITY-PROPERTIES"),
        station: "auberean-physics::cpu::contact::solid::an_object_made_from_an_entity_takes_its_properties",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "collision.solid.an-objects-description-sets-what-touching-it-reports",
        says: "An object's description sets its properties by group: physics, gameplay and pusher \
               properties are kept as the object's own (a body touching the object keeps them \
               among what it touches), the physics property naming a physics material keeps that \
               material's id, and an ethereal property holding a truth value sets or clears its \
               bit of the object's physics type; applying a whole description first drops what \
               the object had.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-OBJECT-PROPERTIES"),
        station: "auberean-physics::cpu::contact::solid::an_objects_description_sets_what_touching_it_reports",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "collision.solid.bodies-with-no-masks-are-solid-to-each-other",
        says: "Two objects pass through each other only when one is not solid to the other's collision settings; bodies whose descriptions name no physics type and no mask stop each other.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-SOLID-RULE"),
        station: "auberean-physics::cpu::contact::solid::bodies_with_no_masks_are_solid_to_each_other",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "collision.solid.both-sides-of-a-touch-agree-it-is-passed-through",
        says: "Whether two bodies pass through each other is asked from both sides, whichever of \
               them keeps the touch: a creature that passes through players, run into by a player \
               that does not pass through creatures, is a touch both keep as passed through.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-INTERPERMEABLE-BOTH"),
        station: "auberean-physics::cpu::contact::solid::both_sides_of_a_touch_agree_it_is_passed_through",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "collision.solid.outdoor-scenery-blocks-a-player-and-an-item-does-not",
        says: "Entities placed outside are of the outdoor scenery's physics type, which a player's mask lacks: they stop a player, and a mover whose mask has that type passes; an item, of a type in every player's mask, does not stop a player.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-STATIC-ETHEREAL"),
        station: "auberean-physics::cpu::contact::solid::outdoor_scenery_blocks_a_player_and_an_item_does_not",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "collision.solid.outdoor-tree-scenery-asks-what-a-mover-passes",
        says: "Scenery placed outside is an object like any other whatever its shape: tree-shaped \
               scenery marked as something some mover passes through lets those movers by and holds \
               the others up.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-OUTDOOR-TREES-ASK"),
        station: "auberean-physics::dat::objects::outdoor_tree_scenery_lets_by_only_those_that_pass_through_its_mark",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "collision.solid.scenery-takes-its-entitys-properties",
        says: "What a landblock or cell places takes the properties of the entities it is made \
               from, the outdoor scenery's type added outside: scenery whose entity is of the \
               type new movers pass through lets a player run through it; having no object id, it \
               is never among what a body touches, and the body keeps its properties instead.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-STATIC-PROPERTIES"),
        station: "auberean-physics::cpu::contact::solid::scenery_takes_its_entitys_properties",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "collision.solid.what-a-body-touches-pushes-it",
        says: "An object whose description pushes moves what touches it, by the push for where the \n               body is, each tick it touches.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-TOUCH-PUSHERS"),
        station: "auberean-physics::cpu::contact::solid::what_a_body_touches_pushes_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "collision.solid.what-stands-still-is-met-before-what-moves",
        says: "In a cell's collision list what stands still comes first, in the order it came, then \n               what moves.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CELL-LIST-ORDER"),
        station: "auberean-physics::cpu::contact::solid::what_stands_still_is_met_before_what_moves",
        tier: Tier::Cpu,
    },
];
