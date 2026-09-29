// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Adapter/GDLE/Models/Event.cs, Source/ACE.Adapter/GDLE/Models/EventValue.cs, Source/ACE.Adapter/GDLE/Models/Spells.cs, Source/ACE.Adapter/GDLE/Models/SpellTable.cs, Source/ACE.Adapter/GDLE/Models/SpellBaseHash.cs, Source/ACE.Adapter/GDLE/Models/SpellValue.cs, Source/ACE.Adapter/GDLE/Models/MetaSpell.cs, Source/ACE.Adapter/GDLE/Models/Spell.cs, Source/ACE.Adapter/GDLE/Models/StatMod.cs, Source/ACE.Adapter/GDLE/Models/CreateOffset.cs, Source/ACE.Adapter/GDLE/Models/Region.cs, Source/ACE.Adapter/GDLE/Models/Encounter.cs, Source/ACE.Adapter/GDLE/Models/TerrainData.cs, Source/ACE.Adapter/GDLE/Models/WieldedTreasureTable.cs, Source/ACE.Adapter/GDLE/Models/WieldedTreasure.cs, Source/ACE.Adapter/GDLE/Models/WorldSpawns.cs, Source/ACE.Adapter/GDLE/Models/Origin.cs, Source/ACE.Adapter/GDLE/Models/Angles.cs
//! The GDLE models that only `GDLELoader` reads (events, spells, regions, terrain, wielded
//! treasure, world spawns), with the property names, types and initial values of the C# classes.
//! The documents `import-json` also reads (landblocks, quests, recipes) are in
//! [`crate::import::json::models`]. Each `read` is `JsonSerializer.Deserialize` for its class:
//! C#'s initializers, then every known property in document order (a repeated one read again).

use crate::import::json::models::{class, class_list, Landblock, Position};
use crate::import::json::value::{bool_, f32_, f64_, i32_, list, opt, string, u16_, u32_, Json, R};

// ACE: Event (GDLE)
#[derive(Debug, Clone, Default)]
pub struct Event {
    pub key: Option<String>,
    pub value: Option<EventValue>,
}

impl Event {
    pub(crate) fn read(j: &Json, what: &str) -> R<Option<Self>> {
        class(j, what, |t: &mut Self, k, v| {
            match k {
                "key" => t.key = string(v, k)?,
                "value" => t.value = EventValue::read(v, k)?,
                _ => {}
            }
            Ok(())
        })
    }
}

// ACE: EventValue
#[derive(Debug, Clone, Default)]
pub struct EventValue {
    pub start_time: i32,
    pub end_time: i32,
    pub event_state: i32,
}

impl EventValue {
    fn read(j: &Json, what: &str) -> R<Option<Self>> {
        class(j, what, |t: &mut Self, k, v| {
            match k {
                "startTime" => t.start_time = i32_(v, k)?,
                "endTime" => t.end_time = i32_(v, k)?,
                "eventState" => t.event_state = i32_(v, k)?,
                _ => {}
            }
            Ok(())
        })
    }
}

// ACE: Spells
#[derive(Debug, Clone, Default)]
pub struct Spells {
    pub table: Option<SpellTable>,
}

impl Spells {
    pub(crate) fn read(j: &Json, what: &str) -> R<Option<Self>> {
        class(j, what, |t: &mut Self, k, v| {
            if k == "table" {
                t.table = SpellTable::read(v, k)?;
            }
            Ok(())
        })
    }
}

// ACE: SpellTable
#[derive(Debug, Clone, Default)]
pub struct SpellTable {
    pub spell_base_hash: Option<Vec<Option<SpellBaseHash>>>,
}

impl SpellTable {
    fn read(j: &Json, what: &str) -> R<Option<Self>> {
        class(j, what, |t: &mut Self, k, v| {
            if k == "spellBaseHash" {
                t.spell_base_hash = class_list(v, k, SpellBaseHash::read)?;
            }
            Ok(())
        })
    }
}

// ACE: SpellBaseHash
#[derive(Debug, Clone, Default)]
pub struct SpellBaseHash {
    pub key: u32,
    pub value: Option<SpellValue>,
}

