// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Adapter/GDLE/GDLEConverter.cs
//! The `GDLEConverter.TryConvert` overloads only `GDLELoader` calls: GDLE events, spells and
//! wielded-treasure tables to World rows, and one recipe precursor to a cook book. The overloads
//! `import-json` uses (recipe, landblock, quest) are in [`crate::import::json::gdle`].
//!
//! Each C# overload wraps its body in `try`/`catch` and returns `false` on any exception (a null
//! dereference, mostly); that is an `Err` here. C#'s numeric conversions are [`CsCast`] casts.

use empyrean_common::dotnet::CsCast;

use super::models::{Event, SpellValue, WieldedTreasureTable};
use crate::import::json::models::RecipePrecursor;
use crate::import::json::value::R;
use crate::import::json::world::CookBook;
use crate::models::world::{Event as WorldEvent, Spell, TreasureWielded};

fn some<'a, T>(v: &'a Option<T>, what: &str) -> R<&'a T> {
    v.as_ref().ok_or_else(|| format!("{what} is null"))
}

/// `TryConvert(Models.Event, out Event)`.
// ACE: GDLEConverter.TryConvert
pub fn try_convert_event(input: &Event) -> R<WorldEvent> {
    let value = some(&input.value, "value")?;
    Ok(WorldEvent {
        //result.Id // This is an Auto Increment field in the ACE schema
        // DIVERGE: a GDLE event without a key converts with an empty name; ACE keeps null, which
        // the NOT NULL `event.name` column could never store.
        name: input.key.clone().unwrap_or_default(),
        start_time: value.start_time,
        end_time: value.end_time,
        state: value.event_state,
        ..WorldEvent::default()
    })
}

