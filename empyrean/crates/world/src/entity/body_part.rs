// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/BodyPart.cs
//! Port of `Source/ACE.Server/Entity/BodyPart.cs`.
//!
//! The `BodyPart` flags, the pure `BodyParts` statics, and the three `GetBodyPart` overloads,
//! which roll `ThreadSafeRandom`.

use std::collections::HashMap;
use std::sync::LazyLock;

use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_entity::enums::{AttackHeight, CombatBodyPart, CoverageMask};
use empyrean_entity::models::PropertiesBodyPart;
use empyrean_entity::ObjectGuid;

use crate::World;

/// ACE enum `BodyPart` (`[Flags]`, declared in ACE.Server), underlying `int`. "This is more like a
/// combined coverage mask?"
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct BodyPart(pub i32);

#[allow(non_upper_case_globals)]
impl BodyPart {
    pub const Head: Self = Self(0x1);
    pub const Chest: Self = Self(0x2);
    pub const Abdomen: Self = Self(0x4);
    pub const UpperArm: Self = Self(0x8);
    pub const LowerArm: Self = Self(0x10);
    pub const Hand: Self = Self(0x20);
    pub const UpperLeg: Self = Self(0x40);
    pub const LowerLeg: Self = Self(0x80);
    pub const Foot: Self = Self(0x100);

    /// Every declared member, in `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[
        Self::Head,
        Self::Chest,
        Self::Abdomen,
        Self::UpperArm,
        Self::LowerArm,
        Self::Hand,
        Self::UpperLeg,
        Self::LowerLeg,
        Self::Foot,
    ];

    /// `Enum.HasFlag`: `(self & flag) == flag`.
    #[must_use]
    pub const fn has_flag(self, flag: Self) -> bool {
        self.0 & flag.0 == flag.0
    }
}

