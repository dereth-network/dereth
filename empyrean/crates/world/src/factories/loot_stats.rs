// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/LootStats.cs
//! Port of `Source/ACE.Server/Factories/LootStats.cs`.
//!
//! The `/testlootgen` statistics: counters and per-kind tables of generated items. C#'s
//! string interpolation is .NET's: a `double`/`float` prints its shortest round-trip form, a
//! nullable prints nothing when `null`, a `bool` prints `True`/`False`, an enum its name.

use empyrean_common::dotnet::{format, to_string};
use empyrean_entity::enums::{AmmoType, AttackType, EquipMask, ItemType, Skill};

use crate::entity::aetheria;
use crate::factories::loot_tables::CANTRIP_SETS;
use crate::world_objects::world_object::WorldObject;

/// `{x}` of an `int?`: the number, or nothing for `null`.
fn opt<T: ToString>(v: Option<T>) -> String {
    v.map_or_else(String::new, |v| v.to_string())
}

/// `{x}` of a `double`.
fn dbl(v: f64) -> String {
    to_string(v)
}

/// `string.PadRight(n)`.
fn pad_right(s: &str, n: usize) -> String {
    format!("{s:<n$}")
}

/// `wo.Name` (`PropertyString.Name`).
fn name(wo: &WorldObject) -> String {
    wo.get_property(empyrean_entity::enums::PropertyString::Name)
        .unwrap_or_default()
}

/// ACE `LootStats`: the counters (floats in ACE, so the drop rates divide in float), the tables
/// of item lines, and the mana, armor level and pet rating statistics.
// ACE: LootStats
#[derive(Debug, Default, Clone)]
pub struct LootStats {
    // Counters
    pub armor_count: f32,
    pub melee_weapon_count: f32,
    pub caster_count: f32,
    pub missile_weapon_count: f32,
    pub jewelry_count: f32,
    pub jewelry_necklace_count: f32,
    pub jewelry_bracelet_count: f32,
    pub jewelry_ring_count: f32,
    pub jewelry_trinket_count: f32,
    pub gem_count: f32,
    pub aetheria_count: f32,
    pub clothing_count: f32,
    pub cloak_count: f32,
    pub other_count: f32,
    pub null_count: f32,
    pub min_items_created: i32,
    pub max_items_created: i32,
    pub spell_components: f32,
    pub food: f32,
    pub key: f32,
    pub mana_stone: f32,
    pub misc: f32,
    pub total_items: f32,
    pub scrolls: f32,
    pub pets_count: f32,
    pub spirits: f32,
    pub potions: f32,
    pub healing_kit: f32,
    pub dinner_ware: f32,
    pub level_eight_comp: f32,
    pub minor_cantrip_count: f32,
    pub major_cantrip_count: f32,
    pub epic_cantrip_count: f32,
    pub legendary_cantrip_count: f32,

    // Tables
    pub melee_weapons: Vec<String>,
    pub missile_weapons: Vec<String>,
    pub caster_weapons: Vec<String>,
    pub armor: Vec<String>,
    pub cloaks: Vec<String>,
    pub pets: Vec<String>,
    pub aetheria: Vec<String>,

    pub jewelry: Vec<String>,

    // Item Stats
    pub item_max_mana: i32,
    pub min_mana: i32,
    pub max_mana: i32,
    pub has_mana_count: i32,
    pub total_max_mana: i32,
    pub min_al: i32,
    pub max_al: i32,
    pub min_al_item: Option<String>,
    pub max_al_item: Option<String>,

    // Pet Stats
    pub pets_total_ratings: i32,
    pub pet_ratings_equal_zero: i32,
    pub pet_ratings_equal_one: i32,
    pub pet_ratings_over_ten: i32,
    pub pet_ratings_over_twenty: i32,
    pub pet_ratings_over_thirty: i32,
    pub pet_ratings_over_forty: i32,
    pub pet_ratings_over_fifty: i32,
    pub pet_ratings_over_sixty: i32,
    pub pet_ratings_over_seventy: i32,
    pub pet_ratings_over_eighty: i32,
    pub pet_ratings_over_ninety: i32,
    pub pet_ratings_over_hundred: i32,
}

