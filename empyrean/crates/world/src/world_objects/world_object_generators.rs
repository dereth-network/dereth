// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/WorldObject_Generators.cs
//! Port of `Source/ACE.Server/WorldObjects/WorldObject_Generators.cs`.
//!
//! A generator handles spawning other WorldObjects. Members reading only the generator (profile
//! selection and its probability sums, which draw `ThreadSafeRandom` and fill the profiles' spawn
//! queues) are `impl WorldObject` methods taking `DateTime.UtcNow`; the rest are free functions.
//!
//! ACE's `InitCreate`/`MaxCreate` are aliases of `InitGeneratedObjects`/`MaxGeneratedObjects`
//! (WorldObject_Properties.cs); they are read through the latter's wrappers here.

use empyrean_common::dotnet::datetime::DotNetDateTime;
use empyrean_common::dotnet::CsCast;
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_content::models::world::LandblockInstance;
use empyrean_entity::enums::{
    GeneratorDestruct, GeneratorTimeType, PropertyString, RegenLocationType, RegenerationType,
};
use empyrean_entity::models::PropertiesGenerator;
use empyrean_entity::{LandblockId, ObjectGuid};

use crate::entity::generator_profile::{self, GeneratorProfile};
use crate::entity::timers;
use crate::world_objects::world_object::{self, CtorEnv, WorldObject};
use crate::World;

/// Non-property fields declared in `WorldObject_Generators.cs`.
#[derive(Debug, Default)]
pub struct WorldObjectGeneratorsFields {
    /// The generator that spawned this WorldObject.
    // ACE: WorldObject.Generator
    pub generator: Option<ObjectGuid>,
    /// A generator can have multiple profiles / spawn multiple types of objects. Each generator
    /// profile can in turn spawn multiple objects (init_create / max_create). ACE's list is null
    /// only until the constructor's `InitializeGenerator`, so it is a plain list here.
    // ACE: WorldObject.GeneratorProfiles
    pub generator_profiles: Vec<GeneratorProfile>,
    // ACE: WorldObject.eventStatusChanged
    pub event_status_changed: bool,
}

/// The constructor's call: `InitializeGenerator()`, with the construction environment's clock.
pub fn initialize_generator(o: &mut WorldObject, env: &CtorEnv<'_>) {
    o.initialize_generator(env.w.now.utc);
}

impl WorldObject {
    /// Returns TRUE if this object is a generator (spawns other world objects).
    // ACE: WorldObject.IsGenerator
    #[must_use]
    pub fn is_generator(&self) -> bool {
        !self
            .wo
            .world_object_generators
            .generator_profiles
            .is_empty()
    }

    fn profiles(&self) -> &[GeneratorProfile] {
        &self.wo.world_object_generators.generator_profiles
    }

    /// Creates a list of active generator profiles from a list of biota generators.
    // ACE: WorldObject.AddGeneratorProfiles
    pub fn add_generator_profiles(&mut self, utc_now: DotNetDateTime) {
        let mut generator_profiles = Vec::new();
        let mut i: u32 = 0;

        if let Some(generators) = &self.biota.properties_generator {
            for generator in generators.iter() {
                generator_profiles.push(GeneratorProfile::new(
                    self.guid,
                    generator.clone(),
                    i,
                    utc_now,
                ));
                i += 1;
            }
        }

        self.wo.world_object_generators.generator_profiles = generator_profiles;
    }

    /// Initialize Generator system.
    // ACE: WorldObject.InitializeGenerator
    pub fn initialize_generator(&mut self, utc_now: DotNetDateTime) {
        // ensure if Max <= 0 (or defaulted to 0 from null) is not less than Init.
        // Profiles may have different settings but the core slots require Max to be greater than 0 if Init is greater than 0
        // defaulting to Max == Init for our purposes.
        let (max, init) = (self.max_generated_objects(), self.init_generated_objects());
        if (max <= 0 || max < init) && init > 0 {
            log::warn!(
                "[GENERATOR] 0x{} {}.InitializeGenerator: ({}) MaxGeneratedObjects = {max} | InitGeneratedObjects = {init}. Setting MaxGeneratedObjects = InitGeneratedObjects",
                self.guid.full(),
                self.get_property(PropertyString::Name).unwrap_or_default(),
                self.biota.weenie_class_id
            );
            self.set_max_generated_objects(init);
        }

        self.add_generator_profiles(utc_now);
    }

