// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Adapter/GDLE/GDLEConverter.cs
//! `GDLEConverter.TryConvert` for the three GDLE documents `import-json` reads: a recipe with its
//! precursors, a landblock spawn map, and a quest. C#'s unchecked casts are `as` casts.

use super::models;
use super::value::R;
use super::world::*;

fn some<'a, T>(v: &'a Option<T>, what: &str) -> R<&'a T> {
    v.as_ref().ok_or_else(|| format!("{what} is null"))
}

// ACE: GDLEConverter.TryConvert
/// `TryConvert(Models.RecipeCombined, out List<CookBook>, out Recipe)`.
pub fn try_convert_recipe(input: &models::RecipeCombined) -> R<(Vec<CookBook>, Recipe)> {
    let mut recipe = convert_recipe(some(&input.recipe, "recipe")?)?;
    recipe.id = input.key;
    let mut cookbooks = Vec::new();
    for precursor in some(&input.precursors, "precursors")? {
        // TryConvert(RecipePrecursor) catches the null dereference and returns false.
        let precursor = some(precursor, "precursor")?;
        cookbooks.push(CookBook {
            recipe_id: input.key,
            source_wcid: precursor.tool,
            target_wcid: precursor.target,
            last_modified: empyrean_common::dotnet::DotNetDateTime::MIN_VALUE,
        });
    }
    Ok((cookbooks, recipe))
}

