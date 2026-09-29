// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Creature.cs
//! Port of `Source/ACE.Server/WorldObjects/Creature.cs`.

use empyrean_common::dotnet::{CsCast, DotNetDict};
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_dat::file_types::palette_set::PaletteSetExt;
use empyrean_dat::file_types::sex_cg::SexCgExt;
use empyrean_dat::file_types::PaletteSet;
use empyrean_entity::enums::{
    AceEnum, Gender, HeritageGroup, MovementType, PropertyDataId, PropertyInt, PropertyString,
    TargetingTactic,
};
use empyrean_entity::{ObjectGuid, Position};

use crate::network::motion::move_to_parameters::RetailMoveTo;
use crate::network::motion::movement_data::Motion;
use crate::world_objects::world_object::{CtorEnv, WorldObject};
use crate::world_objects::world_object_networking;
use crate::World;

/// Non-property fields declared in `Creature.cs`.
#[derive(Debug, Default)]
pub struct CreatureFields {
    /// ACE's `_questManager`: `Creature.QuestManager` creates it on first use
    /// (`managers/quest_manager.rs`).
    pub quest_manager: Option<crate::managers::quest_manager::QuestManager>,
    /// The players who have this creature selected, keyed by their guid (`WorldObjectInfo` holds
    /// a weak reference; here the guid, resolved on use).
    // ACE: Creature.selectedTargets
    pub selected_targets: DotNetDict<u32, ObjectGuid>,
    /// Not ACE: whether [`post_insert`] has run for this object (ACE's constructor runs once per
    /// object; the store can see one object inserted more than once).
    pub post_insert_done: bool,
}

fn object(w: &World, g: ObjectGuid) -> &WorldObject {
    w.objects.get(g).unwrap_or_else(|| {
        panic!(
            "System.NullReferenceException: object 0x{:08X} is not in World.objects",
            g.full()
        )
    })
}

fn fields_mut(w: &mut World, this: ObjectGuid) -> &mut CreatureFields {
    &mut w
        .objects
        .get_mut(this)
        .and_then(|o| o.creature.as_mut())
        .expect("ACE: this is a Creature")
        .creature
}

// ACE: Creature.IsExhausted
/// `Stamina.Current == 0`.
#[must_use]
pub fn is_exhausted(o: &WorldObject) -> bool {
    o.stamina().current(o) == 0
}

// ACE: Creature.IsNPC
/// verify logic: `!(this is Player) && !Attackable && TargetingTactic == TargetingTactic.None`.
#[must_use]
pub fn is_npc(o: &WorldObject) -> bool {
    !o.is_player() && !o.attackable() && o.targeting_tactic() == TargetingTactic::None
}

// ACE: Creature.IsAlive
/// This will be false when creature is dead and waits for respawn: `Health.Current > 0`.
#[must_use]
pub fn is_alive(o: &WorldObject) -> bool {
    o.health().current(o) > 0
}

/// `Enum.TryParse(value, true, out TEnum)` for an `int`-backed enum: a number (sign, digits,
/// trailing white space) or comma-separated member names matched ignoring case and OR-ed.
fn enum_try_parse_ignore_case<E: AceEnum>(value: &str) -> Option<i32> {
    let value = value.trim_start();
    let first = value.chars().next()?;
    if first.is_ascii_digit() || first == '-' || first == '+' {
        let digits = value.trim_end();
        if let Ok(v) = digits.parse::<i64>() {
            return i32::try_from(v).ok();
        }
        if digits
            .strip_prefix('+')
            .and_then(|d| d.parse::<i64>().ok())
            .is_some()
        {
            return None;
        }
    }
    let mut result: u64 = 0;
    let mut rest = value;
    loop {
        let (sub, next) = match rest.find(',') {
            None => (rest.trim(), None),
            Some(i) if i != rest.len() - 1 => (rest[..i].trim(), Some(&rest[i + 1..])),
            Some(_) => return None,
        };
        let i = E::MEMBER_NAMES
            .iter()
            .position(|n| n.eq_ignore_ascii_case(sub))?;
        result |= E::MEMBERS[i].key();
        match next {
            Some(n) => rest = n,
            None => break,
        }
    }
    Some((result & u64::from(u32::MAX)).cs_cast())
}

