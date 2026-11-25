//! The game's tables as the Tables tab shows and changes them ([`crate::tables`]): the skills with
//! their costs and formulas, the heritages with their credits, skill costs and templates, the
//! experience curve with its raise costs, and the creature types' names. Each is shown as the world has it; a change is the pack
//! being edited's row of it, in its `records/` ([`crate::table_edit`]).
//!
//! The experience curve never falls, which the tables' build refuses; the tab refuses such a value
//! as it is typed ([`curve_refuses`]), before anything is written. A higher level cap adds levels,
//! and each added level is given its experience ([`raised`]): the curve's last step, continued.

use dereth_assets::tables::{CharGen, EnumMapper, SkillFormula, SkillTable, XpTable};
use toml::Value as T;

use crate::table_edit::key_of;

/// The records file of each page's rows.
pub const SKILLS_FILE: &str = "skills.toml";
pub const HERITAGES_FILE: &str = crate::starter_areas::FILE;
pub const EXPERIENCE_FILE: &str = "experience.toml";
pub const CREATURE_TYPES_FILE: &str = "creature-types.toml";

/// The attributes as formulas number them.
pub const ATTRIBUTES: [(u32, &str); 6] = [
    (1, "Strength"),
    (2, "Endurance"),
    (3, "Quickness"),
    (4, "Coordination"),
    (5, "Focus"),
    (6, "Self"),
];

/// A template's attributes, in the record's order, by their row fields.
pub const TEMPLATE_ATTRIBUTES: [&str; 6] = [
    "strength",
    "endurance",
    "coordination",
    "quickness",
    "focus",
    "self",
];

fn attribute(n: u32) -> Option<&'static str> {
    ATTRIBUTES.iter().find(|(k, _)| *k == n).map(|(_, a)| *a)
}

/// One skill as the world has it.
#[derive(Clone, Debug, PartialEq)]
pub struct ShownSkill {
    pub id: u32,
    pub name: String,
    pub trained_cost: i32,
    pub specialized_cost: i32,
    pub formula: SkillFormula,
}

/// The named skills of `t`, by name.
#[must_use]
pub fn skills(t: &SkillTable) -> Vec<ShownSkill> {
    let mut v: Vec<ShownSkill> = t
        .skills
        .iter()
        .filter(|(_, s)| !s.name.trim().is_empty())
        .map(|(id, s)| ShownSkill {
            id: *id,
            name: s.name.clone(),
            trained_cost: s.trained_cost,
            specialized_cost: s.specialized_cost,
            formula: s.formula,
        })
        .collect();
    v.sort_by(|a, b| a.name.cmp(&b.name));
    v
}

/// A formula in words: `(Strength + Coordination) / 3`, `2 × Focus / 3`, `none`.
#[must_use]
pub fn formula_words(f: &SkillFormula) -> String {
    let term = |m: u32, a: u32| {
        attribute(a).map(|n| {
            if m == 1 {
                n.to_owned()
            } else {
                format!("{m} × {n}")
            }
        })
    };
    let mut parts: Vec<String> = [term(f.x, f.attr1), term(f.y, f.attr2)]
        .into_iter()
        .flatten()
        .collect();
    if f.w > 0 {
        parts.push(f.w.to_string());
    }
    if parts.is_empty() || f.z == 0 {
        return "none".into();
    }
    let sum = parts.join(" + ");
    match (parts.len(), f.z) {
        (_, 1) => sum,
        (1, d) => format!("{sum} / {d}"),
        (_, d) => format!("({sum}) / {d}"),
    }
}

/// A formula as a `[[skill]]` row writes it.
#[must_use]
pub fn formula_row(attributes: &[&str], multipliers: &[u32], add: u32, divisor: u32) -> T {
    let mut t = toml::Table::new();
    t.insert(
        "attributes".into(),
        T::Array(
            attributes
                .iter()
                .map(|a| T::String((*a).to_owned()))
                .collect(),
        ),
    );
    if multipliers.iter().any(|m| *m != 1) {
        t.insert(
            "multipliers".into(),
            T::Array(
                multipliers
                    .iter()
                    .map(|m| T::Integer(i64::from(*m)))
                    .collect(),
            ),
        );
    }
    if add > 0 {
        t.insert("add".into(), T::Integer(i64::from(add)));
    }
    t.insert("divisor".into(), T::Integer(i64::from(divisor)));
    T::Table(t)
}

/// The key of skill `id`'s row in records file `text`: its number, when the file's rows add it
/// (`id = …`), else its name.
#[must_use]
pub fn skill_key(text: Option<&str>, id: u32, name: &str) -> Vec<(&'static str, T)> {
    let by_id = vec![("id", T::Integer(i64::from(id)))];
    if crate::table_edit::row(text, "skill", &by_id)
        .ok()
        .flatten()
        .is_some()
    {
        by_id
    } else {
        vec![("skill", T::String(name.to_owned()))]
    }
}

