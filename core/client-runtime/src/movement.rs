//! Movement interpretation: the wire's `0xF74C` movement buffer as the motion runtime calls it.
//!
//! Movement-buffer unpacking and the four `MoveTo`/`TurnTo` destinations it
//! feeds, for the local player and for a server object, plus the two wire -> runtime conversions
//! either side of it. The scene calls these with its object table and its body instead of
//! handing over `&mut WorldScene`.

use std::collections::BTreeMap;

use dereth_animation::MotionCommand;
use dereth_primitives::{CellId, DataId, ObjectId};

use crate::character::Character;
use crate::object_step::{AsObjectSim, ObjectStepStats};
use crate::objects::ObjectStream;

/// The wire `ObjDesc` as the animation runtime's one.
///
/// The two crates on either side deliberately do not know each other —
/// `dereth_protocol::types::ObjDesc` is the wire shape and
/// `dereth_animation::parts::ObjDesc` is the part-array runtime shape — and the
/// application is the only crate allowed to depend on both, exactly as for
/// [`apply_movement`]'s two `InterpretedMotionState`s.
///
/// Nothing is converted on the way across: the wire's `offset`/`numcolors` bytes travel as
/// they are, because `PaletteRange` documents them as in 8-entry units with a length of 0
/// meaning 256, resolved by the palette expansion rather than here, and
/// `ExpandedPalette::apply_subpalette` is where the `x8` happens.
pub fn to_anim_objdesc(od: &dereth_protocol::types::ObjDesc) -> dereth_animation::parts::ObjDesc {
    use dereth_animation::parts::{AnimPartChange, ObjDesc, PaletteRange, TextureMapChange};
    ObjDesc {
        part_changes: od
            .anim_part_changes
            .iter()
            .map(|c| AnimPartChange {
                part_index: u32::from(c.part_index),
                part_id: DataId(c.part_id),
            })
            .collect(),
        texture_changes: od
            .texture_changes
            .iter()
            .map(|c| TextureMapChange {
                part_index: u32::from(c.part_index),
                old_texture: DataId(c.old_tex_id),
                new_texture: DataId(c.new_tex_id),
            })
            .collect(),
        palette_id: DataId(od.palette_id),
        subpalettes: od
            .subpalettes
            .iter()
            .map(|x| PaletteRange {
                palette_set: DataId(x.sub_id),
                offset: u32::from(x.offset),
                length: u32::from(x.num_colors),
            })
            .collect(),
    }
}

/// Network movement-parameter unpacking fills a **default-constructed**
/// `MovementParameters`, so every field it does not carry keeps the client's own default --
/// `context_id`, `hold_key_to_apply` and `action_stamp` among them.
///
/// Shared by [`apply_player_movement`] and the remote arm, which need the identical
/// conversion. It is the same function in the client: `case 6`..`case 9` of
/// movement unpacking do not know whose object they are unpacking
/// for.
pub fn wire_to_params(
    w: &dereth_protocol::movement::MovementParameters,
) -> dereth_animation::motion::MovementParameters {
    use dereth_animation::motion::MovementParameters;
    use dereth_protocol::movement::MovementParameters as WireParams;
    match *w {
        WireParams::MoveTo {
            bitfield,
            distance_to_object,
            min_distance,
            fail_distance,
            speed,
            walk_run_threshold,
            desired_heading,
        } => MovementParameters {
            flags: bitfield,
            distance_to_object,
            min_distance,
            fail_distance,
            speed,
            walk_run_threshold,
            desired_heading,
            ..MovementParameters::default()
        },
        WireParams::TurnTo {
            bitfield,
            speed,
            desired_heading,
        } => MovementParameters {
            flags: bitfield,
            speed,
            desired_heading,
            ..MovementParameters::default()
        },
    }
}

