// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Physics/Animation/MotionTable.cs, Source/ACE.DatLoader/FileTypes/MotionTable.cs
//! ACE's `MotionTable` timing helpers, over the shared motion-table data.
//!
//! Two ACE classes share the name. `Physics.Animation.MotionTable` has the static helpers gameplay
//! calls (`GetAnimationLength`, `GetCycleLength`, `GetAttackFrames`, run and turn speeds); they read
//! the dat table and call the instance methods of `DatLoader.FileTypes.MotionTable`
//! (`GetAnimData`, the `GetAnimationLength` overloads, `GetAttackFrames`,
//! `GetAnimationFinalPositionFromStart`). Both are ported here over the shared decoder's
//! `MotionTable` and `Animation` (read through `World.dats`). The motion-sequencing members of the
//! physics class (`GetObjectSequence`, `add_motion`, ...) are the shared `dereth-animation` crate's job
//! (V1) and are not ported.
//!
//! Not ACE's (retail, V352): the timings (`GetAnimationLength`,
//! `GetCycleLength`, `GetAttackFrames`) are the client's playback of the motion, from `dereth-animation`:
//! the animations the client queues for the motion from the current one (its link choice, including
//! the two-hop and style-change paths), each playing from its starting frame until the player
//! leaves it (`high_frame + 1`, one frame more than ACE's `high_frame - low_frame` when the range is
//! explicit), and each attack hook at the time the player leaves the hook's frame (ACE placed it at
//! the frame's share of every frame of the link's whole animations, a frame or more early). The
//! vectors over every retail motion table are `anim_timing::rr89_vectors` (real-content tier).
//!
//! * A table or animation the portal dat does not have reads as ACE's empty `new T()`: no links,
//!   no cycles, zero frames (V11).
//! * The shared decoder keeps `Cycles` and each `Links` entry as lists whose entries carry their
//!   dictionary key (`MotionData::key`); a lookup is a search for that key.
//! * ACE's three static speed caches and its attack-frame cache are left out: each caches a pure
//!   function of immutable dat data, so the cached and uncached answers are the same.

use std::sync::Arc;

use dereth_animation::data::AnimAssets;
use dereth_animation::{MotionState, Sequence};
use dereth_assets::motion::{AnimData, MotionData};
use dereth_assets::AnimHook;
use dereth_primitives::DataId;
use empyrean_common::dotnet::{Quaternion, Vector3};
use empyrean_dat::file_types::{Animation, MotionTable};
use empyrean_dat::DatManager;
use empyrean_entity::enums::{AnimationHookType, MotionCommand, MotionStance};
use empyrean_entity::Position;

use crate::World;

/// `MotionCommand.Ready`'s low word, as `GetLinkData` masks it.
const READY_LOW: u32 = MotionCommand::Ready.0 & 0xFFFF;

/// The hook type of ACE's `AttackHook` (`AnimationHookType.Attack`).
const ATTACK_HOOK: u32 = 3;

/// The client's sequence for `motion` requested while `current_motion` plays in `stance` at
/// `speed`, played from its start, and how many of its animations play before the motion is done
/// (the animation-done count the client waits for). `None` when the client would not play it.
fn client_sequence(
    w: &World,
    motion_table_id: u32,
    stance: MotionStance,
    current_motion: MotionCommand,
    motion: MotionCommand,
    speed: f32,
) -> Option<(Sequence, usize)> {
    let assets: &dyn AnimAssets = w.phys_ext.motion_assets.as_ref();
    let table = dereth_animation::MotionTable::new(assets.motion_table(DataId(motion_table_id))?);
    let mut state = MotionState {
        style: dereth_animation::MotionCommand(stance.0),
        substate: dereth_animation::MotionCommand(current_motion.0),
        ..MotionState::new()
    };
    let mut seq = Sequence::new();
    let n = table.get_object_sequence(
        dereth_animation::MotionCommand(motion.0),
        &mut state,
        &mut seq,
        speed,
        false,
        assets,
    )?;
    Some((seq, n as usize))
}

