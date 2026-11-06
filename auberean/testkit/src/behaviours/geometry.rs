//! Geometry: the matrices, frames, planes, boxes, outlines, positions, colors, whole numbers of any
//! size and weighted selection tables the rest of the client works with, held to the September
//! 2013 client's own answers.
//!
//! A row claims what the September 2013 client's own geometry code answers.

use super::{Behaviour, Evidence, Tier, RETAIL};

/// This subject's rows, in id order.
pub static ROWS: &[Behaviour] = &[
    behaviour! {
        id: "geometry.box.every-operation-answers-as-the-client-does",
        says: "A box made from two corners, put in order, scaled about its center and measured for \
               its radius, a heading as radians either side of north and turned on by another, and \
               the binary form of a box, a rotation and a frame all come out as the client's own \
               code gives them, to the bit.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-GEOM-BOX"),
        station: "auberean-physics::cpu::geometry::boxes::every_box_heading_and_packing_operation_answers_as_the_client_does",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "geometry.color.every-operation-answers-as-the-client-does",
        says: "A color capped low and high, scaled to its brightest channel, checked for lying \
               within 0 to 1 (a channel that is not a number passing), compared within 0.0002 a \
               channel and packed into one word, and the tests of a float near zero and of two \
               floats near each other, all answer as the client's own code does, to the bit.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-GEOM-COLOR"),
        station: "auberean-primitives::cpu::color::every_color_operation_answers_as_the_client_does",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "geometry.frame.a-new-frame-is-unturned-at-the-origin",
        says: "A frame made new stands at the origin with no rotation, a copy of a frame has its \
               origin and rotation, and a copy of a sphere its center and radius.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-GEOM-FRAME-NEW"),
        station: "auberean-primitives::cpu::frames::a_new_frame_is_unturned_at_the_origin",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "geometry.frame.every-operation-answers-as-the-client-does",
        says: "A vector turned by a rotation, the rotation about an axis, the rotation of three \
               axes (as given and made unit length), a vector or point carried into and out of a \
               frame, a frame's matrix, the direction it faces and its pitch, a frame set from the \
               directions it faces and has above it, turned to face along a vector and tilted to a \
               pitch all come out as the client's own code gives them, to the bit.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-GEOM-FRAME"),
        station: "auberean-physics::cpu::geometry::frame::every_frame_operation_answers_as_the_client_does",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "geometry.frame.the-frame-between-two-takes-the-line-and-the-shorter-arc",
        says: "The frame part of the way between two others has its origin that part of the way \
               along the line between theirs, to the bit, and its rotation that part of the way \
               along the shorter arc between theirs.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-GEOM-FRAME-SLERP"),
        station: "auberean-physics::cpu::geometry::frame::the_frame_between_two_takes_the_line_and_the_shorter_arc",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "geometry.matrix.every-operation-answers-as-the-client-does",
        says: "A 4 x 4 matrix made a translation, a turn about an axis or a scale, its determinant, \
               adjoint, transpose and both inverses (the full one divided by the determinant of the \
               adjoint it has just written), the rotation it holds, the matrix of a rotation and \
               the matrix looking from one point at another all come out as the client's own code \
               gives them, to the bit.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-GEOM-MATRIX"),
        station: "auberean-physics::cpu::geometry::matrix::every_matrix_operation_answers_as_the_client_does",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "geometry.misc.cylinders-directions-and-map-positions-answer-as-the-client-does",
        says: "A cylinder scaled (its radius by the mean of the two factors across the ground), a \
               direction packed into three bytes, and a position moved to map coordinates come out \
               as the client's own code gives them, to the bit.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-GEOM-MISC"),
        station: "auberean-physics::cpu::geometry::misc::cylinders_directions_and_map_positions_answer_as_the_client_does",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "geometry.outline.every-operation-answers-as-the-client-does",
        says: "An outline's edges and their upright planes, an edge replaced by its middle, whether \
               its corners lie in a plane, the box around them, short edges dropped, whether a \
               point lies outside, its area in space and on the ground, whether it is degenerate or \
               convex, and corners on straight runs or concave turns dropped all come out as the \
               client's own code gives them, to the bit.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-GEOM-OUTLINE"),
        station: "auberean-physics::cpu::geometry::polygon::every_outline_operation_answers_as_the_client_does",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "geometry.plane.every-operation-answers-as-the-client-does",
        says: "A point's height on a plane, whether a plane is one, a plane carried into another \
               position's landblock, where and how far along a ray meets a plane, where a segment \
               crosses it, a ray's span clipped to its positive side and a ray cut in two by it all \
               come out as the client's own code gives them, to the bit.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-GEOM-PLANE"),
        station: "auberean-physics::cpu::geometry::plane::every_plane_operation_answers_as_the_client_does",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "geometry.position.every-operation-answers-as-the-client-does",
        says: "A position's heading and its turn to another, whether two differ, the land cell \
               outside one, another's origin in its landblock, the pitch toward another and the \
               quadrant it stands in, a position sent as a cell and offset, and the land's own \
               answers on its 255-block map all come out as the client's own code gives them, to \
               the bit.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-GEOM-POSITION"),
        station: "auberean-physics::cpu::geometry::position::every_position_operation_answers_as_the_client_does",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "geometry.selection.weighted-draws-follow-the-client",
        says: "Objects drawn by weight from a selection table, each once until the table starts \
               over, come out in the client's own order from the generator in the state every run \
               starts in; the table's binary form and its subtree weights worked out from the \
               weights alone match the client's word for word.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-GEOM-SELECTION"),
        station: "auberean-primitives::cpu::selection::weighted_draws_follow_the_client",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "geometry.sphere.the-time-of-collision-answers-as-the-client-does",
        says: "The time a moving sphere first touches another (the earlier root ahead of the start, \
               else the later; -1 for spheres already overlapping, a move shorter than 0.0002 \
               squared, or no meeting) comes out as the client's own code gives it, to the bit of \
               its double.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-GEOM-SPHERE-TIME"),
        station: "auberean-physics::cpu::geometry::sphere::the_time_of_collision_answers_as_the_client_does",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "geometry.text.a-heading-reads-as-its-point-of-the-compass",
        says: "A heading reads as the nearest of the sixteen points of the compass: within 11.25 \
               degrees of north, south, east or west, the one letter; within 11.25 degrees of the \
               middle of a quarter, its two letters; else three, the nearer of the two first.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-GEOM-TEXT"),
        station: "auberean-physics::cpu::geometry::text::a_heading_reads_as_its_point_of_the_compass",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "geometry.text.vectors-and-planes-read-with-three-places",
        says: "A vector reads as (x, y, z) and a plane as its normal and distance, each with three \
               places, as the client's C library writes them.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-GEOM-TEXT-VECTOR"),
        station: "auberean-physics::cpu::geometry::text::vectors_and_planes_read_with_three_places",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "geometry.tree.object-trees-split-as-the-client-splits-them",
        says: "A tree of squares built from objects' boxes has the client's root middle and squared \
               box to the bit, and holds the same objects in the same squares: a square splits \
               while larger than the smallest size and holding more than one object, and an object \
               straddling a middle stays above it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-GEOM-TREE"),
        station: "auberean-physics::cpu::geometry::tree::object_trees_split_as_the_client_splits_them",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "geometry.vlong.every-operation-answers-as-the-client-does",
        says: "A number of any size made from a word or from words, multiplied (the product cut \
               to the sum of its factors' bit lengths), added across signs (the smaller magnitude \
               taken from the larger, the larger's sign kept) and written in its binary form (its \
               word count, then its words, without its sign) comes out as the client's own code \
               gives it, word for word.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-GEOM-VLONG"),
        station: "auberean-primitives::cpu::vlong::every_vlong_operation_answers_as_the_client_does",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "geometry.vlong.hex-strings-and-the-binary-form-read-back",
        says: "A string of hexadecimal digits reads as the number it writes, digit by digit, a \
               character that is not a digit counting as 0 and a nul ending it; a number's binary \
               form reads back as the number, and a form shorter than its count promises is \
               refused.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-GEOM-VLONG-READ"),
        station: "auberean-primitives::cpu::vlong::hex_strings_and_the_binary_form_read_back",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "geometry.workpoly.areas-projections-and-packing-answer-as-the-client-does",
        says: "A clipping polygon's area and its corners carried through a matrix, and a physics \
               polygon's binary size and form, come out as the client's own code gives them, to the \
               bit.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-GEOM-WORKPOLY"),
        station: "auberean-physics::cpu::geometry::workpoly::work_polygons_and_physics_polygon_packing_answer_as_the_client_does",
        tier: Tier::Cpu,
    },
];
