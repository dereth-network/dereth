//! Motion and teleport wire boundaries.

/// The four sequences every outbound movement pack echoes back, off the object
/// table.
///
/// `send_movement_event` and `send_position_event` both pass
/// `update_times[8], [5], [4], [6]` — the instance, server-controlled-move, teleport and
/// force-position timestamps — in that argument order. They are the *server's* numbers and live on
/// [`crate::objects::Presence`], not on the body, which is why [`body_motion`] takes them rather
/// than reaching for them.
///
/// Free and `pub` for the same reason `body_motion` is: this is the only route from the object
/// table to the wire, so a test that asserts the echo has to be able to call exactly what
/// `App::player_motion` calls. Wiring it to the wrong slot would otherwise be invisible to
/// everything except a driven session.
#[must_use]
pub fn player_timestamps(
    presence: Option<&crate::objects::Presence>,
) -> dereth_protocol::movement::MoveTimestamps {
    dereth_protocol::movement::MoveTimestamps {
        instance: presence.map_or(0, |p| p.instance),
        server_control: presence.map_or(0, |p| p.server_control_ts),
        teleport: presence.map_or(0, |p| p.teleport_ts),
        force_position: presence.map_or(0, |p| p.force_position_ts),
    }
}

/// Legacy isolated/debug ground-edge observer, retained for the jump body tests. Actual App input
/// uses the direct result and immediate physical velocity in crate::jump;
/// it does not infer acceptance from a later physics edge.
#[must_use]
pub fn jump_edge_sample(character: &crate::character::Character) -> u64 {
    character.ground_edges().left
}

#[must_use]
pub fn body_jump(
    character: &crate::character::Character,
    left_before: u64,
) -> (bool, dereth_protocol::types::Vec3) {
    let accepted = character.ground_edges().left > left_before;
    let rotation = character.position().frame.rotation;
    let global = character
        .world
        .get(character.handle)
        .map_or(dereth_primitives::Vec3::ZERO, |o| o.velocity_vector);
    let local = dereth_physics::math::globaltolocalvec(dereth_physics::math::l2g(rotation), global);
    (
        accepted,
        dereth_protocol::types::Vec3 {
            x: local.x,
            y: local.y,
            z: local.z,
        },
    )
}