impl SpellBaseHash {
    fn read(j: &Json, what: &str) -> R<Option<Self>> {
        class(j, what, |t: &mut Self, k, v| {
            match k {
                "key" => t.key = u32_(v, k)?,
                "value" => t.value = SpellValue::read(v, k)?,
                _ => {}
            }
            Ok(())
        })
    }
}

// ACE: SpellValue
#[derive(Debug, Clone, Default)]
pub struct SpellValue {
    pub base_mana: u32,
    pub base_range_constant: f64,
    pub base_range_mod: f64,
    pub bitfield: u32,
    pub caster_effect: u32,
    pub category: u32,
    pub component_loss: f64,
    pub desc: Option<String>,
    pub display_order: u32,
    pub fizzle_effect: u32,
    pub formula: Option<Vec<i32>>,
    pub formula_version: u32,
    pub icon_id: u32,
    pub mana_mod: u32,
    pub meta_spell: Option<MetaSpell>,
    pub name: Option<String>,
    pub non_component_target_type: u32,
    pub power: u32,
    pub recovery_amount: u32,
    pub recovery_interval: u32,
    pub school: i32,
    pub spell_economy_mod: u32,
    pub target_effect: u32,
}

impl SpellValue {
    fn read(j: &Json, what: &str) -> R<Option<Self>> {
        class(j, what, |t: &mut Self, k, v| {
            match k {
                "base_mana" => t.base_mana = u32_(v, k)?,
                "base_range_constant" => t.base_range_constant = f64_(v, k)?,
                "base_range_mod" => t.base_range_mod = f64_(v, k)?,
                "bitfield" => t.bitfield = u32_(v, k)?,
                "caster_effect" => t.caster_effect = u32_(v, k)?,
                "category" => t.category = u32_(v, k)?,
                "component_loss" => t.component_loss = f64_(v, k)?,
                "desc" => t.desc = string(v, k)?,
                "display_order" => t.display_order = u32_(v, k)?,
                "fizzle_effect" => t.fizzle_effect = u32_(v, k)?,
                // List<int>: a null element fails (int is not nullable).
                "formula" => t.formula = list(v, k, i32_)?,
                "formula_version" => t.formula_version = u32_(v, k)?,
                "iconID" => t.icon_id = u32_(v, k)?,
                "mana_mod" => t.mana_mod = u32_(v, k)?,
                "meta_spell" => t.meta_spell = MetaSpell::read(v, k)?,
                "name" => t.name = string(v, k)?,
                "non_component_target_type" => t.non_component_target_type = u32_(v, k)?,
                "power" => t.power = u32_(v, k)?,
                "recovery_amount" => t.recovery_amount = u32_(v, k)?,
                "recovery_interval" => t.recovery_interval = u32_(v, k)?,
                "school" => t.school = i32_(v, k)?,
                "spell_economy_mod" => t.spell_economy_mod = u32_(v, k)?,
                "target_effect" => t.target_effect = u32_(v, k)?,
                _ => {}
            }
            Ok(())
        })
    }
}

// ACE: MetaSpell
#[derive(Debug, Clone, Default)]
pub struct MetaSpell {
    pub r#type: i32,
    pub spell: Option<Spell>,
}

impl MetaSpell {
    fn read(j: &Json, what: &str) -> R<Option<Self>> {
        class(j, what, |t: &mut Self, k, v| {
            match k {
                "sp_type" => t.r#type = i32_(v, k)?,
                "spell" => t.spell = Spell::read(v, k)?,
                _ => {}
            }
            Ok(())
        })
    }
}

