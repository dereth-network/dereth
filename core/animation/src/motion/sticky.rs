//! `StickyManager`: the "stick to that object" end state of a move-to.
//!
//! Three constants are the whole behaviour: the **−0.3 m** stand-off, the **×5.0** catch-up speed
//! multiplier, and the **1 second** timeout after `StickTo` (not after target updates).

use dereth_primitives::{Frame, ObjectId, Position, Vec3};

use super::interp::MotionCtx;
use super::moveto::{distance, position_heading};
use super::{MotionEffect, MotionInterp};
use crate::frame::{get_heading, globaltolocalvec, l2g, set_heading, V3};

/// The sticky stand-off distance.
pub const STICKY_STANDOFF: f32 = 0.3;
/// The sticky catch-up speed multiplier.
pub const STICKY_SPEED_MULTIPLIER: f32 = 5.0;
/// The floor used when there is no motion interpreter to ask for a max speed.
pub const STICKY_DEFAULT_MAX_SPEED: f32 = 15.0;
/// How long a stick survives after `StickTo`; only another `StickTo` renews it.
pub const STICKY_TIMEOUT: f32 = 1.0;

/// `StickyManager`.
#[derive(Debug, Default, Clone)]
pub struct StickyManager {
    pub target_id: ObjectId,
    pub target_radius: f32,
    pub target_position: Option<Position>,
    pub initialized: bool,
    pub sticky_timeout_time: f64,
}

impl StickyManager {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn is_sticky(&self) -> bool {
        self.target_id != ObjectId(0)
    }

    /// Stick to a target.
    pub fn stick_to(&mut self, target: ObjectId, radius: f32, now: f64, ctx: &mut MotionCtx<'_>) {
        self.unstick(ctx);
        self.stick_to_after_unstick(target, radius, now, ctx);
    }

    /// Replacement installation after the stick-to entry's synchronous UnStick callbacks have returned.
    pub(super) fn stick_to_after_unstick(
        &mut self,
        target: ObjectId,
        radius: f32,
        now: f64,
        ctx: &mut MotionCtx<'_>,
    ) {
        self.target_radius = radius;
        self.target_id = target;
        self.initialized = false;
        // The client loads 1.0, adds the current time and stores the sum as a double.
        // The current time is not narrowed to a float on the way.
        self.sticky_timeout_time = now + f64::from(STICKY_TIMEOUT);
        ctx.effects.push(MotionEffect::SetTarget {
            id: target,
            radius: super::moveto::TARGET_RADIUS,
            quantum: 0.5,
        });
    }

    /// Stick cancellation's state reset, before its two physics callbacks.
    /// Kept separate so MovementManager can resolve the cancellation synchronously.
    pub(super) fn clear_stick(&mut self) -> bool {
        if !self.is_sticky() {
            return false;
        }
        self.target_id = ObjectId(0);
        self.initialized = false;
        // Retail retains the cached position/radius/deadline. adjust_offset checks both the
        // target id and initialized before reading that position, including after a new StickTo.
        true
    }

    /// Isolated unstick adapter. Its host must resolve the
    /// ClearTarget/CancelMoveTo callbacks in order before installing a replacement operation.
    /// A host owning MovementManager can instead use its synchronous unstick_from_object.
    pub fn unstick(&mut self, ctx: &mut MotionCtx<'_>) {
        if self.clear_stick() {
            ctx.effects.push(MotionEffect::ClearTarget);
            ctx.effects.push(MotionEffect::CancelMoveTo);
        }
    }

    /// Refresh on success, drop the stick on any
    /// other status.
    pub fn handle_update_target(
        &mut self,
        id: ObjectId,
        ok: bool,
        position: Position,
        _now: f64,
        ctx: &mut MotionCtx<'_>,
    ) {
        if id != self.target_id {
            return;
        }
        if !ok {
            self.unstick(ctx);
            return;
        }
        self.target_position = Some(position);
        self.initialized = true;
    }

    /// The stick expires **1 second** after `StickTo`.
    /// A target update does not write the deadline, despite the earlier inference that a
    /// successful target update renews it.
    pub fn use_time(&mut self, ctx: &mut MotionCtx<'_>) {
        if !self.is_sticky() {
            return;
        }
        let now = ctx.env.cur_time.0;
        if self.sticky_timeout_time < now {
            self.unstick(ctx);
        }
    }