/// How long the client takes to play `motion` from `current_motion` in `stance` at `speed`; 0 when
/// it would not play it.
#[allow(clippy::cast_possible_truncation)] // the helpers' float seconds
fn client_length(
    w: &World,
    motion_table_id: u32,
    stance: MotionStance,
    current_motion: MotionCommand,
    motion: MotionCommand,
    speed: f32,
) -> f32 {
    client_sequence(w, motion_table_id, stance, current_motion, motion, speed)
        .map_or(0.0, |(seq, n)| seq.play_time(n) as f32)
}

/// `DatManager.PortalDat.ReadFromDat<MotionTable>(id)`; `None` stands for ACE's empty table.
fn read_motion_table(dats: &DatManager, id: u32) -> Option<Arc<MotionTable>> {
    dats.portal_dat().read_from_dat::<MotionTable>(id)
}

/// `DatManager.PortalDat.ReadFromDat<Animation>(id)`; `None` stands for ACE's empty animation.
fn read_animation(dats: &DatManager, id: u32) -> Option<Arc<Animation>> {
    dats.portal_dat().read_from_dat::<Animation>(id)
}

/// `Dictionary<uint, MotionData>.TryGetValue(key)` over the shared decoder's keyed list.
fn find(list: &[MotionData], key: u32) -> Option<&MotionData> {
    list.iter().find(|m| m.key == key)
}

/// `Links.TryGetValue(hash, out link)` then `link.TryGetValue(motion, out motionData)`.
fn link(mt: &MotionTable, hash: u32, motion: u32) -> (bool, Option<&MotionData>) {
    match mt.links.get(&hash) {
        None => (false, None),
        Some(l) => (true, find(l, motion)),
    }
}

// ---------------------------------------------------------------------------------------------
// ACE.DatLoader.FileTypes.MotionTable (instance methods)
// ---------------------------------------------------------------------------------------------

/// Gets the default style for the requested MotionStance, or `MotionCommand.Invalid`.
// ACE: FileTypes.MotionTable.GetDefaultMotion
#[must_use]
pub fn get_default_motion(mt: Option<&MotionTable>, style: MotionStance) -> MotionCommand {
    match mt.and_then(|mt| mt.style_defaults.get(&style.0)) {
        Some(&v) => MotionCommand(v),
        None => MotionCommand::Invalid,
    }
}

/// `motionTable.GetAnimationLength(MotionCommand motion)`: from the table's default style and its
/// default motion.
// ACE: FileTypes.MotionTable.GetAnimationLength
#[must_use]
pub fn get_animation_length_of(w: &World, mt: Option<&MotionTable>, motion: MotionCommand) -> f32 {
    let default_stance = MotionStance(mt.map_or(0, |mt| mt.default_style));
    let default_motion = get_default_motion(mt, default_stance);

    get_animation_length_from(w, mt, default_stance, motion, default_motion)
}

/// `motionTable.GetAnimationLength(MotionStance stance, MotionCommand motion, MotionCommand? currentMotion)`:
/// a `null` current motion is the stance's default motion.
// ACE: FileTypes.MotionTable.GetAnimationLength
#[must_use]
pub fn get_animation_length_in(
    w: &World,
    mt: Option<&MotionTable>,
    stance: MotionStance,
    motion: MotionCommand,
    current_motion: Option<MotionCommand>,
) -> f32 {
    let current_motion = current_motion.unwrap_or_else(|| get_default_motion(mt, stance));

    get_animation_length_from(w, mt, stance, motion, current_motion)
}

