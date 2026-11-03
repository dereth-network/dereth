//! How a body is asked to move, turn and be placed, as scripts fill it for the engine: a move's
//! parameters, what a finished move reports, an order to put a body somewhere, a heading and a ray.
//!
//! The values are the physics crate's own; these natives fill and read them as the client's do.
//! A new one is the engine's default (a move at full speed between cylinders, a ray at the origin
//! as long as a ray gets, north). Floats pass through the floating-point unit, which quiets a
//! signalling not-a-number; a yes-or-no is the low byte of its slot. A null receiver ends the
//! script; a null argument where an object is wanted stops the run (the engine would read through
//! it).
//!
//! - **A move** ([`MovementParameters`]): its flags one by one (setting a distance, an object or a
//!   heading raises the flag it implies), its target, distances, speed, offset, the caller's tag,
//!   and the places to pass on the way (set from a list of positions, taken one at a time).
//! - **What a move reports** ([`MovementReturn`]): a copy of the move, the target object, why it
//!   ended and how, the place, the object's radius and height, and whether it was to persist.
//! - **An order to place a body** ([`SetPosition`]): its flags, the place, the line a scatter keeps
//!   to (given, or from one place to another), the scatter's radii and how many tries.
//! - **A heading** ([`Heading`]): degrees set (brought into 0 to 360) and read (also from -180 to
//!   180), and whether it lies within a tolerance of another.
//! - **A ray** ([`Ray`]): made from a start and an offset or from two points; its start, unit
//!   direction, end and length read; finite when its length is not the default's.
//! - **A line of sight to test** ([`SightLine`]): what it passes through (a list of objects and a
//!   mask of physics types, copied), what it is tested against (the physics shapes, the exact
//!   drawing, the environment, portals, unconnected portals), whether water blocks it, objects
//!   added to pass through (all but the invalid id), and where it starts: a ray in a cell, a place
//!   and an offset, or one place to another (the second's offset taken across landblocks). After
//!   a test: whether it was unblocked, whether it reached an unloaded landblock, its final ray and
//!   cell, and its end in the starting cell's landblock frame.

use auberean_physics::ethereal::SightLine;
use auberean_physics::geom::ray::Ray;
use auberean_physics::geom::Heading;
use auberean_physics::motion::params::{MovementParameters, MovementReturn};
use auberean_physics::pack::{Pack, Writer};
use auberean_physics::place::SetPosition;
use auberean_script::heap::Heap;
use auberean_script::library::Elem;
use auberean_script::native::{Call, NativeFn, NativeTable, Status, Stop};
use auberean_script::native_object::{native_type, Engine, EngineValue, Item, NativeObject};
use auberean_script::value::{split, tag, NULL};
use dereth_primitives::{Position, Vec3};

use crate::missile::{float, object_id, position, vector, PhysicsValue};
use crate::position::block_offset;

/// A line of sight to test.
pub const LOS_PARAMETERS: u32 = 0x3F;

/// A heading.
pub const HEADING: u32 = 0x2D;
/// A move's parameters.
pub const MOVEMENT_PARAMETERS: u32 = 0x49;
/// What a finished move reports.
pub const MOVEMENT_RETURN: u32 = 0x4A;
/// A ray.
pub const RAY: u32 = 0x62;
/// An order to place a body.
pub const SET_POSITION: u32 = 0x67;

/// A motion value scripts hold, as the engine keeps it.
#[derive(Clone, Debug, PartialEq)]
pub enum MotionValue {
    Move(Box<MovementParameters>),
    Return(Box<MovementReturn>),
    Place(Box<SetPosition>),
    Heading(Heading),
    Ray(Ray),
    Sight(Box<SightLine>),
}

impl MotionValue {
    /// A new one of a native type, as the engine makes it; `None` for a type that is not one.
    #[must_use]
    pub fn new(ty: u32) -> Option<Self> {
        Some(match ty {
            MOVEMENT_PARAMETERS => Self::Move(Box::default()),
            MOVEMENT_RETURN => Self::Return(Box::default()),
            SET_POSITION => Self::Place(Box::default()),
            HEADING => Self::Heading(Heading::ZERO),
            RAY => Self::Ray(Ray::default()),
            LOS_PARAMETERS => Self::Sight(Box::default()),
            _ => return None,
        })
    }

    /// The value a native object holds: one no native has touched yet is a new one.
    #[must_use]
    pub fn of(o: &NativeObject) -> Option<Self> {
        match o {
            NativeObject::Engine(e) => e.get::<Self>().cloned(),
            NativeObject::Opaque { ty } => Self::new(*ty),
            _ => None,
        }
    }
}