impl LootStats {
    /// The table headers are CSV lines when `logstats` (the file output), tab-aligned otherwise.
    // ACE: LootStats.LootStats
    #[must_use]
    pub fn new(logstats: bool) -> Self {
        let mut s = Self {
            // Counters
            min_mana: 50000,
            min_items_created: 100,
            min_al: 1000,
            ..Self::default()
        };

        // Tables
        if logstats {
            s.melee_weapons = vec!["-----Melee Weapons----\nSkill,Wield,Damage,MStrike,Variance,DefenseMod,MagicDBonus,MissileDBonus,Cantrip,Value,Burden,Type".to_owned()];
            s.missile_weapons = vec!["-----Missile Weapons----\nType,Wield,Modifier,ElementBonus,DefenseMod,MagicDBonus,MissileDBonus,Value,Burden".to_owned()];
            s.caster_weapons = vec!["-----Caster Weapons----\nWield,Element,ElementBonus,DefenseMod,MagicDBonus,MissileDBonus,Value,MaxMana,Burden".to_owned()];
            s.armor = vec![
                "-----Armor----\nAL,Arcane,Value,Burden,Epic,Legendary,EquipmentSet,Type"
                    .to_owned(),
            ];
            s.pets =
                vec!["-----Pet Devices----\nLevel,Dmg,DmgR,Crit,CritD,CDR,CritR,Total".to_owned()];
            s.aetheria = vec!["-----Aetheria----\nColor,Level".to_owned()];
            s.cloaks = vec!["-----Cloaks----\nLevel,Wield,Proc,Value,Set".to_owned()];
            s.jewelry = vec!["-----Jewelry----\nSlot,Arcane,Value".to_owned()];
        } else {
            s.melee_weapons = vec!["-----Melee Weapons----\n Skill \t\t\t Wield \t Damage \t MStrike \t Variance \t DefenseMod \t MagicDBonus \t MissileDBonus\tCantrip\t Value\t Burden\t Type".to_owned()];
            s.missile_weapons = vec!["-----Missile Weapons----\n Type \t Wield \t Modifier \tElementBonus \t DefenseMod \t MagicDBonus \t MissileDBonus\t Value\t Burden".to_owned()];
            s.caster_weapons = vec!["-----Caster Weapons----\n Wield \t Element \t ElementBonus \t DefenseMod \t MagicDBonus \t MissileDBonus \t Value\t Burden\t MaxMana".to_owned()];
            s.armor = vec!["-----Armor----\n AL\tArcane\tValue\tBurden\tEpics\tLegend\tEquipment Set\t\t\tType".to_owned()];
            s.pets = vec!["-----Pet Devices----\n Level \t Dmg \t DmgR \t Crit \t CritD \t CDR \t CritR \t Total".to_owned()];
            s.aetheria = vec!["-----Aetheria----\n Color \t Level".to_owned()];
            s.cloaks = vec!["-----Cloaks----\n Level\t Wield\t Proc\t Value\t Set".to_owned()];
            s.jewelry = vec!["-----Jewelry----\n Slot\t Arcane\t Value".to_owned()];
        }
        s
    }

