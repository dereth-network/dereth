// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/ItemType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/ItemType.cs`; do not edit by hand

/// ACE enum `ItemType` (`[Flags]`), underlying `uint`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct ItemType(pub u32);

#[allow(non_upper_case_globals)]
impl ItemType {
    pub const None: Self = Self(0x0);
    pub const MeleeWeapon: Self = Self(0x1);
    pub const Armor: Self = Self(0x2);
    pub const Clothing: Self = Self(0x4);
    pub const Jewelry: Self = Self(0x8);
    pub const Creature: Self = Self(0x10);
    pub const Food: Self = Self(0x20);
    pub const Money: Self = Self(0x40);
    pub const Misc: Self = Self(0x80);
    pub const MissileWeapon: Self = Self(0x100);
    pub const Container: Self = Self(0x200);
    pub const Useless: Self = Self(0x400);
    pub const Gem: Self = Self(0x800);
    pub const SpellComponents: Self = Self(0x1000);
    pub const Writable: Self = Self(0x2000);
    pub const Key: Self = Self(0x4000);
    pub const Caster: Self = Self(0x8000);
    pub const Portal: Self = Self(0x10000);
    pub const Lockable: Self = Self(0x20000);
    pub const PromissoryNote: Self = Self(0x40000);
    pub const ManaStone: Self = Self(0x80000);
    pub const Service: Self = Self(0x100000);
    pub const MagicWieldable: Self = Self(0x200000);
    pub const CraftCookingBase: Self = Self(0x400000);
    pub const CraftAlchemyBase: Self = Self(0x800000);
    /// DIVERGE: the retail client's value, not ACE's (V327).
    pub const CraftFletchingBase: Self = Self(0x1000000);
    pub const CraftAlchemyIntermediate: Self = Self(0x4000000);
    pub const CraftFletchingIntermediate: Self = Self(0x8000000);
    pub const LifeStone: Self = Self(0x10000000);
    pub const TinkeringTool: Self = Self(0x20000000);
    pub const TinkeringMaterial: Self = Self(0x40000000);
    pub const Gameboard: Self = Self(0x80000000);
    pub const PortalMagicTarget: Self = Self(0x10010000);
    pub const LockableMagicTarget: Self = Self(0x280);
    pub const Vestements: Self = Self(0x6);
    pub const Weapon: Self = Self(0x101);
    pub const WeaponOrCaster: Self = Self(0x8101);
    pub const Item: Self = Self(0x2DFBEF);
    pub const RedirectableItemEnchantmentTarget: Self = Self(0x8107);
    pub const ItemEnchantableTarget: Self = Self(0x88B8F);
    pub const VendorShopKeep: Self = Self(0x480467A7);
    pub const VendorGrocer: Self = Self(0x446220);
}

impl ItemType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::None, Self::MeleeWeapon, Self::Armor, Self::Clothing, Self::Vestements, Self::Jewelry, Self::Creature, Self::Food, Self::Money, Self::Misc, Self::MissileWeapon, Self::Weapon, Self::Container, Self::LockableMagicTarget, Self::Useless, Self::Gem, Self::SpellComponents, Self::Writable, Self::Key, Self::Caster, Self::WeaponOrCaster, Self::RedirectableItemEnchantmentTarget, Self::Portal, Self::Lockable, Self::PromissoryNote, Self::ManaStone, Self::ItemEnchantableTarget, Self::Service, Self::MagicWieldable, Self::Item, Self::CraftCookingBase, Self::VendorGrocer, Self::CraftAlchemyBase, Self::CraftFletchingBase, Self::CraftAlchemyIntermediate, Self::CraftFletchingIntermediate, Self::LifeStone, Self::PortalMagicTarget, Self::TinkeringTool, Self::TinkeringMaterial, Self::VendorShopKeep, Self::Gameboard];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["None", "MeleeWeapon", "Armor", "Clothing", "Vestements", "Jewelry", "Creature", "Food", "Money", "Misc", "MissileWeapon", "Weapon", "Container", "LockableMagicTarget", "Useless", "Gem", "SpellComponents", "Writable", "Key", "Caster", "WeaponOrCaster", "RedirectableItemEnchantmentTarget", "Portal", "Lockable", "PromissoryNote", "ManaStone", "ItemEnchantableTarget", "Service", "MagicWieldable", "Item", "CraftCookingBase", "VendorGrocer", "CraftAlchemyBase", "CraftFletchingBase", "CraftAlchemyIntermediate", "CraftFletchingIntermediate", "LifeStone", "PortalMagicTarget", "TinkeringTool", "TinkeringMaterial", "VendorShopKeep", "Gameboard"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[2, 19, 3, 12, 32, 34, 30, 33, 35, 6, 7, 41, 15, 29, 26, 5, 18, 36, 23, 13, 28, 25, 1, 9, 10, 8, 0, 22, 37, 24, 21, 27, 16, 39, 38, 14, 31, 40, 4, 11, 20, 17];
}

super::support::ace_enum!(ItemType, u32, flags);
super::support::ace_enum_from!(ItemType, u32 => u64, i64);
