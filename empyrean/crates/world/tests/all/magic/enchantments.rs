//! Vectors: fixtures/vectors/enchantments/
//! EnchantmentManager(+caching) and AddEnchantmentResult: add/refresh/survivor per category, ACE
//! vectors with retail ordering.
//! Fixture: ACE vectors and explicit expected values, synthetic dats, isolated world state.

use empyrean_common::era::EraExt as _;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use dereth_assets::tables::{SpellBase, SpellSet};
use dereth_assets::SpellTable;
use dereth_primitives::DataId;
use empyrean_common::clock::ClockSnapshot;
use empyrean_common::dotnet::datetime::DotNetDateTime;
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_content::models::world::Spell as DbSpell;
use empyrean_content::MemContent;
use empyrean_dat::FakeDats;
use empyrean_entity::enums::properties::{
    PropertyAttribute, PropertyAttribute2nd, PropertyFloat, PropertyInt,
};
use empyrean_entity::enums::{
    DamageType, EnchantmentTypeFlags as F, MagicSchool, Skill, SpellCategory,
};
use empyrean_entity::models::PropertiesEnchantmentRegistry;
use empyrean_entity::ObjectGuid;
use empyrean_net::SessionId;
use empyrean_world::dispatch::Class;
use empyrean_world::entity::spell::Spell;
use empyrean_world::managers::player_manager::OnlinePlayer;
use empyrean_world::network::game_messages::game_message::{start_capture, take_sent};
use empyrean_world::sessions::SessionData;
use empyrean_world::world_objects::managers::enchantment_manager::{self as em, StackType};
use empyrean_world::world_objects::managers::enchantment_manager_with_caching as emc;
use empyrean_world::world_objects::world_object::WorldObject;
use empyrean_world::World;

// ------------------------------------------------------------------ fixture

const S: SessionId = SessionId {
    client_id: 1,
    generation: 1,
};
const PLAYER: ObjectGuid = ObjectGuid::new(0x5000_0001);
const OTHER_PLAYER: ObjectGuid = ObjectGuid::new(0x5000_0002);
const MONSTER: ObjectGuid = ObjectGuid::new(0x8000_0001);
const ITEM: ObjectGuid = ObjectGuid::new(0x8000_0002);

const ADD_ATTR: F = F(F::Attribute.0 | F::SingleStat.0 | F::Additive.0);
const MUL_ATTR: F = F(F::Attribute.0 | F::SingleStat.0 | F::Multiplicative.0);
const ADD_SKILL: F = F(F::Skill.0 | F::SingleStat.0 | F::Additive.0);
const MUL_FLOAT: F = F(F::Float.0 | F::SingleStat.0 | F::Multiplicative.0);
const ADD_INT: F = F(F::Int.0 | F::SingleStat.0 | F::Additive.0);

/// One test spell: dat half and world-database half.
struct Def {
    id: u32,
    category: u32,
    power: u32,
    duration: f64,
    beneficial: bool,
    school: u32,
    stat: F,
    key: u32,
    val: f32,
}

#[allow(clippy::too_many_arguments)]
const fn d(
    id: u32,
    category: u32,
    power: u32,
    duration: f64,
    beneficial: bool,
    stat: F,
    key: u32,
    val: f32,
) -> Def {
    Def {
        id,
        category,
        power,
        duration,
        beneficial,
        school: 4,
        stat,
        key,
        val,
    }
}

/// Strength (category 1), Weakness (2), a multiplicative Strength (3), skills, resistances and
/// ratings. Ids are made up except 666 (Vitae).
fn defs() -> Vec<Def> {
    let str_ = u32::from(PropertyAttribute::Strength.0);
    vec![
        d(1, 1, 10, 1800.0, true, ADD_ATTR, str_, 10.0), // Strength Self I
        d(2, 1, 50, 1800.0, true, ADD_ATTR, str_, 15.0), // Strength Self II
        d(3, 1, 50, 900.0, true, ADD_ATTR, str_, 15.0),  // a same-power rival, shorter
        d(4, 1, 50, 3600.0, true, ADD_ATTR, str_, 15.0), // a same-power rival, longer
        d(5, 2, 10, 1800.0, false, ADD_ATTR, str_, -10.0), // Weakness Other I
        d(6, 3, 10, 1800.0, true, MUL_ATTR, str_, 1.25), // a multiplicative strength
        d(
            7,
            4,
            10,
            30.0,
            true,
            ADD_ATTR,
            u32::from(PropertyAttribute::Self_.0),
            5.0,
        ), // short-lived
        d(
            8,
            5,
            10,
            1800.0,
            true,
            ADD_SKILL,
            Skill::MeleeDefense.0.cast_unsigned(),
            20.0,
        ),
        d(
            9,
            6,
            10,
            1800.0,
            false,
            F(F::Skill.0 | F::Additive.0 | F::DefenseSkills.0),
            0,
            -7.6,
        ), // as Unbalancing Blow (0x28010)
        d(
            10,
            7,
            10,
            1800.0,
            true,
            MUL_FLOAT,
            u32::from(PropertyFloat::ResistFire.0),
            0.5,
        ),
        d(
            11,
            8,
            10,
            1800.0,
            false,
            MUL_FLOAT,
            u32::from(PropertyFloat::ResistFire.0),
            2.0,
        ),
        d(
            12,
            9,
            10,
            1800.0,
            true,
            F(F::BodyArmorValue.0 | F::SingleStat.0 | F::Additive.0),
            0,
            50.0,
        ),
        d(
            13,
            10,
            10,
            1800.0,
            false,
            F(F::BodyArmorValue.0 | F::SingleStat.0 | F::Additive.0),
            0,
            -30.0,
        ),
        d(
            14,
            11,
            10,
            1800.0,
            true,
            ADD_INT,
            u32::from(PropertyInt::DamageRating.0),
            4.5,
        ),
        d(
            15,
            615,
            10,
            1800.0,
            true,
            F(F::Float.0 | F::SingleStat.0 | F::Multiplicative.0),
            0,
            1.1,
        ), // TrinketXPRaising
        d(
            16,
            615,
            20,
            1800.0,
            true,
            F(F::Float.0 | F::SingleStat.0 | F::Additive.0),
            0,
            0.2,
        ),
        d(
            17,
            12,
            10,
            1800.0,
            true,
            F(F::SecondAtt.0 | F::SingleStat.0 | F::Additive.0),
            u32::from(PropertyAttribute2nd::MaxHealth.0),
            25.0,
        ),
        d(
            666,
            204,
            0,
            0.0,
            false,
            F(F::MultipleStat.0 | F::Multiplicative.0 | F::Vitae.0),
            0,
            1.0,
        ),
    ]
}

fn spell_base(def: &Def) -> SpellBase {
    SpellBase {
        name: format!("Spell {}", def.id),
        description: String::new(),
        school: def.school,
        icon: 0,
        category: def.category,
        bitfield: if def.beneficial { 0x4 } else { 0 },
        base_mana: 0,
        base_range_constant: 0.0,
        base_range_mod: 0.0,
        power: i32::try_from(def.power).expect("a small power"),
        spell_economy_mod: 1.0,
        formula_version: 0,
        component_loss: 0.0,
        meta_spell_type: 1,
        meta_spell_id: def.id,
        duration: Some((def.duration, 0.0, 0.0)),
        portal_lifetime: None,
        raw_comps: [0; 8],
        comp_key: 0,
        comps: Vec::new(),
        caster_effect: 0,
        target_effect: 0,
        fizzle_effect: 0,
        recovery_interval: 0.0,
        recovery_amount: 0.0,
        display_order: 0,
        non_component_target_type: 0,
        mana_mod: 0,
    }
}

fn db_spell(def: &Def) -> DbSpell {
    DbSpell {
        id: def.id,
        name: format!("Spell {}", def.id),
        stat_mod_type: Some(def.stat.0.cast_unsigned()),
        stat_mod_key: Some(def.key),
        stat_mod_val: Some(def.val),
        ..DbSpell::default()
    }
}

fn world_with(defs: &[Def], extra_db: Vec<DbSpell>, spellsets: BTreeMap<u32, SpellSet>) -> World {
    let table = SpellTable {
        id: DataId(0x0E00_000E),
        spell_buckets: 64,
        spells: defs.iter().map(|d| (d.id, spell_base(d))).collect(),
        spellset_bucket_index: 1,
        spellsets,
    };
    let dats = FakeDats::new()
        .with_spell_table(table)
        .build()
        .expect("fake dats");
    let now = ClockSnapshot {
        portal_year_ticks: 0.0,
        unix_time: 1_790_000_000.0,
        utc: DotNetDateTime::new(2026, 9, 1),
        monotonic: Duration::ZERO,
    };
    let mut w = World::new(now, dats);
    let mut content = MemContent::new();
    for def in defs
        .iter()
        .filter(|d| extra_db.iter().all(|r| r.id != d.id))
    {
        content = content.spell(db_spell(def));
    }
    for row in extra_db {
        content = content.spell(row);
    }
    w.content = Arc::new(content);

    for (guid, class) in [
        (PLAYER, Class::Player),
        (OTHER_PLAYER, Class::Player),
        (MONSTER, Class::Creature),
        (ITEM, Class::GenericObject),
    ] {
        let mut o = WorldObject::allocate(class);
        o.guid = guid;
        o.biota.id = guid.full();
        o.biota.properties_enchantment_registry = Some(Vec::new());
        w.objects.insert(o).expect("fresh guid");
    }
    w.objects.get_mut(PLAYER).unwrap().set_level(Some(10));
    w.sessions.insert(
        S,
        SessionData {
            player: Some(PLAYER),
            ..SessionData::default()
        },
    );
    w
}