    /// Counts one generated item (a `null` item counts as `NullCount`) and adds its table line.
    ///
    /// # Panics
    /// Where ACE reads `.Value` of a `null` property (a melee weapon without Damage, say) and
    /// throws `InvalidOperationException`.
    // ACE: LootStats.AddItem
    #[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
    pub fn add_item(&mut self, wo: Option<&WorldObject>, log_stats: bool) {
        const NULLABLE: &str = "InvalidOperationException: Nullable object must have a value";

        // Weapon Properties
        let mut missile_def_mod = 0.00f64;
        let mut magic_def_mod = 0.00f64;
        let mut wield = 0.00f64;
        let mut value = 0i32;

        self.total_items += 1.0;

        // Loop depending on how many items you are creating
        let Some(test_item) = wo else {
            self.null_count += 1.0;
            return;
        };

        let epic_cantrips = test_item
            .biota
            .get_matching_spells(&CANTRIP_SETS.epic_cantrips)
            .len();
        let legendary_cantrips = test_item
            .biota
            .get_matching_spells(&CANTRIP_SETS.legendary_cantrips)
            .len();
        let item_name = name(test_item);

        match test_item.item_type() {
            ItemType::None => {}
            ItemType::MeleeWeapon => {
                self.melee_weapon_count += 1.0;

                let mut cantrip = false;
                if epic_cantrips > 0 {
                    cantrip = true;
                }
                if legendary_cantrips > 0 {
                    cantrip = true;
                }

                let mut strike_type = "N";
                if let Some(v) = test_item.weapon_magic_defense() {
                    magic_def_mod = v;
                }
                if let Some(v) = test_item.value() {
                    value = v;
                }
                if let Some(v) = test_item.weapon_missile_defense() {
                    missile_def_mod = v;
                }
                if let Some(v) = test_item.wield_difficulty() {
                    wield = f64::from(v);
                }
                let skill = test_item.weapon_skill().to_dotnet_string();
                let damage = test_item.damage().expect(NULLABLE);
                let variance = dbl(test_item.damage_variance().expect(NULLABLE));
                let defense = dbl(test_item.weapon_defense().expect(NULLABLE));
                let cantrip_s = if cantrip { "True" } else { "False" };
                let burden = opt(test_item.encumbrance_val());
                if test_item.weapon_skill() == Skill::TwoHandedCombat {
                    if log_stats {
                        self.melee_weapons.push(format!("{skill},{},{damage},{strike_type},{variance},{defense},{},{},{cantrip_s},{value},{burden},{item_name}", dbl(wield), dbl(magic_def_mod), dbl(missile_def_mod)));
                    } else {
                        self.melee_weapons.push(format!("{skill}\t\t {}\t {damage}\t\t {strike_type} \t\t {variance}\t\t {defense}\t\t {}\t\t {}\t\t {cantrip_s}\t {value}\t {burden} \t {item_name}", dbl(wield), format(magic_def_mod, "G4"), format(missile_def_mod, "G4")));
                    }
                } else {
                    if test_item
                        .w_attack_type()
                        .intersects(AttackType::TripleStrike)
                    {
                        strike_type = "3x";
                    } else if test_item
                        .w_attack_type()
                        .intersects(AttackType::DoubleStrike)
                    {
                        strike_type = "2x";
                    }
                    if log_stats {
                        self.melee_weapons.push(format!("{skill},{},{damage},{strike_type},{variance},{defense},{},{},{cantrip_s},{value},{burden},{item_name}", dbl(wield), dbl(magic_def_mod), dbl(missile_def_mod)));
                    } else {
                        self.melee_weapons.push(format!(" {skill}\t\t {}\t {damage}\t\t {strike_type}\t\t {variance}\t\t {defense}\t\t {}\t\t {}\t\t {cantrip_s}\t {value}\t {burden} \t {item_name}", dbl(wield), format(magic_def_mod, "G4"), format(missile_def_mod, "G4")));
                    }
                }
            }
            ItemType::Armor => {
                self.armor_count += 1.0;
                let mut equipment_set = "None    ".to_owned();
                // float cantripSpells = 0;

                if let Some(set) = test_item.equipment_set_id() {
                    equipment_set = set.to_dotnet_string();
                }
                let al = opt(test_item.armor_level());
                let arcane = opt(test_item.item_difficulty());
                let item_value = test_item.value().expect(NULLABLE);
                let burden = opt(test_item.encumbrance_val());
                if log_stats {
                    self.armor.push(format!("{al},{arcane},{item_value},{burden},{epic_cantrips},{legendary_cantrips},{equipment_set},{item_name}"));
                } else {
                    self.armor.push(format!(" {al}\t{arcane}\t{item_value}\t{burden}\t{epic_cantrips}\t{legendary_cantrips}\t{}\t\t\t{item_name}", pad_right(&equipment_set, 8)));
                }
                if item_name.contains("Sheild") {
                    // typo?
                } else {
                    if let Some(armor_level) = test_item.armor_level() {
                        if armor_level > self.max_al {
                            self.max_al = armor_level;
                            self.max_al_item = Some(item_name.clone());
                        }
                        if armor_level < self.min_al {
                            self.min_al = armor_level;
                            self.min_al_item = Some(item_name.clone());
                        }
                    }
                }
            }
            ItemType::Clothing => {
                if item_name.contains("Cloak") {
                    let mut cloak_set = "None ".to_owned();
                    if let Some(set) = test_item.equipment_set_id() {
                        cloak_set = set.to_dotnet_string();
                    }
                    self.cloak_count += 1.0;
                    let level = opt(test_item.item_max_level());
                    let wield_difficulty = opt(test_item.wield_difficulty());
                    let proc = test_item.cloak_weave_proc().expect(NULLABLE);
                    let item_value = test_item.value().expect(NULLABLE);
                    if log_stats {
                        self.cloaks.push(format!(
                            "{level},{wield_difficulty},{proc},{item_value},{cloak_set}"
                        ));
                    } else {
                        self.cloaks.push(format!(
                            " {level}\t {wield_difficulty}\t {proc}\t {item_value}\t {cloak_set}"
                        ));
                    }
                } else {
                    self.clothing_count += 1.0;
                }
            }
            ItemType::Jewelry => {
                self.jewelry_count += 1.0;
                let mut jewelry_slot = "";
                match test_item.valid_locations() {
                    Some(EquipMask::NeckWear) => {
                        self.jewelry_necklace_count += 1.0;
                        jewelry_slot = "Neck";
                    }
                    Some(EquipMask::WristWear) => {
                        self.jewelry_bracelet_count += 1.0;
                        jewelry_slot = "Brace";
                    }
                    Some(EquipMask::FingerWear) => {
                        self.jewelry_ring_count += 1.0;
                        jewelry_slot = "Ring";
                    }
                    Some(EquipMask::TrinketOne) => {
                        self.jewelry_trinket_count += 1.0;
                        jewelry_slot = "Trink";
                    }
                    _ => {
                        // Console.WriteLine(testItem.Name);
                    }
                }
                let arcane = opt(test_item.item_difficulty());
                let item_value = opt(test_item.value());
                if log_stats {
                    self.jewelry
                        .push(format!("{jewelry_slot},{arcane},{item_value}"));
                } else {
                    self.jewelry
                        .push(format!(" {jewelry_slot}\t {arcane}\t {item_value}"));
                }
            }
            ItemType::Food => self.food += 1.0,
            ItemType::Misc => self.add_misc(test_item, &item_name, log_stats),
            ItemType::MissileWeapon => {
                let mut ele_bonus = 0.00f64;
                let mut damage_mod = 0.00f64;
                let mut missile_type = "Other";
                if let Some(ammo_type) = test_item.ammo_type() {
                    match ammo_type {
                        AmmoType::Arrow => {
                            missile_type = "Bow";
                            self.missile_weapon_count += 1.0;
                        }
                        AmmoType::Bolt => {
                            missile_type = "X Bow";
                            self.missile_weapon_count += 1.0;
                        }
                        AmmoType::Atlatl => {
                            missile_type = "Thrown";
                            self.missile_weapon_count += 1.0;
                        }
                        _ => {}
                    }
                }
                if let Some(v) = test_item.weapon_magic_defense() {
                    magic_def_mod = v;
                }
                if let Some(v) = test_item.value() {
                    value = v;
                }
                if let Some(v) = test_item.weapon_missile_defense() {
                    missile_def_mod = v;
                }
                if let Some(v) = test_item.wield_difficulty() {
                    wield = f64::from(v);
                }
                if let Some(v) = test_item.elemental_damage_bonus() {
                    ele_bonus = f64::from(v);
                }
                if let Some(v) = test_item.damage_mod() {
                    damage_mod = v;
                }

                if missile_type == "Other" {
                    self.dinner_ware += 1.0;
                } else {
                    let defense = dbl(test_item.weapon_defense().expect(NULLABLE));
                    let burden = opt(test_item.encumbrance_val());
                    if log_stats {
                        self.missile_weapons.push(format!(
                            "{missile_type},{},{},{},{defense},{},{},{value},{burden}",
                            dbl(wield),
                            dbl(damage_mod),
                            dbl(ele_bonus),
                            dbl(magic_def_mod),
                            dbl(missile_def_mod)
                        ));
                    } else {
                        self.missile_weapons.push(format!("{missile_type}\t {}\t {}\t\t{}\t\t {defense}\t\t {}\t\t {}\t\t {value}\t {burden}", dbl(wield), dbl(damage_mod), dbl(ele_bonus), format(magic_def_mod, "G4"), format(missile_def_mod, "G4")));
                    }
                }
            }
            ItemType::Gem => {
                let mut aetheria_color = "None";
                if aetheria::is_aetheria(test_item.biota.weenie_class_id) {
                    self.aetheria_count += 1.0;
                    match test_item.wield_difficulty() {
                        Some(75) => aetheria_color = "Blue  ",
                        Some(150) => aetheria_color = "Yellow",
                        Some(225) => aetheria_color = "Red   ",
                        _ => {}
                    }
                    let level = opt(test_item.item_max_level());
                    if log_stats {
                        self.aetheria.push(format!("{aetheria_color},{level}"));
                    } else {
                        self.aetheria.push(format!(" {aetheria_color}\t {level}"));
                    }
                } else {
                    self.gem_count += 1.0;
                }
            }
            ItemType::SpellComponents => self.spell_components += 1.0,
            ItemType::Writable => {
                let scrolls = "Scroll";

                if item_name.contains(scrolls) {
                    self.scrolls += 1.0;
                } else {
                    empyrean_common::console_write_line!("ItemType.Writeable Name={item_name}");
                }
            }
            ItemType::Key => self.key += 1.0,
            ItemType::Caster => {
                self.caster_count += 1.0;
                let mut ele_mod = 0.00f64;
                if let Some(v) = test_item.weapon_magic_defense() {
                    magic_def_mod = v;
                }
                if let Some(v) = test_item.value() {
                    value = v;
                }
                if let Some(v) = test_item.weapon_missile_defense() {
                    missile_def_mod = v;
                }
                if let Some(v) = test_item.wield_difficulty() {
                    wield = f64::from(v);
                }
                if let Some(v) = test_item.elemental_damage_mod() {
                    ele_mod = v;
                }
                if let Some(v) = test_item.item_max_mana() {
                    self.item_max_mana = v;
                }
                let defense = dbl(test_item.weapon_defense().expect(NULLABLE));
                let burden = opt(test_item.encumbrance_val());
                if log_stats {
                    self.caster_weapons.push(format!(
                        "{},{item_name},{},{defense},{},{},{value},{burden},{}",
                        dbl(wield),
                        dbl(ele_mod),
                        dbl(magic_def_mod),
                        dbl(missile_def_mod),
                        self.item_max_mana
                    ));
                } else {
                    self.caster_weapons.push(format!(
                        " {}\t {}{}\t\t {defense}\t\t {}\t\t {}\t\t {value}\t {burden} \t {}",
                        dbl(wield),
                        pad_right(&item_name, 16),
                        dbl(ele_mod),
                        format(magic_def_mod, "G4"),
                        format(missile_def_mod, "G4"),
                        self.item_max_mana
                    ));
                }
            }
            ItemType::ManaStone => self.mana_stone += 1.0,
            ItemType::CraftCookingBase
            | ItemType::CraftAlchemyBase
            | ItemType::CraftFletchingBase
            | ItemType::CraftAlchemyIntermediate
            | ItemType::CraftFletchingIntermediate
            | ItemType::TinkeringTool
            | ItemType::TinkeringMaterial => self.other_count += 1.0,
            ItemType::Item => {
                empyrean_common::console_write_line!("ItemType.item Name={item_name}")
            }
            ItemType::Creature
            | ItemType::Money
            | ItemType::Container
            | ItemType::Useless
            | ItemType::Portal
            | ItemType::Lockable
            | ItemType::PromissoryNote
            | ItemType::Service
            | ItemType::MagicWieldable
            | ItemType::LifeStone
            | ItemType::Gameboard
            | ItemType::PortalMagicTarget
            | ItemType::LockableMagicTarget
            | ItemType::Vestements
            | ItemType::Weapon
            | ItemType::WeaponOrCaster
            | ItemType::RedirectableItemEnchantmentTarget
            | ItemType::ItemEnchantableTarget
            | ItemType::VendorShopKeep
            | ItemType::VendorGrocer => {}
            _ => self.other_count += 1.0,
        }

        if let Some(item_max_mana) = test_item.item_max_mana() {
            if item_max_mana > self.max_mana {
                self.max_mana = item_max_mana;
            }
            if item_max_mana < self.min_mana {
                self.min_mana = item_max_mana;
            }
            self.has_mana_count += 1;
            self.total_max_mana = self.total_max_mana.wrapping_add(item_max_mana);
        }
        if test_item
            .get_property(empyrean_entity::enums::PropertyString::Name)
            .is_none()
        {
            empyrean_common::console_write_line!("*Name is Null*");
            return;
        }
        if epic_cantrips > 0 {
            self.epic_cantrip_count += 1.0;
        }
        if legendary_cantrips > 0 {
            self.legendary_cantrip_count += 1.0;
        }
    }

