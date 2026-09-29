// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/DamageHistoryInfo.cs
//! Port of `Source/ACE.Server/Entity/DamageHistoryInfo.cs`.
//!
//! ACE's `WeakReference<WorldObject>`/`WeakReference<Player>` become guids resolved against
//! `World.objects` on use: an attacker gone from the store is a dead weak
//! reference.

use empyrean_entity::ObjectGuid;

use crate::World;

// ACE: DamageHistoryInfo
/// One damager of a creature: who, and how much damage in total.
#[derive(Debug, Clone, PartialEq)]
pub struct DamageHistoryInfo {
    // ACE: DamageHistoryInfo.Attacker
    /// The weak reference to the attacker (its guid).
    pub attacker: ObjectGuid,

    // ACE: DamageHistoryInfo.Guid
    pub guid: ObjectGuid,
    // ACE: DamageHistoryInfo.Name
    pub name: Option<String>,

    // ACE: DamageHistoryInfo.TotalDamage
    pub total_damage: f32,

    // ACE: DamageHistoryInfo.PetOwner
    /// The weak reference to a combat pet's owner (null for anything else).
    pub pet_owner: Option<ObjectGuid>,

    // ACE: DamageHistoryInfo.IsOlthoiPlayer
    pub is_olthoi_player: bool,
}

impl DamageHistoryInfo {
    // ACE: DamageHistoryInfo.DamageHistoryInfo
    /// `new DamageHistoryInfo(attacker, totalDamage = 0.0f)`.
    ///
    /// # Panics
    /// When `attacker` is not in the store (ACE: `NullReferenceException` on `attacker.Guid`).
    #[must_use]
    pub fn new(w: &World, attacker: ObjectGuid, total_damage: f32) -> Self {
        let o = w
            .objects
            .get(attacker)
            .expect("NullReferenceException: DamageHistoryInfo(attacker)");
        let is_player = o.is_player();
        let is_combat_pet = o.is_combat_pet();

        let guid = attacker;
        let name = crate::dispatch::name::name(w, attacker);

        let is_olthoi_player = is_player && player_is_olthoi_player(w, attacker);

        let mut pet_owner = None;
        if is_combat_pet {
            if let Some(owner) = combat_pet_p_pet_owner(w, attacker) {
                pet_owner = Some(owner);
            }
        }

        Self {
            attacker,
            guid,
            name,
            total_damage,
            pet_owner,
            is_olthoi_player,
        }
    }

    // ACE: DamageHistoryInfo.IsPlayer
    #[must_use]
    pub fn is_player(&self) -> bool {
        self.guid.is_player()
    }

    // ACE: DamageHistoryInfo.TryGetAttacker
    /// The attacker, if it is still alive.
    #[must_use]
    pub fn try_get_attacker(&self, w: &World) -> Option<ObjectGuid> {
        w.objects.get(self.attacker).map(|_| self.attacker)
    }

    // ACE: DamageHistoryInfo.TryGetPetOwner
    ///
    /// # Panics
    /// When there is no pet owner (ACE: `NullReferenceException` on the null `PetOwner`).
    #[must_use]
    pub fn try_get_pet_owner(&self, w: &World) -> Option<ObjectGuid> {
        let owner = self
            .pet_owner
            .expect("NullReferenceException: DamageHistoryInfo.PetOwner");
        w.objects.get(owner).map(|_| owner)
    }

    // ACE: DamageHistoryInfo.TryGetPetOwnerOrAttacker
    #[must_use]
    pub fn try_get_pet_owner_or_attacker(&self, w: &World) -> Option<ObjectGuid> {
        if self.pet_owner.is_some() {
            self.try_get_pet_owner(w)
        } else {
            self.try_get_attacker(w)
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Not ACE: pointers to members ported in other files, named after them.
// ---------------------------------------------------------------------------------------------

/// `Player.IsOlthoiPlayer` (`Player_Properties.cs`), as `Player.SetEphemeralValues` set it from
/// the heritage.
pub(crate) fn player_is_olthoi_player(w: &World, player: ObjectGuid) -> bool {
    w.objects
        .get(player)
        .and_then(|o| o.player.as_ref())
        .is_some_and(|p| p.player_properties.is_olthoi_player)
}

/// `CombatPet.P_PetOwner` (`Pet.cs`).
fn combat_pet_p_pet_owner(w: &World, pet: ObjectGuid) -> Option<ObjectGuid> {
    crate::world_objects::pet::p_pet_owner(w, pet)
}