/// `(uint)ThreadSafeRandom.Next(0, count - 1)`.
fn next_index(count: usize) -> u32 {
    let max = i32::try_from(count).unwrap_or(i32::MAX) - 1;
    ThreadSafeRandom::next(0, max).cs_cast()
}

fn list_index<T: Copy>(list: &[T], i: u32) -> T {
    *usize::try_from(i)
        .ok()
        .and_then(|i| list.get(i))
        .expect("ArgumentOutOfRangeException: Index was out of range")
}

// ACE: Creature.GenerateNewFace
/// A random face for a non-player creature with a heritage and gender, from the character
/// generator's data; each appearance property the weenie already has is kept.
///
/// # Panics
/// Where ACE throws: a style index past its list, or a strip without a texture change.
pub fn generate_new_face(o: &mut WorldObject, env: &CtorEnv<'_>) {
    if o.get_property(PropertyInt::HeritageGroup).is_none() {
        if let Some(heritage_group_name) = o
            .get_property(PropertyString::HeritageGroup)
            .filter(|s| !s.is_empty())
        {
            if let Some(heritage) =
                enum_try_parse_ignore_case::<HeritageGroup>(&heritage_group_name.replace('\'', ""))
            {
                o.set_property(PropertyInt::HeritageGroup, heritage);
            }
        }
    }

    if o.get_property(PropertyInt::Gender).is_none() {
        if let Some(sex) = o
            .get_property(PropertyString::Sex)
            .filter(|s| !s.is_empty())
        {
            if let Some(gender) = enum_try_parse_ignore_case::<Gender>(&sex) {
                o.set_property(PropertyInt::Gender, gender);
            }
        }
    }

    let (Some(heritage), Some(gender)) = (
        o.get_property(PropertyInt::HeritageGroup),
        o.get_property(PropertyInt::Gender),
    ) else {
        return;
    };

    // `DatManager.PortalDat.CharGen` (read first in ACE; a synthetic FakeDats may lack it, which
    // the retail dat never does: then no face, as for an unknown heritage)
    let Some(cg) = env.w.dats.portal_dat().try_char_gen() else {
        return;
    };

    let heritage_key: u32 = heritage.cs_cast();
    let gender_key: u32 = gender.cs_cast();
    let Some(sex) = cg
        .heritage_groups
        .get(&heritage_key)
        .and_then(|h| (gender >= 0).then(|| h.sexes.get(&gender_key)).flatten())
    else {
        log::debug!(
            "Creature.GenerateNewFace: {} (0x{:08X}) - wcid {} - Heritage: {heritage} | Gender: {gender} - Data invalid, Cannot randomize face.",
            o.get_property(PropertyString::Name).unwrap_or_default(),
            o.guid.full(),
            o.weenie_class_id()
        );
        return;
    };

    o.set_property(PropertyDataId::PaletteBase, sex.base_palette.0);

    // Get the hair first, because we need to know if you're bald, and that's the name of that tune!
    let hair_style: u32 = if sex.hair_styles.len() > 1 {
        if crate::managers::property_manager::get_bool(
            env.w,
            "npc_hairstyle_fullrange",
            false,
            true,
        )
        .item
        {
            next_index(sex.hair_styles.len())
        } else {
            let count_minus_one = i32::try_from(sex.hair_styles.len()).unwrap_or(i32::MAX) - 1;
            ThreadSafeRandom::next(0, count_minus_one.min(8)).cs_cast() // retail range data compiled by OptimShi
        }
    } else {
        0
    };

    if (sex.hair_styles.len() as u64) < u64::from(hair_style) {
        log::warn!(
            "Creature.GenerateNewFace: {} (0x{:08X}) - wcid {} - HairStyle = {hair_style} | HairStyleList.Count = {} - Data invalid, Cannot randomize face.",
            o.get_property(PropertyString::Name).unwrap_or_default(),
            o.guid.full(),
            o.weenie_class_id(),
            sex.hair_styles.len()
        );
        return;
    }

    let hairstyle = usize::try_from(hair_style)
        .ok()
        .and_then(|i| sex.hair_styles.get(i))
        .expect("ArgumentOutOfRangeException: Index was out of range");

    let hair_color = next_index(sex.hair_colors.len());
    let hair_hue = ThreadSafeRandom::next_float(0.0, 1.0);

    let eye_color = next_index(sex.eye_colors.len());
    let eyes = next_index(sex.eye_strips.len());

    let mouth = next_index(sex.mouth_strips.len());

    let nose = next_index(sex.nose_strips.len());

    let skin_hue = ThreadSafeRandom::next_float(0.0, 1.0);

    //// Certain races (Undead, Tumeroks, Others?) have multiple body styles available. This is controlled via the "hair style".
    ////if (hairstyle.AlternateSetup > 0)
    ////    character.SetupTableId = hairstyle.AlternateSetup;

    let bald = hairstyle.bald != 0;
    let ok = |r: Result<u32, empyrean_dat::AceThrow>| r.unwrap_or_else(|e| panic!("{e}"));
    if o.get_property(PropertyDataId::EyesTexture).is_none() {
        o.set_property(
            PropertyDataId::EyesTexture,
            ok(sex.get_eye_texture(eyes, bald)),
        );
    }
    if o.get_property(PropertyDataId::DefaultEyesTexture).is_none() {
        o.set_property(
            PropertyDataId::DefaultEyesTexture,
            ok(sex.get_default_eye_texture(eyes, bald)),
        );
    }
    if o.get_property(PropertyDataId::NoseTexture).is_none() {
        o.set_property(PropertyDataId::NoseTexture, ok(sex.get_nose_texture(nose)));
    }
    if o.get_property(PropertyDataId::DefaultNoseTexture).is_none() {
        o.set_property(
            PropertyDataId::DefaultNoseTexture,
            ok(sex.get_default_nose_texture(nose)),
        );
    }
    if o.get_property(PropertyDataId::MouthTexture).is_none() {
        o.set_property(
            PropertyDataId::MouthTexture,
            ok(sex.get_mouth_texture(mouth)),
        );
    }
    if o.get_property(PropertyDataId::DefaultMouthTexture)
        .is_none()
    {
        o.set_property(
            PropertyDataId::DefaultMouthTexture,
            ok(sex.get_default_mouth_texture(mouth)),
        );
    }
    if o.get_property(PropertyDataId::HeadObject).is_none() {
        // `HeadObjectDID = null` removes nothing that is there
        if let Some(head_object) = sex
            .get_head_object(hair_style)
            .unwrap_or_else(|e| panic!("{e}"))
        {
            o.set_property(PropertyDataId::HeadObject, head_object);
        }
    }

    // Skin is stored as PaletteSet (list of Palettes), so we need to read in the set to get the specific palette
    // (ReadFromDat answers an empty PaletteSet for a missing file, whose GetPaletteID is 0.)
    let skin_pal_set = env
        .w
        .dats
        .portal_dat()
        .read_from_dat::<PaletteSet>(sex.skin_palset.0);
    if o.get_property(PropertyDataId::SkinPalette).is_none() {
        o.set_property(
            PropertyDataId::SkinPalette,
            skin_pal_set.map_or(0, |s| s.get_palette_id(skin_hue)),
        );
    }

    // Hair is stored as PaletteSet (list of Palettes), so we need to read in the set to get the specific palette
    let hair_pal_set = env
        .w
        .dats
        .portal_dat()
        .read_from_dat::<PaletteSet>(list_index(&sex.hair_colors, hair_color));
    if o.get_property(PropertyDataId::HairPalette).is_none() {
        o.set_property(
            PropertyDataId::HairPalette,
            hair_pal_set.map_or(0, |s| s.get_palette_id(hair_hue)),
        );
    }

    // Eye Color
    if o.get_property(PropertyDataId::EyesPalette).is_none() {
        o.set_property(
            PropertyDataId::EyesPalette,
            list_index(&sex.eye_colors, eye_color),
        );
    }
}

