//! What the scripts set on an object's body, kept for the scene to apply and read back as the
//! body holds it; and the scripts told when one of an object's qualities changes.
//!
//! - **The collision state** a body holds: its own physics types, the physics types it passes
//!   through when placed and when moving, and the objects it passes through placed and moving. A
//!   body starts solid to every physics type and every object, of no physics type. Setting it
//!   takes the parts the given settings mark set (the object lists brought up to the given ones;
//!   its own physics types as they are) and the least impact speeds that are not zero; it answers
//!   0. Reading it fills the given settings with the body's own physics types, its placement and
//!   its moving physics types, each marked set, and answers 1.
//! - **Drawn or not**: a body's no-draw flag, set or cleared; it answers 1.
//! - **Its scale**, held between a tenth and ten times, and **whether it slides along edges**, as
//!   the object's repository sets them with its qualities.
//!
//! An object with no body (none in the world) answers 0 and keeps nothing.
//!
//! **A quality changed** ([`Scripts::stat_changed`]): each quality the server sets on an object,
//! whoever holds it (the object's description or its own qualities), is then told to the object's
//! scripts with the quality's kind and number. The objects' scripts keep their bodies' collision
//! state in step with the quality words that hold its masks this way.

use std::collections::HashMap;

use auberean_glue::missile::PhysicsValue;
use auberean_physics::collision::{settings, CollisionSettings};
use auberean_protocol::ServerMessage;
use auberean_script::native::{Call, NativeFn, Status, Stop};
use auberean_script::native_object::{Engine, NativeObject};
use auberean_script::value::{tag, NativeId, NULL};

use crate::host::{host, receiver_object, CallResult, ClientHost, Scripts};
use crate::skills::Asker;

/// What the scripts set on one object's body.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BodyState {
    pub collision: CollisionSettings,
    pub no_draw: bool,
    /// The scale the body was given (between a tenth and ten), when one was.
    pub scale: Option<f32>,
    /// Whether the body slides along edges, when it was told.
    pub edge_slide: Option<bool>,
    /// Whether the body is undetectable (neither colliding nor reporting collisions), when it
    /// was told.
    pub undetectable: Option<bool>,
    /// The jump scale it was given (none below zero), when it was.
    pub jump_scale: Option<f32>,
    /// Whether it places marker objects along its way.
    pub place_markers: bool,
}

/// The bodies' states, by object.
pub type Bodies = HashMap<u64, BodyState>;

/// What a body says of itself when the scripts ask, as a front end last read it: the element it
/// is in, and the motion it was last asked for (none without a motion interpreter); its speed,
/// acceleration and jump scales; its radius and height at its scale; its mode and the mode it is
/// going to; the motion the server last asked of it, when it did.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BodyReading {
    pub element: u32,
    pub last_motion: [f32; 3],
    pub velocity_scale: f32,
    pub acceleration_scale: f32,
    pub jump_scale: f32,
    pub radius: f32,
    pub height: f32,
    pub mode: u32,
    pub desired_mode: u32,
    pub remote_motion: Option<[f32; 3]>,
}

/// The bodies' readings, by object.
pub type Readings = HashMap<u64, BodyReading>;

/// What the cells a body has been in made of it, as a front end last read it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BodyMaterial {
    /// The cell it was read in: it changes only when the body changes cell.
    pub cell: u32,
    pub properties: auberean_assets::property::PropertyCollection,
}

/// The bodies' materials, by object: a body that has stood in no cell has none.
pub type Materials = HashMap<u64, BodyMaterial>;

/// The interface's function a changed quality is told to: its kind, then its number.
pub const STAT_CHANGED: u32 = 0x007F << 16 | 0x0044;

/// The kinds of quality, as the scripts are told them.
pub mod stat {
    pub const BOOL: u32 = 1;
    pub const DATA_ID: u32 = 2;
    pub const FLOAT: u32 = 3;
    pub const INT: u32 = 4;
    pub const INSTANCE_ID: u32 = 5;
    pub const POSITION: u32 = 6;
    pub const STRING: u32 = 7;
    pub const TIMESTAMP: u32 = 8;
    pub const STRING_INFO: u32 = 10;
    pub const LONG_INT: u32 = 12;
}