/// Object lookup over this build's object table.
///
/// **The local player is in it, and that is not a convenience.** The client has
/// exactly one object table and the player's own physics body is in it, so object lookup finds
/// him. This build splits him out into [`crate::character::Character`] and leaves him out of
/// `WorldScene::objects` whenever there is a local body -- which is invisible while only the
/// *player's* buffers are unpacked (he is never his own target) and decisive for a remote
/// creature's, because **every** `MoveToObject` and `TurnToObject` the server sends a remote
/// object in the seven captures names the session's own character: 14 of 14 and 53 of 53, and
/// all 46 `StickToObject` ids as well. Without him here every one of those would take
/// `unpack_movement`'s object-not-found fall-through and walk to a packed origin instead of
/// following the player.
///
/// Returns `(top_level_id, radius, height)`. The radius and height belong to the **named** object
/// (zero with no part array), while the id handed on is its **parent's** when it has one -- one
/// hop, not a walk to the root. Turning takes the same hop and no dimensions at all.
pub fn move_to_target<S: AsObjectSim>(
    objects: &BTreeMap<ObjectId, S>,
    character: &Option<Character>,
    stream: &ObjectStream,
    id: ObjectId,
) -> Option<(ObjectId, f32, f32)> {
    if let Some(c) = character.as_ref() {
        if stream.player() == Some(id) {
            // The player has no `parent` in this build -- `Character` is never a child -- so
            // the hop is the identity and the dimensions are his own body's.
            return Some((id, c.radius(), c.height()));
        }
    }
    let o = objects.get(&id)?.sim();
    let top = o.parent.map_or(id, |(p, _)| p);
    Some((top, o.radius, o.height))
}

/// Resolve a stick-to-object target, which is **not**
/// [`move_to_target`]'s however alike they look.
///
/// 1. Look the object up; no object means no stick at all.
/// 2. If it has a parent, hop to the parent -- the hop comes FIRST.
/// 3. Height and radius are the hopped-to object's part-array height and radius, or 0 when it
///    has no part array.
/// 4. `stick_to` the hopped-to object's id with that radius and height.
///
/// Moving to an object reads the dimensions off the object the server **named** and only
/// then hops to the parent; this reads both off the object it hopped **to**. Two functions
/// eleven lines apart in the same file that group the same three statements differently, so
/// they are two functions here as well rather than one with a flag.
pub fn stick_target<S: AsObjectSim>(
    objects: &BTreeMap<ObjectId, S>,
    character: &Option<Character>,
    stream: &ObjectStream,
    id: ObjectId,
) -> Option<(ObjectId, f32, f32)> {
    if let Some(c) = character.as_ref() {
        if stream.player() == Some(id) {
            return Some((id, c.radius(), c.height()));
        }
    }
    let o = objects.get(&id)?.sim();
    let top = o.parent.map_or(id, |(p, _)| p);
    // The hop first: the dimensions belong to whatever we landed on.
    let (r, h) = objects
        .get(&top)
        .map_or((0.0, 0.0), |p| (p.sim().radius, p.sim().height));
    Some((top, r, h))
}

/// The movement buffer's cases 6 through 9, represented as a request.
///
/// The fall-throughs are the client's and are the reason this is one function rather than four
/// call sites: `case 6` falls back to the packed origin when
/// the object lookup misses, and `case 8` `goto`s into `case 9`'s `TurnToHeading` for the same
/// reason. Shared by [`apply_player_movement`] and `apply_movement`.
pub fn move_to_request<S: AsObjectSim>(
    objects: &BTreeMap<ObjectId, S>,
    character: &Option<Character>,
    stream: &ObjectStream,
    arm: &dereth_protocol::movement::MoveToArm,
) -> (
    dereth_animation::motion::MoveToRequest,
    dereth_animation::motion::MovementParameters,
    Option<f32>,
) {
    use dereth_animation::motion::{MoveToRequest, MovementParameters};
    use dereth_protocol::movement::MoveToArm;
    match *arm {
        MoveToArm::MoveToObject {
            target,
            origin,
            params,
            run_rate,
        } => {
            let p = wire_to_params(&params);
            match move_to_target(objects, character, stream, target) {
                Some((top_level_id, radius, height)) => (
                    MoveToRequest::MoveToObject {
                        object_id: target,
                        top_level_id,
                        radius,
                        height,
                    },
                    p,
                    Some(run_rate),
                ),
                None => (
                    MoveToRequest::MoveToPosition {
                        pos: origin_position(&origin),
                    },
                    p,
                    Some(run_rate),
                ),
            }
        }
        MoveToArm::MoveToPosition {
            origin,
            params,
            run_rate,
        } => (
            MoveToRequest::MoveToPosition {
                pos: origin_position(&origin),
            },
            wire_to_params(&params),
            Some(run_rate),
        ),
        MoveToArm::TurnToObject {
            target,
            desired_heading,
            params,
        } => {
            // The heading is read before the parameters and **overwrites** the one they
            // carry; `case 8` does that assignment after the parameter unpack returns.
            let p = MovementParameters {
                desired_heading,
                ..wire_to_params(&params)
            };
            match move_to_target(objects, character, stream, target) {
                Some((top_level_id, _, _)) => (
                    MoveToRequest::TurnToObject {
                        object_id: target,
                        top_level_id,
                    },
                    p,
                    None,
                ),
                None => (MoveToRequest::TurnToHeading, p, None),
            }
        }
        MoveToArm::TurnToHeading { params } => {
            let _ = MovementParameters::default();
            (MoveToRequest::TurnToHeading, wire_to_params(&params), None)
        }
    }
}

