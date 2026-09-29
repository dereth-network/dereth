// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Monster_Awareness.cs
//! Port of `Source/ACE.Server/WorldObjects/Monster_Awareness.cs`: when a monster wakes up from
//! its idle state, and whom it targets.

use dereth_primitives::Vec3;
use empyrean_common::dotnet::datetime::{DotNetDateTime, TimeSpan};
use empyrean_common::dotnet::DotNetDict;
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_entity::enums::{CombatMode, TargetingTactic, Tolerance};
use empyrean_entity::ObjectGuid;

use crate::dispatch;
use crate::entity::actions::action_chain::ActionChain;
use crate::entity::actions::i_actor::Actor;
use crate::entity::timers;
use crate::physics::object_maint::{self, VisibleObjectType};
use crate::physics::phys_ext;
use crate::world_objects::monster::{self, State};
use crate::world_objects::{creature_combat, monster_combat};
use crate::world_objects::{monster_navigation, monster_tick};
use crate::World;

/// Non-property fields declared in `Monster_Awareness.cs`.
#[derive(Debug, Default)]
pub struct MonsterAwarenessFields {
    /// Monsters wake up when players are in visual range.
    // ACE: Creature.IsAwake
    pub is_awake: bool,
    /// The current targeting tactic for this monster.
    // ACE: Creature.CurrentTargetingTactic
    pub current_targeting_tactic: TargetingTactic,
    // ACE: Creature.NextFindTarget
    pub next_find_target: f64,
    // ACE: Creature._visualAwarenessRangeSq
    pub visual_awareness_range_sq: Option<f32>,
    // ACE: Creature._auralAwarenessRangeSq
    pub aural_awareness_range_sq: Option<f32>,
    /// AttackTarget => last alerted time.
    // ACE: Creature.Alerted
    pub alerted: Option<DotNetDict<u32, DotNetDateTime>>,
}

/// `Creature`'s `Monster_Awareness.cs` fields.
///
/// # Panics
/// When `this` is gone or not a creature.
#[must_use]
pub fn fields(w: &World, this: ObjectGuid) -> &MonsterAwarenessFields {
    &w.objects
        .get(this)
        .and_then(|o| o.creature.as_ref())
        .expect("ACE: Creature is null (NullReferenceException)")
        .monster_awareness
}

/// Mutable [`fields`].
///
/// # Panics
/// As [`fields`].
pub fn fields_mut(w: &mut World, this: ObjectGuid) -> &mut MonsterAwarenessFields {
    &mut w
        .objects
        .get_mut(this)
        .and_then(|o| o.creature.as_mut())
        .expect("ACE: Creature is null (NullReferenceException)")
        .monster_awareness
}

/// `IsAwake`.
#[must_use]
pub fn is_awake(w: &World, this: ObjectGuid) -> bool {
    fields(w, this).is_awake
}

/// Transitions a monster from idle to awake state (`WakeUp(bool alertNearby = true)`).
// ACE: Creature.WakeUp
pub fn wake_up(w: &mut World, this: ObjectGuid, alert_nearby: bool) {
    monster::set_monster_state_value(w, this, State::Awake);
    fields_mut(w, this).is_awake = true;
    //DoAttackStance();
    let target = monster_combat::attack_target_creature(w, this);
    emote_manager_on_wake_up(w, this, target);
    emote_manager_on_new_enemy(w, this, target);
    //SelectTargetingTactic();

    if alert_nearby {
        alert_friendly(w, this);
    }
}

/// Transitions a monster from awake to idle state.
// ACE: Creature.Sleep
pub fn creature_sleep(w: &mut World, this: ObjectGuid) {
    // `if (DebugMove) Console.WriteLine(...)`: the debug flag is never set

    creature_combat::set_combat_mode(w, this, CombatMode::NonCombat);

    monster_combat::fields_mut(w, this).current_attack = None;
    monster_tick::fields_mut(w, this).first_update = true;
    monster_combat::fields_mut(w, this).attack_target = None;
    fields_mut(w, this).is_awake = false;
    monster_navigation::fields_mut(w, this).is_moving = false;
    monster::set_monster_state_value(w, this, State::Idle);

    let h = monster_navigation::physics_obj(w, this);
    if let Some(o) = w.physics.get_mut(h) {
        o.cached_velocity = Vec3::ZERO;
    }

    creature_combat::clear_retaliate_targets(w, this);
}