    /// The number of currently spawned objects + the number of objects currently in the spawn queue.
    // ACE: WorldObject.CurrentCreate
    #[must_use]
    pub fn current_create(&self) -> i32 {
        self.profiles()
            .iter()
            .map(GeneratorProfile::current_create)
            .sum()
    }

    /// A list of indices into GeneratorProfiles where CurrentCreate > 0 or is on cooldown.
    // ACE: WorldObject.GeneratorActiveProfiles
    #[must_use]
    pub fn generator_active_profiles(&self, utc_now: DotNetDateTime) -> Vec<i32> {
        let mut active_profiles = Vec::new();

        for (i, profile) in self.profiles().iter().enumerate() {
            if profile.current_create() > 0 || !profile.is_available(utc_now) {
                active_profiles.push(i32::try_from(i).expect("a List index is an int"));
            }
        }
        active_profiles
    }

    /// Returns TRUE if all generator profiles are at max objects created.
    // ACE: WorldObject.AllProfilesMaxed
    #[must_use]
    pub fn all_profiles_maxed(&self) -> bool {
        !self
            .profiles()
            .iter()
            .any(|i| !i.is_placeholder() && !i.is_maxed())
    }

    /// Returns TRUE if all generator profiles are unavailable.
    // ACE: WorldObject.AllProfilesUnavailable
    #[must_use]
    pub fn all_profiles_unavailable(&self, utc_now: DotNetDateTime) -> bool {
        !self
            .profiles()
            .iter()
            .any(|i| !i.is_placeholder() && i.is_available(utc_now))
    }

    /// Adds object(s) to the spawn queue from a single RNG roll. Draws `ThreadSafeRandom` once,
    /// unless a stop condition holds on entry.
    // ACE: WorldObject.SelectAProfile
    pub fn select_a_profile(&mut self, utc_now: DotNetDateTime) {
        //History.Add($"[{DateTime.UtcNow}] - SelectAProfile()");

        //bool rng_selected = false;

        if self.gen_stop_select_profile_conditions(utc_now) {
            return;
        }

        //var totalProbability = rng_selected ? GetTotalProbability() : 1.0f;
        //var rng = ThreadSafeRandom.Next(0.0f, totalProbability);
        //var rng = ThreadSafeRandom.Next(0.0f, 1.0f);
        let rng = ThreadSafeRandom::next_float(0.0, self.get_total_probability(utc_now));

        for i in 0..self.profiles().len() {
            let profile = &self.profiles()[i];

            // skip PlaceHolder objects
            if profile.is_placeholder() {
                continue;
            }

            // is this profile already at its max_create?
            if profile.is_maxed() {
                continue;
            }

            // is this profile currently timed out?
            if !profile.is_available(utc_now) {
                continue;
            }

            if profile
                .regen_location_type()
                .contains(RegenLocationType::Treasure)
            {
                if profile.biota.init_create > 1 {
                    log::warn!(
                        "[GENERATOR] 0x{} {}.SelectAProfile(): profile[{i}].RegenLocationType({:?}), profile.Biota.WCID({}), profile.Biota.InitCreate({}) > 1, set to 1. WCID: {} - LOC: {}",
                        self.guid,
                        self.get_property(PropertyString::Name).unwrap_or_default(),
                        profile.regen_location_type(),
                        profile.biota.weenie_class_id,
                        profile.biota.init_create,
                        self.biota.weenie_class_id,
                        self.loc_string()
                    );
                    self.wo.world_object_generators.generator_profiles[i]
                        .biota
                        .init_create = 1;
                }

                let profile = &self.profiles()[i];
                if profile.biota.max_create > 1 {
                    log::warn!(
                        "[GENERATOR] 0x{} {}.SelectAProfile(): profile[{i}].RegenLocationType({:?}), profile.Biota.WCID({}), profile.Biota.MaxCreate({}) > 1, set to 1. WCID: {} - LOC: {}",
                        self.guid,
                        self.get_property(PropertyString::Name).unwrap_or_default(),
                        profile.regen_location_type(),
                        profile.biota.weenie_class_id,
                        profile.biota.max_create,
                        self.biota.weenie_class_id,
                        self.loc_string()
                    );
                    self.wo.world_object_generators.generator_profiles[i]
                        .biota
                        .max_create = 1;
                }
            }

            //var probability = rng_selected ? GetAdjustedProbability(i) : profile.Biota.Probability;
            //var probability = profile.Biota.Probability;
            let probability = self.get_adjusted_probability(i, utc_now);

            if rng < f64::from(probability) || probability == -1.0 {
                let num_objects = self.get_spawn_objects_for_profile(i);
                self.wo.world_object_generators.generator_profiles[i].enqueue(num_objects, utc_now);
                //log.Info($"[GENERATOR] 0x{Guid} {Name}.SelectAProfile(): profile[{i}] Enqueued {numObjects} {profile.Biota.WeenieClassId} for spawning. ...");

                //var rng_str = probability == -1 ? "" : "RNG ";
                //History.Add($"[{DateTime.UtcNow}] - SelectAProfile() - {rng_str}selected slot {i} to spawn, adding {numObjects} objects ({profile.CurrentCreate}/{profile.MaxCreate})");

                // if RNG rolled, we are done with this roll
                if self.profiles()[i].biota.probability != -1.0 {
                    //rng_selected = true;
                    break;
                }

                // stop conditions
                if self.gen_stop_select_profile_conditions(utc_now) {
                    return;
                }
            }
        }
    }

