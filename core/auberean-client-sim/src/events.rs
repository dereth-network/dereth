//! The server's events for bodies, as the bodies take them: what it asks of a body's physics (a
//! push, a push away from another object, a missile launched, its speed, acceleration and jump
//! scales, the motion description it moves by, what the ground does to its motion, its effect
//! scripts and the layers and behaviors it stops), and the moves it orders the player's body to
//! make (turning to face what it attacks, walking up to what it uses).
//!
//! What it asks of the bodies' figures, their effects and their stories is a window's to play.

use auberean_animation::CachedRecords;
use auberean_assets::animation::MotionDesc;
use auberean_physics::body::missile::NoObjects;
use auberean_physics::body::{Animate, Body, Notice};
use auberean_physics::land::Land;
use auberean_physics::motion::moveto::TargetBody;
use auberean_physics::motion::params::MovementParameters;
use auberean_physics::world::World;
use auberean_protocol::physics::{Missile, MoveOrder, TargetedImpulse};
use auberean_protocol::types::{MoveTarget, MovementParams, Placement};
use auberean_protocol::ServerMessage;
use auberean_world::records::WorldRecords;
use dereth_primitives::{CellId, Frame as PFrame, Position};

use crate::player::{pv, Player};

/// The place a placement on the wire names.
#[must_use]
pub fn position_of_placement(p: &Placement) -> Position {
    let q = p.frame.rotation;
    Position::new(
        CellId(p.cell),
        PFrame::new(
            pv(p.frame.origin),
            dereth_primitives::Quat {
                w: q.w,
                x: q.x,
                y: q.y,
                z: q.z,
            },
        ),
    )
}

/// A move the server orders, as the body takes it: each part the order carries (by its flags),
/// the rest as a move made afresh has them.
#[must_use]
pub fn movement_parameters(w: &MovementParams) -> MovementParameters {
    let mut p = MovementParameters {
        flags: w.flags,
        ..Default::default()
    };
    if w.flags & (MovementParams::MAX_SUCCESS | MovementParams::MIN_SUCCESS) != 0 {
        p.desired_velocity = w.desired_velocity;
        p.fail_distance = w.fail_distance;
        p.desired_distance = w.desired_distance;
    }
    if w.flags & MovementParams::MAX_SUCCESS != 0 {
        p.max_success_distance = w.max_success_distance;
    }
    if w.flags & MovementParams::MIN_SUCCESS != 0 {
        p.min_success_distance = w.min_success_distance;
    }
    match &w.target {
        MoveTarget::Object(id) => p.target_object = *id,
        MoveTarget::Place(at) => p.target_position = position_of_placement(at),
    }
    p.offset = pv(w.offset);
    p.offset_id = w.offset_id;
    p.waypoints = w.waypoints.iter().map(position_of_placement).collect();
    p.context = w.context;
    p
}

/// Plays event `m` on body `b` when it is one a body takes by itself: a push; its speed,
/// acceleration and jump scales; the motion description it moves by (`records` holding it), which
/// wakes its movement; an effect script, a layer stopped, the behaviors the server asked for
/// stopped and those a move cancels, which its figure is asked in turn; how high it hovers, the
/// modifier on all its motion and what each kind of ground does to it. Whether it was one.
// D2-05905
pub fn play_on_body(records: &WorldRecords, b: &mut Body, m: &ServerMessage) -> bool {
    match m {
        ServerMessage::VelocityScale(e) => b.set_velocity_scale(e.action.0),
        ServerMessage::AccelerationScale(e) => b.set_acceleration_scale(e.action.0),
        ServerMessage::JumpScale(e) => b.set_jump_scale(e.action.0),
        ServerMessage::MotionInterpChange(e) => {
            if let Some(md) = records.get::<MotionDesc>(e.action.0) {
                b.minterp
                    .set_description(e.action.0, std::sync::Arc::new(md));
                b.set_movement_active(true);
            }
        }
        ServerMessage::Push(e) => {
            b.add_impulse(pv(e.action.direction));
        }
        ServerMessage::DoFxScript(e) => {
            b.receive_do_fx_script(e.action.0);
        }
        ServerMessage::EndLayer(e) => b.receive_stop_layer(e.action.0),
        ServerMessage::StopAllBehaviors(_) => b.receive_stop_all_server_behaviors(),
        ServerMessage::StopMoveCancelsBehaviors(_) => {
            b.receive_stop_move_to_cancels_behaviors();
        }
        ServerMessage::HoverHeight(e) => b.set_hover_height(e.action.0),
        ServerMessage::GlobalModifier(e) => b.set_global_modifier(e.action.0),
        ServerMessage::TerrainChange(e) => {
            b.add_terrain_modifier(e.action.terrain, e.action.modifier);
        }
        ServerMessage::ClearTerrainChanges(_) => b.clear_terrain_modifiers(),
        _ => return false,
    }
    true
}

