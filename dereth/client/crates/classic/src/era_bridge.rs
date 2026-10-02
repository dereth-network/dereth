//! Bridges an early 2005 character-creation result onto the final-era creation tables, so the
//! classic creation wizard can create a character on a final-era server (a local Empyrean).
//!
//! This is a convenience, not era parity. The classic wizard's choices are made against the
//! early 2005 tables; the server validates against the final dats. Each field is carried over
//! when the final tables accept it and otherwise replaced by the nearest acceptable value, and
//! every such replacement is reported, so the host can show the player what changed. A server of
//! the classic interface's own era would take the legacy body unchanged instead.
use crate::int::u32_from;
use crate::panels::{pregame::data::CreationData, LegacyCreation};
use dereth_assets::tables::{CharGen, SkillTable};
use dereth_client_contract::pregame::CharGenResultData;
use dereth_rules::chargen::{
    default_skill_class, skill_costs, skill_credits_used, SkillAdvancementClass, TOTAL_NUM_SKILLS,
};

/// The final-era tables the bridge validates against.
#[derive(Debug)]
pub struct FinalTables {
    pub char_gen: CharGen,
    pub skills: SkillTable,
}

/// The final-era creation tables' fixed ids: `CharGen` and the skill table.
const CHAR_GEN_ID: u32 = 0x0E00_0002;
const SKILL_TABLE_ID: u32 = 0x0E00_0004;

impl FinalTables {
    pub fn load(store: &dereth_dat::RetailDatStore) -> Result<Self, String> {
        use dereth_assets::Decode;
        use dereth_primitives::AssetSource;
        // Each table in the layout of the files it is in: the world's own era.
        let read = |id: u32| {
            let id = dereth_primitives::DataId(id);
            store
                .read(id)
                .map(|bytes| (store.era_of(id), id, bytes))
                .map_err(|e| format!("{:#010X}: {e}", id.0))
        };
        let (era, id, bytes) = read(CHAR_GEN_ID)?;
        let char_gen = CharGen::decode_payload_in(era, id, &bytes).map_err(|e| e.to_string())?;
        let (era, id, bytes) = read(SKILL_TABLE_ID)?;
        let skills = SkillTable::decode_payload_in(era, id, &bytes).map_err(|e| e.to_string())?;
        Ok(Self { char_gen, skills })
    }
}

/// The weapon skills the final era retired, with the combined skill that replaced each.
/// Heavy 44, Light 45, Finesse 46, Missile 47.
const RETIRED_WEAPONS: [(u32, u32); 11] = [
    (1, 44),  // Axe
    (5, 44),  // Mace
    (9, 44),  // Spear
    (11, 44), // Sword
    (10, 45), // Staff
    (13, 45), // Unarmed Combat
    (4, 46),  // Dagger
    (2, 47),  // Bow
    (3, 47),  // Crossbow
    (8, 47),  // Sling
    (12, 47), // Thrown Weapons
];

fn class_of(value: i32) -> SkillAdvancementClass {
    match value {
        1 => SkillAdvancementClass::Untrained,
        2 => SkillAdvancementClass::Trained,
        3 => SkillAdvancementClass::Specialized,
        _ => SkillAdvancementClass::Inactive,
    }
}

fn clamp_index(value: i32, len: usize, what: &str, notes: &mut Vec<String>) -> i32 {
    let len = i32::try_from(len).unwrap_or(i32::MAX);
    if len == 0 {
        return 0;
    }
    if (0..len).contains(&value) {
        value
    } else {
        notes.push(format!("{what} {value} is not in the final table; using 0"));
        0
    }
}

