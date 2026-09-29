// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Monster.cs
//! Port of `Source/ACE.Server/WorldObjects/Monster.cs`: the monster AI's cached classification
//! (`SetMonsterState`) and its exclusive state (`MonsterState`).
//!
//! The monster AI files (`Monster_*.cs`) are `partial class Creature`: each file's fields live in
//! its own `*Fields` struct inside `CreatureData`, reached through that module's `fields` /
//! `fields_mut`.

use empyrean_entity::enums::{TargetingTactic, WeenieType};
use empyrean_entity::ObjectGuid;

use crate::world_objects::world_object::WorldObject;
use crate::World;

/// The exclusive states the monster can be in.
// ACE: Creature.State
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum State {
    #[default]
    Idle,
    Awake,
    Return,
}

/// Non-property fields declared in `Monster.cs`.
#[derive(Debug, Default)]
pub struct MonsterFields {
    // ACE: Creature.IsMonster
    pub is_monster: bool,
    // ACE: Creature.IsChessPiece
    pub is_chess_piece: bool,
    // ACE: Creature.IsPassivePet
    pub is_passive_pet: bool,
    // ACE: Creature.IsFactionMob
    pub is_faction_mob: bool,
    // ACE: Creature.HasFoeType
    pub has_foe_type: bool,
    /// The exclusive state of the monster.
    // ACE: Creature.MonsterState
    pub monster_state: State,
}

/// `Creature`'s `Monster.cs` fields.
///
/// # Panics
/// When `this` is gone or not a creature (ACE only reaches these members on a live `Creature`).
#[must_use]
pub fn fields(w: &World, this: ObjectGuid) -> &MonsterFields {
    &w.objects
        .get(this)
        .and_then(|o| o.creature.as_ref())
        .expect("ACE: Creature is null (NullReferenceException)")
        .monster
}

/// Mutable [`fields`].
///
/// # Panics
/// As [`fields`].
pub fn fields_mut(w: &mut World, this: ObjectGuid) -> &mut MonsterFields {
    &mut w
        .objects
        .get_mut(this)
        .and_then(|o| o.creature.as_mut())
        .expect("ACE: Creature is null (NullReferenceException)")
        .monster
}

/// `MonsterState`.
#[must_use]
pub fn monster_state(w: &World, this: ObjectGuid) -> State {
    fields(w, this).monster_state
}

/// `MonsterState = value`.
pub fn set_monster_state_value(w: &mut World, this: ObjectGuid, value: State) {
    fields_mut(w, this).monster_state = value;
}

/// Determines if this creature runs combat ai, and caches into IsMonster. It reads and writes only
/// its own object, so it takes the object (the constructor runs it before the store has it).
// ACE: Creature.SetMonsterState
pub fn set_monster_state(o: &mut WorldObject) {
    if o.is_player() {
        return;
    }

    let weenie_type = o.biota.weenie_type;
    let is_passive_pet = weenie_type == WeenieType::Pet;
    let is_chess_piece = weenie_type == WeenieType::GamePiece;

    // includes CombatPets
    let is_monster = o.attackable() || o.targeting_tactic() != TargetingTactic::None;

    let is_faction_mob =
        is_monster && weenie_type != WeenieType::CombatPet && o.faction1_bits().is_some();

    let has_foe_type = is_monster && o.foe_type().is_some();

    let m = &mut o
        .creature
        .as_mut()
        .expect("ACE: SetMonsterState is a Creature member")
        .monster;
    m.is_passive_pet = is_passive_pet;
    m.is_chess_piece = is_chess_piece;
    m.is_monster = is_monster;
    m.is_faction_mob = is_faction_mob;
    m.has_foe_type = has_foe_type;
}
