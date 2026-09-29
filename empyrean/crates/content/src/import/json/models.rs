// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Adapter/GDLE/Models/*.cs
//! The ACE.Adapter JSON models that ACE's `import-json` deserializes (Lifestoned weenies and the
//! GDLE recipe, landblock and quest documents), with the property names, types and initial values
//! of the C# classes. `JsonIgnore`d members, read-only members and public fields (which
//! `System.Text.Json` skips) are not read. A class-typed value (`null` allowed) is an `Option`.
//!
//! Each `read` mirrors `JsonSerializer.Deserialize` for that class: it starts from the C#
//! initializers and assigns every known property in document order.

use super::value::{
    bool_, datetime, f32_, f64_, i32_, i64_, list, object, opt, string, u32_, u64_, u8_, Json, R,
};

/// Read a class-typed value: `None` for `null`, else the members applied over `T::default()`.
pub(crate) fn class<T: Default>(
    j: &Json,
    what: &str,
    mut set: impl FnMut(&mut T, &str, &Json) -> R<()>,
) -> R<Option<T>> {
    let Some(members) = object(j, what)? else {
        return Ok(None);
    };
    let mut t = T::default();
    for (k, v) in members {
        set(&mut t, k, v)?;
    }
    Ok(Some(t))
}

/// `List<Class>`: `null` elements stay `None`, as C# keeps them.
pub(crate) fn class_list<T>(
    j: &Json,
    what: &str,
    read: fn(&Json, &str) -> R<Option<T>>,
) -> R<Option<Vec<Option<T>>>> {
    list(j, what, read)
}

// ACE: LSDWeenie
#[derive(Debug, Clone)]
pub struct LsdWeenie {
    pub weenie_id: u32,
    pub weenie_type_id: i32,
    pub attributes: Option<AttributeSet>,
    pub body: Option<Body>,
    pub book: Option<Book>,
    pub bool_stats: Option<Vec<Option<BoolStat>>>,
    pub int_stats: Option<Vec<Option<IntStat>>>,
    pub did_stats: Option<Vec<Option<DidStat>>>,
    pub iid_stats: Option<Vec<Option<IidStat>>>,
    pub float_stats: Option<Vec<Option<FloatStat>>>,
    pub int64_stats: Option<Vec<Option<Int64Stat>>>,
    pub string_stats: Option<Vec<Option<StringStat>>>,
    pub create_list: Option<Vec<Option<CreateItem>>>,
    pub skills: Option<Vec<Option<SkillListing>>>,
    pub emote_table: Option<Vec<Option<EmoteCategoryListing>>>,
    pub spells: Option<Vec<Option<SpellbookEntry>>>,
    pub positions: Option<Vec<Option<PositionListing>>>,
    pub generator_table: Option<Vec<Option<GeneratorTable>>>,
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
        }
    }
}

impl LsdWeenie {
    pub fn read(j: &Json) -> R<Option<Self>> {
        class(j, "LSDWeenie", |t: &mut Self, k, v| {
            match k {
                "wcid" => t.weenie_id = u32_(v, k)?,
                "weenieType" => t.weenie_type_id = i32_(v, k)?,
                "attributes" => t.attributes = AttributeSet::read(v, k)?,
                "body" => t.body = Body::read(v, k)?,
                "pageDataList" => t.book = Book::read(v, k)?,
                "boolStats" => t.bool_stats = class_list(v, k, BoolStat::read)?,
                "intStats" => t.int_stats = class_list(v, k, IntStat::read)?,
                "didStats" => t.did_stats = class_list(v, k, DidStat::read)?,
                "iidStats" => t.iid_stats = class_list(v, k, IidStat::read)?,
                "floatStats" => t.float_stats = class_list(v, k, FloatStat::read)?,
                "int64Stats" => t.int64_stats = class_list(v, k, Int64Stat::read)?,
                "stringStats" => t.string_stats = class_list(v, k, StringStat::read)?,
                "createList" => t.create_list = class_list(v, k, CreateItem::read)?,
                "skills" => t.skills = class_list(v, k, SkillListing::read)?,
                "emoteTable" => t.emote_table = class_list(v, k, EmoteCategoryListing::read)?,
                "spellbook" => t.spells = class_list(v, k, SpellbookEntry::read)?,
                "posStats" => t.positions = class_list(v, k, PositionListing::read)?,
                "generatorTable" => t.generator_table = class_list(v, k, GeneratorTable::read)?,
                // Metadata: deserialized (so a bad value fails the load) but never converted.
                "lastModified" => {
                    opt(v, k, datetime)?;
                }
                "modifiedBy" | "userChangeSummary" | "comments" => {
                    string(v, k)?;
                }
                "changelog" => {
                    class_list(v, k, ChangelogEntry::read)?;
                }
                "isDone" => {
                    bool_(v, k)?;
                }
                _ => {}
            }
            Ok(())
        })
    }

