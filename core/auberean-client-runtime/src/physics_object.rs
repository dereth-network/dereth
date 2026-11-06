//! An object's physics object as the scripts reach it: what they ask of its body, and what they
//! read back from it.
//!
//! Every object the client was sent has a physics object; a script asking one of an object that is
//! not here gets the answer of none. Asked of a null object, each of these crashes the call.
//!
//! **What the scripts read** comes from the body as a front end last read it
//! ([`crate::body::BodyReading`]): its speed, acceleration and jump scales, its radius and height
//! (its own, or those of the object it is held by), its mode and the mode it is going to, and the
//! motion the server last asked of it. With no physics object each reads 0 (a zero vector for the
//! motion). Whether it is in the world is whether it is placed (it has a place and hangs from
//! nothing); whether
//! it is undetectable, or its movement or its animation frozen, is as the scripts last set it
//! (undetectable reads 1 with no physics object).
//!
//! **What the scripts ask** goes, in order, to the host's body requests ([`crate::host::ClientHost`])
//! for a front end to carry out on the body: a speed or acceleration scale (a float, a signalling
//! not-a-number quieted on the way), drawing it cloaked, reporting or ignoring collisions, turning to
//! look at an object, a push (in the world's frame or the body's own, or toward an object), stopping
//! a layer of its animation. Each answers 1 (stopping a layer 0) when the object has a physics
//! object, and 0 (stopping a layer 8) when it has none; the scales answer nothing.

use auberean_script::native::{Call, NativeFn, Status, Stop};
use auberean_script::native_object::NativeObject;
use auberean_script::value::{join, tag, NativeId, NULL};

use crate::host::{host, receiver_object, ClientHost};

/// What the scripts asked of an object's body, for a front end to carry out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BodyRequest {
    /// Scale its speed.
    VelocityScale { object: u64, scale: f32 },
    /// Scale its acceleration.
    AccelerationScale { object: u64, scale: f32 },
    /// Draw it cloaked, or not.
    Cloaked { object: u64, on: bool },
    /// Report its collisions, or not.
    ReportCollisions { object: u64, on: bool },
    /// Ignore collisions, or not.
    IgnoreCollisions { object: u64, on: bool },
    /// Turn to look at another object.
    LookAt { object: u64, target: u64 },
    /// A push: a vector in the world's frame, or in the body's own when `local`, and a word the
    /// body takes with it.
    Impulse {
        object: u64,
        vector: [f32; 3],
        local: bool,
        flags: u32,
    },
    /// A push toward another object: its speed, a word the body takes with it, and a float.
    TargetedImpulse {
        object: u64,
        target: u64,
        speed: f32,
        flags: u32,
        extra: f32,
    },
    /// Stop a layer of its animation.
    EndLayer { object: u64, layer: u32 },
}

/// The answer of a call on an object with no physics object to a request whose body answers by
/// what it did (stopping a layer).
const NO_PHYSICS_OBJECT: u32 = 8;

/// Whether an object has a physics object: whether the client was sent it.
#[must_use]
pub fn has_physics_object(h: &ClientHost, object: u64) -> bool {
    h.model.object(object).is_some()
}

/// The object whose radius and height an object's top level reads: the object it is held by, or
/// itself.
// D2-05967
#[must_use]
pub fn top_level(h: &ClientHost, object: u64) -> u64 {
    h.model
        .object(object)
        .and_then(|o| o.parent)
        .map_or(object, |(p, _)| p)
}

/// The body's reading of an object, or a zero one.
fn reading(h: &ClientHost, object: u64) -> crate::body::BodyReading {
    h.readings.get(&object).copied().unwrap_or_default()
}

/// Whether an object is in the world: placed there (it has a place and hangs from nothing).
// D2-08178
#[must_use]
pub fn in_world(h: &ClientHost, object: u64) -> bool {
    crate::lifecycle::placeable(&h.model, object)
}

/// Whether an object is undetectable as the scripts last set it; with no physics object, it is.
// D2-08203
#[must_use]
pub fn undetectable(h: &ClientHost, object: u64) -> bool {
    if !has_physics_object(h, object) {
        return true;
    }
    h.bodies
        .get(&object)
        .and_then(|b| b.undetectable)
        .unwrap_or(false)
}

/// Whether an object's movement (`movement`) or its animation is frozen as the scripts last set
/// it; no with no physics object.
// D2-08204, D2-08206
#[must_use]
pub fn frozen(h: &ClientHost, object: u64, movement: bool) -> bool {
    if !has_physics_object(h, object) {
        return false;
    }
    h.frozen
        .get(&object)
        .is_some_and(|f| if movement { f.0 } else { f.1 })
}