    /// The per-frame nudge toward the stuck object.
    ///
    /// The offset is computed in the object's **local** frame with z zeroed, capped at
    /// `max_speed × 5 × quantum` (floored at 15 m/s when there is no interpreter), and the frame's
    /// heading is set to face the target.
    pub fn adjust_offset(
        &self,
        frame: &mut Frame,
        quantum: f32,
        my_position: &Position,
        my_radius: f32,
        interp: Option<&MotionInterp>,
        env: &super::MotionEnv,
    ) {
        if !self.is_sticky() || !self.initialized {
            return;
        }
        let Some(target) = self.target_position else {
            return;
        };
        let global = super::moveto::get_offset(my_position, &target);
        let mut off = globaltolocalvec(l2g(my_position.frame.rotation), global);
        off.z = 0.0;
        // The radial helper still measures the 3-D centre distance;
        // unlike cylinder_distance it subtracts the radii without a second height-gap term.
        let mut d =
            distance(my_position, &target) - (my_radius + self.target_radius) - STICKY_STANDOFF;
        if off.normalize_check_small() {
            off = Vec3::ZERO;
        }
        let mut maxs = interp.map_or(0.0, |i| i.get_max_speed(env) * STICKY_SPEED_MULTIPLIER);
        if maxs < dereth_primitives::num::consts::EPSILON {
            maxs = STICKY_DEFAULT_MAX_SPEED;
        }
        let step = maxs * quantum;
        if d.abs() <= step {
            off = off.mul(d);
        } else {
            off = off.mul(step);
            d = step;
        }
        // z was zeroed above; the multiply is retained from the original, where it is a no-op.
        off.z *= d;
        frame.origin = off;
        let h = position_heading(my_position, &target) - get_heading(&my_position.frame);
        let h = if h < 0.0 { h + 360.0 } else { h };
        set_heading(frame, h);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::NoAssets;
    use crate::motion::MotionEnv;
    use crate::seq::Sequence;
    use crate::table::MotionTableManager;
    use dereth_primitives::{CellId, LocalTime, Quat};

    fn pos(x: f32, y: f32) -> Position {
        Position::new(
            CellId(0xA9B4_0001),
            Frame::new(Vec3::new(x, y, 0.0), Quat::IDENTITY),
        )
    }

    struct H {
        mgr: MotionTableManager,
        seq: Sequence,
        env: MotionEnv,
        effects: Vec<MotionEffect>,
        events: Vec<crate::hooks::AnimEvent>,
    }

    impl H {
        fn new(now: f64) -> Self {
            Self {
                mgr: MotionTableManager::new(None),
                seq: Sequence::new(),
                env: MotionEnv {
                    cur_time: LocalTime(now),
                    ..MotionEnv::default()
                },
                effects: Vec::new(),
                events: Vec::new(),
            }
        }
        fn ctx(&mut self) -> MotionCtx<'_> {
            MotionCtx {
                mgr: &mut self.mgr,
                seq: &mut self.seq,
                assets: &NoAssets,
                env: &self.env,
                effects: &mut self.effects,
                events: &mut self.events,
            }
        }
    }