/// `SelectTargetingTactic()`: picks one of the monster's targeting tactics at random.
// ACE: Creature.SelectTargetingTactic
pub fn select_targeting_tactic(w: &mut World, this: ObjectGuid) {
    // monsters have multiple targeting tactics, ex. Focused | Random

    // when should this function be called?
    // when a monster spawns in, does it choose 1 TargetingTactic?

    // or do they randomly select a TargetingTactic from their list of possible tactics,
    // each time they go to find a new target?

    //Console.WriteLine($"{Name}.TargetingTactics: {TargetingTactic}");

    // if targeting tactic is none,
    // use the most common targeting tactic
    // TODO: ensure all monsters in the db have a targeting tactic
    let mut targeting_tactic = w.objects.get(this).expect("ACE: this").targeting_tactic();
    if targeting_tactic == TargetingTactic::None {
        targeting_tactic = TargetingTactic::Random | TargetingTactic::TopDamager;
    }

    let possible_tactics = targeting_tactic_get_flags(targeting_tactic);
    let count: i32 = i32::try_from(possible_tactics.len()).expect("a handful of flags");
    let mut rng = ThreadSafeRandom::next(1, count - 1);

    if targeting_tactic == TargetingTactic::None {
        rng = 0;
    }

    let index = usize::try_from(rng).expect("ACE: ArgumentOutOfRangeException (negative index)");
    fields_mut(w, this).current_targeting_tactic = possible_tactics[index];

    //Console.WriteLine($"{Name}.TargetingTactic: {CurrentTargetingTactic}");
}

/// `EnumHelper.GetFlags(targetingTactic)`: every declared member the value `HasFlag`s, in
/// `Enum.GetValues` order; `None` (0) always matches.
fn targeting_tactic_get_flags(value: TargetingTactic) -> Vec<TargetingTactic> {
    TargetingTactic::ALL
        .iter()
        .copied()
        .filter(|&f| value.0 & f.0 == f.0)
        .collect()
}

// ACE: Creature.HandleFindTarget
pub fn creature_handle_find_target(w: &mut World, this: ObjectGuid) {
    if timers::running_time(w) < fields(w, this).next_find_target {
        return;
    }

    dispatch::find_next_target::find_next_target(w, this);
}

// ACE: Creature.SetNextTargetTime
pub fn set_next_target_time(w: &mut World, this: ObjectGuid) {
    // use rng?

    //var rng = ThreadSafeRandom.Next(5.0f, 10.0f);
    let rng = 5.0f32;

    fields_mut(w, this).next_find_target = timers::running_time(w) + f64::from(rng);
}

