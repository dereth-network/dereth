//! A move's parameters, what a move reports, an order to place a body, a heading and a ray, each
//! filled and read through its natives as scripts do.
//! Fixture: new objects of each type, as a script's `new` makes them, and the natives called on
//! them as the machine calls one.

use auberean_glue::motion::{
    position_list, MotionValue, HEADING, MOVEMENT_PARAMETERS, MOVEMENT_RETURN, RAY, SET_POSITION,
};
use auberean_physics::motion::params::{flags, MovementParameters, MovementReturn};
use auberean_script::native_object::{native_type, Engine, NativeObject};
use auberean_script::value::{split, NULL};
use auberean_script::Status;
use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};

use crate::call::Bench;

fn place(cell: u32, x: f32) -> Position {
    Position::new(
        CellId(cell),
        Frame::new(Vec3::new(x, 2.0, 3.0), Quat::IDENTITY),
    )
}

fn position_object(p: &Position) -> NativeObject {
    let (o, q) = (p.frame.origin, p.frame.rotation);
    NativeObject::Position {
        cell: p.cell.0,
        frame: [o.x, o.y, o.z, q.w, q.x, q.y, q.z].map(f32::to_bits),
    }
}

fn vector(b: &mut Bench, x: f32, y: f32, z: f32) -> u32 {
    b.object(
        native_type::VECTOR,
        NativeObject::Vector([x, y, z].map(f32::to_bits)),
    )
}

fn vector_of(b: &Bench, r: u32) -> [f32; 3] {
    match b.vm.heap.native(r).unwrap() {
        NativeObject::Vector(v) => v.map(f32::from_bits),
        o => panic!("not a vector: {o:?}"),
    }
}

fn value(b: &Bench, r: u32) -> MotionValue {
    MotionValue::of(b.vm.heap.native(r).unwrap()).expect("a motion value")
}