    /// Primary unpack prefix and UnStick. Constructed manager states,
    /// not a retail packet or a DAT animation journey.
    #[test]
    fn received_prefix_cancels_and_unsticks_once_before_a_replacement_target() {
        use crate::motion::{MoveToRequest, MovementManager, MovementParameters};
        for old_move in [false, true] {
            for old_stick in [false, true] {
                let mut h = H::new(100.0);
                let mut m = MovementManager::new();
                let old = ObjectId(0x7000_0001);
                let new = ObjectId(0x7000_0002);
                let req = |id| MoveToRequest::TurnToObject {
                    object_id: id,
                    top_level_id: id,
                };
                if old_move {
                    m.perform_movement(&req(old), &MovementParameters::default(), &mut h.ctx());
                    assert!(m.is_moving_to());
                }
                if old_stick {
                    m.sticky.stick_to(old, 0.4, 100.0, &mut h.ctx());
                    m.sticky
                        .handle_update_target(old, true, pos(1.0, 0.0), 100.0, &mut h.ctx());
                    assert!(m.sticky.initialized);
                }
                h.effects.clear();
                m.prepare_received_movement(&mut h.ctx());
                assert!(!m.is_moving_to() && !m.sticky.is_sticky() && !m.sticky.initialized);
                assert_eq!(
                    h.effects
                        .iter()
                        .filter(|e| matches!(e, MotionEffect::MoveToFailed(0x36)))
                        .count(),
                    usize::from(old_move)
                );
                assert_eq!(
                    h.effects
                        .iter()
                        .filter(|e| matches!(e, MotionEffect::ClearTarget))
                        .count(),
                    usize::from(old_move) + usize::from(old_stick)
                );
                assert!(!h.effects.iter().any(|e| matches!(
                    e,
                    MotionEffect::CancelMoveTo | MotionEffect::UnstickFromObject
                )));
                if old_move && old_stick {
                    assert!(matches!(
                        h.effects.first(),
                        Some(MotionEffect::MoveToFailed(0x36))
                    ));
                    assert_eq!(h.effects.last(), Some(&MotionEffect::ClearTarget));
                }
                h.effects.clear();
                let raw = m.interp.raw_state.clone();
                let interpreted = m.interp.interpreted_state.clone();
                m.prepare_received_movement(&mut h.ctx());
                assert!(
                    h.effects.is_empty(),
                    "repeated prefix has no cancellation/reporting tail"
                );
                assert_eq!(m.interp.raw_state, raw);
                assert_eq!(m.interp.interpreted_state, interpreted);
                m.perform_movement(&req(new), &MovementParameters::default(), &mut h.ctx());
                assert!(m.is_moving_to());
                assert_eq!(m.moveto.top_level_object_id, new);
                assert!(
                    matches!(h.effects.last(),Some(MotionEffect::SetTarget { id,.. }) if *id==new)
                );
                // The earlier sticky deadline cannot later clear/cancel the new operation.
                h.effects.clear();
                h.env.cur_time = LocalTime(102.0);
                m.sticky.use_time(&mut h.ctx());
                assert!(m.is_moving_to());
                assert!(h.effects.is_empty());
            }
        }
    }

    /// The retained isolated adapter is genuinely supported: resolving its callbacks before
    /// the next command must produce the same state, sequence and ordered host tail as the owner.
    #[test]
    fn owner_unstick_matches_an_immediately_resolved_isolated_adapter() {
        use crate::motion::{MoveToRequest, MovementManager, MovementParameters};
        for active_move in [false, true] {
            let mut owner_h = H::new(100.0);
            let mut adapter_h = H::new(100.0);
            let mut owner = MovementManager::new();
            let mut adapter = MovementManager::new();
            let old = ObjectId(0x7000_0001);
            for (m, h) in [(&mut owner, &mut owner_h), (&mut adapter, &mut adapter_h)] {
                if active_move {
                    m.perform_movement(
                        &MoveToRequest::TurnToObject {
                            object_id: old,
                            top_level_id: old,
                        },
                        &MovementParameters::default(),
                        &mut h.ctx(),
                    );
                }
                m.sticky.stick_to(old, 0.4, 100.0, &mut h.ctx());
                m.sticky
                    .handle_update_target(old, true, pos(1.0, 0.0), 100.0, &mut h.ctx());
                h.effects.clear();
            }
            owner.unstick_from_object(&mut owner_h.ctx());
            adapter.sticky.unstick(&mut adapter_h.ctx());
            assert_eq!(adapter_h.effects.pop(), Some(MotionEffect::CancelMoveTo));
            adapter.cancel_move_to(0x36, &mut adapter_h.ctx());
            assert_eq!(owner_h.effects, adapter_h.effects);
            assert_eq!(owner.interp.raw_state, adapter.interp.raw_state);
            assert_eq!(
                owner.interp.interpreted_state,
                adapter.interp.interpreted_state
            );
            assert_eq!(owner.interp.pending_motions, adapter.interp.pending_motions);
            assert_eq!(
                format!("{:?}", owner.moveto),
                format!("{:?}", adapter.moveto)
            );
            assert_eq!(
                format!("{:?}", owner.sticky),
                format!("{:?}", adapter.sticky)
            );
            assert_eq!(owner_h.seq.frame_number, adapter_h.seq.frame_number);
            assert_eq!(owner_h.mgr.state, adapter_h.mgr.state);
            assert_eq!(owner_h.events, adapter_h.events);
        }
    }

