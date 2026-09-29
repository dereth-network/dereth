// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/ObjectDescriptionFlag.cs
// @generated from ACE's `Source/ACE.Entity/Enum/ObjectDescriptionFlag.cs`; do not edit by hand

/// ACE enum `ObjectDescriptionFlag` (`[Flags]`), underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct ObjectDescriptionFlag(pub i32);

#[allow(non_upper_case_globals)]
impl ObjectDescriptionFlag {
    pub const None: Self = Self(0x0);
    pub const Openable: Self = Self(0x1);
    pub const Inscribable: Self = Self(0x2);
    pub const Stuck: Self = Self(0x4);
    pub const Player: Self = Self(0x8);
    pub const Attackable: Self = Self(0x10);
    pub const PlayerKiller: Self = Self(0x20);
    pub const HiddenAdmin: Self = Self(0x40);
    pub const UiHidden: Self = Self(0x80);
    pub const Book: Self = Self(0x100);
    pub const Vendor: Self = Self(0x200);
    pub const PkSwitch: Self = Self(0x400);
    pub const NpkSwitch: Self = Self(0x800);
    pub const Door: Self = Self(0x1000);
    pub const Corpse: Self = Self(0x2000);
    pub const LifeStone: Self = Self(0x4000);
    pub const Food: Self = Self(0x8000);
    pub const Healer: Self = Self(0x10000);
    pub const Lockpick: Self = Self(0x20000);
    pub const Portal: Self = Self(0x40000);
    pub const Admin: Self = Self(0x100000);
    pub const FreePkStatus: Self = Self(0x200000);
    pub const ImmuneCellRestrictions: Self = Self(0x400000);
    pub const RequiresPackSlot: Self = Self(0x800000);
    pub const Retained: Self = Self(0x1000000);
    pub const PkLiteStatus: Self = Self(0x2000000);
    pub const IncludesSecondHeader: Self = Self(0x4000000);
    pub const BindStone: Self = Self(0x8000000);
    pub const VolatileRare: Self = Self(0x10000000);
    pub const WieldOnUse: Self = Self(0x20000000);
    pub const WieldLeft: Self = Self(0x40000000);
}

impl ObjectDescriptionFlag {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::None, Self::Openable, Self::Inscribable, Self::Stuck, Self::Player, Self::Attackable, Self::PlayerKiller, Self::HiddenAdmin, Self::UiHidden, Self::Book, Self::Vendor, Self::PkSwitch, Self::NpkSwitch, Self::Door, Self::Corpse, Self::LifeStone, Self::Food, Self::Healer, Self::Lockpick, Self::Portal, Self::Admin, Self::FreePkStatus, Self::ImmuneCellRestrictions, Self::RequiresPackSlot, Self::Retained, Self::PkLiteStatus, Self::IncludesSecondHeader, Self::BindStone, Self::VolatileRare, Self::WieldOnUse, Self::WieldLeft];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["None", "Openable", "Inscribable", "Stuck", "Player", "Attackable", "PlayerKiller", "HiddenAdmin", "UiHidden", "Book", "Vendor", "PkSwitch", "NpkSwitch", "Door", "Corpse", "LifeStone", "Food", "Healer", "Lockpick", "Portal", "Admin", "FreePkStatus", "ImmuneCellRestrictions", "RequiresPackSlot", "Retained", "PkLiteStatus", "IncludesSecondHeader", "BindStone", "VolatileRare", "WieldOnUse", "WieldLeft"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[20, 5, 27, 9, 14, 13, 16, 21, 17, 7, 22, 26, 2, 15, 18, 0, 12, 1, 25, 11, 4, 6, 19, 23, 24, 3, 8, 10, 28, 30, 29];
}

super::support::ace_enum!(ObjectDescriptionFlag, i32, flags);
super::support::ace_enum_from!(ObjectDescriptionFlag, i32 => i64);
