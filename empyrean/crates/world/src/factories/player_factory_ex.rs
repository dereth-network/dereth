// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/PlayerFactoryEx.cs
//! Port of `Source/ACE.Server/Factories/PlayerFactoryEx.cs`.
//!
//! Developer templates: fully levelled level-275 characters (heavy weapons, missile weapons, war
//! magic) built on `PlayerFactory.Create`, with default spell bars and a common inventory. ACE
//! itself never calls this class ("should suffice for unreferenced test method"); it is ported for
//! completeness. The weapons come from 4.10's loot generators. `Player.SpendAllXp`, `Player.LearnSpellsInBulk`,
//! `Creature.TryDequipObject`, `Container.TryRemoveFromInventory` and `RecipeManager.IconUnderlay`
//! are not ported yet and are `not_ported!` pointers; the rest (the template payload, the spell
//! bars, the level-up, the inventory helpers) runs, through `player_factory`'s shims.

use std::sync::Arc;

use empyrean_common::dotnet::CsCast;
use empyrean_common::random::with_thread_rng;
use empyrean_content::models::world::TreasureDeath;
use empyrean_entity::enums::{
    BondedStatus, DamageType, HeritageGroup, ImbuedEffectType, ItemType, PropertyDataId,
    PropertyFloat, PropertyInt, PropertyString, Skill, SkillAdvancementClass, UiEffects,
    WeenieType, WieldRequirement,
};
use empyrean_entity::{BinaryReader, CharacterCreateInfo, ObjectGuid, Weenie};
use empyrean_tables::enums::WeenieClassName;

use crate::factories::player_factory::{self, CreatedPlayer};
use crate::factories::{
    loot_generation_factory_caster, loot_generation_factory_melee, loot_generation_factory_missile,
};
use crate::world_objects::world_object::WorldObject;
use crate::World;

// ACE: PlayerFactoryEx.rand
// DIVERGE: ACE's class has its own unseeded `new Random()`; the port draws from the world thread's
// seeded generator (`empyrean_common::random`), as ported code may not read an unseeded RNG.

