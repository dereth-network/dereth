//! An object's parts: the part groups its look puts together, each a setup's meshes on its bones
//! hung from a connection point of its parent; what is asked of the setup; the box around what is
//! drawn; collisions and rays through the parts' physics trees; and where a body holds things.
//!
//! A row claims what the September 2013 client does with an object's parts.

use super::{Behaviour, Evidence, Tier, RETAIL};

/// This subject's rows, in id order.
pub static ROWS: &[Behaviour] = &[
    behaviour! {
        id: "parts.array.a-change-to-a-setup-reaches-every-array-of-it",
        says: "A part array's setup is the loaded one every array made from it shares: the \
               step-down height or the spheres set through one array are every other array's of \
               that setup, and an array made from it afterwards takes them.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PARTS-SHARED-SETUP"),
        station: "auberean-physics::cpu::contact::objects::a_change_to_a_setup_reaches_every_array_of_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "parts.array.a-part-array-answers-from-its-setup",
        says: "A part array answers from its setup: its spheres, cylinders, radius, height, step \
               heights, free turning, selection and bounding spheres; without a setup it steps up \
               and down a hundredth of a meter and has none of the rest. A setup not in the data \
               changes nothing; one with a mesh not in the data keeps the parts made before it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PARTS-SETUP"),
        station: "auberean-physics::cpu::contact::objects::a_part_array_answers_from_its_setup",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "parts.body.a-body-answers-for-its-shape-from-its-part-group",
        says: "A body answers for its shape from its part group: its setup's cylinders, whether \
               it turns freely, its setup's bounding sphere to be sorted by, and its setup's \
               selection sphere scaled by its look's scale and then its own; with no part group \
               none of them. It is the top of what holds it unless something holds it, and \
               stands for its cell at its first sphere's center. Made with a part group, it \
               reports impacts no slower than its standard forward speed along the ground and \
               its jump impulse times its jump scale up and down, each plus 0.1.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-REP-CALLS"),
        station: "auberean-physics::cpu::contact::objects::a_body_answers_for_its_shape_from_its_part_group",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "parts.body.a-body-steps-as-its-setup-says",
        says: "A body put together from its look steps as its setup says, and a change to its \
               step-down height is its setup's, reaching every body of that setup; asked to stand in \
               another pose of its setup it places its bones so and asks its figure to stand in it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PARTS-BODY"),
        station: "auberean-physics::cpu::contact::objects::a_body_steps_as_its_setup_says",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "parts.body.a-body-that-turns-freely-takes-its-end-facing-first",
        says: "A body whose setup lets it turn freely, or with no part group, goes into a move \
               having taken its end facing; one whose setup does not turns along the way.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-FREE-TURN"),
        station: "auberean-physics::cpu::contact::objects::a_body_that_turns_freely_takes_its_end_facing_first",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "parts.bones.the-nearest-bone-and-the-bones-as-points",
        says: "The bone nearest a frame (by the bones' origins) gives its index and the turn from \
               the bone's rotation to the frame's, at the bone's origin; the bones as points (place \
               and parent) are worked out only for parts weighted to bones.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PARTS-BONES"),
        station: "auberean-physics::cpu::contact::objects::the_nearest_bone_and_the_bones_as_points",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "parts.collision.the-parts-trees-meet-a-sphere-in-the-objects-frame",
        says: "An object's parts meet a sphere placed in their physics trees, every part group's \
               parts tested in the object's own frame whatever the group hangs from; parts not yet \
               found to have trees meet nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PARTS-COLLIDE"),
        station: "auberean-physics::cpu::contact::objects::the_parts_trees_meet_a_sphere_in_the_objects_frame",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "parts.data.placed-objects-collide-in-their-own-frame",
        says: "An object the cells or the land place collides as any object does, every part of \
               it in the object's own frame: its solid parts meet a sphere where its parts put \
               together from its look do, standing where it stands, whatever bone a part is drawn \
               on (the hall's look drawn 0.11 m low, the land's trees with the part on a bone 8 m \
               up).",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PARTS-PLACED"),
        station: "auberean-physics::dat::objects::placed_objects_collide_in_their_own_frame",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "parts.data.the-halls-objects-parts-collide-as-their-shapes-do",
        says: "Every object the human training hall's cells place puts its parts together from its \
               look, and those with physics trees meet a sphere placed around them exactly where \
               the world's shape of the same look does.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PARTS-HALL"),
        station: "auberean-physics::dat::objects::the_halls_objects_parts_collide_as_their_shapes_do",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "parts.data.the-worlds-records-keep-one-loaded-setup",
        says: "The world's records keep one loaded setup for each setup: the step-down height or \
               the spheres changed through one part array of the human male's setup are another \
               array's, and what the records read of the setup afterwards.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PARTS-LOADED-SETUP"),
        station: "auberean-physics::dat::objects::the_worlds_records_keep_one_loaded_setup",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "parts.group.a-changed-look-remakes-adds-and-drops-groups",
        says: "A changed look brings the part groups up to date: a group whose setup or animation \
               map changed is made anew with the groups under it, a group the look adds is made and \
               hung, a group it no longer has goes, and one it leaves as it was keeps its parts.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PARTS-LOOK-CHANGE"),
        station: "auberean-physics::cpu::contact::objects::a_changed_look_remakes_adds_and_drops_groups",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "parts.group.a-look-hangs-each-group-from-its-connection-point",
        says: "A look puts its part groups together, each with a part per mesh on bones placed in \
               its setup's pose and hung from its connection point on the parent's bone; a part's \
               transform is its bone's, then the connection point's, the parent bone's and the \
               parent's frame. A missing animation map stops it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PARTS-HANG"),
        station: "auberean-physics::cpu::contact::objects::a_look_hangs_each_group_from_its_connection_point",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "parts.group.a-looks-changes-swap-meshes-and-materials",
        says: "A look's mesh swaps change the mesh (and physics tree) of each part of the swap's \
               kind, its last opacity stays, and each material change whose modifier is in the \
               data reaches every part; applying changes again restores the setup's meshes and \
               drops the material changes but keeps the opacity.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PARTS-ATOMS"),
        station: "auberean-physics::cpu::contact::objects::a_looks_changes_swap_meshes_and_materials",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "parts.group.the-box-around-what-is-drawn-takes-in-every-part",
        says: "The box around what a part group draws takes in every part of it and of the groups \
               under it: a mesh's own box when its setup has bones, carried through the part's \
               transform when it has none, a mesh drawing nothing adding nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PARTS-BOX"),
        station: "auberean-physics::cpu::contact::objects::the_box_around_what_is_drawn_takes_in_every_part",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "parts.holding.a-carried-system-follows-the-first-groups-bone",
        says: "A particle system carried at a holding location follows the first part group's \
               bone of the location's number, whichever group names the location, and no bone \
               when that group has none of that number; with no such location, its first bone.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PARTS-CARRIED-BONE"),
        station: "auberean-animation::dat::holding::a_carried_system_follows_the_first_groups_bone_whichever_group_names_the_place",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "parts.holding.the-human-male-holds-things-where-its-setups-say",
        says: "A body holds a thing at the holding location of the first part group whose setup \
               names that kind, on its bone as the body is posed; a bone kind is found at the first \
               bone of the kind in the first group that has it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PARTS-HOLD"),
        station: "auberean-animation::dat::holding::the_human_male_holds_things_where_its_setups_say",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "parts.ray.a-ray-stops-at-the-solid-side-and-parts-multiply",
        says: "A ray through a physics tree stops where it first reaches the solid side, the cut \
               plane's normal facing back along it; through an object's parts it is brought into \
               the object's frame and shrunk by its scale, and every part it reaches multiplies \
               the fraction in turn.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PARTS-RAY"),
        station: "auberean-physics::cpu::contact::objects::a_ray_stops_at_the_solid_side_and_parts_multiply",
        tier: Tier::Cpu,
    },
];
