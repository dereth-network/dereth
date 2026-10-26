//! The character's fuller reads for plugins: its attributes, its fellowship with each fellow's
//! vitals, its allegiance, titles, augmentations, contracts (the quests the client tracks), spell
//! bars and options. Each answers what the game's own panels read, so a plugin sees what the player
//! could see.

use dereth_client_contract::GameView;
use dereth_client_runtime::frame_hook::HookFrame;
use dereth_plugin_api::Value;

/// The six primary attributes, by id.
pub const ATTRIBUTES: [(u32, &str); 6] = [
    (1, "strength"),
    (2, "endurance"),
    (3, "quickness"),
    (4, "coordination"),
    (5, "focus"),
    (6, "self"),
];

/// The three vitals' maximums as attributes, by their secondary attribute id.
pub const VITAL_STATS: [(u32, &str); 3] = [(1, "health"), (3, "stamina"), (5, "mana")];

/// The augmentations a character can hold, by the integer property that counts each.
pub const AUGMENTATIONS: &[(u32, &str)] = &[
    (215, "stat"),
    (216, "family_stat"),
    (217, "innate_family"),
    (218, "innate_strength"),
    (219, "innate_endurance"),
    (220, "innate_coordination"),
    (221, "innate_quickness"),
    (222, "innate_focus"),
    (223, "innate_self"),
    (224, "specialize_salvaging"),
    (225, "specialize_item_tinkering"),
    (226, "specialize_armor_tinkering"),
    (227, "specialize_magic_item_tinkering"),
    (228, "specialize_weapon_tinkering"),
    (229, "extra_pack_slot"),
    (230, "increased_carrying_capacity"),
    (231, "less_death_item_loss"),
    (232, "spells_remain_past_death"),
    (233, "critical_defense"),
    (234, "bonus_xp"),
    (235, "bonus_salvage"),
    (236, "bonus_imbue_chance"),
    (237, "faster_regen"),
    (238, "increased_spell_duration"),
    (239, "resistance_family"),
    (240, "resistance_slash"),
    (241, "resistance_pierce"),
    (242, "resistance_blunt"),
    (243, "resistance_acid"),
    (244, "resistance_fire"),
    (245, "resistance_frost"),
    (246, "resistance_lightning"),
    (293, "specialize_gearcraft"),
    (294, "infused_creature_magic"),
    (295, "infused_item_magic"),
    (296, "infused_life_magic"),
    (297, "infused_war_magic"),
    (298, "critical_expertise"),
    (299, "critical_power"),
    (300, "skilled_melee"),
    (301, "skilled_missile"),
    (302, "skilled_magic"),
    (309, "damage_bonus"),
    (310, "damage_reduction"),
    (326, "jack_of_all_trades"),
    (327, "resistance_nether"),
    (328, "infused_void_magic"),
    (333, "luminance_damage_rating"),
    (334, "luminance_damage_reduction_rating"),
    (335, "luminance_crit_damage_rating"),
    (336, "luminance_crit_reduction_rating"),
    (337, "luminance_surge_effect_rating"),
    (338, "luminance_surge_chance_rating"),
    (339, "luminance_item_mana_usage"),
    (340, "luminance_item_mana_gain"),
    (341, "luminance_vitality"),
    (342, "luminance_healing_rating"),
    (343, "luminance_skilled_craft"),
    (344, "luminance_skilled_spec"),
    (345, "luminance_no_destroy_craft"),
];