/// The player's own `0xF74C`, which is how the approach walk starts.
///
/// The movement-buffer path applies a buffer to the player unless it is
/// **autonomous**, which would be the server echoing back what he just sent
/// (`last_move_was_autonomous`); that guard is the first line here.
///
/// Its four `MoveTo`/`TurnTo` cases resolve the target and **fall through to the packed origin
/// when it is not there** -- so an approach to
/// an object this client has never been told about walks to the spot the server named instead
/// of doing nothing. A landblock static takes that arm here (it has no `SceneObject`), and
/// walking to its packed origin is the right answer for something that cannot move.
///
/// **`unpack_movement` is a two-part function, and both parts are here.** Before any `case` is
/// entered it reads the buffer's second `u16` as a style index,
/// looks up `command_ids[style_ix]`, compares it with `interpreted_state.current_style` and
/// applies the motion style with default parameters when the two differ — and only then
/// enters the switch on the buffer type.
///
/// So the style word is applied for **every** buffer type, `MoveTo` and `TurnTo` included --
/// and in those arms it is the only thing that can change a stance, because they carry no
/// `InterpretedMotionState` at all. Between them, `body.current_style` and the interpreted state
/// are the whole of the local player's stance: skip either and he never leaves `NonCombat`.
///
/// `case 0`'s own body is taken here too. Movement application has already
/// refused the autonomous echo above -- that is the only player-specific rule in the path --
/// and `case 0` itself has no player exception, so the server's interpreted state reaches his
/// motion interpreter exactly as it reaches a remote object's. It has to arrive *here*: with a
/// local body the player has no `SceneObject`, so [`apply_movement`], which is the only other
/// caller of the same two entry points, could never see him.
///
/// # `case 0`'s last two statements
///
/// ```text
/// move_to_interpreted_state(this, &state);
/// if (sticky object id != 0) stick_to_object(physics_obj, sticky object id);
/// motion_interpreter.standing_longjump = flags & 0x200;
/// ```
///
/// Both apply to the player as well as to a remote object. The corpus carries
/// **16** `StickToObject` buffers addressed to the session's own character -- all in
/// `long-solo-play`, all naming the creature `0x800009D9`, none of them autonomous -- so this arm
/// is not an inert one.
///
/// The player's shared motion applies the sticky manager's correction inside collision-tested
/// position integration, after animation-origin scaling. Its absolute initial
/// 1-second deadline is not renewed by target updates.
/// Target cadence/prediction and full substep effect ordering are not yet matched to retail;
/// this consumer must not claim the position-manager hook is absent.
pub fn apply_player_movement<S: AsObjectSim>(
    objects: &BTreeMap<ObjectId, S>,
    character: &mut Option<Character>,
    buf: &dereth_protocol::movement::MovementBuffer,
    stream: &ObjectStream,
) -> bool {
    use dereth_animation::motion::{flags, MovementParameters};

    let mut applied = false;
    if buf.autonomous {
        return applied;
    }

    // **The movement setter's `return 1`.** Everything past this guard is the case it
    // returns 1 for: the buffer is the player's and it is not autonomous. The smart-box
    // dispatcher's `0xF74C` arm hands control of the player to the server on exactly that,
    // *before* `unpack_movement`'s effects are read by anything, so the latch is set here rather
    // than at the tail — the two arms below both
    // `return`, and a stop applied by `case 0` and a `MoveToObject` applied by `case 6` are
    // both "the server moved the player". See `WorldScene::player_movement_applied`.
    applied = true;

    if let Some(c) = character.as_mut() {
        c.prepare_received_movement();
    }

    // `unpack_movement`'s pre-switch half, then `case 0`.
    let numbering = stream.command_numbering();
    let state = buf
        .body
        .interpreted
        .as_ref()
        .map(|w| interpreted_state_in(w, numbering));
    let style = numbering
        .command_from_wire(buf.body.current_style)
        .or_else(|| state.as_ref().map(|s| s.current_style));
    if let Some(c) = character.as_mut() {
        if let Some(style) = style {
            c.apply_movement_style(style);
        }
        if let Some(state) = state.as_ref() {
            // `is_the_player = true`: skips an action
            // node the player raised himself, which is the one rule in it that reads the flag.
            c.move_to_interpreted_state(state);
        }
    }
    if state.is_some() {
        // `case 0`'s tail, after `move_to_interpreted_state` and before the
        // `return 1`. The id is read from the buffer only when `MotionFlags & StickToObject`
        // is set, and `stick_to_object` is called only when it is non-zero.
        let stuck = buf
            .body
            .sticky_object
            .filter(|s| *s != ObjectId(0))
            .and_then(|s| stick_target(objects, character, stream, s));
        if let Some(c) = character.as_mut() {
            if let Some((target, radius, height)) = stuck {
                c.stick_to_object(target, radius, height)
            }
            // `standing_longjump = motion_flags & 0x200` -- assigned on
            // every `case 0`, set or clear, which is why it is outside the `if` above.
            c.driver_mut().movement.interp.standing_longjump = buf.body.motion_flags
                & dereth_protocol::movement::motion_flags::STANDING_LONG_JUMP
                != 0;
        }
        // `case 0` returns 1 without entering the switch.
        return applied;
    }

    let Ok(Some(arm)) = buf.body.decode_move_to() else {
        return applied;
    };
    let (req, params, run_rate) = move_to_request(objects, character, stream, &arm);
    debug_assert!(MovementParameters::default().has(flags::USE_SPHERES));
    if let Some(c) = character.as_mut() {
        c.perform_move_to(&req, &params, run_rate);
    }
    applied
}

