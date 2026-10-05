//! The free camera.
//!
//! The client's real camera (including the swept sphere, smoother and the mouse-look dead zone)
//! is `CameraControl` below and is outside this debug camera's scope. Nothing here claims to
//! reproduce it: it is a debug flycam whose only job is to put a viewpoint somewhere over
//! Dereth so the static scene can be looked at.
//!
//! What is **not** invented here is the frame it produces. `dereth_render::camera::view_from_frame`
//! consumes a `Frame` whose rotation matrix has the camera's local axes as its *rows* — row 0
//! right, row 1 forward, row 2 up — because that is the client's
//! convention. `FreeCamera::frame` builds exactly that, and the test at the bottom checks it by
//! pushing points through the renderer's own view matrix rather than by restating the algebra.
//!
//! Coordinates are the client's: **+x east, +y north, +z up**, in the viewer-block-relative space
//! used to render the landscape, so the origin is the south-west corner
//! of the block the camera started in.

use dereth_physics::math as pmath;
use dereth_physics::math::V3;
use dereth_primitives::num::math;
use dereth_primitives::{CellId, Frame, LocalTime, ObjectId, Position, Quat, Vec3};

/// Metres per second at a walk. A landblock is 192 m across, so this crosses one in about eight
/// seconds — fast enough to look around, slow enough to see the ground.
pub const MOVE_SPEED: f32 = 25.0;
/// The multiplier while Shift is held.
pub const FAST_MULTIPLIER: f32 = 6.0;
/// Radians per second for keyboard look.
pub const LOOK_SPEED: f32 = 1.2;
/// Radians per pixel of mouse motion while the right button is held.
pub const MOUSE_SENSITIVITY: f32 = 0.003;
/// The pitch clamp. Straight up and straight down both make the basis degenerate.
pub const PITCH_LIMIT: f32 = 1.552_912_8; // 88.97 degrees

/// Which of the camera's controls are held this frame. Filled by the message pump; the camera
/// itself has no opinion about which key means what beyond this struct.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CameraInput {
    pub forward: bool,
    pub back: bool,
    pub left: bool,
    pub right: bool,
    pub up: bool,
    pub down: bool,
    pub look_left: bool,
    pub look_right: bool,
    pub look_up: bool,
    pub look_down: bool,
    pub fast: bool,
}

/// A position plus a yaw and a pitch.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FreeCamera {
    /// Viewer-block-relative, in the client's Z-up world units.
    pub position: Vec3,
    /// Rotation about the client's +z, counter-clockwise seen from above. 0 faces north (+y).
    pub yaw: f32,
    /// Positive looks up.
    pub pitch: f32,
}

impl Default for FreeCamera {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
        }
    }
}

impl FreeCamera {
    #[must_use]
    pub fn new(position: Vec3, yaw: f32, pitch: f32) -> Self {
        Self {
            position,
            yaw,
            pitch: pitch.clamp(-PITCH_LIMIT, PITCH_LIMIT),
        }
    }

    /// The camera's forward direction in world coordinates: `R_z(yaw) · R_x(pitch) · (0, 1, 0)`.
    #[must_use]
    pub fn forward(&self) -> Vec3 {
        let (sy, cy) = (math::sinf(self.yaw), math::cosf(self.yaw));
        let (sp, cp) = (math::sinf(self.pitch), math::cosf(self.pitch));
        Vec3::new(-sy * cp, cy * cp, sp)
    }

    /// The camera's right direction: `R_z(yaw) · (1, 0, 0)`, with no pitch component, so that
    /// strafing stays level.
    #[must_use]
    pub fn right(&self) -> Vec3 {
        Vec3::new(math::cosf(self.yaw), math::sinf(self.yaw), 0.0)
    }

    /// Advance by `dt` seconds under `input`. Movement is along the pitched forward vector, which is
    /// what makes it a flycam rather than a walk.
    pub fn update(&mut self, input: CameraInput, dt: f32) {
        let axis = |neg: bool, pos: bool| f32::from(u8::from(pos)) - f32::from(u8::from(neg));
        self.yaw += LOOK_SPEED * dt * axis(input.look_right, input.look_left);
        self.pitch += LOOK_SPEED * dt * axis(input.look_down, input.look_up);
        self.pitch = self.pitch.clamp(-PITCH_LIMIT, PITCH_LIMIT);

        let speed = MOVE_SPEED * dt * if input.fast { FAST_MULTIPLIER } else { 1.0 };
        let f = self.forward();
        let r = self.right();
        let fwd = axis(input.back, input.forward) * speed;
        let side = axis(input.left, input.right) * speed;
        let vert = axis(input.down, input.up) * speed;
        self.position = Vec3::new(
            self.position.x + f.x * fwd + r.x * side,
            self.position.y + f.y * fwd + r.y * side,
            self.position.z + f.z * fwd + r.z * side + vert,
        );
    }

    /// Mouse-look, in raw pixels of cursor motion.
    pub fn look(&mut self, dx: f32, dy: f32) {
        self.yaw -= dx * MOUSE_SENSITIVITY;
        self.pitch = (self.pitch - dy * MOUSE_SENSITIVITY).clamp(-PITCH_LIMIT, PITCH_LIMIT);
    }

    /// The `Frame` `dereth_render::camera::view_from_frame` wants.
    ///
    /// The quaternion is `q_z(yaw) · q_x(pitch)`, whose rotation matrix — in the row-per-local-axis
    /// form the renderer reads — has row 0 = right, row 1 = forward, row 2 = up.
    /// `dereth_primitives::Quat` stores `w` first, which is the dat order.
    #[must_use]
    pub fn frame(&self) -> Frame {
        let qz = axis_angle(Vec3::new(0.0, 0.0, 1.0), self.yaw);
        let qx = axis_angle(Vec3::new(1.0, 0.0, 0.0), self.pitch);
        Frame::new(self.position, mul(qz, qx))
    }
}

/// A unit quaternion for `angle` radians about a unit `axis`, `w` first.
fn axis_angle(axis: Vec3, angle: f32) -> Quat {
    let h = angle * 0.5;
    let s = math::sinf(h);
    Quat::new(math::cosf(h), axis.x * s, axis.y * s, axis.z * s)
}

/// Hamilton product, `w` first.
fn mul(a: Quat, b: Quat) -> Quat {
    Quat::new(
        a.w * b.w - a.x * b.x - a.y * b.y - a.z * b.z,
        a.w * b.x + a.x * b.w + a.y * b.z - a.z * b.y,
        a.w * b.y - a.x * b.z + a.y * b.w + a.z * b.x,
        a.w * b.z + a.x * b.y - a.y * b.x + a.z * b.w,
    )
}

// =================================================================================================
// The real camera.
//
// This module owns the camera model, its modes and zoom, and the swept-sphere obstruction test.
//
// The *input* half is `crate::actions::camera` and is not repeated here: this module calls
// `MouseLook::handle`, including
// the mouse-input filter and the six-frame dead zone, and turns its `MouseLookResult` into
// the `Rotate`/`Raise`/`Lower` calls below.
// =================================================================================================

/// The camera target-status bit mask.
pub mod target {
    /// No target at all.
    pub const INVALID_TARGET: u32 = 0;
    /// Add `direction`, transformed into world space by the pivot object's frame.
    pub const LOOK_IN_DIRECTION: u32 = 1;
    /// Add the normalised vector from the pivot point to the target object's position.
    pub const LOOK_AT_OBJECT: u32 = 2;
    /// After placing the eye, aim it back at the pivot point.
    pub const LOOK_AT_PIVOT: u32 = 4;
    /// Declared and **never read** by camera updating.
    pub const ALIGN_WITH_PIVOT: u32 = 8;
    /// Derive the look direction from the pivot's motion / contact plane.
    pub const ALIGN_WITH_PLANE: u32 = 16;
}

/// The in-head test checks **exactly** this offset, and `Closer`, `Farther`,
/// `Raise`, `Lower` and `set_target_for_offset` all special-case it.
pub const IN_HEAD_OFFSET: Vec3 = Vec3::new(0.0, 0.18, 0.0);

/// `angle`, the shipped rotation-rate constant: 0.13962634 rad = **8 degrees** per unit of
/// `Rotate`/`Raise`/`Lower` step (`angle = 0.13962634`).
pub const CAMERA_ANGLE_STEP: f32 = 0.139_626_34;

/// `Closer`, `Farther`, `Raise`, `Lower` and `Rotate` all refuse to run more often than this.
/// `crate::actions::camera::ROTATE_MIN_INTERVAL` is the same number, named for `Rotate` alone.
pub const ZOOM_MIN_INTERVAL: f64 = 0.0002;

/// The first-person step, in **degrees**, is `8.0f`, added
/// to or subtracted before the turn-to-heading call. It is [`CAMERA_ANGLE_STEP`] in radians —
/// 0.13962634 rad *is* 8 degrees — but the first-person arm works in the heading's own degrees
/// and never converts, so the two are spelled separately.
pub const CAMERA_TURN_DEGREES: f32 = 8.0;

/// `360.0f` is the wrap the first-person rotation arm applies to `heading ± 8`.
pub const HEADING_WRAP_DEGREES: f32 = 360.0;

/// The mouse-turning dead-zone thresholds are the doubles `-0.02` and `+0.02`; a step strictly
/// inside them stops the drift instead of
/// issuing a turn.
pub const MOUSE_TURN_DEAD_ZONE: f64 = 0.02;

/// The mouse-turning extent ceiling is the double `1.5`, compared against `step * 2`.
pub const MOUSE_TURN_MAX_EXTENT: f64 = 1.5;

/// The double `0.5` controls how often the mouse-turning arm may send a movement event.
pub const ROTATE_SERVER_MESSAGE_INTERVAL: f64 = 0.5;

/// Gameplay camera setup opens with the float `0x3F8CCCCD` = **1.1**: it fetches the UI system's
/// `CameraState` and, when there is one, applies that scale. Nothing else guards it.
///
/// It runs on **every** frame the gameplay UI is up and is idempotent after
/// the first — so `1.1` and not `1.0` is the scale the shipped world camera actually runs at.
pub const GAMEPLAY_CAMERA_SCALE: f32 = 1.1;

/// The three `Camera.*` preferences, and where retail consumes each.
///
/// The registry points all three entries at the manager's own fields, so each profile value
/// lands in its field at construction:
/// registry construction immediately applies a preloaded value to the target field.
///
/// | preference | field | change callback | read by |
/// |---|---|---|---|
/// | `Camera.AlignToSlope` (bool) | slope alignment | none | the slope-alignment branch |
/// | `Camera.Stiffness` (`Float32`) | stiffness | apply to current camera | construction and the callback both apply the field |
/// | `Camera.AdjustmentSpeed` (`Float32`) | adjustment speed | none | closer, farther, raise, lower and rotate actions at rate 1.0 |
///
/// The profile carries the constructor defaults, so reading it into [`CameraManager`] changes
/// no pixel with the default profile; the wiring matters because nondefault profiles and the
/// options page both write these three values.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraPreferences {
    /// `Camera.AlignToSlope`.
    pub align_to_slope: bool,
    /// `Camera.Stiffness`.
    pub stiffness: f32,
    /// `Camera.AdjustmentSpeed`.
    pub adjustment_speed: f32,
}

impl CameraPreferences {
    pub const ALIGN_TO_SLOPE: &'static str = "Camera.AlignToSlope";
    pub const STIFFNESS: &'static str = "Camera.Stiffness";
    pub const ADJUSTMENT_SPEED: &'static str = "Camera.AdjustmentSpeed";
}

impl Default for CameraPreferences {
    /// The camera manager defaults are `true`, `0.45`, `40.0`.
    fn default() -> Self {
        Self {
            align_to_slope: true,
            stiffness: 0.45,
            adjustment_speed: 40.0,
        }
    }
}

/// Degrees to radians, as the client spells it (`0.017453292`).
const DEG_TO_RAD: f32 = 0.017_453_292;

/// The camera update's final early-out thresholds: `0x39D1B717` on the distance and
/// `0.0002` on the rotation comparison. They stop sub-pixel jitter.
const SETTLE_DISTANCE: f32 = 0.000_400_000_02; // 0x39D1B717
const SETTLE_ROTATION: f32 = 0.0002;

/// Compare all four quaternion components within `eps`.
#[must_use]
pub fn close_rotation(a: &Frame, b: &Frame, eps: f32) -> bool {
    (a.rotation.w - b.rotation.w).abs() < eps
        && (a.rotation.x - b.rotation.x).abs() < eps
        && (a.rotation.y - b.rotation.y).abs() < eps
        && (a.rotation.z - b.rotation.z).abs() < eps
}

/// Everything reads from the pivot object's snapshot selected by `pivot_object_id`.
///
/// A snapshot rather than a borrow: the camera runs after physics has finished with the object,
/// and taking a copy is what keeps this module free of the arena and of the physics handles.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PivotState {
    /// `position`.
    pub position: Position,
    /// The part's own position when `pivot_part_index` is valid. `None` falls back to
    /// `position`, which is also the fallback for index `-1`.
    pub part_position: Option<Position>,
    /// The object's velocity getter supplies this value.
    pub velocity: Vec3,
    /// `transient_state & 1` — on contact.
    pub in_contact: bool,
    /// `contact_plane.N`.
    pub contact_normal: Vec3,
}

/// The camera manager is 192 bytes and contains the fields camera update actually reads.
#[derive(Debug, Clone, PartialEq)]
pub struct CameraManager {
    /// Translational stiffness, `0…1`; 1 snaps.
    pub t_stiffness: f32,
    /// Rotational stiffness.
    pub r_stiffness: f32,
    pub pivot_object_id: ObjectId,
    /// `-1` means the object's own position.
    pub pivot_part_index: i32,
    /// In pivot-local space.
    pub pivot_offset: Vec3,
    pub target_object_id: ObjectId,
    pub target_part_index: i32,
    pub target_offset: Vec3,
    /// The look direction in pivot-local space; `+y` is forward.
    pub direction: Vec3,
    /// A [`target`] bit mask.
    pub target_status: u32,
    /// Eye position relative to the pivot frame **after** it has been aimed.
    pub viewer_offset: Vec3,
    /// Registry `Camera.AlignToSlope`.
    pub align_camera_to_slope: bool,
    /// Registry `Camera.Stiffness`.
    pub camera_stiffness: f32,
    /// Registry `Camera.AdjustmentSpeed` — the zoom/raise/lower rate multiplier.
    pub camera_adjustment_speed: f32,
    /// Debug: continuous pivot-offset nudging, a `flags_to_vector` direction mask.
    pub pivot_offset_movement: u32,
    /// Debug: continuous viewer-offset nudging.
    pub camera_offset_movement: u32,
    /// Speed for the two above.
    pub movement_speed: f32,
    /// Overall camera distance scale.
    pub scale: f32,
    pub last_update_time: f64,
    /// A rolling window of the pivot object's velocity.
    old_velocities: [Vec3; 5],
    old_velocity_num: u32,
    /// When false the camera update returns the input position unchanged.
    pub enabled: bool,
}

