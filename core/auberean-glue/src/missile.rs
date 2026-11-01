//! What a missile is, as scripts make it for the engine: the object it is made from, where on the
//! attacker it waits, how it flies, what it aims at, what it passes through, whether it is the
//! thrown weapon itself and whether it looks like the weapon.
//!
//! The engine hands an object's scripts a missile to fill when an attack makes or fires one. The
//! values are the physics crate's own: scripts make each part new (a missile, its aim, what it
//! passes through, a mask of physics types, a list of objects) or take a flight saved with an
//! attack, fill the parts through these natives, and copy them into the missile.
//!
//! - **The missile** ([`MissileInfo`]): its record and holding place (words), whether it is
//!   thrown and whether it is a copy of the weapon (each the argument's low byte), and copies of
//!   the flight, the aim and what it passes through.
//! - **Its flight** ([`MissileParameters`]): each setter stores its values and marks its group set.
//!   Floats pass through the engine's floating-point unit, which quiets a signalling not-a-number;
//!   a yes-or-no is any word but 0.
//! - **Its aim** ([`MissileTargetingParameters`]): an object to strike (its id, a height on it as a
//!   fraction, whether the offset scales with the object, the offset) or a place; an object or a
//!   place to start from; a starting speed and pitch, a range, a chance to go off near the target.
//! - **What it passes through** ([`CollisionSettings`]): masks of physics types (its own, those it
//!   passes through when placed and when moving) and lists of objects (placed, moving). A physics
//!   type is a flag property of the install's property list, its bit the number the property's
//!   descriptor carries; a property that is not a flag sets nothing and answers 0. The first change
//!   to a mask by property clears it; setting a whole mask copies it; setting a mask's low word
//!   also fills its high word with the low word's sign, and neither marks the mask set.
//! - **A mask of physics types** ([`PhysicsTypes`]): solid to all (no bits), ethereal to all (every
//!   bit), one type by property, solid only to the surroundings, ethereal to the water layer.
//! - **A list of objects** ([`ObjectEthereality`]): solid or ethereal to all, and objects added to
//!   either list (each added once; adding answers 1).
//!
//! Reading a mask or a list answers a new object holding a copy. A null receiver ends the script;
//! a null argument where an object is wanted stops the run (the engine would read through it).

use auberean_assets::property::PropertyList;
use auberean_assets::Context;
use auberean_physics::collision::{settings, CollisionSettings};
use auberean_physics::ethereal::{ObjectEthereality, PhysicsTypes};
use auberean_physics::missile::{MissileParameters, MissileTargetingParameters};
use auberean_physics::pack::{Pack, Reader, Writer};
pub use auberean_physics::story::MissileInfo;
use auberean_script::heap::Heap;
use auberean_script::native::{Call, NativeFn, NativeTable, Status, Stop};
use auberean_script::native_object::{Engine, EngineValue, NativeObject};
use auberean_script::value::{join, split, tag, NULL};
use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};

use crate::combat::through_fpu;

/// What a body passes through.
pub const COLLISION_PARAMETERS: u32 = 0x1A;
/// A mask of physics types.
pub const ETHEREAL_PHYSICS_TYPE: u32 = 0x24;
/// A list of objects passed through.
pub const ETHEREAL_WEENIE_TYPE: u32 = 0x25;
/// A missile.
pub const MISSILE_INFO: u32 = 0x45;
/// A missile's flight.
pub const MISSILE_PARAMETERS: u32 = 0x46;
/// A missile's aim.
pub const MISSILE_TARGETING_PARAMETERS: u32 = 0x47;

/// A physics value scripts hold, as the engine keeps it.
#[derive(Clone, Debug, PartialEq)]
pub enum PhysicsValue {
    Missile(Box<MissileInfo>),
    Flight(Box<MissileParameters>),
    Aim(Box<MissileTargetingParameters>),
    Collision(Box<CollisionSettings>),
    /// The physics types a body is ethereal to.
    Types(PhysicsTypes),
    /// The objects a body passes through.
    Objects(ObjectEthereality),
}

impl PhysicsValue {
    /// A new one of a native type, as the engine makes it; `None` for a type that is not one.
    #[must_use]
    pub fn new(ty: u32) -> Option<Self> {
        Some(match ty {
            MISSILE_INFO => Self::Missile(Box::default()),
            MISSILE_PARAMETERS => Self::Flight(Box::default()),
            MISSILE_TARGETING_PARAMETERS => Self::Aim(Box::default()),
            COLLISION_PARAMETERS => Self::Collision(Box::default()),
            ETHEREAL_PHYSICS_TYPE => Self::Types(PhysicsTypes::default()),
            ETHEREAL_WEENIE_TYPE => Self::Objects(ObjectEthereality::default()),
            _ => return None,
        })
    }

    /// The value a native object holds: one no native has touched yet is a new one; a saved
    /// flight is read from its packed form.
    #[must_use]
    pub fn of(o: &NativeObject) -> Option<Self> {
        match o {
            NativeObject::Engine(e) => e.get::<Self>().cloned(),
            NativeObject::Opaque { ty } => Self::new(*ty),
            NativeObject::Packed { ty, bytes } if *ty == MISSILE_PARAMETERS => {
                let mut r = Reader::new(bytes);
                let p = MissileParameters::unpack(&mut r, &Context::empty()).ok()?;
                (r.remaining() == 0).then(|| Self::Flight(Box::new(p)))
            }
            _ => None,
        }
    }