impl EngineValue for MotionValue {
    fn native_type(&self) -> u32 {
        match self {
            Self::Move(_) => MOVEMENT_PARAMETERS,
            Self::Return(_) => MOVEMENT_RETURN,
            Self::Place(_) => SET_POSITION,
            Self::Heading(_) => HEADING,
            Self::Ray(_) => RAY,
            Self::Sight(_) => LOS_PARAMETERS,
        }
    }

    // D2-04274
    fn clone_value(&self) -> Box<dyn EngineValue> {
        Box::new(self.clone())
    }

    fn same(&self, other: &dyn EngineValue) -> bool {
        other.as_any().downcast_ref::<Self>() == Some(self)
    }

    fn pack(&self) -> Option<Vec<u8>> {
        let mut w = Writer::new();
        match self {
            Self::Place(p) => p.write(&mut w),
            Self::Ray(r) => r.pack(&mut w),
            Self::Move(_) | Self::Return(_) | Self::Heading(_) | Self::Sight(_) => return None,
        }
        Some(w.finish())
    }

    fn as_any(&self) -> &dyn core::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn core::any::Any {
        self
    }
}

/// This module's natives: package, number, function.
const NATIVES: &[(u32, u32, NativeFn)] = &[
    (MOVEMENT_PARAMETERS, 0x01, set_persistent),
    (MOVEMENT_PARAMETERS, 0x02, set_persistent_fail_exits),
    (MOVEMENT_PARAMETERS, 0x03, set_callback_when_in_range),
    (MOVEMENT_PARAMETERS, 0x04, set_move_towards),
    (MOVEMENT_PARAMETERS, 0x05, set_move_away),
    (MOVEMENT_PARAMETERS, 0x06, set_turn),
    (MOVEMENT_PARAMETERS, 0x07, set_object),
    (MOVEMENT_PARAMETERS, 0x08, set_use_cylinders),
    (MOVEMENT_PARAMETERS, 0x09, set_target_object),
    (MOVEMENT_PARAMETERS, 0x0A, set_max_success_distance),
    (MOVEMENT_PARAMETERS, 0x0B, set_min_success_distance),
    (MOVEMENT_PARAMETERS, 0x0C, set_fail_distance),
    (MOVEMENT_PARAMETERS, 0x0D, set_desired_distance),
    (MOVEMENT_PARAMETERS, 0x0E, set_target_position),
    (MOVEMENT_PARAMETERS, 0x0F, set_target_heading),
    (MOVEMENT_PARAMETERS, 0x10, set_offset),
    (MOVEMENT_PARAMETERS, 0x11, set_desired_velocity),
    (MOVEMENT_PARAMETERS, 0x12, set_context),
    (MOVEMENT_PARAMETERS, 0x13, context),
    (MOVEMENT_PARAMETERS, 0x14, set_waypoints),
    (MOVEMENT_PARAMETERS, 0x15, pop_waypoint),
    (MOVEMENT_RETURN, 0x01, return_movement),
    (MOVEMENT_RETURN, 0x02, return_object),
    (MOVEMENT_RETURN, 0x03, return_error),
    (MOVEMENT_RETURN, 0x04, return_status),
    (MOVEMENT_RETURN, 0x05, return_position),
    (MOVEMENT_RETURN, 0x06, return_radius),
    (MOVEMENT_RETURN, 0x07, return_height),
    (MOVEMENT_RETURN, 0x08, return_persistent),
    (SET_POSITION, 0x01, place_has_flag),
    (SET_POSITION, 0x02, place_set_flags),
    (SET_POSITION, 0x03, place_add_flags),
    (SET_POSITION, 0x04, place_remove_flags),
    (SET_POSITION, 0x05, place_flags),
    (SET_POSITION, 0x06, place_set_position),
    (SET_POSITION, 0x07, place_set_line),
    (SET_POSITION, 0x08, place_set_line_by_positions),
    (SET_POSITION, 0x09, place_set_scatter_radius),
    (SET_POSITION, 0x0A, place_set_tries),
    (HEADING, 0x01, heading_set_degrees),
    (HEADING, 0x02, heading_degrees),
    (HEADING, 0x03, heading_degrees_centered),
    (HEADING, 0x04, heading_in_range),
    (HEADING, 0x05, heading_compass),
    (RAY, 0x01, ray_line_segment),
    (RAY, 0x02, ray_from_offset),
    (RAY, 0x03, ray_direction),
    (RAY, 0x04, ray_start),
    (RAY, 0x05, ray_end),
    (RAY, 0x06, ray_length),
    (RAY, 0x07, ray_is_finite),
    (RAY, 0x08, ray_normalize),
    (LOS_PARAMETERS, 0x01, sight_set_passes),
    (LOS_PARAMETERS, 0x02, sight_environment_only),
    (LOS_PARAMETERS, 0x03, sight_physics_shapes),
    (LOS_PARAMETERS, 0x04, sight_exact_drawing),
    (LOS_PARAMETERS, 0x05, sight_portals_only),
    (LOS_PARAMETERS, 0x06, sight_unconnected_portals_only),
    (LOS_PARAMETERS, 0x07, sight_set_ray),
    (LOS_PARAMETERS, 0x08, sight_from_position_and_offset),
    (LOS_PARAMETERS, 0x09, sight_from_begin_and_end),
    (LOS_PARAMETERS, 0x0A, sight_unblocked),
    (LOS_PARAMETERS, 0x0B, sight_hit_unloaded_block),
    (LOS_PARAMETERS, 0x0C, sight_set_blocked_by_water),
    (LOS_PARAMETERS, 0x0D, sight_end_point),
    (LOS_PARAMETERS, 0x0E, sight_final_ray),
    (LOS_PARAMETERS, 0x0F, sight_final_cell),
    (LOS_PARAMETERS, 0x10, sight_pass_through),
];

