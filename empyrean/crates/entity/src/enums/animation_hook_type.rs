// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/AnimationHookType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/AnimationHookType.cs`; do not edit by hand

/// ACE enum `AnimationHookType`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct AnimationHookType(pub i32);

#[allow(non_upper_case_globals)]
impl AnimationHookType {
    pub const Unknown: Self = Self(-1);
    pub const NoOp: Self = Self(0);
    pub const Sound: Self = Self(1);
    pub const SoundTable: Self = Self(2);
    pub const Attack: Self = Self(3);
    pub const AnimationDone: Self = Self(4);
    pub const ReplaceObject: Self = Self(5);
    pub const Ethereal: Self = Self(6);
    pub const TransparentPart: Self = Self(7);
    pub const Luminous: Self = Self(8);
    pub const LuminousPart: Self = Self(9);
    pub const Diffuse: Self = Self(10);
    pub const DiffusePart: Self = Self(11);
    pub const Scale: Self = Self(12);
    pub const CreateParticle: Self = Self(13);
    pub const DestroyParticle: Self = Self(14);
    pub const StopParticle: Self = Self(15);
    pub const NoDraw: Self = Self(16);
    pub const DefaultScript: Self = Self(17);
    pub const DefaultScriptPart: Self = Self(18);
    pub const CallPES: Self = Self(19);
    pub const Transparent: Self = Self(20);
    pub const SoundTweaked: Self = Self(21);
    pub const SetOmega: Self = Self(22);
    pub const TextureVelocity: Self = Self(23);
    pub const TextureVelocityPart: Self = Self(24);
    pub const SetLight: Self = Self(25);
    pub const CreateBlockingParticle: Self = Self(26);
    pub const ForceAnimationHook32Bit: Self = Self(0x7FFFFFFF);
}

impl AnimationHookType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::NoOp, Self::Sound, Self::SoundTable, Self::Attack, Self::AnimationDone, Self::ReplaceObject, Self::Ethereal, Self::TransparentPart, Self::Luminous, Self::LuminousPart, Self::Diffuse, Self::DiffusePart, Self::Scale, Self::CreateParticle, Self::DestroyParticle, Self::StopParticle, Self::NoDraw, Self::DefaultScript, Self::DefaultScriptPart, Self::CallPES, Self::Transparent, Self::SoundTweaked, Self::SetOmega, Self::TextureVelocity, Self::TextureVelocityPart, Self::SetLight, Self::CreateBlockingParticle, Self::ForceAnimationHook32Bit, Self::Unknown];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["NoOp", "Sound", "SoundTable", "Attack", "AnimationDone", "ReplaceObject", "Ethereal", "TransparentPart", "Luminous", "LuminousPart", "Diffuse", "DiffusePart", "Scale", "CreateParticle", "DestroyParticle", "StopParticle", "NoDraw", "DefaultScript", "DefaultScriptPart", "CallPES", "Transparent", "SoundTweaked", "SetOmega", "TextureVelocity", "TextureVelocityPart", "SetLight", "CreateBlockingParticle", "ForceAnimationHook32Bit", "Unknown"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[4, 3, 19, 26, 13, 17, 18, 14, 10, 11, 6, 27, 8, 9, 16, 0, 5, 12, 25, 22, 1, 2, 21, 15, 23, 24, 20, 7, 28];
}

super::support::ace_enum!(AnimationHookType, i32, plain);
super::support::ace_enum_from!(AnimationHookType, i32 => i64);