    /// The missile a native object holds.
    #[must_use]
    pub fn missile(o: &NativeObject) -> Option<MissileInfo> {
        match Self::of(o)? {
            Self::Missile(m) => Some(*m),
            _ => None,
        }
    }
}

impl EngineValue for PhysicsValue {
    fn native_type(&self) -> u32 {
        match self {
            Self::Missile(_) => MISSILE_INFO,
            Self::Flight(_) => MISSILE_PARAMETERS,
            Self::Aim(_) => MISSILE_TARGETING_PARAMETERS,
            Self::Collision(_) => COLLISION_PARAMETERS,
            Self::Types(_) => ETHEREAL_PHYSICS_TYPE,
            Self::Objects(_) => ETHEREAL_WEENIE_TYPE,
        }
    }

    fn clone_value(&self) -> Box<dyn EngineValue> {
        Box::new(self.clone())
    }

    fn same(&self, other: &dyn EngineValue) -> bool {
        other.as_any().downcast_ref::<Self>() == Some(self)
    }

    fn pack(&self) -> Option<Vec<u8>> {
        let mut w = Writer::new();
        match self {
            Self::Missile(m) => m.pack(&mut w),
            Self::Flight(p) => p.pack(&mut w),
            Self::Aim(a) => a.pack(&mut w),
            Self::Collision(c) => c.pack(&mut w),
            Self::Types(t) => t.pack(&mut w),
            Self::Objects(o) => o.pack(&mut w),
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
    (MISSILE_INFO, 0x01, set_entity),
    (MISSILE_INFO, 0x02, set_holding_location),
    (MISSILE_INFO, 0x03, set_flight),
    (MISSILE_INFO, 0x04, set_aim),
    (MISSILE_INFO, 0x05, set_thrown),
    (MISSILE_INFO, 0x06, set_clone),
    (MISSILE_INFO, 0x07, set_collision),
    (MISSILE_PARAMETERS, 0x01, set_speed_limits),
    (MISSILE_PARAMETERS, 0x02, set_max_acceleration_uniform),
    (MISSILE_PARAMETERS, 0x03, set_max_acceleration),
    (MISSILE_PARAMETERS, 0x04, set_total_fuel),
    (MISSILE_PARAMETERS, 0x05, set_use_no_fuel),
    (MISSILE_PARAMETERS, 0x06, set_total_lifespan),
    (MISSILE_PARAMETERS, 0x07, set_has_no_lifespan),
    (MISSILE_PARAMETERS, 0x08, set_drag),
    (MISSILE_PARAMETERS, 0x09, set_affected_by_gravity),
    (MISSILE_PARAMETERS, 0x0A, set_follow_ground),
    (MISSILE_PARAMETERS, 0x0B, set_conserve_fuel),
    (MISSILE_PARAMETERS, 0x0C, set_maintain_distance),
    (MISSILE_PARAMETERS, 0x0D, set_homing),
    (MISSILE_PARAMETERS, 0x0E, set_reaction),
    (MISSILE_PARAMETERS, 0x0F, set_turn_method),
    (MISSILE_PARAMETERS, 0x10, use_env_max_speeds),
    (MISSILE_PARAMETERS, 0x11, set_align_to_path),
    (MISSILE_PARAMETERS, 0x12, set_spin_rate),
    (MISSILE_PARAMETERS, 0x13, set_no_align),
    (MISSILE_PARAMETERS, 0x14, set_sticky),
    (MISSILE_PARAMETERS, 0x15, set_can_land),
    (MISSILE_PARAMETERS, 0x16, set_detonates_on_miss),
    (MISSILE_PARAMETERS, 0x17, set_detonation_proximity),
    (MISSILE_TARGETING_PARAMETERS, 0x01, set_target_object),
    (MISSILE_TARGETING_PARAMETERS, 0x02, set_target_position),
    (MISSILE_TARGETING_PARAMETERS, 0x03, set_initial_speed),
    (MISSILE_TARGETING_PARAMETERS, 0x04, set_initial_pitch),
    (MISSILE_TARGETING_PARAMETERS, 0x05, set_start_toward_target),
    (
        MISSILE_TARGETING_PARAMETERS,
        0x06,
        set_always_start_toward_target,
    ),
    (MISSILE_TARGETING_PARAMETERS, 0x07, use_parent_for_base),
    (MISSILE_TARGETING_PARAMETERS, 0x08, use_source_for_base),
    (MISSILE_TARGETING_PARAMETERS, 0x09, set_base_position),
    (MISSILE_TARGETING_PARAMETERS, 0x0A, set_no_fire_if_too_far),
    (MISSILE_TARGETING_PARAMETERS, 0x0B, set_max_range),
    (MISSILE_TARGETING_PARAMETERS, 0x0C, set_detonation_chance),
    (COLLISION_PARAMETERS, 0x01, add_to_ethereal_list),
    (COLLISION_PARAMETERS, 0x02, set_min_impact_xy),
    (COLLISION_PARAMETERS, 0x03, set_min_impact_z),
    (COLLISION_PARAMETERS, 0x04, set_physics_type),
    (COLLISION_PARAMETERS, 0x05, get_physics_type),
    (COLLISION_PARAMETERS, 0x06, set_placement_physics),
    (COLLISION_PARAMETERS, 0x07, update_physics_type),
    (COLLISION_PARAMETERS, 0x08, remove_physics_type),
    (COLLISION_PARAMETERS, 0x09, get_placement_physics),
    (COLLISION_PARAMETERS, 0x0A, set_movement_physics),
    (COLLISION_PARAMETERS, 0x0B, update_movement_ethereal),
    (COLLISION_PARAMETERS, 0x0C, update_movement_solid),
    (COLLISION_PARAMETERS, 0x0D, get_movement_physics),
    (COLLISION_PARAMETERS, 0x0E, set_placement_objects),
    (COLLISION_PARAMETERS, 0x0F, get_placement_objects),
    (COLLISION_PARAMETERS, 0x10, update_placement_ethereal),
    (COLLISION_PARAMETERS, 0x11, update_placement_solid),
    (COLLISION_PARAMETERS, 0x12, set_movement_objects),
    (COLLISION_PARAMETERS, 0x13, get_movement_objects),
    (COLLISION_PARAMETERS, 0x14, set_physics_type_low),
    (COLLISION_PARAMETERS, 0x15, set_physics_type_high),
    (COLLISION_PARAMETERS, 0x16, set_movement_low),
    (COLLISION_PARAMETERS, 0x17, set_movement_high),
    (COLLISION_PARAMETERS, 0x18, set_placement_low),
    (COLLISION_PARAMETERS, 0x19, set_placement_high),
    (ETHEREAL_PHYSICS_TYPE, 0x01, types_to_solid),
    (ETHEREAL_PHYSICS_TYPE, 0x02, types_set_property),
    (ETHEREAL_PHYSICS_TYPE, 0x03, types_set_property_ethereal),
    (ETHEREAL_PHYSICS_TYPE, 0x04, types_set_property_solid),
    (ETHEREAL_PHYSICS_TYPE, 0x05, types_to_ethereal),
    (ETHEREAL_PHYSICS_TYPE, 0x06, types_solid_only_to_environment),
    (ETHEREAL_PHYSICS_TYPE, 0x07, types_ethereal_to_water),
    (ETHEREAL_WEENIE_TYPE, 0x01, objects_to_ethereal),
    (ETHEREAL_WEENIE_TYPE, 0x02, objects_to_solid),
    (ETHEREAL_WEENIE_TYPE, 0x03, objects_add_ethereal),
    (ETHEREAL_WEENIE_TYPE, 0x04, objects_add_solid),
];

/// Bind this module's natives.
pub fn bind(t: &mut NativeTable) {
    crate::bind_table(t, NATIVES);
}

/// The physics value a reference names, made the engine's own the first time a native touches it.
fn value(heap: &mut Heap, native: u32, r: u32) -> Result<&mut PhysicsValue, Stop> {
    let o = heap.native_mut(r)?;
    if !matches!(o, NativeObject::Engine(_)) {
        if let Some(v) = PhysicsValue::of(o) {
            *o = NativeObject::Engine(Engine::new(v));
        }
    }
    match o {
        NativeObject::Engine(e) => e.get_mut::<PhysicsValue>().ok_or_else(|| Stop::Native {
            native,
            what: "not a physics value".into(),
        }),
        _ => Err(Stop::Native {
            native,
            what: "not a physics value".into(),
        }),
    }
}

/// A copy of the physics value an argument names; a null one stops the run.
fn argument(c: &Call<'_>, n: i64) -> Result<PhysicsValue, Stop> {
    let r = c.arg(n)?;
    if r == NULL {
        return Err(Stop::NullReference);
    }
    PhysicsValue::of(c.vm.heap.native(r)?).ok_or_else(|| c.fail("not a physics value"))
}

/// The receiver's value, through `f`; `None` for the null receiver (the call crashes).
fn with<T>(
    c: &mut Call<'_>,
    f: impl FnOnce(&mut PhysicsValue, &PropertyList) -> Result<T, String>,
) -> Result<Option<T>, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(None);
    }
    let native = c.native;
    let empty = PropertyList::default();
    let list = c
        .host
        .database()
        .and_then(auberean_assets::Database::property_list)
        .unwrap_or(&empty);
    let v = value(&mut c.vm.heap, native, r)?;
    f(v, list)
        .map(Some)
        .map_err(|what| Stop::Native { native, what })
}

