//! Versioned source loot, separate from ACE's three-group treasure representation.
// Rules ported from ac1999 (Adam Plumb), AGPL-3.0: Server/Source/TreasureFactory.cpp.

use crate::error::{ImportError, PackError};
use crate::import::mysqldump::Row;
use crate::models::world::TreasureDeath;
use crate::pack::{Codec, Cursor};
use crate::records::FromRow;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

pub const SOURCE_REVISION: &str = "5fe910e7a3e549d955117920a1bd30b4ee646938";
pub const ALGORITHM: &str = "ac1999-six-groups-v1";
pub const MAX_POLICY_BYTES: usize = 4 * 1024 * 1024;

/// A chance is tested separately for every slot, after drawing the count.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SlotGroup {
    pub chance: u32,
    pub min: u32,
    pub max: u32,
}

/// Fixed order: weapon, armor, item, magic item, consumable, valuable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroupedTreasureDeath {
    pub id: u32,
    pub treasure_type: u32,
    pub tier: u32,
    pub quality: f64,
    pub groups: [SlotGroup; 6],
}

impl GroupedTreasureDeath {
    pub fn validate(&self) -> Result<(), String> {
        ensure((1..=5).contains(&self.tier), "loot tier must be 1..5")?;
        ensure(
            self.quality.is_finite() && (0.0..=1.0).contains(&self.quality),
            "loot quality must be 0..1",
        )?;
        for g in self.groups {
            ensure(
                g.chance <= 100 && g.min <= 100 && g.max <= 100,
                "loot slots exceed reviewed limits",
            )?;
        }
        Ok(())
    }
}

/// Both profiles retain their own semantics and record layout.
#[derive(Debug, Clone)]
pub enum DeathLootProfile {
    Ace(Arc<TreasureDeath>),
    Ac1999(Arc<GroupedTreasureDeath>, Arc<SourceLootPolicy>),
}
impl DeathLootProfile {
    #[must_use]
    pub fn tier(&self) -> i32 {
        match self {
            Self::Ace(r) => r.tier,
            Self::Ac1999(r, _) => r.tier as i32,
        }
    }
    #[must_use]
    pub fn id(&self) -> u32 {
        match self {
            Self::Ace(r) => r.id,
            Self::Ac1999(r, _) => r.id,
        }
    }
}

/// All inputs are compiled into the pack. No runtime file paths or loose JSON reads.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceLootPolicy {
    pub id: u32,
    pub schema: u32,
    pub algorithm: String,
    /// Reviewed server implementation; independent of the selected content revision.
    pub algorithm_revision: String,
    pub revision: String,
    pub files_sha256: BTreeMap<String, String>,
    pub profile: Profile,
    pub tiers: BTreeMap<u32, Tier>,
    pub pools: BTreeMap<String, Vec<Category>>,
    pub mutations: BTreeMap<String, Vec<Band>>,
    pub spells: BTreeMap<String, Vec<SpellFamily>>,
    pub material_values: BTreeMap<String, f64>,
    pub material_colors: Vec<MaterialColors>,
}