/// **A move's parameters, what a move reports and an order to place a body are filled and read as
/// the client's natives do**: each setter does what the engine's own does (a distance asks for a
/// move toward, an object or a place sets or clears the object flag, a heading asks for a turn,
/// the failing distance and the speed are stored as given, a yes-or-no is its low byte); the
/// places to pass are taken one at a time into a position; a report gives a copy of its move, its
/// object, place, radius, height, error and status; an order's flags are set, added, removed and
/// tested, its scatter radii taken in order. A heading is set in degrees (brought into 0 to 360),
/// read both ways and tested against another within a tolerance; a ray from a start and an offset
/// has the offset's direction and length and is finite, a new one is not.
///
/// Behaviour: script.glue.moves-placements-headings-and-rays
#[test]
fn moves_placements_headings_and_rays_are_filled_and_read_as_the_client_does() {
    let mut b = Bench::new();
    let f = f32::to_bits;

    // A move.
    let m = b.object(
        MOVEMENT_PARAMETERS,
        NativeObject::Opaque {
            ty: MOVEMENT_PARAMETERS,
        },
    );
    let target = b.object(
        native_type::POSITION,
        position_object(&place(0x1234_0001, 1.0)),
    );
    let id: u64 = 0x8130_0000_1000_0002;
    let h = b.object(HEADING, NativeObject::Opaque { ty: HEADING });
    b.call(HEADING, 0x01, h, &[f(90.0)], 0);
    let offset = vector(&mut b, 0.5, 0.0, 0.0);
    for (n, args) in [
        (0x0B, vec![f(3.0)]),
        (0x09, vec![split(id).1, split(id).0]),
        (0x0E, vec![target]),
        (0x0C, vec![f(40.0)]),
        (0x11, vec![f(0.5)]),
        (0x0F, vec![h]),
        (0x10, vec![offset]),
        (0x01, vec![0x100]),
        (0x03, vec![0x101]),
        (0x12, vec![77]),
    ] {
        assert_eq!(
            b.call(MOVEMENT_PARAMETERS, n, m, &args, 0).0,
            Status::Success,
            "{n:#x}"
        );
    }
    let mut want = MovementParameters::default();
    want.set_min_success_distance(3.0);
    want.set_target_object(id);
    want.set_target_position(place(0x1234_0001, 1.0));
    want.fail_distance = 40.0;
    want.desired_velocity = 0.5;
    want.set_target_heading(auberean_physics::geom::Heading::new(90.0));
    want.set_offset(Vec3::new(0.5, 0.0, 0.0));
    want.set_callback_when_in_range(true);
    want.set_context(77);
    assert_eq!(value(&b, m), MotionValue::Move(Box::new(want.clone())));
    assert_eq!(want.flags & flags::PERSISTENT, 0, "0x100's low byte is 0");
    assert_eq!(
        want.flags & flags::OBJECT,
        0,
        "a place after the object clears it"
    );
    assert_eq!(b.answer(MOVEMENT_PARAMETERS, 0x13, m, &[]), 77);

    // The places to pass, one at a time.
    let list = position_list(&mut b.vm.heap, 0x65, &[place(1, 10.0), place(2, 20.0)]);
    b.call(MOVEMENT_PARAMETERS, 0x14, m, &[list], 0);
    let out = b.object(native_type::POSITION, position_object(&place(9, 0.0)));
    for want in [Some(place(1, 10.0)), Some(place(2, 20.0)), None] {
        let (status, slots) = b.call_with_out(MOVEMENT_PARAMETERS, 0x15, m, out, 1);
        assert_eq!(status, Status::Success);
        assert_eq!(slots[0].0, u32::from(want.is_some()));
        let held = b.vm.heap.native(out).unwrap().clone();
        assert_eq!(held, position_object(&want.unwrap_or(place(2, 20.0))));
    }

    // What a move reports.
    let report = MovementReturn {
        params: want,
        error: 0x36,
        status: 2,
        position: place(0x5555_0002, 4.0),
        object_radius: 0.75,
        object_height: 1.8,
        persistent: true,
    };
    let r = b.object(
        MOVEMENT_RETURN,
        NativeObject::Engine(Engine::new(MotionValue::Return(Box::new(report.clone())))),
    );
    let copy = b.answer(MOVEMENT_RETURN, 0x01, r, &[]);
    assert_eq!(
        value(&b, copy),
        MotionValue::Move(Box::new(report.params.clone()))
    );
    let (_, ids) = b.call(MOVEMENT_RETURN, 0x02, r, &[], 2);
    assert_eq!((ids[1].0, ids[0].0), split(id));
    assert_eq!(b.answer(MOVEMENT_RETURN, 0x03, r, &[]), 0x36);
    assert_eq!(b.answer(MOVEMENT_RETURN, 0x04, r, &[]), 2);
    let at = b.answer(MOVEMENT_RETURN, 0x05, r, &[]);
    assert_eq!(
        *b.vm.heap.native(at).unwrap(),
        position_object(&report.position)
    );
    assert_eq!(b.answer(MOVEMENT_RETURN, 0x06, r, &[]), f(0.75));
    assert_eq!(b.answer(MOVEMENT_RETURN, 0x07, r, &[]), f(1.8));
    assert_eq!(b.answer(MOVEMENT_RETURN, 0x08, r, &[]), 1);

    // An order to place a body.
    let p = b.object(SET_POSITION, NativeObject::Opaque { ty: SET_POSITION });
    b.call(SET_POSITION, 0x02, p, &[0x41], 0);
    b.call(SET_POSITION, 0x03, p, &[0x200], 0);
    b.call(SET_POSITION, 0x04, p, &[0x1], 0);
    assert_eq!(b.answer(SET_POSITION, 0x05, p, &[]), 0x240);
    assert_eq!(b.answer(SET_POSITION, 0x01, p, &[0x201]), 1);
    assert_eq!(b.answer(SET_POSITION, 0x01, p, &[0x1]), 0);
    b.call(SET_POSITION, 0x09, p, &[f(2.0), f(5.0)], 0);
    b.call(SET_POSITION, 0x0A, p, &[3], 0);
    b.call(SET_POSITION, 0x06, p, &[target], 0);
    let MotionValue::Place(order) = value(&b, p) else {
        panic!("an order");
    };
    let mut want = auberean_physics::place::SetPosition {
        flags: 0x240,
        ..Default::default()
    };
    want.set_scatter_radius(2.0, 5.0);
    want.set_tries(3);
    want.set_position(place(0x1234_0001, 1.0));
    assert_eq!(*order, want);

    // A heading's point of the compass.
    b.call(HEADING, 0x01, h, &[f(292.5)], 0);
    let text = b.answer(HEADING, 0x05, h, &[]);
    assert_eq!(String::from_utf16_lossy(&b.text_of(text)), "WNW");
    let v = vector(&mut b, 1.0, -2.5, 0.0);
    let text = b.answer(native_type::VECTOR, 0x0F, v, &[]);
    assert_eq!(
        String::from_utf16_lossy(&b.text_of(text)),
        "(1.000, -2.500, 0.000)"
    );

    // A heading.
    b.call(HEADING, 0x01, h, &[f(-90.0)], 0);
    assert_eq!(f32::from_bits(b.answer(HEADING, 0x02, h, &[])), 270.0);
    assert_eq!(f32::from_bits(b.answer(HEADING, 0x03, h, &[])), -90.0);
    let (center, tolerance) = (
        b.object(HEADING, NativeObject::Opaque { ty: HEADING }),
        b.object(HEADING, NativeObject::Opaque { ty: HEADING }),
    );
    b.call(HEADING, 0x01, center, &[f(280.0)], 0);
    b.call(HEADING, 0x01, tolerance, &[f(15.0)], 0);
    assert_eq!(
        b.call(HEADING, 0x04, h, &[center, tolerance], 1).1[0].0 & 0xFF,
        1
    );
    b.call(HEADING, 0x01, tolerance, &[f(5.0)], 0);
    assert_eq!(
        b.call(HEADING, 0x04, h, &[center, tolerance], 1).1[0].0 & 0xFF,
        0
    );

    // A ray.
    let ray = b.object(RAY, NativeObject::Opaque { ty: RAY });
    assert_eq!(
        b.answer(RAY, 0x07, ray, &[]),
        0,
        "a new ray is as long as a ray gets"
    );
    let (start, along) = (
        vector(&mut b, 1.0, 2.0, 3.0),
        vector(&mut b, 0.0, 0.0, 10.0),
    );
    b.call(RAY, 0x02, ray, &[start, along], 0);
    assert_eq!(f32::from_bits(b.answer(RAY, 0x06, ray, &[])), 10.0);
    assert_eq!(b.answer(RAY, 0x07, ray, &[]), 1);
    let d = b.answer(RAY, 0x03, ray, &[]);
    assert_eq!(vector_of(&b, d), [0.0, 0.0, 1.0]);
    let s = b.answer(RAY, 0x04, ray, &[]);
    assert_eq!(vector_of(&b, s), [1.0, 2.0, 3.0]);
    let e = b.answer(RAY, 0x05, ray, &[]);
    assert_eq!(vector_of(&b, e), [1.0, 2.0, 13.0]);
    let to = vector(&mut b, 4.0, 6.0, 3.0);
    b.call(RAY, 0x01, ray, &[start, to], 0);
    assert_eq!(f32::from_bits(b.answer(RAY, 0x06, ray, &[])), 5.0);

    // A null receiver ends the script.
    assert_eq!(b.call(RAY, 0x06, NULL, &[], 1).0, Status::Crash);
}

