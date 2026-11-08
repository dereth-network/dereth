//! A template's design values by property name, read along its chain of entities (a derived
//! template's own value wins over its parent's), and the gameplay numbers the server takes from
//! them.

use std::collections::BTreeMap;

use auberean_assets::entity::Entity;
use auberean_assets::property::PropertyValue;

use crate::{entity_kind, ENTITY_TOP};

/// One property's value, as the server reads it.
#[derive(Debug, Clone, PartialEq)]
pub enum Prop {
    Bool(bool),
    Int(i64),
    Float(f32),
    Text(String),
    /// An enumeration's value, or a data id.
    Word(u32),
    /// A string reference: string id, table.
    Str(u32, u32),
    /// A place: a cell and a point in its landblock.
    Place(u32, [f32; 3]),
}

/// A template's properties by name.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Props {
    pub values: BTreeMap<String, Prop>,
}

const CHAIN_LIMIT: usize = 16;

/// The properties of what an item is called, and the qualities that say the same.
const NAME_QUALITIES: [(&str, u32); 2] = [("Name", 0x10B), ("Description", 0x102)];

/// The properties of where an item goes and what it is held as, and the qualities that say the
/// same.
const PLACE_QUALITIES: [(&str, u32); 9] = [
    ("Item_ValidInventoryLocations", 0x113),
    ("Item_PreferredInventoryLocation", 0x11A),
    ("Item_PrecludedInventoryLocations", 0x11B),
    ("Item_PrimaryParentingLocation", 0x114),
    ("Item_SecondaryParentingLocation", 0x115),
    ("Item_ImplementType", 0xFA0),
    ("Weapon_SingleWeaponStance", 0x25A),
    ("Weapon_ShieldStance", 0x25B),
    ("Weapon_TwoWeaponStance", 0x25C),
];

/// The property of a least level, and the mark that only its class says it.
const LEAST_LEVEL: &str = "Usage_MinLevel";
const LEAST_LEVEL_OF_CLASS: &str = "Usage_MinLevel_OfClass";

impl Props {
    /// The properties along a template's chain: the template's own, then its parent's that it
    /// does not set, up to its class.
    #[must_use]
    pub fn of(
        template: u32,
        entity: &dyn Fn(u32) -> Option<Entity>,
        name: &dyn Fn(u32) -> Option<String>,
    ) -> Self {
        let mut out = Self::default();
        let mut did = template;
        for _ in 0..CHAIN_LIMIT {
            let Some(e) = entity(did) else { break };
            for p in e.properties.iter().flat_map(|c| c.iter()) {
                let Some(n) = name(p.id) else { continue };
                if out.values.contains_key(&n) {
                    continue;
                }
                let v = match &p.value {
                    PropertyValue::Bool(b) => Prop::Bool(*b),
                    PropertyValue::Integer(i) => Prop::Int(i64::from(*i)),
                    PropertyValue::LongInteger(i) => Prop::Int(*i),
                    PropertyValue::Float(f) => Prop::Float(*f),
                    PropertyValue::String(s) => Prop::Text(s.clone()),
                    PropertyValue::Enum(v) | PropertyValue::Package(v) => Prop::Word(*v),
                    PropertyValue::DataFile(d) => Prop::Word(d.0),
                    PropertyValue::StringInfo { info, .. } => Prop::Str(info.token, info.table.0),
                    PropertyValue::Position(p) => Prop::Place(
                        p.cell.0,
                        [p.frame.origin.x, p.frame.origin.y, p.frame.origin.z],
                    ),
                    _ => continue,
                };
                out.values.insert(n, v);
            }
            match (e.kind, e.object) {
                (Some(entity_kind::DERIVED), Some(o)) => did = o,
                (Some(entity_kind::CLASS), Some(o)) if o >> 24 == ENTITY_TOP => did = o,
                _ => break,
            }
        }
        out
    }

    /// The properties with where an item goes, what it is held as and what it is called taken from
    /// its qualities record where its own properties do not say: some item templates (Initiate
    /// Moccasins among them) keep those only there.
    #[must_use]
    pub fn with_places_of(mut self, q: Option<&auberean_assets::script::Qualities>) -> Self {
        let Some(q) = q else { return self };
        for (name, quality) in PLACE_QUALITIES {
            if !self.values.contains_key(name) {
                if let Some((_, v)) = q.ints.iter().find(|(k, _)| *k == quality) {
                    self.values
                        .insert(name.to_string(), Prop::Int(i64::from(*v)));
                }
            }
        }
        for (name, quality) in NAME_QUALITIES {
            if !self.values.contains_key(name) {
                if let Some((_, v)) = q.string_infos.iter().find(|(k, _)| *k == quality) {
                    self.values
                        .insert(name.to_string(), Prop::Str(v.token, v.table.0));
                }
            }
        }
        self
    }