    /// ACE: LSDWeenie.Name
    pub fn name(&self) -> R<String> {
        if let Some(stats) = &self.string_stats {
            for s in stats {
                let s = s.as_ref().ok_or("stringStats: null element")?;
                if s.key == 1 {
                    return Ok(s
                        .value
                        .clone()
                        .unwrap_or_else(|| self.weenie_id.to_string()));
                }
            }
        }
        Ok(self.weenie_id.to_string())
    }
}

// ACE: ChangelogEntry
#[derive(Debug, Clone, Default)]
pub struct ChangelogEntry;

impl ChangelogEntry {
    pub fn read(j: &Json, what: &str) -> R<Option<Self>> {
        class(j, what, |_: &mut Self, k, v| {
            match k {
                "created" => datetime(v, k)?,
                "author" | "comment" => {
                    string(v, k)?;
                }
                _ => {}
            }
            Ok(())
        })
    }
}

// ACE: AttributeSet
#[derive(Debug, Clone)]
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
            health: Some(Vital::default()),
            stamina: Some(Vital::default()),
            mana: Some(Vital::default()),
        }
    }
}

impl AttributeSet {
    pub fn read(j: &Json, what: &str) -> R<Option<Self>> {
        class(j, what, |t: &mut Self, k, v| {
            match k {
                "strength" => t.strength = Attribute::read(v, k)?,
                "endurance" => t.endurance = Attribute::read(v, k)?,
                "coordination" => t.coordination = Attribute::read(v, k)?,
                "quickness" => t.quickness = Attribute::read(v, k)?,
                "focus" => t.focus = Attribute::read(v, k)?,
                "self" => t.self_ = Attribute::read(v, k)?,
                "health" => t.health = Vital::read(v, k)?,
                "stamina" => t.stamina = Vital::read(v, k)?,
                "mana" => t.mana = Vital::read(v, k)?,
                _ => {}
            }
            Ok(())
        })
    }
}

// ACE: Attribute
#[derive(Debug, Clone)]
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

impl Attribute {
    pub fn read(j: &Json, what: &str) -> R<Option<Self>> {
        class(j, what, |t: &mut Self, k, v| {
            match k {
                "cp_spent" => t.xp_spent = opt(v, k, u32_)?,
                "level_from_cp" => t.level_from_cp = u32_(v, k)?,
                "init_level" => t.ranks = opt(v, k, u32_)?,
                _ => {}
            }
            Ok(())
        })
    }
}

// ACE: Vital
#[derive(Debug, Clone)]
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

impl Vital {
    pub fn read(j: &Json, what: &str) -> R<Option<Self>> {
        class(j, what, |t: &mut Self, k, v| {
            match k {
                "cp_spent" => t.xp_spent = opt(v, k, u32_)?,
                "level_from_cp" => t.level_from_cp = opt(v, k, u32_)?,
                "init_level" => t.ranks = opt(v, k, u32_)?,
                "current" => t.current = opt(v, k, u32_)?,
                _ => {}
            }
            Ok(())
        })
    }
}

// ACE: Body
#[derive(Debug, Clone)]
pub struct Body {
    pub body_parts: Option<Vec<Option<BodyPartListing>>>,
}

impl Default for Body {
    fn default() -> Self {
        Self {
            body_parts: Some(Vec::new()),
        }
    }
}

impl Body {
    pub fn read(j: &Json, what: &str) -> R<Option<Self>> {
        class(j, what, |t: &mut Self, k, v| {
            if k == "body_part_table" {
                t.body_parts = class_list(v, k, BodyPartListing::read)?;
            }
            Ok(())
        })
    }
}

// ACE: BodyPartListing
#[derive(Debug, Clone)]
pub struct BodyPartListing {
    pub key: i32,
    pub body_part: Option<BodyPart>,
}

// ACE: BodyPartListing.BodyPartListing
impl Default for BodyPartListing {
    fn default() -> Self {
        Self {
            key: 0,
            body_part: Some(BodyPart::default()),
        }
    }
}

impl BodyPartListing {
    pub fn read(j: &Json, what: &str) -> R<Option<Self>> {
        class(j, what, |t: &mut Self, k, v| {
            match k {
                "key" => t.key = i32_(v, k)?,
                "value" => t.body_part = BodyPart::read(v, k)?,
                _ => {}
            }
            Ok(())
        })
    }
}

// ACE: BodyPart
#[derive(Debug, Clone)]
pub struct BodyPart {
    pub d_type: i32,
    pub d_val: i32,
    pub d_var: f32,
    pub armor_values: Option<ArmorValues>,
    pub bh: i32,
    pub sd: Option<Zones>,
}

// ACE: BodyPart.BodyPart
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

impl BodyPart {
    pub fn read(j: &Json, what: &str) -> R<Option<Self>> {
        class(j, what, |t: &mut Self, k, v| {
            match k {
                "dtype" => t.d_type = i32_(v, k)?,
                "dval" => t.d_val = i32_(v, k)?,
                "dvar" => t.d_var = f32_(v, k)?,
                "acache" => t.armor_values = ArmorValues::read(v, k)?,
                "bh" => t.bh = i32_(v, k)?,
                "bpsd" => t.sd = Zones::read(v, k)?,
                _ => {}
            }
            Ok(())
        })
    }
}