/// `TryConvert(Models.Recipe, out Recipe)`.
#[allow(
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap
)]
pub(crate) fn convert_recipe(input: &models::Recipe) -> R<Recipe> {
    let mut result = Recipe {
        id: input.recipe_id,
        unknown_1: input.unknown as u32,
        skill: input.skill,
        difficulty: input.difficulty,
        salvage_type: input.skill_check_formula_type as u32,
        success_wcid: input.success_wcid,
        success_amount: input.success_amount,
        success_message: input.success_message.clone(),
        fail_wcid: input.fail_wcid,
        fail_amount: input.fail_amount,
        fail_message: input.fail_message.clone(),
        success_destroy_target_chance: input.success_consume_target_chance,
        success_destroy_target_amount: input.success_consume_target_amount,
        success_destroy_target_message: input.success_consume_target_message.clone(),
        success_destroy_source_chance: input.success_consume_tool_chance,
        success_destroy_source_amount: input.success_consume_tool_amount,
        success_destroy_source_message: input.success_consume_tool_message.clone(),
        fail_destroy_target_chance: input.failure_consume_target_chance,
        fail_destroy_target_amount: input.failure_consume_target_amount,
        fail_destroy_target_message: input.failure_consume_target_message.clone(),
        fail_destroy_source_chance: input.failure_consume_tool_chance,
        fail_destroy_source_amount: input.failure_consume_tool_amount,
        fail_destroy_source_message: input.failure_consume_tool_message.clone(),
        data_id: input.data_id,
        ..Recipe::default()
    };

    let mut index: i8 = -1;
    for value in some(&input.requirements, "Requirements")? {
        index = index.wrapping_add(1);
        let Some(value) = value else { continue };
        let l = &value.lists;
        if let Some(reqs) = &l.int_requirements {
            for r in reqs {
                let r = some(r, "IntRequirements element")?;
                result.requirements_int.push(row(
                    index,
                    r.stat,
                    r.value,
                    r.operation_type,
                    r.message.clone(),
                    0,
                ));
            }
        }
        if let Some(reqs) = &l.did_requirements {
            for r in reqs {
                let r = some(r, "DIDRequirements element")?;
                result.requirements_did.push(row(
                    index,
                    r.stat,
                    r.value,
                    r.operation_type,
                    r.message.clone(),
                    0,
                ));
            }
        }
        if let Some(reqs) = &l.iid_requirements {
            for r in reqs {
                let r = some(r, "IIDRequirements element")?;
                result.requirements_iid.push(row(
                    index,
                    r.stat,
                    r.value,
                    r.operation_type,
                    r.message.clone(),
                    0,
                ));
            }
        }
        if let Some(reqs) = &l.float_requirements {
            for r in reqs {
                let r = some(r, "FloatRequirements element")?;
                result.requirements_float.push(row(
                    index,
                    r.stat,
                    r.value,
                    r.operation_type,
                    r.message.clone(),
                    0,
                ));
            }
        }
        if let Some(reqs) = &l.string_requirements {
            for r in reqs {
                let r = some(r, "StringRequirements element")?;
                // Not ACE's (a fix): the requirement's Message is kept.
                result.requirements_string.push(row(
                    index,
                    r.stat,
                    r.value.clone(),
                    r.operation_type,
                    r.message.clone(),
                    0,
                ));
            }
        }
        if let Some(reqs) = &l.bool_requirements {
            for r in reqs {
                let r = some(r, "BoolRequirements element")?;
                result.requirements_bool.push(row(
                    index,
                    r.stat,
                    r.value,
                    r.operation_type,
                    r.message.clone(),
                    0,
                ));
            }
        }
    }

    let mods = some(&input.mods, "Mods")?;
    for i in 0..8usize {
        // Must be 8: input.Mods[i] throws past the end of the list.
        let value = mods.get(i).ok_or("Mods has fewer than 8 entries")?;
        let Some(value) = value else { continue };
        let idx = i as i8;
        let mut recipe_mod = RecipeMod::default();
        let l = &value.lists;
        if let Some(m) = &l.int_requirements {
            for r in m {
                let r = some(r, "IntRequirements element")?;
                recipe_mod.mods_int.push(row(
                    idx,
                    r.stat,
                    r.value,
                    r.operation_type,
                    None,
                    r.unknown.unwrap_or(0),
                ));
            }
        }
        if let Some(m) = &l.did_requirements {
            for r in m {
                let r = some(r, "DIDRequirements element")?;
                recipe_mod.mods_did.push(row(
                    idx,
                    r.stat,
                    r.value,
                    r.operation_type,
                    None,
                    r.unknown.unwrap_or(0),
                ));
            }
        }
        if let Some(m) = &l.iid_requirements {
            for r in m {
                let r = some(r, "IIDRequirements element")?;
                recipe_mod.mods_iid.push(row(
                    idx,
                    r.stat,
                    r.value,
                    r.operation_type,
                    None,
                    r.unknown.unwrap_or(0),
                ));
            }
        }
        if let Some(m) = &l.float_requirements {
            for r in m {
                let r = some(r, "FloatRequirements element")?;
                recipe_mod.mods_float.push(row(
                    idx,
                    r.stat,
                    r.value,
                    r.operation_type,
                    None,
                    r.unknown.unwrap_or(0),
                ));
            }
        }
        if let Some(m) = &l.string_requirements {
            for r in m {
                let r = some(r, "StringRequirements element")?;
                recipe_mod.mods_string.push(row(
                    idx,
                    r.stat,
                    r.value.clone(),
                    r.operation_type,
                    None,
                    r.unknown,
                ));
            }
        }
        if let Some(m) = &l.bool_requirements {
            for r in m {
                let r = some(r, "BoolRequirements element")?;
                recipe_mod.mods_bool.push(row(
                    idx,
                    r.stat,
                    r.value,
                    r.operation_type,
                    None,
                    r.unknown.unwrap_or(0),
                ));
            }
        }
        // The first 4 are "act on success", the second 4 are "act on failure".
        recipe_mod.executes_on_success = i <= 3;
        recipe_mod.health = value.modify_health;
        recipe_mod.stamina = value.modify_stamina;
        recipe_mod.mana = value.modify_mana;
        recipe_mod.unknown_7 = value.unknown7;
        recipe_mod.data_id = value.modification_script_id;
        recipe_mod.unknown_9 = value.unknown9;
        recipe_mod.instance_id = value.unknown10;

        let m = &recipe_mod;
        let add = m.health > 0 || m.stamina > 0 || m.mana > 0;
        let add = add || m.unknown_7 || m.data_id > 0 || m.unknown_9 > 0 || m.instance_id > 0;
        let add = add
            || !m.mods_bool.is_empty()
            || !m.mods_did.is_empty()
            || !m.mods_float.is_empty()
            || !m.mods_iid.is_empty()
            || !m.mods_int.is_empty()
            || !m.mods_string.is_empty();
        if add {
            result.recipe_mod.push(recipe_mod);
        }
    }
    Ok(result)
}