/// Change the receiver's value, which must be the kind `get` picks out; answers nothing.
fn change<T: ?Sized>(
    c: &mut Call<'_>,
    get: impl FnOnce(&mut PhysicsValue) -> Option<&mut T>,
    f: impl FnOnce(&mut T, &PropertyList),
) -> Result<Status, Stop> {
    let done = with(c, |v, list| {
        let t = get(v).ok_or_else(|| "the receiver is another kind of physics value".to_owned())?;
        f(t, list);
        Ok(())
    })?;
    Ok(done.map_or(Status::Crash, |()| Status::Success))
}

/// Change the receiver's value and write its answer below the arguments (`fp[at]`).
fn answer<T: ?Sized>(
    c: &mut Call<'_>,
    at: i64,
    get: impl FnOnce(&mut PhysicsValue) -> Option<&mut T>,
    f: impl FnOnce(&mut T, &PropertyList) -> u32,
) -> Result<Status, Stop> {
    let a = with(c, |v, list| {
        let t = get(v).ok_or_else(|| "the receiver is another kind of physics value".to_owned())?;
        Ok(f(t, list))
    })?;
    match a {
        Some(a) => {
            c.write(at, a, tag::MIXED)?;
            Ok(Status::Success)
        }
        None => Ok(Status::Crash),
    }
}

