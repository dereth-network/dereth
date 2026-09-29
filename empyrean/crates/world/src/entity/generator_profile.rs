// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/GeneratorProfile.cs
//! Port of `Source/ACE.Server/Entity/GeneratorProfile.cs`.
//!
//! A profile lives in its generator's `GeneratorProfiles` list
//! ([`WorldObjectGeneratorsFields`](crate::world_objects::world_object_generators::WorldObjectGeneratorsFields)).
//! Members that read only the profile are methods; members that touch the world (spawning, the
//! generator's other fields, other objects) are free functions taking the generator's guid and
//! the profile's index in that list, which is stable because ACE only ever appends to it.
//! `DateTime.UtcNow` is `w.now.utc`.

use empyrean_common::dotnet::datetime::DotNetDateTime;
use empyrean_common::dotnet::numerics::{Quaternion, Vector3};
use empyrean_common::dotnet::{CsCast, DotNetDict};
use empyrean_entity::enums::{GeneratorType, PropertyFloat, RegenLocationType, RegenerationType};
use empyrean_entity::models::PropertiesGenerator;
use empyrean_entity::{ObjectGuid, Position};

use crate::entity::position_extensions::is_walkable;
use crate::entity::world_object_info::WorldObjectInfo;
use crate::factories::world_object_factory;
use crate::world_objects::world_object::{self, CtorEnv, WorldObject};
use crate::World;

/// A generator profile for a Generator.
// ACE: GeneratorProfile
#[derive(Debug, Clone)]
pub struct GeneratorProfile {
    /// The id for the profile. This id will be either a GUID from Landblock_Instances or an
    /// incremental id based on profile order from biota entry.
    // ACE: GeneratorProfile.Id
    pub id: u32,
    /// The biota with all the generator profile info.
    // DIVERGE: ACE holds a reference to the generator biota's `PropertiesGenerator` entry (itself
    // shared with the cached weenie); this is a copy, so SelectAProfile's Treasure clamp of
    // InitCreate/MaxCreate changes only this profile, not the biota or the weenie cache.
    // ACE: GeneratorProfile.Biota
    pub biota: PropertiesGenerator,
    /// A list of objects that have been spawned by this generator. Mapping of object guid =>
    /// registry node, which provides a bunch of detailed info about the spawn.
    // ACE: GeneratorProfile.Spawned
    pub spawned: DotNetDict<u32, WorldObjectInfo>,
    /// The list of pending times awaiting respawning.
    // ACE: GeneratorProfile.SpawnQueue
    pub spawn_queue: Vec<DotNetDateTime>,
    /// TRUE if this Profile generated treasure using TreasureGenerator.
    // ACE: GeneratorProfile.GeneratedTreasureItem
    pub generated_treasure_item: bool,
    /// Flag indicates if generator profile is performing the initial spawn (TRUE / default), or the
    /// respawn (false).
    // ACE: GeneratorProfile.FirstSpawn
    pub first_spawn: bool,
    /// DateTime for when the profile is available as a possible spawn choice.
    // ACE: GeneratorProfile.NextAvailable
    pub next_available: DotNetDateTime,
    /// The generator world object for this profile.
    // ACE: GeneratorProfile.Generator
    pub generator: ObjectGuid,
}

/// A C# collection's `Count` (an `int`).
fn count(n: usize) -> i32 {
    i32::try_from(n).expect("a .NET collection holds at most int.MaxValue items")
}

/// The weenie class of a placeholder profile.
const PLACEHOLDER_WCID: u32 = 3666;

impl GeneratorProfile {
    /// Constructs a new active generator profile from a biota generator. `utc_now` is the
    /// `DateTime.UtcNow` of `NextAvailable`'s initializer.
    // ACE: GeneratorProfile.GeneratorProfile
    #[must_use]
    pub fn new(
        generator: ObjectGuid,
        biota: PropertiesGenerator,
        profile_id: u32,
        utc_now: DotNetDateTime,
    ) -> Self {
        GeneratorProfile {
            id: profile_id,
            biota,
            spawned: DotNetDict::new(),
            spawn_queue: Vec::new(),
            generated_treasure_item: false,
            first_spawn: true,
            next_available: utc_now,
            generator,
        }
    }

    // ACE: GeneratorProfile.LinkId
    #[must_use]
    pub fn link_id(&self) -> String {
        if self.id > 0x7000_0000 {
            format!("0x{:08X}", self.id)
        } else {
            format!("{}", self.id)
        }
    }

