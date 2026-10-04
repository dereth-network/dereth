//! Detection, targeting and the attack cone.
//!
//! Transcribed against the client's own targeting, detection-cylsphere and sphere code.
//!
//! The four periods are load-bearing for gameplay feel: **1.0 s** detection,
//! **0.5 s** targeting, **10.0 s** target timeout and **5.0 m** of movement before the global cell
//! list is rebuilt. Detection uses full 3-D with `radius + otherRadius` and
//! has **no hysteresis band** — the `object_detected` latch only prevents repeated reports.

use dereth_primitives::{ObjectId, Position, Vec3};

use crate::globals;
use crate::math::{self, V3};

/// `DetectionType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DetectionType {
    #[default]
    NoChange = 0,
    Entered = 1,
    Left = 2,
}

/// `DetectionInfo`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DetectionInfo {
    pub object_id: ObjectId,
    pub object_status: DetectionType,
}

/// `DetectionCylsphere`.
#[derive(Debug, Clone)]
pub struct DetectionCylsphere {
    /// The caller's handle for this registration.
    pub context_id: u32,
    pub radius: f32,
    /// The hysteresis **latch**, not a band.
    pub object_detected: bool,
    pub info: DetectionInfo,
    /// Stored into the global voyeur record's type; no consumer of it is known.
    pub detection_type: u32,
}

/// `TargetStatus`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TargetStatus {
    #[default]
    Undef = 0,
    Ok = 1,
    ExitWorld = 2,
    Teleported = 3,
    Contained = 4,
    Parented = 5,
    TimedOut = 6,
}

/// `TargetInfo`.
#[derive(Debug, Clone, Copy)]
pub struct TargetInfo {
    /// `0` is reserved for the physics-internal consumers (`MoveToManager` and `StickyManager`).
    pub context_id: u32,
    pub object_id: ObjectId,
    /// How far the target may drift before an update is wanted.
    pub radius: f32,
    /// How far ahead to extrapolate.
    pub quantum: f64,
    pub target_position: Position,
    pub interpolated_position: Position,
    pub velocity: Vec3,
    pub status: TargetStatus,
    /// Measured against the **wall** clock.
    pub last_update_time: f64,
}

/// `TargettedVoyeurInfo` — who is tracking me.
#[derive(Debug, Clone, Copy)]
pub struct TargettedVoyeurInfo {
    pub object_id: ObjectId,
    pub quantum: f64,
    pub radius: f32,
    pub last_sent_position: Position,
}

/// `DetectionManager`.
#[derive(Debug, Clone, Default)]
pub struct DetectionManager {
    pub detection_table: Vec<DetectionCylsphere>,
    pub last_update_time: f64,
    /// Where we were when the global cell list was last built.
    pub last_global_update: Option<Position>,
    /// `DestroyDetectionCylsphere` is **deferred**: the id goes here and the next tick removes it,
    /// which is what makes it safe to destroy a registration from inside a detection callback.
    pub pending_deletions: Vec<u32>,
}

impl DetectionManager {
    /// The detection check -- the actual test.
    ///
    /// No hysteresis band: the same threshold is used for entering and leaving.
    #[must_use]
    pub fn check_one(
        cyl: &mut DetectionCylsphere,
        mine: &Position,
        other: &Position,
        other_radius: f32,
    ) -> DetectionType {
        let d = math::distance(mine, other);
        if other_radius + cyl.radius <= d {
            if cyl.object_detected {
                cyl.object_detected = false;
                return DetectionType::Left;
            }
        } else if !cyl.object_detected {
            cyl.object_detected = true;
            return DetectionType::Entered;
        }
        DetectionType::NoChange
    }

    /// Run pending deletions at the top of every tick.
    pub fn clear_pending_deletions(&mut self) {
        for id in std::mem::take(&mut self.pending_deletions) {
            self.detection_table.retain(|c| c.context_id != id);
        }
    }

    /// Destroying the detection cylsphere -- deferred, never immediate.
    pub fn destroy(&mut self, context_id: u32) {
        self.pending_deletions.push(context_id);
    }

    /// The per-frame entry, gated at exactly
    /// **1.0 simulated second**. Returns whether the tick ran.
    pub fn tick(&mut self, sim_time: f64) -> bool {
        self.clear_pending_deletions();
        if sim_time - self.last_update_time < globals::DETECTION_TICK {
            return false;
        }
        self.last_update_time = sim_time;
        true
    }

