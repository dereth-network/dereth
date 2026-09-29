// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/DamageHistoryEntry.cs
//! Port of `Source/ACE.Server/Entity/DamageHistoryEntry.cs`.

use empyrean_common::dotnet::DotNetDateTime;
use empyrean_entity::enums::DamageType;
use empyrean_entity::ObjectGuid;

use crate::world_objects::entity::creature_attribute::StatCtx;
use crate::World;

// ACE: DamageHistoryEntry
/// One damage (negative amount) or healing (positive amount) event.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DamageHistoryEntry {
    // ACE: DamageHistoryEntry.Attacker
    pub attacker: ObjectGuid,

    // ACE: DamageHistoryEntry.DamageType
    pub damage_type: DamageType,
    // ACE: DamageHistoryEntry.Amount
    pub amount: i32,

    // ACE: DamageHistoryEntry.CurrentHealth
    pub current_health: u32,
    // ACE: DamageHistoryEntry.MaxHealth
    pub max_health: u32,

    // ACE: DamageHistoryEntry.Time
    pub time: DotNetDateTime,
}

impl DamageHistoryEntry {
    // ACE: DamageHistoryEntry.DamageHistoryEntry
    /// Constructs a new entry for the DamageHistory. `attacker` is the creature source of the
    /// damage (for a projectile, the creature that launched it); `amount` is negative for damage
    /// taken, positive for healing. `DateTime.UtcNow` is the tick's `w.now.utc`.
    ///
    /// # Panics
    /// When `creature` is not a creature in the store.
    #[must_use]
    pub fn new(
        w: &mut World,
        creature: ObjectGuid,
        attacker: ObjectGuid,
        damage_type: DamageType,
        amount: i32,
    ) -> Self {
        let c = w
            .objects
            .get(creature)
            .expect("NullReferenceException: DamageHistoryEntry(creature)");
        let health = c.health();
        let current_health = health.current(c);

        Self {
            attacker,
            damage_type,
            amount,
            current_health,
            max_health: health.max_value(&mut StatCtx::in_world(w, creature)),
            time: w.now.utc,
        }
    }
}