    /// Returns TRUE if this profile is a placeholder object. Placeholder objects are used for
    /// linkable generators, and are used as a template for the real items contained in the links.
    // ACE: GeneratorProfile.IsPlaceholder
    #[must_use]
    pub fn is_placeholder(&self) -> bool {
        self.weenie_class_id() == PLACEHOLDER_WCID
    }

    /// The total # of active spawned objects + awaiting spawning.
    // ACE: GeneratorProfile.CurrentCreate
    #[must_use]
    pub fn current_create(&self) -> i32 {
        if !self.generated_treasure_item {
            count(self.spawned.len()) + count(self.spawn_queue.len())
        } else if !self.spawned.is_empty() || !self.spawn_queue.is_empty() {
            1
        } else {
            0
        }
    }

    // ACE: GeneratorProfile.InitCreate
    #[must_use]
    pub fn init_create(&self) -> i32 {
        self.biota.init_create
    }

    // ACE: GeneratorProfile.MaxCreate
    #[must_use]
    pub fn max_create(&self) -> i32 {
        self.biota.max_create
    }

    // ACE: GeneratorProfile.WeenieClassId
    #[must_use]
    pub fn weenie_class_id(&self) -> u32 {
        self.biota.weenie_class_id
    }

    /// Returns TRUE if this profile is not currently on timed-out as a result of being notified of
    /// destruction/pick-up.
    // DIVERGE: ACE tests `DateTime.UtcNow > NextAvailable` on a clock that always moves on between
    // two reads; `w.now` is frozen for a tick, so the equal case (a profile built or reset earlier
    // in the same tick, or a Delay of 0) counts as later, as it always is in ACE.
    // ACE: GeneratorProfile.IsAvailable
    #[must_use]
    pub fn is_available(&self, utc_now: DotNetDateTime) -> bool {
        utc_now >= self.next_available
    }

    /// Returns TRUE if this profile MaxCreate is not infinite (-1) and CurrentCreate does not
    /// currently meet or exceed MaxCreate.
    // ACE: GeneratorProfile.IsMaxed
    #[must_use]
    pub fn is_maxed(&self) -> bool {
        self.max_create() != -1 && self.current_create() >= self.max_create()
    }

    // ACE: GeneratorProfile.RegenLocationType
    #[must_use]
    pub fn regen_location_type(&self) -> RegenLocationType {
        self.biota.where_create
    }

    /// Determines the spawn times for initial object spawning, and for respawning.
    // ACE: GeneratorProfile.GetSpawnTime
    #[must_use]
    pub fn get_spawn_time(&self, utc_now: DotNetDateTime) -> DotNetDateTime {
        utc_now
    }

    /// Enqueues 1 or multiple objects from this generator profile adds these items to the spawn
    /// queue.
    // ACE: GeneratorProfile.Enqueue
    pub fn enqueue(&mut self, num_objects: i32, utc_now: DotNetDateTime) {
        for _ in 0..num_objects {
            let t = self.get_spawn_time(utc_now);
            self.spawn_queue.push(t);
        }
    }
}

// ---------------------------------------------------------------- members that touch the world

fn generator_of(w: &World, generator: ObjectGuid) -> &WorldObject {
    w.objects.get(generator).unwrap_or_else(|| {
        panic!("System.NullReferenceException: GeneratorProfile.Generator {generator:?}")
    })
}

/// The profile at `index` of `generator`'s `GeneratorProfiles`, if both still exist.
#[must_use]
pub fn profile(w: &World, generator: ObjectGuid, index: usize) -> Option<&GeneratorProfile> {
    w.objects
        .get(generator)?
        .wo
        .world_object_generators
        .generator_profiles
        .get(index)
}

/// As [`profile`], mutably.
pub fn profile_mut(
    w: &mut World,
    generator: ObjectGuid,
    index: usize,
) -> Option<&mut GeneratorProfile> {
    w.objects
        .get_mut(generator)?
        .wo
        .world_object_generators
        .generator_profiles
        .get_mut(index)
}

/// The delay for respawning objects.
// ACE: GeneratorProfile.Delay
#[must_use]
pub fn delay(w: &World, generator: ObjectGuid, index: usize) -> f32 {
    let g = generator_of(w, generator);
    if g.is_chest() {
        return 0.0;
    }

    let profiles = &g.wo.world_object_generators.generator_profiles;
    profiles[index]
        .biota
        .delay
        .or_else(|| {
            profiles
                .first()
                .expect("System.ArgumentOutOfRangeException: GeneratorProfiles[0]")
                .biota
                .delay
        })
        .unwrap_or(0.0)
}

