// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Adapter/GDLE/Models/*.cs
//! The ACE.Adapter models ACE's JSON export writes (Lifestoned weenies, GDLE recipes, landblocks and
//! quests), as `System.Text.Json` serializes them: the serialized members in declaration order,
//! under their `[JsonPropertyName]` (or C# property name), with the C# initializers as `Default`.
//! `[JsonIgnore]`d members, computed properties and public fields (which `System.Text.Json` skips)
//! have no serialized form; a reference type or `Nullable<T>` is an `Option` (`null` is left out).

use super::writer::{JsonDateTime, Node, ToJson};

/// Build an object node from `name => value` pairs, in order.
macro_rules! obj {
    ($($name:literal => $value:expr),* $(,)?) => {
        Node::Obj(vec![$(($name, ToJson::to_json(&$value))),*])
    };
}

// ACE: LSDWeenie
#[derive(Debug, Clone, PartialEq)]
pub struct LsdWeenie {
    pub weenie_id: u32,
    pub weenie_type_id: i32,
    pub attributes: Option<AttributeSet>,
    pub body: Option<Body>,
    pub book: Option<Book>,
    pub bool_stats: Option<Vec<BoolStat>>,
    pub int_stats: Option<Vec<IntStat>>,
    pub did_stats: Option<Vec<DidStat>>,
    pub iid_stats: Option<Vec<IidStat>>,
    pub float_stats: Option<Vec<FloatStat>>,
    pub int64_stats: Option<Vec<Int64Stat>>,
    pub string_stats: Option<Vec<StringStat>>,
    pub create_list: Option<Vec<CreateItem>>,
    pub skills: Option<Vec<SkillListing>>,
    pub emote_table: Option<Vec<EmoteCategoryListing>>,
    pub spells: Option<Vec<SpellbookEntry>>,
    pub positions: Option<Vec<PositionListing>>,
    pub generator_table: Option<Vec<GeneratorTable>>,
    pub last_modified: Option<JsonDateTime>,
    pub modified_by: Option<String>,
    /// A changelog read from an existing file may hold `null` entries.
    pub changelog: Option<Vec<Option<ChangelogEntry>>>,
    pub user_change_summary: Option<String>,
    pub is_done: bool,
    pub comments: Option<String>,
}

impl Default for LsdWeenie {
    fn default() -> Self {
        Self {
            weenie_id: 0,
            weenie_type_id: 0,
            attributes: None,
            body: None,
            book: None,
            // `= new List<...>()` initializers.
            bool_stats: Some(Vec::new()),
            int_stats: Some(Vec::new()),
            did_stats: Some(Vec::new()),
            iid_stats: Some(Vec::new()),
            float_stats: Some(Vec::new()),
            int64_stats: Some(Vec::new()),
            string_stats: Some(Vec::new()),
            create_list: None,
            skills: None,
            emote_table: None,
            spells: None,
            positions: None,
            generator_table: None,
            last_modified: None,
            modified_by: None,
            changelog: Some(Vec::new()),
            user_change_summary: None,
            is_done: false,
            comments: None,
        }
    }
}

impl ToJson for LsdWeenie {
    fn to_json(&self) -> Node {
        obj! {
            "wcid" => self.weenie_id,
            "weenieType" => self.weenie_type_id,
            "attributes" => self.attributes,
            "body" => self.body,
            "pageDataList" => self.book,
            "boolStats" => self.bool_stats,
            "intStats" => self.int_stats,
            "didStats" => self.did_stats,
            "iidStats" => self.iid_stats,
            "floatStats" => self.float_stats,
            "int64Stats" => self.int64_stats,
            "stringStats" => self.string_stats,
            "createList" => self.create_list,
            "skills" => self.skills,
            "emoteTable" => self.emote_table,
            "spellbook" => self.spells,
            "posStats" => self.positions,
            "generatorTable" => self.generator_table,
            "lastModified" => self.last_modified,
            "modifiedBy" => self.modified_by,
            "changelog" => self.changelog,
            "userChangeSummary" => self.user_change_summary,
            "isDone" => self.is_done,
            "comments" => self.comments,
        }
    }
}

// ACE: ChangelogEntry
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ChangelogEntry {
    pub created: JsonDateTime,
    pub author: Option<String>,
    pub comment: Option<String>,
}

impl ToJson for ChangelogEntry {
    fn to_json(&self) -> Node {
        obj! { "created" => self.created, "author" => self.author, "comment" => self.comment }
    }
}