// ACE: Creature.MoveToObject
/// Sends the network commands to move a player towards an object, with the flag word of `kind`.
pub fn move_to_object(
    w: &mut World,
    this: ObjectGuid,
    target: ObjectGuid,
    use_radius: Option<f32>,
    kind: RetailMoveTo,
) {
    let distance_to_object =
        use_radius.unwrap_or_else(|| object(w, target).use_radius().unwrap_or(0.6));

    let mut move_to_object = Motion::to_object(w, this, target, MovementType::MoveToObject);
    move_to_object.move_to_parameters.distance_to_object = distance_to_object;
    // Not ACE's (the retail captures, V257): the flag word follows the kind of move (a pickup the
    // defaults, a use UseFinalHeading, a portal no UseSpheres, a melee approach the attack
    // chase); ACE sends the defaults here.
    kind.apply(&mut move_to_object.move_to_parameters);

    // move directly to portal origin
    //if (target is Portal)
    //moveToObject.MoveToParameters.MovementParameters &= ~MovementParams.UseSpheres;

    let target_location = object(w, target)
        .location()
        .expect("System.NullReferenceException: target.Location");
    set_walk_run_threshold(w, this, &mut move_to_object, &target_location);

    world_object_networking::enqueue_broadcast_motion(w, this, &move_to_object, None, None);
}