/// Bind this module's natives.
pub fn bind(t: &mut NativeTable) {
    crate::bind_table(t, NATIVES);
}

// ---------------------------------------------------------------------------------------------
// Values
// ---------------------------------------------------------------------------------------------

/// The motion value a reference names, made the engine's own the first time a native touches it.
fn value(heap: &mut Heap, native: u32, r: u32) -> Result<&mut MotionValue, Stop> {
    let o = heap.native_mut(r)?;
    if !matches!(o, NativeObject::Engine(_)) {
        if let Some(v) = MotionValue::of(o) {
            *o = NativeObject::Engine(Engine::new(v));
        }
    }
    let not = || Stop::Native {
        native,
        what: "not the motion value this native reads".into(),
    };
    match o {
        NativeObject::Engine(e) => e.get_mut::<MotionValue>().ok_or_else(not),
        _ => Err(not()),
    }
}

/// The receiver's value, picked out by `get`, through `f`; `None` for the null receiver.
fn with<T: ?Sized, R>(
    c: &mut Call<'_>,
    get: fn(&mut MotionValue) -> Option<&mut T>,
    f: impl FnOnce(&mut T) -> R,
) -> Result<Option<R>, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(None);
    }
    let native = c.native;
    let v = value(&mut c.vm.heap, native, r)?;
    let t = get(v).ok_or_else(|| Stop::Native {
        native,
        what: "the receiver is another kind of motion value".into(),
    })?;
    Ok(Some(f(t)))
}

/// Change the receiver's value; answers nothing.
fn change<T: ?Sized>(
    c: &mut Call<'_>,
    get: fn(&mut MotionValue) -> Option<&mut T>,
    f: impl FnOnce(&mut T),
) -> Result<Status, Stop> {
    Ok(with(c, get, f)?.map_or(Status::Crash, |()| Status::Success))
}

/// A word read from the receiver's value, written at `fp[at]` as a native's.
fn answer<T: ?Sized>(
    c: &mut Call<'_>,
    at: i64,
    get: fn(&mut MotionValue) -> Option<&mut T>,
    f: impl FnOnce(&mut T) -> u32,
) -> Result<Status, Stop> {
    match with(c, get, f)? {
        Some(v) => {
            c.write(at, v, tag::MIXED)?;
            Ok(Status::Success)
        }
        None => Ok(Status::Crash),
    }
}

/// A yes-or-no at `fp[at]` written as the client writes a byte: the slot's low byte, tagged.
fn answer_byte(c: &mut Call<'_>, at: i64, b: bool) -> Result<(), Stop> {
    let i = c.cx.frame_index(at)?;
    c.cx.data[i] = (c.cx.data[i] & !0xFF) | u32::from(b);
    c.cx.tags[i] = tag::MIXED;
    Ok(())
}

fn movement(v: &mut MotionValue) -> Option<&mut MovementParameters> {
    match v {
        MotionValue::Move(m) => Some(m),
        _ => None,
    }
}

fn report(v: &mut MotionValue) -> Option<&mut MovementReturn> {
    match v {
        MotionValue::Return(m) => Some(m),
        _ => None,
    }
}

fn placement(v: &mut MotionValue) -> Option<&mut SetPosition> {
    match v {
        MotionValue::Place(p) => Some(p),
        _ => None,
    }
}

fn heading(v: &mut MotionValue) -> Option<&mut Heading> {
    match v {
        MotionValue::Heading(h) => Some(h),
        _ => None,
    }
}

fn ray(v: &mut MotionValue) -> Option<&mut Ray> {
    match v {
        MotionValue::Ray(r) => Some(r),
        _ => None,
    }
}

fn sight(v: &mut MotionValue) -> Option<&mut SightLine> {
    match v {
        MotionValue::Sight(s) => Some(s),
        _ => None,
    }
}

/// Argument `n` as a yes-or-no: the low byte of its slot.
fn flag(c: &Call<'_>, n: i64) -> Result<bool, Stop> {
    Ok(c.arg(n)? & 0xFF != 0)
}