/// Heritage: Gear Knight. 10 for all attributes; trained Creature/Item/Life/Mana Conversion. This
/// will make sure the player starts off with foci.
// ACE: PlayerFactoryEx.baseGearKnight
static BASE_GEAR_KNIGHT: [u8; 400] = [
    0x01, 0x00, 0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x02, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x09, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x09, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x09, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xF0, 0x3F, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0xF0, 0x3F, 0x88, 0x2E, 0x44, 0x17, 0xA2, 0x0B, 0xD1, 0x3F, 0xC7, 0xBF, 0xE3, 0xDF,
    0xF1, 0xEF, 0xE8, 0x3F, 0xD4, 0x1E, 0x6A, 0x0F, 0xB5, 0x87, 0xDA, 0x3F, 0xAD, 0x76, 0x56, 0x3B,
    0xAB, 0x9D, 0xD5, 0x3F, 0x00, 0x00, 0x00, 0x00, 0x0A, 0x00, 0x00, 0x00, 0x0A, 0x00, 0x00, 0x00,
    0x0A, 0x00, 0x00, 0x00, 0x0A, 0x00, 0x00, 0x00, 0x0A, 0x00, 0x00, 0x00, 0x0A, 0x00, 0x00, 0x00,
    0x02, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x37, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00,
    0x01, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00,
    0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00,
    0x02, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00,
    0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00,
    0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00,
    0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00,
    0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x07, 0x00, 0x4E, 0x6F, 0x20, 0x4E, 0x61, 0x6D,
    0x65, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

// ACE: PlayerFactoryEx.CreateCharacterCreateInfo
/// The Gear Knight template with `name` and the six abilities, then (by default in ACE) a random
/// heritage and appearance.
///
/// # Panics
/// When the template does not unpack (it always does), or where `RandomizeHeritage` throws.
#[allow(clippy::too_many_arguments)]
pub fn create_character_create_info(
    w: &World,
    name: &str,
    strength: u32,
    endurance: u32,
    coordination: u32,
    quickness: u32,
    focus: u32,
    self_: u32,
    randomize_heritage_and_apperance: bool,
) -> CharacterCreateInfo {
    let mut character_create_info = CharacterCreateInfo::default();

    let mut binary_reader = BinaryReader::new(&BASE_GEAR_KNIGHT);
    character_create_info
        .unpack(&mut binary_reader)
        .expect("EndOfStreamException: baseGearKnight");

    character_create_info.name = Some(name.to_owned());

    character_create_info.strength_ability = strength;
    character_create_info.endurance_ability = endurance;
    character_create_info.coordination_ability = coordination;
    character_create_info.quickness_ability = quickness;
    character_create_info.focus_ability = focus;
    character_create_info.self_ability = self_;

    if randomize_heritage_and_apperance {
        randomize_heritage(w, &mut character_create_info);
    }

    character_create_info
}

/// `rand.Next(min, max)` (exclusive `max`).
fn rand_next(min: i32, max: i32) -> i32 {
    with_thread_rng(|r| r.next_range(min, max))
}

/// `rand.Next(0, list.Count)` as an index.
fn rand_index(count: usize) -> u32 {
    rand_next(0, i32::try_from(count).unwrap_or(i32::MAX)).cs_cast()
}

// ACE: PlayerFactoryEx.RandomizeHeritage
fn randomize_heritage(w: &World, character_create_info: &mut CharacterCreateInfo) {
    let heritage = HeritageGroup(rand_next(1, 11));
    let key: u32 = heritage.0.cs_cast();
    let char_gen = w.dats.portal_dat().char_gen();
    let heritage_group = char_gen
        .heritage_groups
        .get(&key)
        .unwrap_or_else(|| panic!("KeyNotFoundException: {key}"));

    character_create_info.heritage = heritage;
    // `Genders.ElementAt(rand.Next(0, Genders.Count)).Key`: the dictionary in the dat's order,
    // which the decoded table keeps in key order (the retail dat stores the sexes ascending).
    let gender_index = usize::try_from(rand_next(
        0,
        i32::try_from(heritage_group.sexes.len()).unwrap_or(i32::MAX),
    ))
    .unwrap_or(0);
    character_create_info.gender = *heritage_group
        .sexes
        .keys()
        .nth(gender_index)
        .expect("ArgumentOutOfRangeException");

    let sex = &heritage_group.sexes[&character_create_info.gender];

    character_create_info.appearance.hair_color = rand_index(sex.hair_colors.len());
    character_create_info.appearance.hair_style = rand_index(sex.hair_styles.len());

    character_create_info.appearance.eyes = rand_index(sex.eye_strips.len());
    character_create_info.appearance.eye_color = rand_index(sex.eye_colors.len());
    character_create_info.appearance.nose = rand_index(sex.nose_strips.len());
    character_create_info.appearance.mouth = rand_index(sex.mouth_strips.len());

    // todo randomize skin
}

/// `player.TryRemoveFromInventory(guid)`: Player_Inventory, not ported.
fn try_remove_from_inventory(w: &mut World, p: &mut CreatedPlayer, guid: ObjectGuid) -> bool {
    player_factory::with_player_in_world(w, p, None, |w, player| {
        crate::world_objects::container::try_remove_from_inventory(w, player, guid, false)
    })
}

/// The key of the first entry whose name contains `text`, or `default(ObjectGuid)`.
fn first_named(items: &[WorldObject], text: &str) -> ObjectGuid {
    items
        .iter()
        .find(|i| {
            i.get_property(PropertyString::Name)
                .unwrap_or_default()
                .contains(text)
        })
        .map_or_else(ObjectGuid::default, |i| i.guid)
}

// ACE: PlayerFactoryEx.Create275Base
/// Creates a fully leveled/augmented 275 base character player.
fn create_275_base(
    w: &mut World,
    character_create_info: &CharacterCreateInfo,
    weenie: Arc<Weenie>,
    guid: ObjectGuid,
    account_id: u32,
) -> CreatedPlayer {
    let (_, mut player) = player_factory::create(
        w,
        character_create_info,
        weenie,
        guid,
        account_id,
        WeenieType::Creature,
    );

    // Remove junk inventory
    // player.TryDequipObject(<"Leather Boots">, out var wo, out _); if (wo != null) player.TryRemoveFromInventory(wo.Guid);
    let boots = first_named(&player.equipped_objects, "Leather Boots");
    let wo = player_factory::with_player_in_world(w, &mut player, None, |w, p| {
        crate::world_objects::creature_equipment::try_dequip_object(w, p, boots).map(|(wo, _)| wo)
    });
    if let Some(wo) = wo {
        // the dequipped boots are in no inventory, so this answers false and they are dropped
        try_remove_from_inventory(w, &mut player, wo);
    }

    let wand = first_named(&player.inventory, "Training Wand");
    try_remove_from_inventory(w, &mut player, wand);
    let letter = first_named(&player.inventory, "Letter From Home");
    try_remove_from_inventory(w, &mut player, letter);

    level_up_player(&mut player.player);

    add_all_spells(w, &mut player);

    load_default_spell_bars(&mut player);

    player
}

// ACE: PlayerFactoryEx.LevelUpPlayer
/// Level 275, the experience for it, 46 more skill credits, and the playability augmentations
/// (paid for in experience).
pub fn level_up_player(player: &mut WorldObject) {
    let add64 = |v: Option<i64>, x: i64| v.map(|v| v.wrapping_add(x));
    let add32 = |v: Option<i32>, x: i32| v.map(|v| v.wrapping_add(x));

    player.set_available_experience(add64(player.available_experience(), 191_226_310_247));
    player.set_total_experience(add64(player.total_experience(), 191_226_310_247));
    player.set_level(Some(275));
    player.set_available_skill_credits(add32(player.available_skill_credits(), 46));
    player.set_total_skill_credits(add32(player.total_skill_credits(), 46));

    // Playability Augs
    if player.augmentation_extra_pack_slot() == 0 {
        player.set_augmentation_extra_pack_slot(1);
        player.set_available_experience(add64(player.available_experience(), -4_000_000_000));
    }

    while player.augmentation_increased_carrying_capacity() < 5 {
        player.set_augmentation_increased_carrying_capacity(
            player.augmentation_increased_carrying_capacity() + 1,
        );
        player.set_available_experience(add64(player.available_experience(), -1_000_000_000));
    }

    while player.augmentation_less_death_item_loss() < 3 {
        player
            .set_augmentation_less_death_item_loss(player.augmentation_less_death_item_loss() + 1);
        player.set_available_experience(add64(player.available_experience(), -2_000_000_000));
    }

    if player.augmentation_spells_remain_past_death() == 0 {
        player.set_augmentation_spells_remain_past_death(1);
        player.set_available_experience(add64(player.available_experience(), -4_000_000_000));
    }

    while player.augmentation_increased_spell_duration() < 5 {
        player.set_augmentation_increased_spell_duration(
            player.augmentation_increased_spell_duration() + 1,
        );
        player.set_available_experience(add64(player.available_experience(), -1_000_000_000));
    }

    if player.augmentation_jack_of_all_trades() == 0 {
        player.set_augmentation_jack_of_all_trades(1);
        player.set_available_experience(add64(player.available_experience(), -4_000_000_000));
    }

    // todo: Optionally add other augs
}

/// The seven default bars, in order: Recall Spells, Vitals, Buffs - Self, Buffs - Item,
/// Buffs - Other, Fellowship, Debuffs.
pub static DEFAULT_SPELL_BARS: [&[u32]; 7] = [
    &[
        2645, 48, 157, 47, 2647, 2648, 2646, 1635, 2644, 2041, 2931, 2941, 3865, 4084, 4128, 4198,
        4213, 5330, 5541, 6321,
    ],
    &[2073, 2343, 2083, 2345, 3194, 2072, 2082, 2336, 3193],
    &[
        562, 1426, 1450, 4325, 4299, 4297, 4319, 4305, 4329, 4496, 4498, 4494, 4530, 4582, 4564,
        4602, 4510, 4616, 4572, 4548, 4578, 4560, 4558, 4596, 4624, 4522, 4638, 6123, 4556, 4552,
        4468, 4470, 4462, 4472, 4464, 4466, 4460,
    ],
    &[
        5183, 4414, 4417, 4405, 4400, 4418, 4407, 4401, 4409, 4393, 4412, 4397, 4403, 4391,
    ],
    &[
        4324, 4298, 4296, 4318, 4304, 4328, 4495, 4497, 4493, 4529, 4581, 4563, 4601, 4509, 4615,
        4571, 4547, 4577, 4559, 4557, 4595, 4623, 4521, 4637, 6115, 4555, 4551, 4467, 4469, 4461,
        4471, 4463, 4465, 4459, 5997, 6022, 6031, 6014, 6006, 5989,
    ],
    &[
        3178, 3162, 3158, 3170, 3166, 3174, 3477, 3481, 3473, 3387, 3395, 3391, 3399, 3355, 3351,
        3359, 3403, 3339, 3343, 3327, 3347, 3331, 3335, 3323,
    ],
    &[
        2074, 4481, 4483, 4475, 4485, 4477, 4479, 4473, 4633, 4597, 4543, 4402, 4410, 4394, 4413,
        4398, 4404, 4392, 4396, 4419, 4411, 4408, 4415, 4406,
    ],
];

/// Bar 7 for a specialized War Magic character.
pub static WAR_MAGIC_SPELL_BAR: &[u32] = &[
    4423, 4426, 4422, 4424, 4427, 4421, 4425, 4439, 4451, 4457, 4443, 4455, 4447, 4433, 4440, 4452,
    4458, 4444, 4456, 4432, 4448, 2934, 1785, 1788, 1784, 1786, 1789, 1787, 1783, 4428, 3818,
];

// ACE: PlayerFactoryEx.LoadDefaultSpellBars
/// `Character.AddSpellToBar(barNumber, indexInBar++, spell)` for each bar in order.
pub fn load_default_spell_bars(player: &mut CreatedPlayer) {
    for (bar_number, bar) in (0u32..).zip(DEFAULT_SPELL_BARS.iter()) {
        for (index_in_bar, &spell) in (0u32..).zip(bar.iter()) {
            let _ = player
                .character_mut()
                .add_spell_to_bar(bar_number, index_in_bar, spell);
        }
    }
}

// ACE: PlayerFactoryEx.LoadSkillSpecificDefaultSpellBar
pub fn load_skill_specific_default_spell_bar(player: &mut CreatedPlayer) {
    let war_magic_specialized = player
        .player
        .biota
        .get_skill(Skill::WarMagic)
        .is_some_and(|s| s.sac == SkillAdvancementClass::Specialized);
    if war_magic_specialized {
        // War
        let bar_number = 7;
        for (index_in_bar, &spell) in (0u32..).zip(WAR_MAGIC_SPELL_BAR.iter()) {
            let _ = player
                .character_mut()
                .add_spell_to_bar(bar_number, index_in_bar, spell);
        }
    }
}

/// `player.TrainSkill(skill, credits)` then the specializations, in ACE's order.
fn train_and_specialize(
    w: &mut World,
    player: &mut CreatedPlayer,
    trained: &[(Skill, i32)],
    specialized: &[(Skill, i32)],
) {
    for &(skill, credits) in trained {
        player_factory::with_player_in_world(w, player, None, |w, p| {
            crate::world_objects::player_skills::train_skill_with(w, p, skill, credits, false)
        });
    }
    for &(skill, credits) in specialized {
        player_factory::with_player_in_world(w, player, None, |w, p| {
            crate::world_objects::player_skills::specialize_skill_with(w, p, skill, credits, true)
        });
    }
}

// ACE: PlayerFactoryEx.Create275HeavyWeapons
/// Creates a fully leveled 275 Heavy Weapons character player. No augmentations are included.
pub fn create_275_heavy_weapons(
    w: &mut World,
    weenie: Arc<Weenie>,
    guid: ObjectGuid,
    account_id: u32,
    name: &str,
) -> CreatedPlayer {
    let character_create_info =
        create_character_create_info(w, name, 100, 10, 100, 100, 10, 10, true);

    let mut player = create_275_base(w, &character_create_info, weenie, guid, account_id);

    train_and_specialize(
        w,
        &mut player,
        // Trained skills
        &[
            (Skill::HeavyWeapons, 6),
            (Skill::Healing, 6),
            (Skill::MeleeDefense, 10),
            (Skill::MissileDefense, 6),
            (Skill::Shield, 2),
        ],
        // Specialized skills
        &[
            (Skill::HeavyWeapons, 6),
            (Skill::Healing, 4),
            (Skill::MagicDefense, 12),
            (Skill::MeleeDefense, 10),
            (Skill::Shield, 2),
        ],
    );
    // 0 remaining skill points.
    // If/When we add the 4 skill points in LevelUpPlayer, we can spend them here as well

    load_skill_specific_default_spell_bar(&mut player);

    // todo aug endurance

    spend_all_xp(w, &mut player);

    add_common_inventory(w, &mut player, &[&RELIC_ALDURESSA]);

    // Create a dummy treasure profile for passing in tier value
    let profile = dummy_tier7_profile();

    // create 12 heavy weapons. this isn't the most efficient method, but should suffice for unreferenced test method
    let mut created = 0;
    while created < 12 {
        let mut item = loot_generation_factory_melee::create_melee_weapon(w, &profile, true)
            .expect("NullReferenceException: item.WeaponSkill");
        if item.weapon_skill() != Skill::HeavyWeapons {
            continue;
        }
        add_rend(&mut item);
        player_factory::try_add_to_inventory_created(w, &mut player, Some(item));
        created += 1;
    }
    player
}

// ACE: PlayerFactoryEx.Create275MissileWeapons
/// Creates a fully leveled 275 Missile Weapons character player. No augmentations are included.
pub fn create_275_missile_weapons(
    w: &mut World,
    weenie: Arc<Weenie>,
    guid: ObjectGuid,
    account_id: u32,
    name: &str,
) -> CreatedPlayer {
    let character_create_info =
        create_character_create_info(w, name, 10, 100, 100, 10, 10, 100, true);

    let mut player = create_275_base(w, &character_create_info, weenie, guid, account_id);

    train_and_specialize(
        w,
        &mut player,
        // Trained skills
        &[
            (Skill::Healing, 6),
            (Skill::MeleeDefense, 10),
            (Skill::MissileDefense, 6),
            (Skill::MissileWeapons, 6),
            (Skill::Fletching, 4),
        ],
        // Specialized skills
        &[
            (Skill::MagicDefense, 12),
            (Skill::MeleeDefense, 10),
            (Skill::MissileWeapons, 6),
            (Skill::Fletching, 4),
        ],
    );
    // 0 remaining skill points.

    load_skill_specific_default_spell_bar(&mut player);

    // todo aug what attribute?

    spend_all_xp(w, &mut player);

    add_common_inventory(w, &mut player, &[&NOBLE_RELIC]);

    // Create a dummy treasure profile for passing in tier value
    let profile = dummy_tier7_profile();

    for _ in 0..12 {
        let mut item =
            loot_generation_factory_missile::create_missile_weapon(w, &profile, true, true)
                .expect("NullReferenceException: AddRend(item)");
        add_rend(&mut item);
        player_factory::try_add_to_inventory_created(w, &mut player, Some(item));
    }

    player
}

// ACE: PlayerFactoryEx.Create275WarMagic
/// Creates a fully leveled 275 War Magic character player. No augmentations are included.
pub fn create_275_war_magic(
    w: &mut World,
    weenie: Arc<Weenie>,
    guid: ObjectGuid,
    account_id: u32,
    name: &str,
) -> CreatedPlayer {
    let character_create_info =
        create_character_create_info(w, name, 10, 100, 10, 10, 100, 100, true);

    let mut player = create_275_base(w, &character_create_info, weenie, guid, account_id);

    train_and_specialize(
        w,
        &mut player,
        // Trained skills
        &[
            (Skill::MeleeDefense, 10),
            (Skill::MissileDefense, 6),
            (Skill::Summoning, 8),
            (Skill::WarMagic, 16),
        ],
        // Specialized skills
        &[
            (Skill::LifeMagic, 8),
            (Skill::Summoning, 4),
            (Skill::WarMagic, 12),
        ],
    );
    // 0 remaining skill points

    let foci = player_factory::create_new_world_object_by_wcid(w, 15271); // Foci of Strife
    player_factory::try_add_to_inventory_created(w, &mut player, foci);

    load_skill_specific_default_spell_bar(&mut player);

    // todo aug what attribute?

    spend_all_xp(w, &mut player);

    add_common_inventory(w, &mut player, &[&ANCIENT_RELIC]);

    // Create a dummy treasure profile for passing in tier value
    let profile = dummy_tier7_profile();

    // create 12 war elemental wands. this isn't the most efficient method, but should suffice for unreferenced test method
    let mut created = 0;
    while created < 12 {
        let mut item = loot_generation_factory_caster::create_caster(w, &profile, true)
            .expect("NullReferenceException: item.WeenieClassId");
        let wcid = item.biota.weenie_class_id;
        if wcid < WeenieClassName::wandacid.0.cs_cast()
            || wcid > WeenieClassName::wandslashing.0.cs_cast()
        {
            continue;
        }
        add_rend(&mut item);
        player_factory::try_add_to_inventory_created(w, &mut player, Some(item));
        created += 1;
    }
    player
}

/// `new Database.Models.World.TreasureDeath { Tier = 7, LootQualityMod = 0 }` (the three
/// Create275 templates' dummy profile; every other member keeps its default).
fn dummy_tier7_profile() -> TreasureDeath {
    TreasureDeath {
        tier: 7,
        loot_quality_mod: 0.0,
        ..TreasureDeath::default()
    }
}

// ACE: PlayerFactoryEx.SpendAllXp
fn spend_all_xp(w: &mut World, player: &mut CreatedPlayer) {
    use empyrean_entity::enums::PropertyAttribute2nd;

    player_factory::with_player_in_world(w, player, None, |w, p| {
        crate::world_objects::player_xp::spend_all_xp(w, p, false);

        for (max_vital, vital) in [
            (
                PropertyAttribute2nd::MaxHealth,
                PropertyAttribute2nd::Health,
            ),
            (
                PropertyAttribute2nd::MaxStamina,
                PropertyAttribute2nd::Stamina,
            ),
            (PropertyAttribute2nd::MaxMana, PropertyAttribute2nd::Mana),
        ] {
            let max = crate::world_objects::world_object_magic::vital_max_value(w, p, max_vital);
            let o = w.objects.get_mut(p).expect("the staged player");
            let v = o
                .get_creature_vital(vital)
                .expect("Health, Stamina and Mana are vitals");
            v.set_current(o, max);
        }
    });
}

// ACE: PlayerFactoryEx.CommonSpellComponents
pub static COMMON_SPELL_COMPONENTS: [u32; 10] =
    [691, 689, 686, 688, 687, 690, 8897, 7299, 37155, 20631];
// ACE: PlayerFactoryEx.NobleRelic
static NOBLE_RELIC: [u32; 5] = [33584, 33585, 33586, 33587, 33588];
// ACE: PlayerFactoryEx.RelicAlduressa
static RELIC_ALDURESSA: [u32; 5] = [33574, 33575, 33576, 33577, 33578];
// ACE: PlayerFactoryEx.AncientRelic
static ANCIENT_RELIC: [u32; 5] = [33579, 33580, 33581, 33582, 33583];

// ACE: PlayerFactoryEx.AddCommonInventory
fn add_common_inventory(w: &mut World, player: &mut CreatedPlayer, additional_groups: &[&[u32]]) {
    // MMD
    add_weenies_to_inventory(w, player, &[20630, 20630, 20630, 20630, 20630, 20630], None);

    // Spell Components
    add_weenies_to_inventory(w, player, &COMMON_SPELL_COMPONENTS, None);

    // Focusing Stone
    add_weenies_to_inventory(w, player, &[8904], None);

    add_weenies_to_inventory(w, player, &[5893], None); // Hoary Robe
    add_weenies_to_inventory(w, player, &[14594], None); // Helm of the Elements

    for group in additional_groups {
        add_weenies_to_inventory(w, player, group, None);
    }

    // `WorldObjectFactory.CreateNewWorldObject("Orb")`
    let weenie = w.content.get_cached_weenie_by_class_name("Orb");
    let mut orb = weenie
        .and_then(|weenie| player_factory::create_new_world_object(w, weenie))
        .expect("NullReferenceException: orb");

    orb.remove_property(PropertyInt::PaletteTemplate);
    orb.remove_property(PropertyFloat::Shade);
    orb.remove_property(PropertyFloat::Shade2);
    orb.remove_property(PropertyFloat::Shade3);
    orb.remove_property(PropertyFloat::Shade4);
    orb.remove_property(PropertyDataId::MutateFilter);
    orb.remove_property(PropertyDataId::TsysMutationFilter);

    // biota_properties_int
    orb.set_property(PropertyInt::UiEffects, UiEffects::Magical.0.cs_cast());
    orb.set_property(PropertyInt::Bonded, BondedStatus::Bonded.0);
    orb.set_property(PropertyInt::DamageType, DamageType::Slash.0.cs_cast());
    orb.set_property(PropertyInt::TargetType, ItemType::Creature.0.cs_cast());
    orb.set_property(PropertyInt::ItemSpellcraft, 325);
    orb.set_property(PropertyInt::ItemCurMana, 1000);
    orb.set_property(PropertyInt::ItemMaxMana, 1000);
    orb.set_property(PropertyInt::ItemDifficulty, 280);
    orb.set_property(PropertyInt::WieldRequirements, WieldRequirement::Skill.0);
    orb.set_property(PropertyInt::WieldSkillType, 31);
    orb.set_property(PropertyInt::WieldDifficulty, 355);

    // biota_properties_float
    orb.set_property(PropertyFloat::ManaRate, -0.033_333);
    orb.set_property(PropertyFloat::WeaponDefense, 1.15);
    orb.set_property(PropertyFloat::DefaultScale, f64::from(1.3_f32));
    orb.set_property(PropertyFloat::Translucency, f64::from(0.6_f32));
    orb.set_property(PropertyFloat::ManaConversionMod, 0.31);
    orb.set_property(PropertyFloat::ElementalDamageMod, 1.2);

    // biota_properties_d_i_d
    orb.set_property(
        PropertyString::Name,
        "Replica Drudge Scrying Orb".to_owned(),
    );
    orb.set_property(
        PropertyString::LongDesc,
        "Same same, but different.".to_owned(),
    );

    // biota_properties_d_i_d
    orb.set_property(PropertyDataId::Setup, 33_558_259);
    orb.set_property(PropertyDataId::SoundTable, 536_870_932);
    orb.set_property(PropertyDataId::PaletteBase, 67_111_919);
    orb.set_property(PropertyDataId::Icon, 100_674_116);
    orb.set_property(PropertyDataId::PhysicsEffectTable, 872_415_275);
    orb.set_property(PropertyDataId::Spell, 2076);
    orb.set_property(PropertyDataId::IconUnderlay, 100_686_604);

    // biota_properties_spell_book
    for spell in [2076, 2101, 2242, 2244, 2507, 2577, 2581] {
        let _ = orb.biota.get_or_add_known_spell(spell, 2.0);
    }

    player_factory::try_add_to_inventory_created(w, player, Some(orb));

    // todo Buffing wand that has all defenses maxed
}

// ACE: PlayerFactoryEx.AddWeeniesToInventory
fn add_weenies_to_inventory(
    w: &mut World,
    player: &mut CreatedPlayer,
    weenie_ids: &[u32],
    mut stack_size: Option<u16>,
) {
    for &weenie_id in weenie_ids {
        let Some(mut loot) = player_factory::create_new_world_object_by_wcid(w, weenie_id) else {
            continue; // weenie doesn't exist
        };

        if stack_size.is_none() {
            stack_size = loot.max_stack_size();
        }

        if stack_size.is_some_and(|s| s > 1) {
            loot.set_stack_size(stack_size.map(i32::from));
        }

        // Make sure the item is full of mana
        if loot.item_cur_mana().is_some() {
            let max = loot.item_max_mana();
            loot.set_item_cur_mana(max);
        }

        player_factory::try_add_to_inventory_created(w, player, Some(loot));
    }
}

// ACE: PlayerFactoryEx.AddAllSpells
fn add_all_spells(w: &mut World, player: &mut CreatedPlayer) {
    use crate::world_objects::player_spells::learn_spells_in_bulk;
    use empyrean_entity::enums::MagicSchool;

    player_factory::with_player_in_world(w, player, None, |w, player| {
        for spell_level in 1..=8u32 {
            learn_spells_in_bulk(
                w,
                player,
                MagicSchool::CreatureEnchantment,
                spell_level,
                false,
            );
            learn_spells_in_bulk(w, player, MagicSchool::ItemEnchantment, spell_level, false);
            learn_spells_in_bulk(w, player, MagicSchool::LifeMagic, spell_level, false);
            learn_spells_in_bulk(w, player, MagicSchool::VoidMagic, spell_level, false);
            learn_spells_in_bulk(w, player, MagicSchool::WarMagic, spell_level, false);
        }
    });
}

// ACE: PlayerFactoryEx.AddRend
/// The rending imbue for the item's damage type (`W_DamageType`), its icon underlay, and one more
/// tinker.
pub fn add_rend(world_object: &mut WorldObject) {
    let mut imbued_effect_type = ImbuedEffectType::Undef;

    let damage_type = DamageType(
        world_object
            .get_property(PropertyInt::DamageType)
            .unwrap_or(0)
            .cs_cast(),
    );
    if damage_type == DamageType::Slash {
        imbued_effect_type = ImbuedEffectType::SlashRending;
    }
    if damage_type == DamageType::Pierce {
        imbued_effect_type = ImbuedEffectType::PierceRending;
    }
    if damage_type == DamageType::Bludgeon {
        imbued_effect_type = ImbuedEffectType::BludgeonRending;
    }
    if damage_type == DamageType::Cold {
        imbued_effect_type = ImbuedEffectType::ColdRending;
    }
    if damage_type == DamageType::Fire {
        imbued_effect_type = ImbuedEffectType::FireRending;
    }
    if damage_type == DamageType::Acid {
        imbued_effect_type = ImbuedEffectType::AcidRending;
    }
    if damage_type == DamageType::Electric {
        imbued_effect_type = ImbuedEffectType::ElectricRending;
    }
    if damage_type == (DamageType::Slash | DamageType::Pierce) {
        imbued_effect_type = ImbuedEffectType::SlashRending;
    }

    if imbued_effect_type != ImbuedEffectType::Undef {
        world_object.set_property(PropertyInt::ImbuedEffect, imbued_effect_type.0.cs_cast());
        let underlay = *crate::managers::recipe_manager::ICON_UNDERLAY
            .get(&imbued_effect_type)
            .unwrap_or_else(|| {
                panic!("KeyNotFoundException: IconUnderlay[{imbued_effect_type:?}]")
            });
        world_object.set_icon_underlay_id(Some(underlay));
        let tinkered = world_object
            .get_property(PropertyInt::NumTimesTinkered)
            .map(|n| n.wrapping_add(1));
        match tinkered {
            Some(n) => world_object.set_property(PropertyInt::NumTimesTinkered, n),
            None => world_object.remove_property(PropertyInt::NumTimesTinkered),
        }
    }
}

// ACE: PlayerFactoryEx.MakeSurePlayerHasFullStackForWeenies
/// For each weenie the player holds less than a full stack of (or none of), one more item topping
/// the amount up to a full stack.
pub fn make_sure_player_has_full_stack_for_weenies(
    w: &mut World,
    player: &mut CreatedPlayer,
    weenie_ids: &[u32],
) {
    for &weenie_id in weenie_ids {
        let mut amount_found: i32 = 0;
        let mut max_stack_size: i32 = 0;
        for item in player.get_all_possessions(w) {
            if item.biota.weenie_class_id == weenie_id {
                amount_found = amount_found.wrapping_add(item.stack_size().unwrap_or(1));
                max_stack_size = item.max_stack_size().map_or(1, i32::from);
            }
        }

        if amount_found > 0 && amount_found >= max_stack_size {
            continue;
        }

        let Some(mut loot) = player_factory::create_new_world_object_by_wcid(w, weenie_id) else {
            continue; // weenie doesn't exist
        };

        if loot.max_stack_size().is_some_and(|m| m > 1) {
            let amount_to_add = loot
                .max_stack_size()
                .map(|m| i32::from(m).wrapping_sub(amount_found));
            loot.set_stack_size_prop(amount_to_add);
            let stack = amount_to_add.unwrap_or(1);
            loot.set_encumbrance_val(Some(
                loot.stack_unit_encumbrance()
                    .unwrap_or(0)
                    .wrapping_mul(stack),
            ));
            loot.set_value(Some(
                loot.stack_unit_value().unwrap_or(0).wrapping_mul(stack),
            ));
        }

        player_factory::try_add_to_inventory_created(w, player, Some(loot));
    }
}