/// The object, the kind and the number of the quality a server's update sets: the player's for a
/// private update, the named object's for a visual one. `None` for any other message.
#[must_use]
pub fn stat_of(m: &ServerMessage, player: Option<u64>) -> Option<(u64, u32, u32)> {
    use ServerMessage as S;
    let (object, kind, quality) = match m {
        S::UpdateInt(u) => (player, stat::INT, u.quality),
        S::UpdateIntVisual(u) => (Some(u.object.id), stat::INT, u.quality),
        S::UpdateBool(u) => (player, stat::BOOL, u.quality),
        S::UpdateBoolVisual(u) => (Some(u.object.id), stat::BOOL, u.quality),
        S::UpdateFloat(u) => (player, stat::FLOAT, u.quality),
        S::UpdateFloatVisual(u) => (Some(u.object.id), stat::FLOAT, u.quality),
        S::UpdateString(u) => (player, stat::STRING, u.quality),
        S::UpdateStringVisual(u) => (Some(u.object.id), stat::STRING, u.quality),
        S::UpdateDataId(u) => (player, stat::DATA_ID, u.quality),
        S::UpdateDataIdVisual(u) => (Some(u.object.id), stat::DATA_ID, u.quality),
        S::UpdateInstanceId(u) => (player, stat::INSTANCE_ID, u.quality),
        S::UpdateInstanceIdVisual(u) => (Some(u.object.id), stat::INSTANCE_ID, u.quality),
        S::UpdatePlacement(u) => (player, stat::POSITION, u.quality),
        S::UpdatePositionVisual(u) => (Some(u.object.id), stat::POSITION, u.quality),
        S::UpdateTimestamp(u) => (player, stat::TIMESTAMP, u.quality),
        S::UpdateTimestampVisual(u) => (Some(u.object.id), stat::TIMESTAMP, u.quality),
        S::UpdateStringInfo(u) => (player, stat::STRING_INFO, u.quality),
        S::UpdateStringInfoVisual(u) => (Some(u.object.id), stat::STRING_INFO, u.quality),
        S::UpdateLongInt(u) => (player, stat::LONG_INT, u.quality),
        S::UpdateLongIntVisual(u) => (Some(u.object.id), stat::LONG_INT, u.quality),
        _ => return None,
    };
    Some((object?, kind, quality))
}

impl Scripts {
    /// Tell an object's scripts one of its qualities changed: the quality's number, then its kind,
    /// pushed in that order. `None` when the object is not here or has no scripts.
    // D2-08280, D2-08281, D2-08282, D2-08283, D2-08284, D2-08285, D2-08286, D2-08287, D2-08288
    // D2-08289, D2-08061, D2-04687
    pub fn stat_changed(&mut self, object: u64, kind: u32, quality: u32) -> Option<CallResult> {
        let mut a = Asker {
            s: self,
            incomplete: Vec::new(),
        };
        let args = [(quality, tag::MIXED), (kind, tag::MIXED)];
        a.ask_object(object, None, STAT_CHANGED, &[], &args, false)
            .map(|answer| answer.result)
    }
}

/// Whether an object has a body: whatever the client was sent, in the world or in a pack.
fn has_body(c: &mut Call<'_>, object: u64) -> Result<bool, Stop> {
    Ok(holds_body(host(c)?, object))
}

pub(crate) fn holds_body(h: &ClientHost, object: u64) -> bool {
    h.model.player == Some(object) || crate::physics_object::has_physics_object(h, object)
}

/// Scale an object's body, held between a tenth and ten times; whether it has a body.
// D2-08195
pub(crate) fn set_scale(h: &mut ClientHost, object: u64, scale: f32) -> bool {
    if !holds_body(h, object) {
        return false;
    }
    // Not a clamp: a scale that is not a number becomes a tenth.
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    let scale = if !(0.1 <= scale) {
        0.1
    } else if 10.0 < scale {
        10.0
    } else {
        scale
    };
    h.bodies.entry(object).or_default().scale = Some(scale);
    true
}

/// Tell an object's body whether it slides along edges; whether it has a body.
// D2-08193
pub(crate) fn set_edge_slide(h: &mut ClientHost, object: u64, on: bool) -> bool {
    if !holds_body(h, object) {
        return false;
    }
    h.bodies.entry(object).or_default().edge_slide = Some(on);
    true
}

/// The collision settings a reference names, as the engine keeps them; a null one stops the run.
fn settings_at(c: &Call<'_>, r: u32) -> Result<CollisionSettings, Stop> {
    if r == NULL {
        return Err(Stop::NullReference);
    }
    match PhysicsValue::of(c.vm.heap.native(r)?) {
        Some(PhysicsValue::Collision(s)) => Ok(*s),
        _ => Err(c.fail("not collision settings")),
    }
}