/// The heading an argument names (a copy); a null one stops the run.
fn heading_arg(c: &mut Call<'_>, n: i64) -> Result<Heading, Stop> {
    let r = c.arg(n)?;
    if r == NULL {
        return Err(Stop::NullReference);
    }
    let native = c.native;
    match value(&mut c.vm.heap, native, r)? {
        MotionValue::Heading(h) => Ok(*h),
        _ => Err(c.fail("not a heading")),
    }
}

/// The ray an argument names (a copy); a null one stops the run.
fn ray_arg(c: &mut Call<'_>, n: i64) -> Result<Ray, Stop> {
    let r = c.arg(n)?;
    if r == NULL {
        return Err(Stop::NullReference);
    }
    let native = c.native;
    match value(&mut c.vm.heap, native, r)? {
        MotionValue::Ray(ray) => Ok(*ray),
        _ => Err(c.fail("not a ray")),
    }
}

/// The physics value an argument names (a copy); a null one stops the run.
fn physics_arg(c: &Call<'_>, n: i64) -> Result<PhysicsValue, Stop> {
    let r = c.arg(n)?;
    if r == NULL {
        return Err(Stop::NullReference);
    }
    PhysicsValue::of(c.vm.heap.native(r)?).ok_or_else(|| c.fail("not a physics value"))
}

/// A position as a script's native object holds it.
fn position_object(p: &Position) -> NativeObject {
    let (o, q) = (p.frame.origin, p.frame.rotation);
    NativeObject::Position {
        cell: p.cell.0,
        frame: [o.x, o.y, o.z, q.w, q.x, q.y, q.z].map(f32::to_bits),
    }
}

/// A new object written as the answer at `fp[at]`.
fn give(c: &mut Call<'_>, at: i64, ty: u32, o: NativeObject) -> Result<Status, Stop> {
    let r = c.vm.heap.add_native(ty, o);
    c.write(at, r, tag::REFERENCE)?;
    Ok(Status::Success)
}

/// A new vector, the answer at `fp[-2]`.
fn give_vector(c: &mut Call<'_>, v: Vec3) -> Result<Status, Stop> {
    give(
        c,
        -2,
        native_type::VECTOR,
        NativeObject::Vector([v.x, v.y, v.z].map(f32::to_bits)),
    )
}

// ---------------------------------------------------------------------------------------------
// A move
// ---------------------------------------------------------------------------------------------

// D2-08704
fn set_persistent(c: &mut Call<'_>) -> Result<Status, Stop> {
    let on = flag(c, 0)?;
    change(c, movement, |m| m.set_persistent(on))
}

// D2-08703
fn set_persistent_fail_exits(c: &mut Call<'_>) -> Result<Status, Stop> {
    let on = flag(c, 0)?;
    change(c, movement, |m| m.set_persistent_fail_exits(on))
}

// D2-08692
fn set_callback_when_in_range(c: &mut Call<'_>) -> Result<Status, Stop> {
    let on = flag(c, 0)?;
    change(c, movement, |m| m.set_callback_when_in_range(on))
}

// D2-08700
fn set_move_towards(c: &mut Call<'_>) -> Result<Status, Stop> {
    let on = flag(c, 0)?;
    change(c, movement, |m| m.set_move_towards(on))
}

// D2-08699
fn set_move_away(c: &mut Call<'_>) -> Result<Status, Stop> {
    let on = flag(c, 0)?;
    change(c, movement, |m| m.set_move_away(on))
}

// D2-08708
fn set_turn(c: &mut Call<'_>) -> Result<Status, Stop> {
    let on = flag(c, 0)?;
    change(c, movement, |m| m.set_turn(on))
}

// D2-08701
fn set_object(c: &mut Call<'_>) -> Result<Status, Stop> {
    let on = flag(c, 0)?;
    change(c, movement, |m| m.set_object(on))
}

// D2-08709
fn set_use_cylinders(c: &mut Call<'_>) -> Result<Status, Stop> {
    let on = flag(c, 0)?;
    change(c, movement, |m| m.set_use_cylinders(on))
}

/// The object to move to (`fp[-3]` low, `fp[-2]` high); asks for a move to an object.
// D2-08706
fn set_target_object(c: &mut Call<'_>) -> Result<Status, Stop> {
    let id = object_id(c)?;
    change(c, movement, |m| m.set_target_object(id))
}

// D2-08697
fn set_max_success_distance(c: &mut Call<'_>) -> Result<Status, Stop> {
    let d = float(c, 0)?;
    change(c, movement, |m| m.set_max_success_distance(d))
}

// D2-08698
fn set_min_success_distance(c: &mut Call<'_>) -> Result<Status, Stop> {
    let d = float(c, 0)?;
    change(c, movement, |m| m.set_min_success_distance(d))
}

