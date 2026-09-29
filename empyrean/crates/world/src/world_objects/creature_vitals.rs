// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Creature_Vitals.cs
//! Port of `Source/ACE.Server/WorldObjects/Creature_Vitals.cs`.
//!
//! The `Vitals` dictionary holds the live [`CreatureVital`]s; `Health`, `Stamina` and `Mana`
//! return copies (see `creature_vital.rs`). Members that change another object, send messages or
//! run virtual calls are free functions over `(w, this)`; the pure getters are `WorldObject`
//! methods.

use empyrean_common::dotnet::{CsCast, DotNetDict};
use empyrean_entity::enums::{CombatMode, DamageType, MotionCommand, PropertyAttribute2nd};
use empyrean_entity::ObjectGuid;

use crate::dispatch;
use crate::world_objects::entity::creature_attribute::{em, int_to_f32, StatCtx};
use crate::world_objects::entity::creature_vital::CreatureVital;
use crate::world_objects::world_object::WorldObject;
use crate::World;

/// Non-property fields declared in `Creature_Vitals.cs`.
#[derive(Debug, Default)]
pub struct CreatureVitalsFields {
    // ACE: Creature.Vitals
    /// Filled by the constructor (`Creature.SetEphemeralValues`) with the three max vitals.
    pub vitals: DotNetDict<PropertyAttribute2nd, CreatureVital>,
}

impl WorldObject {
    /// `Creature.Vitals`.
    ///
    /// # Panics
    /// When this object is not a Creature.
    #[must_use]
    pub fn vitals(&self) -> &DotNetDict<PropertyAttribute2nd, CreatureVital> {
        &self
            .creature
            .as_ref()
            .expect("Creature.Vitals on an object that is not a Creature")
            .creature_vitals
            .vitals
    }

    /// `Creature.Vitals`, for the constructor and the runtime fields.
    ///
    /// # Panics
    /// When this object is not a Creature.
    pub fn vitals_mut(&mut self) -> &mut DotNetDict<PropertyAttribute2nd, CreatureVital> {
        &mut self
            .creature
            .as_mut()
            .expect("Creature.Vitals on an object that is not a Creature")
            .creature_vitals
            .vitals
    }

    /// The live `Vitals[vital]` (its `RegenRate` and `PartialRegen` are written through this).
    ///
    /// # Panics
    /// When absent (`KeyNotFoundException`).
    pub fn vital_mut(&mut self, vital: PropertyAttribute2nd) -> &mut CreatureVital {
        self.vitals_mut().get_mut(&vital).unwrap_or_else(|| {
            panic!(
                "KeyNotFoundException: Creature.Vitals[{}]",
                vital.to_dotnet_string()
            )
        })
    }

    /// `Vitals[vital]`: a copy of the live entry.
    fn vital_of(&self, vital: PropertyAttribute2nd) -> CreatureVital {
        self.vitals().get(&vital).copied().unwrap_or_else(|| {
            panic!(
                "KeyNotFoundException: Creature.Vitals[{}]",
                vital.to_dotnet_string()
            )
        })
    }

    // ACE: Creature.Health
    #[must_use]
    pub fn health(&self) -> CreatureVital {
        self.vital_of(PropertyAttribute2nd::MaxHealth)
    }

    // ACE: Creature.Stamina
    #[must_use]
    pub fn stamina(&self) -> CreatureVital {
        self.vital_of(PropertyAttribute2nd::MaxStamina)
    }

    // ACE: Creature.Mana
    #[must_use]
    pub fn mana(&self) -> CreatureVital {
        self.vital_of(PropertyAttribute2nd::MaxMana)
    }

    // ACE: Creature.GetCreatureVital
    /// The max vital for a *current* vital id (`Health` gives `Health`, i.e. `MaxHealth`); `None`
    /// (and an error log) for anything else.
    #[must_use]
    pub fn get_creature_vital(&self, vital: PropertyAttribute2nd) -> Option<CreatureVital> {
        match vital {
            PropertyAttribute2nd::Health => Some(self.health()),
            PropertyAttribute2nd::Stamina => Some(self.stamina()),
            PropertyAttribute2nd::Mana => Some(self.mana()),
            _ => {
                log::error!(
                    "{}.GetCreatureVital({}): unexpected vital",
                    self.get_property(empyrean_entity::enums::PropertyString::Name)
                        .unwrap_or_default(),
                    vital.to_dotnet_string()
                );
                None
            }
        }
    }