/// Set the body's collision state from the settings the argument names (`fp[-2]`); answers 0
/// (`fp[-3]`).
// D2-04743, D2-08126, D2-05925, D2-06764
fn set_collision_state(c: &mut Call<'_>) -> Result<Status, Stop> {
    if c.receiver()? == NULL {
        return Ok(Status::Crash);
    }
    let object = receiver_object(c)?;
    let given = settings_at(c, c.arg(0)?)?;
    if has_body(c, object)? {
        let body = host(c)?.bodies.entry(object).or_default();
        take(&mut body.collision, &given);
    }
    c.write(-3, 0, tag::MIXED)?;
    Ok(Status::Success)
}

/// What a body takes of settings it is given: the parts they mark set, and the least impact
/// speeds that are not zero.
fn take(body: &mut CollisionSettings, s: &CollisionSettings) {
    if s.has(settings::MOVEMENT_PHYSICS) {
        body.movement_physics = s.movement_physics;
        body.set_bit(settings::MOVEMENT_PHYSICS);
    }
    if s.has(settings::MOVEMENT_OBJECTS) {
        s.movement_objects.update(&mut body.movement_objects);
        body.set_bit(settings::MOVEMENT_OBJECTS);
    }
    if s.has(settings::PLACEMENT_PHYSICS) {
        body.placement_physics = s.placement_physics;
        body.set_bit(settings::PLACEMENT_PHYSICS);
    }
    if s.has(settings::PLACEMENT_OBJECTS) {
        s.placement_objects.update(&mut body.placement_objects);
        body.set_bit(settings::PLACEMENT_OBJECTS);
    }
    if s.has(settings::PHYSICS_TYPE) {
        body.physics_type = s.physics_type;
    }
    if s.min_impact_xy != 0.0 {
        body.min_impact_xy = s.min_impact_xy;
    }
    if s.min_impact_z != 0.0 {
        body.min_impact_z = s.min_impact_z;
    }
}

/// Fill the settings the slot an out reference (`fp[-2]`) reaches holds with the body's own
/// physics types, its placement and its moving physics types, each marked set; answers 1, or 0
/// with no body (`fp[-3]`).
// D2-04769, D2-08127, D2-05926, D2-06719
fn get_collision_state(c: &mut Call<'_>) -> Result<Status, Stop> {
    if c.receiver()? == NULL {
        return Ok(Status::Crash);
    }
    let object = receiver_object(c)?;
    let out = c.arg(0)?;
    let prog = std::sync::Arc::clone(c.vm.program());
    let (target, _) = c.vm.read_slot(c.host, c.cx, out, 0, &prog)?;
    let mut s = settings_at(c, target)?;
    let answer = if has_body(c, object)? {
        let body = host(c)?
            .bodies
            .get(&object)
            .map(|b| b.collision.clone())
            .unwrap_or_default();
        s.physics_type = body.physics_type;
        s.set_bit(settings::PHYSICS_TYPE);
        s.placement_physics = body.placement_physics;
        s.set_bit(settings::PLACEMENT_PHYSICS);
        s.movement_physics = body.movement_physics;
        s.set_bit(settings::MOVEMENT_PHYSICS);
        *c.vm.heap.native_mut(target)? =
            NativeObject::Engine(Engine::new(PhysicsValue::Collision(Box::new(s))));
        1
    } else {
        0
    };
    c.write(-3, answer, tag::MIXED)?;
    Ok(Status::Success)
}

/// Draw the body or not (`fp[-2]`, any word but 0 for not); answers 1, or 0 with no body
/// (`fp[-3]`).
// D2-04747, D2-08188, D2-06017
fn set_no_draw(c: &mut Call<'_>) -> Result<Status, Stop> {
    if c.receiver()? == NULL {
        return Ok(Status::Crash);
    }
    let object = receiver_object(c)?;
    let on = c.arg(0)? != 0;
    let answer = if has_body(c, object)? {
        host(c)?.bodies.entry(object).or_default().no_draw = on;
        1
    } else {
        0
    };
    c.write(-3, answer, tag::MIXED)?;
    Ok(Status::Success)
}

/// Make the body undetectable or detectable again (`fp[-2]`, any word but 0 for undetectable);
/// answers 1, or 0 with no body (`fp[-3]`).
// D2-04748, D2-08190
fn set_undetectable(c: &mut Call<'_>) -> Result<Status, Stop> {
    if c.receiver()? == NULL {
        return Ok(Status::Crash);
    }
    let object = receiver_object(c)?;
    let on = c.arg(0)? != 0;
    let answer = if has_body(c, object)? {
        host(c)?.bodies.entry(object).or_default().undetectable = Some(on);
        1
    } else {
        0
    };
    c.write(-3, answer, tag::MIXED)?;
    Ok(Status::Success)
}