    /// The properties with the worn appearances of template `template` taken from
    /// [`crate::worn_table::WORN_IN_CODE`] where its own properties name none.
    #[must_use]
    pub fn with_worn_of(mut self, template: u32) -> Self {
        if self.values.contains_key("Item_WornAppearanceFile1") {
            return self;
        }
        let rows = crate::worn_table::WORN_IN_CODE
            .iter()
            .filter(|(t, ..)| *t == template);
        for (n, (_, race, sex, file)) in rows.enumerate() {
            let n = n + 1;
            for (name, v) in [("File", *file), ("Race", *race), ("Sex", *sex)] {
                self.values
                    .insert(format!("Item_WornAppearance{name}{n}"), Prop::Word(v));
            }
        }
        self
    }

    /// With the quest that keeps an item from being taken, for a template whose class names it in code.
    #[must_use]
    pub fn with_take_gate_of(mut self, template: u32) -> Self {
        if let Some((_, quest, status)) = crate::take_gates::TAKE_GATES
            .iter()
            .find(|(t, ..)| *t == template)
        {
            self.values
                .insert("Usage_RequiredQuest_ForTaking".into(), Prop::Word(*quest));
            self.values.insert(
                "Usage_RequiredQuestStatus_ForTaking".into(),
                Prop::Word(*status),
            );
        }
        self
    }

    /// The script package a template's chain of entities ends in: its class.
    #[must_use]
    pub fn class_of(template: u32, entity: &dyn Fn(u32) -> Option<Entity>) -> Option<u32> {
        let mut did = template;
        for _ in 0..CHAIN_LIMIT {
            let e = entity(did)?;
            match (e.kind, e.object) {
                (Some(entity_kind::DERIVED), Some(o)) => did = o,
                (Some(entity_kind::CLASS), Some(o)) if o >> 24 == ENTITY_TOP => did = o,
                (Some(entity_kind::CLASS), Some(o)) => return Some(o),
                _ => return None,
            }
        }
        None
    }

    /// With what a template's class says in code, where its own properties and its chain say nothing.
    #[must_use]
    pub fn with_class(mut self, said: &[(&str, Prop)]) -> Self {
        for (name, v) in said {
            if !self.values.contains_key(*name) {
                self.values.insert((*name).to_string(), v.clone());
                if *name == LEAST_LEVEL {
                    self.values
                        .insert(LEAST_LEVEL_OF_CLASS.to_string(), Prop::Bool(true));
                }
            }
        }
        self
    }

    /// Does the template itself set its least level, not only its class? A class's is a default a
    /// drop overrides (2017: 1,882 of 1,893 dropped pieces of such templates showed their own).
    #[must_use]
    pub fn sets_least_level(&self) -> bool {
        self.int(LEAST_LEVEL).is_some() && !self.flag(LEAST_LEVEL_OF_CLASS)
    }