// ACE: ArmorValues
#[derive(Debug, Clone, Default)]
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

impl ArmorValues {
    pub fn read(j: &Json, what: &str) -> R<Option<Self>> {
        class(j, what, |t: &mut Self, k, v| {
            let slot = match k {
                "base_armor" => &mut t.base_armor,
                "armor_vs_slash" => &mut t.armor_vs_slash,
                "armor_vs_pierce" => &mut t.armor_vs_pierce,
                "armor_vs_bludgeon" => &mut t.armor_vs_bludgeon,
                "armor_vs_cold" => &mut t.armor_vs_cold,
                "armor_vs_fire" => &mut t.armor_vs_fire,
                "armor_vs_acid" => &mut t.armor_vs_acid,
                "armor_vs_electric" => &mut t.armor_vs_electric,
                "armor_vs_nether" => &mut t.armor_vs_nether,
                _ => return Ok(()),
            };
            *slot = i32_(v, k)?;
            Ok(())
        })
    }
}

// ACE: Zones
/// The twelve `double?` hit-location weights, keyed by their C# names (no `JsonPropertyName`).
#[derive(Debug, Clone, Default)]
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

impl Zones {
    pub fn read(j: &Json, what: &str) -> R<Option<Self>> {
        class(j, what, |t: &mut Self, k, v| {
            let slot = match k {
                "HLF" => &mut t.hlf,
                "MLF" => &mut t.mlf,
                "LLF" => &mut t.llf,
                "HRF" => &mut t.hrf,
                "MRF" => &mut t.mrf,
                "LRF" => &mut t.lrf,
                "HLB" => &mut t.hlb,
                "MLB" => &mut t.mlb,
                "LLB" => &mut t.llb,
                "HRB" => &mut t.hrb,
                "MRB" => &mut t.mrb,
                "LRB" => &mut t.lrb,
                _ => return Ok(()),
            };
            *slot = opt(v, k, f64_)?;
            Ok(())
        })
    }
}

// ACE: Book
#[derive(Debug, Clone)]
pub struct Book {
    pub max_characters_per_page: i32,
    pub max_number_pages: i32,
    pub pages: Option<Vec<Option<Page>>>,
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

impl Book {
    pub fn read(j: &Json, what: &str) -> R<Option<Self>> {
        class(j, what, |t: &mut Self, k, v| {
            match k {
                "maxNumCharsPerPage" => t.max_characters_per_page = i32_(v, k)?,
                "maxNumPages" => t.max_number_pages = i32_(v, k)?,
                "pages" => t.pages = class_list(v, k, Page::read)?,
                _ => {}
            }
            Ok(())
        })
    }
}

// ACE: Page
/// `authorAccount` is read (absent or null: its initializer `""`).
///
/// Not ACE's (a fix): ACE declared it as a public field, which its
/// deserializer does not bind, so it always kept `""`.
#[derive(Debug, Clone, Default)]
pub struct Page {
    pub author_account: String,
    pub author_id: Option<u32>,
    pub author_name: Option<String>,
    pub ignore_autor_binder: Option<u8>,
    pub page_text: Option<String>,
}

impl Page {
    pub fn read(j: &Json, what: &str) -> R<Option<Self>> {
        class(j, what, |t: &mut Self, k, v| {
            match k {
                "authorAccount" => t.author_account = string(v, k)?.unwrap_or_default(),
                "authorID" => t.author_id = opt(v, k, u32_)?,
                "authorName" => t.author_name = string(v, k)?,
                "ignoreAuthor" => t.ignore_autor_binder = opt(v, k, u8_)?,
                // ACE: Page.PageText (the setter strips '\r', and throws on null)
                "pageText" => {
                    let s = string(v, k)?.ok_or("pageText: the setter throws on null")?;
                    t.page_text = Some(s.replace('\r', ""));
                }
                _ => {}
            }
            Ok(())
        })
    }