/// **A line of sight is set up and read as the client's natives do**: what it passes through is a
/// copy of a list of objects and a mask of physics types; each test kind sets its number (the
/// physics shapes 0, the exact drawing 1, the environment 2, portals 3, unconnected portals 4);
/// it starts along a ray in a cell, at a place along an offset, or from one place to another (the
/// second's offset taken across landblocks, 160 meters each); an object added to pass through is
/// kept unless it is the invalid id. After a test it answers whether it was unblocked or reached an
/// unloaded landblock, its final ray and cell, and its end less the final cell's landblock offset.
///
/// Behaviour: script.glue.a-line-of-sight-is-set-up-and-read
#[test]
fn a_line_of_sight_is_set_up_and_read_as_the_client_does() {
    use auberean_glue::missile::PhysicsValue;
    use auberean_glue::motion::LOS_PARAMETERS;
    use auberean_physics::ethereal::{ObjectEthereality, PhysicsTypes, SightLine};
    use auberean_physics::geom::ray::Ray;

    let mut b = Bench::new();
    let los = b.object(LOS_PARAMETERS, NativeObject::Opaque { ty: LOS_PARAMETERS });
    let sight = |b: &Bench| match value(b, los) {
        MotionValue::Sight(s) => *s,
        v => panic!("not a line of sight: {v:?}"),
    };

    // What it passes through, copied.
    let mut objects = ObjectEthereality::default();
    objects.add_to_ethereal_list(0x8130_0000_1000_0005);
    let types = PhysicsTypes::new(0x30);
    let o = b.object(
        0x25,
        NativeObject::Engine(Engine::new(PhysicsValue::Objects(objects.clone()))),
    );
    let t = b.object(
        0x24,
        NativeObject::Engine(Engine::new(PhysicsValue::Types(types))),
    );
    b.call(LOS_PARAMETERS, 0x01, los, &[o, t], 0);
    let mut want = SightLine::default();
    want.set_los_params(&objects, types);
    assert_eq!(sight(&b), want);

    // The test kinds.
    for (n, kind) in [(0x02, 2), (0x03, 0), (0x04, 1), (0x05, 3), (0x06, 4)] {
        b.call(LOS_PARAMETERS, n, los, &[], 0);
        assert_eq!(sight(&b).intersect, kind, "{n:#x}");
    }

    // Passing through an object; the invalid id is not added.
    let id: u64 = 0x8130_0000_1000_0007;
    b.call(LOS_PARAMETERS, 0x10, los, &[split(id).1, split(id).0], 0);
    b.call(LOS_PARAMETERS, 0x10, los, &[0, 0], 0);
    let mut kept = objects.clone();
    kept.add_to_ethereal_list(id);
    assert_eq!(sight(&b).objects, kept);

    // Where it starts: a place and an offset.
    let at = b.object(
        native_type::POSITION,
        position_object(&place(0x2020_0001, 10.0)),
    );
    let up = vector(&mut b, 0.0, 0.0, 4.0);
    b.call(LOS_PARAMETERS, 0x08, los, &[at, up], 0);
    let s = sight(&b);
    assert_eq!(
        (s.initial_cell, s.initial_ray),
        (
            0x2020_0001,
            Ray::from_offset(Vec3::new(10.0, 2.0, 3.0), Vec3::new(0.0, 0.0, 4.0))
        )
    );
    // One place to another, a landblock east: 160 meters across.
    let east = b.object(
        native_type::POSITION,
        position_object(&place(0x2120_0001, 10.0)),
    );
    b.call(LOS_PARAMETERS, 0x09, los, &[at, east], 0);
    let s = sight(&b);
    assert_eq!(s.initial_ray.length, 160.0);
    assert_eq!(s.initial_ray.dir, Vec3::new(1.0, 0.0, 0.0));
    // A ray in a cell.
    let ray = b.object(RAY, NativeObject::Opaque { ty: RAY });
    let (start, along) = (vector(&mut b, 1.0, 1.0, 1.0), vector(&mut b, 0.0, 3.0, 0.0));
    b.call(RAY, 0x02, ray, &[start, along], 0);
    b.call(LOS_PARAMETERS, 0x07, los, &[ray, 0x2020_0002], 0);
    assert_eq!(sight(&b).initial_cell, 0x2020_0002);

    // After a test: the engine has filled the outcome.
    let MotionValue::Sight(mut s) = value(&b, los) else {
        unreachable!()
    };
    s.trace_result = 2;
    s.final_cell = 0x2120_0003;
    s.final_ray = Ray::from_offset(Vec3::new(5.0, 6.0, 7.0), Vec3::new(2.0, 0.0, 0.0));
    let done = b.object(
        LOS_PARAMETERS,
        NativeObject::Engine(Engine::new(MotionValue::Sight(s))),
    );
    assert_eq!(b.answer(LOS_PARAMETERS, 0x0A, done, &[]), 0);
    assert_eq!(b.answer(LOS_PARAMETERS, 0x0B, done, &[]), 1);
    assert_eq!(b.answer(LOS_PARAMETERS, 0x0F, done, &[]), 0x2120_0003);
    let r = b.answer(LOS_PARAMETERS, 0x0E, done, &[]);
    assert_eq!(f32::from_bits(b.answer(RAY, 0x06, r, &[])), 2.0);
    let end = b.answer(LOS_PARAMETERS, 0x0D, done, &[]);
    assert_eq!(vector_of(&b, end), [7.0 - 160.0, 6.0, 7.0]);
}