/// Called upon Generate request. Processes the SpawnQueue.
// ACE: GeneratorProfile.Spawn_HeartBeat
pub fn spawn_heart_beat(w: &mut World, generator: ObjectGuid, index: usize) {
    if profile(w, generator, index).is_some_and(|p| !p.spawn_queue.is_empty()) {
        process_queue(w, generator, index);
    }
}

/// Spawns generator objects at the correct SpawnTime. Called on when Generate is requested.
// ACE: GeneratorProfile.ProcessQueue
pub fn process_queue(w: &mut World, generator: ObjectGuid, index: usize) {
    let mut i = 0;

    loop {
        let utc_now = w.now.utc;
        let Some(p) = profile(w, generator, index) else {
            return;
        };
        if i >= p.spawn_queue.len() {
            break;
        }

        let queued_time = p.spawn_queue[i];

        if queued_time > utc_now {
            // not time to spawn yet
            i += 1;
            continue;
        }

        let max_create = p.max_create();
        if max_create == -1 || count(p.spawned.len()) < max_create {
            let objects = spawn(w, generator, index);

            if let Some(objects) = objects {
                for woi in objects {
                    let obj = woi.guid;

                    let Some(p) = profile_mut(w, generator, index) else {
                        return;
                    };
                    // `Dictionary.Add` throws on a duplicate key.
                    assert!(
                        p.spawned.insert(obj.full(), woi).is_none(),
                        "System.ArgumentException: an item with the same key has already been added: {obj:?}"
                    );
                }
            }
        } else {
            // this shouldn't happen (hopefully)
            log::warn!(
                "[GENERATOR] {generator}:{} ProcessQueue(): {}:{} object(s) enqueued for {}, but MaxCreate({max_create}) already reached!",
                generator_of(w, generator).biota.weenie_class_id,
                p.link_id(),
                p.biota.weenie_class_id,
                name(w, generator),
            );
        }

        let Some(p) = profile_mut(w, generator, index) else {
            return;
        };
        p.spawn_queue.remove(i);
    }

    if let Some(p) = profile_mut(w, generator, index) {
        p.first_spawn = false;
    }
}

/// Spawns an object from the generator queue. For RNG treasure, can spawn multiple objects. If an
/// object failed to spawn, but FirstSpawn is true, the object will still be returned as a spawned
/// item, but, it will have been Destroy()'d first.
// ACE: GeneratorProfile.Spawn
/// Returns each spawned object's [`WorldObjectInfo`], taken while the object exists: ACE keeps a
/// failed first spawn in the list after destroying it, and its info reads the destroyed (but still
/// referenced) object, whereas here a destroyed object has left the store.
pub fn spawn(w: &mut World, generator: ObjectGuid, index: usize) -> Option<Vec<WorldObjectInfo>> {
    let p = profile(w, generator, index)?;
    let regen_location_type = p.regen_location_type();
    let biota = p.biota.clone();

    let objects: Vec<ObjectGuid> = if regen_location_type.contains(RegenLocationType::Treasure) {
        let treasure = treasure_generator(w, generator, index);

        if treasure.as_ref().is_some_and(|t| !t.is_empty()) {
            if let Some(g) = w.objects.get_mut(generator) {
                g.set_generated_treasure_item(true);
            }
            if let Some(p) = profile_mut(w, generator, index) {
                p.generated_treasure_item = true;
            }
        }

        // Not ACE's (a fix, V340): a wielded-treasure roll that creates nothing
        // spawns nothing, and the spawn completes. ACE iterated the null list its roll returned
        // and threw, ending the spawn.
        treasure.unwrap_or_default()
    } else {
        let Some(wo) = world_object_factory_create_new_world_object(w, biota.weenie_class_id)
        else {
            log::warn!(
                "[GENERATOR] {generator}:{} {}.Spawn(): failed to create wcid {}",
                generator_of(w, generator).biota.weenie_class_id,
                name(w, generator),
                biota.weenie_class_id
            );
            return None;
        };

        let palette = biota.palette_id.is_some_and(|v| v > 0);
        let shade = biota.shade.is_some_and(|v| v > 0.0);
        {
            let o = w.objects.get_mut(wo).expect("just created");

            if let Some(palette_id) = biota.palette_id.filter(|_| palette) {
                o.set_palette_template(Some(palette_id.cs_cast()));
            }

            if let Some(s) = biota.shade.filter(|_| shade) {
                o.set_shade(Some(f64::from(s)));
            }
        }

        if shade || palette {
            crate::dispatch::calculate_obj_desc::calculate_obj_desc(w, wo); // to update icon
        }

        if biota.stack_size.is_some_and(|v| v > 0) {
            w.objects
                .get_mut(wo)
                .expect("just created")
                .set_stack_size(biota.stack_size);
        }

        vec![wo]
    };

    let mut spawned = Vec::new();

    for obj in objects {
        //log.DebugFormat("{0}.Spawn({1})", _generator.Name, obj.Name);

        if let Some(o) = w.objects.get_mut(obj) {
            o.wo.world_object_generators.generator = Some(generator);
            o.set_generator_id(Some(generator.full()));
        }

        let success = if regen_location_type.contains(RegenLocationType::Specific) {
            spawn_specific(w, generator, index, obj)
        } else if regen_location_type.contains(RegenLocationType::Scatter) {
            spawn_scatter(w, generator, index, obj)
        } else if regen_location_type.contains(RegenLocationType::Contain) {
            spawn_container(w, generator, obj)
        } else if regen_location_type.contains(RegenLocationType::Shop) {
            spawn_shop(w, generator, obj)
        } else {
            spawn_default(w, generator, obj)
        };

        // if first spawn fails, don't continually attempt to retry
        let first_spawn = profile(w, generator, index).is_some_and(|p| p.first_spawn);
        if success || first_spawn {
            spawned.push(WorldObjectInfo::new(w, obj));
        }

        // If the object failed to spawn, we still destroy it. This cleans up the object and releases the GUID.
        // This object still may be returned in the spawned collection if FirstSpawn is true. This is to prevent retry spam.
        if !success {
            log::debug!("[GENERATOR] {generator}: {}.Spawn(): failed to spawn {obj} at {regen_location_type:?}", name(w, generator));
            world_object::destroy(w, obj, true, false);
        }
    }

    Some(spawned)
}