    /// ACE: Page.IgnoreAuthor
    #[must_use]
    pub fn ignore_author(&self) -> Option<bool> {
        self.ignore_autor_binder.map(|b| b != 0)
    }
}

macro_rules! key_value_stat {
    ($(#[$doc:meta])* $name:ident, $value:ty, $read:expr) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Default)]
        pub struct $name {
            pub key: i32,
            pub value: $value,
        }

        impl $name {
            pub fn read(j: &Json, what: &str) -> R<Option<Self>> {
                class(j, what, |t: &mut Self, k, v| {
                    match k {
                        "key" => t.key = i32_(v, k)?,
                        "value" => t.value = $read(v, k)?,
                        _ => {}
                    }
                    Ok(())
                })
            }
        }
    };
}

key_value_stat!(
    /// ACE: BoolStat
    BoolStat, i32, i32_
);
// ACE: IntStat
/// `MultiSelectRaw` is a `JsonIgnore`d auto-property the `MultiSelect` setter fills.
#[derive(Debug, Clone, Default)]
pub struct IntStat {
    pub key: i32,
    pub value: i32,
    pub multi_select_raw: Option<Vec<String>>,
}

impl IntStat {
    pub fn read(j: &Json, what: &str) -> R<Option<Self>> {
        class(j, what, |t: &mut Self, k, v| {
            match k {
                "key" => t.key = i32_(v, k)?,
                "value" => t.value = i32_(v, k)?,
                _ => {}
            }
            Ok(())
        })
    }
}
key_value_stat!(
    /// ACE: DidStat
    DidStat, u32, u32_
);
key_value_stat!(
    /// ACE: IidStat
    IidStat, i32, i32_
);
key_value_stat!(
    /// ACE: FloatStat
    FloatStat, f32, f32_
);
key_value_stat!(
    /// ACE: Int64Stat
    Int64Stat, i64, i64_
);
key_value_stat!(
    /// ACE: StringStat
    StringStat, Option<String>, string
);

// ACE: CreateItem
#[derive(Debug, Clone, Default)]
pub struct CreateItem {
    pub weenie_class_id: Option<u32>,
    pub palette: Option<u32>,
    pub shade: Option<f64>,
    pub destination: Option<u32>,
    pub stack_size: Option<i32>,
    pub try_to_bond: Option<u8>,
}

impl CreateItem {
    pub fn read(j: &Json, what: &str) -> R<Option<Self>> {
        class(j, what, |t: &mut Self, k, v| {
            match k {
                "wcid" => t.weenie_class_id = opt(v, k, u32_)?,
                "palette" => t.palette = opt(v, k, u32_)?,
                "shade" => t.shade = opt(v, k, f64_)?,
                "destination" => t.destination = opt(v, k, u32_)?,
                "stack_size" => t.stack_size = opt(v, k, i32_)?,
                "try_to_bond" => t.try_to_bond = opt(v, k, u8_)?,
                _ => {}
            }
            Ok(())
        })
    }
}

// ACE: SkillListing
#[derive(Debug, Clone, Default)]
pub struct SkillListing {
    pub skill_id: Option<i32>,
    pub skill: Option<Skill>,
}

impl SkillListing {
    pub fn read(j: &Json, what: &str) -> R<Option<Self>> {
        class(j, what, |t: &mut Self, k, v| {
            match k {
                "key" => t.skill_id = opt(v, k, i32_)?,
                "value" => t.skill = Skill::read(v, k)?,
                _ => {}
            }
            Ok(())
        })
    }
}

// ACE: Skill
#[derive(Debug, Clone)]
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

impl Skill {
    pub fn read(j: &Json, what: &str) -> R<Option<Self>> {
        class(j, what, |t: &mut Self, k, v| {
            match k {
                "level_from_pp" => t.level_from_pp = opt(v, k, u32_)?,
                "last_used_time" => t.last_used = opt(v, k, f32_)?,
                "init_level" => t.ranks = opt(v, k, u32_)?,
                "pp" => t.xp_invested = opt(v, k, u32_)?,
                "resistance_of_last_check" => t.resistance_of_last_check = opt(v, k, u32_)?,
                "sac" => t.trained_level = opt(v, k, i32_)?,
                _ => {}
            }
            Ok(())
        })
    }
}

// ACE: EmoteCategoryListing
#[derive(Debug, Clone)]
pub struct EmoteCategoryListing {
    pub emote_category_id: i32,
    pub emotes: Option<Vec<Option<Emote>>>,
}

impl Default for EmoteCategoryListing {
    fn default() -> Self {
        Self {
            emote_category_id: 0,
            emotes: Some(Vec::new()),
        }
    }
}

impl EmoteCategoryListing {
    pub fn read(j: &Json, what: &str) -> R<Option<Self>> {
        class(j, what, |t: &mut Self, k, v| {
            match k {
                "key" => t.emote_category_id = i32_(v, k)?,
                "value" => t.emotes = class_list(v, k, Emote::read)?,
                _ => {}
            }
            Ok(())
        })
    }
}

// ACE: Emote
#[derive(Debug, Clone, Default)]
pub struct Emote {
    pub category: u32,
    pub actions: Option<Vec<Option<EmoteAction>>>,
    pub probability: Option<f32>,
    pub vendor_type: Option<u32>,
    pub quest: Option<String>,
    pub class_id: Option<u32>,
    pub style: Option<u32>,
    pub sub_style: Option<u32>,
    pub min_health: Option<f32>,
    pub max_health: Option<f32>,
}

impl Emote {
    pub fn read(j: &Json, what: &str) -> R<Option<Self>> {
        class(j, what, |t: &mut Self, k, v| {
            match k {
                "category" => t.category = u32_(v, k)?,
                "emotes" => t.actions = class_list(v, k, EmoteAction::read)?,
                "probability" => t.probability = opt(v, k, f32_)?,
                "vendorType" => t.vendor_type = opt(v, k, u32_)?,
                "quest" => t.quest = string(v, k)?,
                "classID" => t.class_id = opt(v, k, u32_)?,
                "style" => t.style = opt(v, k, u32_)?,
                "substyle" => t.sub_style = opt(v, k, u32_)?,
                "minhealth" => t.min_health = opt(v, k, f32_)?,
                "maxhealth" => t.max_health = opt(v, k, f32_)?,
                _ => {}
            }
            Ok(())
        })
    }
}

// ACE: EmoteAction
#[derive(Debug, Clone, Default)]
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
    pub fn read(j: &Json, what: &str) -> R<Option<Self>> {
        class(j, what, |t: &mut Self, k, v| {
            match k {
                "type" => t.emote_action_type = u32_(v, k)?,
                "delay" => t.delay = opt(v, k, f32_)?,
                "extent" => t.extent = opt(v, k, f32_)?,
                "amount" => t.amount = opt(v, k, u32_)?,
                "motion" => t.motion = opt(v, k, u32_)?,
                "msg" => t.message = string(v, k)?,
                "amount64" => t.amount64 = opt(v, k, i64_)?,
                "heroxp64" => t.hero_xp64 = opt(v, k, u64_)?,
                "cprof" => t.item = CreateItem::read(v, k)?,
                "min64" => t.minimum64 = opt(v, k, i64_)?,
                "max64" => t.maximum64 = opt(v, k, i64_)?,
                "percent" => t.percent = opt(v, k, f32_)?,
                "display" => t.display_binder = opt(v, k, u8_)?,
                "max" => t.max = opt(v, k, u32_)?,
                "min" => t.min = opt(v, k, u32_)?,
                "fmax" => t.f_max = opt(v, k, f32_)?,
                "fmin" => t.f_min = opt(v, k, f32_)?,
                "stat" => t.stat = opt(v, k, u32_)?,
                "pscript" => t.p_script = opt(v, k, u32_)?,
                "sound" => t.sound = opt(v, k, u32_)?,
                "mPosition" => t.m_position = Position::read(v, k)?,
                "frame" => t.frame = Frame::read(v, k)?,
                "spellid" => t.spell_id = opt(v, k, u32_)?,
                "teststring" => t.test_string = string(v, k)?,
                "wealth_rating" => t.wealth_rating = opt(v, k, u32_)?,
                "treasure_class" => t.treasure_class = opt(v, k, u32_)?,
                "treasure_type" => t.treasure_type = opt(v, k, i32_)?,
                _ => {}
            }
            Ok(())
        })
    }

    /// ACE: EmoteAction.Display
    #[must_use]
    pub fn display(&self) -> Option<bool> {
        self.display_binder.map(|b| b != 0)
    }
}

// ACE: SpellbookEntry
#[derive(Debug, Clone)]
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

impl SpellbookEntry {
    pub fn read(j: &Json, what: &str) -> R<Option<Self>> {
        class(j, what, |t: &mut Self, k, v| {
            match k {
                "key" => t.spell_id = i32_(v, k)?,
                "value" => t.stats = SpellCastingStats::read(v, k)?,
                _ => {}
            }
            Ok(())
        })
    }
}

// ACE: SpellCastingStats
#[derive(Debug, Clone, Default)]
pub struct SpellCastingStats {
    pub casting_chance: Option<f64>,
}

impl SpellCastingStats {
    pub fn read(j: &Json, what: &str) -> R<Option<Self>> {
        class(j, what, |t: &mut Self, k, v| {
            if k == "casting_likelihood" {
                t.casting_chance = opt(v, k, f64_)?;
            }
            Ok(())
        })
    }
}

// ACE: PositionListing
#[derive(Debug, Clone)]
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

impl PositionListing {
    pub fn read(j: &Json, what: &str) -> R<Option<Self>> {
        class(j, what, |t: &mut Self, k, v| {
            match k {
                "key" => t.position_type = i32_(v, k)?,
                "value" => t.position = Position::read(v, k)?,
                _ => {}
            }
            Ok(())
        })
    }
}

// ACE: Position
#[derive(Debug, Clone, Default)]
pub struct Position {
    pub frame: Option<Frame>,
    pub land_cell_id: u32,
}

impl Position {
    pub fn read(j: &Json, what: &str) -> R<Option<Self>> {
        class(j, what, |t: &mut Self, k, v| {
            match k {
                "frame" => t.frame = Frame::read(v, k)?,
                "objcell_id" => t.land_cell_id = u32_(v, k)?,
                _ => {}
            }
            Ok(())
        })
    }
}

// ACE: Frame
#[derive(Debug, Clone, Default)]
pub struct Frame {
    pub position: Option<Xyz>,
    pub rotations: Option<Quaternion>,
}

impl Frame {
    pub fn read(j: &Json, what: &str) -> R<Option<Self>> {
        class(j, what, |t: &mut Self, k, v| {
            match k {
                "origin" => t.position = Xyz::read(v, k)?,
                "angles" => t.rotations = Quaternion::read(v, k)?,
                _ => {}
            }
            Ok(())
        })
    }