fn world() -> World {
    world_with(&defs(), Vec::new(), BTreeMap::new())
}

fn spell(w: &World, id: u32) -> Spell {
    Spell::new(w, id, true)
}

fn cast(
    w: &mut World,
    target: ObjectGuid,
    id: u32,
    caster: ObjectGuid,
) -> empyrean_world::entity::add_enchantment_result::AddEnchantmentResult {
    let s = spell(w, id);
    emc::add(w, target, &s, Some(caster), None, false, false)
}

fn registry(w: &World, this: ObjectGuid) -> Vec<PropertiesEnchantmentRegistry> {
    w.objects
        .get(this)
        .unwrap()
        .biota
        .properties_enchantment_registry
        .clone()
        .unwrap()
}

fn layers(w: &World, this: ObjectGuid) -> Vec<(i32, u16, u32)> {
    registry(w, this)
        .iter()
        .map(|e| (e.spell_id, e.layer_id, e.caster_object_id))
        .collect()
}

/// `(opcode, game event type or 0)` of each captured send.
fn sent_kinds() -> Vec<(u32, u32)> {
    take_sent()
        .into_iter()
        .map(|(_, _, b)| {
            let op = u32::from_le_bytes(b[0..4].try_into().unwrap());
            let ev = if op == 0xF7B0 {
                u32::from_le_bytes(b[12..16].try_into().unwrap())
            } else {
                0
            };
            (op, ev)
        })
        .collect()
}

const EV_UPDATE: (u32, u32) = (0xF7B0, 0x02C2);
const EV_REMOVE: (u32, u32) = (0xF7B0, 0x02C3);
const EV_DISPEL: (u32, u32) = (0xF7B0, 0x02C7);
const EV_DISPEL_MULTI: (u32, u32) = (0xF7B0, 0x02C8);
const SOUND: (u32, u32) = (0xF750, 0);

// ------------------------------------------------------------------ stacking (Add / BuildStack)

/// `Add`: no entry in the category writes layer 1 (`StackType.Initial`); a higher power surpasses
/// onto the next layer; the top layer wins the aggregation.
#[test]
fn higher_power_surpasses_onto_the_next_layer() {
    let mut w = world();
    let r = cast(&mut w, MONSTER, 1, PLAYER);
    assert_eq!(r.stack_type, StackType::Initial);
    assert_eq!(r.enchantment.as_ref().unwrap().layer_id, 1);
    assert!(r.surpass.is_empty() && r.surpass_spell.is_none());

    let r = cast(&mut w, MONSTER, 2, PLAYER);
    assert_eq!(r.stack_type, StackType::Surpass);
    assert_eq!(r.surpass_spell.as_ref().unwrap().name(), "Spell 1");
    assert_eq!(r.enchantment.as_ref().unwrap().layer_id, 2);
    assert_eq!(
        layers(&w, MONSTER),
        [(1, 1, PLAYER.full()), (2, 2, PLAYER.full())]
    );
    assert_eq!(
        emc::get_attribute_mod_additive(&mut w, MONSTER, PropertyAttribute::Strength),
        15
    );

    // A lower power afterwards is surpassed, but still written to a new layer.
    let r = cast(&mut w, MONSTER, 1, OTHER_PLAYER);
    assert_eq!(r.stack_type, StackType::Surpassed);
    assert_eq!(r.surpassed_spell.as_ref().unwrap().name(), "Spell 2");
    assert_eq!(r.enchantment.as_ref().unwrap().layer_id, 3);
    assert_eq!(
        emc::get_attribute_mod_additive(&mut w, MONSTER, PropertyAttribute::Strength),
        15
    );
}

/// The same spell from the same caster refreshes its own entry (StartTime back to 0 when the new
/// duration beats the time remaining); from a different caster it is a new layer, still
/// `StackType.Refresh` (`RefreshCaster` is null, so `Add` takes the new-layer path).
#[test]
fn same_spell_refreshes_per_caster() {
    let mut w = world();
    cast(&mut w, MONSTER, 1, PLAYER);
    em::heart_beat(&mut w, MONSTER, 5.0);
    em::heart_beat(&mut w, MONSTER, 5.0);
    assert_eq!(registry(&w, MONSTER)[0].start_time, -10.0);

    let r = cast(&mut w, MONSTER, 1, PLAYER);
    assert_eq!(r.stack_type, StackType::Refresh);
    assert_eq!(r.refresh_spell.as_ref().unwrap().name(), "Spell 1");
    assert_eq!(
        r.refresh_caster.as_ref().unwrap().caster_object_id,
        PLAYER.full()
    );
    let reg = registry(&w, MONSTER);
    assert_eq!(reg.len(), 1);
    assert_eq!(
        (reg[0].start_time, reg[0].duration, reg[0].layer_id),
        (0.0, 1800.0, 1)
    );
    assert_eq!(r.enchantment.as_ref().unwrap().start_time, 0.0);

    let r = cast(&mut w, MONSTER, 1, OTHER_PLAYER);
    assert_eq!(r.stack_type, StackType::Refresh);
    assert!(r.refresh_caster.is_none());
    assert_eq!(
        layers(&w, MONSTER),
        [(1, 1, PLAYER.full()), (1, 2, OTHER_PLAYER.full())]
    );
}

/// A refresh whose duration does not beat `Duration + StartTime` leaves the entry alone.
#[test]
fn refresh_keeps_a_longer_remaining_time() {
    let mut w = world();
    cast(&mut w, MONSTER, 1, PLAYER);
    // Stretch the entry so the remaining time (4000 - 0) beats the spell's 1800.
    w.objects
        .get_mut(MONSTER)
        .unwrap()
        .biota
        .properties_enchantment_registry
        .as_mut()
        .unwrap()[0]
        .duration = 4000.0;
    em::heart_beat(&mut w, MONSTER, 5.0);
    cast(&mut w, MONSTER, 1, PLAYER);
    let reg = registry(&w, MONSTER);
    assert_eq!((reg[0].start_time, reg[0].duration), (-5.0, 4000.0));
}

/// Equal power, different spell: the longer duration surpasses, the shorter is surpassed.
#[test]
fn equal_power_rivals_compare_durations() {
    let mut w = world();
    cast(&mut w, MONSTER, 2, PLAYER);
    assert_eq!(
        cast(&mut w, MONSTER, 3, PLAYER).stack_type,
        StackType::Surpassed
    ); // 900 < 1800
    let r = cast(&mut w, MONSTER, 4, PLAYER); // 3600 beats both
    assert_eq!(r.stack_type, StackType::Surpass);
    assert_eq!(r.surpass.len(), 2);
    assert_eq!(r.enchantment.as_ref().unwrap().layer_id, 3);
}

/// Equipping (`equip: true`) writes Duration -1 for a non-creature caster; a caster player's
/// AugmentationIncreasedSpellDuration stretches the duration by 20% per point.
#[test]
fn build_entry_durations() {
    let mut w = world();
    let s = spell(&w, 1);
    let r = emc::add(&mut w, PLAYER, &s, Some(ITEM), None, true, false);
    assert_eq!(r.enchantment.as_ref().unwrap().duration, -1.0);
    assert_eq!(
        r.enchantment.as_ref().unwrap().caster_object_id,
        ITEM.full()
    );
    // Beneficial is or-ed into StatModType.
    assert_eq!(
        r.enchantment.as_ref().unwrap().stat_mod_type,
        F(ADD_ATTR.0 | F::Beneficial.0)
    );

    w.objects
        .get_mut(PLAYER)
        .unwrap()
        .set_augmentation_increased_spell_duration(2);
    let s = spell(&w, 8);
    let r = emc::add(&mut w, MONSTER, &s, Some(PLAYER), None, false, false);
    // 1800 * (1.0f + 2 * 0.2f)
    assert_eq!(
        r.enchantment.as_ref().unwrap().duration,
        1800.0 * f64::from(1.0f32 + 2.0 * 0.2f32)
    );
    // A null caster writes the target's own guid.
    let s = spell(&w, 10);
    let r = emc::add(&mut w, MONSTER, &s, None, None, false, false);
    assert_eq!(
        r.enchantment.as_ref().unwrap().caster_object_id,
        MONSTER.full()
    );
}

