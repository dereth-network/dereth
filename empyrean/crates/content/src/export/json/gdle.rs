// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Adapter/GDLE/GDLEConverter.cs
//! `GDLEConverter`'s export direction: ACE quests, landblock instances and recipes (with their
//! cookbooks) to the GDLE JSON models.

use std::collections::HashMap;
use std::sync::Arc;

use empyrean_common::dotnet::{format, CsCast, DotNetDict};

use super::models;
use crate::models::world::{CookBook, LandblockInstance, Quest, Recipe, RecipeMod};

/// Converts ACE -> GDLE quest. Never fails.
// ACE: GDLEConverter.TryConvert(Quest, out Models.Quest)
#[must_use]
pub fn try_convert_quest(input: &Quest) -> Option<models::Quest> {
    let quest = models::QuestValue {
        fullname: input.message.clone(),
        mindelta: input.min_delta.cs_cast(),
        maxsolves: input.max_solves,
    };
    Some(models::Quest {
        key: Some(input.name.clone()),
        value: Some(quest),
    })
}

/// Converts ACE landblock instances -> GDLE landblock instances. `weenie_names` and
/// `weenie_class_names` are ACE's static `GDLEConverter.WeenieNames` / `WeenieClassNames`
/// (`GetAllWeenieNames` / `GetAllWeenieClassNames`); the `desc` texts are written only when both
/// are set.
///
/// An empty list converts to `{"key": 0}`. `None` where ACE throws out of the command: two
/// instances with one guid (`Dictionary.Add`), or a first instance without a landblock (the
/// `(uint)` cast of a null `int?`).
// ACE: GDLEConverter.TryConvert(List<LandblockInstance>, out Models.Landblock)
#[must_use]
pub fn try_convert_landblock(
    input: &[LandblockInstance],
    weenie_names: Option<&DotNetDict<u32, String>>,
    weenie_class_names: Option<&DotNetDict<u32, String>>,
) -> Option<models::Landblock> {
    let mut instance_wcids: HashMap<u32, u32> = HashMap::new();
    for instance in input {
        // DIVERGE: ACE's Dictionary.Add throws (uncaught); the conversion fails instead.
        if instance_wcids
            .insert(instance.guid, instance.weenie_class_id)
            .is_some()
        {
            return None;
        }
    }

    let mut result = models::Landblock::default();

    let Some(first) = input.first() else {
        return Some(result);
    };

    // DIVERGE: ACE's `(uint)` of a null `Landblock` throws (uncaught); the conversion fails instead.
    let landblock = first.landblock?;
    let landblock_u: u32 = landblock.cs_cast();
    result.key = landblock_u << 16;

    let mut value = models::LandblockValue::default();

    result.desc = Some(format!("{}({})", result.key, format(landblock, "X4")));

    let names = weenie_names.zip(weenie_class_names);
    let lookup = |d: &DotNetDict<u32, String>, wcid: u32| d.get(&wcid).cloned().unwrap_or_default();

    for lbi in input {
        let weenies = value.weenies.get_or_insert_with(Vec::new);

        let mut weenie = models::LandblockWeenie {
            id: lbi.guid,
            wcid: lbi.weenie_class_id,
            ..Default::default()
        };

        if let Some((names, class_names)) = names {
            let weenie_name = lookup(names, lbi.weenie_class_id);
            let weenie_class_name = lookup(class_names, lbi.weenie_class_id);

            weenie.desc = Some(format!("{weenie_name}({weenie_class_name})"));
        }

        // fix this ***, write it properly.
        let pos = models::Position {
            land_cell_id: lbi.obj_cell_id,
            frame: Some(models::Frame {
                position: Some(models::Xyz {
                    x: lbi.origin_x,
                    y: lbi.origin_y,
                    z: lbi.origin_z,
                }),
                rotations: Some(models::Quaternion {
                    w: lbi.angles_w,
                    x: lbi.angles_x,
                    y: lbi.angles_y,
                    z: lbi.angles_z,
                }),
            }),
        };
        weenie.pos = Some(pos);

        weenies.push(weenie);

        for link in &lbi.landblock_instance_link {
            let links = value.links.get_or_insert_with(Vec::new);

            let mut l = models::LandblockLink {
                target: link.parent_guid,
                source: link.child_guid,
                desc: None,
            };

            if let Some((names, class_names)) = names {
                // ACE also looks up the target's name, which it never uses.
                let target_class_name = lookup(class_names, lbi.weenie_class_id);

                if let Some(&source_wcid) = instance_wcids.get(&link.child_guid) {
                    let source_name = lookup(names, source_wcid);
                    let source_class_name = lookup(class_names, source_wcid);

                    l.desc = Some(format!(
                        "{source_name}({source_class_name})(wcid: {source_wcid}) -> {target_class_name}(wcid: {})",
                        lbi.weenie_class_id
                    ));
                }
            }
            links.push(l);
        }
    }
    result.value = Some(value);
    Some(result)
}