/// Picks the next attack target by the current targeting tactic; answers whether there is one.
// ACE: Creature.FindNextTarget
pub fn creature_find_next_target(w: &mut World, this: ObjectGuid) -> bool {
    //stopwatch.Restart();

    select_targeting_tactic(w, this);
    set_next_target_time(w, this);

    let visible_targets = get_attack_targets(w, this);
    if visible_targets.is_empty() {
        if monster::monster_state(w, this) != State::Return {
            monster_navigation::move_to_home(w, this);
        }

        return false;
    }

    // Generally, a creature chooses whom to attack based on:
    //  - who it was last attacking,
    //  - who attacked it last,
    //  - or who caused it damage last.

    // When players first enter the creature's detection radius, however, none of these things are useful yet,
    // so the creature chooses a target randomly, weighted by distance.

    // Players within the creature's detection sphere are weighted by how close they are to the creature --
    // the closer you are, the more chance you have to be selected to be attacked.

    let prev_attack_target = monster_combat::attack_target(w, this);

    let tactic = fields(w, this).current_targeting_tactic;
    match tactic {
        TargetingTactic::None => {
            log::info!("{}.FindNextTarget(): TargetingTactic.None", name(w, this));
            // same as focused?
        }

        TargetingTactic::Random => {
            // this is a very common tactic with monsters,
            // although it is not truly random, it is weighted by distance
            let target_distances = build_target_distance(w, this, &visible_targets, false);
            let target = select_weighted_distance(w, this, &target_distances);
            monster_combat::set_attack_target(w, this, Some(target));
        }

        TargetingTactic::Focused => {
            // always stick with original target?
        }

        TargetingTactic::LastDamager => {
            let last_damager = crate::entity::damage_history::of(w, this).last_damager();
            let last_damager = last_damager
                .and_then(|d| d.try_get_attacker(w))
                .filter(|&g| is_creature(w, g));
            if let Some(last_damager) = last_damager {
                monster_combat::set_attack_target(w, this, Some(last_damager));
            }
        }

        TargetingTactic::TopDamager => {
            let top_damager = crate::entity::damage_history::of(w, this).top_damager();
            let top_damager = top_damager
                .and_then(|d| d.try_get_attacker(w))
                .filter(|&g| is_creature(w, g));
            if let Some(top_damager) = top_damager {
                monster_combat::set_attack_target(w, this, Some(top_damager));
            }
        }

        // these below don't seem to be used in PY16 yet...
        TargetingTactic::Weakest => {
            // should probably shuffle the list beforehand,
            // in case a bunch of levels of same level are in a group,
            // so the same player isn't always selected
            let lowest_level = order_by_level(w, &visible_targets, false).first().copied();
            monster_combat::set_attack_target(w, this, lowest_level);
        }

        TargetingTactic::Strongest => {
            let highest_level = order_by_level(w, &visible_targets, true).first().copied();
            monster_combat::set_attack_target(w, this, highest_level);
        }

        TargetingTactic::Nearest => {
            let nearest = build_target_distance(w, this, &visible_targets, false);
            monster_combat::set_attack_target(w, this, Some(nearest[0].target));
        }

        _ => {}
    }

    //Console.WriteLine($"{Name}.FindNextTarget = {AttackTarget.Name}");

    let attack_target = monster_combat::attack_target(w, this);
    if attack_target.is_some() && attack_target != prev_attack_target {
        emote_manager_on_new_enemy(w, this, attack_target);
    }

    attack_target.is_some()
}

/// `visibleTargets.OrderBy(p => p.Level)` (or `OrderByDescending`): a stable sort on the nullable
/// level, where `null` sorts before every value.
fn order_by_level(w: &World, targets: &[ObjectGuid], descending: bool) -> Vec<ObjectGuid> {
    let mut sorted = targets.to_vec();
    let level = |g: &ObjectGuid| w.objects.get(*g).and_then(|o| o.level());
    if descending {
        sorted.sort_by_key(|g| std::cmp::Reverse(level(g)));
    } else {
        sorted.sort_by_key(level);
    }
    sorted
}

/// Returns a list of attackable targets currently visible to this monster.
// ACE: Creature.GetAttackTargets
#[must_use]
pub fn get_attack_targets(w: &mut World, this: ObjectGuid) -> Vec<ObjectGuid> {
    let mut visible_targets = Vec::new();

    let h = monster_navigation::physics_obj(w, this);
    let attack_target = monster_combat::attack_target(w, this);
    let tolerance = w.objects.get(this).expect("ACE: this").tolerance();
    // `VisualAwarenessRangeSq` is a lazily cached value: read once here
    let visual_awareness_range_sq = visual_awareness_range_sq(w, this);

    for creature in object_maint::get_visible_targets_values_of_type_creature(w, h) {
        let Some(c) = w.objects.get(creature) else {
            continue;
        };

        // ensure attackable
        if !c.attackable() && c.targeting_tactic() == TargetingTactic::None
            || c.wo.world_object.teleporting
        {
            continue;
        }

        // ensure within 'detection radius' ?
        let chase_dist_sq = if Some(creature) == attack_target {
            monster_navigation::MAX_CHASE_RANGE_SQ
        } else {
            visual_awareness_range_sq
        };

        /*if (Location.SquaredDistanceTo(creature.Location) > chaseDistSq)
        continue;*/

        if monster_navigation::get_distance_sq_to_object(w, this, creature, true)
            > f64::from(chase_dist_sq)
        {
            continue;
        }

        // if this monster belongs to a faction,
        // ensure target does not belong to the same faction
        if creature_combat::same_faction(w, this, creature) {
            // unless they have been provoked
            if !object_maint::retaliate_targets_contains_key(w, h, creature.full()) {
                continue;
            }
        }

        // cannot switch AttackTargets with Tolerance.Target
        if tolerance.contains(Tolerance::Target) && Some(creature) != attack_target {
            continue;
        }

        // can only target other monsters with Tolerance.Monster -- cannot target players or combat pets
        if tolerance.contains(Tolerance::Monster) && (c.is_player() || c.is_combat_pet()) {
            continue;
        }

        visible_targets.push(creature);
    }

    visible_targets
}