/// The length of one cycle of `motion` in `stance`, summed over its animations as the client
/// plays them.
// ACE: FileTypes.MotionTable.GetCycleLength
#[must_use]
pub fn get_cycle_length_in(
    w: &World,
    mt: Option<&MotionTable>,
    stance: MotionStance,
    motion: MotionCommand,
) -> f32 {
    let key = stance.0.wrapping_shl(16) | motion.0 & 0xFFFFF;

    let Some(motion_data) = mt.and_then(|mt| find(&mt.cycles, key)) else {
        return 0.0;
    };

    let mut length = 0.0_f32;
    for anim in &motion_data.anims {
        length += get_animation_length_anim(w, anim);
    }

    length
}

/// The attack hooks the client fires while it plays `motion` from the stance's default motion,
/// each with the time it fires as a fraction of the motion's length.
// ACE: FileTypes.MotionTable.GetAttackFrames
#[must_use]
#[allow(clippy::cast_possible_truncation)] // the fractions are floats
pub fn get_attack_frames_in(
    w: &World,
    mt: Option<&MotionTable>,
    motion_table_id: u32,
    stance: MotionStance,
    motion: MotionCommand,
) -> Vec<(f32, AnimHook)> {
    let default_motion = get_default_motion(mt, stance);

    let Some((seq, n)) = client_sequence(w, motion_table_id, stance, default_motion, motion, 1.0)
    else {
        return Vec::new();
    };
    let total = seq.play_time(n);

    let mut attack_frames = Vec::new();
    let mut start = 0.0_f64;
    for node in seq.nodes().iter().take(n) {
        let animation = read_animation(&w.dats, node.anim_id.0);
        for (frame, t, direction) in node.fired_frames() {
            let part_frame = animation.as_ref().and_then(|a| {
                usize::try_from(frame)
                    .ok()
                    .and_then(|i| a.part_frames.get(i))
            });
            for hook in part_frame.iter().flat_map(|f| f.hooks.iter()) {
                if hook.hook_type == ATTACK_HOOK
                    && (hook.direction == 0 || hook.direction == direction)
                {
                    attack_frames.push((((start + t) / total) as f32, hook.clone()));
                }
            }
        }
        start += node.play_time();
    }

    attack_frames
}

/// The animations of the link from `current_motion` to `motion` in `stance`, falling back to the
/// stance's catch-all link (`stance << 16`).
// ACE: FileTypes.MotionTable.GetAnimData
#[must_use]
pub fn get_anim_data(
    mt: Option<&MotionTable>,
    stance: MotionStance,
    motion: MotionCommand,
    current_motion: MotionCommand,
) -> Vec<AnimData> {
    let Some(mt) = mt else { return Vec::new() };

    let motion_hash = stance.0.wrapping_shl(16) | current_motion.0 & 0xFFFFF;

    let (has_link, motion_data) = link(mt, motion_hash, motion.0);
    if !has_link {
        return Vec::new();
    }

    let motion_data = match motion_data {
        Some(m) => m,
        None => {
            let motion_hash = stance.0.wrapping_shl(16);
            let (has_link, motion_data) = link(mt, motion_hash, motion.0);
            if !has_link {
                return Vec::new();
            }
            match motion_data {
                Some(m) => m,
                None => return Vec::new(),
            }
        }
    };
    motion_data.anims.clone()
}

/// `GetAnimationLength(MotionStance, MotionCommand, MotionCommand)`: how long the client plays
/// `motion` from `current_motion`.
// ACE: FileTypes.MotionTable.GetAnimationLength
#[must_use]
pub fn get_animation_length_from(
    w: &World,
    mt: Option<&MotionTable>,
    stance: MotionStance,
    motion: MotionCommand,
    current_motion: MotionCommand,
) -> f32 {
    mt.map_or(0.0, |mt| {
        client_length(w, mt.id.0, stance, current_motion, motion, 1.0)
    })
}