impl Default for CameraManager {
    /// The camera-manager constructor uses this default.
    fn default() -> Self {
        Self {
            t_stiffness: 0.45,
            r_stiffness: 0.45,
            pivot_object_id: ObjectId(0),
            pivot_part_index: -1,
            pivot_offset: Vec3::ZERO,
            target_object_id: ObjectId(0),
            target_part_index: -1,
            target_offset: Vec3::ZERO,
            direction: Vec3::new(0.0, 1.0, 0.0),
            target_status: target::LOOK_IN_DIRECTION,
            viewer_offset: Vec3::new(0.0, -3.0, 0.0),
            align_camera_to_slope: true,
            camera_stiffness: 0.45,
            camera_adjustment_speed: 40.0,
            pivot_offset_movement: 0,
            camera_offset_movement: 0,
            movement_speed: 1.0,
            scale: 1.0,
            last_update_time: 0.0,
            old_velocities: [Vec3::ZERO; 5],
            old_velocity_num: 0,
            enabled: true,
        }
    }
}

impl CameraManager {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the camera's pivot object.
    pub fn set_pivot_object(&mut self, id: ObjectId, part_index: i32) {
        self.pivot_object_id = id;
        self.pivot_part_index = part_index;
    }

    /// Set the camera's target object.
    pub fn set_target_object(&mut self, id: ObjectId, part_index: i32) {
        self.target_object_id = id;
        self.target_part_index = part_index;
    }

    /// Clear the camera's target.
    pub fn clear_target(&mut self) {
        self.target_status = target::INVALID_TARGET;
    }

    /// Add target-status bits. This **ORs** the mask rather than assigning it.
    pub fn set_target(&mut self, bits: u32) {
        self.target_status |= bits;
    }

    /// Set the camera's target direction.
    pub fn set_target_direction(&mut self, d: Vec3) {
        self.direction = d;
    }

    /// Set translational stiffness, rejecting values outside `0…1`, and return whether it took.
    pub fn set_translational_stiffness(&mut self, v: f32) -> bool {
        if (0.0..=1.0).contains(&v) {
            self.t_stiffness = v;
            return true;
        }
        false
    }

    /// Set the camera's rotational stiffness.
    pub fn set_rotational_stiffness(&mut self, v: f32) -> bool {
        if (0.0..=1.0).contains(&v) {
            self.r_stiffness = v;
            return true;
        }
        false
    }

    /// Set translational stiffness, then rotational stiffness only if the first value took.
    pub fn set_stiffness(&mut self, v: f32) -> bool {
        self.set_translational_stiffness(v) && self.set_rotational_stiffness(v)
    }

    /// Convert a 6-bit direction mask to a velocity.
    #[must_use]
    pub fn flags_to_vector(&self, flags: u32) -> Vec3 {
        let s = self.movement_speed;
        let x = match flags & 3 {
            2 => s,
            1 => -s,
            _ => 0.0,
        };
        let y = match flags & 0xC {
            4 => s,
            8 => -s,
            _ => 0.0,
        };
        let z = match flags & 0x30 {
            0x10 => s,
            0x20 => -s,
            _ => 0.0,
        };
        Vec3::new(x, y, z)
    }

    /// Build the pivot frame: use the pivot object's position (or its part's), rotate
    /// `pivot_offset` into that frame, and add it to the origin.
    ///
    /// **The returned rotation is the identity**, not the object's: the client default-constructs
    /// the `Position` and only ever writes its origin and its cell id.
    #[must_use]
    pub fn query_pivot_position(&self, pivot: &PivotState) -> Position {
        let src = match (self.pivot_part_index, pivot.part_position) {
            (i, Some(p)) if i != -1 => p,
            _ => pivot.position,
        };
        let origin = pmath::localtoglobal(&src.frame, self.pivot_offset);
        // Both branches keep the *object's* cell id: the client copies the pivot object's cell
        // id into the result before either.
        Position::new(pivot.position.cell, Frame::new(origin, Quat::IDENTITY))
    }

    /// Update the camera from its previous position.
    ///
    /// `current` is the previous camera `Position`; the return value is the new one. `target` is
    /// the `LOOK_AT_OBJECT` target's position in its own cell space — `None` when there is no such
    /// object, matching a failed object lookup.
    #[must_use]
    #[allow(clippy::too_many_lines)] // one client function, kept in one piece
    pub fn update_camera(
        &mut self,
        current: Position,
        pivot: &PivotState,
        target: Option<Position>,
        now: f64,
    ) -> Position {
        if !self.enabled {
            return current;
        }
        let dt = now - self.last_update_time;
        self.last_update_time = now;
        #[allow(clippy::cast_possible_truncation)]
        // LINT-OK: the client narrows the double delta
        // to a float before every one of its three uses. Not a value conversion of engine
        // arithmetic; `dereth_primitives::num::to_i32` is for float-to-int, which this is not.
        let dtf = dt as f32;

        if self.camera_offset_movement != 0 {
            let v = self.flags_to_vector(self.camera_offset_movement);
            self.viewer_offset = self.viewer_offset.add(v.mul(dtf));
        }
        if self.pivot_offset_movement != 0 {
            let v = self.flags_to_vector(self.pivot_offset_movement);
            self.pivot_offset = self.pivot_offset.add(v.mul(dtf));
        }

        let pivot_pos = self.query_pivot_position(pivot);
        // `P` — the pivot point in the camera's cell space.
        let p = pmath::pos_localtoglobal(&current, &pivot_pos, Vec3::ZERO);

        // `D` — the accumulated look direction.
        let mut d = Vec3::ZERO;
        if self.target_status & target::LOOK_AT_OBJECT != 0 {
            if let Some(t) = target {
                let tp = pmath::pos_localtoglobal(&current, &t, self.target_offset);
                let mut v = tp.sub(p);
                if !v.normalize_check_small() {
                    d = d.add(v);
                }
            }
        }

        let rot = pmath::l2g(pivot.position.frame.rotation);
        let pivot_forward = |v: Vec3| pmath::localtoglobalvec(rot, v);

        if !self.align_camera_to_slope || self.target_status & target::ALIGN_WITH_PLANE == 0 {
            if self.target_status & target::LOOK_IN_DIRECTION != 0 {
                d = d.add(pivot_forward(self.direction));
            }
        } else {
            d = d.add(self.slope_aligned_direction(pivot, &pivot_forward));
        }

        let mut dir = d;
        if dir.normalize_check_small() {
            dir = pivot_forward(self.direction);
        }

        // The ideal eye position: a frame at the pivot point, aimed along `dir`, with
        // `viewer_offset` in its local space.
        let mut aim = Frame::new(p, Quat::IDENTITY);
        pmath::set_vector_heading(&mut aim, dir);
        let eye_origin = pmath::localtoglobal(&aim, self.viewer_offset);
        let mut eye = Frame::new(eye_origin, Quat::IDENTITY);

        if self.target_status & target::LOOK_AT_PIVOT != 0 {
            let mut back = p.sub(eye_origin);
            if back.normalize_check_small() {
                back = Vec3::ZERO;
            }
            d = back;
        }
        let mut d2 = d;
        if d2.normalize_check_small() {
            d2 = pivot_forward(self.direction);
        }
        pmath::set_vector_heading(&mut eye, d2);

        // The smoother: an explicit-Euler exponential approach with the coefficient
        // `stiffness * dt * 10`, clamped to 1. Anything above 0.9998 snaps.
        let (ft, t_snapped) = approach(self.t_stiffness, dtf);
        let (fr, r_snapped) = approach(self.r_stiffness, dtf);

        let mut out = Position::new(current.cell, Frame::new(Vec3::ZERO, Quat::IDENTITY));
        out.frame.origin = pmath::interpolate_origin(&current.frame, &eye, ft);
        pmath::interpolate_rotation(&mut out.frame, &current.frame, &eye, fr);

        // The early-out: if neither channel snapped and nothing moved measurably, keep the old
        // position *exactly*.
        if !t_snapped
            && pmath::distance(&out, &current) < SETTLE_DISTANCE
            && !r_snapped
            && close_rotation(&out.frame, &current.frame, SETTLE_ROTATION)
        {
            return current;
        }
        out
    }

    /// The align-to-slope and `target_status & ALIGN_WITH_PLANE` branch, which is what
    /// tilts the camera when running up or down a hill.
    fn slope_aligned_direction(
        &mut self,
        pivot: &PivotState,
        pivot_forward: &dyn Fn(Vec3) -> Vec3,
    ) -> Vec3 {
        let fallback = |this: &Self| {
            if this.target_status & target::LOOK_IN_DIRECTION != 0 {
                pivot_forward(this.direction)
            } else {
                Vec3::ZERO
            }
        };

        let v = pivot.velocity;
        // The window shifts down by one and the new sample lands in slot 4; the sum is the four
        // survivors plus the new one, and `old_velocity_num` saturates at five.
        let sum = self.old_velocities[1]
            .add(self.old_velocities[2])
            .add(self.old_velocities[3])
            .add(self.old_velocities[4])
            .add(v);
        self.old_velocities[0] = self.old_velocities[1];
        self.old_velocities[1] = self.old_velocities[2];
        self.old_velocities[2] = self.old_velocities[3];
        self.old_velocities[3] = self.old_velocities[4];
        self.old_velocities[4] = v;
        if self.old_velocity_num < 5 {
            self.old_velocity_num += 1;
        }
        #[allow(clippy::cast_precision_loss)] // 1..=5
        let inv = 1.0 / self.old_velocity_num as f32;
        let mut vavg = sum.mul(inv);

        if vavg.normalize_check_small() || vavg.x.abs() < 0.0002 || vavg.y.abs() < 0.0002 {
            return fallback(self);
        }
        let up = if pivot.in_contact {
            pivot.contact_normal
        } else {
            vavg.z *= 0.1;
            if vavg.normalize_check_small() {
                Vec3::new(0.0, 0.0, 1.0)
            } else {
                let mut f = Frame::new(Vec3::ZERO, Quat::IDENTITY);
                pmath::set_vector_heading(&mut f, vavg);
                // Row **2** of the aimed frame — its local Z axis, i.e. the up perpendicular
                // to the flattened velocity.
                //
                // Not row 1 (entries 3..5): the value read is a `Frame`, not a `Position` whose
                // frame sits two floats in, so the read lands on entries 6..8, which form a row and
                // are the only ones that make the branch mean anything.
                let m = pmath::l2g(f.rotation).0;
                Vec3::new(m[6], m[7], m[8])
            }
        };
        let fwd = pivot_forward(Vec3::new(0.0, 1.0, 0.0));
        let mut proj = fwd.sub(up.mul(fwd.dot(up)));
        if proj.normalize_check_small() {
            return fallback(self);
        }
        if self.target_status & target::LOOK_IN_DIRECTION == 0 {
            return proj;
        }
        let mut f = Frame::new(Vec3::ZERO, Quat::IDENTITY);
        pmath::set_vector_heading(&mut f, proj);
        pmath::localtoglobalvec(pmath::l2g(f.rotation), self.direction)
    }
}

/// `ft = (stiffness > 0.9998) ? 1.0 : min(1.0, stiffness * dt * 10.0)`, plus whether it snapped.
///
/// The snap flag is not `ft == 1.0`: the client sets it only on the `> 0.9998` arm, and the
/// clamped arm leaves it false even when the product exceeds 1. That distinction is what the
/// final camera-update early-out reads.
fn approach(stiffness: f32, dt: f32) -> (f32, bool) {
    if stiffness > 0.9998 {
        return (1.0, true);
    }
    let f = stiffness * dt * 10.0;
    (if f > 1.0 { 1.0 } else { f }, false)
}

/// What asked the **command interpreter** to do with the body,
/// because turning the body is not the camera's to do.
///
/// Retail has three of these and they are three different command-interpreter requests, not one
/// bool: move the player, stop drift, and turn toward a heading.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CameraTurn {
    /// `MovePlayer(TurnRight|TurnLeft, 1, extent, 1, 1)` — the `Input.UseMouseTurning` arm.
    /// `command` is a `crate::actions::movement::command` motion command.
    MovePlayer { command: u32, extent: f32 },
    /// `TurnToHeading(heading, 0)` — the first-person arm, in **degrees**.
    TurnToHeading { heading: f32 },
    /// `StopDrift()` — the mouse-turning arm's dead zone, `|step| < 0.02`.
    StopDrift,
}

/// What one `CameraState` call asked the rest of the client to do, because it is not the camera's.
///
/// `look_down` and `set_map_mode` update the camera state directly, while
/// `Rotate` sends the *character* a turn through the command interpreter — in first person and
/// whenever `Input.UseMouseTurning` is on. Neither is a camera decision and neither is dropped:
/// they are reported, and [`crate::app::App::apply_camera_turn`] is what reads the second.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CameraEffects {
    /// Disabling degradation and fogging — the overhead and map modes.
    pub degrades_disabled: bool,
    /// The turn `Rotate` last decided on, drained by the caller.
    pub turn: Option<CameraTurn>,
    /// `SendMovementEvent` — the mouse-turning arm's own 0.5 s throttle,
    /// which is not the position reporter's.
    pub send_movement_event: bool,
}

/// The three things every rate-limited `CameraState` method reads that are not the camera's own.
///
/// The current time, the frame rate (which seeds a limiter on its first call), and
/// `Input.UseMouseTurning` (a byte in the global input-map settings), bundled because the
/// client reads all three off globals and a rebuild would otherwise thread them through
/// nine-argument signatures.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraTick {
    /// The current timer value.
    pub now: f64,
    /// The scene's frames-per-second value.
    pub fps: f32,
    /// `Input.UseMouseTurning`.
    pub mouse_turning: bool,
    /// The player's heading in degrees, or `None` when no player exists.
    ///
    /// Both body-turn arms perform the same smart-box player-pointer read and then
    /// read the player's heading.
    /// `None` is the null pointer, and both body-turn arms return without touching the camera on
    /// it — and both jump to the `!hold` tail.
    pub player_heading: Option<f32>,
}

impl CameraTick {
    /// `now - 1.0 / fps`, the seed every limiter takes on its first call.
    ///
    /// **One guard the client does not have.** Frame rate is 0 until the first
    /// frame ends, and `now - 1/0` is `-inf`, which makes the first step `inf` and the
    /// viewer offset `NaN`. The client cannot reach that — its input is pumped from *inside* the
    /// frame loop, so a frame has always ended before an action arrives — and this rebuild can,
    /// because the window's message pump can deliver an event before the first `App::frame`. A
    /// zero frame rate therefore seeds the limiter to `now`, which is a zero-length step and makes
    /// the call return at its own rate gate. It costs one dropped input event on a state the
    /// original never enters.
    #[must_use]
    pub fn seed(&self) -> f64 {
        if self.fps > 0.0 {
            self.now - 1.0 / f64::from(self.fps)
        } else {
            self.now
        }
    }
}

