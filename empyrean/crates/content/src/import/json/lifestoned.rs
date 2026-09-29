// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Adapter/Lifestoned/LifestonedConverter.cs
//! `LifestonedConverter.TryConvert(LSDWeenie, out Weenie, correctForEnumShift)`: a Lifestoned
//! JSON weenie to the World model. `import-json` passes `false`; `LifestonedLoader`'s
//! `TryLoadWeenie(s)Converted` may pass `true`.
//!
//! C#'s numeric casts here are unchecked (`(ushort)key`, `(sbyte?)palette`, `(long?)heroXp`), so
//! they are Rust `as` casts. A `null` where the C# dereferences (a null list element, a missing
//! frame) throws inside the `try` and the conversion fails.

use empyrean_entity::enums::{PropertyDataId, WeenieClassName};

use super::models::{self, LsdWeenie};
use super::value::R;
use super::weenie_class_id;
use super::world::*;

/// Unwrap a class value the C# dereferences.
fn some<'a, T>(v: &'a Option<T>, what: &str) -> R<&'a T> {
    v.as_ref().ok_or_else(|| format!("{what} is null"))
}

/// Iterate a C# `List<T>` whose elements the loop dereferences.
fn each<'a, T>(v: &'a [Option<T>], what: &'a str) -> impl Iterator<Item = R<&'a T>> + 'a {
    v.iter()
        .map(move |e| e.as_ref().ok_or_else(|| format!("{what}: null element")))
}

/// `string.ToLower()` in en-US: a simple per-character lowercase mapping.
fn to_lower(s: &str) -> String {
    s.chars()
        .map(|c| {
            let mut l = c.to_lowercase();
            match (l.next(), l.next()) {
                (Some(one), None) => one,
                _ => c,
            }
        })
        .collect()
}

/// The class name: the dat's `WeenieClassId` name, else the Feb 05 pdb's `WeenieClassName`,
/// else `ace<wcid>-<name>` with punctuation removed.
fn class_name(input: &LsdWeenie) -> R<String> {
    let wcid = input.weenie_id;
    if let Some(name) = u16::try_from(wcid).ok().and_then(weenie_class_id::name) {
        return Ok(to_lower(name));
    }
    if let Ok(w) = u16::try_from(wcid) {
        let w = WeenieClassName(w);
        if w.is_defined() {
            let name = w.name().ok_or("WeenieClassName without a name")?;
            // .ToLower().Substring(2), then drop the last six characters ("_class").
            let lower: Vec<char> = to_lower(name).chars().collect();
            if lower.len() < 2 + 6 {
                return Err(format!("class name {name} is too short"));
            }
            return Ok(lower[2..lower.len() - 6].iter().collect());
        }
    }
    let mut name = input.name()?;
    for c in ["'", " ", ".", "(", ")", "+", ":", "_", "-", ",", "\""] {
        name = name.replace(c, "");
    }
    Ok(format!("ace{wcid}-{}", to_lower(&name)))
}

/// `TryConvert(input, out result)` (`correctForEnumShift` false).
pub fn try_convert(input: &LsdWeenie) -> R<Weenie> {
    try_convert_with(input, false)
}