    /// `frame.Position` and `frame.Rotations`, failing where C# would dereference null.
    pub fn parts(&self) -> R<(&Xyz, &Quaternion)> {
        Ok((
            self.position.as_ref().ok_or("frame.origin is null")?,
            self.rotations.as_ref().ok_or("frame.angles is null")?,
        ))
    }
}

// ACE: XYZ
#[derive(Debug, Clone, Default)]
pub struct Xyz {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Xyz {
    pub fn read(j: &Json, what: &str) -> R<Option<Self>> {
        class(j, what, |t: &mut Self, k, v| {
            match k {
                "x" => t.x = f32_(v, k)?,
                "y" => t.y = f32_(v, k)?,
                "z" => t.z = f32_(v, k)?,
                _ => {}
            }
            Ok(())
        })
    }
}

// ACE: Quaternion
#[derive(Debug, Clone)]
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

impl Quaternion {
    pub fn read(j: &Json, what: &str) -> R<Option<Self>> {
        class(j, what, |t: &mut Self, k, v| {
            match k {
                "w" => t.w = f32_(v, k)?,
                "x" => t.x = f32_(v, k)?,
                "y" => t.y = f32_(v, k)?,
                "z" => t.z = f32_(v, k)?,
                _ => {}
            }
            Ok(())
        })
    }
}

// ACE: GeneratorTable
#[derive(Debug, Clone)]
pub struct GeneratorTable {
    pub delay: f32,
    pub frame: Option<Frame>,
    pub init_create: u32,
    pub max_number: u32,
    pub object_cell: u32,
    pub probability: f64,
    pub palette_id: u32,
    pub shade: f32,
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
            stack_size: 0,
            weenie_class_id: 0,
            when_create: 0,
            where_create: 0,
        }
    }
}