/// Give the body a jump scale (`fp[-2]`, a float): the slowest landing it reports grows with it;
/// one below zero is not taken.
// D2-04691, D2-08186
fn set_jump_scale(c: &mut Call<'_>) -> Result<Status, Stop> {
    if c.receiver()? == NULL {
        return Ok(Status::Crash);
    }
    let object = receiver_object(c)?;
    let scale = f32::from_bits(c.arg(0)?);
    if has_body(c, object)? && scale >= 0.0 {
        host(c)?.bodies.entry(object).or_default().jump_scale = Some(scale);
    }
    Ok(Status::Success)
}

/// The element the body is in, into `fp[-2]`; 0 with no body.
// D2-04714, D2-08162
fn get_element(c: &mut Call<'_>) -> Result<Status, Stop> {
    if c.receiver()? == NULL {
        return Ok(Status::Crash);
    }
    let object = receiver_object(c)?;
    let element = if has_body(c, object)? {
        host(c)?.readings.get(&object).map_or(0, |r| r.element)
    } else {
        0
    };
    c.write(-2, element, tag::MIXED)?;
    Ok(Status::Success)
}

/// A new vector of the motion the body was last asked for, into `fp[-2]`; nothing with no body.
// D2-04760, D2-08167
fn get_last_do_motion_values(c: &mut Call<'_>) -> Result<Status, Stop> {
    if c.receiver()? == NULL {
        return Ok(Status::Crash);
    }
    let object = receiver_object(c)?;
    let v = if has_body(c, object)? {
        host(c)?
            .readings
            .get(&object)
            .map_or([0.0; 3], |r| r.last_motion)
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

/// The body places marker objects along its way, or stops (taking them away); whether it now
/// places them, into `fp[-2]`; 0 with no body.
// D2-04754, D2-08208, D2-06361, D2-06092
fn toggle_place_marker_objects(c: &mut Call<'_>) -> Result<Status, Stop> {
    if c.receiver()? == NULL {
        return Ok(Status::Crash);
    }
    let object = receiver_object(c)?;
    let placing = if has_body(c, object)? {
        let b = host(c)?.bodies.entry(object).or_default();
        b.place_markers = !b.place_markers;
        u32::from(b.place_markers)
    } else {
        0
    };
    c.write(-2, placing, tag::MIXED)?;
    Ok(Status::Success)
}

/// An object's description set again (the object sent anew in the incarnation the client has):
/// with a body, it is not drawn when its no-draw flag (yes-or-no 10) is set, and drawn again when
/// neither that nor its kept-hidden flag (yes-or-no 117 hexadecimal) is set; either flag missing
/// reads no.
// D2-00627
pub fn described_again(h: &mut ClientHost, object: u64) {
    if !holds_body(h, object) {
        return;
    }
    let flag = |h: &mut ClientHost, key| {
        crate::repository::answer(h, &crate::repository::BOOL, object, key).unwrap_or(0) != 0
    };
    if flag(h, 10) {
        h.bodies.entry(object).or_default().no_draw = true;
    } else if !flag(h, 0x117) {
        h.bodies.entry(object).or_default().no_draw = false;
    }
}

/// Whether object `object` has a body, as a physics object's setter answers: 1, or 0 with none.
pub(crate) fn body_answer(h: &ClientHost, object: u64) -> u32 {
    u32::from(holds_body(h, object))
}

/// The body natives.
pub(crate) fn natives() -> impl Iterator<Item = (NativeId, NativeFn)> {
    const PHYSICS_OBJECT: u32 = 0x80;
    [
        (
            NativeId::new(PHYSICS_OBJECT, 0x0A),
            set_collision_state as NativeFn,
        ),
        (NativeId::new(PHYSICS_OBJECT, 0x07), set_jump_scale),
        (NativeId::new(PHYSICS_OBJECT, 0x0B), get_collision_state),
        (NativeId::new(PHYSICS_OBJECT, 0x1A), get_element),
        (
            NativeId::new(PHYSICS_OBJECT, 0x2B),
            toggle_place_marker_objects,
        ),
        (NativeId::new(PHYSICS_OBJECT, 0x52), set_undetectable),
        (NativeId::new(PHYSICS_OBJECT, 0x57), set_no_draw),
        (
            NativeId::new(PHYSICS_OBJECT, 0x62),
            get_last_do_motion_values,
        ),
    ]
    .into_iter()
}