/// A request to an object's body, kept when it has a physics object; whether it has.
pub fn request(h: &mut ClientHost, object: u64, r: BodyRequest) -> bool {
    if !has_physics_object(h, object) {
        return false;
    }
    h.body_requests.push(r);
    true
}

/// A float as the engine's floating-point unit carries it: a signalling not-a-number quieted.
fn through_fpu(bits: u32) -> f32 {
    if f32::from_bits(bits).is_nan() {
        f32::from_bits(bits | 1 << 22)
    } else {
        f32::from_bits(bits)
    }
}

/// The receiver's object, or `None` when the receiver is the null reference.
fn object(c: &mut Call<'_>) -> Result<Option<u64>, Stop> {
    if c.receiver()? == NULL {
        return Ok(None);
    }
    receiver_object(c).map(Some)
}

/// A word read of the receiver's object into `fp[-2]`.
fn read_word(c: &mut Call<'_>, f: fn(&ClientHost, u64) -> u32) -> Result<Status, Stop> {
    let Some(id) = object(c)? else {
        return Ok(Status::Crash);
    };
    let v = f(host(c)?, id);
    c.write(-2, v, tag::MIXED)?;
    Ok(Status::Success)
}

/// A float read of the receiver's object's body into `fp[-2]`; 0 with no physics object.
fn read_float(c: &mut Call<'_>, f: fn(&ClientHost, u64) -> f32) -> Result<Status, Stop> {
    let Some(id) = object(c)? else {
        return Ok(Status::Crash);
    };
    let h = host(c)?;
    let v = if has_physics_object(h, id) {
        f(h, id).to_bits()
    } else {
        0
    };
    c.write(-2, v, tag::MIXED)?;
    Ok(Status::Success)
}

/// A request with a yes-or-no (`fp[-2]`), answered in `fp[-3]`.
fn ask_flag(c: &mut Call<'_>, make: fn(u64, bool) -> BodyRequest) -> Result<Status, Stop> {
    let Some(id) = object(c)? else {
        return Ok(Status::Crash);
    };
    let on = c.arg(0)? != 0;
    let done = request(host(c)?, id, make(id, on));
    c.write(-3, u32::from(done), tag::MIXED)?;
    Ok(Status::Success)
}

/// A scale (`fp[-2]`, a float) asked of the body; no answer.
fn ask_scale(c: &mut Call<'_>, make: fn(u64, f32) -> BodyRequest) -> Result<Status, Stop> {
    let Some(id) = object(c)? else {
        return Ok(Status::Crash);
    };
    let scale = through_fpu(c.arg(0)?);
    request(host(c)?, id, make(id, scale));
    Ok(Status::Success)
}

// D2-04692, D2-08182
fn set_velocity_scale(c: &mut Call<'_>) -> Result<Status, Stop> {
    ask_scale(c, |object, scale| BodyRequest::VelocityScale {
        object,
        scale,
    })
}

// D2-04690, D2-08184
fn set_acceleration_scale(c: &mut Call<'_>) -> Result<Status, Stop> {
    ask_scale(c, |object, scale| BodyRequest::AccelerationScale {
        object,
        scale,
    })
}

// D2-04742, D2-08189
fn set_cloaked(c: &mut Call<'_>) -> Result<Status, Stop> {
    ask_flag(c, |object, on| BodyRequest::Cloaked { object, on })
}

// D2-04756, D2-08191
fn set_report_collisions(c: &mut Call<'_>) -> Result<Status, Stop> {
    ask_flag(c, |object, on| BodyRequest::ReportCollisions { object, on })
}

// D2-04755, D2-08192
fn set_ignore_collisions(c: &mut Call<'_>) -> Result<Status, Stop> {
    ask_flag(c, |object, on| BodyRequest::IgnoreCollisions { object, on })
}

/// Look at the object whose id is `fp[-3..-2]` (low word first); answered in `fp[-4]`.
// D2-04732, D2-08160
fn look_at(c: &mut Call<'_>) -> Result<Status, Stop> {
    let Some(id) = object(c)? else {
        return Ok(Status::Crash);
    };
    let target = join(c.cx.at(-3)?, c.cx.at(-2)?);
    let done = request(host(c)?, id, BodyRequest::LookAt { object: id, target });
    c.write(-4, u32::from(done), tag::MIXED)?;
    Ok(Status::Success)
}