/// One animation's playing time as the client plays it: from its starting frame until the
/// player leaves it, over the absolute frame rate (a negative rate plays in reverse).
/// `HighFrame == -1` means the whole animation, and a range past the end is clamped to it.
// ACE: FileTypes.MotionTable.GetAnimationLength
#[must_use]
#[allow(clippy::cast_possible_truncation)] // the helpers' float seconds
pub fn get_animation_length_anim(w: &World, anim: &AnimData) -> f32 {
    let a = dereth_animation::data::AnimData {
        anim_id: anim.anim_id,
        low_frame: anim.low_frame,
        high_frame: anim.high_frame,
        framerate: anim.framerate,
    };
    dereth_animation::AnimSequenceNode::new(a, w.phys_ext.motion_assets.as_ref())
        .map_or(0.0, |n| n.play_time() as f32)
}

/// `GetAnimationFinalPositionFromStart(position, objScale, motion)`: from the default style's
/// default motion.
// ACE: FileTypes.MotionTable.GetAnimationFinalPositionFromStart
pub fn get_animation_final_position_from_start_of(
    dats: &DatManager,
    mt: Option<&MotionTable>,
    position: &mut Position,
    obj_scale: f32,
    motion: MotionCommand,
) -> Position {
    let default_style = MotionStance(mt.map_or(0, |mt| mt.default_style));

    // get the default motion for the default
    let default_motion = get_default_motion(mt, default_style);
    get_animation_final_position_from_start(
        dats,
        mt,
        position,
        obj_scale,
        default_motion,
        default_style,
        motion,
    )
}

/// Where the link from `current_motion_state` to `motion` in `style` leaves an object that starts
/// at `position`, from the whole-animation position frames of the link's animations.
// ACE: FileTypes.MotionTable.GetAnimationFinalPositionFromStart
// ACE-BUG: `finalPosition = position` aliases the caller's Position, so the caller's position is
// moved in place, and each further whole animation in the link starts from the already-moved
// position. `position` is `&mut` for that reason. Without a whole animation the result is a new,
// default Position (the origin of cell 0), not the start.
#[allow(clippy::cast_precision_loss)] // the unused length sum, as C# computes it
pub fn get_animation_final_position_from_start(
    dats: &DatManager,
    mt: Option<&MotionTable>,
    position: &mut Position,
    obj_scale: f32,
    current_motion_state: MotionCommand,
    style: MotionStance,
    motion: MotionCommand,
) -> Position {
    let mut length = 0.0_f32; // init our length var...will return as 0 if not found

    // `finalPosition`: a new Position until it is made an alias of `position`.
    let mut aliased = false;

    let motion_hash = (current_motion_state.0 & 0xFFFFFF) | style.0.wrapping_shl(16);

    if let Some(links) = mt.and_then(|mt| mt.links.get(&motion_hash)) {
        if let Some(motion_data) = find(links, motion.0) {
            // loop through all that animations to get our total count
            for anim in &motion_data.anims {
                let num_frames: u32;

                // check if the animation is set to play the whole thing, in which case we need to get the numbers of frames in the raw animation
                if anim.low_frame == 0 && anim.high_frame == -1 {
                    let animation = read_animation(dats, anim.anim_id.0);
                    num_frames = animation.as_ref().map_or(0, |a| a.num_frames);

                    let pos_frames = animation.as_ref().and_then(|a| a.pos_frames.as_ref());
                    match pos_frames {
                        Some(frames) if !frames.is_empty() => {
                            aliased = true;
                            let mut origin = Vector3::new(
                                position.position_x,
                                position.position_y,
                                position.position_z,
                            );
                            let mut orientation = Quaternion::new(
                                position.rotation_x,
                                position.rotation_y,
                                position.rotation_z,
                                position.rotation_w,
                            );
                            for pos_frame in frames {
                                let o = pos_frame.origin;
                                let r = pos_frame.rotation;
                                origin = origin
                                    + Vector3::transform(Vector3::new(o.x, o.y, o.z), orientation)
                                        * obj_scale;

                                orientation = orientation * Quaternion::new(r.x, r.y, r.z, r.w);
                                orientation = Quaternion::normalize(orientation);
                            }

                            position.position_x = origin.x;
                            position.position_y = origin.y;
                            position.position_z = origin.z;

                            position.rotation_w = orientation.w;
                            position.rotation_x = orientation.x;
                            position.rotation_y = orientation.y;
                            position.rotation_z = orientation.z;
                        }
                        _ => return *position,
                    }
                } else {
                    #[allow(clippy::cast_sign_loss)] // C#'s (uint) of an int
                    {
                        num_frames = anim.high_frame.wrapping_sub(anim.low_frame) as u32;
                    }
                }

                length += num_frames as f32 / anim.framerate.abs(); // Framerates can be negative, which tells the client to play in reverse
            }
        }
    }
    let _ = length;

    if aliased {
        *position
    } else {
        Position::new()
    }
}