    /// The global cell registration is refreshed
    /// only when the owner has moved at least **5.0 m**.
    pub fn should_rebuild_cell_list(&mut self, current: &Position) -> bool {
        match self.last_global_update {
            Some(p) if math::distance(&p, current) < globals::DETECTION_RECELL_DISTANCE => false,
            _ => {
                self.last_global_update = Some(*current);
                true
            }
        }
    }
}

/// `TargetManager`.
#[derive(Debug, Clone, Default)]
pub struct TargetManager {
    /// What *I* am tracking.
    pub target_info: Option<TargetInfo>,
    /// Who is tracking *me*.
    pub voyeur_table: Vec<TargettedVoyeurInfo>,
    pub last_update_time: f64,
}

impl TargetManager {
    /// The interpolated position of the target.
    ///
    /// The velocity used is `get_velocity()`, i.e. **`cached_velocity`** — so a blocked object
    /// reports zero and its watchers stop extrapolating.
    #[must_use]
    pub fn interpolated_position(pos: &Position, cached_velocity: Vec3, quantum: f64) -> Position {
        let mut out = *pos;
        #[allow(clippy::cast_possible_truncation)]
        let q = quantum as f32;
        out.frame.origin = out.frame.origin.add(cached_velocity.mul(q));
        out
    }

    /// Send an update only when the target has
    /// drifted further than the watcher's radius.
    #[must_use]
    pub fn should_update_voyeur(
        v: &TargettedVoyeurInfo,
        pos: &Position,
        cached_velocity: Vec3,
    ) -> Option<Position> {
        let p = Self::interpolated_position(pos, cached_velocity, v.quantum);
        if math::distance(&p, &v.last_sent_position) > v.radius {
            Some(p)
        } else {
            None
        }
    }

    /// Gated at **0.5 simulated seconds**, with a
    /// **10.0 s** timeout measured against the **wall** clock. Returns whether the tick ran.
    pub fn tick(&mut self, sim_time: f64, wall_time: f64) -> bool {
        if sim_time - self.last_update_time < globals::TARGET_TICK {
            return false;
        }
        if let Some(ti) = self.target_info.as_mut() {
            if ti.status == TargetStatus::Undef
                && ti.last_update_time + globals::TARGET_TIMEOUT < wall_time
            {
                ti.status = TargetStatus::TimedOut;
            }
        }
        self.last_update_time = sim_time;
        true
    }
}

/// `AttackCone`. `left` and `right` are 2-D direction vectors in the attacker's local frame
/// bounding the wedge.
#[derive(Debug, Clone, Copy)]
pub struct AttackCone {
    pub part_index: i32,
    pub left: (f32, f32),
    pub right: (f32, f32),
    pub radius: f32,
    pub height: f32,
}

/// `AtkCollisionProfile` — the three words filled in per object hit and handed to the game
/// record's `DoCollision` handler.
///
/// The fields in memory order, which is not their declaration order: the part index, then the
/// object id, then the hit location.
///
/// **The client handler never reads any of it** — it goes straight to its own physics object.
/// It is carried anyway because the rebuild's server is a
/// second consumer of this crate and the hit location is the only thing in the client that ever
/// computes one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AtkCollisionProfile {
    /// The cone's own `part_index` — `0xFFFFFFFF` for 681 of the
    /// 1,053 shipped cones.
    pub part: i32,
    /// The object hit.
    pub id: ObjectId,
    /// [`attack`]'s quadrant word, through [`hit_location`].
    pub location: u32,
}

/// The hit-location bitfield shared by attack detection and collision reporting.
pub mod hit_location {
    pub const HIGH: u32 = 0x01;
    pub const MEDIUM: u32 = 0x02;
    pub const LOW: u32 = 0x04;
    pub const LEFT: u32 = 0x08;
    pub const RIGHT: u32 = 0x10;
    pub const FRONT: u32 = 0x20;
    pub const BACK: u32 = 0x40;
}