/// A character field by name, or `None` for a field this module does not answer.
#[must_use]
pub fn field(frame: &HookFrame<'_>, key: &str) -> Option<Value> {
    let view = frame.view;
    Some(match key {
        "attributes" => Value::List(
            ATTRIBUTES
                .iter()
                .map(|(id, name)| {
                    let adv = view.attribute_advancement(*id, false);
                    Value::map()
                        .with("id", *id)
                        .with("name", *name)
                        .with("base", adv.map(|a| a.value))
                        .with("current", view.attribute(*id).or(adv.map(|a| a.effective)))
                        .with("cost", adv.map(|a| a.cost_to_raise))
                })
                .collect(),
        ),
        "vital_stats" => Value::List(
            VITAL_STATS
                .iter()
                .map(|(id, name)| {
                    let adv = view.attribute_advancement(*id, true);
                    Value::map()
                        .with("id", *id)
                        .with("name", *name)
                        .with("base", adv.map(|a| a.value))
                        .with("current", adv.map(|a| a.effective))
                        .with("cost", adv.map(|a| a.cost_to_raise))
                })
                .collect(),
        ),
        "fellowship" => fellowship(view),
        "allegiance" => allegiance(view),
        "monarch" => Value::from(view.allegiance_monarch_quality().map(|m| m.0)),
        "titles" => {
            let t = view.character_titles();
            Value::map()
                .with("current", t.display)
                .with("current_name", view.display_title())
                .with(
                    "list",
                    Value::List(
                        t.titles
                            .iter()
                            .map(|(id, name)| {
                                Value::map().with("id", *id).with("name", name.as_str())
                            })
                            .collect(),
                    ),
                )
        }
        "augmentations" => {
            let mut m = Value::map();
            if let Some(q) = frame.world.player_qualities() {
                for (key, name) in AUGMENTATIONS {
                    let n = q.inq_int(*key);
                    if n != 0 {
                        m = m.with(name, n);
                    }
                }
            }
            m
        }
        "quests" => Value::List(
            view.contracts()
                .into_iter()
                .map(|c| {
                    Value::map()
                        .with("id", c.contract_id)
                        .with("name", c.name.as_str())
                        .with("status", c.status.as_str())
                        .with("stage", c.stage)
                        .with("description", c.description.as_str())
                        .with("contact", c.contact.as_str())
                        .with("timed", c.timed.as_str())
                        .with("repeat_remaining", c.repeat_remaining)
                })
                .collect(),
        ),
        "spell_bars" => Value::List(
            (0..8)
                .map(|t| Value::List(view.spell_tab(t).iter().map(|s| Value::from(*s)).collect()))
                .collect(),
        ),
        "options" => {
            let mut m = Value::map();
            for o in dereth_client_contract::view::PlayerOption::ALL {
                m = m.with(&crate::actions::option_name(o), view.player_option(o));
            }
            m
        }
        "skill_credits" => Value::from(view.skill_credits()),
        "busy" => {
            Value::from(!frame.world.request_lock.is_idle() || frame.world.magic.busy_count > 0)
        }
        "game_time" => game_time(frame.server_time),
        "info" => view.character_info().map_or(Value::Nil, |i| {
            Value::map()
                .with("deaths", i.num_deaths)
                .with("chess_rank", i.chess_rank)
                .with("fishing_skill", i.fishing_skill)
                .with("encumbrance", i.encumbrance)
                .with("capacity", i.capacity)
        }),
        _ => return None,
    })
}

/// The sixteen named times of a Derethian day, from midnight.
pub const HOURS: [&str; 16] = [
    "Darktide",
    "Darktide-and-Half",
    "Foredawn",
    "Foredawn-and-Half",
    "Dawnsong",
    "Dawnsong-and-Half",
    "Morntide",
    "Morntide-and-Half",
    "Midsong",
    "Midsong-and-Half",
    "Warmtide",
    "Warmtide-and-Half",
    "Evensong",
    "Evensong-and-Half",
    "Gloaming",
    "Gloaming-and-Half",
];

/// The twelve thirty-day months of a Portal Year, from its first day.
pub const MONTHS: [&str; 12] = [
    "Morningthaw",
    "Solclaim",
    "Seedsow",
    "Leafdawning",
    "Verdantine",
    "Thistledown",
    "Harvestgain",
    "Leafcull",
    "Frostfell",
    "Snowreap",
    "Coldeve",
    "Wintersebb",
];

/// A Derethian day's length in seconds, and the calendar's start: the game time of the first
/// year's first midnight is minus this offset, in year 10.
const DAY: f64 = 7620.0;
const YEAR_OFFSET: f64 = 3600.0;
const FIRST_YEAR: f64 = 10.0;