// ACE: GDLEConverter.GetIndex
#[must_use]
pub fn get_index(r#mod: &RecipeMod) -> i32 {
    if let Some(m) = r#mod.recipe_mods_bool.first() {
        return i32::from(m.index);
    }
    if let Some(m) = r#mod.recipe_mods_did.first() {
        return i32::from(m.index);
    }
    if let Some(m) = r#mod.recipe_mods_float.first() {
        return i32::from(m.index);
    }
    if let Some(m) = r#mod.recipe_mods_iid.first() {
        return i32::from(m.index);
    }
    if let Some(m) = r#mod.recipe_mods_int.first() {
        return i32::from(m.index);
    }
    if let Some(m) = r#mod.recipe_mods_string.first() {
        return i32::from(m.index);
    }
    -1
}

/// Converts ACE recipe + cookbooks (as `GetCookbooksByRecipeId` returns them) to GDLE recipe +
/// precursors. `None` where ACE returns `false`: no cookbooks, a first cookbook without its recipe,
/// or a recipe that does not convert.
// ACE: GDLEConverter.TryConvert(List<CookBook>, out Models.RecipeCombined)
#[must_use]
pub fn try_convert_cookbooks(
    cookbooks: &[Option<Arc<CookBook>>],
) -> Option<models::RecipeCombined> {
    // DIVERGE: a null first cookbook makes ACE throw (uncaught); the conversion fails instead.
    let first = cookbooks.first()?.as_ref()?;
    let recipe = first.recipe.as_ref()?;

    let mut result = models::RecipeCombined {
        key: recipe.id,
        ..Default::default()
    };
    result.desc = Some(first.source_wcid.to_string()); // TODO: get weenie name

    let mut new_recipe = try_convert_recipe(recipe)?;

    new_recipe.recipe_id = 0;

    result.recipe = Some(new_recipe);

    let mut precursors = Vec::new();
    for cookbook in cookbooks {
        // A null cookbook throws inside TryConvert(CookBook), which catches it and returns false.
        if let Some(mut precursor) = cookbook.as_deref().and_then(try_convert_cookbook) {
            precursor.recipe_id = None;
            precursors.push(precursor);
        }
    }
    result.precursors = Some(precursors);
    Some(result)
}

/// A slot of `Requirements` (3) or `Mods` (8): `None` where ACE's `List` indexer throws
/// `ArgumentOutOfRangeException`.
fn slot<T: Default>(list: &mut [Option<T>], index: i32) -> Option<&mut T> {
    let i = usize::try_from(index).ok()?;
    Some(list.get_mut(i)?.get_or_insert_with(T::default))
}