/// Farther than this from the target, the move has failed: stored as it is, nothing else asked.
// D2-08696
fn set_fail_distance(c: &mut Call<'_>) -> Result<Status, Stop> {
    let d = float(c, 0)?;
    change(c, movement, |m| m.fail_distance = d)
}

// D2-08694
fn set_desired_distance(c: &mut Call<'_>) -> Result<Status, Stop> {
    let d = float(c, 0)?;
    change(c, movement, |m| m.set_desired_distance(d))
}

/// The place to move to (a copy of the position at `fp[-2]`).
// D2-08707
fn set_target_position(c: &mut Call<'_>) -> Result<Status, Stop> {
    let p = position(c, 0)?;
    change(c, movement, |m| m.set_target_position(p))
}

/// The heading to turn to (the heading at `fp[-2]`).
// D2-08705
fn set_target_heading(c: &mut Call<'_>) -> Result<Status, Stop> {
    let h = heading_arg(c, 0)?;
    change(c, movement, |m| m.set_target_heading(h))
}

/// The offset from the target (the vector at `fp[-2]`).
// D2-08702
fn set_offset(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = vector(c, 0)?;
    change(c, movement, |m| m.set_offset(v))
}

/// The fraction of full speed to move at: stored as it is, nothing else asked.
// D2-08695
fn set_desired_velocity(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = float(c, 0)?;
    change(c, movement, |m| m.desired_velocity = v)
}

// D2-08693
fn set_context(c: &mut Call<'_>) -> Result<Status, Stop> {
    let id = c.arg(0)?;
    change(c, movement, |m| m.set_context(id))
}

// D2-08710
fn context(c: &mut Call<'_>) -> Result<Status, Stop> {
    answer(c, -2, movement, |m| m.context)
}

/// The places to pass on the way: the positions of the list at `fp[-2]`, in order.
// D2-08711, D2-08792
fn set_waypoints(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.arg(0)?;
    if r == NULL {
        return Err(Stop::NullReference);
    }
    let items = match c.vm.heap.native(r)? {
        NativeObject::Sequence { items, .. } => items.clone(),
        _ => return Err(c.fail("not a list")),
    };
    let mut list = std::collections::VecDeque::new();
    for i in items {
        let Item::Ref(p) = i else {
            return Err(c.fail("not a list of references"));
        };
        if p == NULL {
            return Err(Stop::NullReference);
        }
        match c.vm.heap.native(p)? {
            NativeObject::Position { cell, frame } => {
                let f = frame.map(f32::from_bits);
                list.push_back(Position::new(
                    dereth_primitives::CellId(*cell),
                    dereth_primitives::Frame::new(
                        Vec3::new(f[0], f[1], f[2]),
                        dereth_primitives::Quat::new(f[3], f[4], f[5], f[6]),
                    ),
                ));
            }
            _ => return Err(c.fail("not a position")),
        }
    }
    change(c, movement, |m| m.set_waypoints(list))
}

/// The next place to pass, into the position the reference at `fp[-2]` reaches (left as it was
/// when none is left); whether there was one, at `fp[-3]`.
// D2-08712
fn pop_waypoint(c: &mut Call<'_>) -> Result<Status, Stop> {
    let at = c.arg(0)?;
    let prog = std::sync::Arc::clone(c.vm.program());
    let (target, _) = c.vm.read_slot(c.host, c.cx, at, 0, &prog)?;
    let Some(next) = with(c, movement, MovementParameters::pop_waypoint)? else {
        return Ok(Status::Crash);
    };
    if let Some(p) = &next {
        if target == NULL {
            return Err(Stop::NullReference);
        }
        *c.vm.heap.native_mut(target)? = position_object(p);
    }
    c.write(-3, u32::from(next.is_some()), tag::MIXED)?;
    Ok(Status::Success)
}

// ---------------------------------------------------------------------------------------------
// What a move reports
// ---------------------------------------------------------------------------------------------

/// A copy of the move, a new object at `fp[-2]`.
// D2-08718
fn return_movement(c: &mut Call<'_>) -> Result<Status, Stop> {
    let Some(m) = with(c, report, |r| r.params.clone())? else {
        return Ok(Status::Crash);
    };
    give(
        c,
        -2,
        MOVEMENT_PARAMETERS,
        NativeObject::Engine(Engine::new(MotionValue::Move(Box::new(m)))),
    )
}

/// The move's target object, `fp[-3]` low and `fp[-2]` high, the low slot tagged as a native's
/// long.
// D2-08715
fn return_object(c: &mut Call<'_>) -> Result<Status, Stop> {
    let Some(id) = with(c, report, |r| r.params.target_object())? else {
        return Ok(Status::Crash);
    };
    let (lo, hi) = split(id);
    c.write(-3, lo, tag::LONG | tag::MIXED)?;
    let i = c.cx.frame_index(-2)?;
    c.cx.data[i] = hi;
    Ok(Status::Success)
}

