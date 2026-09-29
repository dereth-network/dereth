// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Monster_Tick.cs
//! Port of `Source/ACE.Server/WorldObjects/Monster_Tick.cs`: the monster think, run from the
//! landblock's `sortedCreaturesByNextTick` list every `monsterTickInterval`.

use empyrean_entity::enums::MotionStance;
use empyrean_entity::ObjectGuid;

use crate::dispatch;
use crate::physics::phys_ext;
use crate::world_objects::creature_combat::CombatType;
use crate::world_objects::creature_navigation::MONSTER_TICK_INTERVAL;
use crate::world_objects::monster::{self, State};
use crate::world_objects::monster_combat;
use crate::world_objects::world_object::WorldObject;
use crate::world_objects::{
    creature_equipment, monster_awareness, monster_inventory, monster_magic, monster_missile,
    monster_navigation,
};
use crate::World;

/// Non-property fields declared in `Monster_Tick.cs`.
#[derive(Debug)]
pub struct MonsterTickFields {
    // ACE: Creature.NextMonsterTickTime
    pub next_monster_tick_time: f64,
    // ACE: Creature.firstUpdate
    pub first_update: bool,
}

impl Default for MonsterTickFields {
    fn default() -> Self {
        // `private bool firstUpdate = true;`
        MonsterTickFields {
            next_monster_tick_time: 0.0,
            first_update: true,
        }
    }
}

/// `Creature`'s `Monster_Tick.cs` fields.
///
/// # Panics
/// When `this` is gone or not a creature.
#[must_use]
pub fn fields(w: &World, this: ObjectGuid) -> &MonsterTickFields {
    &w.objects
        .get(this)
        .and_then(|o| o.creature.as_ref())
        .expect("ACE: Creature is null (NullReferenceException)")
        .monster_tick
}

/// Mutable [`fields`].
///
/// # Panics
/// As [`fields`].
pub fn fields_mut(w: &mut World, this: ObjectGuid) -> &mut MonsterTickFields {
    &mut w
        .objects
        .get_mut(this)
        .and_then(|o| o.creature.as_mut())
        .expect("ACE: Creature is null (NullReferenceException)")
        .monster_tick
}

/// `creature.NextMonsterTickTime`, which the landblock's creature tick list is ordered by (0 for
/// an object that is not a creature, as its field would never be set).
#[must_use]
pub fn next_monster_tick_time(o: &WorldObject) -> f64 {
    o.creature
        .as_ref()
        .map_or(0.0, |c| c.monster_tick.next_monster_tick_time)
}