/// Hand a server movement buffer to the motion runtime, including the full movement-manager
/// unpacking path for a remote object.
///
/// The wire carries **indices** into the client's 412-entry `command_ids` table, not command
/// ids (raw state first, then interpreted state), and
/// resolving them is `MotionCommand::from_index`'s job. **Retail does not check the index**:
/// `command_ids[index]` is a bare array read at six sites (one here plus five in
/// interpreted-state unpacking), so an index of 412 or more reads past the table. This
/// build drops such a buffer's word instead, which cannot fire
/// against anything the corpus contains: over the seven captures the s2c `0xF74C` stream
/// performs **5,286** index resolutions and the largest index in any of them is **339**.
///
/// The two crates on either side of this deliberately do not know each other:
/// `dereth_protocol::movement::InterpretedMotionState` is the wire shape and
/// `dereth_animation::motion::InterpretedMotionState` is the runtime one, and the application is the
/// only crate allowed to depend on both.
///
/// # The steps, in the order the client runs them
///
/// | step | what |
/// |---|---|
/// | `cancel_moveto` | cancels any move-to, **before the buffer is read** |
/// | `unstick_from_object` | drops any stick, likewise |
/// | style decoding | the pre-switch style word |
/// | `case 0` `move_to_interpreted_state` | the interpreted state |
/// | `case 0` `stick_to_object(id)` | `MotionFlags & StickToObject` |
/// | `case 0` `standing_longjump = flags & 0x200` | its sibling |
/// | `case 6`..`case 9` network unpack + movement manager | the four destinations |
///
/// Every step matters on the recorded traffic: the four non-`case 0` arms carry **83**
/// buffers in the seven captures (14 `MoveToObject`, 11 `MoveToPosition`, 53 `TurnToObject`,
/// 5 `TurnToHeading`) and the sticky flag another **46**; applying only the style and the
/// interpreted state would drop all 129.
///
/// **Translation is outside this seam.** This function hands the motion runtime its request;
/// moving the object is the per-object step's job, and without internal physics integration there
/// a remote creature given `MoveToPosition` would animate the requested walk but move only as fast
/// as the server's `0xF748` updates. The stick correction is applied separately: its offset
/// adjustment is a direct pull rather than an integration and precedes internal physics
/// integration during position update.
#[allow(clippy::too_many_arguments)] // one parameter per input the call takes
pub fn apply_movement<S: AsObjectSim>(
    objects: &mut BTreeMap<ObjectId, S>,
    character: &mut Option<Character>,
    stats: &mut ObjectStepStats,
    last_object_time: f64,
    id: ObjectId,
    buf: &dereth_protocol::movement::MovementBuffer,
    stream: &ObjectStream,
    is_the_player: bool,
) {
    // The movement-unpack path's first two statements, on the physics object and
    // before a single byte of the buffer has been looked at:
    //
    //     cancel any move-to on the physics object (passing 0x36, which the client's
    //     cancel never reads; ACE names 0x36 `ActionCancelled`);
    //     unstick this object's physics state from the supporting object;
    //
    // So a plain `case 0` buffer ends whatever approach and whatever stick were in force. In
    // the corpus this is what ends all seven of the recorded sticks: the server stops sending
    // `StickToObject` and the next ordinary buffer takes it off.
    crate::object_step::drive_object_motion(objects, character, stats, id, |m, ctx| {
        m.cancel_move_to(0x36, ctx);
        m.unstick_from_object(ctx);
    });

    let numbering = stream.command_numbering();
    let interpreted = buf
        .body
        .interpreted
        .as_ref()
        .map(|w| interpreted_state_in(w, numbering));
    // the style comes from the buffer's own
    // `style_ix`, which is a separate field from the state's.
    let style = numbering
        .command_from_wire(buf.body.current_style)
        .or_else(|| interpreted.as_ref().map(|s| s.current_style));
    if let Some(o) = objects.get_mut(&id).map(AsObjectSim::sim_mut) {
        if let Some(style) = style {
            o.driver.borrow_mut().apply_movement_style(style);
        }
    }

    if let Some(state) = interpreted {
        if let Some(o) = objects.get_mut(&id).map(AsObjectSim::sim_mut) {
            o.driver
                .borrow_mut()
                .move_to_interpreted_state(&state, is_the_player);
            // `standing_longjump = motion_flags & 0x200`. Unlike its sticky sibling this one is
            // inert on the recorded traffic -- **0** of the 615 remote buffers and 0 of the 1,606
            // player ones set the bit -- and it is wired anyway, because the two adjacent
            // statements belong together and leaving either out goes unnoticed.
            o.driver.borrow_mut().movement.interp.standing_longjump = buf.body.motion_flags
                & dereth_protocol::movement::motion_flags::STANDING_LONG_JUMP
                != 0;
        }
        // `if (sticky object id != 0) stick_to_object(physics_obj, sticky object id)`.
        let stuck = buf
            .body
            .sticky_object
            .filter(|s| *s != ObjectId(0))
            .and_then(|s| stick_target(&*objects, character, stream, s));
        if let Some((target, radius, height)) = stuck {
            let now = last_object_time;
            crate::object_step::drive_object_motion(objects, character, stats, id, |m, ctx| {
                m.stick_to_object(target, radius, now, ctx);
                // The original stick-to operation keeps the **height** too; the runtime
                // struct in `dereth-animation` does not carry it because `adjust_offset`
                // never reads one -- it works in the plane with z zeroed.
                let _ = height;
            });
            stats.remote_sticks_applied += 1;
        } else if buf.body.sticky_object.is_some() {
            // The object lookup missed: `stick_to_object` returns without touching the position
            // manager, so the object stays unstuck. Counted rather than silent.
            stats.remote_sticks_unresolved += 1;
        }
        // `case 0` returns 1 without entering the switch.
        return;
    }

    // `case 6`..`case 9`: movement-parameter unpacking and the
    // movement manager the four cases feed, for a remote object.
    let Ok(Some(arm)) = buf.body.decode_move_to() else {
        return;
    };
    let (req, params, run_rate) = move_to_request(&*objects, character, stream, &arm);
    crate::object_step::drive_object_motion(objects, character, stats, id, |m, ctx| {
        // `case 6`/`case 7` write the motion interpreter's `my_run_rate` from the float that
        // follows the parameters, before `MoveToObject`/`MoveToPosition` is called.
        if let Some(r) = run_rate {
            m.interp.my_run_rate = r;
        }
        m.perform_movement(&req, &params, ctx);
    });
    stats.remote_move_tos_performed += 1;
}