/// A potential attack target and its distance (`Entity/TargetDistance.cs`).
///
/// ACE declares it in `Source/ACE.Server/Entity/TargetDistance.cs`; its counterpart here,
/// `entity/target_distance.rs`, is still empty, so the type is defined beside its user.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TargetDistance {
    pub target: ObjectGuid,
    pub distance: f32,
}

impl TargetDistance {
    // ACE: TargetDistance.TargetDistance
    #[must_use]
    pub fn new(target: ObjectGuid, distance: f32) -> Self {
        TargetDistance { target, distance }
    }
}

/// Returns the list of potential attack targets, sorted by closest distance (`distSq`: squared).
// ACE: Creature.BuildTargetDistance
#[must_use]
pub fn build_target_distance(
    w: &World,
    this: ObjectGuid,
    targets: &[ObjectGuid],
    dist_sq: bool,
) -> Vec<TargetDistance> {
    let mut target_distance = Vec::new();

    for &target in targets {
        //targetDistance.Add(new TargetDistance(target, distSq ? Location.SquaredDistanceTo(target.Location) : Location.DistanceTo(target.Location)));
        #[allow(clippy::cast_possible_truncation)] // ACE's `(float)` cast
        let distance = if dist_sq {
            monster_navigation::get_distance_sq_to_object(w, this, target, true) as f32
        } else {
            monster_navigation::get_distance_to_object(w, this, target, true) as f32
        };
        target_distance.push(TargetDistance::new(target, distance));
    }

    order_by_distance(target_distance)
}

/// `targetDistance.OrderBy(i => i.Distance).ToList()`: a stable sort under `Comparer<float>`,
/// which orders NaN before every number.
#[must_use]
pub fn order_by_distance(mut target_distance: Vec<TargetDistance>) -> Vec<TargetDistance> {
    target_distance.sort_by(|a, b| float_compare(a.distance, b.distance));
    target_distance
}

/// `float.CompareTo`: NaN is less than everything, and equal to NaN.
fn float_compare(a: f32, b: f32) -> std::cmp::Ordering {
    match (a.is_nan(), b.is_nan()) {
        (true, true) => std::cmp::Ordering::Equal,
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        (false, false) => a.partial_cmp(&b).expect("not NaN"),
    }
}

/// Uses weighted RNG selection by distance to select a target.
///
/// # Panics
/// On an empty list (ACE: `ArgumentOutOfRangeException`).
// ACE: Creature.SelectWeightedDistance
#[must_use]
pub fn select_weighted_distance(
    w: &World,
    this: ObjectGuid,
    target_distances: &[TargetDistance],
) -> ObjectGuid {
    if target_distances.len() == 1 {
        return target_distances[0].target;
    }

    // http://asheron.wikia.com/wiki/Wi_Flag

    // `Select(i => i.Distance).Sum()`: LINQ sums floats in a double, then casts back
    let dist_sum_wide: f64 = target_distances.iter().map(|i| f64::from(i.distance)).sum();
    #[allow(clippy::cast_possible_truncation)] // LINQ's `(float)` result
    let dist_sum = dist_sum_wide as f32;

    // get the sum of the inverted ratios
    let inv_ratio_sum: i32 = i32::try_from(target_distances.len()).expect("a list") - 1;

    // roll between 0 - invRatioSum here,
    // instead of 0-1 (the source of the original wi bug)
    #[allow(clippy::cast_precision_loss)] // `int` to `float`, as C# converts it
    let rng = ThreadSafeRandom::next_float(0.0, inv_ratio_sum as f32);

    // walk the list
    let mut inv_ratio = 0.0f32;
    for target_distance in target_distances {
        inv_ratio += 1.0f32 - (target_distance.distance / dist_sum);

        if rng < f64::from(inv_ratio) {
            return target_distance.target;
        }
    }
    // precision error?
    log::info!(
        "{}.SelectWeightedDistance: couldn't find target: {}",
        name(w, this),
        target_distances
            .iter()
            .map(|i| crate::world_objects::monster_combat::float_to_string(i.distance))
            .collect::<Vec<_>>()
            .join(",")
    );
    target_distances[0].target
}