fn missile(v: &mut PhysicsValue) -> Option<&mut MissileInfo> {
    match v {
        PhysicsValue::Missile(m) => Some(m),
        _ => None,
    }
}

fn flight(v: &mut PhysicsValue) -> Option<&mut MissileParameters> {
    match v {
        PhysicsValue::Flight(p) => Some(p),
        _ => None,
    }
}

fn aim(v: &mut PhysicsValue) -> Option<&mut MissileTargetingParameters> {
    match v {
        PhysicsValue::Aim(a) => Some(a),
        _ => None,
    }
}

fn collision(v: &mut PhysicsValue) -> Option<&mut CollisionSettings> {
    match v {
        PhysicsValue::Collision(c) => Some(c),
        _ => None,
    }
}

fn types(v: &mut PhysicsValue) -> Option<&mut PhysicsTypes> {
    match v {
        PhysicsValue::Types(t) => Some(t),
        _ => None,
    }
}

fn objects(v: &mut PhysicsValue) -> Option<&mut ObjectEthereality> {
    match v {
        PhysicsValue::Objects(o) => Some(o),
        _ => None,
    }
}

/// Argument `n` as a float through the floating-point unit.
fn float(c: &Call<'_>, n: i64) -> Result<f32, Stop> {
    Ok(f32::from_bits(through_fpu(c.arg(n)?)))
}

/// Argument `n` as a yes-or-no: any word but 0.
fn yes(c: &Call<'_>, n: i64) -> Result<bool, Stop> {
    Ok(c.arg(n)? != 0)
}

/// A 64-bit object id whose low word is at `fp[-3]`.
fn object_id(c: &Call<'_>) -> Result<u64, Stop> {
    c.long_at(-3)
}

/// The three floats of the vector an argument names, copied as they are.
fn vector(c: &Call<'_>, n: i64) -> Result<Vec3, Stop> {
    let r = c.arg(n)?;
    if r == NULL {
        return Err(Stop::NullReference);
    }
    match c.vm.heap.native(r)? {
        NativeObject::Vector(v) => Ok(Vec3::new(
            f32::from_bits(v[0]),
            f32::from_bits(v[1]),
            f32::from_bits(v[2]),
        )),
        _ => Err(c.fail("not a vector")),
    }
}

/// The place an argument names: its cell, origin and rotation, copied as they are.
fn position(c: &Call<'_>, n: i64) -> Result<Position, Stop> {
    let r = c.arg(n)?;
    if r == NULL {
        return Err(Stop::NullReference);
    }
    match c.vm.heap.native(r)? {
        NativeObject::Position { cell, frame } => {
            let f = frame.map(f32::from_bits);
            Ok(Position::new(
                CellId(*cell),
                Frame::new(
                    Vec3::new(f[0], f[1], f[2]),
                    Quat::new(f[3], f[4], f[5], f[6]),
                ),
            ))
        }
        _ => Err(c.fail("not a position")),
    }
}

/// A new object of a physics value, written as the answer at `fp[-2]`.
fn give(c: &mut Call<'_>, v: PhysicsValue) -> Result<Status, Stop> {
    let ty = v.native_type();
    let r =
        c.vm.heap
            .add_native(ty, NativeObject::Engine(Engine::new(v)));
    c.write(-2, r, tag::REFERENCE)?;
    Ok(Status::Success)
}

// The missile.

/// The record the missile is made from (`fp[-2]`).
// D2-08561
fn set_entity(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = c.arg(0)?;
    change(c, missile, |m, _| m.entity = v)
}

/// Where on the attacker the missile waits to be fired (`fp[-2]`; 0 for nowhere).
// D2-08563
fn set_holding_location(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = c.arg(0)?;
    change(c, missile, |m, _| m.holding_location = v)
}

/// A copy of the flight the argument names.
// D2-08632
fn set_flight(c: &mut Call<'_>) -> Result<Status, Stop> {
    if c.receiver()? == NULL {
        return Ok(Status::Crash);
    }
    let PhysicsValue::Flight(p) = argument(c, 0)? else {
        return Err(c.fail("not a missile's flight"));
    };
    change(c, missile, |m, _| m.set_missile_parameters(&p))
}

/// A copy of the aim the argument names.
// D2-08633
fn set_aim(c: &mut Call<'_>) -> Result<Status, Stop> {
    if c.receiver()? == NULL {
        return Ok(Status::Crash);
    }
    let PhysicsValue::Aim(a) = argument(c, 0)? else {
        return Err(c.fail("not a missile's aim"));
    };
    change(c, missile, |m, _| m.set_targeting_parameters(&a))
}

/// A copy of what the missile passes through, from the argument.
// D2-08631
fn set_collision(c: &mut Call<'_>) -> Result<Status, Stop> {
    if c.receiver()? == NULL {
        return Ok(Status::Crash);
    }
    let PhysicsValue::Collision(s) = argument(c, 0)? else {
        return Err(c.fail("not a collision's settings"));
    };
    change(c, missile, |m, _| m.set_collision_parameters(&s))
}

/// Whether the missile is the thrown weapon itself (`fp[-2]`'s low byte).
// D2-08634
fn set_thrown(c: &mut Call<'_>) -> Result<Status, Stop> {
    let on = c.arg(0)? & 0xFF != 0;
    change(c, missile, |m, _| m.set_thrown(on))
}