    /// Set a value (for content made by hand).
    pub fn set(&mut self, name: &str, v: Prop) -> &mut Self {
        self.values.insert(name.to_string(), v);
        self
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Prop> {
        self.values.get(name)
    }

    /// A whole number (an integer, or a word).
    #[must_use]
    pub fn int(&self, name: &str) -> Option<i64> {
        match self.get(name)? {
            Prop::Int(i) => Some(*i),
            Prop::Word(w) => Some(i64::from(*w)),
            Prop::Bool(b) => Some(i64::from(*b)),
            _ => None,
        }
    }

    /// A number (a float, or a whole number).
    #[must_use]
    pub fn float(&self, name: &str) -> Option<f32> {
        match self.get(name)? {
            Prop::Float(f) => Some(*f),
            #[allow(clippy::cast_precision_loss)]
            Prop::Int(i) => Some(*i as f32),
            _ => None,
        }
    }

    /// A truth value (absent is false).
    #[must_use]
    pub fn flag(&self, name: &str) -> bool {
        matches!(self.get(name), Some(Prop::Bool(true)))
    }

    /// A word: an enumeration's value or a data id.
    #[must_use]
    pub fn word(&self, name: &str) -> Option<u32> {
        match self.get(name)? {
            Prop::Word(w) => Some(*w),
            Prop::Int(i) => u32::try_from(*i).ok(),
            _ => None,
        }
    }

    /// A text.
    #[must_use]
    pub fn text(&self, name: &str) -> Option<&str> {
        match self.get(name)? {
            Prop::Text(t) => Some(t),
            _ => None,
        }
    }

    /// A place: its cell and its point in the cell's landblock.
    #[must_use]
    pub fn place(&self, name: &str) -> Option<(u32, [f32; 3])> {
        match self.get(name)? {
            Prop::Place(cell, at) => Some((*cell, *at)),
            _ => None,
        }
    }

    /// The properties as text: `name=value` pairs, `;` between (values: `b:` truth, `i:` whole,
    /// `f:` float, `w:` word in hex, `s:` string:table in hex, `p:` cell in hex:x:y:z, `t:` text).
    #[must_use]
    pub fn to_text(&self) -> String {
        self.values
            .iter()
            .map(|(k, v)| {
                let v = match v {
                    Prop::Bool(b) => format!("b:{}", u8::from(*b)),
                    Prop::Int(i) => format!("i:{i}"),
                    Prop::Float(f) => format!("f:{f}"),
                    Prop::Word(w) => format!("w:{w:X}"),
                    Prop::Str(s, t) => format!("s:{s:X}:{t:X}"),
                    Prop::Place(c, [x, y, z]) => format!("p:{c:X}:{x}:{y}:{z}"),
                    Prop::Text(t) => format!("t:{}", t.replace([';', '\t', '\n', '='], " ")),
                };
                format!("{k}={v}")
            })
            .collect::<Vec<_>>()
            .join(";")
    }

    /// Read the properties' text.
    ///
    /// # Errors
    /// A pair that does not read.
    pub fn from_text(s: &str) -> Result<Self, String> {
        let mut out = Self::default();
        for pair in s.split(';').filter(|p| !p.is_empty()) {
            let (k, v) = pair
                .split_once('=')
                .ok_or_else(|| format!("a property is name=value, not {pair}"))?;
            let (kind, v) = v
                .split_once(':')
                .ok_or_else(|| format!("a value is kind:value, not {v}"))?;
            let hex = |s: &str| u32::from_str_radix(s, 16).map_err(|e| format!("{s}: {e}"));
            let p = match kind {
                "b" => Prop::Bool(v == "1"),
                "i" => Prop::Int(v.parse().map_err(|e| format!("{v}: {e}"))?),
                "f" => Prop::Float(v.parse().map_err(|e| format!("{v}: {e}"))?),
                "w" => Prop::Word(hex(v)?),
                "s" => {
                    let (a, b) = v
                        .split_once(':')
                        .ok_or_else(|| format!("a string is id:table, not {v}"))?;
                    Prop::Str(hex(a)?, hex(b)?)
                }
                "p" => {
                    let parts: Vec<&str> = v.split(':').collect();
                    let [c, x, y, z] = parts[..] else {
                        return Err(format!("a place is cell:x:y:z, not {v}"));
                    };
                    let f = |s: &str| s.parse::<f32>().map_err(|e| format!("{s}: {e}"));
                    Prop::Place(hex(c)?, [f(x)?, f(y)?, f(z)?])
                }
                "t" => Prop::Text(v.to_string()),
                other => return Err(format!("a value of kind {other}")),
            };
            out.values.insert(k.to_string(), p);
        }
        Ok(out)
    }
}

/// What a creature template says about fighting, dying and behaving.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CreatureStats {
    pub level: i32,
    /// How it moves at rest: its movement type, wandering, homesick and perception radii (its
    /// template's, a camp's over them).
    pub movement: crate::movement::Movement,
    pub max_health: i32,
    pub max_vigor: i32,
    /// The experience its death is worth before the killer's level is weighed.
    pub death_xp: i64,
    /// The focus its death is worth to a hero, whatever the killer's level.
    pub death_focus: i32,
    /// Its natural damage and the share either side of it a hit may vary by.
    pub natural_damage: f32,
    pub natural_variance: f32,
    pub armor: i32,
    /// Health and vigor regained a second at rest.
    pub health_regen: f32,
    pub vigor_regen: f32,
    /// Whether it attacks a player it notices (else only one that attacks it).
    pub attacks_first: bool,
    /// Its friend table (who it thinks of as its own): creatures of one table come to each other's
    /// fights.
    pub friend_table: Option<u32>,
    /// Whether it ever fights or moves: a creature friendly to players, or a stationary one (a
    /// practice target), does neither.
    pub fights: bool,
    /// Its scale (its running speed goes with it).
    pub scale: f32,
    /// Its skills and their levels, in the template's order.
    pub skills: Vec<(u32, i64)>,
    /// The skills among them that are its basic attacks (the rest of its attacks are specials).
    pub basic: Vec<u32>,
    /// Its death-loot profile (a value of the generator-profile names).
    pub loot_profile: Option<u32>,
    pub loot_min: Option<i64>,
    pub loot_max: Option<i64>,
    /// The quality its loot is made at, when its template sets one (in tenths of a level).
    pub loot_quality: Option<i64>,
    /// Fixed drops: the item template and the chance of it.
    pub trophies: Vec<(u32, f32)>,
    /// The effects its death applies to its killer, by name.
    pub death_effects: Vec<String>,
    /// The weapon it wields (a template's name), how many of it, and the item it holds beside it
    /// (a shield, a second weapon).
    pub weapon: Option<String>,
    pub weapons_held: i64,
    pub beside: Option<String>,
    /// Whether it wields two weapons when it holds two, and a weapon with a shield when it holds
    /// one.
    pub dual_wields: bool,
    pub weapon_and_shield: bool,
    /// Whether it wields implements at all (without, it fights as a monster does).
    pub wields: bool,
    /// Its kind among the generic monsters' tables (minion, protector, conqueror, paragon...),
    /// when it has one.
    pub sub_class: Option<u32>,
    /// The share of its health at or below which it heals itself.
    pub warning_level: f32,
    /// The share of an effect's slowing of its attacks that it takes (all of it when the template
    /// names none).
    pub speed_resistance: f32,
    /// Its table of behavior (a healer's, a protector's...), by name.
    pub npc_table: Option<String>,
    /// It plays its idle behaviors now and then at rest: it has a class of behavior, and not the
    /// summoned creatures' ([`SUMMONED_CLASS`]).
    pub idles: bool,
    /// How long its corpse lasts.
    pub rot: CorpseRot,
}