/// The server pushes body `b` away from another object, which stands at `at`: measured from
/// there; what the body kept of that object for a move stays as it was.
// D2-05844
pub fn push_toward(b: &mut Body, at: Position, a: &TargetedImpulse) {
    let kept = b.targets.insert(
        a.target,
        TargetBody {
            position: at,
            radius: 0.0,
            height: 0.0,
        },
    );
    b.add_targeted_impulse(a.target, a.magnitude, a.falloff != 0, a.vertical);
    match kept {
        Some(k) => {
            b.targets.insert(a.target, k);
        }
        None => {
            b.targets.remove(&a.target);
        }
    }
}

/// The server launches body `b` as a missile against the world (`land`, `table`): it flies at the
/// velocity given toward where its target is.
pub fn initialize_missile(b: &mut Body, land: &Land, table: &mut World, m: &Missile) {
    let target = auberean_physics::target::Aim {
        object: m.target,
        position: position_of_placement(&m.target_offset),
        offset: pv(m.target_velocity),
        valid: m.initialized != 0,
    };
    b.receive_initialize_missile(land, table, &mut NoObjects, pv(m.velocity), &target);
}

impl<F> Player<F> {
    /// A move the server orders the player's body to make (turning to face what it attacks,
    /// walking up to what it uses): the behaviors a move cancels stop on its figure as it is taken,
    /// before what the server asks after it.
    pub fn receive_move_to(&mut self, anims: &CachedRecords<'_>, order: &MoveOrder) {
        let start = self.body.notices.len();
        let _ = self
            .body
            .receive_move_to(&movement_parameters(&order.params), order.stamp);
        let mut i = start;
        while i < self.body.notices.len() {
            if matches!(
                self.body.notices[i],
                Notice::Animate(Animate::StopMoveToCancels)
            ) {
                self.body.notices.remove(i);
                self.figure.stop_move_to_cancels(anims);
            } else {
                i += 1;
            }
        }
    }

    /// The object the player's body moves or turns to, while it moves to one.
    #[must_use]
    pub fn move_target(&self) -> Option<u64> {
        self.body
            .minterp
            .move_to
            .as_ref()
            .filter(|m| m.moving_to && m.params.to_object())
            .map(|m| m.params.target_object)
    }

    /// Where the object the player's body moves or turns to stands, kept on the body for its move:
    /// the body `find` gives for it, when that body is in the world (its place, radius and
    /// height); none when there is no such body.
    pub fn keep_move_target<'b>(&mut self, find: impl FnOnce(u64) -> Option<&'b Body>) {
        let target = self.move_target();
        self.body.targets.clear();
        let Some(id) = target else {
            return;
        };
        let Some(b) = find(id).filter(|b| b.in_world) else {
            return;
        };
        self.body.targets.insert(
            id,
            TargetBody {
                position: b.position,
                radius: b.radius(),
                height: b.height(),
            },
        );
    }
}
