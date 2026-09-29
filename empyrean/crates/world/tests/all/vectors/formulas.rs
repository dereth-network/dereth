//! Vectors: fixtures/vectors/formulas/
//! Pure ACE helpers replay every case of ACE formula/helper vectors.
//! Fixture: ACE vectors and explicit expected values.

use std::cell::Cell;
use std::panic::{self, AssertUnwindSafe};
use std::sync::Once;

use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_common::vectors::{
    self, f32_of, f64_of, i64_of, same_f32, same_f64, throws, u64_of, Case, VectorFile,
};
use empyrean_entity::enums::{
    CombatBodyPart, CoverageMask, DamageType, EquipMask, Gender, HeritageGroup, ItemXpStyle,
    MaterialType, PowerAccuracy,
};
use empyrean_tables::logic::tables::{gem_material_chance, material_table};
use empyrean_world::entity::adjacency::{self, Adjacency};
use empyrean_world::entity::aetheria::{self, AetheriaColor};
use empyrean_world::entity::allegiance_rank as titles;
use empyrean_world::entity::base_damage::BaseDamage;
use empyrean_world::entity::body_part::{self, BodyPart};
use empyrean_world::entity::chess::chess_match;
use empyrean_world::entity::range::Range;
use empyrean_world::entity::spell_formula::{self, Scarab};
use empyrean_world::entity::stamina_table::{self, StaminaCost};
use empyrean_world::entity::{
    core_plating, death_item, experience_system, probability, strings, tailoring,
};
use empyrean_world::world_objects::{
    creature_combat, creature_rating, player_crafting, skill_check, skill_formula,
    world_object_weapon,
};
use serde_json::Value;

// ---------------------------------------------------------------------------------- harness

thread_local!(static QUIET: Cell<bool> = const { Cell::new(false) });
static HOOK: Once = Once::new();

/// Runs `f`, turning a panic (an ACE exception) into `Err`, without printing it.
fn catch<R>(f: impl FnOnce() -> R) -> Result<R, ()> {
    HOOK.call_once(|| {
        let previous = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            if !QUIET.with(Cell::get) {
                previous(info);
            }
        }));
    });
    QUIET.with(|q| q.set(true));
    let r = panic::catch_unwind(AssertUnwindSafe(f));
    QUIET.with(|q| q.set(false));
    r.map_err(|_| ())
}

/// Collects mismatches over many cases and fails once, listing them.
struct Report {
    name: String,
    total: usize,
    failures: Vec<String>,
}

impl Report {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            total: 0,
            failures: Vec::new(),
        }
    }

    fn check(&mut self, case: &Case, ok: bool, got: impl std::fmt::Debug) {
        self.total += 1;
        if !ok {
            self.failures.push(format!(
                "in {} expected {} got {got:?}",
                case.input, case.output
            ));
        }
    }

    fn finish(self) {
        assert!(self.total > 0, "{}: no cases ran", self.name);
        if !self.failures.is_empty() {
            let shown: Vec<_> = self.failures.iter().take(25).cloned().collect();
            panic!(
                "{}: {} of {} cases differ from ACE:\n  {}",
                self.name,
                self.failures.len(),
                self.total,
                shown.join("\n  ")
            );
        }
    }
}

fn load(area: &str, name: &str) -> VectorFile {
    vectors::load_named(area, name)
}

/// Replays `<area>/<name>.json`, calling `each` for every case.
fn replay(area: &str, name: &str, mut each: impl FnMut(&mut Report, &Case)) {
    let file = load(area, name);
    let mut report = Report::new(&format!("{area}/{name}"));
    for case in &file.cases {
        each(&mut report, case);
    }
    report.finish();
}

fn i32_in(v: &Value, key: &str) -> i32 {
    i32::try_from(i64_of(&v[key]).unwrap_or_else(|| panic!("{key}: not an integer in {v}")))
        .expect("fits i32")
}