// ACE: Creature.MoveToPosition
/// Sends the network commands to move a player towards a position (a portal's), with the flag
/// word of `kind`.
pub fn move_to_position(w: &mut World, this: ObjectGuid, position: &Position, kind: RetailMoveTo) {
    let mut move_to_position = Motion::to_position(w, this, position);
    move_to_position.move_to_parameters.distance_to_object = 0.0;
    // Not ACE's (the retail captures, V257): a move to use a portal carried 0x1EA4F (UseFinalHeading
    // set, UseSpheres clear); ACE sends the defaults here.
    kind.apply(&mut move_to_position.move_to_parameters);

    set_walk_run_threshold(w, this, &mut move_to_position, position);

    world_object_networking::enqueue_broadcast_motion(w, this, &move_to_position, None, None);
}

// ACE: Creature.SetWalkRunThreshold
pub fn set_walk_run_threshold(
    w: &mut World,
    this: ObjectGuid,
    motion: &mut Motion,
    target_location: &Position,
) {
    // FIXME: WalkRunThreshold (default 15 distance) seems to not be used automatically by client
    // player will always walk instead of run, and if MovementParams.CanCharge is sent, they will always charge
    // to remedy this, we manually calculate a threshold based on WalkRunThreshold

    let location = object(w, this)
        .location()
        .expect("System.NullReferenceException: Location");
    let dist = location.distance_to(target_location);
    if dist >= motion.move_to_parameters.walk_run_threshold / 2.0 {
        // default 15 distance seems too far, especially with weird in-combat walking animation?
        // Not ACE's (the retail captures, V257): retail never set CanCharge on a use-move (0 of
        // 9,149 at 7.5 m or more); ACE sets it here. The run rate stays.

        // TODO: find the correct runrate here
        // the default runrate / charge seems much too fast...
        //motion.RunRate = GetRunRate() / 4.0f;
        motion.run_rate = crate::world_objects::monster_navigation::get_run_rate(w, this);
    }
}

// ACE: Creature.OnTargetSelected
/// Called when a player selects a target.
pub fn on_target_selected(w: &mut World, this: ObjectGuid, player: ObjectGuid) -> bool {
    let targets = &mut fields_mut(w, this).selected_targets;
    if targets.contains_key(&player.full()) {
        return false;
    }
    targets.insert(player.full(), player);
    true
}

// ACE: Creature.OnTargetDeselected
/// Called when a player deselects a target.
pub fn on_target_deselected(w: &mut World, this: ObjectGuid, player: ObjectGuid) -> bool {
    fields_mut(w, this)
        .selected_targets
        .remove(&player.full())
        .is_some()
}