    #[test]
    fn owner_stick_cancellation_precedes_new_subscription_and_keeps_inactive_approach() {
        use crate::motion::{MoveToRequest, MovementManager, MovementParameters};
        for old_stick in [false, true] {
            let mut h = H::new(100.0);
            let mut m = MovementManager::new();
            let old = ObjectId(0x7000_0001);
            let new = ObjectId(0x7000_0002);
            m.perform_movement(
                &MoveToRequest::TurnToObject {
                    object_id: old,
                    top_level_id: old,
                },
                &MovementParameters::default(),
                &mut h.ctx(),
            );
            if old_stick {
                m.stick_to_object(old, 0.4, 100.0, &mut h.ctx());
            }
            h.effects.clear();
            m.stick_to_object(new, 0.6, 100.25, &mut h.ctx());
            assert_eq!(m.is_moving_to(), !old_stick);
            assert_eq!(m.sticky.target_id, new);
            assert_eq!(m.sticky.target_radius, 0.6);
            assert_eq!(m.sticky.sticky_timeout_time, 101.25);
            assert!(!m.sticky.initialized);
            assert_eq!(
                h.effects.last(),
                Some(&MotionEffect::SetTarget {
                    id: new,
                    radius: 0.5,
                    quantum: 0.5
                })
            );
            assert!(!h.effects.iter().any(|e| matches!(
                e,
                MotionEffect::CancelMoveTo | MotionEffect::UnstickFromObject
            )));
            assert_eq!(
                h.effects
                    .iter()
                    .filter(|e| matches!(e, MotionEffect::MoveToFailed(0x36)))
                    .count(),
                usize::from(old_stick)
            );
            if old_stick {
                assert_eq!(h.effects.first(), Some(&MotionEffect::ClearTarget));
                assert_eq!(h.effects.get(1), Some(&MotionEffect::MoveToFailed(0x36)));
            }
        }
    }

    #[test]
    fn owner_sticky_deadline_is_strict_and_target_updates_do_not_renew_it() {
        use crate::motion::{MoveToRequest, MovementManager, MovementParameters};
        let mut h = H::new(100.0);
        let mut m = MovementManager::new();
        let old = ObjectId(0x7000_0001);
        m.perform_movement(
            &MoveToRequest::TurnToObject {
                object_id: old,
                top_level_id: old,
            },
            &MovementParameters::default(),
            &mut h.ctx(),
        );
        m.stick_to_object(old, 0.4, 100.0, &mut h.ctx());
        h.effects.clear();
        let mut info = super::super::moveto::TargetInfo {
            object_id: ObjectId(0x7000_0002),
            ok: false,
            target_position: pos(3.0, 0.0),
            interpolated_position: pos(4.0, 0.0),
        };
        m.handle_sticky_update_target(&info, &mut h.ctx());
        assert!(h.effects.is_empty() && m.is_moving_to());
        assert_eq!(m.sticky.target_id, old);
        info.object_id = old;
        info.ok = true;
        h.env.cur_time = LocalTime(101.0);
        m.handle_sticky_update_target(&info, &mut h.ctx());
        assert!(m.sticky.initialized);
        assert_eq!(m.sticky.target_position, Some(info.target_position));
        assert_eq!(m.sticky.sticky_timeout_time, 101.0);
        m.use_time(&mut h.ctx());
        assert!(
            m.sticky.is_sticky() && m.is_moving_to(),
            "equal deadline is not expired"
        );
        h.effects.clear();
        h.env.cur_time = LocalTime(f64::from_bits(101.0_f64.to_bits() + 1));
        m.use_time(&mut h.ctx());
        assert!(!m.sticky.is_sticky() && !m.is_moving_to());
        assert_eq!(h.effects.first(), Some(&MotionEffect::ClearTarget));
        assert_eq!(h.effects.get(1), Some(&MotionEffect::MoveToFailed(0x36)));
        assert!(!h.effects.contains(&MotionEffect::CancelMoveTo));
        h.effects.clear();
        m.use_time(&mut h.ctx());
        assert!(h.effects.is_empty(), "expiration is not repeated");
    }