/// `TryConvert(uint id, Models.SpellValue, out Spell)`.
// ACE: GDLEConverter.TryConvert
#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
pub fn try_convert_spell(id: u32, input: &SpellValue) -> R<Spell> {
    let meta = some(&input.meta_spell, "meta_spell")?;
    let spell = some(&meta.spell, "meta_spell.spell")?;

    let mut result = Spell {
        id,
        // DIVERGE: a GDLE spell without a name converts with an empty name; ACE keeps null, which
        // the NOT NULL `spell.name` column could never store.
        name: input.name.clone().unwrap_or_default(),
        ..Spell::default()
    };

    if let Some(stat_mod) = &spell.stat_mod {
        result.stat_mod_type = Some(stat_mod.r#type);
        result.stat_mod_key = Some(stat_mod.key);
        result.stat_mod_val = Some(stat_mod.val as f32);
    }

    // Projectile, LifeProjectile
    result.e_type = spell.e_type;
    result.base_intensity = spell.base_intensity;
    result.variance = spell.variance;
    result.wcid = spell.wcid;
    result.num_projectiles = spell.num_projectiles;
    result.num_projectiles_variance = spell.num_projectiles_variance.map(|v| v.cs_cast());
    result.spread_angle = spell.spread_angle.map(|v| v as f32);
    result.vertical_angle = spell.vertical_angle.map(|v| v as f32);
    result.default_launch_angle = spell.default_launch_angle.map(|v| v as f32);
    result.non_tracking = spell.non_tracking;

    if let Some(o) = &spell.create_offset {
        result.create_offset_origin_x = Some(o.x as f32);
        result.create_offset_origin_y = Some(o.y as f32);
        result.create_offset_origin_z = Some(o.z as f32);
    }

    if let Some(o) = &spell.padding {
        result.padding_origin_x = Some(o.x as f32);
        result.padding_origin_y = Some(o.y as f32);
        result.padding_origin_z = Some(o.z as f32);
    }

    if let Some(o) = &spell.dims {
        result.dims_origin_x = Some(o.x as f32);
        result.dims_origin_y = Some(o.y as f32);
        result.dims_origin_z = Some(o.z as f32);
    }

    if let Some(o) = &spell.peturbation {
        result.peturbation_origin_x = Some(o.x as f32);
        result.peturbation_origin_y = Some(o.y as f32);
        result.peturbation_origin_z = Some(o.z as f32);
    }

    result.imbued_effect = spell.imbued_effect;
    result.slayer_creature_type = spell.slayer_creature_type;
    result.slayer_damage_bonus = spell.slayer_damage_bonus.map(|v| v as f32);
    result.crit_freq = spell.crit_freq.map(f64::from);
    result.crit_multiplier = spell.crit_multiplier.map(f64::from);
    result.ignore_magic_resist = spell.ignore_magic_resist;
    result.elemental_modifier = spell.elemental_modifier.map(f64::from);

    // LifeProjectile
    result.drain_percentage = spell.drain_percentage.map(|v| v as f32);
    result.damage_ratio = spell.damage_ratio.map(|v| v as f32);

    // Boost, FellowBoost
    result.damage_type = spell.damage_type;
    result.boost = spell.boost;
    result.boost_variance = spell.boost_variance;

    // Transfer
    result.source = spell.source;
    result.destination = spell.dest;
    result.proportion = spell.proportion.map(|v| v as f32);
    result.loss_percent = spell.loss_percent.map(|v| v as f32);
    result.source_loss = spell.source_loss;
    result.transfer_cap = spell.transfer_cap;
    result.max_boost_allowed = spell.max_boost_allowed;
    result.transfer_bitfield = spell.bitfield;

    // PortalLink
    result.index = spell.index;

    // PortalSummon
    result.link = spell.link;

    // PortalSending, FellowPortalSending
    if let Some(position) = &spell.position {
        result.position_obj_cell_id = Some(position.land_cell_id);

        let frame = some(&position.frame, "pos.frame")?;
        let (origin, angles) = frame.parts()?;
        result.position_origin_x = Some(origin.x);
        result.position_origin_y = Some(origin.y);
        result.position_origin_z = Some(origin.z);

        result.position_angles_w = Some(angles.w);
        result.position_angles_x = Some(angles.x);
        result.position_angles_y = Some(angles.y);
        result.position_angles_z = Some(angles.z);
    }

    // Dispel, FellowDispel
    result.min_power = spell.min_power;
    result.max_power = spell.max_power;
    result.power_variance = spell.power_variance.map(|v| v as f32);
    //result.DispelSchool = input.DispelSchool; // TODO!!!
    result.align = spell.align;
    result.number = spell.number;
    result.number_variance = spell.number_variance.map(|v| v as f32);

    Ok(result)
}

/// `TryConvert(Models.RecipePrecursor, out CookBook)`.
// ACE: GDLEConverter.TryConvert
#[must_use]
pub fn try_convert_precursor(input: &RecipePrecursor) -> CookBook {
    CookBook {
        recipe_id: input.recipe_id.unwrap_or(0),
        source_wcid: input.tool,
        target_wcid: input.target,
        last_modified: empyrean_common::dotnet::DotNetDateTime::MIN_VALUE,
    }
}

/// `TryConvert(Models.WieldedTreasureTable, out List<TreasureWielded>)`.
// ACE: GDLEConverter.TryConvert
pub fn try_convert_wielded_treasure(input: &WieldedTreasureTable) -> R<Vec<TreasureWielded>> {
    let mut results = Vec::new();

    for entry in some(&input.value, "value")? {
        let entry = some(entry, "value element")?;
        results.push(TreasureWielded {
            treasure_type: input.key,

            continues_previous_set: entry.continues_previous_set,
            has_sub_set: entry.has_sub_set,
            palette_id: entry.palette_id,
            probability: entry.probability,
            set_start: entry.set_start,
            shade: entry.shade,
            stack_size: entry.stack_size,
            stack_size_variance: entry.stack_size_variance,
            unknown_1: entry.unknown1,
            unknown_10: entry.unknown10,
            unknown_11: entry.unknown11,
            unknown_12: entry.unknown12,
            unknown_3: entry.unknown3,
            unknown_4: entry.unknown4,
            unknown_5: entry.unknown5,
            unknown_9: entry.unknown9,
            weenie_class_id: entry.weenie_class_id,
            ..TreasureWielded::default()
        });
    }

    Ok(results)
}