/// The melee cone test.
///
/// Returns the hit-location quadrant, or `None` for a miss.
///
/// Each of these comparisons is easy to read the wrong way round; the directions below are the
/// retail client's, strict and non-strict as written. All 1,053 shipped cones reach this test.
///
/// Three details change behaviour at real inputs.
///
/// 1. **The wedge block is entered on `cl > 0 || cr > 0`, strictly**, not
///    `>= 0`. For the commonest shipped cone a target straight ahead gives `cl = cr = -0.5`, so
///    the ordinary hit is the *fall-through* and the block is the outside case.
/// 2. **`cross(left, right) < 0` is the ordinary wedge, not the reflex one.** See the comment on
///    the branch below: it is 1,053 of 1,053 shipped cones.
/// 3. Inside the `cross >= 0` branch, `cl * cr <= 0`, `cl <= r` and `cr <= r` are each a **hit**.
///
/// Reading any of these the other way round makes the cone accept targets behind the attacker.
#[must_use]
pub fn attack(
    target: &Position,
    target_radius: f32,
    target_height: f32,
    attacker: &Position,
    cone: &AttackCone,
    cone_radius: f32,
    cone_height: f32,
) -> Option<u32> {
    // The TARGET's origin, expressed in the ATTACKER's local frame.
    let d = math::localtolocal(attacker, target, Vec3::ZERO);

    // The cone's height band must land on the target.
    if cone_height < 0.0 || cone_height > target_height {
        return None;
    }
    let rr = target_radius + cone_radius;
    let xy2 = d.x * d.x + d.y * d.y;
    if rr * rr < xy2 {
        return None;
    }
    if d.z.abs() > rr {
        return None;
    }

    // The quadrant is computed from the ATTACKER's origin in the TARGET's frame, and the height
    // band from the **cone's** height relative to the target's - so a low swing against a tall
    // creature reports "Low" and against a short one reports "High".
    let e = math::localtolocal(target, attacker, Vec3::ZERO);
    let mut q = if e.x >= 0.0 {
        hit_location::RIGHT
    } else {
        hit_location::LEFT
    };
    q |= if e.y >= 0.0 {
        hit_location::FRONT
    } else {
        hit_location::BACK
    };
    if cone_height < target_height * 0.333_333 {
        q |= hit_location::LOW;
    } else if cone_height < target_height * 0.666_667 {
        q |= hit_location::MEDIUM;
    } else {
        q |= hit_location::HIGH;
    }

    let (lx, ly) = cone.left;
    let (rx, ry) = cone.right;
    let cl = d.y * lx - d.x * ly;
    let cr = d.x * ry - d.y * rx;

    // Two `<= 0` tests, and the fall-through returns the quadrant. Inside both half-planes, or
    // exactly on either edge, is a hit with no further test.
    if cl <= 0.0 && cr <= 0.0 {
        return Some(q);
    }

    // `left.x * right.y - left.y * right.x` against zero:
    // `>= 0` takes the reflex arm and `< 0` falls through to the ordinary wedge.
    //
    // **Which of those two is the "wider than 180 degrees" case is easy to get backwards**, and
    // the shipped data decides it rather than the prose. All **1,053**
    // `AttackHook` instances in `client_portal.dat` have `cross(left, right) < 0`, every one of
    // them a plain wedge of plus-or-minus 10, 30, 40 or 45 degrees with `left` on the **-X** side
    // (west when facing north) and `right` on **+X** — which is what "left" and "right" mean. So
    // `< 0` is the **ordinary** wedge, `>= 0` is the reflex one, and it is 0 of 1,053. Checked
    // against the geometry and not only the count: with `left = (-0.5, 0.866)` and
    // `right = (0.5, 0.866)` -- the commonest shipped pair -- a target straight ahead gives
    // `cl = cr = -0.5`, so it never reaches this block at all, and one directly behind gives
    // `cl = cr = +0.5` and lands on the client's radius test, which is the rejection.
    if lx * ry - ly * rx >= 0.0 {
        // **Reflex, wider than 180 degrees. 0 of 1,053 shipped cones.**
        // Three ways to hit and one way to miss, which is the right shape for a wedge that covers
        // almost everything: the miss is the narrow notch, where the target is outside *both*
        // edges by more than its own radius.
        if cr * cl <= 0.0 {
            return Some(q);
        }
        if cl <= target_radius {
            return Some(q);
        }
        if cr <= target_radius {
            return Some(q);
        }
        return None;
    }

    // **The ordinary wedge.** Every shipped cone takes it: the first swing of any of the 716
    // creature animations that have an attack hook comes through here.
    //
    // Control only arrives with `cl > 0 || cr > 0`, i.e. outside at least one edge.
    if cl < 0.0 {
        // Inside the left edge, outside the right: the
        // target's own radius has to reach back across the right edge.
        return if cr <= target_radius { Some(q) } else { None };
    }
    if cr < 0.0 {
        // Inside the right edge, outside the left: `cl > r` is a miss.
        return if cl <= target_radius { Some(q) } else { None };
    }
    // Outside **both** edges -- behind the attacker for a convex wedge. The only test left is
    // `target_radius * target_radius` against the squared horizontal distance, so the only way a
    // target behind you is hit is if it
    // is fat enough to contain the attacker.
    if target_radius * target_radius < xy2 {
        return None;
    }
    Some(q)
}