impl GeneratorTable {
    pub fn read(j: &Json, what: &str) -> R<Option<Self>> {
        class(j, what, |t: &mut Self, k, v| {
            match k {
                "delay" => t.delay = f32_(v, k)?,
                "frame" => t.frame = Frame::read(v, k)?,
                "initCreate" => t.init_create = u32_(v, k)?,
                "maxNum" => t.max_number = u32_(v, k)?,
                "objcell_id" => t.object_cell = u32_(v, k)?,
                "probability" => t.probability = f64_(v, k)?,
                "ptid" => t.palette_id = u32_(v, k)?,
                "shade" => t.shade = f32_(v, k)?,
                "slot" => {
                    u32_(v, k)?;
                }
                "stackSize" => t.stack_size = i32_(v, k)?,
                "type" => t.weenie_class_id = u32_(v, k)?,
                "whenCreate" => t.when_create = u32_(v, k)?,
                "whereCreate" => t.where_create = u32_(v, k)?,
                _ => {}
            }
            Ok(())
        })
    }
}

// ---- GDLE recipe -----------------------------------------------------------------------------

// ACE: RecipeCombined
#[derive(Debug, Clone, Default)]
pub struct RecipeCombined {
    pub key: u32,
    pub desc: Option<String>,
    pub recipe: Option<Recipe>,
    pub precursors: Option<Vec<Option<RecipePrecursor>>>,
}

impl RecipeCombined {
    pub fn read(j: &Json) -> R<Option<Self>> {
        class(j, "RecipeCombined", |t: &mut Self, k, v| {
            match k {
                "key" => t.key = u32_(v, k)?,
                "desc" => t.desc = string(v, k)?,
                "recipe" => t.recipe = Recipe::read(v, k)?,
                "precursors" => t.precursors = class_list(v, k, RecipePrecursor::read)?,
                _ => {}
            }
            Ok(())
        })
    }
}

// ACE: RecipePrecursor
#[derive(Debug, Clone, Default)]
pub struct RecipePrecursor {
    pub tool: u32,
    pub target: u32,
    pub recipe_id: Option<u32>,
}

impl RecipePrecursor {
    pub fn read(j: &Json, what: &str) -> R<Option<Self>> {
        class(j, what, |t: &mut Self, k, v| {
            match k {
                "Tool" => t.tool = u32_(v, k)?,
                "Target" => t.target = u32_(v, k)?,
                "RecipeId" => t.recipe_id = opt(v, k, u32_)?,
                _ => {}
            }
            Ok(())
        })
    }
}

// ACE: Recipe (GDLE)
#[derive(Debug, Clone, Default)]
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
    pub mods: Option<Vec<Option<Mod>>>,
    pub requirements: Option<Vec<Option<RecipeRequirements>>>,
}