    /// `Location.ToLOCString()` for a log line.
    // ACE-BUG: SelectAProfile's and Generator_Generate's warnings format `Location.ToLOCString()`
    // unguarded, so a generator without a Location throws NullReferenceException mid-selection.
    fn loc_string(&self) -> String {
        self.location()
            .expect("System.NullReferenceException: Location")
            .to_loc_string()
    }

    /// Returns the total probability of all RNG profiles which arent at max objects spawned yet or
    /// on cooldown.
    // ACE: WorldObject.GetTotalProbability
    #[must_use]
    pub fn get_total_probability(&self, utc_now: DotNetDateTime) -> f32 {
        let mut total_probability = 0.0f32;
        let mut last_probability = 0.0f32;

        for profile in self.profiles() {
            let probability = profile.biota.probability;

            if probability == -1.0 {
                //if (!profile.IsMaxed)
                if !profile.is_maxed() && profile.is_available(utc_now) {
                    return 1.0;
                }

                continue;
            }
            //if (!profile.IsMaxed)
            if !profile.is_maxed() && profile.is_available(utc_now) {
                if last_probability > probability {
                    last_probability = 0.0;
                }

                let diff = probability - last_probability;
                total_probability += diff;
            }
            last_probability = probability;
        }

        total_probability
    }

    /// Returns the max probability from all the generator profiles.
    // ACE: WorldObject.GetMaxProbability
    #[must_use]
    pub fn get_max_probability(&self) -> f32 {
        let mut max_probability = f32::MIN;

        // note: this will also include maxed generator profiles!
        for profile in self.profiles() {
            let probability = profile.biota.probability;

            if probability > max_probability {
                max_probability = probability;
            }
        }
        max_probability
    }

    /// Returns the adjust probability for a generator profile index, taking into account previous
    /// profile probabilities which are already at max objects spawned or on cooldown.
    // ACE: WorldObject.GetAdjustedProbability
    #[must_use]
    pub fn get_adjusted_probability(&self, index: usize, utc_now: DotNetDateTime) -> f32 {
        // say theres a generator with 2 profiles
        // the first has a 99% chance to spawn, and the second has a 1% chance
        // the generator init_create is 1, and the max_create is 2
        // when the generator first spawns in, the first object is created, as expected
        // then the hearbeat happens later, and it sees it can spawn up to 1 additional object
        // the rare item is the only item left, with the 1 % chance
        // so the question is, in that scenario, would the rare item always spawn then?
        // or would it only do 1 roll, and the rare item would still have only a 1% chance to spawn?

        let profiles = self.profiles();
        if profiles[index].biota.probability == -1.0 {
            return -1.0;
        }

        let mut total_probability = 0.0f32;
        let mut last_probability = 0.0f32;

        for profile in &profiles[..=index] {
            let probability = profile.biota.probability;

            if probability == -1.0 {
                continue;
            }

            //if (!profile.IsMaxed)
            if !profile.is_maxed() && profile.is_available(utc_now) {
                if last_probability > probability {
                    last_probability = 0.0;
                }

                let diff = probability - last_probability;
                total_probability += diff;
            }
            last_probability = probability;
        }
        total_probability
    }