/// V252: a built-in item spell never takes the caster's
/// AugmentationIncreasedSpellDuration, on the first layer or on a new one (a second caster's
/// cast). ACE's new-layer path dropped the flag, so the second layer got the bonus.
#[test]
fn a_built_in_item_spell_takes_no_duration_augmentation_on_any_layer() {
    let mut w = world();
    for p in [PLAYER, OTHER_PLAYER] {
        w.objects
            .get_mut(p)
            .unwrap()
            .set_augmentation_increased_spell_duration(2);
    }
    let s = spell(&w, 8);
    let first = emc::add(&mut w, MONSTER, &s, Some(PLAYER), None, false, true);
    assert_eq!(first.enchantment.as_ref().unwrap().duration, 1800.0);
    let second = emc::add(&mut w, MONSTER, &s, Some(OTHER_PLAYER), None, false, true);
    let e = second.enchantment.as_ref().unwrap();
    assert_eq!(
        (e.layer_id, e.caster_object_id),
        (2, OTHER_PLAYER.full()),
        "a new layer"
    );
    assert_eq!(e.duration, 1800.0);
}

// ------------------------------------------------------------------ expiry and removal

/// `HeartBeat` counts StartTime down by the interval and removes an entry once
/// `StartTime <= -Duration`: a 30 s spell cast at t survives the heartbeats at t+5 .. t+25 and
/// goes at t+30. The player gets MagicRemoveEnchantment and the expiry sound.
#[test]
fn expiry_on_the_heartbeat_that_reaches_the_duration() {
    let mut w = world();
    cast(&mut w, PLAYER, 7, PLAYER);
    start_capture();
    for _ in 0..5 {
        em::heart_beat(&mut w, PLAYER, 5.0);
    }
    assert_eq!(registry(&w, PLAYER).len(), 1);
    assert!(sent_kinds().is_empty());
    em::heart_beat(&mut w, PLAYER, 5.0);
    assert!(registry(&w, PLAYER).is_empty());
    assert_eq!(sent_kinds(), [EV_REMOVE, SOUND]);
    // A permanent (equipped) entry never expires.
    let s = spell(&w, 1);
    emc::add(&mut w, PLAYER, &s, Some(ITEM), None, true, false);
    for _ in 0..1000 {
        em::heart_beat(&mut w, PLAYER, 5.0);
    }
    assert_eq!(registry(&w, PLAYER).len(), 1);
}

/// `Remove` on an item: the online owner is told "The spell X on Y has expired." plus the sound.
#[test]
fn item_expiry_tells_the_online_owner() {
    let mut w = world();
    w.player_manager.online_players.insert(
        PLAYER.full(),
        OnlinePlayer {
            guid: PLAYER,
            account: None,
        },
    );
    w.objects
        .get_mut(ITEM)
        .unwrap()
        .set_wielder_id(Some(PLAYER.full()));
    cast(&mut w, ITEM, 7, PLAYER);
    start_capture();
    for _ in 0..6 {
        em::heart_beat(&mut w, ITEM, 5.0);
    }
    assert!(registry(&w, ITEM).is_empty());
    let sent = take_sent();
    assert_eq!(sent.len(), 2);
    let text_len = u16::from_le_bytes(sent[0].2[4..6].try_into().unwrap()) as usize;
    assert_eq!(
        std::str::from_utf8(&sent[0].2[6..6 + text_len]).unwrap(),
        "The spell Spell 7 on  has expired."
    );
}

/// `RemoveAllEnchantments` keeps item spells (Duration -1) and cooldowns (id > short.MaxValue);
/// `RemoveAllBadEnchantments` also keeps beneficial ones.
#[test]
fn remove_all_keeps_items_and_cooldowns() {
    let mut w = world();
    cast(&mut w, PLAYER, 5, MONSTER); // harmful
    cast(&mut w, PLAYER, 8, PLAYER); // beneficial
    let s = spell(&w, 10);
    emc::add(&mut w, PLAYER, &s, Some(ITEM), None, true, false); // item
    w.objects.get_mut(ITEM).unwrap().set_cooldown_id(Some(3));
    w.objects
        .get_mut(ITEM)
        .unwrap()
        .set_cooldown_duration(Some(10.0));
    emc::start_cooldown(&mut w, PLAYER, ITEM);
    let ids = |w: &World| {
        registry(w, PLAYER)
            .iter()
            .map(|e| e.spell_id)
            .collect::<Vec<_>>()
    };
    let before = registry(&w, PLAYER);
    emc::remove_all_bad_enchantments(&mut w, PLAYER);
    assert_eq!(ids(&w), [8, 10, 0x8003]);
    w.objects
        .get_mut(PLAYER)
        .unwrap()
        .biota
        .properties_enchantment_registry = Some(before);
    emc::remove_all_enchantments(&mut w, PLAYER);
    assert_eq!(ids(&w), [10, 0x8003]);
}

// ------------------------------------------------------------------ cooldowns

/// `StartCooldown`: spell 0x8000 | id, category 0x8000, layer 1, `Cooldown` flags, and the
/// player gets MagicUpdateEnchantment; `GetCooldown` = Duration - |StartTime|; the expiry sends
/// no sound (cooldown category).
#[test]
fn cooldowns() {
    let mut w = world();
    w.objects.get_mut(ITEM).unwrap().set_cooldown_id(Some(3));
    w.objects
        .get_mut(ITEM)
        .unwrap()
        .set_cooldown_duration(Some(10.0));
    assert!(em::check_cooldown(&w, PLAYER, Some(3)));
    assert!(em::check_cooldown(&w, PLAYER, None));
    start_capture();
    assert!(emc::start_cooldown(&mut w, PLAYER, ITEM));
    assert_eq!(sent_kinds(), [EV_UPDATE]);
    let e = &registry(&w, PLAYER)[0];
    assert_eq!(
        (e.spell_id, e.spell_category, e.layer_id, e.stat_mod_type),
        (0x8003, SpellCategory(0x8000), 1, F::Cooldown)
    );
    assert_eq!(
        (e.degrade_limit, e.enchantment_category, e.has_spell_set_id),
        (-666.0, 8, true)
    );
    assert_eq!(em::get_cooldown(&w, PLAYER, 3), 10.0);
    em::heart_beat(&mut w, PLAYER, 5.0);
    assert_eq!(em::get_cooldown(&w, PLAYER, 3), 5.0);
    assert!(!em::check_cooldown(&w, PLAYER, Some(3)));
    em::heart_beat(&mut w, PLAYER, 5.0);
    assert_eq!(sent_kinds(), [EV_REMOVE]);
    assert!(em::check_cooldown(&w, PLAYER, Some(3)));
    // No CooldownId: nothing.
    w.objects.get_mut(ITEM).unwrap().set_cooldown_id(None);
    assert!(!emc::start_cooldown(&mut w, PLAYER, ITEM));
    assert_eq!(em::get_cooldown_spell_id(3), 0x8003);
}

// ------------------------------------------------------------------ vitae

/// `GetMinVitae`: max penalty 3% per level above 1 (at least 1%), capped by
/// `vitae_penalty_max` (0.40): 0.99 at level 1, 0.73 at level 10, 0.60 from level 15.
#[test]
fn min_vitae_by_level() {
    let mut w = world();
    empyrean_world::managers::property_manager::initialize(&mut w, true);
    let w = &w;
    assert_eq!(em::get_min_vitae(w, 1), 0.99);
    assert_eq!(em::get_min_vitae(w, 2), 0.97);
    assert_eq!(em::get_min_vitae(w, 10), 0.73);
    assert_eq!(em::get_min_vitae(w, 14), 0.61);
    assert_eq!(em::get_min_vitae(w, 15), 0.60);
    assert_eq!(em::get_min_vitae(w, 275), 0.60);
    // `(level - 1) * 3` in uint: level 0 wraps to a huge penalty, capped at 40%.
    assert_eq!(em::get_min_vitae(w, 0), 0.60);
}

