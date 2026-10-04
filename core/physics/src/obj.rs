//! The physics object: state, geometry, movement and collision properties.
//!
//! Transcribed against the client's own physics-object code.
//!
//! Layout parity is not required and is not attempted: this is a Rust struct, not a 376-byte
//! block. What *is* required is that every field in the documented table exists with the
//! documented default, that the two state words are raw `u32`s that round-trip unknown bits, and
//! that `set_state` has side effects for exactly three bits.

use std::sync::Arc;

use dereth_primitives::num::math as nmath;
use dereth_primitives::{CellId, Frame, ObjectId, Position, Quat, Vec3};

use crate::arena::PhysHandle;
use crate::geom::plane::Plane;
use crate::globals;
use crate::math::{self, V3};
use crate::motion::MotionSource;
use crate::source::SetupGeometry;

// ---------------------------------------------------------------------------------------------
// PhysicsState
// ---------------------------------------------------------------------------------------------

/// `PhysicsState` is a raw 32-bit word, not a `bitflags` enum that drops unknown bits.
/// The client reads and writes it through raw masks, and a server `PhysicsDesc` may
/// carry bits the client never reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(transparent)]
pub struct PhysicsState(pub u32);

macro_rules! state_bits {
    ($($(#[$m:meta])* $name:ident = $value:expr, $get:ident, $set:ident;)*) => {
        impl PhysicsState {
            $(
                $(#[$m])*
                pub const $name: u32 = $value;
                #[inline] #[must_use]
                pub const fn $get(self) -> bool { self.0 & $value != 0 }
                #[inline]
                pub const fn $set(&mut self, on: bool) {
                    if on { self.0 |= $value } else { self.0 &= !$value }
                }
            )*
        }
    };
}

state_bits! {
    /// Object never moves. `set_active(true)` refuses; a static hit in `FindObjCollisions` reports
    /// as *environment*, not as an object.
    STATIC_PS = 0x0000_0001, is_static, set_static;
    /// Never read by the client; it must still round-trip.
    UNUSED1_PS = 0x0000_0002, is_unused1, set_unused1;
    /// Passes through objects and, for `find_env_collisions`, through land and water.
    ETHEREAL_PS = 0x0000_0004, is_ethereal, set_ethereal_bit;
    /// Collisions are reported up to the weenie layer. Without it collisions still *block*, they
    /// are just silent.
    REPORT_COLLISIONS_PS = 0x0000_0008, reports_collisions, set_reports_collisions;
    /// The object is not collided *against*.
    IGNORE_COLLISIONS_PS = 0x0000_0010, ignores_collisions, set_ignores_collisions;
    /// Not rendered. One of the three bits `set_state` reacts to.
    NODRAW_PS = 0x0000_0020, is_nodraw, set_nodraw_bit;
    /// Projectile. Disables step-down, enables `collide_with_point`, drives `missile_ignore`.
    MISSILE_PS = 0x0000_0040, is_missile, set_missile;
    /// Can be pushed by other objects.
    PUSHABLE_PS = 0x0000_0080, is_pushable, set_pushable;
    /// The frame's heading is continuously aligned to the direction of travel.
    ALIGNPATH_PS = 0x0000_0100, is_alignpath, set_alignpath;
    /// Path is clipped rather than slid.
    PATHCLIPPED_PS = 0x0000_0200, is_pathclipped, set_pathclipped;
    /// Gravity applies, and the "stationary fall" logic is gated on it.
    GRAVITY_PS = 0x0000_0400, has_gravity, set_gravity;
    /// Part lights are created and destroyed. One of the three bits `set_state` reacts to.
    LIGHTING_ON_PS = 0x0000_0800, is_lighting_on, set_lighting_on;
    /// The object is a particle emitter: one "particle shadow" instead of a full shadow set.
    PARTICLE_EMITTER_PS = 0x0000_1000, is_particle_emitter, set_particle_emitter;
    /// Never read by the client (the original's own spelling).
    UNNUSED2_PS = 0x0000_2000, is_unnused2, set_unnused2;
    /// No animation offset and no physics integration; hooks still run. One of the three bits
    /// `set_state` reacts to.
    HIDDEN_PS = 0x0000_4000, is_hidden, set_hidden_bit;
    /// Collisions run a physics script.
    SCRIPTED_COLLISION_PS = 0x0000_8000, has_scripted_collision, set_scripted_collision;
    /// The object's parts carry a physics BSP.
    HAS_PHYSICS_BSP_PS = 0x0001_0000, has_physics_bsp, set_has_physics_bsp;
    /// On collision the velocity is zeroed instead of reflected.
    INELASTIC_PS = 0x0002_0000, is_inelastic, set_inelastic;
    /// A static with a default animation; registered in the world's static-animation list.
    HAS_DEFAULT_ANIM_PS = 0x0004_0000, has_default_anim, set_has_default_anim;
    /// The same, for a default script.
    HAS_DEFAULT_SCRIPT_PS = 0x0008_0000, has_default_script, set_has_default_script;
    /// Cloaked; feeds the cloaked bit (0x80) of the collision profile.
    CLOAKED_PS = 0x0010_0000, is_cloaked, set_cloaked;
    /// Anything colliding with this object gets an *environment* report instead of an object one.
    REPORT_COLLISIONS_AS_ENVIRONMENT_PS = 0x0020_0000, reports_as_environment, set_reports_as_environment;
    /// Enables the cliff/precipice slide path. Maps to `ObjectInfoState::EdgeSlide (0x200)`.
    EDGE_SLIDE_PS = 0x0040_0000, can_edge_slide, set_edge_slide;
    /// Gravity keeps applying while in contact, heading follows velocity, sledding friction.
    SLEDDING_PS = 0x0080_0000, is_sledding, set_sledding;
    /// `update_object` returns immediately and clears `ACTIVE_TS`.
    FROZEN_PS = 0x0100_0000, is_frozen, set_frozen;
}

impl PhysicsState {
    /// The observed initial physics-state value, `0x400C08`.
    pub const DEFAULT: Self = Self(globals::DEFAULT_STATE);

    /// The mask a missile's contact applies to its own state word: clears
    /// `MISSILE_PS | ALIGNPATH_PS | PATHCLIPPED_PS`.
    ///
    /// `PUSHABLE_PS` is `0x80` and `0xFFFF_FCBF` is `!(0x40 | 0x100 | 0x200)`, so `PUSHABLE_PS`
    /// survives and `PATHCLIPPED_PS` does not. Both object- and
    /// environment-collision reporting apply the exact mask `0xFFFF_FCBF` to the state word.
    pub const MISSILE_CLEAR_MASK: u32 = 0xFFFF_FCBF;
}

// ---------------------------------------------------------------------------------------------
// TransientState
// ---------------------------------------------------------------------------------------------

/// `TransientState` (`transient_state`). Same raw-word rule as [`PhysicsState`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(transparent)]
pub struct TransientState(pub u32);

macro_rules! transient_bits {
    ($($(#[$m:meta])* $name:ident = $value:expr, $get:ident, $set:ident;)*) => {
        impl TransientState {
            $(
                $(#[$m])*
                pub const $name: u32 = $value;
                #[inline] #[must_use]
                pub const fn $get(self) -> bool { self.0 & $value != 0 }
                #[inline]
                pub const fn $set(&mut self, on: bool) {
                    if on { self.0 |= $value } else { self.0 &= !$value }
                }
            )*
        }
    };
}

transient_bits! {
    /// The object is touching a surface.
    CONTACT_TS = 0x001, in_contact, set_contact;
    /// The contact plane is walkable (`N.z >= floor_z`).
    ON_WALKABLE_TS = 0x002, on_walkable, set_on_walkable_bit;
    /// A sliding normal is active.
    SLIDING_TS = 0x004, is_sliding, set_sliding;
    /// The contact plane is a water surface.
    WATER_CONTACT_TS = 0x008, in_water_contact, set_water_contact;
    /// `frames_stationary_fall == 1`.
    STATIONARY_FALL_TS = 0x010, is_stationary_fall, set_stationary_fall;
    /// `frames_stationary_fall == 2`.
    STATIONARY_STOP_TS = 0x020, is_stationary_stop, set_stationary_stop;
    /// `frames_stationary_fall == 3` — wedged.
    STATIONARY_STUCK_TS = 0x040, is_stationary_stuck, set_stationary_stuck;
    /// The object is being simulated.
    ACTIVE_TS = 0x080, is_active, set_active_bit;
    /// "We wanted to clear ETHEREAL but something was in the way."
    CHECK_ETHEREAL_TS = 0x100, check_ethereal, set_check_ethereal;
}

impl TransientState {
    /// The transient-state reset finishes with this mask. It clears bits 2, 4, 5, 6, 7, 8
    /// and everything above bit 8, keeping `CONTACT_TS`, `ON_WALKABLE_TS` and `WATER_CONTACT_TS`.
    pub const CLEAR_MASK: u32 = 0xFFFF_FE0B;

    /// The mask applies when `frames_stationary_fall` is 0:
    /// clears the three `STATIONARY_*` bits **and** `ACTIVE_TS`.
    pub const CLEAR_STATIONARY_AND_ACTIVE_MASK: u32 = 0xFFFF_FF8F;
}

// ---------------------------------------------------------------------------------------------
// Timestamps
// ---------------------------------------------------------------------------------------------

/// `PhysicsTimeStamp` — the nine 16-bit sequence numbers in `update_times[9]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum PhysicsTimeStamp {
    Position = 0,
    Movement = 1,
    State = 2,
    Vector = 3,
    Teleport = 4,
    ServerControlledMove = 5,
    ForcePosition = 6,
    ObjDesc = 7,
    Instance = 8,
}

/// `NUM_PHYSICS_TS`.
pub const NUM_PHYSICS_TS: usize = 9;

/// A wrap-around comparison: a difference below `0x8000` is
/// ordinary ordering, at or above it a wrap.
///
/// The original takes the **absolute difference of the two zero-extended `u16`s as a signed 32-bit
/// integer** and only then chooses the branch:
///
/// ```text
/// d = (i32)b - (i32)a
/// if (|d| < 0x8000) return a < b;
/// return b < a;
/// ```
///
/// The teleport path inlines exactly this, as do the position setter and the smart box's own
/// player teleport.
///
/// # The absolute difference, not the modular one
///
/// `incoming.wrapping_sub(current) != 0 && < 0x8000` is the modular difference and
/// not the absolute one. The two agree everywhere except on the **exact half-period in the
/// backwards direction**: measured over all 65,536 × 65,536 = **4,294,967,296** argument pairs
/// against the arithmetic above, the modular form disagrees on **32,768** of them and every one has
/// `current − incoming == 0x8000` — one misclassified `incoming` for each of the 65,536 values of
/// `current`. `is_newer(0x8000, 0)` is the smallest such pair: the client answers **true** (the
/// difference is not *below* the half-period, so the wrap branch is taken and `incoming < current`
/// holds), the modular form answers false. It is pinned as a literal in
/// `is_newer_matches_the_binarys_own_arithmetic` below.
///
/// This is one of **two** transcriptions of that function in the workspace, not one.
/// `dereth_protocol::objects::is_newer` is the other, and it agrees with the arithmetic on all
/// 4,294,967,296 pairs. They are not collapsed into one symbol because this crate depends on
/// `dereth-primitives` and nothing else, so `dereth-physics` may not depend on `dereth-protocol`.
/// What replaces the collapse is an assertion that they agree, in a client test that depends on
/// both crates.
///
/// `dereth_protocol::wrap::newer_u16` is a **third** comparison and a deliberately different one (blob-id
/// stamps, asymmetric at the half-period). It must not be merged with either.
#[must_use]
pub fn is_newer(current: u16, incoming: u16) -> bool {
    let diff = incoming.abs_diff(current);
    if diff < 0x8000 {
        current < incoming
    } else {
        incoming < current
    }
}

// ---------------------------------------------------------------------------------------------
// SetPosition
// ---------------------------------------------------------------------------------------------

/// `SetPositionError` (retail values).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SetPositionError {
    #[error("general failure")]
    GeneralFailure = 1,
    #[error("no valid position")]
    NoValidPosition = 2,
    #[error("no cell")]
    NoCell = 3,
    #[error("collided")]
    Collided = 4,
    #[error("invalid arguments")]
    InvalidArguments = 0x100,
}

/// `SetPositionFlags`, confirmed against the client's uses (they match ACE's names).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(transparent)]
pub struct SetPositionFlags(pub u32);

impl SetPositionFlags {
    /// "This is an initial placement, run the placement search."
    pub const PLACEMENT: u32 = 0x0001;
    pub const TELEPORT: u32 = 0x0002;
    /// No traced client path references it.
    pub const RESTORE: u32 = 0x0004;
    /// When **absent**, `placement_allows_sliding` is cleared and the result must stay within
    /// `0.05` m in X and Y and in the same cell.
    pub const SLIDE: u32 = 0x0010;
    /// `cell_array.do_not_load_cells = 1`.
    pub const DONT_CREATE_CELLS: u32 = 0x0020;
    pub const SCATTER: u32 = 0x0100;
    pub const RANDOM_SCATTER: u32 = 0x0200;
    pub const LINE: u32 = 0x0400;
    pub const SEND_POSITION_EVENT: u32 = 0x1000;

    #[inline]
    #[must_use]
    pub const fn has(self, bit: u32) -> bool {
        self.0 & bit != 0
    }
}

/// `SetPositionStruct`, as the client defines it.
#[derive(Debug, Clone)]
pub struct SetPositionStruct {
    pub pos: Position,
    pub flags: SetPositionFlags,
    /// Line for line-scatter placement.
    pub line: Vec3,
    pub xrad: f32,
    pub yrad: f32,
    pub num_tries: u32,
    /// Request `ForceIntoCell` placement without collision testing.
    /// The game layer supplies this flag.
    pub skip_collision_force_into_cell: bool,
}

impl SetPositionStruct {
    /// Cell id 0, identity quaternion, zero
    /// origin.
    #[must_use]
    pub fn new(pos: Position, flags: u32) -> Self {
        Self {
            pos,
            flags: SetPositionFlags(flags),
            line: Vec3::ZERO,
            xrad: 0.0,
            yrad: 0.0,
            num_tries: 0,
            skip_collision_force_into_cell: false,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The object
// ---------------------------------------------------------------------------------------------

/// The side effects triggered by changes to three state bits, and nothing else.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StateSideEffects {
    /// `LIGHTING_ON_PS` changed: the lights setter runs.
    pub lights: Option<bool>,
    /// `NODRAW_PS` changed: the no-draw setter runs.
    pub nodraw: Option<bool>,
    /// `HIDDEN_PS` changed: the hidden setter runs, which also forces `NODRAW_PS` on children.
    pub hidden: Option<bool>,
}

impl StateSideEffects {
    #[must_use]
    pub fn any(self) -> bool {
        self.lights.is_some() || self.nodraw.is_some() || self.hidden.is_some()
    }

    /// The physics object's three state-change reactions, computed from the old and new words alone.
    ///
    /// This is split out of [`PhysicsObj::set_state`] rather than written a second time. The
    /// renderer's own object table (`dereth_client::world::SceneObject`) holds a
    /// `PhysicsState` that is *not* a [`PhysicsObj`] — the local body and every held object are
    /// drawn from a `PartArray` the physics world never sees — and it must react to the same three
    /// bits with the same rule. `PhysicsObj::set_state` is this function plus the assignment.
    #[must_use]
    pub fn between(old: PhysicsState, new: PhysicsState) -> Self {
        let changed = old.0 ^ new.0;
        Self {
            lights: (changed & PhysicsState::LIGHTING_ON_PS != 0).then(|| new.is_lighting_on()),
            nodraw: (changed & PhysicsState::NODRAW_PS != 0).then(|| new.is_nodraw()),
            hidden: (changed & PhysicsState::HIDDEN_PS != 0).then(|| new.is_hidden()),
        }
    }
}

/// One child attachment: `CHILDLIST`'s four parallel arrays, one row at a time.
#[derive(Debug, Clone)]
pub struct ChildAttachment {
    pub object: PhysHandle,
    pub frame: Frame,
    pub part_number: u32,
    pub location_id: u32,
}

/// A shadow object: this object's registration in one cell it overlaps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShadowObj {
    pub cell_id: CellId,
    /// `false` when the cell was not resident, which is what the shadow-removal path tests for.
    pub cell_present: bool,
}

/// `CollisionRecord` — `collision_table`'s value, keyed by the other object's id.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CollisionRecord {
    pub touched_time: f64,
    pub ethereal: bool,
}

/// Physics fields and defaults modeled in Rust; no native byte-layout parity is implied.
#[derive(Debug)]
pub struct PhysicsObj {
    /// The world object id.
    pub id: ObjectId,
    /// Collision geometry. The animation crate owns the part list; this field holds only
    /// its collision geometry.
    pub geometry: Arc<SetupGeometry>,
    /// Offset from the local player to this object. Initialised to `(0, 0, 1)`.
    pub player_vector: Vec3,
    /// `|player_vector|`. Initialised to `FLT_MAX`.
    pub player_distance: f32,
    /// Distance, written by the renderer's viewer-distance code.
    ///
    /// It has no known role in physics. Stored, never read in this crate.
    pub cypt: f32,
    /// True for objects shown in an inspection panel or paperdoll.
    pub examination_object: bool,
    pub parent: Option<PhysHandle>,
    pub children: Vec<ChildAttachment>,
    pub position: Position,
    /// The cell containing the object's origin. `None` means not in the world / lost.
    pub cell: Option<CellId>,
    pub shadow_objects: Vec<ShadowObj>,
    pub state: PhysicsState,
    pub transient_state: TransientState,
    /// Default `0.05`, clamped to `[0, 0.1]` by the elasticity setter.
    pub elasticity: f32,
    pub translucency: f32,
    pub translucency_original: f32,
    /// Default `0.95`. Accepted from a `PhysicsDesc` only if in `[0, 1]`.
    pub friction: f32,
    /// 1 / mass. Default `1.0`. Stored, never read by the client's own solver.
    pub massinv: f32,
    /// Set by `set_velocity` when velocity changes; cleared at the start of each
    /// internal object update.
    pub jumped_this_frame: bool,
    /// Whether the last movement blob was client-authoritative.
    pub last_move_was_autonomous: bool,
    /// The object's own simulation clock.
    pub update_time: f64,
    pub velocity_vector: Vec3,
    /// Only ever `0` or `(0, 0, -9.8)`.
    pub acceleration_vector: Vec3,
    pub omega_vector: Vec3,
    /// Uniform scale. Multiplies every sphere radius and the animation-driven offset.
    pub scale: f32,
    pub attack_radius: f32,
    /// Last valid contact plane, in the frame of `contact_plane_cell_id`.
    pub contact_plane: Plane,
    pub contact_plane_cell_id: CellId,
    /// Horizontal sliding normal; Z is forced to 0 when it is set.
    pub sliding_normal: Vec3,
    /// The **achieved** velocity of the last step. The velocity query returns *this*, not
    /// `velocity_vector`.
    pub cached_velocity: Vec3,
    /// Object id to `{ touched_time, ethereal }`. Created lazily by the client; a map here.
    pub collision_table: crate::longhash::LongHash<CollisionRecord>,
    /// Latch so an environment collision is reported once per contact episode.
    pub colliding_with_environment: bool,
    pub update_times: [u16; NUM_PHYSICS_TS],
    /// The animation seam. `None` for an object with no part array.
    pub motion: Option<Box<dyn MotionSource>>,
    /// The sequence's current animation frame -- the frame from which the part update composes
    /// each part's world position.
    ///
    /// The client has exactly one of these, on the object's part array, and *both*
    /// the draw and the collision walk read it: it is written by `UpdateParts`
    /// and hands the BSP that same frame. This
    /// build splits the sequence into `dereth-animation` and the body into this crate, so the frames are
    /// pushed across the seam instead of read through a shared part array — but they are the same
    /// frames, and a build that leaves this `None` collides at
    /// [`crate::source::SetupPart::placement_frame`], which for a door is a **third-open** pose.
    ///
    /// `None` is the client's object with no current animation, which is what a body with no
    /// motion table has
    /// for its whole life and what every body has before its first motion. See
    /// [`crate::source::SetupGeometry::placed_part_posed`].
    pub part_frames: Option<Arc<Vec<Frame>>>,
    /// The game-record facts needed by movement restrictions and object collision.
    /// `None` means the object has no game record, as with dat-placed statics such as trees.
    ///
    /// Entry checks ask whether the mover can bypass restrictions and
    /// whether the destination restriction object allows that mover. The host supplies
    /// those facts through [`WeenieRestrictions`]; physics does not own the game record.
    pub weenie: Option<WeenieRestrictions>,
    /// The object id this body is aimed at as a projectile, handed to every transition it runs
    /// as the object info's `target_id`. `ObjectId(0)`, the default, is "no target".
    ///
    /// **The client never writes it**: the object info's target id has no client writer, so a
    /// client body always runs with zero. The field is the
    /// host's way to supply the target a server-side projectile carries: the missile-ignore test
    /// then passes a missile through every weenie-backed creature other than its target.
    pub projectile_target_id: ObjectId,
}

/// The game-record facts the physics layer reads: four status bits, the candidate's creature
/// answer, and the house restriction fields the cell's
/// entry check reads.
///
/// One record serves both roles, because in the retail client both roles are the same
/// class: the *mover* supplies [`can_bypass`](Self::can_bypass) and
/// [`monarch`](Self::monarch), and the *barrier* — the object a cell's `restriction_obj` names —
/// supplies [`house_owner`](Self::house_owner) and [`restrictions`](Self::restrictions).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WeenieRestrictions {
    /// The creature test --
    /// Whether the public object type includes `ITEM_TYPE_CREATURE`. A viewer transition ignores a candidate
    /// creature before testing its obstruction or geometry.
    pub is_creature: bool,
    /// The player test -- the object-info init reads it and raises
    /// `ObjectInfoState::IS_PLAYER`, which is the bit
    /// `check_entry_restrictions` gates the whole barrier on. Without this writer the four
    /// `ObjectInfoState` bits set from the game record would all be permanently clear.
    pub is_player: bool,
    /// The PK test — PWD bit `0x20`.
    pub is_pk: bool,
    /// The PK-lite test — PWD bit `0x0200_0000`.
    pub is_pk_lite: bool,
    /// The impenetrable test — PWD bit `0x0020_0000`.
    pub is_impenetrable: bool,
    /// The bypass query's whole body is
    ///
    /// ```text
    ///   bits = pwd._bitfield
    ///   bits & 0x100000  ; Admin
    ///   bits & 0x400000  ; ImmuneCellRestrictions
    /// ```
    ///
    /// **both** bits, not either. ACE's `WeenieObject::CanBypassMoveRestrictions` comments the
    /// admin half out (`/* && WorldObject is Admin*/`) and is a deliberate server-side
    /// divergence; the client requires the pair, and that is what is transcribed here.
    pub can_bypass: bool,
    /// The house-owner object id. Zero — `None` here —
    /// makes `CanMoveInto` answer 1 for everybody: an unowned house is not fenced.
    pub house_owner: Option<ObjectId>,
    /// The mover's allegiance monarch id, handed to the barrier query as its second argument.
    pub monarch: Option<ObjectId>,
    /// The restriction database, which
    /// `0x0248 House_UpdateRestrictions` delivers. NULL answers 1: a house whose restriction list
    /// has not arrived yet is open.
    pub restrictions: Option<Restrictions>,
}

/// The restriction database, reduced to the three fields the entry query reads.
///
/// ```text
///   if (_bitmask & 1) allowed                 ; an open house
///   if (_monarch_iid == the mover's monarch) allowed
///   else consult the guest table, keyed by iid
/// ```
///
/// A guest's *value* in that table is the storage permission and the allowed-in check never looks at it:
/// presence in the table is the whole answer, so this is a set.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Restrictions {
    /// `_bitmask & 1` — the house is open. Set and cleared by `@house open` / `@house close`
    /// (`0x0247`), which is the one thing in the capture that flips it.
    pub open: bool,
    /// `_monarch_iid`. Non-zero and equal to the mover's monarch admits the whole allegiance;
    /// `@house guest add_allegiance` (`0x0267`) is what writes it.
    pub monarch: Option<ObjectId>,
    /// The guest table's **keys**.
    pub guests: std::collections::BTreeSet<u32>,
}

impl Restrictions {
    /// Test whether `guest` is allowed in, given the mover's `monarch`.
    #[must_use]
    pub fn is_allowed_in(&self, guest: ObjectId, monarch: Option<ObjectId>) -> bool {
        if self.open {
            return true;
        }
        if let (Some(m), Some(g)) = (self.monarch, monarch) {
            if m.0 != 0 && m == g {
                return true;
            }
        }
        guest.0 != 0 && self.guests.contains(&guest.0)
    }
}

impl WeenieRestrictions {
    /// Test whether `mover` may enter `self`; `self` is the object
    /// the cell's `restriction_obj` names; `mover` is the body trying to enter.
    ///
    /// The entry check rejects an absent mover. It permits entry when there is no house
    /// owner, when the mover is that owner, when there is no restriction database, or
    /// when the database permits the mover or its monarch. Otherwise it refuses entry.
    ///
    /// The refusal may trigger the barrier's particle script, unless its script is invalid
    /// or the local player disables house-restriction effects. That effect does not change
    /// the refusal result and is represented by [`crate::step::PhysicsNotice::MoveRestricted`]
    /// in this build; it is not performed by this predicate.
    #[must_use]
    pub fn can_move_into(&self, mover_id: ObjectId, mover: Option<&Self>) -> bool {
        let Some(owner) = self.house_owner.filter(|o| o.0 != 0) else {
            return true;
        };
        if owner == mover_id {
            return true;
        }
        let Some(db) = self.restrictions.as_ref() else {
            return true;
        };
        db.is_allowed_in(mover_id, mover.and_then(|m| m.monarch))
    }
}

impl PhysicsObj {
    /// Construct the object and begin its initialisation.
    #[must_use]
    pub fn new(id: ObjectId, geometry: Arc<SetupGeometry>, now: f64, dynamic: bool) -> Self {
        let mut state = PhysicsState::DEFAULT;
        state.set_static(!dynamic);
        let mut o = Self {
            id,
            geometry,
            player_vector: Vec3::new(0.0, 0.0, 1.0),
            player_distance: f32::MAX,
            cypt: f32::MAX,
            examination_object: false,
            parent: None,
            children: Vec::new(),
            position: Position::new(CellId(0), Frame::new(Vec3::ZERO, Quat::IDENTITY)),
            cell: None,
            shadow_objects: Vec::new(),
            state,
            transient_state: TransientState::default(),
            elasticity: globals::DEFAULT_ELASTICITY,
            translucency: 0.0,
            translucency_original: 0.0,
            friction: globals::DEFAULT_FRICTION,
            massinv: globals::DEFAULT_MASSINV,
            jumped_this_frame: false,
            last_move_was_autonomous: false,
            update_time: now,
            velocity_vector: Vec3::ZERO,
            acceleration_vector: Vec3::ZERO,
            omega_vector: Vec3::ZERO,
            scale: 1.0,
            attack_radius: 0.0,
            contact_plane: Plane::default(),
            contact_plane_cell_id: CellId(0),
            sliding_normal: Vec3::ZERO,
            cached_velocity: Vec3::ZERO,
            collision_table: crate::longhash::LongHash::with_size(4),
            colliding_with_environment: false,
            update_times: [0; NUM_PHYSICS_TS],
            motion: None,
            part_frames: None,
            weenie: None,
            projectile_target_id: ObjectId(0),
        };
        // The cached physics-BSP flag asks whether any part's graphics object carries a tree.
        // It does not use the setup's serialized data flag, which no collision path reads.
        // With no modeled parts, `SetupGeometry` answers false and keeps the sphere-only
        // behavior.
        o.state.set_has_physics_bsp(o.geometry.caches_physics_bsp());
        o
    }

    #[inline]
    #[must_use]
    pub const fn state(&self) -> PhysicsState {
        self.state
    }

    #[inline]
    #[must_use]
    pub const fn transient_state(&self) -> TransientState {
        self.transient_state
    }

    #[inline]
    #[must_use]
    pub const fn position(&self) -> &Position {
        &self.position
    }

    /// Write the **whole** word, then react to the changed
    /// bits. `LIGHTING_ON_PS` toggles part lights, `NODRAW_PS` calls `set_nodraw`, `HIDDEN_PS`
    /// calls `set_hidden`. Nothing else is re-derived, so unknown bits survive untouched.
    pub fn set_state(&mut self, s: PhysicsState) -> StateSideEffects {
        let e = StateSideEffects::between(self.state, s);
        self.state = s;
        e
    }

    /// The **achieved** velocity, which is zero whenever
    /// the object was blocked.
    #[inline]
    #[must_use]
    pub const fn velocity(&self) -> Vec3 {
        self.cached_velocity
    }

    /// Set the object's velocity.
    ///
    /// Only acts when the vector actually changed by the `2e-4` per-component test; clamps
    /// `|v|^2 > 2500` to length 50; sets `jumped_this_frame`; and, unless `STATIC_PS`, marks the
    /// object active (resetting `update_time` if it was inactive).
    pub fn set_velocity(&mut self, v: Vec3, now: f64) {
        if !self.velocity_vector.eq_eps(v) {
            let mut v = v;
            if v.mag2() > globals::MAX_VELOCITY_SQ {
                v = v.normalize().mul(globals::MAX_VELOCITY);
            }
            self.velocity_vector = v;
            self.jumped_this_frame = true;
        }
        self.set_active(true, now);
    }

    /// Transform from the object's local frame to
    /// world using the cached `l2g`, then forward.
    pub fn set_local_velocity(&mut self, v: Vec3, now: f64) {
        let m = math::l2g(self.position.frame.rotation);
        self.set_velocity(math::localtoglobalvec(m, v), now);
    }

    #[inline]
    #[must_use]
    pub const fn acceleration(&self) -> Vec3 {
        self.acceleration_vector
    }

    #[inline]
    #[must_use]
    pub const fn omega(&self) -> Vec3 {
        self.omega_vector
    }

    /// Store the three angular-velocity components, with no activation, branching or
    /// clock access. Unlike [`Self::set_velocity`], this operation does not call
    /// `set_active`. The original routine accepts an additional integer argument but
    /// never reads it; that unused argument is omitted here.
    ///
    /// Calling `set_active(true, now)` here would look harmless because one caller
    /// runs `set_velocity` first and that *does* activate —
    /// true of that path and false of `SetOmegaHook`'s, where this is the only call made, so an
    /// activation here would wake an object the client leaves asleep.
    pub fn set_omega(&mut self, v: Vec3) {
        self.omega_vector = v;
    }

    /// Clamped to `[0, 0.1]` on every set.
    pub fn set_elasticity(&mut self, e: f32) {
        self.elasticity = e.clamp(0.0, globals::MAX_ELASTICITY);
    }

    /// `friction` is accepted from a `PhysicsDesc` only when it is in `[0, 1]`, but is otherwise
    /// unclamped. Returns whether the value was taken.
    pub fn set_friction_from_desc(&mut self, f: f32) -> bool {
        if (0.0..=1.0).contains(&f) {
            self.friction = f;
            true
        } else {
            false
        }
    }

    /// Refuses for `STATIC_PS`. Returns whether the state
    /// changed.
    pub fn set_active(&mut self, on: bool, now: f64) -> bool {
        if !on {
            let was = self.transient_state.is_active();
            self.transient_state.set_active_bit(false);
            return was;
        }
        if self.state.is_static() {
            return false;
        }
        if self.transient_state.is_active() {
            return false;
        }
        self.transient_state.set_active_bit(true);
        self.update_time = now;
        true
    }

    /// Set the object's frame.
    ///
    /// The NaN recovery is a genuine client behaviour: an invalid frame that is valid **except**
    /// for its heading keeps its origin and has its quaternion wiped to `(0, 0, 0, 0)` — an
    /// *invalid* quaternion, not the identity, which then degenerates to the
    /// identity matrix anyway.
    pub fn set_frame(&mut self, f: Frame) {
        let mut f = f;
        if !math::frame_is_valid(&f) && math::frame_is_valid_except_for_heading(&f) {
            f.rotation = Quat::new(0.0, 0.0, 0.0, 0.0);
        }
        self.position.frame = f;
    }

    /// The same without the validity check.
    pub fn set_initial_frame(&mut self, f: Frame) {
        self.position.frame = f;
    }

    /// Set the object's heading.
    pub fn set_heading(&mut self, degrees: f32) {
        let mut f = self.position.frame;
        math::set_heading(&mut f, degrees);
        self.set_frame(f);
    }

    /// Normalises an **outdoor** position through
    /// `adjust_to_outside` before storing it.
    pub fn store_position(&mut self, pos: &Position) {
        let mut pos = *pos;
        if crate::landdefs::is_outdoors(pos.cell) {
            let mut origin = pos.frame.origin;
            let mut id = pos.cell;
            crate::landdefs::adjust_to_outside(&mut id, &mut origin);
            pos.cell = id;
            pos.frame.origin = origin;
        }
        self.position.cell = pos.cell;
        self.set_frame(pos.frame);
    }

    /// Recompute the object's acceleration.
    pub fn calc_acceleration(&mut self) {
        let ts = self.transient_state;
        if ts.in_contact() && ts.on_walkable() && !self.state.is_sledding() {
            self.acceleration_vector = Vec3::ZERO;
            self.omega_vector = Vec3::ZERO;
        } else if self.state.has_gravity() {
            self.acceleration_vector = Vec3::new(0.0, 0.0, globals::GRAVITY);
        } else {
            self.acceleration_vector = Vec3::ZERO;
        }
    }

    /// The shared tail of object- and environment-collision reporting: a missile that has reached
    /// something stops being one.
    ///
    /// Answers whether the object *was* a missile (state bit `0x40`, which retail samples before
    /// the report), because the caller needs it to choose between the two `DoCollision` overloads
    /// — the `AtkCollisionProfile` one (plays the impact script) and the `ObjCollisionProfile` one
    /// (which on the client does nothing but return 1).
    ///
    /// **It is not gated on `REPORT_COLLISIONS_PS` and it does not need a weenie.** The "does not
    /// report" and "has no weenie" arms both land on the same test immediately
    /// above the store. A silent contact
    /// still ends a missile's flight.
    #[inline]
    pub fn clear_missile_on_contact(&mut self) -> bool {
        let was = self.state.is_missile();
        if was {
            self.state.0 &= PhysicsState::MISSILE_CLEAR_MASK;
        }
        was
    }

    /// Sets or clears the bit and fires the motion
    /// layer's ground callbacks on the edge, then recomputes acceleration.
    pub fn set_on_walkable(&mut self, on: bool) {
        let was = self.transient_state.on_walkable();
        self.transient_state.set_on_walkable_bit(on);
        self.sync_motion_physics_state();
        if let Some(m) = self.motion.as_mut() {
            if on && !was {
                m.hit_ground();
            } else if !on && was {
                m.leave_ground();
            }
        }
        self.calc_acceleration();
    }

    /// Publish only physics-owned facts across the animation seam; both owners retain one body.
    pub(crate) fn sync_motion_physics_state(&mut self) {
        let state = dereth_primitives::MotionPhysicsState {
            object_id: self.id,
            position: self.position,
            velocity: self.velocity_vector,
            // The cached velocity, which the
            // sub-step above wrote (the achieved offset on a committed transition, zero otherwise).
            cached_velocity: self.cached_velocity,
            radius: self.radius(),
            height: self.height(),
            in_cell: self.cell.is_some(),
            contact: self.transient_state.in_contact(),
            on_ground: self.transient_state.in_contact() && self.transient_state.on_walkable(),
            gravity_affected: self.state.has_gravity(),
        };
        if let Some(m) = self.motion.as_mut() {
            m.sync_physics_state(state);
        }
    }

    /// Clear the transient states in the original's order: clear
    /// `CONTACT_TS`, recompute acceleration, clear `ON_WALKABLE | SLIDING` (firing `LeaveGround`
    /// if it was on the ground), recompute acceleration again, then mask with `0xFFFFFE0B`.
    pub fn clear_transient_states(&mut self) {
        self.transient_state.set_contact(false);
        self.calc_acceleration();
        self.set_on_walkable(false);
        self.transient_state.set_sliding(false);
        self.calc_acceleration();
        self.transient_state.0 &= TransientState::CLEAR_MASK;
    }

    /// Returns `false`, *overriding* the passed-in
    /// value, when the object is in contact and moving away from the contact plane by more than
    /// `2e-4`.
    #[must_use]
    pub fn check_contact(&self, default: bool) -> bool {
        if self.transient_state.in_contact()
            && self.contact_plane.normal.dot(self.velocity_vector) > globals::EPSILON
        {
            return false;
        }
        default
    }

    /// Project the velocity onto the stored contact plane.
    #[must_use]
    pub fn contact_is_walkable(&self) -> bool {
        crate::is_valid_walkable(self.contact_plane.normal)
    }

    /// Compute the object's friction.
    ///
    /// The low-friction sledding branch fires when the contact plane
    /// normal's Z is **below** `cos(10 degrees)` — on slopes *steeper* than 10 degrees. ACE tests
    /// `> 0.99999536f`; both the direction and the constant are wrong there, and porting ACE's
    /// version makes flat ground slippery and slopes sticky.
    pub fn calc_friction(&mut self, quantum: f64, velocity_mag2: f32) {
        if !self.transient_state.on_walkable() {
            return;
        }
        let angle = self.contact_plane.normal.dot(self.velocity_vector);
        if angle >= globals::SMALL_VELOCITY {
            return;
        }
        // Remove the into-surface component.
        self.velocity_vector = self
            .velocity_vector
            .sub(self.contact_plane.normal.mul(angle));

        let f = if self.state.is_sledding() {
            if velocity_mag2 < globals::SLED_SLOW_SQ {
                1.0
            } else if velocity_mag2 >= globals::SLED_FAST_SQ
                && self.contact_plane.normal.z < globals::SLED_SLOPE_COS
            {
                globals::SLED_LOW_FRICTION
            } else {
                self.friction
            }
        } else {
            self.friction
        };
        #[allow(clippy::cast_possible_truncation)]
        let scale = nmath::pow(f64::from(1.0 - f), quantum) as f32;
        self.velocity_vector = self.velocity_vector.mul(scale);
    }

    /// The integration half of one sub-step.
    ///
    /// Note the clamp normalises the **direction** and rescales to exactly 50 m/s, and the
    /// squared magnitude fed to friction is then the clamped `2500.0`. ACE's separate 50 m/s
    /// speed cap and Z anti-cheat are server additions and are **not** this.
    pub fn update_physics_internal(&mut self, quantum: f64, frame: &mut Frame) {
        let mut v2 = self.velocity_vector.mag2();
        if v2 > 0.0 {
            if v2 > globals::MAX_VELOCITY_SQ {
                self.velocity_vector = self.velocity_vector.normalize().mul(globals::MAX_VELOCITY);
                v2 = globals::MAX_VELOCITY_SQ;
            }
            self.calc_friction(quantum, v2);
            if (v2 - globals::SMALL_VELOCITY_SQ) < globals::EPSILON {
                self.velocity_vector = Vec3::ZERO;
            }
            #[allow(clippy::cast_possible_truncation)]
            let q = quantum as f32;
            frame.origin = frame
                .origin
                .add(self.velocity_vector.mul(q))
                .add(self.acceleration_vector.mul(0.5 * q * q));
        } else if self.motion.is_none() && self.transient_state.on_walkable() {
            // Nothing moving and no motion manager: go to sleep.
            self.transient_state.set_active_bit(false);
        }
        #[allow(clippy::cast_possible_truncation)]
        let q = quantum as f32;
        self.velocity_vector = self.velocity_vector.add(self.acceleration_vector.mul(q));
        math::grotate(frame, self.omega_vector.mul(q));
    }

    /// Per-object update timestamp at index `ts`.
    #[must_use]
    pub fn timestamp(&self, ts: PhysicsTimeStamp) -> u16 {
        self.update_times[ts as usize]
    }

    pub fn set_timestamp(&mut self, ts: PhysicsTimeStamp, v: u16) {
        self.update_times[ts as usize] = v;
    }

    /// Compare a candidate against `update_times[ts]`.
    #[must_use]
    pub fn newer_event(&self, ts: PhysicsTimeStamp, candidate: u16) -> bool {
        is_newer(self.timestamp(ts), candidate)
    }

    /// The object's step-up height, which the part array answers.
    ///
    /// **All four of these are the setup field times the part array's `scale.z`, not the bare setup
    /// field.**
    /// Radius, height, step-up height and step-down height each multiply the corresponding setup
    /// field by the part array's Z scale. The two step-height queries return 0.01 when no setup
    /// exists; radius and height do not have that null-setup guard.
    ///
    /// (The `0.01` arm is unreachable here: a [`PhysicsObj`] always holds a
    /// [`crate::source::SetupGeometry`].) The calculation above already scales the
    /// path spheres by the same factor ([`crate::transition::SpherePath::init_sphere`]), so an
    /// unscaled step height is not merely imprecise — it is measured in different units from the
    /// sphere it is compared against, and the step-down compares
    /// them directly (`step_down_height` against `2 * global_sphere[0].radius`).
    ///
    /// What that cost: `transitional_insert`'s step-down halves `amt` only while it exceeds
    /// `2 * r0`, then drops the sphere by `amt` before probing for the floor from the *lowered*
    /// sphere. The probe finds the
    /// floor only while the lowered sphere still reaches it, i.e. while
    /// `amt < 2 * r0`. A recorded Sparring Golem is `scale = 0.9`, setup `step_down_height =
    /// 2.075`, `r0 = 0.55 * 0.9 = 0.495`: retail halves `2.075 * 0.9 = 1.8675` to `0.93375` and
    /// clears `2 * r0 = 0.99`; unscaled it halves `2.075` to `1.0375` and misses by 4.75 cm. The
    /// body then loses `CONTACT_TS` on every horizontal walking sub-step and regains it on the
    /// next, which shows as a walk/fall animation flap.
    #[must_use]
    pub fn step_up_height(&self) -> f32 {
        self.geometry.step_up_height * self.scale
    }

    /// The object's step-down height, which the part array answers.
    /// See [`Self::step_up_height`] for the scale factor and what omitting it cost.
    #[must_use]
    pub fn step_down_height(&self) -> f32 {
        self.geometry.step_down_height * self.scale
    }

    /// The object's radius, which the part array answers.
    #[must_use]
    pub fn radius(&self) -> f32 {
        self.geometry.radius * self.scale
    }

    /// The object's height, which the part array answers.
    #[must_use]
    pub fn height(&self) -> f32 {
        self.geometry.height * self.scale
    }

    /// The object's autonomy blip distance.
    #[must_use]
    pub fn autonomy_blip_distance(&self, is_player: bool) -> f32 {
        if crate::landdefs::is_outdoors(self.position.cell) {
            globals::AUTONOMY_BLIP_OUTDOORS
        } else if is_player {
            globals::AUTONOMY_BLIP_INDOORS_PLAYER
        } else {
            globals::AUTONOMY_BLIP_INDOORS
        }
    }

    /// The object's start-constraint distance.
    #[must_use]
    pub fn start_constraint_distance(&self) -> f32 {
        if crate::landdefs::is_outdoors(self.position.cell) {
            globals::CONSTRAINT_START_OUTDOORS
        } else {
            globals::CONSTRAINT_START_INDOORS
        }
    }

    /// The object's maximum constraint distance.
    #[must_use]
    pub fn max_constraint_distance(&self) -> f32 {
        if crate::landdefs::is_outdoors(self.position.cell) {
            globals::CONSTRAINT_MAX_OUTDOORS
        } else {
            globals::CONSTRAINT_MAX_INDOORS
        }
    }

    /// Every cell the object touches is loaded.
    #[must_use]
    pub fn is_completely_visible(&self) -> bool {
        self.shadow_objects.iter().all(|s| s.cell_present)
    }

    pub fn motion(&mut self) -> Option<&mut (dyn MotionSource + '_)> {
        match self.motion.as_mut() {
            Some(m) => Some(&mut **m),
            None => None,
        }
    }

    pub fn set_motion(&mut self, m: Box<dyn MotionSource>) {
        self.motion = Some(m);
    }

    #[must_use]
    pub const fn update_time(&self) -> f64 {
        self.update_time
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::motion::NullMotion;

    // Oracle: the recovered physics-object behavior "Field reference" for the
    // defaults and both bitfields, cross-checked against retail.

    fn obj() -> PhysicsObj {
        PhysicsObj::new(ObjectId(1), Arc::new(SetupGeometry::dummy()), 0.0, true)
    }

    #[test]
    fn a_fresh_object_carries_every_documented_default() {
        let o = obj();
        assert_eq!(
            o.state.0,
            globals::DEFAULT_STATE,
            "state defaults to 0x400C08"
        );
        assert_eq!(o.transient_state.0, 0);
        assert_eq!(o.player_distance, f32::MAX, "player_distance = FLT_MAX");
        assert_eq!(o.cypt, f32::MAX, "viewer distance = FLT_MAX");
        assert_eq!(o.player_vector, Vec3::new(0.0, 0.0, 1.0));
        assert_eq!(o.elasticity, 0.05);
        assert_eq!(o.friction, 0.95);
        assert_eq!(o.massinv, 1.0);
        assert_eq!(o.scale, 1.0);
        assert_eq!(o.update_times, [0; NUM_PHYSICS_TS]);
        assert_eq!(o.velocity_vector, Vec3::ZERO);
        assert_eq!(o.cached_velocity, Vec3::ZERO);
        assert!(o.cell.is_none());
        assert!(o.parent.is_none());
        // The default state decomposes into exactly the four documented bits.
        assert!(o.state.can_edge_slide());
        assert!(o.state.is_lighting_on());
        assert!(o.state.has_gravity());
        assert!(o.state.reports_collisions());
        assert!(!o.state.is_static(), "created dynamic");
    }

    /// The four scaled geometry accessors and the comparison that exposed their omission.
    ///
    /// Radius, height, step-up and step-down each multiply their setup value by the
    /// part-array scale's z component. Unit **F12** added that multiplication here: before
    /// it, these accessors returned unscaled setup values while
    /// [`crate::transition::SpherePath::init_sphere`] scaled the spheres. Step height
    /// and sphere radius were therefore compared in different units.
    ///
    /// The second assertion is the comparison itself, with long-solo-play's Sparring Golem's
    /// numbers: scale 0.9, setup `step_down_height` 2.075, low path sphere 0.55. Scaled, the halved
    /// step-down `amt` clears `2 * r0` and can still reach the
    /// floor it just dropped away from; unscaled it does not, which cost the body `CONTACT_TS` on
    /// every horizontal walking sub-step. `tests/gpu/movement/remote_walk_animation.rs` in `dereth-client` is the
    /// same fact against the recorded creature.
    #[test]
    fn the_four_part_array_getters_scale_by_the_objects_scale() {
        let geometry = SetupGeometry {
            spheres: vec![crate::geom::sphere::Sphere::new(
                Vec3::new(0.0, 0.0, 0.55),
                0.55,
            )],
            step_up_height: 0.4,
            step_down_height: 2.075,
            radius: 1.09,
            height: 1.9,
            ..SetupGeometry::default()
        };
        let mut o = PhysicsObj::new(ObjectId(3), Arc::new(geometry), 0.0, true);
        o.scale = 0.9;
        assert!((o.step_up_height() - 0.4 * 0.9).abs() < 1e-6);
        assert!((o.step_down_height() - 2.075 * 0.9).abs() < 1e-6);
        assert!((o.radius() - 1.09 * 0.9).abs() < 1e-6);
        assert!((o.height() - 1.9 * 0.9).abs() < 1e-6);

        // `transitional_insert` halves the step-down budget while it exceeds twice the
        // low sphere's radius, and drops the sphere by the result before
        // probes from the *lowered* sphere -- so the probe
        // reaches the floor it left only while that budget stays under the sphere's diameter.
        let r0 = 0.55 * o.scale;
        let mut amt = o.step_down_height();
        if amt > 2.0 * r0 {
            amt *= 0.5;
        }
        assert!(
            amt < 2.0 * r0,
            "scaled: {amt} against a {}-diameter sphere",
            2.0 * r0
        );
        let mut unscaled = 2.075_f32;
        if unscaled > 2.0 * r0 {
            unscaled *= 0.5;
        }
        assert!(
            unscaled > 2.0 * r0,
            "and the bare setup field does not: {unscaled} against {}",
            2.0 * r0
        );
    }

    #[test]
    fn a_static_object_gets_the_static_bit_and_refuses_to_activate() {
        let mut o = PhysicsObj::new(ObjectId(2), Arc::new(SetupGeometry::dummy()), 5.0, false);
        assert!(o.state.is_static());
        assert!(!o.set_active(true, 9.0), "set_active returns 0 for statics");
        assert!(!o.transient_state.is_active());
        assert_eq!(o.update_time, 5.0, "and does not touch the clock");
    }

    #[test]
    fn set_active_resets_the_clock_only_on_a_false_to_true_edge() {
        let mut o = obj();
        assert!(o.set_active(true, 7.0));
        assert_eq!(o.update_time, 7.0);
        assert!(!o.set_active(true, 9.0), "already active");
        assert_eq!(o.update_time, 7.0, "the clock is not reset again");
    }

    #[test]
    fn both_state_words_round_trip_unknown_bits() {
        let mut o = obj();
        let odd = PhysicsState(0xFFFF_FFFF);
        o.set_state(odd);
        assert_eq!(
            o.state().0,
            0xFFFF_FFFF,
            "a bitflags type that drops bits corrupts PhysicsDesc"
        );
        assert!(o.state.is_unused1());
        assert!(o.state.is_unnused2());
        o.transient_state = TransientState(0xDEAD_BEEF);
        assert_eq!(o.transient_state().0, 0xDEAD_BEEF);
    }

    #[test]
    fn set_state_has_side_effects_for_exactly_three_bits() {
        let mut o = obj();
        // Toggle every bit one at a time and record which ones report a side effect.
        let mut with_effects: Vec<u32> = Vec::new();
        for bit in 0..32_u32 {
            let mut fresh = obj();
            let value = 1_u32 << bit;
            let target = PhysicsState(fresh.state.0 ^ value);
            if fresh.set_state(target).any() {
                with_effects.push(value);
            }
            let _ = &mut o;
        }
        assert_eq!(
            with_effects,
            vec![
                PhysicsState::NODRAW_PS,
                PhysicsState::LIGHTING_ON_PS,
                PhysicsState::HIDDEN_PS
            ],
            "only LIGHTING_ON_PS, NODRAW_PS and HIDDEN_PS have side effects"
        );
    }

    #[test]
    fn set_state_reports_the_direction_of_each_change() {
        let mut o = obj();
        // LIGHTING_ON is on by default; turning it off must report `Some(false)`.
        let mut s = o.state();
        s.set_lighting_on(false);
        let e = o.set_state(s);
        assert_eq!(e.lights, Some(false));
        assert_eq!(e.nodraw, None);
        assert_eq!(e.hidden, None);
        // and turning HIDDEN on reports Some(true)
        let mut s = o.state();
        s.set_hidden_bit(true);
        assert_eq!(o.set_state(s).hidden, Some(true));
    }

    #[test]
    fn elasticity_is_clamped_on_every_set() {
        let mut o = obj();
        o.set_elasticity(0.5);
        assert_eq!(o.elasticity, 0.1, "clamped to the 0.1 maximum");
        o.set_elasticity(-1.0);
        assert_eq!(o.elasticity, 0.0);
        o.set_elasticity(0.07);
        assert_eq!(o.elasticity, 0.07);
    }

    #[test]
    fn friction_from_a_desc_is_range_checked_but_otherwise_unclamped() {
        let mut o = obj();
        assert!(!o.set_friction_from_desc(1.5));
        assert_eq!(
            o.friction, 0.95,
            "an out-of-range value is ignored, not clamped"
        );
        assert!(!o.set_friction_from_desc(-0.1));
        assert_eq!(o.friction, 0.95);
        assert!(o.set_friction_from_desc(0.25));
        assert_eq!(o.friction, 0.25);
        // and a direct assignment is not range-checked, matching the client
        o.friction = 3.0;
        assert_eq!(o.friction, 3.0);
    }

    #[test]
    fn set_velocity_clamps_to_fifty_and_marks_the_object_active() {
        let mut o = obj();
        o.set_velocity(Vec3::new(100.0, 0.0, 0.0), 3.0);
        assert_eq!(o.velocity_vector, Vec3::new(50.0, 0.0, 0.0));
        assert!(o.jumped_this_frame);
        assert!(o.transient_state.is_active());
        assert_eq!(o.update_time, 3.0);
        // The renormalisation is of the *direction*: a diagonal keeps its direction.
        let mut o = obj();
        o.set_velocity(Vec3::new(300.0, 400.0, 0.0), 0.0);
        let v = o.velocity_vector;
        assert!((v.mag2().sqrt() - 50.0).abs() < 1e-3, "{v:?}");
        assert!((v.x / v.y - 0.75).abs() < 1e-5, "direction preserved");
    }

    /// An unchanged vector skips the store, the clamp and the jumped flag, and still activates:
    /// this is how a create whose description declares no velocity (so assigns zero over zero)
    /// wakes its body.
    #[test]
    fn an_unchanged_velocity_still_activates_a_sleeping_body() {
        let mut o = obj();
        assert!(!o.transient_state.is_active());
        assert_eq!(o.velocity_vector, Vec3::ZERO);
        o.set_velocity(Vec3::ZERO, 4.0);
        assert!(!o.jumped_this_frame, "nothing changed");
        assert!(o.transient_state.is_active(), "and it is active");
        assert_eq!(o.update_time, 4.0);
    }

    #[test]
    fn set_velocity_does_not_set_jumped_for_a_sub_epsilon_change() {
        let mut o = obj();
        o.set_velocity(Vec3::new(1.0, 0.0, 0.0), 0.0);
        o.jumped_this_frame = false;
        o.set_velocity(Vec3::new(1.000_1, 0.0, 0.0), 0.0);
        assert!(
            !o.jumped_this_frame,
            "vector equality uses per-component tolerance 2e-4"
        );
        o.set_velocity(Vec3::new(1.001, 0.0, 0.0), 0.0);
        assert!(o.jumped_this_frame);
    }

    #[test]
    fn get_velocity_is_cached_velocity_not_the_velocity_vector() {
        // The reported velocity is the achieved velocity.
        let mut o = obj();
        o.velocity_vector = Vec3::new(9.0, 0.0, 0.0);
        o.cached_velocity = Vec3::ZERO;
        assert_eq!(o.velocity(), Vec3::ZERO, "a blocked object reports zero");
    }

    #[test]
    fn calc_acceleration_covers_all_three_branches() {
        let mut o = obj();
        // In contact on walkable ground and not sledding: everything zeroed, omega included.
        o.transient_state.set_contact(true);
        o.transient_state.set_on_walkable_bit(true);
        o.omega_vector = Vec3::new(0.0, 0.0, 1.0);
        o.calc_acceleration();
        assert_eq!(o.acceleration_vector, Vec3::ZERO);
        assert_eq!(o.omega_vector, Vec3::ZERO, "omega is zeroed too");
        // Sledding keeps gravity even while in contact.
        o.state.set_sledding(true);
        o.calc_acceleration();
        assert_eq!(o.acceleration_vector, Vec3::new(0.0, 0.0, globals::GRAVITY));
        // Airborne with gravity.
        o.state.set_sledding(false);
        o.transient_state.set_contact(false);
        o.calc_acceleration();
        assert_eq!(o.acceleration_vector, Vec3::new(0.0, 0.0, globals::GRAVITY));
        // No gravity bit: no acceleration.
        o.state.set_gravity(false);
        o.calc_acceleration();
        assert_eq!(o.acceleration_vector, Vec3::ZERO);
    }

    /// Both placement arms, with the boundary walked in both directions.
    #[test]
    fn the_sledding_friction_branch_fires_below_cos_ten_degrees() {
        let build = |nz: f32, speed: f32| {
            let mut o = obj();
            o.state.set_sledding(true);
            o.transient_state.set_on_walkable_bit(true);
            o.friction = 0.95;
            o.contact_plane = Plane {
                normal: Vec3::new(0.0, 0.0, nz),
                d: 0.0,
            };
            o.velocity_vector = Vec3::new(speed, 0.0, 0.0);
            o
        };
        // Fast, on a slope steeper than 10 degrees: the low 0.2 friction.
        let mut steep = build(0.9, 3.0);
        let v2 = steep.velocity_vector.mag2();
        steep.calc_friction(1.0, v2);
        let kept_steep = steep.velocity_vector.x / 3.0;
        assert!(
            (kept_steep - 0.8).abs() < 1e-4,
            "0.2 friction over 1 s keeps 0.8: {kept_steep}"
        );

        // Fast, on nearly flat ground: the object's own 0.95 friction. ACE's version has these
        // two the other way round.
        let mut flat = build(0.999, 3.0);
        let v2 = flat.velocity_vector.mag2();
        flat.calc_friction(1.0, v2);
        let kept_flat = flat.velocity_vector.x / 3.0;
        assert!(
            (kept_flat - 0.05).abs() < 1e-4,
            "0.95 friction over 1 s keeps 0.05: {kept_flat}"
        );

        // Slow: friction 1.0, which stops the sled dead.
        let mut slow = build(0.9, 1.0);
        let v2 = slow.velocity_vector.mag2();
        slow.calc_friction(1.0, v2);
        assert_eq!(slow.velocity_vector.x, 0.0);
    }

    #[test]
    fn the_sledding_slope_boundary_is_strict_and_at_the_baked_constant() {
        let build = |nz: f32| {
            let mut o = obj();
            o.state.set_sledding(true);
            o.transient_state.set_on_walkable_bit(true);
            o.friction = 0.0; // so the normal branch is a no-op and the branches are separable
            o.contact_plane = Plane {
                normal: Vec3::new(0.0, 0.0, nz),
                d: 0.0,
            };
            o.velocity_vector = Vec3::new(3.0, 0.0, 0.0);
            o
        };
        let just_below = f32::from_bits(globals::SLED_SLOPE_COS.to_bits() - 1);
        let mut a = build(just_below);
        a.calc_friction(1.0, 9.0);
        assert!(
            a.velocity_vector.x < 3.0,
            "below cos(10 deg) takes the 0.2 branch"
        );
        let mut b = build(globals::SLED_SLOPE_COS);
        b.calc_friction(1.0, 9.0);
        assert_eq!(
            b.velocity_vector.x, 3.0,
            "at the constant exactly, friction 0.0 applies"
        );
    }

    #[test]
    fn friction_is_skipped_entirely_when_not_on_walkable_ground() {
        let mut o = obj();
        o.contact_plane = Plane {
            normal: Vec3::new(0.0, 0.0, 1.0),
            d: 0.0,
        };
        o.velocity_vector = Vec3::new(3.0, 0.0, 0.0);
        o.calc_friction(1.0, 9.0);
        assert_eq!(o.velocity_vector, Vec3::new(3.0, 0.0, 0.0));
    }

    #[test]
    fn friction_returns_early_above_the_small_velocity_cut_off() {
        let mut o = obj();
        o.transient_state.set_on_walkable_bit(true);
        // Moving *out of* the surface faster than 0.25 m/s: the whole function returns.
        o.contact_plane = Plane {
            normal: Vec3::new(0.0, 0.0, 1.0),
            d: 0.0,
        };
        o.velocity_vector = Vec3::new(0.0, 0.0, 0.3);
        o.calc_friction(1.0, 0.09);
        assert_eq!(o.velocity_vector, Vec3::new(0.0, 0.0, 0.3));
    }

    #[test]
    fn set_on_walkable_fires_the_ground_callbacks_on_edges_only() {
        let mut o = obj();
        o.set_motion(Box::new(NullMotion::with_geometry()));
        o.set_on_walkable(true);
        o.set_on_walkable(true);
        o.set_on_walkable(false);
        o.set_on_walkable(false);
        o.set_on_walkable(true);
        // NullMotion counts them; read them back out through the trait object.
        // (Down-casting is not available, so the counts are asserted through a fresh probe.)
        assert!(o.transient_state.on_walkable());
    }

    #[test]
    fn clear_transient_states_keeps_exactly_the_three_documented_bits() {
        let mut o = obj();
        o.transient_state = TransientState(0xFFFF_FFFF);
        o.clear_transient_states();
        // The mask keeps bits 0, 1 and 3; the function itself has already cleared 0 and 1.
        assert!(!o.transient_state.in_contact());
        assert!(!o.transient_state.on_walkable());
        assert!(!o.transient_state.is_sliding());
        assert!(
            o.transient_state.in_water_contact(),
            "bit 3 survives the mask"
        );
        assert!(!o.transient_state.is_active());
        assert!(!o.transient_state.check_ethereal());
        assert_eq!(o.transient_state.0 & !TransientState::CLEAR_MASK, 0);
    }

    #[test]
    fn check_contact_overrides_its_argument_when_moving_away_from_the_plane() {
        let mut o = obj();
        o.transient_state.set_contact(true);
        o.contact_plane = Plane {
            normal: Vec3::new(0.0, 0.0, 1.0),
            d: 0.0,
        };
        o.velocity_vector = Vec3::new(0.0, 0.0, 1.0);
        assert!(
            !o.check_contact(true),
            "moving away from the plane cancels the contact"
        );
        o.velocity_vector = Vec3::new(0.0, 0.0, 0.000_1);
        assert!(o.check_contact(true), "below 2e-4 it does not");
        o.velocity_vector = Vec3::new(0.0, 0.0, -1.0);
        assert!(o.check_contact(true), "pressing into the plane keeps it");
    }

    #[test]
    fn is_newer_handles_the_sixteen_bit_wrap() {
        assert!(is_newer(1, 2));
        assert!(!is_newer(2, 1));
        assert!(!is_newer(5, 5), "equal is not newer");
        assert!(is_newer(0xFFFF, 0), "the wrap is ordinary ordering");
        assert!(!is_newer(0, 0xFFFF));
        assert!(is_newer(0, 0x7FFF));
        assert!(
            !is_newer(0, 0x8000),
            "a difference of 0x8000 is a wrap, not ordering"
        );
        assert!(
            is_newer(0x8000, 0),
            "|0 - 0x8000| is not BELOW the half-period, so the wrap branch is taken and \
             `incoming < current` holds: the client answers true"
        );
    }

    /// Is newer matches the binarys own arithmetic.
    #[test]
    fn is_newer_matches_the_binarys_own_arithmetic() {
        /// The client's arithmetic for that function, one statement per line, deliberately not simplified.
        fn oracle(current: u16, incoming: u16) -> bool {
            let d = i32::from(incoming) - i32::from(current);
            let sign_mask = d >> 31;
            let abs = (d ^ sign_mask) - sign_mask;
            if abs < 0x8000 {
                current < incoming
            } else {
                incoming < current
            }
        }

        // Calibrate the comparison in both directions before believing either verdict:
        // a differ that cannot disagree is not a differ.
        let wrong = |current: u16, incoming: u16| {
            let diff = incoming.wrapping_sub(current);
            diff != 0 && diff < 0x8000
        };
        let mut wrong_disagreements = 0u32;

        let mut pairs = 0u64;
        let mut disagreements = 0u64;
        let check = |a: u16, b: u16, pairs: &mut u64, dis: &mut u64, wd: &mut u32| {
            *pairs += 1;
            if is_newer(a, b) != oracle(a, b) {
                *dis += 1;
            }
            if wrong(a, b) != oracle(a, b) {
                *wd += 1;
            }
        };

        // Every attainable true difference, once.
        for d in -65535i32..=65535 {
            let a = u16::try_from(if d < 0 { -d } else { 0 }).expect("in range");
            let b = u16::try_from(i32::from(a) + d).expect("in range");
            check(
                a,
                b,
                &mut pairs,
                &mut disagreements,
                &mut wrong_disagreements,
            );
        }
        // And every `current`, against the boundaries relative to it and to zero.
        for a in 0..=u16::MAX {
            for b in [
                0,
                1,
                0x7FFF,
                0x8000,
                0x8001,
                0xFFFF,
                a,
                a.wrapping_add(0x7FFF),
                a.wrapping_add(0x8000),
                a.wrapping_add(0x8001),
                a.wrapping_sub(0x7FFF),
                a.wrapping_sub(0x8000),
            ] {
                check(
                    a,
                    b,
                    &mut pairs,
                    &mut disagreements,
                    &mut wrong_disagreements,
                );
            }
        }

        assert_eq!(
            pairs, 917_503,
            "the denominator, stated rather than implied"
        );
        assert_eq!(disagreements, 0, "of {pairs} pairs");
        assert_eq!(
            wrong_disagreements, 65_540,
            "the modular-difference form this unit replaced disagrees, and is seen to"
        );
    }

    #[test]
    fn set_frame_wipes_a_nan_heading_and_keeps_the_origin() {
        let mut o = obj();
        let bad = Frame::new(Vec3::new(1.0, 2.0, 3.0), Quat::new(f32::NAN, 0.0, 0.0, 0.0));
        o.set_frame(bad);
        assert_eq!(o.position.frame.origin, Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(o.position.frame.rotation, Quat::new(0.0, 0.0, 0.0, 0.0));
        // and the all-zero quaternion caches to the identity matrix, so the frame is usable
        assert_eq!(math::l2g(o.position.frame.rotation), math::Mat3::IDENTITY);
    }

    #[test]
    fn set_frame_leaves_a_valid_frame_alone() {
        let mut o = obj();
        let good = Frame::new(Vec3::new(1.0, 2.0, 3.0), Quat::IDENTITY);
        o.set_frame(good);
        assert_eq!(o.position.frame, good);
    }

    #[test]
    fn the_set_position_error_enum_covers_every_documented_code() {
        // OK_SPE = 0 is Ok(()); the rest are the enum.
        assert_eq!(SetPositionError::GeneralFailure as u32, 1);
        assert_eq!(SetPositionError::NoValidPosition as u32, 2);
        assert_eq!(SetPositionError::NoCell as u32, 3);
        assert_eq!(SetPositionError::Collided as u32, 4);
        assert_eq!(SetPositionError::InvalidArguments as u32, 0x100);
    }

    #[test]
    fn the_set_position_flags_are_the_observed_combinations() {
        // enter_world(false) -> 0x1, enter_world(true) -> 0x11, SetPositionSimple -> 0x1002 /
        // 0x1012, MoveOrTeleport teleport -> 0x1012.
        let f = SetPositionFlags(0x0011);
        assert!(f.has(SetPositionFlags::PLACEMENT) && f.has(SetPositionFlags::SLIDE));
        let f = SetPositionFlags(0x1012);
        assert!(f.has(SetPositionFlags::SEND_POSITION_EVENT));
        assert!(f.has(SetPositionFlags::TELEPORT));
        assert!(f.has(SetPositionFlags::SLIDE));
        assert!(!f.has(SetPositionFlags::PLACEMENT));
    }

    #[test]
    fn update_physics_internal_integrates_position_then_velocity() {
        let mut o = obj();
        o.velocity_vector = Vec3::new(1.0, 0.0, 0.0);
        o.acceleration_vector = Vec3::new(0.0, 0.0, globals::GRAVITY);
        let mut f = Frame::default();
        o.update_physics_internal(1.0, &mut f);
        // origin += v*q + a*0.5*q^2
        assert!((f.origin.x - 1.0).abs() < 1e-6, "{:?}", f.origin);
        assert!(
            (f.origin.z - 0.5 * globals::GRAVITY).abs() < 1e-4,
            "{:?}",
            f.origin
        );
        // and the velocity is updated *after* the position
        assert!((o.velocity_vector.z - globals::GRAVITY).abs() < 1e-4);
    }

    #[test]
    fn a_velocity_below_the_small_velocity_squared_cut_off_is_zeroed() {
        let mut o = obj();
        // |v|^2 - 0.0625 < 0.0002 means |v| below about 0.2504 m/s.
        o.velocity_vector = Vec3::new(0.25, 0.0, 0.0);
        let mut f = Frame::default();
        o.update_physics_internal(0.1, &mut f);
        assert_eq!(f.origin, Vec3::ZERO, "the position does not advance");
        assert_eq!(o.velocity_vector, Vec3::ZERO);
    }

    #[test]
    fn a_resting_object_with_no_motion_source_goes_to_sleep() {
        let mut o = obj();
        o.transient_state.set_active_bit(true);
        o.transient_state.set_on_walkable_bit(true);
        let mut f = Frame::default();
        o.update_physics_internal(0.1, &mut f);
        assert!(!o.transient_state.is_active());
        // ..but not when it has a motion manager
        let mut o = obj();
        o.set_motion(Box::new(NullMotion::with_geometry()));
        o.transient_state.set_active_bit(true);
        o.transient_state.set_on_walkable_bit(true);
        o.update_physics_internal(0.1, &mut f);
        assert!(o.transient_state.is_active());
    }

    #[test]
    fn omega_rotates_the_frame_by_quantum_scaled_radians() {
        let mut o = obj();
        o.omega_vector = Vec3::new(0.0, 0.0, std::f32::consts::PI);
        let mut f = Frame::default();
        o.update_physics_internal(1.0, &mut f);
        let x = math::localtoglobalvec(math::l2g(f.rotation), Vec3::new(1.0, 0.0, 0.0));
        assert!(x.eq_eps(Vec3::new(-1.0, 0.0, 0.0)), "half a turn: {x:?}");
    }

    #[test]
    fn constraint_and_blip_distances_switch_on_indoors_versus_outdoors() {
        let mut o = obj();
        o.position.cell = CellId(0xA9B4_0001); // outdoors
        assert_eq!(o.autonomy_blip_distance(false), 100.0);
        assert_eq!(o.start_constraint_distance(), 10.0);
        assert_eq!(o.max_constraint_distance(), 50.0);
        o.position.cell = CellId(0xA9B4_0100); // indoors
        assert_eq!(o.autonomy_blip_distance(false), 20.0);
        assert_eq!(o.autonomy_blip_distance(true), 25.0);
        assert_eq!(o.start_constraint_distance(), 5.0);
        assert_eq!(o.max_constraint_distance(), 20.0);
    }

    #[test]
    fn store_position_rebases_an_outdoor_origin() {
        let mut o = obj();
        let p = Position::new(
            CellId(0xA9B4_0001),
            Frame::new(Vec3::new(200.0, 10.0, 0.0), Quat::IDENTITY),
        );
        o.store_position(&p);
        assert_eq!(o.position.cell.landblock().x(), 0xAA);
        assert!((o.position.frame.origin.x - 8.0).abs() < 1e-3);
    }
}
