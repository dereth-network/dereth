//! What is solid to what.
//!
//! Every object has a physics type: a mask of physics-type bits. An object with collision
//! settings also says, for moving and for being placed, which physics types it passes through and
//! which objects it passes through or not whatever their type. One object is solid to a mover when
//! none of its physics-type bits is among those the mover passes through (or it is on the mover's
//! solid list despite them), and it is not on the mover's pass-through list nor, for now, on its
//! temporary list. Two objects pass through each other when either finds the other not solid.
//!
//! The server gives each object its physics type and its two masks in the object's description,
//! and the object's script applies them when the object is made: each as the description has it,
//! or nothing at all (solid) when the description lacks either half ([`Ethereality`]).

use dereth_primitives::Position;

use crate::collision::{settings, CollisionSettings};
use crate::ethereal::{ObjectEthereality, PhysicsTypes};

/// One side of a solidity question: an object's id (0 for none), its physics type, and its
/// collision settings when it has any.
#[derive(Clone, Copy, Debug)]
pub struct Party<'a> {
    pub id: u64,
    pub physics_type: PhysicsTypes,
    pub settings: Option<&'a CollisionSettings>,
}

/// Whether an object of physics type `physics_type` and id `id` blocks a mover that passes
/// through the physics types `physics` and the objects `objects` say: an object of a type the
/// mover passes through blocks only when on the solid list; any other unless on the pass-through
/// list.
// D2-05932
#[must_use]
pub fn is_blocking(
    physics_type: PhysicsTypes,
    id: u64,
    objects: &ObjectEthereality,
    physics: &PhysicsTypes,
) -> bool {
    if physics_type.mask & physics.mask != 0 {
        objects.in_solid_list(id)
    } else {
        !objects.in_ethereal_list(id)
    }
}

/// Whether `this` is solid to an object with collision settings `other`, moving or being placed.
/// An object that would block but has an id on the other's temporary pass-through list does not,
/// and sets `by_temporary`. No settings: not solid.
// D2-06129
#[must_use]
pub fn check_if_solid(
    this: &Party<'_>,
    other: Option<&CollisionSettings>,
    placement: bool,
    by_temporary: &mut bool,
) -> bool {
    let Some(s) = other else {
        return false;
    };
    let (physics, objects) = if placement {
        (&s.placement_physics, &s.placement_objects)
    } else {
        (&s.movement_physics, &s.movement_objects)
    };
    let blocking = is_blocking(this.physics_type, this.id, objects, physics);
    if blocking && this.id != 0 && s.in_temp_ethereal_list(this.id) {
        *by_temporary = true;
        return false;
    }
    blocking
}

/// Whether `this` and `other` pass through each other: `this` is not solid to `other`'s settings
/// (being placed when `placement`), or `other` is not solid to `this`'s settings (moving). An
/// object without settings asks only the other side. `by_temporary` starts false and is set when
/// a temporary pass-through list decided.
// D2-05929
#[must_use]
pub fn interpermeable(
    this: &Party<'_>,
    other: Option<&Party<'_>>,
    placement: bool,
    by_temporary: &mut bool,
) -> bool {
    *by_temporary = false;
    if let Some(o) = other {
        if o.settings.is_some() && !check_if_solid(this, o.settings, placement, by_temporary) {
            return true;
        }
    }
    if let (Some(_), Some(o)) = (this.settings, other) {
        if !check_if_solid(o, this.settings, false, by_temporary) {
            return true;
        }
    }
    false
}

/// Whether an object at `at` with containment radius `radius` is near enough to a mover checked
/// at `check` to be tested: within `radius + a + b` of it (the radii taken together, squared).
// D2-05957
#[must_use]
pub fn can_collide(at: &Position, radius: f32, a: f32, b: f32, check: &Position) -> bool {
    let d = crate::geom::offset(check, at);
    let reach = radius + a + b;
    let d2 = f64::from(d.x) * f64::from(d.x)
        + f64::from(d.y) * f64::from(d.y)
        + f64::from(d.z) * f64::from(d.z);
    // Unordered values can collide, as in the client.
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    let near = !(f64::from(reach) * f64::from(reach) < d2);
    near
}

/// What an object's description says it is and passes through: its physics type, and the physics
/// types it passes through moving and being placed. Each is the description's two halves, or
/// nothing (solid) when the description lacks either.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Ethereality {
    pub physics_type: u64,
    pub movement: u64,
    pub placement: u64,
}