/// The generator's location. ACE dereferences `Generator.Location` without a check.
fn generator_location(w: &World, generator: ObjectGuid) -> Position {
    generator_of(w, generator)
        .location()
        .expect("System.NullReferenceException: Generator.Location")
}

fn set_location(w: &mut World, obj: ObjectGuid, location: Position) {
    if let Some(o) = w.objects.get_mut(obj) {
        o.set_location(Some(location));
    }
}

/// Spawns an object at a specific position.
// ACE: GeneratorProfile.Spawn_Specific
pub fn spawn_specific(w: &mut World, generator: ObjectGuid, index: usize, obj: ObjectGuid) -> bool {
    let Some(b) = profile(w, generator, index).map(|p| p.biota.clone()) else {
        return false;
    };
    let (ox, oy, oz) = (
        b.origin_x.unwrap_or(0.0),
        b.origin_y.unwrap_or(0.0),
        b.origin_z.unwrap_or(0.0),
    );
    let (ax, ay, az, aw) = (
        b.angles_x.unwrap_or(0.0),
        b.angles_y.unwrap_or(0.0),
        b.angles_z.unwrap_or(0.0),
        b.angles_w.unwrap_or(0.0),
    );

    // specific position
    let location = if b.obj_cell_id.unwrap_or(0) > 0 {
        Position::from_components(
            b.obj_cell_id.unwrap_or(0),
            ox,
            oy,
            oz,
            ax,
            ay,
            az,
            aw,
            false,
        )
    }
    // offset from generator location
    else {
        let g = generator_location(w, generator);
        if property_manager_get_bool_use_generator_rotation_offset(w) {
            let offset = Vector3::transform(Vector3::new(ox, oy, oz), g.rotation());

            if generator_of(w, generator).generator_type() == GeneratorType::Relative {
                let rotate = Quaternion::new(ax, ay, az, aw) * g.rotation();

                Position::from_components(
                    g.cell(),
                    g.position_x + offset.x,
                    g.position_y + offset.y,
                    g.position_z + offset.z,
                    rotate.x,
                    rotate.y,
                    rotate.z,
                    rotate.w,
                    false,
                )
            } else {
                Position::from_components(
                    g.cell(),
                    g.position_x + offset.x,
                    g.position_y + offset.y,
                    g.position_z + offset.z,
                    ax,
                    ay,
                    az,
                    aw,
                    false,
                )
            }
        } else {
            // ACE-BUG: `Generator.Location.PositionX + Biota.OriginX ?? 0` parses as
            // `(PositionX + OriginX) ?? 0`, so a null origin component puts that coordinate at 0
            // instead of at the generator's (only with use_generator_rotation_offset off).
            let add = |p: f32, o: Option<f32>| o.map_or(0.0, |o| p + o);
            Position::from_components(
                g.cell(),
                add(g.position_x, b.origin_x),
                add(g.position_y, b.origin_y),
                add(g.position_z, b.origin_z),
                ax,
                ay,
                az,
                aw,
                false,
            )
        }
    };
    set_location(w, obj, location);

    if !verify_landblock(w, generator, obj) || !verify_walkable_slope(w, obj) {
        return false;
    }

    crate::dispatch::enter_world::enter_world(w, obj)
}