/// Whether the missile is made as a copy of the weapon (`fp[-2]`'s low byte).
// D2-08630
fn set_clone(c: &mut Call<'_>) -> Result<Status, Stop> {
    let on = c.arg(0)? & 0xFF != 0;
    change(c, missile, |m, _| m.set_clone(on))
}

// The flight.

/// The top speed (`fp[-2]`) and the least (`fp[-3]`).
// D2-08650
fn set_speed_limits(c: &mut Call<'_>) -> Result<Status, Stop> {
    let (max, min) = (float(c, 0)?, float(c, 1)?);
    change(c, flight, |p, _| p.set_speed_limits(max, min))
}

/// One acceleration for every direction (`fp[-2]`).
// D2-08646
fn set_max_acceleration_uniform(c: &mut Call<'_>) -> Result<Status, Stop> {
    let a = float(c, 0)?;
    change(c, flight, |p, _| p.set_max_acceleration_uniform(a))
}

/// The accelerations forward, slowing, left, right, up and down (`fp[-2]` to `fp[-7]`).
// D2-08645
fn set_max_acceleration(c: &mut Call<'_>) -> Result<Status, Stop> {
    let a = [
        float(c, 0)?,
        float(c, 1)?,
        float(c, 2)?,
        float(c, 3)?,
        float(c, 4)?,
        float(c, 5)?,
    ];
    change(c, flight, |p, _| {
        p.set_max_acceleration(a[0], a[1], a[2], a[3], a[4], a[5]);
    })
}

/// The fuel it burns (`fp[-2]`), and whether it says when it runs out (`fp[-3]`).
// D2-08653
fn set_total_fuel(c: &mut Call<'_>) -> Result<Status, Stop> {
    let (fuel, report) = (float(c, 0)?, yes(c, 1)?);
    change(c, flight, |p, _| p.set_total_fuel(fuel, report))
}

// D2-08656
fn set_use_no_fuel(c: &mut Call<'_>) -> Result<Status, Stop> {
    change(c, flight, |p, _| p.set_use_no_fuel())
}

/// How long it lives, seconds (`fp[-2]`).
// D2-08654
fn set_total_lifespan(c: &mut Call<'_>) -> Result<Status, Stop> {
    let s = float(c, 0)?;
    change(c, flight, |p, _| p.set_total_lifespan(s))
}

// D2-08643
fn set_has_no_lifespan(c: &mut Call<'_>) -> Result<Status, Stop> {
    change(c, flight, |p, _| p.set_has_no_lifespan())
}

/// How much of its speed the air takes each second (`fp[-2]`).
// D2-08641
fn set_drag(c: &mut Call<'_>) -> Result<Status, Stop> {
    let d = float(c, 0)?;
    change(c, flight, |p, _| p.set_drag(d))
}

// D2-08635
fn set_affected_by_gravity(c: &mut Call<'_>) -> Result<Status, Stop> {
    let on = yes(c, 0)?;
    change(c, flight, |p, _| p.set_affected_by_gravity(on))
}

/// How it follows the ground (`fp[-2]`) and how high above it (`fp[-3]`).
// D2-08642
fn set_follow_ground(c: &mut Call<'_>) -> Result<Status, Stop> {
    let (kind, height) = (c.arg(0)?, float(c, 1)?);
    change(c, flight, |p, _| p.set_follow_ground(kind, height))
}

/// Whether it saves fuel (`fp[-2]`), the least speed it catches up at (`fp[-3]`), and whether it
/// minds the time it has left (`fp[-4]`).
// D2-08638
fn set_conserve_fuel(c: &mut Call<'_>) -> Result<Status, Stop> {
    let (on, catchup, remaining) = (yes(c, 0)?, float(c, 1)?, yes(c, 2)?);
    change(c, flight, |p, _| {
        p.set_conserve_fuel(on, catchup, remaining)
    })
}

/// The distance it keeps from its target (`fp[-2]`).
// D2-08644
fn set_maintain_distance(c: &mut Call<'_>) -> Result<Status, Stop> {
    let d = float(c, 0)?;
    change(c, flight, |p, _| p.set_maintain_distance(d))
}

// D2-08647
fn set_homing(c: &mut Call<'_>) -> Result<Status, Stop> {
    change(c, flight, |p, _| p.set_homing())
}

/// How often it corrects its course (`fp[-2]`), how long it takes to (`fp[-3]`), and whether it
/// allows for the target's own speed (`fp[-4]`).
// D2-08648
fn set_reaction(c: &mut Call<'_>) -> Result<Status, Stop> {
    let (interval, time, correct) = (float(c, 0)?, float(c, 1)?, yes(c, 2)?);
    change(c, flight, |p, _| p.set_reaction(interval, time, correct))
}

// D2-08655
fn set_turn_method(c: &mut Call<'_>) -> Result<Status, Stop> {
    let m = c.arg(0)?;
    change(c, flight, |p, _| p.set_turn_method(m))
}

// D2-08657
fn use_env_max_speeds(c: &mut Call<'_>) -> Result<Status, Stop> {
    let on = yes(c, 0)?;
    change(c, flight, |p, _| p.use_env_max_speeds(on))
}

/// Aligned to its path, not spinning.
// D2-08636
fn set_align_to_path(c: &mut Call<'_>) -> Result<Status, Stop> {
    change(c, flight, |p, _| p.set_align(0, 0.0))
}

