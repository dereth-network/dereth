// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/World/*.cs
//! The World models as the JSON converters fill them and the SQL writers read them: only the
//! columns the writers print, with C#'s nullability (a C# `string` may be null here, unlike in
//! [`crate::models::world`], which holds rows as the database returns them).

use empyrean_common::dotnet::DotNetDateTime;

// ACE: Weenie
#[derive(Debug, Clone, Default)]
pub struct Weenie {
    pub class_id: u32,
    pub class_name: String,
    pub r#type: i32,
    pub last_modified: DotNetDateTime,
    pub attribute: Vec<Attribute>,
    pub attribute_2nd: Vec<Attribute2nd>,
    pub body_part: Vec<BodyPart>,
    pub book: Option<Book>,
    pub book_page_data: Vec<BookPageData>,
    pub bool_: Vec<Prop<bool>>,
    pub create_list: Vec<CreateList>,
    pub did: Vec<Prop<u32>>,
    pub emote: Vec<Emote>,
    pub float: Vec<Prop<f64>>,
    pub generator: Vec<Generator>,
    pub iid: Vec<Prop<u32>>,
    pub int: Vec<Prop<i32>>,
    pub int64: Vec<Prop<i64>>,
    pub position: Vec<Position>,
    pub skill: Vec<Skill>,
    pub spell_book: Vec<SpellBook>,
    pub string: Vec<Prop<Option<String>>>,
}

/// ACE: WeeniePropertiesInt, WeeniePropertiesInt64, WeeniePropertiesBool, WeeniePropertiesFloat,
/// WeeniePropertiesString, WeeniePropertiesDID, WeeniePropertiesIID (`Type`, `Value`).
#[derive(Debug, Clone)]
pub struct Prop<T> {
    pub r#type: u16,
    pub value: T,
}

// ACE: WeeniePropertiesAttribute
#[derive(Debug, Clone)]
pub struct Attribute {
    pub r#type: u16,
    pub init_level: u32,
    pub level_from_cp: u32,
    pub cp_spent: u32,
}

// ACE: WeeniePropertiesAttribute2nd
#[derive(Debug, Clone)]
pub struct Attribute2nd {
    pub r#type: u16,
    pub init_level: u32,
    pub level_from_cp: u32,
    pub cp_spent: u32,
    pub current_level: u32,
}

// ACE: WeeniePropertiesBodyPart
#[derive(Debug, Clone)]
pub struct BodyPart {
    pub key: u16,
    pub d_type: i32,
    pub d_val: i32,
    pub d_var: f32,
    pub base_armor: i32,
    pub armor_vs_slash: i32,
    pub armor_vs_pierce: i32,
    pub armor_vs_bludgeon: i32,
    pub armor_vs_cold: i32,
    pub armor_vs_fire: i32,
    pub armor_vs_acid: i32,
    pub armor_vs_electric: i32,
    pub armor_vs_nether: i32,
    pub bh: i32,
    pub zones: [f32; 12],
}

// ACE: WeeniePropertiesBook
#[derive(Debug, Clone)]
pub struct Book {
    pub max_num_pages: i32,
    pub max_num_chars_per_page: i32,
}

// ACE: WeeniePropertiesBookPageData
#[derive(Debug, Clone)]
pub struct BookPageData {
    pub page_id: u32,
    pub author_id: u32,
    pub author_name: Option<String>,
    pub author_account: Option<String>,
    pub ignore_author: bool,
    pub page_text: Option<String>,
}

// ACE: WeeniePropertiesCreateList
#[derive(Debug, Clone)]
pub struct CreateList {
    pub destination_type: i8,
    pub weenie_class_id: u32,
    pub stack_size: i32,
    pub palette: i8,
    pub shade: f32,
    pub try_to_bond: bool,
}