/// The camera set is the user-facing layer.
///
/// The client's user-facing camera layer holds a pointer to its manager; here every method takes
/// `&mut CameraManager`, the same relationship without that pointer. Both are held together by
/// [`CameraControl`].
#[derive(Debug, Clone, PartialEq)]
pub struct CameraState {
    /// `current_stiffness` — the value `set_stiffness` restores when the offset leaves the
    /// steep-overhead band.
    pub current_stiffness: f32,
    pub looking_down: bool,
    pub in_map_mode: bool,
    pub mouselook_active: bool,
    /// `TrackTarget` has selected an object.
    pub targeting: bool,
    pub rot_left: bool,
    pub rot_right: bool,
    pub raise: bool,
    pub lower: bool,
    pub closer: bool,
    pub farther: bool,
    /// The three rate limiters. Zero means "not started"; the first call seeds them to
    /// `now - 1/frames_per_second`.
    pub last_rotate: f64,
    pub last_raise_or_lower: f64,
    pub last_zoom: f64,
    /// The last-server-message time, used by the mouse-turning arm's own 0.5 s throttle on
    /// `SendMovementEvent`. Shared with the mouse-look handler's idle tick in retail; here the idle
    /// tick's copy lives in [`crate::actions::camera::MouseLook::last_server_message`] and this is
    /// `Rotate`'s.
    pub last_server_message: f64,
    /// `lookdown_saved_offset` / `lookdown_saved_target_dir`.
    lookdown_saved_offset: Vec3,
    lookdown_saved_target_dir: Vec3,
    /// What the last call asked of the rest of the client.
    pub effects: CameraEffects,
}

impl Default for CameraState {
    fn default() -> Self {
        Self {
            current_stiffness: 0.45,
            looking_down: false,
            in_map_mode: false,
            mouselook_active: false,
            targeting: false,
            rot_left: false,
            rot_right: false,
            raise: false,
            lower: false,
            closer: false,
            farther: false,
            last_rotate: 0.0,
            last_raise_or_lower: 0.0,
            last_zoom: 0.0,
            last_server_message: 0.0,
            lookdown_saved_offset: Vec3::ZERO,
            lookdown_saved_target_dir: Vec3::new(0.0, 1.0, 0.0),
            effects: CameraEffects::default(),
        }
    }
}

impl CameraState {
    /// Camera-set construction copies `cm.t_stiffness` into
    /// `current_stiffness`, then applies the default offsets.
    #[must_use]
    pub fn new(cm: &mut CameraManager, player: ObjectId, t: CameraTick) -> Self {
        let mut s = Self {
            current_stiffness: cm.t_stiffness,
            ..Self::default()
        };
        s.set_default_offsets(cm, player, t);
        s
    }

    /// The in-head camera uses `viewer_offset == (0, 0.18, 0)`, exactly.
    #[must_use]
    pub fn in_head(cm: &CameraManager) -> bool {
        cm.viewer_offset == IN_HEAD_OFFSET
    }

    /// Derive the target mask implied by a viewer offset.
    ///
    /// Every float comparison here is `a <= k` and is written that way.
    pub fn set_target_for_offset(&self, cm: &mut CameraManager, offset: Vec3) {
        cm.clear_target();
        if !self.looking_down {
            if offset != IN_HEAD_OFFSET {
                if self.targeting {
                    cm.set_target(target::LOOK_AT_PIVOT | target::LOOK_AT_OBJECT);
                    return;
                }
                if offset.x <= 1.0 && offset.x >= -1.0 && offset.y <= 0.0 {
                    cm.set_target(target::ALIGN_WITH_PLANE);
                    if offset.y <= -0.5 && offset.z >= -0.5 {
                        cm.set_target(target::LOOK_IN_DIRECTION);
                    }
                    if offset.z <= 0.75 && offset.z >= -0.5 {
                        return;
                    }
                }
                cm.set_target(target::LOOK_AT_PIVOT);
                return;
            }
            cm.set_target(target::ALIGN_WITH_PLANE);
        }
        cm.set_target(target::LOOK_IN_DIRECTION);
    }

    /// Install the shipped third-person camera: 2.5 units
    /// behind and 0.75 above a pivot 1.5 units above the player's origin, scaled by `cm.scale`.
    pub fn set_default_offsets(&mut self, cm: &mut CameraManager, player: ObjectId, t: CameraTick) {
        if self.looking_down {
            self.look_down(cm, false);
        }
        if Self::in_head(cm) {
            // Step out of first person first, with the client's own `Farther(0, 1.0)`.
            self.farther(cm, false, 1.0, player, t);
        }
        let s = cm.scale;
        if player.0 != 0 {
            cm.set_pivot_object(player, -1);
            cm.pivot_offset = Vec3::new(0.0, 0.0, 1.5);
        }
        cm.set_target_direction(Vec3::new(0.0, 1.0, 0.0));
        let st = self.current_stiffness;
        if cm.set_translational_stiffness(st) {
            cm.set_rotational_stiffness(st);
        }
        cm.viewer_offset = Vec3::new(0.0, -2.5 * s, 0.75 * s);
        let off = cm.viewer_offset;
        self.set_target_for_offset(cm, off);
    }

    /// Enter first person, with no positional lag.
    pub fn set_in_head(&mut self, cm: &mut CameraManager) {
        if self.looking_down {
            self.look_down(cm, false);
        }
        cm.set_translational_stiffness(1.0);
        cm.set_target_direction(Vec3::new(0.0, 1.0, 0.0));
        cm.viewer_offset = IN_HEAD_OFFSET;
        self.set_target_for_offset(cm, IN_HEAD_OFFSET);
    }

    /// Set both stiffness values; in first person only the rotational half moves.
    pub fn set_stiffness(&mut self, cm: &mut CameraManager, v: f32) {
        self.current_stiffness = v;
        // In first person only the rotational half moves; otherwise the rotational half moves
        // only when the translational one took, as in the original.
        if Self::in_head(cm) || cm.set_translational_stiffness(v) {
            cm.set_rotational_stiffness(v);
        }
    }

    /// Rescale the whole viewer offset, and **re-derive the
    /// target mask from the scaled offset**.
    ///
    /// That second half is the entire camera pitch. Scaling returns at once in first person or
    /// when the scale is unchanged
    /// (so it is idempotent); otherwise it divides the offset by the old scale, multiplies by
    /// the new one, sets translational stiffness to 1.0 while the eye jumps, stores the new
    /// scale, and re-derives the target mask with `set_target_for_offset` before restoring the
    /// previous stiffness.
    ///
    /// Gameplay UI simulation opens by applying scale 1.1 when a camera set exists —
    /// unconditionally, every frame the gameplay UI is up, which is every frame in the world.
    /// So the shipped third-person offset is **not** `(0, −2.5, 0.75)` but that times
    /// [`GAMEPLAY_CAMERA_SCALE`]: `(0, −2.75, 0.825)`.
    ///
    /// `0.825 > 0.75` is the knife edge in [`Self::set_target_for_offset`]
    /// (the comparison is strictly greater than 0.75), so the scaled offset picks up
    /// [`target::LOOK_AT_PIVOT`] where the unscaled one does not. With it,
    /// the camera update re-aims the eye down the vector back to the pivot, and the
    /// camera sits pitched **down `atan(0.825 / 2.75)` = 16.7°** instead of dead level. That
    /// pitch is visible in the retail reference view, whose vertical edges converge; the unscaled
    /// view keeps them dead vertical.
    pub fn set_scale(&mut self, cm: &mut CameraManager, v: f32) {
        if Self::in_head(cm) {
            return;
        }
        if cm.scale == v {
            return;
        }
        let mut off = cm.viewer_offset;
        if cm.scale != 1.0 {
            let inv = 1.0 / cm.scale;
            off = off.mul(inv);
        }
        if v != 1.0 {
            off = off.mul(v);
        }
        // The stiffness is forced to 1.0 *across the change* so the camera does not lag toward
        // the new offset, then restored from `current_stiffness` once the mask is re-derived.
        if cm.set_translational_stiffness(1.0) {
            cm.set_rotational_stiffness(1.0);
        }
        cm.scale = v;
        cm.viewer_offset = off;
        self.set_target_for_offset(cm, off);
        let st = self.current_stiffness;
        if cm.set_translational_stiffness(st) {
            cm.set_rotational_stiffness(st);
        }
    }

    /// Track the camera target.
    pub fn track_target(&mut self, cm: &mut CameraManager, id: ObjectId) {
        if id.0 == 0 {
            self.targeting = false;
        } else {
            self.targeting = true;
            cm.set_target_object(id, -1);
            cm.target_offset = Vec3::new(0.0, 0.0, 0.5);
        }
        let off = cm.viewer_offset;
        self.set_target_for_offset(cm, off);
    }

    /// Look down in overhead mode.
    pub fn look_down(&mut self, cm: &mut CameraManager, on: bool) {
        if on == self.looking_down {
            return;
        }
        self.looking_down = on;
        let new_offset = if on {
            self.lookdown_saved_offset = cm.viewer_offset;
            self.lookdown_saved_target_dir = cm.direction;
            cm.set_target_direction(Vec3::new(0.0, 0.5, -1.8));
            if self.in_map_mode {
                if cm.set_translational_stiffness(1.0) {
                    cm.set_rotational_stiffness(1.0);
                }
                self.effects.degrades_disabled = true;
                Vec3::new(0.0, -450.0, 0.75)
            } else {
                if self.lookdown_saved_offset == IN_HEAD_OFFSET {
                    cm.set_stiffness(self.current_stiffness);
                }
                Vec3::new(0.0, -2.0, 0.75)
            }
        } else {
            if self.in_map_mode {
                self.in_map_mode = false;
                if self.lookdown_saved_offset != IN_HEAD_OFFSET {
                    cm.set_stiffness(self.current_stiffness);
                }
                self.effects.degrades_disabled = false;
            } else if self.lookdown_saved_offset == IN_HEAD_OFFSET {
                cm.set_stiffness(1.0);
            }
            cm.set_target_direction(self.lookdown_saved_target_dir);
            self.lookdown_saved_offset
        };
        cm.viewer_offset = new_offset;
        self.set_target_for_offset(cm, new_offset);
    }

    /// Set map mode.
    pub fn set_map_mode(&mut self, cm: &mut CameraManager, on: bool) {
        if on == self.in_map_mode {
            return;
        }
        if on {
            self.in_map_mode = true;
        }
        if self.in_map_mode && cm.set_translational_stiffness(1.0) {
            cm.set_rotational_stiffness(1.0);
        }
        self.look_down(cm, on);
        if self.in_map_mode {
            let (x, z) = (cm.viewer_offset.x, cm.viewer_offset.z);
            self.effects.degrades_disabled = true;
            cm.viewer_offset = Vec3::new(x, -450.0, z);
        }
    }

    /// Move the camera closer by uniformly scaling the whole offset by `1 - step * 0.2`,
    /// refused when it would bring the eye closer than 0.5 units to the pivot.
    pub fn closer(&mut self, cm: &mut CameraManager, hold: bool, rate: f32, t: CameraTick) {
        if rate == 0.0 {
            return;
        }
        self.closer = hold;
        self.farther = false;
        if self.looking_down {
            self.look_down(cm, false);
        }
        let off = cm.viewer_offset;
        if off == IN_HEAD_OFFSET {
            // Reaching first person stops zooming in entirely.
            return;
        }
        if self.last_zoom == 0.0 {
            self.last_zoom = t.seed();
        }
        let elapsed = t.now - self.last_zoom;
        if elapsed < ZOOM_MIN_INTERVAL {
            return;
        }
        #[allow(clippy::cast_possible_truncation)]
        // LINT-OK: the client computes elapsed zoom time times camera adjustment speed in
        // double precision and stores the product into a float offset; this is that store.
        let step = if rate == 1.0 {
            (elapsed * f64::from(cm.camera_adjustment_speed)) as f32
        } else {
            1.0 / rate
        };
        let k = 1.0 - step * 0.2;
        let new = off.mul(k);
        self.last_zoom = t.now;
        if new.mag2().sqrt() < 0.5 {
            return;
        }
        cm.set_stiffness(if new.z <= -1.8 {
            1.0
        } else {
            self.current_stiffness
        });
        cm.viewer_offset = new;
        self.set_target_for_offset(cm, new);
        if !hold {
            self.closer = false;
            self.last_zoom = 0.0;
        }
    }

    /// Move the camera farther away.
    ///
    /// Note the order: the hold flags and the look-down cancel happen **before** the rate gate, so
    /// a call the gate refuses still records that the key is down and still leaves overhead mode.
    /// `Closer`, `Raise` and `Lower` are each ordered differently again and each is transcribed as
    /// it stands.
    pub fn farther(
        &mut self,
        cm: &mut CameraManager,
        hold: bool,
        rate: f32,
        player: ObjectId,
        t: CameraTick,
    ) {
        let now = t.now;
        self.farther = hold;
        self.closer = false;
        if self.looking_down {
            self.look_down(cm, false);
        }
        if self.last_zoom == 0.0 {
            self.last_zoom = t.seed();
        }
        if now - self.last_zoom < ZOOM_MIN_INTERVAL {
            return;
        }
        let off = cm.viewer_offset;
        let new = if off == IN_HEAD_OFFSET {
            // Leaving first person: re-pivot on the player and jump to (0, -0.6, 0.5).
            self.last_zoom = now;
            if player.0 != 0 {
                cm.set_pivot_object(player, -1);
                cm.pivot_offset = Vec3::new(0.0, 0.0, 1.5);
            }
            let st = self.current_stiffness;
            if cm.set_translational_stiffness(st) {
                cm.set_rotational_stiffness(st);
            }
            Vec3::new(0.0, -0.6, 0.5)
        } else {
            let elapsed = now - self.last_zoom;
            #[allow(clippy::cast_possible_truncation)] // LINT-OK: see `closer`.
            let step = if rate == 1.0 {
                (elapsed * f64::from(cm.camera_adjustment_speed)) as f32
            } else {
                rate
            };
            let k = 1.0 + step * 0.2;
            let n = off.mul(k);
            self.last_zoom = now;
            if n.x.abs() > 10.0 || n.y.abs() > 10.0 || n.z > 450.0 {
                return;
            }
            cm.set_stiffness(if n.z <= -1.8 {
                1.0
            } else {
                self.current_stiffness
            });
            n
        };
        cm.viewer_offset = new;
        self.set_target_for_offset(cm, new);
        if !hold {
            self.farther = false;
            self.last_zoom = 0.0;
        }
    }

