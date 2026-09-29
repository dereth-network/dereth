// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/LootGenerationFactory_Test.cs
//! Port of `Source/ACE.Server/Factories/LootGenerationFactory_Test.cs`.
//!
//! The `/testlootgen` and `/testlootgencorpse` simulators: generate many items (or corpses),
//! gather [`LootStats`] and print (optionally save) a report.

use empyrean_content::models::world::TreasureDeath;
use empyrean_tables::enums::LootBias;

use crate::factories::loot_generation_factory as lgf;
use crate::factories::loot_stats::LootStats;
use crate::World;

/// Generates `num_items` test items of the tier and reports their statistics.
// ACE: LootGenerationFactory_Test.TestLootGen
pub fn test_loot_gen(
    w: &mut World,
    num_items: i32,
    tier: i32,
    log_stats: bool,
    display_table: &str,
) -> String {
    let mut display_header =
        "\n LootFactory Simulator - Items\n ---------------------\n".to_owned();

    println!("Creating {num_items} items, that are in tier {tier}");

    let mut loot_stats = LootStats::new(log_stats);

    // Create a dummy treasure profile for passing in tier value
    let profile = TreasureDeath {
        tier,
        loot_quality_mod: 0.0,
        ..TreasureDeath::default()
    };

    // Loop depending on how many items you are creating
    for _ in 0..num_items {
        let test_item = lgf::create_random_loot_objects_test(w, &profile, true, LootBias::UnBiased);
        loot_stats.add_item(test_item.as_ref(), log_stats);
    }

    let display_stats = build_display_stats(&loot_stats, display_table);

    println!("{display_header}");
    println!("{display_stats}");

    display_header += &format!(
        " A total of {} items were generated in Tier {tier}. \n",
        empyrean_common::dotnet::to_string(loot_stats.total_items)
    );

    if log_stats {
        // DIVERGE: `DateTime.Now` (local time) in the file name is the world clock's UTC.
        let myfilename = format!("LootSim-{}.csv", file_stamp(w));
        if let Err(e) = std::fs::write(&myfilename, format!("{display_header}{display_stats}")) {
            log::error!("TestLootGen: couldn't write {myfilename}: {e}");
        }
    }

    display_header
}

/// `{0:hh-mm-ss-tt_MM-dd-yyyy}` of the world clock.
fn file_stamp(w: &World) -> String {
    let t = w.now.utc;
    let hour = t.hour();
    let (h12, tt) = match hour {
        0 => (12, "AM"),
        1..=11 => (hour, "AM"),
        12 => (12, "PM"),
        _ => (hour - 12, "PM"),
    };
    format!(
        "{h12:02}-{:02}-{:02}-{tt}_{:02}-{:02}-{:04}",
        t.minute(),
        t.second(),
        t.month(),
        t.day(),
        t.year()
    )
}

/// Generates `num_corpses` corpses of a death treasure profile and reports their statistics.
// ACE: LootGenerationFactory_Test.TestLootGenMonster
pub fn test_loot_gen_monster(
    w: &mut World,
    death_treasure_did: u32,
    num_corpses: i32,
    log_stats: bool,
    display_table: &str,
) -> String {
    let mut display_header =
        "\n LootFactory Simulator - Corpses\n ---------------------\n".to_owned();

    let mut ls = LootStats::new(log_stats);

    println!("Creating {num_corpses} corpses.");

    let Some(death_treasure) = w.content.get_cached_death_treasure(death_treasure_did) else {
        display_header += &format!(" DID {death_treasure_did} you specified is invalid. \n");
        return display_header;
    };
    display_header += &format!(
        " Loot profile {} (a Tier {} profile) from DID {death_treasure_did} was used for creating {num_corpses} corpses. \n",
        death_treasure.id, death_treasure.tier
    );

    for _ in 0..num_corpses {
        let corpse_container = lgf::create_random_loot_objects(w, &death_treasure);

        let count =
            i32::try_from(corpse_container.len()).expect("a corpse's item count fits an int");
        if count < ls.min_items_created {
            ls.min_items_created = count;
        }
        if count > ls.max_items_created {
            ls.max_items_created = count;
        }

        for loot_item in &corpse_container {
            ls.add_item(Some(loot_item), log_stats);
        }
    }

    let display_stats = build_display_stats(&ls, display_table);

    println!("{display_header}");
    println!("{display_stats}");

    display_header += &format!(
        " A total of {} unique items were generated. \n",
        empyrean_common::dotnet::to_string(ls.total_items)
    );

    if log_stats {
        // DIVERGE: `DateTime.Now` (local time) in the file name is the world clock's UTC.
        let myfilename = format!(
            "LootSim_DeathTreasureDID-{death_treasure_did}_{}.csv",
            file_stamp(w)
        );
        if let Err(e) = std::fs::write(&myfilename, format!("{display_header}{display_stats}")) {
            log::error!("TestLootGenMonster: couldn't write {myfilename}: {e}");
        }
    }

    display_header
}