fn u32_in(v: &Value, key: &str) -> u32 {
    u32::try_from(
        u64_of(&v[key]).unwrap_or_else(|| panic!("{key}: not an unsigned integer in {v}")),
    )
    .expect("fits u32")
}

fn u64_in(v: &Value, key: &str) -> u64 {
    u64_of(&v[key]).unwrap_or_else(|| panic!("{key}: not an unsigned integer in {v}"))
}

fn f32_in(v: &Value, key: &str) -> f32 {
    f32_of(&v[key]).unwrap_or_else(|| panic!("{key}: not a float in {v}"))
}

fn bool_in(v: &Value, key: &str) -> bool {
    v[key]
        .as_bool()
        .unwrap_or_else(|| panic!("{key}: not a bool in {v}"))
}

fn f32_list(v: &Value) -> Vec<f32> {
    v.as_array()
        .expect("array")
        .iter()
        .map(|x| f32_of(x).expect("float"))
        .collect()
}

fn f32_out(case: &Case) -> f32 {
    f32_of(&case.output).expect("float out")
}

fn f64_out(case: &Case) -> f64 {
    f64_of(&case.output).expect("double out")
}

fn i64_out(v: &Value) -> i64 {
    i64_of(v).expect("integer out")
}

// ------------------------------------------------------------------------ formulas/: SkillCheck

#[test]
fn skill_check_get_skill_chance() {
    replay("formulas", "skill_check_get_skill_chance", |r, c| {
        let got = skill_check::get_skill_chance(
            i32_in(&c.input, "skill"),
            i32_in(&c.input, "difficulty"),
            f32_in(&c.input, "factor"),
        );
        r.check(c, same_f64(got, f64_out(c)), got);
    });
}

#[test]
fn skill_check_get_skill_chance_uint() {
    replay("formulas", "skill_check_get_skill_chance_uint", |r, c| {
        let got = skill_check::get_skill_chance_uint(
            u32_in(&c.input, "skill"),
            u32_in(&c.input, "difficulty"),
            f32_in(&c.input, "factor"),
        );
        r.check(c, same_f64(got, f64_out(c)), got);
    });
}

#[test]
fn skill_check_default_factor_is_three_hundredths() {
    // Hand-derived (the vectors pass the factor explicitly): ACE's default argument is 0.03f.
    assert!(same_f32(skill_check::DEFAULT_FACTOR, 0.03));
}

#[test]
fn skill_check_get_magic_skill_chance() {
    replay("formulas", "skill_check_get_magic_skill_chance", |r, c| {
        let got = skill_check::get_magic_skill_chance(
            i32_in(&c.input, "skill"),
            i32_in(&c.input, "difficulty"),
        );
        r.check(c, same_f64(got, f64_out(c)), got);
    });
}

// ---------------------------------------------------------------------- formulas/: SkillFormula

#[test]
fn skill_formula_get_attribute_mod() {
    replay("formulas", "skill_formula_get_attribute_mod", |r, c| {
        let got = skill_formula::get_attribute_mod(
            i32_in(&c.input, "current_skill"),
            bool_in(&c.input, "is_bow"),
        );
        r.check(c, same_f32(got, f32_out(c)), got);
    });
}

#[test]
fn skill_formula_calc_armor_mod() {
    replay("formulas", "skill_formula_calc_armor_mod", |r, c| {
        let got = skill_formula::calc_armor_mod(f32_in(&c.input, "armor_level"));
        r.check(c, same_f32(got, f32_out(c)), got);
    });
}

// -------------------------------------------------------------------- formulas/: Creature rating

#[test]
fn creature_get_positive_rating_mod() {
    replay("formulas", "creature_get_positive_rating_mod", |r, c| {
        let got = creature_rating::get_positive_rating_mod(i32_in(&c.input, "rating"));
        r.check(c, same_f32(got, f32_out(c)), got);
    });
}

#[test]
fn creature_get_negative_rating_mod() {
    replay("formulas", "creature_get_negative_rating_mod", |r, c| {
        let got = creature_rating::get_negative_rating_mod(
            i32_in(&c.input, "rating"),
            bool_in(&c.input, "allow_bug"),
        );
        r.check(c, same_f32(got, f32_out(c)), got);
    });
}