/// Whether an attack cone is **reflex** (wider than 180 degrees) -- the sign test, taking
/// the branch.
///
/// `cross(left, right) < 0` is easy to mistake for the >180-degree case. It is the other way
/// round: `cross < 0` is the ordinary wedge and is **1,053 of 1,053** shipped cones,
/// `cross >= 0` is the reflex one and is **0**.
#[must_use]
pub fn cone_is_reflex(cone: &AttackCone) -> bool {
    cone.left.0 * cone.right.1 - cone.left.1 * cone.right.0 >= 0.0
}

/// The **target** side of one cone test.
///
/// ```text
///   parent != NULL                                     -> 0
///   state & 0x10     (IGNORE_COLLISIONS_PS)            -> 0
///   state & 0x200000 (REPORT_..._AS_ENVIRONMENT_PS)    -> 0
///   height = part_array ? part_array.height() : 0.0
///   radius = part_array ? part_array.radius() : 0.0
///   (&position, radius, height, attacker, &cone->left, &cone->right,
///                           attacker_scale * cone->radius + attack_radius,
///                           attacker_scale * cone->height)
/// ```
///
/// `attack_radius` is copied from the attack manager into each attack record.
/// **It is `0.0` for the whole life of the client**: both that field (the attack cone's own) and
/// the unrelated physics-object field
/// are zeroed in their constructors and assigned nowhere else in the client. It is a
/// parameter here so the call reads like the original, not because anything varies it.
///
/// `GetHeight`/`GetRadius` are `setup->height * scale.z` and `setup->radius * scale.z`, and the
/// rebuild's uniform `scale` is that `scale.z`.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn check_attack(
    target: &Position,
    target_has_parent: bool,
    target_state: crate::obj::PhysicsState,
    target_setup_radius: f32,
    target_setup_height: f32,
    target_scale: f32,
    attacker: &Position,
    attacker_scale: f32,
    cone: &AttackCone,
    attack_radius: f32,
) -> Option<u32> {
    if !target_is_attackable(
        target_has_parent,
        target_state.ignores_collisions(),
        target_state.reports_as_environment(),
    ) {
        return None;
    }
    attack(
        target,
        target_setup_radius * target_scale,
        target_setup_height * target_scale,
        attacker,
        cone,
        attacker_scale * cone.radius + attack_radius,
        attacker_scale * cone.height,
    )
}