    // ACE: Creature.GetAttributeMod
    /// The health regeneration bonus a player gets from strength and endurance: `1 + (str + 2 *
    /// end - 200) / 600`, capped at 2.1; 1.0 for other vitals and for non-players.
    #[must_use]
    pub fn get_attribute_mod(&self, vital: CreatureVital) -> f32 {
        // only applies to players
        if !self.is_player() {
            return 1.0f32;
        }

        // only applies for health?
        if vital.vital != PropertyAttribute2nd::MaxHealth {
            return 1.0f32;
        }

        // The combination of strength and endurance (with endurance being more important) allows one to regenerate hit points
        // at a faster rate the higher one's endurance is. This bonus is in addition to any regeneration spells one may have placed upon themselves.
        // This regeneration bonus caps at around 110%.

        let strength: i32 = self.strength().base(self).cs_cast();
        let endurance: i32 = self.endurance().base(self).cs_cast();

        let str_and_end = strength.wrapping_add(endurance.wrapping_mul(2));

        //var modifier = 1.0 + (0.0494 * Math.Pow(strAndEnd, 1.179) / 100.0f);    // formula deduced from values present in the client pdb
        //var attributeMod = Math.Clamp(modifier, 1.0, 2.1);      // cap between + 0-110%

        if str_and_end <= 200 {
            return 1.0f32;
        }

        let modifier = 1.0f32 + int_to_f32(str_and_end.wrapping_sub(200)) / 600.0f32;

        math_clamp_f32(modifier, 1.0f32, 2.1f32)
    }

    // ACE: Creature.ForwardCommand
    /// `CurrentMovementData.Invalid.State.ForwardCommand` when the current movement is
    /// `MovementType.Invalid` with an `Invalid` state, else `MotionCommand.Invalid`.
    fn forward_command(&self) -> MotionCommand {
        let d = &self.wo.world_object_properties.current_movement_data;
        match &d.invalid {
            Some(invalid) if d.movement_type == empyrean_entity::enums::MovementType::Invalid => {
                invalid.state.forward_command
            }
            _ => MotionCommand::Invalid,
        }
    }

    // ACE: Creature.GetStanceMod
    /// The player's regeneration modifier by stance: 0.5 in combat mode or running, 2.0
    /// crouching, 2.5 sitting, 3.0 sleeping, else 1.0 (mana and non-players: 1.0).
    #[must_use]
    pub fn get_stance_mod(&self, vital: CreatureVital) -> f32 {
        // only applies to players
        if !self.is_player() {
            return 1.0f32;
        }

        // does not apply for mana?
        if vital.vital == PropertyAttribute2nd::MaxMana {
            return 1.0f32;
        }

        let forward_command = self.forward_command();

        // combat mode / running
        if creature_combat_mode(self) != CombatMode::NonCombat
            || forward_command == MotionCommand::RunForward
        {
            return 0.5f32;
        }

        match forward_command {
            // TODO: verify multipliers
            MotionCommand::Crouch => 2.0f32,
            MotionCommand::Sitting => 2.5f32,
            MotionCommand::Sleeping => 3.0f32,
            _ => 1.0f32,
        }
    }
}

/// `Math.Clamp(float, float, float)` (min <= max here).
fn math_clamp_f32(value: f32, min: f32, max: f32) -> f32 {
    if value < min {
        min
    } else if value > max {
        max
    } else {
        value
    }
}

/// `Creature.IsDead` (`Monster_Combat.cs`): `Health.Current <= 0`.
fn is_dead(o: &WorldObject) -> bool {
    o.health().current(o) == 0
}

/// `Creature.CombatMode` (`Creature_Combat.cs`).
fn creature_combat_mode(o: &WorldObject) -> CombatMode {
    crate::world_objects::creature_combat::fields(o).combat_mode
}