/// The lowest skill number `t` doesn't use, for a new skill.
#[must_use]
pub fn free_skill(t: &SkillTable) -> u32 {
    (1..)
        .find(|n| !t.skills.contains_key(n))
        .unwrap_or(u32::MAX)
}

/// One template ("class") of a heritage.
#[derive(Clone, Debug, PartialEq)]
pub struct ShownTemplate {
    pub name: String,
    /// In [`TEMPLATE_ATTRIBUTES`]' order.
    pub attributes: [u32; 6],
    pub trained: Vec<String>,
    pub specialized: Vec<String>,
}

/// One heritage as the world has it.
#[derive(Clone, Debug, PartialEq)]
pub struct ShownHeritage {
    pub name: String,
    pub attribute_credits: u32,
    pub skill_credits: u32,
    /// The skills it offers at creation: each by name, with its trained and specialized cost.
    pub skills: Vec<(String, i32, i32)>,
    pub templates: Vec<ShownTemplate>,
}

/// The heritages of `cg`, in its order, their skills named from `t`.
#[must_use]
pub fn heritages(cg: &CharGen, t: &SkillTable) -> Vec<ShownHeritage> {
    let name = |id: u32| {
        t.skills
            .get(&id)
            .map_or_else(|| format!("skill {id}"), |s| s.name.clone())
    };
    cg.heritage_order
        .iter()
        .filter_map(|k| cg.heritage_groups.get(k))
        .map(|h| ShownHeritage {
            name: h.name.clone(),
            attribute_credits: h.attribute_credits,
            skill_credits: h.skill_credits,
            skills: h
                .skills
                .iter()
                .map(|(id, tc, sc)| (name(*id), *tc, *sc))
                .collect(),
            templates: h
                .templates
                .iter()
                .map(|x| ShownTemplate {
                    name: x.name.clone(),
                    attributes: x.attributes,
                    trained: x.normal_skills.iter().map(|id| name(*id)).collect(),
                    specialized: x.primary_skills.iter().map(|id| name(*id)).collect(),
                })
                .collect(),
        })
        .collect()
}

/// Why level `n` can't take `xp` experience on curve `levels` (each level's experience, from
/// level 0): the level below it takes more, or the level above less. The curve never falls.
#[must_use]
pub fn curve_refuses(levels: &[u64], n: usize, xp: u64) -> Option<(usize, u64)> {
    if n >= 2 && levels.get(n - 1).is_some_and(|below| xp < *below) {
        return Some((n - 1, levels[n - 1]));
    }
    levels
        .get(n + 1)
        .filter(|above| n >= 1 && xp > **above)
        .map(|above| (n + 1, *above))
}

/// The levels a cap of `max` adds to curve `levels`, each with its experience: the curve's last
/// step continued (the step from its last level but one; none, for a curve of one level).
#[must_use]
pub fn raised(levels: &[u64], max: usize) -> Vec<(usize, u64)> {
    let last = levels.len().saturating_sub(1);
    let top = levels.last().copied().unwrap_or(0);
    let step = match levels.len() {
        0..=2 => 0,
        n => levels[n - 1].saturating_sub(levels[n - 2]),
    };
    (last + 1..=max)
        .map(|n| {
            (
                n,
                top.saturating_add(step.saturating_mul((n - last) as u64)),
            )
        })
        .collect()
}

/// The curve of `t`: each level's experience and the skill credits it gives, from level 0.
#[must_use]
pub fn curve(t: &XpTable) -> Vec<(u64, u32)> {
    t.level_xp
        .iter()
        .zip(t.level_credits.iter().chain(std::iter::repeat(&0)))
        .map(|(x, c)| (*x, *c))
        .collect()
}

/// The creature types of `m`, by number, each with the name a player reads.
#[must_use]
pub fn creature_types(m: &EnumMapper) -> Vec<(u32, String)> {
    let mut v: Vec<(u32, String)> = m
        .id_to_string
        .iter()
        .map(|(id, n)| (*id, crate::tables::creature_type_shown(n)))
        .collect();
    v.sort_by_key(|(id, _)| *id);
    v
}

/// The number of a new creature type: one past the highest of `m`.
#[must_use]
pub fn free_creature_type(m: &EnumMapper) -> u32 {
    m.id_to_string
        .iter()
        .map(|(id, _)| id.saturating_add(1))
        .max()
        .unwrap_or(1)
}