// ACE: Creature.OnHealthUpdate
/// Called when a creature's health changes: each player that has it selected gets its health.
pub fn on_health_update(w: &mut World, this: ObjectGuid) {
    let Some(c) = w.objects.get(this).and_then(|o| o.creature.as_ref()) else {
        return;
    };
    let targets: Vec<(u32, ObjectGuid)> = c
        .creature
        .selected_targets
        .iter()
        .map(|(k, v)| (*k, *v))
        .collect();

    for (key, player) in targets {
        let is_player = w.objects.get(player).is_some_and(WorldObject::is_player);
        let session = if is_player {
            crate::managers::player_manager::player_session(w, player)
        } else {
            None
        };

        if let Some(session) = session {
            crate::world_objects::world_object::query_health(w, this, session);
        } else {
            fields_mut(w, this).selected_targets.remove(&key);
        }
    }
}

// ---- virtual-dispatch targets ----

// ACE: Creature.GetBurdenMod
#[allow(unused_variables)]
pub fn creature_get_burden_mod(w: &mut crate::World, this: empyrean_entity::ObjectGuid) -> f32 {
    1.0 // override for players
}

// ACE: Creature.ActOnUse
/// Handled in base.OnActivate -> EmoteManager.OnUse().
#[allow(unused_variables)]
pub fn creature_act_on_use(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    activator: empyrean_entity::ObjectGuid,
) {
    // handled in base.OnActivate -> EmoteManager.OnUse()
}

// ACE: Creature.OnCollideObject
pub fn creature_on_collide_object(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    target: empyrean_entity::ObjectGuid,
) {
    let Some(t) = w.objects.get(target) else {
        return;
    };
    if t.report_collisions() == Some(false) {
        return;
    }

    if t.is_door() {
        crate::world_objects::door::door_on_collide_object(w, target, this);
    } else if t.is_hotspot() {
        crate::world_objects::hotspot::hotspot_on_collide_object(w, target, this);
    }
}

// ACE: Creature.GetNaturalResistance
#[allow(unused_variables)]
pub fn creature_get_natural_resistance(
    w: &crate::World,
    this: empyrean_entity::ObjectGuid,
    damage_type: empyrean_entity::enums::DamageType,
) -> f32 {
    // overridden for players
    1.0
}

// ---- constructors and SetEphemeralValues ----

/// `new Creature(weenie, guid)` / `new Creature(biota)`: the `Container` constructor, then
/// Creature's `InitializePropertyDictionaries` and `SetEphemeralValues`.
// ACE: Creature.Creature
pub fn creature_ctor(
    o: &mut crate::world_objects::world_object::WorldObject,
    env: &crate::world_objects::world_object::CtorEnv<'_>,
    src: crate::world_objects::world_object::CtorSource,
) {
    crate::world_objects::container::container_ctor(o, env, src);
    creature_initialize_property_dictionaries(o);
    creature_set_ephemeral_values(o, env);
}

// ACE: Creature.InitializePropertyDictionaries
fn creature_initialize_property_dictionaries(
    o: &mut crate::world_objects::world_object::WorldObject,
) {
    use empyrean_common::dotnet::DotNetDict;

    if o.biota.properties_attribute.is_none() {
        o.biota.properties_attribute = Some(DotNetDict::new());
    }
    if o.biota.properties_attribute_2nd.is_none() {
        o.biota.properties_attribute_2nd = Some(DotNetDict::new());
    }
    if o.biota.properties_body_part.is_none() {
        o.biota.properties_body_part = Some(std::sync::Arc::new(DotNetDict::new()));
    }
    if o.biota.properties_skill.is_none() {
        o.biota.properties_skill = Some(DotNetDict::new());
    }
}