/// Why the move ended.
// D2-08713
fn return_error(c: &mut Call<'_>) -> Result<Status, Stop> {
    answer(c, -2, report, |r| r.error)
}

/// How the move ended.
// D2-08621
fn return_status(c: &mut Call<'_>) -> Result<Status, Stop> {
    answer(c, -2, report, |r| r.status)
}

/// The place, a new position at `fp[-2]`.
// D2-08719
fn return_position(c: &mut Call<'_>) -> Result<Status, Stop> {
    let Some(p) = with(c, report, |r| r.position)? else {
        return Ok(Status::Crash);
    };
    give(c, -2, native_type::POSITION, position_object(&p))
}

// D2-08716
fn return_radius(c: &mut Call<'_>) -> Result<Status, Stop> {
    answer(c, -2, report, |r| r.object_radius.to_bits())
}

// D2-08714
fn return_height(c: &mut Call<'_>) -> Result<Status, Stop> {
    answer(c, -2, report, |r| r.object_height.to_bits())
}

// D2-08717
fn return_persistent(c: &mut Call<'_>) -> Result<Status, Stop> {
    answer(c, -2, report, |r| u32::from(r.persistent))
}

// ---------------------------------------------------------------------------------------------
// An order to place a body
// ---------------------------------------------------------------------------------------------

/// Whether any of the flags at `fp[-2]` is set, at `fp[-3]`.
// D2-08956
fn place_has_flag(c: &mut Call<'_>) -> Result<Status, Stop> {
    let f = c.arg(0)?;
    answer(c, -3, placement, |p| u32::from(p.flags & f != 0))
}

// D2-08949
fn place_set_flags(c: &mut Call<'_>) -> Result<Status, Stop> {
    let f = c.arg(0)?;
    change(c, placement, |p| p.flags = f)
}

// D2-08947
fn place_add_flags(c: &mut Call<'_>) -> Result<Status, Stop> {
    let f = c.arg(0)?;
    change(c, placement, |p| p.add_flags(f))
}

// D2-08948
fn place_remove_flags(c: &mut Call<'_>) -> Result<Status, Stop> {
    let f = c.arg(0)?;
    change(c, placement, |p| p.remove_flags(f))
}

// D2-08955
fn place_flags(c: &mut Call<'_>) -> Result<Status, Stop> {
    answer(c, -2, placement, |p| p.flags)
}

// D2-08953
fn place_set_position(c: &mut Call<'_>) -> Result<Status, Stop> {
    let at = position(c, 0)?;
    change(c, placement, |p| p.set_position(at))
}

/// The line a scatter keeps to, the vector at `fp[-2]` (made unit length).
// D2-08950
fn place_set_line(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = vector(c, 0)?;
    change(c, placement, |p| {
        p.set_line(v);
    })
}

/// The line from the position at `fp[-2]` to the one at `fp[-3]`.
// D2-08951
fn place_set_line_by_positions(c: &mut Call<'_>) -> Result<Status, Stop> {
    let from = position(c, 0)?;
    let to = position(c, 1)?;
    change(c, placement, |p| {
        p.set_line_by_positions(&from, &to);
    })
}

/// The scatter's radii: `fp[-2]` and `fp[-3]`.
// D2-08954
fn place_set_scatter_radius(c: &mut Call<'_>) -> Result<Status, Stop> {
    let a = float(c, 0)?;
    let b = float(c, 1)?;
    change(c, placement, |p| p.set_scatter_radius(a, b))
}

// D2-08952
fn place_set_tries(c: &mut Call<'_>) -> Result<Status, Stop> {
    let n = c.arg(0)?;
    change(c, placement, |p| p.set_tries(n))
}

// ---------------------------------------------------------------------------------------------
// A heading
// ---------------------------------------------------------------------------------------------

/// The heading set to the degrees at `fp[-2]`, brought into 0 to 360.
// D2-08600
fn heading_set_degrees(c: &mut Call<'_>) -> Result<Status, Stop> {
    let d = float(c, 0)?;
    change(c, heading, |h| *h = Heading::new(d))
}

// D2-08678
fn heading_degrees(c: &mut Call<'_>) -> Result<Status, Stop> {
    answer(c, -2, heading, |h| h.degrees().to_bits())
}

// D2-08601
fn heading_degrees_centered(c: &mut Call<'_>) -> Result<Status, Stop> {
    answer(c, -2, heading, |h| h.degrees_centered().to_bits())
}

/// Whether the heading is within the tolerance at `fp[-3]` of the heading at `fp[-2]`, at
/// `fp[-4]`.
// D2-08602
fn heading_in_range(c: &mut Call<'_>) -> Result<Status, Stop> {
    let center = heading_arg(c, 0)?;
    let tolerance = heading_arg(c, 1)?;
    let Some(inside) = with(c, heading, |h| h.is_in_range(center, tolerance))? else {
        return Ok(Status::Crash);
    };
    answer_byte(c, -4, inside)?;
    Ok(Status::Success)
}