/// The control transfer between the server and
/// [`crate::character::MovementCommands`], both halves, once per frame.
///
/// Lose control to the server when the position update returns 1, then
/// apply the command interpreter's control-retake decision and, on it,
/// `take_control_from_server`'s body half. Returns `(lost, retook)`.
///
/// **Free and `pub` for the same reason [`apply_player_teleport`] and [`body_motion`] are**: this
/// is the application's own path, so a test that drives a real body over a real capture must be
/// able to call exactly what [`App::command_interpreter_control_transfer`] calls, without a
/// device, a window or a link. Without that the only observable is the call counter, and a
/// mutation that empties the body of the step survives it (see
/// [`App::player_teleport_use_time`]).
///
/// The order inside is retail's and is load-bearing: the loss must precede the retake, because
/// the per-frame step's first term is `controlled_by_server` and `lose_control_to_server` is what
/// sets it. With no body there is no retake at all: the per-frame step returns before sampling
/// any other gate.
pub fn command_interpreter_control_transfer(
    scene: Option<&mut (dyn crate::present::SceneMut + '_)>,
    movement: &mut crate::character::MovementCommands,
    input: &mut crate::character::CharacterInput,
) -> (bool, bool) {
    command_interpreter_control_transfer_with_finish(scene, movement, input, |character| {
        if let Some(character) = character {
            character.finish_jump();
        }
    })
}

/// App-owned finish_jump at the accepted loss boundary; compatibility callers above have no
/// combat model but still clear the actual body before the retake gate is sampled.
pub(super) fn command_interpreter_control_transfer_with_finish(
    scene: Option<&mut (dyn crate::present::SceneMut + '_)>,
    movement: &mut crate::character::MovementCommands,
    input: &mut crate::character::CharacterInput,
    finish_jump: impl FnOnce(Option<&crate::character::Character>),
) -> (bool, bool) {
    let Some(scene) = scene else {
        return (false, false);
    };
    // The smart box's event dispatch loses control to the server when setting the object's
    // movement returns non-zero, taken
    // off the latch `world.rs::apply_player_movement` set on this frame's dispatch.
    let lost = scene.world_mut().take_player_movement_applied();
    if lost {
        movement.lose_control_to_server_with_finish(input, || finish_jump(scene.character()));
    }
    // Sample `motions_pending` and `is_moving_to` from the local body. They are copied into a pair
    // so the scene borrow is released before the retake, which
    // takes the same body mutably.
    let Some(gates) = scene.character().map(|c| {
        let d = c.driver();
        (d.movement.motions_pending(), d.movement.is_moving_to())
    }) else {
        return (lost, false);
    };
    if !movement.use_time(gates.0, gates.1, input) {
        return (lost, false);
    }
    if let Some(c) = scene.world_mut().character.as_mut() {
        c.take_control_from_server();
    }
    (lost, true)
}

/// Apply a pending server teleport to the player's own body. With no player the
/// operation is a no-op; otherwise it sets the position and runs the player-position-updated tail.
///
/// [`crate::character::Character::teleport`] **is** the client's simple position set with its
/// teleport flag — it normalises an outdoor cell, re-enters the cell, restarts the object's clock
/// and resets the camera smoother, all of which that position set's `set_frame`/`enter_cell` path
/// does. This is the **call**.
///
/// The implemented player-position-updated (teleport) tail now performs teleport_hook's movement,
/// sticky and target cleanup. The command-interpreter tail is composed by
/// [`complete_player_teleport`]. Interpolation, constraints, voyeur notifications and complete
/// collision-end bookkeeping remain explicit Character-side limitations, not audio-only work.
///
/// Free and `pub` for the same reason [`body_motion`] and [`player_timestamps`] are: this is the
/// application's own path from the object stream to the body, so a test that asserts the body
/// moved has to be able to call exactly what [`App::player_teleport_use_time`] calls. Returns the
/// destination when a teleport was pending, `None` otherwise.
pub fn apply_player_teleport(
    objects: &mut crate::objects::ObjectStream,
    character: &mut crate::character::Character,
) -> Option<dereth_primitives::Position> {
    let pos = objects.take_player_teleport()?;
    apply_player_teleport_at(pos, character);
    Some(pos)
}

fn apply_player_teleport_at(
    pos: dereth_primitives::Position,
    character: &mut crate::character::Character,
) {
    // **This build's prerequisite, not retail's.** The landscape holds every loaded block and the
    // client's position setting finds the destination cell already there; this build's land
    // source is prefetched per block, and `App::load_pending_scene` prefetches the window it builds
    // (`character.land().load_block_cells(..)`). A teleport moves the body into a block
    // the window has never held, so the same prefetch has to happen here or the body enters a cell
    // with no interior geometry under it. Cheap and idempotent: the source caches per block.
    #[allow(clippy::cast_possible_truncation)]
    // LINT-OK: a cell id's top 16 bits are its landblock. Not a float conversion.
    let block = dereth_primitives::LandblockId((pos.cell.0 >> 16) as u16);
    character.land().load_block_cells(block);
    character.teleport(pos);
    // The player-position-updated (teleport) path calls teleport_hook after the position set. Keep
    // this on the accepted TELEPORT_TS edge, not Character::teleport's initial-placement callers.
    character.player_teleport_hook();
}

/// The accepted teleport chain: teleport the player, update its position, report it teleported.
/// The send tail runs after body cleanup and set_auto_run(0, 1)'s actual raw-motion reapplication,
/// even when autorun was already off. The callback lets App supply its existing session sender
/// and lets headless component tests inspect its real outgoing payload without a live link.
pub fn complete_player_teleport(
    objects: &mut crate::objects::ObjectStream,
    character: &mut crate::character::Character,
    movement: &mut crate::character::MovementCommands,
    input: &mut crate::character::CharacterInput,
    send_movement: impl FnOnce(&crate::character::Character),
) -> Option<dereth_primitives::Position> {
    let pos = objects.take_player_teleport()?;
    complete_player_teleport_at(pos, character, movement, input, send_movement);
    Some(pos)
}

/// One already-accepted teleport, also used by the ordered App dispatcher. The caller supplies
/// the edge's own position and captures its timestamp snapshot in the sender closure.
pub fn complete_player_teleport_at(
    pos: dereth_primitives::Position,
    character: &mut crate::character::Character,
    movement: &mut crate::character::MovementCommands,
    input: &mut crate::character::CharacterInput,
    send_movement: impl FnOnce(&crate::character::Character),
) {
    apply_player_teleport_at(pos, character);
    if movement.player_teleported(input) {
        character.reapply_teleport_input(*input);
    }
    send_movement(character);
}

/// Everything the two senders read off the physics object, as one value.
///
/// Free rather than a method so a test can build a real [`crate::character::Character`] over the
/// retail landblocks and assert this translation without a device, a link or an `App` — the
/// `App` method is one call to it. The four timestamps are the caller's because they are the
/// *server's* sequence numbers and live on the object table, not on the body.
#[must_use]
pub fn body_motion(
    character: &crate::character::Character,
    timestamps: dereth_protocol::movement::MoveTimestamps,
) -> dereth_client_net::client_session::PlayerMotion {
    body_motion_in(
        character,
        timestamps,
        dereth_world_data::command_numbering::CommandNumbering::Final,
    )
}

/// [`body_motion`] with the motion commands in `numbering`, the world files' command numbering
/// ([`raw_motion_state_to_wire_in`]).
#[must_use]
pub fn body_motion_in(
    character: &crate::character::Character,
    timestamps: dereth_protocol::movement::MoveTimestamps,
    numbering: dereth_world_data::command_numbering::CommandNumbering,
) -> dereth_client_net::client_session::PlayerMotion {
    let pos = character.position();
    let position = dereth_protocol::types::PositionWire {
        objcell_id: pos.cell.0,
        frame: dereth_protocol::types::Frame {
            origin: dereth_protocol::types::Vec3 {
                x: pos.frame.origin.x,
                y: pos.frame.origin.y,
                z: pos.frame.origin.z,
            },
            orientation: dereth_protocol::types::Quat {
                w: pos.frame.rotation.w,
                x: pos.frame.rotation.x,
                y: pos.frame.rotation.y,
                z: pos.frame.rotation.z,
            },
        },
    };
    // Position validity requires an inbound-valid cell id (transcribed in
    // `dereth_physics::landdefs`, not copied) and seven finite frame components.
    let position_valid = dereth_physics::landdefs::inbound_valid_cellid(pos.cell)
        && ![
            pos.frame.origin.x,
            pos.frame.origin.y,
            pos.frame.origin.z,
            pos.frame.rotation.w,
            pos.frame.rotation.x,
            pos.frame.rotation.y,
            pos.frame.rotation.z,
        ]
        .iter()
        .any(|f| f.is_nan());
    let plane = character
        .world
        .get(character.handle)
        .map(|o| o.contact_plane)
        .unwrap_or_default();
    let (raw_motion_state, longjump_mode) = {
        let driver = character.driver();
        (
            raw_motion_state_to_wire_in(&driver.movement.interp.raw_state, numbering),
            driver.movement.interp.standing_longjump,
        )
    };
    dereth_client_net::client_session::PlayerMotion {
        position,
        position_valid,
        timestamps,
        // `send_position_event` requires both contact and walkable-surface state, exactly the
        // condition represented by `Character::on_ground`.
        contact: character.on_ground(),
        longjump_mode,
        raw_motion_state,
        contact_plane: dereth_client_net::client_session::ContactPlane {
            normal: dereth_protocol::types::Vec3 {
                x: plane.normal.x,
                y: plane.normal.y,
                z: plane.normal.z,
            },
            d: plane.d,
        },
    }
}

/// Pack the player's raw motion state into its wire form.
///
/// Each field's presence bit is decided by comparing it with a default:
///
/// | bit | field | present when |
/// |---:|---|---|
/// | 0 | `current_holdkey` | `!=` the no-hold-key value (1) |
/// | 1 | `current_style` | `!= 0x8000003D` (`NonCombat`) |
/// | 2 | `forward_command` | `!= 0x41000003` (`Ready`) |
/// | 3 | `forward_holdkey` | `!=` the invalid hold key (0) |
/// | 4 | `forward_speed` | `!= 1.0` |
/// | 5 | `sidestep_command` | `!= 0` |
/// | 6 | `sidestep_holdkey` | `!=` the invalid hold key |
/// | 7 | `sidestep_speed` | `!= 1.0` |
/// | 8 | `turn_command` | `!= 0` |
/// | 9 | `turn_holdkey` | `!=` the invalid hold key |
/// | 10 | `turn_speed` | `!= 1.0` |
/// | 11–15 | the action count |
///
/// Those are exactly the animation interpreter's pending movement fields, which transcribe the same
/// constructor defaults, so this is `Some(x) if x != default` field by field and nothing else.
///
///
/// **What the corpus exercises.** Across the 1,187 recorded `0xF61C` bodies the flags word takes
/// nine distinct values, all of them below `0x800`: `current_holdkey` appears in 1,182 (always
/// the run hold key, 2), `current_style` in 175, `forward_command` in 673 and `turn_command` in
/// 524.
/// **No recorded body carries a queued action, 0 of 1,187**, and none carries a per-axis hold key
/// or a speed other than 1.0 — so the action arm and five of the eleven presence bits below are
/// transcribed but unexercised by the corpus, which is stated here rather than implied by a
/// passing test.
pub fn raw_motion_state_to_wire(
    s: &dereth_animation::motion::RawMotionState,
) -> dereth_protocol::movement::RawMotionState {
    raw_motion_state_to_wire_in(
        s,
        dereth_world_data::command_numbering::CommandNumbering::Final,
    )
}

/// [`raw_motion_state_to_wire`] with the commands in `numbering`, the world files' command
/// numbering: the client of an older world's day wrote its style and commands as its files
/// numbered them. The presence bits are decided in the final numbering, before the translation;
/// a command the numbering lacks is left out (its bit clear), as one the table does not name.
pub fn raw_motion_state_to_wire_in(
    s: &dereth_animation::motion::RawMotionState,
    numbering: dereth_world_data::command_numbering::CommandNumbering,
) -> dereth_protocol::movement::RawMotionState {
    use dereth_animation::command::MotionCommand;
    use dereth_animation::motion::HoldKey;

    let opt_key = |k: HoldKey, default: HoldKey| (k != default).then_some(k as u32);
    let opt_cmd = |c: MotionCommand, default: u32| {
        (c.0 != default).then_some(c).and_then(|c| match numbering {
            dereth_world_data::command_numbering::CommandNumbering::Final => Some(c.0),
            n => n.from_final(c),
        })
    };
    let opt_speed = |v: f32| (v != 1.0).then_some(v);

    dereth_protocol::movement::RawMotionState {
        current_holdkey: opt_key(s.current_holdkey, HoldKey::None),
        current_style: opt_cmd(s.current_style, MotionCommand::NON_COMBAT.0),
        forward_command: opt_cmd(s.forward_command, MotionCommand::READY.0),
        forward_holdkey: opt_key(s.forward_holdkey, HoldKey::Invalid),
        forward_speed: opt_speed(s.forward_speed),
        sidestep_command: opt_cmd(s.sidestep_command, 0),
        sidestep_holdkey: opt_key(s.sidestep_holdkey, HoldKey::Invalid),
        sidestep_speed: opt_speed(s.sidestep_speed),
        turn_command: opt_cmd(s.turn_command, 0),
        turn_holdkey: opt_key(s.turn_holdkey, HoldKey::Invalid),
        turn_speed: opt_speed(s.turn_speed),
        // `command_index` is the index into the client's 412-entry `command_ids` table, never the
        // id itself; `MotionCommand::to_index` is that table. A command the table does not name
        // cannot be expressed on the wire, so it is dropped rather than sent as a wrong index.
        actions: s
            .actions
            .iter()
            .filter_map(|a| {
                numbering.command_to_wire(a.action).map(|command_index| {
                    dereth_protocol::movement::MotionAction {
                        command_index,
                        // `(stamp & 0x7FFF) | (autonomous ? 0x8000 : 0)`; the mask makes the
                        // narrowing total, so the fallback is unreachable rather than a guess.
                        stamp_and_autonomy: u16::try_from(a.stamp & 0x7FFF).unwrap_or(0)
                            | if a.autonomous { 0x8000 } else { 0 },
                        speed: a.speed,
                    }
                })
            })
            .take(31)
            .collect(),
    }
}
