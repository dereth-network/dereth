//! How a behavior is to be played: the setters scripts fill a behavior's parameters with before
//! asking an object to play it.
//!
//! A word setter stores its argument; a 64-bit one (a target, the camera's owner) its two slots,
//! low first; the time scale goes through the floating-point unit, which quiets a signalling
//! not-a-number; a switch (moving cancels it, an early call back, the camera parented, targeted,
//! held or restored, children fading, actions locked, idle left alone, passed on to the parts)
//! is turned on and takes no argument. None answers anything but the behavior's id, read back.

use auberean_script::native::{Call, NativeFn, NativeTable, Status, Stop};
use auberean_script::native_object::{BehaviorParams, NativeObject};
use auberean_script::value::{join, tag, NULL};

const BEHAVIOR_PARAMS: u32 = 0x17;

/// A field of the parameters.
#[derive(Clone, Copy)]
enum Field {
    Behavior,
    TimeScale,
    Mode,
    HoldCycles,
    MoveToCancels,
    EarlyCallback,
    CameraParented,
    CameraTargeted,
    CameraHold,
    CameraRestore,
    CameraTarget,
    CameraBehavior,
    FadeChildren,
    ActionsLocked,
    SendEvent,
    FxScript,
    Target,
    VisualToClone,
    LeaveIdleAlone,
    Propagate,
    Fx,
    WeenieEmote,
    ClonedAppearance,
}

const FIELDS: [Field; 23] = [
    Field::Behavior,
    Field::TimeScale,
    Field::Mode,
    Field::HoldCycles,
    Field::MoveToCancels,
    Field::EarlyCallback,
    Field::CameraParented,
    Field::CameraTargeted,
    Field::CameraHold,
    Field::CameraRestore,
    Field::CameraTarget,
    Field::CameraBehavior,
    Field::FadeChildren,
    Field::ActionsLocked,
    Field::SendEvent,
    Field::FxScript,
    Field::Target,
    Field::VisualToClone,
    Field::LeaveIdleAlone,
    Field::Propagate,
    Field::Fx,
    Field::WeenieEmote,
    Field::ClonedAppearance,
];

/// This module's natives: package, number, function.
const NATIVES: &[(u32, u32, NativeFn)] = &[
    (BEHAVIOR_PARAMS, 0x01, set::<0>),
    (BEHAVIOR_PARAMS, 0x02, behavior_id),
    (BEHAVIOR_PARAMS, 0x03, set::<1>),
    (BEHAVIOR_PARAMS, 0x04, set::<2>),
    (BEHAVIOR_PARAMS, 0x05, set::<3>),
    (BEHAVIOR_PARAMS, 0x06, set::<4>),
    (BEHAVIOR_PARAMS, 0x07, set::<5>),
    (BEHAVIOR_PARAMS, 0x08, set::<6>),
    (BEHAVIOR_PARAMS, 0x09, set::<7>),
    (BEHAVIOR_PARAMS, 0x0A, set::<8>),
    (BEHAVIOR_PARAMS, 0x0B, set::<9>),
    (BEHAVIOR_PARAMS, 0x0C, set::<10>),
    (BEHAVIOR_PARAMS, 0x0D, set::<11>),
    (BEHAVIOR_PARAMS, 0x0E, set::<12>),
    (BEHAVIOR_PARAMS, 0x0F, set::<13>),
    (BEHAVIOR_PARAMS, 0x10, set::<14>),
    (BEHAVIOR_PARAMS, 0x11, set::<15>),
    (BEHAVIOR_PARAMS, 0x12, set::<16>),
    (BEHAVIOR_PARAMS, 0x13, set::<17>),
    (BEHAVIOR_PARAMS, 0x15, set::<18>),
    (BEHAVIOR_PARAMS, 0x16, set::<19>),
    (BEHAVIOR_PARAMS, 0x17, set::<20>),
    (BEHAVIOR_PARAMS, 0x18, set::<21>),
    (BEHAVIOR_PARAMS, 0x19, set::<22>),
];

/// Bind this module's natives.
pub fn bind(t: &mut NativeTable) {
    crate::bind_table(t, NATIVES);
}

/// The parameters a reference names (`None` for the null reference: the call crashes).
pub fn params<'a>(c: &'a mut Call<'_>, r: u32) -> Result<Option<&'a mut BehaviorParams>, Stop> {
    if r == NULL {
        return Ok(None);
    }
    let native = c.native;
    match c.vm.heap.native_mut(r)? {
        NativeObject::Behavior(b) => Ok(Some(b)),
        _ => Err(Stop::Native {
            native,
            what: "not a behavior's parameters".into(),
        }),
    }
}

/// Set one field from the arguments (none for a switch).
// D2-01370, D2-08413, D2-01366, D2-01363, D2-08406, D2-08401, D2-08408, D2-08411, D2-08403
// D2-08410, D2-08407, D2-00943, D2-08402, D2-08405, D2-01557, D2-01513, D2-00949, D2-01637
// D2-08404, D2-08409, D2-01514, D2-08414, D2-00948
fn set<const F: usize>(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    let word = if r == NULL { 0 } else { c.cx.at(-2)? };
    let long = if r == NULL {
        0
    } else {
        join(c.cx.at(-3).unwrap_or(0), word)
    };
    let Some(p) = params(c, r)? else {
        return Ok(Status::Crash);
    };
    match FIELDS[F] {
        Field::Behavior => p.behavior = word,
        Field::TimeScale => {
            let mut v = word;
            if f32::from_bits(v).is_nan() {
                v |= 1 << 22;
            }
            p.time_scale = v;
        }
        Field::Mode => p.mode = word,
        Field::HoldCycles => p.hold_cycles = word,
        Field::MoveToCancels => p.move_to_cancels = 1,
        Field::EarlyCallback => p.early_callback = 1,
        Field::CameraParented => p.camera_parented = 1,
        Field::CameraTargeted => p.camera_targeted = 1,
        Field::CameraHold => p.camera_hold = 1,
        Field::CameraRestore => p.camera_restore = 1,
        Field::CameraTarget => p.camera_target = long,
        Field::CameraBehavior => p.camera_behavior = word,
        Field::FadeChildren => p.fade_children = 1,
        Field::ActionsLocked => p.lock_actions = 1,
        Field::SendEvent => p.send_event = word,
        Field::FxScript => p.fx_script = word,
        Field::Target => p.target = long,
        Field::VisualToClone => p.visual_to_clone = word,
        Field::LeaveIdleAlone => p.leave_idle_alone = 1,
        Field::Propagate => p.propagate = 1,
        Field::Fx => p.fx = word,
        Field::WeenieEmote => p.weenie_emote = word,
        Field::ClonedAppearance => p.cloned_appearance = word,
    }
    Ok(Status::Success)
}

/// The behavior's id.
// D2-18959
fn behavior_id(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    let Some(p) = params(c, r)? else {
        return Ok(Status::Crash);
    };
    let v = p.behavior;
    c.write(-2, v, tag::MIXED)?;
    Ok(Status::Success)
}