    /// Raise the camera by swinging the offset up around the pivot by `angle * step`,
    /// keeping its length. `CameraTick::mouse_turning` is `Input.UseMouseTurning`, which quarters
    /// the step.
    pub fn raise(&mut self, cm: &mut CameraManager, hold: bool, rate: f32, t: CameraTick) {
        self.raise = hold;
        self.lower = false;
        if self.last_raise_or_lower == 0.0 {
            self.last_raise_or_lower = t.seed();
        }
        if t.now - self.last_raise_or_lower < ZOOM_MIN_INTERVAL {
            return;
        }
        let off = cm.viewer_offset;
        if self.looking_down {
            // Retail's test is `y >= -9.8`.
            if off.y >= -9.8 {
                cm.viewer_offset = Vec3::new(off.x, off.y - 0.2, off.z);
            }
        } else if off == IN_HEAD_OFFSET {
            // In first person the *direction* pitches instead, clamped to +/- 0.8.
            let d = cm.direction;
            cm.set_target_direction(Vec3::new(d.x, d.y, (d.z - 0.2).clamp(-0.8, 0.8)));
        } else {
            let step = self.swing_step(cm, rate, t);
            let mag = off.mag2().sqrt();
            let h = vector_heading(off) * DEG_TO_RAD;
            let theta = CAMERA_ANGLE_STEP * step + vector_pitch(off) * DEG_TO_RAD;
            let (sc, ss) = (math::cosf(theta), math::sinf(theta));
            let new = Vec3::new(math::sinf(h) * mag * sc, math::cosf(h) * mag * sc, ss * mag);
            // Refuse a step that would flip either horizontal component's sign.
            if off.y >= 0.0 {
                if off.y > 0.0 && new.y <= 0.0 {
                    return;
                }
            } else if new.y >= 0.0 {
                return;
            }
            if off.x >= 0.0 {
                if off.x > 0.0 && new.x <= 0.0 {
                    return;
                }
            } else if new.x >= 0.0 {
                return;
            }
            if new.z > -1.8 {
                cm.set_stiffness(self.current_stiffness);
            }
            cm.set_target_direction(Vec3::new(0.0, 1.0, 0.0));
            cm.viewer_offset = new;
            self.set_target_for_offset(cm, new);
        }
        if !hold {
            self.raise = false;
            self.last_raise_or_lower = 0.0;
        }
    }

    /// Lower the camera.
    pub fn lower(&mut self, cm: &mut CameraManager, hold: bool, rate: f32, t: CameraTick) {
        self.lower = hold;
        self.raise = false;
        if self.last_raise_or_lower == 0.0 {
            self.last_raise_or_lower = t.seed();
        }
        if t.now - self.last_raise_or_lower < ZOOM_MIN_INTERVAL {
            return;
        }
        let off = cm.viewer_offset;
        if self.looking_down {
            if off.y <= 0.8 {
                cm.viewer_offset = Vec3::new(off.x, off.y + 0.2, off.z);
            }
        } else if off == IN_HEAD_OFFSET {
            let d = cm.direction;
            cm.set_target_direction(Vec3::new(d.x, d.y, (d.z + 0.2).clamp(-0.8, 0.8)));
        } else {
            let step = self.swing_step(cm, rate, t);
            let mag = off.mag2().sqrt();
            let h = vector_heading(off) * DEG_TO_RAD;
            let theta = vector_pitch(off) * DEG_TO_RAD - CAMERA_ANGLE_STEP * step;
            let (sc, ss) = (math::cosf(theta), math::sinf(theta));
            let new = Vec3::new(math::sinf(h) * mag * sc, math::cosf(h) * mag * sc, ss * mag);
            // Refuse a step that would drop the eye below the pivot inside a 1.2 m cylinder.
            if new.x * new.x + new.y * new.y < 1.44 && new.z < 0.0 {
                return;
            }
            if new.z <= -1.8 {
                cm.set_stiffness(1.0);
            }
            cm.set_target_direction(Vec3::new(0.0, 1.0, 0.0));
            cm.viewer_offset = new;
            self.set_target_for_offset(cm, new);
        }
        if !hold {
            self.lower = false;
            self.last_raise_or_lower = 0.0;
        }
    }

    /// The step `Raise` and `Lower` share, including the mouse-turning quarter.
    fn swing_step(&mut self, cm: &CameraManager, rate: f32, t: CameraTick) -> f32 {
        let mut s = if rate == 1.0 {
            (t.now - self.last_raise_or_lower) * f64::from(cm.camera_adjustment_speed)
        } else {
            f64::from(rate)
        };
        if t.mouse_turning {
            s *= 0.25;
        }
        self.last_raise_or_lower = t.now;
        #[allow(clippy::cast_possible_truncation)] // LINT-OK: see `closer`.
        {
            s as f32
        }
    }

    /// Swing the offset around the pivot in the horizontal plane,
    /// **or turn the body**, which is two of its three arms and neither of them is the camera's.
    ///
    /// `from_camera_action` is true when the key came from the camera action
    /// map, 0 when it came from the mouse-look handler. The distinction matters:
    ///
    /// Disabling mouse turning or passing a nonzero `from_camera_action` selects the other arm.
    ///
    /// The mouse-turning arm requires mouse turning and a non-camera-action input, and **the offset is not
    /// part of that test**. The conditions split into three arms:
    ///
    /// * mouse turning with non-camera input → request player turn-right or turn-left with the extent,
    ///   **at any offset**, first person or not;
    /// * otherwise, `viewer_offset == (0, 0.18, 0)` — the **first-person** offset —
    ///   → `TurnToHeading(get_heading() ± 8°, 0)`;
    /// * otherwise the camera orbits.
    ///
    /// `left` is the camera's sense, not the body's, and the two are opposite on purpose:
    /// Camera action dispatch passes `left = (action == 0x35)` for *Rotate Camera
    /// Left*, which spins the camera to the player's left and therefore swings the **view** to the
    /// right. First person keeps that view sense by turning the body right: it issues
    /// `TurnRight 0x6500000D` for `left != 0`, and adds `+8` to a heading that
    /// the frame measures **clockwise** from north.
    ///
    /// What the body then does with the turn is the command interpreter's; it is reported in
    /// [`CameraEffects::turn`] and drained by [`crate::app::App::apply_camera_turn`].
    pub fn rotate(
        &mut self,
        cm: &mut CameraManager,
        left: bool,
        hold: bool,
        rate: f32,
        from_camera_action: bool,
        t: CameraTick,
    ) {
        if self.last_rotate == 0.0 {
            self.last_rotate = t.seed();
        }
        if t.now - self.last_rotate < ZOOM_MIN_INTERVAL {
            return;
        }
        if self.looking_down {
            self.look_down(cm, false);
        }
        let off = cm.viewer_offset;
        if !t.mouse_turning || from_camera_action {
            if off == IN_HEAD_OFFSET {
                // **First person turns the body, not the camera.**
                //
                // With a player object it reads in degrees, adds or
                // subtracts 8, wraps at 360 (heading - 352 on the other side), and calls
                // `TurnToHeading(h, 0)`; with no player object it falls through to the !hold tail.
                //
                // The right arm is the mirror: `heading - 8`, and `< 0` wraps to
                // `360 - 8 + heading`. **This arm never writes the last-rotate time** — the orbit and
                // mouse-turning arms both do — so the 0.0002 s gate stays open and a held key
                // re-aims 8 degrees ahead of the *current* heading on every frame, which is what
                // makes it a continuous turn rather than one 8-degree step.
                let Some(h) = t.player_heading else {
                    return self.release(hold);
                };
                let heading = if left {
                    self.rot_left = hold;
                    self.rot_right = false;
                    let n = CAMERA_TURN_DEGREES + h;
                    if n >= HEADING_WRAP_DEGREES {
                        h - (HEADING_WRAP_DEGREES - CAMERA_TURN_DEGREES)
                    } else {
                        n
                    }
                } else {
                    self.rot_right = hold;
                    self.rot_left = false;
                    let n = h - CAMERA_TURN_DEGREES;
                    if n < 0.0 {
                        HEADING_WRAP_DEGREES - CAMERA_TURN_DEGREES + h
                    } else {
                        n
                    }
                };
                self.effects.turn = Some(CameraTurn::TurnToHeading { heading });
            } else {
                #[allow(clippy::cast_possible_truncation)] // LINT-OK: see `closer`.
                let step = if rate == 1.0 {
                    ((t.now - self.last_rotate) * f64::from(cm.camera_adjustment_speed)) as f32
                } else {
                    rate
                };
                let (s, c) = math::sin_cosf(step * CAMERA_ANGLE_STEP);
                let (nx, ny) = if left {
                    self.rot_left = hold;
                    self.rot_right = false;
                    (off.x * c + s * off.y, c * off.y - s * off.x)
                } else {
                    self.rot_right = hold;
                    self.rot_left = false;
                    (off.x * c - s * off.y, s * off.x + c * off.y)
                };
                self.last_rotate = t.now;
                cm.set_target_direction(Vec3::new(0.0, 1.0, 0.0));
                let new = Vec3::new(nx, ny, off.z);
                cm.viewer_offset = new;
                self.set_target_for_offset(cm, new);
            }
        } else {
            // **`Input.UseMouseTurning`**. The command interpreter turns the body at
            // any offset, and a step inside the dead zone stops the drift instead.
            //
            // A missing player takes the release tail. Otherwise compute the step from elapsed
            // time and adjustment speed. Steps strictly between -0.02 and +0.02 stop drift and
            // return; the boundary values issue movement. Double the step for the extent, cap
            // its upper value at 1.5, then issue the move-player command.
            //
            // The stop-drift arm returns **without** the release tail or a
            // last-rotation timestamp write. The issuing arm records the current time, then
            // runs its own 0.5 s movement-event throttle.
            if t.player_heading.is_none() {
                return self.release(hold);
            }
            let step = if rate == 1.0 {
                (t.now - self.last_rotate) * f64::from(cm.camera_adjustment_speed)
            } else {
                f64::from(rate)
            };
            if step < MOUSE_TURN_DEAD_ZONE && step > -MOUSE_TURN_DEAD_ZONE {
                self.effects.turn = Some(CameraTurn::StopDrift);
                return;
            }
            let doubled = step + step;
            #[allow(clippy::cast_possible_truncation)]
            // LINT-OK: the client stores it as a 32-bit float.
            let extent = if doubled > MOUSE_TURN_MAX_EXTENT {
                MOUSE_TURN_MAX_EXTENT as f32
            } else {
                doubled as f32
            };
            // `left` is the camera's sense and the body's is its opposite: the branch
            // takes `left == 0` to `TurnLeft 0x6500000E` and leaves `left != 0` on
            // `TurnRight 0x6500000D`.
            self.effects.turn = Some(CameraTurn::MovePlayer {
                command: if left {
                    crate::actions::movement::command::TURN_RIGHT
                } else {
                    crate::actions::movement::command::TURN_LEFT
                },
                extent,
            });
            self.last_rotate = t.now;
            if t.now > self.last_server_message + ROTATE_SERVER_MESSAGE_INTERVAL {
                self.last_server_message = t.now;
                self.effects.send_movement_event = true;
            }
        }
        self.release(hold);
    }

    /// The input-action tail tests the `hold` argument, and on a release it clears `rot_left`, `rot_right` and the last-rotate timestamp.
    /// Split out because three arms reach it and two others deliberately do not.
    fn release(&mut self, hold: bool) {
        if !hold {
            self.rot_left = false;
            self.rot_right = false;
            self.last_rotate = 0.0;
        }
    }

    /// The camera set's held-key repeat runs once per frame during gameplay
    /// smart-box simulation, before the camera update.
    ///
    /// The translucency arm (whole-hierarchy translucency as the camera closes on the body) is
    /// reported through [`CameraControl::player_translucency`] rather than applied here.
    pub fn update_held_keys(&mut self, cm: &mut CameraManager, player: ObjectId, t: CameraTick) {
        if self.rot_left {
            self.rotate(cm, true, true, 1.0, true, t);
        } else if self.rot_right {
            self.rotate(cm, false, true, 1.0, true, t);
        }
        if self.closer {
            self.closer(cm, true, 1.0, t);
        } else if self.farther {
            self.farther(cm, true, 1.0, player, t);
        }
        if self.raise {
            self.raise(cm, true, 1.0, t);
        } else if self.lower {
            self.lower(cm, true, 1.0, t);
        }
    }
}

/// Heading in **degrees**, 0 = +Y, increasing clockwise, from
/// the X/Y components alone. A degenerate XY answers 0.
#[must_use]
pub fn vector_heading(v: Vec3) -> f32 {
    let mut flat = Vec3::new(v.x, v.y, 0.0);
    if flat.normalize_check_small() {
        return 0.0;
    }
    dereth_primitives::frame::vector_get_heading(flat)
}

/// Pitch in **degrees**, `asin(z)` after normalising.
#[must_use]
pub fn vector_pitch(v: Vec3) -> f32 {
    let mut n = v;
    if n.normalize_check_small() {
        return 0.0;
    }
    #[allow(clippy::cast_possible_truncation)] // the client's own float return
    {
        (math::asin(f64::from(n.z)) * 57.295_779_513_082_32) as f32
    }
}

/// Smoothed frames-per-second estimate over a twenty-frame window: `20 / Σ(the last 20 frame
/// deltas)`, or 0 when that sum is at or below 0.0002.
///
/// The client has **one** scene frame rate: the camera's three rate limiters and the render
/// degradation calculation both read it, so there is one meter and not two that could drift.
///
/// The client samples local time between flips. **The ring starts zeroed and the divisor is the
/// whole sum**, so the first twenty frames over-report the frame rate by up to 20x; that is the
/// client's own arithmetic and it is what the degrade-level calculation sees at start-up, so it is
/// reproduced rather than primed.
///
/// It measures *whatever clock it is pushed*: a headless run steps a fixed quantum, so the meter
/// reports the same number on every machine.
#[derive(Debug, Clone, Copy)]
pub struct FrameRate {
    deltas: [f64; Self::WINDOW],
    next: usize,
}

impl Default for FrameRate {
    fn default() -> Self {
        Self {
            deltas: [0.0; Self::WINDOW],
            next: 0,
        }
    }
}

impl FrameRate {
    /// The window length, in frames.
    pub const WINDOW: usize = 20;

    /// At or below this sum the meter reports 0.
    pub const MIN_SUM: f64 = 0.0002;

    /// Record one frame's duration, in seconds.
    pub fn push(&mut self, dt: f64) {
        self.deltas[self.next] = dt;
        self.next = (self.next + 1) % Self::WINDOW;
    }

    /// Frames per second over the window.
    #[must_use]
    pub fn fps(&self) -> f32 {
        let sum: f64 = self.deltas.iter().sum();
        if sum > Self::MIN_SUM {
            #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
            // LINT-OK: the client's own float frame rate, `20.0 / sum`.
            {
                (Self::WINDOW as f64 / sum) as f32
            }
        } else {
            0.0
        }
    }
}

// -------------------------------------------------------------------------------------------
// The camera's half of the frame: what the frame loop calls with the drawn world in hand.
// -------------------------------------------------------------------------------------------

/// Mouse-look, in raw pixels of cursor motion, routed to whichever camera is live.
///
/// With a body this goes through the mouse-look handler in `crate::actions::camera`; without one
/// it is the debug flycam's look, unchanged.
pub fn mouse_look(scene: &mut dyn crate::present::SceneMut, dx: f32, dy: f32, now: LocalTime) {
    if let Some(c) = scene.world_mut().character.as_mut() {
        c.camera.mouse_look(
            dereth_primitives::num::to_i32(dx),
            dereth_primitives::num::to_i32(dy),
            now,
        );
    } else {
        scene.world_mut().look(dx, dy);
    }
}