#[test]
fn creature_mod_to_rating() {
    replay("formulas", "creature_mod_to_rating", |r, c| {
        let m = f32_in(&c.input, "mod");
        let got = (
            creature_rating::mod_to_rating(m),
            creature_rating::negative_mod_to_rating(m),
        );
        let want = (
            i64_out(&c.output["mod_to_rating"]),
            i64_out(&c.output["negative_mod_to_rating"]),
        );
        r.check(c, (i64::from(got.0), i64::from(got.1)) == want, got);
    });
}

#[test]
fn creature_additive_combine() {
    // Also exercises the private `GetRatingMod`, which `AdditiveCombine` returns through.
    replay("formulas", "creature_additive_combine", |r, c| {
        let got = creature_rating::additive_combine(&f32_list(&c.input["mods"]));
        r.check(c, same_f32(got, f32_out(c)), got);
    });
}

#[test]
fn creature_get_rating_mod_hand_derived() {
    // Hand-derived from Creature_Rating.cs: 0 -> 1.0; +r -> (100 + r) / 100; -r -> 100 / (100 + r).
    assert!(same_f32(creature_rating::get_rating_mod(0), 1.0));
    assert!(same_f32(creature_rating::get_rating_mod(25), 1.25));
    assert!(same_f32(
        creature_rating::get_rating_mod(-25),
        100.0 / 125.0
    ));
}

// ---------------------------------------------------------- formulas/: WorldObject, chess, player

#[test]
fn world_object_get_interval() {
    replay("formulas", "world_object_get_interval", |r, c| {
        let got = world_object_weapon::get_interval(
            i32_in(&c.input, "num"),
            i32_in(&c.input, "min"),
            i32_in(&c.input, "max"),
        );
        r.check(c, same_f32(got, f32_out(c)), got);
    });
}

#[test]
fn world_object_set_interval() {
    replay("formulas", "world_object_set_interval", |r, c| {
        let got = world_object_weapon::set_interval(
            f32_in(&c.input, "interval"),
            f32_in(&c.input, "min"),
            f32_in(&c.input, "max"),
        );
        r.check(c, same_f32(got, f32_out(c)), got);
    });
}

#[test]
fn chess_match_expectation_to_win() {
    replay("formulas", "chess_match_expectation_to_win", |r, c| {
        let got =
            chess_match::expectation_to_win(i32_in(&c.input, "rank"), i32_in(&c.input, "op_rank"));
        r.check(c, same_f64(got, f64_out(c)), got);
    });
}

#[test]
fn player_calc_num_units() {
    replay("formulas", "player_calc_num_units", |r, c| {
        let got = player_crafting::calc_num_units(
            i32_in(&c.input, "skill"),
            f32_in(&c.input, "workmanship"),
            i32_in(&c.input, "num_augs"),
        );
        r.check(c, i64::from(got) == i64_out(&c.output), got);
    });
}

// ------------------------------------------------------------- formulas/: ExperienceSystem, stamina

fn xp_style(v: &Value) -> ItemXpStyle {
    ItemXpStyle::from_name(v["xp_scheme"].as_str().expect("xp_scheme")).expect("ItemXpStyle name")
}

#[test]
fn experience_system_item_level_to_total_xp() {
    replay(
        "formulas",
        "experience_system_item_level_to_total_xp",
        |r, c| {
            let got = experience_system::item_level_to_total_xp(
                i32_in(&c.input, "item_level"),
                u64_in(&c.input, "base_xp"),
                i32_in(&c.input, "max_level"),
                xp_style(&c.input),
            );
            // V379: retail's curve now. ACE's recorded output must still match wherever an item can be:
            // a style content uses, a real base and max level, and a total that does not wrap.
            let reachable = content_style(&c.input)
                && u64_in(&c.input, "base_xp") > 0
                && i32_in(&c.input, "max_level") >= 1
                && u64_of(&c.output).is_some_and(|v| v < 1 << 62);
            r.check(c, !reachable || Some(got) == u64_of(&c.output), got);
        },
    );
}