// ---------------------------------------------------------------------------------------------
// ACE.Server.Physics.Animation.MotionTable (static helpers)
// ---------------------------------------------------------------------------------------------

/// The attack frames of `motion` in `stance` for a motion table id; none for id 0.
// ACE: Animation.MotionTable.GetAttackFrames
#[must_use]
pub fn get_attack_frames(
    w: &World,
    motion_table_id: u32,
    stance: MotionStance,
    motion: MotionCommand,
) -> Vec<(f32, AnimHook)> {
    if motion_table_id == 0 {
        return Vec::new();
    }

    let motion_table = read_motion_table(&w.dats, motion_table_id);
    get_attack_frames_in(w, motion_table.as_deref(), motion_table_id, stance, motion)
}

/// `GetAnimationLength(motionTableId, stance, motion, speed = 1.0f)`: how long the client plays
/// `motion` from the stance's default motion at `speed`.
// ACE: Animation.MotionTable.GetAnimationLength
#[must_use]
pub fn get_animation_length(
    w: &World,
    motion_table_id: u32,
    stance: MotionStance,
    motion: MotionCommand,
    speed: f32,
) -> f32 {
    if motion_table_id == 0 {
        return 0.0;
    }

    let motion_table = read_motion_table(&w.dats, motion_table_id);
    let default_motion = get_default_motion(motion_table.as_deref(), stance);
    client_length(w, motion_table_id, stance, default_motion, motion, speed)
}

/// `GetAnimationLength(motionTableId, stance, currentMotion, motion, speed = 1.0f)`: how long the
/// client plays `motion` from `current_motion` at `speed`. A style change goes through the old
/// style's default motion, as the client's own style change does (ACE added the link back to
/// Ready).
// ACE: Animation.MotionTable.GetAnimationLength
#[must_use]
pub fn get_animation_length_between(
    w: &World,
    motion_table_id: u32,
    stance: MotionStance,
    current_motion: MotionCommand,
    motion: MotionCommand,
    speed: f32,
) -> f32 {
    if motion_table_id == 0 {
        return 0.0;
    }

    client_length(w, motion_table_id, stance, current_motion, motion, speed)
}

/// The length of one cycle of `motion` in `stance`, over `speed`; 0 for motion table 0.
// ACE: Animation.MotionTable.GetCycleLength
#[must_use]
pub fn get_cycle_length(
    w: &World,
    motion_table_id: u32,
    stance: MotionStance,
    motion: MotionCommand,
    speed: f32,
) -> f32 {
    if motion_table_id == 0 {
        return 0.0;
    }

    let motion_table = read_motion_table(&w.dats, motion_table_id);
    get_cycle_length_in(w, motion_table.as_deref(), stance, motion) / speed
}

/// Returns the distance per second for a running animation.
// ACE: Animation.MotionTable.GetRunSpeed
#[must_use]
pub fn get_run_speed(w: &World, motion_table_id: u32) -> f32 {
    let run_motion = MotionCommand::RunForward.0;
    let Some(motion_data) = get_motion_data(w, motion_table_id, run_motion, None) else {
        return 0.0;
    };

    get_anim_dist(w, &motion_data)
}