/// Converts `legacy` (chosen against `data`, the January 2005 tables) into a result the final
/// tables accept. Returns the result and the list of substitutions made.
pub fn to_final(
    legacy: &LegacyCreation,
    data: &CreationData,
    tables: &FinalTables,
) -> Result<(CharGenResultData, Vec<String>), String> {
    let r = &legacy.result;
    let mut notes = vec![];
    let cg = &tables.char_gen;
    let legacy_heritage = data
        .heritages
        .get(r.heritage_group as usize)
        .ok_or("legacy heritage index out of range")?;
    let (heritage_key, heritage) = cg
        .heritage_groups
        .iter()
        .find(|(_, h)| h.name.eq_ignore_ascii_case(&legacy_heritage.name))
        .or_else(|| cg.heritage_groups.iter().next())
        .ok_or("final tables have no heritage")?;
    if !heritage.name.eq_ignore_ascii_case(&legacy_heritage.name) {
        notes.push(format!(
            "heritage {} is not in the final table; using {}",
            legacy_heritage.name, heritage.name
        ));
    }
    let legacy_sex = legacy_heritage
        .sexes
        .get(r.gender as usize)
        .ok_or("legacy sex index out of range")?;
    let (gender_key, sex) = heritage
        .sexes
        .iter()
        .find(|(_, s)| s.name.eq_ignore_ascii_case(&legacy_sex.name))
        .or_else(|| heritage.sexes.iter().next())
        .ok_or("final heritage has no sex")?;

    let mut out = r.clone();
    out.heritage_group = *heritage_key;
    out.gender = *gender_key;
    out.eyes_strip = clamp_index(r.eyes_strip, sex.eye_strips.len(), "eyes", &mut notes);
    out.nose_strip = clamp_index(r.nose_strip, sex.nose_strips.len(), "nose", &mut notes);
    out.mouth_strip = clamp_index(r.mouth_strip, sex.mouth_strips.len(), "mouth", &mut notes);
    out.hair_color = clamp_index(
        r.hair_color,
        sex.hair_colors.len(),
        "hair colour",
        &mut notes,
    );
    out.eye_color = clamp_index(r.eye_color, sex.eye_colors.len(), "eye colour", &mut notes);
    out.hair_style = clamp_index(
        r.hair_style,
        sex.hair_styles.len(),
        "hair style",
        &mut notes,
    );
    // A negative headgear style is "no headgear" in both eras.
    if r.headgear_style >= 0 {
        out.headgear_style =
            clamp_index(r.headgear_style, sex.headgear.len(), "headgear", &mut notes);
    }
    out.shirt_style = clamp_index(r.shirt_style, sex.shirts.len(), "shirt", &mut notes);
    out.trousers_style = clamp_index(r.trousers_style, sex.pants.len(), "trousers", &mut notes);
    out.footwear_style = clamp_index(r.footwear_style, sex.footwear.len(), "footwear", &mut notes);
    let first_colour = sex.clothing_colors.first().copied().unwrap_or(0);
    let bare_head = out.headgear_style < 0;
    for (colour, what) in [
        (&mut out.headgear_color, "headgear colour"),
        (&mut out.shirt_color, "shirt colour"),
        (&mut out.trousers_color, "trousers colour"),
        (&mut out.footwear_color, "footwear colour"),
    ] {
        if bare_head && what == "headgear colour" {
            continue;
        }
        if !sex.clothing_colors.contains(colour) {
            notes.push(format!("{what} {} is not in the final table", *colour));
            *colour = first_colour;
        }
    }

    // Profession: the same name if the final heritage has it, else the first template.
    let legacy_template = usize::try_from(r.template_num)
        .ok()
        .and_then(|i| legacy_sex.templates.get(i))
        .map(|t| t.name.as_str());
    let template = legacy_template
        .and_then(|name| {
            heritage
                .templates
                .iter()
                .position(|t| t.name.eq_ignore_ascii_case(name))
        })
        .unwrap_or_else(|| {
            notes.push(format!(
                "profession {} is not in the final table; using {}",
                legacy_template.unwrap_or("(custom)"),
                heritage.templates.first().map_or("", |t| t.name.as_str())
            ));
            0
        });
    out.template_num = i32::try_from(template).unwrap_or(0);

    // Attributes: each within 10..=100 and the total within the final heritage's credits.
    let mut attributes = [
        r.strength,
        r.endurance,
        r.coordination,
        r.quickness,
        r.focus,
        r.self_,
    ]
    .map(|a| a.clamp(10, 100));
    let budget = i32::try_from(heritage.attribute_credits).unwrap_or(330);
    let mut index = 0;
    while attributes.iter().sum::<i32>() > budget {
        if attributes[index] > 10 {
            attributes[index] -= 1;
        }
        index = (index + 1) % 6;
    }
    if attributes
        != [
            r.strength,
            r.endurance,
            r.coordination,
            r.quickness,
            r.focus,
            r.self_,
        ]
    {
        notes.push(format!(
            "attributes reduced to {attributes:?} for the final credits"
        ));
    }
    [
        out.strength,
        out.endurance,
        out.coordination,
        out.quickness,
        out.focus,
        out.self_,
    ] = attributes;

    // Skills: start from the final defaults, then carry each legacy choice over (a retired weapon
    // skill to its combined successor), never below the default, and drop the costliest choices
    // until the final credits cover them.
    let heritage_id = *heritage_key;
    let mut levels: Vec<SkillAdvancementClass> = (0..u32_from(TOTAL_NUM_SKILLS))
        .map(|id| default_skill_class(cg, &tables.skills, heritage_id, id))
        .collect();
    let defaults = levels.clone();
    let mut chosen = vec![];
    for (id, &value) in r.skill_advancement_classes.iter().enumerate() {
        let wanted = class_of(value);
        if !matches!(
            wanted,
            SkillAdvancementClass::Trained | SkillAdvancementClass::Specialized
        ) {
            continue;
        }
        let id = u32_from(id);
        // A skill the world's table still has stays itself; only a retired one moves to its
        // combined successor.
        let available = |s: u32| {
            (s as usize) < TOTAL_NUM_SKILLS && levels[s as usize] != SkillAdvancementClass::Inactive
        };
        let target = if available(id) {
            id
        } else {
            RETIRED_WEAPONS
                .iter()
                .find(|(old, _)| *old == id)
                .map_or(id, |(_, new)| *new)
        };
        if target != id {
            notes.push(format!("retired skill {id} becomes skill {target}"));
        }
        let slot = target as usize;
        if slot >= TOTAL_NUM_SKILLS || levels[slot] == SkillAdvancementClass::Inactive {
            notes.push(format!("skill {id} is not available in the final table"));
            continue;
        }
        if (levels[slot] as i32) < (wanted as i32) {
            levels[slot] = wanted;
            chosen.push(slot);
        }
    }
    let credits = i32::try_from(heritage.skill_credits).unwrap_or(0);
    // The last choice gives way first: a specialisation is first reduced to training, and a
    // training is then dropped back to the default.
    while skill_credits_used(cg, &tables.skills, heritage_id, &levels) > credits {
        let Some(&slot) = chosen.last() else { break };
        let (trained, _) = skill_costs(cg, &tables.skills, heritage_id, u32_from(slot));
        let reduced = SkillAdvancementClass::Trained.max_with(defaults[slot]);
        if levels[slot] == SkillAdvancementClass::Specialized
            && trained >= 0
            && reduced != SkillAdvancementClass::Specialized
        {
            levels[slot] = reduced;
            notes.push(format!(
                "skill {slot} trained only, to fit the final skill credits"
            ));
        } else {
            chosen.pop();
            levels[slot] = defaults[slot];
            notes.push(format!(
                "skill {slot} dropped to fit the final skill credits"
            ));
        }
    }
    out.skill_advancement_classes = levels.iter().map(|&l| l as i32).collect();

    // Starting town: the final area whose name begins the legacy one (Holtburg South -> Holtburg).
    let legacy_area = data
        .areas
        .get(r.start_area as usize)
        .map(|a| a.name.as_str());
    out.start_area = legacy_area
        .and_then(|name| {
            cg.starter_areas
                .iter()
                .position(|a| name.starts_with(a.name.as_str()))
        })
        .map_or_else(
            || {
                notes.push(format!(
                    "starting town {legacy_area:?} is not in the final table"
                ));
                0
            },
            u32_from,
        );
    Ok((out, notes))
}