    /// Get the current number of objects to spawn for a specific profile.
    // ACE: WorldObject.GetSpawnObjectsForProfile
    #[must_use]
    pub fn get_spawn_objects_for_profile(&self, index: usize) -> i32 {
        // get the number of objects to spawn for this profile
        // usually profile.InitCreate, must be at least profile.InitCreate while not to exceed generator.MaxCreate and profile.MaxCreate,
        // -1 for profile.InitCreate == 1
        // -1 for profile.MaxCreate == profile can be spawned infinitely as long as generator.MaxCreate has not been met.

        let profile = &self.profiles()[index];
        let init_create = profile.init_create();
        let max_create = profile.max_create();

        let mut num_objects = if init_create == -1 || max_create == -1 {
            1
        } else {
            init_create
        };

        let gen_slots_available = self.max_generated_objects() - self.current_create();
        let profile_slots_available = profile.max_create() - profile.current_create();

        if gen_slots_available < num_objects {
            num_objects = gen_slots_available;
        }

        if profile.max_create() != -1 && profile_slots_available < num_objects {
            num_objects = profile_slots_available;
        }

        let name = self.get_property(PropertyString::Name).unwrap_or_default();
        if num_objects == 0 && init_create == 0 {
            log::warn!(
                "[GENERATOR] 0x{}:{} {name}.GetSpawnObjectsForProfile(profile[{}]): profile.InitCreate = {} | profile.MaxCreate = {} | profile.WeenieClassId = {} | Profile Init invalid, cannot spawn.",
                self.guid,
                self.biota.weenie_class_id,
                profile.link_id(),
                profile.init_create(),
                profile.max_create(),
                profile.weenie_class_id()
            );
        } else if num_objects == 0 {
            log::warn!(
                "[GENERATOR] 0x{}:{} {name}.GetSpawnObjectsForProfile(profile[{}]): profile.InitCreate = {} | profile.MaxCreate = {} | profile.WeenieClassId = {} | genSlotsAvailable = {gen_slots_available} | profileSlotsAvailable = {profile_slots_available} | numObjects = {num_objects}, cannot spawn.",
                self.guid,
                self.biota.weenie_class_id,
                profile.link_id(),
                profile.init_create(),
                profile.max_create(),
                profile.weenie_class_id()
            );
        }

        num_objects
    }

    /// Returns TRUE if stop conditions have been reached for aborting generator profile selection.
    // ACE: WorldObject.GenStopSelectProfileConditions
    #[must_use]
    pub fn gen_stop_select_profile_conditions(&self, utc_now: DotNetDateTime) -> bool {
        if self.current_create() >= self.max_generated_objects() {
            //if (CurrentCreate > InitCreate)
            //log.DebugFormat("{0} - 0x{1}:{2}.StopConditionsInit(): CurrentCreate({3}) > InitCreate({4})", WeenieClassId, Guid, Name, CurrentCreate, InitCreate);

            return true;
        }

        if self.currently_powering_up() && self.current_create() >= self.init_generated_objects() {
            return true;
        }

        self.all_profiles_unavailable(utc_now) || self.all_profiles_maxed()
    }

    fn get_next_regeneration_time(
        &self,
        generator_initial_delay: f64,
        current_unix_time: f64,
    ) -> f64 {
        next_regeneration_time(
            self.regeneration_timestamp(),
            generator_initial_delay,
            current_unix_time,
        )
    }

