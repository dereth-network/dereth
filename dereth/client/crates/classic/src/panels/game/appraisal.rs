//! The classic interface's appraisal text, block by block in its own order. Only the stable
//! formatting primitives are shared with the modern presentation; changed blocks are local.
use super::super::ClassicAppraisalExtra;
use dereth_client_contract::view::AppraisalView;
use dereth_presentation::appraisal as shared;
use shared::ItemInfo;
fn line(out: &mut Vec<ItemInfo>, text: impl Into<String>, same_line: bool, color: u8) {
    out.push(ItemInfo {
        text: text.into(),
        same_line,
        color,
    });
}
fn description(p: &AppraisalView) -> Option<String> {
    let Some(long) = p.long_desc.as_ref() else {
        return p.short_desc.clone();
    };
    let Some(flags) = p.long_desc_decoration else {
        return Some(long.clone());
    };
    let mut s = String::new();
    if flags & 1 != 0 {
        if let Some(w) = p.workmanship {
            s.push_str(&shared::workmanship_adjective(w));
            s.push(' ');
        }
    }
    if flags & 2 != 0 {
        if let Some(m) = &p.description_material {
            s.push_str(m);
            s.push(' ');
        }
    }
    s.push_str(long);
    if flags & 4 != 0 {
        if let Some((n, name)) = &p.description_gems {
            s.push_str(&format!(", set with {} {name}", shared::insert_commas(*n)));
        }
    }
    Some(s)
}
fn special(p: &AppraisalView, extra: &ClassicAppraisalExtra) -> Vec<ItemInfo> {
    let s = &p.special;
    let mut names: Vec<String> = vec![];
    if let Some((_, name)) = &s.slayer {
        names.push(format!("{name} slayer"));
    }
    if extra.attack_type.is_some_and(|n| n & 0x1e0 != 0) {
        names.push("Multi-Strike".into());
    }
    let bits = s.imbued.unwrap_or(0);
    for (bit, name) in [
        (1, "Critical Strike"),
        (2, "Crippling Blow"),
        (4, "Armor Rending"),
        (8, "Slash Rending"),
        (16, "Pierce Rending"),
        (32, "Bludgeon Rending"),
        (64, "Acid Rending"),
        (128, "Cold Rending"),
        (256, "Lightning Rending"),
        (512, "Fire Rending"),
        (1024, "+1 Melee Defense"),
        (2048, "+1 Missile Defense"),
        (4096, "+1 Magic Defense"),
        (0x20000000, "Magic Absorbing"),
        (0x80000000, "Phantasmal"),
    ] {
        if bits & bit != 0 {
            names.push(name.into());
        }
    }
    if s.item_spellcraft.is_some_and(|n| n > 9998) {
        names.push("Unenchantable".into());
    }
    if let Some(name) = s.attuned.and_then(shared::attuned_status_to_string) {
        names.push(name.into());
    }
    if let Some(name) = s.bonded.and_then(shared::bonded_status_to_string) {
        names.push(name.into());
    }
    for (yes, name) in [
        (s.retained == Some(true), "Retained"),
        (s.critical_multiplier, "Crushing Blow"),
        (s.critical_frequency, "Biting Strike"),
        (s.ivoryable == Some(true), "Ivoryable"),
        (s.dyeable == Some(true), "Dyeable"),
    ] {
        if yes {
            names.push(name.into());
        }
    }
    let mut out = vec![];
    if !names.is_empty() {
        line(
            &mut out,
            format!("Special Properties: {}", names.join(", ")),
            true,
            0,
        );
    }
    if bits != 0 {
        line(&mut out, "This item cannot be further imbued.", true, 0);
    }
    if !out.is_empty() {
        line(&mut out, "", true, 0);
    }
    out
}
fn weapon(p: &AppraisalView, extra: &ClassicAppraisalExtra) -> Vec<ItemInfo> {
    let mut out = vec![];
    let loc = p.valid_locations;
    if loc & 0x200000 != 0 {
        line(
            &mut out,
            p.armor_level.map_or("Shield Level: Unknown".into(), |n| {
                format!("Shield Level: {n}")
            }),
            true,
            shared::mod_color(p, &[0x1c]),
        );
    } else if loc & 0x1f00000 == 0 && loc & 0x7fff != 0 {
        match p.armor_level {
            None => line(&mut out, "Armor Level:  Unknown", true, 0),
            Some(n) if n > 0 => line(
                &mut out,
                format!("Armor Level:  {n}"),
                true,
                shared::mod_color(p, &[0x1c]),
            ),
            _ => {}
        }
    }
    let mut classic = p.clone();
    classic.valid_locations &= 0x1ffffff;
    let mut rows = shared::weapon_and_armor_lines(&classic);
    rows.retain(|(s, _)| !s.starts_with("Skill: "));
    if let (Some(n), Some(w)) = (extra.elemental_damage_bonus.filter(|n| *n > 0), p.weapon) {
        let at = rows
            .iter()
            .position(|(s, _)| {
                s.starts_with("Damage Modifier")
                    || s.starts_with("Speed:")
                    || s.starts_with("Range:")
                    || s.starts_with("Bonus to Attack")
                    || s.starts_with("Uses ")
                    || s.starts_with("Used as ")
            })
            .unwrap_or(rows.len());
        rows.insert(
            at,
            (
                format!(
                    "Elemental Damage Bonus: {n}, {}.",
                    shared::damage_type_to_string(w.damage_type)
                ),
                0,
            ),
        );
    }
    // The classic skill sentence follows speed/range and precedes attack bonus.
    if let Some(w) = p.weapon.filter(|_| loc & 0x1f00000 != 0) {
        if let Some(skill) =
            dereth_client_contract::panels::examination::skill_to_string(w.weapon_skill)
        {
            let at = rows
                .iter()
                .position(|(s, _)| {
                    s.starts_with("Bonus to Attack")
                        || s.starts_with("Uses ")
                        || s.starts_with("Used as ")
                })
                .unwrap_or(rows.len());
            rows.insert(at, (format!("Uses {skill} Skill"), 0));
        }
    }
    for (s, c) in rows {
        line(&mut out, s, true, c);
    }
    out
}
fn short_magic(p: &AppraisalView) -> Vec<ItemInfo> {
    let mut out = vec![];
    let Some(spells) = &p.magic.spells else {
        return out;
    };
    if !p.success {
        line(&mut out, "Spells: unknown.", false, 0);
        return out;
    }
    let names: Vec<_> = spells
        .iter()
        .filter(|s| !s.enchantment)
        .map(|s| s.name.as_str())
        .collect();
    if !names.is_empty() {
        line(
            &mut out,
            format!("Casts the following spells: {}", names.join(", ")),
            false,
            0,
        );
    }
    out
}
fn magic(p: &AppraisalView, extra: &ClassicAppraisalExtra) -> Vec<ItemInfo> {
    let mut out = vec![];
    let Some(spells) = &p.magic.spells else {
        return out;
    };
    if !p.success {
        line(&mut out, "Spells: unknown.", false, 0);
        return out;
    }
    let mut normal = String::from("Spell Descriptions:\n");
    let mut ench = String::from("Enchantments:\n");
    let (mut any_normal, mut any_ench) = (false, false);
    for s in spells {
        let entry = format!("\n     {} ({})", s.name, s.description);
        if s.enchantment {
            ench.push_str(&entry);
            any_ench = true;
        } else {
            normal.push_str(&entry);
            any_normal = true;
        }
    }
    if any_normal {
        let mut terms = vec![];
        if let Some(n) = p.item_difficulty.filter(|n| *n > 0) {
            terms.push(format!("Arcane Lore: {n}"));
        }
        if let Some(n) = p.allegiance_rank_limit.filter(|n| *n > 0) {
            terms.push(format!("Allegiance Rank: {n}"));
        }
        if let Some(s) = &extra.activation_heritage {
            terms.push(s.clone());
        }
        if let Some((s, n)) = p.activation_skill.as_ref().filter(|(_, n)| *n > 0) {
            terms.push(format!("{s}: {n}"));
        }
        line(
            &mut out,
            format!("Activation Requirements: {}", terms.join(", ")),
            false,
            0,
        );
        if p.has_allowed_activator {
            line(
                &mut out,
                format!(
                    "This item can only be activated by {}.",
                    p.craftsman_name.as_deref().unwrap_or("the original owner")
                ),
                false,
                0,
            );
        }
        if let Some(n) = p.magic.spellcraft {
            line(&mut out, format!("Spellcraft: {n}."), false, 0);
        }
        if let (Some(n), Some(max)) = (p.magic.cur_mana, p.magic.max_mana) {
            line(&mut out, format!("Mana: {n} / {max}."), false, 0);
        }
        if let Some(rate) = p.magic.mana_rate {
            let seconds = dereth_primitives::num::to_i64_f64((1. / rate).abs() + 0.5);
            line(
                &mut out,
                format!("Mana Cost: 1 point per {seconds} seconds."),
                true,
                0,
            );
        } else if let Some(cost) = p.magic.mana_cost {
            line(
                &mut out,
                if cost > 0 {
                    format!("Mana Cost: {cost}.\n(Can be reduced by the Mana Conversion skill)")
                } else {
                    format!("Mana Cost: {cost}.")
                },
                false,
                0,
            );
        }
        line(&mut out, normal, false, 0);
    }
    if any_ench {
        line(&mut out, ench, false, 0);
    }
    out
}
pub fn runs(p: &AppraisalView, extra: &ClassicAppraisalExtra) -> Vec<ItemInfo> {
    let mut out = vec![];
    if let Some(s) = description(p) {
        line(&mut out, s, false, 0);
    }
    out.extend(shared::portal_restriction_lines(p));
    if p.num_times_tinkered.is_some()
        || p.tinker_name.is_some()
        || p.imbuer_name.is_some()
        || p.workmanship.is_some()
    {
        out.extend(shared::tinkering_lines(p));
    }
    out.extend(special(p, extra));
    if let Some(s) = &p.use_text {
        line(&mut out, s, false, 0);
    }
    out.extend(weapon(p, extra));
    out.extend(shared::defense_mod_lines(p));
    for mut r in shared::caster_data_lines(p) {
        r.text = r.text.replace(" spells:", " war spells:");
        out.push(r);
    }
    out.extend(shared::level_limit_lines(p));
    // The classic interface shows one wield requirement; later clients added three more.
    let mut one = p.clone();
    one.wield_requirements.truncate(1);
    one.account_requirements = None;
    one.heritage_specific_armor = None;
    out.extend(shared::wield_requirement_lines(&one));
    out.extend(short_magic(p));
    out.extend(shared::craftsman_lines(p));
    for (s, c) in shared::armor_mod_lines(p) {
        line(&mut out, s, true, c);
    }
    out.extend(shared::boost_value_lines(p));
    out.extend(shared::heal_kit_lines(p));
    out.extend(shared::capacity_lines(p));
    for s in shared::lock_appraise_lines(p) {
        line(&mut out, s, false, 0);
    }
    out.extend(magic(p, extra));
    out.extend(shared::mana_stone_lines(p));
    out.extend(shared::remaining_uses_lines(p));
    out.extend(shared::cannot_be_sold_lines(p));
    out
}
pub fn rich(p: &AppraisalView, extra: &ClassicAppraisalExtra) -> Vec<crate::TextRun> {
    let mut out = vec![];
    let mut has_text = false;
    for run in runs(p, extra) {
        let prefix = if has_text {
            if run.same_line {
                "\n"
            } else {
                "\n\n"
            }
        } else {
            ""
        };
        let text = format!("{prefix}{}", run.text);
        has_text |= !text.is_empty();
        out.push(crate::TextRun {
            text,
            color: match run.color {
                1 => 0xff00ff00,
                2 => 0xffff0000,
                _ => 0xffd2d2c8,
            },
        });
    }
    out
}