/// Returns the rotational velocity / omega for a turning animation.
// ACE: Animation.MotionTable.GetTurnSpeed
#[must_use]
pub fn get_turn_speed(w: &World, motion_table_id: u32) -> f32 {
    let turn_motion = MotionCommand::TurnRight.0;
    let Some(motion_data) = get_motion_data(w, motion_table_id, turn_motion, None) else {
        return 0.0;
    };

    motion_data.omega.map_or(0.0, |o| o.z).abs()
}

/// Returns the cycle MotionData for a motion table and motion ID, in `current_style` or the
/// table's default style.
// ACE: Animation.MotionTable.GetMotionData
#[must_use]
pub fn get_motion_data(
    w: &World,
    motion_table_id: u32,
    motion: u32,
    current_style: Option<u32>,
) -> Option<MotionData> {
    if motion_table_id == 0 {
        return None;
    }

    let motion_table = read_motion_table(&w.dats, motion_table_id)?;
    let current_style = current_style.unwrap_or(motion_table.default_style);
    let motion_id = motion & 0xFFFFFF;
    let key = current_style.wrapping_shl(16) | motion_id;
    find(&motion_table.cycles, key).cloned()
}

/// Returns the link MotionData from Ready to `motion` in `current_style` or the default style.
// ACE: Animation.MotionTable.GetLinkData
#[must_use]
pub fn get_link_data(
    w: &World,
    motion_table_id: u32,
    motion: u32,
    current_style: Option<u32>,
) -> Option<MotionData> {
    if motion_table_id == 0 {
        return None;
    }

    let motion_table = read_motion_table(&w.dats, motion_table_id)?;
    let current_style = current_style.unwrap_or(motion_table.default_style);
    let key = current_style.wrapping_shl(16) | READY_LOW;
    let links = motion_table.links.get(&key)?;
    find(links, motion).cloned()
}

/// Returns the movement distance per second from an animation: the summed position-frame offset
/// over the frame count, at the first animation's frame rate.
// ACE: Animation.MotionTable.GetAnimDist
#[must_use]
#[allow(clippy::cast_precision_loss)] // C#'s int to float
pub fn get_anim_dist(w: &World, motion_data: &MotionData) -> f32 {
    let mut offset = Vector3::ZERO;
    let mut total_frames = 0_i32;
    for anim in &motion_data.anims {
        let animation = read_animation(&w.dats, anim.anim_id.0);
        for frame in animation.iter().flat_map(|a| a.pos_frames.iter().flatten()) {
            // orientation?
            offset = offset + Vector3::new(frame.origin.x, frame.origin.y, frame.origin.z);
            total_frames += 1;
        }
    }
    let dist = offset.length();
    #[allow(clippy::float_cmp)]
    if dist == 0.0 {
        return 0.0;
    }
    // ACE indexes Anims[0] unguarded; a motion with frames always has a first animation.
    dist / total_frames as f32 * motion_data.anims[0].framerate
}

/// Returns TRUE if this animation has a DefaultScript hook type.
// ACE: Animation.MotionTable.HasDefaultScript
#[must_use]
pub fn has_default_script(
    w: &World,
    motion_table_id: u32,
    motion: u32,
    current_style: u32,
) -> bool {
    let Some(motion_data) = get_link_data(w, motion_table_id, motion, Some(current_style)) else {
        return false;
    };

    for anim in &motion_data.anims {
        let Some(animation) = read_animation(&w.dats, anim.anim_id.0) else {
            continue;
        };

        for frame in &animation.part_frames {
            for hook in &frame.hooks {
                if i64::from(hook.hook_type) == i64::from(AnimationHookType::DefaultScript.0) {
                    return true;
                }
            }
        }
    }
    false
}
