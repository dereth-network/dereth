//! Motion and teleport wire boundaries.

/// The four sequences every outbound movement pack echoes back, off the object
/// table.
///
/// `send_movement_event` and `send_position_event` both pass
/// `update_times[8], [5], [4], [6]` — the instance, server-controlled-move, teleport and
/// force-position timestamps — in that argument order. They are the *server's* numbers and live on
/// [`crate::objects::Presence`], not on the body, which is why `body_motion` takes them rather
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

pub(super) fn apply_player_teleport_at(
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

/// `body_motion` with the motion commands in `numbering`, the world files' command numbering
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

/// `raw_motion_state_to_wire` with the commands in `numbering`, the world files' command
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