/// The property names [`CreatureStats`] reads.
pub mod creature_property {
    pub const LEVEL: &str = "GameplayStatistics_Level";
    pub const MAX_HEALTH: &str = "GameplayStatistics_MaxHealth";
    pub const MAX_VIGOR: &str = "GameplayStatistics_MaxVigor";
    pub const DEATH_XP: &str = "GameplayStatistics_DeathExperience";
    pub const DEATH_FOCUS: &str = "GameplayStatistics_DeathFocus";
    pub const HEALTH_REGEN: &str = "GameplayStatistics_HealthRegenRate";
    pub const VIGOR_REGEN: &str = "GameplayStatistics_VigorRegenRate";
    pub const NATURAL_DAMAGE: &str = "AICombat_NaturalDamage";
    pub const NATURAL_VARIANCE: &str = "AICombat_NaturalVariance";
    pub const ARMOR: &str = "NPC_ArmorLevel";
    pub const PERCEPTION: &str = "AI_PerceptionRadius";
    pub const HOMESICK: &str = "AI_HomesickRadius";
    pub const WANDER_RANGE: &str = "AI_WanderingRange";
    pub const WANDER_CHANCE: &str = "AI_WanderingProb";
    pub const WANDERS: &str = "AI_Wandering";
    pub const FREE_ATTACKING: &str = "AI_FreeAttacking";
    pub const LOVE_TABLE: &str = "AI_LoveTable";
    pub const MOVEMENT_TYPE: &str = "AI_MovementType";
    pub const USES_DOORS: &str = "AI_CanUseDoors";
    pub const SCALE: &str = "Scale";
    pub const LOOT_PROFILE: &str = "NPC_DeathLootProfile";
    pub const LOOT_MIN: &str = "NPC_DeathLootMinQuantityOverride";
    pub const LOOT_QUALITY: &str = "NPC_DeathLootQualityOverride";
    pub const LOOT_MAX: &str = "NPC_DeathLootMaxQuantityOverride";
    pub const WEAPON: &str = "AI_Inventory1";
    pub const WEAPONS_HELD: &str = "AI_Inventory1Quantity";
    pub const BESIDE: &str = "AI_Inventory2";
    pub const DUAL_WIELDS: &str = "AICombat_CanDualWield";
    pub const WEAPON_AND_SHIELD: &str = "AICombat_UseWeaponAndShield";
    pub const WIELDS: &str = "AICombat_WieldImplements";
    pub const SUB_CLASS: &str = "AI_AISubClass";
    pub const WARNING_LEVEL: &str = "AI_HealthWarningLevel";
    pub const SPEED_RESISTANCE: &str = "NPC_CombatSpeedResistance";
    pub const NPC_TABLE: &str = "AI_NPCTable";
    pub const SUPER_CLASS: &str = "AI_AISuperClass";
    pub const ROT_UNTOUCHED: &str = "NPC_BaseRotTime";
    pub const ROT_ITEMS_LEFT: &str = "NPC_MaxAcceleratedRotTime";
    pub const ROT_EMPTY: &str = "NPC_MinAcceleratedRotTime";
}