// ACE: GeneratorProfile.Spawn_Scatter
pub fn spawn_scatter(w: &mut World, generator: ObjectGuid, index: usize, obj: ObjectGuid) -> bool {
    let Some(b) = profile(w, generator, index).map(|p| p.biota.clone()) else {
        return false;
    };
    #[allow(clippy::cast_possible_truncation)]
    let gen_radius = generator_of(w, generator)
        .get_property(PropertyFloat::GeneratorRadius)
        .unwrap_or(0.0) as f32;
    let mut location = Position::from_position(&generator_location(w, generator));

    // Skipping using same offset code above for offsetting scatter pos due to issues with rotation that were not expected at time content was rebuilt (Colo, others)
    // perhaps it should be same or similar but not able to spend time on verifying it out and making rotational adjustments at this time.

    // the following allows profile to offset from generators position, with no rotation changes, before then scattering from that position. Use case is mainly to spawn something higher or lower.

    if b.obj_cell_id.unwrap_or(0) == 0 {
        // if ObjCellId is specific, throw out that position (probably a linkable) and just use the generator's position else use the data as an offset. It is also possible that scatter always throws out all of it all cases.
        location.position_x += b.origin_x.unwrap_or(0.0);
        location.position_y += b.origin_y.unwrap_or(0.0);
        location.position_z += b.origin_z.unwrap_or(0.0);
    }

    location.position_z += 0.05;
    set_location(w, obj, location);

    // we are going to delay this scatter logic until the physics engine,
    // where the remnants of this function are in the client (SetScatterPositionInternal)

    // this is due to each randomized position being required to go through the full InitialPlacement process, to verify success
    // if InitialPlacement fails, then we retry up to maxTries

    let scatter_origin = w
        .objects
        .get(obj)
        .and_then(WorldObject::location)
        .map(|l| crate::physics::phys_ext::to_physics_position(&l));
    if let (Some(pos), Some(o)) = (scatter_origin, w.objects.get_mut(obj)) {
        o.wo.world_object.scatter_pos = Some(crate::world_objects::world_object::ScatterPos::new(
            pos, gen_radius,
        ));
    }

    let success = crate::dispatch::enter_world::enter_world(w, obj);

    if let Some(o) = w.objects.get_mut(obj) {
        o.wo.world_object.scatter_pos = None;
    }

    success
}

// ACE: GeneratorProfile.Spawn_Container
pub fn spawn_container(w: &mut World, generator: ObjectGuid, obj: ObjectGuid) -> bool {
    let success = generator_of(w, generator).is_container()
        && container_try_add_to_inventory(w, generator, obj);

    if !success {
        log::warn!(
            "[GENERATOR] {generator}:{} {}.Spawn_Container({}) - failed to add to container inventory",
            generator_of(w, generator).biota.weenie_class_id,
            name(w, generator),
            name(w, obj)
        );
    }

    success
}

// ACE: GeneratorProfile.Spawn_Shop
pub fn spawn_shop(w: &mut World, generator: ObjectGuid, obj: ObjectGuid) -> bool {
    // spawn item in vendor shop inventory
    if !generator_of(w, generator).is_vendor() {
        log::warn!(
            "[GENERATOR] {generator}:{} {}.Spawn_Shop({}) - generator is not a vendor type",
            generator_of(w, generator).biota.weenie_class_id,
            name(w, generator),
            name(w, obj)
        );
        return false;
    }

    crate::world_objects::vendor::add_default_item(w, generator, obj);
    true
}

// ACE: GeneratorProfile.Spawn_Default
pub fn spawn_default(w: &mut World, generator: ObjectGuid, obj: ObjectGuid) -> bool {
    // default location handler?
    //log.DebugFormat("{0}.Spawn_Default({1}): default handler for RegenLocationType {2}", _generator.Name, obj.Name, RegenLocationType);

    let location = Position::from_position(&generator_location(w, generator));
    set_location(w, obj, location);

    crate::dispatch::enter_world::enter_world(w, obj)
}

// ACE: GeneratorProfile.VerifyLandblock
#[must_use]
pub fn verify_landblock(w: &World, generator: ObjectGuid, obj: ObjectGuid) -> bool {
    let location = w.objects.get(obj).and_then(WorldObject::location);
    match location {
        Some(l) if l.landblock() == generator_location(w, generator).landblock() => true,
        //log.DebugFormat("{0}.VerifyLandblock({1}) - spawn location is invalid landblock", _generator.Name, obj.Name);
        _ => false,
    }
}