fn row<V>(
    index: i8,
    stat: i32,
    value: V,
    r#enum: i32,
    message: Option<String>,
    source: i32,
) -> RecipeRow<V> {
    RecipeRow {
        index,
        stat,
        value,
        r#enum,
        message,
        source,
    }
}

/// `TryConvert(Models.Landblock, out List<LandblockInstance>, out List<LandblockInstanceLink>)`,
/// with its re-guid step. Guids are left as the document gives them when they are inside the
/// landblock's static range.
pub fn try_convert_landblock(
    input: &models::Landblock,
) -> R<(Vec<LandblockInstance>, Vec<LandblockInstanceLink>)> {
    let value = some(&input.value, "value")?;
    let mut results = Vec::new();
    for w in some(&value.weenies, "weenies")? {
        let w = some(w, "weenie")?;
        let pos = some(&w.pos, "pos")?;
        let (origin, angles) = some(&pos.frame, "pos frame")?.parts()?;
        results.push(LandblockInstance {
            guid: w.id,
            weenie_class_id: w.wcid,
            obj_cell_id: pos.land_cell_id,
            origin_x: origin.x,
            origin_y: origin.y,
            origin_z: origin.z,
            angles_w: angles.w,
            angles_x: angles.x,
            angles_y: angles.y,
            angles_z: angles.z,
            is_link_child: false,
            last_modified: empyrean_common::dotnet::DotNetDateTime::MIN_VALUE,
            links: Vec::new(),
        });
    }
    let mut links = Vec::new();
    if let Some(list) = &value.links {
        for l in list {
            let l = some(l, "link")?;
            links.push(LandblockInstanceLink {
                parent_guid: l.target,
                child_guid: l.source,
                last_modified: empyrean_common::dotnet::DotNetDateTime::MIN_VALUE,
            });
        }
    }
    #[allow(clippy::cast_possible_truncation)]
    re_guid_and_convert_landblocks(&mut results, &mut links, (input.key >> 16) as u16)?;
    Ok((results, links))
}

// ACE: GDLEConverter.ReGuidAndConvertLandblocks
fn re_guid_and_convert_landblocks(
    instances: &mut [LandblockInstance],
    links: &mut [LandblockInstanceLink],
    landblock_id: u16,
) -> R<()> {
    let first_guid = 0x7000_0000 | (u32::from(landblock_id) << 12);
    let last_guid = first_guid | 0xFFF;
    let mut next_guid = first_guid;
    // A Dictionary: Add throws on a repeated key.
    let mut reguid: Vec<(u32, u32)> = Vec::new();
    for instance in instances.iter_mut() {
        if instance.guid < first_guid || instance.guid > last_guid {
            if reguid.iter().any(|(k, _)| *k == instance.guid) {
                return Err(format!("guid {:#X} appears twice", instance.guid));
            }
            reguid.push((instance.guid, next_guid));
            instance.guid = next_guid;
            next_guid = next_guid.wrapping_add(1);
        }
    }
    let map = |g: u32| reguid.iter().find(|(k, _)| *k == g).map(|(_, v)| *v);
    for link in links.iter_mut() {
        if let Some(p) = map(link.parent_guid) {
            link.parent_guid = p;
        }
        if let Some(c) = map(link.child_guid) {
            link.child_guid = c;
        }
    }
    Ok(())
}

/// `TryConvert(Models.Quest, out Quest)`.
#[allow(clippy::cast_sign_loss)]
pub fn try_convert_quest(input: &models::Quest) -> R<Quest> {
    let value = some(&input.value, "value")?;
    Ok(Quest {
        name: input.key.clone(),
        // FIXME in ACE: db schema should be int.
        min_delta: value.mindelta as u32,
        max_solves: value.maxsolves,
        message: value.fullname.clone(),
        last_modified: empyrean_common::dotnet::DotNetDateTime::MIN_VALUE,
    })
}