/// A push by the vector `fp[-2]` names and the word `fp[-3]`; answered in `fp[-4]`.
fn push(c: &mut Call<'_>, local: bool) -> Result<Status, Stop> {
    let Some(id) = object(c)? else {
        return Ok(Status::Crash);
    };
    let r = c.arg(0)?;
    let vector = match c.vm.heap.native(r)? {
        NativeObject::Vector(v) => v.map(f32::from_bits),
        _ => return Err(c.fail("the push is not a vector")),
    };
    let flags = c.arg(1)?;
    let done = request(
        host(c)?,
        id,
        BodyRequest::Impulse {
            object: id,
            vector,
            local,
            flags,
        },
    );
    c.write(-4, u32::from(done), tag::MIXED)?;
    Ok(Status::Success)
}

// D2-04694, D2-08213
fn add_impulse(c: &mut Call<'_>) -> Result<Status, Stop> {
    push(c, false)
}

// D2-04695, D2-08214
fn add_impulse_local(c: &mut Call<'_>) -> Result<Status, Stop> {
    push(c, true)
}

/// A push toward the object whose id is `fp[-3..-2]` (low word first): the speed `fp[-4]`, the word
/// `fp[-5]`, the float `fp[-6]`; answered in `fp[-7]`.
// D2-04696, D2-08161
fn add_targeted_impulse(c: &mut Call<'_>) -> Result<Status, Stop> {
    let Some(id) = object(c)? else {
        return Ok(Status::Crash);
    };
    let target = join(c.cx.at(-3)?, c.cx.at(-2)?);
    let speed = f32::from_bits(c.cx.at(-4)?);
    let flags = c.cx.at(-5)?;
    let extra = f32::from_bits(c.cx.at(-6)?);
    let done = request(
        host(c)?,
        id,
        BodyRequest::TargetedImpulse {
            object: id,
            target,
            speed,
            flags,
            extra,
        },
    );
    c.write(-7, u32::from(done), tag::MIXED)?;
    Ok(Status::Success)
}

/// Stop the layer `fp[-2]` of the body's animation; answered in `fp[-3]`: 0, or 8 with no physics
/// object.
// D2-04752, D2-08156
fn stop_layer(c: &mut Call<'_>) -> Result<Status, Stop> {
    let Some(id) = object(c)? else {
        return Ok(Status::Crash);
    };
    let layer = c.arg(0)?;
    let done = request(host(c)?, id, BodyRequest::EndLayer { object: id, layer });
    c.write(-3, if done { 0 } else { NO_PHYSICS_OBJECT }, tag::MIXED)?;
    Ok(Status::Success)
}

// D2-04721, D2-08181
fn get_velocity_scale(c: &mut Call<'_>) -> Result<Status, Stop> {
    read_float(c, |h, id| reading(h, id).velocity_scale)
}

// D2-04712, D2-08183
fn get_acceleration_scale(c: &mut Call<'_>) -> Result<Status, Stop> {
    read_float(c, |h, id| reading(h, id).acceleration_scale)
}

// D2-04716, D2-08185
fn get_jump_scale(c: &mut Call<'_>) -> Result<Status, Stop> {
    read_float(c, |h, id| reading(h, id).jump_scale)
}

// D2-04718, D2-08199
fn get_radius(c: &mut Call<'_>) -> Result<Status, Stop> {
    read_float(c, |h, id| reading(h, id).radius)
}

// D2-04715, D2-08198
fn get_height(c: &mut Call<'_>) -> Result<Status, Stop> {
    read_float(c, |h, id| reading(h, id).height)
}

// D2-04720, D2-08200
fn get_top_level_radius(c: &mut Call<'_>) -> Result<Status, Stop> {
    read_float(c, |h, id| reading(h, top_level(h, id)).radius)
}

// D2-04719, D2-08201
fn get_top_level_height(c: &mut Call<'_>) -> Result<Status, Stop> {
    read_float(c, |h, id| reading(h, top_level(h, id)).height)
}

// D2-04717, D2-08164
fn get_mode(c: &mut Call<'_>) -> Result<Status, Stop> {
    read_word(c, |h, id| {
        if has_physics_object(h, id) {
            reading(h, id).mode
        } else {
            0
        }
    })
}

// D2-04713, D2-08163
fn get_desired_mode(c: &mut Call<'_>) -> Result<Status, Stop> {
    read_word(c, |h, id| {
        if has_physics_object(h, id) {
            reading(h, id).desired_mode
        } else {
            0
        }
    })
}