impl std::ops::BitOr for BodyPart {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

// ACE: BodyParts.Upper
pub const UPPER: BodyPart = BodyPart(BodyPart::Head.0 | BodyPart::Chest.0 | BodyPart::UpperArm.0);
// ACE: BodyParts.Mid
pub const MID: BodyPart = BodyPart(
    BodyPart::Chest.0
        | BodyPart::Abdomen.0
        | BodyPart::UpperArm.0
        | BodyPart::LowerArm.0
        | BodyPart::Hand.0
        | BodyPart::UpperLeg.0,
);
// ACE: BodyParts.Lower
pub const LOWER: BodyPart = BodyPart(BodyPart::Foot.0 | BodyPart::LowerLeg.0);

// ACE: BodyParts.BodyParts
/// `BodyParts.Indices`, built by the static constructor. These map to `CombatBodyPart`.
pub static INDICES: LazyLock<HashMap<BodyPart, i32>> = LazyLock::new(|| {
    HashMap::from([
        (BodyPart::Head, 0),
        (BodyPart::Chest, 1),
        (BodyPart::Abdomen, 2),
        (BodyPart::UpperArm, 3),
        (BodyPart::LowerArm, 4),
        (BodyPart::Hand, 5),
        (BodyPart::UpperLeg, 6),
        (BodyPart::LowerLeg, 7),
        (BodyPart::Foot, 8),
    ])
});

// ACE: BodyParts.GetBodyPart
/// `GetBodyPart(BodyPart bodyParts)`: a random part among the flags of `body_parts`.
///
/// # Panics
/// For no flags (ACE: `ArgumentOutOfRangeException`).
#[must_use]
pub fn get_body_part_of(body_parts: BodyPart) -> BodyPart {
    // get individual parts in bodyParts
    let parts = get_flags(body_parts);

    // return a random part within list
    let count = i32::try_from(parts.len()).unwrap_or(i32::MAX);
    parts[usize::try_from(ThreadSafeRandom::next(0, count - 1))
        .expect("System.ArgumentOutOfRangeException")]
}

// ACE: BodyParts.GetBodyPart
/// `GetBodyPart(WorldObject target, AttackHeight height)`: a random body part of a creature at
/// the attack height (`BH == height`); `None` for a non-creature or no such part.
#[must_use]
pub fn get_body_part_at(
    w: &World,
    target: ObjectGuid,
    height: AttackHeight,
) -> Option<PropertiesBodyPart> {
    let creature = w.objects.get(target).filter(|o| o.is_creature())?;

    // get all of the body parts for this creature
    // at this attack height
    let height_parts: Vec<&PropertiesBodyPart> = creature
        .biota
        .properties_body_part
        .as_ref()
        .expect("System.ArgumentNullException: Biota.PropertiesBodyPart")
        .values()
        .filter(|b| b.bh == height.0)
        .collect();
    if height_parts.is_empty() {
        return None;
    }

    // get random body part
    let count = i32::try_from(height_parts.len()).unwrap_or(i32::MAX);
    let rng = ThreadSafeRandom::next(0, count - 1);
    Some(height_parts[usize::try_from(rng).expect("index")].clone())
}

// ACE: BodyParts.GetBodyPart
/// `GetBodyPart(AttackHeight attackHeight)`: a random body part of the player hit table at the
/// height (anything but High and Medium is Low).
#[must_use]
pub fn get_body_part(attack_height: AttackHeight) -> BodyPart {
    match attack_height {
        AttackHeight::High => get_body_part_of(UPPER),
        AttackHeight::Medium => get_body_part_of(MID),
        _ => get_body_part_of(LOWER),
    }
}

const OUTER_UNDER_ABDOMEN: CoverageMask =
    CoverageMask(CoverageMask::OuterwearAbdomen.0 | CoverageMask::UnderwearAbdomen.0);
const OUTER_UNDER_CHEST: CoverageMask =
    CoverageMask(CoverageMask::OuterwearChest.0 | CoverageMask::UnderwearChest.0);
const OUTER_UNDER_LOWER_ARMS: CoverageMask =
    CoverageMask(CoverageMask::OuterwearLowerArms.0 | CoverageMask::UnderwearLowerArms.0);
const OUTER_UNDER_LOWER_LEGS: CoverageMask =
    CoverageMask(CoverageMask::OuterwearLowerLegs.0 | CoverageMask::UnderwearLowerLegs.0);
const OUTER_UNDER_UPPER_ARMS: CoverageMask =
    CoverageMask(CoverageMask::OuterwearUpperArms.0 | CoverageMask::UnderwearUpperArms.0);
const OUTER_UNDER_UPPER_LEGS: CoverageMask =
    CoverageMask(CoverageMask::OuterwearUpperLegs.0 | CoverageMask::UnderwearUpperLegs.0);

// ACE: BodyParts.GetCoverageMask
/// `GetCoverageMask(BodyPart)`: the clothing coverage of one body part; `Unknown` otherwise.
#[must_use]
pub fn get_coverage_mask(body_part: BodyPart) -> CoverageMask {
    match body_part {
        BodyPart::Abdomen => OUTER_UNDER_ABDOMEN,
        BodyPart::Chest => OUTER_UNDER_CHEST,
        BodyPart::Foot => CoverageMask::Feet,
        BodyPart::Hand => CoverageMask::Hands,
        BodyPart::Head => CoverageMask::Head,
        BodyPart::LowerArm => OUTER_UNDER_LOWER_ARMS,
        BodyPart::LowerLeg => OUTER_UNDER_LOWER_LEGS,
        BodyPart::UpperArm => OUTER_UNDER_UPPER_ARMS,
        BodyPart::UpperLeg => OUTER_UNDER_UPPER_LEGS,
        _ => CoverageMask::Unknown,
    }
}

// ACE: BodyParts.GetCoverageMask
/// `GetCoverageMask(CombatBodyPart)`: the clothing coverage of one body part; `Unknown` otherwise.
#[must_use]
pub fn get_coverage_mask_combat(body_part: CombatBodyPart) -> CoverageMask {
    match body_part {
        CombatBodyPart::Abdomen => OUTER_UNDER_ABDOMEN,
        CombatBodyPart::Chest => OUTER_UNDER_CHEST,
        CombatBodyPart::Foot => CoverageMask::Feet,
        CombatBodyPart::Hand => CoverageMask::Hands,
        CombatBodyPart::Head => CoverageMask::Head,
        CombatBodyPart::LowerArm => OUTER_UNDER_LOWER_ARMS,
        CombatBodyPart::LowerLeg => OUTER_UNDER_LOWER_LEGS,
        CombatBodyPart::UpperArm => OUTER_UNDER_UPPER_ARMS,
        CombatBodyPart::UpperLeg => OUTER_UNDER_UPPER_LEGS,
        _ => CoverageMask::Unknown,
    }
}

// ACE: BodyParts.GetFlags
/// `GetFlags(BodyPart)`: the declared members whose bits are all set, in `Enum.GetValues` order.
#[must_use]
pub fn get_flags(body_parts: BodyPart) -> Vec<BodyPart> {
    BodyPart::ALL
        .iter()
        .copied()
        .filter(|&p| body_parts.has_flag(p))
        .collect()
}

// ACE: BodyParts.GetFlags
/// `GetFlags(CoverageMask)`: the declared members except `Unknown` whose bits are all set, in
/// `Enum.GetValues` order.
#[must_use]
pub fn get_flags_coverage(coverage: CoverageMask) -> Vec<CoverageMask> {
    CoverageMask::ALL
        .iter()
        .copied()
        .filter(|&p| p != CoverageMask::Unknown && coverage_has_flag(coverage, p))
        .collect()
}

// ACE: BodyParts.HasAny
/// Whether `coverage` has every bit of any one of `flags`; `false` for `null`.
#[must_use]
pub fn has_any(coverage: Option<CoverageMask>, flags: &[CoverageMask]) -> bool {
    let Some(coverage) = coverage else {
        return false;
    };

    for &flag in flags {
        if coverage_has_flag(coverage, flag) {
            return true;
        }
    }
    false
}

/// `Enum.HasFlag` for `CoverageMask`: `(value & flag) == flag`.
fn coverage_has_flag(value: CoverageMask, flag: CoverageMask) -> bool {
    value.0 & flag.0 == flag.0
}