    /// Called by ActivateLinks in WorldObject_Links for generators. `linked_instances` is
    /// `LinkedInstances`, a `WorldObject_Links.cs` field.
    // ACE: WorldObject.AddGeneratorLinks
    pub fn add_generator_links(
        &mut self,
        linked_instances: &[LandblockInstance],
        utc_now: DotNetDateTime,
    ) {
        if self.profiles().is_empty() {
            // `Console.WriteLine`
            log::info!(
                "{}.AddGeneratorLinks(): no profiles to link!",
                self.get_property(PropertyString::Name).unwrap_or_default()
            );
            return;
        }

        let profile_template = self.profiles()[0].biota.clone();

        for link in linked_instances {
            let profile = PropertiesGenerator {
                weenie_class_id: link.weenie_class_id,
                obj_cell_id: Some(link.obj_cell_id),
                origin_x: Some(link.origin_x),
                origin_y: Some(link.origin_y),
                origin_z: Some(link.origin_z),
                angles_w: Some(link.angles_w),
                angles_x: Some(link.angles_x),
                angles_y: Some(link.angles_y),
                angles_z: Some(link.angles_z),
                delay: profile_template.delay,
                probability: profile_template.probability,
                init_create: profile_template.init_create,
                max_create: profile_template.max_create,
                when_create: profile_template.when_create,
                where_create: profile_template.where_create,
                ..PropertiesGenerator::default()
            };

            let (probability, init_create, max_create) =
                (profile.probability, profile.init_create, profile.max_create);
            self.wo
                .world_object_generators
                .generator_profiles
                .push(GeneratorProfile::new(
                    self.guid, profile, link.guid, utc_now,
                ));
            if probability == -1.0 {
                let v = self.init_generated_objects() + init_create;
                self.set_init_generated_objects(v);
                let v = self.max_generated_objects() + max_create;
                self.set_max_generated_objects(v);
            }
        }
    }

    // ACE: WorldObject.GetStaticGuid
    #[must_use]
    pub fn get_static_guid(&self, dynamic_guid: u32) -> Option<u32> {
        self.profiles()
            .iter()
            .find(|p| p.spawned.contains_key(&dynamic_guid))
            .map(|p| p.id)
    }
}

/// `GetNextRegenerationTime`: now if the generator never regenerated, else after the initial delay.
// ACE: WorldObject.GetNextRegenerationTime
fn next_regeneration_time(
    regeneration_timestamp: f64,
    generator_initial_delay: f64,
    current_unix_time: f64,
) -> f64 {
    if regeneration_timestamp == 0.0 {
        return current_unix_time;
    }

    current_unix_time + generator_initial_delay
}

fn obj(w: &World, this: ObjectGuid) -> Option<&WorldObject> {
    w.objects.get(this)
}

/// Enables/disables a generator based on time status.
// ACE: WorldObject.CheckGeneratorStatus
pub fn check_generator_status(w: &mut World, this: ObjectGuid) {
    let Some(o) = obj(w, this) else { return };
    match o.generator_time_type() {
        // TODO: defined
        GeneratorTimeType::RealTime => check_real_time_status(w, this),
        GeneratorTimeType::Event => check_event_status(w, this),
        GeneratorTimeType::Night | GeneratorTimeType::Day => check_time_of_day_status(w, this),
        _ => {}
    }
}

/// Enables/disables a generator based on in-game time of day, Day or Night.
// ACE: WorldObject.CheckTimeOfDayStatus
pub fn check_time_of_day_status(w: &mut World, this: ObjectGuid) {
    let Some(o) = obj(w, this) else { return };
    let prev_disabled = o.generator_disabled();

    let is_day = timers::current_in_game_time(w).is_day();
    let is_day_generator = o.generator_time_type() == GeneratorTimeType::Day;

    //GeneratorDisabled = isDay != isDayGenerator;
    //HandleStatus(prevDisabled);

    handle_status_staged(w, this, prev_disabled, is_day, is_day_generator);
}

/// Enables/disables a generator based on realtime between start/end time.
// ACE: WorldObject.CheckRealTimeStatus
pub fn check_real_time_status(w: &mut World, this: ObjectGuid) {
    let Some(o) = obj(w, this) else { return };
    let prev_disabled = o.generator_disabled();

    let now: i32 = w.now.unix_time.cs_cast();

    let (start_time, end_time) = (o.generator_start_time(), o.generator_end_time());
    let start = (now < start_time) && (start_time > 0);
    let end = (now > end_time) && (end_time > 0);

    //GeneratorDisabled = ((now < GeneratorStartTime) && (GeneratorStartTime > 0)) || ((now > GeneratorEndTime) && (GeneratorEndTime > 0));
    //HandleStatus(prevDisabled);

    handle_status_staged(w, this, prev_disabled, start, end);
}