impl Recipe {
    pub fn read(j: &Json, what: &str) -> R<Option<Self>> {
        class(j, what, |t: &mut Self, k, v| {
            match k {
                "RecipeID" => t.recipe_id = u32_(v, k)?,
                "Skill" => t.skill = u32_(v, k)?,
                "SkillCheckFormulaType" => t.skill_check_formula_type = i32_(v, k)?,
                "DataID" => t.data_id = u32_(v, k)?,
                "Difficulty" => t.difficulty = u32_(v, k)?,
                "SuccessWcid" => t.success_wcid = u32_(v, k)?,
                "SuccessAmount" => t.success_amount = u32_(v, k)?,
                "SuccessMessage" => t.success_message = string(v, k)?,
                "SuccessConsumeTargetAmount" => t.success_consume_target_amount = u32_(v, k)?,
                "SuccessConsumeTargetChance" => t.success_consume_target_chance = f64_(v, k)?,
                "SuccessConsumeTargetMessage" => t.success_consume_target_message = string(v, k)?,
                "SuccessConsumeToolAmount" => t.success_consume_tool_amount = u32_(v, k)?,
                "SuccessConsumeToolChance" => t.success_consume_tool_chance = f64_(v, k)?,
                "SuccessConsumeToolMessage" => t.success_consume_tool_message = string(v, k)?,
                "FailWcid" => t.fail_wcid = u32_(v, k)?,
                "FailAmount" => t.fail_amount = u32_(v, k)?,
                "FailMessage" => t.fail_message = string(v, k)?,
                "FailureConsumeTargetAmount" => t.failure_consume_target_amount = u32_(v, k)?,
                "FailureConsumeTargetChance" => t.failure_consume_target_chance = f64_(v, k)?,
                "FailureConsumeTargetMessage" => t.failure_consume_target_message = string(v, k)?,
                "FailureConsumeToolAmount" => t.failure_consume_tool_amount = u32_(v, k)?,
                "FailureConsumeToolChance" => t.failure_consume_tool_chance = f64_(v, k)?,
                "FailureConsumeToolMessage" => t.failure_consume_tool_message = string(v, k)?,
                "Unknown" => t.unknown = i32_(v, k)?,
                "Mods" => t.mods = class_list(v, k, Mod::read)?,
                "Requirements" => t.requirements = class_list(v, k, RecipeRequirements::read)?,
                _ => {}
            }
            Ok(())
        })
    }
}

/// The six typed requirement lists shared by `RecipeRequirements` and `Mod`.
#[derive(Debug, Clone)]
pub struct Requirements {
    pub int_requirements: Option<Vec<Option<Requirement<i32>>>>,
    pub did_requirements: Option<Vec<Option<Requirement<u32>>>>,
    pub iid_requirements: Option<Vec<Option<Requirement<u32>>>>,
    pub float_requirements: Option<Vec<Option<Requirement<f64>>>>,
    pub string_requirements: Option<Vec<Option<StringRequirement>>>,
    pub bool_requirements: Option<Vec<Option<Requirement<bool>>>>,
}