// ACE: Metadata
#[derive(Debug, Clone, PartialEq)]
pub struct Metadata {
    pub last_modified: Option<JsonDateTime>,
    pub modified_by: Option<String>,
    pub changelog: Option<Vec<Option<ChangelogEntry>>>,
    pub user_change_summary: Option<String>,
    pub is_done: bool,
}

impl Metadata {
    // ACE: Metadata.Metadata
    #[must_use]
    pub fn new(weenie: &LsdWeenie) -> Self {
        Self {
            last_modified: weenie.last_modified,
            modified_by: weenie.modified_by.clone(),
            changelog: weenie.changelog.clone(),
            user_change_summary: weenie.user_change_summary.clone(),
            is_done: weenie.is_done,
        }
    }

    // ACE: Metadata.HasInfo
    #[must_use]
    pub fn has_info(&self) -> bool {
        self.last_modified.is_some()
            || self.modified_by.is_some()
            || self.changelog.as_ref().is_some_and(|c| !c.is_empty())
            || self.user_change_summary.is_some()
            || self.is_done
    }
}

impl ToJson for Metadata {
    fn to_json(&self) -> Node {
        obj! {
            "LastModified" => self.last_modified,
            "ModifiedBy" => self.modified_by,
            "Changelog" => self.changelog,
            "UserChangeSummary" => self.user_change_summary,
            "IsDone" => self.is_done,
        }
    }
}

// ACE: AttributeSet
#[derive(Debug, Clone, PartialEq)]
pub struct AttributeSet {
    pub strength: Option<Attribute>,
    pub endurance: Option<Attribute>,
    pub coordination: Option<Attribute>,
    pub quickness: Option<Attribute>,
    pub focus: Option<Attribute>,
    pub self_: Option<Attribute>,
    pub health: Option<Vital>,
    pub stamina: Option<Vital>,
    pub mana: Option<Vital>,
}

impl Default for AttributeSet {
    fn default() -> Self {
        Self {
            strength: None,
            endurance: None,
            coordination: None,
            quickness: None,
            focus: None,
            self_: None,
            // `= new Vital()`.
            health: Some(Vital::default()),
            stamina: Some(Vital::default()),
            mana: Some(Vital::default()),
        }
    }
}

impl ToJson for AttributeSet {
    fn to_json(&self) -> Node {
        obj! {
            "strength" => self.strength,
            "endurance" => self.endurance,
            "coordination" => self.coordination,
            "quickness" => self.quickness,
            "focus" => self.focus,
            "self" => self.self_,
            "health" => self.health,
            "stamina" => self.stamina,
            "mana" => self.mana,
        }
    }
}

// ACE: Attribute
#[derive(Debug, Clone, PartialEq)]
pub struct Attribute {
    pub xp_spent: Option<u32>,
    pub level_from_cp: u32,
    pub ranks: Option<u32>,
}

impl Default for Attribute {
    fn default() -> Self {
        Self {
            xp_spent: Some(0),
            level_from_cp: 0,
            ranks: Some(0),
        }
    }
}

impl ToJson for Attribute {
    fn to_json(&self) -> Node {
        obj! { "cp_spent" => self.xp_spent, "level_from_cp" => self.level_from_cp, "init_level" => self.ranks }
    }
}

// ACE: Vital
#[derive(Debug, Clone, PartialEq)]
pub struct Vital {
    pub xp_spent: Option<u32>,
    pub level_from_cp: Option<u32>,
    pub ranks: Option<u32>,
    pub current: Option<u32>,
}

impl Default for Vital {
    fn default() -> Self {
        Self {
            xp_spent: Some(0),
            level_from_cp: Some(0),
            ranks: Some(0),
            current: Some(0),
        }
    }
}

impl ToJson for Vital {
    fn to_json(&self) -> Node {
        obj! {
            "cp_spent" => self.xp_spent,
            "level_from_cp" => self.level_from_cp,
            "init_level" => self.ranks,
            "current" => self.current,
        }
    }
}

// ACE: Body
#[derive(Debug, Clone, PartialEq)]
pub struct Body {
    pub body_parts: Option<Vec<BodyPartListing>>,
}

impl Default for Body {
    fn default() -> Self {
        Self {
            body_parts: Some(Vec::new()),
        }
    }
}

impl ToJson for Body {
    fn to_json(&self) -> Node {
        obj! { "body_part_table" => self.body_parts }
    }
}

// ACE: BodyPartListing
#[derive(Debug, Clone, PartialEq)]
pub struct BodyPartListing {
    pub key: i32,
    pub body_part: Option<BodyPart>,
}