/// Enables/disables a generator based on world event status.
// ACE: WorldObject.CheckEventStatus
pub fn check_event_status(w: &mut World, this: ObjectGuid) {
    let Some(o) = obj(w, this) else { return };
    let Some(generator_event) = o.generator_event().filter(|e| !e.is_empty()) else {
        return;
    };

    let prev_state = o.generator_disabled();

    if !event_manager_is_event_available(w, &generator_event) {
        return;
    }

    let enabled = event_manager_is_event_enabled(w, &generator_event);
    let started = event_manager_is_event_started(w, &generator_event, this);

    //GeneratorDisabled = !enabled || !started;
    //HandleStatus(prevState);

    handle_status_staged(w, this, prev_state, enabled, started);
}

/// Handles starting/stopping the generator.
// ACE: WorldObject.HandleStatus
pub fn handle_status(w: &mut World, this: ObjectGuid, prev_disabled: bool) {
    let Some(o) = obj(w, this) else { return };
    if prev_disabled == o.generator_disabled() {
        return; // no state change
    }

    if prev_disabled {
        start_generator(w, this);
    } else {
        disable_generator(w, this);
    }
}

/// Handles starting/stopping the generator.
// ACE: WorldObject.HandleStatusStaged
pub fn handle_status_staged(
    w: &mut World,
    this: ObjectGuid,
    prev_disabled: bool,
    cond1: bool,
    cond2: bool,
) {
    let Some(o) = w.objects.get_mut(this) else {
        return;
    };
    let change = match o.generator_time_type() {
        GeneratorTimeType::RealTime => cond1 || cond2,
        GeneratorTimeType::Event => !cond1 || !cond2,
        GeneratorTimeType::Day | GeneratorTimeType::Night => cond1 != cond2,
        _ => false,
    };

    if o.wo.world_object_generators.event_status_changed {
        o.set_generator_disabled(change);

        let generator_disabled = o.generator_disabled();
        if !generator_disabled {
            start_generator(w, this);
        } else {
            disable_generator(w, this);
        }

        if let Some(o) = w.objects.get_mut(this) {
            o.wo.world_object_generators.event_status_changed = false;
        }
    } else if prev_disabled != change {
        o.wo.world_object_generators.event_status_changed = true;
    }
}

/// Called when a generator is first created.
// ACE: WorldObject.StartGenerator
pub fn start_generator(w: &mut World, this: ObjectGuid) {
    //Console.WriteLine($"{Name}.StartGenerator()");

    let now = w.now.unix_time;
    let Some(o) = w.objects.get_mut(this) else {
        return;
    };

    if o.currently_powering_up() {
        return;
    }

    o.set_currently_powering_up(true);

    let generator_initial_delay = o.generator_initial_delay();
    if generator_initial_delay > 0.0 {
        o.wo.world_object_tick.next_generator_regeneration_time =
            o.get_next_regeneration_time(generator_initial_delay, now);
        let current_landblock = o.current_landblock;
        if let Some(landblock) = current_landblock {
            landblock_resort_world_object_into_sorted_generator_regeneration_list(
                w, landblock, this,
            );
        }
    } else {
        let run = (o.is_container() && !o.is_creature())
            || (o.generator_event().is_some_and(|e| !e.is_empty())
                && o.regeneration_interval() == 0.0);
        if run {
            generator_generate(w, this);
        }

        let Some(o) = w.objects.get_mut(this) else {
            return;
        };
        if o.init_generated_objects() == 0 {
            o.set_currently_powering_up(false);
        }
    }
}

/// Disables a generator and Processes GeneratorDestructionDirective.
// ACE: WorldObject.DisableGenerator
pub fn disable_generator(w: &mut World, this: ObjectGuid) {
    // generator has been disabled, potentially destroy, kill or leave behind everything in registry and reset back to defaults
    let Some(o) = obj(w, this) else { return };
    let t = o.generator_end_destruction_type();
    process_generator_destruction_directive(w, this, t, false);
}