    #[test]
    fn unstick_without_a_target_is_a_true_no_op_and_active_clear_retains_the_cache() {
        use crate::motion::MovementManager;
        let mut h = H::new(100.0);
        let mut m = MovementManager::new();
        m.sticky = StickyManager {
            target_id: ObjectId(0),
            target_radius: 4.2,
            target_position: Some(pos(8.0, 9.0)),
            initialized: true,
            sticky_timeout_time: 123.5,
        };
        let before = format!("{:?}", m.sticky);
        m.sticky.unstick(&mut h.ctx());
        m.unstick_from_object(&mut h.ctx());
        assert_eq!(format!("{:?}", m.sticky), before);
        assert!(h.effects.is_empty());
        m.sticky.target_id = ObjectId(0x7000_0001);
        m.unstick_from_object(&mut h.ctx());
        assert_eq!(m.sticky.target_id, ObjectId(0));
        assert!(!m.sticky.initialized);
        assert_eq!(m.sticky.target_position, Some(pos(8.0, 9.0)));
        assert_eq!(m.sticky.target_radius, 4.2);
        assert_eq!(m.sticky.sticky_timeout_time, 123.5);
        let mut offset = Frame::new(Vec3::new(1.0, 2.0, 3.0), Quat::IDENTITY);
        let untouched = offset;
        m.sticky.adjust_offset(
            &mut offset,
            0.1,
            &h.env.position,
            0.5,
            Some(&m.interp),
            &h.env,
        );
        assert_eq!(
            offset, untouched,
            "inactive cache is not a sticky position producer"
        );
        m.sticky
            .stick_to(ObjectId(0x7000_0002), 0.3, 100.0, &mut h.ctx());
        m.sticky.adjust_offset(
            &mut offset,
            0.1,
            &h.env.position,
            0.5,
            Some(&m.interp),
            &h.env,
        );
        assert_eq!(
            offset, untouched,
            "new target must initialize before any cached position is read"
        );
    }

    /// ORACLE: the recovered movement-manager behavior
    /// section 7, the stick-to entry and the per-frame tick.
    #[test]
    fn a_stick_expires_one_second_after_stick_to() {
        let mut h = H::new(100.0);
        let mut s = StickyManager::new();
        s.stick_to(ObjectId(0x5000_0001), 0.5, 100.0, &mut h.ctx());
        assert!(s.is_sticky());
        assert!(
            h.effects.iter().any(|e| matches!(e,
                MotionEffect::SetTarget { radius, quantum, .. }
                    if *radius == 0.5 && *quantum == 0.5
            )),
            "StickTo requests the retail 0.5 radius and 0.5 second prediction quantum"
        );

        h.env.cur_time = LocalTime(100.5);
        s.use_time(&mut h.ctx());
        assert!(s.is_sticky(), "still inside the timeout");

        h.env.cur_time = LocalTime(101.5);
        s.use_time(&mut h.ctx());
        assert!(!s.is_sticky(), "expired");
    }

    /// Initialization writes the position and initialized flag, not the timeout.
    /// Expiry compares Timer's double to the deadline.
    #[test]
    fn target_updates_do_not_renew_the_stick_deadline() {
        let mut h = H::new(100.0);
        let mut s = StickyManager::new();
        let id = ObjectId(0x5000_0001);
        s.stick_to(id, 0.5, 100.0, &mut h.ctx());
        s.handle_update_target(id, true, pos(1.0, 0.0), 100.9, &mut h.ctx());
        assert!(s.initialized);
        h.env.cur_time = LocalTime(101.0);
        s.use_time(&mut h.ctx());
        assert!(
            s.is_sticky(),
            "strict less-than: the deadline itself survives"
        );
        h.effects.clear();
        h.env.cur_time = LocalTime(101.000_001);
        s.use_time(&mut h.ctx());
        assert!(
            !s.is_sticky(),
            "updates do not extend the one-second deadline"
        );
        assert!(!s.initialized);
        // UnStick/UseTime clear the id and initialized flag, not the cached target frame.
        assert_eq!(s.target_position, Some(pos(1.0, 0.0)));
        assert!(matches!(
            h.effects.as_slice(),
            [MotionEffect::ClearTarget, MotionEffect::CancelMoveTo]
        ));

        // A new StickTo, rather than a TargetInfo, starts a fresh lifetime after cleanup.
        s.stick_to(id, 0.5, 102.0, &mut h.ctx());
        s.handle_update_target(id, true, pos(2.0, 0.0), 102.9, &mut h.ctx());
        h.env.cur_time = LocalTime(103.0);
        s.use_time(&mut h.ctx());
        assert!(s.is_sticky());
        assert!(s.initialized);

        s.handle_update_target(id, false, pos(1.0, 0.0), 102.0, &mut h.ctx());
        assert!(!s.is_sticky());
        // An update for a different object is ignored entirely.
        let mut s = StickyManager::new();
        s.stick_to(id, 0.5, 100.0, &mut h.ctx());
        s.handle_update_target(
            ObjectId(0x5000_0002),
            false,
            pos(0.0, 0.0),
            100.0,
            &mut h.ctx(),
        );
        assert!(s.is_sticky());
    }