/// `UpdateVitae` adds vitae at 1 - 0.05 (layer 1, category `EnchantmentMask.Vitae`), then takes
/// 5% per death down to the level's minimum; `ReduceVitae` gives back 1% and answers 1.0 at the
/// top; `RemoveVitae` sends MagicRemoveEnchantment at layer 0.
#[test]
fn vitae() {
    let mut w = world();
    assert!(!emc::has_vitae(&mut w, PLAYER));
    assert_eq!(emc::update_vitae(&mut w, PLAYER), 0.95);
    assert!(emc::has_vitae(&mut w, PLAYER));
    let v = &registry(&w, PLAYER)[0];
    assert_eq!(
        (
            v.spell_id,
            v.layer_id,
            v.enchantment_category,
            v.caster_object_id
        ),
        (666, 1, 4, PLAYER.full())
    );
    assert_eq!(emc::update_vitae(&mut w, PLAYER), 0.95 - 0.05);
    for _ in 0..10 {
        emc::update_vitae(&mut w, PLAYER);
    }
    assert_eq!(registry(&w, PLAYER)[0].stat_mod_value, 0.73);
    assert_eq!(emc::reduce_vitae(&mut w, PLAYER), 0.73 + 0.01);
    // Non-players get 0 and no entry.
    assert_eq!(emc::update_vitae(&mut w, MONSTER), 0.0);
    assert!(registry(&w, MONSTER).is_empty());

    registry_set_vitae(&mut w, 0.995);
    assert_eq!(emc::reduce_vitae(&mut w, PLAYER), 1.0); // epsilon-equal / above 1: 1.0, value not clamped
    assert_eq!(registry(&w, PLAYER)[0].stat_mod_value, 0.995 + 0.01);

    start_capture();
    em::remove_vitae(&mut w, PLAYER);
    let sent = take_sent();
    assert_eq!(sent.len(), 2);
    // MagicRemoveEnchantment(spell 666, layer 0).
    assert_eq!(&sent[0].2[16..20], &[0x9A, 0x02, 0x00, 0x00]);
    assert!(!emc::has_vitae(&mut w, PLAYER));
}

fn registry_set_vitae(w: &mut World, v: f32) {
    w.objects
        .get_mut(PLAYER)
        .unwrap()
        .biota
        .properties_enchantment_registry
        .as_mut()
        .unwrap()[0]
        .stat_mod_value = v;
}

// ------------------------------------------------------------------ aggregations

/// The aggregations read the top layer of each category: additive sums (cast to int per entry),
/// multiplicative products, the Dirty Fighting debuff rounded into the defense skills, the
/// protection/vulnerability split, the body armor sign split, ratings (rounded), XP bonus
/// (highest power only, multiplicative counted as value - 1).
#[test]
fn aggregated_modifiers() {
    let mut w = world();
    for id in [2, 5, 6, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17] {
        cast(&mut w, PLAYER, id, MONSTER);
    }
    let str_ = PropertyAttribute::Strength;
    assert_eq!(
        emc::get_attribute_mod_additive(&mut w, PLAYER, str_),
        15 - 10
    );
    assert_eq!(
        emc::get_attribute_mod_multiplier(&mut w, PLAYER, str_),
        1.25
    );
    assert_eq!(
        emc::get_attribute_mod_additive(&mut w, PLAYER, PropertyAttribute::Self_),
        0
    );
    // MeleeDefense: +20, and the multiple-stat defense debuff Math.Round(-7.6) = -8.
    assert_eq!(
        emc::get_skill_mod_additives(&mut w, PLAYER, Skill::MeleeDefense),
        20 - 8
    );
    assert_eq!(
        emc::get_skill_mod_additives(&mut w, PLAYER, Skill::MissileDefense),
        -8
    );
    assert_eq!(emc::get_skill_mod_additives(&mut w, PLAYER, Skill::Run), 0);
    assert_eq!(em::get_defense_debuff_mod(&w, PLAYER), -8);
    assert_eq!(
        emc::get_skill_mod_multiplier(&mut w, PLAYER, Skill::MeleeDefense),
        1.0
    );
    assert_eq!(
        emc::get_resistance_mod(&mut w, PLAYER, DamageType::Fire),
        1.0
    );
    assert_eq!(
        emc::get_protection_resistance_mod(&mut w, PLAYER, DamageType::Fire),
        0.5
    );
    assert_eq!(
        emc::get_vulnerability_resistance_mod(&mut w, PLAYER, DamageType::Fire),
        2.0
    );
    assert_eq!(
        emc::get_resistance_mod(&mut w, PLAYER, DamageType::Cold),
        1.0
    );
    assert_eq!(emc::get_body_armor_mod(&mut w, PLAYER), 20);
    assert_eq!(em::get_body_armor_mod_positive(&w, PLAYER, true), 50);
    assert_eq!(em::get_body_armor_mod_positive(&w, PLAYER, false), -30);
    assert_eq!(
        emc::get_rating(&mut w, PLAYER, PropertyInt::DamageRating),
        4
    ); // Math.Round(4.5) half-even
    assert_eq!(emc::get_xp_bonus(&mut w, PLAYER), 0.2); // power 20 wins: additive 0.2
    assert_eq!(
        emc::get_vital_mod_additives(&mut w, PLAYER, PropertyAttribute2nd::MaxHealth),
        25.0
    );
    assert_eq!(
        emc::get_vital_mod_multiplier(&mut w, PLAYER, PropertyAttribute2nd::MaxHealth),
        1.0
    );
    assert_eq!(
        em::get_vital_rate_key(PropertyAttribute2nd::MaxMana),
        PropertyFloat::ManaRate
    );
    assert_eq!(
        em::get_impen_bane_key(DamageType::Nether),
        PropertyFloat::ArmorModVsNether
    );
    assert_eq!(em::get_resistance_key(DamageType::Health), PropertyFloat(0));
    // Nothing else set.
    assert_eq!(emc::get_damage_bonus(&mut w, PLAYER), 0);
    assert_eq!(emc::get_mana_conv_mod(&mut w, PLAYER), 1.0);
    assert_eq!(
        emc::get_regeneration_mod(&mut w, PLAYER, PropertyAttribute2nd::MaxHealth),
        1.0
    );
    // School filter over the top layers.
    assert_eq!(
        em::get_enchantments_school(&w, PLAYER, MagicSchool::CreatureEnchantment).len(),
        12
    ); // 15 and 16 share a category
    assert!(em::get_enchantments_school(&w, PLAYER, MagicSchool::WarMagic).is_empty());
}

// ------------------------------------------------------------------ the cache

/// The caching layer answers from its cache until `Add`, `Remove`, `Dispel`, the vitae calls or
/// `StartCooldown` clear it; a registry edit that bypasses the manager is not seen (ACE).
#[test]
fn cache_invalidates_on_add_and_remove() {
    let mut w = world();
    let str_ = PropertyAttribute::Strength;
    assert_eq!(emc::get_attribute_mod_additive(&mut w, PLAYER, str_), 0);
    assert!(!emc::has_enchantments(&mut w, PLAYER));

    cast(&mut w, PLAYER, 1, PLAYER); // Add clears
    assert_eq!(emc::get_attribute_mod_additive(&mut w, PLAYER, str_), 10);
    assert!(emc::has_enchantments(&mut w, PLAYER));

    // Bypass the manager: the cached value stays.
    w.objects
        .get_mut(PLAYER)
        .unwrap()
        .biota
        .properties_enchantment_registry
        .as_mut()
        .unwrap()[0]
        .stat_mod_value = 99.0;
    assert_eq!(emc::get_attribute_mod_additive(&mut w, PLAYER, str_), 10);
    assert_eq!(em::get_attribute_mod_additive(&w, PLAYER, str_), 99);

    cast(&mut w, PLAYER, 2, PLAYER); // Add clears
    assert_eq!(emc::get_attribute_mod_additive(&mut w, PLAYER, str_), 15);

    let top = registry(&w, PLAYER)[1].clone();
    emc::remove(&mut w, PLAYER, Some(&top), true); // Remove clears
    assert_eq!(emc::get_attribute_mod_additive(&mut w, PLAYER, str_), 99);

    let rest = registry(&w, PLAYER)[0].clone();
    emc::dispel(&mut w, PLAYER, Some(&rest)); // Dispel clears
    assert_eq!(emc::get_attribute_mod_additive(&mut w, PLAYER, str_), 0);
    assert!(!emc::has_enchantments(&mut w, PLAYER));

    // Remove(null) returns before ClearCache.
    cast(&mut w, PLAYER, 1, PLAYER);
    assert_eq!(emc::get_attribute_mod_additive(&mut w, PLAYER, str_), 10);
    w.objects
        .get_mut(PLAYER)
        .unwrap()
        .biota
        .properties_enchantment_registry
        .as_mut()
        .unwrap()
        .clear();
    emc::remove(&mut w, PLAYER, None, true);
    assert_eq!(emc::get_attribute_mod_additive(&mut w, PLAYER, str_), 10);
    emc::remove_all_enchantments(&mut w, PLAYER);
    assert_eq!(emc::get_attribute_mod_additive(&mut w, PLAYER, str_), 0);
}

// ------------------------------------------------------------------ dispel

/// `Dispel(entry)` sends MagicDispelEnchantment; `DispelAllEnchantments` sends one
/// MagicDispelMultipleEnchantments; a non-player gets nothing sent.
#[test]
fn dispel_messages() {
    let mut w = world();
    cast(&mut w, PLAYER, 1, PLAYER);
    cast(&mut w, PLAYER, 5, PLAYER);
    cast(&mut w, PLAYER, 8, PLAYER);
    start_capture();
    let first = registry(&w, PLAYER)[0].clone();
    emc::dispel(&mut w, PLAYER, Some(&first));
    assert_eq!(sent_kinds(), [EV_DISPEL]);
    em::dispel_all_enchantments(&mut w, PLAYER);
    assert_eq!(sent_kinds(), [EV_DISPEL_MULTI]);
    assert!(registry(&w, PLAYER).is_empty());
    cast(&mut w, MONSTER, 1, PLAYER);
    em::dispel_all_enchantments(&mut w, MONSTER);
    assert!(sent_kinds().is_empty());
    assert!(registry(&w, MONSTER).is_empty());
}