/// If one of these fields is set, monster scanning for targets when it first spawns in is
/// terminated immediately.
// ACE: Creature.ExcludeSpawnScan
pub const EXCLUDE_SPAWN_SCAN: Tolerance = Tolerance(
    Tolerance::NoAttack.0 | Tolerance::Appraise.0 | Tolerance::Provoke.0 | Tolerance::Retaliate.0,
);

/// Called when a monster is first spawning in: after 0.75 s, the closest visible target within
/// visual range alerts it.
// ACE: Creature.CheckTargets
pub fn check_targets(w: &mut World, this: ObjectGuid) {
    let Some(o) = w.objects.get(this) else { return };
    if !o.attackable() && o.targeting_tactic() == TargetingTactic::None
        || (o.tolerance().0 & EXCLUDE_SPAWN_SCAN.0) != 0
    {
        return;
    }

    let mut action_chain = ActionChain::new();
    action_chain.add_delay_seconds(w, f64::from(0.75f32));
    action_chain.add_action(Actor::Object(this), move |w| check_targets_inner(w, this));
    action_chain.enqueue_chain(w);
}

// ACE: Creature.CheckTargets_Inner
pub fn check_targets_inner(w: &mut World, this: ObjectGuid) {
    let mut closest_target = None;
    let mut closest_dist_sq = f32::MAX;

    let h = monster_navigation::physics_obj(w, this);
    let tolerance = w.objects.get(this).expect("ACE: this").tolerance();

    for creature in object_maint::get_visible_targets_values_of_type_creature(w, h) {
        let Some(c) = w.objects.get(creature) else {
            continue;
        };
        if c.is_player()
            && (!c.attackable()
                || c.wo.world_object.teleporting
                || phys_ext::get_physics_state(
                    w,
                    creature,
                    empyrean_entity::enums::PhysicsState::Hidden,
                ))
        {
            continue;
        }

        if tolerance.contains(Tolerance::Monster) && (c.is_player() || c.is_combat_pet()) {
            continue;
        }

        //var distSq = Location.SquaredDistanceTo(creature.Location);
        let dist_sq = monster_navigation::get_distance_sq_to_object(w, this, creature, true);
        if dist_sq < f64::from(closest_dist_sq) {
            #[allow(clippy::cast_possible_truncation)] // ACE's `(float)` cast
            {
                closest_dist_sq = dist_sq as f32;
            }
            closest_target = Some(creature);
        }
    }
    let Some(closest_target) = closest_target else {
        return;
    };
    if closest_dist_sq > visual_awareness_range_sq(w, this) {
        return;
    }

    creature_combat::alert_monster(w, closest_target, this);
}

/// The most common value from retail. Some other common values are in the range of 12-25.
// ACE: Creature.VisualAwarenessRange_Default
pub const VISUAL_AWARENESS_RANGE_DEFAULT: f32 = 18.0;

/// The highest value found in the current database.
// ACE: Creature.VisualAwarenessRange_Highest
pub const VISUAL_AWARENESS_RANGE_HIGHEST: f32 = 75.0;

/// The squared visual awareness range, scaled by `mob_awareness_range`, cached on first read.
// ACE: Creature.VisualAwarenessRangeSq
pub fn visual_awareness_range_sq(w: &mut World, this: ObjectGuid) -> f32 {
    let v = visual_awareness_range_sq_of(w, this);
    fields_mut(w, this).visual_awareness_range_sq = Some(v);
    v
}

/// [`visual_awareness_range_sq`] without storing the cache (for callers holding `&World`; the
/// value is the same, since its inputs are the object's property and a server setting).
#[must_use]
pub fn visual_awareness_range_sq_of(w: &World, this: ObjectGuid) -> f32 {
    if let Some(v) = fields(w, this).visual_awareness_range_sq {
        return v;
    }
    let range = w
        .objects
        .get(this)
        .expect("ACE: this")
        .visual_awareness_range();
    let scale = property_manager_get_double(w, "mob_awareness_range");
    #[allow(clippy::cast_possible_truncation)] // ACE's `(float)` cast
    let visual_awareness_range =
        (range.unwrap_or(f64::from(VISUAL_AWARENESS_RANGE_DEFAULT)) * scale) as f32;

    visual_awareness_range * visual_awareness_range
}

