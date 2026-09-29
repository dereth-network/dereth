// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/CastQueue.cs
//! Port of `Source/ACE.Server/Entity/CastQueue.cs`.

use empyrean_entity::ObjectGuid;

/// questionable if this was ever in retail
// ACE: CastQueue
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CastQueue {
    pub type_: CastQueueType,
    pub target_guid: u32,
    pub spell_id: u32,
    //public bool BuiltInSpell;
    pub caster_item: Option<ObjectGuid>,
}

impl CastQueue {
    // ACE: CastQueue.CastQueue
    #[must_use]
    pub fn new(
        type_: CastQueueType,
        target_guid: u32,
        spell_id: u32,
        caster_item: Option<ObjectGuid>,
    ) -> Self {
        CastQueue {
            type_,
            target_guid,
            spell_id,
            //BuiltInSpell = builtInSpell;
            caster_item,
        }
    }
}

// ACE: CastQueueType
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CastQueueType {
    Targeted,
    Untargeted,
}
