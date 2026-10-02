// Rules ported from ClassicACE (bDekaru), AGPL-3.0: Source/ACE.Server/Factories/Tables/Spells/WandSpells.cs
// @generated from ClassicACE's `Source/ACE.Server/Factories/Tables/Spells/WandSpells.cs`; do not edit by hand

//! The tables of ClassicACE's `WandSpells` under its Infiltration ruleset
//! (`Factories/Tables/Spells/WandSpells.cs`).

use empyrean_entity::enums::SpellId;

/// ClassicACE `WandSpells.wandSpells`, as its Infiltration ruleset sets it (`List<(SpellId, float)>`).
pub static WAND_SPELLS: [(SpellId, f32); 2] = [
    (SpellId::DefenderSelf1, 0.25),
    (SpellId::HermeticLinkSelf1, 0.50),
];