// ACE: WeeniePropertiesEmote
#[derive(Debug, Clone)]
pub struct Emote {
    pub category: u32,
    pub probability: f32,
    pub weenie_class_id: Option<u32>,
    pub style: Option<u32>,
    pub substyle: Option<u32>,
    pub quest: Option<String>,
    pub vendor_type: Option<i32>,
    pub min_health: Option<f32>,
    pub max_health: Option<f32>,
    pub actions: Vec<EmoteAction>,
}

// ACE: WeeniePropertiesEmoteAction
#[derive(Debug, Clone, Default)]
pub struct EmoteAction {
    pub order: u32,
    pub r#type: u32,
    pub delay: f32,
    pub extent: f32,
    pub motion: Option<u32>,
    pub message: Option<String>,
    pub test_string: Option<String>,
    pub min: Option<i32>,
    pub max: Option<i32>,
    pub min_64: Option<i64>,
    pub max_64: Option<i64>,
    pub min_dbl: Option<f64>,
    pub max_dbl: Option<f64>,
    pub stat: Option<i32>,
    pub display: Option<bool>,
    pub amount: Option<i32>,
    pub amount_64: Option<i64>,
    pub hero_xp_64: Option<i64>,
    pub percent: Option<f64>,
    pub spell_id: Option<i32>,
    pub wealth_rating: Option<i32>,
    pub treasure_class: Option<i32>,
    pub treasure_type: Option<i32>,
    pub p_script: Option<i32>,
    pub sound: Option<i32>,
    pub destination_type: Option<i8>,
    pub weenie_class_id: Option<u32>,
    pub stack_size: Option<i32>,
    pub palette: Option<i32>,
    pub shade: Option<f32>,
    pub try_to_bond: Option<bool>,
    pub obj_cell_id: Option<u32>,
    pub origin_x: Option<f32>,
    pub origin_y: Option<f32>,
    pub origin_z: Option<f32>,
    pub angles_w: Option<f32>,
    pub angles_x: Option<f32>,
    pub angles_y: Option<f32>,
    pub angles_z: Option<f32>,
}

// ACE: WeeniePropertiesGenerator
#[derive(Debug, Clone)]
pub struct Generator {
    pub probability: f32,
    pub weenie_class_id: u32,
    pub delay: Option<f32>,
    pub init_create: i32,
    pub max_create: i32,
    pub when_create: u32,
    pub where_create: u32,
    pub stack_size: Option<i32>,
    pub palette_id: Option<u32>,
    pub shade: Option<f32>,
    pub obj_cell_id: Option<u32>,
    pub origin_x: Option<f32>,
    pub origin_y: Option<f32>,
    pub origin_z: Option<f32>,
    pub angles_w: Option<f32>,
    pub angles_x: Option<f32>,
    pub angles_y: Option<f32>,
    pub angles_z: Option<f32>,
}

// ACE: WeeniePropertiesPosition
#[derive(Debug, Clone)]
pub struct Position {
    pub position_type: u16,
    pub obj_cell_id: u32,
    pub origin_x: f32,
    pub origin_y: f32,
    pub origin_z: f32,
    pub angles_w: f32,
    pub angles_x: f32,
    pub angles_y: f32,
    pub angles_z: f32,
}

// ACE: WeeniePropertiesSkill
#[derive(Debug, Clone)]
pub struct Skill {
    pub r#type: u16,
    pub level_from_pp: u16,
    pub sac: u32,
    pub pp: u32,
    pub init_level: u32,
    pub resistance_at_last_check: u32,
    pub last_used_time: f64,
}

// ACE: WeeniePropertiesSpellBook
#[derive(Debug, Clone)]
pub struct SpellBook {
    pub spell: i32,
    pub probability: f32,
}