/// Spinning at a rate (`fp[-2]`).
// D2-08651
fn set_spin_rate(c: &mut Call<'_>) -> Result<Status, Stop> {
    let rate = float(c, 0)?;
    change(c, flight, |p, _| p.set_align(1, rate))
}

/// Not aligned at all.
// D2-08652
fn set_no_align(c: &mut Call<'_>) -> Result<Status, Stop> {
    change(c, flight, |p, _| p.set_align(2, 0.0))
}

// D2-08649
fn set_sticky(c: &mut Call<'_>) -> Result<Status, Stop> {
    let on = yes(c, 0)?;
    change(c, flight, |p, _| p.set_sticky(on))
}

// D2-08637
fn set_can_land(c: &mut Call<'_>) -> Result<Status, Stop> {
    let on = yes(c, 0)?;
    change(c, flight, |p, _| p.set_can_land(on))
}

// D2-08639
fn set_detonates_on_miss(c: &mut Call<'_>) -> Result<Status, Stop> {
    let on = yes(c, 0)?;
    change(c, flight, |p, _| p.set_detonates_on_miss(on))
}

/// How near it goes off (`fp[-2]`) and the chance it does (`fp[-3]`).
// D2-08640
fn set_detonation_proximity(c: &mut Call<'_>) -> Result<Status, Stop> {
    let (proximity, chance) = (float(c, 0)?, float(c, 1)?);
    change(c, flight, |p, _| {
        p.set_detonation_proximity(proximity, chance)
    })
}

// The aim.

/// An object to strike (`fp[-3..-2]`), the height on it as a fraction (`fp[-4]`), whether the
/// offset scales with the object (`fp[-5]`), and the offset (the vector `fp[-6]` names).
// D2-08667
fn set_target_object(c: &mut Call<'_>) -> Result<Status, Stop> {
    if c.receiver()? == NULL {
        return Ok(Status::Crash);
    }
    let (id, height, scaled, offset) = (object_id(c)?, float(c, 2)?, yes(c, 3)?, vector(c, 4)?);
    change(c, aim, |a, _| {
        a.set_target_object(id, height, scaled, offset)
    })
}

/// A place to strike (the position `fp[-2]` names).
// D2-08668
fn set_target_position(c: &mut Call<'_>) -> Result<Status, Stop> {
    if c.receiver()? == NULL {
        return Ok(Status::Crash);
    }
    let p = position(c, 0)?;
    change(c, aim, |a, _| a.set_target_position(p))
}

// D2-08662
fn set_initial_speed(c: &mut Call<'_>) -> Result<Status, Stop> {
    let s = float(c, 0)?;
    change(c, aim, |a, _| a.set_initial_speed(s))
}

/// Degrees above level (`fp[-2]`).
// D2-08661
fn set_initial_pitch(c: &mut Call<'_>) -> Result<Status, Stop> {
    let p = float(c, 0)?;
    change(c, aim, |a, _| a.set_initial_pitch(p))
}

// D2-08666
fn set_start_toward_target(c: &mut Call<'_>) -> Result<Status, Stop> {
    change(c, aim, |a, _| a.set_start_toward_target())
}

// D2-08659
fn set_always_start_toward_target(c: &mut Call<'_>) -> Result<Status, Stop> {
    change(c, aim, |a, _| a.set_always_start_toward_target())
}

// D2-08658
fn use_parent_for_base(c: &mut Call<'_>) -> Result<Status, Stop> {
    change(c, aim, |a, _| a.use_parent_for_base())
}

/// An object to start from, as [`set_target_object`] names one.
// D2-08669
fn use_source_for_base(c: &mut Call<'_>) -> Result<Status, Stop> {
    if c.receiver()? == NULL {
        return Ok(Status::Crash);
    }
    let (id, height, scaled, offset) = (object_id(c)?, float(c, 2)?, yes(c, 3)?, vector(c, 4)?);
    change(c, aim, |a, _| {
        a.use_source_for_base(id, height, scaled, offset)
    })
}

/// A place to start from (the position `fp[-2]` names).
// D2-08660
fn set_base_position(c: &mut Call<'_>) -> Result<Status, Stop> {
    if c.receiver()? == NULL {
        return Ok(Status::Crash);
    }
    let p = position(c, 0)?;
    change(c, aim, |a, _| a.set_base_position(p))
}

// D2-08665
fn set_no_fire_if_too_far(c: &mut Call<'_>) -> Result<Status, Stop> {
    change(c, aim, |a, _| a.set_no_fire_if_too_far())
}

// D2-08663
fn set_max_range(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = float(c, 0)?;
    change(c, aim, |a, _| a.set_max_range(r))
}

// D2-08664
fn set_detonation_chance(c: &mut Call<'_>) -> Result<Status, Stop> {
    let p = float(c, 0)?;
    change(c, aim, |a, _| a.set_detonation_chance(p))
}

// What a body passes through.

/// Passes through an object (`fp[-3..-2]`), placed and moving; answers 1 (`fp[-4]`).
// D2-08448
fn add_to_ethereal_list(c: &mut Call<'_>) -> Result<Status, Stop> {
    let id = object_id(c)?;
    answer(c, -4, collision, |s, _| {
        s.add_to_ethereal_list(id);
        1
    })
}

/// The least impact speed reported across (`fp[-2]`), copied as it is.
// D2-08435
fn set_min_impact_xy(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = f32::from_bits(c.arg(0)?);
    change(c, collision, |s, _| s.min_impact_xy = v)
}