/// The squared aural awareness range (falling back to the visual one), scaled by
/// `mob_awareness_range`, cached on first read.
// ACE: Creature.AuralAwarenessRangeSq
pub fn aural_awareness_range_sq(w: &mut World, this: ObjectGuid) -> f32 {
    let v = aural_awareness_range_sq_of(w, this);
    fields_mut(w, this).aural_awareness_range_sq = Some(v);
    v
}

/// [`aural_awareness_range_sq`] without storing the cache.
#[must_use]
pub fn aural_awareness_range_sq_of(w: &World, this: ObjectGuid) -> f32 {
    if let Some(v) = fields(w, this).aural_awareness_range_sq {
        return v;
    }
    let o = w.objects.get(this).expect("ACE: this");
    let range = o
        .aural_awareness_range()
        .or_else(|| o.visual_awareness_range());
    let scale = property_manager_get_double(w, "mob_awareness_range");
    #[allow(clippy::cast_possible_truncation)] // ACE's `(float)` cast
    let aural_awareness_range =
        (range.unwrap_or(f64::from(VISUAL_AWARENESS_RANGE_DEFAULT)) * scale) as f32;

    aural_awareness_range * aural_awareness_range
}

/// `PropertyManager.GetDouble(key).Item`.
fn property_manager_get_double(w: &World, key: &str) -> f64 {
    crate::managers::property_manager::get_double(w, key, 0.0, true).item
}

/// A monster can only alert friendly mobs to the presence of each attack target once every
/// AlertThreshold.
// ACE: Creature.AlertThreshold
#[must_use]
pub fn alert_threshold() -> TimeSpan {
    TimeSpan::from_minutes(2.0)
}

/// Alerts the friendly creatures around this monster (same creature type, or its friend type)
/// within their aural range to its attack target, at most once per target every two minutes.
// ACE: Creature.AlertFriendly
pub fn alert_friendly(w: &mut World, this: ObjectGuid) {
    let attack_target = monster_combat::attack_target(w, this)
        .expect("ACE: AttackTarget is null (NullReferenceException)");

    // if current attacker has already alerted this monster recently,
    // don't re-alert friendlies
    let now = w.now.utc;
    if let Some(last_alert_time) = fields(w, this)
        .alerted
        .as_ref()
        .and_then(|a| a.get(&attack_target.full()).copied())
    {
        if now - last_alert_time < alert_threshold() {
            return;
        }
    }

    let h = monster_navigation::physics_obj(w, this);
    let cell = phys_ext::cur_cell(w, h);
    let visible_objs = object_maint::get_visible_objects(w, h, cell, VisibleObjectType::All);

    let target_creature = monster_combat::attack_target_creature(w, this);

    let (creature_type, friend_type) = {
        let o = w.objects.get(this).expect("ACE: this");
        (o.creature_type(), o.friend_type())
    };

    let mut alerted = false;

    for obj in visible_objs {
        let Some(nearby_creature) = phys_ext::weenie_obj(w, obj)
            .world_object(w)
            .filter(|&g| is_creature(w, g))
        else {
            continue;
        };
        let nearby = w.objects.get(nearby_creature).expect("resolved above");
        if is_awake(w, nearby_creature)
            || !nearby.attackable() && nearby.targeting_tactic() == TargetingTactic::None
        {
            continue;
        }

        if (nearby.tolerance().0 & creature_combat::ALERT_EXCLUDE.0) != 0 {
            continue;
        }

        let nearby_type = nearby.creature_type();
        if creature_type.is_some() && creature_type == nearby_type
            || friend_type.is_some() && friend_type == nearby_type
        {
            //var distSq = Location.SquaredDistanceTo(nearbyCreature.Location);
            let dist_sq =
                monster_navigation::get_distance_sq_to_object(w, this, nearby_creature, true);
            if dist_sq > f64::from(aural_awareness_range_sq(w, nearby_creature)) {
                continue;
            }

            // scenario: spawn a faction mob, and then spawn a non-faction mob next to it, of the same CreatureType
            // the spawning mob will become alerted by the faction mob, and will then go to alert its friendly types
            // the faction mob happens to be a friendly type, so it in effect becomes alerted to itself
            // this is to prevent the faction mob from adding itself to its retaliate targets / visible targets,
            // and setting itself to its AttackTarget
            if nearby_creature == attack_target {
                continue;
            }

            let target_creature = target_creature
                .expect("ACE: AttackTarget as Creature is null (NullReferenceException)");

            if creature_combat::same_faction(w, nearby_creature, target_creature) {
                creature_combat::add_retaliate_target(w, nearby_creature, attack_target);
            }

            if creature_combat::potential_foe(w, this, target_creature) {
                if creature_combat::potential_foe(w, nearby_creature, target_creature) {
                    creature_combat::add_retaliate_target(w, nearby_creature, attack_target);
                } else {
                    continue;
                }
            }

            alerted = true;

            monster_combat::set_attack_target(w, nearby_creature, Some(attack_target));
            wake_up(w, nearby_creature, false);
        }
    }
    // only set alerted if monsters were actually alerted
    if alerted {
        let now = w.now.utc;
        let f = fields_mut(w, this);
        f.alerted
            .get_or_insert_with(DotNetDict::new)
            .insert(attack_target.full(), now);
    }
}

