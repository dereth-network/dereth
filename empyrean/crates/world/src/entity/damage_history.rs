// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/DamageHistory.cs
//! Port of `Source/ACE.Server/Entity/DamageHistory.cs`.
//!
//! The history lives inside its creature (`Creature.DamageHistory`, see [`of`]). Members that
//! read only the history are methods; members that read the creature's health or call
//! `Creature.OnHealthUpdate` are free functions over `(w, this)`, `this` being the creature.

use std::cmp::Ordering;
use std::fmt;

use empyrean_common::dotnet::{
    format as dotnet_format, CsCast, DotNetDateTime, DotNetDict, DotNetHashSet, TimeSpan,
};
use empyrean_entity::enums::DamageType;
use empyrean_entity::ObjectGuid;

use crate::entity::damage_history_entry::DamageHistoryEntry;
use crate::entity::damage_history_info::DamageHistoryInfo;
use crate::world_objects::entity::creature_attribute::StatCtx;
use crate::World;

// ACE: DamageHistory
/// Tracks the recent damage sources for Players / Creatures.
#[derive(Debug, Clone, Default)]
pub struct DamageHistory {
    // ACE: DamageHistory.Creature
    /// The player or creature this Damage History is tracking.
    pub creature: ObjectGuid,

    // ACE: DamageHistory.Log
    /// A list of damage sources, amounts, and timestamps.
    pub log: Vec<DamageHistoryEntry>,

    // ACE: DamageHistory.TotalDamage
    /// A lookup table of WorldObjects that have damaged this WorldObject, and the total amount of
    /// damage they have inflicted.
    pub total_damage: DotNetDict<ObjectGuid, DamageHistoryInfo>,

    // ACE: DamageHistory.LastPruneTime
    /// The last time the log was pruned.
    pub last_prune_time: DotNetDateTime,
}

// ACE: DamageHistory.minimumPruneInterval
const MINIMUM_PRUNE_INTERVAL_SECONDS: f64 = 30.0;

// ACE: DamageHistory.maximumTimeToRetain
const MAXIMUM_TIME_TO_RETAIN_MINUTES: f64 = 3.0;

/// `Comparer<float>.Default` (NaN sorts below every number), for `OrderByDescending`.
fn compare_f32(a: f32, b: f32) -> Ordering {
    match (a.is_nan(), b.is_nan()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        _ => a.partial_cmp(&b).unwrap_or(Ordering::Equal),
    }
}

impl DamageHistory {
    // ACE: DamageHistory.DamageHistory
    /// Constructs a new DamageHistory for a Player / Creature. `now` is `DateTime.UtcNow`.
    #[must_use]
    pub fn new(creature: ObjectGuid, now: DotNetDateTime) -> Self {
        Self {
            creature,
            log: Vec::new(),
            total_damage: DotNetDict::new(),
            last_prune_time: now,
        }
    }

    // ACE: DamageHistory.Damagers
    /// Returns the list of players or creatures who inflicted damage.
    #[must_use]
    pub fn damagers(&self) -> Vec<DamageHistoryInfo> {
        self.total_damage.values().cloned().collect()
    }

    // ACE: DamageHistory.LastDamager
    /// Returns the DamageHistoryInfo for the last damager.
    #[must_use]
    pub fn last_damager(&self) -> Option<DamageHistoryInfo> {
        let last_damager = self.log.iter().rev().find(|l| l.amount < 0)?;

        self.total_damage.get(&last_damager.attacker).cloned()
    }

    // ACE: DamageHistory.TotalHealth
    /// `TotalDamage.Values.Sum(i => i.TotalDamage)`: LINQ's float `Sum` accumulates in `double`
    /// and narrows the result.
    #[must_use]
    pub fn total_health(&self) -> f32 {
        let mut sum = 0.0f64;
        for i in self.total_damage.values() {
            sum += f64::from(i.total_damage);
        }
        #[allow(clippy::cast_possible_truncation)]
        let total = sum as f32;
        total
    }

    // ACE: DamageHistory.TopDamager
    /// Returns the DamageHistoryInfo for the top damager, for determining 'Killed by' corpse
    /// looting rights.
    #[must_use]
    pub fn top_damager(&self) -> Option<DamageHistoryInfo> {
        self.get_top_damager(true)
    }