// D2-04723, D2-08168
fn has_remote_do_motion_values(c: &mut Call<'_>) -> Result<Status, Stop> {
    read_word(c, |h, id| {
        u32::from(has_physics_object(h, id) && reading(h, id).remote_motion.is_some())
    })
}

/// A new vector of the motion the server last asked of the body, into `fp[-2]`; a zero one with
/// no physics object or none asked.
// D2-04762, D2-08169
fn get_remote_do_motion_values(c: &mut Call<'_>) -> Result<Status, Stop> {
    let Some(id) = object(c)? else {
        return Ok(Status::Crash);
    };
    let h = host(c)?;
    let v = if has_physics_object(h, id) {
        reading(h, id).remote_motion.unwrap_or_default()
    } else {
        [0.0; 3]
    };
    let r = c.vm.heap.add_native(
        auberean_script::native_object::native_type::VECTOR,
        NativeObject::Vector(v.map(f32::to_bits)),
    );
    c.write(-2, r, tag::REFERENCE)?;
    Ok(Status::Success)
}

// D2-04727
fn is_in_world(c: &mut Call<'_>) -> Result<Status, Stop> {
    read_word(c, |h, id| u32::from(in_world(h, id)))
}

// D2-04730
fn is_undetectable(c: &mut Call<'_>) -> Result<Status, Stop> {
    read_word(c, |h, id| u32::from(undetectable(h, id)))
}

// D2-04728
fn is_movement_frozen(c: &mut Call<'_>) -> Result<Status, Stop> {
    read_word(c, |h, id| u32::from(frozen(h, id, true)))
}

// D2-04725
fn is_animation_frozen(c: &mut Call<'_>) -> Result<Status, Stop> {
    read_word(c, |h, id| u32::from(frozen(h, id, false)))
}

/// What the cells the object's body has been in made of it, into the collection at `fp[-2]`;
/// whether it has any (`fp[-3]`): a body that has stood in no cell, or none at all, has not.
// D2-04770, D2-08196
fn current_properties(c: &mut Call<'_>) -> Result<Status, Stop> {
    let id = receiver_object(c)?;
    let collection = properties_of(c, id)?;
    let out = c.cx.at(-2)?;
    let found = collection.is_some();
    if let Some(col) = collection {
        auberean_glue::properties::give_collection(c, out, col)?;
    }
    c.write(-3, u32::from(found), tag::MIXED)?;
    Ok(Status::Success)
}

/// What the cells the object's body has been in made of it: none for a body that has stood in no
/// cell, or with no body.
fn properties_of(
    c: &mut Call<'_>,
    id: u64,
) -> Result<Option<auberean_assets::property::PropertyCollection>, Stop> {
    Ok(host(c)?.materials.get(&id).map(|m| m.properties.clone()))
}

/// The one property of an object named at `fp[-2]`, into the property the reference at `fp[-3]`
/// reaches; whether it has it (`fp[-4]`).
// D2-04771
fn current_property(c: &mut Call<'_>) -> Result<Status, Stop> {
    let id = receiver_object(c)?;
    let name = c.cx.at(-2)?;
    let found = properties_of(c, id)?.and_then(|all| all.iter().find(|p| p.id == name).cloned());
    auberean_glue::properties::give_property(c, found)
}

/// The physics object natives this module binds.
pub(crate) fn natives() -> impl Iterator<Item = (NativeId, NativeFn)> {
    const PHYSICS_OBJECT: u32 = 0x80;
    let n: [(u32, NativeFn); 27] = [
        (0x4A, current_properties),
        (0x4B, current_property),
        (0x03, set_velocity_scale),
        (0x04, get_velocity_scale),
        (0x05, set_acceleration_scale),
        (0x06, get_acceleration_scale),
        (0x08, get_jump_scale),
        (0x0C, set_report_collisions),
        (0x0D, set_ignore_collisions),
        (0x18, get_desired_mode),
        (0x19, get_mode),
        (0x24, add_impulse),
        (0x25, add_impulse_local),
        (0x26, add_targeted_impulse),
        (0x28, stop_layer),
        (0x2A, look_at),
        (0x37, is_in_world),
        (0x4D, get_radius),
        (0x4E, get_height),
        (0x4F, get_top_level_radius),
        (0x50, get_top_level_height),
        (0x51, is_undetectable),
        (0x53, is_movement_frozen),
        (0x55, is_animation_frozen),
        (0x58, set_cloaked),
        (0x63, has_remote_do_motion_values),
        (0x64, get_remote_do_motion_values),
    ];
    n.into_iter()
        .map(|(number, f)| (NativeId::new(PHYSICS_OBJECT, number), f))
}