/// How long a corpse lasts, in seconds: its creature's template's, each where the template sets it,
/// else a creature's (3 minutes, 1 minute, 15 seconds).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CorpseRot {
    /// From its making, when it may hold something.
    pub untouched: f64,
    /// From its closing with items left.
    pub items_left: f64,
    /// From its closing empty, and from its making when it can hold nothing.
    pub empty: f64,
}

impl CorpseRot {
    /// Any creature's whose template sets none.
    pub const CREATURE: Self = Self {
        untouched: 180.0,
        items_left: 60.0,
        empty: 15.0,
    };
    /// A player's, whatever is done to it.
    pub const PLAYER: Self = Self {
        untouched: 60.0,
        items_left: 60.0,
        empty: 60.0,
    };

    /// A template's times.
    #[must_use]
    pub fn from_props(p: &Props) -> Self {
        use creature_property as k;
        let time =
            |n: &str, default: f64| p.float(n).filter(|t| *t >= 0.0).map_or(default, f64::from);
        Self {
            untouched: time(k::ROT_UNTOUCHED, Self::CREATURE.untouched),
            items_left: time(k::ROT_ITEMS_LEFT, Self::CREATURE.items_left),
            empty: time(k::ROT_EMPTY, Self::CREATURE.empty),
        }
    }
}

impl Default for CorpseRot {
    fn default() -> Self {
        Self::CREATURE
    }
}

/// The share of its health at or below which a creature whose template names none heals itself
/// (every healer of 2017 that names one names 0.65).
pub const DEFAULT_WARNING_LEVEL: f32 = 0.65;

/// Friend tables (what a creature thinks of whom).
pub mod friend_table {
    /// Friendly to players: never fights them.
    pub const PLAYER_FRIENDLY: u32 = 0x18;
    /// Tables whose creatures never attack a player first (2017: rabbits 1 of 386 players in
    /// reach, the peaceful 8 of 291, peaceful drudges 1 of 64, one side of a war 0 of 17).
    pub const NEVER_FIRST: [u32; 5] = [PLAYER_FRIENDLY, 0x0E, 0x23, 0x38, 0x35];
}

pub use crate::movement::SETTLED_HOMESICK;

/// The class of behavior of summoned creatures, pets among them, which never play an idle behavior
/// (2017: none in 14.7 hours near players, against about 8 an hour for the others).
pub const SUMMONED_CLASS: u32 = 0xFF03;

/// Movement types.
pub mod movement_type {
    /// Never moves.
    pub const STATIONARY: u32 = 0x4000_0001;
    /// Turns where it stands, never moving from it.
    pub const TURN_ONLY: u32 = 0x4000_0002;
}

/// The level of a creature that never moves and whose template gives none (barrels, turrets: 2017
/// showed none for them, their examinations and selections included).
pub const UNLEVELLED: i32 = 1;