    // ACE: DamageHistory.GetTopDamager
    /// The first damager with the most total damage (a stable descending sort's first element).
    #[must_use]
    pub fn get_top_damager(&self, include_self: bool) -> Option<DamageHistoryInfo> {
        let mut best: Option<&DamageHistoryInfo> = None;
        for wo in self
            .total_damage
            .values()
            .filter(|wo| include_self || wo.guid != self.creature)
        {
            match best {
                Some(b) if compare_f32(wo.total_damage, b.total_damage) != Ordering::Greater => {}
                _ => best = Some(wo),
            }
        }
        best.cloned()
    }

    // ACE: DamageHistory.AddInternal
    /// Internally increments the total damage table (the `WorldObject attacker` overload).
    fn add_internal_new(&mut self, w: &World, attacker: ObjectGuid, amount: u32) {
        if let Some(value) = self.total_damage.get_mut(&attacker) {
            let amount: f32 = amount.cs_cast();
            value.total_damage += amount;
        } else {
            let info = DamageHistoryInfo::new(w, attacker, amount.cs_cast());
            self.total_damage.add(attacker, info);
        }
    }

    // ACE: DamageHistory.AddInternal
    /// Internally increments the total damage table (the `ObjectGuid attacker` overload).
    fn add_internal(&mut self, attacker: ObjectGuid, amount: u32) {
        // todo: investigate, this shouldn't happen?
        // key 0 from BuildTotalDamage()
        if let Some(value) = self.total_damage.get_mut(&attacker) {
            let amount: f32 = amount.cs_cast();
            value.total_damage += amount;
        }
    }

    // ACE: DamageHistory.OnHealInternal
    /// Internally scales TotalDamage entries by a healing amount: on heal, scale the damage from
    /// each source by 1 - healAmount / previous missingHealth.
    fn on_heal_internal(&mut self, heal_amount: u32, current_health: u32, max_health: u32) {
        let missing_health = max_health.wrapping_sub(current_health.wrapping_sub(heal_amount));
        if heal_amount == 0 || missing_health == 0 {
            return;
        }
        let (heal, missing): (f32, f32) = (heal_amount.cs_cast(), missing_health.cs_cast());
        let scalar = 1.0f32 - heal / missing;

        let attackers: Vec<ObjectGuid> = self.total_damage.keys().copied().collect();

        for attacker in attackers {
            if let Some(v) = self.total_damage.get_mut(&attacker) {
                v.total_damage *= scalar;
            }
        }
    }

    // ACE: DamageHistory.Reset
    /// Resets the damage log (eg. on player death).
    pub fn reset(&mut self) {
        self.log.clear();
        self.total_damage.clear();
    }

    // ACE: DamageHistory.TryPrune
    /// Tries pruning the log according to the minimum pruning time.
    pub fn try_prune(&mut self, now: DotNetDateTime) {
        if self.last_prune_time + TimeSpan::from_seconds(MINIMUM_PRUNE_INTERVAL_SECONDS) < now {
            self.prune(now);
        }
    }

    // ACE: DamageHistory.Prune
    /// Removes log entries older than the retention time.
    pub fn prune(&mut self, now: DotNetDateTime) {
        let mut entries_to_remove = 0;

        for entry in &self.log {
            if entry.time + TimeSpan::from_minutes(MAXIMUM_TIME_TO_RETAIN_MINUTES) < now {
                entries_to_remove += 1;
            } else {
                break;
            }
        }

        if entries_to_remove > 0 {
            self.log.drain(0..entries_to_remove);
            self.build_total_damage();
            //Console.WriteLine($"DamageHistory.Prune() - {entriesToRemove} entries removed");
        }

        self.last_prune_time = now;
    }

    // ACE: DamageHistory.BuildTotalDamage
    /// Rebuilds TotalDamage from the current state of the history log.
    pub fn build_total_damage(&mut self) {
        // This is a little bit hacky.
        // We don't want to clear our TotalDamage entries because we might lose references to WorldObjects
        // Instead, we remove entries that are no longer needed, and set all the values to 0.

        let mut guids = DotNetHashSet::new();
        for entry in &self.log {
            guids.insert(entry.attacker);
        }

        let keys: Vec<ObjectGuid> = self.total_damage.keys().copied().collect();
        for key in keys {
            if guids.contains(&key) {
                if let Some(v) = self.total_damage.get_mut(&key) {
                    v.total_damage = 0.0;
                }
            } else {
                self.total_damage.remove(&key);
            }
        }

        // TotalDamage is now reset

        let log = self.log.clone();
        for entry in &log {
            if entry.amount < 0 {
                self.add_internal(entry.attacker, entry.amount.wrapping_neg().cs_cast());
            } else {
                self.on_heal_internal(
                    entry.amount.cs_cast(),
                    entry.current_health,
                    entry.max_health,
                );
            }
        }
    }