trait MaxWith {
    fn max_with(self, other: Self) -> Self;
}
impl MaxWith for SkillAdvancementClass {
    fn max_with(self, other: Self) -> Self {
        if (other as i32) > (self as i32) {
            other
        } else {
            self
        }
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (classic front-end adapter; no retail behaviour claim).
    use super::*;
    use crate::panels::pregame::data::{Area, Heritage, Sex, Template};

    fn final_tables() -> Option<FinalTables> {
        let dir = std::env::var_os("DERETH_TEST_DAT_DIR")?;
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .join(dir);
        let store = dereth_dat::RetailDatStore::open_dir(&dir).ok()?;
        FinalTables::load(&store).ok()
    }

    fn legacy_data() -> CreationData {
        let sex = |name: &str| Sex {
            name: name.into(),
            templates: vec![
                Template {
                    name: "Adventurer".into(),
                    ..Default::default()
                },
                Template {
                    name: "Bow Hunter".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        CreationData {
            heritages: vec![
                Heritage {
                    name: "Aluvian".into(),
                    sexes: vec![sex("Female"), sex("Male")],
                    ..Default::default()
                },
                Heritage {
                    name: "Sho".into(),
                    sexes: vec![sex("Female"), sex("Male")],
                    ..Default::default()
                },
            ],
            areas: vec![
                Area {
                    name: "Holtburg South".into(),
                },
                Area {
                    name: "Shoushi West".into(),
                },
            ],
            ..Default::default()
        }
    }

    fn legacy(heritage: u32, gender: u32, skills: &[(usize, i32)]) -> LegacyCreation {
        let mut classes = vec![0; 41];
        for &(id, class) in skills {
            classes[id] = class;
        }
        LegacyCreation {
            result: CharGenResultData {
                heritage_group: heritage,
                gender,
                eyes_strip: 0,
                nose_strip: 0,
                mouth_strip: 0,
                hair_color: 0,
                eye_color: 0,
                hair_style: 0,
                headgear_style: -1,
                headgear_color: 0,
                shirt_style: 0,
                shirt_color: 0,
                trousers_style: 0,
                trousers_color: 0,
                footwear_style: 0,
                footwear_color: 0,
                skin_shade: 0.5,
                hair_shade: 0.5,
                headgear_shade: 0.5,
                shirt_shade: 0.5,
                trousers_shade: 0.5,
                footwear_shade: 0.5,
                template_num: 1,
                strength: 100,
                endurance: 100,
                coordination: 10,
                quickness: 10,
                focus: 10,
                self_: 100,
                slot: 0,
                class_id: 1,
                skill_advancement_classes: classes,
                name: "Bridge test".into(),
                start_area: 1,
                is_admin: 0,
                is_envoy: 0,
            },
            heraldry_symbol: 0,
            heraldry_color: 0,
        }
    }

    #[test]
    fn classic_choices_map_to_final_keys_skills_and_town() {
        let Some(tables) = final_tables() else {
            eprintln!("skipped: no final-era dats (DERETH_TEST_DAT_DIR)");
            return;
        };
        let data = legacy_data();
        // Sho (legacy index 1), Male (legacy index 1), Sword specialised, War Magic trained.
        let (out, _notes) = to_final(&legacy(1, 1, &[(11, 3), (34, 2)]), &data, &tables).unwrap();
        let heritage = &tables.char_gen.heritage_groups[&out.heritage_group];
        assert_eq!(heritage.name, "Sho");
        assert_eq!(heritage.sexes[&out.gender].name, "Male");
        assert_eq!(out.skill_advancement_classes.len(), TOTAL_NUM_SKILLS);
        // Sword is retired: its specialisation lands on Heavy Weapons, and Sword stays unchosen.
        assert!(out.skill_advancement_classes[44] >= 2);
        assert!(out.skill_advancement_classes[11] < 2);
        assert!(tables.char_gen.starter_areas[out.start_area as usize]
            .name
            .starts_with("Shoushi"));
        let levels: Vec<_> = out
            .skill_advancement_classes
            .iter()
            .map(|&v| class_of(v))
            .collect();
        assert!(
            skill_credits_used(
                &tables.char_gen,
                &tables.skills,
                out.heritage_group,
                &levels
            ) <= heritage.skill_credits as i32
        );
        let total =
            out.strength + out.endurance + out.coordination + out.quickness + out.focus + out.self_;
        assert!(total <= heritage.attribute_credits as i32);
    }

    #[test]
    fn unaffordable_classic_skills_are_dropped_until_the_final_credits_cover_them() {
        let Some(tables) = final_tables() else {
            eprintln!("skipped: no final-era dats (DERETH_TEST_DAT_DIR)");
            return;
        };
        // Every magic school specialised costs far more than any heritage's credits.
        let (out, notes) = to_final(
            &legacy(0, 0, &[(31, 3), (32, 3), (33, 3), (34, 3), (6, 3), (7, 3)]),
            &legacy_data(),
            &tables,
        )
        .unwrap();
        let heritage = &tables.char_gen.heritage_groups[&out.heritage_group];
        let levels: Vec<_> = out
            .skill_advancement_classes
            .iter()
            .map(|&v| class_of(v))
            .collect();
        assert!(
            skill_credits_used(
                &tables.char_gen,
                &tables.skills,
                out.heritage_group,
                &levels
            ) <= heritage.skill_credits as i32
        );
        assert!(notes.iter().any(|n| n.contains("dropped")));
    }
}