impl Default for Requirements {
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

impl Requirements {
    /// One of the six list properties; `false` if `k` is not one of them.
    fn set(&mut self, k: &str, v: &Json) -> R<bool> {
        match k {
            "IntRequirements" => {
                self.int_requirements = list(v, k, |e, w| Requirement::read(e, w, i32_))?
            }
            "DIDRequirements" => {
                self.did_requirements = list(v, k, |e, w| Requirement::read(e, w, u32_))?
            }
            "IIDRequirements" => {
                self.iid_requirements = list(v, k, |e, w| Requirement::read(e, w, u32_))?
            }
            "FloatRequirements" => {
                self.float_requirements = list(v, k, |e, w| Requirement::read(e, w, f64_))?
            }
            "StringRequirements" => {
                self.string_requirements = class_list(v, k, StringRequirement::read)?
            }
            "BoolRequirements" => {
                self.bool_requirements = list(v, k, |e, w| Requirement::read(e, w, bool_))?
            }
            _ => return Ok(false),
        }
        Ok(true)
    }
}

// ACE: RecipeRequirements
#[derive(Debug, Clone, Default)]
pub struct RecipeRequirements {
    pub lists: Requirements,
}

impl RecipeRequirements {
    pub fn read(j: &Json, what: &str) -> R<Option<Self>> {
        class(j, what, |t: &mut Self, k, v| t.lists.set(k, v).map(drop))
    }
}

// ACE: Mod
#[derive(Debug, Clone, Default)]
pub struct Mod {
    pub lists: Requirements,
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

impl Mod {
    pub fn read(j: &Json, what: &str) -> R<Option<Self>> {
        class(j, what, |t: &mut Self, k, v| {
            if t.lists.set(k, v)? {
                return Ok(());
            }
            match k {
                "ModifyHealth" => t.modify_health = i32_(v, k)?,
                "ModifyStamina" => t.modify_stamina = i32_(v, k)?,
                "ModifyMana" => t.modify_mana = i32_(v, k)?,
                "RequiresHealth" => t.requires_health = i32_(v, k)?,
                "RequiresStamina" => t.requires_stamina = i32_(v, k)?,
                "RequiresMana" => t.requires_mana = i32_(v, k)?,
                "Unknown7" => t.unknown7 = bool_(v, k)?,
                "ModificationScriptId" => t.modification_script_id = i32_(v, k)?,
                "Unknown9" => t.unknown9 = i32_(v, k)?,
                "Unknown10" => t.unknown10 = i32_(v, k)?,
                _ => {}
            }
            Ok(())
        })
    }
}

/// ACE: IntRequirement, DIDRequirement, IIDRequirement, FloatRequirement, BoolRequirement
/// (identical but for the type of `Value`).
#[derive(Debug, Clone, Default)]
pub struct Requirement<V> {
    pub unknown: Option<i32>,
    pub operation_type: i32,
    pub message: Option<String>,
    pub stat: i32,
    pub value: V,
}

impl<V: Default> Requirement<V> {
    pub(crate) fn read(j: &Json, what: &str, value: fn(&Json, &str) -> R<V>) -> R<Option<Self>> {
        class(j, what, |t: &mut Self, k, v| {
            match k {
                "Unknown" => t.unknown = opt(v, k, i32_)?,
                "OperationType" => t.operation_type = i32_(v, k)?,
                "Message" => t.message = string(v, k)?,
                "Stat" => t.stat = i32_(v, k)?,
                "Value" => t.value = value(v, k)?,
                _ => {}
            }
            Ok(())
        })
    }
}

// ACE: StringRequirement
/// Not ACE's (a fix): its `Message` is read, as the other
/// requirements' are; ACE's has no such member, so an import lost it.
#[derive(Debug, Clone, Default)]
pub struct StringRequirement {
    pub unknown: i32,
    pub operation_type: i32,
    pub message: Option<String>,
    pub stat: i32,
    pub value: Option<String>,
}

impl StringRequirement {
    pub fn read(j: &Json, what: &str) -> R<Option<Self>> {
        class(j, what, |t: &mut Self, k, v| {
            match k {
                "Unknown" => t.unknown = i32_(v, k)?,
                "OperationType" => t.operation_type = i32_(v, k)?,
                "Message" => t.message = string(v, k)?,
                "Stat" => t.stat = i32_(v, k)?,
                "Value" => t.value = string(v, k)?,
                _ => {}
            }
            Ok(())
        })
    }
}

// ---- GDLE landblock and quest ----------------------------------------------------------------

// ACE: Landblock (GDLE)
#[derive(Debug, Clone, Default)]
pub struct Landblock {
    pub key: u32,
    pub value: Option<LandblockValue>,
    pub desc: Option<String>,
}

impl Landblock {
    pub fn read(j: &Json) -> R<Option<Self>> {
        class(j, "Landblock", |t: &mut Self, k, v| {
            match k {
                "key" => t.key = u32_(v, k)?,
                "value" => t.value = LandblockValue::read(v, k)?,
                "desc" => t.desc = string(v, k)?,
                _ => {}
            }
            Ok(())
        })
    }
}

// ACE: LandblockValue
#[derive(Debug, Clone, Default)]
pub struct LandblockValue {
    pub links: Option<Vec<Option<LandblockLink>>>,
    pub weenies: Option<Vec<Option<LandblockWeenie>>>,
}

impl LandblockValue {
    pub fn read(j: &Json, what: &str) -> R<Option<Self>> {
        class(j, what, |t: &mut Self, k, v| {
            match k {
                "links" => t.links = class_list(v, k, LandblockLink::read)?,
                "weenies" => t.weenies = class_list(v, k, LandblockWeenie::read)?,
                _ => {}
            }
            Ok(())
        })
    }
}

// ACE: LandblockWeenie
#[derive(Debug, Clone, Default)]
pub struct LandblockWeenie {
    pub id: u32,
    pub wcid: u32,
    pub pos: Option<Position>,
    pub desc: Option<String>,
}

impl LandblockWeenie {
    pub fn read(j: &Json, what: &str) -> R<Option<Self>> {
        class(j, what, |t: &mut Self, k, v| {
            match k {
                "id" => t.id = u32_(v, k)?,
                "wcid" => t.wcid = u32_(v, k)?,
                "desc" => t.desc = string(v, k)?,
                "pos" => t.pos = Position::read(v, k)?,
                _ => {}
            }
            Ok(())
        })
    }
}

// ACE: LandblockLink
#[derive(Debug, Clone, Default)]
pub struct LandblockLink {
    pub target: u32,
    pub source: u32,
    pub desc: Option<String>,
}

impl LandblockLink {
    pub fn read(j: &Json, what: &str) -> R<Option<Self>> {
        class(j, what, |t: &mut Self, k, v| {
            match k {
                "target" => t.target = u32_(v, k)?,
                "source" => t.source = u32_(v, k)?,
                "desc" => t.desc = string(v, k)?,
                _ => {}
            }
            Ok(())
        })
    }
}

// ACE: Quest (GDLE)
#[derive(Debug, Clone, Default)]
pub struct Quest {
    pub key: Option<String>,
    pub value: Option<QuestValue>,
}

impl Quest {
    pub fn read(j: &Json) -> R<Option<Self>> {
        class(j, "Quest", |t: &mut Self, k, v| {
            match k {
                "key" => t.key = string(v, k)?,
                "value" => t.value = QuestValue::read(v, k)?,
                _ => {}
            }
            Ok(())
        })
    }
}

// ACE: QuestValue
#[derive(Debug, Clone, Default)]
pub struct QuestValue {
    pub fullname: Option<String>,
    pub maxsolves: i32,
    pub mindelta: i32,
}

impl QuestValue {
    pub fn read(j: &Json, what: &str) -> R<Option<Self>> {
        class(j, what, |t: &mut Self, k, v| {
            match k {
                "fullname" => t.fullname = string(v, k)?,
                "maxsolves" => t.maxsolves = i32_(v, k)?,
                "mindelta" => t.mindelta = i32_(v, k)?,
                _ => {}
            }
            Ok(())
        })
    }
}