/// Called upon death of a generator and Processes GeneratorDestructionDirective.
// ACE: WorldObject.OnGeneratorDeath
pub fn on_generator_death(w: &mut World, this: ObjectGuid) {
    // generator has been killed, potentially destroy, kill or leave behind everything in registry and reset back to defaults
    let Some(o) = obj(w, this) else { return };
    let t = o.generator_destruction_type();
    process_generator_destruction_directive(w, this, t, false);
}

/// Called upon death of a generator and Processes GeneratorDestructionDirective.
// ACE: WorldObject.OnGeneratorDestroy
pub fn on_generator_destroy(w: &mut World, this: ObjectGuid) {
    // generator has been destroyed, potentially destroy, kill or leave behind everything in registry and reset back to defaults
    let Some(o) = obj(w, this) else { return };
    let t = o.generator_destruction_type();
    process_generator_destruction_directive(w, this, t, false);
}

/// Destroys/Kills all of its spawned objects, if specifically directed, and resets back to default.
// ACE: WorldObject.ProcessGeneratorDestructionDirective
pub fn process_generator_destruction_directive(
    w: &mut World,
    this: ObjectGuid,
    generator_destruct_type: GeneratorDestruct,
    from_landblock_unload: bool,
) {
    let count = obj(w, this).map_or(0, |o| o.profiles().len());
    match generator_destruct_type {
        GeneratorDestruct::Kill => {
            for i in 0..count {
                generator_profile::kill_all(w, this, i);
            }
        }
        GeneratorDestruct::Destroy => {
            for i in 0..count {
                generator_profile::destroy_all(w, this, i, from_landblock_unload);
            }
        }
        // GeneratorDestruct.Nothing, default
        _ => {}
    }
}

/// Callback system for objects notifying their generators of events, ie. item pickup.
// ACE: WorldObject.NotifyOfEvent
pub fn notify_of_event(w: &mut World, this: ObjectGuid, regeneration_type: RegenerationType) {
    let Some(o) = obj(w, this) else { return };
    // A generator gone from the store is ACE's null `Generator`.
    let Some(generator) =
        o.wo.world_object_generators
            .generator
            .filter(|&g| w.objects.contains(g))
    else {
        return;
    };
    if o.generator_id().is_none() {
        return;
    }

    let index = obj(w, generator).and_then(|g| {
        g.profiles()
            .iter()
            .position(|p| p.spawned.contains_key(&this.full()))
    });
    if let Some(index) = index {
        generator_profile::notify_generator(w, generator, index, this, regeneration_type);
    }

    if let Some(g) = obj(w, generator) {
        let wiped_out = g.init_generated_objects() > 0 && g.current_create() == 0;
        let (controlled, is_container, automatic_destruction) = (
            g.generator_id().is_some_and(|id| id > 0),
            g.is_container(),
            g.generator_automatic_destruction(),
        );
        if controlled {
            // Generator is controlled by another generator.
            if (!is_container || automatic_destruction) && wiped_out {
                // Parent generator is non-container (Container, Corpse, Chest, Slumlord, Storage, Hook, Creature) generator
                world_object::destroy(w, generator, true, false); // Generator's complete spawn count has been wiped out
            }
        } else if automatic_destruction && wiped_out {
            world_object::destroy(w, generator, true, false); // Generator's complete spawn count has been wiped out
        }
    }

    //if (!Generator.IsDestroyed)
    //    Generator.SelectAProfile();

    if let Some(o) = w.objects.get_mut(this) {
        o.wo.world_object_generators.generator = None;
        o.set_generator_id(None);
    }
}

/// Called every `RegenerationInterval` seconds. Also called from EmoteManager, Chest.Reset(),
/// WorldObject.OnGenerate().
// ACE: WorldObject.Generator_Update
pub fn generator_update(w: &mut World, this: ObjectGuid) {
    //Console.WriteLine($"{Name}.Generator_HeartBeat({HeartbeatInterval})");

    let Some(o) = w.objects.get_mut(this) else {
        return;
    };
    if !o.first_enter_world_done() {
        o.set_first_enter_world_done(true);
    }

    check_generator_status(w, this);

    let Some(o) = obj(w, this) else { return };
    if !o.generator_entered_world() {
        check_generator_status(w, this); // due to staging if generator hadn't entered world, reprocess CheckGeneratorStatus for first generator status to update

        if obj(w, this).is_some_and(|o| !o.generator_disabled()) {
            start_generator(w, this); // spawn initial objects for this generator
        }

        if let Some(o) = w.objects.get_mut(this) {
            o.set_generator_entered_world(true);
        }
    }
}