pub type MaterialColors = KeyValue<Option<Vec<KeyValue<Option<Vec<PaletteChance>>>>>>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeyValue<T> {
    pub key: u32,
    pub value: T,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaletteChance {
    pub chance: f64,
    pub value: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Profile {
    pub spell_properties: SpellProperties,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SpellProperties {
    pub spell_power: Vec<i32>,
    pub spell_mana: Vec<i32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Tier {
    pub tier_id: u32,
    pub scroll_categories: Vec<String>,
    pub component_categories: Vec<String>,
    pub melee_weapon_base_tier_weights: Vec<i32>,
    pub missile_weapon_base_tier_weights: Vec<i32>,
    pub armor_base_tier_weights: Vec<i32>,
    pub mundane_base_tier_weights: Vec<i32>,
    pub value_factor_min: f64,
    pub value_factor_max: f64,
    pub gem_value_factor_min: f64,
    pub gem_value_factor_max: f64,
    pub max_amount_of_spells: u32,
    pub chance_of_spells: f64,
    pub min_spell_level: u32,
    pub max_spell_level: u32,
    pub preferred_spell_level: u32,
    pub preferred_spell_level_strength: f64,
    pub materials_ceramic: Vec<String>,
    pub materials_cloth: Vec<String>,
    pub materials_gem: Vec<String>,
    pub materials_leather: Vec<String>,
    pub materials_metal: Vec<String>,
    pub materials_stone: Vec<String>,
    pub materials_wood: Vec<String>,
}
impl Tier {
    #[must_use]
    pub fn materials(&self, bucket: &str) -> &[String] {
        match bucket {
            "ceramic" => &self.materials_ceramic,
            "cloth" => &self.materials_cloth,
            "gem" => &self.materials_gem,
            "leather" => &self.materials_leather,
            "metal" => &self.materials_metal,
            "stone" => &self.materials_stone,
            "wood" => &self.materials_wood,
            _ => &[],
        }
    }
}

fn one_i() -> i32 {
    1
}
fn one() -> f64 {
    1.0
}
fn unset() -> f64 {
    -1.0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Category {
    pub category: String,
    #[serde(default)]
    pub base_tier: u32,
    #[serde(default = "one_i")]
    pub weight: i32,
    #[serde(default)]
    pub possible_materials: Vec<String>,
    #[serde(default)]
    pub mutation_bands: Vec<Band>,
    pub entries: Vec<Entry>,
    #[serde(default)]
    pub heritage: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Entry {
    pub name: String,
    pub wcid: u32,
    #[serde(default)]
    pub max_amount: u32,
    #[serde(default)]
    pub stack_size: u32,
    #[serde(default)]
    pub possible_materials: Vec<String>,
    #[serde(default)]
    pub elemental_variants: Vec<u32>,
    #[serde(default)]
    pub material: u32,
    #[serde(default)]
    pub base_tier: u32,
    #[serde(default)]
    pub tier_weights: Vec<i32>,
    #[serde(default)]
    pub cast_spell_pool: String,
    #[serde(default = "unset")]
    pub allegiance_rank_chance: f64,
    #[serde(default = "one")]
    pub mana_capacity_mult: f64,
    #[serde(default)]
    pub weapon_skill: Option<String>,
    #[serde(default)]
    pub heritage: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Band {
    #[serde(default)]
    pub min_attack_skill_bonus: f64,
    #[serde(default)]
    pub max_attack_skill_bonus: f64,
    #[serde(default)]
    pub min_melee_defense_bonus: f64,
    #[serde(default)]
    pub max_melee_defense_bonus: f64,
    #[serde(default)]
    pub attack_skill_bonus_chance: f64,
    #[serde(default)]
    pub melee_defense_bonus_chance: f64,
    #[serde(default)]
    pub elemental_chance: f64,
    #[serde(default)]
    pub min_damage_bonus: i32,
    #[serde(default)]
    pub max_damage_bonus: i32,
    #[serde(default)]
    pub slow_speed_mod: f64,
    #[serde(default)]
    pub fast_speed_mod: f64,
    #[serde(default)]
    pub min_damage_mod_bonus: f64,
    #[serde(default)]
    pub max_damage_mod_bonus: f64,
    #[serde(default = "one")]
    pub min_armor_mult: f64,
    #[serde(default = "one")]
    pub max_armor_mult: f64,
    #[serde(default)]
    pub min_burden_mod: f64,
    #[serde(default)]
    pub max_burden_mod: f64,
    #[serde(default = "one")]
    pub min_shield_armor_mult: f64,
    #[serde(default = "one")]
    pub max_shield_armor_mult: f64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SpellFamily {
    pub spell_name: String,
    #[serde(default)]
    pub spell_category: SpellCategory,
    pub spells: Vec<u32>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SpellCategory {
    #[default]
    NoCategory,
    MagicSkillMastery,
    WeaponSkillMastery,
}

pub const POOLS: &[&str] = &[
    "armor",
    "casters",
    "clothing",
    "components",
    "gems",
    "jewelry",
    "melee-weapons",
    "misc",
    "missile-weapons",
    "scrolls",
];
pub const SPELL_POOLS: &[&str] = &[
    "armor-feet",
    "armor-hands",
    "armor-head",
    "armor-item",
    "armor-lower",
    "armor-upper",
    "caster-attack",
    "caster-support",
    "caster",
    "clothing-feet",
    "clothing-hands",
    "clothing-head",
    "clothing-item",
    "gem",
    "jewelry",
    "melee",
    "missile",
    "shield",
];

fn ensure(ok: bool, message: &str) -> Result<(), String> {
    if ok {
        Ok(())
    } else {
        Err(message.into())
    }
}
fn range(lo: f64, hi: f64, max: f64) -> Result<(), String> {
    ensure(
        lo.is_finite() && hi.is_finite() && 0.0 <= lo && lo <= hi && hi <= max,
        "invalid loot numeric range",
    )
}
fn weights(v: &[i32]) -> Result<(), String> {
    ensure(
        v.len() <= 5 && v.iter().all(|&x| (0..=10000).contains(&x)),
        "invalid loot weights",
    )
}
fn names(v: &[String]) -> Result<(), String> {
    ensure(
        v.len() <= 128 && v.iter().all(|s| s.len() <= 128),
        "loot name list too large",
    )
}

/// Material names follow the source's exact spelling, including its capitalized RedJade.
#[must_use]
pub fn material_id(s: &str) -> Option<u32> {
    use empyrean_entity::enums::MaterialType;
    if s == "none" {
        return Some(0);
    }
    MaterialType::NAMES
        .iter()
        .zip(MaterialType::ALL)
        .find_map(|(n, v)| {
            let mut name = n.to_string();
            if name != "RedJade" {
                name[0..1].make_ascii_lowercase();
            }
            (s == name).then_some(v.0)
        })
}
#[must_use]
pub fn heritage_id(s: &str) -> Option<i32> {
    match s {
        "Aluvian" => Some(1),
        "Gharu'ndim" => Some(2),
        "Sho" => Some(3),
        _ => None,
    }
}
#[must_use]
pub fn skill_id(s: &str) -> Option<i32> {
    use empyrean_entity::enums::Skill;
    if s == "None" {
        return Some(0);
    }
    Skill::NAMES
        .iter()
        .zip(Skill::ALL)
        .find_map(|(n, v)| (*n == s).then_some(v.0))
}

impl SourceLootPolicy {
    pub fn from_json(bytes: &[u8]) -> Result<Self, String> {
        ensure(
            bytes.len() <= MAX_POLICY_BYTES,
            "loot policy exceeds size limit",
        )?;
        let result: Self = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        result.validate()?;
        Ok(result)
    }
    pub fn validate(&self) -> Result<(), String> {
        ensure(
            self.id == 1
                && self.schema == 1
                && self.algorithm == ALGORITHM
                && self.algorithm_revision == SOURCE_REVISION,
            "unreviewed source loot policy",
        )?;
        ensure(
            self.revision.len() == 40
                && self.revision.bytes().all(|b| b.is_ascii_hexdigit())
                && self.files_sha256.len() == 39
                && self.files_sha256.iter().all(|(p, h)| {
                    p.len() <= 128
                        && !p.contains("..")
                        && h.len() == 64
                        && h.bytes().all(|b| b.is_ascii_hexdigit())
                }),
            "invalid loot provenance",
        )?;
        ensure(
            self.tiers.keys().copied().eq(1..=5),
            "loot requires tiers 1..5",
        )?;
        ensure(
            self.pools.len() == POOLS.len() && POOLS.iter().all(|p| self.pools.contains_key(*p)),
            "missing or unknown loot pool",
        )?;
        ensure(
            self.spells.len() == SPELL_POOLS.len()
                && SPELL_POOLS.iter().all(|p| self.spells.contains_key(*p)),
            "missing or unknown spell pool",
        )?;
        let props = &self.profile.spell_properties;
        ensure(
            props.spell_power.len() == 6
                && props.spell_mana.len() == 6
                && props
                    .spell_power
                    .iter()
                    .chain(&props.spell_mana)
                    .all(|&v| (1..=10000).contains(&v)),
            "invalid spell properties",
        )?;
        for (id, t) in &self.tiers {
            ensure(t.tier_id == *id, "tier key mismatch")?;
            for w in [
                &t.melee_weapon_base_tier_weights,
                &t.missile_weapon_base_tier_weights,
                &t.armor_base_tier_weights,
                &t.mundane_base_tier_weights,
            ] {
                weights(w)?;
            }
            range(t.value_factor_min, t.value_factor_max, 10.0)?;
            range(t.gem_value_factor_min, t.gem_value_factor_max, 10.0)?;
            range(t.chance_of_spells, t.chance_of_spells, 1.0)?;
            ensure(
                t.min_spell_level >= 1
                    && t.min_spell_level <= t.max_spell_level
                    && t.max_spell_level <= 6
                    && (1..=6).contains(&t.preferred_spell_level)
                    && t.max_amount_of_spells <= 6,
                "invalid spell levels/count",
            )?;
            range(
                t.preferred_spell_level_strength,
                t.preferred_spell_level_strength,
                10.0,
            )?;
            for (list, pool) in [
                (&t.scroll_categories, "scrolls"),
                (&t.component_categories, "components"),
            ] {
                names(list)?;
                ensure(
                    !list.is_empty()
                        && list
                            .iter()
                            .all(|n| self.pools[pool].iter().any(|c| c.category == *n)),
                    "unknown tier category",
                )?;
            }
            for bucket in [
                "ceramic", "cloth", "gem", "leather", "metal", "stone", "wood",
            ] {
                let v = t.materials(bucket);
                names(v)?;
                ensure(
                    v.iter().all(|s| material_id(s).is_some()),
                    "unknown tier material",
                )?;
            }
        }
        for cats in self.pools.values() {
            ensure(
                !cats.is_empty() && cats.len() <= 32,
                "invalid category count",
            )?;
            let mut seen = BTreeSet::new();
            for c in cats {
                ensure(
                    seen.insert(&c.category)
                        && c.category.len() <= 128
                        && !c.entries.is_empty()
                        && c.entries.len() <= 2048
                        && c.base_tier <= 5
                        && (0..=10000).contains(&c.weight),
                    "invalid loot category",
                )?;
                validate_material_buckets(&c.possible_materials)?;
                ensure(
                    c.heritage
                        .as_deref()
                        .is_none_or(|h| heritage_id(h).is_some()),
                    "unknown category heritage",
                )?;
                validate_bands(&c.mutation_bands)?;
                for e in &c.entries {
                    ensure(
                        e.wcid > 0
                            && e.name.len() <= 256
                            && e.base_tier <= 5
                            && e.max_amount <= 1000000
                            && e.stack_size <= 1000000
                            && e.elemental_variants.len() <= 32
                            && e.elemental_variants.iter().all(|&v| v > 0)
                            && e.material <= 77,
                        "invalid loot entry",
                    )?;
                    weights(&e.tier_weights)?;
                    validate_material_buckets(&e.possible_materials)?;
                    ensure(
                        matches!(e.cast_spell_pool.as_str(), "" | "attack" | "support"),
                        "unknown cast spell pool",
                    )?;
                    ensure(
                        e.allegiance_rank_chance == -1.0
                            || (e.allegiance_rank_chance.is_finite()
                                && (0.0..=1.0).contains(&e.allegiance_rank_chance)),
                        "invalid allegiance chance",
                    )?;
                    range(e.mana_capacity_mult, e.mana_capacity_mult, 100.0)?;
                    ensure(
                        e.heritage
                            .as_deref()
                            .is_none_or(|h| heritage_id(h).is_some())
                            && e.weapon_skill
                                .as_deref()
                                .is_none_or(|s| skill_id(s).is_some()),
                        "unknown entry heritage/skill",
                    )?;
                }
            }
        }
        ensure(
            self.mutations.len() <= 32 && self.mutations.contains_key("armor"),
            "invalid mutation families",
        )?;
        for (name, bands) in &self.mutations {
            ensure(name.len() <= 128, "mutation name too long")?;
            validate_bands(bands)?;
        }
        let mut families = BTreeMap::new();
        for list in self.spells.values() {
            ensure(
                !list.is_empty() && list.len() <= 128,
                "invalid spell family count",
            )?;
            for f in list {
                ensure(
                    f.spell_name.len() <= 128 && f.spells.len() == 6,
                    "invalid spell family",
                )?;
                if let Some(previous) = families.insert(&f.spell_name, f) {
                    ensure(previous == f, "conflicting spell family")?;
                }
            }
        }
        ensure(self.material_values.len() <= 78, "material value count")?;
        for (n, v) in &self.material_values {
            ensure(material_id(n).is_some(), "unknown material value")?;
            range(*v, *v, 100.0)?;
        }
        ensure(self.material_colors.len() <= 256, "material color count")?;
        let mut materials = BTreeSet::new();
        for m in &self.material_colors {
            ensure(
                m.key <= 255 && materials.insert(m.key),
                "invalid material color key",
            )?;
            if let Some(codes) = &m.value {
                ensure(codes.len() <= 256, "color code count")?;
                let mut seen = BTreeSet::new();
                for c in codes {
                    ensure(c.key <= 255 && seen.insert(c.key), "invalid color code")?;
                    if let Some(values) = &c.value {
                        ensure(values.len() <= 256, "palette count")?;
                        for v in values {
                            range(v.chance, v.chance, 1.0)?;
                            ensure(v.value <= 10000, "palette id")?;
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
fn validate_material_buckets(v: &[String]) -> Result<(), String> {
    names(v)?;
    ensure(
        v.iter().all(|s| {
            matches!(
                s.as_str(),
                "ceramic" | "cloth" | "gem" | "leather" | "metal" | "stone" | "wood"
            )
        }),
        "unknown material bucket",
    )
}
fn validate_bands(bands: &[Band]) -> Result<(), String> {
    ensure(bands.len() <= 5, "too many mutation bands")?;
    for b in bands {
        for (lo, hi) in [
            (b.min_attack_skill_bonus, b.max_attack_skill_bonus),
            (b.min_melee_defense_bonus, b.max_melee_defense_bonus),
            (b.min_damage_mod_bonus, b.max_damage_mod_bonus),
            (b.fast_speed_mod, b.slow_speed_mod),
            (b.min_armor_mult, b.max_armor_mult),
            (b.min_shield_armor_mult, b.max_shield_armor_mult),
            (b.min_burden_mod, b.max_burden_mod),
        ] {
            range(lo, hi, 100.0)?;
        }
        for p in [
            b.attack_skill_bonus_chance,
            b.melee_defense_bonus_chance,
            b.elemental_chance,
        ] {
            range(p, p, 1.0)?;
        }
        ensure(
            0 <= b.min_damage_bonus
                && b.min_damage_bonus <= b.max_damage_bonus
                && b.max_damage_bonus <= 10000,
            "invalid damage range",
        )?;
    }
    Ok(())
}

macro_rules! json_record {
    ($ty:ty, $table:literal, $max:expr, $key:ident) => {
        impl Codec for $ty {
            fn put(&self, w: &mut Vec<u8>) {
                serde_json::to_string(self)
                    .expect("validated loot JSON")
                    .put(w);
            }
            fn get(c: &mut Cursor<'_>) -> Result<Self, PackError> {
                let n = c.u32()? as usize;
                if n > $max {
                    return Err(PackError::InvalidLoot("record size limit".into()));
                }
                let v: Self = serde_json::from_slice(c.bytes(n)?)
                    .map_err(|e| PackError::InvalidLoot(e.to_string()))?;
                v.validate().map_err(PackError::InvalidLoot)?;
                Ok(v)
            }
        }
        impl FromRow for $ty {
            const TABLE: &'static str = $table;
            const COLUMNS: &'static [&'static str] = &["id", "profile"];
            fn from_row(row: &Row<'_>) -> Result<Self, ImportError> {
                let text: String = row.get("profile")?;
                if text.len() > $max {
                    return Err(ImportError::Refused(vec!["loot record size limit".into()]));
                }
                let v: Self = serde_json::from_str(&text)
                    .map_err(|e| ImportError::Refused(vec![e.to_string()]))?;
                v.validate().map_err(|e| ImportError::Refused(vec![e]))?;
                if v.$key != row.get::<u32>("id")? {
                    return Err(ImportError::Refused(
                        vec!["loot record key mismatch".into()],
                    ));
                }
                Ok(v)
            }
        }
    };
}
json_record!(
    GroupedTreasureDeath,
    "empyrean_grouped_treasure",
    4096,
    treasure_type
);
json_record!(
    SourceLootPolicy,
    "empyrean_loot_policy",
    MAX_POLICY_BYTES,
    id
);