// ACE: Creature.SetMaxVitals
/// Fills health, stamina and mana, and tells the damage history about the healed amount.
pub fn creature_set_max_vitals(w: &mut World, this: ObjectGuid) {
    let (health, stamina, mana, missing_health, max) = {
        let o = w
            .objects
            .get(this)
            .expect("Creature.SetMaxVitals: missing object");
        let (health, stamina, mana) = (o.health(), o.stamina(), o.mana());
        let c = &mut StatCtx::in_world(w, this);
        let missing_health = health.missing(c);
        let max = (health.max_value(c), stamina.max_value(c), mana.max_value(c));
        (health, stamina, mana, missing_health, max)
    };

    let o = w
        .objects
        .get_mut(this)
        .expect("Creature.SetMaxVitals: missing object");
    health.set_current(o, max.0);
    stamina.set_current(o, max.1);
    mana.set_current(o, max.2);

    damage_history_on_heal(w, this, missing_health);
}

// ACE: Creature.UpdateVital
/// Sets the current vital to `new_val` clamped to `[0, MaxValue]`; returns the actual change.
pub fn creature_update_vital(
    w: &mut World,
    this: ObjectGuid,
    vital: CreatureVital,
    new_val: i32,
) -> i32 {
    let before = vital.current(
        w.objects
            .get(this)
            .expect("Creature.UpdateVital: missing object"),
    );
    let max = vital.max_value(&mut StatCtx::in_world(w, this));

    // `Math.Clamp(int, int, uint)` binds to the `long` overload
    let clamped: u32 = i64::from(new_val).clamp(0, i64::from(max)).cs_cast();

    let o = w
        .objects
        .get_mut(this)
        .expect("Creature.UpdateVital: missing object");
    vital.set_current(o, clamped);
    let after = vital.current(o);

    after.wrapping_sub(before).cs_cast()
}

// ACE: Creature.UpdateVital
/// The `uint` overload: `UpdateVital(vital, (int)newVal)`, virtually.
pub fn creature_update_vital_uint(
    w: &mut World,
    this: ObjectGuid,
    vital: CreatureVital,
    new_val: u32,
) -> i32 {
    dispatch::update_vital::update_vital(w, this, vital, new_val.cs_cast())
}

// ACE: Creature.UpdateVitalDelta
/// Updates a vital relative to its current value (the virtual `UpdateVital`).
pub fn update_vital_delta(
    w: &mut World,
    this: ObjectGuid,
    vital: CreatureVital,
    delta: i32,
) -> i32 {
    let current: i32 = {
        let o = w
            .objects
            .get(this)
            .expect("Creature.UpdateVitalDelta: missing object");
        vital.current(o).cs_cast()
    };
    let new_vital = current.wrapping_add(delta);

    dispatch::update_vital::update_vital(w, this, vital, new_vital)
}

// ACE: Creature.UpdateVitalDelta
/// The `uint` overload: `UpdateVitalDelta(vital, (int)delta)`.
pub fn update_vital_delta_uint(
    w: &mut World,
    this: ObjectGuid,
    vital: CreatureVital,
    delta: u32,
) -> i32 {
    update_vital_delta(w, this, vital, delta.cs_cast())
}

// ACE: Creature.VitalHeartBeat
/// Called every ~5 seconds to regenerate vitals; true if health changed.
pub fn creature_vital_heart_beat(w: &mut World, this: ObjectGuid) -> bool {
    let (dead, health, stamina, mana) = {
        let o = w
            .objects
            .get(this)
            .expect("Creature.VitalHeartBeat: missing object");
        (is_dead(o), o.health(), o.stamina(), o.mana())
    };
    if dead {
        return false;
    }

    let mut vital_update = false;

    vital_update |= vital_heart_beat(w, this, health.vital);

    vital_update |= vital_heart_beat(w, this, stamina.vital);

    vital_update |= vital_heart_beat(w, this, mana.vital);

    vital_update
}

/// The creature of `VitalHeartBeat`.
fn heart_beat_obj(w: &World, this: ObjectGuid) -> &WorldObject {
    w.objects
        .get(this)
        .expect("Creature.VitalHeartBeat: missing object")
}