// ACE: GeneratorProfile.VerifyWalkableSlope
#[must_use]
pub fn verify_walkable_slope(w: &World, obj: ObjectGuid) -> bool {
    let location = w
        .objects
        .get(obj)
        .and_then(WorldObject::location)
        .expect("System.NullReferenceException: obj.Location");
    if !location.indoors()
        && !is_walkable(w, &location)
        && !VERIFY_WALKABLE_SLOPE_EXCLUDED_LANDBLOCKS.contains(&location.landblock_id().landblock())
    {
        //log.DebugFormat("{0}.VerifyWalkableSlope({1}) - spawn location is unwalkable slope", _generator.Name, obj.Name);
        return false;
    }
    true
}

/// A list of landblocks the excluded from VerifyWalkableSlope check.
///
/// TODO gmriggs: Hack until this can be looked into more.
// ACE: GeneratorProfile.VerifyWalkableSlopeExcludedLandblocks
pub const VERIFY_WALKABLE_SLOPE_EXCLUDED_LANDBLOCKS: [u16; 2] = [
    0x9EE5, // Northwatch Castle
    0xF92F, // Freebooter Keep
];

/// Generates a randomized treasure from LootGenerationFactory. The items join `World.objects`
/// here (4.10's factory returns owned objects, which ACE holds on no landblock yet).
// ACE: GeneratorProfile.TreasureGenerator
pub fn treasure_generator(
    w: &mut World,
    generator: ObjectGuid,
    index: usize,
) -> Option<Vec<ObjectGuid>> {
    let data_id = profile(w, generator, index)?.biota.weenie_class_id;

    // profile.WeenieClassId is not a weenieClassId,
    // it's a DeathTreasure or WieldedTreasure table DID
    // there is no overlap of DIDs between these 2 tables,
    // so they can be searched in any order..
    let death_treasure = w.content.get_cached_death_treasure(data_id);
    if let Some(death_treasure) = death_treasure {
        // TODO: get randomly generated death treasure from LootGenerationFactory
        //log.DebugFormat("{0}.TreasureGenerator(): found death treasure {1}", _generator.Name, Biota.WeenieClassId);
        let items = crate::factories::loot_generation_factory::create_random_loot_objects(
            w,
            &death_treasure,
        );
        return Some(insert_all(w, items));
    }

    // `GetCachedWieldedTreasure` returns a list, empty when nothing matches, never null: ACE's
    // `wieldedTreasure != null` test always passes and its "couldn't find" branch is dead code.
    let wielded_treasure = w.content.get_cached_wielded_treasure(data_id);

    // TODO: get randomly generated wielded treasure from LootGenerationFactory
    //log.DebugFormat("{0}.TreasureGenerator(): found wielded treasure {1}", _generator.Name, Biota.WeenieClassId);

    // roll into the wielded treasure table
    //var table = new TreasureWieldedTable(wieldedTreasure);
    crate::world_objects::world_object_equipment::generate_wielded_treasure_sets(
        w,
        &wielded_treasure,
    )
    .map(|items| insert_all(w, items))
}

/// Adds newly created objects to `World.objects`, in order, and returns their guids.
fn insert_all(w: &mut World, items: Vec<WorldObject>) -> Vec<ObjectGuid> {
    items
        .into_iter()
        .map(|wo| {
            let guid = wo.guid;
            if let Err(dup) = w.objects.insert(wo) {
                panic!("two live objects with guid {:?}", dup.guid);
            }
            crate::world_objects::creature::post_insert(w, guid);
            guid
        })
        .collect()
}

/// Removes all of the objects from a container for this profile.
// ACE: GeneratorProfile.RemoveTreasure
pub fn remove_treasure(w: &mut World, generator: ObjectGuid, index: usize) {
    if !generator_of(w, generator).is_container() {
        log::warn!(
            "[GENERATOR] {generator}:{} {}.RemoveTreasure(): container not found",
            generator_of(w, generator).biota.weenie_class_id,
            name(w, generator)
        );
        return;
    }
    let Some(keys) =
        profile(w, generator, index).map(|p| p.spawned.keys().copied().collect::<Vec<_>>())
    else {
        return;
    };
    for spawned in keys {
        let inventory_obj_guid = ObjectGuid::new(spawned);
        let Some(inventory_obj) =
            container_inventory_try_get_value(w, generator, inventory_obj_guid)
        else {
            log::warn!(
                "[GENERATOR] {generator}:{} {}.RemoveTreasure(): couldn't find {inventory_obj_guid}",
                generator_of(w, generator).biota.weenie_class_id,
                name(w, generator)
            );
            continue;
        };
        container_try_remove_from_inventory(w, generator, inventory_obj_guid);
        world_object::destroy(w, inventory_obj, true, false);
    }
    if let Some(p) = profile_mut(w, generator, index) {
        p.spawned.clear();
    }
}