/// The least impact speed reported up and down (`fp[-2]`), copied as it is.
// D2-08436
fn set_min_impact_z(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = f32::from_bits(c.arg(0)?);
    change(c, collision, |s, _| s.min_impact_z = v)
}

/// The mask an argument names (`fp[-2]`).
fn mask_argument(c: &Call<'_>) -> Result<u64, Stop> {
    match argument(c, 0)? {
        PhysicsValue::Types(t) => Ok(t.mask),
        _ => Err(c.fail("not a mask of physics types")),
    }
}

/// The objects list an argument names (`fp[-2]`).
fn objects_argument(c: &Call<'_>) -> Result<ObjectEthereality, Stop> {
    match argument(c, 0)? {
        PhysicsValue::Objects(o) => Ok(o),
        _ => Err(c.fail("not a list of objects")),
    }
}

/// Its own physics types: a copy of the argument's mask, marked set.
// D2-08441
fn set_physics_type(c: &mut Call<'_>) -> Result<Status, Stop> {
    if c.receiver()? == NULL {
        return Ok(Status::Crash);
    }
    let m = mask_argument(c)?;
    change(c, collision, |s, _| {
        s.physics_type.mask = m;
        s.set_bit(settings::PHYSICS_TYPE);
    })
}

/// The physics types it passes through when placed: a copy of the argument's mask, marked set.
// D2-08440
fn set_placement_physics(c: &mut Call<'_>) -> Result<Status, Stop> {
    if c.receiver()? == NULL {
        return Ok(Status::Crash);
    }
    let m = mask_argument(c)?;
    change(c, collision, |s, _| {
        s.placement_physics.mask = m;
        s.set_bit(settings::PLACEMENT_PHYSICS);
    })
}

/// The physics types it passes through when moving: a copy of the argument's mask, marked set.
// D2-08439
fn set_movement_physics(c: &mut Call<'_>) -> Result<Status, Stop> {
    if c.receiver()? == NULL {
        return Ok(Status::Crash);
    }
    let m = mask_argument(c)?;
    change(c, collision, |s, _| {
        s.movement_physics.mask = m;
        s.set_bit(settings::MOVEMENT_PHYSICS);
    })
}

/// The objects it passes through when placed: brought up to the argument's, marked set.
// D2-08447
fn set_placement_objects(c: &mut Call<'_>) -> Result<Status, Stop> {
    if c.receiver()? == NULL {
        return Ok(Status::Crash);
    }
    let o = objects_argument(c)?;
    change(c, collision, |s, _| {
        o.update(&mut s.placement_objects);
        s.set_bit(settings::PLACEMENT_OBJECTS);
    })
}

/// The objects it passes through when moving: brought up to the argument's, marked set.
// D2-08446
fn set_movement_objects(c: &mut Call<'_>) -> Result<Status, Stop> {
    if c.receiver()? == NULL {
        return Ok(Status::Crash);
    }
    let o = objects_argument(c)?;
    change(c, collision, |s, _| {
        o.update(&mut s.movement_objects);
        s.set_bit(settings::MOVEMENT_OBJECTS);
    })
}

/// A copy of one of the receiver's masks or lists, as a new object.
fn read_part(
    c: &mut Call<'_>,
    part: impl FnOnce(&CollisionSettings) -> PhysicsValue,
) -> Result<Status, Stop> {
    let v = with(c, |v, _| match v {
        PhysicsValue::Collision(s) => Ok(part(s)),
        _ => Err("the receiver is another kind of physics value".to_owned()),
    })?;
    match v {
        Some(v) => give(c, v),
        None => Ok(Status::Crash),
    }
}

// D2-08457
fn get_physics_type(c: &mut Call<'_>) -> Result<Status, Stop> {
    read_part(c, |s| PhysicsValue::Types(s.physics_type))
}

// D2-08456
fn get_placement_physics(c: &mut Call<'_>) -> Result<Status, Stop> {
    read_part(c, |s| PhysicsValue::Types(s.placement_physics))
}

// D2-08455
fn get_movement_physics(c: &mut Call<'_>) -> Result<Status, Stop> {
    read_part(c, |s| PhysicsValue::Types(s.movement_physics))
}

// D2-08459
fn get_placement_objects(c: &mut Call<'_>) -> Result<Status, Stop> {
    read_part(c, |s| PhysicsValue::Objects(s.placement_objects.clone()))
}

// D2-08458
fn get_movement_objects(c: &mut Call<'_>) -> Result<Status, Stop> {
    read_part(c, |s| PhysicsValue::Objects(s.movement_objects.clone()))
}

/// A physics type (`fp[-2]`) changed in one mask; answers whether it is a physics type (`fp[-3]`).
fn by_property(
    c: &mut Call<'_>,
    f: fn(&mut CollisionSettings, &PropertyList, u32) -> bool,
) -> Result<Status, Stop> {
    let property = c.arg(0)?;
    answer(c, -3, collision, |s, list| u32::from(f(s, list, property)))
}

// D2-08452
fn update_physics_type(c: &mut Call<'_>) -> Result<Status, Stop> {
    by_property(c, CollisionSettings::update_physics_type)
}

// D2-08449
fn remove_physics_type(c: &mut Call<'_>) -> Result<Status, Stop> {
    by_property(c, CollisionSettings::remove_physics_type)
}

// D2-08450
fn update_movement_ethereal(c: &mut Call<'_>) -> Result<Status, Stop> {
    by_property(c, CollisionSettings::update_movement_ethereal)
}