// ACE: Spell (GDLE)
#[derive(Debug, Clone, Default)]
pub struct Spell {
    pub degrade_limit: Option<f64>,
    pub degrade_modifier: Option<u32>,
    pub duration: Option<f64>,
    pub stat_mod: Option<StatMod>,
    pub spell_category: Option<u32>,
    pub spell_id: u32,
    pub boost: Option<i32>,
    pub boost_variance: Option<i32>,
    pub damage_type: Option<i32>,
    pub bitfield: Option<u32>,
    pub dest: Option<i32>,
    pub loss_percent: Option<f64>,
    pub max_boost_allowed: Option<i32>,
    pub proportion: Option<f64>,
    pub source_loss: Option<i32>,
    pub source: Option<i32>,
    pub transfer_cap: Option<i32>,
    pub non_tracking: Option<bool>,
    pub base_intensity: Option<i32>,
    pub create_offset: Option<CreateOffset>,
    pub crit_freq: Option<u32>,
    pub crit_multiplier: Option<u32>,
    pub default_launch_angle: Option<u32>,
    pub dims: Option<CreateOffset>,
    pub elemental_modifier: Option<u32>,
    pub e_type: Option<u32>,
    pub ignore_magic_resist: Option<i32>,
    pub imbued_effect: Option<u32>,
    pub num_projectiles: Option<i32>,
    pub num_projectiles_variance: Option<f64>,
    pub padding: Option<CreateOffset>,
    pub peturbation: Option<CreateOffset>,
    pub slayer_creature_type: Option<i32>,
    pub slayer_damage_bonus: Option<u32>,
    pub spread_angle: Option<u32>,
    pub variance: Option<i32>,
    pub vertical_angle: Option<u32>,
    pub wcid: Option<u32>,
    pub index: Option<i32>,
    pub link: Option<i32>,
    pub portal_lifetime: Option<u32>,
    pub position: Option<Position>,
    pub align: Option<i32>,
    pub max_power: Option<i32>,
    pub min_power: Option<i32>,
    pub number: Option<i32>,
    pub number_variance: Option<f64>,
    pub power_variance: Option<u32>,
    pub school: Option<u32>,
    pub damage_ratio: Option<f64>,
    pub drain_percentage: Option<f64>,
}

impl Spell {
    fn read(j: &Json, what: &str) -> R<Option<Self>> {
        class(j, what, |t: &mut Self, k, v| {
            match k {
                "degrade_limit" => t.degrade_limit = opt(v, k, f64_)?,
                "degrade_modifier" => t.degrade_modifier = opt(v, k, u32_)?,
                "duration" => t.duration = opt(v, k, f64_)?,
                "smod" => t.stat_mod = StatMod::read(v, k)?,
                "spellCategory" => t.spell_category = opt(v, k, u32_)?,
                "spell_id" => t.spell_id = u32_(v, k)?,
                "boost" => t.boost = opt(v, k, i32_)?,
                "boostVariance" => t.boost_variance = opt(v, k, i32_)?,
                "dt" => t.damage_type = opt(v, k, i32_)?,
                "bitfield" => t.bitfield = opt(v, k, u32_)?,
                "dest" => t.dest = opt(v, k, i32_)?,
                "lossPercent" => t.loss_percent = opt(v, k, f64_)?,
                "maxBoostAllowed" => t.max_boost_allowed = opt(v, k, i32_)?,
                "proportion" => t.proportion = opt(v, k, f64_)?,
                "sourceLoss" => t.source_loss = opt(v, k, i32_)?,
                "src" => t.source = opt(v, k, i32_)?,
                "transferCap" => t.transfer_cap = opt(v, k, i32_)?,
                "bNonTracking" => t.non_tracking = opt(v, k, bool_)?,
                "baseIntensity" => t.base_intensity = opt(v, k, i32_)?,
                "createOffset" => t.create_offset = CreateOffset::read(v, k)?,
                "critFreq" => t.crit_freq = opt(v, k, u32_)?,
                "critMultiplier" => t.crit_multiplier = opt(v, k, u32_)?,
                "defaultLaunchAngle" => t.default_launch_angle = opt(v, k, u32_)?,
                "dims" => t.dims = CreateOffset::read(v, k)?,
                "elementalModifier" => t.elemental_modifier = opt(v, k, u32_)?,
                "etype" => t.e_type = opt(v, k, u32_)?,
                "ignoreMagicResist" => t.ignore_magic_resist = opt(v, k, i32_)?,
                "imbuedEffect" => t.imbued_effect = opt(v, k, u32_)?,
                "numProjectiles" => t.num_projectiles = opt(v, k, i32_)?,
                "numProjectilesVariance" => t.num_projectiles_variance = opt(v, k, f64_)?,
                "padding" => t.padding = CreateOffset::read(v, k)?,
                "peturbation" => t.peturbation = CreateOffset::read(v, k)?,
                "slayerCreatureType" => t.slayer_creature_type = opt(v, k, i32_)?,
                "slayerDamageBonus" => t.slayer_damage_bonus = opt(v, k, u32_)?,
                "spreadAngle" => t.spread_angle = opt(v, k, u32_)?,
                "variance" => t.variance = opt(v, k, i32_)?,
                "verticalAngle" => t.vertical_angle = opt(v, k, u32_)?,
                "wcid" => t.wcid = opt(v, k, u32_)?,
                "index" => t.index = opt(v, k, i32_)?,
                "link" => t.link = opt(v, k, i32_)?,
                "portal_lifetime" => t.portal_lifetime = opt(v, k, u32_)?,
                "pos" => t.position = Position::read(v, k)?,
                "align" => t.align = opt(v, k, i32_)?,
                "max_power" => t.max_power = opt(v, k, i32_)?,
                "min_power" => t.min_power = opt(v, k, i32_)?,
                "number" => t.number = opt(v, k, i32_)?,
                "number_variance" => t.number_variance = opt(v, k, f64_)?,
                "power_variance" => t.power_variance = opt(v, k, u32_)?,
                "school" => t.school = opt(v, k, u32_)?,
                "damage_ratio" => t.damage_ratio = opt(v, k, f64_)?,
                "drain_percentage" => t.drain_percentage = opt(v, k, f64_)?,
                _ => {}
            }
            Ok(())
        })
    }
}