/// The report: the selected tables, the counts, the drop rates (float division, so `NaN` when
/// nothing of a kind was generated, as in ACE), armor levels, pet ratings, jewelry, cantrips,
/// mana and corpse sizes.
// ACE: LootGenerationFactory_Test.BuildDisplayStats
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn build_display_stats(ls: &LootStats, displaytable: &str) -> String {
    let f = |v: f32| empyrean_common::dotnet::to_string(v);
    let pct = |n: f32, d: f32| f(n / d * 100.0);
    let mut display_stats = String::new();
    // Seeing if a table was specified to display
    // fixme
    match displaytable {
        "melee" => display_stats += &format!("{}\n", ls.melee_weapons.join("\n")),
        "missile" => display_stats += &format!("{}\n", ls.missile_weapons.join("\n")),
        "caster" => display_stats += &format!("{}\n", ls.caster_weapons.join("\n")),
        "jewelry" => display_stats += &format!("{}\n", ls.jewelry.join("\n")),
        "armor" => display_stats += &format!("{}\n", ls.armor.join("\n")),
        "cloak" => display_stats += &format!("{}\n", ls.cloaks.join("\n")),
        "pet" => display_stats += &format!("{}\n", ls.pets.join("\n")),
        "aetheria" => display_stats += &format!("{}\n", ls.aetheria.join("\n")),
        "all" => {
            for table in [
                &ls.melee_weapons,
                &ls.missile_weapons,
                &ls.caster_weapons,
                &ls.jewelry,
                &ls.armor,
                &ls.cloaks,
                &ls.pets,
                &ls.aetheria,
            ] {
                display_stats += &format!("{}\n", table.join("\n"));
            }
        }
        _ => {
            display_stats +=
                "\n No Table(s) was selected to display, showing only general statistics"
        }
    }

    let total_found = ls.armor_count
        + ls.melee_weapon_count
        + ls.caster_count
        + ls.missile_weapon_count
        + ls.jewelry_count
        + ls.gem_count
        + ls.clothing_count
        + ls.food
        + ls.spell_components
        + ls.key
        + ls.mana_stone
        + ls.pets_count
        + ls.spirits
        + ls.scrolls
        + ls.potions
        + ls.level_eight_comp
        + ls.healing_kit
        + ls.dinner_ware
        + ls.misc
        + ls.other_count
        + ls.null_count;

    display_stats += &format!(
        " \n Treasure Items \n ---- \n Armor={} \n MeleeWeapon={} \n Caster={} \n MissileWeapon={} \n Jewelry={} \n Gem={} \n Aetheria={} \n Clothing={} \n Cloaks={} \n \n Generic Items \n ---- \n Food={} \n SpellComps={} \n Keys={} \n ManaStones={} \n Pets={} \n EncapSpirits={} \n Scrolls={} \n Potions={} \n Healing Kits={} \n Level 8 Comps={} \n DinnerWare={} \n Misc={} \n Other={} \n NullCount={} \n Total Found={} \n TotalGenerated={}\n",
        f(ls.armor_count), f(ls.melee_weapon_count), f(ls.caster_count), f(ls.missile_weapon_count), f(ls.jewelry_count),
        f(ls.gem_count), f(ls.aetheria_count), f(ls.clothing_count), f(ls.cloak_count), f(ls.food), f(ls.spell_components),
        f(ls.key), f(ls.mana_stone), f(ls.pets_count), f(ls.spirits), f(ls.scrolls), f(ls.potions), f(ls.healing_kit),
        f(ls.level_eight_comp), f(ls.dinner_ware), f(ls.misc), f(ls.other_count), f(ls.null_count), f(total_found),
        f(ls.total_items)
    );

    let t = ls.total_items;
    display_stats += &format!(
        "\n Drop Rates \n ----\n Armor= {}% \n MeleeWeapon= {}% \n Caster= {}% \n MissileWeapon= {}% \n Jewelry= {}% \n Gem= {}% \n Aetheria= {}% \n Clothing= {}% \n Cloaks= {}% \n Food= {}% \n SpellComps= {}% \n Keys= {}% \n Mana Stones= {}% \n Pets= {}% \n Encap. Spirits= {}% \n Scrolls= {}% \n Potions= {}% \n Healing Kits= {}% \n Level 8 Comps= {}% \n DinnerWare= {}% \n Misc= {}% \n Other={}% \n",
        pct(ls.armor_count, t), pct(ls.melee_weapon_count, t), pct(ls.caster_count, t), pct(ls.missile_weapon_count, t),
        pct(ls.jewelry_count, t), pct(ls.gem_count, t), pct(ls.aetheria_count, t), pct(ls.clothing_count, t),
        pct(ls.cloak_count, t), pct(ls.food, t), pct(ls.spell_components, t), pct(ls.key, t), pct(ls.mana_stone, t),
        pct(ls.pets_count, t), pct(ls.spirits, t), pct(ls.scrolls, t), pct(ls.potions, t), pct(ls.healing_kit, t),
        pct(ls.level_eight_comp, t), pct(ls.dinner_ware, t), pct(ls.misc, t), pct(ls.other_count, t)
    );

    // Armor Level Stats
    display_stats += &format!(
        "\n Armor Levels \n ----\n MinAL = {}\t {}\n MaxAL = {}\t {}\n",
        ls.min_al,
        ls.min_al_item.as_deref().unwrap_or_default(),
        ls.max_al,
        ls.max_al_item.as_deref().unwrap_or_default()
    );

    // Pet Summons Stats
    display_stats += &format!(
        "\n Pets Ratings Stats \n ----\n   100+ = {} \n  90-99 = {} \n  80-89 = {} \n  70-79 = {} \n  60-69 = {} \n  50-59 = {} \n  40-49 = {} \n  30-39 = {} \n  20-29 = {} \n  10-19 = {} \n    1-9 = {} \n      0 = {} \n Total Pets Generated = {} \n",
        ls.pet_ratings_over_hundred, ls.pet_ratings_over_ninety, ls.pet_ratings_over_eighty, ls.pet_ratings_over_seventy,
        ls.pet_ratings_over_sixty, ls.pet_ratings_over_fifty, ls.pet_ratings_over_forty, ls.pet_ratings_over_thirty,
        ls.pet_ratings_over_twenty, ls.pet_ratings_over_ten, ls.pet_ratings_equal_one, ls.pet_ratings_equal_zero,
        f(ls.pets_count)
    );
    // Jewelry
    let j = ls.jewelry_count;
    display_stats += &format!(
        "\n Jewelry Counts Stats \n ----\n Necklace = {}\t Droprate = {}%\n Bracelet = {}\t Droprate = {}%\n     Ring = {}\t Droprate = {}%\n  Trinket = {}\t Droprate = {}%\n",
        f(ls.jewelry_necklace_count), pct(ls.jewelry_necklace_count, j), f(ls.jewelry_bracelet_count),
        pct(ls.jewelry_bracelet_count, j), f(ls.jewelry_ring_count), pct(ls.jewelry_ring_count, j),
        f(ls.jewelry_trinket_count), pct(ls.jewelry_trinket_count, j)
    );
    // Cantrip Counts
    display_stats += &format!(
        "\n Cantip Counts Stats \n ----\n      Epic = {}\t Droprate = {}%\n Legendary = {}\t Droprate = {}%\n",
        //$"     Minor = {ls.MinorCantripCount}\t Droprate = {ls.MinorCantripCount / ls.TotalItems * 100}%\n" +
        //$"     Major = {ls.MajorCantripCount}\t Droprate = {ls.MajorCantripCount / ls.TotalItems * 100}%\n" +
        f(ls.epic_cantrip_count),
        pct(ls.epic_cantrip_count, t),
        f(ls.legendary_cantrip_count),
        pct(ls.legendary_cantrip_count, t)
    );

    if ls.has_mana_count != 0 {
        display_stats += &format!(
            "\n Mana capacity across all items Min={}  Max={} Avg Mana={}",
            ls.min_mana,
            ls.max_mana,
            ls.total_max_mana / ls.has_mana_count
        );
    }
    if ls.min_items_created != 100 {
        display_stats += &format!(
            "\n Min Items on a corpse = {}, Max Items on coprse = {} \n",
            ls.min_items_created, ls.max_items_created
        );
    }
    display_stats
}