// ACE: Recipe
#[derive(Debug, Clone, Default)]
pub struct Recipe {
    pub id: u32,
    pub unknown_1: u32,
    pub skill: u32,
    pub difficulty: u32,
    pub salvage_type: u32,
    pub success_wcid: u32,
    pub success_amount: u32,
    pub success_message: Option<String>,
    pub fail_wcid: u32,
    pub fail_amount: u32,
    pub fail_message: Option<String>,
    pub success_destroy_source_chance: f64,
    pub success_destroy_source_amount: u32,
    pub success_destroy_source_message: Option<String>,
    pub success_destroy_target_chance: f64,
    pub success_destroy_target_amount: u32,
    pub success_destroy_target_message: Option<String>,
    pub fail_destroy_source_chance: f64,
    pub fail_destroy_source_amount: u32,
    pub fail_destroy_source_message: Option<String>,
    pub fail_destroy_target_chance: f64,
    pub fail_destroy_target_amount: u32,
    pub fail_destroy_target_message: Option<String>,
    pub data_id: u32,
    pub last_modified: DotNetDateTime,
    pub requirements_int: Vec<RecipeRow<i32>>,
    pub requirements_did: Vec<RecipeRow<u32>>,
    pub requirements_iid: Vec<RecipeRow<u32>>,
    pub requirements_float: Vec<RecipeRow<f64>>,
    pub requirements_string: Vec<RecipeRow<Option<String>>>,
    pub requirements_bool: Vec<RecipeRow<bool>>,
    pub recipe_mod: Vec<RecipeMod>,
}

/// ACE: RecipeRequirementsInt/DID/IID/Float/String/Bool (`message`) and RecipeModsInt/DID/IID/
/// Float/String/Bool (`source`): `Index`, `Stat`, `Value`, `Enum` and the table's last column.
#[derive(Debug, Clone)]
pub struct RecipeRow<V> {
    pub index: i8,
    pub stat: i32,
    pub value: V,
    pub r#enum: i32,
    pub message: Option<String>,
    pub source: i32,
}

// ACE: RecipeMod
#[derive(Debug, Clone, Default)]
pub struct RecipeMod {
    pub executes_on_success: bool,
    pub health: i32,
    pub stamina: i32,
    pub mana: i32,
    pub unknown_7: bool,
    pub data_id: i32,
    pub unknown_9: i32,
    pub instance_id: i32,
    pub mods_int: Vec<RecipeRow<i32>>,
    pub mods_did: Vec<RecipeRow<u32>>,
    pub mods_iid: Vec<RecipeRow<u32>>,
    pub mods_float: Vec<RecipeRow<f64>>,
    pub mods_string: Vec<RecipeRow<Option<String>>>,
    pub mods_bool: Vec<RecipeRow<bool>>,
}

// ACE: CookBook
#[derive(Debug, Clone)]
pub struct CookBook {
    pub recipe_id: u32,
    pub source_wcid: u32,
    pub target_wcid: u32,
    pub last_modified: DotNetDateTime,
}

// ACE: LandblockInstance
#[derive(Debug, Clone)]
pub struct LandblockInstance {
    pub guid: u32,
    pub weenie_class_id: u32,
    pub obj_cell_id: u32,
    pub origin_x: f32,
    pub origin_y: f32,
    pub origin_z: f32,
    pub angles_w: f32,
    pub angles_x: f32,
    pub angles_y: f32,
    pub angles_z: f32,
    pub is_link_child: bool,
    pub last_modified: DotNetDateTime,
    /// Indexes into the converter's link list (C# shares the link objects between the list and
    /// each parent's collection).
    pub links: Vec<usize>,
}

// ACE: LandblockInstanceLink
#[derive(Debug, Clone)]
pub struct LandblockInstanceLink {
    pub parent_guid: u32,
    pub child_guid: u32,
    pub last_modified: DotNetDateTime,
}

// ACE: Quest
#[derive(Debug, Clone)]
pub struct Quest {
    pub name: Option<String>,
    pub min_delta: u32,
    pub max_solves: i32,
    pub message: Option<String>,
    pub last_modified: DotNetDateTime,
}