    /// The `ItemType.Misc` arm: spirits, pet devices (with their rating buckets), potions,
    /// level 8 components, healing kits, other.
    fn add_misc(&mut self, test_item: &WorldObject, item_name: &str, log_stats: bool) {
        let spirit = "Spirit";
        let potions = [
            "Philtre", "Elixir", "Tonic", "Brew", "Potion", "Draught", "Tincture",
        ];

        let healing_kits = "Kit";
        let spellcomps = ["Glyph", "Ink", "Quill"];

        if item_name.contains(spirit) {
            self.spirits += 1.0;
        } else if test_item.is_pet_device() {
            self.pets_count += 1.0;
            let mut total_ratings = 0;
            let mut damage = 0;
            let mut damage_resist = 0;
            let mut crit = 0;
            let mut crit_damage = 0;
            let mut crit_damage_resist = 0;
            let mut crit_resist = 0;

            let pet_level = match test_item.use_requires_skill_level() {
                Some(570) => 200,
                Some(530) => 180,
                Some(475) => 150,
                Some(430) => 125,
                Some(400) => 100,
                Some(370) => 80,
                Some(310) => 50,
                _ => 0,
            };

            if let Some(v) = test_item.gear_damage() {
                total_ratings += v;
                damage = v;
            }
            if let Some(v) = test_item.gear_damage_resist() {
                total_ratings += v;
                damage_resist = v;
            }
            if let Some(v) = test_item.gear_crit() {
                total_ratings += v;
                crit = v;
            }
            if let Some(v) = test_item.gear_crit_damage() {
                total_ratings += v;
                crit_damage = v;
            }
            if let Some(v) = test_item.gear_crit_damage_resist() {
                total_ratings += v;
                crit_damage_resist = v;
            }
            if let Some(v) = test_item.gear_crit_resist() {
                total_ratings += v;
                crit_resist = v;
            }
            if log_stats {
                self.pets.push(format!("{pet_level},{damage},{damage_resist},{crit},{crit_damage},{crit_damage_resist},{crit_resist},{total_ratings}"));
            } else {
                self.pets.push(format!(" {pet_level}\t {damage}\t {damage_resist}\t {crit}\t {crit_damage}\t {crit_damage_resist}\t {crit_resist}\t {total_ratings}"));
            }

            if total_ratings > 99 {
                self.pet_ratings_over_hundred += 1;
            } else if total_ratings > 89 {
                self.pet_ratings_over_ninety += 1;
            } else if total_ratings > 79 {
                self.pet_ratings_over_eighty += 1;
            } else if total_ratings > 69 {
                self.pet_ratings_over_seventy += 1;
            } else if total_ratings > 59 {
                self.pet_ratings_over_sixty += 1;
            } else if total_ratings > 49 {
                self.pet_ratings_over_fifty += 1;
            } else if total_ratings > 39 {
                self.pet_ratings_over_forty += 1;
            } else if total_ratings > 29 {
                self.pet_ratings_over_thirty += 1;
            } else if total_ratings > 19 {
                self.pet_ratings_over_twenty += 1;
            } else if total_ratings > 9 {
                self.pet_ratings_over_ten += 1;
            } else if total_ratings > 0 {
                self.pet_ratings_equal_one += 1;
            } else if total_ratings < 1 {
                self.pet_ratings_equal_zero += 1;
            }
        } else if potions.iter().any(|p| item_name.contains(p)) {
            self.potions += 1.0;
        } else if spellcomps.iter().any(|p| item_name.contains(p)) {
            self.level_eight_comp += 1.0;
        } else if item_name.contains(healing_kits) {
            self.healing_kit += 1.0;
        } else {
            // Console.WriteLine($"ItemType.Misc Name={testItem.Name}");
            self.misc += 1.0;
        }
    }
}