/// Run the real camera for this frame and hand the result to the scene.
///
/// Called from [`crate::app::App::frame`] immediately after the scene's update, matching the
/// original viewer-update order: physics has
/// already moved the body, and the landblock window has already been re-centred, so the frame this
/// produces is in the same viewer-block space the scene draws in.
///
/// Without a body there is nothing to pivot on and the debug free camera is left alone;
/// that is the `--no-character` path, and it is the *only* thing the debug camera still does.
pub fn update_viewer(
    scene: &mut dyn crate::present::SceneMut,
    input: CameraInput,
    now: LocalTime,
    dt: f64,
) {
    let Some(c) = scene.world_mut().character.as_mut() else {
        return;
    };
    c.update_camera(input, now, dt);
    // Finish the camera update with the body-translucency pass. It runs after
    // the sweep has placed `viewer` (which is what [`CameraControl::update_translucency`] measures
    // to) and before the frame is drawn, so the body fades on the frame the camera closes in.
    scene.apply_camera_translucency();
    let Some(c) = scene.world_mut().character.as_mut() else {
        return;
    };
    let Some(f) = c.camera_render_frame() else {
        return;
    };
    scene.world_mut().camera = FreeCamera::from_frame(&f);
    trace_live_camera(scene, &f);
}

/// **A diagnostic instrument, not a client feature.** With the `dereth::trace::camera` target on
/// in the log filter ([`crate::trace::CAMERA`]), print the body's `position`, the `CameraManager`
/// fields that decide the eye, and the render frame, once every 30 frames (at `debug`).
/// The paired live camera capture reads these fields from the retail process, so they can be
/// compared number for number rather than
/// pixel for pixel alone. Off (one filter check per frame) unless asked for.
fn trace_live_camera(scene: &dyn crate::present::Scene, f: &Frame) {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    if !crate::trace::camera() {
        return;
    }
    let n = N.fetch_add(1, Ordering::Relaxed);
    if !n.is_multiple_of(30) {
        return;
    }
    let Some(c) = scene.character() else { return };
    let cam = &c.camera;
    let m = &cam.manager;
    let body = c.position();
    let v = cam.viewer;
    let fc = FreeCamera::from_frame(f);
    tracing::debug!(
        target: "dereth::trace::camera",
        "camera-trace frame={n} body cell=0x{:08X} origin=({:.6}, {:.6}, {:.6}) q=({:.6}, {:.6}, {:.6}, {:.6})",
        body.cell.0,
        body.frame.origin.x,
        body.frame.origin.y,
        body.frame.origin.z,
        body.frame.rotation.w,
        body.frame.rotation.x,
        body.frame.rotation.y,
        body.frame.rotation.z,
    );
    tracing::debug!(
        target: "dereth::trace::camera",
        "camera-trace viewer cell=0x{:08X} origin=({:.6}, {:.6}, {:.6}) q=({:.6}, {:.6}, {:.6}, {:.6}) viewer_cell={:?}",
        v.cell.0,
        v.frame.origin.x,
        v.frame.origin.y,
        v.frame.origin.z,
        v.frame.rotation.w,
        v.frame.rotation.x,
        v.frame.rotation.y,
        v.frame.rotation.z,
        cam.viewer_cell.map(|c| c.0),
    );
    tracing::debug!(
        target: "dereth::trace::camera",
        "camera-trace manager pivot_offset=({:.4}, {:.4}, {:.4}) direction=({:.4}, {:.4}, {:.4}) target_status={} viewer_offset=({:.6}, {:.6}, {:.6}) scale={:.4} t_stiff={:.4} r_stiff={:.4} align={} sweeps={} blocked={} failed={}",
        m.pivot_offset.x,
        m.pivot_offset.y,
        m.pivot_offset.z,
        m.direction.x,
        m.direction.y,
        m.direction.z,
        m.target_status,
        m.viewer_offset.x,
        m.viewer_offset.y,
        m.viewer_offset.z,
        m.scale,
        m.t_stiffness,
        m.r_stiffness,
        m.align_camera_to_slope,
        cam.stats.sweeps,
        cam.stats.sweeps_blocked,
        cam.stats.sweeps_failed,
    );
    let s = cam.sought;
    tracing::debug!(
        target: "dereth::trace::camera",
        "camera-trace sought cell=0x{:08X} origin=({:.6}, {:.6}, {:.6}) q=({:.6}, {:.6}, {:.6}, {:.6})",
        s.cell.0,
        s.frame.origin.x,
        s.frame.origin.y,
        s.frame.origin.z,
        s.frame.rotation.w,
        s.frame.rotation.x,
        s.frame.rotation.y,
        s.frame.rotation.z,
    );
    tracing::debug!(
        target: "dereth::trace::camera",
        "camera-trace render origin=({:.6}, {:.6}, {:.6}) yaw={:.4} deg pitch={:.4} deg",
        f.origin.x,
        f.origin.y,
        f.origin.z,
        fc.yaw.to_degrees(),
        fc.pitch.to_degrees(),
    );
    // The projection half, as `WorldScene::view_params` builds it. A `0x0` viewport here means
    // `game_viewport` is `None` (the whole back buffer), in which case the aspect and FOV printed
    // are the 4:3 defaults rather than the frame's, and the window size is the answer instead.
    let vp = scene.effective_viewport(0, 0);
    let view = scene.view_params(0, 0);
    tracing::debug!(
        target: "dereth::trace::camera",
        "camera-trace view viewport=({}, {}, {}, {}) aspect={:.6} fov_y={:.7} rad ({:.4} deg) znear={} zfar={}",
        vp.x,
        vp.y,
        vp.width,
        vp.height,
        view.aspect,
        view.fov_y_rad,
        view.fov_y_rad.to_degrees(),
        view.znear,
        view.zfar,
    );
}

/// Update camera target tracking from combat state.
///
/// This is the only caller of the camera's target-tracking setter, which is the only writer of
/// the targeting flag. Without it the camera target-selection function's first branch —
/// `cm.set_target(LOOK_AT_PIVOT | LOOK_AT_OBJECT)` — is unreachable, and with
/// `ViewCombatTarget` on and a monster selected the camera falls through to `ALIGN_WITH_PLANE`
/// instead of framing it.
///
/// Five gates control tracking: the camera tracks the attack
/// target only when there is a `CameraState`, target tracking is on, the combat mode is
/// MELEE or MISSILE, the attack target id is non-zero and that target is attackable;
/// every other path clears the tracked target with id 0.
///
/// **`TrackTarget(0)` is not a no-op**, which is why all four refusals must reach it rather than
/// return: it clears `targeting` and re-runs the offset-target setter, so letting a target go restores
/// the ordinary chase camera. The early return at the top is the only path that does nothing, and
/// it is "there is no camera at all" — here, no body.
///
/// `MELEE == 2` and `MISSILE == 4` are the two values retail compares against, not a guess at the enum.
pub fn update_target_tracking(
    scene: &mut dyn crate::present::SceneMut,
    game: &dereth_client_model::World,
    tracking_target: bool,
) {
    // No camera set: no body, no `CameraState`, and retail returns without
    // touching anything. It does **not** fall through to `TrackTarget(0)`.
    let Some(c) = scene.world_mut().character.as_mut() else {
        return;
    };
    let in_combat = matches!(
        game.combat.combat_mode,
        dereth_client_model::combat::CombatMode::Melee
            | dereth_client_model::combat::CombatMode::Missile
    );
    let id = if tracking_target && in_combat {
        // The attack-target lookup then the attackable test, in that order: the
        // second is only asked about a non-zero id, which matters because the attackable test
        // answers **true** for id 0.
        game.get_attack_target()
            .filter(|t| game.object_is_attackable(*t))
            .unwrap_or(ObjectId(0))
    } else {
        ObjectId(0)
    };
    let CameraControl { set, manager, .. } = &mut c.camera;
    set.track_target(manager, id);
}

#[cfg(test)]
mod frame_rate_tests {
    use super::FrameRate;

    /// Oracle: a 20-entry ring of frame durations,
    /// frames per second `= 20 / Σ`, 0 when `Σ <= 0.0002`.
    #[test]
    fn the_frame_rate_meter_is_the_clients_twenty_frame_window() {
        let mut r = FrameRate::default();
        assert_eq!(
            r.fps(),
            0.0,
            "0 until the first end-of-frame processing, exactly as the client's is"
        );
        for _ in 0..FrameRate::WINDOW {
            r.push(1.0 / 30.0);
        }
        assert!((r.fps() - 30.0).abs() < 1e-3, "{}", r.fps());
        // The ring starts zeroed and the divisor is the whole sum: one frame in, the client
        // reports twenty times the rate. Reproduced, not primed.
        let mut r = FrameRate::default();
        r.push(1.0 / 30.0);
        assert!((r.fps() - 600.0).abs() < 1e-1, "{}", r.fps());
    }
}

/// The viewer sphere is `{ center = (0,0,0), radius = 0.3 }`.
/// The radius is [`dereth_physics::globals::VIEWER_SPHERE_RADIUS`].
#[must_use]
pub fn viewer_sphere() -> dereth_physics::Sphere {
    dereth_physics::Sphere::new(Vec3::ZERO, dereth_physics::globals::VIEWER_SPHERE_RADIUS)
}

/// The whole camera: the model, the modes, the input half and `WorldObjects`'s three camera fields.
///
/// One frame is [`Self::update`]: first sweep the sought position into the placed viewer, then
/// update held keys and body translucency. A physics tick also runs the smoother before that
/// frame's sweep. This ordering gives the camera its documented one-frame lag.
///
/// **One half of that sentence needs care.** Gameplay UI simulation reaches
/// the held keys, the zoom and the body translucency — and
/// that much is per display frame. It does **not** reach the smoother.
/// The smoother is called only inside the player-physics callback, reached from the physics
/// tick's player arm.
/// The smoother is on the **body's**
/// 30 Hz clock, so between physics ticks the camera does not move and neither does the drawn body
/// (part drawing copies `pos` into `draw_pos` with no time term at all).
#[derive(Debug)]
pub struct CameraControl {
    pub manager: CameraManager,
    pub set: CameraState,
    /// The mouse-look input half: the filter, the six-frame dead zone and the
    /// two extent counters.
    pub mouse_look: crate::actions::camera::MouseLook,
    /// The four `Input.*` preferences the mouse-look input half registers.
    pub prefs: crate::actions::camera::MouseLookPreferences,
    /// The sought viewer position: what the smoother wants before the sweep.
    pub sought: Position,
    /// The placed viewer position: where the camera actually is.
    pub viewer: Position,
    /// The placed viewer cell. `None` makes normal-mode rendering skip the 3D world for the frame.
    pub viewer_cell: Option<CellId>,
    /// The frame rate, which seeds the three rate limiters.
    pub frame_rate: FrameRate,
    /// The body-translucency global — how transparent the body is because the camera has closed on it.
    pub player_translucency: f32,
    /// How many sweeps ran, how many the sweep moved (the camera was pulled in), and how many
    /// failed outright and fell back.
    pub stats: CameraStats,
    /// False until the first [`Self::attach`], which installs the viewer position with reset set.
    attached: bool,
    /// Last frame's key latch, so a press and a release each reach `on_action` once.
    applied_input: CameraInput,
    /// The player's heading in degrees, refreshed once a frame from the pivot the
    /// sweep is about to start from. `None` means no player exists.
    ///
    /// Retail reads it *inside*, off a pointer that is always live; here the
    /// camera does not hold the body, so it is copied at the one place that does — the top of
    /// [`Self::update`], immediately after `pivot_state`. `update_held_keys` (the frame-by-frame
    /// repeat, which is what a held turn key actually runs on) therefore sees **this** frame's
    /// heading; only the press edge that arrives through [`Self::apply_input`] sees the previous
    /// frame's, and it is followed by a repeat on the very next frame.
    player_heading: Option<f32>,
}

/// Counters for the log line and for the tests. Rebuild-only.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CameraStats {
    /// Swept-position validation calls made for the camera.
    pub sweeps: u64,
    /// Sweeps whose result differs from the sought position by more than the sphere radius —
    /// the camera was pulled in by geometry.
    pub sweeps_blocked: u64,
    /// Sweeps that failed and fell back to `AdjustPosition`.
    pub sweeps_failed: u64,
    /// Frames on which even the fallback failed and the world was not drawn.
    pub frames_without_viewer: u64,
}

impl CameraControl {
    /// Build the camera for `player`, including its manager and
    /// [`CameraState`] construction, which ends by applying the default offsets.
    #[must_use]
    pub fn new(player: ObjectId) -> Self {
        let mut manager = CameraManager::new();
        // Camera construction runs before any frame has ended, so
        // the scene frame rate is 0 here in the client too; nothing in
        // default-offset setup reads it only when the camera is already in first person, which a
        // freshly constructed `CameraManager` (`viewer_offset = (0, -3, 0)`) is not.
        let set = CameraState::new(
            &mut manager,
            player,
            CameraTick {
                now: 0.0,
                fps: 0.0,
                mouse_turning: false,
                player_heading: None,
            },
        );
        Self {
            manager,
            set,
            mouse_look: crate::actions::camera::MouseLook::default(),
            prefs: crate::actions::camera::MouseLookPreferences::default(),
            sought: Position::new(CellId(0), Frame::new(Vec3::ZERO, Quat::IDENTITY)),
            viewer: Position::new(CellId(0), Frame::new(Vec3::ZERO, Quat::IDENTITY)),
            viewer_cell: None,
            frame_rate: FrameRate::default(),
            player_translucency: 0.0,
            stats: CameraStats::default(),
            attached: false,
            applied_input: CameraInput::default(),
            player_heading: None,
        }
    }

    /// Supply the player's heading for a caller that has the body and the camera does not.
    pub fn set_player_heading(&mut self, heading: Option<f32>) {
        self.player_heading = heading;
    }

    /// What [`Self::set_player_heading`] last stored.
    #[must_use]
    pub const fn player_heading(&self) -> Option<f32> {
        self.player_heading
    }

    /// The profile's three `Camera.*` values, applied the way retail's construction order
    /// applies them: the registry writes the three fields during camera-manager construction,
    /// then hands the `CameraState` to the UI setup path. Its last act applies
    /// configured camera stiffness to the current state, so the stiffness a player runs at is
    /// the profile's, not the `0.45` copied out of `t_stiffness`.
    pub fn apply_preferences(&mut self, p: &CameraPreferences) {
        self.manager.align_camera_to_slope = p.align_to_slope;
        self.manager.camera_adjustment_speed = p.adjustment_speed;
        self.set_stiffness_preference(p.stiffness);
    }

    /// `Camera.Stiffness` written at run time: the registry stores the field, then the camera
    /// manager applies its configured stiffness to the current state.
    pub fn set_stiffness_preference(&mut self, v: f32) {
        self.manager.camera_stiffness = v;
        self.set.set_stiffness(&mut self.manager, v);
    }

    /// `Camera.AlignToSlope` written at run time: the registry stores the field and there is no
    /// callback; the camera update reads it on its next tick.
    pub fn set_align_to_slope_preference(&mut self, v: bool) {
        self.manager.align_camera_to_slope = v;
    }

