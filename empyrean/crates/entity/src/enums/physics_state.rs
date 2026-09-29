// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/PhysicsState.cs
// @generated from ACE's `Source/ACE.Entity/Enum/PhysicsState.cs`; do not edit by hand

/// ACE enum `PhysicsState` (`[Flags]`), underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct PhysicsState(pub i32);

#[allow(non_upper_case_globals)]
impl PhysicsState {
    pub const Static: Self = Self(0x1);
    pub const Unused1: Self = Self(0x2);
    pub const Ethereal: Self = Self(0x4);
    pub const ReportCollisions: Self = Self(0x8);
    pub const IgnoreCollisions: Self = Self(0x10);
    pub const NoDraw: Self = Self(0x20);
    pub const Missile: Self = Self(0x40);
    pub const Pushable: Self = Self(0x80);
    pub const AlignPath: Self = Self(0x100);
    pub const PathClipped: Self = Self(0x200);
    pub const Gravity: Self = Self(0x400);
    pub const LightingOn: Self = Self(0x800);
    pub const ParticleEmitter: Self = Self(0x1000);
    pub const Unused2: Self = Self(0x2000);
    pub const Hidden: Self = Self(0x4000);
    pub const ScriptedCollision: Self = Self(0x8000);
    pub const HasPhysicsBSP: Self = Self(0x10000);
    pub const Inelastic: Self = Self(0x20000);
    pub const HasDefaultAnim: Self = Self(0x40000);
    pub const HasDefaultScript: Self = Self(0x80000);
    pub const Cloaked: Self = Self(0x100000);
    pub const ReportCollisionsAsEnvironment: Self = Self(0x200000);
    pub const EdgeSlide: Self = Self(0x400000);
    pub const Sledding: Self = Self(0x800000);
    pub const Frozen: Self = Self(0x1000000);
}

impl PhysicsState {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Static, Self::Unused1, Self::Ethereal, Self::ReportCollisions, Self::IgnoreCollisions, Self::NoDraw, Self::Missile, Self::Pushable, Self::AlignPath, Self::PathClipped, Self::Gravity, Self::LightingOn, Self::ParticleEmitter, Self::Unused2, Self::Hidden, Self::ScriptedCollision, Self::HasPhysicsBSP, Self::Inelastic, Self::HasDefaultAnim, Self::HasDefaultScript, Self::Cloaked, Self::ReportCollisionsAsEnvironment, Self::EdgeSlide, Self::Sledding, Self::Frozen];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Static", "Unused1", "Ethereal", "ReportCollisions", "IgnoreCollisions", "NoDraw", "Missile", "Pushable", "AlignPath", "PathClipped", "Gravity", "LightingOn", "ParticleEmitter", "Unused2", "Hidden", "ScriptedCollision", "HasPhysicsBSP", "Inelastic", "HasDefaultAnim", "HasDefaultScript", "Cloaked", "ReportCollisionsAsEnvironment", "EdgeSlide", "Sledding", "Frozen"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[8, 20, 22, 2, 24, 10, 18, 19, 16, 14, 4, 17, 11, 6, 5, 12, 9, 7, 3, 21, 15, 23, 0, 1, 13];
}

super::support::ace_enum!(PhysicsState, i32, flags);
super::support::ace_enum_from!(PhysicsState, i32 => i64);