/// The compass point nearest the heading, as a new string at `fp[-2]`.
// D2-08603, D2-13365
fn heading_compass(c: &mut Call<'_>) -> Result<Status, Stop> {
    let Some(text) = with(c, heading, |h| h.compass())? else {
        return Ok(Status::Crash);
    };
    let r = crate::machine::new_text(c, text.encode_utf16().collect());
    c.write(-2, r, tag::REFERENCE)?;
    Ok(Status::Success)
}

// ---------------------------------------------------------------------------------------------
// A ray
// ---------------------------------------------------------------------------------------------

/// From the point at `fp[-2]` to the one at `fp[-3]`.
// D2-08935
fn ray_line_segment(c: &mut Call<'_>) -> Result<Status, Stop> {
    let a = vector(c, 0)?;
    let b = vector(c, 1)?;
    change(c, ray, |r| r.init_line_segment(a, b))
}

/// From the start at `fp[-2]` along the offset at `fp[-3]`.
// D2-08934
fn ray_from_offset(c: &mut Call<'_>) -> Result<Status, Stop> {
    let start = vector(c, 0)?;
    let offset = vector(c, 1)?;
    change(c, ray, |r| r.init_from_offset(start, offset))
}

// D2-08938
fn ray_direction(c: &mut Call<'_>) -> Result<Status, Stop> {
    let Some(v) = with(c, ray, |r| r.dir)? else {
        return Ok(Status::Crash);
    };
    give_vector(c, v)
}

// D2-08940
fn ray_start(c: &mut Call<'_>) -> Result<Status, Stop> {
    let Some(v) = with(c, ray, |r| r.point)? else {
        return Ok(Status::Crash);
    };
    give_vector(c, v)
}

// D2-08939
fn ray_end(c: &mut Call<'_>) -> Result<Status, Stop> {
    let Some(v) = with(c, ray, |r| r.end_point())? else {
        return Ok(Status::Crash);
    };
    give_vector(c, v)
}

// D2-01521
fn ray_length(c: &mut Call<'_>) -> Result<Status, Stop> {
    answer(c, -2, ray, |r| r.length.to_bits())
}

/// Whether the ray's length is not the default's 100 km.
// D2-08937
fn ray_is_finite(c: &mut Call<'_>) -> Result<Status, Stop> {
    #[allow(clippy::float_cmp)]
    answer(c, -2, ray, |r| u32::from(r.length != 100_000.0))
}

/// Make the ray's direction one long, in place, as a vector is made one long
/// ([`crate::position::normalized`]); its start and length stay.
// D2-08936
fn ray_normalize(c: &mut Call<'_>) -> Result<Status, Stop> {
    change(c, ray, |r| {
        let d = crate::position::normalized([r.dir.x, r.dir.y, r.dir.z]);
        r.dir = Vec3::new(d[0], d[1], d[2]);
    })
}

// ---------------------------------------------------------------------------------------------
// A line of sight
// ---------------------------------------------------------------------------------------------

/// What the line passes through: a copy of the list of objects at `fp[-2]` and of the mask of
/// physics types at `fp[-3]`.
// D2-08618
fn sight_set_passes(c: &mut Call<'_>) -> Result<Status, Stop> {
    let PhysicsValue::Objects(objects) = physics_arg(c, 0)? else {
        return Err(c.fail("not a list of objects"));
    };
    let PhysicsValue::Types(types) = physics_arg(c, 1)? else {
        return Err(c.fail("not a mask of physics types"));
    };
    change(c, sight, |s| s.set_los_params(&objects, types))
}

/// What the line is tested against.
fn intersect(c: &mut Call<'_>, kind: u32) -> Result<Status, Stop> {
    change(c, sight, |s| s.set_intersect_type(kind))
}

// D2-08612
fn sight_environment_only(c: &mut Call<'_>) -> Result<Status, Stop> {
    intersect(c, 2)
}

// D2-08619
fn sight_physics_shapes(c: &mut Call<'_>) -> Result<Status, Stop> {
    intersect(c, 0)
}

// D2-08615
fn sight_exact_drawing(c: &mut Call<'_>) -> Result<Status, Stop> {
    intersect(c, 1)
}

// D2-08613
fn sight_portals_only(c: &mut Call<'_>) -> Result<Status, Stop> {
    intersect(c, 3)
}

// D2-08614
fn sight_unconnected_portals_only(c: &mut Call<'_>) -> Result<Status, Stop> {
    intersect(c, 4)
}

/// Starts along the ray at `fp[-2]` in the cell at `fp[-3]`.
// D2-08620
fn sight_set_ray(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = ray_arg(c, 0)?;
    let cell = c.arg(1)?;
    change(c, sight, |s| s.set_ray(r, cell))
}

