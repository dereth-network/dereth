//! Codec and row mapping for every World-DB model. Written once from ACE's model files and the
//! dump's `CREATE TABLE`s; keep in step with `crate::models::world` by hand.

use crate::models::world::*;

crate::impl_codec!(CookBook {
    id,
    recipe_id,
    source_wcid,
    target_wcid,
    last_modified,
    recipe
});
crate::impl_from_row!(CookBook, "cook_book" { id: "id", recipe_id: "recipe_Id", source_wcid: "source_W_C_I_D", target_wcid: "target_W_C_I_D", last_modified: "last_Modified" });

crate::impl_codec!(Encounter {
    id,
    landblock,
    weenie_class_id,
    cell_x,
    cell_y,
    last_modified
});
crate::impl_from_row!(Encounter, "encounter" { id: "id", landblock: "landblock", weenie_class_id: "weenie_Class_Id", cell_x: "cell_X", cell_y: "cell_Y", last_modified: "last_Modified" });

crate::impl_codec!(Event {
    id,
    name,
    start_time,
    end_time,
    state,
    last_modified
});
crate::impl_from_row!(Event, "event" { id: "id", name: "name", start_time: "start_Time", end_time: "end_Time", state: "state", last_modified: "last_Modified" });

crate::impl_codec!(HousePortal {
    id,
    house_id,
    obj_cell_id,
    origin_x,
    origin_y,
    origin_z,
    angles_w,
    angles_x,
    angles_y,
    angles_z,
    last_modified
});
crate::impl_from_row!(HousePortal, "house_portal" { id: "id", house_id: "house_Id", obj_cell_id: "obj_Cell_Id", origin_x: "origin_X", origin_y: "origin_Y", origin_z: "origin_Z", angles_w: "angles_W", angles_x: "angles_X", angles_y: "angles_Y", angles_z: "angles_Z", last_modified: "last_Modified" });

crate::impl_codec!(LandblockInstance {
    guid,
    landblock,
    weenie_class_id,
    obj_cell_id,
    origin_x,
    origin_y,
    origin_z,
    angles_w,
    angles_x,
    angles_y,
    angles_z,
    is_link_child,
    last_modified,
    landblock_instance_link
});
crate::impl_from_row!(LandblockInstance, "landblock_instance" { guid: "guid", weenie_class_id: "weenie_Class_Id", obj_cell_id: "obj_Cell_Id", origin_x: "origin_X", origin_y: "origin_Y", origin_z: "origin_Z", angles_w: "angles_W", angles_x: "angles_X", angles_y: "angles_Y", angles_z: "angles_Z", is_link_child: "is_Link_Child", last_modified: "last_Modified" });

crate::impl_codec!(LandblockInstanceLink {
    id,
    parent_guid,
    child_guid,
    last_modified
});
crate::impl_from_row!(LandblockInstanceLink, "landblock_instance_link" { id: "id", parent_guid: "parent_GUID", child_guid: "child_GUID", last_modified: "last_Modified" });

crate::impl_codec!(PointsOfInterest {
    id,
    name,
    weenie_class_id,
    last_modified
});
crate::impl_from_row!(PointsOfInterest, "points_of_interest" { id: "id", name: "name", weenie_class_id: "weenie_Class_Id", last_modified: "last_Modified" });

crate::impl_codec!(Quest {
    id,
    name,
    min_delta,
    max_solves,
    message,
    last_modified
});
crate::impl_from_row!(Quest, "quest" { id: "id", name: "name", min_delta: "min_Delta", max_solves: "max_Solves", message: "message", last_modified: "last_Modified" });