    /// Oracle: the client loads 1.0 as a float, adds the
    /// current time as a double and stores the sum as a double.
    #[test]
    fn stick_deadline_preserves_fractional_double_time() {
        let now = 100.000_001;
        let mut h = H::new(now);
        let mut s = StickyManager::new();
        s.stick_to(ObjectId(1), 0.5, now, &mut h.ctx());
        // Narrowing now to f32 would set a deadline of 101.0 and expire at this earlier point.
        h.env.cur_time = LocalTime(101.000_000_5);
        s.use_time(&mut h.ctx());
        assert!(
            s.is_sticky(),
            "retail adds to double time without narrowing to f32"
        );
        let deadline = now + 1.0;
        assert_eq!(s.sticky_timeout_time, deadline);
        h.env.cur_time = LocalTime(deadline);
        s.use_time(&mut h.ctx());
        assert!(s.is_sticky(), "strict equality survives");
        h.env.cur_time = LocalTime(deadline + 0.000_000_1);
        s.use_time(&mut h.ctx());
        assert!(!s.is_sticky(), "the next double-time interval expires");
    }

    /// The approach distance is the 3-D centre distance minus radii.
    /// Its name does not mean flatten z, nor does it perform cylinder_distance's height-gap arm.
    #[test]
    fn sticky_distance_uses_the_retail_radial_helper_with_elevation() {
        let mut h = H::new(100.0);
        let mut s = StickyManager::new();
        let id = ObjectId(1);
        let me = pos(0.0, 0.0);
        s.stick_to(id, 0.5, 100.0, &mut h.ctx());
        let mut target = pos(0.0, 3.0);
        target.frame.origin.z = 4.0;
        s.handle_update_target(id, true, target, 100.0, &mut h.ctx());
        let mut f = Frame::default();
        s.adjust_offset(&mut f, 1.0, &me, 0.5, None, &h.env);
        // sqrt(3^2 + 4^2) - (0.5 + 0.5) - 0.3 = 3.7, projected into the local XY plane.
        assert!((f.origin.y - 3.7).abs() < 1e-5, "{:?}", f.origin);
        assert_eq!(f.origin.z, 0.0);
        // Overlapping radii retain a negative radial gap, even with different elevations.
        s.handle_update_target(id, true, target, 100.0, &mut h.ctx());
        s.target_radius = 3.0;
        s.adjust_offset(&mut f, 1.0, &me, 3.0, None, &h.env);
        assert!((f.origin.y + 1.3).abs() < 1e-5, "{:?}", f.origin);
    }

    /// `adjust_offset` caps the step at `max_speed × 5 × quantum` and faces the target.
    #[test]
    fn adjust_offset_caps_the_step_and_faces_the_target() {
        let mut h = H::new(100.0);
        let mut s = StickyManager::new();
        let id = ObjectId(0x5000_0001);
        s.stick_to(id, 0.0, 100.0, &mut h.ctx());
        // The target is 100 m north: far more than one step.
        s.handle_update_target(id, true, pos(0.0, 100.0), 100.0, &mut h.ctx());
        let me = pos(0.0, 0.0);
        let mut f = Frame::default();
        // No interpreter: the max speed floors at 15 m/s, so one 0.1 s step is 15 * 5 * 0.1? No —
        // the floor is applied to `max_speed * 5`, giving 15 m/s, hence 1.5 m in 0.1 s.
        s.adjust_offset(&mut f, 0.1, &me, 0.0, None, &h.env);
        assert!((f.origin.y - 1.5).abs() < 1e-4, "{:?}", f.origin);
        assert_eq!(f.origin.z, 0.0, "z is zeroed before the multiply");

        // Close enough that the whole remaining distance fits in one step.
        s.handle_update_target(id, true, pos(0.0, 0.4), 100.0, &mut h.ctx());
        let mut f = Frame::default();
        s.adjust_offset(&mut f, 0.1, &me, 0.0, None, &h.env);
        assert!(
            (f.origin.y - (0.4 - STICKY_STANDOFF)).abs() < 1e-4,
            "{:?}",
            f.origin
        );
    }

    /// An uninitialised stick produces no nudge at all.
    #[test]
    fn an_uninitialised_stick_does_nothing() {
        let mut h = H::new(100.0);
        let mut s = StickyManager::new();
        s.stick_to(ObjectId(1), 0.5, 100.0, &mut h.ctx());
        let mut f = Frame::new(Vec3::new(9.0, 9.0, 9.0), Quat::IDENTITY);
        s.adjust_offset(&mut f, 0.1, &pos(0.0, 0.0), 0.0, None, &h.env);
        assert_eq!(f.origin, Vec3::new(9.0, 9.0, 9.0), "untouched");
    }
}