/// Callback system for objects notifying their generators of events, ie. item pickup.
// ACE: GeneratorProfile.NotifyGenerator
pub fn notify_generator(
    w: &mut World,
    generator: ObjectGuid,
    index: usize,
    target: ObjectGuid,
    event_type: RegenerationType,
) {
    //log.DebugFormat("{0}.NotifyGenerator({1:X8}, {2})", _generator.Name, target, eventType);

    let Some(p) = profile(w, generator, index) else {
        return;
    };
    let Some(woi) = p.spawned.get(&target.full()).cloned() else {
        return;
    };

    let mut adj_event_type = event_type; // some generators use pickup when they mean to use destruction, some use destruction when they mean to use pickup. this data comes from 16py mostly and these issues are corrected below.
    let when_create = p.biota.when_create;
    let mut adj_when_create = p.biota.when_create;

    if event_type == RegenerationType::PickUp && when_create == RegenerationType::Destruction {
        adj_event_type = RegenerationType::Destruction;
    }

    if event_type == RegenerationType::Destruction && when_create == RegenerationType::PickUp {
        adj_event_type = RegenerationType::PickUp;
    }

    // If WhenCreate is Undef, assume it means Destruction (bad data)
    if event_type == RegenerationType::Destruction && when_create == RegenerationType::Undef {
        adj_when_create = RegenerationType::Destruction;
    }

    // If WhenCreate is Undef, assume it means Pickup (bad data)
    if event_type == RegenerationType::PickUp && when_create == RegenerationType::Undef {
        adj_when_create = RegenerationType::PickUp;
    }

    if when_create != adj_when_create {
        log::warn!(
            "[GENERATOR] {generator}:{}({}).GeneratorProfile[{}].NotifyGenerator: RegenerationType = {event_type:?}, WhenCreate = {when_create:?}, Using {adj_when_create:?} as WhenCreate instead",
            name(w, generator),
            generator_of(w, generator).biota.weenie_class_id,
            p.link_id()
        );
    }

    if adj_when_create != adj_event_type {
        return;
    }

    let delay = delay(w, generator, index);
    let utc_now = w.now.utc;
    let Some(p) = profile_mut(w, generator, index) else {
        return;
    };
    p.spawned.remove(&woi.guid.full());

    p.next_available = utc_now.add_seconds(f64::from(delay));
}

// ACE: GeneratorProfile.Reset
pub fn reset(w: &mut World, generator: ObjectGuid, index: usize) {
    let Some(nodes) =
        profile(w, generator, index).map(|p| p.spawned.values().cloned().collect::<Vec<_>>())
    else {
        return;
    };
    for r_node in nodes {
        let wo = r_node.try_get_world_object(w);

        if let Some(wo) = wo {
            if w.objects.get(wo).is_some_and(WorldObject::is_generator) {
                crate::dispatch::reset_generator::reset_generator(w, wo);
            }

            // `var container = Generator as Container; container?.TryRemoveFromInventory(wo.Guid);`
            if world_object_container_is(w, wo, generator)
                && generator_of(w, generator).is_container()
            {
                container_try_remove_from_inventory(w, generator, wo);
            }

            world_object::destroy(w, wo, true, false);
        }
    }

    cleanup_profile(w, generator, index);
}

// ACE: GeneratorProfile.KillAll
pub fn kill_all(w: &mut World, generator: ObjectGuid, index: usize) {
    let Some(nodes) =
        profile(w, generator, index).map(|p| p.spawned.values().cloned().collect::<Vec<_>>())
    else {
        return;
    };
    for r_node in nodes {
        let wo = r_node.try_get_world_object(w);

        if let Some(wo) = wo.filter(|&g| w.objects.get(g).is_some_and(WorldObject::is_creature)) {
            if !creature_is_dead(w, wo) {
                // `creature.Smite(Generator, true);` (Creature_Death.cs)
                crate::world_objects::creature_death::smite(w, wo, generator, true);
            }
        }
    }

    cleanup_profile(w, generator, index);
}