// ACE: Creature.VitalHeartBeat
/// `VitalHeartBeat(CreatureVital vital)`: updates one vital by its regeneration rate, carrying
/// the fractional part to the next tick.
/// Returns true when the vital changed. Not ACE's (a fix, V259): ACE's
/// `return true` sits inside the health-only branch, so a stamina or mana tick that changed the
/// vital returned false and fellows' stamina and mana bars went stale during regeneration. A
/// health tick still answers true even when clamped at zero, as ACE's does.
pub fn vital_heart_beat(w: &mut World, this: ObjectGuid, vital_key: PropertyAttribute2nd) -> bool {
    let (vital, vital_current, vital_max, attribute_mod, stance_mod, enchantment_mod, aug_mod) = {
        let vital = heart_beat_obj(w, this)
            .vitals()
            .get(&vital_key)
            .copied()
            .expect("Creature.VitalHeartBeat: vital");

        // Current and MaxValue are properties and include overhead in getting their values. We cache them so we only hit the overhead once.
        let vital_current = vital.current(heart_beat_obj(w, this));
        let vital_max = vital.max_value(&mut StatCtx::in_world(w, this));

        if vital_current == vital_max && vital.regen_rate > 0.0 {
            return false;
        }

        if vital_current > vital_max {
            (vital, vital_current, vital_max, None, 0.0, 0.0, 0.0)
        } else {
            if vital.regen_rate == 0.0 {
                return false;
            }

            // take attributes into consideration (strength, endurance)
            let attribute_mod = heart_beat_obj(w, this).get_attribute_mod(vital);

            // take stance into consideration (combat, crouch, sitting, sleeping)
            let stance_mod = heart_beat_obj(w, this).get_stance_mod(vital);

            // take enchantments into consideration:
            // (regeneration / rejuvenation / mana renewal / etc.)
            let enchantment_mod = em::get_regeneration_mod(w, this, vital);

            let o = heart_beat_obj(w, this);
            let mut aug_mod = 1.0f32;
            if o.is_player()
                && o.augmentation_faster_regen() > 0
                && o.forward_command() == MotionCommand::Sleeping
            {
                aug_mod += int_to_f32(o.augmentation_faster_regen());
            }

            (
                vital,
                vital_current,
                vital_max,
                Some(attribute_mod),
                stance_mod,
                enchantment_mod,
                aug_mod,
            )
        }
    };

    let Some(attribute_mod) = attribute_mod else {
        // vitalCurrent > vitalMax
        let _ = vital_current;
        dispatch::update_vital::update_vital_uint(w, this, vital, vital_max);
        return true;
    };

    // cap rate?
    let mut current_tick = vital.regen_rate
        * f64::from(attribute_mod)
        * f64::from(stance_mod)
        * f64::from(enchantment_mod)
        * f64::from(aug_mod);

    // Not ACE's (the retail captures, V280): a player's health step is the per-5-s
    // step times the seconds since its previous heartbeat over 5 (its heartbeat comes every 4 to
    // 6 s); stamina and mana keep a fixed step per tick, as retail's did. Other creatures keep ACE's.
    if vital_key == PropertyAttribute2nd::MaxHealth {
        current_tick *=
            crate::world_objects::player_tick::health_regen_scale(heart_beat_obj(w, this));
    }

    // add in partially accumulated / rounded vitals from previous tick(s)
    let total_tick = current_tick + vital.partial_regen;

    // accumulate partial vital rates between ticks
    let int_tick: i32 = total_tick.cs_cast();
    let partial_regen = total_tick - f64::from(int_tick);
    w.objects
        .get_mut(this)
        .expect("Creature.VitalHeartBeat: missing object")
        .vital_mut(vital_key)
        .partial_regen = partial_regen;

    if int_tick != 0 {
        //if (this is Player)
        //Console.WriteLine($"VitalTick({vital.Vital.ToSentence()}): attributeMod={attributeMod}, stanceMod={stanceMod}, enchantmentMod={enchantmentMod}, regenRate={vital.RegenRate}, currentTick={currentTick}, totalTick={totalTick}, accumulated={vital.PartialRegen}");

        let applied = update_vital_delta(w, this, vital, int_tick);
        if vital.vital == PropertyAttribute2nd::MaxHealth {
            if int_tick > 0 {
                damage_history_on_heal(w, this, int_tick.cs_cast());
            } else {
                damage_history_add(w, this, DamageType::Health, int_tick.unsigned_abs());

                let health_current = {
                    let o = w
                        .objects
                        .get(this)
                        .expect("Creature.VitalHeartBeat: missing object");
                    o.health().current(o)
                };
                if health_current == 0 {
                    let last_damager = crate::entity::damage_history::of(w, this).last_damager();
                    dispatch::on_death::on_death(w, this, last_damager, DamageType::Health, false);
                    crate::world_objects::creature_death::die(w, this);
                }
            }
            return true;
        }
        return applied != 0;
    }
    false
}