/// The *target* side's eligibility test.
#[must_use]
pub fn target_is_attackable(
    has_parent: bool,
    ignores_collisions: bool,
    reports_as_environment: bool,
) -> bool {
    !has_parent && !ignores_collisions && !reports_as_environment
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_primitives::{CellId, Frame, Quat};

    // Oracle: the recovered detection behavior, which
    // reproduces the detection check, the cell-list update, the targeting handler and the
    // sphere's attack test.

    fn at(x: f32, y: f32, z: f32) -> Position {
        Position::new(
            CellId(0xA9B4_0001),
            Frame::new(Vec3::new(x, y, z), Quat::IDENTITY),
        )
    }

    /// All four collision-result values.
    #[test]
    fn the_four_periods_are_the_documented_ones() {
        assert_eq!(globals::DETECTION_TICK, 1.0);
        assert_eq!(globals::TARGET_TICK, 0.5);
        assert_eq!(globals::TARGET_TIMEOUT, 10.0);
        assert_eq!(globals::DETECTION_RECELL_DISTANCE, 5.0);
    }

    #[test]
    fn the_detection_tick_is_exactly_one_simulated_second() {
        let mut m = DetectionManager::default();
        assert!(
            m.tick(1.0),
            "the first tick fires (last_update_time starts at 0)"
        );
        assert!(!m.tick(1.5));
        assert!(!m.tick(1.999));
        assert!(m.tick(2.0), "exactly 1.0 later fires");
    }

    #[test]
    fn detection_has_no_hysteresis_band_only_a_latch() {
        let mut cyl = DetectionCylsphere {
            context_id: 1,
            radius: 5.0,
            object_detected: false,
            info: DetectionInfo {
                object_id: ObjectId(2),
                object_status: DetectionType::NoChange,
            },
            detection_type: 0,
        };
        let mine = at(0.0, 0.0, 0.0);
        // 5 + 1 = 6 is the threshold; 5.9 is inside.
        assert_eq!(
            DetectionManager::check_one(&mut cyl, &mine, &at(5.9, 0.0, 0.0), 1.0),
            DetectionType::Entered
        );
        // Repeating it reports nothing: the latch, not a band.
        assert_eq!(
            DetectionManager::check_one(&mut cyl, &mine, &at(5.9, 0.0, 0.0), 1.0),
            DetectionType::NoChange
        );
        // Crossing back out by the *same* threshold reports Left immediately - there is no wider
        // exit radius.
        assert_eq!(
            DetectionManager::check_one(&mut cyl, &mine, &at(6.01, 0.0, 0.0), 1.0),
            DetectionType::Left
        );
        assert_eq!(
            DetectionManager::check_one(&mut cyl, &mine, &at(6.01, 0.0, 0.0), 1.0),
            DetectionType::NoChange
        );
    }

    #[test]
    fn detection_distance_is_full_three_dimensional() {
        let mut cyl = DetectionCylsphere {
            context_id: 1,
            radius: 5.0,
            object_detected: false,
            info: DetectionInfo {
                object_id: ObjectId(2),
                object_status: DetectionType::NoChange,
            },
            detection_type: 0,
        };
        // Directly overhead at 5.9 m: a 2-D test would call this zero distance.
        assert_eq!(
            DetectionManager::check_one(&mut cyl, &at(0.0, 0.0, 0.0), &at(0.0, 0.0, 5.9), 1.0),
            DetectionType::Entered
        );
        cyl.object_detected = false;
        assert_eq!(
            DetectionManager::check_one(&mut cyl, &at(0.0, 0.0, 0.0), &at(0.0, 0.0, 20.0), 1.0),
            DetectionType::NoChange
        );
    }

    #[test]
    fn the_global_cell_list_is_rebuilt_only_after_five_metres_of_movement() {
        let mut m = DetectionManager::default();
        assert!(
            m.should_rebuild_cell_list(&at(0.0, 0.0, 0.0)),
            "the first call always builds"
        );
        assert!(!m.should_rebuild_cell_list(&at(4.9, 0.0, 0.0)));
        assert!(m.should_rebuild_cell_list(&at(5.1, 0.0, 0.0)));
        assert!(
            !m.should_rebuild_cell_list(&at(5.2, 0.0, 0.0)),
            "the anchor moved with it"
        );
    }

    #[test]
    fn destroying_a_detection_registration_is_deferred_to_the_next_tick() {
        let mut m = DetectionManager::default();
        m.detection_table.push(DetectionCylsphere {
            context_id: 7,
            radius: 1.0,
            object_detected: false,
            info: DetectionInfo {
                object_id: ObjectId(0),
                object_status: DetectionType::NoChange,
            },
            detection_type: 0,
        });
        m.destroy(7);
        assert_eq!(
            m.detection_table.len(),
            1,
            "not removed yet: a callback may still be running"
        );
        m.tick(1.0);
        assert!(m.detection_table.is_empty(), "the next tick clears it");
    }

    #[test]
    fn the_target_tick_is_half_a_second_and_the_timeout_is_ten_wall_clock_seconds() {
        let mut m = TargetManager {
            target_info: Some(TargetInfo {
                context_id: 0,
                object_id: ObjectId(3),
                radius: 1.0,
                quantum: 0.5,
                target_position: at(0.0, 0.0, 0.0),
                interpolated_position: at(0.0, 0.0, 0.0),
                velocity: Vec3::ZERO,
                status: TargetStatus::Undef,
                last_update_time: 0.0,
            }),
            ..TargetManager::default()
        };
        assert!(!m.tick(0.4, 0.4));
        assert!(m.tick(0.5, 0.5));
        assert_eq!(m.target_info.expect("set").status, TargetStatus::Undef);
        // Ten wall-clock seconds after last_update_time the target times out.
        assert!(m.tick(1.0, 9.9));
        assert_eq!(m.target_info.expect("set").status, TargetStatus::Undef);
        assert!(m.tick(1.5, 10.1));
        assert_eq!(m.target_info.expect("set").status, TargetStatus::TimedOut);
    }

    /// A blocked object reports zero `cached_velocity`,
    /// so its watchers stop extrapolating.
    #[test]
    fn voyeur_extrapolation_uses_the_achieved_velocity() {
        let pos = at(0.0, 0.0, 0.0);
        let moving = TargetManager::interpolated_position(&pos, Vec3::new(4.0, 0.0, 0.0), 0.5);
        assert_eq!(moving.frame.origin, Vec3::new(2.0, 0.0, 0.0));
        let blocked = TargetManager::interpolated_position(&pos, Vec3::ZERO, 0.5);
        assert_eq!(
            blocked.frame.origin,
            Vec3::ZERO,
            "a blocked object is not extrapolated"
        );
    }

    #[test]
    fn a_voyeur_is_updated_only_once_the_target_drifts_past_its_radius() {
        let v = TargettedVoyeurInfo {
            object_id: ObjectId(1),
            quantum: 0.0,
            radius: 2.0,
            last_sent_position: at(0.0, 0.0, 0.0),
        };
        assert!(TargetManager::should_update_voyeur(&v, &at(1.0, 0.0, 0.0), Vec3::ZERO).is_none());
        assert!(TargetManager::should_update_voyeur(&v, &at(3.0, 0.0, 0.0), Vec3::ZERO).is_some());
    }

    fn cone(half_angle_degrees: f32) -> AttackCone {
        let a = half_angle_degrees.to_radians();
        AttackCone {
            part_index: 0,
            left: (
                -dereth_primitives::num::math::sinf(a),
                dereth_primitives::num::math::cosf(a),
            ),
            right: (
                dereth_primitives::num::math::sinf(a),
                dereth_primitives::num::math::cosf(a),
            ),
            radius: 1.0,
            height: 1.0,
        }
    }

    /// The shipped pair, read out of `client_portal.dat` by
    /// `dereth/client/tests/gpu/combat/attack_hook.rs`: anim `0x0300000E` part 5, `left (-0.5, 0.8660254)`,
    /// `right (0.5, 0.8660254)`, `radius 2`, `height 1`. Here so that the fixture above is anchored
    /// to data rather than to a convention someone chose.
    #[test]
    fn the_fixture_matches_a_shipped_cone() {
        let c = cone(30.0);
        assert!((c.left.0 - -0.5).abs() < 1e-6, "left.x {}", c.left.0);
        assert!((c.left.1 - 0.866_025_4).abs() < 1e-6, "left.y {}", c.left.1);
        assert!((c.right.0 - 0.5).abs() < 1e-6, "right.x {}", c.right.0);
        assert!(
            !cone_is_reflex(&c),
            "a shipped cone is not reflex; cross = {}",
            c.left.0 * c.right.1 - c.left.1 * c.right.0
        );
    }

    #[test]
    fn the_attack_cone_accepts_a_target_in_front() {
        let attacker = at(0.0, 0.0, 0.0);
        let c = cone(45.0);
        // Directly in front, 1 m away, target radius 0.5, cone radius 1.
        let hit = attack(&at(0.0, 1.0, 0.0), 0.5, 2.0, &attacker, &c, 1.0, 1.0);
        assert!(hit.is_some(), "a target straight ahead is hit");
    }

    /// The attack cone rejects a target behind the attacker.
    #[test]
    fn the_attack_cone_rejects_a_target_behind_the_attacker() {
        let attacker = at(0.0, 0.0, 0.0);
        let c = cone(45.0);
        let miss = attack(&at(0.0, -1.0, 0.0), 0.5, 2.0, &attacker, &c, 1.0, 1.0);
        assert!(
            miss.is_none(),
            "a target behind should be missed, got {miss:?}"
        );
        // and the same at every shipped half-angle, because a cone that rejects at 45 degrees and
        // accepts at 10 would be an angle bug hiding behind one fixture.
        for half in [10.0_f32, 30.0, 40.0, 45.0] {
            let c = cone(half);
            assert!(
                attack(&at(0.0, -1.0, 0.0), 0.5, 2.0, &attacker, &c, 1.0, 1.0).is_none(),
                "half angle {half}"
            );
            assert!(
                attack(&at(0.0, 1.0, 0.0), 0.5, 2.0, &attacker, &c, 1.0, 1.0).is_some(),
                "half angle {half}: straight ahead must still hit"
            );
        }
    }

    /// The other side of the angle test: a target **beside** the attacker, inside the reach but
    /// outside the wedge, is a miss — and it becomes a hit once it is fat enough to reach back
    /// across the near edge, which is the client's `cl > target_radius` and the only thing that
    /// test can mean.
    #[test]
    fn the_attack_cone_rejects_a_target_beside_the_attacker_until_its_radius_reaches_the_edge() {
        let attacker = at(0.0, 0.0, 0.0);
        let c = cone(30.0);
        // 1 m to the west. cl = +cos(30) = 0.866, cr = -0.866, so this is the client's arm.
        assert!(attack(&at(-1.0, 0.0, 0.0), 0.5, 2.0, &attacker, &c, 1.0, 1.0).is_none());
        assert!(attack(&at(-1.0, 0.0, 0.0), 0.9, 2.0, &attacker, &c, 1.0, 1.0).is_some());
        // The mirrored point 1 m east takes the client's `cr > target_radius` arm.
        assert!(attack(&at(1.0, 0.0, 0.0), 0.5, 2.0, &attacker, &c, 1.0, 1.0).is_none());
        assert!(attack(&at(1.0, 0.0, 0.0), 0.9, 2.0, &attacker, &c, 1.0, 1.0).is_some());
    }

    #[test]
    fn the_attack_cone_rejects_a_target_out_of_range_and_out_of_the_height_band() {
        let attacker = at(0.0, 0.0, 0.0);
        let c = cone(45.0);
        assert!(attack(&at(0.0, 10.0, 0.0), 0.5, 2.0, &attacker, &c, 1.0, 1.0).is_none());
        // coneHeight above the target's height.
        assert!(attack(&at(0.0, 1.0, 0.0), 0.5, 2.0, &attacker, &c, 1.0, 3.0).is_none());
        // and a negative cone height.
        assert!(attack(&at(0.0, 1.0, 0.0), 0.5, 2.0, &attacker, &c, 1.0, -0.1).is_none());
    }

    #[test]
    fn the_hit_location_band_comes_from_the_cone_height_relative_to_the_target() {
        let attacker = at(0.0, 0.0, 0.0);
        let c = cone(45.0);
        let target = at(0.0, 1.0, 0.0);
        // The same swing against a 3 m creature is Low and against a 1.2 m one is High.
        let tall = attack(&target, 0.5, 3.0, &attacker, &c, 1.0, 0.5).expect("hit");
        assert_eq!(tall & hit_location::LOW, hit_location::LOW, "{tall:#x}");
        let short = attack(&target, 0.5, 1.2, &attacker, &c, 1.0, 1.0).expect("hit");
        assert_eq!(short & hit_location::HIGH, hit_location::HIGH, "{short:#x}");
        // and something in between is Medium.
        let mid = attack(&target, 0.5, 3.0, &attacker, &c, 1.0, 1.5).expect("hit");
        assert_eq!(mid & hit_location::MEDIUM, hit_location::MEDIUM, "{mid:#x}");
    }

    #[test]
    fn the_hit_location_side_is_computed_from_the_attacker_in_the_targets_frame() {
        let c = cone(60.0);
        // Attacker at the origin, target 1 m ahead facing the same way: the attacker is *behind*
        // the target and to its left/right depending on the offset.
        let hit = attack(
            &at(0.0, 1.0, 0.0),
            0.5,
            2.0,
            &at(0.0, 0.0, 0.0),
            &c,
            1.0,
            1.0,
        )
        .expect("hit");
        assert_eq!(hit & hit_location::BACK, hit_location::BACK, "{hit:#x}");
    }

    /// A sweep of convex cone angles: every one is accepted for a target straight ahead, and the
    /// quadrant word always names exactly one height band and one of each lateral pair.
    #[test]
    fn a_sweep_of_convex_cone_angles_produces_a_well_formed_quadrant() {
        let attacker = at(0.0, 0.0, 0.0);
        for half in [5_i32, 15, 30, 45, 60, 75, 89] {
            #[allow(clippy::cast_precision_loss)]
            let c = cone(half as f32);
            assert!(!cone_is_reflex(&c), "half angle {half} must be convex");
            let q = attack(&at(0.0, 1.0, 0.0), 0.5, 2.0, &attacker, &c, 1.0, 1.0)
                .unwrap_or_else(|| panic!("half angle {half} should hit"));
            let bands = [hit_location::HIGH, hit_location::MEDIUM, hit_location::LOW]
                .iter()
                .filter(|b| q & *b != 0)
                .count();
            assert_eq!(bands, 1, "half angle {half}: {q:#x}");
            assert_eq!(
                (q & hit_location::LEFT != 0) as u8 + (q & hit_location::RIGHT != 0) as u8,
                1
            );
            assert_eq!(
                (q & hit_location::FRONT != 0) as u8 + (q & hit_location::BACK != 0) as u8,
                1
            );
        }
    }

    /// A genuinely reflex cone: edges swapped, so the wedge is the 300-degree complement.
    #[test]
    fn a_reflex_cone_is_the_swapped_pair_and_no_shipped_cone_is_one() {
        let a: f32 = 30.0_f32.to_radians();
        let c = AttackCone {
            part_index: 0,
            // The shipped pair, the wrong way round.
            left: (
                dereth_primitives::num::math::sinf(a),
                dereth_primitives::num::math::cosf(a),
            ),
            right: (
                -dereth_primitives::num::math::sinf(a),
                dereth_primitives::num::math::cosf(a),
            ),
            radius: 1.0,
            height: 1.0,
        };
        assert!(
            cone_is_reflex(&c),
            "swapping the edges of a 60 degree wedge gives a 300 degree one"
        );
        assert!(
            !cone_is_reflex(&cone(30.0)),
            "and the shipped order is not reflex"
        );
    }

    /// The reflex cone hits everything except its notch.
    #[test]
    fn the_reflex_cone_hits_everything_except_its_notch() {
        let a: f32 = 30.0_f32.to_radians();
        let c = AttackCone {
            part_index: 0,
            left: (
                dereth_primitives::num::math::sinf(a),
                dereth_primitives::num::math::cosf(a),
            ),
            right: (
                -dereth_primitives::num::math::sinf(a),
                dereth_primitives::num::math::cosf(a),
            ),
            radius: 1.0,
            height: 1.0,
        };
        let attacker = at(0.0, 0.0, 0.0);
        // Straight ahead is the notch: cl = cr = +sin(30) = 0.5, product > 0, both above the
        // 0.1 m target radius -> a miss.
        assert!(
            attack(&at(0.0, 1.0, 0.0), 0.1, 2.0, &attacker, &c, 1.0, 1.0).is_none(),
            "the notch of a reflex wedge is the miss"
        );
        // Behind, beside and diagonally are all inside the 300 degrees.
        for p in [(0.0_f32, -1.0_f32), (1.0, 0.0), (-1.0, 0.0), (0.7, -0.7)] {
            assert!(
                attack(&at(p.0, p.1, 0.0), 0.1, 2.0, &attacker, &c, 1.0, 1.0).is_some(),
                "{p:?} is inside a 300 degree wedge"
            );
        }
    }

    #[test]
    fn check_attack_excludes_parented_ignoring_and_environment_reporting_objects() {
        assert!(target_is_attackable(false, false, false));
        assert!(!target_is_attackable(true, false, false));
        assert!(!target_is_attackable(false, true, false));
        assert!(!target_is_attackable(false, false, true));
    }
}