// ACE: LifestonedConverter.TryConvert
/// The World weenie, or why the C# `TryConvert` returns `false` (or throws).
/// `correct_for_enum_shift` renumbers the MotionCommand and PhysicsScript values the post-16PY
/// enums shifted, and keeps only the first DID row of each type.
pub fn try_convert_with(input: &LsdWeenie, correct_for_enum_shift: bool) -> R<Weenie> {
    if input.weenie_id == 0 {
        return Err("wcid is 0".into());
    }
    let mut result = Weenie {
        class_id: input.weenie_id,
        ..Weenie::default()
    };
    result.class_name = class_name(input)?;
    result.r#type = input.weenie_type_id;

    if let Some(book) = &input.book {
        result.book = Some(Book {
            max_num_pages: book.max_number_pages,
            max_num_chars_per_page: book.max_characters_per_page,
        });
        if let Some(pages) = &book.pages {
            for (page_id, value) in (0u32..).zip(each(pages, "pages")) {
                let value = value?;
                result.book_page_data.push(BookPageData {
                    page_id,
                    author_id: value.author_id.unwrap_or(0),
                    author_name: value.author_name.clone(),
                    author_account: Some(value.author_account.clone()),
                    ignore_author: value.ignore_author().unwrap_or(false),
                    page_text: value.page_text.clone(),
                });
            }
        }
    }

    if let Some(a) = &input.attributes {
        // PropertyAttribute: Strength 1, Endurance 2, Quickness 3, Coordination 4, Focus 5, Self 6.
        let attrs = [
            (1u16, &a.strength),
            (2, &a.endurance),
            (3, &a.quickness),
            (4, &a.coordination),
            (5, &a.focus),
            (6, &a.self_),
        ];
        for (t, attr) in attrs {
            if let Some(attr) = attr {
                result.attribute.push(Attribute {
                    r#type: t,
                    init_level: attr.ranks.unwrap_or(0),
                    level_from_cp: attr.level_from_cp,
                    cp_spent: attr.xp_spent.unwrap_or(0),
                });
            }
        }
        // PropertyAttribute2nd: MaxHealth 1, MaxStamina 3, MaxMana 5.
        for (t, vital) in [(1u16, &a.health), (3, &a.stamina), (5, &a.mana)] {
            if let Some(v) = vital {
                result.attribute_2nd.push(Attribute2nd {
                    r#type: t,
                    init_level: v.ranks.unwrap_or(0),
                    level_from_cp: v.level_from_cp.unwrap_or(0),
                    cp_spent: v.xp_spent.unwrap_or(0),
                    current_level: v.current.unwrap_or(0),
                });
            }
        }
    }

    if let Some(body) = &input.body {
        for value in each(
            some(&body.body_parts, "body_part_table")?,
            "body_part_table",
        ) {
            let value = value?;
            let bp = some(&value.body_part, "body part value")?;
            let av = some(&bp.armor_values, "acache")?;
            let sd = some(&bp.sd, "bpsd")?;
            #[allow(clippy::cast_possible_truncation)]
            let z = |v: Option<f64>| v.map_or(0.0, |d| d as f32);
            #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
            result.body_part.push(BodyPart {
                key: value.key as u16,
                d_type: bp.d_type,
                d_val: bp.d_val,
                d_var: bp.d_var,
                base_armor: av.base_armor,
                armor_vs_slash: av.armor_vs_slash,
                armor_vs_pierce: av.armor_vs_pierce,
                armor_vs_bludgeon: av.armor_vs_bludgeon,
                armor_vs_cold: av.armor_vs_cold,
                armor_vs_fire: av.armor_vs_fire,
                armor_vs_acid: av.armor_vs_acid,
                armor_vs_electric: av.armor_vs_electric,
                armor_vs_nether: av.armor_vs_nether,
                bh: bp.bh,
                zones: [
                    z(sd.hlf),
                    z(sd.mlf),
                    z(sd.llf),
                    z(sd.hrf),
                    z(sd.mrf),
                    z(sd.lrf),
                    z(sd.hlb),
                    z(sd.mlb),
                    z(sd.llb),
                    z(sd.hrb),
                    z(sd.mrb),
                    z(sd.lrb),
                ],
            });
        }
    }

    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    if let Some(stats) = &input.bool_stats {
        for value in each(stats, "boolStats") {
            let value = value?;
            if !result.bool_.iter().any(|x| x.r#type == value.key as u16) {
                result.bool_.push(Prop {
                    r#type: value.key as u16,
                    value: value.value != 0,
                });
            }
        }
    }

    if let Some(list) = &input.create_list {
        for value in each(list, "createList") {
            let value = value?;
            #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
            result.create_list.push(CreateList {
                weenie_class_id: value.weenie_class_id.unwrap_or(0),
                palette: value.palette.map_or(0, |p| p as i8),
                shade: value.shade.map_or(0.0, |s| s as f32),
                destination_type: value.destination.map_or(0, |d| d as i8),
                stack_size: value.stack_size.unwrap_or(0),
                // Not ACE's (a fix): a create-list item without
                // `try_to_bond` is not bonded; ACE's `TryToBond != 0` on a nullable byte was true
                // when it was absent, so such an item was bonded.
                try_to_bond: value.try_to_bond.is_some_and(|b| b != 0),
            });
        }
    }

    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    if let Some(stats) = &input.did_stats {
        for value in each(stats, "didStats") {
            let value = value?;
            if !correct_for_enum_shift {
                // Every row is kept, duplicates too.
                result.did.push(Prop {
                    r#type: value.key as u16,
                    value: value.value,
                });
            } else {
                let mut val_corrected = value.value;

                // Fix PhysicsScript ENUM shift post 16PY data
                if value.key == PHYSICS_SCRIPT {
                    // These are the only ones in 16PY database, not entirely certain where the shift started but the change below is correct for end of retail enum
                    if (83..=89).contains(&val_corrected) {
                        val_corrected += 1;
                    }
                }

                // Fix PhysicsScript ENUM shift post 16PY data
                if value.key == RESTRICTION_EFFECT && val_corrected >= 83 {
                    val_corrected = val_corrected.wrapping_add(1);
                }

                if !result.did.iter().any(|x| x.r#type == value.key as u16) {
                    result.did.push(Prop {
                        r#type: value.key as u16,
                        value: val_corrected,
                    });
                }
            }
        }
    }

    if let Some(table) = &input.emote_table {
        for kvp in each(table, "emoteTable") {
            let kvp = kvp?;
            for value in each(some(&kvp.emotes, "emoteTable value")?, "emotes") {
                let value = value?;
                #[allow(clippy::cast_possible_wrap)]
                let mut ef_emote = Emote {
                    category: value.category,
                    probability: value.probability.unwrap_or(0.0),
                    weenie_class_id: value.class_id,
                    style: value.style,
                    substyle: value.sub_style,
                    quest: value.quest.clone(),
                    vendor_type: value.vendor_type.map(|v| v as i32),
                    min_health: value.min_health,
                    max_health: value.max_health,
                    actions: Vec::new(),
                };

                // Fix MotionCommand ENUM shift post 16PY data
                if correct_for_enum_shift {
                    ef_emote.style = ef_emote.style.map(shift_motion);
                    ef_emote.substyle = ef_emote.substyle.map(shift_motion);
                }

                for (order, action) in (0u32..).zip(each(some(&value.actions, "emotes")?, "emotes"))
                {
                    let action = action?;
                    ef_emote
                        .actions
                        .push(convert_action(order, action, correct_for_enum_shift)?);
                }
                result.emote.push(ef_emote);
            }
        }
    }

    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    if let Some(stats) = &input.float_stats {
        for value in each(stats, "floatStats") {
            let value = value?;
            if !result.float.iter().any(|x| x.r#type == value.key as u16) {
                result.float.push(Prop {
                    r#type: value.key as u16,
                    value: f64::from(value.value),
                });
            }
        }
    }

    if let Some(table) = &input.generator_table {
        for value in each(table, "generatorTable") {
            let value = value?;
            let (origin, angles) = some(&value.frame, "generator frame")?.parts()?;
            #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
            result.generator.push(Generator {
                probability: value.probability as f32,
                weenie_class_id: value.weenie_class_id,
                delay: Some(value.delay),
                init_create: value.init_create as i32,
                max_create: value.max_number as i32,
                when_create: value.when_create,
                where_create: value.where_create,
                stack_size: Some(value.stack_size),
                palette_id: Some(value.palette_id),
                shade: Some(value.shade),
                obj_cell_id: Some(value.object_cell),
                origin_x: Some(origin.x),
                origin_y: Some(origin.y),
                origin_z: Some(origin.z),
                angles_w: Some(angles.w),
                angles_x: Some(angles.x),
                angles_y: Some(angles.y),
                angles_z: Some(angles.z),
            });
        }
    }

    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    if let Some(stats) = &input.iid_stats {
        for value in each(stats, "iidStats") {
            let value = value?;
            if !result.iid.iter().any(|x| x.r#type == value.key as u16) {
                result.iid.push(Prop {
                    r#type: value.key as u16,
                    value: value.value as u32,
                });
            }
        }
    }

    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    if let Some(stats) = &input.int_stats {
        for value in each(stats, "intStats") {
            let value = value?;
            if !result.int.iter().any(|x| x.r#type == value.key as u16) {
                result.int.push(Prop {
                    r#type: value.key as u16,
                    value: value.value,
                });
            }
        }
    }

    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    if let Some(stats) = &input.int64_stats {
        for value in each(stats, "int64Stats") {
            let value = value?;
            if !result.int64.iter().any(|x| x.r#type == value.key as u16) {
                result.int64.push(Prop {
                    r#type: value.key as u16,
                    value: value.value,
                });
            }
        }
    }

    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    if let Some(list) = &input.positions {
        for value in each(list, "posStats") {
            let value = value?;
            let t = value.position_type as u16;
            if !result.position.iter().any(|x| x.position_type == t) {
                let pos = some(&value.position, "posStats value")?;
                let (origin, angles) = some(&pos.frame, "position frame")?.parts()?;
                result.position.push(Position {
                    position_type: t,
                    obj_cell_id: pos.land_cell_id,
                    origin_x: origin.x,
                    origin_y: origin.y,
                    origin_z: origin.z,
                    angles_w: angles.w,
                    angles_x: angles.x,
                    angles_y: angles.y,
                    angles_z: angles.z,
                });
            }
        }
    }

    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    if let Some(list) = &input.skills {
        for value in each(list, "skills") {
            let value = value?;
            // `(ushort)value.SkillId` on an `int?` throws when it is null.
            let t = value.skill_id.ok_or("skill key is null")? as u16;
            if !result.skill.iter().any(|x| x.r#type == t) {
                let s = some(&value.skill, "skill value")?;
                result.skill.push(Skill {
                    r#type: t,
                    level_from_pp: s.level_from_pp.ok_or("level_from_pp is null")? as u16,
                    sac: s.trained_level.map_or(0, |v| v as u32),
                    pp: s.xp_invested.unwrap_or(0),
                    init_level: s.ranks.unwrap_or(0),
                    resistance_at_last_check: s.resistance_of_last_check.unwrap_or(0),
                    last_used_time: s.last_used.map_or(0.0, f64::from),
                });
            }
        }
    }

    if let Some(list) = &input.spells {
        for value in each(list, "spellbook") {
            let value = value?;
            if !result.spell_book.iter().any(|x| x.spell == value.spell_id) {
                let stats = some(&value.stats, "spellbook value")?;
                #[allow(clippy::cast_possible_truncation)]
                result.spell_book.push(SpellBook {
                    spell: value.spell_id,
                    probability: stats.casting_chance.map_or(0.0, |c| c as f32),
                });
            }
        }
    }

    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    if let Some(stats) = &input.string_stats {
        for value in each(stats, "stringStats") {
            let value = value?;
            if !result.string.iter().any(|x| x.r#type == value.key as u16) {
                result.string.push(Prop {
                    r#type: value.key as u16,
                    value: value.value.clone(),
                });
            }
        }
    }

    Ok(result)
}