/// Wakes up a faction monster from any non-faction monsters wandering within range.
// ACE: Creature.FactionMob_CheckMonsters
pub fn faction_mob_check_monsters(w: &mut World, this: ObjectGuid) {
    if monster::monster_state(w, this) != State::Idle {
        return;
    }

    let h = monster_navigation::physics_obj(w, this);
    let creatures = object_maint::get_visible_targets_values_of_type_creature(w, h);

    for creature in creatures {
        let Some(c) = w.objects.get(creature) else {
            continue;
        };
        // ensure type isn't already handled elsewhere
        if c.is_player() || c.is_combat_pet() {
            continue;
        }

        // ensure attackable
        if monster_combat::is_dead(c)
            || !c.attackable() && c.targeting_tactic() == TargetingTactic::None
            || c.wo.world_object.teleporting
        {
            continue;
        }

        // ensure another faction
        if creature_combat::same_faction(w, this, creature)
            && !creature_combat::potential_foe(w, this, creature)
        {
            continue;
        }

        // ensure within detection range
        if monster_navigation::get_distance_sq_to_object(w, this, creature, true)
            > f64::from(visual_awareness_range_sq(w, this))
        {
            continue;
        }

        creature_combat::alert_monster(w, this, creature);
        break;
    }
}

// ---------------------------------------------------------------------------------------------
// Helpers and pointers
// ---------------------------------------------------------------------------------------------

/// `wo is Creature`.
pub(crate) fn is_creature(w: &World, g: ObjectGuid) -> bool {
    w.objects
        .get(g)
        .is_some_and(crate::world_objects::world_object::WorldObject::is_creature)
}

/// `Name` (virtual), for log lines.
pub(crate) fn name(w: &World, this: ObjectGuid) -> String {
    dispatch::name::name(w, this).unwrap_or_default()
}

/// `EmoteManager.OnWakeUp(target)`.
pub(crate) fn emote_manager_on_wake_up(
    w: &mut World,
    this: ObjectGuid,
    target: Option<ObjectGuid>,
) {
    crate::world_objects::managers::emote_manager::on_wake_up(w, this, target);
}

/// `EmoteManager.OnNewEnemy(target)`.
pub(crate) fn emote_manager_on_new_enemy(
    w: &mut World,
    this: ObjectGuid,
    target: Option<ObjectGuid>,
) {
    crate::world_objects::managers::emote_manager::on_new_enemy(w, this, target);
}

/// `EmoteManager.OnHomeSick(attackTarget)`.
pub(crate) fn emote_manager_on_home_sick(
    w: &mut World,
    this: ObjectGuid,
    attack_target: Option<ObjectGuid>,
) {
    crate::world_objects::managers::emote_manager::on_home_sick(w, this, attack_target);
}

/// `EmoteManager.OnAttack(target)`.
pub(crate) fn emote_manager_on_attack(w: &mut World, this: ObjectGuid, target: Option<ObjectGuid>) {
    crate::world_objects::managers::emote_manager::on_attack(w, this, target);
}

/// `EmoteManager.IsBusy`.
pub(crate) fn emote_manager_is_busy(w: &World, this: ObjectGuid) -> bool {
    crate::world_objects::managers::emote_manager::is_busy(w, this)
}