    // ACE: DamageHistory.HasDamager
    /// Returns TRUE if damage history contains wo as recent attacker. If `non_zero`, the attacker
    /// must have TotalDamage > 0.
    #[must_use]
    pub fn has_damager(&self, wo: ObjectGuid, non_zero: bool) -> bool {
        let Some(total_damage) = self.total_damage.get(&wo) else {
            return false;
        };

        if non_zero {
            total_damage.total_damage > 0.0
        } else {
            true
        }
    }
}

// ACE: DamageHistory.ToString
impl fmt::Display for DamageHistory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut table = String::new();

        for attacker in self.total_damage.values() {
            table += &format!(
                "{} ({}) - {}\n",
                attacker.name.as_deref().unwrap_or(""),
                attacker.guid,
                dotnet_format(attacker.total_damage, "")
            );
        }
        f.write_str(&table)
    }
}

/// `Creature.DamageHistory` (declared in `Creature_Combat.cs`; held in
/// `CreatureDeathFields::damage_history`).
///
/// # Panics
/// When `creature` is not a creature in the store.
#[must_use]
pub fn of(w: &World, creature: ObjectGuid) -> &DamageHistory {
    &w.objects
        .get(creature)
        .and_then(|o| o.creature.as_ref())
        .expect("NullReferenceException: Creature.DamageHistory")
        .creature_death
        .damage_history
}

/// The mutable [`of`].
///
/// # Panics
/// When `creature` is not a creature in the store.
pub fn of_mut(w: &mut World, creature: ObjectGuid) -> &mut DamageHistory {
    &mut w
        .objects
        .get_mut(creature)
        .and_then(|o| o.creature.as_mut())
        .expect("NullReferenceException: Creature.DamageHistory")
        .creature_death
        .damage_history
}

// ACE: DamageHistory.Add
/// Logs a damaging event for the creature `this`: `attacker` is the attacker or source of damage,
/// `amount` the amount of damage hit for.
pub fn add(
    w: &mut World,
    this: ObjectGuid,
    attacker: ObjectGuid,
    damage_type: DamageType,
    amount: u32,
) {
    //Console.WriteLine($"{Creature.Name}.DamageHistory.Add({attacker.Name}, {damageType}, {amount})");

    if amount == 0 {
        return;
    }

    let signed: i32 = amount.cs_cast();
    let entry = DamageHistoryEntry::new(w, this, attacker, damage_type, signed.wrapping_neg());
    of_mut(w, this).log.push(entry);

    let mut history = std::mem::take(of_mut(w, this));
    history.add_internal_new(w, attacker, amount);
    *of_mut(w, this) = history;

    creature_on_health_update(w, this);
}

// ACE: DamageHistory.OnHeal
/// Called when the creature `this` regains some health.
pub fn on_heal(w: &mut World, this: ObjectGuid, heal_amount: u32) {
    //Console.WriteLine($"DamageHistory.OnHeal({Creature.Name}, {healAmount})");

    let entry = DamageHistoryEntry::new(
        w,
        this,
        ObjectGuid::INVALID,
        DamageType::Undef,
        heal_amount.cs_cast(),
    );
    of_mut(w, this).log.push(entry);

    // calculate previous missingHealth
    let (current, max) = {
        let o = w
            .objects
            .get(this)
            .expect("NullReferenceException: DamageHistory.Creature");
        let health = o.health();
        let current = health.current(o);
        (current, health.max_value(&mut StatCtx::in_world(w, this)))
    };
    of_mut(w, this).on_heal_internal(heal_amount, current, max);

    creature_on_health_update(w, this);
}

// ---------------------------------------------------------------------------------------------
// Not ACE: pointers to members ported in other files, named after them.
// ---------------------------------------------------------------------------------------------

/// `Creature.OnHealthUpdate()` (`Creature.cs`).
fn creature_on_health_update(w: &mut World, this: ObjectGuid) {
    crate::world_objects::creature::on_health_update(w, this);
}