/// `(int)PropertyDataId.PhysicsScript` and `(int)PropertyDataId.RestrictionEffect`.
const PHYSICS_SCRIPT: i32 = PropertyDataId::PhysicsScript.0 as i32;
const RESTRICTION_EFFECT: i32 = PropertyDataId::RestrictionEffect.0 as i32;

/// A MotionCommand of the post-16PY enum: an index (low 16 bits) of 0x115 or more moves up by 3.
fn shift_motion(value: u32) -> u32 {
    let index = value & 0xFFFF;
    if index >= 0x115 {
        value.wrapping_add(3)
    } else {
        value
    }
}

/// One `EmoteAction` of `TryConvert`'s emote loop.
#[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
fn convert_action(
    order: u32,
    action: &models::EmoteAction,
    correct_for_enum_shift: bool,
) -> R<EmoteAction> {
    let mut ef = EmoteAction {
        order,
        r#type: action.emote_action_type,
        delay: action.delay.unwrap_or(0.0),
        extent: action.extent.unwrap_or(0.0),
        motion: action.motion,
        message: action.message.clone(),
        test_string: action.test_string.clone(),
        min: action.min.map(|v| v as i32),
        max: action.max.map(|v| v as i32),
        min_64: action.minimum64,
        max_64: action.maximum64,
        min_dbl: action.f_min.map(f64::from),
        max_dbl: action.f_max.map(f64::from),
        stat: action.stat.map(|v| v as i32),
        display: action.display(),
        amount: action.amount.map(|v| v as i32),
        amount_64: action.amount64,
        hero_xp_64: action.hero_xp64.map(|v| v as i64),
        percent: action.percent.map(f64::from),
        spell_id: action.spell_id.map(|v| v as i32),
        wealth_rating: action.wealth_rating.map(|v| v as i32),
        treasure_class: action.treasure_class.map(|v| v as i32),
        treasure_type: action.treasure_type,
        p_script: action.p_script.map(|v| v as i32),
        sound: action.sound.map(|v| v as i32),
        ..EmoteAction::default()
    };

    // Fix MotionCommand ENUM shift post 16PY data
    if correct_for_enum_shift {
        ef.motion = ef.motion.map(shift_motion);
    }

    // Fix PhysicsScript ENUM shift post 16PY data
    if correct_for_enum_shift {
        if let Some(p) = ef.p_script.filter(|&p| p >= 83) {
            ef.p_script = Some(p.wrapping_add(1));
        }
    }
    if let Some(item) = &action.item {
        ef.weenie_class_id = item.weenie_class_id;
        ef.palette = item.palette.map(|v| v as i32);
        ef.shade = item.shade.map(|v| v as f32);
        ef.destination_type = item.destination.map(|v| v as i8);
        ef.stack_size = item.stack_size;
        ef.try_to_bond = item.try_to_bond.map(|v| v != 0);
    }
    if let Some(frame) = &action.frame {
        let (origin, angles) = frame.parts()?;
        set_frame(&mut ef, origin, angles);
    }
    if let Some(pos) = &action.m_position {
        ef.obj_cell_id = Some(pos.land_cell_id);
        let (origin, angles) = some(&pos.frame, "mPosition frame")?.parts()?;
        set_frame(&mut ef, origin, angles);
    }
    Ok(ef)
}

fn set_frame(ef: &mut EmoteAction, origin: &models::Xyz, angles: &models::Quaternion) {
    ef.origin_x = Some(origin.x);
    ef.origin_y = Some(origin.y);
    ef.origin_z = Some(origin.z);
    ef.angles_w = Some(angles.w);
    ef.angles_x = Some(angles.x);
    ef.angles_y = Some(angles.y);
    ef.angles_z = Some(angles.z);
}