/// `SelectDispel` filters by power and Duration != -1, then school and alignment; `Number -1`
/// dispels all, otherwise `ThreadSafeRandom.Next(round(n * (1 - variance)), n)` then a shuffle.
#[test]
fn select_dispel_filters_and_draws() {
    let dispel =
        |id: u32, max_power: i32, school: i32, align: i32, number: i32, variance: f32| DbSpell {
            id,
            name: format!("Dispel {id}"),
            min_power: Some(0),
            max_power: Some(max_power),
            dispel_school: Some(school),
            align: Some(align),
            number: Some(number),
            number_variance: Some(variance),
            ..DbSpell::default()
        };
    let mut defs = defs();
    for id in [100, 101, 102, 103] {
        defs.push(d(id, 300, 1, 0.0, false, F::Undef, 0, 0.0));
    }
    let extra = vec![
        dispel(100, 1000, 0, 0, -1, 0.0),
        dispel(101, 20, 0, 0, -1, 0.0),
        dispel(102, 1000, 0, 2, -1, 0.0),
        dispel(103, 1000, 0, 0, 2, 0.5),
    ];
    let mut w = world_with(&defs, extra, BTreeMap::new());

    cast(&mut w, MONSTER, 2, PLAYER); // power 50, beneficial
    cast(&mut w, MONSTER, 5, PLAYER); // power 10, harmful
    cast(&mut w, MONSTER, 8, PLAYER); // power 10, beneficial
    let s = spell(&w, 10);
    emc::add(&mut w, MONSTER, &s, Some(ITEM), None, true, false); // item: never dispelled

    let ids = |v: Vec<empyrean_world::entity::spell_enchantment::SpellEnchantment>| {
        v.iter().map(|s| s.enchantment.spell_id).collect::<Vec<_>>()
    };
    assert_eq!(
        ids(em::select_dispel(&w, MONSTER, &spell(&w, 100))),
        [2, 5, 8]
    );
    assert_eq!(ids(em::select_dispel(&w, MONSTER, &spell(&w, 101))), [5, 8]);
    assert_eq!(ids(em::select_dispel(&w, MONSTER, &spell(&w, 102))), [5]);

    // Number 2, variance 0.5: Next(round(2 * 0.5) = 1, 2), then a Fisher-Yates shuffle.
    ThreadSafeRandom::seed(7);
    let picked = ids(em::select_dispel(&w, MONSTER, &spell(&w, 103)));
    ThreadSafeRandom::seed(7);
    let n = ThreadSafeRandom::next(1, 2);
    let mut expect = vec![2, 5, 8];
    empyrean_common::extensions::list_extensions::shuffle(&mut expect);
    expect.truncate(usize::try_from(n).unwrap());
    assert_eq!(picked, expect);
}

// ------------------------------------------------------------------ equipment-set tie-break

/// `BuildStack`: equal power and equal duration against a **set spell** (in `SpellSet.SetSpells`,
/// ids from `SetCoordination1` = 4730) falls back on the spell id; against a non-set spell the
/// newcomer surpasses.
#[test]
fn set_spells_fall_back_on_spell_id() {
    let str_ = u32::from(PropertyAttribute::Strength.0);
    let mut defs = defs();
    defs.push(d(4730, 40, 50, 1800.0, true, ADD_ATTR, str_, 5.0));
    defs.push(d(4731, 40, 50, 1800.0, true, ADD_ATTR, str_, 6.0));
    defs.push(d(20, 40, 50, 1800.0, true, ADD_ATTR, str_, 7.0));
    let sets = BTreeMap::from([(
        1,
        SpellSet {
            tiers: BTreeMap::from([(2, vec![4730, 4731])]),
        },
    )]);
    let mut w = world_with(&defs, Vec::new(), sets);
    cast(&mut w, MONSTER, 4731, PLAYER);
    // 4730 < 4731 and 4731 is a set spell: surpassed.
    assert_eq!(
        cast(&mut w, MONSTER, 4730, PLAYER).stack_type,
        StackType::Surpassed
    );
    let mut w2 = world_with(&defs, Vec::new(), sets_again());
    cast(&mut w2, MONSTER, 4730, PLAYER);
    // 20 against set spell 4730: equal durations, so the id decides (20 < 4730): surpassed.
    assert_eq!(
        cast(&mut w2, MONSTER, 20, PLAYER).stack_type,
        StackType::Surpassed
    );
    // 4731 surpasses 4730 by id, and 20 (not a set spell) outright.
    let r = cast(&mut w2, MONSTER, 4731, PLAYER);
    assert_eq!(
        (r.stack_type, r.surpass.len(), r.top_layer_id),
        (StackType::Surpass, 2, 2)
    );
}

fn sets_again() -> BTreeMap<u32, SpellSet> {
    BTreeMap::from([(
        1,
        SpellSet {
            tiers: BTreeMap::from([(2, vec![4730, 4731])]),
        },
    )])
}

// ------------------------------------------------------------------ damage over time

/// A nether DoT (`DotDuration` 15, so `Duration` = 15 + 5 = 20 and 4 ticks at 5 s): `BuildEntry`
/// rescales StatModValue for a 2.5 s heartbeat (`Spell.GetDamagePerTick`: 10 * 4 / 8 = 5);
/// `GetNumTicks` / `GetTotalDamage` / `GetDamagePerTick` over the entry; `GetNetherDotDamageRating`
/// is Math.Round(damage per default tick / 8).
#[test]
fn damage_over_time_maths() {
    let mut defs = defs();
    defs.push(d(
        30,
        636,
        10,
        0.0,
        false,
        ADD_INT,
        u32::from(PropertyInt::NetherOverTime.0),
        10.0,
    ));
    let mut row = db_spell(defs.last().unwrap());
    row.dot_duration = Some(15.0);
    let mut w = world_with(&defs, vec![row], BTreeMap::new());

    let s = spell(&w, 30);
    assert!(s.is_damage_over_time());
    assert_eq!(s.duration(), 20.0);
    assert_eq!((s.get_num_ticks(5.0), s.get_num_ticks(2.5)), (4, 8));
    assert_eq!(s.get_damage_per_tick(5.0), 10.0);
    assert_eq!(s.get_damage_per_tick(2.5), 5.0);

    // Default heartbeat: StatModVal as is.
    cast(&mut w, PLAYER, 30, MONSTER);
    let e = registry(&w, PLAYER)[0].clone();
    assert_eq!((e.stat_mod_value, e.duration), (10.0, 20.0));
    assert_eq!(em::get_num_ticks(&w, PLAYER, &e, None), 4);
    assert_eq!(em::get_total_damage(&w, PLAYER, &e), 40.0);
    assert_eq!(em::get_damage_per_tick(&w, PLAYER, &e, None), 10.0);
    // 40 total over Ceiling(20 / 3) = 7 ticks.
    assert_eq!(
        em::get_damage_per_tick(&w, PLAYER, &e, Some(3.0)),
        40.0 / 7.0
    );
    assert_eq!(emc::get_nether_dot_damage_rating(&mut w, PLAYER), 1); // Math.Round(10 / 8)

    // A 2.5 s heartbeat: the entry is written per 2.5 s tick.
    w.objects
        .get_mut(MONSTER)
        .unwrap()
        .set_heartbeat_interval(Some(2.5));
    cast(&mut w, MONSTER, 30, PLAYER);
    let e = registry(&w, MONSTER)[0].clone();
    assert_eq!(e.stat_mod_value, 5.0);
    assert_eq!(em::get_num_ticks(&w, MONSTER, &e, None), 8);
    // Per default (5 s) tick: 5 * 8 total over 4 ticks.
    assert_eq!(em::get_damage_per_tick(&w, MONSTER, &e, Some(5.0)), 10.0);
    assert_eq!(em::get_nether_dot_damage_rating(&w, MONSTER), 1);
}

/// A nether dot scales with the casters damage rating.
#[test]
fn a_nether_dot_scales_with_the_casters_damage_rating() {
    let mut defs = defs();
    defs.push(d(
        30,
        636,
        10,
        0.0,
        false,
        ADD_INT,
        u32::from(PropertyInt::NetherOverTime.0),
        10.0,
    ));
    let mut row = db_spell(defs.last().unwrap());
    row.dot_duration = Some(15.0);
    let mut w = world_with(&defs, vec![row], BTreeMap::new());
    w.objects
        .get_mut(MONSTER)
        .unwrap()
        .set_damage_rating(Some(20));

    cast(&mut w, PLAYER, 30, MONSTER);
    let e = registry(&w, PLAYER)[0].clone();
    assert_eq!(e.stat_mod_value, 10.0f32 * (1.0f32 * 1.0f32 * 1.2f32));
    assert_eq!(e.stat_mod_value, 12.0);
}