#[test]
fn experience_system_item_total_xp_to_level() {
    replay(
        "formulas",
        "experience_system_item_total_xp_to_level",
        |r, c| {
            let got = experience_system::item_total_xp_to_level(
                u64_in(&c.input, "gained_xp"),
                u64_in(&c.input, "base_xp"),
                i32_in(&c.input, "max_level"),
                xp_style(&c.input),
            );
            // V379: as above; a level quotient past `i32` is out of range.
            let gained = u64_in(&c.input, "gained_xp");
            let base = u64_in(&c.input, "base_xp");
            let reachable = content_style(&c.input)
                && base > 0
                && i32_in(&c.input, "max_level") >= 1
                && gained / base < 1 << 31;
            r.check(c, !reachable || i64::from(got) == i64_out(&c.output), got);
        },
    );
}

/// The item XP styles world content uses (133 Fixed, 18 ScalesWithLevel; none FixedPlusBase).
fn content_style(input: &Value) -> bool {
    matches!(
        xp_style(input),
        ItemXpStyle::Fixed | ItemXpStyle::ScalesWithLevel
    )
}

#[test]
fn stamina_table_get_stamina_cost() {
    replay("formulas", "stamina_table_get_stamina_cost", |r, c| {
        let pa = PowerAccuracy::from_name(c.input["power_accuracy"].as_str().expect("name"))
            .expect("PowerAccuracy");
        let got = stamina_table::get_stamina_cost(pa, i32_in(&c.input, "burden"));
        r.check(c, same_f32(got, f32_out(c)), got);
    });
}

#[test]
fn stamina_table_build_table_and_cost_steps() {
    // Hand-derived from StaminaTable.BuildTable: three descending steps per bar.
    let t = stamina_table::build_table();
    assert_eq!(t.len(), 3);
    assert_eq!(
        t[&PowerAccuracy::Medium],
        vec![
            StaminaCost::new(1600, 3.0),
            StaminaCost::new(1200, 2.0),
            StaminaCost::new(700, 1.0)
        ]
    );
    assert_eq!(
        stamina_table::COSTS[&PowerAccuracy::Low][0],
        StaminaCost {
            burden: 1600,
            stamina: 1.5
        }
    );
    // Costs[powerAccuracy] throws KeyNotFoundException for any other value.
    assert!(catch(|| stamina_table::get_stamina_cost(PowerAccuracy(0), 100)).is_err());
}

fn material(v: &Value) -> Option<MaterialType> {
    if v["material_type"].is_null() {
        None
    } else {
        Some(MaterialType(u32_in(v, "value")))
    }
}

#[test]
fn material_table_get_value_mod() {
    replay("formulas", "material_table_get_value_mod", |r, c| {
        let got = material_table::get_value_mod(material(&c.input));
        r.check(c, same_f32(got, f32_out(c)), got);
    });
}

#[test]
fn gem_material_chance_gem_value() {
    replay("formulas", "gem_material_chance_gem_value", |r, c| {
        let got = gem_material_chance::gem_value(material(&c.input));
        r.check(c, i64::from(got) == i64_out(&c.output), got);
    });
}

// ------------------------------------------------------------------------ helpers/: AllegianceTitle

fn title_case(c: &Case) -> (HeritageGroup, Gender, u32) {
    (
        HeritageGroup(i32_in(&c.input, "heritage")),
        Gender(i32_in(&c.input, "gender")),
        u32_in(&c.input, "rank"),
    )
}

#[test]
fn allegiance_title_get_title() {
    replay("helpers", "allegiance_title_get_title", |r, c| {
        let (h, g, rank) = title_case(c);
        let got = titles::get_title(h, g, rank);
        r.check(c, c.output.as_str() == Some(got), got);
    });
}