/// Converts an ACE recipe to GDLE recipe. `None` where ACE's `try` catches: a requirement index
/// outside 0..3 or a mod index outside 0..8 (other than -1, which ACE skips with a console line).
// ACE: GDLEConverter.TryConvert(Recipe, out Models.Recipe)
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn try_convert_recipe(input: &Recipe) -> Option<models::Recipe> {
    let mut result = models::Recipe {
        recipe_id: input.id,

        unknown: input.unknown_1.cs_cast(),
        skill: input.skill,
        difficulty: input.difficulty,
        skill_check_formula_type: input.salvage_type.cs_cast(),

        success_wcid: input.success_wcid,
        success_amount: input.success_amount,
        success_message: input.success_message.clone(),

        fail_wcid: input.fail_wcid,
        fail_amount: input.fail_amount,
        fail_message: input.fail_message.clone(),

        success_consume_target_chance: input.success_destroy_target_chance,
        success_consume_target_amount: input.success_destroy_target_amount,
        success_consume_target_message: input.success_destroy_target_message.clone(),

        success_consume_tool_chance: input.success_destroy_source_chance,
        success_consume_tool_amount: input.success_destroy_source_amount,
        success_consume_tool_message: input.success_destroy_source_message.clone(),

        failure_consume_target_chance: input.fail_destroy_target_chance,
        failure_consume_target_amount: input.fail_destroy_target_amount,
        failure_consume_target_message: input.fail_destroy_target_message.clone(),

        failure_consume_tool_chance: input.fail_destroy_source_chance,
        failure_consume_tool_amount: input.fail_destroy_source_amount,
        failure_consume_tool_message: input.fail_destroy_source_message.clone(),

        data_id: input.data_id,
        ..Default::default()
    };

    // requirements
    let mut requirements: Vec<Option<models::RecipeRequirements>> = vec![None, None, None];

    for int_req in &input.recipe_requirements_int {
        let r = slot(&mut requirements, i32::from(int_req.index))?;
        r.int_requirements
            .get_or_insert_with(Vec::new)
            .push(models::IntRequirement {
                stat: int_req.stat,
                value: int_req.value,
                operation_type: int_req.r#enum,
                message: int_req.message.clone(),
                unknown: None,
            });
    }

    for did_req in &input.recipe_requirements_did {
        let r = slot(&mut requirements, i32::from(did_req.index))?;
        r.did_requirements
            .get_or_insert_with(Vec::new)
            .push(models::DidRequirement {
                stat: did_req.stat,
                value: did_req.value,
                operation_type: did_req.r#enum,
                message: did_req.message.clone(),
                unknown: None,
            });
    }

    for iid_req in &input.recipe_requirements_iid {
        let r = slot(&mut requirements, i32::from(iid_req.index))?;
        r.iid_requirements
            .get_or_insert_with(Vec::new)
            .push(models::IidRequirement {
                stat: iid_req.stat,
                value: iid_req.value,
                operation_type: iid_req.r#enum,
                message: iid_req.message.clone(),
                unknown: None,
            });
    }

    for float_req in &input.recipe_requirements_float {
        let r = slot(&mut requirements, i32::from(float_req.index))?;
        r.float_requirements
            .get_or_insert_with(Vec::new)
            .push(models::FloatRequirement {
                stat: float_req.stat,
                value: float_req.value,
                operation_type: float_req.r#enum,
                message: float_req.message.clone(),
                unknown: None,
            });
    }

    // Not ACE's (a fix): the string and bool requirements keep their
    // `Message`, as the others do; ACE's string requirement had no Message member and its bool
    // requirement's was not set, so an export lost them.
    for string_req in &input.recipe_requirements_string {
        let r = slot(&mut requirements, i32::from(string_req.index))?;
        r.string_requirements
            .get_or_insert_with(Vec::new)
            .push(models::StringRequirement {
                stat: string_req.stat,
                value: string_req.value.clone(),
                operation_type: string_req.r#enum,
                message: string_req.message.clone(),
                unknown: 0,
            });
    }

    for bool_req in &input.recipe_requirements_bool {
        let r = slot(&mut requirements, i32::from(bool_req.index))?;
        r.bool_requirements
            .get_or_insert_with(Vec::new)
            .push(models::BoolRequirement {
                stat: bool_req.stat,
                value: bool_req.value,
                operation_type: bool_req.r#enum,
                message: bool_req.message.clone(),
                unknown: None,
            });
    }

    result.requirements = Some(requirements);

    // modifications
    let mut mods: Vec<Option<models::Mod>> = vec![None, None, None, None, None, None, None, None];

    for recipe_mod in &input.recipe_mod {
        // should index be on RecipeMod, or RecipeModType?
        let idx = get_index(recipe_mod);
        if idx == -1 {
            empyrean_common::console_write_line!("Couldn't find recipe mod idx");
            continue;
        }

        let m = slot(&mut mods, idx)?;

        // base stats
        m.modify_health = recipe_mod.health;
        m.modify_stamina = recipe_mod.stamina;
        m.modify_mana = recipe_mod.mana;

        // TODO!!! we're missing the following RequiresHealth, RequiresStamina, RequiresMana

        m.unknown7 = recipe_mod.unknown_7;
        m.modification_script_id = recipe_mod.data_id;

        m.unknown9 = recipe_mod.unknown_9;
        m.unknown10 = recipe_mod.instance_id;

        // type mods
        for int_mod in &recipe_mod.recipe_mods_int {
            m.int_requirements
                .get_or_insert_with(Vec::new)
                .push(models::IntRequirement {
                    stat: int_mod.stat,
                    value: int_mod.value,
                    operation_type: int_mod.r#enum,
                    unknown: Some(int_mod.source),
                    message: None,
                });
        }

        for did_mod in &recipe_mod.recipe_mods_did {
            m.did_requirements
                .get_or_insert_with(Vec::new)
                .push(models::DidRequirement {
                    stat: did_mod.stat,
                    value: did_mod.value,
                    operation_type: did_mod.r#enum,
                    unknown: Some(did_mod.source),
                    message: None,
                });
        }

        for iid_mod in &recipe_mod.recipe_mods_iid {
            m.iid_requirements
                .get_or_insert_with(Vec::new)
                .push(models::IidRequirement {
                    stat: iid_mod.stat,
                    value: iid_mod.value,
                    operation_type: iid_mod.r#enum,
                    unknown: Some(iid_mod.source),
                    message: None,
                });
        }

        for float_mod in &recipe_mod.recipe_mods_float {
            m.float_requirements
                .get_or_insert_with(Vec::new)
                .push(models::FloatRequirement {
                    stat: float_mod.stat,
                    value: float_mod.value,
                    operation_type: float_mod.r#enum,
                    unknown: Some(float_mod.source),
                    message: None,
                });
        }

        for string_mod in &recipe_mod.recipe_mods_string {
            m.string_requirements
                .get_or_insert_with(Vec::new)
                .push(models::StringRequirement {
                    stat: string_mod.stat,
                    value: string_mod.value.clone(),
                    operation_type: string_mod.r#enum,
                    message: None,
                    unknown: string_mod.source,
                });
        }

        for bool_mod in &recipe_mod.recipe_mods_bool {
            m.bool_requirements
                .get_or_insert_with(Vec::new)
                .push(models::BoolRequirement {
                    stat: bool_mod.stat,
                    value: bool_mod.value,
                    operation_type: bool_mod.r#enum,
                    unknown: Some(bool_mod.source),
                    message: None,
                });
        }
    }

    result.mods = Some(mods);

    Some(result)
}

/// Converts ACE cookbook to GDLE precursor. Never fails.
// ACE: GDLEConverter.TryConvert(CookBook, out Models.RecipePrecursor)
#[must_use]
pub fn try_convert_cookbook(input: &CookBook) -> Option<models::RecipePrecursor> {
    Some(models::RecipePrecursor {
        recipe_id: Some(input.recipe_id),
        tool: input.source_wcid,
        target: input.target_wcid,
    })
}