/// The wire's [`dereth_protocol::movement::InterpretedMotionState`] as the motion runtime's.
///
/// The wire carries **indices** into the client's 412-entry `command_ids` table, not command
/// ids (raw state first, then interpreted state), and
/// resolving them is `MotionCommand::from_index`'s job. An index the table does not hold is
/// dropped rather than guessed at; the shipped table is dense, so this never fires against a
/// real server, and if it ever does the object simply keeps the motion it had.
///
/// The two crates on either side of this deliberately do not know each other:
/// `dereth_protocol::movement::InterpretedMotionState` is the wire shape and
/// `dereth_animation::motion::InterpretedMotionState` is the runtime one, and the application is the
/// only crate allowed to depend on both.
///
/// Separate from [`apply_movement`] because `apply_player_movement` needs the same conversion
/// for the **local** player, who never passes through that function.
///
/// The indices are read in the final numbering; [`interpreted_state_in`] reads them in a world's.
pub fn interpreted_state(
    wire: &dereth_protocol::movement::InterpretedMotionState,
) -> dereth_animation::motion::InterpretedMotionState {
    interpreted_state_in(
        wire,
        dereth_world_data::command_numbering::CommandNumbering::Final,
    )
}

/// [`interpreted_state`] with the indices in `numbering`, the world files' command numbering: a
/// server for a world of older files numbers its commands as the client of their day did, and
/// the motion runtime works in the final numbering.
pub fn interpreted_state_in(
    wire: &dereth_protocol::movement::InterpretedMotionState,
    numbering: dereth_world_data::command_numbering::CommandNumbering,
) -> dereth_animation::motion::InterpretedMotionState {
    use dereth_animation::motion::{ActionNode, InterpretedMotionState};
    let cmd = |i: Option<u16>, fallback: MotionCommand| {
        i.and_then(|i| numbering.command_from_wire(i))
            .unwrap_or(fallback)
    };
    let base = InterpretedMotionState::default();
    InterpretedMotionState {
        current_style: cmd(wire.current_style, base.current_style),
        forward_command: cmd(wire.forward_command, base.forward_command),
        forward_speed: wire.forward_speed.unwrap_or(base.forward_speed),
        sidestep_command: cmd(wire.sidestep_command, base.sidestep_command),
        sidestep_speed: wire.sidestep_speed.unwrap_or(base.sidestep_speed),
        turn_command: cmd(wire.turn_command, base.turn_command),
        turn_speed: wire.turn_speed.unwrap_or(base.turn_speed),
        actions: wire
            .actions
            .iter()
            .filter_map(|a| {
                Some(ActionNode {
                    action: numbering.command_from_wire(a.command_index)?,
                    speed: a.speed,
                    stamp: u32::from(a.stamp()),
                    autonomous: a.autonomous(),
                })
            })
            .collect(),
    }
}

/// Decode a cell id and an origin, with the orientation left at
/// identity, matching the position passed to the move-to-position request.
pub fn origin_position(o: &dereth_protocol::types::Origin) -> dereth_primitives::Position {
    dereth_primitives::Position::new(
        CellId(o.objcell_id),
        dereth_primitives::Frame::new(
            dereth_primitives::Vec3::new(o.origin.x, o.origin.y, o.origin.z),
            dereth_primitives::Quat::IDENTITY,
        ),
    )
}