#[test]
fn allegiance_title_per_gender_and_heritage_methods() {
    // Each public title method, checked against the GetTitle rows that dispatch to it.
    type TitleFn = fn(u32) -> &'static str;
    let table: &[(HeritageGroup, Gender, TitleFn)] = &[
        (
            HeritageGroup::Aluvian,
            Gender::Male,
            titles::get_aluvian_male_title,
        ),
        (
            HeritageGroup::Aluvian,
            Gender::Female,
            titles::get_aluvian_female_title,
        ),
        (
            HeritageGroup::Gharundim,
            Gender::Male,
            titles::get_gharundim_male_title,
        ),
        (
            HeritageGroup::Gharundim,
            Gender::Female,
            titles::get_gharundim_female_title,
        ),
        (HeritageGroup::Sho, Gender::Male, titles::get_sho_male_title),
        (
            HeritageGroup::Sho,
            Gender::Female,
            titles::get_sho_female_title,
        ),
        (
            HeritageGroup::Viamontian,
            Gender::Male,
            titles::get_viamontian_male_title,
        ),
        (
            HeritageGroup::Viamontian,
            Gender::Female,
            titles::get_viamontian_female_title,
        ),
        (
            HeritageGroup::Shadowbound,
            Gender::Male,
            titles::get_shadowbound_male_title,
        ),
        (
            HeritageGroup::Shadowbound,
            Gender::Female,
            titles::get_shadowbound_female_title,
        ),
        (
            HeritageGroup::Tumerok,
            Gender::Male,
            titles::get_tumerok_title,
        ),
        (
            HeritageGroup::Gearknight,
            Gender::Female,
            titles::get_gearknight_title,
        ),
        (
            HeritageGroup::Lugian,
            Gender::Male,
            titles::get_lugian_title,
        ),
        (
            HeritageGroup::Empyrean,
            Gender::Male,
            titles::get_empyrean_male_title,
        ),
        (
            HeritageGroup::Empyrean,
            Gender::Female,
            titles::get_empyrean_female_title,
        ),
        (
            HeritageGroup::Undead,
            Gender::Male,
            titles::get_undead_male_title,
        ),
        (
            HeritageGroup::Undead,
            Gender::Female,
            titles::get_undead_female_title,
        ),
    ];
    replay("helpers", "allegiance_title_get_title", |r, c| {
        let (h, g, rank) = title_case(c);
        if g == Gender::Male || g == Gender::Female {
            let by_gender = if g == Gender::Male {
                titles::get_male_title(h, rank)
            } else {
                titles::get_female_title(h, rank)
            };
            r.check(c, c.output.as_str() == Some(by_gender), by_gender);
        }
        if let Some(&(_, _, f)) = table.iter().find(|&&(th, tg, _)| th == h && tg == g) {
            let got = f(rank);
            r.check(c, c.output.as_str() == Some(got), got);
        }
    });
}

// --------------------------------------------------------------------------- helpers/: the rest

#[test]
fn adjacency_helper_get_inverse() {
    replay("helpers", "adjacency_helper_get_inverse", |r, c| {
        let got = adjacency::get_inverse(Adjacency(i32_in(&c.input, "adj")));
        let want = if c.output.is_null() {
            None
        } else {
            Some(Adjacency(i32::try_from(i64_out(&c.output)).unwrap()))
        };
        r.check(c, got == want, got);
    });
}

#[test]
fn probability_none_and_any() {
    replay("helpers", "probability", |r, c| {
        let chances = f32_list(&c.input["chances"]);
        let got = (
            probability::get_probability_none(&chances),
            probability::get_probability_any(&chances),
        );
        let ok = same_f32(got.0, f32_of(&c.output["none"]).unwrap())
            && same_f32(got.1, f32_of(&c.output["any"]).unwrap());
        r.check(c, ok, got);
    });
}