/// The key of creature type `id`'s row in records file `text`: by its name (`name`, as a player
/// reads it), when the file's row names it so, else by its number.
#[must_use]
pub fn creature_type_key(text: Option<&str>, id: u32, name: &str) -> Vec<(&'static str, T)> {
    for n in [name.to_owned(), crate::tables::creature_type_stored(name)] {
        let by_name = vec![("creature-type", T::String(n))];
        if crate::table_edit::row(text, "creature-type", &by_name)
            .ok()
            .flatten()
            .is_some()
        {
            return by_name;
        }
    }
    vec![("id", T::Integer(i64::from(id)))]
}

/// The creature types records file `text`'s rows take out by number.
#[must_use]
pub fn removed_creature_types(text: Option<&str>) -> Vec<u32> {
    let Some(all) = text.and_then(|t| toml::from_str::<toml::Table>(t).ok()) else {
        return Vec::new();
    };
    all.get("creature-type")
        .and_then(T::as_array)
        .into_iter()
        .flatten()
        .filter_map(T::as_table)
        .filter(|r| r.get("remove").and_then(T::as_bool) == Some(true))
        .filter_map(|r| r.get("id")?.as_integer())
        .filter_map(|id| u32::try_from(id).ok())
        .collect()
}

/// Whether `a` names the same thing as `b`, spaces and case aside.
#[must_use]
pub fn same_name(a: &str, b: &str) -> bool {
    key_of(a) == key_of(b)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the world editor's tables)
    use super::*;

    /// A level's experience between the levels around it is taken; less than the one below or more
    /// than the one above is refused, naming that level; levels 0 and 1 take what they will.
    #[test]
    fn the_curve_never_falls_and_a_value_that_would_make_it_is_refused() {
        let c = [0, 0, 1000, 2500, 4500];
        assert_eq!(curve_refuses(&c, 3, 2000), None);
        assert_eq!(curve_refuses(&c, 3, 900), Some((2, 1000)));
        assert_eq!(curve_refuses(&c, 3, 5000), Some((4, 4500)));
        assert_eq!(curve_refuses(&c, 4, 9_000_000), None, "the top may rise");
        assert_eq!(curve_refuses(&c, 1, 0), None);
    }

    /// A higher cap continues the curve's last step for every level it adds; the same or a lower
    /// cap adds none.
    #[test]
    fn a_higher_cap_continues_the_last_step() {
        let c = [0, 0, 1000, 2500];
        assert_eq!(raised(&c, 5), [(4, 4000), (5, 5500)]);
        assert!(raised(&c, 3).is_empty());
        assert!(raised(&c, 2).is_empty());
    }

    /// Creature types are shown by number with their underscores as spaces; a new type takes the
    /// number past the highest; a row keeps the key it was written with, by name or by number; the
    /// types a file takes out are those it removes by number.
    #[test]
    fn creature_types_are_shown_numbered_and_keyed_as_written() {
        let m = EnumMapper {
            id: dereth_primitives::DataId(0x2200_000E),
            base_emp_did: dereth_primitives::DataId(0),
            bucket_index: 3,
            id_to_string: vec![(7, "Fire_Elemental".into()), (2, "Olthoi".into())],
        };
        assert_eq!(
            creature_types(&m),
            [(2, "Olthoi".to_owned()), (7, "Fire Elemental".to_owned())]
        );
        assert_eq!(free_creature_type(&m), 8);
        let text = "[[creature-type]]\ncreature-type = \"Fire_Elemental\"\nname = \"Flame\"\n\n[[creature-type]]\nid = 2\nremove = true\n";
        assert_eq!(
            creature_type_key(Some(text), 7, "Fire Elemental"),
            [("creature-type", T::String("Fire_Elemental".into()))]
        );
        assert_eq!(
            creature_type_key(Some(text), 2, "Olthoi"),
            [("id", T::Integer(2))]
        );
        assert_eq!(removed_creature_types(Some(text)), [2]);
        assert!(removed_creature_types(None).is_empty());
    }

    /// A formula reads as its sum over its divisor, its attributes by name.
    #[test]
    fn a_formula_reads_in_words() {
        let f = |x, y, z, w, a1, a2| SkillFormula {
            w,
            x,
            y,
            z,
            attr1: a1,
            attr2: a2,
        };
        assert_eq!(
            formula_words(&f(1, 1, 3, 0, 1, 4)),
            "(Strength + Coordination) / 3"
        );
        assert_eq!(formula_words(&f(2, 0, 3, 0, 5, 0)), "2 × Focus / 3");
        assert_eq!(formula_words(&f(1, 0, 1, 0, 6, 0)), "Self");
        assert_eq!(formula_words(&f(0, 0, 0, 0, 0, 0)), "none");
        let row = formula_row(&["Strength", "Coordination"], &[1, 1], 0, 3);
        assert_eq!(
            row.to_string(),
            "{ attributes = [\"Strength\", \"Coordination\"], divisor = 3 }"
        );
    }
}