// ACE: StatMod
#[derive(Debug, Clone, Default)]
pub struct StatMod {
    pub key: u32,
    pub r#type: u32,
    pub val: f64,
}

impl StatMod {
    fn read(j: &Json, what: &str) -> R<Option<Self>> {
        class(j, what, |t: &mut Self, k, v| {
            match k {
                "key" => t.key = u32_(v, k)?,
                "type" => t.r#type = u32_(v, k)?,
                "val" => t.val = f64_(v, k)?,
                _ => {}
            }
            Ok(())
        })
    }
}

// ACE: CreateOffset
#[derive(Debug, Clone, Default)]
pub struct CreateOffset {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub w: Option<f64>,
}

impl CreateOffset {
    fn read(j: &Json, what: &str) -> R<Option<Self>> {
        class(j, what, |t: &mut Self, k, v| {
            match k {
                "x" => t.x = f64_(v, k)?,
                "y" => t.y = f64_(v, k)?,
                "z" => t.z = f64_(v, k)?,
                "w" => t.w = opt(v, k, f64_)?,
                _ => {}
            }
            Ok(())
        })
    }
}

// ACE: Origin
/// Not read by any loader (no GDLE document holds one); kept for the model set.
#[derive(Debug, Clone, Default)]
pub struct Origin {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

// ACE: Angles
/// Not read by any loader (no GDLE document holds one); kept for the model set.
#[derive(Debug, Clone, Default)]
pub struct Angles {
    pub w: f32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

// ACE: Region
#[derive(Debug, Clone, Default)]
pub struct Region {
    pub encounter_map: Option<Vec<u32>>,
    pub encounters: Option<Vec<Option<Encounter>>>,
    pub table_count: i32,
    pub table_size: i32,
}

impl Region {
    pub(crate) fn read(j: &Json, what: &str) -> R<Option<Self>> {
        class(j, what, |t: &mut Self, k, v| {
            match k {
                "encounterMap" => t.encounter_map = list(v, k, u32_)?,
                // Encounter[]: an array reads like a list.
                "encounters" => t.encounters = class_list(v, k, Encounter::read)?,
                "tableCount" => t.table_count = i32_(v, k)?,
                "tableSize" => t.table_size = i32_(v, k)?,
                _ => {}
            }
            Ok(())
        })
    }
}

// ACE: Encounter (GDLE)
#[derive(Debug, Clone, Default)]
pub struct Encounter {
    pub key: u32,
    pub value: Option<Vec<u32>>,
}

impl Encounter {
    fn read(j: &Json, what: &str) -> R<Option<Self>> {
        class(j, what, |t: &mut Self, k, v| {
            match k {
                "key" => t.key = u32_(v, k)?,
                "value" => t.value = list(v, k, u32_)?,
                _ => {}
            }
            Ok(())
        })
    }
}

// ACE: TerrainData
#[derive(Debug, Clone, Default)]
pub struct TerrainData {
    pub key: u32,
    pub value: Option<Vec<u16>>,
}

impl TerrainData {
    pub(crate) fn read(j: &Json, what: &str) -> R<Option<Self>> {
        class(j, what, |t: &mut Self, k, v| {
            match k {
                "key" => t.key = u32_(v, k)?,
                "value" => t.value = list(v, k, u16_)?,
                _ => {}
            }
            Ok(())
        })
    }
}

// ACE: WieldedTreasureTable
#[derive(Debug, Clone, Default)]
pub struct WieldedTreasureTable {
    pub key: u32,
    pub value: Option<Vec<Option<WieldedTreasure>>>,
}

impl WieldedTreasureTable {
    pub(crate) fn read(j: &Json, what: &str) -> R<Option<Self>> {
        class(j, what, |t: &mut Self, k, v| {
            match k {
                "key" => t.key = u32_(v, k)?,
                "value" => t.value = class_list(v, k, WieldedTreasure::read)?,
                _ => {}
            }
            Ok(())
        })
    }
}

// ACE: WieldedTreasure
#[derive(Debug, Clone, Default)]
pub struct WieldedTreasure {
    pub continues_previous_set: bool,
    pub set_start: bool,
    pub has_sub_set: bool,
    pub probability: f32,
    pub stack_size: i32,
    pub stack_size_variance: f32,
    pub palette_id: u32,
    pub shade: f32,
    pub unknown1: u32,
    pub unknown3: u32,
    pub unknown4: u32,
    pub unknown5: u32,
    pub unknown9: u32,
    pub unknown10: u32,
    pub unknown11: u32,
    pub unknown12: u32,
    pub weenie_class_id: u32,
}

impl WieldedTreasure {
    fn read(j: &Json, what: &str) -> R<Option<Self>> {
        class(j, what, |t: &mut Self, k, v| {
            match k {
                "continuesPreviousSet" => t.continues_previous_set = bool_(v, k)?,
                "setStart" => t.set_start = bool_(v, k)?,
                "hasSubSet" => t.has_sub_set = bool_(v, k)?,
                "probability" => t.probability = f32_(v, k)?,
                "stackSize" => t.stack_size = i32_(v, k)?,
                "stackSizeVariance" => t.stack_size_variance = f32_(v, k)?,
                "paletteId" => t.palette_id = u32_(v, k)?,
                "shade" => t.shade = f32_(v, k)?,
                "unknown1" => t.unknown1 = u32_(v, k)?,
                "unknown3" => t.unknown3 = u32_(v, k)?,
                "unknown4" => t.unknown4 = u32_(v, k)?,
                "unknown5" => t.unknown5 = u32_(v, k)?,
                "unknown9" => t.unknown9 = u32_(v, k)?,
                "unknown10" => t.unknown10 = u32_(v, k)?,
                "unknown11" => t.unknown11 = u32_(v, k)?,
                "unknown12" => t.unknown12 = u32_(v, k)?,
                "weenieClassId" => t.weenie_class_id = u32_(v, k)?,
                _ => {}
            }
            Ok(())
        })
    }
}

// ACE: WorldSpawns
#[derive(Debug, Clone, Default)]
pub struct WorldSpawns {
    pub version: Option<String>,
    pub landblocks: Option<Vec<Option<Landblock>>>,
}

impl WorldSpawns {
    pub(crate) fn read(j: &Json, what: &str) -> R<Option<Self>> {
        class(j, what, |t: &mut Self, k, v| {
            match k {
                "_version" => t.version = string(v, k)?,
                "landblocks" => {
                    t.landblocks = class_list(v, k, |e, _| Landblock::read(e))?;
                }
                _ => {}
            }
            Ok(())
        })
    }
}