#[test]
fn range_avg_and_to_string() {
    replay("helpers", "range", |r, c| {
        let range = if c.input["min"].is_null() {
            Range::default()
        } else {
            Range::new(f32_in(&c.input, "min"), f32_in(&c.input, "max"))
        };
        let text = range.to_string();
        let ok = same_f32(range.avg, f32_of(&c.output["avg"]).unwrap())
            && same_f32(range.min, f32_of(&c.output["min"]).unwrap())
            && same_f32(range.max, f32_of(&c.output["max"]).unwrap())
            && c.output["to_string"].as_str() == Some(text.as_str());
        r.check(c, ok, (range, text));
    });
}

#[test]
fn base_damage_min_damage() {
    replay("helpers", "base_damage_min_damage", |r, c| {
        let bd = BaseDamage::new(i32_in(&c.input, "max_damage"), f32_in(&c.input, "variance"));
        let got = bd.min_damage();
        r.check(c, same_f32(got, f32_out(c)), got);
    });
}

#[test]
fn world_object_get_rend_damage_type() {
    replay("helpers", "world_object_get_rend_damage_type", |r, c| {
        let got =
            world_object_weapon::get_rend_damage_type(DamageType(i32_in(&c.input, "damage_type")));
        r.check(c, i64::from(got.0) == i64_out(&c.output), got);
    });
}

#[test]
fn creature_get_resistance_type() {
    replay("helpers", "creature_get_resistance_type", |r, c| {
        let got = creature_combat::get_resistance_type(DamageType(i32_in(&c.input, "damage_type")));
        r.check(c, i64::from(got.0) == i64_out(&c.output), got);
    });
}

#[test]
fn aetheria_is_aetheria_and_get_color() {
    replay("helpers", "aetheria", |r, c| {
        let wcid = u32_in(&c.input, "wcid");
        let got = (aetheria::is_aetheria(wcid), aetheria::get_color(wcid));
        let want_color = if c.output["get_color"].is_null() {
            None
        } else {
            Some(i64_out(&c.output["get_color"]))
        };
        let ok = Some(got.0) == c.output["is_aetheria"].as_bool()
            && got.1.map(|x: AetheriaColor| x as i64) == want_color;
        r.check(c, ok, got);
    });
}

#[test]
fn core_plating_is_integrator_and_deintegrator() {
    replay("helpers", "core_plating", |r, c| {
        let wcid = u32_in(&c.input, "wcid");
        let got = (
            core_plating::is_integrator(wcid),
            core_plating::is_deintegrator(wcid),
        );
        let ok = Some(got.0) == c.output["is_integrator"].as_bool()
            && Some(got.1) == c.output["is_deintegrator"].as_bool();
        r.check(c, ok, got);
    });
}

#[test]
fn core_plating_get_gear_plating_name() {
    replay("helpers", "core_plating_get_gear_plating_name", |r, c| {
        let got = core_plating::get_gear_plating_name(EquipMask(u32_in(&c.input, "locations")));
        r.check(c, c.output.as_str() == Some(got.as_str()), got);
    });
}

#[test]
fn tailoring_get_armor_wcid() {
    replay("helpers", "tailoring_get_armor_wcid", |r, c| {
        let got = tailoring::get_armor_wcid(EquipMask(u32_in(&c.input, "valid_locations")));
        r.check(c, got.map(u64::from) == u64_of(&c.output), got);
    });
}

#[test]
fn tailoring_is_tailoring_kit() {
    replay("helpers", "tailoring_is_tailoring_kit", |r, c| {
        let got = tailoring::is_tailoring_kit(u32_in(&c.input, "wcid"));
        r.check(c, Some(got) == c.output.as_bool(), got);
    });
}