impl Default for BodyPartListing {
    fn default() -> Self {
        Self {
            key: 0,
            body_part: Some(BodyPart::default()),
        }
    }
}

impl ToJson for BodyPartListing {
    fn to_json(&self) -> Node {
        obj! { "key" => self.key, "value" => self.body_part }
    }
}

// ACE: BodyPart
#[derive(Debug, Clone, PartialEq)]
pub struct BodyPart {
    pub d_type: i32,
    pub d_val: i32,
    pub d_var: f32,
    pub armor_values: Option<ArmorValues>,
    pub bh: i32,
    pub sd: Option<Zones>,
}

impl Default for BodyPart {
    fn default() -> Self {
        Self {
            d_type: 0,
            d_val: 0,
            d_var: 0.0,
            armor_values: Some(ArmorValues::default()),
            bh: 0,
            sd: Some(Zones::default()),
        }
    }
}

impl ToJson for BodyPart {
    fn to_json(&self) -> Node {
        obj! {
            "dtype" => self.d_type,
            "dval" => self.d_val,
            "dvar" => self.d_var,
            "acache" => self.armor_values,
            "bh" => self.bh,
            "bpsd" => self.sd,
        }
    }
}

// ACE: ArmorValues
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ArmorValues {
    pub base_armor: i32,
    pub armor_vs_slash: i32,
    pub armor_vs_pierce: i32,
    pub armor_vs_bludgeon: i32,
    pub armor_vs_cold: i32,
    pub armor_vs_fire: i32,
    pub armor_vs_acid: i32,
    pub armor_vs_electric: i32,
    pub armor_vs_nether: i32,
}

impl ToJson for ArmorValues {
    fn to_json(&self) -> Node {
        obj! {
            "base_armor" => self.base_armor,
            "armor_vs_slash" => self.armor_vs_slash,
            "armor_vs_pierce" => self.armor_vs_pierce,
            "armor_vs_bludgeon" => self.armor_vs_bludgeon,
            "armor_vs_cold" => self.armor_vs_cold,
            "armor_vs_fire" => self.armor_vs_fire,
            "armor_vs_acid" => self.armor_vs_acid,
            "armor_vs_electric" => self.armor_vs_electric,
            "armor_vs_nether" => self.armor_vs_nether,
        }
    }
}

// ACE: Zones
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Zones {
    pub hlf: Option<f64>,
    pub mlf: Option<f64>,
    pub llf: Option<f64>,
    pub hrf: Option<f64>,
    pub mrf: Option<f64>,
    pub lrf: Option<f64>,
    pub hlb: Option<f64>,
    pub mlb: Option<f64>,
    pub llb: Option<f64>,
    pub hrb: Option<f64>,
    pub mrb: Option<f64>,
    pub lrb: Option<f64>,
}

impl ToJson for Zones {
    fn to_json(&self) -> Node {
        obj! {
            "HLF" => self.hlf,
            "MLF" => self.mlf,
            "LLF" => self.llf,
            "HRF" => self.hrf,
            "MRF" => self.mrf,
            "LRF" => self.lrf,
            "HLB" => self.hlb,
            "MLB" => self.mlb,
            "LLB" => self.llb,
            "HRB" => self.hrb,
            "MRB" => self.mrb,
            "LRB" => self.lrb,
        }
    }
}

// ACE: Book
#[derive(Debug, Clone, PartialEq)]
pub struct Book {
    pub max_characters_per_page: i32,
    pub max_number_pages: i32,
    pub pages: Option<Vec<Page>>,
}

impl Default for Book {
    fn default() -> Self {
        Self {
            max_characters_per_page: 0,
            max_number_pages: 0,
            pages: Some(Vec::new()),
        }
    }
}

impl ToJson for Book {
    fn to_json(&self) -> Node {
        obj! {
            "maxNumCharsPerPage" => self.max_characters_per_page,
            "maxNumPages" => self.max_number_pages,
            "pages" => self.pages,
        }
    }
}

// ACE: Page
#[derive(Debug, Clone, PartialEq)]
pub struct Page {
    /// Not ACE's (a fix): written as `authorAccount`, and read back by
    /// the import; ACE declared it as a public field, which its serializer skips, so every export
    /// dropped the page's author account.
    pub author_account: Option<String>,
    pub author_id: Option<u32>,
    pub author_name: Option<String>,
    pub ignore_autor_binder: Option<u8>,
    pub page_text: Option<String>,
}

