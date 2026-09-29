// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/WindupParams.cs
//! Port of `Source/ACE.Server/Entity/WindupParams.cs`.

use empyrean_common::dotnet::format;
use empyrean_entity::ObjectGuid;

use crate::World;

// ACE: WindupParams
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindupParams {
    pub target_guid: u32,
    pub spell_id: u32,
    //public bool BuiltInSpell;
    pub caster_item: Option<ObjectGuid>,
}

impl WindupParams {
    // ACE: WindupParams.WindupParams
    #[must_use]
    pub fn new(target_guid: u32, spell_id: u32, caster_item: Option<ObjectGuid>) -> Self {
        WindupParams {
            target_guid,
            spell_id,
            //BuiltInSpell = builtInSpell;
            caster_item,
        }
    }

    /// A caster item that is gone reads as ACE's `null` name (empty).
    // ACE: WindupParams.ToString
    #[must_use]
    pub fn to_string(&self, w: &World) -> String {
        let caster_item_name = self
            .caster_item
            .filter(|&c| w.objects.get(c).is_some())
            .and_then(|c| crate::dispatch::name::name(w, c))
            .unwrap_or_default();
        format!(
            "TargetGuid: {}, SpellID: {}, CasterItem: {}",
            format(self.target_guid, "X8"),
            self.spell_id,
            caster_item_name
        )
    }
}