#[test]
fn spell_formula_scarabs() {
    replay("helpers", "spell_formula_scarab", |r, c| {
        let id = u32_in(&c.input, "component_id");
        #[allow(clippy::cast_possible_wrap)]
        let scarab = Scarab(id as i32);
        let level = catch(|| spell_formula::get_level(scarab));
        let power = catch(|| spell_formula::get_power(scarab));
        let same = |got: Result<u32, ()>, want: &Value| match (got, throws(want)) {
            (Err(()), Some(t)) => t.ends_with("KeyNotFoundException"),
            (Ok(v), None) => u64_of(want) == Some(u64::from(v)),
            _ => false,
        };
        let is = spell_formula::is_scarab(id);
        let ok = same(level, &c.output["get_level"])
            && same(power, &c.output["get_power"])
            && Some(is) == c.output["is_scarab"].as_bool();
        r.check(c, ok, (level, power, is));
    });
}

#[test]
fn strings_get_fall_message() {
    replay("helpers", "strings_get_fall_message", |r, c| {
        let got =
            strings::get_fall_message(u32_in(&c.input, "damage"), u32_in(&c.input, "max_health"));
        r.check(c, c.output.as_str() == Some(got.as_str()), got);
    });
}

#[test]
fn body_parts_get_coverage_mask() {
    replay("helpers", "body_parts_get_coverage_mask", |r, c| {
        let got = body_part::get_coverage_mask(BodyPart(i32_in(&c.input, "body_part")));
        r.check(c, Some(u64::from(got.0)) == u64_of(&c.output), got);
    });
}

#[test]
fn body_parts_get_coverage_mask_combat() {
    replay("helpers", "body_parts_get_coverage_mask_combat", |r, c| {
        let got =
            body_part::get_coverage_mask_combat(CombatBodyPart(i32_in(&c.input, "body_part")));
        r.check(c, Some(u64::from(got.0)) == u64_of(&c.output), got);
    });
}

#[test]
fn body_parts_get_flags() {
    replay("helpers", "body_parts_get_flags", |r, c| {
        let got: Vec<i64> = body_part::get_flags(BodyPart(i32_in(&c.input, "body_parts")))
            .iter()
            .map(|p| i64::from(p.0))
            .collect();
        let want: Vec<i64> = c.output.as_array().unwrap().iter().map(i64_out).collect();
        r.check(c, got == want, got);
    });
}

#[test]
fn body_parts_get_flags_coverage() {
    replay("helpers", "body_parts_get_flags_coverage", |r, c| {
        let got: Vec<u64> =
            body_part::get_flags_coverage(CoverageMask(u32_in(&c.input, "coverage")))
                .iter()
                .map(|p| u64::from(p.0))
                .collect();
        let want: Vec<u64> = c
            .output
            .as_array()
            .unwrap()
            .iter()
            .map(|v| u64_of(v).unwrap())
            .collect();
        r.check(c, got == want, got);
    });
}

#[test]
fn body_parts_has_any() {
    replay("helpers", "body_parts_has_any", |r, c| {
        let coverage = if c.input["coverage"].is_null() {
            None
        } else {
            Some(CoverageMask(u32_in(&c.input, "coverage")))
        };
        let flags: Vec<CoverageMask> = c.input["flags"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| CoverageMask(u32::try_from(u64_of(v).unwrap()).unwrap()))
            .collect();
        let got = body_part::has_any(coverage, &flags);
        r.check(c, Some(got) == c.output.as_bool(), got);
    });
}

#[test]
fn body_parts_statics_hand_derived() {
    // Hand-derived from BodyPart.cs (plain data, no vector needed).
    assert_eq!(
        body_part::UPPER,
        BodyPart::Head | BodyPart::Chest | BodyPart::UpperArm
    );
    assert_eq!(body_part::LOWER.0, 0x180);
    assert_eq!(body_part::MID.0, 0x7E);
    assert_eq!(body_part::INDICES[&BodyPart::Foot], 8);
    assert_eq!(body_part::INDICES.len(), 9);
}