/// Starts at the place at `fp[-2]`, along the offset at `fp[-3]`, in the place's cell.
// D2-08617
fn sight_from_position_and_offset(c: &mut Call<'_>) -> Result<Status, Stop> {
    let at = position(c, 0)?;
    let offset = vector(c, 1)?;
    let r = Ray::from_offset(at.frame.origin, offset);
    change(c, sight, |s| s.set_ray(r, at.cell.0))
}

/// From the place at `fp[-2]` to the one at `fp[-3]`: the second's origin, across landblocks, less
/// the first's, in the first's cell.
// D2-08616, D2-08609
fn sight_from_begin_and_end(c: &mut Call<'_>) -> Result<Status, Stop> {
    let a = position(c, 0)?;
    let b = position(c, 1)?;
    let [bx, by] = block_offset(a.cell.0, b.cell.0);
    let w = f64::from;
    let (ao, bo) = (a.frame.origin, b.frame.origin);
    #[allow(clippy::cast_possible_truncation)]
    let offset = Vec3::new(
        ((w(bx) + w(bo.x)) - w(ao.x)) as f32,
        ((w(by) + w(bo.y)) - w(ao.y)) as f32,
        (w(bo.z) - w(ao.z)) as f32,
    );
    let r = Ray::from_offset(ao, offset);
    change(c, sight, |s| s.set_ray(r, a.cell.0))
}

/// Whether the test found nothing in the way.
// D2-08623
fn sight_unblocked(c: &mut Call<'_>) -> Result<Status, Stop> {
    answer(c, -2, sight, |s| u32::from(s.trace_result == 0))
}

/// Whether the test reached a landblock not loaded.
// D2-08622
fn sight_hit_unloaded_block(c: &mut Call<'_>) -> Result<Status, Stop> {
    answer(c, -2, sight, |s| u32::from(s.trace_result == 2))
}

/// Whether water blocks the line (any word but 0 at `fp[-2]`).
// D2-08611
fn sight_set_blocked_by_water(c: &mut Call<'_>) -> Result<Status, Stop> {
    let blocked = c.arg(0)? != 0;
    let empty = auberean_assets::property::PropertyList::default();
    let list = c
        .host
        .database()
        .and_then(auberean_assets::Database::property_list)
        .cloned()
        .unwrap_or(empty);
    change(c, sight, |s| s.set_blocked_by_water(&list, blocked))
}

/// The end of the final ray in the starting cell's landblock frame, a new vector: the length
/// along the direction from the start (across, worked in double precision and stored as a float;
/// along and up, the step stored as a float first), less the final cell's landblock offset.
// D2-08624, D2-06112
fn sight_end_point(c: &mut Call<'_>) -> Result<Status, Stop> {
    let Some(s) = with(c, sight, |s| (s.final_ray, s.initial_cell, s.final_cell))? else {
        return Ok(Status::Crash);
    };
    let (r, from, to) = s;
    let w = f64::from;
    #[allow(clippy::cast_possible_truncation)]
    let n = |v: f64| v as f32;
    let x = n(w(r.length) * w(r.dir.x) + w(r.point.x));
    let y = r.length * r.dir.y + r.point.y;
    let z = r.length * r.dir.z + r.point.z;
    let [bx, by] = block_offset(from, to);
    give_vector(c, Vec3::new(x - bx, y - by, z))
}

/// A copy of the final ray, a new object.
// D2-08625
fn sight_final_ray(c: &mut Call<'_>) -> Result<Status, Stop> {
    let Some(r) = with(c, sight, |s| s.final_ray)? else {
        return Ok(Status::Crash);
    };
    give(
        c,
        -2,
        RAY,
        NativeObject::Engine(Engine::new(MotionValue::Ray(r))),
    )
}

// D2-08621
fn sight_final_cell(c: &mut Call<'_>) -> Result<Status, Stop> {
    answer(c, -2, sight, |s| s.final_cell)
}

/// An object to pass through (`fp[-3]` low, `fp[-2]` high), unless it is the invalid id.
// D2-08610
fn sight_pass_through(c: &mut Call<'_>) -> Result<Status, Stop> {
    let id = object_id(c)?;
    change(c, sight, |s| {
        if id != 0 {
            s.objects.add_to_ethereal_list(id);
        }
    })
}

/// A list of positions as new objects, for tests and hosts: the list's references in order.
#[must_use]
pub fn position_list(heap: &mut Heap, ty: u32, places: &[Position]) -> u32 {
    let items = places
        .iter()
        .map(|p| Item::Ref(heap.add_native(native_type::POSITION, position_object(p))))
        .collect();
    heap.add_native(
        ty,
        NativeObject::Sequence {
            item: Elem::Ref,
            items,
        },
    )
}