/// **A vector and a ray's direction are made one long as the client's math library does on a
/// processor with SSE2**: the squared length summed in single precision; below 2^-46 the vector
/// becomes zero; otherwise one Newton step from the reciprocal square root, each step rounded to
/// single precision, which differs from dividing by the length in the last bit now and then. The
/// ray's start and length stay.
///
/// Behaviour: script.glue.vectors-and-rays-are-made-one-long
#[test]
fn a_vector_and_a_rays_direction_are_made_one_long_by_one_newton_step() {
    use auberean_glue::position::normalized;
    let bits = |v: [f32; 3]| v.map(f32::to_bits);
    // One Newton step: 0.1, 0.2, 0.3 come out a bit above what dividing by the length gives.
    assert_eq!(
        bits(normalized([0.1, 0.2, 0.3])),
        [0x3E88_D677, 0x3F08_D677, 0x3F4D_41B2]
    );
    assert_eq!(
        bits(normalized([3.0, 4.0, 0.0])),
        [0x3F19_999A, 0x3F4C_CCCD, 0]
    );
    // The Newton step takes the reciprocal square root of 1.69 one bit down before scaling.
    assert_eq!(
        bits(normalized([0.3, 0.4, 1.2])),
        [0x3E6C_4EC5, 0x3E9D_89D8, 0x3F6C_4EC5]
    );
    assert_eq!(bits(normalized([1e-6, 0.0, 0.0])), [0x3F80_0000, 0, 0]);
    // Below 2^-46 squared, zero.
    assert_eq!(normalized([1e-7, 0.0, 0.0]), [0.0; 3]);
    assert!(normalized([f32::NAN, 0.0, 0.0])[0].is_nan());

    let mut b = Bench::new();
    let v = vector(&mut b, 0.1, 0.2, 0.3);
    assert_eq!(b.call(0x70, 0x05, v, &[], 0).0, Status::Success);
    assert_eq!(
        bits(vector_of(&b, v)),
        [0x3E88_D677, 0x3F08_D677, 0x3F4D_41B2]
    );
    assert_eq!(b.call(0x70, 0x05, NULL, &[], 0).0, Status::Crash);

    let ray = b.object(RAY, NativeObject::Opaque { ty: RAY });
    assert_eq!(b.call(RAY, 0x08, ray, &[], 0).0, Status::Success);
    let d = b.answer(RAY, 0x03, ray, &[]);
    assert_eq!(vector_of(&b, d), [0.0; 3], "a new ray has no direction");
    let (start, to) = (vector(&mut b, 1.0, 2.0, 3.0), vector(&mut b, 1.1, 2.2, 3.3));
    b.call(RAY, 0x01, ray, &[start, to], 0);
    let length = b.answer(RAY, 0x06, ray, &[]);
    let d = b.answer(RAY, 0x03, ray, &[]);
    let before = vector_of(&b, d);
    b.call(RAY, 0x08, ray, &[], 0);
    let d = b.answer(RAY, 0x03, ray, &[]);
    let after = vector_of(&b, d);
    assert_eq!(after, normalized(before));
    assert_eq!(b.answer(RAY, 0x06, ray, &[]), length);
    let s = b.answer(RAY, 0x04, ray, &[]);
    assert_eq!(vector_of(&b, s), [1.0, 2.0, 3.0]);
}