impl CreatureStats {
    /// A creature's numbers from its template's properties. A template without a greatest health,
    /// or without a level unless it never moves, is not a creature that fights.
    #[must_use]
    pub fn from_props(p: &Props) -> Option<Self> {
        use creature_property as k;
        #[allow(clippy::cast_possible_truncation)]
        let i32_of = |n: &str| p.int(n).map(|v| v as i32);
        let movement = p.word(k::MOVEMENT_TYPE);
        let fixed = matches!(
            movement,
            Some(movement_type::STATIONARY | movement_type::TURN_ONLY)
        );
        let level = i32_of(k::LEVEL).or(fixed.then_some(UNLEVELLED))?;
        let max_health = i32_of(k::MAX_HEALTH)?;
        let mut skills = Vec::new();
        let mut basic = Vec::new();
        for n in 1..=20 {
            if let Some(s) = p.word(&format!("NPC_StartingSkill{n}")) {
                skills.push((s, p.int(&format!("NPC_StartingSkill{n}Level")).unwrap_or(0)));
                if p.flag(&format!("NPC_StartingSkill{n}_IsBase")) {
                    basic.push(s);
                }
            }
        }
        let mut trophies = Vec::new();
        for n in 1..=7 {
            if let Some(t) = p.word(&format!("NPC_TrophyDrop{n}")) {
                trophies.push((
                    t,
                    p.float(&format!("NPC_TrophyDrop{n}_Rate")).unwrap_or(0.0),
                ));
            }
        }
        let death_effects = (1..=6)
            .filter_map(|n| p.text(&format!("NPC_DeathEffect{n}")).map(str::to_string))
            .collect();
        let mut stats = Self {
            level,
            movement: crate::movement::Movement::from_props(p),
            max_health,
            max_vigor: i32_of(k::MAX_VIGOR).unwrap_or(max_health),
            death_xp: p.int(k::DEATH_XP).unwrap_or(0),
            death_focus: i32_of(k::DEATH_FOCUS).unwrap_or(0),
            natural_damage: p.float(k::NATURAL_DAMAGE).unwrap_or(0.0),
            natural_variance: p.float(k::NATURAL_VARIANCE).unwrap_or(0.0),
            armor: i32_of(k::ARMOR).unwrap_or(0),
            health_regen: p.float(k::HEALTH_REGEN).unwrap_or(0.0),
            vigor_regen: p.float(k::VIGOR_REGEN).unwrap_or(0.0),
            attacks_first: match p.get(k::FREE_ATTACKING) {
                Some(Prop::Bool(b)) => *b,
                _ => true,
            } && !p
                .word(k::LOVE_TABLE)
                .is_some_and(|t| friend_table::NEVER_FIRST.contains(&t)),
            friend_table: p.word(k::LOVE_TABLE),
            fights: true,
            scale: p.float(k::SCALE).filter(|s| *s > 0.0).unwrap_or(1.0),
            skills,
            basic,
            loot_profile: p.word(k::LOOT_PROFILE),
            loot_min: p.int(k::LOOT_MIN),
            loot_max: p.int(k::LOOT_MAX),
            loot_quality: p.int(k::LOOT_QUALITY),
            trophies,
            death_effects,
            weapon: p.text(k::WEAPON).map(str::to_string),
            weapons_held: p.int(k::WEAPONS_HELD).unwrap_or(1),
            beside: p.text(k::BESIDE).map(str::to_string),
            dual_wields: p.flag(k::DUAL_WIELDS),
            weapon_and_shield: p.flag(k::WEAPON_AND_SHIELD),
            wields: p.flag(k::WIELDS),
            sub_class: p.word(k::SUB_CLASS),
            warning_level: p
                .float(k::WARNING_LEVEL)
                .filter(|w| *w > 0.0)
                .unwrap_or(DEFAULT_WARNING_LEVEL),
            speed_resistance: p
                .float(k::SPEED_RESISTANCE)
                .filter(|r| *r >= 0.0)
                .unwrap_or(1.0),
            npc_table: p.text(k::NPC_TABLE).map(str::to_string),
            idles: p.word(k::SUPER_CLASS).is_some_and(|c| c != SUMMONED_CLASS),
            rot: CorpseRot::from_props(p),
        };
        stats.fights = stats.fights_with(stats.movement);
        Some(stats)
    }

    /// Whether it fights, moving as `m` says: never when friendly to players, nor when it never
    /// moves.
    fn fights_with(&self, m: crate::movement::Movement) -> bool {
        self.friend_table != Some(friend_table::PLAYER_FRIENDLY)
            && m.kind != crate::movement::MovementType::Stationary
    }

    /// A camp's movement settings over its template's: what moves the creature, and whether it
    /// fights at all (a camp that makes it stationary makes it a target).
    pub fn set_movement(&mut self, o: &crate::movement::MovementOverrides) {
        let m = self.movement.with(o);
        if o.kind.is_some() {
            self.fights = self.fights_with(m);
        }
        self.movement = m;
    }

    /// It turns where it stands but never moves (a turret).
    #[must_use]
    pub const fn turns_only(&self) -> bool {
        self.movement.turns_only()
    }

    /// It never wanders at rest ([`crate::movement::Movement::settled`]).
    #[must_use]
    pub fn settled(&self) -> bool {
        self.movement.settled()
    }
}