    /// `Camera.AdjustmentSpeed` written at run time: the field alone, as above.
    pub fn set_adjustment_speed_preference(&mut self, v: f32) {
        self.manager.camera_adjustment_speed = v;
    }

    /// This frame's [`CameraTick`], including timing and input state.
    /// `Input.UseMouseTurning`.
    #[must_use]
    pub fn tick(&self, now: f64) -> CameraTick {
        CameraTick {
            now,
            fps: self.frame_rate.fps(),
            mouse_turning: self.prefs.use_mouse_turning,
            player_heading: self.player_heading,
        }
    }

    /// Mouse look has ended. With mouse turning on, the body stops the turn the mouse gave it
    /// (a held turning key keeps its own) and the server is told.
    pub fn mouse_look_ended(&mut self) {
        if self.prefs.use_mouse_turning {
            self.set.effects.turn = Some(CameraTurn::StopDrift);
            self.set.effects.send_movement_event = true;
        }
    }

    /// Drain what decided the *body* should do this frame — the
    /// `MovePlayer` / `TurnToHeading` / `StopDrift` call and the mouse-turning arm's own
    /// `SendMovementEvent`. Taken, not read, so one press cannot be issued twice.
    pub fn take_turn(&mut self) -> (Option<CameraTurn>, bool) {
        let turn = self.set.effects.turn.take();
        let send = std::mem::take(&mut self.set.effects.send_movement_event);
        (turn, send)
    }

    /// Set the viewer position. With `reset` the sought position is snapped
    /// to the same place, which is what stops the smoother chasing a teleport across the world.
    pub fn set_viewer(&mut self, pos: Position, reset: bool) {
        self.viewer = pos;
        if reset {
            self.sought = pos;
        }
        // The client clears `viewer_cell` here and every caller writes it immediately after.
        self.viewer_cell = None;
    }

    /// Snap the camera onto the body, used after
    /// a teleport and on the first frame the body exists.
    pub fn attach(&mut self, pivot: &PivotState) {
        let p = self.manager.query_pivot_position(pivot);
        self.set_viewer(p, true);
        self.attached = true;
    }

    /// Whether [`Self::attach`] has ever run.
    #[must_use]
    pub fn attached(&self) -> bool {
        self.attached
    }

    /// One mouse-look delta `(dx, dy)`, in raw device units.
    ///
    /// Everything above the `Rotate`/`Raise`/`Lower` calls belongs to `crate::actions::camera`
    /// and is called, not copied:
    /// the mouse-input filter's 0.25 s one-pole smoother, the `sensitivity / 15` scale, the invert that
    /// negates **both** axes and the six-frame per-axis dead zone.
    pub fn mouse_look(&mut self, dx: i32, dy: i32, now: LocalTime) {
        let in_head = CameraState::in_head(&self.manager);
        let r = self.mouse_look.handle(dx, dy, &self.prefs, in_head, now);
        let t = self.tick(now.0);
        // The idle tick with mouse turning on: the mouse has stopped, so the body stops turning.
        if r.stop_drift {
            self.set.effects.turn = Some(CameraTurn::StopDrift);
            self.set.effects.send_movement_event |= r.send_movement_event;
        }
        if let Some((left, extent)) = r.rotate {
            self.set
                .rotate(&mut self.manager, left, false, extent, false, t);
        }
        if let Some((dir, extent)) = r.pitch {
            match dir {
                crate::actions::camera::PitchDirection::Lower => {
                    self.set.lower(&mut self.manager, false, extent, t);
                }
                crate::actions::camera::PitchDirection::Raise => {
                    self.set.raise(&mut self.manager, false, extent, t);
                }
            }
        }
    }

    /// Dispatch one camera command produced by the `crate::actions::camera` event decoder.
    pub fn on_action(
        &mut self,
        cmd: crate::actions::camera::CameraCommand,
        player: ObjectId,
        now: f64,
    ) {
        use crate::actions::camera::CameraCommand as C;
        let t = self.tick(now);
        let cm = &mut self.manager;
        match cmd {
            C::Closer { extent } => self.set.closer(cm, true, extent, t),
            C::StopCloser => self.set.closer(cm, false, 1.0, t),
            C::Farther { extent } => self.set.farther(cm, true, extent, player, t),
            C::StopFarther => self.set.farther(cm, false, 1.0, player, t),
            C::Rotate { left, extent } => self.set.rotate(cm, left, true, extent, true, t),
            C::StopRotating { left } => self.set.rotate(cm, left, false, 1.0, true, t),
            C::Raise { extent } => self.set.raise(cm, true, extent, t),
            C::StopRaising => self.set.raise(cm, false, 1.0, t),
            C::Lower { extent } => self.set.lower(cm, true, extent, t),
            C::StopLowering => self.set.lower(cm, false, 1.0, t),
            C::SetDefaultOffsets => self.set.set_default_offsets(cm, player, t),
            C::SetInHead => self.set.set_in_head(cm),
            C::ToggleLookDown => {
                let on = !self.set.looking_down;
                self.set.look_down(cm, on);
            }
            C::ToggleMapMode => {
                let on = !self.set.in_map_mode;
                self.set.set_map_mode(cm, on);
            }
            // `ToggleMouseLook` resets both extent counters on the way in; that is
            // `MouseLook::toggle`, and action 0x3E falls through to the same behaviour.
            C::ToggleMouseLook(on) | C::AlternateMode { on } => {
                if self.mouse_look.toggle(on) && !on {
                    self.mouse_look_ended();
                }
                self.set.mouselook_active = self.mouse_look.active;
            }
            C::NotHandled => {}
        }
    }

    /// Turn the debug camera's key latch into events.
    ///
    /// The four look keys are the camera action set's `Rotate Camera Left/Right/Up/Down`
    /// (`InputAction` 0x35/0x36/0x37/0x38); a press issues the hold form and a release the `Stop`
    /// form, exactly as the input manager's toggle type 1 does. The local
    /// `rot_left`/`raise`/`lower` flags then repeat it during every camera update.
    ///
    /// This is a latch, not the input seam. `crate::actions::camera::on_action` decodes a
    /// real `InputEvent` into the same [`crate::actions::camera::CameraCommand`]s and is what
    /// [`Self::on_action`] takes; when the binary routes its keyboard through that decoder this
    /// function goes away and nothing below it changes.
    pub fn apply_input(&mut self, input: CameraInput, player: ObjectId, now: f64) {
        use crate::actions::camera::CameraCommand as C;
        if input == self.applied_input {
            return;
        }
        let edge = |was: bool, is: bool| (was != is).then_some(is);
        let rotate = |left: bool, on: bool| {
            if on {
                C::Rotate { left, extent: 1.0 }
            } else {
                C::StopRotating { left }
            }
        };
        if let Some(on) = edge(self.applied_input.look_left, input.look_left) {
            self.on_action(rotate(true, on), player, now);
        }
        if let Some(on) = edge(self.applied_input.look_right, input.look_right) {
            self.on_action(rotate(false, on), player, now);
        }
        if let Some(on) = edge(self.applied_input.look_up, input.look_up) {
            let c = if on {
                C::Raise { extent: 1.0 }
            } else {
                C::StopRaising
            };
            self.on_action(c, player, now);
        }
        if let Some(on) = edge(self.applied_input.look_down, input.look_down) {
            let c = if on {
                C::Lower { extent: 1.0 }
            } else {
                C::StopLowering
            };
            self.on_action(c, player, now);
        }
        self.applied_input = input;
    }

    /// One frame, in the client's own order.
    ///
    /// 1. Sweep the camera sphere, consuming the sought position
    ///    the previous frame left behind, and writing `viewer` / `viewer_cell`.
    /// 2. Update held keys and body translucency.
    /// 3. On a player-physics update, run the smoother, producing the next frame's
    ///    sought position from the placed `viewer`. **Only when `player_physics_updated`**: in
    ///    the client this one is not a frame step at all, it is
    ///    the player-physics-update callback, called from inside physics simulation.
    ///
    /// `input` is the debug camera's key latch, turned into events by
    /// [`Self::apply_input`] before any of the three; `dt` is this frame's
    /// delta, for the scene's frames-per-second value.
    // LINT-OK: `player_physics_updated` is the eighth argument and is the client seam.
    // This is a callback precisely because the camera's step is the body's step. Folding the eight
    // arguments into a struct would hide which one carries that tick.
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        world: &mut dereth_physics::PhysicsWorld,
        player_handle: dereth_physics::PhysHandle,
        player_id: ObjectId,
        input: CameraInput,
        now: f64,
        dt: f64,
        player_physics_updated: bool,
    ) {
        self.frame_rate.push(dt);
        // **Gameplay applies the 1.1 scale every frame, and it is the camera's pitch.**
        //
        // Client simulation runs UI-element simulation before the camera update and applies
        // scale `1.1` whenever a camera exists. So by the time the smoother runs, on every frame
        // in the world, the viewer offset is `(0, -2.75, 0.825)` and its target mask carries
        // `LOOK_AT_PIVOT` — see the scale derivation above. Without this line the offset stays at
        // the unscaled default `(0, -2.5, 0.75)`, `0.75 > 0.75` is false, the mask has
        // no `LOOK_AT_PIVOT`, and the drawn camera is dead level where retail's is pitched down
        // 16.7 degrees.
        //
        // The camera-state lookup's null guard means "there is a camera at all", which here is having
        // reached `CameraControl::update`.
        self.set.set_scale(&mut self.manager, GAMEPLAY_CAMERA_SCALE);
        self.apply_input(input, player_id, now);
        let Some(pivot) = pivot_state(world, player_handle) else {
            return;
        };
        // Camera rotation reads the player object's heading. It is copied here
        // because this is the one place the camera has the body. The heading is degrees,
        // 0 = north, increasing clockwise.
        self.player_heading = Some(pmath::get_heading(&pivot.position.frame));
        let first = !self.attached;
        if first {
            self.attach(&pivot);
        }
        let t = self.tick(now);
        self.set.update_held_keys(&mut self.manager, player_id, t);
        // **Step 3 is the body's, not the frame's.**
        //
        // The camera update is referenced once inside the player-physics callback. That callback
        // is reached only from the physics tick's player arm — the object just stepped is the
        // player. The smoother's own `dt` is `current_time - last_update_time`, so it measures the
        // tick's elapsed time and stays correct whatever the display rate is.
        //
        // Running it every display frame instead would give the body a 21-29 Hz clock and the
        // camera a 60-165 Hz one, and their difference shows as a shimmer of several centimetres
        // on alternate frames. The client has no such difference: between ticks part drawing
        // re-copies an unchanged `pos` into `draw_pos` and the sweep re-runs from an
        // unchanged pivot toward an unchanged sought position, so the frame is a pure repeat.
        //
        // The attach frame is included because `attach` has just snapped `sought` onto the pivot
        // and the client's first frame reaches the player-physics callback as well —
        // resetting the viewer position and the first tick are the same moment there.
        if player_physics_updated || first {
            let current = self.viewer;
            self.sought = self.manager.update_camera(current, &pivot, None, now);
        }
        // The sweep is last because client simulation reaches the player-physics callback, then
        // gameplay UI simulation reaches the camera smoother, and drawing opens with
        // `update_viewer`. So the sought position a tick produces is swept **in that same
        // frame**, not the next one; with the sweep above the smoother the camera would move
        // one frame *after* every tick and the shimmer would remain, only shifted.
        self.update_viewer(world, player_handle, &pivot);
        self.update_translucency(&pivot);
    }

    /// Update body translucency: the body fades out as the camera closes on it and is fully
    /// opaque in first person.
    ///
    /// `1 - (0.2 - d) / -0.24999999` is the shipped expression, clamped to `0…1`, applied only
    /// below 0.45 m. Above that it snaps to 0 in one step.
    fn update_translucency(&mut self, pivot: &PivotState) {
        if CameraState::in_head(&self.manager) {
            self.player_translucency = 1.0;
            return;
        }
        let p = self.manager.query_pivot_position(pivot);
        let d = pmath::distance(&p, &self.viewer);
        if d < 0.45 {
            self.player_translucency = (1.0 - (0.2 - d) / -0.249_999_99).clamp(0.0, 1.0);
        } else {
            self.player_translucency = 0.0;
        }
    }

    /// Smart-box viewer update uses the swept sphere.
    ///
    /// The 0.3-radius `viewer_sphere` slides from the pivot point toward the desired eye position
    /// through the physics BSPs of the cells in between, stopping at the first blocking surface.
    /// **That is the whole of the camera's wall behaviour**: it is the same collision code the
    /// player uses, not a raycast and not a pull-in heuristic.
    fn update_viewer(
        &mut self,
        world: &mut dereth_physics::PhysicsWorld,
        player_handle: dereth_physics::PhysHandle,
        pivot: &PivotState,
    ) {
        let Some(player_cell) = world.get(player_handle).and_then(|o| o.cell) else {
            // `reenter_visibility` belongs to the physics object, and this build never leaves the cell graph;
            // the client's second test failing lands here.
            let pos = pivot.position;
            self.set_viewer(pos, true);
            self.viewer_cell = None;
            self.stats.frames_without_viewer += 1;
            return;
        };

        // The pivot *frame*: the player's own rotation, with `pivot_offset` rotated into it. This
        // is not `query_pivot_position`, whose rotation is the identity.
        let src = match (self.manager.pivot_part_index, pivot.part_position) {
            (i, Some(p)) if i != -1 => p.frame,
            _ => pivot.position.frame,
        };
        let mut pivot_frame = src;
        pivot_frame.origin = pmath::localtoglobal(&src, self.manager.pivot_offset);

        // Outdoors the player's own cell is the start; indoors `AdjustPosition` picks the interior
        // cell the pivot point falls in, because the pivot is 1.5 m above the feet and may be in a
        // different room.
        let start_cell = if dereth_physics::landdefs::is_outdoors(pivot.position.cell) {
            player_cell
        } else {
            let probe = Position::new(pivot.position.cell, pivot_frame);
            world
                .adjust_position(&probe, Vec3::ZERO)
                .and_then(|(_, c)| c)
                .unwrap_or(player_cell)
        };

        let from = Position::new(start_cell, pivot_frame);
        // The sought origin re-expressed in `from`'s cell space; the cell id on `sought` itself is
        // left alone, because the `AdjustPosition` fallback below is handed exactly that.
        let mut sought = self.sought;
        sought.frame.origin = pmath::pos_localtoglobal(&from, &sought, Vec3::ZERO);
        let to = Position::new(start_cell, sought.frame);

        self.stats.sweeps += 1;
        if let Some(t) = world.sweep_sphere(
            player_handle,
            dereth_physics::globals::VIEWER_OBJECT_INFO_STATE,
            viewer_sphere(),
            start_cell,
            &from,
            &to,
        ) {
            let pos = t.sphere_path.curr_pos;
            if pmath::distance(&pos, &to) > dereth_physics::globals::VIEWER_SPHERE_RADIUS {
                self.stats.sweeps_blocked += 1;
            }
            self.set_viewer(pos, false);
            self.viewer_cell = t.sphere_path.curr_cell;
            return;
        }
        self.stats.sweeps_failed += 1;
        if let Some((pos, cell)) = world.adjust_position(&sought, Vec3::ZERO) {
            self.set_viewer(pos, false);
            self.viewer_cell = cell;
            return;
        }
        // No valid camera position at all: the client leaves `viewer_cell` NULL and
        // the world render pass skips the 3D world for the frame.
        let pos = pivot.position;
        self.set_viewer(pos, true);
        self.viewer_cell = None;
        self.stats.frames_without_viewer += 1;
    }
}