/// Primary dispatch for monster think.
// ACE: Creature.Monster_Tick
pub fn monster_tick(w: &mut World, this: ObjectGuid, current_unix_time: f64) {
    let (is_chess_piece, is_passive_pet) = {
        let m = monster::fields(w, this);
        (m.is_chess_piece, m.is_passive_pet)
    };
    let o = w.objects.get(this).expect("ACE: this");
    if is_chess_piece && o.is_game_piece() {
        // faster than virtual dispatch?
        game_piece_tick(w, this, current_unix_time);
        return;
    }

    if is_passive_pet && matches!(o.kind, crate::world_objects::kinds::KindData::Pet(_)) {
        pet_tick(w, this, current_unix_time);
        return;
    }

    fields_mut(w, this).next_monster_tick_time = current_unix_time + MONSTER_TICK_INTERVAL;

    if !monster_awareness::is_awake(w, this) {
        if monster::monster_state(w, this) == State::Return {
            monster::set_monster_state_value(w, this, State::Idle);
        }

        let m = monster::fields(w, this);
        if m.is_faction_mob || m.has_foe_type {
            monster_awareness::faction_mob_check_monsters(w, this);
        }

        return;
    }

    if monster_combat::is_dead(w.objects.get(this).expect("ACE: this")) {
        return;
    }

    if monster_awareness::emote_manager_is_busy(w, this) {
        return;
    }

    dispatch::handle_find_target::handle_find_target(w, this);

    monster_navigation::check_miss_home(w, this); // tickrate?

    if monster_combat::attack_target(w, this).is_none()
        && monster::monster_state(w, this) != State::Return
    {
        dispatch::sleep::sleep(w, this);
        return;
    }

    if monster::monster_state(w, this) == State::Return {
        monster_navigation::movement(w, this);
        return;
    }

    let combat_pet = w.objects.get(this).expect("ACE: this").is_combat_pet();

    let creature_target = monster_combat::attack_target_creature(w, this);

    if let Some(creature_target) = creature_target {
        let target_dead =
            monster_combat::is_dead(w.objects.get(creature_target).expect("resolved"));
        if target_dead
            || (!combat_pet
                && !crate::world_objects::world_object::is_visible_target(w, this, creature_target))
        {
            dispatch::find_next_target::find_next_target(w, this);
            return;
        }
    }

    if fields(w, this).first_update {
        if current_stance(w, this) == MotionStance::NonCombat {
            monster_combat::do_attack_stance(w, this);
        }

        let h = monster_navigation::physics_obj(w, this);
        if phys_ext::is_animating(w, h) {
            //PhysicsObj.ShowPendingMotions();
            phys_ext::update_object(w, h);
            return;
        }

        fields_mut(w, this).first_update = false;
    }

    // select a new weapon if missile launcher is out of ammo
    let weapon = creature_equipment::get_equipped_weapon(w, this, false);
    /*if (weapon != null && weapon.IsAmmoLauncher)
    {
        var ammo = GetEquippedAmmo();
        if (ammo == null)
            SwitchToMeleeAttack();
    }*/

    if weapon.is_none()
        && monster_combat::fields(w, this).current_attack == Some(CombatType::Missile)
    {
        monster_inventory::equip_inventory_items(w, this, true);
        monster_combat::do_attack_stance(w, this);
        monster_combat::fields_mut(w, this).current_attack = None;
    }

    // decide current type of attack
    if monster_combat::fields(w, this).current_attack.is_none() {
        // Not ACE's (retail, V336): each attack decision draws its own
        // closing distance.
        monster_navigation::fields_mut(w, this).closing_distance = None;

        let next = monster_combat::get_next_attack_type(w, this);
        monster_combat::fields_mut(w, this).current_attack = Some(next);
        let max_range = monster_combat::get_max_range(w, this);
        monster_combat::fields_mut(w, this).max_range = max_range;

        //if (CurrentAttack == AttackType.Magic)
        //MaxRange = MaxMeleeRange;   // FIXME: server position sync
    }

    if monster_navigation::is_sticky(w, this) {
        monster_navigation::update_position(w, this, false);
    }

    // get distance to target
    let target_dist = monster_navigation::get_distance_to_target(w, this);
    //Console.WriteLine($"{Name} ({Guid}) - Dist: {targetDist}");

    let attack_target = monster_combat::attack_target(w, this);
    let max_range = monster_combat::fields(w, this).max_range;
    let closes_to_range = monster_navigation::closes_to_range(w, this);
    if closes_to_range && monster_combat::fields(w, this).current_attack == Some(CombatType::Magic)
    {
        // Not ACE's (retail, V336): a caster whose target is beyond its
        // spell's range runs to a drawn distance within it (not a sticky chase that casts on the
        // way), then turns and casts from where it stopped, as a missile monster does below.
        let nav = monster_navigation::fields(w, this);
        if nav.is_turning || nav.is_moving {
            monster_navigation::movement(w, this);
        } else if !monster_navigation::is_in_closing_range(w, this, target_dist)
            || (!monster_navigation::is_facing(w, this, attack_target)
                && !monster_magic::is_self_cast(w, this))
        {
            monster_navigation::start_turn(w, this);
        } else if monster_combat::attack_ready(w, this) {
            // perform attack
            monster_combat::attack(w, this);
        }
    } else if monster_combat::fields(w, this).current_attack != Some(CombatType::Missile) {
        if target_dist > max_range
            || (!monster_navigation::is_facing(w, this, attack_target)
                && !monster_magic::is_self_cast(w, this))
        {
            // turn / move towards
            let nav = monster_navigation::fields(w, this);
            if !nav.is_turning && !nav.is_moving {
                monster_navigation::start_turn(w, this);
            } else {
                monster_navigation::movement(w, this);
            }
        } else {
            // perform attack
            if monster_combat::attack_ready(w, this) {
                monster_combat::attack(w, this);
            }
        }
    } else {
        let nav = monster_navigation::fields(w, this);
        if nav.is_turning || nav.is_moving {
            monster_navigation::movement(w, this);
            return;
        }

        if !monster_navigation::is_facing(w, this, attack_target) {
            monster_navigation::start_turn(w, this);
        } else if target_dist <= max_range
            || monster_navigation::is_in_closing_range(w, this, target_dist)
        {
            // perform attack
            if monster_combat::attack_ready(w, this) {
                monster_combat::attack(w, this);
            }
        } else if closes_to_range {
            // Not ACE's (retail, V336): a missile monster whose target is
            // beyond its range runs to a drawn distance within it, then turns and shoots; ACE's
            // switches to melee.
            monster_navigation::start_turn(w, this);
        } else {
            // monster switches to melee combat immediately,
            // if target is beyond max range?

            // should ranged mobs only get CurrentTargets within MaxRange?
            //Console.WriteLine($"{Name}.MissileAttack({AttackTarget.Name}): targetDist={targetDist}, MaxRange={MaxRange}, switching to melee");
            monster_missile::try_switch_to_melee_attack(w, this);
        }
    }

    // pets drawing aggro
    if combat_pet {
        combat_pet_pet_check_monsters(w, this);
    }
}

/// `CurrentMotionState.Stance` (ACE dereferences it without a check).
pub(crate) fn current_stance(w: &World, this: ObjectGuid) -> MotionStance {
    w.objects
        .get(this)
        .and_then(|o| o.wo.world_object_properties.current_motion_state.as_ref())
        .expect("ACE: CurrentMotionState is null (NullReferenceException)")
        .stance
}

/// `gamePiece.Tick(currentUnixTime)` (`GamePiece.cs`).
fn game_piece_tick(w: &mut World, this: ObjectGuid, current_unix_time: f64) {
    crate::world_objects::game_piece::tick(w, this, current_unix_time);
}

/// `pet.Tick(currentUnixTime)` (`Pet.cs`).
fn pet_tick(w: &mut World, this: ObjectGuid, current_unix_time: f64) {
    crate::world_objects::pet::tick(w, this, current_unix_time);
}

/// `combatPet.PetCheckMonsters()` (`Pet_Monster.cs`).
fn combat_pet_pet_check_monsters(w: &mut World, this: ObjectGuid) {
    crate::world_objects::pet_monster::pet_check_monsters(w, this);
}