impl Ethereality {
    /// The masks a description's numbers give, `number` answering for each number's bit.
    #[must_use]
    pub fn of_numbers(number: impl Fn(u32) -> Option<i32>) -> Self {
        let mask = |low, high| Self::mask(number(low), number(high));
        Self {
            physics_type: mask(0x8000, 0x1_0000),
            movement: mask(0x2_0000, 0x4_0000),
            placement: mask(0x8_0000, 0x10_0000),
        }
    }

    /// One mask from a description's two halves (low, high); 0 when either is missing.
    #[must_use]
    pub fn mask(low: Option<i32>, high: Option<i32>) -> u64 {
        match (low, high) {
            #[allow(clippy::cast_sign_loss)]
            (Some(l), Some(h)) => u64::from(l as u32) | (u64::from(h as u32) << 32),
            _ => 0,
        }
    }

    /// The settings that apply it: the physics type, and both masks.
    #[must_use]
    pub fn settings(&self) -> CollisionSettings {
        CollisionSettings {
            bits: settings::PHYSICS_TYPE | settings::MOVEMENT_PHYSICS | settings::PLACEMENT_PHYSICS,
            physics_type: PhysicsTypes::new(self.physics_type),
            movement_physics: PhysicsTypes::new(self.movement),
            placement_physics: PhysicsTypes::new(self.placement),
            ..CollisionSettings::default()
        }
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (unit checks of the rule's branches; the behavior rows are in the cpu tests)
    use super::*;
    use dereth_primitives::{CellId, Frame, Quat, Vec3};

    fn settings(movement: u64) -> CollisionSettings {
        CollisionSettings {
            movement_physics: PhysicsTypes::new(movement),
            ..CollisionSettings::default()
        }
    }

    #[test]
    fn a_type_the_mover_passes_through_blocks_only_from_the_solid_list() {
        let mut objects = ObjectEthereality::default();
        objects.set_to_solid();
        let physics = PhysicsTypes::new(0x10);
        assert!(!is_blocking(PhysicsTypes::new(0x30), 7, &objects, &physics));
        objects.add_to_solid_list(7);
        assert!(is_blocking(PhysicsTypes::new(0x30), 7, &objects, &physics));
        assert!(is_blocking(PhysicsTypes::new(0x20), 8, &objects, &physics));
        objects.add_to_ethereal_list(8);
        assert!(!is_blocking(PhysicsTypes::new(0x20), 8, &objects, &physics));
    }

    #[test]
    fn either_side_finding_the_other_not_solid_lets_them_pass() {
        let a = settings(0x10);
        let b = settings(0);
        let item = Party {
            id: 2,
            physics_type: PhysicsTypes::new(0x30),
            settings: Some(&b),
        };
        let mover = Party {
            id: 1,
            physics_type: PhysicsTypes::new(0x20),
            settings: Some(&a),
        };
        let mut t = false;
        assert!(interpermeable(&item, Some(&mover), false, &mut t));
        assert!(interpermeable(&mover, Some(&item), false, &mut t));
        let creature = Party {
            id: 3,
            physics_type: PhysicsTypes::new(0x40_0020),
            settings: Some(&b),
        };
        assert!(!interpermeable(&creature, Some(&mover), false, &mut t));
        assert!(!t);
    }

    #[test]
    fn the_temporary_list_lets_a_blocking_object_pass_and_says_so() {
        let mut a = settings(0);
        a.add_to_temp_ethereal_list(5);
        let other = Party {
            id: 5,
            physics_type: PhysicsTypes::new(0x20),
            settings: None,
        };
        let mover = Party {
            id: 1,
            physics_type: PhysicsTypes::new(0x20),
            settings: Some(&a),
        };
        let mut t = false;
        assert!(interpermeable(&other, Some(&mover), false, &mut t));
        assert!(t);
    }

    #[test]
    fn objects_beyond_the_radii_are_not_tested() {
        let p = |x: f32| {
            Position::new(
                CellId(0x0101_0001),
                Frame::new(Vec3::new(x, 10.0, 0.0), Quat::IDENTITY),
            )
        };
        assert!(can_collide(&p(13.0), 1.0, 1.0, 1.0, &p(10.0)));
        assert!(!can_collide(&p(13.1), 1.0, 1.0, 1.0, &p(10.0)));
    }

    #[test]
    fn a_mask_needs_both_halves() {
        assert_eq!(Ethereality::mask(Some(0x3E20_0010), Some(0)), 0x3E20_0010);
        assert_eq!(Ethereality::mask(Some(-1), Some(1)), 0x1_FFFF_FFFF);
        assert_eq!(Ethereality::mask(Some(5), None), 0);
    }
}