/// Snapshot the pivot object out of the physics world.
#[must_use]
pub fn pivot_state(
    world: &dereth_physics::PhysicsWorld,
    h: dereth_physics::PhysHandle,
) -> Option<PivotState> {
    let o = world.get(h)?;
    Some(PivotState {
        position: o.position,
        // `pivot_part_index` is -1 for the shipped camera, so the part arm is never taken; a
        // caller that sets it must supply the part position itself.
        part_position: None,
        velocity: o.velocity(),
        in_contact: o.transient_state.in_contact(),
        contact_normal: o.contact_plane.normal,
    })
}

impl FreeCamera {
    /// Read a [`FreeCamera`] back out of a `Frame` the real camera produced.
    ///
    /// This is exact, not an approximation: the camera's aim frame builds
    /// `euler_set_rotate(pitch, 0, yaw)`, i.e. `Rz(yaw) · Rx(pitch)` with **no roll**, and
    /// `FreeCamera::frame` builds `q_z(yaw) · q_x(pitch)`, which is the same rotation. So the
    /// two representations carry the same information and the round trip loses only float
    /// rounding — checked by the test at the bottom of this file rather than argued.
    ///
    /// The pitch is deliberately **not** clamped to [`PITCH_LIMIT`]: map mode puts the eye
    /// 450 units above the pivot looking straight down, and clamping
    /// would quietly refuse the client's own overhead modes.
    #[must_use]
    pub fn from_frame(f: &Frame) -> Self {
        let fwd = pmath::get_vector_heading(f);
        Self {
            position: f.origin,
            // The frame heading is degrees, 0 = north, increasing clockwise; this
            // camera's yaw is radians counter-clockwise from north, so it negates.
            yaw: -pmath::get_heading(f) * DEG_TO_RAD,
            pitch: math::asinf(fwd.z.clamp(-1.0, 1.0)),
        }
    }
}