// D2-08451
fn update_movement_solid(c: &mut Call<'_>) -> Result<Status, Stop> {
    by_property(c, CollisionSettings::update_movement_solid)
}

// D2-08453
fn update_placement_ethereal(c: &mut Call<'_>) -> Result<Status, Stop> {
    by_property(c, CollisionSettings::update_placement_ethereal)
}

// D2-08454
fn update_placement_solid(c: &mut Call<'_>) -> Result<Status, Stop> {
    by_property(c, CollisionSettings::update_placement_solid)
}

/// A mask's low word from `fp[-2]`, its high word gaining the low word's sign (every bit when
/// negative); the mask is not marked set.
fn low_word(mask: &mut u64, v: u32) {
    let sign = if v & 0x8000_0000 != 0 { u32::MAX } else { 0 };
    let (_, high) = split(*mask);
    *mask = join(v, high | sign);
}

/// A mask's high word from `fp[-2]`, its low word kept; the mask is not marked set.
fn high_word(mask: &mut u64, v: u32) {
    let (low, _) = split(*mask);
    *mask = join(low, v);
}

// D2-08443
fn set_physics_type_low(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = c.arg(0)?;
    change(c, collision, |s, _| low_word(&mut s.physics_type.mask, v))
}

// D2-08442
fn set_physics_type_high(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = c.arg(0)?;
    change(c, collision, |s, _| high_word(&mut s.physics_type.mask, v))
}

// D2-08438
fn set_movement_low(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = c.arg(0)?;
    change(c, collision, |s, _| {
        low_word(&mut s.movement_physics.mask, v)
    })
}

// D2-08437
fn set_movement_high(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = c.arg(0)?;
    change(c, collision, |s, _| {
        high_word(&mut s.movement_physics.mask, v)
    })
}

// D2-08445
fn set_placement_low(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = c.arg(0)?;
    change(c, collision, |s, _| {
        low_word(&mut s.placement_physics.mask, v)
    })
}

// D2-08444
fn set_placement_high(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = c.arg(0)?;
    change(c, collision, |s, _| {
        high_word(&mut s.placement_physics.mask, v)
    })
}

// A mask of physics types.

/// Solid to every physics type: no bits.
// D2-08553
fn types_to_solid(c: &mut Call<'_>) -> Result<Status, Stop> {
    change(c, types, |t, _| t.mask = 0)
}

/// Ethereal to every physics type: every bit.
// D2-08552
fn types_to_ethereal(c: &mut Call<'_>) -> Result<Status, Stop> {
    change(c, types, |t, _| t.mask = u64::MAX)
}

/// A physics type (`fp[-2]`) made ethereal or solid (`fp[-3]`, any word but 0 for ethereal);
/// answers whether it is a physics type (`fp[-4]`).
// D2-08554
fn types_set_property(c: &mut Call<'_>) -> Result<Status, Stop> {
    let (property, on) = (c.arg(0)?, yes(c, 1)?);
    answer(c, -4, types, |t, list| {
        u32::from(t.set_property(list, property, on))
    })
}

/// A physics type (`fp[-2]`) made ethereal; answers whether it is one (`fp[-3]`).
// D2-08555
fn types_set_property_ethereal(c: &mut Call<'_>) -> Result<Status, Stop> {
    let property = c.arg(0)?;
    answer(c, -3, types, |t, list| {
        u32::from(t.set_property(list, property, true))
    })
}

/// A physics type (`fp[-2]`) made solid; answers whether it is one (`fp[-3]`).
// D2-08556
fn types_set_property_solid(c: &mut Call<'_>) -> Result<Status, Stop> {
    let property = c.arg(0)?;
    answer(c, -3, types, |t, list| {
        u32::from(t.set_property(list, property, false))
    })
}

// D2-08551
fn types_solid_only_to_environment(c: &mut Call<'_>) -> Result<Status, Stop> {
    change(c, types, |t, list| t.set_solid_only_to_environment(list))
}

/// Ethereal to the water layer when `fp[-2]` is any word but 0 (the water layer itself ends solid
/// either way).
// D2-08550
fn types_ethereal_to_water(c: &mut Call<'_>) -> Result<Status, Stop> {
    let on = yes(c, 0)?;
    change(c, types, |t, list| t.set_ethereal_to_water_layer(list, on))
}

// A list of objects.

// D2-08557
fn objects_to_ethereal(c: &mut Call<'_>) -> Result<Status, Stop> {
    change(c, objects, |o, _| o.set_to_ethereal())
}

// D2-08558
fn objects_to_solid(c: &mut Call<'_>) -> Result<Status, Stop> {
    change(c, objects, |o, _| o.set_to_solid())
}

/// Passes through an object (`fp[-3..-2]`); answers 1 (`fp[-4]`).
// D2-08559
fn objects_add_ethereal(c: &mut Call<'_>) -> Result<Status, Stop> {
    let id = object_id(c)?;
    answer(c, -4, objects, |o, _| {
        o.add_to_ethereal_list(id);
        1
    })
}

/// Does not pass through an object (`fp[-3..-2]`); answers 1 (`fp[-4]`).
// D2-08560
fn objects_add_solid(c: &mut Call<'_>) -> Result<Status, Stop> {
    let id = object_id(c)?;
    answer(c, -4, objects, |o, _| {
        o.add_to_solid_list(id);
        1
    })
}