/// The game's calendar at game time `t` (the world clock, seconds): `{ ticks, year, month,
/// month_name, day, hour, hour_name, day_fraction, is_day, minutes_until_day,
/// minutes_until_night }`. A day is 2 h 07 min; daytime runs from Dawnsong to the end of
/// Warmtide-and-Half.
#[must_use]
pub fn game_time(t: f64) -> Value {
    let y = t + YEAR_OFFSET;
    let year_len = DAY * 360.0;
    let years = (y / year_len).floor();
    let into_year = y - years * year_len;
    let day_of_year = (into_year / DAY).floor();
    let fraction = (into_year - day_of_year * DAY) / DAY;
    let hour = dereth_primitives::num::to_i32_f64((fraction * 16.0).floor()).clamp(0, 15);
    let month = dereth_primitives::num::to_i32_f64((day_of_year / 30.0).floor()).clamp(0, 11);
    let day = dereth_primitives::num::to_i32_f64(day_of_year) - month * 30 + 1;
    let minutes_to = |fraction_at: f64| {
        let ahead = (fraction_at - fraction).rem_euclid(1.0);
        (ahead * DAY / 60.0 * 10.0).round() / 10.0
    };
    let is_day = (4..12).contains(&hour);
    Value::map()
        .with("ticks", t)
        .with("year", years + FIRST_YEAR)
        .with("month", month)
        .with("month_name", MONTHS[usize::try_from(month).unwrap_or(0)])
        .with("day", day)
        .with("hour", hour)
        .with("hour_name", HOURS[usize::try_from(hour).unwrap_or(0)])
        .with("day_fraction", fraction)
        .with("is_day", is_day)
        .with(
            "minutes_until_day",
            if is_day { 0.0 } else { minutes_to(4.0 / 16.0) },
        )
        .with(
            "minutes_until_night",
            if is_day { minutes_to(12.0 / 16.0) } else { 0.0 },
        )
}

fn vitals(cur: u32, max: u32) -> Value {
    Value::map().with("current", cur).with("max", max)
}

/// The fellowship the character is in, or nil: `{ name, leader, share_xp, even_split, open,
/// locked, members }`, each member `{ id, name, level, xp_percent, health, stamina, mana }`.
#[must_use]
pub fn fellowship(view: &dyn GameView) -> Value {
    let Some(f) = view.fellowship() else {
        return Value::Nil;
    };
    Value::map()
        .with("name", f.name.as_str())
        .with("leader", f.leader.0)
        .with("share_xp", f.share_xp)
        .with("even_split", f.even_xp_split)
        .with("open", f.open_fellow)
        .with("locked", f.locked)
        .with(
            "members",
            Value::List(
                f.members
                    .iter()
                    .map(|m| {
                        Value::map()
                            .with("id", m.id.0)
                            .with("name", m.name.as_str())
                            .with("level", m.level)
                            .with("xp_percent", m.xp_percent)
                            .with("health", vitals(m.current_health, m.max_health))
                            .with("stamina", vitals(m.current_stamina, m.max_stamina))
                            .with("mana", vitals(m.current_mana, m.max_mana))
                    })
                    .collect(),
            ),
        )
}

/// The character's allegiance, or nil outside one: `{ name, members, followers, rank, monarch,
/// patron, vassals }`, each person `{ id, name, online, rank, passed_up }`.
#[must_use]
pub fn allegiance(view: &dyn GameView) -> Value {
    let r = view.allegiance_roster();
    if r.monarch.is_none() && r.patron.is_none() && r.vassals.is_empty() && r.total_members == 0 {
        return Value::Nil;
    }
    let person = |e: &dereth_client_contract::view::AllegianceEntry| {
        Value::map()
            .with("id", e.id.0)
            .with("name", e.full_name.as_str())
            .with("online", e.logged_in)
            .with("rank", u32::from(e.rank))
            .with("passed_up", e.cp_cached)
    };
    Value::map()
        .with("name", r.allegiance_name.as_str())
        .with("members", r.total_members)
        .with("followers", r.total_vassals)
        .with("rank", r.player_rank_quality)
        .with("monarch", r.monarch.as_ref().map_or(Value::Nil, person))
        .with("patron", r.patron.as_ref().map_or(Value::Nil, person))
        .with(
            "vassals",
            Value::List(r.vassals.iter().map(person).collect()),
        )
}