/// Consume `UiRequest::SetPreference` for the three `Camera.*` names. Applying checkbox and
/// slider options stores the field and runs the
/// registration's change callback (the stiffness callback for `Camera.Stiffness`, none for the
/// other two). Requests with other names, or the wrong value kind, are handed back in order,
/// which is the shape `render_prefs::apply_preference_requests` set.
///
/// Without a body there is no `CameraManager` to write, exactly as retail's callback returns
/// when the current camera manager is absent; the request is still consumed, because
/// the profile write it also stands for has already happened.
pub fn apply_preference_requests(
    body: Option<&mut crate::character::Character>,
    requests: Vec<dereth_client_contract::UiRequest>,
) -> Vec<dereth_client_contract::UiRequest> {
    use crate::actions::camera::MouseLookPreferences as MouseLookPrefs;
    use dereth_client_contract::{PrefValue, UiRequest};
    let mut camera = body.map(|c| &mut c.camera);
    requests
        .into_iter()
        .filter(|r| {
            let UiRequest::SetPreference(name, value) = r else {
                return true;
            };
            let owned = match value {
                PrefValue::Bool(v)
                    if name.eq_ignore_ascii_case(CameraPreferences::ALIGN_TO_SLOPE) =>
                {
                    if let Some(c) = camera.as_deref_mut() {
                        c.set_align_to_slope_preference(*v);
                    }
                    true
                }
                PrefValue::Float(v) if name.eq_ignore_ascii_case(CameraPreferences::STIFFNESS) => {
                    if let Some(c) = camera.as_deref_mut() {
                        c.set_stiffness_preference(*v);
                    }
                    true
                }
                PrefValue::Float(v)
                    if name.eq_ignore_ascii_case(CameraPreferences::ADJUSTMENT_SPEED) =>
                {
                    if let Some(c) = camera.as_deref_mut() {
                        c.set_adjustment_speed_preference(*v);
                    }
                    true
                }
                // The four `Input.*` registrations consumed by mouse-look handling
                // and camera rotation. They have the same shape as the three above: store the
                // field, with no change callback in input-manager preference registration.
                PrefValue::Float(v) if name.eq_ignore_ascii_case(MouseLookPrefs::SENSITIVITY) => {
                    if let Some(c) = camera.as_deref_mut() {
                        c.prefs.sensitivity = *v;
                    }
                    true
                }
                PrefValue::Float(v) if name.eq_ignore_ascii_case(MouseLookPrefs::SMOOTHING) => {
                    if let Some(c) = camera.as_deref_mut() {
                        c.prefs.smoothing = *v;
                    }
                    true
                }
                PrefValue::Bool(v) if name.eq_ignore_ascii_case(MouseLookPrefs::INVERT_Y) => {
                    if let Some(c) = camera.as_deref_mut() {
                        c.prefs.invert_y = *v;
                    }
                    true
                }
                PrefValue::Bool(v)
                    if name.eq_ignore_ascii_case(MouseLookPrefs::USE_MOUSE_TURNING) =>
                {
                    if let Some(c) = camera.as_deref_mut() {
                        c.prefs.use_mouse_turning = *v;
                    }
                    true
                }
                _ => false,
            };
            !owned
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Oracle: the camera's own contract, checked by evaluating it rather than by restating it.
    // Yaw is counter-clockwise from above, so a quarter turn from north
    // faces west.
    #[test]
    fn a_quarter_turn_of_yaw_faces_west_and_the_pitch_clamp_holds() {
        let cam = FreeCamera::new(Vec3::ZERO, std::f32::consts::FRAC_PI_2, 0.0);
        let f = cam.forward();
        assert!((f.x + 1.0).abs() < 1e-6 && f.y.abs() < 1e-6, "{f:?}");
        let mut cam = FreeCamera::default();
        cam.look(0.0, -10_000.0);
        assert!((cam.pitch - PITCH_LIMIT).abs() < 1e-6, "{}", cam.pitch);
        cam.look(0.0, 20_000.0);
        assert!((cam.pitch + PITCH_LIMIT).abs() < 1e-6, "{}", cam.pitch);
    }

    // Oracle: as above. Holding forward for one second at the documented speed moves exactly that
    // far along the camera's forward vector, and Shift multiplies it.
    #[test]
    fn movement_is_the_documented_speed_along_the_forward_vector() {
        let mut cam = FreeCamera::new(Vec3::ZERO, 0.0, 0.0);
        cam.update(
            CameraInput {
                forward: true,
                ..CameraInput::default()
            },
            1.0,
        );
        assert!(
            (cam.position.y - MOVE_SPEED).abs() < 1e-4,
            "{:?}",
            cam.position
        );
        let mut cam = FreeCamera::new(Vec3::ZERO, 0.0, 0.0);
        cam.update(
            CameraInput {
                forward: true,
                fast: true,
                ..CameraInput::default()
            },
            1.0,
        );
        assert!(
            (cam.position.y - MOVE_SPEED * FAST_MULTIPLIER).abs() < 1e-3,
            "{:?}",
            cam.position
        );
        // Up is world up, not camera up, so a pitched camera still rises vertically.
        let mut cam = FreeCamera::new(Vec3::ZERO, 0.3, 0.7);
        cam.update(
            CameraInput {
                up: true,
                ..CameraInput::default()
            },
            2.0,
        );
        assert!(
            (cam.position.z - 2.0 * MOVE_SPEED).abs() < 1e-3,
            "{:?}",
            cam.position
        );
        assert!(cam.position.x.abs() < 1e-6 && cam.position.y.abs() < 1e-6);
    }

    use dereth_primitives::{CellId, ObjectId, Position};

    const PLAYER: ObjectId = ObjectId(0x5000_0001);

    fn pivot_at(origin: Vec3) -> PivotState {
        PivotState {
            position: Position::new(CellId(0xA9B4_0001), Frame::new(origin, Quat::IDENTITY)),
            part_position: None,
            velocity: Vec3::ZERO,
            in_contact: true,
            contact_normal: Vec3::new(0.0, 0.0, 1.0),
        }
    }

    fn set_up() -> (CameraManager, CameraState) {
        let mut cm = CameraManager::new();
        let cs = CameraState::new(&mut cm, PLAYER, tick(0.0));
        (cm, cs)
    }

    fn tick(now: f64) -> CameraTick {
        CameraTick {
            now,
            fps: 30.0,
            mouse_turning: false,
            player_heading: Some(0.0),
        }
    }

    // Oracle: the camera-manager constructor defaults.
    #[test]
    fn the_camera_manager_constructs_with_the_documented_defaults() {
        let cm = CameraManager::new();
        assert_eq!(cm.t_stiffness, 0.45);
        assert_eq!(cm.r_stiffness, 0.45);
        assert_eq!(cm.pivot_part_index, -1);
        assert_eq!(cm.target_part_index, -1);
        assert_eq!(cm.direction, Vec3::new(0.0, 1.0, 0.0));
        assert_eq!(cm.target_status, target::LOOK_IN_DIRECTION);
        assert_eq!(cm.viewer_offset, Vec3::new(0.0, -3.0, 0.0));
        assert_eq!(cm.camera_adjustment_speed, 40.0);
        assert_eq!(cm.scale, 1.0);
        assert!(cm.align_camera_to_slope);
        assert!(cm.enabled);
    }

    // Oracle: default offsets put the eye 2.5 behind and 0.75 above a pivot 1.5
    // above the player's origin — and `set_target_for_offset`, which for that offset
    // takes the ALIGN_WITH_PLANE arm and adds LOOK_IN_DIRECTION.
    #[test]
    fn set_default_offsets_is_the_shipped_third_person_camera() {
        let (cm, _cs) = set_up();
        assert_eq!(cm.pivot_object_id, PLAYER);
        assert_eq!(cm.pivot_offset, Vec3::new(0.0, 0.0, 1.5));
        assert_eq!(cm.viewer_offset, Vec3::new(0.0, -2.5, 0.75));
        assert_eq!(cm.direction, Vec3::new(0.0, 1.0, 0.0));
        assert_eq!(
            cm.target_status,
            target::ALIGN_WITH_PLANE | target::LOOK_IN_DIRECTION,
            "x in [-1,1], y <= 0, y <= -0.5, z in [-0.5, 0.75]"
        );
    }

    /// The gameplay ui s scale is what puts look at pivot on the shipped camera.
    #[test]
    fn the_gameplay_ui_s_scale_is_what_puts_look_at_pivot_on_the_shipped_camera() {
        let (mut cm, mut cs) = set_up();
        assert_eq!(
            cm.target_status & target::LOOK_AT_PIVOT,
            0,
            "the unscaled offset must NOT aim at the pivot, or this proves nothing"
        );
        cs.set_scale(&mut cm, GAMEPLAY_CAMERA_SCALE);
        assert_eq!(cm.scale, GAMEPLAY_CAMERA_SCALE);
        assert_eq!(cm.viewer_offset, Vec3::new(0.0, -2.5 * 1.1, 0.75 * 1.1));
        assert_eq!(
            cm.target_status,
            target::ALIGN_WITH_PLANE | target::LOOK_IN_DIRECTION | target::LOOK_AT_PIVOT,
            "z = 0.825 > 0.75, so the mask falls through to LOOK_AT_PIVOT"
        );
        // The stiffness it forced to 1.0 across the change is restored from `current_stiffness`.
        assert_eq!(cm.t_stiffness, 0.45);
        assert_eq!(cm.r_stiffness, 0.45);
    }

    // Oracle: the same function's two early-outs — an unchanged value (`old == new`) and the
    // `InHead()` test above it — which is what lets the caller invoke it every single
    // frame.
    #[test]
    fn set_scale_is_idempotent_and_refuses_in_first_person() {
        let (mut cm, mut cs) = set_up();
        cs.set_scale(&mut cm, GAMEPLAY_CAMERA_SCALE);
        let once = cm.clone();
        for _ in 0..10 {
            cs.set_scale(&mut cm, GAMEPLAY_CAMERA_SCALE);
        }
        assert_eq!(cm.viewer_offset, once.viewer_offset, "the scale compounded");

        // First person: scaling returns before touching anything, so the magic offset other
        // code tests for **exactly** survives it.
        cs.set_in_head(&mut cm);
        cs.set_scale(&mut cm, 2.0);
        assert_eq!(cm.viewer_offset, IN_HEAD_OFFSET);
        assert_eq!(
            cm.scale, GAMEPLAY_CAMERA_SCALE,
            "the scale itself is not written either"
        );
    }

    // Oracle: the divide-then-multiply — `v * (1/old) * new`, not a fresh
    // applying default offsets. A camera the player has zoomed keeps its zoom across a scale change.
    #[test]
    fn set_scale_rescales_whatever_offset_the_camera_already_had() {
        let (mut cm, mut cs) = set_up();
        cs.set_scale(&mut cm, 2.0);
        assert_eq!(cm.viewer_offset, Vec3::new(0.0, -5.0, 1.5));
        cs.set_scale(&mut cm, 1.0);
        assert_eq!(
            cm.viewer_offset,
            Vec3::new(0.0, -2.5, 0.75),
            "the round trip is exact"
        );
        assert_eq!(
            cm.target_status & target::LOOK_AT_PIVOT,
            0,
            "back at 0.75 the mask loses LOOK_AT_PIVOT again"
        );
    }

    // Oracle: and `InHead`, which test for exactly
    // (0, 0.18, 0). First person has no positional lag: t_stiffness goes to 1.0.
    #[test]
    fn first_person_is_the_exact_magic_offset_with_no_translational_lag() {
        let (mut cm, mut cs) = set_up();
        cs.set_in_head(&mut cm);
        assert_eq!(cm.viewer_offset, IN_HEAD_OFFSET);
        assert!(CameraState::in_head(&cm));
        assert_eq!(cm.t_stiffness, 1.0);
        assert_eq!(cm.r_stiffness, 0.45, "only the translational half snaps");
        // In first person `set_target_for_offset` takes the (0,0.18,0) arm: ALIGN_WITH_PLANE then
        // LOOK_IN_DIRECTION, and never LOOK_AT_PIVOT.
        assert_eq!(
            cm.target_status,
            target::ALIGN_WITH_PLANE | target::LOOK_IN_DIRECTION
        );
    }

    // Oracle: the shipped zoom limits. Closer refuses below |offset| = 0.5; Farther refuses
    // |x| > 10, |y| > 10 or z > 450; and Farther out of first person jumps to (0, -0.6, 0.5).
    #[test]
    fn the_zoom_limits_are_the_shipped_values() {
        let (mut cm, mut cs) = set_up();
        // Closer, with an explicit rate so the step does not depend on the clock: rate != 1
        // makes the step `1/rate`, so rate 0.5 is a step of 2 and k = 1 - 0.4 = 0.6.
        cm.viewer_offset = Vec3::new(0.0, -0.8, 0.0);
        cs.closer(&mut cm, false, 0.5, tick(1.0));
        assert_eq!(
            cm.viewer_offset,
            Vec3::new(0.0, -0.8, 0.0),
            "0.48 < 0.5 is refused"
        );
        cm.viewer_offset = Vec3::new(0.0, -2.0, 0.0);
        cs.closer(&mut cm, false, 0.5, tick(2.0));
        assert!(
            (cm.viewer_offset.y + 1.2).abs() < 1e-5,
            "{:?}",
            cm.viewer_offset
        );

        // Farther, rate != 1 uses the rate itself: k = 1 + rate * 0.2.
        cm.viewer_offset = Vec3::new(0.0, -9.5, 0.0);
        cs.farther(&mut cm, false, 1.0e6, PLAYER, tick(3.0));
        assert_eq!(
            cm.viewer_offset,
            Vec3::new(0.0, -9.5, 0.0),
            "|y| > 10 is refused"
        );

        // Out of first person: the documented special case.
        cs.set_in_head(&mut cm);
        cs.farther(&mut cm, false, 1.0, PLAYER, tick(4.0));
        assert_eq!(cm.viewer_offset, Vec3::new(0.0, -0.6, 0.5));
        assert_eq!(cm.pivot_offset, Vec3::new(0.0, 0.0, 1.5));

        // And reaching first person stops zooming in entirely.
        cs.set_in_head(&mut cm);
        cs.closer(&mut cm, false, 0.5, tick(5.0));
        assert_eq!(cm.viewer_offset, IN_HEAD_OFFSET);
    }

    // Oracle: `Closer` / `Farther` — the stiffness is re-picked on every
    // accepted step, snapping to 1.0 when the eye is steeply above the pivot (z <= -1.8).
    #[test]
    fn a_steeply_overhead_offset_snaps_the_stiffness() {
        let (mut cm, mut cs) = set_up();
        cm.viewer_offset = Vec3::new(0.0, -1.0, -2.0);
        cs.farther(&mut cm, false, 1.0e-6, PLAYER, tick(1.0));
        assert_eq!(cm.t_stiffness, 1.0, "z <= -1.8 removes the lag");
        cm.viewer_offset = Vec3::new(0.0, -1.0, 0.5);
        cs.farther(&mut cm, false, 1.0e-6, PLAYER, tick(2.0));
        assert_eq!(cm.t_stiffness, cs.current_stiffness);
    }

    // Oracle: the 6-bit debug-nudge mask.
    #[test]
    fn flags_to_vector_matches_the_documented_table() {
        let cm = CameraManager::new();
        assert_eq!(cm.flags_to_vector(2), Vec3::new(1.0, 0.0, 0.0));
        assert_eq!(cm.flags_to_vector(1), Vec3::new(-1.0, 0.0, 0.0));
        assert_eq!(cm.flags_to_vector(4), Vec3::new(0.0, 1.0, 0.0));
        assert_eq!(cm.flags_to_vector(8), Vec3::new(0.0, -1.0, 0.0));
        assert_eq!(cm.flags_to_vector(0x10), Vec3::new(0.0, 0.0, 1.0));
        assert_eq!(cm.flags_to_vector(0x20), Vec3::new(0.0, 0.0, -1.0));
        // `flags & 3 == 3` is neither, and so is `flags & 0xC == 0xC`.
        assert_eq!(cm.flags_to_vector(3), Vec3::ZERO);
        assert_eq!(cm.flags_to_vector(0x3F), Vec3::ZERO);
    }

    // Oracle: the camera update's interpolation factor is
    // `stiffness * dt * 10` clamped to 1, and anything above 0.9998 snaps outright -- which is a
    // *different* flag from the clamp, because the early-out reads it.
    #[test]
    fn the_smoother_is_stiffness_times_dt_times_ten_clamped_to_one() {
        assert_eq!(
            approach(0.45, 0.1),
            (0.45, false),
            "45% of the gap per 100 ms"
        );
        assert_eq!(
            approach(0.45, 1.0),
            (1.0, false),
            "clamped, but not 'snapped'"
        );
        assert_eq!(approach(0.9999, 0.001), (1.0, true), "above 0.9998 snaps");
        assert_eq!(
            approach(0.9998, 1.0),
            (1.0, false),
            "the boundary is strictly greater"
        );
    }

    // Oracle: the same rule, evaluated rather than restated. A camera 10 m from its ideal
    // eye position, at the default 0.45 stiffness and a 100 ms frame, closes 45% of the gap.
    #[test]
    fn one_update_closes_the_documented_fraction_of_the_gap() {
        let mut cm = CameraManager::new();
        // A pure LOOK_IN_DIRECTION camera 10 units behind, so the ideal eye is a known point.
        cm.target_status = target::LOOK_IN_DIRECTION;
        cm.align_camera_to_slope = false;
        cm.viewer_offset = Vec3::new(0.0, -10.0, 0.0);
        cm.pivot_object_id = PLAYER;
        cm.last_update_time = 1.0;
        let pivot = pivot_at(Vec3::new(100.0, 100.0, 20.0));
        // Start the camera exactly on the pivot; the ideal eye is 10 m south of it.
        let origin = pivot.position.frame.origin;
        let current = Position::new(pivot.position.cell, Frame::new(origin, Quat::IDENTITY));
        let out = cm.update_camera(current, &pivot, None, 1.1);
        // 45% of the 10 m gap along -y.
        assert!(
            (out.frame.origin.y - (100.0 - 4.5)).abs() < 1e-3,
            "{:?}",
            out.frame.origin
        );
        assert!((out.frame.origin.x - 100.0).abs() < 1e-4);
    }

    // Oracle: the final camera-update early-out. A camera already at its
    // ideal position must be returned **identically**, not recomputed, or it jitters below a
    // pixel forever.
    #[test]
    fn a_settled_camera_is_returned_unchanged() {
        let mut cm = CameraManager::new();
        cm.target_status = target::LOOK_IN_DIRECTION;
        cm.align_camera_to_slope = false;
        cm.viewer_offset = Vec3::new(0.0, -2.5, 0.75);
        cm.pivot_object_id = PLAYER;
        let pivot = pivot_at(Vec3::new(100.0, 100.0, 20.0));
        let origin = pivot.position.frame.origin;
        let mut pos = Position::new(pivot.position.cell, Frame::new(origin, Quat::IDENTITY));
        let mut now = 0.0;
        for _ in 0..400 {
            now += 1.0 / 30.0;
            pos = cm.update_camera(pos, &pivot, None, now);
        }
        // It has settled; one more update returns the identical value.
        now += 1.0 / 30.0;
        let again = cm.update_camera(pos, &pivot, None, now);
        assert_eq!(
            again.frame.origin, pos.frame.origin,
            "the early-out did not fire"
        );
        assert_eq!(again.frame.rotation, pos.frame.rotation);
        // And it settled where the offset says: 2.5 south and 0.75 up from a pivot at z = 20.
        assert!(
            (pos.frame.origin.y - 97.5).abs() < 1e-2,
            "{:?}",
            pos.frame.origin
        );
        assert!(
            (pos.frame.origin.z - 20.75).abs() < 1e-2,
            "{:?}",
            pos.frame.origin
        );
    }

    // Oracle: the camera update first checks whether it is enabled and otherwise returns current.
    #[test]
    fn a_disabled_camera_returns_its_input() {
        let mut cm = CameraManager::new();
        cm.enabled = false;
        let pivot = pivot_at(Vec3::new(1.0, 2.0, 3.0));
        let cur = Position::new(
            CellId(7),
            Frame::new(Vec3::new(9.0, 9.0, 9.0), Quat::IDENTITY),
        );
        assert_eq!(cm.update_camera(cur, &pivot, None, 5.0), cur);
        assert_eq!(
            cm.last_update_time, 0.0,
            "it returns before touching the clock"
        );
    }

    // Oracle: the vector heading and pitch computations, which are
    // what `Raise` and `Lower` decompose the offset with. Checked by evaluating the round trip
    // rather than by restating the formulae.
    #[test]
    fn vector_heading_and_pitch_round_trip_through_the_swing() {
        for v in [
            Vec3::new(0.0, -2.5, 0.75),
            Vec3::new(1.5, -1.5, 0.0),
            Vec3::new(-3.0, -1.0, 2.0),
        ] {
            let mag = v.mag2().sqrt();
            let h = vector_heading(v) * DEG_TO_RAD;
            let p = vector_pitch(v) * DEG_TO_RAD;
            let back = Vec3::new(
                math::sinf(h) * mag * math::cosf(p),
                math::cosf(h) * mag * math::cosf(p),
                math::sinf(p) * mag,
            );
            assert!((back.x - v.x).abs() < 1e-4, "{v:?} -> {back:?}");
            assert!((back.y - v.y).abs() < 1e-4, "{v:?} -> {back:?}");
            assert!((back.z - v.z).abs() < 1e-4, "{v:?} -> {back:?}");
        }
        // A degenerate XY answers 0 rather than NaN.
        assert_eq!(vector_heading(Vec3::new(0.0, 0.0, 1.0)), 0.0);
        assert_eq!(vector_pitch(Vec3::ZERO), 0.0);
    }

    // Oracle: the camera swing preserves the
    // offset's length and moves it by `angle * step` radians, and `Lower` refuses to drop the eye
    // below the pivot inside a 1.2 m cylinder.
    #[test]
    fn raise_and_lower_swing_the_offset_without_changing_its_length() {
        let (mut cm, mut cs) = set_up();
        let before = cm.viewer_offset;
        cs.raise(&mut cm, false, 1.0, tick(1.0));
        let after = cm.viewer_offset;
        assert!((after.mag2().sqrt() - before.mag2().sqrt()).abs() < 1e-4);
        assert!(
            after.z > before.z,
            "Raise did not raise: {before:?} -> {after:?}"
        );

        // Lower's refusal: an offset already close in and below the pivot.
        let mut cm2 = CameraManager::new();
        let mut cs2 = CameraState::new(&mut cm2, PLAYER, tick(0.0));
        cm2.viewer_offset = Vec3::new(0.0, -1.0, -0.2);
        let held = cm2.viewer_offset;
        cs2.lower(&mut cm2, false, 1.0, tick(1.0));
        assert_eq!(
            cm2.viewer_offset, held,
            "x^2 + y^2 < 1.44 and z < 0 is refused"
        );
    }

    // Oracle: a rotation of the offset about z by
    // `angle * step`, which preserves its length and its z.
    #[test]
    fn rotate_swings_the_offset_about_the_pivot() {
        let (mut cm, mut cs) = set_up();
        let before = cm.viewer_offset;
        // rate != 1 uses the rate itself as the step, so this is exactly one `angle` radian step.
        cs.rotate(&mut cm, true, false, 1.0e-6, true, tick(1.0));
        cs.rotate(&mut cm, true, false, 1.0, true, tick(2.0));
        let after = cm.viewer_offset;
        assert!((after.mag2().sqrt() - before.mag2().sqrt()).abs() < 1e-4);
        assert_eq!(after.z, before.z, "Rotate is horizontal");
        assert!(after.x.abs() > 0.1, "the offset did not swing: {after:?}");
    }

    // Oracle: the retail target-for-offset branch structure, whose float comparisons are all
    // `a <= k`.
    #[test]
    fn set_target_for_offset_picks_the_documented_masks() {
        let (mut cm, cs) = set_up();
        // A camera pulled far back and high adds LOOK_AT_PIVOT because z > 0.75.
        cs.set_target_for_offset(&mut cm, Vec3::new(0.0, -5.0, 3.0));
        assert_eq!(
            cm.target_status,
            target::ALIGN_WITH_PLANE | target::LOOK_IN_DIRECTION | target::LOOK_AT_PIVOT
        );
        // A camera off to the side by more than 1 unit skips the ALIGN arm entirely.
        cs.set_target_for_offset(&mut cm, Vec3::new(2.0, -2.0, 0.0));
        assert_eq!(cm.target_status, target::LOOK_AT_PIVOT);
        // First person.
        cs.set_target_for_offset(&mut cm, IN_HEAD_OFFSET);
        assert_eq!(
            cm.target_status,
            target::ALIGN_WITH_PLANE | target::LOOK_IN_DIRECTION
        );
        // With a tracked target, everything else is skipped.
        let mut cs2 = cs.clone();
        cs2.targeting = true;
        cs2.set_target_for_offset(&mut cm, Vec3::new(0.0, -2.5, 0.75));
        assert_eq!(
            cm.target_status,
            target::LOOK_AT_PIVOT | target::LOOK_AT_OBJECT
        );
    }

    /// A free camera round trips through the frame the real camera produces.
    #[test]
    fn a_free_camera_round_trips_through_the_frame_the_real_camera_produces() {
        for dir in [
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(-0.4, 0.7, 0.59),
            Vec3::new(0.1, -0.2, -0.97),
        ] {
            let mut f = Frame::new(Vec3::new(5.0, 6.0, 7.0), Quat::IDENTITY);
            pmath::set_vector_heading(&mut f, dir);
            let cam = FreeCamera::from_frame(&f);
            let back = cam.frame();
            assert_eq!(cam.position, f.origin);
            // The two rotations agree: push the same vector through both.
            for v in [
                Vec3::new(0.0, 1.0, 0.0),
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(0.0, 0.0, 1.0),
            ] {
                let a = pmath::localtoglobalvec(pmath::l2g(f.rotation), v);
                let b = pmath::localtoglobalvec(pmath::l2g(back.rotation), v);
                assert!(
                    (a.x - b.x).abs() < 1e-4
                        && (a.y - b.y).abs() < 1e-4
                        && (a.z - b.z).abs() < 1e-4,
                    "{dir:?}: {a:?} != {b:?}"
                );
            }
            // And the camera's own forward is the direction that built the frame.
            let fwd = cam.forward();
            let mut n = dir;
            let _ = n.normalize_check_small();
            assert!(
                (fwd.x - n.x).abs() < 1e-4 && (fwd.y - n.y).abs() < 1e-4,
                "{dir:?} -> {fwd:?}"
            );
        }
    }

    // Oracle: frame rate is `20 / sum` over a 20-frame window, or 0 when that sum is
    // at or below 0.0002.
    #[test]
    fn the_frame_rate_window_is_twenty_frames() {
        let mut fr = FrameRate::default();
        assert_eq!(
            fr.fps(),
            0.0,
            "before any frame it is zero, exactly as the client's is"
        );
        for _ in 0..20 {
            fr.push(1.0 / 60.0);
        }
        assert!((fr.fps() - 60.0).abs() < 1e-3, "{}", fr.fps());
        // One frame recorded is 20 / that one delta, which is why the first zoom step is tiny
        // rather than infinite.
        let mut fr = FrameRate::default();
        fr.push(1.0 / 60.0);
        assert!((fr.fps() - 1200.0).abs() < 1e-1, "{}", fr.fps());
    }
}