/// A character's appraisal. The classic interface predates societies, ratings and title counts.
pub fn character(p: &AppraisalView, name: &str) -> Vec<crate::TextRun> {
    let mut rows: Vec<(String, u32)> = vec![];
    if p.allegiance_rank.unwrap_or(0) > 0 {
        let line = if let Some(m) = &p.monarch_title {
            match &p.patron_title {
                Some(t) if t == m => format!("Monarch/Patron: {m}"),
                Some(t) => format!("Monarch: {m}\nPatron: {t}"),
                None => format!("Monarch: {m}"),
            }
        } else {
            let n = p.allegiance_followers.unwrap_or(0).max(0);
            format!(
                "Allegiance Monarch: {n} Follower{}",
                if n == 1 { "" } else { "s" }
            )
        };
        rows.push((line, 0xffd2d2c8));
    }
    if let Some(s) = &p.fellowship {
        rows.push((format!("Fellowship: {s}"), 0xff00ff00));
    }
    if let Some(s) = &p.date_of_birth {
        rows.push((format!("Arrived in Dereth: {s}"), 0xffd2d2c8));
    }
    if let Some(n) = p.age {
        rows.push((format!("Time in Dereth: {}", elapsed(n as u32)), 0xffd2d2c8));
    }
    if let Some(n) = p.chess_rank {
        rows.push((format!("Chess Rank: {n}"), 0xffd2d2c8));
    }
    if let Some(n) = p.fishing_skill {
        rows.push((format!("Fishing Skill: {n}"), 0xffd2d2c8));
    }
    if let Some(n) = p.num_deaths {
        rows.push((
            if n < 1 {
                format!("{name} has never died.")
            } else {
                format!("Deaths: {n}")
            },
            0xffd2d2c8,
        ));
    }
    rows.into_iter()
        .enumerate()
        .map(|(i, (text, color))| crate::TextRun {
            text: format!("{}{text}", if i == 0 { "" } else { "\n" }),
            color,
        })
        .collect()
}
fn elapsed(mut seconds: u32) -> String {
    let mut parts = vec![];
    for (unit, suffix) in [(2_592_000, "mo"), (86_400, "d"), (3600, "h"), (60, "m")] {
        let n = seconds / unit;
        seconds %= unit;
        if n > 0 {
            parts.push(format!("{n}{suffix}"));
        }
    }
    parts.push(format!("{seconds}s"));
    parts.join(" ")
}
