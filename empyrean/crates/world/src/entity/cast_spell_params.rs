// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/CastSpellParams.cs
//! Port of `Source/ACE.Server/Entity/CastSpellParams.cs`.
//!
//! `CasterItem` and `Target` are guids; `ToString` resolves their names.

use empyrean_entity::enums::SpellFlags;
use empyrean_entity::ObjectGuid;

use crate::entity::spell::Spell;
use crate::World;

/// `Player.CastingPreCheckStatus` (declared in `Player_Magic.cs`, underlying `int`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(transparent)]
pub struct CastingPreCheckStatus(pub i32);

#[allow(non_upper_case_globals)]
impl CastingPreCheckStatus {
    pub const CastFailed: Self = Self(0);
    pub const InvalidPKStatus: Self = Self(1);
    pub const Success: Self = Self(2);

    /// `Enum.ToString()`: the member name, or the number for an undeclared value.
    #[must_use]
    pub fn to_dotnet_string(self) -> String {
        match self {
            Self::CastFailed => "CastFailed".to_owned(),
            Self::InvalidPKStatus => "InvalidPKStatus".to_owned(),
            Self::Success => "Success".to_owned(),
            Self(v) => v.to_string(),
        }
    }
}

// ACE: CastSpellParams
#[derive(Debug, Clone)]
pub struct CastSpellParams {
    // ACE: CastSpellParams.Spell
    pub spell: Spell,
    //public bool IsWeaponSpell { get; set; }
    // ACE: CastSpellParams.CasterItem
    pub caster_item: Option<ObjectGuid>,
    // ACE: CastSpellParams.MagicSkill
    pub magic_skill: u32,
    // ACE: CastSpellParams.ManaUsed
    pub mana_used: u32,
    // ACE: CastSpellParams.Target
    pub target: Option<ObjectGuid>,
    // ACE: CastSpellParams.Status
    pub status: CastingPreCheckStatus,
}

impl CastSpellParams {
    // ACE: CastSpellParams.CastSpellParams
    #[must_use]
    pub fn new(
        spell: Spell,
        caster_item: Option<ObjectGuid>,
        magic_skill: u32,
        mana_used: u32,
        target: Option<ObjectGuid>,
        status: CastingPreCheckStatus,
    ) -> Self {
        CastSpellParams {
            spell,
            caster_item,
            magic_skill,
            mana_used,
            target,
            status,
        }
    }

    /// `!Spell.Flags.HasFlag(SpellFlags.FastCast) && CasterItem == null && Spell.Formula.HasWindupGestures`.
    ///
    /// # Panics
    /// When the spell has no formula (ACE: `NullReferenceException`), if reached.
    // ACE: CastSpellParams.HasWindupGestures
    #[must_use]
    pub fn has_windup_gestures(&self) -> bool {
        (self.spell.flags() & SpellFlags::FastCast) != SpellFlags::FastCast
            && self.caster_item.is_none()
            && self.spell.formula_ref().has_windup_gestures()
    }

    /// `$"{Spell.Name}, {CasterItem?.Name}, {MagicSkill}, {ManaUsed}, {targetName}, {Status}"`.
    /// A guid that no longer resolves reads as ACE's `null`.
    // ACE: CastSpellParams.ToString
    #[must_use]
    pub fn to_string(&self, w: &World) -> String {
        let name_of = |g: ObjectGuid| crate::dispatch::name::name(w, g);
        let target_name = self
            .target
            .filter(|&t| w.objects.get(t).is_some())
            .map_or_else(|| "null".to_owned(), |t| name_of(t).unwrap_or_default());
        let caster_item_name = self
            .caster_item
            .filter(|&c| w.objects.get(c).is_some())
            .and_then(name_of)
            .unwrap_or_default();

        format!(
            "{}, {}, {}, {}, {}, {}",
            self.spell.name(),
            caster_item_name,
            self.magic_skill,
            self.mana_used,
            target_name,
            self.status.to_dotnet_string()
        )
    }
}