crate::impl_codec!(Recipe {
    id,
    unknown_1,
    skill,
    difficulty,
    salvage_type,
    success_wcid,
    success_amount,
    success_message,
    fail_wcid,
    fail_amount,
    fail_message,
    success_destroy_source_chance,
    success_destroy_source_amount,
    success_destroy_source_message,
    success_destroy_target_chance,
    success_destroy_target_amount,
    success_destroy_target_message,
    fail_destroy_source_chance,
    fail_destroy_source_amount,
    fail_destroy_source_message,
    fail_destroy_target_chance,
    fail_destroy_target_amount,
    fail_destroy_target_message,
    data_id,
    last_modified,
    recipe_mod,
    recipe_requirements_bool,
    recipe_requirements_did,
    recipe_requirements_float,
    recipe_requirements_iid,
    recipe_requirements_int,
    recipe_requirements_string
});
crate::impl_from_row!(Recipe, "recipe" { id: "id", unknown_1: "unknown_1", skill: "skill", difficulty: "difficulty", salvage_type: "salvage_Type", success_wcid: "success_W_C_I_D", success_amount: "success_Amount", success_message: "success_Message", fail_wcid: "fail_W_C_I_D", fail_amount: "fail_Amount", fail_message: "fail_Message", success_destroy_source_chance: "success_Destroy_Source_Chance", success_destroy_source_amount: "success_Destroy_Source_Amount", success_destroy_source_message: "success_Destroy_Source_Message", success_destroy_target_chance: "success_Destroy_Target_Chance", success_destroy_target_amount: "success_Destroy_Target_Amount", success_destroy_target_message: "success_Destroy_Target_Message", fail_destroy_source_chance: "fail_Destroy_Source_Chance", fail_destroy_source_amount: "fail_Destroy_Source_Amount", fail_destroy_source_message: "fail_Destroy_Source_Message", fail_destroy_target_chance: "fail_Destroy_Target_Chance", fail_destroy_target_amount: "fail_Destroy_Target_Amount", fail_destroy_target_message: "fail_Destroy_Target_Message", data_id: "data_Id", last_modified: "last_Modified" });

crate::impl_codec!(RecipeMod {
    id,
    recipe_id,
    executes_on_success,
    health,
    stamina,
    mana,
    unknown_7,
    data_id,
    unknown_9,
    instance_id,
    recipe_mods_bool,
    recipe_mods_did,
    recipe_mods_float,
    recipe_mods_iid,
    recipe_mods_int,
    recipe_mods_string
});
crate::impl_from_row!(RecipeMod, "recipe_mod" { id: "id", recipe_id: "recipe_Id", executes_on_success: "executes_On_Success", health: "health", stamina: "stamina", mana: "mana", unknown_7: "unknown_7", data_id: "data_Id", unknown_9: "unknown_9", instance_id: "instance_Id" });