// ---------------------------------------------------------------------------------------------
// Not ACE: the stat statements of `Creature.SetEphemeralValues` (Creature.cs, ported as
// `creature_set_ephemeral_values`), as two calls for that function to make in ACE's order.
// ---------------------------------------------------------------------------------------------

/// The stat block of `Creature.SetEphemeralValues`, in its order: the three `Vitals` entries
/// (`new CreatureVital`, which add missing biota records), the six `Attributes`, a `Skills`
/// entry for every biota skill, then `if (X.Current <= 0) X.Current = X.MaxValue;` for health,
/// stamina and mana. `w` supplies the dats (the object is not in the store yet).
pub fn set_ephemeral_stat_values(w: &World, o: &mut WorldObject) {
    use empyrean_entity::enums::PropertyAttribute;

    use crate::world_objects::entity::creature_attribute::CreatureAttribute;
    use crate::world_objects::entity::creature_skill::CreatureSkill;

    // If any of the vitals don't exist for this biota, one will be created automatically in the CreatureVital ctor
    for v in [
        PropertyAttribute2nd::MaxHealth,
        PropertyAttribute2nd::MaxStamina,
        PropertyAttribute2nd::MaxMana,
    ] {
        let cv = CreatureVital::new(o, v);
        o.vitals_mut().insert(v, cv);
    }

    // If any of the attributes don't exist for this biota, one will be created automatically in the CreatureAttribute ctor
    for a in [
        PropertyAttribute::Strength,
        PropertyAttribute::Endurance,
        PropertyAttribute::Coordination,
        PropertyAttribute::Quickness,
        PropertyAttribute::Focus,
        PropertyAttribute::Self_,
    ] {
        let ca = CreatureAttribute::new(o, a);
        o.attributes_mut().insert(a, ca);
    }

    let skills: Vec<_> = o
        .biota
        .properties_skill
        .as_ref()
        .map(|d| d.keys().copied().collect())
        .unwrap_or_default();
    for skill in skills {
        o.skills_mut().insert(skill, CreatureSkill::new(skill));
    }

    for vital in [o.health(), o.stamina(), o.mana()] {
        if vital.current(o) == 0 {
            let max = vital.max_value(&mut StatCtx::detached(w, o));
            vital.set_current(o, max);
        }
    }
}

/// `Health.Current = Health.MaxValue;` (and stamina, mana): the non-player "fix tod data" fill at
/// the end of `Creature.SetEphemeralValues`.
pub fn fill_vitals_to_max(w: &World, o: &mut WorldObject) {
    for vital in [o.health(), o.stamina(), o.mana()] {
        let max = vital.max_value(&mut StatCtx::detached(w, o));
        vital.set_current(o, max);
    }
}

// ---------------------------------------------------------------------------------------------
// Not ACE: `DamageHistory` (Entity/DamageHistory.cs), named after it.
// ---------------------------------------------------------------------------------------------

/// `DamageHistory.OnHeal(healAmount)`.
fn damage_history_on_heal(w: &mut World, this: ObjectGuid, heal_amount: u32) {
    crate::entity::damage_history::on_heal(w, this, heal_amount);
}

/// `DamageHistory.Add(this, damageType, amount)`.
fn damage_history_add(w: &mut World, this: ObjectGuid, damage_type: DamageType, amount: u32) {
    crate::entity::damage_history::add(w, this, this, damage_type, amount);
}