/// `StartCooldown` clears the cache only when it added an entry.
#[test]
fn start_cooldown_clears_the_cache() {
    let mut w = world();
    assert!(!emc::has_enchantments(&mut w, PLAYER));
    w.objects.get_mut(ITEM).unwrap().set_cooldown_id(Some(4));
    assert!(emc::start_cooldown(&mut w, PLAYER, ITEM));
    assert!(emc::has_enchantments(&mut w, PLAYER));
}

// ------------------------------------------------------------------ ACE vectors

fn vu(v: &serde_json::Value) -> u32 {
    u32::try_from(empyrean_common::vectors::u64_of(v).expect("integer")).expect("u32")
}

fn vi(v: &serde_json::Value) -> i64 {
    empyrean_common::vectors::i64_of(v).expect("integer")
}

fn stack_number(s: StackType) -> i64 {
    match s {
        StackType::None => 0,
        StackType::Initial => 1,
        StackType::Surpass => 2,
        StackType::Refresh => 3,
        StackType::Surpassed => 4,
    }
}

/// ACE's own `EnchantmentManagerWithCaching` over 120 seeded scripts of casts (3 creature casters
/// and an equipped item) and heartbeats: after every step the `AddEnchantmentResult` (StackType,
/// layer), the registry in list order and the cached Strength modifiers match
/// (`fixtures/vectors/enchantments/add_scripts.json`, the server's `ace-vectors/EnchantmentVectors.cs`).
#[test]
fn add_scripts_match_ace() {
    use empyrean_common::vectors::{f32_of, f64_of, load_named, same_f32, same_f64};
    let file = load_named("enchantments", "add_scripts");
    assert_eq!(file.cases.len(), 120);
    let mut failures = Vec::new();
    for (n, case) in file.cases.iter().enumerate() {
        let defs: Vec<Def> = case.input["spells"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| Def {
                id: vu(&s["id"]),
                category: vu(&s["cat"]),
                power: vu(&s["power"]),
                duration: f64_of(&s["dur"]).unwrap(),
                beneficial: s["good"].as_bool().unwrap(),
                school: 4,
                stat: F(i32::try_from(vi(&s["type"])).unwrap()),
                key: vu(&s["key"]),
                val: f32_of(&s["val"]).unwrap(),
            })
            .collect();
        let sets = BTreeMap::from([(
            1,
            SpellSet {
                tiers: BTreeMap::from([(2, vec![4730, 4731])]),
            },
        )]);
        let mut w = world_with(&defs, Vec::new(), sets);
        for (guid, class) in [
            (0x8000_0010, Class::Creature),
            (0x8000_0011, Class::Creature),
            (0x8000_0012, Class::Creature),
            (0x8000_0020, Class::GenericObject),
        ] {
            let mut o = WorldObject::allocate(class);
            o.guid = ObjectGuid::new(guid);
            o.biota.properties_enchantment_registry = Some(Vec::new());
            w.objects.insert(o).unwrap();
        }
        let target = MONSTER;

        let outs = case.output.as_array().unwrap();
        for (i, (op, out)) in case.input["ops"]
            .as_array()
            .unwrap()
            .iter()
            .zip(outs)
            .enumerate()
        {
            let mut got_result = None;
            if let Some(secs) = op.get("heartbeat") {
                em::heart_beat(&mut w, target, f64_of(secs).unwrap());
            } else {
                let s = spell(&w, vu(&op["cast"]));
                let caster = ObjectGuid::new(vu(&op["caster"]));
                let r = emc::add(
                    &mut w,
                    target,
                    &s,
                    Some(caster),
                    None,
                    op["equip"].as_bool().unwrap(),
                    false,
                );
                got_result = Some((
                    stack_number(r.stack_type),
                    i64::from(r.enchantment.unwrap().layer_id),
                ));
            }
            let want_result = (!out["result"].is_null())
                .then(|| (vi(&out["result"]["stack"]), vi(&out["result"]["layer"])));
            let add = emc::get_attribute_mod_additive(&mut w, target, PropertyAttribute::Strength);
            let mul =
                emc::get_attribute_mod_multiplier(&mut w, target, PropertyAttribute::Strength);
            let reg = registry(&w, target);
            // V250 / V251 / V276: the modifiers are retail's duel over the registry (no aura or
            // set-spell keys); ACE's recorded ones must be what ACE's order, keys included, gives
            // over the same registry (which proves the oracle).
            let (want_add, want_mul) = (
                i32::try_from(vi(&out["add"])).unwrap(),
                f32_of(&out["mul"]).unwrap(),
            );
            let mods_ok = {
                use crate::enchant_oracle::{attribute_mods, Keys};
                let str_ = u32::from(PropertyAttribute::Strength.0);
                let candidates: Vec<_> =
                    reg.iter().filter(|e| strength_candidate(e, str_)).collect();
                let aura = |id: i32| {
                    empyrean_entity::models::properties_enchantment_registry_extensions::LEVEL8_AURA_SELF_SPELLS.iter().any(|s| i64::from(s.0) == i64::from(id))
                };
                let set_spell = |id: i32| id == 4730 || id == 4731;
                let keys = Keys {
                    aura: &aura,
                    set_spell: &set_spell,
                };
                let (ace_add, ace_mul) = attribute_mods(&candidates, false, &keys);
                let (retail_add, retail_mul) = attribute_mods(&candidates, true, &keys);
                ace_add == want_add
                    && same_f32(ace_mul, want_mul)
                    && add == retail_add
                    && same_f32(mul, retail_mul)
            };
            let want_reg = out["registry"].as_array().unwrap();
            let reg_ok = reg.len() == want_reg.len()
                && reg.iter().zip(want_reg).all(|(e, x)| {
                    e.caster_object_id == vu(&x["caster"])
                        && same_f64(e.duration, f64_of(&x["dur"]).unwrap())
                        && i64::from(e.layer_id) == vi(&x["layer"])
                        && i64::from(e.spell_id) == vi(&x["spell"])
                        && same_f64(e.start_time, f64_of(&x["start"]).unwrap())
                        && i64::from(e.stat_mod_type.0) == vi(&x["type"])
                        && same_f32(e.stat_mod_value, f32_of(&x["val"]).unwrap())
                });
            if got_result != want_result || !mods_ok || !reg_ok {
                failures.push(format!(
                    "script {n} op {i} {op}: result {got_result:?} vs {want_result:?}, add {add} mul {mul}, registry {:?} vs {}",
                    reg.iter().map(|e| (e.spell_id, e.layer_id, e.caster_object_id, e.start_time, e.duration)).collect::<Vec<_>>(),
                    out["registry"]
                ));
                break;
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} scripts differ from ACE:\n{}",
        failures.len(),
        failures
            .iter()
            .take(5)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// Whether an entry is a candidate for an attribute's modifiers (`GetEnchantments_TopLayer` with
/// `handleMultiple`): a single-stat attribute entry for `key`, or a multiple-stat one (not vitae).
fn strength_candidate(
    e: &empyrean_entity::models::properties_enchantment_registry::PropertiesEnchantmentRegistry,
    key: u32,
) -> bool {
    let t = e.stat_mod_type.0;
    let single = F::Attribute.0 | F::SingleStat.0;
    let multiple = F::Attribute.0 | F::MultipleStat.0;
    (t & single == single && e.stat_mod_key == key)
        || (t & multiple == multiple && t & F::Vitae.0 == 0 && e.stat_mod_key == 0)
}

// ------------------------------------------------------------------ Shared rules: the shared rules' registry

/// A tiny deterministic generator for the sweep (not ACE's RNG; the sweep only needs coverage).
struct Lcg(u64);

impl Lcg {
    fn next(&mut self, n: u32) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        u32::try_from((self.0 >> 33) % u64::from(n)).unwrap()
    }
}

type Both = (
    PropertiesEnchantmentRegistry,
    dereth_rules::enchant::Enchantment,
);

/// One registry entry for both sides: ACE's `PropertiesEnchantmentRegistry` and the client's
/// enchantment-registry entry (start time rebased as `1000 + StartTime`, which keeps the order:
/// ACE counts StartTime down from 0, the client stores receipt time plus the offset).
fn both(
    spell_id: i32,
    category: u32,
    power: u32,
    start: f64,
    kind: F,
    key: u32,
    value: f32,
) -> Both {
    let ace = PropertiesEnchantmentRegistry {
        spell_id,
        layer_id: 1,
        spell_category: SpellCategory(category),
        power_level: power,
        start_time: start,
        duration: 1800.0,
        caster_object_id: MONSTER.full(),
        stat_mod_type: kind,
        stat_mod_key: key,
        stat_mod_value: value,
        ..PropertiesEnchantmentRegistry::default()
    };
    let client = dereth_rules::enchant::Enchantment {
        id: spell_id.cast_unsigned() | (1 << 16),
        spell_category: u16::try_from(category).unwrap(),
        power_level: i32::try_from(power).unwrap(),
        start_time: 1000.0 + start,
        duration: 1800.0,
        caster: dereth_primitives::ObjectId(MONSTER.full()),
        degrade_modifier: 0.0,
        degrade_limit: 0.0,
        last_time_degraded: 0.0,
        smod: dereth_protocol::types::qualities::StatMod {
            kind: kind.0.cast_unsigned(),
            key,
            value,
        },
        spell_set_id: None,
    };
    (ace, client)
}

/// The enchantments each side lets modify attribute `key`: ACE's top layers for
/// `Attribute|Additive` and `Attribute|Multiplicative` (with `handleMultiple`, as
/// `GetAttributeMod_*` query them); the client's `cull(ATTRIBUTE, key)` over its multiplicative
/// and additive lists, both filled in registry order (as one PlayerDescription would).
fn winners(w: &mut World, entries: &[Both], key: u32) -> (Vec<i32>, Vec<i32>) {
    w.objects
        .get_mut(MONSTER)
        .unwrap()
        .biota
        .properties_enchantment_registry = Some(entries.iter().map(|(a, _)| a.clone()).collect());
    let mut ace: Vec<i32> = [
        F(F::Attribute.0 | F::Additive.0),
        F(F::Attribute.0 | F::Multiplicative.0),
    ]
    .into_iter()
    .flat_map(|t| {
        em::get_enchantments_top_layer_key(w, MONSTER, t, key, true)
            .into_iter()
            .map(|e| e.spell_id)
            .collect::<Vec<_>>()
    })
    .collect();
    ace.sort_unstable();

    let is_mult = |c: &dereth_rules::enchant::Enchantment| {
        c.smod.kind & dereth_rules::enchant::ench_type::ADDITIVE == 0
    };
    let reg = dereth_rules::enchant::EnchantmentRegistry {
        mult_list: entries.iter().map(|(_, c)| *c).filter(is_mult).collect(),
        add_list: entries
            .iter()
            .map(|(_, c)| *c)
            .filter(|c| !is_mult(c))
            .collect(),
        ..Default::default()
    };
    let mut client: Vec<i32> = reg
        .cull(dereth_rules::enchant::ench_type::ATTRIBUTE, key)
        .iter()
        .map(|e| i32::from(e.spell_id()))
        .collect();
    client.sort_unstable();
    (ace, client)
}

/// the stacking choice (which enchantment of each spell category modifies an
/// attribute) agrees with the client registry in the shared rules (`dereth_rules::enchant`) on 3000 generated registries of 1 to 6
/// attribute enchantments: 4 categories (each wholly additive or wholly multiplicative, as the
/// spell data is), powers 10/50/50, two keys, and distinct start times. The cases outside this
/// domain where the two disagree are the ignored tests below (documented differences).
#[test]
fn stacking_agrees_with_dereth_rules() {
    let mut rng = Lcg(54);
    let mut w = world();
    let keys = [
        u32::from(PropertyAttribute::Strength.0),
        u32::from(PropertyAttribute::Endurance.0),
    ];
    let mut disagreements = Vec::new();
    for case in 0..3000 {
        let n = 1 + rng.next(6);
        let entries: Vec<Both> = (0..n)
            .map(|i| {
                let category = 1 + rng.next(4);
                let kind = if category % 2 == 1 {
                    ADD_ATTR
                } else {
                    MUL_ATTR
                };
                let power = [10, 50, 50][rng.next(3) as usize];
                let start = -f64::from(rng.next(50) * 16 + i); // distinct per entry
                let key = keys[rng.next(2) as usize];
                let spell_id = i32::try_from(100 + i).unwrap();
                both(
                    spell_id,
                    category,
                    power,
                    start,
                    kind,
                    key,
                    1.0 + f32::from(u8::try_from(i).unwrap()),
                )
            })
            .collect();
        for key in keys {
            let (ace, client) = winners(&mut w, &entries, key);
            if ace != client {
                disagreements.push(format!(
                    "case {case} key {key}: ace {ace:?} client {client:?}"
                ));
            }
        }
    }
    let shown = disagreements
        .iter()
        .take(10)
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        disagreements.is_empty(),
        "{} disagreements:\n{shown}",
        disagreements.len()
    );
}

/// equal start time tie.
/// V250.
#[test]
fn equal_start_time_ties_choose_the_later_entry() {
    let str_ = u32::from(PropertyAttribute::Strength.0);
    let entries = [
        both(100, 1, 50, 0.0, ADD_ATTR, str_, 1.0),
        both(101, 1, 50, 0.0, ADD_ATTR, str_, 2.0),
    ];
    let (ace, client) = winners(&mut world(), &entries, str_);
    assert_eq!(client, [101]);
    assert_eq!(ace, client);
}

// V251.
#[test]
fn mixed_categories_choose_the_same_winner_as_the_client() {
    let str_ = u32::from(PropertyAttribute::Strength.0);
    for (entries, survivor) in [
        (
            [
                both(100, 1, 50, -5.0, ADD_ATTR, str_, 1.0),
                both(101, 1, 10, 0.0, MUL_ATTR, str_, 1.1),
            ],
            100,
        ),
        (
            [
                both(100, 1, 10, -5.0, ADD_ATTR, str_, 1.0),
                both(101, 1, 50, 0.0, MUL_ATTR, str_, 1.1),
            ],
            101,
        ),
        (
            [
                both(100, 1, 50, -5.0, ADD_ATTR, str_, 1.0),
                both(101, 1, 50, 0.0, MUL_ATTR, str_, 1.1),
            ],
            101,
        ),
        (
            [
                both(100, 1, 50, 0.0, ADD_ATTR, str_, 1.0),
                both(101, 1, 50, 0.0, MUL_ATTR, str_, 1.1),
            ],
            100,
        ),
    ] {
        let (ace, client) = winners(&mut world(), &entries, str_);
        assert_eq!(client, [survivor]);
        assert_eq!(ace, client);
    }
}

/// level8 aura tie break.
/// V276.
#[test]
fn level_eight_aura_ties_follow_start_time() {
    let str_ = u32::from(PropertyAttribute::Strength.0);
    // Blood Drinker Self 8 (4395) at -10 against an equal-power newer spell: the newer wins.
    let entries = [
        both(4395, 1, 50, -10.0, ADD_ATTR, str_, 1.0),
        both(101, 1, 50, 0.0, ADD_ATTR, str_, 2.0),
    ];
    let (ace, client) = winners(&mut world(), &entries, str_);
    assert_eq!(client, [101]);
    assert_eq!(ace, client);
    // The aura newer: it wins by start time alone.
    let entries = [
        both(101, 1, 50, -10.0, ADD_ATTR, str_, 2.0),
        both(4395, 1, 50, 0.0, ADD_ATTR, str_, 1.0),
    ];
    let (ace, client) = winners(&mut world(), &entries, str_);
    assert_eq!(client, [4395]);
    assert_eq!(ace, client);
}

/// V276, the accepted quirk: two sets' levels of one spell at equal power (Gauntlet Damage Boost I
/// = 6330 and II = 6331, both set spells) resolve to the later entry on both sides, so a Boost I
/// re-added after Boost II wins (ACE ranked set spells by id, so II always won).
#[test]
fn equal_power_cross_set_ties_choose_the_later_entry() {
    let str_ = u32::from(PropertyAttribute::Strength.0);
    let mut defs = defs();
    defs.push(d(6330, 40, 1, 1800.0, true, ADD_ATTR, str_, 1.0));
    defs.push(d(6331, 40, 1, 1800.0, true, ADD_ATTR, str_, 2.0));
    let sets = BTreeMap::from([(
        1,
        SpellSet {
            tiers: BTreeMap::from([(2, vec![6330, 6331])]),
        },
    )]);
    let mut w = world_with(&defs, Vec::new(), sets);
    for (older, newer) in [(6331, 6330), (6330, 6331)] {
        let entries = [
            both(older, 40, 1, -30.0, ADD_ATTR, str_, 1.0),
            both(newer, 40, 1, -5.0, ADD_ATTR, str_, 2.0),
        ];
        let (ace, client) = winners(&mut w, &entries, str_);
        assert_eq!(client, [newer]);
        assert_eq!(ace, client);
    }
}

/// `Creature.CreateItemSpell`: a creature-school item spell is cast on the wielder (a monster
/// here) with the item as caster (`HandleCastSpell(spell, this, item, equip: true)`); an unknown id tells a player
/// "SpellID {id} Invalid." and answers false (Creature_Magic.cs).
#[test]
fn create_item_spell_casts_the_items_spell_on_the_wielder() {
    let mut w = world();
    assert!(
        empyrean_world::world_objects::creature_magic::create_item_spell(&mut w, MONSTER, ITEM, 1)
    );
    let reg = registry(&w, MONSTER);
    assert_eq!(reg.len(), 1);
    assert_eq!((reg[0].spell_id, reg[0].caster_object_id), (1, ITEM.full()));

    start_capture();
    assert!(
        !empyrean_world::world_objects::creature_magic::create_item_spell(
            &mut w, PLAYER, ITEM, 999
        )
    );
    assert_eq!(sent_kinds(), [(0xF7B0, 0x02EB)], "a transient string");
}

/// Divergence: V397
/// An item spell of a category that later became an aura (Blood Drinker's, damage raising) is
/// cast on the wielder at the end of retail and not in an era before the auras.
#[test]
fn an_era_before_the_auras_casts_no_item_spell_on_the_wielder() {
    let mut aura = d(
        20,
        154,
        10,
        1800.0,
        true,
        ADD_ATTR,
        u32::from(PropertyAttribute::Strength.0),
        1.0,
    );
    aura.school = 3; // item enchantment
    let mut defs = defs();
    defs.push(aura);
    let mut w = world_with(&defs, Vec::new(), BTreeMap::new());
    empyrean_world::world_objects::creature_magic::create_item_spell(&mut w, MONSTER, ITEM, 20);
    assert!(
        registry(&w, MONSTER).iter().any(|e| e.spell_id == 20),
        "the aura is on the wielder"
    );

    let mut w = world_with(&defs, Vec::new(), BTreeMap::new());
    w.era = empyrean_common::era::EraId::Infiltration.rules();
    empyrean_world::world_objects::creature_magic::create_item_spell(&mut w, MONSTER, ITEM, 20);
    assert!(!registry(&w, MONSTER).iter().any(|e| e.spell_id == 20));
}

/// `Creature.RemoveItemSpell`: the item's enchantment on the wielder (a creature-school spell) is
/// removed (`Remove`, the remove event), or dispelled when silent (the dispel event); no item does
/// nothing; an unknown id tells a player "SpellId {id} Invalid." (Creature_Magic.cs).
#[test]
fn remove_item_spell_removes_or_dispels_the_items_enchantment() {
    let mut w = world();
    cast(&mut w, PLAYER, 1, ITEM);
    cast(&mut w, PLAYER, 8, ITEM);
    start_capture();
    empyrean_world::world_objects::creature_magic::remove_item_spell(
        &mut w,
        PLAYER,
        Some(ITEM),
        1,
        false,
    );
    assert_eq!(layers(&w, PLAYER), [(8, 1, ITEM.full())]);
    assert!(sent_kinds().contains(&EV_REMOVE));

    start_capture();
    empyrean_world::world_objects::creature_magic::remove_item_spell(
        &mut w,
        PLAYER,
        Some(ITEM),
        8,
        true,
    );
    assert!(registry(&w, PLAYER).is_empty());
    assert!(sent_kinds().contains(&EV_DISPEL));

    start_capture();
    empyrean_world::world_objects::creature_magic::remove_item_spell(
        &mut w, PLAYER, None, 1, false,
    );
    empyrean_world::world_objects::creature_magic::remove_item_spell(
        &mut w,
        PLAYER,
        Some(ITEM),
        999,
        false,
    );
    assert_eq!(
        sent_kinds(),
        [(0xF7B0, 0x02EB)],
        "one transient string, for the unknown id"
    );
}

/// A network enchantment carries its spells category and power.
#[test]
fn a_network_enchantment_carries_its_spells_category_and_power() {
    let mut w = world();
    cast(&mut w, MONSTER, 2, PLAYER);
    let entry = registry(&w, MONSTER)[0].clone();
    let e = empyrean_world::network::structure::enchantment::enchantment_from_registry(
        &w, MONSTER, &entry,
    );
    assert_eq!((e.spell_id, e.spell_category, e.power_level), (2, 1, 50));
    assert_eq!(e.stat_mod_type & F::Beneficial, F::Beneficial);
}

/// `ArmorMaskHelper` and `ResistMaskHelper` (Network/Enum): an armor-level buff on an item
/// highlights ArmorLevel in green (color bit set); a fire-resistance buff (0.5) on the wielder
/// highlights ResistFire in red (the mod is below 1); nothing without enchantments.
#[test]
fn the_appraisal_masks_follow_the_enchantments() {
    use empyrean_world::world_objects::world_object_networking::shims;
    // an Impenetrability-like spell: additive PropertyInt.ArmorLevel
    let mut all = defs();
    all.push(d(
        20,
        13,
        10,
        1800.0,
        true,
        ADD_INT,
        u32::from(PropertyInt::ArmorLevel.0),
        50.0,
    ));
    let mut w = world_with(&all, Vec::new(), BTreeMap::new());
    assert_eq!(
        (
            shims::armor_mask_helper_get_highlight_mask(&w, ITEM),
            shims::armor_mask_helper_get_color_mask(&w, ITEM)
        ),
        (0, 0)
    );
    cast(&mut w, ITEM, 20, PLAYER);
    assert_eq!(
        (
            shims::armor_mask_helper_get_highlight_mask(&w, ITEM),
            shims::armor_mask_helper_get_color_mask(&w, ITEM)
        ),
        (0x1, 0x1)
    );

    assert_eq!(
        shims::resist_mask_helper_get_highlight_mask(&w, ITEM),
        0,
        "no wielder"
    );
    w.objects.get_mut(ITEM).unwrap().wielder = Some(MONSTER);
    cast(&mut w, MONSTER, 10, PLAYER);
    assert_eq!(shims::resist_mask_helper_get_highlight_mask(&w, ITEM), 0x8);
    assert_eq!(shims::resist_mask_helper_get_color_mask(&w, ITEM), 0);
}

mod world_effects {
    use crate::support::treasure_world::*;

    /// `Creature.GetEffectiveMagicDefense`: `Round(skill.Current * weaponDefenseMod +
    /// defenseImbues)`: 4.0's MagicDefense current (100) plus one equipped item with the MagicDefense
    /// imbue (`EquippedObjects`); no weapon, so the modifier is 1.
    #[test]
    fn effective_magic_defense_reads_the_skill_and_the_equipped_imbues() {
        let mut h = H::new();
        h.creature(Class::Creature, MONSTER, "Drudge");
        h.o_mut(MONSTER)
            .biota
            .properties_skill
            .get_or_insert_with(Default::default)
            .insert(
                Skill::MagicDefense,
                PropertiesSkill {
                    init_level: 100,
                    ..PropertiesSkill::default()
                },
            );
        assert_eq!(
            creature_magic::get_effective_magic_defense(&mut h.w, MONSTER),
            100
        );

        let cloak = ObjectGuid::new(0x8000_0100);
        let mut o = WorldObject::allocate(Class::GenericObject);
        o.guid = cloak;
        o.biota.id = cloak.full();
        o.set_property(
            PropertyInt::ValidLocations,
            i32::try_from(EquipMask::Cloak.0).unwrap(),
        );
        o.set_property(
            PropertyInt::ImbuedEffect,
            i32::try_from(ImbuedEffectType::MagicDefense.0).unwrap(),
        );
        h.w.objects.insert(o).expect("fresh guid");
        assert!(creature_equipment::try_equip_object(
            &mut h.w,
            MONSTER,
            cloak,
            EquipMask::Cloak
        ));
        assert_eq!(
            creature_magic::get_effective_magic_defense(&mut h.w, MONSTER),
            101
        );
    }

    /// A world tick removes an expired enchantment.
    #[test]
    fn a_world_tick_removes_an_expired_enchantment() {
        let mut h = H::new();
        h.load_landblock();
        let item = h.place_new(STACK_WCID, 0x7A9B_4300);
        let ward = Spell::new(&h.w, WARD_SPELL, true);
        emc::add(&mut h.w, item, &ward, Some(item), None, false, false);
        assert_eq!(
            h.o(item)
                .biota
                .properties_enchantment_registry
                .as_ref()
                .map(Vec::len),
            Some(1)
        );
        let interval = h.o(item).wo.world_object_tick.cached_heartbeat_interval;
        assert!(interval > 0.0);

        // Heartbeats until the aged start time reaches -20 s: the entry survives until then.
        let mut beats = 0;
        while h
            .o(item)
            .biota
            .properties_enchantment_registry
            .as_ref()
            .is_some_and(|r| !r.is_empty())
        {
            let next = tick::next_heartbeat_time(h.o(item));
            let wait = (next - h.w.now.unix_time).max(0.0) + 0.001;
            h.advance(wait);
            h.tick();
            beats += 1;
            assert!(beats < 100, "the enchantment never expired");
        }
        // ACE removes an entry once `StartTime <= -Duration` (`StartTime -= interval` per beat).
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let expected = (20.0 / interval).ceil() as usize;
        assert_eq!(beats, expected, "one heartbeat per {interval} s");
    }
}