/// The creature's runtime state: combat mode, damage history, face, vitals, attributes and
/// skills, wielded and inventory treasure, monster state and motion, in ACE's order. The wield
/// list and treasure steps are `not_ported!` pointers here: they create and insert objects, which
/// needs the creature in the world, and the constructor runs before that.
// ACE: Creature.SetEphemeralValues
fn creature_set_ephemeral_values(
    o: &mut crate::world_objects::world_object::WorldObject,
    env: &crate::world_objects::world_object::CtorEnv<'_>,
) {
    if let Some(c) = o.creature.as_mut() {
        c.creature_combat.combat_mode = empyrean_entity::enums::CombatMode::NonCombat;
    }
    // `DamageHistory = new DamageHistory(this);` (held in `CreatureDeathFields`)
    let history = crate::entity::damage_history::DamageHistory::new(o.guid, env.w.now.utc);
    if let Some(c) = o.creature.as_mut() {
        c.creature_death.damage_history = history;
    }

    if !o.is_player() {
        generate_new_face(o, env);
    }

    // The vitals, attributes and skills (`new CreatureVital`/`CreatureAttribute`/`CreatureSkill`,
    // which add missing biota records), then `if (Health.Current <= 0) Health.Current =
    // Health.MaxValue;` and the same for Stamina and Mana, in ACE's order.
    crate::world_objects::creature_vitals::set_ephemeral_stat_values(env.w, o);

    if !o.is_player() {
        // DIVERGE: GenerateWieldList, EquipInventoryItems, GenerateWieldedTreasure,
        // EquipInventoryItems and GenerateInventoryTreasure create objects and resolve them by
        // guid through `World.objects`, which the constructor cannot reach; they run in
        // `post_insert`, as soon as the creature joins the store (before any landblock sees it).

        // TODO: fix tod data
        crate::world_objects::creature_vitals::fill_vitals_to_max(env.w, o);
    }

    crate::world_objects::monster::set_monster_state(o);

    o.wo.world_object_properties.current_motion_state =
        Some(crate::network::motion::movement_data::Motion::new(
            empyrean_entity::enums::MotionStance::NonCombat,
            empyrean_entity::enums::MotionCommand::Ready,
            1.0,
        ));

    // `selectedTargets = new Dictionary<uint, WorldObjectInfo>();`
    if let Some(c) = o.creature.as_mut() {
        c.creature.selected_targets = DotNetDict::new();
    }
}

/// The object-creating half of `Creature.SetEphemeralValues`, run once a newly constructed
/// creature (not a player) has joined `World.objects`: the wield list and wielded treasure are
/// generated and equipped, the inventory treasure generated, and the vitals filled again, in
/// ACE's order. Every site that inserts a newly constructed object calls this (it does nothing
/// for anything else, and nothing the second time for the same object).
// ACE: Creature.SetEphemeralValues
// DIVERGE: ACE runs these steps inside the constructor, before the creature is anywhere; here
// they run just after its insertion into the store, still before a landblock or generator places
// it. The same RNG draws are made in the same order for one creature; a landblock's batch makes
// them when its objects are handed to the store rather than when the factory built them.
pub fn post_insert(w: &mut World, this: ObjectGuid) {
    // `Container(Biota)`'s own inventory load (any container rebuilt from a saved biota)
    crate::world_objects::container::post_insert_load_inventory(w, this);
    // `Container(Weenie)`'s own contain list (any non-creature container built from its weenie)
    crate::world_objects::container::post_insert_generate_contain_list(w, this);

    let Some(o) = w.objects.get_mut(this) else {
        return;
    };
    if o.is_player() {
        return;
    }
    let Some(c) = o.creature.as_mut() else { return };
    if c.creature.post_insert_done {
        return;
    }
    c.creature.post_insert_done = true;

    crate::world_objects::creature_equipment::generate_wield_list(w, this);

    crate::world_objects::monster_inventory::equip_inventory_items(w, this, false);

    crate::world_objects::creature_equipment::generate_wielded_treasure(w, this);

    crate::world_objects::monster_inventory::equip_inventory_items(w, this, false);

    crate::world_objects::creature_equipment::generate_inventory_treasure(w, this);

    // TODO: fix tod data
    // (`Health.Current = Health.MaxValue;` and the same for Stamina and Mana, now over what the
    // creature wields)
    let o = object(w, this);
    for vital in [o.health(), o.stamina(), o.mana()] {
        let max = vital.max_value(
            &mut crate::world_objects::entity::creature_attribute::StatCtx::in_world(w, this),
        );
        if let Some(o) = w.objects.get_mut(this) {
            vital.set_current(o, max);
        }
    }
}