#[test]
fn death_items_get_value_with_variance() {
    // One `ThreadSafeRandom.Next(float, float)` draw per call, from a seeded generator on both sides.
    replay("helpers", "death_items_get_value_with_variance", |r, c| {
        ThreadSafeRandom::seed(u64_in(&c.input, "seed"));
        let values: Vec<i32> = c.input["values"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| i32::try_from(i64_out(v)).unwrap())
            .collect();
        let mut got = Vec::new();
        for _ in 0..i32_in(&c.input, "rounds") {
            for &v in &values {
                got.push(i64::from(death_item::get_value_with_variance(v)));
            }
        }
        let want: Vec<i64> = c.output.as_array().unwrap().iter().map(i64_out).collect();
        r.check(c, got == want, got);
    });
}

// ------------------------------------- Shared rules: cross-checks against the shared rules and dereth-client-model

/// `dereth_rules::advancement`'s curve number for an `ItemXpStyle` (1 linear, 2 geometric, 3 the
/// third closed-form curve); both sides number them from the same client enum.
fn curve(style: ItemXpStyle) -> i32 {
    style.0
}

/// Every `(style, level, base, max)` in the sweep where the two implementations disagree.
fn item_xp_disagreements(styles: &[ItemXpStyle]) -> Vec<String> {
    let mut out = Vec::new();
    for &style in styles {
        for base in [1u64, 10, 1000, 12_345] {
            for max in 1..=12i32 {
                for level in 0..=14i32 {
                    let ours = experience_system::item_level_to_total_xp(level, base, max, style);
                    let theirs = dereth_rules::advancement::item_level_to_total_xp(
                        level,
                        base,
                        max,
                        curve(style),
                    );
                    if ours != theirs {
                        out.push(format!("LevelToTotalXP {style:?} level {level} base {base} max {max}: ACE {ours}, dereth-rules {theirs}"));
                    }
                }
                for gained in (0..=40u64)
                    .map(|k| k * base / 2)
                    .chain([base * 1000, base * 5000 + 7])
                {
                    let ours = experience_system::item_total_xp_to_level(gained, base, max, style);
                    let theirs = dereth_rules::advancement::item_total_xp_to_level(
                        gained,
                        base,
                        max,
                        curve(style),
                    );
                    if ours != theirs {
                        out.push(format!("TotalXPToLevel {style:?} gained {gained} base {base} max {max}: ACE {ours}, dereth-rules {theirs}"));
                    }
                }
            }
        }
    }
    out
}

#[test]
fn rule3_item_xp_fixed_and_scales_with_level_agree_with_dereth_rules() {
    let bad = item_xp_disagreements(&[ItemXpStyle::Fixed, ItemXpStyle::ScalesWithLevel]);
    assert!(
        bad.is_empty(),
        "{} disagreements:\n  {}",
        bad.len(),
        bad.iter()
            .take(20)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}

#[test]
/// item xp fixed plus base and undef agree with dereth rules.
/// V379.
fn rule3_item_xp_fixed_plus_base_and_undef_agree_with_dereth_rules() {
    let bad = item_xp_disagreements(&[ItemXpStyle::FixedPlusBase, ItemXpStyle::Undef]);
    assert!(
        bad.is_empty(),
        "{} disagreements:\n  {}",
        bad.len(),
        bad.iter()
            .take(20)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}

#[test]
fn rule3_allegiance_titles_agree_with_dereth_client_model() {
    let mut bad = Vec::new();
    for h in -1..=14i32 {
        for g in -1..=3i32 {
            for rank in 0..=12u32 {
                let ours = titles::get_title(HeritageGroup(h), Gender(g), rank);
                let theirs = match (u8::try_from(h), u8::try_from(g)) {
                    (Ok(h8), Ok(g8)) => dereth_client_model::allegiance::get_title(
                        u16::try_from(rank).unwrap(),
                        h8,
                        g8,
                    )
                    .unwrap_or(""),
                    _ => "",
                };
                if ours != theirs {
                    bad.push(format!(
                        "heritage {h} gender {g} rank {rank}: ACE {ours:?}, dereth-client-model {theirs:?}"
                    ));
                }
            }
        }
    }
    assert!(
        bad.is_empty(),
        "{} disagreements:\n  {}",
        bad.len(),
        bad.iter()
            .take(20)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}