// ACE: GeneratorProfile.DestroyAll
pub fn destroy_all(
    w: &mut World,
    generator: ObjectGuid,
    index: usize,
    from_landblock_unload: bool,
) {
    let Some(nodes) =
        profile(w, generator, index).map(|p| p.spawned.values().cloned().collect::<Vec<_>>())
    else {
        return;
    };
    for r_node in nodes {
        let wo = r_node.try_get_world_object(w);

        if let Some(wo) = wo {
            let is_creature = w.objects.get(wo).is_some_and(WorldObject::is_creature);
            if !is_creature || !creature_is_dead(w, wo) {
                world_object::destroy(w, wo, true, from_landblock_unload);
            }
        }
    }

    cleanup_profile(w, generator, index);
}

// ACE: GeneratorProfile.CleanupProfile
fn cleanup_profile(w: &mut World, generator: ObjectGuid, index: usize) {
    let utc_now = w.now.utc;
    if let Some(p) = profile_mut(w, generator, index) {
        p.spawned.clear();
        p.spawn_queue.clear();

        p.next_available = utc_now;

        p.generated_treasure_item = false;
    }
    if let Some(g) = w.objects.get_mut(generator) {
        g.set_generated_treasure_item(false);
    }
}

// ---- pointers to members of other units (each a `not_ported!` site until its owner lands) ----

/// The virtual `Name`, for log lines.
fn name(w: &World, guid: ObjectGuid) -> String {
    if w.objects.contains(guid) {
        crate::dispatch::name::name(w, guid).unwrap_or_default()
    } else {
        String::new()
    }
}

/// `GuidManager.NewDynamicGuid()`.
fn guid_manager_new_dynamic_guid(w: &mut World) -> ObjectGuid {
    crate::managers::guid_manager::new_dynamic_guid(w)
}

/// `WorldObjectFactory.CreateNewWorldObject(uint weenieClassId)`: the cached weenie, a new dynamic
/// guid (taken only when the weenie exists, as ACE does), the constructor, and the guid recycled
/// when it builds nothing (`CreateNewWorldObject(Weenie)`); the new object joins `World.objects`.
fn world_object_factory_create_new_world_object(
    w: &mut World,
    weenie_class_id: u32,
) -> Option<ObjectGuid> {
    let weenie = w.content.get_cached_weenie(weenie_class_id)?;
    let guid = guid_manager_new_dynamic_guid(w);
    let Some(wo) = CtorEnv::with_world(w, |env| {
        world_object_factory::create_world_object(env, Some(weenie), guid)
    }) else {
        crate::managers::guid_manager::recycle_dynamic_guid(w, guid);
        return None;
    };
    let guid = wo.guid;
    if let Err(dup) = w.objects.insert(wo) {
        panic!("two live objects with guid {:?}", dup.guid);
    }
    crate::world_objects::creature::post_insert(w, guid);
    Some(guid)
}

/// `PropertyManager.GetBool("use_generator_rotation_offset").Item`.
fn property_manager_get_bool_use_generator_rotation_offset(w: &World) -> bool {
    crate::managers::property_manager::get_bool(w, "use_generator_rotation_offset", false, true)
        .item
}

/// `container.TryAddToInventory(obj)` (Container.cs), with ACE's defaults
/// (`placementPosition = 0`, `limitToMainPackOnly = false`, `burdenCheck = true`).
fn container_try_add_to_inventory(w: &mut World, container: ObjectGuid, obj: ObjectGuid) -> bool {
    crate::world_objects::container::try_add_to_inventory(w, container, obj, 0, false, true)
}

/// `container.TryRemoveFromInventory(guid)` (Container.cs; `forceSave = false`).
fn container_try_remove_from_inventory(w: &mut World, container: ObjectGuid, obj: ObjectGuid) {
    crate::world_objects::container::try_remove_from_inventory(w, container, obj, false);
}

/// `container.Inventory.TryGetValue(guid, out var obj)` (Container.cs).
fn container_inventory_try_get_value(
    w: &World,
    container: ObjectGuid,
    obj: ObjectGuid,
) -> Option<ObjectGuid> {
    let o = w.objects.get(container)?;
    crate::world_objects::container::inventory(o)
        .contains_key(&obj)
        .then_some(obj)
}

/// `wo.Container == Generator` (the `Container` field).
fn world_object_container_is(w: &World, wo: ObjectGuid, container: ObjectGuid) -> bool {
    w.objects
        .get(wo)
        .is_some_and(|o| o.wo.world_object_properties.container == Some(container))
}

/// `creature.IsDead` (`Monster_Combat.cs`).
fn creature_is_dead(w: &World, creature: ObjectGuid) -> bool {
    crate::world_objects::monster_combat::is_dead(
        w.objects
            .get(creature)
            .expect("ACE: creature is null (NullReferenceException)"),
    )
}
