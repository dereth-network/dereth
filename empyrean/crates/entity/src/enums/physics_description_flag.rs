// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/PhysicsDescriptionFlag.cs
// @generated from ACE's `Source/ACE.Entity/Enum/PhysicsDescriptionFlag.cs`; do not edit by hand

/// ACE enum `PhysicsDescriptionFlag` (`[Flags]`), underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct PhysicsDescriptionFlag(pub i32);

#[allow(non_upper_case_globals)]
impl PhysicsDescriptionFlag {
    pub const None: Self = Self(0x0);
    pub const CSetup: Self = Self(0x1);
    pub const MTable: Self = Self(0x2);
    pub const Velocity: Self = Self(0x4);
    pub const Acceleration: Self = Self(0x8);
    pub const Omega: Self = Self(0x10);
    pub const Parent: Self = Self(0x20);
    pub const Children: Self = Self(0x40);
    pub const ObjScale: Self = Self(0x80);
    pub const Friction: Self = Self(0x100);
    pub const Elasticity: Self = Self(0x200);
    pub const Timestamps: Self = Self(0x400);
    pub const STable: Self = Self(0x800);
    pub const PeTable: Self = Self(0x1000);
    pub const DefaultScript: Self = Self(0x2000);
    pub const DefaultScriptIntensity: Self = Self(0x4000);
    pub const Position: Self = Self(0x8000);
    pub const Movement: Self = Self(0x10000);
    pub const AnimationFrame: Self = Self(0x20000);
    pub const Translucency: Self = Self(0x40000);
}

impl PhysicsDescriptionFlag {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::None, Self::CSetup, Self::MTable, Self::Velocity, Self::Acceleration, Self::Omega, Self::Parent, Self::Children, Self::ObjScale, Self::Friction, Self::Elasticity, Self::Timestamps, Self::STable, Self::PeTable, Self::DefaultScript, Self::DefaultScriptIntensity, Self::Position, Self::Movement, Self::AnimationFrame, Self::Translucency];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["None", "CSetup", "MTable", "Velocity", "Acceleration", "Omega", "Parent", "Children", "ObjScale", "Friction", "Elasticity", "Timestamps", "STable", "PeTable", "DefaultScript", "DefaultScriptIntensity", "Position", "Movement", "AnimationFrame", "Translucency"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[4, 18, 1, 7, 14, 15, 10, 9, 2, 17, 0, 8, 5, 6, 13, 16, 12, 11, 19, 3];
}

super::support::ace_enum!(PhysicsDescriptionFlag, i32, flags);
super::support::ace_enum_from!(PhysicsDescriptionFlag, i32 => i64);