impl Default for Page {
    fn default() -> Self {
        Self {
            author_account: Some(String::new()),
            author_id: None,
            author_name: None,
            ignore_autor_binder: None,
            page_text: None,
        }
    }
}

impl Page {
    // ACE: Page.IgnoreAuthor
    pub fn set_ignore_author(&mut self, value: Option<bool>) {
        self.ignore_autor_binder = value.map(u8::from);
    }

    // ACE: Page.PageText
    pub fn set_page_text(&mut self, value: &str) {
        self.page_text = Some(value.replace('\r', ""));
    }
}

impl ToJson for Page {
    fn to_json(&self) -> Node {
        obj! {
            "authorAccount" => self.author_account,
            "authorID" => self.author_id,
            "authorName" => self.author_name,
            "ignoreAuthor" => self.ignore_autor_binder,
            "pageText" => self.page_text,
        }
    }
}

macro_rules! stat {
    ($(#[$m:meta])* $name:ident, $v:ty) => {
        $(#[$m])*
        #[derive(Debug, Clone, PartialEq, Default)]
        pub struct $name {
            pub key: i32,
            pub value: $v,
        }

        impl ToJson for $name {
            fn to_json(&self) -> Node {
                obj! { "key" => self.key, "value" => self.value }
            }
        }
    };
}

stat!(
    /// ACE: BoolStat
    BoolStat, i32
);
stat!(
    /// ACE: IntStat
    IntStat, i32
);
stat!(
    /// ACE: DidStat
    DidStat, u32
);
stat!(
    /// ACE: IidStat
    IidStat, i32
);
stat!(
    /// ACE: FloatStat
    FloatStat, f32
);
stat!(
    /// ACE: Int64Stat
    Int64Stat, i64
);
stat!(
    /// ACE: StringStat
    StringStat, Option<String>
);

// ACE: CreateItem
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CreateItem {
    pub weenie_class_id: Option<u32>,
    pub palette: Option<u32>,
    pub shade: Option<f64>,
    pub destination: Option<u32>,
    pub stack_size: Option<i32>,
    pub try_to_bond: Option<u8>,
}

impl ToJson for CreateItem {
    fn to_json(&self) -> Node {
        obj! {
            "wcid" => self.weenie_class_id,
            "palette" => self.palette,
            "shade" => self.shade,
            "destination" => self.destination,
            "stack_size" => self.stack_size,
            "try_to_bond" => self.try_to_bond,
        }
    }
}

// ACE: SkillListing
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SkillListing {
    pub skill_id: Option<i32>,
    pub skill: Option<Skill>,
}

impl ToJson for SkillListing {
    fn to_json(&self) -> Node {
        obj! { "key" => self.skill_id, "value" => self.skill }
    }
}

// ACE: Skill
#[derive(Debug, Clone, PartialEq)]
pub struct Skill {
    pub level_from_pp: Option<u32>,
    pub last_used: Option<f32>,
    pub ranks: Option<u32>,
    pub xp_invested: Option<u32>,
    pub resistance_of_last_check: Option<u32>,
    pub trained_level: Option<i32>,
}

impl Default for Skill {
    fn default() -> Self {
        Self {
            level_from_pp: Some(0),
            last_used: Some(0.0),
            ranks: Some(0),
            xp_invested: Some(0),
            resistance_of_last_check: Some(0),
            trained_level: Some(0),
        }
    }
}

impl ToJson for Skill {
    fn to_json(&self) -> Node {
        obj! {
            "level_from_pp" => self.level_from_pp,
            "last_used_time" => self.last_used,
            "init_level" => self.ranks,
            "pp" => self.xp_invested,
            "resistance_of_last_check" => self.resistance_of_last_check,
            "sac" => self.trained_level,
        }
    }
}

// ACE: EmoteCategoryListing
#[derive(Debug, Clone, PartialEq)]
pub struct EmoteCategoryListing {
    pub emote_category_id: i32,
    pub emotes: Option<Vec<Emote>>,
}

impl Default for EmoteCategoryListing {
    fn default() -> Self {
        Self {
            emote_category_id: 0,
            emotes: Some(Vec::new()),
        }
    }
}

impl ToJson for EmoteCategoryListing {
    fn to_json(&self) -> Node {
        obj! { "key" => self.emote_category_id, "value" => self.emotes }
    }
}

// ACE: Emote
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Emote {
    pub category: u32,
    pub actions: Option<Vec<EmoteAction>>,
    pub probability: Option<f32>,
    pub vendor_type: Option<u32>,
    pub quest: Option<String>,
    pub class_id: Option<u32>,
    pub style: Option<u32>,
    pub sub_style: Option<u32>,
    pub min_health: Option<f32>,
    pub max_health: Option<f32>,
}

impl ToJson for Emote {
    fn to_json(&self) -> Node {
        obj! {
            "category" => self.category,
            "emotes" => self.actions,
            "probability" => self.probability,
            "vendorType" => self.vendor_type,
            "quest" => self.quest,
            "classID" => self.class_id,
            "style" => self.style,
            "substyle" => self.sub_style,
            "minhealth" => self.min_health,
            "maxhealth" => self.max_health,
        }
    }
}

// ACE: EmoteAction
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EmoteAction {
    pub emote_action_type: u32,
    pub delay: Option<f32>,
    pub extent: Option<f32>,
    pub amount: Option<u32>,
    pub motion: Option<u32>,
    pub message: Option<String>,
    pub amount64: Option<i64>,
    pub hero_xp64: Option<u64>,
    pub item: Option<CreateItem>,
    pub minimum64: Option<i64>,
    pub maximum64: Option<i64>,
    pub percent: Option<f32>,
    pub display_binder: Option<u8>,
    pub max: Option<u32>,
    pub min: Option<u32>,
    pub f_max: Option<f32>,
    pub f_min: Option<f32>,
    pub stat: Option<u32>,
    pub p_script: Option<u32>,
    pub sound: Option<u32>,
    pub m_position: Option<Position>,
    pub frame: Option<Frame>,
    pub spell_id: Option<u32>,
    pub test_string: Option<String>,
    pub wealth_rating: Option<u32>,
    pub treasure_class: Option<u32>,
    pub treasure_type: Option<i32>,
}

impl EmoteAction {
    // ACE: EmoteAction.Display
    pub fn set_display(&mut self, value: Option<bool>) {
        self.display_binder = value.map(u8::from);
    }
}

impl ToJson for EmoteAction {
    fn to_json(&self) -> Node {
        obj! {
            "type" => self.emote_action_type,
            "delay" => self.delay,
            "extent" => self.extent,
            "amount" => self.amount,
            "motion" => self.motion,
            "msg" => self.message,
            "amount64" => self.amount64,
            "heroxp64" => self.hero_xp64,
            "cprof" => self.item,
            "min64" => self.minimum64,
            "max64" => self.maximum64,
            "percent" => self.percent,
            "display" => self.display_binder,
            "max" => self.max,
            "min" => self.min,
            "fmax" => self.f_max,
            "fmin" => self.f_min,
            "stat" => self.stat,
            "pscript" => self.p_script,
            "sound" => self.sound,
            "mPosition" => self.m_position,
            "frame" => self.frame,
            "spellid" => self.spell_id,
            "teststring" => self.test_string,
            "wealth_rating" => self.wealth_rating,
            "treasure_class" => self.treasure_class,
            "treasure_type" => self.treasure_type,
        }
    }
}

// ACE: SpellbookEntry
#[derive(Debug, Clone, PartialEq)]
pub struct SpellbookEntry {
    pub spell_id: i32,
    pub stats: Option<SpellCastingStats>,
}

impl Default for SpellbookEntry {
    fn default() -> Self {
        Self {
            spell_id: 0,
            stats: Some(SpellCastingStats::default()),
        }
    }
}

impl ToJson for SpellbookEntry {
    fn to_json(&self) -> Node {
        obj! { "key" => self.spell_id, "value" => self.stats }
    }
}

// ACE: SpellCastingStats
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SpellCastingStats {
    pub casting_chance: Option<f64>,
}

impl ToJson for SpellCastingStats {
    fn to_json(&self) -> Node {
        obj! { "casting_likelihood" => self.casting_chance }
    }
}

// ACE: PositionListing
#[derive(Debug, Clone, PartialEq)]
pub struct PositionListing {
    pub position_type: i32,
    pub position: Option<Position>,
}

impl Default for PositionListing {
    fn default() -> Self {
        Self {
            position_type: 0,
            position: Some(Position::default()),
        }
    }
}

impl ToJson for PositionListing {
    fn to_json(&self) -> Node {
        obj! { "key" => self.position_type, "value" => self.position }
    }
}

// ACE: Position
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Position {
    pub frame: Option<Frame>,
    pub land_cell_id: u32,
}

impl ToJson for Position {
    fn to_json(&self) -> Node {
        obj! { "frame" => self.frame, "objcell_id" => self.land_cell_id }
    }
}

// ACE: Frame
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Frame {
    pub position: Option<Xyz>,
    pub rotations: Option<Quaternion>,
}

impl ToJson for Frame {
    fn to_json(&self) -> Node {
        obj! { "origin" => self.position, "angles" => self.rotations }
    }
}

// ACE: XYZ
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Xyz {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl ToJson for Xyz {
    fn to_json(&self) -> Node {
        obj! { "x" => self.x, "y" => self.y, "z" => self.z }
    }
}

// ACE: Quaternion
#[derive(Debug, Clone, PartialEq)]
pub struct Quaternion {
    pub w: f32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Default for Quaternion {
    fn default() -> Self {
        Self {
            w: 1.0,
            x: 0.0,
            y: 0.0,
            z: 0.0,
        }
    }
}

impl ToJson for Quaternion {
    fn to_json(&self) -> Node {
        obj! { "w" => self.w, "x" => self.x, "y" => self.y, "z" => self.z }
    }
}

// ACE: GeneratorTable
#[derive(Debug, Clone, PartialEq)]
pub struct GeneratorTable {
    pub delay: f32,
    pub frame: Option<Frame>,
    pub init_create: u32,
    pub max_number: u32,
    pub object_cell: u32,
    pub probability: f64,
    pub palette_id: u32,
    pub shade: f32,
    pub slot: u32,
    pub stack_size: i32,
    pub weenie_class_id: u32,
    pub when_create: u32,
    pub where_create: u32,
}

impl Default for GeneratorTable {
    fn default() -> Self {
        Self {
            delay: 0.0,
            frame: Some(Frame::default()),
            init_create: 0,
            max_number: 0,
            object_cell: 0,
            probability: 0.0,
            palette_id: 0,
            shade: 0.0,
            slot: 0,
            stack_size: 0,
            weenie_class_id: 0,
            when_create: 0,
            where_create: 0,
        }
    }
}

impl ToJson for GeneratorTable {
    fn to_json(&self) -> Node {
        obj! {
            "delay" => self.delay,
            "frame" => self.frame,
            "initCreate" => self.init_create,
            "maxNum" => self.max_number,
            "objcell_id" => self.object_cell,
            "probability" => self.probability,
            "ptid" => self.palette_id,
            "shade" => self.shade,
            "slot" => self.slot,
            "stackSize" => self.stack_size,
            "type" => self.weenie_class_id,
            "whenCreate" => self.when_create,
            "whereCreate" => self.where_create,
        }
    }
}

// ---- GDLE recipe -----------------------------------------------------------------------------

// ACE: RecipeCombined
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RecipeCombined {
    pub key: u32,
    pub desc: Option<String>,
    pub recipe: Option<Recipe>,
    pub precursors: Option<Vec<RecipePrecursor>>,
}

impl ToJson for RecipeCombined {
    fn to_json(&self) -> Node {
        obj! { "key" => self.key, "desc" => self.desc, "recipe" => self.recipe, "precursors" => self.precursors }
    }
}

// ACE: RecipePrecursor
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RecipePrecursor {
    pub tool: u32,
    pub target: u32,
    pub recipe_id: Option<u32>,
}

impl ToJson for RecipePrecursor {
    fn to_json(&self) -> Node {
        obj! { "Tool" => self.tool, "Target" => self.target, "RecipeId" => self.recipe_id }
    }
}

// ACE: Recipe
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Recipe {
    pub recipe_id: u32,
    pub skill: u32,
    pub skill_check_formula_type: i32,
    pub data_id: u32,
    pub difficulty: u32,
    pub success_wcid: u32,
    pub success_amount: u32,
    pub success_message: Option<String>,
    pub success_consume_target_amount: u32,
    pub success_consume_target_chance: f64,
    pub success_consume_target_message: Option<String>,
    pub success_consume_tool_amount: u32,
    pub success_consume_tool_chance: f64,
    pub success_consume_tool_message: Option<String>,
    pub fail_wcid: u32,
    pub fail_amount: u32,
    pub fail_message: Option<String>,
    pub failure_consume_target_amount: u32,
    pub failure_consume_target_chance: f64,
    pub failure_consume_target_message: Option<String>,
    pub failure_consume_tool_amount: u32,
    pub failure_consume_tool_chance: f64,
    pub failure_consume_tool_message: Option<String>,
    pub unknown: i32,
    /// Eight slots; an unused one is `null`.
    pub mods: Option<Vec<Option<Mod>>>,
    /// Three slots; an unused one is `null`.
    pub requirements: Option<Vec<Option<RecipeRequirements>>>,
}

impl ToJson for Recipe {
    fn to_json(&self) -> Node {
        obj! {
            "RecipeID" => self.recipe_id,
            "Skill" => self.skill,
            "SkillCheckFormulaType" => self.skill_check_formula_type,
            "DataID" => self.data_id,
            "Difficulty" => self.difficulty,
            "SuccessWcid" => self.success_wcid,
            "SuccessAmount" => self.success_amount,
            "SuccessMessage" => self.success_message,
            "SuccessConsumeTargetAmount" => self.success_consume_target_amount,
            "SuccessConsumeTargetChance" => self.success_consume_target_chance,
            "SuccessConsumeTargetMessage" => self.success_consume_target_message,
            "SuccessConsumeToolAmount" => self.success_consume_tool_amount,
            "SuccessConsumeToolChance" => self.success_consume_tool_chance,
            "SuccessConsumeToolMessage" => self.success_consume_tool_message,
            "FailWcid" => self.fail_wcid,
            "FailAmount" => self.fail_amount,
            "FailMessage" => self.fail_message,
            "FailureConsumeTargetAmount" => self.failure_consume_target_amount,
            "FailureConsumeTargetChance" => self.failure_consume_target_chance,
            "FailureConsumeTargetMessage" => self.failure_consume_target_message,
            "FailureConsumeToolAmount" => self.failure_consume_tool_amount,
            "FailureConsumeToolChance" => self.failure_consume_tool_chance,
            "FailureConsumeToolMessage" => self.failure_consume_tool_message,
            "Unknown" => self.unknown,
            "Mods" => self.mods,
            "Requirements" => self.requirements,
        }
    }
}

// ACE: Mod
#[derive(Debug, Clone, PartialEq)]
pub struct Mod {
    pub int_requirements: Option<Vec<IntRequirement>>,
    pub did_requirements: Option<Vec<DidRequirement>>,
    pub iid_requirements: Option<Vec<IidRequirement>>,
    pub float_requirements: Option<Vec<FloatRequirement>>,
    pub string_requirements: Option<Vec<StringRequirement>>,
    pub bool_requirements: Option<Vec<BoolRequirement>>,
    pub modify_health: i32,
    pub modify_stamina: i32,
    pub modify_mana: i32,
    pub requires_health: i32,
    pub requires_stamina: i32,
    pub requires_mana: i32,
    pub unknown7: bool,
    pub modification_script_id: i32,
    pub unknown9: i32,
    pub unknown10: i32,
}

impl Default for Mod {
    fn default() -> Self {
        Self {
            int_requirements: Some(Vec::new()),
            did_requirements: Some(Vec::new()),
            iid_requirements: Some(Vec::new()),
            float_requirements: Some(Vec::new()),
            string_requirements: Some(Vec::new()),
            bool_requirements: Some(Vec::new()),
            modify_health: 0,
            modify_stamina: 0,
            modify_mana: 0,
            requires_health: 0,
            requires_stamina: 0,
            requires_mana: 0,
            unknown7: false,
            modification_script_id: 0,
            unknown9: 0,
            unknown10: 0,
        }
    }
}

impl ToJson for Mod {
    fn to_json(&self) -> Node {
        obj! {
            "IntRequirements" => self.int_requirements,
            "DIDRequirements" => self.did_requirements,
            "IIDRequirements" => self.iid_requirements,
            "FloatRequirements" => self.float_requirements,
            "StringRequirements" => self.string_requirements,
            "BoolRequirements" => self.bool_requirements,
            "ModifyHealth" => self.modify_health,
            "ModifyStamina" => self.modify_stamina,
            "ModifyMana" => self.modify_mana,
            "RequiresHealth" => self.requires_health,
            "RequiresStamina" => self.requires_stamina,
            "RequiresMana" => self.requires_mana,
            "Unknown7" => self.unknown7,
            "ModificationScriptId" => self.modification_script_id,
            "Unknown9" => self.unknown9,
            "Unknown10" => self.unknown10,
        }
    }
}

// ACE: RecipeRequirements
#[derive(Debug, Clone, PartialEq)]
pub struct RecipeRequirements {
    pub int_requirements: Option<Vec<IntRequirement>>,
    pub did_requirements: Option<Vec<DidRequirement>>,
    pub iid_requirements: Option<Vec<IidRequirement>>,
    pub float_requirements: Option<Vec<FloatRequirement>>,
    pub string_requirements: Option<Vec<StringRequirement>>,
    pub bool_requirements: Option<Vec<BoolRequirement>>,
}

impl Default for RecipeRequirements {
    fn default() -> Self {
        Self {
            int_requirements: Some(Vec::new()),
            did_requirements: Some(Vec::new()),
            iid_requirements: Some(Vec::new()),
            float_requirements: Some(Vec::new()),
            string_requirements: Some(Vec::new()),
            bool_requirements: Some(Vec::new()),
        }
    }
}

impl ToJson for RecipeRequirements {
    fn to_json(&self) -> Node {
        obj! {
            "IntRequirements" => self.int_requirements,
            "DIDRequirements" => self.did_requirements,
            "IIDRequirements" => self.iid_requirements,
            "FloatRequirements" => self.float_requirements,
            "StringRequirements" => self.string_requirements,
            "BoolRequirements" => self.bool_requirements,
        }
    }
}

macro_rules! requirement {
    ($(#[$m:meta])* $name:ident, $v:ty) => {
        $(#[$m])*
        #[derive(Debug, Clone, PartialEq, Default)]
        pub struct $name {
            pub unknown: Option<i32>,
            pub operation_type: i32,
            pub message: Option<String>,
            pub stat: i32,
            pub value: $v,
        }

        impl ToJson for $name {
            fn to_json(&self) -> Node {
                obj! {
                    "Unknown" => self.unknown,
                    "OperationType" => self.operation_type,
                    "Message" => self.message,
                    "Stat" => self.stat,
                    "Value" => self.value,
                }
            }
        }
    };
}

requirement!(
    /// ACE: IntRequirement
    IntRequirement, i32
);
requirement!(
    /// ACE: DIDRequirement
    DidRequirement, u32
);
requirement!(
    /// ACE: IIDRequirement
    IidRequirement, u32
);
requirement!(
    /// ACE: FloatRequirement
    FloatRequirement, f64
);
requirement!(
    /// ACE: BoolRequirement
    BoolRequirement, bool
);

// ACE: StringRequirement
/// Not ACE's (a fix): it carries the requirement's `Message`, as the
/// other requirements do and GDLE's own recipe format does; ACE's has none, so an export lost it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StringRequirement {
    pub unknown: i32,
    pub operation_type: i32,
    pub message: Option<String>,
    pub stat: i32,
    pub value: Option<String>,
}

impl ToJson for StringRequirement {
    fn to_json(&self) -> Node {
        obj! {
            "Unknown" => self.unknown,
            "OperationType" => self.operation_type,
            "Message" => self.message,
            "Stat" => self.stat,
            "Value" => self.value,
        }
    }
}

// ---- GDLE landblock and quest ----------------------------------------------------------------

// ACE: Landblock
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Landblock {
    pub key: u32,
    pub value: Option<LandblockValue>,
    pub desc: Option<String>,
}

impl ToJson for Landblock {
    fn to_json(&self) -> Node {
        obj! { "key" => self.key, "value" => self.value, "desc" => self.desc }
    }
}

// ACE: LandblockValue
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LandblockValue {
    pub links: Option<Vec<LandblockLink>>,
    pub weenies: Option<Vec<LandblockWeenie>>,
}

impl ToJson for LandblockValue {
    fn to_json(&self) -> Node {
        obj! { "links" => self.links, "weenies" => self.weenies }
    }
}

// ACE: LandblockLink
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LandblockLink {
    pub target: u32,
    pub source: u32,
    pub desc: Option<String>,
}

impl ToJson for LandblockLink {
    fn to_json(&self) -> Node {
        obj! { "target" => self.target, "source" => self.source, "desc" => self.desc }
    }
}

// ACE: LandblockWeenie
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LandblockWeenie {
    pub id: u32,
    pub wcid: u32,
    pub desc: Option<String>,
    pub pos: Option<Position>,
}

impl ToJson for LandblockWeenie {
    fn to_json(&self) -> Node {
        obj! { "id" => self.id, "wcid" => self.wcid, "desc" => self.desc, "pos" => self.pos }
    }
}

// ACE: Quest
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Quest {
    pub key: Option<String>,
    pub value: Option<QuestValue>,
}

impl ToJson for Quest {
    fn to_json(&self) -> Node {
        obj! { "key" => self.key, "value" => self.value }
    }
}

// ACE: QuestValue
#[derive(Debug, Clone, PartialEq, Default)]
pub struct QuestValue {
    pub fullname: Option<String>,
    pub maxsolves: i32,
    pub mindelta: i32,
}

impl ToJson for QuestValue {
    fn to_json(&self) -> Node {
        obj! { "fullname" => self.fullname, "maxsolves" => self.maxsolves, "mindelta" => self.mindelta }
    }
}