/// Called every `RegenerationInterval` seconds. Also called from EmoteManager, Chest.Reset(),
/// WorldObject.OnGenerate().
// ACE: WorldObject.Generator_Generate
pub fn generator_generate(w: &mut World, this: ObjectGuid) {
    //Console.WriteLine($"{Name}.Generator_Generate({RegenerationInterval})");

    let utc_now = w.now.utc;
    let Some(o) = w.objects.get_mut(this) else {
        return;
    };

    if !o.generator_disabled() {
        if o.currently_powering_up() {
            //Console.WriteLine($"{Name}.Generator_Generate({RegenerationInterval}) SelectAProfile: Init={InitCreate} Current={CurrentCreate} Max={MaxCreate} GenStopSelectProfileConditions={GenStopSelectProfileConditions}");
            let mut gen_loop_count = 0;
            while !o.gen_stop_select_profile_conditions(utc_now) {
                o.select_a_profile(utc_now);
                gen_loop_count += 1;

                if gen_loop_count > 1000 {
                    log::error!(
                        "[GENERATOR] 0x{} {}.Generator_Generate(): genLoopCount > 1000, aborted init spawn. GenStopSelectProfileConditions: {} | InitCreate: {} | CurrentCreate: {} | WCID: {} - LOC: {}",
                        o.guid,
                        o.get_property(PropertyString::Name).unwrap_or_default(),
                        o.gen_stop_select_profile_conditions(utc_now),
                        o.init_generated_objects(),
                        o.current_create(),
                        o.biota.weenie_class_id,
                        o.loc_string()
                    );
                    break;
                }
            }
            o.set_currently_powering_up(false);
        } else {
            //Console.WriteLine($"{Name}.Generator_Generate({RegenerationInterval}) SelectAProfile: Init={InitCreate} Current={CurrentCreate} Max={MaxCreate}");
            o.select_a_profile(utc_now);
        }
    }

    let count = o.profiles().len();
    for i in 0..count {
        generator_profile::spawn_heart_beat(w, this, i);
    }
}

// ---- virtual-dispatch targets: each `not_ported!` until it is ported ----

// ACE: WorldObject.ResetGenerator
pub fn world_object_reset_generator(w: &mut crate::World, this: empyrean_entity::ObjectGuid) {
    let count = obj(w, this).map_or(0, |o| o.profiles().len());
    for i in 0..count {
        generator_profile::reset(w, this, i);
    }
}

// ---- pointers to members of other ACE files (each a `not_ported!` site until it is ported) ----

/// `CurrentLandblock?.ResortWorldObjectIntoSortedGeneratorRegenerationList(this)` (Landblock.cs).
fn landblock_resort_world_object_into_sorted_generator_regeneration_list(
    w: &mut World,
    landblock: LandblockId,
    this: ObjectGuid,
) {
    let World {
        landblock_manager,
        objects,
        ..
    } = w;
    if let Some(l) = landblock_manager.landblocks.get_mut(landblock) {
        l.resort_world_object_into_sorted_generator_regeneration_list(objects, this);
    }
}

/// `EventManager.IsEventAvailable(name)` (EventManager.cs).
fn event_manager_is_event_available(w: &World, event_name: &str) -> bool {
    crate::managers::event_manager::is_event_available(w, event_name)
}

/// `EventManager.IsEventEnabled(name)` (EventManager.cs).
fn event_manager_is_event_enabled(w: &World, event_name: &str) -> bool {
    crate::managers::event_manager::is_event_enabled(w, event_name)
}

/// `EventManager.IsEventStarted(name, this, null)` (EventManager.cs).
fn event_manager_is_event_started(w: &mut World, event_name: &str, source: ObjectGuid) -> bool {
    crate::managers::event_manager::is_event_started(w, event_name, Some(source), None)
}