crate::impl_codec!(RecipeModsBool {
    id,
    recipe_mod_id,
    index,
    stat,
    value,
    r#enum,
    source
});
crate::impl_from_row!(RecipeModsBool, "recipe_mods_bool" { id: "id", recipe_mod_id: "recipe_Mod_Id", index: "index", stat: "stat", value: "value", r#enum: "enum", source: "source" });

crate::impl_codec!(RecipeModsDID {
    id,
    recipe_mod_id,
    index,
    stat,
    value,
    r#enum,
    source
});
crate::impl_from_row!(RecipeModsDID, "recipe_mods_d_i_d" { id: "id", recipe_mod_id: "recipe_Mod_Id", index: "index", stat: "stat", value: "value", r#enum: "enum", source: "source" });

crate::impl_codec!(RecipeModsFloat {
    id,
    recipe_mod_id,
    index,
    stat,
    value,
    r#enum,
    source
});
crate::impl_from_row!(RecipeModsFloat, "recipe_mods_float" { id: "id", recipe_mod_id: "recipe_Mod_Id", index: "index", stat: "stat", value: "value", r#enum: "enum", source: "source" });

crate::impl_codec!(RecipeModsIID {
    id,
    recipe_mod_id,
    index,
    stat,
    value,
    r#enum,
    source
});
crate::impl_from_row!(RecipeModsIID, "recipe_mods_i_i_d" { id: "id", recipe_mod_id: "recipe_Mod_Id", index: "index", stat: "stat", value: "value", r#enum: "enum", source: "source" });

crate::impl_codec!(RecipeModsInt {
    id,
    recipe_mod_id,
    index,
    stat,
    value,
    r#enum,
    source
});
crate::impl_from_row!(RecipeModsInt, "recipe_mods_int" { id: "id", recipe_mod_id: "recipe_Mod_Id", index: "index", stat: "stat", value: "value", r#enum: "enum", source: "source" });

crate::impl_codec!(RecipeModsString {
    id,
    recipe_mod_id,
    index,
    stat,
    value,
    r#enum,
    source
});
crate::impl_from_row!(RecipeModsString, "recipe_mods_string" { id: "id", recipe_mod_id: "recipe_Mod_Id", index: "index", stat: "stat", value: "value", r#enum: "enum", source: "source" });

crate::impl_codec!(RecipeRequirementsBool {
    id,
    recipe_id,
    index,
    stat,
    value,
    r#enum,
    message
});
crate::impl_from_row!(RecipeRequirementsBool, "recipe_requirements_bool" { id: "id", recipe_id: "recipe_Id", index: "index", stat: "stat", value: "value", r#enum: "enum", message: "message" });

crate::impl_codec!(RecipeRequirementsDID {
    id,
    recipe_id,
    index,
    stat,
    value,
    r#enum,
    message
});
crate::impl_from_row!(RecipeRequirementsDID, "recipe_requirements_d_i_d" { id: "id", recipe_id: "recipe_Id", index: "index", stat: "stat", value: "value", r#enum: "enum", message: "message" });

crate::impl_codec!(RecipeRequirementsFloat {
    id,
    recipe_id,
    index,
    stat,
    value,
    r#enum,
    message
});
crate::impl_from_row!(RecipeRequirementsFloat, "recipe_requirements_float" { id: "id", recipe_id: "recipe_Id", index: "index", stat: "stat", value: "value", r#enum: "enum", message: "message" });

crate::impl_codec!(RecipeRequirementsIID {
    id,
    recipe_id,
    index,
    stat,
    value,
    r#enum,
    message
});
crate::impl_from_row!(RecipeRequirementsIID, "recipe_requirements_i_i_d" { id: "id", recipe_id: "recipe_Id", index: "index", stat: "stat", value: "value", r#enum: "enum", message: "message" });

crate::impl_codec!(RecipeRequirementsInt {
    id,
    recipe_id,
    index,
    stat,
    value,
    r#enum,
    message
});
crate::impl_from_row!(RecipeRequirementsInt, "recipe_requirements_int" { id: "id", recipe_id: "recipe_Id", index: "index", stat: "stat", value: "value", r#enum: "enum", message: "message" });

crate::impl_codec!(RecipeRequirementsString {
    id,
    recipe_id,
    index,
    stat,
    value,
    r#enum,
    message
});
crate::impl_from_row!(RecipeRequirementsString, "recipe_requirements_string" { id: "id", recipe_id: "recipe_Id", index: "index", stat: "stat", value: "value", r#enum: "enum", message: "message" });

crate::impl_codec!(Spell {
    id,
    name,
    stat_mod_type,
    stat_mod_key,
    stat_mod_val,
    e_type,
    base_intensity,
    variance,
    wcid,
    num_projectiles,
    num_projectiles_variance,
    spread_angle,
    vertical_angle,
    default_launch_angle,
    non_tracking,
    create_offset_origin_x,
    create_offset_origin_y,
    create_offset_origin_z,
    padding_origin_x,
    padding_origin_y,
    padding_origin_z,
    dims_origin_x,
    dims_origin_y,
    dims_origin_z,
    peturbation_origin_x,
    peturbation_origin_y,
    peturbation_origin_z,
    imbued_effect,
    slayer_creature_type,
    slayer_damage_bonus,
    crit_freq,
    crit_multiplier,
    ignore_magic_resist,
    elemental_modifier,
    drain_percentage,
    damage_ratio,
    damage_type,
    boost,
    boost_variance,
    source,
    destination,
    proportion,
    loss_percent,
    source_loss,
    transfer_cap,
    max_boost_allowed,
    transfer_bitfield,
    index,
    link,
    position_obj_cell_id,
    position_origin_x,
    position_origin_y,
    position_origin_z,
    position_angles_w,
    position_angles_x,
    position_angles_y,
    position_angles_z,
    min_power,
    max_power,
    power_variance,
    dispel_school,
    align,
    number,
    number_variance,
    dot_duration,
    last_modified
});
crate::impl_from_row!(Spell, "spell" { id: "id", name: "name", stat_mod_type: "stat_Mod_Type", stat_mod_key: "stat_Mod_Key", stat_mod_val: "stat_Mod_Val", e_type: "e_Type", base_intensity: "base_Intensity", variance: "variance", wcid: "wcid", num_projectiles: "num_Projectiles", num_projectiles_variance: "num_Projectiles_Variance", spread_angle: "spread_Angle", vertical_angle: "vertical_Angle", default_launch_angle: "default_Launch_Angle", non_tracking: "non_Tracking", create_offset_origin_x: "create_Offset_Origin_X", create_offset_origin_y: "create_Offset_Origin_Y", create_offset_origin_z: "create_Offset_Origin_Z", padding_origin_x: "padding_Origin_X", padding_origin_y: "padding_Origin_Y", padding_origin_z: "padding_Origin_Z", dims_origin_x: "dims_Origin_X", dims_origin_y: "dims_Origin_Y", dims_origin_z: "dims_Origin_Z", peturbation_origin_x: "peturbation_Origin_X", peturbation_origin_y: "peturbation_Origin_Y", peturbation_origin_z: "peturbation_Origin_Z", imbued_effect: "imbued_Effect", slayer_creature_type: "slayer_Creature_Type", slayer_damage_bonus: "slayer_Damage_Bonus", crit_freq: "crit_Freq", crit_multiplier: "crit_Multiplier", ignore_magic_resist: "ignore_Magic_Resist", elemental_modifier: "elemental_Modifier", drain_percentage: "drain_Percentage", damage_ratio: "damage_Ratio", damage_type: "damage_Type", boost: "boost", boost_variance: "boost_Variance", source: "source", destination: "destination", proportion: "proportion", loss_percent: "loss_Percent", source_loss: "source_Loss", transfer_cap: "transfer_Cap", max_boost_allowed: "max_Boost_Allowed", transfer_bitfield: "transfer_Bitfield", index: "index", link: "link", position_obj_cell_id: "position_Obj_Cell_ID", position_origin_x: "position_Origin_X", position_origin_y: "position_Origin_Y", position_origin_z: "position_Origin_Z", position_angles_w: "position_Angles_W", position_angles_x: "position_Angles_X", position_angles_y: "position_Angles_Y", position_angles_z: "position_Angles_Z", min_power: "min_Power", max_power: "max_Power", power_variance: "power_Variance", dispel_school: "dispel_School", align: "align", number: "number", number_variance: "number_Variance", dot_duration: "dot_Duration", last_modified: "last_Modified" });

crate::impl_codec!(TreasureDeath {
    id,
    treasure_type,
    tier,
    loot_quality_mod,
    unknown_chances,
    item_chance,
    item_min_amount,
    item_max_amount,
    item_treasure_type_selection_chances,
    magic_item_chance,
    magic_item_min_amount,
    magic_item_max_amount,
    magic_item_treasure_type_selection_chances,
    mundane_item_chance,
    mundane_item_min_amount,
    mundane_item_max_amount,
    mundane_item_type_selection_chances,
    last_modified
});
crate::impl_from_row!(TreasureDeath, "treasure_death" { id: "id", treasure_type: "treasure_Type", tier: "tier", loot_quality_mod: "loot_Quality_Mod", unknown_chances: "unknown_Chances", item_chance: "item_Chance", item_min_amount: "item_Min_Amount", item_max_amount: "item_Max_Amount", item_treasure_type_selection_chances: "item_Treasure_Type_Selection_Chances", magic_item_chance: "magic_Item_Chance", magic_item_min_amount: "magic_Item_Min_Amount", magic_item_max_amount: "magic_Item_Max_Amount", magic_item_treasure_type_selection_chances: "magic_Item_Treasure_Type_Selection_Chances", mundane_item_chance: "mundane_Item_Chance", mundane_item_min_amount: "mundane_Item_Min_Amount", mundane_item_max_amount: "mundane_Item_Max_Amount", mundane_item_type_selection_chances: "mundane_Item_Type_Selection_Chances", last_modified: "last_Modified" });

crate::impl_codec!(TreasureGemCount {
    id,
    gem_code,
    tier,
    count,
    chance
});
crate::impl_from_row!(TreasureGemCount, "treasure_gem_count" { id: "id", gem_code: "gem_Code", tier: "tier", count: "count", chance: "chance" });

crate::impl_codec!(TreasureMaterialBase {
    id,
    material_code,
    tier,
    probability,
    material_id
});
crate::impl_from_row!(TreasureMaterialBase, "treasure_material_base" { id: "id", material_code: "material_Code", tier: "tier", probability: "probability", material_id: "material_Id" });

crate::impl_codec!(TreasureMaterialColor {
    id,
    material_id,
    color_code,
    palette_template,
    probability
});
crate::impl_from_row!(TreasureMaterialColor, "treasure_material_color" { id: "id", material_id: "material_Id", color_code: "color_Code", palette_template: "palette_Template", probability: "probability" });

crate::impl_codec!(TreasureMaterialGroups {
    id,
    material_group,
    tier,
    probability,
    material_id
});
crate::impl_from_row!(TreasureMaterialGroups, "treasure_material_groups" { id: "id", material_group: "material_Group", tier: "tier", probability: "probability", material_id: "material_Id" });

crate::impl_codec!(TreasureWielded {
    id,
    treasure_type,
    weenie_class_id,
    palette_id,
    unknown_1,
    shade,
    stack_size,
    stack_size_variance,
    probability,
    unknown_3,
    unknown_4,
    unknown_5,
    set_start,
    has_sub_set,
    continues_previous_set,
    unknown_9,
    unknown_10,
    unknown_11,
    unknown_12,
    last_modified
});
crate::impl_from_row!(TreasureWielded, "treasure_wielded" { id: "id", treasure_type: "treasure_Type", weenie_class_id: "weenie_Class_Id", palette_id: "palette_Id", unknown_1: "unknown_1", shade: "shade", stack_size: "stack_Size", stack_size_variance: "stack_Size_Variance", probability: "probability", unknown_3: "unknown_3", unknown_4: "unknown_4", unknown_5: "unknown_5", set_start: "set_Start", has_sub_set: "has_Sub_Set", continues_previous_set: "continues_Previous_Set", unknown_9: "unknown_9", unknown_10: "unknown_10", unknown_11: "unknown_11", unknown_12: "unknown_12", last_modified: "last_Modified" });

crate::impl_codec!(Version {
    id,
    base_version,
    patch_version,
    last_modified
});
crate::impl_from_row!(Version, "version" { id: "id", base_version: "base_Version", patch_version: "patch_Version", last_modified: "last_Modified" });

crate::impl_codec!(Weenie {
    class_id,
    class_name,
    r#type,
    last_modified,
    weenie_properties_anim_part,
    weenie_properties_attribute,
    weenie_properties_attribute_2nd,
    weenie_properties_body_part,
    weenie_properties_book,
    weenie_properties_book_page_data,
    weenie_properties_bool,
    weenie_properties_create_list,
    weenie_properties_did,
    weenie_properties_emote,
    weenie_properties_event_filter,
    weenie_properties_float,
    weenie_properties_generator,
    weenie_properties_iid,
    weenie_properties_int,
    weenie_properties_int64,
    weenie_properties_palette,
    weenie_properties_position,
    weenie_properties_skill,
    weenie_properties_spell_book,
    weenie_properties_string,
    weenie_properties_texture_map
});
crate::impl_from_row!(Weenie, "weenie" { class_id: "class_Id", class_name: "class_Name", r#type: "type", last_modified: "last_Modified" });

crate::impl_codec!(WeeniePropertiesAnimPart {
    id,
    object_id,
    index,
    animation_id
});
crate::impl_from_row!(WeeniePropertiesAnimPart, "weenie_properties_anim_part" { id: "id", object_id: "object_Id", index: "index", animation_id: "animation_Id" });

crate::impl_codec!(WeeniePropertiesAttribute {
    id,
    object_id,
    r#type,
    init_level,
    level_from_cp,
    cp_spent
});
crate::impl_from_row!(WeeniePropertiesAttribute, "weenie_properties_attribute" { id: "id", object_id: "object_Id", r#type: "type", init_level: "init_Level", level_from_cp: "level_From_C_P", cp_spent: "c_P_Spent" });

crate::impl_codec!(WeeniePropertiesAttribute2nd {
    id,
    object_id,
    r#type,
    init_level,
    level_from_cp,
    cp_spent,
    current_level
});
crate::impl_from_row!(WeeniePropertiesAttribute2nd, "weenie_properties_attribute_2nd" { id: "id", object_id: "object_Id", r#type: "type", init_level: "init_Level", level_from_cp: "level_From_C_P", cp_spent: "c_P_Spent", current_level: "current_Level" });

crate::impl_codec!(WeeniePropertiesBodyPart {
    id,
    object_id,
    key,
    d_type,
    d_val,
    d_var,
    base_armor,
    armor_vs_slash,
    armor_vs_pierce,
    armor_vs_bludgeon,
    armor_vs_cold,
    armor_vs_fire,
    armor_vs_acid,
    armor_vs_electric,
    armor_vs_nether,
    bh,
    hlf,
    mlf,
    llf,
    hrf,
    mrf,
    lrf,
    hlb,
    mlb,
    llb,
    hrb,
    mrb,
    lrb
});
crate::impl_from_row!(WeeniePropertiesBodyPart, "weenie_properties_body_part" { id: "id", object_id: "object_Id", key: "key", d_type: "d_Type", d_val: "d_Val", d_var: "d_Var", base_armor: "base_Armor", armor_vs_slash: "armor_Vs_Slash", armor_vs_pierce: "armor_Vs_Pierce", armor_vs_bludgeon: "armor_Vs_Bludgeon", armor_vs_cold: "armor_Vs_Cold", armor_vs_fire: "armor_Vs_Fire", armor_vs_acid: "armor_Vs_Acid", armor_vs_electric: "armor_Vs_Electric", armor_vs_nether: "armor_Vs_Nether", bh: "b_h", hlf: "h_l_f", mlf: "m_l_f", llf: "l_l_f", hrf: "h_r_f", mrf: "m_r_f", lrf: "l_r_f", hlb: "h_l_b", mlb: "m_l_b", llb: "l_l_b", hrb: "h_r_b", mrb: "m_r_b", lrb: "l_r_b" });

crate::impl_codec!(WeeniePropertiesBook {
    id,
    object_id,
    max_num_pages,
    max_num_chars_per_page
});
crate::impl_from_row!(WeeniePropertiesBook, "weenie_properties_book" { id: "id", object_id: "object_Id", max_num_pages: "max_Num_Pages", max_num_chars_per_page: "max_Num_Chars_Per_Page" });

crate::impl_codec!(WeeniePropertiesBookPageData {
    id,
    object_id,
    page_id,
    author_id,
    author_name,
    author_account,
    ignore_author,
    page_text
});
crate::impl_from_row!(WeeniePropertiesBookPageData, "weenie_properties_book_page_data" { id: "id", object_id: "object_Id", page_id: "page_Id", author_id: "author_Id", author_name: "author_Name", author_account: "author_Account", ignore_author: "ignore_Author", page_text: "page_Text" });

crate::impl_codec!(WeeniePropertiesBool {
    id,
    object_id,
    r#type,
    value
});
crate::impl_from_row!(WeeniePropertiesBool, "weenie_properties_bool" { id: "id", object_id: "object_Id", r#type: "type", value: "value" });

crate::impl_codec!(WeeniePropertiesCreateList {
    id,
    object_id,
    destination_type,
    weenie_class_id,
    stack_size,
    palette,
    shade,
    try_to_bond
});
crate::impl_from_row!(WeeniePropertiesCreateList, "weenie_properties_create_list" { id: "id", object_id: "object_Id", destination_type: "destination_Type", weenie_class_id: "weenie_Class_Id", stack_size: "stack_Size", palette: "palette", shade: "shade", try_to_bond: "try_To_Bond" });

crate::impl_codec!(WeeniePropertiesDID {
    id,
    object_id,
    r#type,
    value
});
crate::impl_from_row!(WeeniePropertiesDID, "weenie_properties_d_i_d" { id: "id", object_id: "object_Id", r#type: "type", value: "value" });

crate::impl_codec!(WeeniePropertiesEmote {
    id,
    object_id,
    category,
    probability,
    weenie_class_id,
    style,
    substyle,
    quest,
    vendor_type,
    min_health,
    max_health,
    weenie_properties_emote_action
});
crate::impl_from_row!(WeeniePropertiesEmote, "weenie_properties_emote" { id: "id", object_id: "object_Id", category: "category", probability: "probability", weenie_class_id: "weenie_Class_Id", style: "style", substyle: "substyle", quest: "quest", vendor_type: "vendor_Type", min_health: "min_Health", max_health: "max_Health" });

crate::impl_codec!(WeeniePropertiesEmoteAction {
    id,
    emote_id,
    order,
    r#type,
    delay,
    extent,
    motion,
    message,
    test_string,
    min,
    max,
    min_64,
    max_64,
    min_dbl,
    max_dbl,
    stat,
    display,
    amount,
    amount_64,
    hero_xp_64,
    percent,
    spell_id,
    wealth_rating,
    treasure_class,
    treasure_type,
    p_script,
    sound,
    destination_type,
    weenie_class_id,
    stack_size,
    palette,
    shade,
    try_to_bond,
    obj_cell_id,
    origin_x,
    origin_y,
    origin_z,
    angles_w,
    angles_x,
    angles_y,
    angles_z
});
crate::impl_from_row!(WeeniePropertiesEmoteAction, "weenie_properties_emote_action" { id: "id", emote_id: "emote_Id", order: "order", r#type: "type", delay: "delay", extent: "extent", motion: "motion", message: "message", test_string: "test_String", min: "min", max: "max", min_64: "min_64", max_64: "max_64", min_dbl: "min_Dbl", max_dbl: "max_Dbl", stat: "stat", display: "display", amount: "amount", amount_64: "amount_64", hero_xp_64: "hero_X_P_64", percent: "percent", spell_id: "spell_Id", wealth_rating: "wealth_Rating", treasure_class: "treasure_Class", treasure_type: "treasure_Type", p_script: "p_Script", sound: "sound", destination_type: "destination_Type", weenie_class_id: "weenie_Class_Id", stack_size: "stack_Size", palette: "palette", shade: "shade", try_to_bond: "try_To_Bond", obj_cell_id: "obj_Cell_Id", origin_x: "origin_X", origin_y: "origin_Y", origin_z: "origin_Z", angles_w: "angles_W", angles_x: "angles_X", angles_y: "angles_Y", angles_z: "angles_Z" });

crate::impl_codec!(WeeniePropertiesEventFilter {
    id,
    object_id,
    event
});
crate::impl_from_row!(WeeniePropertiesEventFilter, "weenie_properties_event_filter" { id: "id", object_id: "object_Id", event: "event" });

crate::impl_codec!(WeeniePropertiesFloat {
    id,
    object_id,
    r#type,
    value
});
crate::impl_from_row!(WeeniePropertiesFloat, "weenie_properties_float" { id: "id", object_id: "object_Id", r#type: "type", value: "value" });

crate::impl_codec!(WeeniePropertiesGenerator {
    id,
    object_id,
    probability,
    weenie_class_id,
    delay,
    init_create,
    max_create,
    when_create,
    where_create,
    stack_size,
    palette_id,
    shade,
    obj_cell_id,
    origin_x,
    origin_y,
    origin_z,
    angles_w,
    angles_x,
    angles_y,
    angles_z
});
crate::impl_from_row!(WeeniePropertiesGenerator, "weenie_properties_generator" { id: "id", object_id: "object_Id", probability: "probability", weenie_class_id: "weenie_Class_Id", delay: "delay", init_create: "init_Create", max_create: "max_Create", when_create: "when_Create", where_create: "where_Create", stack_size: "stack_Size", palette_id: "palette_Id", shade: "shade", obj_cell_id: "obj_Cell_Id", origin_x: "origin_X", origin_y: "origin_Y", origin_z: "origin_Z", angles_w: "angles_W", angles_x: "angles_X", angles_y: "angles_Y", angles_z: "angles_Z" });

crate::impl_codec!(WeeniePropertiesIID {
    id,
    object_id,
    r#type,
    value
});
crate::impl_from_row!(WeeniePropertiesIID, "weenie_properties_i_i_d" { id: "id", object_id: "object_Id", r#type: "type", value: "value" });

crate::impl_codec!(WeeniePropertiesInt {
    id,
    object_id,
    r#type,
    value
});
crate::impl_from_row!(WeeniePropertiesInt, "weenie_properties_int" { id: "id", object_id: "object_Id", r#type: "type", value: "value" });

crate::impl_codec!(WeeniePropertiesInt64 {
    id,
    object_id,
    r#type,
    value
});
crate::impl_from_row!(WeeniePropertiesInt64, "weenie_properties_int64" { id: "id", object_id: "object_Id", r#type: "type", value: "value" });

crate::impl_codec!(WeeniePropertiesPalette {
    id,
    object_id,
    sub_palette_id,
    offset,
    length
});
crate::impl_from_row!(WeeniePropertiesPalette, "weenie_properties_palette" { id: "id", object_id: "object_Id", sub_palette_id: "sub_Palette_Id", offset: "offset", length: "length" });

crate::impl_codec!(WeeniePropertiesPosition {
    id,
    object_id,
    position_type,
    obj_cell_id,
    origin_x,
    origin_y,
    origin_z,
    angles_w,
    angles_x,
    angles_y,
    angles_z
});
crate::impl_from_row!(WeeniePropertiesPosition, "weenie_properties_position" { id: "id", object_id: "object_Id", position_type: "position_Type", obj_cell_id: "obj_Cell_Id", origin_x: "origin_X", origin_y: "origin_Y", origin_z: "origin_Z", angles_w: "angles_W", angles_x: "angles_X", angles_y: "angles_Y", angles_z: "angles_Z" });

crate::impl_codec!(WeeniePropertiesSkill {
    id,
    object_id,
    r#type,
    level_from_pp,
    sac,
    pp,
    init_level,
    resistance_at_last_check,
    last_used_time
});
crate::impl_from_row!(WeeniePropertiesSkill, "weenie_properties_skill" { id: "id", object_id: "object_Id", r#type: "type", level_from_pp: "level_From_P_P", sac: "s_a_c", pp: "p_p", init_level: "init_Level", resistance_at_last_check: "resistance_At_Last_Check", last_used_time: "last_Used_Time" });

crate::impl_codec!(WeeniePropertiesSpellBook {
    id,
    object_id,
    spell,
    probability
});
crate::impl_from_row!(WeeniePropertiesSpellBook, "weenie_properties_spell_book" { id: "id", object_id: "object_Id", spell: "spell", probability: "probability" });

crate::impl_codec!(WeeniePropertiesString {
    id,
    object_id,
    r#type,
    value
});
crate::impl_from_row!(WeeniePropertiesString, "weenie_properties_string" { id: "id", object_id: "object_Id", r#type: "type", value: "value" });

crate::impl_codec!(WeeniePropertiesTextureMap {
    id,
    object_id,
    index,
    old_id,
    new_id
});
crate::impl_from_row!(WeeniePropertiesTextureMap, "weenie_properties_texture_map" { id: "id", object_id: "object_Id", index: "index", old_id: "old_Id", new_id: "new_Id" });
